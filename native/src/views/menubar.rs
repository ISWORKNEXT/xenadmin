//! Selection-driven commands shared by the main menu and toolbar.
//! No egui layout belongs here: context menus can reuse `resolve`.
use crate::state::{
    ActionKind, ObjectKey, UiAction, UiState, action_allowed, connection_inventory,
    find_connection, find_storage, find_vm, is_online, is_read_only,
};
use xen_core::{
    operations::{HostAction, SrAction},
    properties::{ObjectKind, ObjectTarget},
    provisioning::VmCreationSource,
    workspace::WorkspaceView,
};

pub struct MenuItem {
    pub label: String,
    pub host_ref: Option<String>,
    pub command: &'static str,
    pub children: Vec<MenuItem>,
}
impl MenuItem {
    pub(super) fn item(label: impl Into<String>, command: &'static str) -> Self {
        Self {
            label: label.into(),
            host_ref: None,
            command,
            children: vec![],
        }
    }
    fn submenu(label: &'static str, command: &'static str, children: Vec<Self>) -> Self {
        Self {
            label: label.into(),
            host_ref: None,
            command,
            children,
        }
    }
    pub(super) fn separator() -> Self {
        Self::item("---", "")
    }
}

pub enum CommandAction {
    Dispatch(UiAction),
    DispatchMany(Vec<UiAction>),
    Connect,
    Exit,
    Toolbar,
    TreeView(&'static str),
    Theme,
    Zoom(f32),
    AllConnections(bool),
}
pub type Resolution = Result<CommandAction, &'static str>;

pub fn selection(state: &UiState) -> &ObjectKey {
    state
        .selection
        .as_ref()
        .map_or(&ObjectKey::Home, |tab| &tab.key)
}

pub fn vm_kind(command: &str) -> Option<ActionKind> {
    Some(match command {
        "Install Guest Tools..." => ActionKind::InstallGuestTools,
        "StartVMCommand" => ActionKind::Start,
        "StartPausedVMCommand" => ActionKind::StartPaused,
        "CheckpointVMCommand" => ActionKind::Checkpoint,
        "QuiescedSnapshotVMCommand" => ActionKind::SnapshotWithQuiesce,
        "CloneVMCommand" => ActionKind::Clone,
        "ShutDownVMCommand" => ActionKind::Shutdown,
        "RebootVMCommand" => ActionKind::Reboot,
        "ResumeVMCommand" => ActionKind::Resume,
        "SuspendVMCommand" => ActionKind::Suspend,
        "PauseVMCommand" => ActionKind::Pause,
        "UnPauseVMCommand" => ActionKind::Unpause,
        "ForceVMShutDownCommand" => ActionKind::ForceShutdown,
        "ForceVMRebootCommand" => ActionKind::ForceReboot,
        "TakeSnapshotCommand" => ActionKind::Snapshot,
        "ConvertVMToTemplateCommand" => ActionKind::ConvertToTemplate,
        "DeleteVMCommand"
        | "DeleteTemplateCommand"
        | "DeleteSnapshotCommand"
        | "DeleteVMsAndTemplatesCommand" => ActionKind::Destroy,
        _ => return None,
    })
}

pub fn implemented(command: &str) -> bool {
    vm_kind(command).is_some()
        || matches!(
            command,
            "CustomTemplates"
                | "XenDesktopTemplates"
                | "LocalStorage"
                | "HiddenObjects"
                | "AddHostCommand"
                | "Exit"
                | "Toolbar"
                | "Theme"
                | "ZoomIn"
                | "ZoomOut"
                | "ZoomReset"
                | "Back"
                | "Forward"
                | "Refresh"
                | "ImportCommand"
                | "NewVMCommand"
                | "NewSRCommand"
                | "DisconnectPoolCommand"
                | "DisconnectHostCommand"
                | "ReconnectHostCommand"
                | "ConnectAllHostsCommand"
                | "DisconnectAllHostsCommand"
                | "RemoveHostCommand"
                | "RebootHostCommand"
                | "ShutDownHostCommand"
                | "PowerOnHostCommand"
                | "RestartToolstackCommand"
                | "HostMaintenanceModeCommand"
                | "PoolPropertiesCommand"
                | "HostPropertiesCommand"
                | "PropertiesCommand"
                | "NewVMFromSnapshotCommand"
                | "NewTemplateFromSnapshotCommand"
                | "ExportSnapshotAsTemplateCommand"
                | "VMPropertiesCommand"
                | "SRPropertiesCommand"
                | "TemplatePropertiesCommand"
                | "MoveVMCommand"
                | "CopyVMCommand"
                | "CopyTemplateCommand"
                | "ExportCommand"
                | "ExportTemplateCommand"
                | "NewVMFromTemplateCommand"
                | "RepairSRCommand"
                | "SetAsDefaultSRCommand"
                | "TrimSRCommand"
                | "DetachSRCommand"
                | "ForgetSRCommand"
                | "DestroySRCommand"
                | "ReattachSRCommand"
                | "AddVirtualDiskCommand"
                | "AttachVirtualDiskCommand"
                | "HAConfigureCommand"
                | "HADisableCommand"
                | "DRConfigureCommand"
                | "VMGroupCommandVMSS"
                | "DisconnectWlbServerCommand"
                | "SendCtrlAltDel"
        )
}
pub fn out_of_scope(command: &str) -> bool {
    matches!(
        command,
        "PoolReconnectAsCommand"
            | "HostReconnectAsCommand"
            | "EnableTlsVerificationCommand"
            | "CertificateCommand"
            | "InstallCertificateCommand"
            | "ResetCertificateCommand"
    )
}

pub fn resolve(command: &str, state: &UiState, workspace: &WorkspaceView) -> Resolution {
    if out_of_scope(command) {
        return Err("Excluded from this port");
    }
    if matches!(command, "MigratetoServer" | "CrossPoolMigrateCommand") {
        return Err("untested-lab: cross-pool migration is not implemented yet");
    }
    if !implemented(command) {
        return Err("Not implemented yet");
    }
    if matches!(
        command,
        "AddHostCommand"
            | "Exit"
            | "Toolbar"
            | "Theme"
            | "ZoomIn"
            | "ZoomOut"
            | "ZoomReset"
            | "CustomTemplates"
            | "XenDesktopTemplates"
            | "LocalStorage"
            | "HiddenObjects"
            | "Back"
            | "Forward"
            | "ConnectAllHostsCommand"
            | "DisconnectAllHostsCommand"
    ) {
        return resolve_single(command, state, workspace, selection(state));
    }

    if state.tree_selection.len() > 1 {
        if command == "ExportCommand" {
            let mut targets = Vec::new();
            for selected in &state.tree_selection {
                if let CommandAction::Dispatch(UiAction::OpenXvaExport {
                    connection_id,
                    vm_uuid,
                }) = resolve_single(command, state, workspace, &selected.key)?
                {
                    targets.push((connection_id, vm_uuid, selected.title.clone()));
                }
            }
            return Ok(CommandAction::Dispatch(UiAction::OpenXvaExportMany(
                targets,
            )));
        }

        let Some(kind) = vm_kind(command).filter(|kind| {
            matches!(
                kind,
                ActionKind::Start
                    | ActionKind::Shutdown
                    | ActionKind::Reboot
                    | ActionKind::Suspend
                    | ActionKind::Resume
                    | ActionKind::ForceShutdown
                    | ActionKind::ForceReboot
                    | ActionKind::Destroy
            )
        }) else {
            return Err("This command requires a single selection.");
        };
        let mut targets = Vec::new();
        let mut reason = "This selection does not support this command.";
        for selected in &state.tree_selection {
            match resolve_single(command, state, workspace, &selected.key) {
                Ok(CommandAction::Dispatch(UiAction::Propose(_, mut target))) => {
                    targets.append(&mut target)
                }
                Err(error) => reason = error,
                _ => {}
            }
        }
        if targets.is_empty() {
            return Err(reason);
        }
        return Ok(CommandAction::Dispatch(UiAction::Propose(kind, targets)));
    }
    resolve_single(command, state, workspace, selection(state))
}

fn resolve_single(
    command: &str,
    state: &UiState,
    workspace: &WorkspaceView,
    key: &ObjectKey,
) -> Resolution {
    use CommandAction::*;
    if out_of_scope(command) {
        return Err("Excluded from this port");
    }
    if matches!(command, "MigratetoServer" | "CrossPoolMigrateCommand") {
        return Err("untested-lab: cross-pool migration is not implemented yet");
    }
    if !implemented(command) {
        return Err("Not implemented yet");
    }
    match command {
        "AddHostCommand" => return Ok(Connect),
        "Exit" => return Ok(Exit),
        "Toolbar" => return Ok(Toolbar),
        "CustomTemplates" => return Ok(TreeView("CustomTemplates")),
        "XenDesktopTemplates" => return Ok(TreeView("XenDesktopTemplates")),
        "LocalStorage" => return Ok(TreeView("LocalStorage")),
        "HiddenObjects" => return Ok(TreeView("HiddenObjects")),
        "Theme" => return Ok(Theme),
        "ZoomIn" => return Ok(Zoom(1.1)),
        "ZoomOut" => return Ok(Zoom(0.9)),
        "ZoomReset" => return Ok(Zoom(1.0)),
        "Back" => {
            return if state.history_index > 0 {
                Ok(Dispatch(UiAction::NavigateHistory(false)))
            } else {
                Err("No previous selection.")
            };
        }
        "Forward" => {
            return if state.history_index + 1 < state.selection_history.len() {
                Ok(Dispatch(UiAction::NavigateHistory(true)))
            } else {
                Err("No next selection.")
            };
        }
        "ConnectAllHostsCommand" => {
            return if workspace.connections.iter().any(|c| !is_online(c)) {
                Ok(AllConnections(true))
            } else {
                Err("All servers are connected.")
            };
        }
        "DisconnectAllHostsCommand" => {
            return if workspace.connections.iter().any(is_online) {
                Ok(AllConnections(false))
            } else {
                Err("No connected servers.")
            };
        }
        _ => {}
    }
    if matches!(
        command,
        "RepairSRCommand"
            | "SetAsDefaultSRCommand"
            | "TrimSRCommand"
            | "DetachSRCommand"
            | "ForgetSRCommand"
            | "DestroySRCommand"
            | "ReattachSRCommand"
    ) && !matches!(key, ObjectKey::Storage { .. })
    {
        return Err("Select a storage repository.");
    }
    let id = key
        .connection_id()
        .ok_or("Select a connected server or pool.")?;
    let connection =
        find_connection(workspace, id).ok_or("The selected connection is unavailable.")?;
    if command == "ReconnectHostCommand" {
        return if !is_online(connection) {
            Ok(Dispatch(UiAction::Reconnect(id.into())))
        } else {
            Err("The server is already connected.")
        };
    }
    if command == "RemoveHostCommand" {
        if !matches!(key, ObjectKey::Host { .. } | ObjectKey::Pool(_)) {
            return Err("Select a server.");
        }
        return Ok(Dispatch(UiAction::ConfirmRemove(id.into())));
    }
    if !is_online(connection) {
        return Err("Connect the selected server first.");
    }
    if command == "DisconnectHostCommand" {
        let ObjectKey::Host { host_uuid, .. } = key else {
            return Err("Select a server.");
        };
        let inventory = connection_inventory(connection).ok_or("Refresh the connection first.")?;
        if inventory.hosts.len() != 1
            && !state.pool_status_state.ready().is_some_and(|status| {
                status.uuid == inventory.pool_id
                    && inventory
                        .hosts
                        .iter()
                        .any(|host| &host.uuid == host_uuid && host.name == status.master_name)
            })
        {
            return Err("Select the pool coordinator.");
        }
        return Ok(Dispatch(UiAction::Disconnect(id.into())));
    }
    if command == "DisconnectPoolCommand" {
        return Ok(Dispatch(UiAction::Disconnect(id.into())));
    }
    if command == "Refresh" {
        if state.busy.contains(id) {
            return Err("A refresh is already running.");
        }
        let mut batch = vec![UiAction::Refresh(Some(id.into()))];
        match key {
            ObjectKey::Host { .. } => batch.push(UiAction::ReloadHostDetails),
            ObjectKey::Vm { .. } | ObjectKey::Template { .. } | ObjectKey::Snapshot { .. } => {
                batch.push(UiAction::ReloadVmDetails);
                if state.vm_subtab == crate::state::VmTab::Performance {
                    batch.push(UiAction::LoadVmPerformance);
                }
            }
            _ => {}
        }
        return Ok(DispatchMany(batch));
    }
    let inventory = connection_inventory(connection).ok_or("Refresh the connection first.")?;
    if command == "PropertiesCommand" {
        let (kind, uuid) = match key {
            ObjectKey::Network { network_uuid, .. } => (ObjectKind::Network, network_uuid),
            ObjectKey::Vdi { vdi_uuid, .. } => (ObjectKind::Disk, vdi_uuid),
            _ => return Err("Select an object with properties."),
        };
        return Ok(Dispatch(UiAction::OpenProperties {
            connection_id: id.into(),
            target: ObjectTarget {
                kind,
                uuid: uuid.clone(),
            },
        }));
    }
    let properties_kind = match command {
        "PoolPropertiesCommand" => Some(ObjectKind::Pool),
        "HostPropertiesCommand" => Some(ObjectKind::Host),
        "VMPropertiesCommand" | "TemplatePropertiesCommand" => Some(ObjectKind::Vm),
        "SRPropertiesCommand" => Some(ObjectKind::Storage),
        _ => None,
    };
    if let Some(kind) = properties_kind {
        let uuid = match (command, key) {
            ("PoolPropertiesCommand", ObjectKey::Pool(_)) => &inventory.pool_id,
            ("HostPropertiesCommand", ObjectKey::Host { host_uuid, .. }) => host_uuid,
            (
                "VMPropertiesCommand",
                ObjectKey::Vm { vm_uuid, .. } | ObjectKey::Snapshot { vm_uuid, .. },
            )
            | ("TemplatePropertiesCommand", ObjectKey::Template { vm_uuid, .. }) => vm_uuid,
            ("SRPropertiesCommand", ObjectKey::Storage { sr_uuid, .. }) => sr_uuid,
            _ => return Err("Select an object of this type."),
        };
        return Ok(Dispatch(UiAction::OpenProperties {
            connection_id: id.into(),
            target: ObjectTarget {
                kind,
                uuid: uuid.clone(),
            },
        }));
    }
    if is_read_only(connection)
        && !matches!(
            command,
            "ExportCommand" | "ExportTemplateCommand" | "SendCtrlAltDel"
        )
    {
        return Err("This connection is read-only.");
    }
    if state.busy.contains(id) {
        return Err("Wait for the current connection operation.");
    }
    if let Some(kind) = vm_kind(command) {
        let uuid = match (command, key) {
            ("DeleteTemplateCommand", ObjectKey::Template { vm_uuid, .. })
            | ("DeleteSnapshotCommand", ObjectKey::Snapshot { vm_uuid, .. })
            | (
                "DeleteVMsAndTemplatesCommand",
                ObjectKey::Vm { vm_uuid, .. } | ObjectKey::Template { vm_uuid, .. },
            ) => vm_uuid,
            (_, ObjectKey::Vm { vm_uuid, .. })
                if !matches!(command, "DeleteTemplateCommand" | "DeleteSnapshotCommand") =>
            {
                vm_uuid
            }
            _ => {
                return Err(if command == "DeleteTemplateCommand" {
                    "Select a template."
                } else {
                    "Select a virtual machine."
                });
            }
        };
        let (_, vm) = find_vm(workspace, id, uuid).ok_or("The selected VM is unavailable.")?;
        if !action_allowed(workspace, id, vm, kind.operation()) {
            return Err("The VM state does not allow this operation.");
        }
        return Ok(Dispatch(UiAction::Propose(
            kind,
            vec![(id.into(), uuid.clone())],
        )));
    }
    let action = match command {
        "NewVMFromSnapshotCommand"
        | "NewTemplateFromSnapshotCommand"
        | "ExportSnapshotAsTemplateCommand" => {
            let ObjectKey::Snapshot { vm_uuid, .. } = key else {
                return Err("Select a snapshot.");
            };
            let (_, vm) = find_vm(workspace, id, vm_uuid).ok_or("The snapshot is unavailable.")?;
            if !action_allowed(
                workspace,
                id,
                vm,
                if command == "ExportSnapshotAsTemplateCommand" {
                    "export"
                } else {
                    "clone"
                },
            ) {
                return Err("The snapshot state does not allow this operation.");
            }
            if command == "ExportSnapshotAsTemplateCommand" {
                UiAction::OpenXvaExport {
                    connection_id: id.into(),
                    vm_uuid: vm_uuid.clone(),
                }
            } else {
                UiAction::Propose(
                    if command == "NewVMFromSnapshotCommand" {
                        ActionKind::SnapshotToVm
                    } else {
                        ActionKind::SnapshotToTemplate
                    },
                    vec![(id.into(), vm_uuid.clone())],
                )
            }
        }

        "ImportCommand" => UiAction::OpenXvaImport(id.into()),
        "NewVMCommand" | "NewVMFromTemplateCommand" => {
            if !inventory.hosts.iter().any(|host| host.enabled) {
                return Err("No enabled server is available.");
            }
            let template = if let ObjectKey::Template { vm_uuid, .. } = key {
                vm_uuid
            } else if command == "NewVMFromTemplateCommand" {
                return Err("Select a template.");
            } else {
                &inventory
                    .templates
                    .first()
                    .ok_or("No template is available.")?
                    .uuid
            };
            UiAction::OpenNewVmWizard {
                connection_id: id.into(),
                source: VmCreationSource::Template(template.clone()),
            }
        }
        "NewSRCommand" => UiAction::OpenSrCreate,
        "MoveVMCommand" => {
            let ObjectKey::Vm { vm_uuid, .. } = key else {
                return Err("Select a VM.");
            };
            let (_, vm) = find_vm(workspace, id, vm_uuid).ok_or("The VM is unavailable.")?;
            if vm.power_state != "Halted" || crate::state::running_task(workspace, id, vm_uuid) {
                return Err("Shut down the VM before moving its disks.");
            }
            UiAction::OpenVmMove {
                connection_id: id.into(),
                vm_uuid: vm_uuid.clone(),
            }
        }
        "CopyVMCommand" | "CopyTemplateCommand" | "ExportCommand" | "ExportTemplateCommand" => {
            let uuid = match (command, key) {
                ("CopyVMCommand" | "ExportCommand", ObjectKey::Vm { vm_uuid, .. })
                | (
                    "CopyTemplateCommand" | "ExportTemplateCommand",
                    ObjectKey::Template { vm_uuid, .. },
                ) => vm_uuid,
                _ => return Err("Select a VM or template of this type."),
            };
            let (_, vm) = find_vm(workspace, id, uuid).ok_or("The selected VM is unavailable.")?;
            let operation = if command.starts_with("Copy") {
                "copy"
            } else {
                "export"
            };
            if vm.power_state != "Halted" {
                return Err("The existing transfer dialog requires a halted VM.");
            }
            if !vm.allowed_operations.iter().any(|op| op == operation)
                || crate::state::running_task(workspace, id, uuid)
            {
                return Err("The VM state does not allow this transfer.");
            }
            if command.starts_with("Copy") {
                UiAction::OpenVmCopy {
                    connection_id: id.into(),
                    vm_uuid: uuid.clone(),
                }
            } else {
                UiAction::OpenXvaExport {
                    connection_id: id.into(),
                    vm_uuid: uuid.clone(),
                }
            }
        }
        "RebootHostCommand"
        | "ShutDownHostCommand"
        | "PowerOnHostCommand"
        | "RestartToolstackCommand"
        | "HostMaintenanceModeCommand" => {
            let ObjectKey::Host { host_uuid, .. } = key else {
                return Err("Select a server.");
            };
            let host = inventory
                .hosts
                .iter()
                .find(|h| &h.uuid == host_uuid)
                .ok_or("The selected server is unavailable.")?;
            if command == "PowerOnHostCommand" {
                // Host inventory does not contain liveness or power-on capability.
                return Err("Server power-on capability is unavailable.");
            }
            let action = match command {
                "RebootHostCommand" => HostAction::Reboot,
                "ShutDownHostCommand" => HostAction::Shutdown,
                "PowerOnHostCommand" => HostAction::PowerOn,
                "RestartToolstackCommand" => HostAction::RestartToolstack,
                _ => {
                    if host.enabled {
                        HostAction::Disable
                    } else {
                        HostAction::Enable
                    }
                }
            };
            UiAction::PromptHostAction {
                host_uuid: host.uuid.clone(),
                host_name: host.name.clone(),
                action,
            }
        }
        "RepairSRCommand"
        | "SetAsDefaultSRCommand"
        | "TrimSRCommand"
        | "DetachSRCommand"
        | "ForgetSRCommand"
        | "DestroySRCommand" => {
            let ObjectKey::Storage { sr_uuid, .. } = key else {
                return Err("Select a storage repository.");
            };
            let (_, sr) = find_storage(workspace, id, sr_uuid)
                .ok_or("The selected storage is unavailable.")?;
            if command == "SetAsDefaultSRCommand" {
                if sr.is_default {
                    return Err("This is already the default storage.");
                }
                if !sr.can_be_default
                    || sr.pbd_count == 0
                    || (!sr.shared && inventory.hosts.len() > 1)
                {
                    return Err("This storage cannot be the pool default.");
                }
            }
            if command == "RepairSRCommand" {
                if sr.pbd_count == 0 || sr.pbd_attached == sr.pbd_count {
                    return Err("There are no storage attachments to repair.");
                }
                return Ok(Dispatch(UiAction::RepairSr));
            }
            if command == "TrimSRCommand" && !sr.supports_trim {
                return Err("This storage does not support reclaiming freed space.");
            }
            if command == "ForgetSRCommand" && (!sr.can_forget || sr.has_running_vms) {
                return Err("This storage cannot be forgotten in its current state.");
            }
            if command == "DestroySRCommand" && (!sr.can_destroy || sr.has_running_vms) {
                return Err("This storage cannot be destroyed in its current state.");
            }
            if command == "TrimSRCommand" && sr.pbd_attached == 0 {
                return Err("Attach the storage first.");
            }
            if command == "DetachSRCommand" && sr.has_running_vms {
                return Err(
                    "A running VM is using this storage. Shut it down before detaching the SR.",
                );
            }
            if command == "DetachSRCommand" && sr.pbd_attached == 0 {
                return Err("The storage is already detached.");
            }
            let action = match command {
                "SetAsDefaultSRCommand" => SrAction::SetDefault,
                "TrimSRCommand" => SrAction::Trim,
                "DetachSRCommand" => SrAction::Detach,
                "ForgetSRCommand" => SrAction::Forget,
                _ => SrAction::Destroy,
            };
            UiAction::PromptSrAction {
                connection_id: id.into(),
                sr_uuid: sr.uuid.clone(),
                sr_name: sr.name.clone(),
                action,
            }
        }
        "ReattachSRCommand" => {
            let ObjectKey::Storage { sr_uuid, .. } = key else {
                return Err("Select a storage repository.");
            };
            let (_, sr) = find_storage(workspace, id, sr_uuid)
                .ok_or("The selected storage is unavailable.")?;
            if sr.pbd_count > 0 {
                return Err("The storage already has attachments.");
            }
            UiAction::OpenSrReattach
        }
        "AddVirtualDiskCommand" | "AttachVirtualDiskCommand" => {
            if command == "AddVirtualDiskCommand" && matches!(key, ObjectKey::Storage { .. }) {
                return Ok(Dispatch(UiAction::OpenAddDisk));
            }
            if !matches!(key, ObjectKey::Vm { .. }) {
                return Err("Select a virtual machine.");
            }
            if state.vm_state.ready().is_none() {
                return Err("Wait for the VM details to load.");
            }
            if command == "AddVirtualDiskCommand" {
                UiAction::OpenAddDisk
            } else {
                UiAction::OpenAttachVdi
            }
        }
        "HAConfigureCommand" => {
            if !inventory
                .storage
                .iter()
                .any(|sr| sr.shared && sr.content_type == "user")
            {
                return Err("HA requires shared user storage.");
            }
            UiAction::OpenHaEnable
        }
        "HADisableCommand" => {
            if !state
                .pool_status_state
                .ready()
                .is_some_and(|s| s.uuid == inventory.pool_id && s.ha.enabled)
            {
                return Err("Select Pool status with HA enabled.");
            }
            UiAction::PromptHaDisable
        }
        "DRConfigureCommand" => UiAction::OpenDrTaskDialog,
        "VMGroupCommandVMSS" => UiAction::OpenObject {
            key: ObjectKey::PoolStatus {
                connection_id: id.into(),
            },
            title: "Pool status".into(),
        },
        "DisconnectWlbServerCommand" => {
            if !state
                .pool_status_state
                .ready()
                .is_some_and(|s| s.uuid == inventory.pool_id && !s.wlb.url.is_empty())
            {
                return Err("Select Pool status with WLB enabled.");
            }
            UiAction::PromptWlbDeconfigure
        }
        "SendCtrlAltDel" => {
            if !matches!(key, ObjectKey::Vm { .. }) {
                return Err("Select a virtual machine.");
            }
            let console = state.console.as_ref().ok_or("Open the VM console first.")?;
            if console.connection_id != id
                || !matches!(key, ObjectKey::Vm { vm_uuid, .. } if vm_uuid == &console.vm_uuid)
            {
                return Err("Open the selected VM console first.");
            }
            if console.status != crate::state::ConsoleStatus::Connected {
                return Err("Connect the VM console first.");
            }
            return Ok(DispatchMany(
                [
                    (0xffe3, true),
                    (0xffe9, true),
                    (0xffff, true),
                    (0xffff, false),
                    (0xffe9, false),
                    (0xffe3, false),
                ]
                .into_iter()
                .map(|(keycode, down)| UiAction::ConsoleKey {
                    console_id: console.console_id.clone(),
                    keycode,
                    down,
                })
                .collect(),
            ));
        }
        _ => return Err("Not implemented yet"),
    };
    Ok(Dispatch(action))
}

pub fn menus(state: &UiState, workspace: &WorkspaceView) -> Vec<MenuItem> {
    let mut menus = vec![
        MenuItem::submenu(
            "File",
            "File",
            vec![
                MenuItem::item("Import...", "ImportCommand"),
                MenuItem::item("Import Search...", "ImportSearchCommand"),
                MenuItem::separator(),
                MenuItem::item("Import Server List...", "ImportServerList"),
                MenuItem::item("Export Server List...", "ExportServerList"),
                MenuItem::separator(),
                MenuItem::item("Exit", "Exit"),
            ],
        ),
        MenuItem::submenu(
            "View",
            "View",
            vec![
                MenuItem::item("Custom Templates", "CustomTemplates"),
                MenuItem::item("Xen Desktop Templates", "XenDesktopTemplates"),
                MenuItem::item("Local Storage", "LocalStorage"),
                MenuItem::item("Hidden Objects", "HiddenObjects"),
                MenuItem::separator(),
                MenuItem::item("Toolbar", "Toolbar"),
                MenuItem::separator(),
                MenuItem::submenu("Theme", "", vec![MenuItem::item("Light / Dark", "Theme")]),
                MenuItem::submenu(
                    "Zoom",
                    "",
                    vec![
                        MenuItem::item("Zoom In", "ZoomIn"),
                        MenuItem::item("Zoom Out", "ZoomOut"),
                        MenuItem::item("Reset Zoom", "ZoomReset"),
                    ],
                ),
                MenuItem::item("Refresh", "Refresh"),
            ],
        ),
        MenuItem::submenu(
            "Pool",
            "Pool",
            vec![
                MenuItem::item("New Pool...", "NewPoolCommand"),
                MenuItem::separator(),
                MenuItem::item("Add Server", "AddServer"),
                MenuItem::item("Remove Server", "RemoveServer"),
                MenuItem::item("Reconnect As...", "PoolReconnectAsCommand"),
                MenuItem::item("Disconnect", "DisconnectPoolCommand"),
                MenuItem::separator(),
                MenuItem::item("vApps...", "VMGroupCommandVM_appliance"),
                MenuItem::separator(),
                MenuItem::submenu(
                    "High Availability",
                    "HACommand",
                    vec![
                        MenuItem::item("Configure HA...", "HAConfigureCommand"),
                        MenuItem::item("Disable HA", "HADisableCommand"),
                    ],
                ),
                MenuItem::submenu(
                    "Disaster Recovery",
                    "DRCommand",
                    vec![
                        MenuItem::item("Configure...", "DRConfigureCommand"),
                        MenuItem::item("Disaster Recovery Wizard...", "DisasterRecoveryCommand"),
                    ],
                ),
                MenuItem::item("VM Snapshot Schedule Policies...", "VMGroupCommandVMSS"),
                MenuItem::item("Export Resource Data...", "ExportResourceReportCommand"),
                MenuItem::submenu(
                    "Workload Balancing",
                    "WlbCommand",
                    vec![
                        MenuItem::item("View Workload Reports...", "ViewWorkloadReportsCommand"),
                        MenuItem::item(
                            "Disconnect Workload Balancing Server",
                            "DisconnectWlbServerCommand",
                        ),
                    ],
                ),
                MenuItem::separator(),
                MenuItem::item("Change Server Password...", "ChangeHostPasswordCommand"),
                MenuItem::item("Rotate Pool Secret", "RotatePoolSecretCommand"),
                MenuItem::item(
                    "Enable Certificate Verification",
                    "EnableTlsVerificationCommand",
                ),
                MenuItem::separator(),
                MenuItem::item("Make into standalone server", "DeletePoolCommand"),
                MenuItem::separator(),
                MenuItem::item("Properties", "PoolPropertiesCommand"),
            ],
        ),
        MenuItem::submenu(
            "Server",
            "Server",
            vec![
                MenuItem::item("Add...", "AddHostCommand"),
                MenuItem::separator(),
                MenuItem::item("Reboot", "RebootHostCommand"),
                MenuItem::item("Power On", "PowerOnHostCommand"),
                MenuItem::item("Shut Down", "ShutDownHostCommand"),
                MenuItem::item("Restart Toolstack", "RestartToolstackCommand"),
                MenuItem::separator(),
                MenuItem::submenu(
                    "Connect/Disconnect",
                    "Connect/Disconnect",
                    vec![
                        MenuItem::item("Connect", "ReconnectHostCommand"),
                        MenuItem::item("Disconnect", "DisconnectHostCommand"),
                        MenuItem::item("Reconnect As...", "HostReconnectAsCommand"),
                        MenuItem::separator(),
                        MenuItem::item("Connect All", "ConnectAllHostsCommand"),
                        MenuItem::item("Disconnect All", "DisconnectAllHostsCommand"),
                    ],
                ),
                MenuItem::item("Add to Pool", "AddtoPool"),
                MenuItem::item("Remove From Pool", "RemoveHostFromPoolCommand"),
                MenuItem::separator(),
                MenuItem::item("Back Up...", "BackupHostCommand"),
                MenuItem::item("Restore From Backup...", "RestoreHostFromBackupCommand"),
                MenuItem::separator(),
                MenuItem::submenu(
                    "Certificate",
                    "CertificateCommand",
                    vec![
                        MenuItem::item("Install Certificate...", "InstallCertificateCommand"),
                        MenuItem::item("Reset Certificate", "ResetCertificateCommand"),
                    ],
                ),
                MenuItem::item("Enter Maintenance Mode", "HostMaintenanceModeCommand"),
                MenuItem::item(
                    "Control Domain Memory...",
                    "ChangeControlDomainMemoryCommand",
                ),
                MenuItem::submenu(
                    "Password",
                    "HostPasswordCommand",
                    vec![
                        MenuItem::item("Change...", "ChangeHostPasswordCommand"),
                        MenuItem::item("Forget Password", "ForgetSavedPasswordCommand"),
                    ],
                ),
                MenuItem::separator(),
                MenuItem::item("Destroy", "DestroyHostCommand"),
                MenuItem::item("Remove from Xen Desktop", "RemoveHostCommand"),
                MenuItem::separator(),
                MenuItem::item("Properties", "HostPropertiesCommand"),
            ],
        ),
        MenuItem::submenu(
            "VM",
            "VM",
            vec![
                MenuItem::item("New VM...", "NewVMCommand"),
                MenuItem::item("Start/Shut down", "Start/Shutdown"),
                MenuItem::item("Resume on Server", "ResumeonServer"),
                MenuItem::item("Migrate to Server", "MigratetoServer"),
                MenuItem::item("Start on Server", "StartonServer"),
                MenuItem::separator(),
                MenuItem::item(
                    "Assign to Snapshot Schedule...",
                    "AssigntoSnapshotSchedule...",
                ),
                MenuItem::item("Assign to vApp", "AssigntovApp"),
                MenuItem::separator(),
                MenuItem::item("Copy VM", "CopyVMCommand"),
                MenuItem::item("Clone VM…", "CloneVMCommand"),
                MenuItem::item("Move VM", "MoveVMCommand"),
                MenuItem::submenu(
                    "Take a Snapshot",
                    "Snapshots",
                    vec![
                        MenuItem::item("Take Snapshot…", "TakeSnapshotCommand"),
                        MenuItem::item("Snapshot with memory…", "CheckpointVMCommand"),
                        MenuItem::item("Quiesced snapshot…", "QuiescedSnapshotVMCommand"),
                    ],
                ),
                MenuItem::item("Convert to Template", "ConvertVMToTemplateCommand"),
                MenuItem::item("Export...", "ExportCommand"),
                MenuItem::separator(),
                MenuItem::item("Trusted Platform Module Manager...", "VtpmCommand"),
                MenuItem::item(
                    "Disable Changed Block Tracking",
                    "DisableChangedBlockTrackingCommand",
                ),
                MenuItem::item("Enable PVS-Accelerator...", "EnablePvsReadCachingCommand"),
                MenuItem::item("Disable PVS-Accelerator", "DisablePvsReadCachingCommand"),
                MenuItem::separator(),
                MenuItem::item("Install Guest Tools...", "Install Guest Tools..."),
                MenuItem::item("Send Ctrl+Alt+Del", "SendCtrlAltDel"),
                MenuItem::separator(),
                MenuItem::item("Delete VM", "DeleteVMCommand"),
                MenuItem::separator(),
                MenuItem::item("Properties", "VMPropertiesCommand"),
            ],
        ),
        MenuItem::submenu(
            "Storage",
            "Storage",
            vec![
                MenuItem::item("New Storage...", "NewSRCommand"),
                MenuItem::separator(),
                MenuItem::item("Repair...", "RepairSRCommand"),
                MenuItem::item("Set as Default", "SetAsDefaultSRCommand"),
                MenuItem::separator(),
                MenuItem::submenu(
                    "Virtual Disks",
                    "VirtualDiskCommand",
                    vec![
                        MenuItem::item("New Virtual Disk...", "AddVirtualDiskCommand"),
                        MenuItem::item("Attach Virtual Disk...", "AttachVirtualDiskCommand"),
                    ],
                ),
                MenuItem::item("Reclaim freed space", "TrimSRCommand"),
                MenuItem::separator(),
                MenuItem::item("Detach", "DetachSRCommand"),
                MenuItem::item("Reattach...", "ReattachSRCommand"),
                MenuItem::item("Forget", "ForgetSRCommand"),
                MenuItem::item("Destroy", "DestroySRCommand"),
                MenuItem::separator(),
                MenuItem::item("Properties", "SRPropertiesCommand"),
            ],
        ),
        MenuItem::submenu(
            "Templates",
            "Templates",
            vec![
                MenuItem::submenu(
                    "Create VM From Selection",
                    "CreateVMFromTemplateCommand",
                    vec![
                        MenuItem::item("New VM wizard...", "NewVMFromTemplateCommand"),
                        MenuItem::item("Quick Create", "InstantVMFromTemplateCommand"),
                    ],
                ),
                MenuItem::separator(),
                MenuItem::item("Export to File...", "ExportTemplateCommand"),
                MenuItem::item("Copy...", "CopyTemplateCommand"),
                MenuItem::separator(),
                MenuItem::item("Delete Template", "DeleteTemplateCommand"),
                MenuItem::separator(),
                MenuItem::item("Properties", "TemplatePropertiesCommand"),
            ],
        ),
        MenuItem::submenu(
            "Tools",
            "Tools",
            vec![MenuItem::item("Options...", "Options")],
        ),
        MenuItem::item("Window", "Window"),
        MenuItem::submenu(
            "Help",
            "Help",
            vec![
                MenuItem::item("View Xen Desktop Log Files", "ViewXenDesktopLogFiles"),
                MenuItem::separator(),
                MenuItem::item("Check for Xen Desktop Updates", "CheckforXenDesktopUpdates"),
                MenuItem::item("Xen Desktop on the Web", "XenDesktopontheWeb"),
                MenuItem::item("Xen Desktop Plugins online", "XenDesktopPluginsonline"),
                MenuItem::separator(),
                MenuItem::item("About", "About"),
            ],
        ),
    ];
    expand_dynamic(&mut menus, state, workspace);
    menus
}

fn expand_dynamic(items: &mut [MenuItem], state: &UiState, workspace: &WorkspaceView) {
    for item in items {
        match item.label.as_str() {
            "Start/Shut down" => {
                let shutdown = resolve("ShutDownVMCommand", state, workspace).is_ok();
                let resume = resolve("ResumeVMCommand", state, workspace).is_ok();
                item.children = vec![
                    MenuItem::item(
                        if shutdown { "Shut Down" } else { "Start" },
                        if shutdown {
                            "ShutDownVMCommand"
                        } else {
                            "StartVMCommand"
                        },
                    ),
                    MenuItem::item(
                        if resume { "Resume" } else { "Suspend" },
                        if resume {
                            "ResumeVMCommand"
                        } else {
                            "SuspendVMCommand"
                        },
                    ),
                    MenuItem::item("Reboot", "RebootVMCommand"),
                    MenuItem::item("Start Paused", "StartPausedVMCommand"),
                    MenuItem::item("Start in Recovery Mode", "VMRecoveryModeCommand"),
                    MenuItem::separator(),
                    MenuItem::item("Force Shut Down", "ForceVMShutDownCommand"),
                    MenuItem::item("Force Reboot", "ForceVMRebootCommand"),
                    MenuItem::separator(),
                    MenuItem::item("Start vApp", "VappStartCommand"),
                    MenuItem::item("Shut Down vApp", "VappShutDownCommand"),
                ];
            }
            "Start on Server" | "Resume on Server" => {
                let command = if item.label == "Start on Server" {
                    "StartVMOnHostCommand"
                } else {
                    "ResumeVMOnHostCommand"
                };
                item.command = command;
                if let Some(inventory) = selection(state)
                    .connection_id()
                    .and_then(|id| find_connection(workspace, id))
                    .and_then(connection_inventory)
                {
                    item.children = inventory
                        .hosts
                        .iter()
                        .flat_map(|host| {
                            let mut entry = MenuItem::item(host.name.clone(), command);
                            entry.host_ref = Some(host.reference.clone());
                            let mut entries = vec![entry];
                            if command == "StartVMOnHostCommand" {
                                let mut paused = MenuItem::item(
                                    format!("{} (paused)", host.name),
                                    "StartPausedVMOnHostCommand",
                                );
                                paused.host_ref = Some(host.reference.clone());
                                entries.push(paused);
                            }
                            entries
                        })
                        .collect();
                }
            }
            "Enter Maintenance Mode" => {
                if let ObjectKey::Host {
                    connection_id,
                    host_uuid,
                } = selection(state)
                    && let Some(inventory) =
                        find_connection(workspace, connection_id).and_then(connection_inventory)
                    && inventory
                        .hosts
                        .iter()
                        .any(|host| &host.uuid == host_uuid && !host.enabled)
                {
                    item.label = "Exit Maintenance Mode".into();
                }
            }
            _ => {}
        }
        expand_dynamic(&mut item.children, state, workspace);
    }
}

pub fn resolve_item(item: &MenuItem, state: &UiState, workspace: &WorkspaceView) -> Resolution {
    if state.tree_selection.len() > 1 && item.host_ref.is_some() {
        return Err("This command requires a single selection.");
    }
    if matches!(
        item.command,
        "StartVMOnHostCommand" | "StartPausedVMOnHostCommand" | "ResumeVMOnHostCommand"
    ) {
        let ObjectKey::Vm {
            connection_id,
            vm_uuid,
        } = selection(state)
        else {
            return Err("Select a virtual machine.");
        };
        let (connection, vm) =
            find_vm(workspace, connection_id, vm_uuid).ok_or("The selected VM is unavailable.")?;
        let kind = match item.command {
            "StartVMOnHostCommand" => ActionKind::Start,
            "StartPausedVMOnHostCommand" => ActionKind::StartPaused,
            _ => ActionKind::Resume,
        };
        let operation = if item.command == "ResumeVMOnHostCommand" {
            "resume_on"
        } else {
            "start_on"
        };
        if !action_allowed(workspace, connection_id, vm, operation) {
            return Err("The VM state does not allow this operation.");
        }
        let host_ref = item
            .host_ref
            .as_ref()
            .ok_or("Select a destination server.")?;
        let host = connection_inventory(connection)
            .and_then(|i| i.hosts.iter().find(|h| &h.reference == host_ref))
            .ok_or("The server is unavailable.")?;
        if !host.enabled {
            return Err("The destination server is in maintenance mode.");
        }
        return Ok(CommandAction::Dispatch(UiAction::ProposeOnHost(
            kind,
            vec![(connection_id.clone(), vm_uuid.clone())],
            host_ref.clone(),
            host.name.clone(),
        )));
    }
    resolve(item.command, state, workspace)
}

pub fn submenu_reason(
    item: &MenuItem,
    state: &UiState,
    workspace: &WorkspaceView,
) -> Option<&'static str> {
    if out_of_scope(item.command) {
        return Some("Excluded from this port");
    }
    if item.children.iter().filter(|c| c.label != "---").any(|c| {
        if c.children.is_empty() {
            resolve_item(c, state, workspace).is_ok()
        } else {
            submenu_reason(c, state, workspace).is_none()
        }
    }) {
        None
    } else {
        item.children.iter().find(|c| c.label != "---").map(|c| {
            if c.children.is_empty() {
                resolve_item(c, state, workspace)
                    .err()
                    .unwrap_or("Not implemented yet")
            } else {
                submenu_reason(c, state, workspace).unwrap_or("Not implemented yet")
            }
        })
    }
}
