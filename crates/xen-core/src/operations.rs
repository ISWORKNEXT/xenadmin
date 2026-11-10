use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::xapi::{ApiError, Connection};

#[derive(Clone, Deserialize, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum VmAction {
    Start,
    StartOn {
        host_ref: String,
    },
    Shutdown,
    Reboot,
    ForceShutdown,
    ForceReboot,
    Pause,
    Unpause,
    Suspend,
    Resume,
    ResumeOn {
        host_ref: String,
    },
    Clone {
        name: String,
    },
    Snapshot {
        name: String,
    },
    Revert,
    Destroy {
        delete_disks: bool,
        delete_snapshots: bool,
    },
}

impl VmAction {
    pub fn allowed_operation(&self) -> &'static str {
        match self {
            Self::Start => "start",
            Self::StartOn { .. } => "start_on",
            Self::Shutdown => "clean_shutdown",
            Self::Reboot => "clean_reboot",
            Self::ForceShutdown => "hard_shutdown",
            Self::ForceReboot => "hard_reboot",
            Self::Pause => "pause",
            Self::Unpause => "unpause",
            Self::Suspend => "suspend",
            Self::Resume => "resume",
            Self::ResumeOn { .. } => "resume_on",
            Self::Clone { .. } => "clone",
            Self::Snapshot { .. } => "snapshot",
            Self::Revert => "revert",
            Self::Destroy { .. } => "destroy",
        }
    }

    pub fn title(&self) -> &'static str {
        match self {
            Self::Start => "Start",
            Self::StartOn { .. } => "Start on host",
            Self::Shutdown => "Shut down",
            Self::Reboot => "Reboot",
            Self::ForceShutdown => "Force shut down",
            Self::ForceReboot => "Force reboot",
            Self::Pause => "Pause",
            Self::Unpause => "Unpause",
            Self::Suspend => "Suspend",
            Self::Resume => "Resume",
            Self::ResumeOn { .. } => "Resume on host",
            Self::Clone { .. } => "Clone",
            Self::Snapshot { .. } => "Take snapshot",
            Self::Revert => "Revert snapshot",
            Self::Destroy { .. } => "Delete",
        }
    }

    fn params(&self, session: &str, reference: &str) -> Result<Value, ApiError> {
        Ok(match self {
            Self::Start | Self::Resume => json!([session, reference, false, false]),
            Self::StartOn { host_ref } | Self::ResumeOn { host_ref } => {
                json!([session, reference, host_ref, false, false])
            }
            Self::Clone { name } | Self::Snapshot { name } => {
                if name.trim().is_empty() || name.chars().count() > 255 {
                    return Err(ApiError::Xen(
                        "Enter a name between 1 and 255 characters.".into(),
                    ));
                }
                json!([session, reference, name.trim()])
            }
            _ => json!([session, reference]),
        })
    }
}

impl Connection {
    /// Re-read the record before submitting. A stale UI cannot grant an operation.
    pub async fn begin_vm_action(&self, uuid: &str, action: &VmAction) -> Result<String, ApiError> {
        if self.settings.read_only {
            return Err(ApiError::Xen(
                "This connection is read-only. Changes are blocked by the Rust backend.".into(),
            ));
        }
        let reference: String = self
            .request("VM.get_by_uuid", json!([self.session, uuid]))
            .await?;
        let record: Value = self
            .request("VM.get_record", json!([self.session, reference]))
            .await?;
        if record["is_control_domain"].as_bool() == Some(true) {
            return Err(ApiError::Xen(
                "Control domains cannot be managed through VM actions.".into(),
            ));
        }
        let snapshot = record["is_a_snapshot"].as_bool() == Some(true);
        if matches!(action, VmAction::Revert) != snapshot {
            return Err(ApiError::Xen(
                "This operation is not valid for the selected object.".into(),
            ));
        }
        let template = record["is_a_template"].as_bool() == Some(true);
        if template && !matches!(action, VmAction::Clone { .. } | VmAction::Destroy { .. }) {
            return Err(ApiError::Xen(
                "Templates can only be cloned or deleted.".into(),
            ));
        }
        let allowed = record["allowed_operations"].as_array().is_some_and(|ops| {
            ops.iter()
                .any(|op| op.as_str() == Some(action.allowed_operation()))
        });
        if !allowed {
            return Err(ApiError::Xen(
                "The host does not currently allow this operation. Refresh and check the VM state."
                    .into(),
            ));
        }
        if matches!(
            action,
            VmAction::Start
                | VmAction::StartOn { .. }
                | VmAction::Resume
                | VmAction::ResumeOn { .. }
        ) {
            let host_ref = match action {
                VmAction::StartOn { host_ref } | VmAction::ResumeOn { host_ref } => {
                    Some(host_ref.clone())
                }
                _ => {
                    let resident = record["resident_on"].as_str().unwrap_or_default();
                    let affinity = record["affinity"].as_str().unwrap_or_default();
                    [resident, affinity]
                        .into_iter()
                        .find(|value| !value.is_empty() && *value != "OpaqueRef:NULL")
                        .map(str::to_owned)
                }
            };
            if let Some(host_ref) = host_ref {
                match self
                    .request::<Value>(
                        "VM.assert_can_boot_here",
                        json!([self.session, reference, host_ref]),
                    )
                    .await
                {
                    Ok(_) | Err(ApiError::Unsupported) => {}
                    Err(ApiError::XenCode(code, _)) if code == "MESSAGE_METHOD_UNKNOWN" => {}
                    Err(error) => {
                        return Err(context("The VM cannot boot on this host", error));
                    }
                }
            }
        }
        let method = format!("Async.VM.{}", action.allowed_operation());
        self.request(&method, action.params(&self.session, &reference)?)
            .await.map_err(|error| match error {
                ApiError::Connection => ApiError::Xen("The submission outcome is unknown. The host may have accepted this operation. Refresh the VM and inspect the host tasks before submitting again.".into()),
                other => other,
            })
    }

    pub async fn task_record(&self, reference: &str) -> Result<RemoteTask, ApiError> {
        let record: Value = self
            .request("task.get_record", json!([self.session, reference]))
            .await?;
        let status = match record["status"].as_str() {
            Some("pending") => TaskStatus::Running,
            Some("success") => TaskStatus::Succeeded,
            Some("failure") => TaskStatus::Failed,
            Some("cancelled") => TaskStatus::Cancelled,
            _ => return Err(ApiError::Protocol),
        };
        let progress = record["progress"]
            .as_f64()
            .or_else(|| record["progress"].as_str().and_then(|p| p.parse().ok()))
            .unwrap_or(0.0)
            .clamp(0.0, 1.0);
        // Error codes are useful without returning parameters that may contain private values.
        let code = record["error_info"][0]
            .as_str()
            .unwrap_or("Operation failed");
        let message = if status == TaskStatus::Failed
            && code.len() <= 128
            && code.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
        {
            code.to_owned()
        } else {
            String::new()
        };
        let can_cancel = record["allowed_operations"]
            .as_array()
            .is_some_and(|ops| ops.iter().any(|v| v.as_str() == Some("cancel")));
        Ok(RemoteTask {
            status,
            progress,
            message,
            can_cancel,
        })
    }

    pub async fn cancel_task(&self, reference: &str) -> Result<(), ApiError> {
        if self.settings.read_only {
            return Err(ApiError::Xen("This connection is read-only.".into()));
        }
        self.request::<Value>("task.cancel", json!([self.session, reference]))
            .await
            .map(|_| ())
    }

    pub async fn destroy_task(&self, reference: &str) {
        let _ = self
            .request::<Value>("task.destroy", json!([self.session, reference]))
            .await;
    }
}

#[derive(Clone, Copy, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskStatus {
    Running,
    Succeeded,
    Failed,
    Cancelled,
    Unreachable,
}

impl TaskStatus {
    pub fn terminal(self) -> bool {
        matches!(self, Self::Succeeded | Self::Failed | Self::Cancelled)
    }
}

pub struct RemoteTask {
    pub status: TaskStatus,
    pub progress: f64,
    pub message: String,
    pub can_cancel: bool,
}

impl Connection {
    /// Clone a template or VM. Templates are provisioned after cloning so the
    /// new VM receives the disks declared in `other_config.disks`.
    pub async fn clone_vm(&self, uuid: &str, name: &str) -> Result<String, ApiError> {
        if self.settings.read_only {
            return Err(ApiError::Xen(
                "This connection is read-only. Changes are blocked by the Rust backend.".into(),
            ));
        }
        let name = name.trim();
        if name.is_empty() || name.chars().count() > 255 {
            return Err(ApiError::Xen(
                "Enter a name between 1 and 255 characters.".into(),
            ));
        }
        let reference: String = self
            .request("VM.get_by_uuid", json!([self.session, uuid]))
            .await
            .map_err(|error| context("Cannot find the object to clone", error))?;
        let record: Value = self
            .request("VM.get_record", json!([self.session, reference]))
            .await
            .map_err(|error| context("Cannot read the object to clone", error))?;
        if record["uuid"].as_str() != Some(uuid) {
            return Err(ApiError::Protocol);
        }
        if record["is_control_domain"].as_bool() == Some(true) {
            return Err(ApiError::Xen("Control domains cannot be cloned.".into()));
        }
        let allowed = record["allowed_operations"]
            .as_array()
            .is_some_and(|ops| ops.iter().any(|op| op.as_str() == Some("clone")));
        if !allowed {
            return Err(ApiError::Xen(
                "The host does not currently allow cloning this object. Refresh and check its state."
                    .into(),
            ));
        }

        let task: String = self
            .request("Async.VM.clone", json!([self.session, reference, name]))
            .await
            .map_err(|error| context("The host rejected the clone request", error))?;
        let cloned = wait_task_value(self, &task)
            .await
            .map_err(|error| context("The clone task failed", error))?;
        let cloned_ref = decode_ref(&cloned).ok_or(ApiError::Protocol)?;

        if record["is_a_template"].as_bool() == Some(true) {
            // Provisioning turns the cloned template record into a real VM and
            // materialises any disks declared in `other_config.disks`.
            let task: String = self
                .request("Async.VM.provision", json!([self.session, cloned_ref]))
                .await
                .map_err(|error| context("The clone exists but cannot be provisioned", error))?;
            wait_task(self, &task)
                .await
                .map_err(|error| context("Provisioning the new VM failed", error))?;
            let _ = self
                .request::<Value>(
                    "VM.remove_from_other_config",
                    json!([self.session, cloned_ref, "disks"]),
                )
                .await;
        }

        let new_uuid: String = self
            .request("VM.get_uuid", json!([self.session, cloned_ref]))
            .await
            .map_err(|error| context("Cannot read the new VM", error))?;
        Ok(new_uuid)
    }

    /// Update the editable name/description and, when the VM is halted, its
    /// vCPU and static-memory limits. Only changed values are submitted.
    #[allow(clippy::too_many_arguments)]
    pub async fn update_vm(
        &self,
        uuid: &str,
        name: &str,
        description: &str,
        vcpus_max: u64,
        vcpus_at_startup: u64,
        memory_static_max_mib: u64,
        memory_dynamic_min_mib: u64,
        memory_dynamic_max_mib: u64,
        boot_order: Option<String>,
    ) -> Result<(), ApiError> {
        if self.settings.read_only {
            return Err(ApiError::Xen(
                "This connection is read-only. Changes are blocked by the Rust backend.".into(),
            ));
        }
        let name = name.trim();
        if name.is_empty() || name.chars().count() > 255 {
            return Err(ApiError::Xen(
                "Enter a name between 1 and 255 characters.".into(),
            ));
        }
        if description.chars().count() > 1024 {
            return Err(ApiError::Xen(
                "The description must be 1024 characters or fewer.".into(),
            ));
        }
        if vcpus_max < 1 || vcpus_at_startup < 1 || vcpus_at_startup > vcpus_max {
            return Err(ApiError::Xen(
                "vCPUs at startup must be between 1 and the vCPU maximum.".into(),
            ));
        }
        if memory_static_max_mib == 0 || memory_dynamic_min_mib == 0 || memory_dynamic_max_mib == 0
        {
            return Err(ApiError::Xen("Enter a memory size in MiB.".into()));
        }
        if memory_dynamic_min_mib > memory_dynamic_max_mib
            || memory_dynamic_max_mib > memory_static_max_mib
        {
            return Err(ApiError::Xen(
                "Dynamic memory minimum must not exceed the maximum, and the dynamic maximum must not exceed the static maximum.".into(),
            ));
        }
        let boot_order = boot_order.map(|order| order.trim().to_ascii_lowercase());
        if let Some(order) = &boot_order {
            let valid = !order.is_empty()
                && order.len() <= 3
                && order.chars().all(|c| matches!(c, 'c' | 'd' | 'n'))
                && order
                    .chars()
                    .collect::<std::collections::HashSet<_>>()
                    .len()
                    == order.len();
            if !valid {
                return Err(ApiError::Xen(
                    "Boot order must contain unique device letters: c (CD), d (disk), n (network)."
                        .into(),
                ));
            }
        }

        let reference: String = self
            .request("VM.get_by_uuid", json!([self.session, uuid]))
            .await?;
        let record: Value = self
            .request("VM.get_record", json!([self.session, reference]))
            .await?;
        if record["uuid"].as_str() != Some(uuid)
            || record["is_control_domain"].as_bool() == Some(true)
        {
            return Err(ApiError::Protocol);
        }
        if record["is_a_template"].as_bool() == Some(true)
            || record["is_a_snapshot"].as_bool() == Some(true)
        {
            return Err(ApiError::Xen("Only virtual machines can be edited.".into()));
        }

        let current_name = record["name_label"].as_str().unwrap_or_default();
        let current_description = record["name_description"].as_str().unwrap_or_default();
        let current_vcpus_max = number(&record, "VCPUs_max");
        let current_vcpus_startup = number(&record, "VCPUs_at_startup");
        let current_static_min = number(&record, "memory_static_min");
        let current_static_max = number(&record, "memory_static_max");
        let current_dynamic_min = number(&record, "memory_dynamic_min");
        let current_dynamic_max = number(&record, "memory_dynamic_max");
        let static_max = memory_static_max_mib.saturating_mul(1024 * 1024);
        let dynamic_min = memory_dynamic_min_mib.saturating_mul(1024 * 1024);
        let dynamic_max = memory_dynamic_max_mib.saturating_mul(1024 * 1024);
        if static_max < current_static_min || dynamic_min < current_static_min {
            return Err(ApiError::Xen(format!(
                "Memory cannot be lower than the static minimum ({} MiB).",
                current_static_min / (1024 * 1024)
            )));
        }

        let current_boot_order = record["HVM_boot_params"]["order"]
            .as_str()
            .unwrap_or("cd")
            .to_ascii_lowercase();
        let boot_changed = boot_order
            .as_ref()
            .is_some_and(|order| order != &current_boot_order);

        let vcpus_changed =
            vcpus_max != current_vcpus_max || vcpus_at_startup != current_vcpus_startup;
        let static_changed = static_max != current_static_max;
        let dynamic_changed =
            dynamic_min != current_dynamic_min || dynamic_max != current_dynamic_max;
        let halted = record["power_state"].as_str() == Some("Halted");
        if (vcpus_changed || static_changed) && !halted {
            return Err(ApiError::Xen(
                "Shut down the VM before changing its CPU or static memory settings.".into(),
            ));
        }
        if dynamic_changed && !halted {
            let allowed = record["allowed_operations"].as_array().is_some_and(|ops| {
                ops.iter()
                    .any(|op| op.as_str() == Some("changing_dynamic_range"))
            });
            if !allowed {
                return Err(ApiError::Xen(
                    "The host does not currently allow changing dynamic memory for this VM.".into(),
                ));
            }
        }

        if name != current_name {
            self.request::<Value>("VM.set_name_label", json!([self.session, reference, name]))
                .await?;
        }
        if description != current_description {
            self.request::<Value>(
                "VM.set_name_description",
                json!([self.session, reference, description]),
            )
            .await?;
        }

        if vcpus_changed {
            if vcpus_max < current_vcpus_startup {
                self.request::<Value>(
                    "VM.set_VCPUs_at_startup",
                    json!([self.session, reference, vcpus_at_startup]),
                )
                .await?;
                self.request::<Value>(
                    "VM.set_VCPUs_max",
                    json!([self.session, reference, vcpus_max]),
                )
                .await?;
            } else {
                self.request::<Value>(
                    "VM.set_VCPUs_max",
                    json!([self.session, reference, vcpus_max]),
                )
                .await?;
                self.request::<Value>(
                    "VM.set_VCPUs_at_startup",
                    json!([self.session, reference, vcpus_at_startup]),
                )
                .await?;
            }
        }

        if halted && (static_changed || dynamic_changed) {
            self.request::<Value>(
                "VM.set_memory_limits",
                json!([
                    self.session,
                    reference,
                    current_static_min,
                    static_max,
                    dynamic_min,
                    dynamic_max
                ]),
            )
            .await?;
        } else if dynamic_changed {
            self.request::<Value>(
                "VM.set_memory_dynamic_range",
                json!([self.session, reference, dynamic_min, dynamic_max]),
            )
            .await?;
        }

        if boot_changed && let Some(order) = boot_order {
            let mut params = record["HVM_boot_params"]
                .as_object()
                .cloned()
                .unwrap_or_default();
            params.insert("order".into(), Value::String(order));
            self.request::<Value>(
                "VM.set_HVM_boot_params",
                json!([self.session, reference, Value::Object(params)]),
            )
            .await?;
        }

        Ok(())
    }

    /// Delete a VM and, optionally, the disks it owns. Owner VBDs are identified
    /// the same way as upstream XenAdmin: a VBD with an `owner` key in
    /// `other_config`. The suspended-image VDI is removed as well.
    pub async fn destroy_vm(
        &self,
        uuid: &str,
        delete_disks: bool,
        delete_snapshots: bool,
    ) -> Result<(), ApiError> {
        if self.settings.read_only {
            return Err(ApiError::Xen(
                "This connection is read-only. Changes are blocked by the Rust backend.".into(),
            ));
        }
        let reference: String = self
            .request("VM.get_by_uuid", json!([self.session, uuid]))
            .await?;
        let record: Value = self
            .request("VM.get_record", json!([self.session, reference]))
            .await?;
        if record["uuid"].as_str() != Some(uuid)
            || record["is_control_domain"].as_bool() == Some(true)
        {
            return Err(ApiError::Protocol);
        }
        let allowed = record["allowed_operations"]
            .as_array()
            .is_some_and(|ops| ops.iter().any(|op| op.as_str() == Some("destroy")));
        if !allowed {
            return Err(ApiError::Xen(
                "The host does not currently allow deleting this object. Shut it down and refresh before trying again.".into(),
            ));
        }

        let vbds = if delete_disks || delete_snapshots {
            Some(self.records("VBD").await?)
        } else {
            None
        };

        let mut snapshot_failure = None;
        if delete_snapshots {
            let snapshots = self.records("VM").await?;
            for (snapshot_ref, snapshot) in &snapshots {
                if snapshot["snapshot_of"].as_str() != Some(reference.as_str()) {
                    continue;
                }
                let snapshot_allowed = snapshot["allowed_operations"]
                    .as_array()
                    .is_some_and(|ops| ops.iter().any(|op| op.as_str() == Some("destroy")));
                if !snapshot_allowed {
                    snapshot_failure.get_or_insert_with(|| {
                        ApiError::Xen(
                            "A snapshot cannot be deleted in its current state. Delete it manually before deleting the VM."
                                .into(),
                        )
                    });
                    continue;
                }
                let vdis = vbds
                    .as_ref()
                    .map(|vbds| owner_vdis(vbds, snapshot, snapshot_ref))
                    .unwrap_or_default();
                if snapshot["power_state"].as_str() == Some("Suspended") {
                    match self
                        .async_task(
                            "Async.VM.hard_shutdown",
                            json!([self.session, snapshot_ref]),
                        )
                        .await
                    {
                        Ok(()) => {}
                        Err(error) => {
                            snapshot_failure.get_or_insert(error);
                            continue;
                        }
                    }
                }
                match self.destroy_ref(snapshot_ref).await {
                    Ok(()) => {
                        if let Err(error) = self.destroy_vdis(vdis).await {
                            snapshot_failure.get_or_insert(error);
                        }
                    }
                    Err(ApiError::XenCode(code, _)) if code == "HANDLE_INVALID" => {}
                    Err(error) => {
                        snapshot_failure.get_or_insert(error);
                    }
                }
            }
        }

        let vdis = if delete_disks {
            vbds.as_ref()
                .map(|vbds| owner_vdis(vbds, &record, &reference))
                .unwrap_or_default()
        } else {
            Vec::new()
        };

        match self.destroy_ref(&reference).await {
            Ok(()) => match self.destroy_vdis(vdis).await {
                Ok(()) => match snapshot_failure {
                    Some(error) => Err(error),
                    None => Ok(()),
                },
                Err(error) => Err(snapshot_failure.unwrap_or(error)),
            },
            Err(error) => Err(error),
        }
    }

    async fn destroy_ref(&self, reference: &str) -> Result<(), ApiError> {
        let task: String = self
            .request("Async.VM.destroy", json!([self.session, reference]))
            .await?;
        wait_task(self, &task).await
    }

    async fn destroy_vdis(&self, vdis: Vec<String>) -> Result<(), ApiError> {
        let mut failure = None;
        for vdi in vdis {
            match self
                .request::<Value>("VDI.destroy", json!([self.session, vdi]))
                .await
            {
                Ok(_) => {}
                Err(ApiError::XenCode(code, _)) if code == "HANDLE_INVALID" => {}
                Err(error) => failure = failure.or(Some(error)),
            }
        }
        match failure {
            Some(error) => Err(error),
            None => Ok(()),
        }
    }

    async fn async_task(&self, method: &str, params: Value) -> Result<(), ApiError> {
        let task: String = self.request(method, params).await?;
        wait_task(self, &task).await
    }
}

fn number(record: &Value, key: &str) -> u64 {
    let value = &record[key];
    value
        .as_u64()
        .or_else(|| value.as_str().and_then(|value| value.parse().ok()))
        .unwrap_or(0)
}

fn owner_vdis(vbds: &HashMap<String, Value>, record: &Value, reference: &str) -> Vec<String> {
    let mut vdis = Vec::new();
    for vbd in vbds.values() {
        if vbd["VM"].as_str() != Some(reference) {
            continue;
        }
        if vbd["other_config"].get("owner").is_none() {
            continue;
        }
        if let Some(vdi) = vbd["VDI"].as_str()
            && !vdi.is_empty()
            && vdi != "OpaqueRef:NULL"
        {
            vdis.push(vdi.to_owned());
        }
    }
    if let Some(vdi) = record["suspend_VDI"].as_str()
        && !vdi.is_empty()
        && vdi != "OpaqueRef:NULL"
    {
        vdis.push(vdi.to_owned());
    }
    vdis
}

/// XenAPI wraps references returned by task results in an XML `<value>` element.
fn decode_ref(value: &Value) -> Option<String> {
    let mut text = value.as_str()?.trim();
    for (open, close) in [("<value>", "</value>"), ("<string>", "</string>")] {
        if let Some(inner) = text
            .strip_prefix(open)
            .and_then(|text| text.strip_suffix(close))
        {
            text = inner.trim();
        }
    }
    (!text.is_empty() && text != "OpaqueRef:NULL").then(|| text.to_owned())
}

fn context(prefix: &str, error: ApiError) -> ApiError {
    match error {
        ApiError::XenCode(code, message) => ApiError::XenCode(code, format!("{prefix}: {message}")),
        ApiError::Xen(message) => ApiError::Xen(format!("{prefix}: {message}")),
        other => other,
    }
}

async fn wait_task(connection: &Connection, reference: &str) -> Result<(), ApiError> {
    wait_task_value(connection, reference).await.map(|_| ())
}

async fn wait_task_value(connection: &Connection, reference: &str) -> Result<Value, ApiError> {
    for _ in 0..7200 {
        let task = connection.task_record(reference).await?;
        match task.status {
            TaskStatus::Succeeded => {
                let result: Value = connection
                    .request("task.get_record", json!([connection.session, reference]))
                    .await
                    .map(|record: Value| record["result"].clone())?;
                connection.destroy_task(reference).await;
                return Ok(result);
            }
            TaskStatus::Failed => {
                let detail = if task.message.is_empty() {
                    "The host rejected the operation.".into()
                } else {
                    format!("The host rejected the operation: {}.", task.message)
                };
                connection.destroy_task(reference).await;
                return Err(ApiError::Xen(detail));
            }
            TaskStatus::Cancelled => {
                connection.destroy_task(reference).await;
                return Err(ApiError::Xen("The operation was cancelled.".into()));
            }
            TaskStatus::Running | TaskStatus::Unreachable => {
                tokio::time::sleep(std::time::Duration::from_millis(500)).await;
            }
        }
    }
    Err(ApiError::Xen(
        "The operation is still running on the host. Check the task list before retrying.".into(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn start_and_resume_never_force_a_vm() {
        assert_eq!(
            VmAction::Start.params("s", "v").unwrap(),
            json!(["s", "v", false, false])
        );
        assert_eq!(
            VmAction::Resume.params("s", "v").unwrap(),
            json!(["s", "v", false, false])
        );
        assert_eq!(
            VmAction::StartOn {
                host_ref: "h".into()
            }
            .params("s", "v")
            .unwrap(),
            json!(["s", "v", "h", false, false])
        );
        assert_eq!(
            VmAction::ResumeOn {
                host_ref: "h".into()
            }
            .allowed_operation(),
            "resume_on"
        );
    }
    #[test]
    fn destroy_uses_the_plain_object_reference() {
        assert_eq!(
            VmAction::Destroy {
                delete_disks: true,
                delete_snapshots: false,
            }
            .params("s", "v")
            .unwrap(),
            json!(["s", "v"])
        );
        assert_eq!(
            VmAction::Destroy {
                delete_disks: false,
                delete_snapshots: false,
            }
            .allowed_operation(),
            "destroy"
        );
    }
    #[test]
    fn only_owned_disks_and_the_suspend_image_are_cleaned_up() {
        let vbds: HashMap<String, Value> = serde_json::from_value(json!({
            "vbd-owned": {"VM": "vm", "VDI": "vdi-owned", "other_config": {"owner": ""}},
            "vbd-shared": {"VM": "vm", "VDI": "vdi-shared", "other_config": {}},
            "vbd-other-vm": {"VM": "other", "VDI": "vdi-other", "other_config": {"owner": ""}},
            "vbd-empty": {"VM": "vm", "VDI": "OpaqueRef:NULL", "other_config": {"owner": ""}}
        }))
        .unwrap();
        let record = json!({"suspend_VDI": "vdi-suspend"});
        assert_eq!(
            owner_vdis(&vbds, &record, "vm"),
            vec!["vdi-owned".to_string(), "vdi-suspend".to_string()]
        );
        assert!(owner_vdis(&vbds, &json!({"suspend_VDI": "OpaqueRef:NULL"}), "vm").len() == 1);
    }
    #[test]
    fn task_references_are_unwrapped() {
        assert_eq!(
            decode_ref(&json!("<value>OpaqueRef:abc</value>")),
            Some("OpaqueRef:abc".into())
        );
        assert_eq!(
            decode_ref(&json!("<value><string>OpaqueRef:abc</string></value>")),
            Some("OpaqueRef:abc".into())
        );
        assert_eq!(decode_ref(&json!("OpaqueRef:NULL")), None);
        assert_eq!(decode_ref(&json!("")), None);
    }
    #[test]
    fn empty_names_cannot_submit_clone_or_snapshot() {
        assert!(
            VmAction::Clone { name: "  ".into() }
                .params("s", "v")
                .is_err()
        );
        assert!(
            VmAction::Snapshot {
                name: "a".repeat(256)
            }
            .params("s", "v")
            .is_err()
        );
    }
}
