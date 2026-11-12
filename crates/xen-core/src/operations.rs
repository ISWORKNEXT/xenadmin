use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::xapi::{ApiError, Connection};

/// CPU, memory and boot values. Absent optional fields retain their current values.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VmResourceUpdate {
    pub vcpus_max: u64,
    pub vcpus_at_startup: u64,
    pub memory_static_max_mib: u64,
    pub memory_dynamic_min_mib: u64,
    pub memory_dynamic_max_mib: u64,
    pub boot_order: Option<String>,
    pub firmware: Option<String>,
    pub secure_boot: Option<bool>,
    pub boot_policy: Option<String>,
    pub after_crash: Option<String>,
    pub cpu_weight: Option<u16>,
    pub cores_per_socket: Option<u64>,
    pub pv_args: Option<String>,
    pub auto_poweron: Option<bool>,
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum VmAction {
    InstallGuestTools,
    Start,
    StartPaused,
    StartPausedOn {
        host_ref: String,
    },
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
    CreateFromSnapshot {
        name: String,
        as_template: bool,
    },
    Snapshot {
        name: String,
    },
    Checkpoint {
        name: String,
    },
    SnapshotWithQuiesce {
        name: String,
    },
    WithDescription {
        action: Box<VmAction>,
        description: String,
    },
    DeleteSelected {
        disk_uuids: Vec<String>,
        snapshot_uuids: Vec<String>,
    },
    Move {
        sr_uuid: String,
    },
    ConvertToTemplate,
    Copy {
        request: crate::provisioning::CopyVmRequest,
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
            Self::InstallGuestTools => "install_guest_tools",
            Self::Start | Self::StartPaused => "start",
            Self::StartOn { .. } | Self::StartPausedOn { .. } => "start_on",
            Self::Shutdown => "clean_shutdown",
            Self::Reboot => "clean_reboot",
            Self::ForceShutdown => "hard_shutdown",
            Self::ForceReboot => "hard_reboot",
            Self::Pause => "pause",
            Self::Unpause => "unpause",
            Self::Suspend => "suspend",
            Self::Resume => "resume",
            Self::ResumeOn { .. } => "resume_on",
            Self::Clone { .. } | Self::CreateFromSnapshot { .. } => "clone",
            Self::Snapshot { .. } => "snapshot",
            Self::Checkpoint { .. } => "checkpoint",
            Self::SnapshotWithQuiesce { .. } => "snapshot_with_quiesce",
            Self::ConvertToTemplate => "make_into_template",
            Self::Copy { .. } => "copy",
            Self::Revert => "revert",
            Self::Destroy { .. } | Self::DeleteSelected { .. } => "destroy",
            Self::WithDescription { action, .. } => action.allowed_operation(),
            Self::Move { .. } => "migrate_send",
        }
    }

    pub fn title(&self) -> &'static str {
        match self {
            Self::InstallGuestTools => "Install Guest Tools",
            Self::Start => "Start",
            Self::StartPaused | Self::StartPausedOn { .. } => "Start paused",
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
            Self::CreateFromSnapshot {
                as_template: true, ..
            } => "Create template from snapshot",
            Self::CreateFromSnapshot {
                as_template: false, ..
            } => "Create VM from snapshot",
            Self::Snapshot { .. } => "Take snapshot",
            Self::Checkpoint { .. } => "Take checkpoint",
            Self::SnapshotWithQuiesce { .. } => "Take quiesced snapshot",
            Self::ConvertToTemplate => "Convert to template",
            Self::Copy { .. } => "Full copy",
            Self::Revert => "Revert snapshot",
            Self::Destroy { .. } | Self::DeleteSelected { .. } => "Delete",
            Self::WithDescription { action, .. } => action.title(),
            Self::Move { .. } => "Move VM",
        }
    }

    fn params(&self, session: &str, reference: &str) -> Result<Value, ApiError> {
        Ok(match self {
            Self::Start | Self::Resume => json!([session, reference, false, false]),
            Self::StartPaused => json!([session, reference, true, false]),
            Self::StartPausedOn { host_ref } => json!([session, reference, host_ref, true, false]),
            Self::StartOn { host_ref } | Self::ResumeOn { host_ref } => {
                json!([session, reference, host_ref, false, false])
            }
            Self::Clone { name }
            | Self::Snapshot { name }
            | Self::Checkpoint { name }
            | Self::SnapshotWithQuiesce { name }
            | Self::CreateFromSnapshot { name, .. } => {
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
        if matches!(
            action,
            VmAction::InstallGuestTools
                | VmAction::CreateFromSnapshot { .. }
                | VmAction::ConvertToTemplate
                | VmAction::Copy { .. }
                | VmAction::WithDescription { .. }
                | VmAction::DeleteSelected { .. }
                | VmAction::Move { .. }
        ) {
            return Err(ApiError::Xen(
                "This operation requires its configured workspace action.".into(),
            ));
        }
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
                | VmAction::StartPaused
                | VmAction::StartPausedOn { .. }
                | VmAction::Resume
                | VmAction::ResumeOn { .. }
        ) {
            self.preflight_boot(&reference).await?;
            let host_ref = match action {
                VmAction::StartOn { host_ref }
                | VmAction::StartPausedOn { host_ref }
                | VmAction::ResumeOn { host_ref } => Some(host_ref.clone()),
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

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SrAction {
    Scan,
    SetDefault,
    Detach,
    Trim,
    Forget,
    Destroy,
}

impl SrAction {
    pub fn title(self) -> &'static str {
        match self {
            Self::Scan => "Scan storage",
            Self::SetDefault => "Set as default storage",
            Self::Detach => "Detach storage repository",
            Self::Trim => "Reclaim storage space",
            Self::Forget => "Forget storage repository",
            Self::Destroy => "Destroy storage repository",
        }
    }

    pub fn destructive(self) -> bool {
        matches!(self, Self::Destroy | Self::Detach | Self::Forget)
    }
}

#[derive(Clone, Copy, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum HostAction {
    EnterMaintenance,
    EnterMaintenanceSuspend,
    EnterMaintenanceShutdown,
    ExitMaintenance,
    ExitMaintenanceRestore,
    Enable,
    Disable,
    PowerOn,
    Evacuate,
    Reboot,
    RebootPrepared { suspend: bool },
    Shutdown,
    ShutdownPrepared { suspend: bool },
    RestartToolstack,
    RescanPifs,
}

impl HostAction {
    fn method(self) -> &'static str {
        match self {
            Self::ExitMaintenanceRestore | Self::ExitMaintenance | Self::Enable => {
                "Async.host.enable"
            }
            Self::EnterMaintenance
            | Self::EnterMaintenanceSuspend
            | Self::EnterMaintenanceShutdown
            | Self::Disable => "Async.host.disable",
            Self::PowerOn => "Async.host.power_on",
            Self::Evacuate => "Async.host.evacuate",
            Self::Reboot | Self::RebootPrepared { .. } => "Async.host.reboot",
            Self::Shutdown | Self::ShutdownPrepared { .. } => "Async.host.shutdown",
            Self::RestartToolstack => "Async.host.restart_agent",
            Self::RescanPifs => "Async.PIF.scan",
        }
    }

    pub fn title(self) -> &'static str {
        match self {
            Self::EnterMaintenance
            | Self::EnterMaintenanceSuspend
            | Self::EnterMaintenanceShutdown => "Enter Maintenance Mode",
            Self::ExitMaintenanceRestore | Self::ExitMaintenance => "Exit Maintenance Mode",
            Self::Enable => "Enable host",
            Self::Disable => "Disable host",
            Self::PowerOn => "Power on host",
            Self::Evacuate => "Evacuate host",
            Self::Reboot | Self::RebootPrepared { .. } => "Reboot host",
            Self::Shutdown | Self::ShutdownPrepared { .. } => "Shut down host",
            Self::RestartToolstack => "Restart toolstack",
            Self::RescanPifs => "Rescan physical interfaces",
        }
    }

    pub fn destructive(self) -> bool {
        matches!(
            self,
            Self::Reboot
                | Self::RebootPrepared { .. }
                | Self::Shutdown
                | Self::ShutdownPrepared { .. }
        )
    }
}

impl Connection {
    pub async fn create_network(
        &self,
        name: &str,
        description: &str,
        mtu: u64,
    ) -> Result<(), ApiError> {
        self.create_private_network(name, description, mtu, true)
            .await
    }

    pub async fn create_private_network(
        &self,
        name: &str,
        description: &str,
        mtu: u64,
        autoplug: bool,
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
                    "other_config": {"automatic": autoplug.to_string(), "autoplug": autoplug.to_string()},
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
        if network["VIFs"]
            .as_array()
            .is_some_and(|vifs| !vifs.is_empty())
        {
            return Err(ApiError::Xen(
                "Remove this network's virtual interfaces before deleting it.".into(),
            ));
        }
        let (pifs, bonds, network_sriovs, tunnels) = tokio::join!(
            self.records("PIF"),
            self.records("Bond"),
            self.records("network_sriov"),
            self.records("tunnel")
        );
        let pifs = pifs?;
        let bonds = bonds?;
        let network_sriovs = network_sriovs.unwrap_or_default();
        let tunnels = tunnels.unwrap_or_default();
        let attached_pifs: Vec<(&String, &Value)> = pifs
            .iter()
            .filter(|(_, pif)| pif["network"].as_str() == Some(network_ref.as_str()))
            .collect();
        if attached_pifs
            .iter()
            .any(|(_, pif)| pif["management"].as_bool() == Some(true))
        {
            return Err(ApiError::Xen(
                "A network carrying the management interface cannot be destroyed.".into(),
            ));
        }
        // Tear down SR-IOV logical PIFs and CHIN tunnels before the network.
        for (sriov_ref, sriov) in &network_sriovs {
            let logical_is_attached = sriov["logical_PIF"]
                .as_str()
                .and_then(|logical| pifs.get(logical))
                .is_some_and(|pif| pif["network"].as_str() == Some(network_ref.as_str()));
            if logical_is_attached {
                self.async_task(
                    "Async.Network_sriov.destroy",
                    json!([self.session, sriov_ref]),
                )
                .await?;
            }
        }
        for (tunnel_ref, tunnel) in &tunnels {
            let access_is_attached = tunnel["access_PIF"]
                .as_str()
                .and_then(|access| pifs.get(access))
                .is_some_and(|pif| pif["network"].as_str() == Some(network_ref.as_str()));
            if access_is_attached {
                self.async_task("Async.Tunnel.destroy", json!([self.session, tunnel_ref]))
                    .await?;
            }
        }
        for (_, pif) in &attached_pifs {
            let vlan_ref = pif["VLAN_master_of"].as_str().unwrap_or_default();
            if number_i64(pif, "VLAN") > 0 && !vlan_ref.is_empty() && vlan_ref != "OpaqueRef:NULL" {
                self.request::<Value>("VLAN.destroy", json!([self.session, vlan_ref]))
                    .await?;
            }
        }
        for (bond_ref, bond) in &bonds {
            let touches_network = bond["master"]
                .as_str()
                .and_then(|master| pifs.get(master))
                .is_some_and(|master| master["network"].as_str() == Some(network_ref.as_str()));
            if touches_network {
                self.async_task("Async.Bond.destroy", json!([self.session, bond_ref]))
                    .await?;
            }
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
        match action {
            SrAction::SetDefault => {
                let pools = self.records("pool").await?;
                if pools.len() != 1 {
                    return Err(ApiError::Protocol);
                }
                let (pool_ref, pool) = pools.iter().next().ok_or(ApiError::Protocol)?;
                if pool["default_SR"].as_str() == Some(sr_ref.as_str()) {
                    return Ok(());
                }
                let hosts = self.records("host").await?;
                let pbds = self.records("PBD").await?;
                if sr["content_type"].as_str() == Some("iso")
                    || !sr["allowed_operations"]
                        .as_array()
                        .is_some_and(|ops| ops.iter().any(|op| op.as_str() == Some("vdi_create")))
                    || (!sr["shared"].as_bool().unwrap_or(false) && hosts.len() > 1)
                    || !pbds.values().any(|pbd| {
                        pbd["SR"].as_str() == Some(sr_ref.as_str())
                            && pbd["currently_attached"].as_bool() == Some(true)
                    })
                {
                    return Err(ApiError::Xen("Select attached writable storage. A multi-host pool requires shared default storage.".into()));
                }
                self.request::<Value>(
                    "pool.set_default_SR",
                    json!([self.session, pool_ref, sr_ref]),
                )
                .await?;
                let actual: String = self
                    .request("pool.get_default_SR", json!([self.session, pool_ref]))
                    .await?;
                if actual != sr_ref {
                    return Err(ApiError::Xen("The pool default storage changed before readback. Refresh and review the current default.".into()));
                }
                Ok(())
            }
            SrAction::Trim => self.sr_trim(sr_uuid).await,
            SrAction::Detach => {
                self.guard_sr_not_in_use(&sr_ref).await?;
                let pbds = self.records("PBD").await?;
                for (pbd_ref, pbd) in pbds
                    .iter()
                    .filter(|(_, pbd)| pbd["SR"].as_str() == Some(sr_ref.as_str()))
                {
                    if pbd["currently_attached"].as_bool() == Some(true) {
                        self.async_task("Async.PBD.unplug", json!([self.session, pbd_ref]))
                            .await?;
                    }
                    self.async_task("Async.PBD.destroy", json!([self.session, pbd_ref]))
                        .await?;
                }
                Ok(())
            }
            SrAction::Forget => {
                self.guard_sr_not_in_use(&sr_ref).await?;
                let pbds = self.records("PBD").await?;
                for (pbd_ref, pbd) in pbds
                    .iter()
                    .filter(|(_, pbd)| pbd["SR"].as_str() == Some(sr_ref.as_str()))
                {
                    if pbd["currently_attached"].as_bool() == Some(true) {
                        self.async_task("Async.PBD.unplug", json!([self.session, pbd_ref]))
                            .await?;
                    }
                    self.async_task("Async.PBD.destroy", json!([self.session, pbd_ref]))
                        .await?;
                }
                self.request::<Value>("SR.forget", json!([self.session, sr_ref]))
                    .await?;
                Ok(())
            }
            SrAction::Destroy => {
                self.guard_sr_not_in_use(&sr_ref).await?;
                // Keep the PBD records: some backends (notably `file`) need one
                // to locate the storage while destroying it. SR.destroy removes
                // them once the backend has finished.
                let pbds = self.records("PBD").await?;
                for (pbd_ref, pbd) in pbds
                    .iter()
                    .filter(|(_, pbd)| pbd["SR"].as_str() == Some(sr_ref.as_str()))
                {
                    if pbd["currently_attached"].as_bool() == Some(true) {
                        self.async_task("Async.PBD.unplug", json!([self.session, pbd_ref]))
                            .await?;
                    }
                }
                self.async_task("Async.SR.destroy", json!([self.session, sr_ref]))
                    .await
            }
            SrAction::Scan => {
                self.async_task("Async.SR.scan", json!([self.session, sr_ref]))
                    .await
            }
        }
    }

    /// Introduce an existing file-backed SR record without touching its data.
    pub async fn introduce_sr(
        &self,
        uuid: &str,
        name: &str,
        description: &str,
        sr_type: &str,
        content_type: &str,
        shared: bool,
    ) -> Result<(), ApiError> {
        if self.settings.read_only {
            return Err(ApiError::Xen(
                "This connection is read-only. Changes are blocked by the Rust backend.".into(),
            ));
        }
        validate_sr_introduce(uuid, name, description, sr_type, content_type, shared)?;
        let srs = self.records("SR").await?;
        if srs
            .values()
            .any(|sr| sr["uuid"].as_str().is_some_and(|value| value == uuid))
        {
            return Err(ApiError::Xen(
                "A storage repository with this UUID is already managed.".into(),
            ));
        }
        self.request::<String>(
            "SR.introduce",
            json!([
                self.session,
                uuid,
                name.trim(),
                description,
                sr_type,
                content_type,
                shared
            ]),
        )
        .await?;
        Ok(())
    }

    /// Create a file-backed SR on a host. The location is an absolute path on
    /// that host's local filesystem.
    pub async fn create_sr(
        &self,
        host_uuid: &str,
        name: &str,
        description: &str,
        location: &str,
    ) -> Result<(), ApiError> {
        if self.settings.read_only {
            return Err(ApiError::Xen(
                "This connection is read-only. Changes are blocked by the Rust backend.".into(),
            ));
        }
        validate_sr_fields(name, description, location)?;
        let host_ref: String = self
            .request("host.get_by_uuid", json!([self.session, host_uuid]))
            .await?;
        let host: Value = self
            .request("host.get_record", json!([self.session, host_ref]))
            .await?;
        if host["uuid"].as_str() != Some(host_uuid) {
            return Err(ApiError::Protocol);
        }
        self.request::<String>(
            "SR.create",
            json!([
                self.session,
                host_ref,
                {"location": location},
                "0",
                name,
                description,
                "file",
                "user",
                false,
                {}
            ]),
        )
        .await?;
        Ok(())
    }

    /// Reattach a detached SR by creating/plugging a PBD for the host.
    pub async fn reattach_sr(
        &self,
        sr_uuid: &str,
        host_uuid: &str,
        location: &str,
    ) -> Result<(), ApiError> {
        if self.settings.read_only {
            return Err(ApiError::Xen(
                "This connection is read-only. Changes are blocked by the Rust backend.".into(),
            ));
        }
        validate_sr_location(location)?;
        let sr_ref: String = self
            .request("SR.get_by_uuid", json!([self.session, sr_uuid]))
            .await?;
        let host_ref: String = self
            .request("host.get_by_uuid", json!([self.session, host_uuid]))
            .await?;
        let pbds = self.records("PBD").await?;
        if let Some((pbd_ref, pbd)) = pbds.iter().find(|(_, pbd)| {
            pbd["SR"].as_str() == Some(sr_ref.as_str())
                && pbd["host"].as_str() == Some(host_ref.as_str())
        }) {
            if pbd["currently_attached"].as_bool() == Some(true) {
                return Ok(());
            }
            self.async_task("Async.PBD.plug", json!([self.session, pbd_ref]))
                .await?;
            return Ok(());
        }
        let pbd_ref: String = self
            .request(
                "PBD.create",
                json!([self.session, {
                    "host": host_ref,
                    "SR": sr_ref,
                    "device_config": {"location": location},
                    "other_config": {}
                }]),
            )
            .await?;
        self.async_task("Async.PBD.plug", json!([self.session, pbd_ref]))
            .await?;
        Ok(())
    }

    async fn sr_trim(&self, sr_uuid: &str) -> Result<(), ApiError> {
        let sr_ref_check: String = self
            .request("SR.get_by_uuid", json!([self.session, sr_uuid]))
            .await?;
        let sr: Value = self
            .request("SR.get_record", json!([self.session, sr_ref_check]))
            .await?;
        if !self
            .records("SM")
            .await?
            .values()
            .any(|sm| sm["type"] == sr["type"] && sm["features"].get("SR_TRIM").is_some())
        {
            return Err(ApiError::Xen(
                "This storage does not support reclaiming freed space.".into(),
            ));
        }
        let pbds = self.records("PBD").await?;
        let sr_ref: String = self
            .request("SR.get_by_uuid", json!([self.session, sr_uuid]))
            .await?;
        let Some((_, pbd)) = pbds.iter().find(|(_, pbd)| {
            pbd["SR"].as_str() == Some(sr_ref.as_str())
                && pbd["currently_attached"].as_bool() == Some(true)
        }) else {
            return Err(ApiError::Xen(
                "Attach the storage repository to a host before reclaiming space.".into(),
            ));
        };
        let host_ref = text(pbd, "host");
        let result: String = self
            .request(
                "host.call_plugin",
                json!([
                    self.session,
                    host_ref,
                    "trim",
                    "do_trim",
                    {"sr_uuid": sr_uuid}
                ]),
            )
            .await?;
        if trim_succeeded(&result) {
            Ok(())
        } else {
            Err(ApiError::Xen(trim_error(&result)))
        }
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
        if matches!(
            action,
            HostAction::Reboot
                | HostAction::Shutdown
                | HostAction::RebootPrepared { .. }
                | HostAction::ShutdownPrepared { .. }
        ) {
            let maintenance = match action {
                HostAction::RebootPrepared { suspend: true }
                | HostAction::ShutdownPrepared { suspend: true } => {
                    HostAction::EnterMaintenanceSuspend
                }
                HostAction::RebootPrepared { suspend: false }
                | HostAction::ShutdownPrepared { suspend: false } => {
                    HostAction::EnterMaintenanceShutdown
                }
                _ => HostAction::EnterMaintenance,
            };
            Box::pin(self.host_action(host_uuid, maintenance)).await?;
            let _: String = self
                .request(action.method(), json!([self.session, host_ref]))
                .await?;
            return Ok(());
        }
        if matches!(action, HostAction::RestartToolstack) {
            let _: String = self
                .request(action.method(), json!([self.session, host_ref]))
                .await?;
            return Ok(());
        }
        if matches!(
            action,
            HostAction::EnterMaintenance
                | HostAction::EnterMaintenanceSuspend
                | HostAction::EnterMaintenanceShutdown
        ) {
            let vms = self.records("VM").await?;
            let running = vms.values().any(|v| {
                v["resident_on"].as_str() == Some(&host_ref)
                    && v["is_control_domain"].as_bool() == Some(false)
                    && matches!(v["power_state"].as_str(), Some("Running" | "Paused"))
            });
            if running
                && self.records("host").await?.len() == 1
                && matches!(action, HostAction::EnterMaintenance)
            {
                return Err(ApiError::Xen("Running VMs must be suspended or shut down before entering maintenance mode on a standalone server.".into()));
            }
            let migrated = vms
                .values()
                .filter(|v| {
                    v["resident_on"].as_str() == Some(&host_ref)
                        && v["is_control_domain"].as_bool() == Some(false)
                })
                .filter_map(|v| v["uuid"].as_str())
                .collect::<Vec<_>>()
                .join(",");
            self.request::<Value>(
                "host.remove_from_other_config",
                json!([
                    self.session,
                    host_ref,
                    "MAINTENANCE_MODE_EVACUATED_VMS_MIGRATED"
                ]),
            )
            .await?;
            self.request::<Value>(
                "host.add_to_other_config",
                json!([
                    self.session,
                    host_ref,
                    "MAINTENANCE_MODE_EVACUATED_VMS_MIGRATED",
                    migrated
                ]),
            )
            .await?;
            self.request::<Value>(
                "host.remove_from_other_config",
                json!([self.session, host_ref, "MAINTENANCE_MODE"]),
            )
            .await?;
            self.request::<Value>(
                "host.add_to_other_config",
                json!([self.session, host_ref, "MAINTENANCE_MODE", "true"]),
            )
            .await?;
            self.async_task("Async.host.disable", json!([self.session, host_ref]))
                .await?;
            if running
                && matches!(
                    action,
                    HostAction::EnterMaintenanceSuspend | HostAction::EnterMaintenanceShutdown
                )
            {
                let mut suspended = Vec::new();
                let mut halted = Vec::new();
                for (reference, vm) in vms.iter().filter(|(_, v)| {
                    v["resident_on"].as_str() == Some(&host_ref)
                        && v["is_control_domain"].as_bool() == Some(false)
                        && matches!(v["power_state"].as_str(), Some("Running" | "Paused"))
                }) {
                    let can_suspend = matches!(action, HostAction::EnterMaintenanceSuspend)
                        && vm["allowed_operations"]
                            .as_array()
                            .is_some_and(|ops| ops.iter().any(|op| op == "suspend"));
                    let method = if can_suspend {
                        "Async.VM.suspend"
                    } else {
                        "Async.VM.clean_shutdown"
                    };
                    self.async_task(method, json!([self.session, reference]))
                        .await?;
                    if let Some(uuid) = vm["uuid"].as_str() {
                        if can_suspend {
                            suspended.push(uuid);
                        } else {
                            halted.push(uuid);
                        }
                    }
                }
                for (kind, uuids) in [("SUSPENDED", suspended), ("HALTED", halted)] {
                    let key = format!("MAINTENANCE_MODE_EVACUATED_VMS_{kind}");
                    self.request::<Value>(
                        "host.remove_from_other_config",
                        json!([self.session, host_ref, key]),
                    )
                    .await?;
                    self.request::<Value>(
                        "host.add_to_other_config",
                        json!([self.session, host_ref, key, uuids.join(",")]),
                    )
                    .await?;
                }
                self.request::<Value>(
                    "host.remove_from_other_config",
                    json!([
                        self.session,
                        host_ref,
                        "MAINTENANCE_MODE_EVACUATED_VMS_MIGRATED"
                    ]),
                )
                .await?;
            } else if running {
                self.async_task("Async.host.evacuate", json!([self.session, host_ref]))
                    .await?;
            }
            return Ok(());
        }
        if matches!(
            action,
            HostAction::ExitMaintenance | HostAction::ExitMaintenanceRestore
        ) {
            self.async_task("Async.host.enable", json!([self.session, host_ref]))
                .await?;
            self.request::<Value>(
                "host.remove_from_other_config",
                json!([self.session, host_ref, "MAINTENANCE_MODE"]),
            )
            .await?;
            if matches!(action, HostAction::ExitMaintenanceRestore) {
                let vms = self.records("VM").await?;
                for (kind, state, method) in [
                    ("MIGRATED", "Running", "Async.VM.pool_migrate"),
                    ("HALTED", "Halted", "Async.VM.start_on"),
                    ("SUSPENDED", "Suspended", "Async.VM.resume_on"),
                ] {
                    let key = format!("MAINTENANCE_MODE_EVACUATED_VMS_{kind}");
                    for uuid in host["other_config"][&key]
                        .as_str()
                        .unwrap_or_default()
                        .split(',')
                        .filter(|s| !s.is_empty())
                    {
                        if let Some((reference, vm)) = vms.iter().find(|(_, v)| {
                            v["uuid"].as_str() == Some(uuid)
                                && v["power_state"].as_str() == Some(state)
                                && v["is_control_domain"].as_bool() == Some(false)
                        }) {
                            if state == "Running" {
                                if vm["resident_on"].as_str() != Some(&host_ref) {
                                    self.async_task(
                                        method,
                                        json!([self.session,reference,host_ref,{"live":"true"}]),
                                    )
                                    .await?;
                                }
                            } else {
                                self.async_task(
                                    method,
                                    json!([self.session, reference, host_ref, false, false]),
                                )
                                .await?;
                            }
                        }
                    }
                    self.request::<Value>(
                        "host.remove_from_other_config",
                        json!([self.session, host_ref, key]),
                    )
                    .await?;
                }
            }
            return Ok(());
        }
        self.async_task(action.method(), json!([self.session, host_ref]))
            .await
    }

    /// Clone a template or VM. Templates are provisioned after cloning so the
    /// new VM receives the disks declared in `other_config.disks`.
    pub async fn clone_vm(&self, uuid: &str, name: &str) -> Result<String, ApiError> {
        self.clone_vm_impl(uuid, name, true).await
    }

    pub(crate) async fn clone_vm_unprovisioned(
        &self,
        uuid: &str,
        name: &str,
    ) -> Result<String, ApiError> {
        self.clone_vm_impl(uuid, name, false).await
    }

    async fn clone_vm_impl(
        &self,
        uuid: &str,
        name: &str,
        provision: bool,
    ) -> Result<String, ApiError> {
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

        if provision && record["is_a_template"].as_bool() == Some(true) {
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

    /// Update CPU, memory and boot settings with host-state guards.
    pub async fn update_vm_resources(
        &self,
        uuid: &str,
        resources: &VmResourceUpdate,
    ) -> Result<(), ApiError> {
        let VmResourceUpdate {
            vcpus_max,
            vcpus_at_startup,
            memory_static_max_mib,
            memory_dynamic_min_mib,
            memory_dynamic_max_mib,
            boot_order,
            firmware,
            secure_boot,
            boot_policy,
            after_crash,
            cpu_weight,
            cores_per_socket,
            pv_args,
            auto_poweron,
        } = resources.clone();
        if self.settings.read_only {
            return Err(ApiError::Xen(
                "This connection is read-only. Changes are blocked by the Rust backend.".into(),
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
        if pv_args
            .as_ref()
            .is_some_and(|value| value.contains('\0') || value.chars().count() > 32_767)
        {
            return Err(ApiError::Xen(
                "PV OS parameters must contain at most 32767 characters and no NUL characters."
                    .into(),
            ));
        }
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

        let current_vcpus_max = number(&record, "VCPUs_max");
        if cpu_weight == Some(0) {
            return Err(ApiError::Xen(
                "CPU scheduling weight must be between 1 and 65535.".into(),
            ));
        }
        if cores_per_socket.is_some() || vcpus_max != current_vcpus_max {
            let cores=match cores_per_socket {
                Some(cores)=>cores,
                None=>record["platform"]["cores-per-socket"].as_str().unwrap_or("1").parse().map_err(|_|ApiError::Xen("The existing topology is invalid. Select valid cores per socket before changing the CPU maximum.".into()))?,
            };
            let hosts = self.records("host").await?;
            let limit = if hosts.len() == 1 {
                hosts
                    .values()
                    .next()
                    .map_or(0, crate::vm::host_cores_per_socket)
            } else {
                let (vbds, vdis, srs, pbds, metrics, vms) = tokio::try_join!(
                    self.records("VBD"),
                    self.records("VDI"),
                    self.records("SR"),
                    self.records("PBD"),
                    self.records("host_metrics"),
                    self.records("VM")
                )?;
                crate::vm::max_cores_per_socket(
                    &record,
                    &crate::vm::CpuTopologyRecords {
                        hosts: &hosts,
                        vbds: &vbds,
                        vdis: &vdis,
                        srs: &srs,
                        pbds: &pbds,
                        metrics: &metrics,
                        vms: &vms,
                    },
                )
            };
            crate::vm::validate_cpu_topology(vcpus_max, cores, limit)?;
        }
        let current_weight = record["VCPUs_params"]["weight"].as_str().unwrap_or("256");
        let current_cores = record["platform"]["cores-per-socket"]
            .as_str()
            .unwrap_or("1");
        let weight_changed = cpu_weight.is_some_and(|weight| weight.to_string() != current_weight);
        let topology_changed =
            cores_per_socket.is_some_and(|cores| cores.to_string() != current_cores);
        if (weight_changed || topology_changed) && record["power_state"].as_str() != Some("Halted")
        {
            return Err(ApiError::Xen(
                "Shut down the VM before changing CPU topology or scheduling weight.".into(),
            ));
        }
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
        let current_pv_args = record["PV_args"].as_str().unwrap_or_default();
        let pv_args_changed = pv_args
            .as_ref()
            .is_some_and(|value| value != current_pv_args);
        if is_hvm && pv_args_changed {
            return Err(ApiError::Xen(
                "PV OS parameters are only available for PV guests.".into(),
            ));
        }
        let current_auto = record["other_config"]["auto_poweron"].as_str() == Some("true");
        let auto_changed = auto_poweron.is_some_and(|value| value != current_auto);
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
            (firmware.as_deref() == Some("bios")
                && current_firmware != "bios"
                && current_secure != "false")
                .then(|| "false".to_owned())
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
        let live_vcpus_changed = vcpus_changed && !halted;
        if static_changed && !halted {
            return Err(ApiError::Xen(
                "Shut down the VM before changing static memory.".into(),
            ));
        }
        let guest_records = if live_vcpus_changed {
            Some(self.records("VM_guest_metrics").await?)
        } else {
            None
        };
        if live_vcpus_changed
            && (vcpus_max != current_vcpus_max
                || vcpus_at_startup < current_vcpus_startup
                || !crate::vm::vcpu_hotplug_allowed(&record, guest_records.as_ref()))
        {
            return Err(ApiError::Xen(
                "Live CPU changes require a running supported guest and can only add vCPUs within its configured maximum. Shut down the VM for other CPU changes.".into(),
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

        if live_vcpus_changed {
            self.request::<Value>(
                "VM.set_VCPUs_number_live",
                json!([self.session, reference, vcpus_at_startup]),
            )
            .await?;
        } else if vcpus_changed {
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
        if secure_changed || topology_changed {
            let mut platform: std::collections::BTreeMap<String, String> = self
                .request("VM.get_platform", json!([self.session, reference]))
                .await?;
            if secure_changed && let Some(secure) = secure_desired {
                platform.insert("secureboot".into(), secure);
            }
            if topology_changed && let Some(cores) = cores_per_socket {
                platform.insert("cores-per-socket".into(), cores.to_string());
            }
            self.request::<Value>(
                "VM.set_platform",
                json!([self.session, reference, platform]),
            )
            .await?;
        }
        if weight_changed && let Some(weight) = cpu_weight {
            let mut params: std::collections::BTreeMap<String, String> = self
                .request("VM.get_VCPUs_params", json!([self.session, reference]))
                .await?;
            params.insert("weight".into(), weight.to_string());
            self.request::<Value>(
                "VM.set_VCPUs_params",
                json!([self.session, reference, params]),
            )
            .await?;
        }
        if weight_changed || topology_changed {
            let actual: Value = self
                .request("VM.get_record", json!([self.session, reference]))
                .await?;
            if (weight_changed
                && actual["VCPUs_params"]["weight"].as_str()
                    != cpu_weight.map(|v| v.to_string()).as_deref())
                || (topology_changed
                    && actual["platform"]["cores-per-socket"].as_str()
                        != cores_per_socket.map(|v| v.to_string()).as_deref())
            {
                return Err(ApiError::Xen("CPU settings changed before readback. Refresh and review the current configuration.".into()));
            }
        }
        if crash_changed && let Some(after_crash) = after_crash {
            self.request::<Value>(
                "VM.set_actions_after_crash",
                json!([self.session, reference, after_crash]),
            )
            .await?;
        }

        if pv_args_changed && let Some(args) = &pv_args {
            self.request::<Value>("VM.set_PV_args", json!([self.session, reference, args]))
                .await?;
        }
        if auto_changed && let Some(enabled) = auto_poweron {
            let mut other: std::collections::BTreeMap<String, String> = self
                .request("VM.get_other_config", json!([self.session, reference]))
                .await?;
            other.insert("auto_poweron".into(), enabled.to_string());
            self.request::<Value>(
                "VM.set_other_config",
                json!([self.session, reference, other]),
            )
            .await?;
        }
        if pv_args_changed || auto_changed {
            let actual: Value = self
                .request("VM.get_record", json!([self.session, reference]))
                .await?;
            if (pv_args_changed && actual["PV_args"].as_str() != pv_args.as_deref())
                || (auto_changed
                    && actual["other_config"]["auto_poweron"].as_str()
                        != auto_poweron.map(|v| v.to_string()).as_deref())
            {
                return Err(ApiError::Xen("Boot settings changed before readback. Refresh and review the current configuration.".into()));
            }
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
        self.add_disk_described(vm_uuid, sr_uuid, size_bytes, name, "")
            .await
    }

    pub async fn add_disk_described(
        &self,
        vm_uuid: &str,
        sr_uuid: &str,
        size_bytes: u64,
        name: Option<&str>,
        description: &str,
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
                        "name_description": description,
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
        self.attach_vdi_mode(vm_uuid, vdi_uuid, bootable, false)
            .await
    }

    pub async fn attach_vdi_mode(
        &self,
        vm_uuid: &str,
        vdi_uuid: &str,
        bootable: bool,
        read_only: bool,
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
        let mode = vbd_mode(read_only || vdi["read_only"].as_bool() == Some(true));
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

    /// Update VM HA restart policy, boot/shutdown delays and affinity.
    pub async fn update_vm_ha(
        &self,
        uuid: &str,
        restart_priority: &str,
        start_delay: u64,
        shutdown_delay: u64,
        affinity_host_uuid: Option<&str>,
    ) -> Result<(), ApiError> {
        if self.settings.read_only {
            return Err(ApiError::Xen(
                "This connection is read-only. Changes are blocked by the Rust backend.".into(),
            ));
        }
        if !valid_restart_priority(restart_priority) {
            return Err(ApiError::Xen(
                "Restart priority must be Never restart, Restart if possible or Always restart."
                    .into(),
            ));
        }
        if start_delay > MAX_VM_DELAY || shutdown_delay > MAX_VM_DELAY {
            return Err(ApiError::Xen(
                "Start and shutdown delays must be 7 days or fewer.".into(),
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
            || record["is_a_template"].as_bool() == Some(true)
            || record["is_a_snapshot"].as_bool() == Some(true)
        {
            return Err(ApiError::Xen(
                "HA settings can only be changed on a virtual machine.".into(),
            ));
        }
        let affinity_ref = match affinity_host_uuid
            .map(str::trim)
            .filter(|uuid| !uuid.is_empty())
        {
            Some(host_uuid) => {
                let host_ref: String = self
                    .request("host.get_by_uuid", json!([self.session, host_uuid]))
                    .await?;
                let host: Value = self
                    .request("host.get_record", json!([self.session, host_ref]))
                    .await?;
                if host["uuid"].as_str() != Some(host_uuid) {
                    return Err(ApiError::Protocol);
                }
                host_ref
            }
            None => String::new(),
        };
        let current_priority = text(&record, "ha_restart_priority");
        if current_priority != restart_priority {
            self.request::<Value>(
                "VM.set_ha_restart_priority",
                json!([self.session, reference, restart_priority]),
            )
            .await?;
        }
        if number(&record, "start_delay") != start_delay {
            self.request::<Value>(
                "VM.set_start_delay",
                json!([self.session, reference, start_delay]),
            )
            .await?;
        }
        if number(&record, "shutdown_delay") != shutdown_delay {
            self.request::<Value>(
                "VM.set_shutdown_delay",
                json!([self.session, reference, shutdown_delay]),
            )
            .await?;
        }
        let current_affinity = text(&record, "affinity");
        let current_affinity = if current_affinity == "OpaqueRef:NULL" {
            String::new()
        } else {
            current_affinity
        };
        if current_affinity != affinity_ref {
            self.request::<Value>(
                "VM.set_affinity",
                json!([self.session, reference, affinity_ref]),
            )
            .await?;
        }
        Ok(())
    }

    /// Move a data disk to another SR. Running VM disks are migrated live with
    /// VDI.pool_migrate; halted VM disks are copied and their VBD is swapped.
    pub async fn move_vdi(&self, vdi_uuid: &str, target_sr_uuid: &str) -> Result<(), ApiError> {
        if self.settings.read_only {
            return Err(ApiError::Xen("This connection is read-only.".into()));
        }
        let vdi: String = self
            .request("VDI.get_by_uuid", json!([self.session, vdi_uuid]))
            .await?;
        let record: Value = self
            .request("VDI.get_record", json!([self.session, vdi]))
            .await?;
        if record["read_only"] == true
            || record["is_a_snapshot"] == true
            || record["VBDs"]
                .as_array()
                .is_none_or(|vbds| !vbds.is_empty())
            || record["snapshots"]
                .as_array()
                .is_some_and(|snapshots| !snapshots.is_empty())
        {
            return Err(ApiError::Xen(
                "Select an unattached writable disk without snapshots.".into(),
            ));
        }
        let sr: String = self
            .request("SR.get_by_uuid", json!([self.session, target_sr_uuid]))
            .await?;
        let target: Value = self
            .request("SR.get_record", json!([self.session, sr]))
            .await?;
        if record["SR"] == sr
            || target["content_type"] == "iso"
            || !target["allowed_operations"]
                .as_array()
                .is_some_and(|ops| ops.iter().any(|op| op == "vdi_create"))
            || number(&target, "physical_size")
                .saturating_sub(number(&target, "physical_utilisation"))
                < number(&record, "virtual_size")
        {
            return Err(ApiError::Xen(
                "Choose different writable storage with enough free space.".into(),
            ));
        }
        let task: String = self
            .request("Async.VDI.copy", json!([self.session, vdi, sr]))
            .await?;
        let copied = wait_task_value(self, &task).await?;
        let new_ref = decode_ref(&copied).ok_or(ApiError::Protocol)?;
        self.delete_vdi(vdi_uuid).await.map_err(|e| {
            ApiError::Xen(format!(
                "The disk was copied to {new_ref}, but the source was retained: {e}"
            ))
        })
    }

    pub async fn move_disk(
        &self,
        vm_uuid: &str,
        vbd_uuid: &str,
        target_sr_uuid: &str,
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
        let power = text(&vm, "power_state");
        if !movable_power_state(&power) {
            return Err(ApiError::Xen(
                "Only a halted or running VM can move a disk.".into(),
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
            || text(&vbd, "type") != "Disk"
            || vbd["empty"].as_bool() == Some(true)
        {
            return Err(ApiError::Xen(
                "Only a non-empty data disk attached to this VM can be moved.".into(),
            ));
        }
        if power == "Running" && vbd["currently_attached"].as_bool() != Some(true) {
            return Err(ApiError::Xen(
                "Plug the disk into the running VM before moving it live.".into(),
            ));
        }
        let vdi_ref = text(&vbd, "VDI");
        if vdi_ref.is_empty() || vdi_ref == "OpaqueRef:NULL" {
            return Err(ApiError::Xen("This disk has no VDI to move.".into()));
        }
        let vdi: Value = self
            .request("VDI.get_record", json!([self.session, vdi_ref]))
            .await?;
        if vdi["read_only"].as_bool() == Some(true) {
            return Err(ApiError::Xen(
                "Read-only base disks cannot be moved.".into(),
            ));
        }
        if vdi["snapshots"]
            .as_array()
            .is_some_and(|snapshots| !snapshots.is_empty())
            || vdi["is_a_snapshot"].as_bool() == Some(true)
        {
            return Err(ApiError::Xen(
                "Disks with snapshots cannot be moved; remove the snapshots first.".into(),
            ));
        }
        let source_sr = text(&vdi, "SR");
        let target_sr_ref: String = self
            .request("SR.get_by_uuid", json!([self.session, target_sr_uuid]))
            .await?;
        if source_sr == target_sr_ref {
            return Err(ApiError::Xen(
                "Choose a different storage repository for the disk.".into(),
            ));
        }
        let target_sr: Value = self
            .request("SR.get_record", json!([self.session, target_sr_ref]))
            .await?;
        if target_sr["uuid"].as_str() != Some(target_sr_uuid)
            || target_sr["content_type"].as_str() == Some("iso")
        {
            return Err(ApiError::Xen(
                "Choose a writable storage repository for the disk.".into(),
            ));
        }
        let free = number(&target_sr, "physical_size")
            .saturating_sub(number(&target_sr, "physical_utilisation"));
        if free < number(&vdi, "virtual_size") {
            return Err(ApiError::Xen(
                "The target storage repository does not have enough free space.".into(),
            ));
        }
        if power == "Running" {
            return self
                .async_task(
                    "Async.VDI.pool_migrate",
                    json!([self.session, vdi_ref, target_sr_ref, {}]),
                )
                .await;
        }
        let task: String = self
            .request(
                "Async.VDI.copy",
                json!([self.session, vdi_ref, target_sr_ref]),
            )
            .await?;
        let result = wait_task_value(self, &task)
            .await
            .map_err(|error| context("The disk copy failed", error))?;
        let new_vdi_ref = decode_ref(&result).ok_or(ApiError::Protocol)?;
        let vbd_json = |vdi: &str| {
            json!({
                "VM": vm_ref,
                "VDI": vdi,
                "userdevice": text(&vbd, "userdevice"),
                "bootable": vbd["bootable"].as_bool().unwrap_or(false),
                "mode": text(&vbd, "mode"),
                "type": "Disk",
                "empty": false,
                "unpluggable": vbd["unpluggable"].as_bool().unwrap_or(true),
                "device": text(&vbd, "device"),
                "other_config": vbd["other_config"].as_object().cloned().unwrap_or_default(),
                "qos_algorithm_type": text(&vbd, "qos_algorithm_type"),
                "qos_algorithm_params": vbd["qos_algorithm_params"].as_object().cloned().unwrap_or_default()
            })
        };
        if let Err(error) = self
            .async_task("Async.VBD.destroy", json!([self.session, vbd_ref]))
            .await
        {
            let _ = self
                .request::<Value>("VDI.destroy", json!([self.session, new_vdi_ref]))
                .await;
            return Err(error);
        }
        let new_vbd_ref = match self
            .request::<String>(
                "Async.VBD.create",
                json!([self.session, vbd_json(&new_vdi_ref)]),
            )
            .await
        {
            Ok(task) => match wait_task_value(self, &task).await {
                Ok(value) => decode_ref(&value),
                Err(_) => None,
            },
            Err(_) => None,
        };
        let Some(_new_vbd_ref) = new_vbd_ref else {
            // Put the original attachment back so a failed move is not destructive.
            let _ = self
                .async_task(
                    "Async.VBD.create",
                    json!([self.session, vbd_json(&vdi_ref)]),
                )
                .await;
            let _ = self
                .request::<Value>("VDI.destroy", json!([self.session, new_vdi_ref]))
                .await;
            return Err(ApiError::Xen(
                "The disk was copied but could not be attached at the new location.".into(),
            ));
        };
        match self
            .request::<Value>("VDI.destroy", json!([self.session, vdi_ref]))
            .await
        {
            Ok(_) => {}
            Err(ApiError::XenCode(code, _)) if code == "HANDLE_INVALID" => {}
            Err(error) => return Err(error),
        }
        Ok(())
    }

    /// Change a data VBD between read-only and read-write while the VM is halted.
    pub async fn set_vbd_mode(
        &self,
        vm_uuid: &str,
        vbd_uuid: &str,
        mode: &str,
    ) -> Result<(), ApiError> {
        if self.settings.read_only {
            return Err(ApiError::Xen(
                "This connection is read-only. Changes are blocked by the Rust backend.".into(),
            ));
        }
        if !valid_vbd_mode(mode) {
            return Err(ApiError::Xen("Disk mode must be RO or RW.".into()));
        }
        let vm_ref: String = self
            .request("VM.get_by_uuid", json!([self.session, vm_uuid]))
            .await?;
        let vm: Value = self
            .request("VM.get_record", json!([self.session, vm_ref]))
            .await?;
        if vm["power_state"].as_str() != Some("Halted") {
            return Err(ApiError::Xen(
                "Shut down the VM before changing a disk mode.".into(),
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
            || text(&vbd, "type") != "Disk"
        {
            return Err(ApiError::Xen(
                "Only a data disk attached to this VM can change mode.".into(),
            ));
        }
        if mode == "RW" {
            let vdi_ref = text(&vbd, "VDI");
            if !vdi_ref.is_empty() && vdi_ref != "OpaqueRef:NULL" {
                let vdi: Value = self
                    .request("VDI.get_record", json!([self.session, vdi_ref]))
                    .await?;
                if vdi["read_only"].as_bool() == Some(true) {
                    return Err(ApiError::Xen(
                        "This disk is read-only media and cannot be attached read-write.".into(),
                    ));
                }
            }
        }
        if text(&vbd, "mode") == mode {
            return Ok(());
        }
        self.request::<Value>("VBD.set_mode", json!([self.session, vbd_ref, mode]))
            .await?;
        Ok(())
    }

    /// Hot-plug or unplug a data disk from a running VM.
    pub async fn set_vbd_plugged(
        &self,
        vm_uuid: &str,
        vbd_uuid: &str,
        plugged: bool,
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
        if vm["power_state"].as_str() != Some("Running") {
            return Err(ApiError::Xen(
                "Start the VM before plugging or unplugging a disk.".into(),
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
            || text(&vbd, "type") != "Disk"
        {
            return Err(ApiError::Xen(
                "Only a data disk attached to this VM can be plugged or unplugged.".into(),
            ));
        }
        if vbd["currently_attached"].as_bool().unwrap_or(false) == plugged {
            return Ok(());
        }
        let method = if plugged {
            "Async.VBD.plug"
        } else {
            "Async.VBD.unplug"
        };
        self.async_task(method, json!([self.session, vbd_ref]))
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
        match vm["power_state"].as_str() {
            Some("Halted" | "Running") => {}
            _ => {
                return Err(ApiError::Xen(
                    "Resume or shut down the VM before resizing a disk.".into(),
                ));
            }
        };
        if vm["is_control_domain"].as_bool() != Some(false)
            || vm["is_a_template"].as_bool() != Some(false)
            || vm["is_a_snapshot"].as_bool() != Some(false)
        {
            return Err(ApiError::Xen(
                "Only a normal VM's writable disk can be resized.".into(),
            ));
        }
        let vdi_ref: String = self
            .request("VDI.get_by_uuid", json!([self.session, vdi_uuid]))
            .await?;
        let vdi: Value = self
            .request("VDI.get_record", json!([self.session, vdi_ref]))
            .await?;
        let operation = ["resize", "resize_online"].into_iter().find(|operation| {
            vdi["allowed_operations"]
                .as_array()
                .is_some_and(|ops| ops.iter().any(|op| op.as_str() == Some(*operation)))
        });
        let Some(operation) = operation.filter(|_| vdi["read_only"].as_bool() == Some(false))
        else {
            return Err(ApiError::Xen(
                "The storage backend does not currently allow resizing this disk.".into(),
            ));
        };
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
                && vbd["type"].as_str() == Some("Disk")
                && vbd["mode"].as_str() == Some("RW")
        }) {
            return Err(ApiError::Xen(
                "The selected disk is not attached to this VM.".into(),
            ));
        }
        self.request::<Value>(
            if operation == "resize" {
                "VDI.resize"
            } else {
                "VDI.resize_online"
            },
            json!([self.session, vdi_ref, size_bytes]),
        )
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
        self.add_vif_impl(vm_uuid, network_uuid, mac, false).await
    }

    pub(crate) async fn add_imported_vif(
        &self,
        vm_uuid: &str,
        network_uuid: &str,
    ) -> Result<(), ApiError> {
        self.add_vif_impl(vm_uuid, network_uuid, None, true).await
    }

    async fn add_vif_impl(
        &self,
        vm_uuid: &str,
        network_uuid: &str,
        mac: Option<&str>,
        allow_template: bool,
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
            || (!allow_template && vm["is_a_template"].as_bool() == Some(true))
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
        let mut devices: Vec<String> = self
            .request("VM.get_allowed_VIF_devices", json!([self.session, vm_ref]))
            .await?;
        let Some(device) = next_vif_device(&mut devices) else {
            return Err(ApiError::Xen(
                "This VM has no free virtual network interface slots.".into(),
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
    pub(crate) async fn preflight_boot(&self, vm_ref: &str) -> Result<(), ApiError> {
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
        let vif: Value = self
            .request("VIF.get_record", json!([self.session, vif_ref]))
            .await?;
        if vif["currently_attached"].as_bool() == Some(true)
            && vif["allowed_operations"]
                .as_array()
                .is_some_and(|ops| ops.iter().any(|op| op.as_str() == Some("unplug")))
        {
            match self
                .async_task("Async.VIF.unplug", json!([self.session, vif_ref]))
                .await
            {
                Ok(()) => {}
                Err(ApiError::XenCode(code, _)) if code == "DEVICE_ALREADY_DETACHED" => {}
                Err(error) => return Err(error),
            }
        }
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

    /// Destroy an unattached VDI. Attached disks must be detached first.
    pub async fn delete_vdi(&self, vdi_uuid: &str) -> Result<(), ApiError> {
        if self.settings.read_only {
            return Err(ApiError::Xen(
                "This connection is read-only. Changes are blocked by the Rust backend.".into(),
            ));
        }
        let vdi_ref: String = self
            .request("VDI.get_by_uuid", json!([self.session, vdi_uuid]))
            .await?;
        let vdi: Value = self
            .request("VDI.get_record", json!([self.session, vdi_ref]))
            .await?;
        if vdi["uuid"].as_str() != Some(vdi_uuid) {
            return Err(ApiError::Protocol);
        }
        if !deletable_user_vdi(&vdi) {
            return Err(ApiError::Xen(
                "Only writable user disks can be deleted; ISO media and base disks are protected."
                    .into(),
            ));
        }
        let vbds = self.records("VBD").await?;
        if vbds
            .values()
            .any(|vbd| vbd["VDI"].as_str() == Some(vdi_ref.as_str()))
        {
            return Err(ApiError::Xen(
                "This disk is still attached to a VM. Detach it first.".into(),
            ));
        }
        self.request::<Value>("VDI.destroy", json!([self.session, vdi_ref]))
            .await?;
        Ok(())
    }

    pub(crate) async fn described_vm_action(
        &self,
        uuid: &str,
        action: &VmAction,
        description: &str,
    ) -> Result<String, ApiError> {
        if description.chars().count() > crate::properties::MAX_DESCRIPTION_CHARS {
            return Err(ApiError::Xen("Description is too long.".into()));
        }
        let reference = match action {
            VmAction::Clone { name } => {
                let cloned_uuid = self.clone_vm(uuid, name).await?;
                self.request::<String>("VM.get_by_uuid", json!([self.session, cloned_uuid]))
                    .await?
            }
            VmAction::Snapshot { .. }
            | VmAction::Checkpoint { .. }
            | VmAction::SnapshotWithQuiesce { .. } => {
                let task = self.begin_vm_action(uuid, action).await?;
                decode_ref(&wait_task_value(self, &task).await?).ok_or(ApiError::Protocol)?
            }
            _ => {
                return Err(ApiError::Xen(
                    "This action does not accept a description.".into(),
                ));
            }
        };
        self.request::<Value>(
            "VM.set_name_description",
            json!([self.session, reference, description]),
        )
        .await
        .map_err(|e| context("Object created, but setting its description failed", e))?;
        self.request("VM.get_uuid", json!([self.session, reference]))
            .await
    }

    pub(crate) async fn move_vm_disks(
        &self,
        uuid: &str,
        sr_uuid: &str,
    ) -> Result<String, ApiError> {
        let details = self.vm_details(uuid).await?;
        if details.power_state != "Halted" || details.is_template || details.is_snapshot {
            return Err(ApiError::Xen(
                "Shut down the VM before moving its disks.".into(),
            ));
        }
        for disk in details
            .disks
            .iter()
            .filter(|d| !d.is_cd && !d.empty && d.sr_uuid != sr_uuid)
        {
            self.move_disk(uuid, &disk.vbd_uuid, sr_uuid)
                .await
                .map_err(|e| {
                    context(
                        "Move stopped; previously moved disks remain on the destination SR",
                        e,
                    )
                })?;
        }
        Ok(uuid.to_owned())
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
        self.destroy_vm_selected(uuid, delete_disks, delete_snapshots, None)
            .await
    }

    pub(crate) async fn destroy_vm_selected(
        &self,
        uuid: &str,
        delete_disks: bool,
        delete_snapshots: bool,
        selection: Option<(&[String], &[String])>,
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

        if let Some((disk_ids, snapshot_ids)) = selection {
            let vdis = self.records("VDI").await?;
            let owned = vbds
                .as_ref()
                .map(|vbds| owner_vdis(vbds, &record, &reference))
                .unwrap_or_default();
            let all_vms = self.records("VM").await?;
            if disk_ids.iter().any(|id| {
                !owned
                    .iter()
                    .any(|r| vdis.get(r).is_some_and(|v| v["uuid"].as_str() == Some(id)))
            }) || snapshot_ids.iter().any(|id| {
                !all_vms.values().any(|v| {
                    v["uuid"].as_str() == Some(id)
                        && v["snapshot_of"].as_str() == Some(reference.as_str())
                })
            }) {
                return Err(ApiError::Xen(
                    "The selected disks or snapshots changed. Reopen Delete VM.".into(),
                ));
            }
        }

        let mut snapshot_failure = None;
        if delete_snapshots {
            let snapshots = self.records("VM").await?;
            for (snapshot_ref, snapshot) in &snapshots {
                if snapshot["snapshot_of"].as_str() != Some(reference.as_str())
                    || selection.is_some_and(|(_, ids)| {
                        !ids.iter().any(|id| snapshot["uuid"].as_str() == Some(id))
                    })
                {
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
            let mut vdis = vbds
                .as_ref()
                .map(|vbds| owner_vdis(vbds, &record, &reference))
                .unwrap_or_default();
            if let Some((ids, _)) = selection {
                let records = self.records("VDI").await?;
                vdis.retain(|r| {
                    record["suspend_VDI"].as_str() == Some(r)
                        || records
                            .get(r)
                            .is_some_and(|v| ids.iter().any(|id| v["uuid"].as_str() == Some(id)))
                });
            }
            vdis
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

    pub(crate) async fn async_task(&self, method: &str, params: Value) -> Result<(), ApiError> {
        let task: String = self.request(method, params).await?;
        wait_task(self, &task).await
    }
}

const MAX_VM_DELAY: u64 = 7 * 24 * 60 * 60;

fn validate_sr_introduce(
    uuid: &str,
    name: &str,
    description: &str,
    sr_type: &str,
    content_type: &str,
    shared: bool,
) -> Result<(), ApiError> {
    if uuid::Uuid::parse_str(uuid.trim()).is_err() {
        return Err(ApiError::Xen(
            "Enter the storage repository UUID in standard UUID form.".into(),
        ));
    }
    let name = name.trim();
    if name.is_empty() || name.chars().count() > 255 {
        return Err(ApiError::Xen(
            "Enter a storage repository name between 1 and 255 characters.".into(),
        ));
    }
    if description.chars().count() > 1024 {
        return Err(ApiError::Xen(
            "The description must be 1024 characters or fewer.".into(),
        ));
    }
    if sr_type != "file" {
        return Err(ApiError::Xen(
            "Only host-local file-backed storage can be introduced here.".into(),
        ));
    }
    if content_type != "user" {
        return Err(ApiError::Xen(
            "Only user storage repositories can be introduced here.".into(),
        ));
    }
    if shared {
        return Err(ApiError::Xen(
            "File-backed storage repositories cannot be shared.".into(),
        ));
    }
    Ok(())
}

fn validate_sr_fields(name: &str, description: &str, location: &str) -> Result<(), ApiError> {
    let name = name.trim();
    if name.is_empty() || name.chars().count() > 255 {
        return Err(ApiError::Xen(
            "Enter a storage repository name between 1 and 255 characters.".into(),
        ));
    }
    if description.chars().count() > 1024 {
        return Err(ApiError::Xen(
            "The description must be 1024 characters or fewer.".into(),
        ));
    }
    validate_sr_location(location)
}

fn validate_sr_location(location: &str) -> Result<(), ApiError> {
    let location = location.trim();
    if !location.starts_with('/')
        || location == "/"
        || location.contains("..")
        || location.chars().count() > 255
        || location.chars().any(char::is_control)
    {
        return Err(ApiError::Xen(
            "Enter an absolute host path such as /var/tmp/xen-storage.".into(),
        ));
    }
    Ok(())
}

fn trim_succeeded(value: &str) -> bool {
    value.trim().eq_ignore_ascii_case("true")
}

fn trim_error(value: &str) -> String {
    if let Some(message) = value
        .split("<key>errmsg</key>")
        .nth(1)
        .and_then(|rest| rest.split("<value>").nth(1))
        .and_then(|rest| rest.split("</value>").next())
    {
        return format!("Trim failed: {message}");
    }
    format!("Trim failed: {value}")
}

fn number_i64(record: &Value, key: &str) -> i64 {
    record[key]
        .as_i64()
        .or_else(|| record[key].as_str().and_then(|value| value.parse().ok()))
        .unwrap_or(-1)
}

fn movable_power_state(power: &str) -> bool {
    matches!(power, "Halted" | "Running")
}

fn valid_vbd_mode(mode: &str) -> bool {
    matches!(mode, "RO" | "RW")
}

fn deletable_user_vdi(vdi: &Value) -> bool {
    text(vdi, "type") == "user" && vdi["read_only"].as_bool() != Some(true)
}

fn valid_restart_priority(value: &str) -> bool {
    matches!(value, "" | "restart" | "best-effort" | "0" | "1")
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
        if vbd["type"].as_str() == Some("CD") || vbd["other_config"].get("owner").is_none() {
            continue;
        }
        if let Some(vdi) = vbd["VDI"].as_str()
            && !vdi.is_empty()
            && vdi != "OpaqueRef:NULL"
            && !vbds.values().any(|other| {
                other["VDI"].as_str() == Some(vdi) && other["VM"].as_str() != Some(reference)
            })
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
pub(crate) fn decode_ref(value: &Value) -> Option<String> {
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

pub(crate) async fn wait_task_value(
    connection: &Connection,
    reference: &str,
) -> Result<Value, ApiError> {
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

/// XenAPI device lists are unordered; creation fills numeric slots in order.
pub(crate) fn next_vif_device(devices: &mut [String]) -> Option<&String> {
    devices.sort_by_key(|device| device.parse::<u64>().unwrap_or(u64::MAX));
    devices.first()
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
            "vbd-owned-shared": {"VM": "vm", "VDI": "vdi-both", "other_config": {"owner": ""}},
            "vbd-second-owner": {"VM": "other", "VDI": "vdi-both", "other_config": {"owner": ""}},
            "vbd-iso": {"VM": "vm", "VDI": "iso", "type": "CD", "other_config": {"owner": ""}},
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
    fn trim_results_are_interpreted() {
        assert!(trim_succeeded("true"));
        assert!(trim_succeeded(" TRUE "));
        assert!(!trim_succeeded("false"));
        assert!(!trim_succeeded(
            "<trim_response><key_value_pair><key>errcode</key><value>UnsupportedSRForTrim</value></key_value_pair><key_value_pair><key>errmsg</key><value>Trim on SR not supported</value></key_value_pair></trim_response>"
        ));
        assert_eq!(
            trim_error(
                "<trim_response><key_value_pair><key>errmsg</key><value>Trim on SR not supported</value></key_value_pair></trim_response>"
            ),
            "Trim failed: Trim on SR not supported"
        );
    }

    #[test]
    fn sr_introduce_fields_are_validated() {
        let uuid = "cc494741-8793-c045-b2f4-c3b6fb929730";
        assert!(validate_sr_introduce(uuid, "storage", "", "file", "user", false).is_ok());
        for bad_uuid in ["", "not-a-uuid", "cc494741"] {
            assert!(validate_sr_introduce(bad_uuid, "storage", "", "file", "user", false).is_err());
        }
        assert!(validate_sr_introduce(uuid, "", "", "file", "user", false).is_err());
        assert!(validate_sr_introduce(uuid, &"x".repeat(256), "", "file", "user", false).is_err());
        assert!(
            validate_sr_introduce(uuid, "storage", &"x".repeat(1025), "file", "user", false)
                .is_err()
        );
        assert!(validate_sr_introduce(uuid, "storage", "", "ext", "user", false).is_err());
        assert!(validate_sr_introduce(uuid, "storage", "", "file", "iso", false).is_err());
        assert!(validate_sr_introduce(uuid, "storage", "", "file", "user", true).is_err());
    }

    #[test]
    fn sr_create_fields_and_locations_are_validated() {
        assert!(validate_sr_fields("storage", "", "/var/tmp/xen-storage").is_ok());
        assert!(validate_sr_fields("", "", "/var/tmp/xen-storage").is_err());
        assert!(validate_sr_fields(&"x".repeat(256), "", "/var/tmp/xen-storage").is_err());
        assert!(validate_sr_fields("storage", &"x".repeat(1025), "/var/tmp/xen-storage").is_err());
        for location in [
            "",
            "/",
            "var/tmp/xen-storage",
            "/var/../etc",
            "/var/\nstorage",
        ] {
            assert!(
                validate_sr_fields("storage", "", location).is_err(),
                "{location:?} should fail"
            );
        }
        assert!(validate_sr_location("/var/tmp/xen-storage").is_ok());
    }

    #[test]
    fn disk_move_accepts_only_halted_or_running_vms() {
        assert!(movable_power_state("Halted"));
        assert!(movable_power_state("Running"));
        assert!(!movable_power_state("Paused"));
        assert!(!movable_power_state("Suspended"));
        assert!(!movable_power_state(""));
    }

    #[test]
    fn disk_mode_validation_is_strict() {
        assert!(valid_vbd_mode("RO"));
        assert!(valid_vbd_mode("RW"));
        assert!(!valid_vbd_mode("ro"));
        assert!(!valid_vbd_mode("read-only"));
        assert!(!valid_vbd_mode(""));
    }

    #[test]
    fn only_writable_user_disks_are_deletable() {
        assert!(deletable_user_vdi(
            &json!({"type":"user","read_only":false})
        ));
        assert!(!deletable_user_vdi(
            &json!({"type":"user","read_only":true})
        ));
        assert!(!deletable_user_vdi(
            &json!({"type":"iso","read_only":false})
        ));
    }

    #[test]
    fn validates_vm_ha_restart_priority_and_delays() {
        for value in ["", "restart", "best-effort", "0", "1"] {
            assert!(valid_restart_priority(value), "{value}");
        }
        assert!(!valid_restart_priority("always"));
        assert!(!valid_restart_priority("restart-if-possible"));
        assert_eq!(MAX_VM_DELAY, 604800);
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
