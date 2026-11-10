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
    Connect {
        settings: ConnectionSettings,
        password: String,
        profile_id: Option<String>,
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
    Action {
        connection_id: String,
        vm_uuid: String,
        action: VmAction,
    },
    CheckTask {
        task_id: String,
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
        location: String,
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
    EditVm {
        connection_id: String,
        vm_uuid: String,
        name: String,
        description: String,
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
        tags: Vec<String>,
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
    },
    SetVbdBootable {
        connection_id: String,
        vm_uuid: String,
        vbd_uuid: String,
        bootable: bool,
    },
    SetVbdMode {
        connection_id: String,
        vm_uuid: String,
        vbd_uuid: String,
        mode: String,
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
    UpdateVmHa {
        connection_id: String,
        vm_uuid: String,
        restart_priority: String,
        start_delay: u64,
        shutdown_delay: u64,
        affinity_host_uuid: Option<String>,
    },
    AddDisk {
        connection_id: String,
        vm_uuid: String,
        sr_uuid: String,
        size_bytes: u64,
        name: Option<String>,
    },
    ResizeDisk {
        connection_id: String,
        vm_uuid: String,
        vdi_uuid: String,
        size_bytes: u64,
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
    VmDetails {
        seq: u64,
        result: Result<Box<VmDetails>, String>,
    },
    VmPerformance {
        seq: u64,
        result: Result<Box<HostPerformance>, String>,
    },
    VmEdited {
        vm_uuid: String,
        result: Result<(), String>,
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
    VmHaUpdated {
        vm_uuid: String,
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
                let _ = events.send(Event::HostDetails { seq, result });
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
                location,
            } => {
                let result = workspace
                    .create_sr(&connection_id, &host_uuid, name, description, location)
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
            Command::EditVm {
                connection_id,
                vm_uuid,
                name,
                description,
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
                tags,
            } => {
                let result = async {
                    workspace
                        .update_vm(
                            &connection_id,
                            &vm_uuid,
                            name,
                            description,
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
                        )
                        .await?;
                    workspace.set_vm_tags(&connection_id, &vm_uuid, tags).await
                }
                .await;
                let _ = events.send(Event::VmEdited { vm_uuid, result });
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
            } => {
                let result = workspace
                    .attach_vdi(&connection_id, &vm_uuid, &vdi_uuid, bootable)
                    .await;
                let _ = events.send(Event::DiskChanged {
                    vm_uuid,
                    action: "attach".into(),
                    result,
                });
            }
            Command::UpdateVmHa {
                connection_id,
                vm_uuid,
                restart_priority,
                start_delay,
                shutdown_delay,
                affinity_host_uuid,
            } => {
                let result = workspace
                    .update_vm_ha(
                        &connection_id,
                        &vm_uuid,
                        restart_priority,
                        start_delay,
                        shutdown_delay,
                        affinity_host_uuid,
                    )
                    .await;
                let _ = events.send(Event::VmHaUpdated { vm_uuid, result });
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
            Command::SetVbdMode {
                connection_id,
                vm_uuid,
                vbd_uuid,
                mode,
            } => {
                let result = workspace
                    .set_vbd_mode(&connection_id, &vm_uuid, &vbd_uuid, &mode)
                    .await;
                let _ = events.send(Event::DiskChanged {
                    vm_uuid,
                    action: "mode".into(),
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
            Command::SetVbdBootable {
                connection_id,
                vm_uuid,
                vbd_uuid,
                bootable,
            } => {
                let result = workspace
                    .set_vbd_bootable(&connection_id, &vm_uuid, &vbd_uuid, bootable)
                    .await;
                let _ = events.send(Event::DiskChanged {
                    vm_uuid,
                    action: "bootable".into(),
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
            } => {
                let result = workspace
                    .add_disk(&connection_id, &vm_uuid, &sr_uuid, size_bytes, name)
                    .await;
                let _ = events.send(Event::DiskChanged {
                    vm_uuid,
                    action: "add".into(),
                    result,
                });
            }
            Command::ResizeDisk {
                connection_id,
                vm_uuid,
                vdi_uuid,
                size_bytes,
            } => {
                let result = workspace
                    .resize_disk(&connection_id, &vm_uuid, &vdi_uuid, size_bytes)
                    .await;
                let _ = events.send(Event::DiskChanged {
                    vm_uuid,
                    action: "resize".into(),
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
