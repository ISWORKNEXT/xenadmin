use std::time::{Duration, Instant};

use chrono::Local;
use eframe::egui;
use xen_core::workspace::WorkspaceView;

use crate::state::{
    ActionKind, ConnectState, Intent, LoadState, Scope, Toast, ToastKind, UiAction, UiState, View,
    connection_id, find_connection, find_vm, visible_connections,
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
        theme::set_light(startup_light.unwrap_or_else(crate::config::load_light_preference));
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

    fn drain_events(&mut self) {
        while let Ok(event) = self.worker.events.try_recv() {
            match event {
                Event::Workspace(workspace) => {
                    self.workspace = workspace;
                    self.state.loaded = true;
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
                            self.apply(vec![UiAction::OpenHostDetails {
                                connection_id,
                                host_uuid: uuid,
                                host_name,
                            }]);
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
                            self.state.detail = Some((connection_id, uuid));
                        }
                    }
                    if let Some(id) = self.state.pending_scope.clone()
                        && find_connection(&self.workspace, &id).is_some_and(|connection| {
                            !matches!(
                                connection,
                                xen_core::workspace::ConnectionView::Offline { .. }
                            )
                        })
                    {
                        self.state.scope = Scope::Connection(id);
                        self.state.pending_scope = None;
                        self.state.expanded.insert(
                            self.state
                                .scope
                                .connection_id()
                                .unwrap_or_default()
                                .to_owned(),
                        );
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
        let keys = [
            egui::Key::Num1,
            egui::Key::Num2,
            egui::Key::Num3,
            egui::Key::Num4,
            egui::Key::Num5,
            egui::Key::Num6,
            egui::Key::Num7,
        ];
        let mut selected = None;
        let mut focus_search = false;
        let mut refresh = false;
        ctx.input_mut(|input| {
            for (index, key) in keys.iter().enumerate() {
                if input.consume_key(egui::Modifiers::COMMAND, *key) {
                    selected = Some(View::ALL[index]);
                }
            }
            if input.consume_key(egui::Modifiers::COMMAND, egui::Key::K) {
                focus_search = true;
            }
            if input.consume_key(egui::Modifiers::NONE, egui::Key::F5) {
                refresh = true;
            }
        });
        if let Some(view) = selected {
            self.state.view = view;
            self.state.host_route = None;
            self.state.page = 0;
            self.state.selected.clear();
        }
        if focus_search {
            self.state.view = View::Vms;
            self.state.host_route = None;
            self.state.focus_search = true;
        }
        if refresh {
            let actions = vec![UiAction::Refresh(None)];
            self.apply(actions);
        }
    }

    fn apply(&mut self, actions: Vec<UiAction>) {
        for action in actions {
            match action {
                UiAction::SetView(view) => {
                    self.state.view = view;
                    self.state.host_route = None;
                    self.state.page = 0;
                    self.state.selected.clear();
                    self.state.detail = None;
                }
                UiAction::SetScope(scope) => {
                    self.state.scope = scope;
                    self.state.selected.clear();
                    self.state.page = 0;
                    self.state.detail = None;
                    self.state.host_route = None;
                }
                UiAction::ToggleExpand(id) => {
                    if !self.state.expanded.remove(&id) {
                        self.state.expanded.insert(id);
                    }
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
                    self.state.detail = Some((connection_id, vm_uuid));
                }
                UiAction::CloseVm => self.state.detail = None,
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
                    self.state.view = View::Tasks;
                    self.state.detail = None;
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
                    self.state.host_route = Some(crate::state::HostRoute {
                        connection_id: connection_id.clone(),
                        host_uuid: host_uuid.clone(),
                        host_name,
                    });
                    self.state.host_tab = crate::state::HostTab::General;
                    self.state.record_query.clear();
                    self.state.record_page = 0;
                    self.state.host_seq += 1;
                    self.state.host_state = LoadState::Loading;
                    self.state.perf_state = LoadState::Idle;
                    self.worker.send(Command::HostDetails {
                        seq: self.state.host_seq,
                        connection_id,
                        host_uuid,
                    });
                }
                UiAction::CloseHostDetails => {
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
        views::sidebar::show(ui, &mut self.state, &self.workspace, &mut actions);
        views::vm_detail::show(ui, &mut self.state, &self.workspace, &mut actions);
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
