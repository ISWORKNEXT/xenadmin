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
        if template
            && !snapshot
            && !matches!(action, VmAction::Clone { .. } | VmAction::Destroy { .. })
        {
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
            self.preflight_boot(&reference).await?;
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

#[derive(Clone, Copy, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SrAction {
    Scan,
    Destroy,
}

impl SrAction {
    fn method(self) -> &'static str {
        match self {
            Self::Scan => "Async.SR.scan",
            Self::Destroy => "Async.SR.destroy",
        }
    }

    pub fn title(self) -> &'static str {
        match self {
            Self::Scan => "Scan storage",
            Self::Destroy => "Destroy storage repository",
        }
    }

    pub fn destructive(self) -> bool {
        matches!(self, Self::Destroy)
    }
}

#[derive(Clone, Copy, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum HostAction {
    Enable,
    Disable,
    PowerOn,
    Evacuate,
    Reboot,
    Shutdown,
    RestartToolstack,
    RescanPifs,
}

impl HostAction {
    fn method(self) -> &'static str {
        match self {
            Self::Enable => "Async.host.enable",
            Self::Disable => "Async.host.disable",
            Self::PowerOn => "Async.host.power_on",
            Self::Evacuate => "Async.host.evacuate",
            Self::Reboot => "Async.host.reboot",
            Self::Shutdown => "Async.host.shutdown",
            Self::RestartToolstack => "Async.host.restart_agent",
            Self::RescanPifs => "Async.PIF.scan",
        }
    }

    pub fn title(self) -> &'static str {
        match self {
            Self::Enable => "Enable host",
            Self::Disable => "Disable host",
            Self::PowerOn => "Power on host",
            Self::Evacuate => "Evacuate host",
            Self::Reboot => "Reboot host",
            Self::Shutdown => "Shut down host",
            Self::RestartToolstack => "Restart toolstack",
            Self::RescanPifs => "Rescan physical interfaces",
        }
    }

    pub fn destructive(self) -> bool {
        matches!(self, Self::Reboot | Self::Shutdown)
    }
}

impl Connection {
    pub async fn create_network(
        &self,
        name: &str,
        description: &str,
        mtu: u64,
    ) -> Result<(), ApiError> {
        if self.settings.read_only {
            return Err(ApiError::Xen(
                "This connection is read-only. Changes are blocked by the Rust backend.".into(),
            ));
        }
        let name = name.trim();
        if name.is_empty() || name.chars().count() > 255 {
            return Err(ApiError::Xen(
                "Enter a network name between 1 and 255 characters.".into(),
            ));
        }
        if description.chars().count() > 1024 {
            return Err(ApiError::Xen(
                "The description must be 1024 characters or fewer.".into(),
            ));
        }
        if !(1500..=9000).contains(&mtu) {
            return Err(ApiError::Xen("MTU must be between 1500 and 9000.".into()));
        }
        self.request::<Value>(
            "network.create",
            json!([
                self.session,
                {
                    "name_label": name,
                    "name_description": description,
                    "MTU": mtu,
                    "other_config": {},
                    "tags": [],
                    "bridge": ""
                }
            ]),
        )
        .await?;
        Ok(())
    }

    pub async fn destroy_network(&self, network_uuid: &str) -> Result<(), ApiError> {
        if self.settings.read_only {
            return Err(ApiError::Xen(
                "This connection is read-only. Changes are blocked by the Rust backend.".into(),
            ));
        }
        let network_ref: String = self
            .request("network.get_by_uuid", json!([self.session, network_uuid]))
            .await?;
        let network: Value = self
            .request("network.get_record", json!([self.session, network_ref]))
            .await?;
        if network["uuid"].as_str() != Some(network_uuid) {
            return Err(ApiError::Protocol);
        }
        self.async_task("Async.network.destroy", json!([self.session, network_ref]))
            .await
    }

    pub async fn sr_action(&self, sr_uuid: &str, action: SrAction) -> Result<(), ApiError> {
        if self.settings.read_only {
            return Err(ApiError::Xen(
                "This connection is read-only. Changes are blocked by the Rust backend.".into(),
            ));
        }
        let sr_ref: String = self
            .request("SR.get_by_uuid", json!([self.session, sr_uuid]))
            .await?;
        let sr: Value = self
            .request("SR.get_record", json!([self.session, sr_ref]))
            .await?;
        if sr["uuid"].as_str() != Some(sr_uuid) {
            return Err(ApiError::Protocol);
        }
        self.async_task(action.method(), json!([self.session, sr_ref]))
            .await
    }

    pub async fn dismiss_alert(&self, message_uuid: &str) -> Result<(), ApiError> {
        if self.settings.read_only {
            return Err(ApiError::Xen(
                "This connection is read-only. Changes are blocked by the Rust backend.".into(),
            ));
        }
        let message_ref: String = self
            .request("message.get_by_uuid", json!([self.session, message_uuid]))
            .await?;
        self.request::<Value>("message.destroy", json!([self.session, message_ref]))
            .await?;
        Ok(())
    }

    pub async fn host_action(&self, host_uuid: &str, action: HostAction) -> Result<(), ApiError> {
        if self.settings.read_only {
            return Err(ApiError::Xen(
                "This connection is read-only. Changes are blocked by the Rust backend.".into(),
            ));
        }
        let host_ref: String = self
            .request("host.get_by_uuid", json!([self.session, host_uuid]))
            .await?;
        let host: Value = self
            .request("host.get_record", json!([self.session, host_ref]))
            .await?;
        if host["uuid"].as_str() != Some(host_uuid) {
            return Err(ApiError::Protocol);
        }
        self.async_task(action.method(), json!([self.session, host_ref]))
            .await
    }

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
        firmware: Option<String>,
        secure_boot: Option<bool>,
        boot_policy: Option<String>,
        after_crash: Option<String>,
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
        let firmware = firmware.map(|value| value.trim().to_ascii_lowercase());
        if let Some(firmware) = &firmware
            && !valid_firmware(firmware)
        {
            return Err(ApiError::Xen("Firmware must be BIOS or UEFI.".into()));
        }
        let boot_policy = boot_policy.map(|value| {
            if value.trim().is_empty() {
                String::new()
            } else if value.trim().eq_ignore_ascii_case("BIOS order") {
                "BIOS order".into()
            } else {
                value.trim().to_owned()
            }
        });
        let after_crash = after_crash.map(|value| value.trim().to_ascii_lowercase());
        if let Some(after_crash) = &after_crash
            && !valid_after_crash(after_crash)
        {
            return Err(ApiError::Xen(
                "After-crash policy must be restart, destroy, preserve or rename_restart.".into(),
            ));
        }
        if let Some(order) = &boot_order
            && !valid_boot_order(order)
        {
            return Err(ApiError::Xen(
                "Boot order must contain unique device letters: c (CD), d (disk), n (network)."
                    .into(),
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
        let current_firmware = record["HVM_boot_params"]["firmware"]
            .as_str()
            .filter(|value| !value.is_empty())
            .unwrap_or("bios")
            .to_ascii_lowercase();
        let current_secure = record["platform"]["secureboot"].as_str().unwrap_or("false");
        let current_policy = record["HVM_boot_policy"].as_str().unwrap_or("");
        let current_after_crash = record["actions_after_crash"].as_str().unwrap_or("restart");
        let is_hvm = !current_policy.is_empty();
        if !is_hvm
            && (boot_order.is_some()
                || firmware.is_some()
                || secure_boot.is_some()
                || boot_policy.is_some())
        {
            return Err(ApiError::Xen(
                "Firmware and boot order options are only available for HVM guests.".into(),
            ));
        }
        let effective_firmware = firmware.as_deref().unwrap_or(&current_firmware);
        if secure_boot_requires_uefi(effective_firmware, secure_boot) {
            return Err(ApiError::Xen("Secure boot requires UEFI firmware.".into()));
        }
        let secure_desired = secure_boot.map(|secure| secure.to_string()).or_else(|| {
            (effective_firmware == "bios" && current_secure != "false").then(|| "false".to_owned())
        });
        let boot_changed = boot_order
            .as_ref()
            .is_some_and(|order| order != &current_boot_order)
            || firmware
                .as_ref()
                .is_some_and(|firmware| firmware != &current_firmware);
        let policy_changed = boot_policy
            .as_ref()
            .is_some_and(|policy| policy != current_policy);
        let secure_changed = secure_desired
            .as_ref()
            .is_some_and(|secure| secure != current_secure);
        let crash_changed = after_crash
            .as_ref()
            .is_some_and(|after_crash| after_crash != current_after_crash);

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

        if boot_changed {
            let mut params = record["HVM_boot_params"]
                .as_object()
                .cloned()
                .unwrap_or_default();
            if let Some(order) = boot_order {
                params.insert("order".into(), Value::String(order));
            }
            if let Some(firmware) = firmware {
                params.insert("firmware".into(), Value::String(firmware));
            }
            self.request::<Value>(
                "VM.set_HVM_boot_params",
                json!([self.session, reference, Value::Object(params)]),
            )
            .await?;
        }
        if policy_changed && let Some(policy) = boot_policy {
            self.request::<Value>(
                "VM.set_HVM_boot_policy",
                json!([self.session, reference, policy]),
            )
            .await?;
        }
        if secure_changed && let Some(secure) = secure_desired {
            let mut platform = record["platform"].as_object().cloned().unwrap_or_default();
            platform.insert("secureboot".into(), Value::String(secure));
            self.request::<Value>(
                "VM.set_platform",
                json!([self.session, reference, Value::Object(platform)]),
            )
            .await?;
        }
        if crash_changed && let Some(after_crash) = after_crash {
            self.request::<Value>(
                "VM.set_actions_after_crash",
                json!([self.session, reference, after_crash]),
            )
            .await?;
        }

        Ok(())
    }

    /// Insert or eject media in a virtual CD drive. The VBD is resolved from
    /// its UUID and must belong to the selected VM.
    pub async fn change_cd(
        &self,
        vm_uuid: &str,
        vbd_uuid: &str,
        iso_uuid: Option<&str>,
    ) -> Result<(), ApiError> {
        if self.settings.read_only {
            return Err(ApiError::Xen(
                "This connection is read-only. Changes are blocked by the Rust backend.".into(),
            ));
        }
        let vm_ref: String = self
            .request("VM.get_by_uuid", json!([self.session, vm_uuid]))
            .await?;
        let vm: Value = self
            .request("VM.get_record", json!([self.session, vm_ref]))
            .await?;
        if vm["uuid"].as_str() != Some(vm_uuid) || vm["is_control_domain"].as_bool() == Some(true) {
            return Err(ApiError::Protocol);
        }
        if vm["is_a_template"].as_bool() == Some(true)
            || vm["is_a_snapshot"].as_bool() == Some(true)
        {
            return Err(ApiError::Xen(
                "CD media can only be changed for a virtual machine.".into(),
            ));
        }
        let vbd_ref: String = self
            .request("VBD.get_by_uuid", json!([self.session, vbd_uuid]))
            .await?;
        let vbd: Value = self
            .request("VBD.get_record", json!([self.session, vbd_ref]))
            .await?;
        if vbd["uuid"].as_str() != Some(vbd_uuid)
            || vbd["VM"].as_str() != Some(vm_ref.as_str())
            || vbd["type"].as_str() != Some("CD")
        {
            return Err(ApiError::Xen(
                "The selected CD drive does not belong to this virtual machine.".into(),
            ));
        }
        let empty = vbd["empty"].as_bool().unwrap_or(true);
        match iso_uuid {
            Some(iso_uuid) => {
                let vdi_ref: String = self
                    .request("VDI.get_by_uuid", json!([self.session, iso_uuid]))
                    .await?;
                let vdi: Value = self
                    .request("VDI.get_record", json!([self.session, vdi_ref]))
                    .await?;
                if vdi["uuid"].as_str() != Some(iso_uuid) {
                    return Err(ApiError::Protocol);
                }
                let sr_ref = vdi["SR"].as_str().unwrap_or_default();
                let sr: Value = self
                    .request("SR.get_record", json!([self.session, sr_ref]))
                    .await?;
                if vdi["type"].as_str() != Some("iso") && sr["content_type"].as_str() != Some("iso")
                {
                    return Err(ApiError::Xen(
                        "The selected VDI is not an ISO image.".into(),
                    ));
                }
                if !empty {
                    self.request::<Value>("VBD.eject", json!([self.session, vbd_ref]))
                        .await?;
                }
                self.request::<Value>("VBD.insert", json!([self.session, vbd_ref, vdi_ref]))
                    .await?;
            }
            None if !empty => {
                self.request::<Value>("VBD.eject", json!([self.session, vbd_ref]))
                    .await?;
            }
            None => {}
        }
        Ok(())
    }

    pub async fn add_disk(
        &self,
        vm_uuid: &str,
        sr_uuid: &str,
        size_bytes: u64,
        name: Option<&str>,
    ) -> Result<(), ApiError> {
        if self.settings.read_only {
            return Err(ApiError::Xen(
                "This connection is read-only. Changes are blocked by the Rust backend.".into(),
            ));
        }
        if size_bytes == 0 {
            return Err(ApiError::Xen("Enter a disk size greater than zero.".into()));
        }
        let vm_ref: String = self
            .request("VM.get_by_uuid", json!([self.session, vm_uuid]))
            .await?;
        let vm: Value = self
            .request("VM.get_record", json!([self.session, vm_ref]))
            .await?;
        if vm["uuid"].as_str() != Some(vm_uuid)
            || vm["is_control_domain"].as_bool() == Some(true)
            || vm["is_a_template"].as_bool() == Some(true)
            || vm["is_a_snapshot"].as_bool() == Some(true)
        {
            return Err(ApiError::Xen(
                "Disks can only be added to a virtual machine.".into(),
            ));
        }
        if vm["power_state"].as_str() != Some("Halted") {
            return Err(ApiError::Xen(
                "Shut down the VM before adding a disk.".into(),
            ));
        }
        let sr_ref: String = self
            .request("SR.get_by_uuid", json!([self.session, sr_uuid]))
            .await?;
        let sr: Value = self
            .request("SR.get_record", json!([self.session, sr_ref]))
            .await?;
        if sr["uuid"].as_str() != Some(sr_uuid) {
            return Err(ApiError::Protocol);
        }
        if sr["content_type"].as_str() == Some("iso") {
            return Err(ApiError::Xen(
                "ISO storage repositories cannot hold virtual disks.".into(),
            ));
        }
        let mut name = name.unwrap_or_default().trim().to_owned();
        if name.is_empty() {
            let vm_name = vm["name_label"].as_str().unwrap_or("VM");
            name = format!("{vm_name} disk");
        }
        if name.chars().count() > 255 {
            return Err(ApiError::Xen(
                "The disk name must be 255 characters or fewer.".into(),
            ));
        }
        let devices: Vec<String> = self
            .request("VM.get_allowed_VBD_devices", json!([self.session, vm_ref]))
            .await?;
        let Some(userdevice) = devices.first().cloned() else {
            return Err(ApiError::Xen(
                "This VM has no free virtual disk device slots.".into(),
            ));
        };
        let vdi_ref: String = self
            .request(
                "VDI.create",
                json!([
                    self.session,
                    {
                        "name_label": name,
                        "name_description": "",
                        "SR": sr_ref,
                        "virtual_size": size_bytes,
                        "type": "user",
                        "sharable": false,
                        "read_only": false,
                        "other_config": {},
                        "sm_config": {},
                        "tags": []
                    }
                ]),
            )
            .await?;
        let vbd = json!({
            "VM": vm_ref,
            "VDI": vdi_ref,
            "userdevice": userdevice,
            "bootable": false,
            "mode": "RW",
            "type": "Disk",
            "empty": false,
            "unpluggable": true,
            "device": "",
            "other_config": {"owner": "true"},
            "qos_algorithm_type": "",
            "qos_algorithm_params": {}
        });
        if let Err(error) = self
            .async_task("Async.VBD.create", json!([self.session, vbd]))
            .await
        {
            let _ = self
                .request::<Value>("VDI.destroy", json!([self.session, vdi_ref]))
                .await;
            return Err(error);
        }
        Ok(())
    }

    /// Attach an existing unattached user VDI as a disk on a halted VM.
    pub async fn attach_vdi(
        &self,
        vm_uuid: &str,
        vdi_uuid: &str,
        bootable: bool,
    ) -> Result<(), ApiError> {
        if self.settings.read_only {
            return Err(ApiError::Xen(
                "This connection is read-only. Changes are blocked by the Rust backend.".into(),
            ));
        }
        let vm_ref: String = self
            .request("VM.get_by_uuid", json!([self.session, vm_uuid]))
            .await?;
        let vm: Value = self
            .request("VM.get_record", json!([self.session, vm_ref]))
            .await?;
        if vm["uuid"].as_str() != Some(vm_uuid)
            || vm["is_control_domain"].as_bool() == Some(true)
            || vm["is_a_template"].as_bool() == Some(true)
            || vm["is_a_snapshot"].as_bool() == Some(true)
        {
            return Err(ApiError::Xen(
                "Disks can only be attached to a virtual machine.".into(),
            ));
        }
        if vm["power_state"].as_str() != Some("Halted") {
            return Err(ApiError::Xen(
                "Shut down the VM before attaching a disk.".into(),
            ));
        }
        let vdi_ref: String = self
            .request("VDI.get_by_uuid", json!([self.session, vdi_uuid]))
            .await?;
        let vdi: Value = self
            .request("VDI.get_record", json!([self.session, vdi_ref]))
            .await?;
        if vdi["uuid"].as_str() != Some(vdi_uuid) || !attachable_vdi(&vdi) {
            return Err(ApiError::Xen(
                "Select an available writable user disk.".into(),
            ));
        }
        let vbds = self.records("VBD").await?;
        if vbds.values().any(|vbd| {
            vbd["VDI"].as_str() == Some(vdi_ref.as_str())
                && vbd["VM"].as_str() == Some(vm_ref.as_str())
        }) {
            return Err(ApiError::Xen(
                "This disk is already attached to the VM.".into(),
            ));
        }
        if vbds
            .values()
            .any(|vbd| vbd["VDI"].as_str() == Some(vdi_ref.as_str()))
        {
            return Err(ApiError::Xen(
                "This disk is attached to another VM. Detach it first.".into(),
            ));
        }
        let devices: Vec<String> = self
            .request("VM.get_allowed_VBD_devices", json!([self.session, vm_ref]))
            .await?;
        let Some(userdevice) = devices.first().cloned() else {
            return Err(ApiError::Xen(
                "This VM has no free virtual disk device slots.".into(),
            ));
        };
        let mode = vbd_mode(vdi["read_only"].as_bool() == Some(true));
        let vbd = json!({
            "VM": vm_ref,
            "VDI": vdi_ref,
            "userdevice": userdevice,
            "bootable": bootable,
            "mode": mode,
            "type": "Disk",
            "empty": false,
            "unpluggable": true,
            "device": "",
            "other_config": {"owner": "true"},
            "qos_algorithm_type": "",
            "qos_algorithm_params": {}
        });
        self.async_task("Async.VBD.create", json!([self.session, vbd]))
            .await
    }

    pub async fn set_vbd_bootable(
        &self,
        vm_uuid: &str,
        vbd_uuid: &str,
        bootable: bool,
    ) -> Result<(), ApiError> {
        if self.settings.read_only {
            return Err(ApiError::Xen(
                "This connection is read-only. Changes are blocked by the Rust backend.".into(),
            ));
        }
        let vm_ref: String = self
            .request("VM.get_by_uuid", json!([self.session, vm_uuid]))
            .await?;
        let vbd_ref: String = self
            .request("VBD.get_by_uuid", json!([self.session, vbd_uuid]))
            .await?;
        let vbd: Value = self
            .request("VBD.get_record", json!([self.session, vbd_ref]))
            .await?;
        if vbd["uuid"].as_str() != Some(vbd_uuid) || vbd["VM"].as_str() != Some(vm_ref.as_str()) {
            return Err(ApiError::Xen(
                "The selected disk does not belong to this VM.".into(),
            ));
        }
        self.request::<Value>("VBD.set_bootable", json!([self.session, vbd_ref, bootable]))
            .await?;
        Ok(())
    }

    pub async fn resize_disk(
        &self,
        vm_uuid: &str,
        vdi_uuid: &str,
        size_bytes: u64,
    ) -> Result<(), ApiError> {
        if self.settings.read_only {
            return Err(ApiError::Xen(
                "This connection is read-only. Changes are blocked by the Rust backend.".into(),
            ));
        }
        let vm_ref: String = self
            .request("VM.get_by_uuid", json!([self.session, vm_uuid]))
            .await?;
        let vm: Value = self
            .request("VM.get_record", json!([self.session, vm_ref]))
            .await?;
        if vm["power_state"].as_str() != Some("Halted") {
            return Err(ApiError::Xen(
                "Shut down the VM before resizing a disk.".into(),
            ));
        }
        let vdi_ref: String = self
            .request("VDI.get_by_uuid", json!([self.session, vdi_uuid]))
            .await?;
        let vdi: Value = self
            .request("VDI.get_record", json!([self.session, vdi_ref]))
            .await?;
        let current = number(&vdi, "virtual_size");
        if size_bytes <= current {
            return Err(ApiError::Xen(
                "A disk can only be increased in size.".into(),
            ));
        }
        let vbds = self.records("VBD").await?;
        if !vbds.values().any(|vbd| {
            vbd["VM"].as_str() == Some(vm_ref.as_str())
                && vbd["VDI"].as_str() == Some(vdi_ref.as_str())
        }) {
            return Err(ApiError::Xen(
                "The selected disk is not attached to this VM.".into(),
            ));
        }
        self.request::<Value>("VDI.resize", json!([self.session, vdi_ref, size_bytes]))
            .await?;
        Ok(())
    }

    pub async fn remove_disk(
        &self,
        vm_uuid: &str,
        vbd_uuid: &str,
        delete_vdi: bool,
    ) -> Result<(), ApiError> {
        if self.settings.read_only {
            return Err(ApiError::Xen(
                "This connection is read-only. Changes are blocked by the Rust backend.".into(),
            ));
        }
        let vm_ref: String = self
            .request("VM.get_by_uuid", json!([self.session, vm_uuid]))
            .await?;
        let vm: Value = self
            .request("VM.get_record", json!([self.session, vm_ref]))
            .await?;
        if vm["power_state"].as_str() != Some("Halted") {
            return Err(ApiError::Xen(
                "Shut down the VM before removing a disk.".into(),
            ));
        }
        let vbd_ref: String = self
            .request("VBD.get_by_uuid", json!([self.session, vbd_uuid]))
            .await?;
        let vbd: Value = self
            .request("VBD.get_record", json!([self.session, vbd_ref]))
            .await?;
        if vbd["uuid"].as_str() != Some(vbd_uuid)
            || vbd["VM"].as_str() != Some(vm_ref.as_str())
            || vbd["type"].as_str() != Some("Disk")
        {
            return Err(ApiError::Xen(
                "The selected disk does not belong to this VM.".into(),
            ));
        }
        if delete_vdi && vbd["other_config"].get("owner").is_none() {
            return Err(ApiError::Xen(
                "This disk is shared, so only the attachment can be removed.".into(),
            ));
        }
        let vdi_ref = vbd["VDI"]
            .as_str()
            .filter(|vdi| !vdi.is_empty())
            .map(str::to_owned);
        self.async_task("Async.VBD.destroy", json!([self.session, vbd_ref]))
            .await?;
        if delete_vdi && let Some(vdi_ref) = vdi_ref {
            match self
                .request::<Value>("VDI.destroy", json!([self.session, vdi_ref]))
                .await
            {
                Ok(_) => {}
                Err(ApiError::XenCode(code, _)) if code == "HANDLE_INVALID" => {}
                Err(error) => return Err(error),
            }
        }
        Ok(())
    }

    /// Attach an empty virtual CD drive.
    pub async fn add_cd_drive(&self, vm_uuid: &str) -> Result<(), ApiError> {
        if self.settings.read_only {
            return Err(ApiError::Xen(
                "This connection is read-only. Changes are blocked by the Rust backend.".into(),
            ));
        }
        let vm_ref: String = self
            .request("VM.get_by_uuid", json!([self.session, vm_uuid]))
            .await?;
        let vm: Value = self
            .request("VM.get_record", json!([self.session, vm_ref]))
            .await?;
        if vm["uuid"].as_str() != Some(vm_uuid)
            || vm["is_control_domain"].as_bool() == Some(true)
            || vm["is_a_template"].as_bool() == Some(true)
            || vm["is_a_snapshot"].as_bool() == Some(true)
        {
            return Err(ApiError::Xen(
                "CD drives can only be added to a virtual machine.".into(),
            ));
        }
        if vm["power_state"].as_str() != Some("Halted") {
            return Err(ApiError::Xen(
                "Shut down the VM before adding a CD drive.".into(),
            ));
        }
        let devices: Vec<String> = self
            .request("VM.get_allowed_VBD_devices", json!([self.session, vm_ref]))
            .await?;
        let Some(userdevice) = devices
            .iter()
            .find(|device| device.as_str() == "3")
            .or_else(|| devices.first())
            .cloned()
        else {
            return Err(ApiError::Xen(
                "This VM has no free virtual device slots.".into(),
            ));
        };
        let vbd = json!({
            "VM": vm_ref,
            "VDI": "OpaqueRef:NULL",
            "userdevice": userdevice,
            "bootable": false,
            "mode": "RO",
            "type": "CD",
            "empty": true,
            "unpluggable": true,
            "device": "",
            "other_config": {},
            "qos_algorithm_type": "",
            "qos_algorithm_params": {}
        });
        self.async_task("Async.VBD.create", json!([self.session, vbd]))
            .await
    }

    /// Create a VIF on the next free device slot.
    pub async fn add_vif(
        &self,
        vm_uuid: &str,
        network_uuid: &str,
        mac: Option<&str>,
    ) -> Result<(), ApiError> {
        if self.settings.read_only {
            return Err(ApiError::Xen(
                "This connection is read-only. Changes are blocked by the Rust backend.".into(),
            ));
        }
        let vm_ref: String = self
            .request("VM.get_by_uuid", json!([self.session, vm_uuid]))
            .await?;
        let vm: Value = self
            .request("VM.get_record", json!([self.session, vm_ref]))
            .await?;
        if vm["uuid"].as_str() != Some(vm_uuid)
            || vm["is_control_domain"].as_bool() == Some(true)
            || vm["is_a_template"].as_bool() == Some(true)
            || vm["is_a_snapshot"].as_bool() == Some(true)
        {
            return Err(ApiError::Xen(
                "Network interfaces can only be added to a virtual machine.".into(),
            ));
        }
        let network_ref: String = self
            .request("network.get_by_uuid", json!([self.session, network_uuid]))
            .await?;
        let network: Value = self
            .request("network.get_record", json!([self.session, network_ref]))
            .await?;
        if network["uuid"].as_str() != Some(network_uuid) {
            return Err(ApiError::Protocol);
        }
        let vifs = self.records("VIF").await?;
        let mut used = std::collections::BTreeSet::new();
        for vif in vifs.values() {
            if vif["VM"].as_str() == Some(vm_ref.as_str())
                && let Ok(device) = text(vif, "device").parse::<u8>()
            {
                used.insert(device);
            }
        }
        let Some(device) = (0..4).find(|device| !used.contains(device)) else {
            return Err(ApiError::Xen(
                "This VM already has the maximum number of network interfaces.".into(),
            ));
        };
        let mac = mac.unwrap_or_default().trim();
        if !mac.is_empty()
            && (mac.len() != 17
                || !mac
                    .chars()
                    .all(|c| c.is_ascii_hexdigit() || c == ':' || c == '-'))
        {
            return Err(ApiError::Xen(
                "Enter a MAC address like 02:00:00:00:00:01, or leave it empty to generate one."
                    .into(),
            ));
        }
        self.request::<Value>(
            "VIF.create",
            json!([
                self.session,
                {
                    "VM": vm_ref,
                    "network": network_ref,
                    "device": device.to_string(),
                    "MAC": mac,
                    "MTU": 0,
                    "other_config": {},
                    "qos_algorithm_type": "",
                    "qos_algorithm_params": {},
                    "locking_mode": "network_default",
                    "ipv4_allowed": [],
                    "ipv6_allowed": []
                }
            ]),
        )
        .await?;
        Ok(())
    }

    pub async fn update_vif(
        &self,
        vm_uuid: &str,
        vif_uuid: &str,
        network_uuid: &str,
        locking_mode: &str,
    ) -> Result<(), ApiError> {
        let vif_ref = self.vif_for_vm(vm_uuid, vif_uuid).await?;
        let vif: Value = self
            .request("VIF.get_record", json!([self.session, vif_ref]))
            .await?;
        if !matches!(locking_mode, "network_default" | "locked" | "unlocked") {
            return Err(ApiError::Xen("Unknown VIF locking mode.".into()));
        }
        let current_network = text(&vif, "network");
        let network_ref: String = self
            .request("network.get_by_uuid", json!([self.session, network_uuid]))
            .await?;
        let network: Value = self
            .request("network.get_record", json!([self.session, network_ref]))
            .await?;
        if network["uuid"].as_str() != Some(network_uuid) {
            return Err(ApiError::Protocol);
        }
        if current_network != network_ref {
            self.async_task(
                "Async.VIF.move",
                json!([self.session, vif_ref, network_ref]),
            )
            .await?;
        }
        let current_locking = text(&vif, "locking_mode");
        if current_locking != locking_mode {
            self.async_task(
                "Async.VIF.set_locking_mode",
                json!([self.session, vif_ref, locking_mode]),
            )
            .await?;
        }
        Ok(())
    }

    /// Read the parent VM and its current power state before reverting a
    /// snapshot, so the workspace can restore it afterwards.
    pub async fn revert_restore_hint(
        &self,
        snapshot_uuid: &str,
    ) -> Result<Option<(String, String)>, ApiError> {
        let snapshot_ref: String = self
            .request("VM.get_by_uuid", json!([self.session, snapshot_uuid]))
            .await?;
        let snapshot: Value = self
            .request("VM.get_record", json!([self.session, snapshot_ref]))
            .await?;
        if snapshot["is_a_snapshot"].as_bool() != Some(true) {
            return Ok(None);
        }
        let parent_ref = snapshot["snapshot_of"].as_str().unwrap_or_default();
        if parent_ref.is_empty() || parent_ref == "OpaqueRef:NULL" {
            return Ok(None);
        }
        let power = snapshot["snapshot_info"]["power-state-at-snapshot"]
            .as_str()
            .unwrap_or_default()
            .to_owned();
        if power != "Running" {
            return Ok(None);
        }
        let parent_uuid: String = self
            .request("VM.get_uuid", json!([self.session, parent_ref]))
            .await?;
        if parent_uuid.is_empty() {
            return Ok(None);
        }
        Ok(Some((parent_uuid, power)))
    }

    /// Reject a start when a configured disk or CD media points at a missing
    /// VDI or an unavailable storage repository.
    async fn preflight_boot(&self, vm_ref: &str) -> Result<(), ApiError> {
        let Ok(vbds) = self.records("VBD").await else {
            return Ok(());
        };
        let Ok(vdis) = self.records("VDI").await else {
            return Ok(());
        };
        let Ok(srs) = self.records("SR").await else {
            return Ok(());
        };
        for vbd in vbds.values() {
            if vbd["VM"].as_str() != Some(vm_ref) || vbd["empty"].as_bool() == Some(true) {
                continue;
            }
            let vdi_ref = vbd["VDI"].as_str().unwrap_or_default();
            if vdi_ref.is_empty() || vdi_ref == "OpaqueRef:NULL" {
                continue;
            }
            let device = text(vbd, "device");
            let Some(vdi) = vdis.get(vdi_ref) else {
                return Err(ApiError::Xen(format!(
                    "Cannot start: disk {device} points to a VDI that no longer exists."
                )));
            };
            if vdi["missing"].as_bool() == Some(true) {
                return Err(ApiError::Xen(format!(
                    "Cannot start: disk {device} uses missing VDI {}.",
                    text(vdi, "name_label")
                )));
            }
            let sr_ref = text(vdi, "SR");
            let Some(sr) = srs.get(&sr_ref) else {
                return Err(ApiError::Xen(format!(
                    "Cannot start: the storage repository for disk {device} is unavailable."
                )));
            };
            if sr["PBDs"].as_array().is_none_or(Vec::is_empty) {
                return Err(ApiError::Xen(format!(
                    "Cannot start: storage repository {} has no attached PBD.",
                    text(sr, "name_label")
                )));
            }
        }
        Ok(())
    }

    pub async fn wait_task(&self, reference: &str) -> Result<(), ApiError> {
        wait_task(self, reference).await
    }

    pub async fn remove_vif(&self, vm_uuid: &str, vif_uuid: &str) -> Result<(), ApiError> {
        let vif_ref = self.vif_for_vm(vm_uuid, vif_uuid).await?;
        self.async_task("Async.VIF.destroy", json!([self.session, vif_ref]))
            .await
    }

    pub async fn set_vif_plugged(
        &self,
        vm_uuid: &str,
        vif_uuid: &str,
        plugged: bool,
    ) -> Result<(), ApiError> {
        let vif_ref = self.vif_for_vm(vm_uuid, vif_uuid).await?;
        let vif: Value = self
            .request("VIF.get_record", json!([self.session, vif_ref]))
            .await?;
        let attached = vif["currently_attached"].as_bool().unwrap_or(false);
        if attached == plugged {
            return Ok(());
        }
        let method = if plugged {
            "Async.VIF.plug"
        } else {
            "Async.VIF.unplug"
        };
        self.async_task(method, json!([self.session, vif_ref]))
            .await
    }

    async fn vif_for_vm(&self, vm_uuid: &str, vif_uuid: &str) -> Result<String, ApiError> {
        if self.settings.read_only {
            return Err(ApiError::Xen(
                "This connection is read-only. Changes are blocked by the Rust backend.".into(),
            ));
        }
        let vm_ref: String = self
            .request("VM.get_by_uuid", json!([self.session, vm_uuid]))
            .await?;
        let vif_ref: String = self
            .request("VIF.get_by_uuid", json!([self.session, vif_uuid]))
            .await?;
        let vif: Value = self
            .request("VIF.get_record", json!([self.session, vif_ref]))
            .await?;
        if vif["VM"].as_str() != Some(vm_ref.as_str()) {
            return Err(ApiError::Xen(
                "The selected network interface does not belong to this VM.".into(),
            ));
        }
        Ok(vif_ref)
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

fn attachable_vdi(vdi: &Value) -> bool {
    text(vdi, "type") == "user" && vdi["missing"].as_bool() != Some(true)
}

fn vbd_mode(read_only: bool) -> &'static str {
    if read_only { "RO" } else { "RW" }
}

fn valid_firmware(value: &str) -> bool {
    matches!(value, "bios" | "uefi")
}

fn valid_after_crash(value: &str) -> bool {
    matches!(value, "restart" | "destroy" | "preserve" | "rename_restart")
}

fn valid_boot_order(order: &str) -> bool {
    !order.is_empty()
        && order.len() <= 3
        && order.chars().all(|c| matches!(c, 'c' | 'd' | 'n'))
        && order
            .chars()
            .collect::<std::collections::HashSet<_>>()
            .len()
            == order.len()
}

fn secure_boot_requires_uefi(firmware: &str, secure_boot: Option<bool>) -> bool {
    secure_boot == Some(true) && firmware != "uefi"
}

fn text(record: &Value, key: &str) -> String {
    record[key].as_str().unwrap_or_default().to_owned()
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
    fn attach_accepts_only_available_user_disks() {
        assert!(attachable_vdi(&json!({"type":"user","missing":false})));
        assert!(!attachable_vdi(&json!({"type":"iso","missing":false})));
        assert!(!attachable_vdi(&json!({"type":"user","missing":true})));
        assert_eq!(vbd_mode(false), "RW");
        assert_eq!(vbd_mode(true), "RO");
    }

    #[test]
    fn boot_option_validation_rejects_unsafe_combinations() {
        assert!(valid_firmware("bios"));
        assert!(valid_firmware("uefi"));
        assert!(!valid_firmware("coreboot"));
        assert!(valid_boot_order("dc"));
        assert!(valid_boot_order("ncd"));
        assert!(!valid_boot_order("cc"));
        assert!(!valid_boot_order("dx"));
        assert!(valid_after_crash("restart"));
        assert!(valid_after_crash("rename_restart"));
        assert!(!valid_after_crash("ignore"));
        assert!(secure_boot_requires_uefi("bios", Some(true)));
        assert!(!secure_boot_requires_uefi("uefi", Some(true)));
        assert!(!secure_boot_requires_uefi("bios", Some(false)));
    }

    #[test]
    fn host_actions_use_async_host_methods() {
        assert_eq!(HostAction::Enable.method(), "Async.host.enable");
        assert_eq!(HostAction::Disable.method(), "Async.host.disable");
        assert_eq!(HostAction::PowerOn.method(), "Async.host.power_on");
        assert_eq!(HostAction::Evacuate.method(), "Async.host.evacuate");
        assert_eq!(HostAction::Reboot.method(), "Async.host.reboot");
        assert_eq!(HostAction::Shutdown.method(), "Async.host.shutdown");
        assert_eq!(
            HostAction::RestartToolstack.method(),
            "Async.host.restart_agent"
        );
        assert!(HostAction::Reboot.destructive());
        assert!(!HostAction::Enable.destructive());
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
