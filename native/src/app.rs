use std::time::{Duration, Instant};

use chrono::Local;
use eframe::egui;
use xen_core::workspace::WorkspaceView;

use crate::state::{
    ActionKind, ConnectState, Intent, ListKind, LoadState, ObjectKey, ObjectTab, Scope, Toast,
    ToastKind, UiAction, UiState, View, connection_id, find_connection, find_vm,
    visible_connections,
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
    pub light: Option<bool>,
}

pub struct App {
    pub state: UiState,
    pub workspace: WorkspaceView,
    worker: Worker,
    pending_host: Option<String>,
    pending_host_tab: Option<crate::state::HostTab>,
    pending_vm: Option<String>,
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
                let _ = vm_uuid;
                self.state.vm_subtab = crate::state::VmTab::General;
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
                                && inventory
                                    .vms
                                    .iter()
                                    .chain(&inventory.templates)
                                    .chain(&inventory.snapshots)
                                    .any(|vm| vm.uuid == uuid)
                            {
                                found = Some(connection_id(connection).to_owned());
                                break;
                            }
                        }
                        if let Some(connection_id) = found {
                            self.pending_vm = None;
                            self.open_object(
                                ObjectKey::Vm {
                                    connection_id,
                                    vm_uuid: uuid,
                                },
                                None,
                            );
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
                    self.open_object(
                        ObjectKey::Vm {
                            connection_id,
                            vm_uuid,
                        },
                        None,
                    );
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
                    });
                }
                UiAction::SubmitIntent => {
                    let Some(intent) = self.state.intent.take() else {
                        continue;
                    };
                    let action = intent.kind.to_action(&intent.name);
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
        self.shortcuts(ui.ctx());

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
