use std::time::{Duration, Instant};

use chrono::Local;
use eframe::egui;
use xen_core::workspace::WorkspaceView;

use crate::console::ConsoleInput;
use crate::state::{
    ActionKind, ConnectState, ConsoleState, ConsoleStatus, Intent, ListKind, LoadState, ObjectKey,
    ObjectTab, Scope, Toast, ToastKind, UiAction, UiState, View, connection_id, find_connection,
    find_vm, visible_connections,
};
use crate::theme;
use crate::views;
use crate::widgets;
use crate::worker::{Command, Event, Worker};

pub(crate) struct StartupOptions {
    pub startup: Vec<crate::Startup>,
    pub open_connect: bool,
    pub view: Option<View>,
    pub host: Option<String>,
    pub host_tab: Option<crate::state::HostTab>,
    pub vm: Option<String>,
    pub vm_tab: Option<crate::state::VmTab>,
    pub light: Option<bool>,
}

pub struct App {
    pub state: UiState,
    pub workspace: WorkspaceView,
    worker: Worker,
    pending_host: Option<String>,
    pending_host_tab: Option<crate::state::HostTab>,
    pending_vm: Option<String>,
    pending_vm_tab: Option<crate::state::VmTab>,
}

impl App {
    pub(crate) fn new(cc: &eframe::CreationContext<'_>, options: StartupOptions) -> Self {
        let StartupOptions {
            startup,
            open_connect,
            view: startup_view,
            host: startup_host,
            host_tab: startup_host_tab,
            vm: startup_vm,
            vm_tab: startup_vm_tab,
            light: startup_light,
        } = options;
        let preferences = crate::config::load_ui_preferences();
        theme::set_light(startup_light.unwrap_or(preferences.light));
        cc.egui_ctx.set_zoom_factor(preferences.zoom);
        theme::apply(&cc.egui_ctx);
        let worker = Worker::start();
        for entry in startup {
            worker.send(Command::Connect {
                settings: entry.settings,
                password: entry.password,
                profile_id: None,
            });
        }
        let mut state = UiState::default();
        if let Some(view) = startup_view {
            state.view = view;
        }
        if open_connect {
            state.connect = Some(ConnectState::empty());
        }
        if let Some(view) = startup_view {
            let tab = match view {
                View::Overview => None,
                View::Tasks => Some(ObjectTab {
                    key: ObjectKey::Tasks,
                    title: "Activity & tasks".into(),
                }),
                view => {
                    let kind = match view {
                        View::Vms => ListKind::Vms,
                        View::Hosts => ListKind::Hosts,
                        View::Templates => ListKind::Templates,
                        View::Storage => ListKind::Storage,
                        View::Networks => ListKind::Networks,
                        View::Overview | View::Tasks => unreachable!(),
                    };
                    Some(ObjectTab {
                        key: ObjectKey::List {
                            connection_id: None,
                            kind,
                            host_ref: None,
                        },
                        title: kind.title().into(),
                    })
                }
            };
            if let Some(tab) = tab {
                state.object_tabs.push(tab);
                state.active_tab = state.object_tabs.len() - 1;
            }
        }
        Self {
            state,
            workspace: WorkspaceView {
                connections: Vec::new(),
                tasks: Vec::new(),
            },
            worker,
            pending_host: startup_host,
            pending_host_tab: startup_host_tab,
            pending_vm: startup_vm,
            pending_vm_tab: startup_vm_tab,
        }
    }

    fn object_title(&self, key: &ObjectKey) -> String {
        match key {
            ObjectKey::Home => "Home".into(),
            ObjectKey::Pool(id) => self
                .workspace
                .connections
                .iter()
                .find(|connection| connection_id(connection) == id)
                .map_or_else(|| id.clone(), crate::state::connection_name),
            ObjectKey::Host {
                connection_id,
                host_ref,
            } => find_connection(&self.workspace, connection_id)
                .and_then(|connection| {
                    crate::state::connection_inventory(connection).and_then(|inventory| {
                        inventory
                            .hosts
                            .iter()
                            .find(|host| &host.reference == host_ref)
                            .map(|host| host.name.clone())
                    })
                })
                .unwrap_or_else(|| "Host".into()),
            ObjectKey::Vm {
                connection_id,
                vm_uuid,
            }
            | ObjectKey::Template {
                connection_id,
                vm_uuid,
            }
            | ObjectKey::Snapshot {
                connection_id,
                vm_uuid,
            } => find_vm(&self.workspace, connection_id, vm_uuid)
                .map_or_else(|| "Virtual machine".into(), |(_, vm)| vm.name.clone()),
            ObjectKey::Storage {
                connection_id,
                sr_uuid,
            } => crate::state::find_storage(&self.workspace, connection_id, sr_uuid)
                .map_or_else(|| "Storage".into(), |(_, sr)| sr.name.clone()),
            ObjectKey::Network {
                connection_id,
                network_uuid,
            } => crate::state::find_network(&self.workspace, connection_id, network_uuid)
                .map_or_else(|| "Network".into(), |(_, network)| network.name.clone()),
            ObjectKey::List { kind, host_ref, .. } => {
                if let Some(host_ref) = host_ref {
                    let host = self
                        .workspace
                        .connections
                        .iter()
                        .find_map(|connection| {
                            crate::state::connection_inventory(connection).and_then(|inventory| {
                                inventory
                                    .hosts
                                    .iter()
                                    .find(|host| &host.reference == host_ref)
                                    .map(|host| host.name.clone())
                            })
                        })
                        .unwrap_or_else(|| "host".into());
                    format!("{} · {host}", kind.title())
                } else {
                    kind.title().into()
                }
            }
            ObjectKey::Tasks => "Activity & tasks".into(),
        }
    }

    fn open_object(&mut self, key: ObjectKey, title: Option<String>) {
        let title = title.unwrap_or_else(|| self.object_title(&key));
        if let Some(index) = self.state.object_tabs.iter().position(|tab| tab.key == key) {
            self.activate_tab(index);
            return;
        }
        self.state.object_tabs.push(ObjectTab { key, title });
        let index = self.state.object_tabs.len() - 1;
        self.activate_tab(index);
    }

    fn activate_tab(&mut self, index: usize) {
        if self.state.object_tabs.is_empty() {
            return;
        }
        let index = index.min(self.state.object_tabs.len() - 1);
        self.state.active_tab = index;
        let key = self.state.object_tabs[index].key.clone();
        self.state.tree_selection = Some(key.clone());
        if let Some(connection_id) = key.connection_id() {
            self.state
                .tree_expanded
                .insert(format!("pool:{connection_id}"));
            let group = match &key {
                ObjectKey::Host { .. } => Some("Hosts"),
                ObjectKey::Vm { .. } | ObjectKey::Snapshot { .. } => Some("Vms"),
                ObjectKey::Template { .. } => Some("Templates"),
                ObjectKey::Storage { .. } => Some("Storage"),
                ObjectKey::Network { .. } => Some("Networks"),
                ObjectKey::List { kind, .. } => Some(match kind {
                    ListKind::Vms => "Vms",
                    ListKind::Hosts => "Hosts",
                    ListKind::Templates => "Templates",
                    ListKind::Storage => "Storage",
                    ListKind::Networks => "Networks",
                }),
                _ => None,
            };
            if let Some(group) = group {
                self.state
                    .tree_expanded
                    .insert(format!("group:{group}:{connection_id}"));
            }
        }
        self.state.selected.clear();
        self.state.page = 0;
        self.state.detail = None;

        match &key {
            ObjectKey::Home => {
                self.state.scope = Scope::All;
                self.state.view = View::Overview;
            }
            ObjectKey::Pool(id) => {
                self.state.scope = Scope::Connection(id.clone());
                self.state.view = View::Overview;
            }
            ObjectKey::Host {
                connection_id,
                host_ref,
            } => {
                self.state.scope = Scope::Host {
                    connection_id: connection_id.clone(),
                    host_ref: host_ref.clone(),
                };
                let host_name = self.object_title(&key);
                let same = self.state.host_route.as_ref().is_some_and(|route| {
                    &route.connection_id == connection_id && &route.host_uuid == host_ref
                });
                self.state.host_route = Some(crate::state::HostRoute {
                    connection_id: connection_id.clone(),
                    host_uuid: host_ref.clone(),
                    host_name,
                });
                if !same || matches!(self.state.host_state, LoadState::Idle | LoadState::Error(_)) {
                    self.state.host_seq += 1;
                    self.state.host_state = LoadState::Loading;
                    self.worker.send(Command::HostDetails {
                        seq: self.state.host_seq,
                        connection_id: connection_id.clone(),
                        host_uuid: host_ref.clone(),
                    });
                }
            }
            ObjectKey::Vm {
                connection_id,
                vm_uuid,
            }
            | ObjectKey::Template {
                connection_id,
                vm_uuid,
            }
            | ObjectKey::Snapshot {
                connection_id,
                vm_uuid,
            } => {
                self.state.scope = Scope::Connection(connection_id.clone());
                self.state.view = View::Vms;
                let same = matches!(
                    &self.state.vm_state,
                    LoadState::Ready(details) if &details.uuid == vm_uuid
                );
                if !same {
                    self.state.vm_subtab = crate::state::VmTab::General;
                    self.state.vm_seq += 1;
                    self.state.vm_state = LoadState::Loading;
                    self.state.vm_perf_state = LoadState::Idle;
                    self.worker.send(Command::VmDetails {
                        seq: self.state.vm_seq,
                        connection_id: connection_id.clone(),
                        vm_uuid: vm_uuid.clone(),
                    });
                }
            }
            ObjectKey::Storage { connection_id, .. } => {
                self.state.scope = Scope::Connection(connection_id.clone());
                self.state.view = View::Storage;
            }
            ObjectKey::Network { connection_id, .. } => {
                self.state.scope = Scope::Connection(connection_id.clone());
                self.state.view = View::Networks;
            }
            ObjectKey::List {
                connection_id,
                kind,
                host_ref,
            } => {
                self.state.scope = if let Some(host_ref) = host_ref {
                    Scope::Host {
                        connection_id: connection_id.clone().unwrap_or_default(),
                        host_ref: host_ref.clone(),
                    }
                } else if let Some(connection_id) = connection_id {
                    Scope::Connection(connection_id.clone())
                } else {
                    Scope::All
                };
                self.state.view = kind.view();
            }
            ObjectKey::Tasks => {
                self.state.view = View::Tasks;
            }
        }
    }

    /// Close object tabs whose object disappeared after a refresh or a delete.
    /// Offline connections are skipped so stale tabs survive a temporary outage.
    fn prune_missing_tabs(&mut self) {
        let mut missing = Vec::new();
        for (index, tab) in self.state.object_tabs.iter().enumerate().skip(1) {
            let gone = match &tab.key {
                ObjectKey::Vm {
                    connection_id,
                    vm_uuid,
                } => find_connection(&self.workspace, connection_id)
                    .and_then(crate::state::connection_inventory)
                    .is_some_and(|inventory| !inventory.vms.iter().any(|vm| vm.uuid == *vm_uuid)),
                ObjectKey::Template {
                    connection_id,
                    vm_uuid,
                } => find_connection(&self.workspace, connection_id)
                    .and_then(crate::state::connection_inventory)
                    .is_some_and(|inventory| {
                        !inventory.templates.iter().any(|vm| vm.uuid == *vm_uuid)
                    }),
                ObjectKey::Snapshot {
                    connection_id,
                    vm_uuid,
                } => find_connection(&self.workspace, connection_id)
                    .and_then(crate::state::connection_inventory)
                    .is_some_and(|inventory| {
                        !inventory.snapshots.iter().any(|vm| vm.uuid == *vm_uuid)
                    }),
                _ => false,
            };
            if gone {
                missing.push(index);
            }
        }
        for index in missing.into_iter().rev() {
            self.close_tab(index);
        }
        let stale_console = self.state.console.as_ref().and_then(|console| {
            let inventory = find_connection(&self.workspace, &console.connection_id)
                .and_then(crate::state::connection_inventory)?;
            (!inventory
                .vms
                .iter()
                .chain(&inventory.templates)
                .chain(&inventory.snapshots)
                .any(|vm| vm.uuid == console.vm_uuid))
            .then(|| console.console_id.clone())
        });
        if stale_console.is_some() {
            self.close_console();
        }
    }

    fn close_tab(&mut self, index: usize) {
        if index == 0 || index >= self.state.object_tabs.len() {
            return;
        }
        let was_active = index == self.state.active_tab;
        self.state.object_tabs.remove(index);
        if self.state.active_tab > index {
            self.state.active_tab -= 1;
        }
        if was_active {
            self.state.active_tab = index.saturating_sub(1);
            self.activate_tab(self.state.active_tab);
        }
    }

    fn open_list_for_view(&mut self, view: View) {
        match view {
            View::Overview => {
                let key = match self.state.scope.clone() {
                    Scope::Connection(id) => ObjectKey::Pool(id),
                    _ => ObjectKey::Home,
                };
                self.open_object(key, None);
            }
            View::Tasks => self.open_object(ObjectKey::Tasks, None),
            view => {
                let kind = match view {
                    View::Vms => ListKind::Vms,
                    View::Hosts => ListKind::Hosts,
                    View::Templates => ListKind::Templates,
                    View::Storage => ListKind::Storage,
                    View::Networks => ListKind::Networks,
                    View::Overview => unreachable!(),
                    View::Tasks => unreachable!(),
                };
                let connection_id = self.state.scope.connection_id().map(str::to_owned);
                let host_ref = self
                    .state
                    .scope
                    .host_ref()
                    .map(str::to_owned)
                    .filter(|_| matches!(kind, ListKind::Vms));
                let key = ObjectKey::List {
                    connection_id,
                    kind,
                    host_ref,
                };
                self.open_object(key, None);
            }
        }
    }

    fn drain_events(&mut self) {
        while let Ok(event) = self.worker.events.try_recv() {
            match event {
                Event::Workspace(workspace) => {
                    self.workspace = workspace;
                    self.state.loaded = true;
                    self.prune_missing_tabs();
                    if let Some(id) = self.state.pending_scope.clone()
                        && find_connection(&self.workspace, &id).is_some_and(|connection| {
                            !matches!(
                                connection,
                                xen_core::workspace::ConnectionView::Offline { .. }
                            )
                        })
                    {
                        let should_open_pool = self.pending_host.is_none()
                            && self.pending_vm.is_none()
                            && self.state.active_tab == 0;
                        self.state.scope = Scope::Connection(id.clone());
                        self.state.pending_scope = None;
                        self.state.tree_expanded.insert(format!("pool:{id}"));
                        if should_open_pool {
                            self.open_object(ObjectKey::Pool(id), None);
                        }
                    }
                    if let Some(uuid) = self.pending_host.clone() {
                        let mut found = None;
                        for connection in &self.workspace.connections {
                            if let Some(inventory) = crate::state::connection_inventory(connection)
                                && let Some(host) =
                                    inventory.hosts.iter().find(|host| host.uuid == uuid)
                            {
                                found =
                                    Some((connection_id(connection).to_owned(), host.name.clone()));
                                break;
                            }
                        }
                        if let Some((connection_id, host_name)) = found {
                            self.pending_host = None;
                            self.open_object(
                                ObjectKey::Host {
                                    connection_id,
                                    host_ref: uuid,
                                },
                                Some(host_name),
                            );
                            if let Some(tab) = self.pending_host_tab.take() {
                                self.apply(vec![UiAction::SetHostTab(tab)]);
                            }
                        }
                    }
                    if let Some(uuid) = self.pending_vm.clone() {
                        let mut found = None;
                        for connection in &self.workspace.connections {
                            if let Some(inventory) = crate::state::connection_inventory(connection)
                            {
                                let id = connection_id(connection).to_owned();
                                if inventory.vms.iter().any(|vm| vm.uuid == uuid) {
                                    found = Some(ObjectKey::Vm {
                                        connection_id: id,
                                        vm_uuid: uuid.clone(),
                                    });
                                    break;
                                }
                                if inventory.templates.iter().any(|vm| vm.uuid == uuid) {
                                    found = Some(ObjectKey::Template {
                                        connection_id: id,
                                        vm_uuid: uuid.clone(),
                                    });
                                    break;
                                }
                                if inventory.snapshots.iter().any(|vm| vm.uuid == uuid) {
                                    found = Some(ObjectKey::Snapshot {
                                        connection_id: id,
                                        vm_uuid: uuid.clone(),
                                    });
                                    break;
                                }
                            }
                        }
                        if let Some(key) = found {
                            self.pending_vm = None;
                            self.open_object(key, None);
                            if let Some(tab) = self.pending_vm_tab.take() {
                                self.apply(vec![UiAction::SetVmTab(tab)]);
                            }
                        }
                    }
                }
                Event::Connected(id) => {
                    self.state.connect = None;
                    self.state.pending_scope = Some(id);
                    self.state.updated = Some(Local::now());
                    self.state.toasts.push(Toast::new(
                        "Connected. Loading pool inventory.",
                        ToastKind::Success,
                    ));
                }
                Event::ActionQueued => {
                    self.state.updated = Some(Local::now());
                }
                Event::RefreshFinished {
                    connection_id,
                    result,
                } => {
                    self.state.busy.remove(&connection_id);
                    if let Err(message) = result {
                        self.state
                            .toasts
                            .push(Toast::new(message, ToastKind::Error));
                    }
                    self.state.updated = Some(Local::now());
                }
                Event::TaskFinished { task_id, result } => {
                    if let Err(message) = result {
                        if !task_id.is_empty() {
                            self.state.busy.remove(&task_id);
                        }
                        self.state
                            .toasts
                            .push(Toast::new(message, ToastKind::Error));
                    }
                }
                Event::ConnectionClosed {
                    connection_id,
                    result,
                } => {
                    self.state.busy.remove(&connection_id);
                    if self
                        .state
                        .console
                        .as_ref()
                        .is_some_and(|console| console.connection_id == connection_id)
                    {
                        self.close_console();
                    }
                    match result {
                        Ok(()) => self.state.toasts.push(Toast::new(
                            "Connection closed and session logged out.",
                            ToastKind::Info,
                        )),
                        Err(message) => self
                            .state
                            .toasts
                            .push(Toast::new(message, ToastKind::Error)),
                    }
                }
                Event::ConnectionRemoved {
                    connection_id,
                    result,
                } => {
                    if self
                        .state
                        .console
                        .as_ref()
                        .is_some_and(|console| console.connection_id == connection_id)
                    {
                        self.close_console();
                    }
                    if let Err(message) = result {
                        self.state
                            .toasts
                            .push(Toast::new(message, ToastKind::Error));
                    } else {
                        self.state
                            .toasts
                            .push(Toast::new("Saved connection removed.", ToastKind::Info));
                        if self.state.scope == Scope::Connection(connection_id.clone()) {
                            self.state.scope = Scope::All;
                            self.state.host_route = None;
                        }
                    }
                }
                Event::HostDetails { seq, result } => {
                    if seq == self.state.host_seq {
                        self.state.host_state = match result {
                            Ok(details) => LoadState::Ready(*details),
                            Err(message) => LoadState::Error(message),
                        };
                    }
                }
                Event::HostPerformance { seq, result } => {
                    if seq == self.state.perf_seq {
                        self.state.perf_state = match result {
                            Ok(performance) => LoadState::Ready(*performance),
                            Err(message) => LoadState::Error(message),
                        };
                    }
                }
                Event::VmDetails { seq, result } => {
                    if seq == self.state.vm_seq {
                        self.state.vm_state = match result {
                            Ok(details) => LoadState::Ready(*details),
                            Err(message) => LoadState::Error(message),
                        };
                    }
                }
                Event::VmPerformance { seq, result } => {
                    if seq == self.state.vm_perf_seq {
                        self.state.vm_perf_state = match result {
                            Ok(performance) => LoadState::Ready(*performance),
                            Err(message) => LoadState::Error(message),
                        };
                    }
                }
                Event::VmEdited { vm_uuid, result } => {
                    if !self
                        .state
                        .vm_edit
                        .as_ref()
                        .is_some_and(|edit| edit.vm_uuid == vm_uuid)
                    {
                        continue;
                    }
                    match result {
                        Ok(()) => {
                            let Some(edit) = self.state.vm_edit.take() else {
                                continue;
                            };
                            if let Some(tab) = self.state.object_tabs.get_mut(self.state.active_tab)
                                && matches!(
                                    &tab.key,
                                    ObjectKey::Vm { vm_uuid: active, .. } if active == &vm_uuid
                                )
                            {
                                tab.title = edit.name.clone();
                            }
                            self.state.toasts.push(Toast::new(
                                format!("Updated {}.", edit.name),
                                ToastKind::Success,
                            ));
                            self.apply(vec![UiAction::ReloadVmDetails]);
                        }
                        Err(message) => {
                            if let Some(edit) = self.state.vm_edit.as_mut() {
                                edit.submitting = false;
                                edit.error = Some(message);
                            }
                        }
                    }
                }
                Event::CdChanged { vm_uuid, result } => {
                    if !self
                        .state
                        .iso_dialog
                        .as_ref()
                        .is_some_and(|dialog| dialog.vm_uuid == vm_uuid)
                    {
                        continue;
                    }
                    match result {
                        Ok(()) => {
                            self.state.iso_dialog = None;
                            self.state
                                .toasts
                                .push(Toast::new("CD/DVD media updated.", ToastKind::Success));
                            self.apply(vec![UiAction::ReloadVmDetails]);
                        }
                        Err(message) => {
                            if let Some(dialog) = self.state.iso_dialog.as_mut() {
                                dialog.submitting = false;
                                dialog.error = Some(message);
                            }
                        }
                    }
                }
                Event::VifChanged {
                    vm_uuid,
                    action,
                    result,
                } => {
                    let message = match action.as_str() {
                        "add" => "Network interface added.",
                        "remove" => "Network interface removed.",
                        "plug" => "Network interface plugged.",
                        "unplug" => "Network interface unplugged.",
                        _ => "Network interface updated.",
                    };
                    match result {
                        Ok(()) => {
                            if self
                                .state
                                .vif_dialog
                                .as_ref()
                                .is_some_and(|dialog| dialog.vm_uuid == vm_uuid)
                            {
                                self.state.vif_dialog = None;
                            }
                            if self
                                .state
                                .vif_confirm
                                .as_ref()
                                .is_some_and(|confirm| confirm.vm_uuid == vm_uuid)
                            {
                                self.state.vif_confirm = None;
                            }
                            self.state
                                .toasts
                                .push(Toast::new(message, ToastKind::Success));
                            self.apply(vec![UiAction::ReloadVmDetails]);
                        }
                        Err(message) => {
                            if let Some(dialog) = self.state.vif_dialog.as_mut()
                                && dialog.vm_uuid == vm_uuid
                            {
                                dialog.submitting = false;
                                dialog.error = Some(message);
                            } else {
                                self.state
                                    .toasts
                                    .push(Toast::new(message, ToastKind::Error));
                            }
                        }
                    }
                }
                Event::DiskChanged {
                    vm_uuid,
                    action,
                    result,
                } => {
                    let message = match action.as_str() {
                        "add" => "Virtual disk added.",
                        "add_cd" => "CD/DVD drive added.",
                        "resize" => "Virtual disk resized.",
                        "remove" => "Virtual disk removed.",
                        _ => "Virtual disk updated.",
                    };
                    match result {
                        Ok(()) => {
                            if self
                                .state
                                .disk_dialog
                                .as_ref()
                                .is_some_and(|dialog| disk_dialog_vm(dialog) == vm_uuid)
                            {
                                self.state.disk_dialog = None;
                            }
                            self.state
                                .toasts
                                .push(Toast::new(message, ToastKind::Success));
                            self.apply(vec![UiAction::ReloadVmDetails]);
                        }
                        Err(message) => {
                            if let Some(dialog) = self.state.disk_dialog.as_mut()
                                && disk_dialog_vm(dialog) == vm_uuid
                            {
                                disk_dialog_error(dialog, message);
                            } else {
                                self.state
                                    .toasts
                                    .push(Toast::new(message, ToastKind::Error));
                            }
                        }
                    }
                }
                Event::ConsoleOpened {
                    console_id,
                    vm_uuid,
                    console_uuid,
                } => {
                    if let Some(console) = self.state.console.as_mut()
                        && console.console_id == console_id
                        && console.vm_uuid == vm_uuid
                    {
                        console.status = ConsoleStatus::Connected;
                        console.console_uuid = console_uuid;
                        console.message.clear();
                    }
                }
                Event::ConsoleSize {
                    console_id,
                    width,
                    height,
                } => {
                    if let Some(console) = self.state.console.as_mut()
                        && console.console_id == console_id
                    {
                        console.width = width;
                        console.height = height;
                        console.pixels = vec![0; width as usize * height as usize * 4];
                        console.dirty.clear();
                        console.dirty_full = false;
                        console.texture = None;
                        console.version += 1;
                    }
                }
                Event::ConsoleFrame {
                    console_id,
                    x,
                    y,
                    width,
                    height,
                    pixels,
                } => {
                    if let Some(console) = self.state.console.as_mut()
                        && console.console_id == console_id
                    {
                        console.blit(x, y, width, height, &pixels);
                    }
                }
                Event::ConsoleCopy {
                    console_id,
                    dst,
                    src,
                } => {
                    if let Some(console) = self.state.console.as_mut()
                        && console.console_id == console_id
                    {
                        console.copy_rect(dst, src);
                    }
                }
                Event::ConsoleClipboard { console_id, text } => {
                    if let Some(console) = self.state.console.as_mut()
                        && console.console_id == console_id
                    {
                        console.clipboard = text;
                    }
                }
                Event::ConsoleClosed {
                    console_id,
                    message,
                } => {
                    if let Some(console) = self.state.console.as_mut()
                        && console.console_id == console_id
                    {
                        console.status = ConsoleStatus::Closed;
                        console.message =
                            message.unwrap_or_else(|| "Console connection closed.".into());
                    }
                }
                Event::Failed { context, message } => {
                    if context == "connect"
                        && let Some(connect) = self.state.connect.as_mut()
                    {
                        connect.error = Some(message.clone());
                        connect.submitting = false;
                    }
                    self.state
                        .toasts
                        .push(Toast::new(message, ToastKind::Error));
                }
            }
        }
    }

    fn shortcuts(&mut self, ctx: &egui::Context) {
        if self.state.console_focus {
            return;
        }
        let mut focus_search = false;
        let mut refresh = false;
        let mut close_tab = false;
        let mut zoom_action = None;
        ctx.input_mut(|input| {
            if input.consume_key(egui::Modifiers::COMMAND, egui::Key::K) {
                focus_search = true;
            }
            if input.consume_key(egui::Modifiers::NONE, egui::Key::F5) {
                refresh = true;
            }
            if input.consume_key(egui::Modifiers::COMMAND, egui::Key::W) {
                close_tab = true;
            }
            if input.consume_key(egui::Modifiers::COMMAND, egui::Key::Plus)
                || input.consume_key(egui::Modifiers::COMMAND, egui::Key::Equals)
            {
                zoom_action = Some(1);
            }
            if input.consume_key(egui::Modifiers::COMMAND, egui::Key::Minus) {
                zoom_action = Some(-1);
            }
            if input.consume_key(egui::Modifiers::COMMAND, egui::Key::Num0) {
                zoom_action = Some(0);
            }
        });
        if focus_search {
            self.state.focus_search = true;
        }
        if close_tab {
            self.close_tab(self.state.active_tab);
        }
        if refresh {
            let connection = self.state.scope.connection_id().map(str::to_owned);
            self.apply(vec![UiAction::Refresh(connection)]);
        }
        if let Some(direction) = zoom_action {
            let next = if direction == 0 {
                crate::config::default_zoom()
            } else {
                (ctx.zoom_factor() + direction as f32 * 0.05).clamp(0.8, 1.6)
            };
            ctx.set_zoom_factor(next);
            crate::config::save_ui_preferences(&crate::config::UiPreferences {
                light: theme::is_light(),
                zoom: next,
            });
            self.state.toasts.push(Toast::new(
                format!("UI scale {:.0}%", next * 100.0),
                ToastKind::Info,
            ));
        }
    }

    fn apply(&mut self, actions: Vec<UiAction>) {
        for action in actions {
            match action {
                UiAction::SetView(view) => {
                    self.open_list_for_view(view);
                }
                UiAction::SetScope(scope) => {
                    self.state.scope = scope;
                    self.state.selected.clear();
                    self.state.page = 0;
                    self.state.detail = None;
                    self.state.host_route = None;
                }
                UiAction::ToggleTreeNode(id) => {
                    if !self.state.tree_expanded.remove(&id) {
                        self.state.tree_expanded.insert(id);
                    }
                }
                UiAction::OpenObject { key, title } => {
                    self.open_object(key, Some(title));
                }
                UiAction::ActivateTab(index) => {
                    self.activate_tab(index);
                }
                UiAction::CloseTab(index) => {
                    self.close_tab(index);
                }
                UiAction::Connect {
                    settings,
                    password,
                    profile_id,
                } => {
                    if let Some(connect) = self.state.connect.as_mut() {
                        connect.submitting = true;
                        connect.error = None;
                    }
                    self.worker.send(Command::Connect {
                        settings,
                        password,
                        profile_id,
                    });
                }
                UiAction::Refresh(Some(id)) => {
                    self.state.busy.insert(id.clone());
                    self.worker.send(Command::Refresh { connection_id: id });
                }
                UiAction::Refresh(None) => {
                    let ids: Vec<String> = visible_connections(&self.workspace, &self.state.scope)
                        .into_iter()
                        .filter(|connection| {
                            !matches!(
                                connection,
                                xen_core::workspace::ConnectionView::Offline { .. }
                            )
                        })
                        .map(|connection| connection_id(connection).to_owned())
                        .collect();
                    for id in ids {
                        self.state.busy.insert(id.clone());
                        self.worker.send(Command::Refresh { connection_id: id });
                    }
                }
                UiAction::Disconnect(id) => {
                    self.state.busy.insert(id.clone());
                    self.worker.send(Command::Disconnect { connection_id: id });
                }
                UiAction::Remove(id) => {
                    self.worker.send(Command::Remove { connection_id: id });
                }
                UiAction::OpenVm(connection_id, vm_uuid) => {
                    let key = vm_object_key(&self.workspace, &connection_id, &vm_uuid);
                    self.open_object(key, None);
                }
                UiAction::Propose(kind, targets) => {
                    let first_name = targets
                        .first()
                        .and_then(|(connection_id, vm_uuid)| {
                            find_vm(&self.workspace, connection_id, vm_uuid)
                        })
                        .map(|(_, vm)| vm.name.clone())
                        .unwrap_or_else(|| "VM".into());
                    let name = match kind {
                        ActionKind::Clone => format!("{first_name} copy"),
                        ActionKind::Snapshot => {
                            format!("Snapshot {}", Local::now().format("%Y-%m-%d"))
                        }
                        _ => String::new(),
                    };
                    self.state.intent = Some(Intent {
                        kind,
                        targets,
                        name,
                        delete_disks: true,
                        delete_snapshots: false,
                        host: None,
                    });
                }
                UiAction::ProposeOnHost(kind, targets, host_ref, host_name) => {
                    let first_name = targets
                        .first()
                        .and_then(|(connection_id, vm_uuid)| {
                            find_vm(&self.workspace, connection_id, vm_uuid)
                        })
                        .map(|(_, vm)| vm.name.clone())
                        .unwrap_or_else(|| "VM".into());
                    self.state.intent = Some(Intent {
                        kind,
                        targets,
                        name: first_name,
                        delete_disks: true,
                        delete_snapshots: false,
                        host: Some((host_ref, host_name)),
                    });
                }
                UiAction::SubmitIntent => {
                    let Some(intent) = self.state.intent.take() else {
                        continue;
                    };
                    let action = intent.kind.to_action(
                        &intent.name,
                        intent.delete_disks,
                        intent.delete_snapshots,
                        intent
                            .host
                            .as_ref()
                            .map(|(reference, _)| reference.as_str()),
                    );
                    let count = intent.targets.len();
                    for (connection_id, vm_uuid) in intent.targets {
                        self.worker.send(Command::Action {
                            connection_id,
                            vm_uuid,
                            action: action.clone(),
                        });
                    }
                    self.state.selected.clear();
                    self.open_object(ObjectKey::Tasks, None);
                    self.state.toasts.push(Toast::new(
                        format!(
                            "{count} task{} submitted. Track progress in Activity & tasks.",
                            if count == 1 { "" } else { "s" }
                        ),
                        ToastKind::Success,
                    ));
                }
                UiAction::CloseIntent => self.state.intent = None,
                UiAction::CheckTask(id) => {
                    self.worker.send(Command::CheckTask { task_id: id });
                }
                UiAction::CancelTask(id) => {
                    self.worker.send(Command::CancelTask { task_id: id });
                }
                UiAction::OpenHostDetails {
                    connection_id,
                    host_uuid,
                    host_name,
                } => {
                    self.open_object(
                        ObjectKey::Host {
                            connection_id,
                            host_ref: host_uuid,
                        },
                        Some(host_name),
                    );
                }
                UiAction::CloseHostDetails => {
                    self.close_tab(self.state.active_tab);
                    self.state.host_route = None;
                    self.state.host_state = LoadState::Idle;
                    self.state.perf_state = LoadState::Idle;
                }
                UiAction::ReloadHostDetails => {
                    if let Some(route) = self.state.host_route.clone() {
                        self.state.host_seq += 1;
                        self.state.host_state = LoadState::Loading;
                        self.worker.send(Command::HostDetails {
                            seq: self.state.host_seq,
                            connection_id: route.connection_id,
                            host_uuid: route.host_uuid,
                        });
                    }
                }
                UiAction::LoadPerformance => {
                    if let Some(route) = self.state.host_route.clone() {
                        self.state.perf_seq += 1;
                        self.state.perf_state = LoadState::Loading;
                        self.worker.send(Command::HostPerformance {
                            seq: self.state.perf_seq,
                            connection_id: route.connection_id,
                            host_uuid: route.host_uuid,
                            range_seconds: self.state.perf_range,
                        });
                    }
                }
                UiAction::OpenResource { title, rows } => {
                    self.state.resource = Some(crate::state::ResourceDialog { title, rows });
                }
                UiAction::CloseResource => self.state.resource = None,
                UiAction::SetHostTab(tab) => {
                    if tab != self.state.host_tab {
                        self.state.record_query.clear();
                        self.state.record_page = 0;
                    }
                    self.state.host_tab = tab;
                    if tab == crate::state::HostTab::Performance
                        && matches!(self.state.perf_state, LoadState::Idle)
                    {
                        self.apply(vec![UiAction::LoadPerformance]);
                    }
                }
                UiAction::SetVmTab(tab) => {
                    self.state.vm_subtab = tab;
                    if tab == crate::state::VmTab::Performance
                        && matches!(
                            self.state.vm_perf_state,
                            LoadState::Idle | LoadState::Error(_)
                        )
                    {
                        self.apply(vec![UiAction::LoadVmPerformance]);
                    }
                    if tab == crate::state::VmTab::Console
                        && self.state.console.is_none()
                        && let Some(key) = self
                            .state
                            .object_tabs
                            .get(self.state.active_tab)
                            .map(|tab| tab.key.clone())
                        && let Some((connection_id, vm_uuid)) = vm_key(&key)
                        && find_vm(&self.workspace, &connection_id, &vm_uuid)
                            .is_some_and(|(_, vm)| vm.kind == "vm" && vm.power_state == "Running")
                    {
                        self.apply(vec![UiAction::OpenConsole(connection_id, vm_uuid)]);
                    }
                }
                UiAction::ReloadVmDetails => {
                    if let Some(key) = self
                        .state
                        .object_tabs
                        .get(self.state.active_tab)
                        .map(|tab| tab.key.clone())
                        && let Some((connection_id, vm_uuid)) = vm_key(&key)
                    {
                        self.state.vm_seq += 1;
                        self.state.vm_state = LoadState::Loading;
                        self.worker.send(Command::VmDetails {
                            seq: self.state.vm_seq,
                            connection_id,
                            vm_uuid,
                        });
                    }
                }
                UiAction::LoadVmPerformance => {
                    if let Some(key) = self
                        .state
                        .object_tabs
                        .get(self.state.active_tab)
                        .map(|tab| tab.key.clone())
                        && let Some((connection_id, vm_uuid)) = vm_key(&key)
                    {
                        self.state.vm_perf_seq += 1;
                        self.state.vm_perf_state = LoadState::Loading;
                        self.worker.send(Command::VmPerformance {
                            seq: self.state.vm_perf_seq,
                            connection_id,
                            vm_uuid,
                            range_seconds: self.state.vm_perf_range,
                        });
                    }
                }
                UiAction::SetVmPerfRange(range) => {
                    self.state.vm_perf_range = range;
                    self.apply(vec![UiAction::LoadVmPerformance]);
                }
                UiAction::OpenVmEdit => {
                    let Some(key) = self
                        .state
                        .object_tabs
                        .get(self.state.active_tab)
                        .map(|tab| tab.key.clone())
                    else {
                        continue;
                    };
                    let Some((connection_id, vm_uuid)) = vm_key(&key) else {
                        continue;
                    };
                    let LoadState::Ready(details) = &self.state.vm_state else {
                        self.state.toasts.push(Toast::new(
                            "Wait for the virtual machine details to load.",
                            ToastKind::Info,
                        ));
                        continue;
                    };
                    if details.is_template || details.is_snapshot {
                        continue;
                    }
                    self.state.vm_edit = Some(crate::state::VmEditState {
                        connection_id,
                        vm_uuid,
                        name: details.name.clone(),
                        description: details.description.clone(),
                        vcpus_max: details.vcpus_max.to_string(),
                        vcpus_at_startup: details.vcpus_at_startup.to_string(),
                        memory_static_max_mib: (details.memory_static_max / (1024 * 1024))
                            .to_string(),
                        memory_dynamic_min_mib: (details.memory_dynamic_min / (1024 * 1024))
                            .to_string(),
                        memory_dynamic_max_mib: (details.memory_dynamic_max / (1024 * 1024))
                            .to_string(),
                        boot_order: details
                            .hvm_boot_params
                            .get("order")
                            .cloned()
                            .unwrap_or_else(|| "cd".into()),
                        is_hvm: !details.hvm_boot_policy.is_empty()
                            || !details.hvm_boot_params.is_empty(),
                        running: details.power_state != "Halted",
                        error: None,
                        submitting: false,
                    });
                }
                UiAction::CloseVmEdit => {
                    if !self
                        .state
                        .vm_edit
                        .as_ref()
                        .is_some_and(|edit| edit.submitting)
                    {
                        self.state.vm_edit = None;
                    }
                }
                UiAction::SaveVmEdit => {
                    let Some(edit) = self.state.vm_edit.as_mut() else {
                        continue;
                    };
                    let parse = |value: &str| value.trim().parse::<u64>().ok();
                    let Some(vcpus_max) = parse(&edit.vcpus_max) else {
                        edit.error = Some("Enter a whole number for the vCPU maximum.".into());
                        continue;
                    };
                    let Some(vcpus_at_startup) = parse(&edit.vcpus_at_startup) else {
                        edit.error = Some("Enter a whole number for vCPUs at startup.".into());
                        continue;
                    };
                    let Some(memory_static_max_mib) = parse(&edit.memory_static_max_mib) else {
                        edit.error = Some("Enter a whole number for static memory in MiB.".into());
                        continue;
                    };
                    let Some(memory_dynamic_min_mib) = parse(&edit.memory_dynamic_min_mib) else {
                        edit.error =
                            Some("Enter a whole number for the dynamic memory minimum.".into());
                        continue;
                    };
                    let Some(memory_dynamic_max_mib) = parse(&edit.memory_dynamic_max_mib) else {
                        edit.error =
                            Some("Enter a whole number for the dynamic memory maximum.".into());
                        continue;
                    };
                    if edit.name.trim().is_empty() {
                        edit.error = Some("Enter a name for the virtual machine.".into());
                        continue;
                    }
                    if vcpus_max == 0 || vcpus_at_startup == 0 || vcpus_at_startup > vcpus_max {
                        edit.error =
                            Some("vCPUs at startup must be between 1 and the vCPU maximum.".into());
                        continue;
                    }
                    if memory_static_max_mib == 0
                        || memory_dynamic_min_mib == 0
                        || memory_dynamic_max_mib == 0
                    {
                        edit.error = Some("Memory must be greater than zero.".into());
                        continue;
                    }
                    if memory_dynamic_min_mib > memory_dynamic_max_mib
                        || memory_dynamic_max_mib > memory_static_max_mib
                    {
                        edit.error = Some(
                            "Dynamic memory must stay between the static minimum and maximum."
                                .into(),
                        );
                        continue;
                    }
                    edit.submitting = true;
                    edit.error = None;
                    self.worker.send(Command::EditVm {
                        connection_id: edit.connection_id.clone(),
                        vm_uuid: edit.vm_uuid.clone(),
                        name: edit.name.trim().to_owned(),
                        description: edit.description.clone(),
                        vcpus_max,
                        vcpus_at_startup,
                        memory_static_max_mib,
                        memory_dynamic_min_mib,
                        memory_dynamic_max_mib,
                        boot_order: edit.is_hvm.then(|| edit.boot_order.clone()),
                    });
                }
                UiAction::OpenIsoDialog { vbd_uuid, drive } => {
                    let Some(key) = self
                        .state
                        .object_tabs
                        .get(self.state.active_tab)
                        .map(|tab| tab.key.clone())
                    else {
                        continue;
                    };
                    let Some((connection_id, vm_uuid)) = vm_key(&key) else {
                        continue;
                    };
                    let LoadState::Ready(details) = &self.state.vm_state else {
                        self.state.toasts.push(Toast::new(
                            "Wait for the virtual machine details to load.",
                            ToastKind::Info,
                        ));
                        continue;
                    };
                    let Some(disk) = details
                        .disks
                        .iter()
                        .find(|disk| disk.is_cd && disk.vbd_uuid == vbd_uuid)
                    else {
                        self.state.toasts.push(Toast::new(
                            "This CD drive is no longer present.",
                            ToastKind::Error,
                        ));
                        continue;
                    };
                    let selected =
                        (!disk.empty && !disk.vdi_uuid.is_empty()).then(|| disk.vdi_uuid.clone());
                    self.state.iso_dialog = Some(crate::state::IsoDialog {
                        connection_id,
                        vm_uuid,
                        vbd_uuid,
                        drive,
                        isos: details.isos.clone(),
                        selected,
                        search: String::new(),
                        submitting: false,
                        error: None,
                    });
                }
                UiAction::CloseIsoDialog => {
                    if !self
                        .state
                        .iso_dialog
                        .as_ref()
                        .is_some_and(|dialog| dialog.submitting)
                    {
                        self.state.iso_dialog = None;
                    }
                }
                UiAction::SaveIso => {
                    let Some(dialog) = self.state.iso_dialog.as_mut() else {
                        continue;
                    };
                    dialog.submitting = true;
                    dialog.error = None;
                    self.worker.send(Command::ChangeCd {
                        connection_id: dialog.connection_id.clone(),
                        vm_uuid: dialog.vm_uuid.clone(),
                        vbd_uuid: dialog.vbd_uuid.clone(),
                        iso_uuid: dialog.selected.clone(),
                    });
                }
                UiAction::AddCdDrive => {
                    let Some(key) = self
                        .state
                        .object_tabs
                        .get(self.state.active_tab)
                        .map(|tab| tab.key.clone())
                    else {
                        continue;
                    };
                    let Some((connection_id, vm_uuid)) = vm_key(&key) else {
                        continue;
                    };
                    self.worker.send(Command::AddCdDrive {
                        connection_id,
                        vm_uuid,
                    });
                }
                UiAction::OpenAddDisk => {
                    let Some(key) = self
                        .state
                        .object_tabs
                        .get(self.state.active_tab)
                        .map(|tab| tab.key.clone())
                    else {
                        continue;
                    };
                    let Some((connection_id, vm_uuid)) = vm_key(&key) else {
                        continue;
                    };
                    let srs: Vec<(String, String, u64)> =
                        find_connection(&self.workspace, &connection_id)
                            .and_then(crate::state::connection_inventory)
                            .map(|inventory| {
                                inventory
                                    .storage
                                    .iter()
                                    .filter(|sr| sr.content_type != "iso" && sr.total > sr.used)
                                    .map(|sr| {
                                        (
                                            sr.uuid.clone(),
                                            sr.name.clone(),
                                            sr.total.saturating_sub(sr.used),
                                        )
                                    })
                                    .collect()
                            })
                            .unwrap_or_default();
                    if srs.is_empty() {
                        self.state.toasts.push(Toast::new(
                            "No writable storage repositories are available.",
                            ToastKind::Error,
                        ));
                        continue;
                    }
                    self.state.disk_dialog =
                        Some(crate::state::DiskDialog::Add(crate::state::DiskAddDialog {
                            connection_id,
                            vm_uuid,
                            sr_uuid: srs.first().map(|(uuid, _, _)| uuid.clone()),
                            srs,
                            name: String::new(),
                            size_gib: "10".into(),
                            submitting: false,
                            error: None,
                        }));
                }
                UiAction::OpenDiskResize {
                    vdi_uuid,
                    name,
                    current_gib,
                } => {
                    let Some(key) = self
                        .state
                        .object_tabs
                        .get(self.state.active_tab)
                        .map(|tab| tab.key.clone())
                    else {
                        continue;
                    };
                    let Some((connection_id, vm_uuid)) = vm_key(&key) else {
                        continue;
                    };
                    self.state.disk_dialog = Some(crate::state::DiskDialog::Resize(
                        crate::state::DiskResizeDialog {
                            connection_id,
                            vm_uuid,
                            vdi_uuid,
                            name,
                            current_gib,
                            size_gib: current_gib.saturating_add(1).to_string(),
                            submitting: false,
                            error: None,
                        },
                    ));
                }
                UiAction::PromptRemoveDisk {
                    vbd_uuid,
                    name,
                    owner,
                } => {
                    let Some(key) = self
                        .state
                        .object_tabs
                        .get(self.state.active_tab)
                        .map(|tab| tab.key.clone())
                    else {
                        continue;
                    };
                    let Some((connection_id, vm_uuid)) = vm_key(&key) else {
                        continue;
                    };
                    self.state.disk_dialog = Some(crate::state::DiskDialog::Remove(
                        crate::state::DiskRemoveDialog {
                            connection_id,
                            vm_uuid,
                            vbd_uuid,
                            name,
                            owner,
                            delete_vdi: owner,
                            submitting: false,
                            error: None,
                        },
                    ));
                }
                UiAction::CloseDiskDialog => {
                    let submitting = self
                        .state
                        .disk_dialog
                        .as_ref()
                        .is_some_and(disk_dialog_is_submitting);
                    if !submitting {
                        self.state.disk_dialog = None;
                    }
                }
                UiAction::SaveDiskDialog => {
                    const GIB: u64 = 1024 * 1024 * 1024;
                    let Some(dialog) = self.state.disk_dialog.as_mut() else {
                        continue;
                    };
                    match dialog {
                        crate::state::DiskDialog::Add(dialog) => {
                            let Some(size_gib) = dialog.size_gib.trim().parse::<u64>().ok() else {
                                dialog.error = Some("Enter a whole number of GiB.".into());
                                continue;
                            };
                            let Some(size_bytes) = size_gib.checked_mul(GIB) else {
                                dialog.error = Some("The disk size is too large.".into());
                                continue;
                            };
                            let Some(sr_uuid) = dialog.sr_uuid.clone() else {
                                dialog.error = Some("Select a storage repository.".into());
                                continue;
                            };
                            if size_bytes == 0 {
                                dialog.error =
                                    Some("The disk size must be greater than zero.".into());
                                continue;
                            }
                            dialog.submitting = true;
                            dialog.error = None;
                            self.worker.send(Command::AddDisk {
                                connection_id: dialog.connection_id.clone(),
                                vm_uuid: dialog.vm_uuid.clone(),
                                sr_uuid,
                                size_bytes,
                                name: (!dialog.name.trim().is_empty())
                                    .then(|| dialog.name.trim().to_owned()),
                            });
                        }
                        crate::state::DiskDialog::Resize(dialog) => {
                            let Some(size_gib) = dialog.size_gib.trim().parse::<u64>().ok() else {
                                dialog.error = Some("Enter a whole number of GiB.".into());
                                continue;
                            };
                            let Some(size_bytes) = size_gib.checked_mul(GIB) else {
                                dialog.error = Some("The disk size is too large.".into());
                                continue;
                            };
                            if size_gib <= dialog.current_gib {
                                dialog.error = Some("A disk can only be increased in size.".into());
                                continue;
                            }
                            dialog.submitting = true;
                            dialog.error = None;
                            self.worker.send(Command::ResizeDisk {
                                connection_id: dialog.connection_id.clone(),
                                vm_uuid: dialog.vm_uuid.clone(),
                                vdi_uuid: dialog.vdi_uuid.clone(),
                                size_bytes,
                            });
                        }
                        crate::state::DiskDialog::Remove(dialog) => {
                            dialog.submitting = true;
                            dialog.error = None;
                            self.worker.send(Command::RemoveDisk {
                                connection_id: dialog.connection_id.clone(),
                                vm_uuid: dialog.vm_uuid.clone(),
                                vbd_uuid: dialog.vbd_uuid.clone(),
                                delete_vdi: dialog.delete_vdi,
                            });
                        }
                    }
                }
                UiAction::OpenVifDialog => {
                    let Some(key) = self
                        .state
                        .object_tabs
                        .get(self.state.active_tab)
                        .map(|tab| tab.key.clone())
                    else {
                        continue;
                    };
                    let Some((connection_id, vm_uuid)) = vm_key(&key) else {
                        continue;
                    };
                    let networks: Vec<(String, String)> =
                        find_connection(&self.workspace, &connection_id)
                            .and_then(crate::state::connection_inventory)
                            .map(|inventory| {
                                inventory
                                    .networks
                                    .iter()
                                    .map(|network| (network.uuid.clone(), network.name.clone()))
                                    .collect()
                            })
                            .unwrap_or_default();
                    if networks.is_empty() {
                        self.state.toasts.push(Toast::new(
                            "No networks are available in this pool.",
                            ToastKind::Error,
                        ));
                        continue;
                    }
                    self.state.vif_dialog = Some(crate::state::VifDialog {
                        connection_id,
                        vm_uuid,
                        network_uuid: networks.first().map(|(uuid, _)| uuid.clone()),
                        networks,
                        mac: String::new(),
                        submitting: false,
                        error: None,
                    });
                }
                UiAction::CloseVifDialog => {
                    if !self
                        .state
                        .vif_dialog
                        .as_ref()
                        .is_some_and(|dialog| dialog.submitting)
                    {
                        self.state.vif_dialog = None;
                    }
                }
                UiAction::SaveVif => {
                    let Some(dialog) = self.state.vif_dialog.as_mut() else {
                        continue;
                    };
                    let Some(network_uuid) = dialog.network_uuid.clone() else {
                        dialog.error = Some("Select a network for the new interface.".into());
                        continue;
                    };
                    dialog.submitting = true;
                    dialog.error = None;
                    self.worker.send(Command::AddVif {
                        connection_id: dialog.connection_id.clone(),
                        vm_uuid: dialog.vm_uuid.clone(),
                        network_uuid,
                        mac: (!dialog.mac.trim().is_empty()).then(|| dialog.mac.trim().to_owned()),
                    });
                }
                UiAction::PromptRemoveVif { vif_uuid, mac } => {
                    let Some(key) = self
                        .state
                        .object_tabs
                        .get(self.state.active_tab)
                        .map(|tab| tab.key.clone())
                    else {
                        continue;
                    };
                    let Some((connection_id, vm_uuid)) = vm_key(&key) else {
                        continue;
                    };
                    self.state.vif_confirm = Some(crate::state::VifConfirm {
                        connection_id,
                        vm_uuid,
                        vif_uuid,
                        mac,
                    });
                }
                UiAction::CancelRemoveVif => self.state.vif_confirm = None,
                UiAction::RemoveVif => {
                    let Some(confirm) = self.state.vif_confirm.take() else {
                        continue;
                    };
                    self.worker.send(Command::RemoveVif {
                        connection_id: confirm.connection_id,
                        vm_uuid: confirm.vm_uuid,
                        vif_uuid: confirm.vif_uuid,
                    });
                }
                UiAction::SetVifPlugged { vif_uuid, plugged } => {
                    let Some(key) = self
                        .state
                        .object_tabs
                        .get(self.state.active_tab)
                        .map(|tab| tab.key.clone())
                    else {
                        continue;
                    };
                    let Some((connection_id, vm_uuid)) = vm_key(&key) else {
                        continue;
                    };
                    self.worker.send(Command::SetVifPlugged {
                        connection_id,
                        vm_uuid,
                        vif_uuid,
                        plugged,
                    });
                }
                UiAction::OpenConsole(connection_id, vm_uuid) => {
                    self.release_console_modifiers();
                    if let Some(console) = self.state.console.take() {
                        self.worker.send(Command::CloseConsole {
                            console_id: console.console_id,
                        });
                    }
                    self.state.console_seq += 1;
                    let console_id = format!("{vm_uuid}-{}", self.state.console_seq);
                    self.state.console = Some(ConsoleState::connecting(
                        console_id.clone(),
                        connection_id.clone(),
                        vm_uuid.clone(),
                    ));
                    self.worker.send(Command::OpenConsole {
                        console_id,
                        connection_id,
                        vm_uuid,
                    });
                }
                UiAction::CloseConsole => {
                    self.release_console_modifiers();
                    if let Some(console) = self.state.console.take() {
                        self.worker.send(Command::CloseConsole {
                            console_id: console.console_id,
                        });
                    }
                }
                UiAction::ConsoleKey {
                    console_id,
                    keycode,
                    down,
                } => {
                    if self
                        .state
                        .console
                        .as_ref()
                        .is_some_and(|console| console.console_id == console_id)
                    {
                        self.worker.send(Command::ConsoleInput {
                            console_id,
                            input: ConsoleInput::Key { keycode, down },
                        });
                    }
                }
                UiAction::ConsoleClipboard { console_id, text } => {
                    if self
                        .state
                        .console
                        .as_ref()
                        .is_some_and(|console| console.console_id == console_id)
                    {
                        self.worker.send(Command::ConsoleInput {
                            console_id,
                            input: ConsoleInput::Clipboard(text),
                        });
                    }
                }
                UiAction::ConsolePointer {
                    console_id,
                    x,
                    y,
                    buttons,
                } => {
                    if self
                        .state
                        .console
                        .as_ref()
                        .is_some_and(|console| console.console_id == console_id)
                    {
                        self.worker.send(Command::ConsoleInput {
                            console_id,
                            input: ConsoleInput::Pointer { x, y, buttons },
                        });
                    }
                }
                UiAction::DismissToast(index) => {
                    if index < self.state.toasts.len() {
                        self.state.toasts.remove(index);
                    }
                }
                UiAction::ConfirmRemove(id) => self.state.confirm_remove = Some(id),
                UiAction::CancelRemove => self.state.confirm_remove = None,
            }
        }
    }

    fn close_console(&mut self) {
        self.release_console_modifiers();
        if let Some(console) = self.state.console.take() {
            self.worker.send(Command::CloseConsole {
                console_id: console.console_id,
            });
        }
    }

    fn release_console_modifiers(&mut self) {
        let Some(console) = self.state.console.as_mut() else {
            return;
        };
        for (index, keycode) in [0xffe1u32, 0xffe3, 0xffe9, 0xffeb].into_iter().enumerate() {
            let pressed = match index {
                0 => console.modifiers.0,
                1 => console.modifiers.1,
                2 => console.modifiers.2,
                _ => console.modifiers.3,
            };
            if pressed {
                self.worker.send(Command::ConsoleInput {
                    console_id: console.console_id.clone(),
                    input: ConsoleInput::Key {
                        keycode,
                        down: false,
                    },
                });
            }
        }
        console.modifiers = (false, false, false, false);
    }

    fn expired_toasts(&mut self) {
        let now = Instant::now();
        self.state
            .toasts
            .retain(|toast| now.duration_since(toast.created) < Duration::from_secs(5));
    }
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.drain_events();
        self.expired_toasts();
        let console_was_focused = self.state.console_focus;
        self.shortcuts(ui.ctx());
        self.state.console_focus = false;

        let mut actions = Vec::new();

        views::chrome::titlebar(ui, &mut self.state, &self.workspace, &mut actions);
        views::chrome::toolbar(ui, &mut self.state, &self.workspace, &mut actions);
        views::sidebar::show(ui, &mut self.state, &self.workspace, &mut actions);
        views::chrome::object_tabs(ui, &mut self.state, &self.workspace, &mut actions);
        views::chrome::footer(ui, &mut self.state, &self.workspace);

        egui::CentralPanel::default()
            .frame(
                egui::Frame::NONE
                    .fill(theme::bg())
                    .inner_margin(egui::Margin::symmetric(22, 16)),
            )
            .show(ui, |ui| {
                egui::ScrollArea::vertical()
                    .id_salt("workspace-scroll")
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        views::content::show(ui, &mut self.state, &self.workspace, &mut actions);
                    });
            });

        views::chrome::resize_edges(ui);
        let ctx = ui.ctx().clone();
        views::dialogs::show(&ctx, &mut self.state, &self.workspace, &mut actions);
        widgets::toasts(&ctx, &self.state, &mut actions);

        self.apply(actions);
        if console_was_focused && !self.state.console_focus {
            self.release_console_modifiers();
        }

        let busy = self.state.connect.as_ref().is_some_and(|c| c.submitting)
            || !self.state.busy.is_empty()
            || self
                .workspace
                .tasks
                .iter()
                .any(|task| matches!(task.status, xen_core::operations::TaskStatus::Running));
        ctx.request_repaint_after(Duration::from_millis(if busy { 200 } else { 800 }));
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        self.worker.shutdown();
    }
}

fn vm_object_key(workspace: &WorkspaceView, connection_id: &str, vm_uuid: &str) -> ObjectKey {
    let inventory =
        find_connection(workspace, connection_id).and_then(crate::state::connection_inventory);
    let (connection_id, vm_uuid) = (connection_id.to_owned(), vm_uuid.to_owned());
    if inventory.is_some_and(|inventory| inventory.templates.iter().any(|vm| vm.uuid == vm_uuid)) {
        ObjectKey::Template {
            connection_id,
            vm_uuid,
        }
    } else if inventory
        .is_some_and(|inventory| inventory.snapshots.iter().any(|vm| vm.uuid == vm_uuid))
    {
        ObjectKey::Snapshot {
            connection_id,
            vm_uuid,
        }
    } else {
        ObjectKey::Vm {
            connection_id,
            vm_uuid,
        }
    }
}

fn vm_key(key: &ObjectKey) -> Option<(String, String)> {
    match key {
        ObjectKey::Vm {
            connection_id,
            vm_uuid,
        }
        | ObjectKey::Template {
            connection_id,
            vm_uuid,
        }
        | ObjectKey::Snapshot {
            connection_id,
            vm_uuid,
        } => Some((connection_id.clone(), vm_uuid.clone())),
        _ => None,
    }
}

fn disk_dialog_vm(dialog: &crate::state::DiskDialog) -> &str {
    match dialog {
        crate::state::DiskDialog::Add(dialog) => &dialog.vm_uuid,
        crate::state::DiskDialog::Resize(dialog) => &dialog.vm_uuid,
        crate::state::DiskDialog::Remove(dialog) => &dialog.vm_uuid,
    }
}

fn disk_dialog_is_submitting(dialog: &crate::state::DiskDialog) -> bool {
    match dialog {
        crate::state::DiskDialog::Add(dialog) => dialog.submitting,
        crate::state::DiskDialog::Resize(dialog) => dialog.submitting,
        crate::state::DiskDialog::Remove(dialog) => dialog.submitting,
    }
}

fn disk_dialog_error(dialog: &mut crate::state::DiskDialog, message: String) {
    match dialog {
        crate::state::DiskDialog::Add(dialog) => {
            dialog.submitting = false;
            dialog.error = Some(message);
        }
        crate::state::DiskDialog::Resize(dialog) => {
            dialog.submitting = false;
            dialog.error = Some(message);
        }
        crate::state::DiskDialog::Remove(dialog) => {
            dialog.submitting = false;
            dialog.error = Some(message);
        }
    }
}
