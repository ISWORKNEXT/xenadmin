use std::collections::HashSet;
use std::time::Instant;

use chrono::{DateTime, Local};
use xen_core::{
    host::{DataSection, HostDetails, InfoGroup},
    operations::{TaskStatus, VmAction},
    performance::HostPerformance,
    vm::{VmDetails, VmIso},
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
        host_ref: String,
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
            | ObjectKey::Health { connection_id } => Some(connection_id),
            ObjectKey::List { connection_id, .. } => connection_id.as_deref(),
        }
    }
}

#[derive(Clone)]
pub struct ObjectTab {
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
    Ha,
    Snapshots,
    Console,
}

impl VmTab {
    pub const ALL: [VmTab; 7] = [
        VmTab::General,
        VmTab::Performance,
        VmTab::Storage,
        VmTab::Network,
        VmTab::Ha,
        VmTab::Snapshots,
        VmTab::Console,
    ];

    pub fn label(self) -> &'static str {
        match self {
            VmTab::General => "General",
            VmTab::Performance => "Performance",
            VmTab::Storage => "Storage",
            VmTab::Network => "Network",
            VmTab::Ha => "HA",
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
            "ha" => Some(VmTab::Ha),
            "snapshots" | "snapshot" => Some(VmTab::Snapshots),
            "console" | "vnc" => Some(VmTab::Console),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum ActionKind {
    Start,
    Shutdown,
    Reboot,
    ForceShutdown,
    ForceReboot,
    Pause,
    Unpause,
    Suspend,
    Resume,
    Clone,
    Snapshot,
    Revert,
    Destroy,
}

impl ActionKind {
    pub const MENU: [ActionKind; 12] = [
        ActionKind::Start,
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
            ActionKind::Shutdown => "clean_shutdown",
            ActionKind::Reboot => "clean_reboot",
            ActionKind::ForceShutdown => "hard_shutdown",
            ActionKind::ForceReboot => "hard_reboot",
            ActionKind::Pause => "pause",
            ActionKind::Unpause => "unpause",
            ActionKind::Suspend => "suspend",
            ActionKind::Resume => "resume",
            ActionKind::Clone => "clone",
            ActionKind::Snapshot => "snapshot",
            ActionKind::Revert => "revert",
            ActionKind::Destroy => "destroy",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            ActionKind::Start => "Start",
            ActionKind::Shutdown => "Shut down",
            ActionKind::Reboot => "Reboot",
            ActionKind::ForceShutdown => "Force shut down",
            ActionKind::ForceReboot => "Force reboot",
            ActionKind::Pause => "Pause",
            ActionKind::Unpause => "Unpause",
            ActionKind::Suspend => "Suspend",
            ActionKind::Resume => "Resume",
            ActionKind::Clone => "Clone",
            ActionKind::Snapshot => "Take snapshot",
            ActionKind::Revert => "Revert snapshot",
            ActionKind::Destroy => "Delete",
        }
    }

    pub fn destructive(self) -> bool {
        matches!(
            self,
            ActionKind::ForceShutdown
                | ActionKind::ForceReboot
                | ActionKind::Revert
                | ActionKind::Destroy
        )
    }

    pub fn needs_name(self) -> bool {
        matches!(self, ActionKind::Clone | ActionKind::Snapshot)
    }

    pub fn to_action(
        self,
        name: &str,
        delete_disks: bool,
        delete_snapshots: bool,
        host_ref: Option<&str>,
    ) -> VmAction {
        match self {
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
            for pixel in row.chunks_exact_mut(4) {
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

pub struct NetworkCreateDialog {
    pub connection_id: String,
    pub name: String,
    pub description: String,
    pub mtu: String,
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

pub struct SrActionConfirm {
    pub connection_id: String,
    pub sr_uuid: String,
    pub sr_name: String,
    pub action: xen_core::operations::SrAction,
    pub acknowledged: bool,
    pub submitting: bool,
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

pub struct VmEditState {
    pub connection_id: String,
    pub vm_uuid: String,
    pub name: String,
    pub description: String,
    pub vcpus_max: String,
    pub vcpus_at_startup: String,
    pub memory_static_max_mib: String,
    pub memory_dynamic_min_mib: String,
    pub memory_dynamic_max_mib: String,
    pub boot_order: String,
    pub is_hvm: bool,
    pub running: bool,
    pub error: Option<String>,
    pub submitting: bool,
}

pub struct UiState {
    pub view: View,
    pub scope: Scope,
    pub object_tabs: Vec<ObjectTab>,
    pub active_tab: usize,
    pub tree_query: String,
    pub tree_expanded: HashSet<String>,
    pub tree_selection: Option<ObjectKey>,
    pub vm_subtab: VmTab,
    pub vm_state: LoadState<VmDetails>,
    pub vm_seq: u64,
    pub vm_perf_state: LoadState<HostPerformance>,
    pub vm_perf_seq: u64,
    pub vm_perf_range: u64,
    pub console: Option<ConsoleState>,
    pub console_seq: u64,
    pub console_focus: bool,
    pub vm_edit: Option<VmEditState>,
    pub iso_dialog: Option<IsoDialog>,
    pub vif_dialog: Option<VifDialog>,
    pub vif_confirm: Option<VifConfirm>,
    pub disk_dialog: Option<DiskDialog>,
    pub host_action_confirm: Option<HostActionConfirm>,
    pub snapshot_selection: HashSet<String>,
    pub snapshot_confirm: Option<SnapshotDeleteConfirm>,
    pub sr_action_confirm: Option<SrActionConfirm>,
    pub network_create_dialog: Option<NetworkCreateDialog>,
    pub network_destroy_confirm: Option<NetworkDestroyConfirm>,
    pub query: String,
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
            view: View::Overview,
            scope: Scope::All,
            object_tabs: vec![ObjectTab {
                key: ObjectKey::Home,
                title: "Home".into(),
            }],
            active_tab: 0,
            tree_query: String::new(),
            tree_expanded: HashSet::new(),
            tree_selection: None,
            vm_subtab: VmTab::General,
            vm_state: LoadState::Idle,
            vm_seq: 0,
            vm_perf_state: LoadState::Idle,
            vm_perf_seq: 0,
            vm_perf_range: 900,
            console: None,
            console_seq: 0,
            console_focus: false,
            vm_edit: None,
            iso_dialog: None,
            vif_dialog: None,
            vif_confirm: None,
            disk_dialog: None,
            host_action_confirm: None,
            snapshot_selection: HashSet::new(),
            snapshot_confirm: None,
            sr_action_confirm: None,
            network_create_dialog: None,
            network_destroy_confirm: None,
            query: String::new(),
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
    SetView(View),
    SetScope(Scope),
    OpenObject {
        key: ObjectKey,
        title: String,
    },
    ActivateTab(usize),
    CloseTab(usize),
    ToggleTreeNode(String),
    Connect {
        settings: ConnectionSettings,
        password: String,
        profile_id: Option<String>,
    },
    Refresh(Option<String>),
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
    OpenVmEdit,
    CloseVmEdit,
    SaveVmEdit,
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

pub fn connected_connections(workspace: &WorkspaceView) -> Vec<&ConnectionView> {
    workspace
        .connections
        .iter()
        .filter(|connection| !matches!(connection, ConnectionView::Offline { .. }))
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
