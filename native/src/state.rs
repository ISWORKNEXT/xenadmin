use std::collections::HashSet;
use std::time::Instant;

use chrono::{DateTime, Local};

use crate::config::{GroupBy, SavedSearch};
use xen_core::{
    host::{DataSection, HostDetails, InfoGroup},
    network::PifSummary,
    operations::{TaskStatus, VmAction},
    performance::HostPerformance,
    storage::{PbdSummary, SrProbe},
    vm::{SrVdi, VdiSummary, VmDetails, VmIso},
    workspace::{ConnectionView, Operation, WorkspaceView},
    xapi::{ConnectionSettings, Host, Inventory, VirtualMachine},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum View {
    #[default]
    Overview,
    Vms,
    Hosts,
    Templates,
    Storage,
    Networks,
    Tasks,
}

impl View {
    pub fn title(self) -> &'static str {
        match self {
            View::Overview => "Overview",
            View::Vms => "Virtual machines",
            View::Hosts => "Hosts",
            View::Templates => "Templates",
            View::Storage => "Storage",
            View::Networks => "Networks",
            View::Tasks => "Activity & tasks",
        }
    }

    pub fn from_key(key: &str) -> Option<Self> {
        match key.to_ascii_lowercase().as_str() {
            "overview" => Some(View::Overview),
            "vms" | "virtual-machines" => Some(View::Vms),
            "hosts" => Some(View::Hosts),
            "templates" => Some(View::Templates),
            "storage" => Some(View::Storage),
            "networks" => Some(View::Networks),
            "tasks" => Some(View::Tasks),
            _ => None,
        }
    }

    pub fn subtitle(self) -> &'static str {
        match self {
            View::Overview => "One workspace. Every connection, host and virtual machine.",
            View::Vms => "Guest virtual machines across the selected scope.",
            View::Hosts => "Pool members, capacity and host information.",
            View::Templates => "Provisioning templates reported by each pool.",
            View::Storage => "Storage repositories and capacity per connection.",
            View::Networks => "Bridges and attached interfaces per connection.",
            View::Tasks => "Every operation and the result reported by XenAPI.",
        }
    }
}

#[derive(Clone, PartialEq, Eq, Default)]
pub enum Scope {
    #[default]
    All,
    Connection(String),
    Host {
        connection_id: String,
        host_ref: String,
    },
}

impl Scope {
    pub fn connection_id(&self) -> Option<&str> {
        match self {
            Scope::All => None,
            Scope::Connection(id) => Some(id),
            Scope::Host { connection_id, .. } => Some(connection_id),
        }
    }

    pub fn host_ref(&self) -> Option<&str> {
        match self {
            Scope::Host { host_ref, .. } => Some(host_ref),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ListKind {
    Vms,
    Hosts,
    Templates,
    Storage,
    Networks,
}

impl ListKind {
    pub fn title(self) -> &'static str {
        match self {
            ListKind::Vms => "Virtual machines",
            ListKind::Hosts => "Hosts",
            ListKind::Templates => "Templates",
            ListKind::Storage => "Storage",
            ListKind::Networks => "Networks",
        }
    }

    pub fn view(self) -> View {
        match self {
            ListKind::Vms => View::Vms,
            ListKind::Hosts => View::Hosts,
            ListKind::Templates => View::Templates,
            ListKind::Storage => View::Storage,
            ListKind::Networks => View::Networks,
        }
    }
}

#[derive(Clone, PartialEq, Eq, Hash)]
pub enum ObjectKey {
    Home,
    Pool(String),
    Host {
        connection_id: String,
        host_uuid: String,
    },
    Vm {
        connection_id: String,
        vm_uuid: String,
    },
    Template {
        connection_id: String,
        vm_uuid: String,
    },
    Snapshot {
        connection_id: String,
        vm_uuid: String,
    },
    Storage {
        connection_id: String,
        sr_uuid: String,
    },
    Network {
        connection_id: String,
        network_uuid: String,
    },
    Health {
        connection_id: String,
    },
    Auth {
        connection_id: String,
    },
    PoolStatus {
        connection_id: String,
    },
    List {
        connection_id: Option<String>,
        kind: ListKind,
        host_ref: Option<String>,
    },
    Tasks,
}

impl ObjectKey {
    pub fn connection_id(&self) -> Option<&str> {
        match self {
            ObjectKey::Home | ObjectKey::Tasks => None,
            ObjectKey::Pool(id) => Some(id),
            ObjectKey::Host { connection_id, .. }
            | ObjectKey::Vm { connection_id, .. }
            | ObjectKey::Template { connection_id, .. }
            | ObjectKey::Snapshot { connection_id, .. }
            | ObjectKey::Storage { connection_id, .. }
            | ObjectKey::Network { connection_id, .. }
            | ObjectKey::Health { connection_id }
            | ObjectKey::Auth { connection_id }
            | ObjectKey::PoolStatus { connection_id } => Some(connection_id),
            ObjectKey::List { connection_id, .. } => connection_id.as_deref(),
        }
    }
}

#[derive(Clone)]
pub struct ObjectSelection {
    pub key: ObjectKey,
    pub title: String,
}

#[derive(Clone, Copy, PartialEq, Eq, Default)]
pub enum VmTab {
    #[default]
    General,
    Performance,
    Storage,
    Network,
    Memory,
    Snapshots,
    Console,
}

impl VmTab {
    pub const ALL: [VmTab; 7] = [
        VmTab::General,
        VmTab::Memory,
        VmTab::Storage,
        VmTab::Network,
        VmTab::Console,
        VmTab::Performance,
        VmTab::Snapshots,
    ];

    pub fn label(self) -> &'static str {
        match self {
            VmTab::General => "General",
            VmTab::Performance => "Performance",
            VmTab::Storage => "Storage",
            VmTab::Network => "Networking",
            VmTab::Memory => "Memory",
            VmTab::Snapshots => "Snapshots",
            VmTab::Console => "Console",
        }
    }

    pub fn from_key(key: &str) -> Option<Self> {
        match key.to_ascii_lowercase().as_str() {
            "general" => Some(VmTab::General),
            "performance" | "perf" => Some(VmTab::Performance),
            "storage" | "disks" => Some(VmTab::Storage),
            "network" | "networking" => Some(VmTab::Network),
            "memory" => Some(VmTab::Memory),
            "snapshots" | "snapshot" => Some(VmTab::Snapshots),
            "console" | "vnc" => Some(VmTab::Console),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum ActionKind {
    Start,
    StartPaused,
    Checkpoint,
    SnapshotWithQuiesce,
    ConvertToTemplate,
    Shutdown,
    Reboot,
    ForceShutdown,
    ForceReboot,
    Pause,
    Unpause,
    Suspend,
    Resume,
    Clone,
    SnapshotToVm,
    SnapshotToTemplate,
    Snapshot,
    Revert,
    Destroy,
}

impl ActionKind {
    pub const MENU: [ActionKind; 16] = [
        ActionKind::Start,
        ActionKind::StartPaused,
        ActionKind::Checkpoint,
        ActionKind::SnapshotWithQuiesce,
        ActionKind::ConvertToTemplate,
        ActionKind::Shutdown,
        ActionKind::Reboot,
        ActionKind::Pause,
        ActionKind::Unpause,
        ActionKind::Suspend,
        ActionKind::Resume,
        ActionKind::Snapshot,
        ActionKind::Clone,
        ActionKind::ForceShutdown,
        ActionKind::ForceReboot,
        ActionKind::Destroy,
    ];

    pub const BULK: [ActionKind; 4] = [
        ActionKind::Start,
        ActionKind::Shutdown,
        ActionKind::Reboot,
        ActionKind::Snapshot,
    ];

    pub fn operation(self) -> &'static str {
        match self {
            ActionKind::Start => "start",
            ActionKind::StartPaused => "start",
            ActionKind::Checkpoint => "checkpoint",
            ActionKind::SnapshotWithQuiesce => "snapshot_with_quiesce",
            ActionKind::ConvertToTemplate => "make_into_template",
            ActionKind::Shutdown => "clean_shutdown",
            ActionKind::Reboot => "clean_reboot",
            ActionKind::ForceShutdown => "hard_shutdown",
            ActionKind::ForceReboot => "hard_reboot",
            ActionKind::Pause => "pause",
            ActionKind::Unpause => "unpause",
            ActionKind::Suspend => "suspend",
            ActionKind::Resume => "resume",
            ActionKind::Clone => "clone",
            ActionKind::SnapshotToVm | ActionKind::SnapshotToTemplate => "clone",
            ActionKind::Snapshot => "snapshot",
            ActionKind::Revert => "revert",
            ActionKind::Destroy => "destroy",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            ActionKind::Start => "Start",
            ActionKind::StartPaused => "Start paused",
            ActionKind::Checkpoint => "Take checkpoint",
            ActionKind::SnapshotWithQuiesce => "Take quiesced snapshot",
            ActionKind::ConvertToTemplate => "Convert to template",
            ActionKind::Shutdown => "Shut down",
            ActionKind::Reboot => "Reboot",
            ActionKind::ForceShutdown => "Force shut down",
            ActionKind::ForceReboot => "Force reboot",
            ActionKind::Pause => "Pause",
            ActionKind::Unpause => "Unpause",
            ActionKind::Suspend => "Suspend",
            ActionKind::Resume => "Resume",
            ActionKind::Clone => "Clone",
            ActionKind::SnapshotToVm => "Create VM from snapshot",
            ActionKind::SnapshotToTemplate => "Create template from snapshot",
            ActionKind::Snapshot => "Take snapshot",
            ActionKind::Revert => "Revert snapshot",
            ActionKind::Destroy => "Delete",
        }
    }

    pub fn destructive(self) -> bool {
        matches!(
            self,
            ActionKind::ForceShutdown
                | ActionKind::ConvertToTemplate
                | ActionKind::ForceReboot
                | ActionKind::Revert
                | ActionKind::Destroy
        )
    }

    pub fn needs_name(self) -> bool {
        matches!(
            self,
            ActionKind::Clone
                | ActionKind::Snapshot
                | ActionKind::Checkpoint
                | ActionKind::SnapshotWithQuiesce
                | ActionKind::SnapshotToVm
                | ActionKind::SnapshotToTemplate
        )
    }

    pub fn to_action(
        self,
        name: &str,
        delete_disks: bool,
        delete_snapshots: bool,
        host_ref: Option<&str>,
    ) -> VmAction {
        match self {
            ActionKind::StartPaused => match host_ref {
                Some(host_ref) => VmAction::StartPausedOn {
                    host_ref: host_ref.to_owned(),
                },
                None => VmAction::StartPaused,
            },
            ActionKind::Checkpoint => VmAction::Checkpoint {
                name: name.trim().to_owned(),
            },
            ActionKind::SnapshotWithQuiesce => VmAction::SnapshotWithQuiesce {
                name: name.trim().to_owned(),
            },
            ActionKind::ConvertToTemplate => VmAction::ConvertToTemplate,
            ActionKind::Start => match host_ref {
                Some(host_ref) => VmAction::StartOn {
                    host_ref: host_ref.to_owned(),
                },
                None => VmAction::Start,
            },
            ActionKind::Shutdown => VmAction::Shutdown,
            ActionKind::Reboot => VmAction::Reboot,
            ActionKind::ForceShutdown => VmAction::ForceShutdown,
            ActionKind::ForceReboot => VmAction::ForceReboot,
            ActionKind::Pause => VmAction::Pause,
            ActionKind::Unpause => VmAction::Unpause,
            ActionKind::Suspend => VmAction::Suspend,
            ActionKind::Resume => match host_ref {
                Some(host_ref) => VmAction::ResumeOn {
                    host_ref: host_ref.to_owned(),
                },
                None => VmAction::Resume,
            },
            ActionKind::Clone => VmAction::Clone {
                name: name.trim().to_owned(),
            },
            ActionKind::SnapshotToVm | ActionKind::SnapshotToTemplate => {
                VmAction::CreateFromSnapshot {
                    name: name.trim().to_owned(),
                    as_template: self == ActionKind::SnapshotToTemplate,
                }
            }
            ActionKind::Snapshot => VmAction::Snapshot {
                name: name.trim().to_owned(),
            },
            ActionKind::Revert => VmAction::Revert,
            ActionKind::Destroy => VmAction::Destroy {
                delete_disks,
                delete_snapshots,
            },
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Default)]
pub enum PowerFilter {
    #[default]
    All,
    Running,
    Halted,
    Paused,
    Suspended,
}

impl PowerFilter {
    pub const ALL: [PowerFilter; 5] = [
        PowerFilter::All,
        PowerFilter::Running,
        PowerFilter::Halted,
        PowerFilter::Paused,
        PowerFilter::Suspended,
    ];

    pub fn label(self) -> &'static str {
        match self {
            PowerFilter::All => "All power states",
            PowerFilter::Running => "Running",
            PowerFilter::Halted => "Stopped",
            PowerFilter::Paused => "Paused",
            PowerFilter::Suspended => "Suspended",
        }
    }

    pub fn matches(self, state: &str) -> bool {
        match self {
            PowerFilter::All => true,
            PowerFilter::Halted => state == "Halted" || state == "Stopped",
            _ => self.label() == state,
        }
    }
}

#[derive(Clone)]
pub struct Intent {
    pub kind: ActionKind,
    pub targets: Vec<(String, String)>,
    pub name: String,
    pub delete_disks: bool,
    pub delete_snapshots: bool,
    pub host: Option<(String, String)>,
}

#[derive(Clone)]
pub struct ConnectState {
    pub profile_id: Option<String>,
    pub settings: ConnectionSettings,
    pub password: String,
    pub proxy: String,
    pub error: Option<String>,
    pub submitting: bool,
}

impl ConnectState {
    pub fn empty() -> Self {
        Self {
            profile_id: None,
            settings: ConnectionSettings {
                host: String::new(),
                username: "root".into(),
                label: String::new(),
                read_only: false,
                tls: Default::default(),
                proxy: None,
            },
            password: String::new(),
            proxy: String::new(),
            error: None,
            submitting: false,
        }
    }

    pub fn from_profile(profile_id: String, settings: ConnectionSettings) -> Self {
        Self {
            profile_id: Some(profile_id),
            proxy: settings.proxy.clone().unwrap_or_default(),
            settings,
            password: String::new(),
            error: None,
            submitting: false,
        }
    }

    pub fn protected_host(&self) -> bool {
        normalized_host(&self.settings.host) == "10.100.1.11"
    }
}

pub fn normalized_host(host: &str) -> String {
    host.trim()
        .trim_start_matches("https://")
        .trim_start_matches("http://")
        .split(['/', ':'])
        .next()
        .unwrap_or_default()
        .to_ascii_lowercase()
}

#[derive(Clone, Copy, PartialEq, Eq, Default)]
pub enum HostTab {
    #[default]
    General,
    Cpu,
    Performance,
    Networking,
    Storage,
    Hardware,
    Updates,
    Certificates,
    Health,
}

impl HostTab {
    pub const ALL: [HostTab; 9] = [
        HostTab::General,
        HostTab::Cpu,
        HostTab::Performance,
        HostTab::Networking,
        HostTab::Storage,
        HostTab::Hardware,
        HostTab::Updates,
        HostTab::Certificates,
        HostTab::Health,
    ];

    pub fn from_key(key: &str) -> Option<Self> {
        match key.to_ascii_lowercase().as_str() {
            "general" => Some(HostTab::General),
            "cpu" | "cpu-memory" => Some(HostTab::Cpu),
            "performance" | "perf" => Some(HostTab::Performance),
            "networking" | "network" => Some(HostTab::Networking),
            "storage" => Some(HostTab::Storage),
            "hardware" => Some(HostTab::Hardware),
            "updates" => Some(HostTab::Updates),
            "certificates" => Some(HostTab::Certificates),
            "health" => Some(HostTab::Health),
            _ => None,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            HostTab::General => "General",
            HostTab::Cpu => "CPU & memory",
            HostTab::Performance => "Performance",
            HostTab::Networking => "Networking",
            HostTab::Storage => "Storage",
            HostTab::Hardware => "Hardware",
            HostTab::Updates => "Updates",
            HostTab::Certificates => "Certificates",
            HostTab::Health => "Health",
        }
    }
}

#[derive(Clone)]
pub struct HostRoute {
    pub connection_id: String,
    pub host_uuid: String,
    pub host_name: String,
}

#[derive(Clone)]
pub enum LoadState<T> {
    Idle,
    Loading,
    Ready(T),
    Error(String),
}

impl<T> LoadState<T> {
    pub fn is_loading(&self) -> bool {
        matches!(self, LoadState::Loading)
    }

    pub fn ready(&self) -> Option<&T> {
        match self {
            LoadState::Ready(value) => Some(value),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum ToastKind {
    Info,
    Success,
    Error,
}

pub struct Toast {
    pub text: String,
    pub kind: ToastKind,
    pub created: Instant,
}

impl Toast {
    pub fn new(text: impl Into<String>, kind: ToastKind) -> Self {
        Self {
            text: text.into(),
            kind,
            created: Instant::now(),
        }
    }
}

#[derive(Clone)]
pub struct ResourceDialog {
    pub title: String,
    pub rows: Vec<(String, String)>,
}

pub struct RecordDialog {
    pub title: String,
    pub subtitle: String,
    pub values: Vec<(String, String)>,
    pub groups: Vec<InfoGroup>,
}

pub struct PropertiesDialog {
    pub connection_id: String,
    pub target: xen_core::properties::ObjectTarget,
    pub properties: LoadState<xen_core::properties::ObjectProperties>,
    pub original: Option<xen_core::properties::ObjectProperties>,
    pub new_tag: String,
    pub submitting: bool,
    pub error: Option<String>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum ConsoleStatus {
    Connecting,
    Connected,
    Closed,
}

pub struct ConsoleState {
    pub console_id: String,
    pub connection_id: String,
    pub vm_uuid: String,
    pub console_uuid: String,
    pub status: ConsoleStatus,
    pub message: String,
    pub width: u16,
    pub height: u16,
    pub pixels: Vec<u8>,
    pub version: u64,
    pub dirty: Vec<(u16, u16, u16, u16)>,
    pub dirty_full: bool,
    pub texture: Option<egui::TextureHandle>,
    pub clipboard: String,
    pub last_pointer: Option<(u16, u16, u8)>,
    pub modifiers: (bool, bool, bool, bool),
}

impl ConsoleState {
    fn in_bounds(&self, x: u16, y: u16, width: u16, height: u16) -> bool {
        let right = x as u32 + width as u32;
        let bottom = y as u32 + height as u32;
        width > 0 && height > 0 && right <= self.width as u32 && bottom <= self.height as u32
    }

    pub fn blit(&mut self, x: u16, y: u16, width: u16, height: u16, pixels: &[u8]) {
        if !self.in_bounds(x, y, width, height)
            || pixels.len() < width as usize * height as usize * 4
        {
            return;
        }
        let stride = self.width as usize * 4;
        let row_bytes = width as usize * 4;
        for row in 0..height as usize {
            let dst = (y as usize + row) * stride + x as usize * 4;
            let src = row * row_bytes;
            let row = &mut self.pixels[dst..dst + row_bytes];
            row.copy_from_slice(&pixels[src..src + row_bytes]);
            // RFB returns an unused alpha byte; make every pixel opaque.
            for pixel in row.as_chunks_mut::<4>().0 {
                pixel[3] = 255;
            }
        }
        if self.dirty.len() < 64 {
            self.dirty.push((x, y, width, height));
        } else {
            self.dirty_full = true;
        }
        self.version = self.version.wrapping_add(1);
    }

    pub fn copy_rect(&mut self, dst: (u16, u16, u16, u16), src: (u16, u16, u16, u16)) {
        let (dx, dy, width, height) = dst;
        let (sx, sy, src_width, src_height) = src;
        if width == 0
            || height == 0
            || width > src_width
            || height > src_height
            || !self.in_bounds(dx, dy, width, height)
            || !self.in_bounds(sx, sy, src_width, src_height)
        {
            return;
        }
        let stride = self.width as usize * 4;
        let row_bytes = width as usize * 4;
        let mut row = vec![0u8; row_bytes];
        self.dirty_full = true;
        for offset in 0..height as usize {
            let source = (sy as usize + offset) * stride + sx as usize * 4;
            row.copy_from_slice(&self.pixels[source..source + row_bytes]);
            let target = (dy as usize + offset) * stride + dx as usize * 4;
            self.pixels[target..target + row_bytes].copy_from_slice(&row);
        }
        self.version = self.version.wrapping_add(1);
    }

    pub fn connecting(console_id: String, connection_id: String, vm_uuid: String) -> Self {
        Self {
            console_id,
            connection_id,
            vm_uuid,
            console_uuid: String::new(),
            status: ConsoleStatus::Connecting,
            message: String::new(),
            width: 0,
            height: 0,
            pixels: Vec::new(),
            version: 0,
            dirty: Vec::new(),
            dirty_full: false,
            texture: None,
            clipboard: String::new(),
            last_pointer: None,
            modifiers: (false, false, false, false),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SshStatus {
    Connecting,
    Connected,
    Closed,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RdpStatus {
    Connecting,
    Connected,
    Closed,
}

pub struct RdpConsoleState {
    pub console_id: String,
    pub connection_id: String,
    pub vm_uuid: String,
    pub host: String,
    pub username: String,
    pub status: RdpStatus,
    pub message: String,
    pub width: u16,
    pub height: u16,
    pub pixels: Vec<u32>,
    pub texture: Option<egui::TextureHandle>,
    pub dirty: bool,
    pub buttons: u8,
}

pub struct RdpCredentialDialog {
    pub connection_id: String,
    pub vm_uuid: String,
    pub host: String,
    pub port: String,
    pub username: String,
    pub domain: String,
    pub password: String,
    pub error: Option<String>,
}

pub struct SshConsoleState {
    pub console_id: String,
    pub connection_id: String,
    pub vm_uuid: String,
    pub host: String,
    pub username: String,
    pub status: SshStatus,
    pub message: String,
    pub buffer: crate::ssh::TerminalBuffer,
}

pub struct SshCredentialDialog {
    pub connection_id: String,
    pub vm_uuid: String,
    pub host: String,
    pub port: String,
    pub username: String,
    pub password: String,
    pub error: Option<String>,
}

pub struct DrTaskDialog {
    pub connection_id: String,
    pub sr_type: String,
    pub server: String,
    pub serverpath: String,
    pub target: String,
    pub target_iqn: String,
    pub whitelist: String,
    pub acknowledged: bool,
    pub submitting: bool,
    pub error: Option<String>,
}

pub struct DrTaskDeleteConfirm {
    pub connection_id: String,
    pub uuid: String,
    pub introduced_srs: Vec<String>,
    pub submitting: bool,
    pub error: Option<String>,
}

pub struct PvsSiteDialog {
    pub connection_id: String,
    pub name: String,
    pub description: String,
    pub submitting: bool,
    pub error: Option<String>,
}

pub struct PvsServerDialog {
    pub connection_id: String,
    pub sites: Vec<(String, String)>,
    pub site_uuid: Option<String>,
    pub addresses: String,
    pub first_port: String,
    pub last_port: String,
    pub submitting: bool,
    pub error: Option<String>,
}

pub struct PvsCacheDialog {
    pub connection_id: String,
    pub sites: Vec<(String, String)>,
    pub site_uuid: Option<String>,
    pub hosts: Vec<(String, String)>,
    pub host_uuid: Option<String>,
    pub srs: Vec<(String, String)>,
    pub sr_uuid: Option<String>,
    pub size_mib: String,
    pub submitting: bool,
    pub error: Option<String>,
}

pub struct PvsProxyDialog {
    pub connection_id: String,
    pub sites: Vec<(String, String)>,
    pub site_uuid: Option<String>,
    pub vifs: Vec<xen_core::pvs::PvsVif>,
    pub vif_uuid: Option<String>,
    pub loading: bool,
    pub submitting: bool,
    pub error: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PvsDeleteKind {
    Site,
    Server,
    Proxy,
    Cache,
}

pub struct PvsDeleteConfirm {
    pub connection_id: String,
    pub kind: PvsDeleteKind,
    pub uuid: String,
    pub name: String,
    pub submitting: bool,
    pub error: Option<String>,
}

pub struct VmssDialog {
    pub connection_id: String,
    pub uuid: Option<String>,
    pub name: String,
    pub description: String,
    pub snapshot_type: String,
    pub frequency: String,
    pub minute: String,
    pub hour: String,
    pub days: HashSet<String>,
    pub retained: String,
    pub enabled: bool,
    pub vms: Vec<(String, String)>,
    pub selected_vms: HashSet<String>,
    pub submitting: bool,
    pub error: Option<String>,
}

pub struct VmssDeleteConfirm {
    pub connection_id: String,
    pub uuid: String,
    pub name: String,
    pub submitting: bool,
    pub error: Option<String>,
}

pub struct HaEnableDialog {
    pub connection_id: String,
    pub srs: Vec<(String, String)>,
    pub selected: HashSet<String>,
    pub failures: String,
    pub acknowledged: bool,
    pub submitting: bool,
    pub error: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PoolConfirmAction {
    DisableHa,
    EnableWlb,
    DisableWlb,
    DeconfigureWlb,
}

pub struct PoolConfirm {
    pub connection_id: String,
    pub action: PoolConfirmAction,
    pub acknowledged: bool,
    pub submitting: bool,
    pub error: Option<String>,
}

pub struct WlbConfigDialog {
    pub connection_id: String,
    pub url: String,
    pub wlb_username: String,
    pub wlb_password: String,
    pub xenserver_username: String,
    pub xenserver_password: String,
    pub acknowledged: bool,
    pub submitting: bool,
    pub error: Option<String>,
}

pub struct AdEnableDialog {
    pub connection_id: String,
    pub domain: String,
    pub user: String,
    pub password: String,
    pub acknowledged: bool,
    pub submitting: bool,
    pub error: Option<String>,
}

pub struct AdDisableDialog {
    pub connection_id: String,
    pub user: String,
    pub password: String,
    pub disable_machine_account: bool,
    pub acknowledged: bool,
    pub submitting: bool,
    pub error: Option<String>,
}

pub struct UserRoleDialog {
    pub connection_id: String,
    pub subject_uuid: String,
    pub identifier: String,
    pub display_name: String,
    pub selected: HashSet<String>,
    pub submitting: bool,
    pub error: Option<String>,
}

pub struct SrProbeDialog {
    pub connection_id: String,
    pub hosts: Vec<(String, String)>,
    pub host_uuid: Option<String>,
    pub location: String,
    pub submitting: bool,
    pub error: Option<String>,
    pub result: Option<SrProbe>,
}

pub struct BondCreateDialog {
    pub connection_id: String,
    pub name: String,
    pub mode: String,
    pub mtu: String,
    pub autoplug: bool,
    pub members: HashSet<String>,
    pub submitting: bool,
    pub error: Option<String>,
}

pub struct VlanCreateDialog {
    pub connection_id: String,
    pub pif_uuid: Option<String>,
    pub tag: String,
    pub name: String,
    pub description: String,
    pub mtu: String,
    pub autoplug: bool,
    pub submitting: bool,
    pub error: Option<String>,
}

pub struct NetworkCreateDialog {
    pub connection_id: String,
    pub kind: String,
    pub name: String,
    pub description: String,
    pub mtu: String,
    pub pif_uuid: Option<String>,
    pub protocol: String,
    pub autoplug: bool,
    pub submitting: bool,
    pub error: Option<String>,
}

pub struct NetworkDestroyConfirm {
    pub connection_id: String,
    pub network_uuid: String,
    pub name: String,
    pub acknowledged: bool,
    pub submitting: bool,
    pub error: Option<String>,
}

pub struct SrIntroduceDialog {
    pub connection_id: String,
    pub uuid: String,
    pub name: String,
    pub description: String,
    pub submitting: bool,
    pub error: Option<String>,
}

pub struct SrCreateDialog {
    pub connection_id: String,
    pub hosts: Vec<(String, String)>,
    pub host_uuid: Option<String>,
    pub name: String,
    pub description: String,
    pub location: String,
    pub submitting: bool,
    pub error: Option<String>,
}

pub struct SrReattachDialog {
    pub connection_id: String,
    pub sr_uuid: String,
    pub sr_name: String,
    pub hosts: Vec<(String, String)>,
    pub host_uuid: Option<String>,
    pub location: String,
    pub submitting: bool,
    pub error: Option<String>,
}

pub struct SrActionConfirm {
    pub connection_id: String,
    pub sr_uuid: String,
    pub sr_name: String,
    pub action: xen_core::operations::SrAction,
    pub acknowledged: bool,
    pub submitting: bool,
    pub error: Option<String>,
}

pub struct SaveSearchState {
    pub name: String,
    pub query: String,
    pub group_by: GroupBy,
    pub editing: Option<usize>,
    pub error: Option<String>,
}

pub struct SnapshotDeleteConfirm {
    pub connection_id: String,
    pub vm_uuid: String,
    pub vm_name: String,
    pub snapshots: Vec<(String, String)>,
    pub submitting: bool,
    pub error: Option<String>,
}

pub struct HostActionConfirm {
    pub connection_id: String,
    pub host_uuid: String,
    pub host_name: String,
    pub action: xen_core::operations::HostAction,
    pub acknowledged: bool,
    pub submitting: bool,
    pub error: Option<String>,
}

pub struct MoveDiskDialog {
    pub connection_id: String,
    pub vm_uuid: String,
    pub vbd_uuid: String,
    pub name: String,
    pub size: u64,
    pub live: bool,
    pub srs: Vec<(String, String, u64)>,
    pub selected: Option<String>,
    pub submitting: bool,
    pub error: Option<String>,
}

pub struct VdiDeleteConfirm {
    pub connection_id: String,
    pub sr_uuid: String,
    pub vdi_uuid: String,
    pub name: String,
    pub size: u64,
    pub submitting: bool,
    pub error: Option<String>,
}

pub struct HaEditDialog {
    pub connection_id: String,
    pub vm_uuid: String,
    pub hosts: Vec<(String, String)>,
    pub restart_priority: String,
    pub start_delay: String,
    pub shutdown_delay: String,
    pub affinity_host_uuid: Option<String>,
    pub submitting: bool,
    pub error: Option<String>,
}

pub struct AttachVdiDialog {
    pub connection_id: String,
    pub vm_uuid: String,
    pub vdis: Vec<VdiSummary>,
    pub selected: Option<String>,
    pub search: String,
    pub bootable: bool,
    pub loading: bool,
    pub submitting: bool,
    pub error: Option<String>,
}

pub struct DiskAddDialog {
    pub connection_id: String,
    pub vm_uuid: String,
    pub srs: Vec<(String, String, u64)>,
    pub sr_uuid: Option<String>,
    pub name: String,
    pub size_gib: String,
    pub submitting: bool,
    pub error: Option<String>,
}

pub struct DiskResizeDialog {
    pub connection_id: String,
    pub vm_uuid: String,
    pub vdi_uuid: String,
    pub name: String,
    pub current_gib: u64,
    pub size_gib: String,
    pub submitting: bool,
    pub error: Option<String>,
}

pub struct DiskRemoveDialog {
    pub connection_id: String,
    pub vm_uuid: String,
    pub vbd_uuid: String,
    pub name: String,
    pub owner: bool,
    pub delete_vdi: bool,
    pub submitting: bool,
    pub error: Option<String>,
}

pub enum DiskDialog {
    Add(DiskAddDialog),
    Resize(DiskResizeDialog),
    Remove(DiskRemoveDialog),
}

pub struct VifDialog {
    pub connection_id: String,
    pub vm_uuid: String,
    pub vif_uuid: Option<String>,
    pub networks: Vec<(String, String)>,
    pub network_uuid: Option<String>,
    pub mac: String,
    pub locking_mode: String,
    pub submitting: bool,
    pub error: Option<String>,
}

pub struct VifConfirm {
    pub connection_id: String,
    pub vm_uuid: String,
    pub vif_uuid: String,
    pub mac: String,
}

pub struct IsoDialog {
    pub connection_id: String,
    pub vm_uuid: String,
    pub vbd_uuid: String,
    pub drive: String,
    pub isos: Vec<VmIso>,
    pub selected: Option<String>,
    pub search: String,
    pub submitting: bool,
    pub error: Option<String>,
}

pub struct PoolOptionsDialog {
    pub connection_id: String,
    pub uuid: String,
    pub original: Option<xen_core::pool_options::PoolOptions>,
    pub data: LoadState<xen_core::pool_options::PoolOptions>,
    pub error: Option<String>,
    pub submitting: bool,
}

pub struct VmCopyDialog {
    pub connection_id: String,
    pub vm_uuid: String,
    pub request: xen_core::provisioning::CopyVmRequest,
}

pub struct VmResourceEditState {
    pub connection_id: String,
    pub vm_uuid: String,
    pub vcpus_max: String,
    pub vcpus_at_startup: String,
    pub cpu_weight: String,
    pub original_cpu_weight: String,
    pub cores_per_socket: String,
    pub original_cores_per_socket: String,
    pub original_vcpus_max: u64,
    pub max_cores_per_socket: u64,
    pub memory_static_max_mib: String,
    pub memory_dynamic_min_mib: String,
    pub memory_dynamic_max_mib: String,
    pub boot_order: String,
    pub firmware: String,
    pub secure_boot: bool,
    pub boot_policy: String,
    pub after_crash: String,
    pub pv_args: String,
    pub original_pv_args: String,
    pub auto_poweron: bool,
    pub original_auto_poweron: bool,
    pub is_hvm: bool,
    pub running: bool,
    pub can_hotplug_vcpus: bool,
    pub current_vcpus: u64,
    pub error: Option<String>,
    pub submitting: bool,
}

impl VmResourceEditState {
    pub fn parse_resources(&self) -> Result<xen_core::operations::VmResourceUpdate, String> {
        let parse = |value: &str, label: &str| {
            value
                .trim()
                .parse::<u64>()
                .map_err(|_| format!("Enter a whole number for {label}."))
        };
        let vcpus_max = parse(&self.vcpus_max, "the vCPU maximum")?;
        let vcpus_at_startup = parse(&self.vcpus_at_startup, "vCPUs at startup")?;
        let memory_static_max_mib = parse(&self.memory_static_max_mib, "static memory in MiB")?;
        let memory_dynamic_min_mib =
            parse(&self.memory_dynamic_min_mib, "the dynamic memory minimum")?;
        let memory_dynamic_max_mib =
            parse(&self.memory_dynamic_max_mib, "the dynamic memory maximum")?;
        if vcpus_max == 0 || vcpus_at_startup == 0 || vcpus_at_startup > vcpus_max {
            return Err("vCPUs at startup must be between 1 and the vCPU maximum.".into());
        }
        if memory_static_max_mib == 0 || memory_dynamic_min_mib == 0 || memory_dynamic_max_mib == 0
        {
            return Err("Memory must be greater than zero.".into());
        }
        if memory_dynamic_min_mib > memory_dynamic_max_mib
            || memory_dynamic_max_mib > memory_static_max_mib
        {
            return Err("Dynamic memory must stay between the static minimum and maximum.".into());
        }
        let cpu_weight = if self.cpu_weight != self.original_cpu_weight {
            let weight = self
                .cpu_weight
                .trim()
                .parse::<u16>()
                .map_err(|_| "CPU weight must be between 1 and 65535.")?;
            if weight == 0 {
                return Err("CPU weight must be between 1 and 65535.".into());
            }
            Some(weight)
        } else {
            None
        };
        let cores_per_socket = if self.cores_per_socket != self.original_cores_per_socket
            || vcpus_max != self.original_vcpus_max
        {
            let cores = parse(&self.cores_per_socket, "cores per socket")?;
            xen_core::vm::validate_cpu_topology(vcpus_max, cores, self.max_cores_per_socket)
                .map_err(|e| e.to_string())?;
            Some(cores)
        } else {
            None
        };
        Ok(xen_core::operations::VmResourceUpdate {
            vcpus_max,
            vcpus_at_startup,
            memory_static_max_mib,
            memory_dynamic_min_mib,
            memory_dynamic_max_mib,
            boot_order: self.is_hvm.then(|| self.boot_order.clone()),
            firmware: self.is_hvm.then(|| self.firmware.clone()),
            secure_boot: self.is_hvm.then_some(self.secure_boot),
            boot_policy: self.is_hvm.then(|| self.boot_policy.clone()),
            after_crash: Some(self.after_crash.clone()),
            pv_args: (!self.is_hvm && self.pv_args != self.original_pv_args)
                .then(|| self.pv_args.clone()),
            auto_poweron: (self.auto_poweron != self.original_auto_poweron)
                .then_some(self.auto_poweron),
            cpu_weight,
            cores_per_socket,
        })
    }
}

pub struct NewVmWizard {
    pub connection_id: String,
    pub draft: xen_core::provisioning::CreateVmRequest,
    pub step: usize,
    pub template: LoadState<VmDetails>,
    pub error: Option<String>,
    pub submitting: bool,
}

pub struct XvaDialog {
    pub connection_id: String,
    pub vm_uuid: Option<String>,
    pub path: String,
    pub name: String,
    pub sr_uuid: Option<String>,
    pub network_uuid: Option<String>,
    pub error: Option<String>,
    pub submitting: bool,
}

pub struct UiState {
    pub toolbar_visible: bool,
    pub selection_history: Vec<ObjectKey>,
    pub history_index: usize,
    pub navigating_history: bool,
    pub properties: Option<PropertiesDialog>,
    pub properties_seq: u64,
    pub xva_dialog: Option<XvaDialog>,
    pub new_vm: Option<NewVmWizard>,
    pub new_vm_seq: u64,
    pub view: View,
    pub scope: Scope,
    pub selection: Option<ObjectSelection>,
    pub navigation_mode: crate::views::sidebar::NavigationMode,
    pub tree_initialized: HashSet<String>,
    pub tree_preferences: crate::config::TreePreferences,
    pub tree_query: String,
    pub tree_expanded: HashSet<String>,
    pub vm_subtab: VmTab,
    pub vm_last_tabs: [VmTab; 3],
    pub vm_state: LoadState<VmDetails>,
    pub vm_seq: u64,
    pub vm_perf_state: LoadState<HostPerformance>,
    pub vm_perf_seq: u64,
    pub vm_perf_range: u64,
    pub console: Option<ConsoleState>,
    pub console_seq: u64,
    pub console_focus: bool,
    pub vm_resource_edit: Option<VmResourceEditState>,
    pub vm_copy: Option<VmCopyDialog>,
    pub pool_options: Option<PoolOptionsDialog>,
    pub pool_options_seq: u64,
    pub iso_dialog: Option<IsoDialog>,
    pub vif_dialog: Option<VifDialog>,
    pub vif_confirm: Option<VifConfirm>,
    pub disk_dialog: Option<DiskDialog>,
    pub attach_vdi_dialog: Option<AttachVdiDialog>,
    pub ha_edit_dialog: Option<HaEditDialog>,
    pub sr_vdis: Vec<SrVdi>,
    pub sr_vdis_sr: Option<String>,
    pub sr_vdis_loading: bool,
    pub sr_vdis_error: Option<String>,
    pub sr_vdis_seq: u64,
    pub vdi_delete_confirm: Option<VdiDeleteConfirm>,
    pub move_disk_dialog: Option<MoveDiskDialog>,
    pub vdi_catalog_seq: u64,
    pub host_action_confirm: Option<HostActionConfirm>,
    pub snapshot_selection: HashSet<String>,
    pub snapshot_confirm: Option<SnapshotDeleteConfirm>,
    pub sr_action_confirm: Option<SrActionConfirm>,
    pub sr_create_dialog: Option<SrCreateDialog>,
    pub sr_introduce_dialog: Option<SrIntroduceDialog>,
    pub sr_reattach_dialog: Option<SrReattachDialog>,
    pub network_create_dialog: Option<NetworkCreateDialog>,
    pub bond_create_dialog: Option<BondCreateDialog>,
    pub vlan_create_dialog: Option<VlanCreateDialog>,
    pub pif_catalog: Vec<PifSummary>,
    pub pif_catalog_connection: Option<String>,
    pub pif_catalog_loading: bool,
    pub pif_catalog_error: Option<String>,
    pub pif_catalog_seq: u64,
    pub pbd_catalog: Vec<PbdSummary>,
    pub pbd_catalog_sr: Option<String>,
    pub pbd_catalog_loading: bool,
    pub pbd_catalog_error: Option<String>,
    pub pbd_catalog_seq: u64,
    pub sr_probe_dialog: Option<SrProbeDialog>,
    pub user_role_dialog: Option<UserRoleDialog>,
    pub ad_enable_dialog: Option<AdEnableDialog>,
    pub ssh_console: Option<SshConsoleState>,
    pub ssh_credentials: Option<SshCredentialDialog>,
    pub ssh_seq: u64,
    pub rdp_console: Option<RdpConsoleState>,
    pub rdp_credentials: Option<RdpCredentialDialog>,
    pub rdp_seq: u64,
    pub ha_enable_dialog: Option<HaEnableDialog>,
    pub pool_confirm: Option<PoolConfirm>,
    pub wlb_config_dialog: Option<WlbConfigDialog>,
    pub vmss_dialog: Option<VmssDialog>,
    pub pvs_site_dialog: Option<PvsSiteDialog>,
    pub pvs_server_dialog: Option<PvsServerDialog>,
    pub pvs_cache_dialog: Option<PvsCacheDialog>,
    pub pvs_proxy_dialog: Option<PvsProxyDialog>,
    pub pvs_delete_confirm: Option<PvsDeleteConfirm>,
    pub pvs_vif_seq: u64,
    pub dr_task_dialog: Option<DrTaskDialog>,
    pub dr_task_delete_confirm: Option<DrTaskDeleteConfirm>,
    pub vmss_delete_confirm: Option<VmssDeleteConfirm>,
    pub ad_disable_dialog: Option<AdDisableDialog>,
    pub sr_probe_seq: u64,
    pub network_destroy_confirm: Option<NetworkDestroyConfirm>,
    pub query: String,
    pub group_by: GroupBy,
    pub saved_searches: Vec<SavedSearch>,
    pub save_search: Option<SaveSearchState>,
    pub power: PowerFilter,
    pub page: usize,
    pub selected: HashSet<(String, String)>,
    pub detail: Option<(String, String)>,
    pub connect: Option<ConnectState>,
    pub intent: Option<Intent>,
    pub toasts: Vec<Toast>,
    pub busy: HashSet<String>,
    pub updated: Option<DateTime<Local>>,
    pub host_route: Option<HostRoute>,
    pub host_tab: HostTab,
    pub host_state: LoadState<HostDetails>,
    pub auth_state: LoadState<xen_core::auth::AuthInfo>,
    pub auth_seq: u64,
    pub pool_status_state: LoadState<xen_core::pool_status::PoolStatus>,
    pub pool_status_seq: u64,
    pub host_seq: u64,
    pub perf_range: u64,
    pub perf_state: LoadState<HostPerformance>,
    pub perf_seq: u64,
    pub perf_query: String,
    pub perf_page: usize,
    pub help: bool,
    pub resource: Option<ResourceDialog>,
    pub record_query: String,
    pub record_page: usize,
    pub record_dialog: Option<RecordDialog>,
    pub focus_search: bool,
    pub confirm_remove: Option<String>,
    pub pending_scope: Option<String>,
    pub loaded: bool,
}

impl Default for UiState {
    fn default() -> Self {
        Self {
            toolbar_visible: true,
            selection_history: vec![ObjectKey::Home],
            history_index: 0,
            navigating_history: false,
            properties: None,
            properties_seq: 0,
            xva_dialog: None,
            new_vm: None,
            new_vm_seq: 0,
            view: View::Overview,
            scope: Scope::All,
            selection: Some(ObjectSelection {
                key: ObjectKey::Home,
                title: "Home".into(),
            }),
            navigation_mode: Default::default(),
            tree_initialized: HashSet::new(),
            tree_preferences: crate::config::load_ui_preferences().tree,
            tree_query: String::new(),
            tree_expanded: HashSet::new(),
            vm_subtab: VmTab::General,
            vm_last_tabs: [VmTab::General; 3],
            vm_state: LoadState::Idle,
            vm_seq: 0,
            vm_perf_state: LoadState::Idle,
            vm_perf_seq: 0,
            vm_perf_range: 900,
            console: None,
            console_seq: 0,
            console_focus: false,
            vm_resource_edit: None,
            vm_copy: None,
            pool_options: None,
            pool_options_seq: 0,
            iso_dialog: None,
            vif_dialog: None,
            vif_confirm: None,
            disk_dialog: None,
            attach_vdi_dialog: None,
            ha_edit_dialog: None,
            sr_vdis: Vec::new(),
            sr_vdis_sr: None,
            sr_vdis_loading: false,
            sr_vdis_error: None,
            sr_vdis_seq: 0,
            vdi_delete_confirm: None,
            move_disk_dialog: None,
            vdi_catalog_seq: 0,
            host_action_confirm: None,
            snapshot_selection: HashSet::new(),
            snapshot_confirm: None,
            sr_action_confirm: None,
            sr_create_dialog: None,
            sr_introduce_dialog: None,
            sr_reattach_dialog: None,
            network_create_dialog: None,
            bond_create_dialog: None,
            vlan_create_dialog: None,
            pif_catalog: Vec::new(),
            pif_catalog_connection: None,
            pif_catalog_loading: false,
            pif_catalog_error: None,
            pif_catalog_seq: 0,
            pbd_catalog: Vec::new(),
            pbd_catalog_sr: None,
            pbd_catalog_loading: false,
            pbd_catalog_error: None,
            pbd_catalog_seq: 0,
            sr_probe_dialog: None,
            user_role_dialog: None,
            ad_enable_dialog: None,
            ssh_console: None,
            ssh_credentials: None,
            ssh_seq: 0,
            rdp_console: None,
            rdp_credentials: None,
            rdp_seq: 0,
            ha_enable_dialog: None,
            pool_confirm: None,
            wlb_config_dialog: None,
            vmss_dialog: None,
            pvs_site_dialog: None,
            pvs_server_dialog: None,
            pvs_cache_dialog: None,
            pvs_proxy_dialog: None,
            pvs_delete_confirm: None,
            pvs_vif_seq: 0,
            dr_task_dialog: None,
            dr_task_delete_confirm: None,
            vmss_delete_confirm: None,
            ad_disable_dialog: None,
            sr_probe_seq: 0,
            network_destroy_confirm: None,
            query: String::new(),
            group_by: GroupBy::None,
            saved_searches: Vec::new(),
            save_search: None,
            power: PowerFilter::All,
            page: 0,
            selected: HashSet::new(),
            detail: None,
            connect: None,
            intent: None,
            toasts: Vec::new(),
            busy: HashSet::new(),
            updated: None,
            host_route: None,
            host_tab: HostTab::General,
            host_state: LoadState::Idle,
            auth_state: LoadState::Idle,
            auth_seq: 0,
            pool_status_state: LoadState::Idle,
            pool_status_seq: 0,
            host_seq: 0,
            perf_range: 900,
            perf_state: LoadState::Idle,
            perf_seq: 0,
            perf_query: String::new(),
            perf_page: 0,
            help: false,
            resource: None,
            record_query: String::new(),
            record_page: 0,
            record_dialog: None,
            focus_search: false,
            confirm_remove: None,
            pending_scope: None,
            loaded: false,
        }
    }
}

pub enum UiAction {
    OpenProperties {
        connection_id: String,
        target: xen_core::properties::ObjectTarget,
    },
    ReloadProperties,
    SaveProperties,
    CloseProperties,
    OpenXvaExport {
        connection_id: String,
        vm_uuid: String,
    },
    OpenXvaImport(String),
    CloseXvaDialog,
    SubmitXvaTransfer,
    OpenNewVmWizard {
        connection_id: String,
        source: xen_core::provisioning::VmCreationSource,
    },
    SelectNewVmSource(xen_core::provisioning::VmCreationSource),
    CloseNewVmWizard,
    CreateNewVm,
    SetView(View),
    SetScope(Scope),
    OpenObject {
        key: ObjectKey,
        title: String,
    },
    NavigateHistory(bool),
    ToggleTreeNode(String),
    Connect {
        settings: ConnectionSettings,
        password: String,
        profile_id: Option<String>,
    },
    Refresh(Option<String>),
    Reconnect(String),
    Disconnect(String),
    Remove(String),
    OpenVm(String, String),
    Propose(ActionKind, Vec<(String, String)>),
    ProposeOnHost(ActionKind, Vec<(String, String)>, String, String),
    SubmitIntent,
    CloseIntent,
    CheckTask(String),
    CancelTask(String),
    OpenHostDetails {
        connection_id: String,
        host_uuid: String,
        host_name: String,
    },
    CloseHostDetails,
    ReloadHostDetails,
    LoadPerformance,
    OpenResource {
        title: String,
        rows: Vec<(String, String)>,
    },
    CloseResource,
    SetHostTab(HostTab),
    SetVmTab(VmTab),
    ReloadVmDetails,
    LoadVmPerformance,
    SetVmPerfRange(u64),
    OpenVmCopy {
        connection_id: String,
        vm_uuid: String,
    },
    CloseVmCopy,
    SubmitVmCopy,
    OpenPoolOptions {
        connection_id: String,
    },
    ReloadPoolOptions,
    SavePoolOptions,
    ClosePoolOptions,
    OpenVmResources,
    CloseVmResources,
    SaveVmResources,
    OpenIsoDialog {
        vbd_uuid: String,
        drive: String,
    },
    CloseIsoDialog,
    SaveIso,
    DismissAlert {
        connection_id: String,
        message_uuid: String,
    },
    LoadPbdCatalog {
        seq: u64,
        connection_id: String,
        sr_uuid: String,
    },
    SetPbdPlugged {
        connection_id: String,
        pbd_uuid: String,
        plugged: bool,
    },
    RepairSr,
    OpenSrProbe,
    CloseSrProbe,
    SaveSrProbe,
    LoadPifCatalog {
        seq: u64,
        connection_id: String,
    },
    SetPifPlugged {
        connection_id: String,
        pif_uuid: String,
        plugged: bool,
    },
    OpenBondCreate,
    CloseBondCreate,
    SaveBondCreate,
    OpenVlanCreate,
    CloseVlanCreate,
    SaveVlanCreate,
    OpenNetworkCreate,
    CloseNetworkCreate,
    SaveNetwork,
    PromptDestroyNetwork {
        connection_id: String,
        network_uuid: String,
        name: String,
    },
    CancelNetworkDestroy,
    ConfirmNetworkDestroy,
    OpenDrTaskDialog,
    CloseDrTaskDialog,
    SaveDrTaskDialog,
    PromptDrTaskDestroy {
        uuid: String,
        introduced_srs: Vec<String>,
    },
    CancelDrTaskDestroy,
    ConfirmDrTaskDestroy,
    OpenPvsSiteDialog,
    ClosePvsSiteDialog,
    SavePvsSiteDialog,
    OpenPvsServerDialog,
    ClosePvsServerDialog,
    SavePvsServerDialog,
    OpenPvsCacheDialog,
    ClosePvsCacheDialog,
    SavePvsCacheDialog,
    OpenPvsProxyDialog,
    ClosePvsProxyDialog,
    SavePvsProxyDialog,
    PromptPvsDelete {
        kind: PvsDeleteKind,
        uuid: String,
        name: String,
    },
    CancelPvsDelete,
    ConfirmPvsDelete,
    OpenVmssCreate,
    OpenVmssEdit(String),
    CloseVmssDialog,
    SaveVmssDialog,
    PromptVmssDelete {
        uuid: String,
        name: String,
    },
    CancelVmssDelete,
    ConfirmVmssDelete,
    OpenHaEnable,
    CloseHaEnable,
    SaveHaEnable,
    PromptHaDisable,
    OpenWlbConfig,
    CloseWlbConfig,
    SaveWlbConfig,
    PromptWlbToggle(bool),
    PromptWlbDeconfigure,
    CancelPoolConfirm,
    ConfirmPoolConfirm,
    OpenRdpDialog,
    CloseRdpDialog,
    ConnectRdp,
    RdpInput {
        console_id: String,
        input: crate::rdp::RdpInput,
    },
    CloseRdpConsole,
    ClearRdpConsole,
    OpenSshDialog,
    CloseSshDialog,
    ConnectSsh,
    SshInput {
        console_id: String,
        data: Vec<u8>,
    },
    CloseSshConsole,
    ClearSshConsole,
    OpenAdEnable,
    CloseAdEnable,
    SaveAdEnable,
    OpenAdDisable,
    CloseAdDisable,
    SaveAdDisable,
    OpenUserRoles(String),
    CloseUserRoles,
    SaveUserRoles,
    OpenSrIntroduce,
    CloseSrIntroduce,
    SaveSrIntroduce,
    OpenSrCreate,
    CloseSrCreate,
    SaveSrCreate,
    OpenSrReattach,
    CloseSrReattach,
    SaveSrReattach,
    PromptSrAction {
        connection_id: String,
        sr_uuid: String,
        sr_name: String,
        action: xen_core::operations::SrAction,
    },
    CancelSrAction,
    ConfirmSrAction,
    PromptDeleteSnapshots,
    CloseSnapshotDelete,
    ConfirmSnapshotDelete,
    PromptHostAction {
        host_uuid: String,
        host_name: String,
        action: xen_core::operations::HostAction,
    },
    CancelHostAction,
    ConfirmHostAction,
    OpenAddDisk,
    LoadSrVdis {
        seq: u64,
        connection_id: String,
        sr_uuid: String,
    },
    PromptDeleteVdi {
        vdi_uuid: String,
        name: String,
        size: u64,
    },
    CancelDeleteVdi,
    ConfirmDeleteVdi,
    LoadAuth,
    LoadPoolStatus,
    OpenHaEdit,
    CloseHaEdit,
    SaveHaEdit,
    OpenAttachVdi,
    CloseAttachVdi,
    SaveAttachVdi,
    SetVbdBootable {
        vbd_uuid: String,
        bootable: bool,
    },
    OpenMoveDisk {
        vbd_uuid: String,
        name: String,
        size: u64,
        source_sr_uuid: String,
    },
    CloseMoveDisk,
    SaveMoveDisk,
    SetVbdMode {
        vbd_uuid: String,
        mode: String,
    },
    SetVbdPlugged {
        vbd_uuid: String,
        plugged: bool,
    },
    ApplySavedSearch(usize),
    DeleteSavedSearch(usize),
    EditSavedSearch(usize),
    OpenSaveSearch,
    CloseSaveSearch,
    SaveCurrentSearch,
    AddCdDrive,
    OpenDiskResize {
        vdi_uuid: String,
        name: String,
        current_gib: u64,
    },
    PromptRemoveDisk {
        vbd_uuid: String,
        name: String,
        owner: bool,
    },
    CloseDiskDialog,
    SaveDiskDialog,
    OpenVifDialog(Option<String>),
    CloseVifDialog,
    SaveVif,
    PromptRemoveVif {
        vif_uuid: String,
        mac: String,
    },
    CancelRemoveVif,
    RemoveVif,
    SetVifPlugged {
        vif_uuid: String,
        plugged: bool,
    },
    OpenConsole(String, String),
    CloseConsole,
    ConsoleKey {
        console_id: String,
        keycode: u32,
        down: bool,
    },
    ConsolePointer {
        console_id: String,
        x: u16,
        y: u16,
        buttons: u8,
    },
    ConsoleClipboard {
        console_id: String,
        text: String,
    },
    LaunchRdp(String),
    DismissToast(usize),
    ConfirmRemove(String),
    CancelRemove,
}

// ---------------------------------------------------------------------------
// Workspace helpers
// ---------------------------------------------------------------------------

pub fn connection_id(connection: &ConnectionView) -> &str {
    match connection {
        ConnectionView::Offline { id, .. }
        | ConnectionView::Online { id, .. }
        | ConnectionView::Stale { id, .. } => id,
    }
}

pub fn connection_settings(connection: &ConnectionView) -> &ConnectionSettings {
    match connection {
        ConnectionView::Offline { settings, .. }
        | ConnectionView::Online { settings, .. }
        | ConnectionView::Stale { settings, .. } => settings,
    }
}

pub fn connection_inventory(connection: &ConnectionView) -> Option<&Inventory> {
    match connection {
        ConnectionView::Online { inventory, .. } | ConnectionView::Stale { inventory, .. } => {
            Some(inventory)
        }
        ConnectionView::Offline { .. } => None,
    }
}

pub fn connection_name(connection: &ConnectionView) -> String {
    let settings = connection_settings(connection);
    let label = settings.label.trim();
    if !label.is_empty() {
        return label.to_owned();
    }
    match connection {
        ConnectionView::Online { inventory, .. } | ConnectionView::Stale { inventory, .. } => {
            inventory.pool_name.clone()
        }
        ConnectionView::Offline { settings, .. } => settings.host.clone(),
    }
}

pub fn connection_status(connection: &ConnectionView) -> &'static str {
    match connection {
        ConnectionView::Offline { .. } => "offline",
        ConnectionView::Online { .. } => "online",
        ConnectionView::Stale { .. } => "stale",
    }
}

pub fn is_online(connection: &ConnectionView) -> bool {
    matches!(connection, ConnectionView::Online { .. })
}

pub fn is_read_only(connection: &ConnectionView) -> bool {
    connection_settings(connection).read_only
}

pub fn find_connection<'a>(workspace: &'a WorkspaceView, id: &str) -> Option<&'a ConnectionView> {
    workspace
        .connections
        .iter()
        .find(|connection| connection_id(connection) == id)
}

pub fn visible_connections<'a>(
    workspace: &'a WorkspaceView,
    scope: &Scope,
) -> Vec<&'a ConnectionView> {
    workspace
        .connections
        .iter()
        .filter(|connection| match scope.connection_id() {
            Some(id) => connection_id(connection) == id,
            None => true,
        })
        .collect()
}

#[derive(Clone, Copy)]
pub struct Target<'a> {
    pub connection: &'a ConnectionView,
    pub vm: &'a VirtualMachine,
}

pub fn scoped_targets<'a>(workspace: &'a WorkspaceView, scope: &Scope) -> Vec<Target<'a>> {
    let mut out = Vec::new();
    for connection in visible_connections(workspace, scope) {
        let Some(inventory) = connection_inventory(connection) else {
            continue;
        };
        for vm in &inventory.vms {
            if let Some(host_ref) = scope.host_ref()
                && vm.resident_on != host_ref
                && !(vm.host_name == "Unassigned" && inventory.hosts.len() == 1)
            {
                continue;
            }
            out.push(Target { connection, vm });
        }
    }
    out
}

pub fn scoped_templates<'a>(workspace: &'a WorkspaceView, scope: &Scope) -> Vec<Target<'a>> {
    let mut out = Vec::new();
    for connection in visible_connections(workspace, scope) {
        let Some(inventory) = connection_inventory(connection) else {
            continue;
        };
        for vm in &inventory.templates {
            out.push(Target { connection, vm });
        }
    }
    out
}

pub fn scoped_hosts<'a>(
    workspace: &'a WorkspaceView,
    scope: &Scope,
) -> Vec<(&'a ConnectionView, &'a Host)> {
    let mut out = Vec::new();
    for connection in visible_connections(workspace, scope) {
        let Some(inventory) = connection_inventory(connection) else {
            continue;
        };
        for host in &inventory.hosts {
            if let Some(host_ref) = scope.host_ref()
                && host.reference != host_ref
            {
                continue;
            }
            out.push((connection, host));
        }
    }
    out
}

pub fn scoped_tasks<'a>(workspace: &'a WorkspaceView, scope: &Scope) -> Vec<&'a Operation> {
    workspace
        .tasks
        .iter()
        .filter(|task| match scope.connection_id() {
            Some(id) => task.connection_id == id,
            None => true,
        })
        .collect()
}

pub fn find_vm<'a>(
    workspace: &'a WorkspaceView,
    connection_id: &str,
    vm_uuid: &str,
) -> Option<(&'a ConnectionView, &'a VirtualMachine)> {
    let connection = find_connection(workspace, connection_id)?;
    let inventory = connection_inventory(connection)?;
    let vm = inventory
        .vms
        .iter()
        .chain(&inventory.templates)
        .chain(&inventory.snapshots)
        .find(|vm| vm.uuid == vm_uuid)?;
    Some((connection, vm))
}

pub fn find_storage<'a>(
    workspace: &'a WorkspaceView,
    connection_id: &str,
    sr_uuid: &str,
) -> Option<(&'a ConnectionView, &'a xen_core::xapi::Storage)> {
    let connection = find_connection(workspace, connection_id)?;
    let inventory = connection_inventory(connection)?;
    let sr = inventory.storage.iter().find(|sr| sr.uuid == sr_uuid)?;
    Some((connection, sr))
}

pub fn find_network<'a>(
    workspace: &'a WorkspaceView,
    connection_id: &str,
    network_uuid: &str,
) -> Option<(&'a ConnectionView, &'a xen_core::xapi::Network)> {
    let connection = find_connection(workspace, connection_id)?;
    let inventory = connection_inventory(connection)?;
    let network = inventory
        .networks
        .iter()
        .find(|network| network.uuid == network_uuid)?;
    Some((connection, network))
}

pub fn running_task(workspace: &WorkspaceView, connection_id: &str, vm_uuid: &str) -> bool {
    workspace.tasks.iter().any(|task| {
        task.connection_id == connection_id
            && task.vm_uuid == vm_uuid
            && matches!(task.status, TaskStatus::Running | TaskStatus::Unreachable)
    })
}

pub fn action_allowed(
    workspace: &WorkspaceView,
    connection_id: &str,
    vm: &VirtualMachine,
    operation: &str,
) -> bool {
    let Some(connection) = find_connection(workspace, connection_id) else {
        return false;
    };
    is_online(connection)
        && !is_read_only(connection)
        && vm.allowed_operations.iter().any(|value| value == operation)
        && !running_task(workspace, connection_id, &vm.uuid)
}

pub fn task_status_label(status: TaskStatus) -> &'static str {
    match status {
        TaskStatus::Running => "running",
        TaskStatus::Succeeded => "succeeded",
        TaskStatus::Failed => "failed",
        TaskStatus::Cancelled => "cancelled",
        TaskStatus::Unreachable => "unreachable",
    }
}

pub fn section_columns(section: &DataSection) -> Vec<String> {
    match section {
        DataSection::Ready { columns, .. } => columns.clone(),
        _ => Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn console(width: u16, height: u16, pixels: Vec<u8>) -> ConsoleState {
        let mut console = ConsoleState::connecting("c".into(), "conn".into(), "vm".into());
        console.width = width;
        console.height = height;
        console.pixels = pixels;
        console
    }

    #[test]
    fn console_blit_forces_opaque_alpha_and_ignores_out_of_bounds() {
        let mut console = console(2, 1, vec![0; 2 * 4]);
        console.blit(0, 0, 1, 1, &[10, 20, 30, 0]);
        assert_eq!(&console.pixels[0..4], &[10, 20, 30, 255]);
        console.blit(2, 0, 1, 1, &[1, 2, 3, 4]);
        assert_eq!(&console.pixels[4..8], &[0, 0, 0, 0]);
    }

    #[test]
    fn console_copy_rect_supports_overlapping_regions() {
        let mut console = console(3, 1, vec![1, 0, 0, 255, 2, 0, 0, 255, 3, 0, 0, 255]);
        console.copy_rect((0, 0, 2, 1), (1, 0, 2, 1));
        assert_eq!(&console.pixels[0..4], &[2, 0, 0, 255]);
        assert_eq!(&console.pixels[4..8], &[3, 0, 0, 255]);
    }
}
