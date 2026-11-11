use std::{path::Path, sync::Arc, thread::JoinHandle, time::Duration};

use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender, unbounded_channel};
use xen_core::{
    host::HostDetails,
    operations::VmAction,
    performance::HostPerformance,
    vm::VmDetails,
    workspace::{Workspace, WorkspaceView},
    xapi::ConnectionSettings,
};

/// Commands sent from the UI thread to the background workspace thread.
pub enum Command {
    PropertyExtras {
        seq: u64,
        connection_id: String,
        target: xen_core::properties::ObjectTarget,
        pool_uuid: Option<String>,
    },
    SavePropertiesDialog {
        seq: u64,
        dialog: Box<crate::state::PropertiesDialog>,
    },
    ObjectProperties {
        seq: u64,
        connection_id: String,
        target: xen_core::properties::ObjectTarget,
    },
    XvaBatch {
        requests: Vec<(String, xen_core::xva::XvaRequest)>,
    },
    XvaTransfer {
        connection_id: String,
        request: xen_core::xva::XvaRequest,
    },
    CreateVm {
        connection_id: String,
        request: xen_core::provisioning::CreateVmRequest,
    },
    WizardTemplate {
        seq: u64,
        connection_id: String,
        source_uuid: String,
    },
    Connect {
        settings: ConnectionSettings,
        password: String,
        profile_id: Option<String>,
    },
    Reconnect {
        connection_id: String,
    },
    Refresh {
        connection_id: String,
    },
    Disconnect {
        connection_id: String,
    },
    Remove {
        connection_id: String,
    },
    ActionBatch {
        targets: Vec<(String, String)>,
        action: VmAction,
    },
    Action {
        connection_id: String,
        vm_uuid: String,
        action: VmAction,
    },
    CheckTask {
        task_id: String,
    },
    CancelExternalTask {
        connection_id: String,
        reference: String,
    },
    CancelTask {
        task_id: String,
    },
    HostDetails {
        seq: u64,
        connection_id: String,
        host_uuid: String,
    },
    HostPerformance {
        seq: u64,
        connection_id: String,
        host_uuid: String,
        range_seconds: u64,
    },
    HostAction {
        connection_id: String,
        host_uuid: String,
        action: xen_core::operations::HostAction,
    },
    DismissAlert {
        connection_id: String,
        message_uuid: String,
    },
    SrAction {
        connection_id: String,
        sr_uuid: String,
        action: xen_core::operations::SrAction,
    },
    CreateSr {
        connection_id: String,
        host_uuid: String,
        name: String,
        description: String,
        config: xen_core::storage::SrConfig,
    },
    IntroduceSr {
        connection_id: String,
        uuid: String,
        name: String,
        description: String,
    },
    ReattachSr {
        connection_id: String,
        sr_uuid: String,
        host_uuid: String,
        location: String,
    },
    CreateNetwork {
        connection_id: String,
        name: String,
        description: String,
        mtu: u64,
    },
    DestroyNetwork {
        connection_id: String,
        network_uuid: String,
    },
    PifCatalog {
        seq: u64,
        connection_id: String,
    },
    SetPifPlugged {
        connection_id: String,
        pif_uuid: String,
        plugged: bool,
    },
    CreateSriovNetwork {
        connection_id: String,
        name: String,
        description: String,
        mtu: u64,
        autoplug: bool,
        pif_uuids: Vec<String>,
    },
    CreateChinNetwork {
        connection_id: String,
        name: String,
        description: String,
        mtu: u64,
        autoplug: bool,
        protocol: String,
        pif_uuids: Vec<String>,
    },
    CreateBond {
        connection_id: String,
        name: String,
        members: Vec<String>,
        mode: String,
        mtu: u64,
        autoplug: bool,
    },
    CreateVlan {
        connection_id: String,
        pif_uuid: String,
        tag: i64,
        name: String,
        description: String,
        mtu: u64,
        autoplug: bool,
    },
    PbdCatalog {
        seq: u64,
        connection_id: String,
        sr_uuid: String,
    },
    SetPbdPlugged {
        connection_id: String,
        pbd_uuid: String,
        plugged: bool,
    },
    RepairSr {
        connection_id: String,
        sr_uuid: String,
    },
    ProbeNewSr {
        connection_id: String,
        host_uuid: String,
        config: xen_core::storage::SrConfig,
    },
    ProbeSr {
        seq: u64,
        connection_id: String,
        host_uuid: String,
        location: String,
    },
    AuthInfo {
        seq: u64,
        connection_id: String,
    },
    PoolStatus {
        seq: u64,
        connection_id: String,
    },
    CreateDrTask {
        connection_id: String,
        sr_type: String,
        device_config: Vec<(String, String)>,
        whitelist: Vec<String>,
    },
    DestroyDrTask {
        connection_id: String,
        uuid: String,
    },
    IntroducePvsSite {
        connection_id: String,
        name: String,
        description: String,
    },
    ForgetPvsSite {
        connection_id: String,
        uuid: String,
    },
    IntroducePvsServer {
        connection_id: String,
        site_uuid: String,
        addresses: Vec<String>,
        first_port: u16,
        last_port: u16,
    },
    ForgetPvsServer {
        connection_id: String,
        uuid: String,
    },
    CreatePvsProxy {
        connection_id: String,
        site_uuid: String,
        vif_uuid: String,
    },
    DestroyPvsProxy {
        connection_id: String,
        uuid: String,
    },
    CreatePvsCache {
        connection_id: String,
        site_uuid: String,
        host_uuid: String,
        sr_uuid: String,
        size_mib: u64,
    },
    DestroyPvsCache {
        connection_id: String,
        uuid: String,
    },
    PvsVifCatalog {
        seq: u64,
        connection_id: String,
    },
    CreateVmss {
        connection_id: String,
        name: String,
        description: String,
        snapshot_type: String,
        schedule: xen_core::vmss::VmssSchedule,
        retained_snapshots: u64,
        enabled: bool,
        vm_uuids: Vec<String>,
    },
    UpdateVmss {
        connection_id: String,
        uuid: String,
        name: String,
        description: String,
        snapshot_type: String,
        schedule: xen_core::vmss::VmssSchedule,
        retained_snapshots: u64,
        enabled: bool,
        vm_uuids: Vec<String>,
    },
    DeleteVmss {
        connection_id: String,
        uuid: String,
    },
    EnableHa {
        connection_id: String,
        heartbeat_sr_uuids: Vec<String>,
        failures_to_tolerate: u64,
    },
    DisableHa {
        connection_id: String,
    },
    ConfigureWlb {
        connection_id: String,
        wlb_url: String,
        wlb_username: String,
        wlb_password: String,
        xenserver_username: String,
        xenserver_password: String,
    },
    SetWlbEnabled {
        connection_id: String,
        enabled: bool,
    },
    DeconfigureWlb {
        connection_id: String,
    },
    SetSubjectRoles {
        connection_id: String,
        subject_uuid: String,
        roles: Vec<String>,
    },
    EnableAd {
        connection_id: String,
        domain: String,
        user: String,
        password: String,
    },
    DisableAd {
        connection_id: String,
        user: Option<String>,
        password: Option<String>,
    },
    DeleteSnapshots {
        connection_id: String,
        vm_uuid: String,
        snapshot_uuids: Vec<String>,
    },
    DeleteDetails {
        targets: Vec<(String, String)>,
    },
    VmDetails {
        seq: u64,
        connection_id: String,
        vm_uuid: String,
    },
    VmPerformance {
        seq: u64,
        connection_id: String,
        vm_uuid: String,
        range_seconds: u64,
    },
    ChangeCd {
        connection_id: String,
        vm_uuid: String,
        vbd_uuid: String,
        iso_uuid: Option<String>,
    },
    AddCdDrive {
        connection_id: String,
        vm_uuid: String,
    },
    VdiCatalog {
        seq: u64,
        connection_id: String,
    },
    SrVdis {
        seq: u64,
        connection_id: String,
        sr_uuid: String,
    },
    DeleteVdi {
        connection_id: String,
        vdi_uuid: String,
    },
    AttachVdi {
        connection_id: String,
        vm_uuid: String,
        vdi_uuid: String,
        bootable: bool,
        read_only: bool,
    },
    MoveDisk {
        connection_id: String,
        vm_uuid: String,
        vbd_uuid: String,
        sr_uuid: String,
    },
    SetVbdPlugged {
        connection_id: String,
        vm_uuid: String,
        vbd_uuid: String,
        plugged: bool,
    },
    AddDisk {
        connection_id: String,
        vm_uuid: String,
        sr_uuid: String,
        size_bytes: u64,
        name: Option<String>,
        description: String,
    },
    RemoveDisk {
        connection_id: String,
        vm_uuid: String,
        vbd_uuid: String,
        delete_vdi: bool,
    },
    AddVif {
        connection_id: String,
        vm_uuid: String,
        network_uuid: String,
        mac: Option<String>,
    },
    UpdateVif {
        connection_id: String,
        vm_uuid: String,
        vif_uuid: String,
        network_uuid: String,
        locking_mode: String,
    },
    RemoveVif {
        connection_id: String,
        vm_uuid: String,
        vif_uuid: String,
    },
    SetVifPlugged {
        connection_id: String,
        vm_uuid: String,
        vif_uuid: String,
        plugged: bool,
    },
    OpenConsole {
        console_id: String,
        connection_id: String,
        vm_uuid: String,
    },
    OpenSshConsole {
        console_id: String,
        host: String,
        port: u16,
        username: String,
        password: String,
    },
    SshInput {
        console_id: String,
        input: crate::ssh::SshInput,
    },
    CloseSshConsole {
        console_id: String,
    },
    OpenRdpConsole {
        console_id: String,
        host: String,
        port: u16,
        username: String,
        domain: String,
        password: String,
        width: u16,
        height: u16,
    },
    RdpInput {
        console_id: String,
        input: crate::rdp::RdpInput,
    },
    CloseRdpConsole {
        console_id: String,
    },
    ConsoleInput {
        console_id: String,
        input: crate::console::ConsoleInput,
    },
    CloseConsole {
        console_id: String,
    },
    Shutdown,
}

/// Results sent back to the UI thread.
pub enum Event {
    PropertyExtras {
        seq: u64,
        result: Result<
            (
                Option<Box<VmDetails>>,
                Option<xen_core::pool_options::PoolOptions>,
            ),
            String,
        >,
    },
    ObjectProperties {
        seq: u64,
        result: Result<xen_core::properties::ObjectProperties, String>,
    },
    ObjectPropertiesSaved {
        seq: u64,
        result: Result<xen_core::properties::ObjectProperties, String>,
    },
    XvaQueued {
        result: Result<(), String>,
    },
    VmCreationQueued {
        result: Result<(), String>,
    },
    WizardTemplate {
        seq: u64,
        result: Result<Box<VmDetails>, String>,
    },
    Workspace(WorkspaceView),
    Connected(String),
    ActionQueued,
    RefreshFinished {
        connection_id: String,
        result: Result<(), String>,
    },
    TaskFinished {
        task_id: String,
        result: Result<(), String>,
    },
    ConnectionClosed {
        connection_id: String,
        result: Result<(), String>,
    },
    ConnectionRemoved {
        connection_id: String,
        result: Result<(), String>,
    },
    HostDetails {
        seq: u64,
        connection_id: String,
        host_uuid: String,
        result: Result<Box<HostDetails>, String>,
    },
    HostPerformance {
        seq: u64,
        result: Result<Box<HostPerformance>, String>,
    },
    HostActionFinished {
        host_uuid: String,
        action: xen_core::operations::HostAction,
        result: Result<(), String>,
    },
    AlertDismissed {
        result: Result<(), String>,
    },
    SnapshotsDeleted {
        vm_uuid: String,
        result: Result<usize, String>,
    },
    SrActionFinished {
        sr_uuid: String,
        action: xen_core::operations::SrAction,
        result: Result<(), String>,
    },
    SrCreated {
        result: Result<(), String>,
    },
    SrIntroduced {
        result: Result<(), String>,
    },
    SrReattached {
        sr_uuid: String,
        result: Result<(), String>,
    },
    NetworkChanged {
        action: String,
        result: Result<(), String>,
    },
    PifCatalog {
        seq: u64,
        result: Result<Vec<xen_core::network::PifSummary>, String>,
    },
    PifChanged {
        result: Result<(), String>,
    },
    PbdCatalog {
        seq: u64,
        result: Result<Vec<xen_core::storage::PbdSummary>, String>,
    },
    PbdChanged {
        result: Result<(), String>,
    },
    SrRepaired {
        sr_uuid: String,
        result: Result<usize, String>,
    },
    NewSrProbed {
        result: Result<xen_core::storage::SrProbe, String>,
    },
    SrProbed {
        seq: u64,
        result: Result<xen_core::storage::SrProbe, String>,
    },
    AuthInfo {
        seq: u64,
        result: Result<xen_core::auth::AuthInfo, String>,
    },
    PoolStatus {
        seq: u64,
        result: Result<Box<xen_core::pool_status::PoolStatus>, String>,
    },
    SubjectRolesUpdated {
        subject_uuid: String,
        result: Result<(), String>,
    },
    AdEnabled {
        result: Result<(), String>,
    },
    AdDisabled {
        result: Result<(), String>,
    },
    DeleteDetails {
        targets: Vec<(String, String)>,
        result: Result<Vec<crate::state::DeleteVmItem>, String>,
    },
    VmDetails {
        seq: u64,
        result: Result<Box<VmDetails>, String>,
    },
    VmPerformance {
        seq: u64,
        result: Result<Box<HostPerformance>, String>,
    },
    VdiCatalog {
        seq: u64,
        result: Result<Vec<xen_core::vm::VdiSummary>, String>,
    },
    SrVdis {
        seq: u64,
        result: Result<Vec<xen_core::vm::SrVdi>, String>,
    },
    VdiDeleted {
        vdi_uuid: String,
        result: Result<(), String>,
    },
    CdChanged {
        vm_uuid: String,
        result: Result<(), String>,
    },
    VifChanged {
        vm_uuid: String,
        action: String,
        result: Result<(), String>,
    },
    DiskChanged {
        vm_uuid: String,
        action: String,
        result: Result<(), String>,
    },
    ConsoleOpened {
        console_id: String,
        vm_uuid: String,
        console_uuid: String,
    },
    ConsoleSize {
        console_id: String,
        width: u16,
        height: u16,
    },
    ConsoleFrame {
        console_id: String,
        x: u16,
        y: u16,
        width: u16,
        height: u16,
        pixels: Vec<u8>,
    },
    ConsoleCopy {
        console_id: String,
        dst: (u16, u16, u16, u16),
        src: (u16, u16, u16, u16),
    },
    ConsoleClipboard {
        console_id: String,
        text: String,
    },
    ConsoleClosed {
        console_id: String,
        message: Option<String>,
    },
    SshOpened {
        console_id: String,
        host: String,
        username: String,
    },
    SshOutput {
        console_id: String,
        data: Vec<u8>,
    },
    SshClosed {
        console_id: String,
        message: Option<String>,
    },
    RdpOpened {
        console_id: String,
        host: String,
        username: String,
    },
    RdpFrame {
        console_id: String,
        width: u16,
        height: u16,
        pixels: Vec<u32>,
    },
    RdpClosed {
        console_id: String,
        message: Option<String>,
    },
    PoolActionFinished {
        action: String,
        result: Result<(), String>,
    },
    VmssChanged {
        action: String,
        result: Result<(), String>,
    },
    PvsChanged {
        action: String,
        result: Result<(), String>,
    },
    DrChanged {
        action: String,
        result: Result<(), String>,
    },
    PvsVifCatalog {
        seq: u64,
        result: Result<Vec<xen_core::pvs::PvsVif>, String>,
    },
    Failed {
        context: String,
        message: String,
    },
}

pub struct Worker {
    commands: UnboundedSender<Command>,
    pub events: UnboundedReceiver<Event>,
    join: Option<JoinHandle<()>>,
}

impl Worker {
    pub fn start() -> Self {
        let (commands, command_rx) = unbounded_channel::<Command>();
        let (event_tx, events) = unbounded_channel::<Event>();
        let join = std::thread::Builder::new()
            .name("xen-workspace".into())
            .spawn(move || run_worker(command_rx, event_tx))
            .expect("start workspace thread");
        Self {
            commands,
            events,
            join: Some(join),
        }
    }

    pub fn send(&self, command: Command) {
        let _ = self.commands.send(command);
    }

    pub fn shutdown(&mut self) {
        let _ = self.commands.send(Command::Shutdown);
        if let Some(join) = self.join.take() {
            let _ = join.join();
        }
    }
}

impl Drop for Worker {
    fn drop(&mut self) {
        let _ = self.commands.send(Command::Shutdown);
        if let Some(join) = self.join.take() {
            let _ = join.join();
        }
    }
}

fn run_worker(mut commands: UnboundedReceiver<Command>, events: UnboundedSender<Event>) {
    let runtime = match tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .worker_threads(2)
        // Large inventory and pool-status futures can be several hundred KiB
        // of async state; give the workers headroom instead of the default 2 MiB.
        .thread_stack_size(16 * 1024 * 1024)
        .build()
    {
        Ok(runtime) => runtime,
        Err(_) => return,
    };

    runtime.block_on(async move {
        let path = crate::config::profiles_path();
        let profiles = std::fs::read(&path)
            .ok()
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .unwrap_or_default();
        let workspace = Workspace::with_profiles(profiles);
        let consoles = crate::console::ConsoleHandles::default();
        let ssh = crate::ssh::SshHandles::default();
        let rdp = crate::rdp::RdpHandles::default();
        let _ = events.send(Event::Workspace(workspace.view().await));

        let mut tick = tokio::time::interval(Duration::from_millis(1_000));
        tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

        loop {
            tokio::select! {
                command = commands.recv() => {
                    let Some(command) = command else { break };
                    if matches!(command, Command::Shutdown) {
                        workspace.close().await;
                        break;
                    }
                    dispatch(&workspace, &events, &path, &consoles, &ssh, &rdp, command);
                }
                _ = tick.tick() => {
                    let _ = events.send(Event::Workspace(workspace.view().await));
                }
            }
        }
    });
}

fn dispatch(
    workspace: &Arc<Workspace>,
    events: &UnboundedSender<Event>,
    path: &Path,
    consoles: &crate::console::ConsoleHandles,
    ssh: &crate::ssh::SshHandles,
    rdp: &crate::rdp::RdpHandles,
    command: Command,
) {
    let workspace = workspace.clone();
    let events = events.clone();
    let path = path.to_path_buf();
    let consoles = consoles.clone();
    let ssh = ssh.clone();
    let rdp = rdp.clone();

    tokio::spawn(async move {
        match command {
            Command::PropertyExtras {
                seq,
                connection_id,
                target,
                pool_uuid,
            } => {
                let result = async {
                    let vm = if target.kind == xen_core::properties::ObjectKind::Vm {
                        Some(Box::new(
                            workspace.vm_details(&connection_id, &target.uuid).await?,
                        ))
                    } else {
                        None
                    };
                    let pool = if let Some(uuid) = pool_uuid {
                        Some(workspace.pool_options(&connection_id, &uuid).await?)
                    } else {
                        None
                    };
                    Ok((vm, pool))
                }
                .await;
                let _ = events.send(Event::PropertyExtras { seq, result });
            }
            Command::SavePropertiesDialog { seq, dialog } => {
                let result = save_properties_dialog(&workspace, &dialog).await;
                let _ = events.send(Event::ObjectPropertiesSaved { seq, result });
            }
            Command::ObjectProperties {
                seq,
                connection_id,
                target,
            } => {
                let result = workspace.object_properties(&connection_id, &target).await;
                let _ = events.send(Event::ObjectProperties { seq, result });
            }
            Command::XvaBatch { requests } => {
                let workspace = workspace.clone();
                let events = events.clone();
                let _ = events.send(Event::XvaQueued { result: Ok(()) });
                tokio::spawn(async move {
                    for (connection_id, request) in requests {
                        match workspace.start_xva_transfer(connection_id, request).await {
                            Ok(task) => {
                                let _ = events.send(Event::ActionQueued);
                                loop {
                                    let view = workspace.view().await;
                                    if view.tasks.iter().find(|t| t.id == task.id).is_none_or(|t| {
                                        t.status != xen_core::operations::TaskStatus::Running
                                    }) {
                                        break;
                                    }
                                    tokio::time::sleep(Duration::from_millis(250)).await;
                                }
                            }
                            Err(message) => {
                                let _ = events.send(Event::Failed {
                                    context: "export".into(),
                                    message,
                                });
                            }
                        }
                    }
                });
            }
            Command::XvaTransfer {
                connection_id,
                request,
            } => {
                let result = workspace
                    .start_xva_transfer(connection_id, request)
                    .await
                    .map(|_| ());
                let _ = events.send(Event::XvaQueued { result });
            }
            Command::CreateVm {
                connection_id,
                request,
            } => {
                let result = workspace
                    .create_configured_vm(connection_id, request)
                    .await
                    .map(|_| ());
                let _ = events.send(Event::VmCreationQueued { result });
            }
            Command::WizardTemplate {
                seq,
                connection_id,
                source_uuid,
            } => {
                let result = workspace
                    .vm_details(&connection_id, &source_uuid)
                    .await
                    .map(Box::new);
                let _ = events.send(Event::WizardTemplate { seq, result });
            }
            Command::Connect {
                settings,
                password,
                profile_id,
            } => match workspace.connect(settings, &password, profile_id).await {
                Ok(id) => {
                    save_profiles(&workspace, &path).await;
                    let _ = events.send(Event::Connected(id));
                }
                Err(error) => {
                    let _ = events.send(Event::Failed {
                        context: "connect".into(),
                        message: error,
                    });
                }
            },
            Command::Reconnect { connection_id } => {
                let result = workspace.reconnect_connection(&connection_id).await;
                if result.is_ok() {
                    save_profiles(&workspace, &path).await;
                }
                let _ = events.send(Event::RefreshFinished {
                    connection_id,
                    result,
                });
            }
            Command::Refresh { connection_id } => {
                let result = workspace.refresh(&connection_id).await;
                let _ = events.send(Event::RefreshFinished {
                    connection_id,
                    result,
                });
            }
            Command::Disconnect { connection_id } => {
                let result = workspace.disconnect(&connection_id).await;
                let _ = events.send(Event::ConnectionClosed {
                    connection_id,
                    result,
                });
            }
            Command::Remove { connection_id } => {
                let result = workspace.remove(&connection_id).await;
                if result.is_ok() {
                    save_profiles(&workspace, &path).await;
                }
                let _ = events.send(Event::ConnectionRemoved {
                    connection_id,
                    result,
                });
            }
            Command::ActionBatch { targets, action } => {
                let workspace = workspace.clone();
                let events = events.clone();
                tokio::spawn(async move {
                    for (connection_id, vm_uuid) in targets {
                        match workspace
                            .start_action(connection_id, vm_uuid, action.clone())
                            .await
                        {
                            Ok(task) => {
                                let _ = events.send(Event::ActionQueued);
                                loop {
                                    let view = workspace.view().await;
                                    if view.tasks.iter().find(|t| t.id == task.id).is_none_or(|t| {
                                        t.status != xen_core::operations::TaskStatus::Running
                                    }) {
                                        break;
                                    }
                                    tokio::time::sleep(Duration::from_millis(250)).await;
                                }
                            }
                            Err(error) => {
                                let _ = events.send(Event::Failed {
                                    context: "action".into(),
                                    message: error,
                                });
                            }
                        }
                    }
                });
            }
            Command::Action {
                connection_id,
                vm_uuid,
                action,
            } => match workspace.start_action(connection_id, vm_uuid, action).await {
                Ok(_) => {
                    let _ = events.send(Event::ActionQueued);
                }
                Err(error) => {
                    let _ = events.send(Event::Failed {
                        context: "action".into(),
                        message: error,
                    });
                }
            },
            Command::CheckTask { task_id } => {
                let result = workspace.check_task(&task_id).await;
                let _ = events.send(Event::TaskFinished { task_id, result });
            }
            Command::CancelExternalTask {
                connection_id,
                reference,
            } => {
                let result = workspace
                    .cancel_external_task(&connection_id, &reference)
                    .await;
                let _ = events.send(Event::TaskFinished {
                    task_id: reference,
                    result,
                });
            }
            Command::CancelTask { task_id } => {
                let result = workspace.cancel_task(&task_id).await;
                let _ = events.send(Event::TaskFinished { task_id, result });
            }
            Command::HostDetails {
                seq,
                connection_id,
                host_uuid,
            } => {
                let result = workspace
                    .host_details(&connection_id, &host_uuid)
                    .await
                    .map(Box::new);
                let _ = events.send(Event::HostDetails {
                    seq,
                    connection_id,
                    host_uuid,
                    result,
                });
            }
            Command::IntroduceSr {
                connection_id,
                uuid,
                name,
                description,
            } => {
                let result = workspace
                    .introduce_sr(&connection_id, uuid, name, description)
                    .await;
                let _ = events.send(Event::SrIntroduced { result });
            }
            Command::CreateSr {
                connection_id,
                host_uuid,
                name,
                description,
                config,
            } => {
                let result = workspace
                    .create_storage(&connection_id, &host_uuid, &name, &description, &config)
                    .await;
                let _ = events.send(Event::SrCreated { result });
            }
            Command::ReattachSr {
                connection_id,
                sr_uuid,
                host_uuid,
                location,
            } => {
                let result = workspace
                    .reattach_sr(&connection_id, &sr_uuid, &host_uuid, location)
                    .await;
                let _ = events.send(Event::SrReattached { sr_uuid, result });
            }
            Command::SrAction {
                connection_id,
                sr_uuid,
                action,
            } => {
                let result = workspace.sr_action(&connection_id, &sr_uuid, action).await;
                let _ = events.send(Event::SrActionFinished {
                    sr_uuid,
                    action,
                    result,
                });
            }
            Command::EnableAd {
                connection_id,
                domain,
                user,
                password,
            } => {
                let result = workspace
                    .enable_ad(&connection_id, domain, user, password)
                    .await;
                let _ = events.send(Event::AdEnabled { result });
            }
            Command::DisableAd {
                connection_id,
                user,
                password,
            } => {
                let result = workspace.disable_ad(&connection_id, user, password).await;
                let _ = events.send(Event::AdDisabled { result });
            }
            Command::SetSubjectRoles {
                connection_id,
                subject_uuid,
                roles,
            } => {
                let result = workspace
                    .set_subject_roles(&connection_id, &subject_uuid, roles)
                    .await;
                let _ = events.send(Event::SubjectRolesUpdated {
                    subject_uuid,
                    result,
                });
            }
            Command::CreateDrTask {
                connection_id,
                sr_type,
                device_config,
                whitelist,
            } => {
                let result = workspace
                    .create_dr_task(&connection_id, sr_type, device_config, whitelist)
                    .await;
                let _ = events.send(Event::DrChanged {
                    action: "create".into(),
                    result,
                });
            }
            Command::DestroyDrTask {
                connection_id,
                uuid,
            } => {
                let result = workspace.destroy_dr_task(&connection_id, &uuid).await;
                let _ = events.send(Event::DrChanged {
                    action: "destroy".into(),
                    result,
                });
            }
            Command::IntroducePvsSite {
                connection_id,
                name,
                description,
            } => {
                let result = workspace
                    .introduce_pvs_site(&connection_id, name, description)
                    .await;
                let _ = events.send(Event::PvsChanged {
                    action: "site_create".into(),
                    result,
                });
            }
            Command::ForgetPvsSite {
                connection_id,
                uuid,
            } => {
                let result = workspace.forget_pvs_site(&connection_id, &uuid).await;
                let _ = events.send(Event::PvsChanged {
                    action: "site_delete".into(),
                    result,
                });
            }
            Command::IntroducePvsServer {
                connection_id,
                site_uuid,
                addresses,
                first_port,
                last_port,
            } => {
                let result = workspace
                    .introduce_pvs_server(
                        &connection_id,
                        &site_uuid,
                        addresses,
                        first_port,
                        last_port,
                    )
                    .await;
                let _ = events.send(Event::PvsChanged {
                    action: "server_create".into(),
                    result,
                });
            }
            Command::ForgetPvsServer {
                connection_id,
                uuid,
            } => {
                let result = workspace.forget_pvs_server(&connection_id, &uuid).await;
                let _ = events.send(Event::PvsChanged {
                    action: "server_delete".into(),
                    result,
                });
            }
            Command::CreatePvsProxy {
                connection_id,
                site_uuid,
                vif_uuid,
            } => {
                let result = workspace
                    .create_pvs_proxy(&connection_id, &site_uuid, &vif_uuid)
                    .await;
                let _ = events.send(Event::PvsChanged {
                    action: "proxy_create".into(),
                    result,
                });
            }
            Command::DestroyPvsProxy {
                connection_id,
                uuid,
            } => {
                let result = workspace.destroy_pvs_proxy(&connection_id, &uuid).await;
                let _ = events.send(Event::PvsChanged {
                    action: "proxy_delete".into(),
                    result,
                });
            }
            Command::CreatePvsCache {
                connection_id,
                site_uuid,
                host_uuid,
                sr_uuid,
                size_mib,
            } => {
                let result = workspace
                    .create_pvs_cache(&connection_id, &site_uuid, &host_uuid, &sr_uuid, size_mib)
                    .await;
                let _ = events.send(Event::PvsChanged {
                    action: "cache_create".into(),
                    result,
                });
            }
            Command::DestroyPvsCache {
                connection_id,
                uuid,
            } => {
                let result = workspace.destroy_pvs_cache(&connection_id, &uuid).await;
                let _ = events.send(Event::PvsChanged {
                    action: "cache_delete".into(),
                    result,
                });
            }
            Command::PvsVifCatalog { seq, connection_id } => {
                let result = workspace.pvs_vif_catalog(&connection_id).await;
                let _ = events.send(Event::PvsVifCatalog { seq, result });
            }
            Command::CreateVmss {
                connection_id,
                name,
                description,
                snapshot_type,
                schedule,
                retained_snapshots,
                enabled,
                vm_uuids,
            } => {
                let result = workspace
                    .create_vmss(
                        &connection_id,
                        name,
                        description,
                        snapshot_type,
                        schedule,
                        retained_snapshots,
                        enabled,
                        vm_uuids,
                    )
                    .await;
                let _ = events.send(Event::VmssChanged {
                    action: "create".into(),
                    result,
                });
            }
            Command::UpdateVmss {
                connection_id,
                uuid,
                name,
                description,
                snapshot_type,
                schedule,
                retained_snapshots,
                enabled,
                vm_uuids,
            } => {
                let result = workspace
                    .update_vmss(
                        &connection_id,
                        &uuid,
                        name,
                        description,
                        snapshot_type,
                        schedule,
                        retained_snapshots,
                        enabled,
                        vm_uuids,
                    )
                    .await;
                let _ = events.send(Event::VmssChanged {
                    action: "update".into(),
                    result,
                });
            }
            Command::DeleteVmss {
                connection_id,
                uuid,
            } => {
                let result = workspace.delete_vmss(&connection_id, &uuid).await;
                let _ = events.send(Event::VmssChanged {
                    action: "delete".into(),
                    result,
                });
            }
            Command::EnableHa {
                connection_id,
                heartbeat_sr_uuids,
                failures_to_tolerate,
            } => {
                let result = workspace
                    .enable_ha(&connection_id, heartbeat_sr_uuids, failures_to_tolerate)
                    .await;
                let _ = events.send(Event::PoolActionFinished {
                    action: "ha_enable".into(),
                    result,
                });
            }
            Command::DisableHa { connection_id } => {
                let result = workspace.disable_ha(&connection_id).await;
                let _ = events.send(Event::PoolActionFinished {
                    action: "ha_disable".into(),
                    result,
                });
            }
            Command::ConfigureWlb {
                connection_id,
                wlb_url,
                wlb_username,
                wlb_password,
                xenserver_username,
                xenserver_password,
            } => {
                let result = workspace
                    .configure_wlb(
                        &connection_id,
                        wlb_url,
                        wlb_username,
                        wlb_password,
                        xenserver_username,
                        xenserver_password,
                    )
                    .await;
                let _ = events.send(Event::PoolActionFinished {
                    action: "wlb_configure".into(),
                    result,
                });
            }
            Command::SetWlbEnabled {
                connection_id,
                enabled,
            } => {
                let result = workspace.set_wlb_enabled(&connection_id, enabled).await;
                let _ = events.send(Event::PoolActionFinished {
                    action: if enabled { "wlb_enable" } else { "wlb_disable" }.into(),
                    result,
                });
            }
            Command::DeconfigureWlb { connection_id } => {
                let result = workspace.deconfigure_wlb(&connection_id).await;
                let _ = events.send(Event::PoolActionFinished {
                    action: "wlb_deconfigure".into(),
                    result,
                });
            }
            Command::PoolStatus { seq, connection_id } => {
                let result = workspace.pool_status(&connection_id).await.map(Box::new);
                let _ = events.send(Event::PoolStatus { seq, result });
            }
            Command::AuthInfo { seq, connection_id } => {
                let result = workspace.auth_info(&connection_id).await;
                let _ = events.send(Event::AuthInfo { seq, result });
            }
            Command::PbdCatalog {
                seq,
                connection_id,
                sr_uuid,
            } => {
                let result = workspace.pbd_catalog(&connection_id, &sr_uuid).await;
                let _ = events.send(Event::PbdCatalog { seq, result });
            }
            Command::SetPbdPlugged {
                connection_id,
                pbd_uuid,
                plugged,
            } => {
                let result = workspace
                    .set_pbd_plugged(&connection_id, &pbd_uuid, plugged)
                    .await;
                let _ = events.send(Event::PbdChanged { result });
            }
            Command::RepairSr {
                connection_id,
                sr_uuid,
            } => {
                let result = workspace.repair_sr(&connection_id, &sr_uuid).await;
                let _ = events.send(Event::SrRepaired { sr_uuid, result });
            }
            Command::ProbeNewSr {
                connection_id,
                host_uuid,
                config,
            } => {
                let result = workspace
                    .probe_storage(&connection_id, &host_uuid, &config)
                    .await;
                let _ = events.send(Event::NewSrProbed { result });
            }
            Command::ProbeSr {
                seq,
                connection_id,
                host_uuid,
                location,
            } => {
                let result = workspace
                    .probe_sr(&connection_id, &host_uuid, &location)
                    .await;
                let _ = events.send(Event::SrProbed { seq, result });
            }
            Command::PifCatalog { seq, connection_id } => {
                let result = workspace.pif_catalog(&connection_id).await;
                let _ = events.send(Event::PifCatalog { seq, result });
            }
            Command::SetPifPlugged {
                connection_id,
                pif_uuid,
                plugged,
            } => {
                let result = workspace
                    .set_pif_plugged(&connection_id, &pif_uuid, plugged)
                    .await;
                let _ = pif_uuid;
                let _ = events.send(Event::PifChanged { result });
            }
            Command::CreateSriovNetwork {
                connection_id,
                name,
                description,
                mtu,
                autoplug,
                pif_uuids,
            } => {
                let result = workspace
                    .create_sriov_network(
                        &connection_id,
                        name,
                        description,
                        mtu,
                        autoplug,
                        pif_uuids,
                    )
                    .await;
                let _ = events.send(Event::NetworkChanged {
                    action: "sriov_create".into(),
                    result,
                });
            }
            Command::CreateChinNetwork {
                connection_id,
                name,
                description,
                mtu,
                autoplug,
                protocol,
                pif_uuids,
            } => {
                let result = workspace
                    .create_chin_network(
                        &connection_id,
                        name,
                        description,
                        mtu,
                        autoplug,
                        protocol,
                        pif_uuids,
                    )
                    .await;
                let _ = events.send(Event::NetworkChanged {
                    action: "chin_create".into(),
                    result,
                });
            }
            Command::CreateBond {
                connection_id,
                name,
                members,
                mode,
                mtu,
                autoplug,
            } => {
                let result = workspace
                    .create_bond(&connection_id, name, members, mode, mtu, autoplug)
                    .await;
                let _ = events.send(Event::NetworkChanged {
                    action: "bond_create".into(),
                    result,
                });
            }
            Command::CreateVlan {
                connection_id,
                pif_uuid,
                tag,
                name,
                description,
                mtu,
                autoplug,
            } => {
                let result = workspace
                    .create_vlan(
                        &connection_id,
                        &pif_uuid,
                        tag,
                        name,
                        description,
                        mtu,
                        autoplug,
                    )
                    .await;
                let _ = events.send(Event::NetworkChanged {
                    action: "vlan_create".into(),
                    result,
                });
            }
            Command::CreateNetwork {
                connection_id,
                name,
                description,
                mtu,
            } => {
                let result = workspace
                    .create_network(&connection_id, name, description, mtu)
                    .await;
                let _ = events.send(Event::NetworkChanged {
                    action: "create".into(),
                    result,
                });
            }
            Command::DestroyNetwork {
                connection_id,
                network_uuid,
            } => {
                let result = workspace
                    .destroy_network(&connection_id, &network_uuid)
                    .await;
                let _ = events.send(Event::NetworkChanged {
                    action: "destroy".into(),
                    result,
                });
            }
            Command::DeleteSnapshots {
                connection_id,
                vm_uuid,
                snapshot_uuids,
            } => {
                let result = workspace
                    .delete_snapshots(&connection_id, &vm_uuid, snapshot_uuids)
                    .await;
                let _ = events.send(Event::SnapshotsDeleted { vm_uuid, result });
            }
            Command::DismissAlert {
                connection_id,
                message_uuid,
            } => {
                let result = workspace.dismiss_alert(&connection_id, &message_uuid).await;
                let _ = events.send(Event::AlertDismissed { result });
            }
            Command::HostAction {
                connection_id,
                host_uuid,
                action,
            } => {
                let result = workspace
                    .host_action(&connection_id, &host_uuid, action)
                    .await;
                let _ = events.send(Event::HostActionFinished {
                    host_uuid,
                    action,
                    result,
                });
            }
            Command::HostPerformance {
                seq,
                connection_id,
                host_uuid,
                range_seconds,
            } => {
                let result = workspace
                    .host_performance(&connection_id, &host_uuid, range_seconds)
                    .await
                    .map(Box::new);
                let _ = events.send(Event::HostPerformance { seq, result });
            }
            Command::DeleteDetails { targets } => {
                let mut items = Vec::new();
                let mut error = None;
                for (connection_id, uuid) in &targets {
                    match workspace.vm_details(connection_id, uuid).await {
                        Ok(d) => items.push(crate::state::DeleteVmItem {
                            connection_id: connection_id.clone(),
                            uuid: uuid.clone(),
                            name: d.name,
                            disks: d
                                .disks
                                .into_iter()
                                .map(|d| {
                                    let checked = d.owner && !d.shared && !d.is_cd;
                                    (d, checked)
                                })
                                .collect(),
                            snapshots: d
                                .snapshots
                                .into_iter()
                                .map(|s| (s.uuid, s.name, false))
                                .collect(),
                        }),
                        Err(e) => {
                            error = Some(e);
                            break;
                        }
                    }
                }
                let result = error.map_or(Ok(items), Err);
                let _ = events.send(Event::DeleteDetails { targets, result });
            }
            Command::VmDetails {
                seq,
                connection_id,
                vm_uuid,
            } => {
                let result = workspace
                    .vm_details(&connection_id, &vm_uuid)
                    .await
                    .map(Box::new);
                let _ = events.send(Event::VmDetails { seq, result });
            }
            Command::VmPerformance {
                seq,
                connection_id,
                vm_uuid,
                range_seconds,
            } => {
                let result = workspace
                    .vm_performance(&connection_id, &vm_uuid, range_seconds)
                    .await
                    .map(Box::new);
                let _ = events.send(Event::VmPerformance { seq, result });
            }
            Command::ChangeCd {
                connection_id,
                vm_uuid,
                vbd_uuid,
                iso_uuid,
            } => {
                let result = workspace
                    .change_cd(&connection_id, &vm_uuid, &vbd_uuid, iso_uuid)
                    .await;
                let _ = events.send(Event::CdChanged { vm_uuid, result });
            }
            Command::SrVdis {
                seq,
                connection_id,
                sr_uuid,
            } => {
                let result = workspace.sr_vdis(&connection_id, &sr_uuid).await;
                let _ = events.send(Event::SrVdis { seq, result });
            }
            Command::DeleteVdi {
                connection_id,
                vdi_uuid,
            } => {
                let result = workspace.delete_vdi(&connection_id, &vdi_uuid).await;
                let _ = events.send(Event::VdiDeleted { vdi_uuid, result });
            }
            Command::VdiCatalog { seq, connection_id } => {
                let result = workspace.vdi_catalog(&connection_id).await;
                let _ = events.send(Event::VdiCatalog { seq, result });
            }
            Command::AttachVdi {
                connection_id,
                vm_uuid,
                vdi_uuid,
                bootable,
                read_only,
            } => {
                let result = workspace
                    .attach_vdi(&connection_id, &vm_uuid, &vdi_uuid, bootable, read_only)
                    .await;
                let _ = events.send(Event::DiskChanged {
                    vm_uuid,
                    action: "attach".into(),
                    result,
                });
            }
            Command::MoveDisk {
                connection_id,
                vm_uuid,
                vbd_uuid,
                sr_uuid,
            } => {
                let result = workspace
                    .move_disk(&connection_id, &vm_uuid, &vbd_uuid, &sr_uuid)
                    .await;
                let _ = events.send(Event::DiskChanged {
                    vm_uuid,
                    action: "move".into(),
                    result,
                });
            }
            Command::SetVbdPlugged {
                connection_id,
                vm_uuid,
                vbd_uuid,
                plugged,
            } => {
                let result = workspace
                    .set_vbd_plugged(&connection_id, &vm_uuid, &vbd_uuid, plugged)
                    .await;
                let _ = events.send(Event::DiskChanged {
                    vm_uuid,
                    action: if plugged { "plug" } else { "unplug" }.into(),
                    result,
                });
            }
            Command::AddCdDrive {
                connection_id,
                vm_uuid,
            } => {
                let result = workspace.add_cd_drive(&connection_id, &vm_uuid).await;
                let _ = events.send(Event::DiskChanged {
                    vm_uuid,
                    action: "add_cd".into(),
                    result,
                });
            }
            Command::AddDisk {
                connection_id,
                vm_uuid,
                sr_uuid,
                size_bytes,
                name,
                description,
            } => {
                let result = if vm_uuid.is_empty() {
                    workspace
                        .create_vdi(
                            &connection_id,
                            &sr_uuid,
                            name.as_deref().unwrap_or("New virtual disk"),
                            &description,
                            size_bytes,
                        )
                        .await
                } else {
                    workspace
                        .add_disk(
                            &connection_id,
                            &vm_uuid,
                            &sr_uuid,
                            size_bytes,
                            name,
                            &description,
                        )
                        .await
                };
                let _ = events.send(Event::DiskChanged {
                    vm_uuid,
                    action: "add".into(),
                    result,
                });
            }
            Command::RemoveDisk {
                connection_id,
                vm_uuid,
                vbd_uuid,
                delete_vdi,
            } => {
                let result = workspace
                    .remove_disk(&connection_id, &vm_uuid, &vbd_uuid, delete_vdi)
                    .await;
                let _ = events.send(Event::DiskChanged {
                    vm_uuid,
                    action: "remove".into(),
                    result,
                });
            }
            Command::AddVif {
                connection_id,
                vm_uuid,
                network_uuid,
                mac,
            } => {
                let result = workspace
                    .add_vif(&connection_id, &vm_uuid, &network_uuid, mac)
                    .await;
                let _ = events.send(Event::VifChanged {
                    vm_uuid,
                    action: "add".into(),
                    result,
                });
            }
            Command::UpdateVif {
                connection_id,
                vm_uuid,
                vif_uuid,
                network_uuid,
                locking_mode,
            } => {
                let result = workspace
                    .update_vif(
                        &connection_id,
                        &vm_uuid,
                        &vif_uuid,
                        &network_uuid,
                        &locking_mode,
                    )
                    .await;
                let _ = events.send(Event::VifChanged {
                    vm_uuid,
                    action: "edit".into(),
                    result,
                });
            }
            Command::RemoveVif {
                connection_id,
                vm_uuid,
                vif_uuid,
            } => {
                let result = workspace
                    .remove_vif(&connection_id, &vm_uuid, &vif_uuid)
                    .await;
                let _ = events.send(Event::VifChanged {
                    vm_uuid,
                    action: "remove".into(),
                    result,
                });
            }
            Command::SetVifPlugged {
                connection_id,
                vm_uuid,
                vif_uuid,
                plugged,
            } => {
                let result = workspace
                    .set_vif_plugged(&connection_id, &vm_uuid, &vif_uuid, plugged)
                    .await;
                let _ = events.send(Event::VifChanged {
                    vm_uuid,
                    action: if plugged { "plug" } else { "unplug" }.into(),
                    result,
                });
            }
            Command::OpenConsole {
                console_id,
                connection_id,
                vm_uuid,
            } => {
                if let Err(message) = crate::console::open(
                    workspace.clone(),
                    events.clone(),
                    consoles,
                    console_id.clone(),
                    connection_id,
                    vm_uuid,
                )
                .await
                {
                    let _ = events.send(Event::ConsoleClosed {
                        console_id,
                        message: Some(message),
                    });
                }
            }
            Command::ConsoleInput { console_id, input } => {
                crate::console::send_input(&consoles, &console_id, input);
            }
            Command::CloseConsole { console_id } => {
                crate::console::close(&consoles, &console_id);
            }
            Command::OpenSshConsole {
                console_id,
                host,
                port,
                username,
                password,
            } => {
                if let Err(message) = crate::ssh::open(
                    events.clone(),
                    ssh,
                    console_id.clone(),
                    host,
                    port,
                    username,
                    password,
                )
                .await
                {
                    let _ = events.send(Event::SshClosed {
                        console_id,
                        message: Some(message),
                    });
                }
            }
            Command::SshInput { console_id, input } => {
                crate::ssh::send_input(&ssh, &console_id, input);
            }
            Command::CloseSshConsole { console_id } => {
                crate::ssh::close(&ssh, &console_id);
            }
            Command::OpenRdpConsole {
                console_id,
                host,
                port,
                username,
                domain,
                password,
                width,
                height,
            } => {
                if let Err(message) = crate::rdp::open(
                    events.clone(),
                    rdp,
                    console_id.clone(),
                    host,
                    port,
                    username,
                    domain,
                    password,
                    width,
                    height,
                )
                .await
                {
                    let _ = events.send(Event::RdpClosed {
                        console_id,
                        message: Some(message),
                    });
                }
            }
            Command::RdpInput { console_id, input } => {
                crate::rdp::send_input(&rdp, &console_id, input);
            }
            Command::CloseRdpConsole { console_id } => {
                crate::rdp::close(&rdp, &console_id);
            }
            Command::Shutdown => {}
        }
        let _ = events.send(Event::Workspace(workspace.view().await));
    });
}

async fn save_profiles(workspace: &Workspace, path: &Path) {
    let profiles = workspace.profiles().await;
    let Ok(bytes) = serde_json::to_vec_pretty(&profiles) else {
        return;
    };
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let temporary = path.with_extension("tmp");
    if std::fs::write(&temporary, bytes).is_ok() {
        let _ = std::fs::rename(&temporary, path);
    }
}

async fn save_properties_dialog(
    workspace: &Workspace,
    dialog: &crate::state::PropertiesDialog,
) -> Result<xen_core::properties::ObjectProperties, String> {
    let crate::state::LoadState::Ready(edited) = &dialog.properties else {
        return Err("General: Properties are not loaded.".into());
    };
    let original = dialog
        .original
        .as_ref()
        .ok_or("General: Properties are not loaded.")?;
    let id = &dialog.connection_id;
    let uuid = &dialog.target.uuid;
    let resources = dialog
        .vm
        .as_ref()
        .map(|vm| vm.parse_resources())
        .transpose()
        .map_err(|e| format!("CPU: {e}"))?;
    if let (Some(setup), Some(before)) = (&dialog.guest_setup, &dialog.original_vm)
        && setup != &xen_core::guest_setup::GuestSetup::from_data(&before.guest_data)
    {
        workspace
            .update_guest_setup(id, uuid, setup, dialog.reset_password)
            .await
            .map_err(|e| format!("Guest Setup: {e}"))?;
    }
    workspace
        .update_object_properties(
            id,
            &xen_core::properties::ObjectPropertiesEdit {
                original: original.clone(),
                edited: edited.clone(),
            },
        )
        .await
        .map_err(|e| format!("General: {e}"))?;
    if let (Some(resources), Some(before)) = (resources, &dialog.original_vm) {
        let baseline = crate::state::VmResourceEditState::from_details(before).parse_resources()?;
        let cpu_only = |mut value: xen_core::operations::VmResourceUpdate| {
            value.boot_order = None;
            value.firmware = None;
            value.secure_boot = None;
            value.boot_policy = None;
            value.after_crash = None;
            value.pv_args = None;
            value.auto_poweron = None;
            value
        };
        let cpu = cpu_only(resources.clone());
        if cpu != cpu_only(baseline.clone()) {
            workspace
                .update_vm_resources(id, uuid, cpu)
                .await
                .map_err(|e| format!("CPU: {e}"))?;
        }
        let boot_changed = resources.boot_order != baseline.boot_order
            || resources.firmware != baseline.firmware
            || resources.secure_boot != baseline.secure_boot
            || resources.boot_policy != baseline.boot_policy
            || resources.after_crash != baseline.after_crash
            || resources.pv_args != baseline.pv_args
            || resources.auto_poweron != baseline.auto_poweron;
        if boot_changed {
            let mut boot = resources;
            boot.cpu_weight = None;
            boot.cores_per_socket = None;
            workspace
                .update_vm_resources(id, uuid, boot)
                .await
                .map_err(|e| format!("Boot Options: {e}"))?;
        }
        let before_affinity = (!before.affinity_ref.is_empty()
            && before.affinity_ref != "OpaqueRef:NULL")
            .then(|| before.affinity_ref.clone());
        if dialog.restart_priority != before.ha_restart_priority
            || dialog.start_delay != before.start_delay
            || dialog.shutdown_delay != before.shutdown_delay
            || dialog.affinity != before_affinity
        {
            workspace
                .update_vm_ha(
                    id,
                    uuid,
                    dialog.restart_priority.clone(),
                    dialog.start_delay,
                    dialog.shutdown_delay,
                    dialog.affinity.clone(),
                )
                .await
                .map_err(|e| format!("High Availability: {e}"))?;
        }
    }
    if let (Some(original), Some(edited)) = (&dialog.original_pool, &dialog.pool) {
        let mut current = original.clone();
        for page in ["Live Patching", "Network Options", "Advanced"] {
            let mut next = current.clone();
            match page {
                "Live Patching" => next.live_patching_disabled = edited.live_patching_disabled,
                "Network Options" => next.igmp_snooping_enabled = edited.igmp_snooping_enabled,
                _ => {
                    next.vm_autostart = edited.vm_autostart;
                    next.migration_compression = edited.migration_compression;
                }
            }
            if next != current {
                current = workspace
                    .update_pool_options(
                        id,
                        &xen_core::pool_options::PoolOptionsEdit {
                            original: current,
                            edited: next,
                        },
                    )
                    .await
                    .map_err(|e| format!("{page}: {e}"))?;
            }
        }
    }
    if let (Some(before), Some(after)) = (&original.disk, &edited.disk) {
        if before.size_bytes != after.size_bytes {
            let device = after
                .devices
                .first()
                .ok_or("Size and Location: Attach this disk to a VM before resizing.")?;
            workspace
                .resize_disk(id, &device.vm_uuid, uuid, after.size_bytes)
                .await
                .map_err(|e| format!("Size and Location: {e}"))?;
        }
        for device in &after.devices {
            let before = before
                .devices
                .iter()
                .find(|v| v.vbd_uuid == device.vbd_uuid)
                .ok_or("General: Disk attachments changed. Reopen Properties.")?;
            if before.mode != device.mode {
                workspace
                    .set_vbd_mode(id, &device.vm_uuid, &device.vbd_uuid, &device.mode)
                    .await
                    .map_err(|e| format!("device-{}: {e}", device.vbd_uuid))?;
            }
            if before.bootable != device.bootable {
                workspace
                    .set_vbd_bootable(id, &device.vm_uuid, &device.vbd_uuid, device.bootable)
                    .await
                    .map_err(|e| format!("device-{}: {e}", device.vbd_uuid))?;
            }
        }
    }
    workspace.object_properties(id, &dialog.target).await
}
