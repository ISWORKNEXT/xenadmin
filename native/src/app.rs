use std::collections::HashSet;
use std::time::{Duration, Instant};

use chrono::Local;
use eframe::egui;
use xen_core::workspace::WorkspaceView;

use crate::console::ConsoleInput;
use crate::state::{
    ActionKind, ConnectState, ConsoleState, ConsoleStatus, Intent, ListKind, LoadState, ObjectKey,
    ObjectSelection, Scope, Toast, ToastKind, UiAction, UiState, View, connection_id,
    find_connection, find_vm, visible_connections,
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
    pub health: bool,
    pub auth: bool,
    pub pool_status: bool,
    pub sr: Option<String>,
    pub network: Option<String>,
    pub search: Option<String>,
    pub group_by: Option<crate::config::GroupBy>,
    pub saved_search: Option<String>,
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
    pending_health: bool,
    pending_auth: bool,
    pending_pool_status: bool,
    pending_sr: Option<String>,
    pending_network: Option<String>,
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
            health: startup_health,
            auth: startup_auth,
            pool_status: startup_pool_status,
            sr: startup_sr,
            network: startup_network,
            search: startup_search,
            group_by: startup_group_by,
            saved_search: startup_saved_search,
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
        let mut state = UiState {
            query: startup_search.unwrap_or_default(),
            saved_searches: crate::config::load_saved_searches(),
            ..UiState::default()
        };
        if let Some(group_by) = startup_group_by {
            state.group_by = group_by;
        }
        if let Some(name) = startup_saved_search
            && let Some(saved) = state
                .saved_searches
                .iter()
                .find(|saved| saved.name.eq_ignore_ascii_case(&name))
        {
            state.query = saved.query.clone();
            state.group_by = saved.group_by;
        }
        if let Some(view) = startup_view {
            state.view = view;
        }
        if open_connect {
            state.connect = Some(ConnectState::empty());
        }
        if let Some(view) = startup_view {
            let tab = match view {
                View::Overview => None,
                View::Tasks => {
                    state.navigation_mode = views::sidebar::NavigationMode::Notifications;
                    Some(ObjectSelection {
                        key: ObjectKey::Tasks,
                        title: "Events".into(),
                    })
                }
                view => {
                    let kind = match view {
                        View::Vms => ListKind::Vms,
                        View::Hosts => ListKind::Hosts,
                        View::Templates => ListKind::Templates,
                        View::Storage => ListKind::Storage,
                        View::Networks => ListKind::Networks,
                        View::Overview | View::Tasks => unreachable!(),
                    };
                    Some(ObjectSelection {
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
                state.selection = Some(tab);
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
            pending_health: startup_health,
            pending_auth: startup_auth,
            pending_pool_status: startup_pool_status,
            pending_sr: startup_sr,
            pending_network: startup_network,
        }
    }

    fn object_title(&self, key: &ObjectKey) -> String {
        match key {
            ObjectKey::Home => "Home".into(),
            ObjectKey::Group(name) => name.clone(),
            ObjectKey::Vdi { vdi_uuid, .. } => self
                .state
                .sr_vdis
                .iter()
                .find(|v| &v.uuid == vdi_uuid)
                .map_or_else(|| "Virtual disk".into(), |v| v.name.clone()),
            ObjectKey::Pool(id) => self
                .workspace
                .connections
                .iter()
                .find(|connection| connection_id(connection) == id)
                .map_or_else(|| id.clone(), crate::state::connection_name),
            ObjectKey::Host {
                connection_id,
                host_uuid,
            } => find_connection(&self.workspace, connection_id)
                .and_then(|connection| {
                    crate::state::connection_inventory(connection).and_then(|inventory| {
                        inventory
                            .hosts
                            .iter()
                            .find(|host| &host.uuid == host_uuid)
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
            ObjectKey::Health { .. } => "Health & diagnostics".into(),
            ObjectKey::Auth { .. } => "Authentication & RBAC".into(),
            ObjectKey::PoolStatus { .. } => "Pool status".into(),
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
            ObjectKey::Tasks => "Events".into(),
        }
    }

    fn open_object(&mut self, key: ObjectKey, title: Option<String>) {
        if matches!(key, ObjectKey::Tasks) {
            if title.is_none() {
                self.state.notifications.alerts = false;
            }
            self.state.navigation_mode = views::sidebar::NavigationMode::Notifications;
        } else if self.state.navigation_mode == views::sidebar::NavigationMode::Notifications {
            self.state.navigation_mode = views::sidebar::NavigationMode::Infrastructure;
        }
        let title = title.unwrap_or_else(|| self.object_title(&key));
        self.state.tree_selection.clear();
        self.state.tree_anchor = None;
        self.state.selection = Some(ObjectSelection { key, title });
        self.activate_selection();
    }

    fn activate_selection(&mut self) {
        let Some(selection) = &self.state.selection else {
            return;
        };
        let key = selection.key.clone();
        if !self.state.navigating_history
            && self.state.selection_history.get(self.state.history_index) != Some(&key)
        {
            self.state
                .selection_history
                .truncate(self.state.history_index + 1);
            self.state.selection_history.push(key.clone());
            self.state.history_index = self.state.selection_history.len() - 1;
        }
        self.state.selected.clear();
        self.state.page = 0;
        self.state.detail = None;

        match &key {
            ObjectKey::Group(_) => {
                self.state.scope = Scope::All;
            }
            ObjectKey::Vdi { connection_id, .. } => {
                self.state.scope = Scope::Connection(connection_id.clone());
            }
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
                host_uuid,
            } => {
                self.state.scope = find_connection(&self.workspace, connection_id)
                    .and_then(crate::state::connection_inventory)
                    .and_then(|inventory| {
                        inventory.hosts.iter().find(|host| &host.uuid == host_uuid)
                    })
                    .map_or_else(
                        || Scope::Connection(connection_id.clone()),
                        |host| Scope::Host {
                            connection_id: connection_id.clone(),
                            host_ref: host.reference.clone(),
                        },
                    );
                let same = self.state.host_route.as_ref().is_some_and(|route| {
                    &route.connection_id == connection_id && &route.host_uuid == host_uuid
                });
                self.state.host_route = Some(crate::state::HostRoute {
                    connection_id: connection_id.clone(),
                    host_uuid: host_uuid.clone(),
                });
                if !same {
                    self.state.perf_seq += 1;
                    self.state.perf_state = LoadState::Idle;
                }
                if !same || matches!(self.state.host_state, LoadState::Idle | LoadState::Error(_)) {
                    self.state.host_seq += 1;
                    self.state.host_state = LoadState::Loading;
                    self.worker.send(Command::HostDetails {
                        seq: self.state.host_seq,
                        connection_id: connection_id.clone(),
                        host_uuid: host_uuid.clone(),
                    });
                }
                if self.state.host_tab == crate::state::HostTab::Performance
                    && matches!(self.state.perf_state, LoadState::Idle)
                {
                    self.apply(vec![UiAction::LoadPerformance]);
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
                    self.state.vm_subtab = self.state.vm_last_tabs[match key {
                        ObjectKey::Template { .. } => 1,
                        ObjectKey::Snapshot { .. } => 2,
                        _ => 0,
                    }];
                    self.state.vm_perf_state = LoadState::Idle;
                }
                self.state.vm_seq += 1;
                self.state.vm_state = LoadState::Loading;
                self.worker.send(Command::VmDetails {
                    seq: self.state.vm_seq,
                    connection_id: connection_id.clone(),
                    vm_uuid: vm_uuid.clone(),
                });
            }
            ObjectKey::Storage { connection_id, .. } => {
                self.state.scope = Scope::Connection(connection_id.clone());
                self.state.view = View::Storage;
            }
            ObjectKey::Network { connection_id, .. } => {
                self.state.scope = Scope::Connection(connection_id.clone());
                self.state.view = View::Networks;
            }
            ObjectKey::Health { connection_id } => {
                self.state.scope = Scope::Connection(connection_id.clone());
            }
            ObjectKey::Auth { connection_id } => {
                self.state.scope = Scope::Connection(connection_id.clone());
            }
            ObjectKey::PoolStatus { connection_id } => {
                self.state.scope = Scope::Connection(connection_id.clone());
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

    /// Clear a selection that disappeared after a refresh or delete.
    fn prune_missing_selection(&mut self) {
        let gone = self
            .state
            .selection
            .as_ref()
            .is_some_and(|selection| match &selection.key {
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
            });
        if gone {
            self.clear_selection();
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

    fn clear_selection(&mut self) {
        self.open_object(ObjectKey::Home, None);
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
                Event::PoolOptions { seq, result } => {
                    if seq != self.state.pool_options_seq {
                        continue;
                    }
                    if let Some(dialog) = self.state.pool_options.as_mut() {
                        match result {
                            Ok(data) => {
                                dialog.original = Some(data.clone());
                                dialog.data = LoadState::Ready(data);
                            }
                            Err(message) => dialog.data = LoadState::Error(message),
                        }
                    }
                }
                Event::PoolOptionsSaved { seq, result } => {
                    if seq != self.state.pool_options_seq {
                        continue;
                    }
                    match result {
                        Ok(_) => {
                            let saved_connection = self
                                .state
                                .pool_options
                                .as_ref()
                                .map(|dialog| dialog.connection_id.clone());
                            self.state.pool_options = None;
                            self.state
                                .toasts
                                .push(Toast::new("Saved pool options.", ToastKind::Success));
                            if self.state.host_route.as_ref().is_some_and(|route| {
                                Some(&route.connection_id) == saved_connection.as_ref()
                            }) {
                                self.apply(vec![UiAction::ReloadHostDetails]);
                            }
                        }
                        Err(message) => {
                            if let Some(dialog) = self.state.pool_options.as_mut() {
                                dialog.submitting = false;
                                dialog.error = Some(message);
                            }
                        }
                    }
                }

                Event::ObjectProperties { seq, result } => {
                    if seq == self.state.properties_seq
                        && let Some(dialog) = self.state.properties.as_mut()
                    {
                        match result {
                            Ok(properties) => {
                                dialog.original = Some(properties.clone());
                                dialog.new_tag.clear();
                                dialog.properties = LoadState::Ready(properties);
                            }
                            Err(message) => dialog.properties = LoadState::Error(message),
                        }
                    }
                }
                Event::ObjectPropertiesSaved { seq, result } => {
                    if seq != self.state.properties_seq {
                        continue;
                    }
                    match result {
                        Ok(properties) => {
                            if let Some(dialog) = self.state.properties.take() {
                                self.state.toasts.push(Toast::new(
                                    format!("Saved properties for {}.", properties.name),
                                    ToastKind::Success,
                                ));
                                self.state.sr_vdis_sr = None;
                                let current =
                                    self.state.selection.as_ref().map(|tab| tab.key.clone());
                                if let Some(key) = current
                                    && key.connection_id() == Some(dialog.connection_id.as_str())
                                {
                                    if vm_key(&key).is_some() {
                                        self.apply(vec![UiAction::ReloadVmDetails]);
                                    } else if matches!(key, ObjectKey::Host { .. }) {
                                        self.apply(vec![UiAction::ReloadHostDetails]);
                                    }
                                }
                            }
                        }
                        Err(message) => {
                            if let Some(dialog) = self.state.properties.as_mut() {
                                dialog.submitting = false;
                                dialog.error = Some(format!("Save failed: {message}"));
                            }
                        }
                    }
                }
                Event::Workspace(workspace) => {
                    let refresh_vm = self
                        .state
                        .selection
                        .as_ref()
                        .and_then(|tab| vm_key(&tab.key))
                        .is_some_and(|(connection_id, vm_uuid)| {
                            workspace.tasks.iter().any(|task| {
                                task.connection_id == connection_id
                                    && task.vm_uuid == vm_uuid
                                    && task.status.terminal()
                                    && !self
                                        .workspace
                                        .tasks
                                        .iter()
                                        .any(|old| old.id == task.id && old.status.terminal())
                            })
                        });
                    self.workspace = workspace;
                    let titles: Vec<_> = self
                        .state
                        .selection
                        .iter()
                        .map(|tab| {
                            matches!(
                                tab.key,
                                ObjectKey::Vm { .. }
                                    | ObjectKey::Template { .. }
                                    | ObjectKey::Snapshot { .. }
                                    | ObjectKey::Host { .. }
                                    | ObjectKey::Storage { .. }
                                    | ObjectKey::Network { .. }
                            )
                            .then(|| self.object_title(&tab.key))
                        })
                        .collect();
                    for (tab, title) in self.state.selection.iter_mut().zip(titles) {
                        if let Some(title) = title {
                            tab.title = title;
                        }
                    }
                    self.state.loaded = true;
                    self.prune_missing_selection();
                    if refresh_vm {
                        self.apply(vec![UiAction::ReloadVmDetails]);
                    }
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
                            && self
                                .state
                                .selection
                                .as_ref()
                                .is_none_or(|s| s.key == ObjectKey::Home);
                        self.state.scope = Scope::Connection(id.clone());
                        self.state.pending_scope = None;
                        if should_open_pool {
                            let key = find_connection(&self.workspace, &id)
                                .and_then(crate::state::connection_inventory)
                                .filter(|inv| !inv.named_pool && inv.hosts.len() == 1)
                                .map_or_else(
                                    || ObjectKey::Pool(id.clone()),
                                    |inv| ObjectKey::Host {
                                        connection_id: id.clone(),
                                        host_uuid: inv.hosts[0].uuid.clone(),
                                    },
                                );
                            self.open_object(key, None);
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
                                    host_uuid: uuid,
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
                    if let Some(uuid) = self.pending_sr.clone() {
                        let mut found = None;
                        for connection in &self.workspace.connections {
                            if let Some(inventory) = crate::state::connection_inventory(connection)
                                && inventory.storage.iter().any(|sr| sr.uuid == uuid)
                            {
                                found = Some(ObjectKey::Storage {
                                    connection_id: connection_id(connection).to_owned(),
                                    sr_uuid: uuid.clone(),
                                });
                                break;
                            }
                        }
                        if let Some(key) = found {
                            self.pending_sr = None;
                            self.open_object(key, None);
                        }
                    }
                    if self.pending_pool_status
                        && let Some(connection_id) =
                            self.state.scope.connection_id().map(str::to_owned)
                    {
                        self.pending_pool_status = false;
                        self.open_object(ObjectKey::PoolStatus { connection_id }, None);
                    }
                    if self.pending_auth
                        && let Some(connection_id) =
                            self.state.scope.connection_id().map(str::to_owned)
                    {
                        self.pending_auth = false;
                        self.open_object(ObjectKey::Auth { connection_id }, None);
                    }
                    if let Some(uuid) = self.pending_network.clone() {
                        let mut found = None;
                        for connection in &self.workspace.connections {
                            if let Some(inventory) = crate::state::connection_inventory(connection)
                                && inventory
                                    .networks
                                    .iter()
                                    .any(|network| network.uuid == uuid)
                            {
                                found = Some(ObjectKey::Network {
                                    connection_id: connection_id(connection).to_owned(),
                                    network_uuid: uuid.clone(),
                                });
                                break;
                            }
                        }
                        if let Some(key) = found {
                            self.pending_network = None;
                            self.open_object(key, None);
                        }
                    }
                    if self.pending_health
                        && let Some(connection_id) =
                            self.state.scope.connection_id().map(str::to_owned)
                    {
                        self.pending_health = false;
                        self.open_object(ObjectKey::Health { connection_id }, None);
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
                Event::WizardTemplate { seq, result } => {
                    if seq == self.state.new_vm_seq
                        && let Some(wizard) = self.state.new_vm.as_mut()
                    {
                        wizard.template = match result {
                            Ok(details) => {
                                wizard.draft.vcpus_max = if details.supports_vcpu_hotplug {
                                    details.vcpus_max.max(1)
                                } else {
                                    details.vcpus_at_startup.max(1)
                                };
                                wizard.draft.vcpus_at_startup =
                                    details.vcpus_at_startup.clamp(1, wizard.draft.vcpus_max);
                                wizard.draft.cores_per_socket = Some(
                                    details
                                        .cores_per_socket
                                        .parse()
                                        .ok()
                                        .filter(|&cores| {
                                            xen_core::vm::validate_cpu_topology(
                                                wizard.draft.vcpus_max,
                                                cores,
                                                0,
                                            )
                                            .is_ok()
                                        })
                                        .unwrap_or(1),
                                );
                                wizard.draft.memory_mib = details
                                    .memory_dynamic_max
                                    .div_ceil(1024 * 1024)
                                    .max(details.memory_static_min.div_ceil(1024 * 1024))
                                    .max(64);
                                wizard.draft.new_disk_gib =
                                    if details.disks.iter().all(|disk| disk.is_cd) {
                                        Some(details.provision_disks.first().map_or(20, |disk| {
                                            disk.size.div_ceil(1024 * 1024 * 1024)
                                        }))
                                    } else {
                                        None
                                    };
                                if wizard.draft.name.is_empty() {
                                    wizard.draft.name = format!("{} VM", details.name);
                                }
                                LoadState::Ready(*details)
                            }
                            Err(message) => LoadState::Error(message),
                        };
                    }
                }
                Event::XvaQueued { result } => match result {
                    Ok(()) => {
                        self.state.xva_dialog = None;
                        self.open_object(ObjectKey::Tasks, None);
                    }
                    Err(message) => {
                        if let Some(dialog) = self.state.xva_dialog.as_mut() {
                            dialog.submitting = false;
                            dialog.error = Some(message);
                        }
                    }
                },
                Event::VmCreationQueued { result } => match result {
                    Ok(()) => {
                        self.state.new_vm = None;
                        self.open_object(ObjectKey::Tasks, None);
                        self.state.toasts.push(Toast::new(
                            "VM creation queued. Follow progress in Events.",
                            ToastKind::Info,
                        ));
                    }
                    Err(message) => {
                        if let Some(wizard) = self.state.new_vm.as_mut() {
                            wizard.submitting = false;
                            wizard.error = Some(message);
                        }
                    }
                },
                Event::HostDetails {
                    seq,
                    connection_id,
                    host_uuid,
                    result,
                } => {
                    let value = match &result {
                        Ok(details) => LoadState::Ready((**details).clone()),
                        Err(error) => LoadState::Error(error.clone()),
                    };
                    self.state
                        .page_hosts
                        .insert((connection_id.clone(), host_uuid.clone()), value);
                    if seq != 0
                        && seq == self.state.host_seq
                        && self.state.host_route.as_ref().is_some_and(|r| {
                            r.connection_id == connection_id && r.host_uuid == host_uuid
                        })
                    {
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
                Event::AlertDismissed { result } => match result {
                    Ok(()) => self
                        .state
                        .toasts
                        .push(Toast::new("Alert dismissed.", ToastKind::Success)),
                    Err(message) => self
                        .state
                        .toasts
                        .push(Toast::new(message, ToastKind::Error)),
                },
                Event::SrIntroduced { result } => match result {
                    Ok(()) => {
                        self.state.sr_introduce_dialog = None;
                        self.state.toasts.push(Toast::new(
                            "Storage repository introduced. Reattach it to a host to access its disks.",
                            ToastKind::Success,
                        ));
                    }
                    Err(message) => {
                        if let Some(dialog) = self.state.sr_introduce_dialog.as_mut() {
                            dialog.submitting = false;
                            dialog.error = Some(message);
                        } else {
                            self.state
                                .toasts
                                .push(Toast::new(message, ToastKind::Error));
                        }
                    }
                },
                Event::SrCreated { result } => match result {
                    Ok(()) => {
                        self.state.sr_create_dialog = None;
                        self.state.toasts.push(Toast::new(
                            "Storage repository created.",
                            ToastKind::Success,
                        ));
                    }
                    Err(message) => {
                        if let Some(dialog) = self.state.sr_create_dialog.as_mut() {
                            dialog.submitting = false;
                            dialog.error = Some(message);
                        } else {
                            self.state
                                .toasts
                                .push(Toast::new(message, ToastKind::Error));
                        }
                    }
                },
                Event::SrReattached { sr_uuid, result } => match result {
                    Ok(()) => {
                        if self
                            .state
                            .sr_reattach_dialog
                            .as_ref()
                            .is_some_and(|dialog| dialog.sr_uuid == sr_uuid)
                        {
                            self.state.sr_reattach_dialog = None;
                        }
                        self.state.toasts.push(Toast::new(
                            "Storage repository reattached.",
                            ToastKind::Success,
                        ));
                    }
                    Err(message) => {
                        if let Some(dialog) = self.state.sr_reattach_dialog.as_mut()
                            && dialog.sr_uuid == sr_uuid
                        {
                            dialog.submitting = false;
                            dialog.error = Some(message);
                        } else {
                            self.state
                                .toasts
                                .push(Toast::new(message, ToastKind::Error));
                        }
                    }
                },
                Event::DrChanged { action, result } => match result {
                    Ok(()) => {
                        self.state.dr_task_dialog = None;
                        self.state.dr_task_delete_confirm = None;
                        self.state.toasts.push(Toast::new(
                            if action == "create" {
                                "DR task created. Introduced SRs are now managed by the pool."
                            } else {
                                "DR task destroyed. Introduced SRs remain until forgotten."
                            },
                            ToastKind::Success,
                        ));
                        self.state.pool_status_state = LoadState::Idle;
                        if matches!(
                            self.state.selection.as_ref().map(|tab| &tab.key),
                            Some(ObjectKey::PoolStatus { .. })
                        ) {
                            self.apply(vec![UiAction::LoadPoolStatus]);
                        }
                    }
                    Err(message) => {
                        let mut handled = false;
                        if let Some(dialog) = self.state.dr_task_dialog.as_mut() {
                            dialog.submitting = false;
                            dialog.error = Some(message.clone());
                            handled = true;
                        }
                        if let Some(confirm) = self.state.dr_task_delete_confirm.as_mut() {
                            confirm.submitting = false;
                            confirm.error = Some(message.clone());
                            handled = true;
                        }
                        if !handled {
                            self.state
                                .toasts
                                .push(Toast::new(message, ToastKind::Error));
                        }
                    }
                },
                Event::PvsChanged { action, result } => match result {
                    Ok(()) => {
                        self.state.pvs_site_dialog = None;
                        self.state.pvs_server_dialog = None;
                        self.state.pvs_cache_dialog = None;
                        self.state.pvs_proxy_dialog = None;
                        self.state.pvs_delete_confirm = None;
                        let message = match action.as_str() {
                            "site_create" => "PVS site added.",
                            "site_delete" => "PVS site removed.",
                            "server_create" => "PVS server added.",
                            "server_delete" => "PVS server removed.",
                            "proxy_create" => "PVS read caching enabled.",
                            "proxy_delete" => "PVS read caching removed.",
                            "cache_create" => "PVS cache storage added.",
                            _ => "PVS cache storage removed.",
                        };
                        self.state
                            .toasts
                            .push(Toast::new(message, ToastKind::Success));
                        self.state.pool_status_state = LoadState::Idle;
                        if matches!(
                            self.state.selection.as_ref().map(|tab| &tab.key),
                            Some(ObjectKey::PoolStatus { .. })
                        ) {
                            self.apply(vec![UiAction::LoadPoolStatus]);
                        }
                    }
                    Err(message) => {
                        let mut handled = false;
                        if let Some(dialog) = self.state.pvs_site_dialog.as_mut() {
                            dialog.submitting = false;
                            dialog.error = Some(message.clone());
                            handled = true;
                        }
                        if let Some(dialog) = self.state.pvs_server_dialog.as_mut() {
                            dialog.submitting = false;
                            dialog.error = Some(message.clone());
                            handled = true;
                        }
                        if let Some(dialog) = self.state.pvs_cache_dialog.as_mut() {
                            dialog.submitting = false;
                            dialog.error = Some(message.clone());
                            handled = true;
                        }
                        if let Some(dialog) = self.state.pvs_proxy_dialog.as_mut() {
                            dialog.submitting = false;
                            dialog.error = Some(message.clone());
                            handled = true;
                        }
                        if let Some(confirm) = self.state.pvs_delete_confirm.as_mut() {
                            confirm.submitting = false;
                            confirm.error = Some(message.clone());
                            handled = true;
                        }
                        if !handled {
                            self.state
                                .toasts
                                .push(Toast::new(message, ToastKind::Error));
                        }
                    }
                },
                Event::PvsVifCatalog { seq, result } => {
                    if seq == self.state.pvs_vif_seq
                        && let Some(dialog) = self.state.pvs_proxy_dialog.as_mut()
                    {
                        dialog.loading = false;
                        match result {
                            Ok(vifs) => {
                                dialog.error = None;
                                dialog.vif_uuid = vifs.first().map(|vif| vif.uuid.clone());
                                dialog.vifs = vifs;
                            }
                            Err(message) => dialog.error = Some(message),
                        }
                    }
                }
                Event::VmssChanged { action, result } => match result {
                    Ok(()) => {
                        self.state.vmss_dialog = None;
                        self.state.vmss_delete_confirm = None;
                        let message = match action.as_str() {
                            "create" => "Snapshot schedule created.",
                            "update" => "Snapshot schedule updated.",
                            _ => "Snapshot schedule deleted.",
                        };
                        self.state
                            .toasts
                            .push(Toast::new(message, ToastKind::Success));
                        self.state.pool_status_state = LoadState::Idle;
                        if matches!(
                            self.state.selection.as_ref().map(|tab| &tab.key),
                            Some(ObjectKey::PoolStatus { .. })
                        ) {
                            self.apply(vec![UiAction::LoadPoolStatus]);
                        }
                    }
                    Err(message) => {
                        let mut handled = false;
                        if let Some(dialog) = self.state.vmss_dialog.as_mut() {
                            dialog.submitting = false;
                            dialog.error = Some(message.clone());
                            handled = true;
                        }
                        if let Some(confirm) = self.state.vmss_delete_confirm.as_mut() {
                            confirm.submitting = false;
                            confirm.error = Some(message.clone());
                            handled = true;
                        }
                        if !handled {
                            self.state
                                .toasts
                                .push(Toast::new(message, ToastKind::Error));
                        }
                    }
                },
                Event::PoolActionFinished { action, result } => match result {
                    Ok(()) => {
                        self.state.ha_enable_dialog = None;
                        self.state.wlb_config_dialog = None;
                        self.state.pool_confirm = None;
                        self.state.pool_status_state = LoadState::Idle;
                        let message = match action.as_str() {
                            "ha_enable" => "High availability enabled.",
                            "ha_disable" => "High availability disabled.",
                            "wlb_configure" => "Workload balancing configured.",
                            "wlb_enable" => "Workload balancing enabled.",
                            "wlb_disable" => "Workload balancing disabled.",
                            _ => "Workload balancing deconfigured.",
                        };
                        self.state
                            .toasts
                            .push(Toast::new(message, ToastKind::Success));
                        if matches!(
                            self.state.selection.as_ref().map(|tab| &tab.key),
                            Some(ObjectKey::PoolStatus { .. })
                        ) {
                            self.apply(vec![UiAction::LoadPoolStatus]);
                        }
                    }
                    Err(message) => {
                        let mut handled = false;
                        if let Some(dialog) = self.state.ha_enable_dialog.as_mut() {
                            dialog.submitting = false;
                            dialog.error = Some(message.clone());
                            handled = true;
                        }
                        if let Some(dialog) = self.state.wlb_config_dialog.as_mut() {
                            dialog.submitting = false;
                            dialog.error = Some(message.clone());
                            handled = true;
                        }
                        if let Some(dialog) = self.state.pool_confirm.as_mut() {
                            dialog.submitting = false;
                            dialog.error = Some(message.clone());
                            handled = true;
                        }
                        if !handled {
                            self.state
                                .toasts
                                .push(Toast::new(message, ToastKind::Error));
                        }
                    }
                },
                Event::RdpOpened {
                    console_id,
                    host,
                    username,
                } => {
                    if let Some(console) = self.state.rdp_console.as_mut()
                        && console.console_id == console_id
                    {
                        console.status = crate::state::RdpStatus::Connected;
                        console.host = host;
                        console.username = username;
                        console.message.clear();
                    }
                }
                Event::RdpFrame {
                    console_id,
                    width,
                    height,
                    pixels,
                } => {
                    if let Some(console) = self.state.rdp_console.as_mut()
                        && console.console_id == console_id
                    {
                        console.width = width;
                        console.height = height;
                        console.pixels = pixels;
                        console.dirty = true;
                    }
                }
                Event::RdpClosed {
                    console_id,
                    message,
                } => {
                    if let Some(console) = self.state.rdp_console.as_mut()
                        && console.console_id == console_id
                    {
                        console.status = crate::state::RdpStatus::Closed;
                        console.message = message.unwrap_or_else(|| "Disconnected.".into());
                    }
                }
                Event::SshOpened {
                    console_id,
                    host,
                    username,
                } => {
                    if let Some(console) = self.state.ssh_console.as_mut()
                        && console.console_id == console_id
                    {
                        console.status = crate::state::SshStatus::Connected;
                        console.host = host;
                        console.username = username;
                        console.message.clear();
                    }
                }
                Event::SshOutput { console_id, data } => {
                    if let Some(console) = self.state.ssh_console.as_mut()
                        && console.console_id == console_id
                    {
                        console.buffer.feed(&data);
                    }
                }
                Event::SshClosed {
                    console_id,
                    message,
                } => {
                    if let Some(console) = self.state.ssh_console.as_mut()
                        && console.console_id == console_id
                    {
                        console.status = crate::state::SshStatus::Closed;
                        console.message = message.unwrap_or_else(|| "Disconnected.".into());
                    }
                }
                Event::AdEnabled { result } => match result {
                    Ok(()) => {
                        self.state.ad_enable_dialog = None;
                        self.state.toasts.push(Toast::new(
                            "Active Directory authentication enabled.",
                            ToastKind::Success,
                        ));
                        self.state.auth_state = LoadState::Idle;
                        self.apply(vec![UiAction::LoadAuth]);
                    }
                    Err(message) => {
                        if let Some(dialog) = self.state.ad_enable_dialog.as_mut() {
                            dialog.submitting = false;
                            dialog.error = Some(message);
                        } else {
                            self.state
                                .toasts
                                .push(Toast::new(message, ToastKind::Error));
                        }
                    }
                },
                Event::AdDisabled { result } => match result {
                    Ok(()) => {
                        self.state.ad_disable_dialog = None;
                        self.state.toasts.push(Toast::new(
                            "Active Directory authentication disabled.",
                            ToastKind::Success,
                        ));
                        self.state.auth_state = LoadState::Idle;
                        self.apply(vec![UiAction::LoadAuth]);
                    }
                    Err(message) => {
                        if let Some(dialog) = self.state.ad_disable_dialog.as_mut() {
                            dialog.submitting = false;
                            dialog.error = Some(message);
                        } else {
                            self.state
                                .toasts
                                .push(Toast::new(message, ToastKind::Error));
                        }
                    }
                },
                Event::SubjectRolesUpdated {
                    subject_uuid,
                    result,
                } => match result {
                    Ok(()) => {
                        if self
                            .state
                            .user_role_dialog
                            .as_ref()
                            .is_some_and(|dialog| dialog.subject_uuid == subject_uuid)
                        {
                            self.state.user_role_dialog = None;
                        }
                        self.state
                            .toasts
                            .push(Toast::new("Subject roles updated.", ToastKind::Success));
                        self.state.auth_state = LoadState::Idle;
                        self.apply(vec![UiAction::LoadAuth]);
                    }
                    Err(message) => {
                        if let Some(dialog) = self.state.user_role_dialog.as_mut()
                            && dialog.subject_uuid == subject_uuid
                        {
                            dialog.submitting = false;
                            dialog.error = Some(message);
                        } else {
                            self.state
                                .toasts
                                .push(Toast::new(message, ToastKind::Error));
                        }
                    }
                },
                Event::PoolStatus { seq, result } => {
                    if seq == self.state.pool_status_seq {
                        self.state.pool_status_state = match result {
                            Ok(status) => LoadState::Ready(*status),
                            Err(message) => LoadState::Error(message),
                        };
                    }
                }
                Event::AuthInfo { seq, result } => {
                    if seq == self.state.auth_seq {
                        self.state.auth_state = match result {
                            Ok(info) => LoadState::Ready(info),
                            Err(message) => LoadState::Error(message),
                        };
                    }
                }
                Event::PbdCatalog { seq, result } => {
                    if seq == self.state.pbd_catalog_seq {
                        self.state.pbd_catalog_loading = false;
                        match result {
                            Ok(pbds) => {
                                self.state.pbd_catalog = pbds;
                                self.state.pbd_catalog_error = None;
                            }
                            Err(message) => {
                                self.state.pbd_catalog = Vec::new();
                                self.state.pbd_catalog_error = Some(message);
                            }
                        }
                    }
                }
                Event::PbdChanged { result } => match result {
                    Ok(()) => {
                        self.state.toasts.push(Toast::new(
                            "Storage attachment updated.",
                            ToastKind::Success,
                        ));
                        self.state.pbd_catalog_sr = None;
                    }
                    Err(message) => self
                        .state
                        .toasts
                        .push(Toast::new(message, ToastKind::Error)),
                },
                Event::SrRepaired { sr_uuid, result } => match result {
                    Ok(repaired) => {
                        if repaired == 0 {
                            self.state.toasts.push(Toast::new(
                                "All storage attachments are already plugged.",
                                ToastKind::Info,
                            ));
                        } else {
                            self.state.toasts.push(Toast::new(
                                format!("Repaired {repaired} storage attachment(s)."),
                                ToastKind::Success,
                            ));
                        }
                        if self.state.selection.as_ref().is_some_and(|tab| {
                            matches!(
                                &tab.key,
                                ObjectKey::Storage { sr_uuid: current, .. } if current == &sr_uuid
                            )
                        }) {
                            self.state.pbd_catalog_sr = None;
                        }
                    }
                    Err(message) => self
                        .state
                        .toasts
                        .push(Toast::new(message, ToastKind::Error)),
                },
                Event::SrProbed { seq, result } => {
                    if seq == self.state.sr_probe_seq
                        && let Some(dialog) = self.state.sr_probe_dialog.as_mut()
                    {
                        dialog.submitting = false;
                        match result {
                            Ok(probe) => {
                                dialog.error = None;
                                dialog.result = Some(probe);
                            }
                            Err(message) => {
                                dialog.result = None;
                                dialog.error = Some(message);
                            }
                        }
                    }
                }
                Event::PifCatalog { seq, result } => {
                    if seq == self.state.pif_catalog_seq {
                        self.state.pif_catalog_loading = false;
                        match result {
                            Ok(pifs) => {
                                self.state.pif_catalog = pifs;
                                self.state.pif_catalog_error = None;
                            }
                            Err(message) => {
                                self.state.pif_catalog = Vec::new();
                                self.state.pif_catalog_error = Some(message);
                            }
                        }
                    }
                }
                Event::PifChanged { result } => match result {
                    Ok(()) => {
                        self.state.toasts.push(Toast::new(
                            "Physical interface updated.",
                            ToastKind::Success,
                        ));
                        self.state.pif_catalog_connection = None;
                        self.state.pif_catalog_loading = false;
                    }
                    Err(message) => self
                        .state
                        .toasts
                        .push(Toast::new(message, ToastKind::Error)),
                },
                Event::NetworkChanged { action, result } => match result {
                    Ok(()) => {
                        self.state.network_create_dialog = None;
                        self.state.network_destroy_confirm = None;
                        self.state.bond_create_dialog = None;
                        self.state.vlan_create_dialog = None;
                        self.state.pif_catalog_connection = None;
                        self.state.pif_catalog_loading = false;
                        self.state.toasts.push(Toast::new(
                            match action.as_str() {
                                "create" => "Network created.",
                                "bond_create" => "Bond created.",
                                "vlan_create" => "VLAN created.",
                                "sriov_create" => "SR-IOV network created.",
                                "chin_create" => "Cross-host network created.",
                                _ => "Network destroyed.",
                            },
                            ToastKind::Success,
                        ));
                    }
                    Err(message) => {
                        let mut handled = false;
                        if let Some(dialog) = self.state.network_create_dialog.as_mut() {
                            dialog.submitting = false;
                            dialog.error = Some(message.clone());
                            handled = true;
                        }
                        if let Some(dialog) = self.state.network_destroy_confirm.as_mut() {
                            dialog.submitting = false;
                            dialog.error = Some(message.clone());
                            handled = true;
                        }
                        if let Some(dialog) = self.state.bond_create_dialog.as_mut() {
                            dialog.submitting = false;
                            dialog.error = Some(message.clone());
                            handled = true;
                        }
                        if let Some(dialog) = self.state.vlan_create_dialog.as_mut() {
                            dialog.submitting = false;
                            dialog.error = Some(message.clone());
                            handled = true;
                        }
                        if !handled {
                            self.state
                                .toasts
                                .push(Toast::new(message, ToastKind::Error));
                        }
                    }
                },
                Event::SrActionFinished {
                    sr_uuid,
                    action,
                    result,
                } => match result {
                    Ok(()) => {
                        if self.state.sr_vdis_sr.as_deref() == Some(sr_uuid.as_str()) {
                            self.state.sr_vdis_sr = None;
                        }
                        if self
                            .state
                            .sr_action_confirm
                            .as_ref()
                            .is_some_and(|confirm| confirm.sr_uuid == sr_uuid)
                        {
                            self.state.sr_action_confirm = None;
                        }
                        self.state.toasts.push(Toast::new(
                            format!("{} completed.", action.title()),
                            ToastKind::Success,
                        ));
                    }
                    Err(message) => {
                        if let Some(confirm) = self.state.sr_action_confirm.as_mut()
                            && confirm.sr_uuid == sr_uuid
                        {
                            confirm.submitting = false;
                            confirm.error = Some(message);
                        } else {
                            self.state
                                .toasts
                                .push(Toast::new(message, ToastKind::Error));
                        }
                    }
                },
                Event::SrVdis { seq, result } => {
                    if seq == self.state.sr_vdis_seq {
                        self.state.sr_vdis_loading = false;
                        match result {
                            Ok(vdis) => {
                                self.state.sr_vdis = vdis;
                                self.state.sr_vdis_error = None;
                            }
                            Err(message) => {
                                self.state.sr_vdis = Vec::new();
                                self.state.sr_vdis_error = Some(message);
                            }
                        }
                    }
                }
                Event::VdiDeleted { vdi_uuid, result } => match result {
                    Ok(()) => {
                        if let Some(confirm) = self.state.vdi_delete_confirm.take() {
                            self.state.sr_vdis_seq += 1;
                            self.state.sr_vdis_loading = true;
                            self.state.sr_vdis_sr = Some(confirm.sr_uuid.clone());
                            self.worker.send(Command::SrVdis {
                                seq: self.state.sr_vdis_seq,
                                connection_id: confirm.connection_id,
                                sr_uuid: confirm.sr_uuid,
                            });
                            self.state.toasts.push(Toast::new(
                                format!("Deleted disk {}.", confirm.name),
                                ToastKind::Success,
                            ));
                        } else {
                            self.state
                                .toasts
                                .push(Toast::new("Virtual disk deleted.", ToastKind::Success));
                        }
                        let _ = vdi_uuid;
                    }
                    Err(message) => {
                        if let Some(confirm) = self.state.vdi_delete_confirm.as_mut() {
                            confirm.submitting = false;
                            confirm.error = Some(message);
                        } else {
                            self.state
                                .toasts
                                .push(Toast::new(message, ToastKind::Error));
                        }
                    }
                },
                Event::VmHaUpdated { vm_uuid, result } => match result {
                    Ok(()) => {
                        if self
                            .state
                            .ha_edit_dialog
                            .as_ref()
                            .is_some_and(|dialog| dialog.vm_uuid == vm_uuid)
                        {
                            self.state.ha_edit_dialog = None;
                        }
                        self.state
                            .toasts
                            .push(Toast::new("HA settings updated.", ToastKind::Success));
                        self.apply(vec![UiAction::ReloadVmDetails]);
                    }
                    Err(message) => {
                        if let Some(dialog) = self.state.ha_edit_dialog.as_mut()
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
                },
                Event::VdiCatalog { seq, result } => {
                    if seq == self.state.vdi_catalog_seq
                        && let Some(dialog) = self.state.attach_vdi_dialog.as_mut()
                    {
                        dialog.loading = false;
                        match result {
                            Ok(vdis) => {
                                if dialog.selected.is_none() {
                                    dialog.selected = vdis.first().map(|vdi| vdi.uuid.clone());
                                }
                                dialog.vdis = vdis;
                            }
                            Err(message) => dialog.error = Some(message),
                        }
                    }
                }
                Event::SnapshotsDeleted { vm_uuid, result } => match result {
                    Ok(count) => {
                        if self
                            .state
                            .snapshot_confirm
                            .as_ref()
                            .is_some_and(|confirm| confirm.vm_uuid == vm_uuid)
                        {
                            self.state.snapshot_confirm = None;
                        }
                        self.state.snapshot_selection.clear();
                        self.state.toasts.push(Toast::new(
                            format!(
                                "Deleted {count} snapshot{}.",
                                if count == 1 { "" } else { "s" }
                            ),
                            ToastKind::Success,
                        ));
                        self.apply(vec![UiAction::ReloadVmDetails]);
                    }
                    Err(message) => {
                        if let Some(confirm) = self.state.snapshot_confirm.as_mut()
                            && confirm.vm_uuid == vm_uuid
                        {
                            confirm.submitting = false;
                            confirm.error = Some(message);
                        } else {
                            self.state
                                .toasts
                                .push(Toast::new(message, ToastKind::Error));
                        }
                    }
                },
                Event::HostActionFinished {
                    host_uuid,
                    action,
                    result,
                } => match result {
                    Ok(()) => {
                        if self
                            .state
                            .host_action_confirm
                            .as_ref()
                            .is_some_and(|confirm| confirm.host_uuid == host_uuid)
                        {
                            self.state.host_action_confirm = None;
                        }
                        self.state.toasts.push(Toast::new(
                            format!("{} completed.", action.title()),
                            ToastKind::Success,
                        ));
                        if self
                            .state
                            .host_route
                            .as_ref()
                            .is_some_and(|route| route.host_uuid == host_uuid)
                        {
                            self.apply(vec![UiAction::ReloadHostDetails]);
                        }
                    }
                    Err(message) => {
                        if let Some(confirm) = self.state.host_action_confirm.as_mut()
                            && confirm.host_uuid == host_uuid
                        {
                            confirm.submitting = false;
                            confirm.error = Some(message);
                        } else {
                            self.state
                                .toasts
                                .push(Toast::new(message, ToastKind::Error));
                        }
                    }
                },
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
                Event::VmResourcesUpdated { vm_uuid, result } => {
                    if !self
                        .state
                        .vm_resource_edit
                        .as_ref()
                        .is_some_and(|edit| edit.vm_uuid == vm_uuid)
                    {
                        continue;
                    }
                    match result {
                        Ok(()) => {
                            self.state.vm_resource_edit = None;
                            self.state.toasts.push(Toast::new(
                                "Saved VM resource settings.",
                                ToastKind::Success,
                            ));
                            self.apply(vec![UiAction::ReloadVmDetails]);
                        }
                        Err(message) => {
                            if let Some(edit) = self.state.vm_resource_edit.as_mut() {
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
                        "edit" => "Network interface updated.",
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
                        "attach" => "Virtual disk attached.",
                        "bootable" => "Boot flag updated.",
                        "mode" => "Disk mode updated.",
                        "plug" => "Disk plugged.",
                        "unplug" => "Disk unplugged.",
                        "move" => "Disk moved.",
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
                            if self
                                .state
                                .attach_vdi_dialog
                                .as_ref()
                                .is_some_and(|dialog| dialog.vm_uuid == vm_uuid)
                            {
                                self.state.attach_vdi_dialog = None;
                            }
                            if self
                                .state
                                .move_disk_dialog
                                .as_ref()
                                .is_some_and(|dialog| dialog.vm_uuid == vm_uuid)
                            {
                                self.state.move_disk_dialog = None;
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
                            } else if let Some(dialog) = self.state.attach_vdi_dialog.as_mut()
                                && dialog.vm_uuid == vm_uuid
                            {
                                dialog.submitting = false;
                                dialog.error = Some(message);
                            } else if let Some(dialog) = self.state.move_disk_dialog.as_mut()
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
        let mut zoom_action = None;
        ctx.input_mut(|input| {
            if input.consume_key(egui::Modifiers::COMMAND, egui::Key::K) {
                focus_search = true;
            }
            if input.consume_key(egui::Modifiers::NONE, egui::Key::F5) {
                refresh = true;
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
                tree: self.state.tree_preferences.clone(),
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
                UiAction::OpenProperties {
                    connection_id,
                    target,
                } => {
                    self.state.properties_seq += 1;
                    self.state.properties = Some(crate::state::PropertiesDialog {
                        connection_id: connection_id.clone(),
                        target: target.clone(),
                        properties: LoadState::Loading,
                        original: None,
                        new_tag: String::new(),
                        submitting: false,
                        error: None,
                    });
                    self.worker.send(Command::ObjectProperties {
                        seq: self.state.properties_seq,
                        connection_id,
                        target,
                    });
                }
                UiAction::ReloadProperties => {
                    if let Some(dialog) = self.state.properties.as_mut()
                        && !dialog.submitting
                    {
                        self.state.properties_seq += 1;
                        dialog.properties = LoadState::Loading;
                        dialog.error = None;
                        self.worker.send(Command::ObjectProperties {
                            seq: self.state.properties_seq,
                            connection_id: dialog.connection_id.clone(),
                            target: dialog.target.clone(),
                        });
                    }
                }
                UiAction::CloseProperties => {
                    if !self
                        .state
                        .properties
                        .as_ref()
                        .is_some_and(|dialog| dialog.submitting)
                    {
                        self.state.properties = None;
                        self.state.properties_seq += 1;
                    }
                }
                UiAction::SaveProperties => {
                    if let Some(dialog) = self.state.properties.as_mut()
                        && !dialog.submitting
                        && let LoadState::Ready(properties) = &mut dialog.properties
                    {
                        let Some(original) = dialog.original.clone() else {
                            continue;
                        };
                        if !dialog.new_tag.trim().is_empty() {
                            properties.tags.push(std::mem::take(&mut dialog.new_tag));
                        }
                        dialog.submitting = true;
                        dialog.error = None;
                        self.worker.send(Command::SaveObjectProperties {
                            seq: self.state.properties_seq,
                            connection_id: dialog.connection_id.clone(),
                            properties: xen_core::properties::ObjectPropertiesEdit {
                                original,
                                edited: properties.clone(),
                            },
                        });
                    }
                }
                UiAction::OpenXvaExportMany(targets) => {
                    if let Some((connection_id, vm_uuid, _)) = targets.first() {
                        self.apply(vec![UiAction::OpenXvaExport {
                            connection_id: connection_id.clone(),
                            vm_uuid: vm_uuid.clone(),
                        }]);
                        if let Some(dialog) = self.state.xva_dialog.as_mut() {
                            dialog.export_targets = targets;
                        }
                    }
                }
                UiAction::OpenXvaExport {
                    connection_id,
                    vm_uuid,
                } => {
                    let name = find_vm(&self.workspace, &connection_id, &vm_uuid)
                        .map(|(_, v)| v.name.clone())
                        .unwrap_or_else(|| "VM".into());
                    let file_name: String = name
                        .chars()
                        .take(64)
                        .map(|c| {
                            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                                c
                            } else {
                                '_'
                            }
                        })
                        .collect();
                    let directory = std::env::var_os("HOME")
                        .map(std::path::PathBuf::from)
                        .unwrap_or_else(|| std::path::PathBuf::from("."));
                    self.state.xva_dialog = Some(crate::state::XvaDialog {
                        export_targets: Vec::new(),
                        connection_id,
                        vm_uuid: Some(vm_uuid),
                        path: directory
                            .join(format!("{file_name}.xva"))
                            .to_string_lossy()
                            .into_owned(),
                        name,
                        sr_uuid: None,
                        network_uuid: None,
                        error: None,
                        submitting: false,
                    });
                }
                UiAction::OpenXvaImport(connection_id) => {
                    self.state.xva_dialog = Some(crate::state::XvaDialog {
                        export_targets: Vec::new(),
                        connection_id,
                        vm_uuid: None,
                        path: String::new(),
                        name: "Imported VM".into(),
                        sr_uuid: None,
                        network_uuid: None,
                        error: None,
                        submitting: false,
                    });
                }
                UiAction::CloseXvaDialog => {
                    if self
                        .state
                        .xva_dialog
                        .as_ref()
                        .is_some_and(|d| !d.submitting)
                    {
                        self.state.xva_dialog = None;
                    }
                }
                UiAction::SubmitXvaTransfer => {
                    if let Some(dialog) = self.state.xva_dialog.as_mut()
                        && !dialog.submitting
                    {
                        if !dialog.export_targets.is_empty() {
                            let path = std::path::PathBuf::from(&dialog.path);
                            let directory =
                                path.parent().unwrap_or_else(|| std::path::Path::new("."));
                            let requests = dialog
                                .export_targets
                                .iter()
                                .enumerate()
                                .map(|(index, (id, uuid, name))| {
                                    let file_name: String = name
                                        .chars()
                                        .take(64)
                                        .map(|c| {
                                            if c.is_ascii_alphanumeric() || matches!(c, '-' | '_') {
                                                c
                                            } else {
                                                '_'
                                            }
                                        })
                                        .collect();
                                    let destination = if index == 0 {
                                        path.clone()
                                    } else {
                                        directory.join(format!("{file_name}-{uuid}.xva"))
                                    };
                                    (
                                        id.clone(),
                                        xen_core::xva::XvaRequest::Export {
                                            vm_uuid: uuid.clone(),
                                            path: destination,
                                        },
                                    )
                                })
                                .collect();
                            dialog.submitting = true;
                            dialog.error = None;
                            self.worker.send(Command::XvaBatch { requests });
                            continue;
                        }
                        let request = match &dialog.vm_uuid {
                            Some(vm_uuid) => xen_core::xva::XvaRequest::Export {
                                vm_uuid: vm_uuid.clone(),
                                path: dialog.path.clone().into(),
                            },
                            None => xen_core::xva::XvaRequest::Import {
                                path: dialog.path.clone().into(),
                                sr_uuid: dialog.sr_uuid.clone().unwrap_or_default(),
                                name: dialog.name.clone(),
                                network_uuid: dialog.network_uuid.clone(),
                            },
                        };
                        dialog.submitting = true;
                        dialog.error = None;
                        self.worker.send(Command::XvaTransfer {
                            connection_id: dialog.connection_id.clone(),
                            request,
                        });
                    }
                }
                UiAction::OpenNewVmWizard {
                    connection_id,
                    source,
                } => {
                    if !find_connection(&self.workspace, &connection_id)
                        .is_some_and(|connection| !crate::state::is_read_only(connection))
                    {
                        self.state.toasts.push(Toast::new(
                            "Connect with write access to create a VM.",
                            ToastKind::Error,
                        ));
                        continue;
                    }
                    self.state.new_vm = Some(crate::state::NewVmWizard {
                        connection_id,
                        draft: xen_core::provisioning::CreateVmRequest {
                            source: source.clone(),
                            name: String::new(),
                            description: String::new(),
                            vcpus_max: 1,
                            vcpus_at_startup: 1,
                            cores_per_socket: None,
                            memory_mib: 1024,
                            sr_uuid: String::new(),
                            new_disk_gib: Some(20),
                            network_uuid: None,
                            iso_uuid: None,
                            affinity_host_uuid: None,
                        },
                        step: 0,
                        template: LoadState::Idle,
                        error: None,
                        submitting: false,
                    });
                    self.apply(vec![UiAction::SelectNewVmSource(source)]);
                }
                UiAction::SelectNewVmSource(source) => {
                    if let Some(wizard) = self.state.new_vm.as_mut() {
                        wizard.draft.source = source.clone();
                        wizard.template = LoadState::Loading;
                        wizard.draft.iso_uuid = None;
                        wizard.error = None;
                        self.state.new_vm_seq += 1;
                        self.worker.send(Command::WizardTemplate {
                            seq: self.state.new_vm_seq,
                            connection_id: wizard.connection_id.clone(),
                            source_uuid: source.uuid().to_owned(),
                        });
                    }
                }
                UiAction::CloseNewVmWizard => {
                    if self
                        .state
                        .new_vm
                        .as_ref()
                        .is_some_and(|wizard| !wizard.submitting)
                    {
                        self.state.new_vm_seq += 1;
                        self.state.new_vm = None;
                    }
                }
                UiAction::CreateNewVm => {
                    if let Some(wizard) = self.state.new_vm.as_mut()
                        && !wizard.submitting
                    {
                        wizard.submitting = true;
                        wizard.error = None;
                        self.worker.send(Command::CreateVm {
                            connection_id: wizard.connection_id.clone(),
                            request: wizard.draft.clone(),
                        });
                    }
                }
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
                UiAction::SelectObjects(selection) => {
                    if let Some(last) = selection.last() {
                        let anchor = self.state.tree_anchor.clone();
                        self.open_object(last.key.clone(), Some(last.title.clone()));
                        self.state.tree_anchor = anchor;
                    }
                    self.state.tree_selection = selection;
                }
                UiAction::OpenObject { key, title } => {
                    self.open_object(key, Some(title));
                }
                UiAction::NavigateHistory(forward) => {
                    let index = if forward {
                        (self.state.history_index + 1).min(self.state.selection_history.len() - 1)
                    } else {
                        self.state.history_index.saturating_sub(1)
                    };
                    let key = self.state.selection_history[index].clone();
                    self.state.history_index = index;
                    self.state.navigating_history = true;
                    self.open_object(key, None);
                    self.state.navigating_history = false;
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
                    self.state.page_hosts.retain(|(cid, _), _| cid != &id);
                    self.state.busy.insert(id.clone());
                    self.worker.send(Command::Refresh { connection_id: id });
                }
                UiAction::Refresh(None) => {
                    self.state.page_hosts.clear();
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
                UiAction::Reconnect(id) => {
                    self.state.busy.insert(id.clone());
                    self.worker.send(Command::Reconnect { connection_id: id });
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
                    if matches!(kind, ActionKind::Clone | ActionKind::SnapshotToVm)
                        && targets.len() == 1
                        && let Some((connection, vm)) = targets.first()
                        && find_vm(&self.workspace, connection, vm).is_some_and(|(_, source)| {
                            match kind {
                                ActionKind::Clone => source.kind == "template",
                                ActionKind::SnapshotToVm => source.kind == "snapshot",
                                _ => false,
                            }
                        })
                    {
                        self.apply(vec![UiAction::OpenNewVmWizard {
                            connection_id: connection.clone(),
                            source: if kind == ActionKind::SnapshotToVm {
                                xen_core::provisioning::VmCreationSource::Snapshot(vm.clone())
                            } else {
                                xen_core::provisioning::VmCreationSource::Template(vm.clone())
                            },
                        }]);
                        continue;
                    }

                    let first_name = targets
                        .first()
                        .and_then(|(connection_id, vm_uuid)| {
                            find_vm(&self.workspace, connection_id, vm_uuid)
                        })
                        .map(|(_, vm)| vm.name.clone())
                        .unwrap_or_else(|| "VM".into());
                    let name = match kind {
                        ActionKind::Clone => format!("{first_name} copy"),
                        ActionKind::SnapshotToVm => format!("{first_name} VM"),
                        ActionKind::SnapshotToTemplate => format!("{first_name} template"),
                        ActionKind::Snapshot
                        | ActionKind::Checkpoint
                        | ActionKind::SnapshotWithQuiesce => {
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
                    self.worker.send(Command::ActionBatch {
                        targets: intent.targets,
                        action,
                    });
                    self.state.selected.clear();
                    self.open_object(ObjectKey::Tasks, None);
                    self.state.toasts.push(Toast::new(
                        format!(
                            "{count} task{} submitted. Track progress in Events.",
                            if count == 1 { "" } else { "s" }
                        ),
                        ToastKind::Success,
                    ));
                }
                UiAction::CloseIntent => self.state.intent = None,
                UiAction::CheckTask(id) => {
                    self.worker.send(Command::CheckTask { task_id: id });
                }
                UiAction::CancelExternalTask {
                    connection_id,
                    reference,
                } => {
                    self.worker.send(Command::CancelExternalTask {
                        connection_id,
                        reference,
                    });
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
                            host_uuid,
                        },
                        Some(host_name),
                    );
                }
                UiAction::CloseHostDetails => {
                    self.clear_selection();
                    self.state.host_route = None;
                    self.state.host_state = LoadState::Idle;
                    self.state.perf_state = LoadState::Idle;
                }
                UiAction::LoadPageHost {
                    connection_id,
                    host_uuid,
                } => {
                    self.state.page_hosts.insert(
                        (connection_id.clone(), host_uuid.clone()),
                        LoadState::Loading,
                    );
                    self.worker.send(Command::HostDetails {
                        seq: 0,
                        connection_id,
                        host_uuid,
                    });
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
                    self.state.object_tab = match tab {
                        crate::state::HostTab::General
                        | crate::state::HostTab::Updates
                        | crate::state::HostTab::Certificates
                        | crate::state::HostTab::Health => crate::state::VmTab::General,
                        crate::state::HostTab::Cpu => crate::state::VmTab::Memory,
                        crate::state::HostTab::Networking => crate::state::VmTab::Network,
                        crate::state::HostTab::Storage => crate::state::VmTab::Storage,
                        crate::state::HostTab::Hardware => crate::state::VmTab::Gpu,
                        crate::state::HostTab::Performance => crate::state::VmTab::Performance,
                    };
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
                        && let Some(key) = self.state.selection.as_ref().map(|tab| tab.key.clone())
                        && let Some((connection_id, vm_uuid)) = vm_key(&key)
                        && find_vm(&self.workspace, &connection_id, &vm_uuid)
                            .is_some_and(|(_, vm)| vm.kind == "vm" && vm.power_state == "Running")
                        && self.state.console.as_ref().is_none_or(|console| {
                            console.connection_id != connection_id || console.vm_uuid != vm_uuid
                        })
                    {
                        self.apply(vec![UiAction::OpenConsole(connection_id, vm_uuid)]);
                    }
                }
                UiAction::ReloadVmDetails => {
                    if let Some(key) = self.state.selection.as_ref().map(|tab| tab.key.clone())
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
                    if let Some(key) = self.state.selection.as_ref().map(|tab| tab.key.clone())
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
                UiAction::OpenVmCopy {
                    connection_id,
                    vm_uuid,
                } => {
                    let Some((connection, vm)) = find_vm(&self.workspace, &connection_id, &vm_uuid)
                    else {
                        continue;
                    };
                    if !crate::state::action_allowed(&self.workspace, &connection_id, vm, "copy")
                        || vm.kind == "snapshot"
                        || vm.power_state != "Halted"
                        || crate::state::is_read_only(connection)
                    {
                        continue;
                    }
                    self.state.vm_copy = Some(crate::state::VmCopyDialog {
                        connection_id,
                        vm_uuid,
                        request: xen_core::provisioning::CopyVmRequest {
                            name: format!("{} full copy", vm.name),
                            description: vm.description.clone(),
                            sr_uuid: String::new(),
                        },
                    });
                }
                UiAction::CloseVmCopy => self.state.vm_copy = None,
                UiAction::SubmitVmCopy => {
                    let Some(dialog) = self.state.vm_copy.take() else {
                        continue;
                    };
                    if dialog.request.name.trim().is_empty() || dialog.request.sr_uuid.is_empty() {
                        self.state.vm_copy = Some(dialog);
                        continue;
                    }
                    self.worker.send(Command::Action {
                        connection_id: dialog.connection_id,
                        vm_uuid: dialog.vm_uuid,
                        action: xen_core::operations::VmAction::Copy {
                            request: dialog.request,
                        },
                    });
                    self.open_object(ObjectKey::Tasks, None);
                }

                UiAction::OpenPoolOptions { connection_id } => {
                    let Some(inventory) = find_connection(&self.workspace, &connection_id)
                        .and_then(crate::state::connection_inventory)
                    else {
                        continue;
                    };
                    let uuid = inventory.pool_id.clone();
                    self.state.pool_options = Some(crate::state::PoolOptionsDialog {
                        connection_id,
                        uuid,
                        original: None,
                        data: LoadState::Loading,
                        error: None,
                        submitting: false,
                    });
                    self.apply(vec![UiAction::ReloadPoolOptions]);
                }
                UiAction::ReloadPoolOptions => {
                    let Some(dialog) = self.state.pool_options.as_mut() else {
                        continue;
                    };
                    if dialog.submitting {
                        continue;
                    }
                    dialog.original = None;
                    dialog.data = LoadState::Loading;
                    dialog.error = None;
                    self.state.pool_options_seq += 1;
                    self.worker.send(Command::PoolOptions {
                        seq: self.state.pool_options_seq,
                        connection_id: dialog.connection_id.clone(),
                        uuid: dialog.uuid.clone(),
                    });
                }
                UiAction::SavePoolOptions => {
                    let Some(dialog) = self.state.pool_options.as_mut() else {
                        continue;
                    };
                    if dialog.submitting {
                        continue;
                    }
                    let (Some(original), LoadState::Ready(edited)) =
                        (&dialog.original, &dialog.data)
                    else {
                        continue;
                    };
                    let edit = xen_core::pool_options::PoolOptionsEdit {
                        original: original.clone(),
                        edited: edited.clone(),
                    };
                    dialog.submitting = true;
                    dialog.error = None;
                    self.worker.send(Command::SavePoolOptions {
                        seq: self.state.pool_options_seq,
                        connection_id: dialog.connection_id.clone(),
                        edit,
                    });
                }
                UiAction::ClosePoolOptions => {
                    if !self
                        .state
                        .pool_options
                        .as_ref()
                        .is_some_and(|d| d.submitting)
                    {
                        self.state.pool_options = None;
                    }
                }
                UiAction::OpenVmResources => {
                    let Some(key) = self.state.selection.as_ref().map(|tab| tab.key.clone()) else {
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
                    self.state.vm_resource_edit = Some(crate::state::VmResourceEditState {
                        connection_id,
                        vm_uuid,
                        vcpus_max: details.vcpus_max.to_string(),
                        vcpus_at_startup: details.vcpus_at_startup.to_string(),
                        cpu_weight: details.cpu_weight.clone(),
                        original_cpu_weight: details.cpu_weight.clone(),
                        cores_per_socket: details.cores_per_socket.clone(),
                        original_cores_per_socket: details.cores_per_socket.clone(),
                        original_vcpus_max: details.vcpus_max,
                        max_cores_per_socket: details.max_cores_per_socket,
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
                        firmware: if details.firmware.is_empty() {
                            "bios".into()
                        } else {
                            details.firmware.clone()
                        },
                        secure_boot: details.secure_boot.eq_ignore_ascii_case("true"),
                        boot_policy: if details.hvm_boot_policy.is_empty() {
                            "BIOS order".into()
                        } else {
                            details.hvm_boot_policy.clone()
                        },
                        after_crash: if details.after_crash.is_empty() {
                            "restart".into()
                        } else {
                            details.after_crash.clone()
                        },
                        pv_args: details.pv_args.clone(),
                        original_pv_args: details.pv_args.clone(),
                        auto_poweron: details.auto_poweron,
                        original_auto_poweron: details.auto_poweron,
                        is_hvm: !details.hvm_boot_policy.is_empty(),
                        running: details.power_state != "Halted",
                        can_hotplug_vcpus: details.can_hotplug_vcpus,
                        current_vcpus: details.vcpus_at_startup,
                        error: None,
                        submitting: false,
                    });
                }
                UiAction::CloseVmResources => {
                    if !self
                        .state
                        .vm_resource_edit
                        .as_ref()
                        .is_some_and(|edit| edit.submitting)
                    {
                        self.state.vm_resource_edit = None;
                    }
                }
                UiAction::SaveVmResources => {
                    let Some(edit) = self.state.vm_resource_edit.as_mut() else {
                        continue;
                    };
                    let resources = match edit.parse_resources() {
                        Ok(resources) => resources,
                        Err(message) => {
                            edit.error = Some(message);
                            continue;
                        }
                    };
                    edit.submitting = true;
                    edit.error = None;
                    self.worker.send(Command::UpdateVmResources {
                        connection_id: edit.connection_id.clone(),
                        vm_uuid: edit.vm_uuid.clone(),
                        resources,
                    });
                }
                UiAction::SetDvdMedia { vbd_uuid, iso_uuid } => {
                    if let Some(key) = self.state.selection.as_ref().map(|s| &s.key)
                        && let Some((connection_id, vm_uuid)) = vm_key(key)
                    {
                        self.worker.send(Command::ChangeCd {
                            connection_id,
                            vm_uuid,
                            vbd_uuid,
                            iso_uuid,
                        });
                    }
                }
                UiAction::OpenIsoDialog { vbd_uuid, drive } => {
                    let Some(key) = self.state.selection.as_ref().map(|tab| tab.key.clone()) else {
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
                    let Some(key) = self.state.selection.as_ref().map(|tab| tab.key.clone()) else {
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
                UiAction::OpenDrTaskDialog => {
                    let Some(connection_id) = self
                        .state
                        .selection
                        .as_ref()
                        .and_then(|tab| tab.key.connection_id())
                        .map(str::to_owned)
                    else {
                        continue;
                    };
                    self.state.dr_task_dialog = Some(crate::state::DrTaskDialog {
                        connection_id,
                        sr_type: "nfs".into(),
                        server: String::new(),
                        serverpath: String::new(),
                        target: String::new(),
                        target_iqn: String::new(),
                        whitelist: String::new(),
                        acknowledged: false,
                        submitting: false,
                        error: None,
                    });
                }
                UiAction::CloseDrTaskDialog => {
                    if !self
                        .state
                        .dr_task_dialog
                        .as_ref()
                        .is_some_and(|dialog| dialog.submitting)
                    {
                        self.state.dr_task_dialog = None;
                    }
                }
                UiAction::SaveDrTaskDialog => {
                    let Some(dialog) = self.state.dr_task_dialog.as_mut() else {
                        continue;
                    };
                    if !dialog.acknowledged {
                        dialog.error = Some("Confirm that you understand this change.".into());
                        continue;
                    }
                    let mut config = Vec::new();
                    match dialog.sr_type.as_str() {
                        "nfs" => {
                            config.push(("server".into(), dialog.server.trim().to_owned()));
                            config.push(("serverpath".into(), dialog.serverpath.trim().to_owned()));
                        }
                        "lvmoiscsi" => {
                            config.push(("target".into(), dialog.target.trim().to_owned()));
                            config.push(("targetIQN".into(), dialog.target_iqn.trim().to_owned()));
                        }
                        _ => {}
                    }
                    let whitelist: Vec<String> = dialog
                        .whitelist
                        .split([',', '\n'])
                        .map(str::trim)
                        .filter(|uuid| !uuid.is_empty())
                        .map(str::to_owned)
                        .collect();
                    if let Err(error) =
                        xen_core::dr::validate_dr_task(&dialog.sr_type, &config, &whitelist)
                    {
                        dialog.error = Some(error.to_string());
                        continue;
                    }
                    dialog.submitting = true;
                    dialog.error = None;
                    self.worker.send(Command::CreateDrTask {
                        connection_id: dialog.connection_id.clone(),
                        sr_type: dialog.sr_type.clone(),
                        device_config: config,
                        whitelist,
                    });
                }
                UiAction::PromptDrTaskDestroy {
                    uuid,
                    introduced_srs,
                } => {
                    let Some(connection_id) = self
                        .state
                        .selection
                        .as_ref()
                        .and_then(|tab| tab.key.connection_id())
                        .map(str::to_owned)
                    else {
                        continue;
                    };
                    self.state.dr_task_delete_confirm = Some(crate::state::DrTaskDeleteConfirm {
                        connection_id,
                        uuid,
                        introduced_srs,
                        submitting: false,
                        error: None,
                    });
                }
                UiAction::CancelDrTaskDestroy => {
                    if !self
                        .state
                        .dr_task_delete_confirm
                        .as_ref()
                        .is_some_and(|confirm| confirm.submitting)
                    {
                        self.state.dr_task_delete_confirm = None;
                    }
                }
                UiAction::ConfirmDrTaskDestroy => {
                    let Some(confirm) = self.state.dr_task_delete_confirm.as_mut() else {
                        continue;
                    };
                    confirm.submitting = true;
                    confirm.error = None;
                    self.worker.send(Command::DestroyDrTask {
                        connection_id: confirm.connection_id.clone(),
                        uuid: confirm.uuid.clone(),
                    });
                }
                UiAction::OpenPvsSiteDialog => {
                    let Some(connection_id) = self
                        .state
                        .selection
                        .as_ref()
                        .and_then(|tab| tab.key.connection_id())
                        .map(str::to_owned)
                    else {
                        continue;
                    };
                    self.state.pvs_site_dialog = Some(crate::state::PvsSiteDialog {
                        connection_id,
                        name: String::new(),
                        description: String::new(),
                        submitting: false,
                        error: None,
                    });
                }
                UiAction::ClosePvsSiteDialog => {
                    if !self
                        .state
                        .pvs_site_dialog
                        .as_ref()
                        .is_some_and(|dialog| dialog.submitting)
                    {
                        self.state.pvs_site_dialog = None;
                    }
                }
                UiAction::SavePvsSiteDialog => {
                    let Some(dialog) = self.state.pvs_site_dialog.as_mut() else {
                        continue;
                    };
                    if dialog.name.trim().is_empty() {
                        dialog.error = Some("Enter a PVS site name.".into());
                        continue;
                    }
                    dialog.submitting = true;
                    dialog.error = None;
                    self.worker.send(Command::IntroducePvsSite {
                        connection_id: dialog.connection_id.clone(),
                        name: dialog.name.trim().to_owned(),
                        description: dialog.description.clone(),
                    });
                }
                UiAction::OpenPvsServerDialog => {
                    let Some(connection_id) = self
                        .state
                        .selection
                        .as_ref()
                        .and_then(|tab| tab.key.connection_id())
                        .map(str::to_owned)
                    else {
                        continue;
                    };
                    let sites = pvs_site_list(&self.state);
                    if sites.is_empty() {
                        self.state.toasts.push(Toast::new(
                            "Add a PVS site before adding servers.",
                            ToastKind::Info,
                        ));
                        continue;
                    }
                    self.state.pvs_server_dialog = Some(crate::state::PvsServerDialog {
                        connection_id,
                        site_uuid: sites.first().map(|(uuid, _)| uuid.clone()),
                        sites,
                        addresses: String::new(),
                        first_port: "6901".into(),
                        last_port: "6910".into(),
                        submitting: false,
                        error: None,
                    });
                }
                UiAction::ClosePvsServerDialog => {
                    if !self
                        .state
                        .pvs_server_dialog
                        .as_ref()
                        .is_some_and(|dialog| dialog.submitting)
                    {
                        self.state.pvs_server_dialog = None;
                    }
                }
                UiAction::SavePvsServerDialog => {
                    let Some(dialog) = self.state.pvs_server_dialog.as_mut() else {
                        continue;
                    };
                    let Some(site_uuid) = dialog.site_uuid.clone() else {
                        dialog.error = Some("Select a PVS site.".into());
                        continue;
                    };
                    let Some(first_port) = dialog.first_port.trim().parse::<u16>().ok() else {
                        dialog.error = Some("Enter a whole-number first port.".into());
                        continue;
                    };
                    let Some(last_port) = dialog.last_port.trim().parse::<u16>().ok() else {
                        dialog.error = Some("Enter a whole-number last port.".into());
                        continue;
                    };
                    let addresses: Vec<String> = dialog
                        .addresses
                        .split([',', '\n'])
                        .map(str::trim)
                        .filter(|address| !address.is_empty())
                        .map(str::to_owned)
                        .collect();
                    if let Err(error) = xen_core::pvs::validate_pvs_addresses(&addresses) {
                        dialog.error = Some(error.to_string());
                        continue;
                    }
                    if let Err(error) = xen_core::pvs::validate_pvs_ports(first_port, last_port) {
                        dialog.error = Some(error.to_string());
                        continue;
                    }
                    dialog.submitting = true;
                    dialog.error = None;
                    self.worker.send(Command::IntroducePvsServer {
                        connection_id: dialog.connection_id.clone(),
                        site_uuid,
                        addresses,
                        first_port,
                        last_port,
                    });
                }
                UiAction::OpenPvsCacheDialog => {
                    let Some(connection_id) = self
                        .state
                        .selection
                        .as_ref()
                        .and_then(|tab| tab.key.connection_id())
                        .map(str::to_owned)
                    else {
                        continue;
                    };
                    let sites = pvs_site_list(&self.state);
                    if sites.is_empty() {
                        self.state.toasts.push(Toast::new(
                            "Add a PVS site before adding cache storage.",
                            ToastKind::Info,
                        ));
                        continue;
                    }
                    let hosts: Vec<(String, String)> =
                        find_connection(&self.workspace, &connection_id)
                            .and_then(crate::state::connection_inventory)
                            .map(|inventory| {
                                inventory
                                    .hosts
                                    .iter()
                                    .map(|host| (host.uuid.clone(), host.name.clone()))
                                    .collect()
                            })
                            .unwrap_or_default();
                    let srs: Vec<(String, String)> =
                        find_connection(&self.workspace, &connection_id)
                            .and_then(crate::state::connection_inventory)
                            .map(|inventory| {
                                inventory
                                    .storage
                                    .iter()
                                    .filter(|sr| sr.content_type != "iso")
                                    .map(|sr| (sr.uuid.clone(), sr.name.clone()))
                                    .collect()
                            })
                            .unwrap_or_default();
                    if hosts.is_empty() || srs.is_empty() {
                        self.state.toasts.push(Toast::new(
                            "PVS cache storage needs at least one host and writable SR.",
                            ToastKind::Error,
                        ));
                        continue;
                    }
                    self.state.pvs_cache_dialog = Some(crate::state::PvsCacheDialog {
                        connection_id,
                        site_uuid: sites.first().map(|(uuid, _)| uuid.clone()),
                        sites,
                        host_uuid: hosts.first().map(|(uuid, _)| uuid.clone()),
                        hosts,
                        sr_uuid: srs.first().map(|(uuid, _)| uuid.clone()),
                        srs,
                        size_mib: "10240".into(),
                        submitting: false,
                        error: None,
                    });
                }
                UiAction::ClosePvsCacheDialog => {
                    if !self
                        .state
                        .pvs_cache_dialog
                        .as_ref()
                        .is_some_and(|dialog| dialog.submitting)
                    {
                        self.state.pvs_cache_dialog = None;
                    }
                }
                UiAction::SavePvsCacheDialog => {
                    let Some(dialog) = self.state.pvs_cache_dialog.as_mut() else {
                        continue;
                    };
                    let Some(site_uuid) = dialog.site_uuid.clone() else {
                        dialog.error = Some("Select a PVS site.".into());
                        continue;
                    };
                    let Some(host_uuid) = dialog.host_uuid.clone() else {
                        dialog.error = Some("Select a host.".into());
                        continue;
                    };
                    let Some(sr_uuid) = dialog.sr_uuid.clone() else {
                        dialog.error = Some("Select a cache SR.".into());
                        continue;
                    };
                    let Some(size_mib) = dialog.size_mib.trim().parse::<u64>().ok() else {
                        dialog.error = Some("Enter the cache size in MiB.".into());
                        continue;
                    };
                    if let Err(error) = xen_core::pvs::validate_pvs_cache_size(size_mib) {
                        dialog.error = Some(error.to_string());
                        continue;
                    }
                    dialog.submitting = true;
                    dialog.error = None;
                    self.worker.send(Command::CreatePvsCache {
                        connection_id: dialog.connection_id.clone(),
                        site_uuid,
                        host_uuid,
                        sr_uuid,
                        size_mib,
                    });
                }
                UiAction::OpenPvsProxyDialog => {
                    let Some(connection_id) = self
                        .state
                        .selection
                        .as_ref()
                        .and_then(|tab| tab.key.connection_id())
                        .map(str::to_owned)
                    else {
                        continue;
                    };
                    let sites = pvs_site_list(&self.state);
                    if sites.is_empty() {
                        self.state.toasts.push(Toast::new(
                            "Add a PVS site before enabling read caching.",
                            ToastKind::Info,
                        ));
                        continue;
                    }
                    self.state.pvs_vif_seq += 1;
                    let seq = self.state.pvs_vif_seq;
                    self.state.pvs_proxy_dialog = Some(crate::state::PvsProxyDialog {
                        connection_id: connection_id.clone(),
                        site_uuid: sites.first().map(|(uuid, _)| uuid.clone()),
                        sites,
                        vifs: Vec::new(),
                        vif_uuid: None,
                        loading: true,
                        submitting: false,
                        error: None,
                    });
                    self.worker
                        .send(Command::PvsVifCatalog { seq, connection_id });
                }
                UiAction::ClosePvsProxyDialog => {
                    if !self
                        .state
                        .pvs_proxy_dialog
                        .as_ref()
                        .is_some_and(|dialog| dialog.submitting)
                    {
                        self.state.pvs_proxy_dialog = None;
                    }
                }
                UiAction::SavePvsProxyDialog => {
                    let Some(dialog) = self.state.pvs_proxy_dialog.as_mut() else {
                        continue;
                    };
                    let Some(site_uuid) = dialog.site_uuid.clone() else {
                        dialog.error = Some("Select a PVS site.".into());
                        continue;
                    };
                    let Some(vif_uuid) = dialog.vif_uuid.clone() else {
                        dialog.error = Some("Select a connected VM interface.".into());
                        continue;
                    };
                    dialog.submitting = true;
                    dialog.error = None;
                    self.worker.send(Command::CreatePvsProxy {
                        connection_id: dialog.connection_id.clone(),
                        site_uuid,
                        vif_uuid,
                    });
                }
                UiAction::PromptPvsDelete { kind, uuid, name } => {
                    let Some(connection_id) = self
                        .state
                        .selection
                        .as_ref()
                        .and_then(|tab| tab.key.connection_id())
                        .map(str::to_owned)
                    else {
                        continue;
                    };
                    self.state.pvs_delete_confirm = Some(crate::state::PvsDeleteConfirm {
                        connection_id,
                        kind,
                        uuid,
                        name,
                        submitting: false,
                        error: None,
                    });
                }
                UiAction::CancelPvsDelete => {
                    if !self
                        .state
                        .pvs_delete_confirm
                        .as_ref()
                        .is_some_and(|confirm| confirm.submitting)
                    {
                        self.state.pvs_delete_confirm = None;
                    }
                }
                UiAction::ConfirmPvsDelete => {
                    let Some(confirm) = self.state.pvs_delete_confirm.as_mut() else {
                        continue;
                    };
                    confirm.submitting = true;
                    confirm.error = None;
                    let connection_id = confirm.connection_id.clone();
                    let uuid = confirm.uuid.clone();
                    let command = match confirm.kind {
                        crate::state::PvsDeleteKind::Site => Command::ForgetPvsSite {
                            connection_id,
                            uuid,
                        },
                        crate::state::PvsDeleteKind::Server => Command::ForgetPvsServer {
                            connection_id,
                            uuid,
                        },
                        crate::state::PvsDeleteKind::Proxy => Command::DestroyPvsProxy {
                            connection_id,
                            uuid,
                        },
                        crate::state::PvsDeleteKind::Cache => Command::DestroyPvsCache {
                            connection_id,
                            uuid,
                        },
                    };
                    self.worker.send(command);
                }
                UiAction::OpenVmssCreate => {
                    let Some(connection_id) = self
                        .state
                        .selection
                        .as_ref()
                        .and_then(|tab| tab.key.connection_id())
                        .map(str::to_owned)
                    else {
                        continue;
                    };
                    let vms: Vec<(String, String)> =
                        find_connection(&self.workspace, &connection_id)
                            .and_then(crate::state::connection_inventory)
                            .map(|inventory| {
                                inventory
                                    .vms
                                    .iter()
                                    .map(|vm| (vm.uuid.clone(), vm.name.clone()))
                                    .collect()
                            })
                            .unwrap_or_default();
                    self.state.vmss_dialog = Some(crate::state::VmssDialog {
                        connection_id,
                        uuid: None,
                        name: String::new(),
                        description: String::new(),
                        snapshot_type: "snapshot".into(),
                        frequency: "daily".into(),
                        minute: "0".into(),
                        hour: "2".into(),
                        days: HashSet::new(),
                        retained: "7".into(),
                        enabled: true,
                        vms,
                        selected_vms: HashSet::new(),
                        submitting: false,
                        error: None,
                    });
                }
                UiAction::OpenVmssEdit(uuid) => {
                    let Some(connection_id) = self
                        .state
                        .selection
                        .as_ref()
                        .and_then(|tab| tab.key.connection_id())
                        .map(str::to_owned)
                    else {
                        continue;
                    };
                    let LoadState::Ready(status) = &self.state.pool_status_state else {
                        continue;
                    };
                    let Some(policy) = status.vmss.iter().find(|policy| policy.uuid == uuid) else {
                        continue;
                    };
                    let vms: Vec<(String, String)> =
                        find_connection(&self.workspace, &connection_id)
                            .and_then(crate::state::connection_inventory)
                            .map(|inventory| {
                                inventory
                                    .vms
                                    .iter()
                                    .map(|vm| (vm.uuid.clone(), vm.name.clone()))
                                    .collect()
                            })
                            .unwrap_or_default();
                    let selected_vms: HashSet<String> = policy
                        .vms
                        .iter()
                        .filter_map(|name| {
                            vms.iter()
                                .find(|(_, candidate)| candidate == name)
                                .map(|(uuid, _)| uuid.clone())
                        })
                        .collect();
                    self.state.vmss_dialog = Some(crate::state::VmssDialog {
                        connection_id,
                        uuid: Some(policy.uuid.clone()),
                        name: policy.name.clone(),
                        description: policy.description.clone(),
                        snapshot_type: if policy.schedule_type.is_empty() {
                            "snapshot".into()
                        } else {
                            policy.schedule_type.clone()
                        },
                        frequency: if policy.frequency.is_empty() {
                            "daily".into()
                        } else {
                            policy.frequency.clone()
                        },
                        minute: policy.minute.to_string(),
                        hour: policy.hour.to_string(),
                        days: policy.days.iter().cloned().collect(),
                        retained: policy.retained_snapshots.to_string(),
                        enabled: policy.enabled,
                        vms,
                        selected_vms,
                        submitting: false,
                        error: None,
                    });
                }
                UiAction::CloseVmssDialog => {
                    if !self
                        .state
                        .vmss_dialog
                        .as_ref()
                        .is_some_and(|dialog| dialog.submitting)
                    {
                        self.state.vmss_dialog = None;
                    }
                }
                UiAction::SaveVmssDialog => {
                    let Some(dialog) = self.state.vmss_dialog.as_mut() else {
                        continue;
                    };
                    let Some(minute) = dialog.minute.trim().parse::<u8>().ok() else {
                        dialog.error = Some("Enter a whole-number minute (0–59).".into());
                        continue;
                    };
                    let Some(hour) = dialog.hour.trim().parse::<u8>().ok() else {
                        dialog.error = Some("Enter a whole-number hour (0–23).".into());
                        continue;
                    };
                    let Some(retained) = dialog.retained.trim().parse::<u64>().ok() else {
                        dialog.error = Some("Enter a whole-number snapshot retention.".into());
                        continue;
                    };
                    if dialog.frequency == "weekly" && dialog.days.is_empty() {
                        dialog.error =
                            Some("Select at least one day for a weekly schedule.".into());
                        continue;
                    }
                    if dialog.name.trim().is_empty() {
                        dialog.error = Some("Enter a schedule name.".into());
                        continue;
                    }
                    let schedule = xen_core::vmss::VmssSchedule {
                        frequency: dialog.frequency.clone(),
                        minute,
                        hour,
                        days: if dialog.frequency == "weekly" {
                            dialog.days.iter().cloned().collect()
                        } else {
                            Vec::new()
                        },
                    };
                    let connection_id = dialog.connection_id.clone();
                    let uuid = dialog.uuid.clone();
                    let name = dialog.name.trim().to_owned();
                    let description = dialog.description.clone();
                    let snapshot_type = dialog.snapshot_type.clone();
                    let enabled = dialog.enabled;
                    let vm_uuids: Vec<String> = dialog.selected_vms.iter().cloned().collect();
                    dialog.submitting = true;
                    dialog.error = None;
                    if let Some(uuid) = uuid {
                        self.worker.send(Command::UpdateVmss {
                            connection_id,
                            uuid,
                            name,
                            description,
                            snapshot_type,
                            schedule,
                            retained_snapshots: retained,
                            enabled,
                            vm_uuids,
                        });
                    } else {
                        self.worker.send(Command::CreateVmss {
                            connection_id,
                            name,
                            description,
                            snapshot_type,
                            schedule,
                            retained_snapshots: retained,
                            enabled,
                            vm_uuids,
                        });
                    }
                }
                UiAction::PromptVmssDelete { uuid, name } => {
                    let Some(connection_id) = self
                        .state
                        .selection
                        .as_ref()
                        .and_then(|tab| tab.key.connection_id())
                        .map(str::to_owned)
                    else {
                        continue;
                    };
                    self.state.vmss_delete_confirm = Some(crate::state::VmssDeleteConfirm {
                        connection_id,
                        uuid,
                        name,
                        submitting: false,
                        error: None,
                    });
                }
                UiAction::CancelVmssDelete => {
                    if !self
                        .state
                        .vmss_delete_confirm
                        .as_ref()
                        .is_some_and(|confirm| confirm.submitting)
                    {
                        self.state.vmss_delete_confirm = None;
                    }
                }
                UiAction::ConfirmVmssDelete => {
                    let Some(confirm) = self.state.vmss_delete_confirm.as_mut() else {
                        continue;
                    };
                    confirm.submitting = true;
                    confirm.error = None;
                    self.worker.send(Command::DeleteVmss {
                        connection_id: confirm.connection_id.clone(),
                        uuid: confirm.uuid.clone(),
                    });
                }
                UiAction::OpenHaEnable => {
                    let Some(connection_id) = self
                        .state
                        .selection
                        .as_ref()
                        .and_then(|tab| tab.key.connection_id())
                        .map(str::to_owned)
                    else {
                        continue;
                    };
                    let srs: Vec<(String, String)> =
                        find_connection(&self.workspace, &connection_id)
                            .and_then(crate::state::connection_inventory)
                            .map(|inventory| {
                                inventory
                                    .storage
                                    .iter()
                                    .filter(|sr| sr.shared && sr.content_type == "user")
                                    .map(|sr| (sr.uuid.clone(), sr.name.clone()))
                                    .collect()
                            })
                            .unwrap_or_default();
                    if srs.is_empty() {
                        self.state.toasts.push(Toast::new(
                            "HA needs at least one shared user SR for heartbeats.",
                            ToastKind::Error,
                        ));
                        continue;
                    }
                    self.state.ha_enable_dialog = Some(crate::state::HaEnableDialog {
                        connection_id,
                        srs,
                        selected: HashSet::new(),
                        failures: "0".into(),
                        acknowledged: false,
                        submitting: false,
                        error: None,
                    });
                }
                UiAction::CloseHaEnable => {
                    if !self
                        .state
                        .ha_enable_dialog
                        .as_ref()
                        .is_some_and(|dialog| dialog.submitting)
                    {
                        self.state.ha_enable_dialog = None;
                    }
                }
                UiAction::SaveHaEnable => {
                    let Some(dialog) = self.state.ha_enable_dialog.as_mut() else {
                        continue;
                    };
                    let Some(failures) = dialog.failures.trim().parse::<u64>().ok() else {
                        dialog.error = Some("Enter a whole-number failure tolerance.".into());
                        continue;
                    };
                    if dialog.selected.is_empty() {
                        dialog.error = Some("Select at least one heartbeat SR.".into());
                        continue;
                    }
                    if !dialog.acknowledged {
                        dialog.error = Some("Confirm that you understand this change.".into());
                        continue;
                    }
                    dialog.submitting = true;
                    dialog.error = None;
                    self.worker.send(Command::EnableHa {
                        connection_id: dialog.connection_id.clone(),
                        heartbeat_sr_uuids: dialog.selected.iter().cloned().collect(),
                        failures_to_tolerate: failures,
                    });
                }
                UiAction::PromptHaDisable => {
                    let Some(connection_id) = self
                        .state
                        .selection
                        .as_ref()
                        .and_then(|tab| tab.key.connection_id())
                        .map(str::to_owned)
                    else {
                        continue;
                    };
                    self.state.pool_confirm = Some(crate::state::PoolConfirm {
                        connection_id,
                        action: crate::state::PoolConfirmAction::DisableHa,
                        acknowledged: false,
                        submitting: false,
                        error: None,
                    });
                }
                UiAction::OpenWlbConfig => {
                    let Some(connection_id) = self
                        .state
                        .selection
                        .as_ref()
                        .and_then(|tab| tab.key.connection_id())
                        .map(str::to_owned)
                    else {
                        continue;
                    };
                    self.state.wlb_config_dialog = Some(crate::state::WlbConfigDialog {
                        connection_id,
                        url: String::new(),
                        wlb_username: String::new(),
                        wlb_password: String::new(),
                        xenserver_username: String::new(),
                        xenserver_password: String::new(),
                        acknowledged: false,
                        submitting: false,
                        error: None,
                    });
                }
                UiAction::CloseWlbConfig => {
                    if !self
                        .state
                        .wlb_config_dialog
                        .as_ref()
                        .is_some_and(|dialog| dialog.submitting)
                    {
                        self.state.wlb_config_dialog = None;
                    }
                }
                UiAction::SaveWlbConfig => {
                    let Some(dialog) = self.state.wlb_config_dialog.as_mut() else {
                        continue;
                    };
                    if !dialog.acknowledged {
                        dialog.error = Some("Confirm that you understand this change.".into());
                        continue;
                    }
                    if dialog.url.trim().is_empty()
                        || dialog.wlb_username.trim().is_empty()
                        || dialog.wlb_password.is_empty()
                        || dialog.xenserver_username.trim().is_empty()
                        || dialog.xenserver_password.is_empty()
                    {
                        dialog.error =
                            Some("Fill in all WLB and XenServer credential fields.".into());
                        continue;
                    }
                    dialog.submitting = true;
                    dialog.error = None;
                    self.worker.send(Command::ConfigureWlb {
                        connection_id: dialog.connection_id.clone(),
                        wlb_url: dialog.url.trim().to_owned(),
                        wlb_username: dialog.wlb_username.trim().to_owned(),
                        wlb_password: dialog.wlb_password.clone(),
                        xenserver_username: dialog.xenserver_username.trim().to_owned(),
                        xenserver_password: dialog.xenserver_password.clone(),
                    });
                }
                UiAction::PromptWlbToggle(enabled) => {
                    let Some(connection_id) = self
                        .state
                        .selection
                        .as_ref()
                        .and_then(|tab| tab.key.connection_id())
                        .map(str::to_owned)
                    else {
                        continue;
                    };
                    if enabled
                        && let LoadState::Ready(status) = &self.state.pool_status_state
                        && status.wlb.url.is_empty()
                    {
                        self.state.toasts.push(Toast::new(
                            "Configure the WLB server before enabling workload balancing.",
                            ToastKind::Error,
                        ));
                        continue;
                    }
                    self.state.pool_confirm = Some(crate::state::PoolConfirm {
                        connection_id,
                        action: if enabled {
                            crate::state::PoolConfirmAction::EnableWlb
                        } else {
                            crate::state::PoolConfirmAction::DisableWlb
                        },
                        acknowledged: false,
                        submitting: false,
                        error: None,
                    });
                }
                UiAction::PromptWlbDeconfigure => {
                    let Some(connection_id) = self
                        .state
                        .selection
                        .as_ref()
                        .and_then(|tab| tab.key.connection_id())
                        .map(str::to_owned)
                    else {
                        continue;
                    };
                    self.state.pool_confirm = Some(crate::state::PoolConfirm {
                        connection_id,
                        action: crate::state::PoolConfirmAction::DeconfigureWlb,
                        acknowledged: false,
                        submitting: false,
                        error: None,
                    });
                }
                UiAction::CancelPoolConfirm => {
                    if !self
                        .state
                        .pool_confirm
                        .as_ref()
                        .is_some_and(|confirm| confirm.submitting)
                    {
                        self.state.pool_confirm = None;
                    }
                }
                UiAction::ConfirmPoolConfirm => {
                    let Some(confirm) = self.state.pool_confirm.as_mut() else {
                        continue;
                    };
                    if !confirm.acknowledged {
                        confirm.error = Some("Confirm that you understand this change.".into());
                        continue;
                    }
                    confirm.submitting = true;
                    confirm.error = None;
                    match confirm.action {
                        crate::state::PoolConfirmAction::DisableHa => {
                            self.worker.send(Command::DisableHa {
                                connection_id: confirm.connection_id.clone(),
                            });
                        }
                        crate::state::PoolConfirmAction::EnableWlb => {
                            self.worker.send(Command::SetWlbEnabled {
                                connection_id: confirm.connection_id.clone(),
                                enabled: true,
                            });
                        }
                        crate::state::PoolConfirmAction::DisableWlb => {
                            self.worker.send(Command::SetWlbEnabled {
                                connection_id: confirm.connection_id.clone(),
                                enabled: false,
                            });
                        }
                        crate::state::PoolConfirmAction::DeconfigureWlb => {
                            self.worker.send(Command::DeconfigureWlb {
                                connection_id: confirm.connection_id.clone(),
                            });
                        }
                    }
                }
                UiAction::OpenRdpDialog => {
                    let Some(key) = self.state.selection.as_ref().map(|tab| tab.key.clone()) else {
                        continue;
                    };
                    let Some((connection_id, vm_uuid)) = vm_key(&key) else {
                        continue;
                    };
                    let host = self
                        .state
                        .vm_state
                        .ready()
                        .and_then(|details| details.guest_ips.first())
                        .cloned()
                        .unwrap_or_default();
                    self.state.rdp_credentials = Some(crate::state::RdpCredentialDialog {
                        connection_id,
                        vm_uuid,
                        host,
                        port: "3389".into(),
                        username: "Administrator".into(),
                        domain: String::new(),
                        password: String::new(),
                        error: None,
                    });
                }
                UiAction::CloseRdpDialog => self.state.rdp_credentials = None,
                UiAction::ConnectRdp => {
                    let Some(dialog) = self.state.rdp_credentials.as_mut() else {
                        continue;
                    };
                    let Some(port) = dialog.port.trim().parse::<u16>().ok() else {
                        dialog.error = Some("Enter a numeric RDP port.".into());
                        continue;
                    };
                    if let Err(error) = crate::rdp::validate_credentials(
                        &dialog.host,
                        port,
                        &dialog.username,
                        &dialog.password,
                    ) {
                        dialog.error = Some(error);
                        continue;
                    }
                    self.state.rdp_seq += 1;
                    let console_id = format!("rdp-{}-{}", self.state.rdp_seq, dialog.vm_uuid);
                    let connection_id = dialog.connection_id.clone();
                    let vm_uuid = dialog.vm_uuid.clone();
                    let host = dialog.host.trim().to_owned();
                    let username = dialog.username.trim().to_owned();
                    let domain = dialog.domain.trim().to_owned();
                    let password = std::mem::take(&mut dialog.password);
                    self.state.rdp_console = Some(crate::state::RdpConsoleState {
                        console_id: console_id.clone(),
                        connection_id,
                        vm_uuid,
                        host: host.clone(),
                        username: username.clone(),
                        status: crate::state::RdpStatus::Connecting,
                        message: String::new(),
                        width: 0,
                        height: 0,
                        pixels: Vec::new(),
                        texture: None,
                        dirty: false,
                        buttons: 0,
                    });
                    self.state.rdp_credentials = None;
                    self.worker.send(Command::OpenRdpConsole {
                        console_id,
                        host,
                        port,
                        username,
                        domain,
                        password,
                        width: 1280,
                        height: 800,
                    });
                }
                UiAction::RdpInput { console_id, input } => {
                    self.worker.send(Command::RdpInput { console_id, input });
                }
                UiAction::CloseRdpConsole => {
                    if let Some(console) = self.state.rdp_console.as_mut() {
                        self.worker.send(Command::CloseRdpConsole {
                            console_id: console.console_id.clone(),
                        });
                        console.status = crate::state::RdpStatus::Closed;
                        console.message = "Disconnected by user.".into();
                    }
                }
                UiAction::ClearRdpConsole => {
                    self.state.rdp_console = None;
                }
                UiAction::OpenSshDialog => {
                    let Some(key) = self.state.selection.as_ref().map(|tab| tab.key.clone()) else {
                        continue;
                    };
                    let Some((connection_id, vm_uuid)) = vm_key(&key) else {
                        continue;
                    };
                    let host = self
                        .state
                        .vm_state
                        .ready()
                        .and_then(|details| details.guest_ips.first())
                        .cloned()
                        .unwrap_or_default();
                    self.state.ssh_credentials = Some(crate::state::SshCredentialDialog {
                        connection_id,
                        vm_uuid,
                        host,
                        port: "22".into(),
                        username: "root".into(),
                        password: String::new(),
                        error: None,
                    });
                }
                UiAction::CloseSshDialog => self.state.ssh_credentials = None,
                UiAction::ConnectSsh => {
                    let Some(dialog) = self.state.ssh_credentials.as_mut() else {
                        continue;
                    };
                    let Some(port) = dialog.port.trim().parse::<u16>().ok() else {
                        dialog.error = Some("Enter a numeric SSH port.".into());
                        continue;
                    };
                    if let Err(error) = crate::ssh::validate_credentials(
                        &dialog.host,
                        port,
                        &dialog.username,
                        &dialog.password,
                    ) {
                        dialog.error = Some(error);
                        continue;
                    }
                    self.state.ssh_seq += 1;
                    let console_id = format!("ssh-{}-{}", self.state.ssh_seq, dialog.vm_uuid);
                    let connection_id = dialog.connection_id.clone();
                    let vm_uuid = dialog.vm_uuid.clone();
                    let host = dialog.host.trim().to_owned();
                    let username = dialog.username.trim().to_owned();
                    let password = std::mem::take(&mut dialog.password);
                    self.state.ssh_console = Some(crate::state::SshConsoleState {
                        console_id: console_id.clone(),
                        connection_id,
                        vm_uuid,
                        host: host.clone(),
                        username: username.clone(),
                        status: crate::state::SshStatus::Connecting,
                        message: String::new(),
                        buffer: crate::ssh::TerminalBuffer::new(5000),
                    });
                    self.state.ssh_credentials = None;
                    self.worker.send(Command::OpenSshConsole {
                        console_id,
                        host,
                        port,
                        username,
                        password,
                    });
                }
                UiAction::SshInput { console_id, data } => {
                    self.worker.send(Command::SshInput {
                        console_id,
                        input: crate::ssh::SshInput::Data(data),
                    });
                }
                UiAction::CloseSshConsole => {
                    if let Some(console) = self.state.ssh_console.as_mut() {
                        self.worker.send(Command::CloseSshConsole {
                            console_id: console.console_id.clone(),
                        });
                        console.status = crate::state::SshStatus::Closed;
                        console.message = "Disconnected by user.".into();
                    }
                }
                UiAction::ClearSshConsole => {
                    if let Some(console) = self.state.ssh_console.as_mut() {
                        console.buffer.clear();
                    }
                }
                UiAction::OpenAdEnable => {
                    let Some(connection_id) = self
                        .state
                        .selection
                        .as_ref()
                        .and_then(|tab| tab.key.connection_id())
                        .map(str::to_owned)
                    else {
                        continue;
                    };
                    self.state.ad_enable_dialog = Some(crate::state::AdEnableDialog {
                        connection_id,
                        domain: String::new(),
                        user: String::new(),
                        password: String::new(),
                        acknowledged: false,
                        submitting: false,
                        error: None,
                    });
                }
                UiAction::CloseAdEnable => {
                    if !self
                        .state
                        .ad_enable_dialog
                        .as_ref()
                        .is_some_and(|dialog| dialog.submitting)
                    {
                        self.state.ad_enable_dialog = None;
                    }
                }
                UiAction::SaveAdEnable => {
                    let Some(dialog) = self.state.ad_enable_dialog.as_mut() else {
                        continue;
                    };
                    if !dialog.acknowledged {
                        dialog.error = Some("Confirm that you understand this change.".into());
                        continue;
                    }
                    if dialog.domain.trim().is_empty()
                        || dialog.user.trim().is_empty()
                        || dialog.password.is_empty()
                    {
                        dialog.error =
                            Some("Enter the domain, service account user and password.".into());
                        continue;
                    }
                    dialog.submitting = true;
                    dialog.error = None;
                    self.worker.send(Command::EnableAd {
                        connection_id: dialog.connection_id.clone(),
                        domain: dialog.domain.trim().to_owned(),
                        user: dialog.user.trim().to_owned(),
                        password: dialog.password.clone(),
                    });
                }
                UiAction::OpenAdDisable => {
                    let Some(connection_id) = self
                        .state
                        .selection
                        .as_ref()
                        .and_then(|tab| tab.key.connection_id())
                        .map(str::to_owned)
                    else {
                        continue;
                    };
                    self.state.ad_disable_dialog = Some(crate::state::AdDisableDialog {
                        connection_id,
                        user: String::new(),
                        password: String::new(),
                        disable_machine_account: false,
                        acknowledged: false,
                        submitting: false,
                        error: None,
                    });
                }
                UiAction::CloseAdDisable => {
                    if !self
                        .state
                        .ad_disable_dialog
                        .as_ref()
                        .is_some_and(|dialog| dialog.submitting)
                    {
                        self.state.ad_disable_dialog = None;
                    }
                }
                UiAction::SaveAdDisable => {
                    let Some(dialog) = self.state.ad_disable_dialog.as_mut() else {
                        continue;
                    };
                    if !dialog.acknowledged {
                        dialog.error = Some("Confirm that you understand this change.".into());
                        continue;
                    }
                    if dialog.disable_machine_account
                        && (dialog.user.trim().is_empty() || dialog.password.is_empty())
                    {
                        dialog.error = Some(
                            "Enter the service account user and password, or keep the machine account."
                                .into(),
                        );
                        continue;
                    }
                    dialog.submitting = true;
                    dialog.error = None;
                    self.worker.send(Command::DisableAd {
                        connection_id: dialog.connection_id.clone(),
                        user: dialog
                            .disable_machine_account
                            .then(|| dialog.user.trim().to_owned()),
                        password: dialog
                            .disable_machine_account
                            .then(|| dialog.password.clone()),
                    });
                }
                UiAction::OpenUserRoles(subject_uuid) => {
                    let Some(connection_id) = self
                        .state
                        .selection
                        .as_ref()
                        .and_then(|tab| tab.key.connection_id())
                        .map(str::to_owned)
                    else {
                        continue;
                    };
                    let LoadState::Ready(info) = &self.state.auth_state else {
                        continue;
                    };
                    let Some(user) = info.users.iter().find(|user| user.uuid == subject_uuid)
                    else {
                        continue;
                    };
                    if user.local_root {
                        continue;
                    }
                    let selected: HashSet<String> = user
                        .roles
                        .iter()
                        .filter_map(|name| {
                            info.roles
                                .iter()
                                .find(|role| role.name == *name || role.uuid == *name)
                                .map(|role| role.uuid.clone())
                        })
                        .collect();
                    self.state.user_role_dialog = Some(crate::state::UserRoleDialog {
                        connection_id,
                        subject_uuid,
                        identifier: user.identifier.clone(),
                        display_name: user.display_name.clone(),
                        selected,
                        submitting: false,
                        error: None,
                    });
                }
                UiAction::CloseUserRoles => {
                    if !self
                        .state
                        .user_role_dialog
                        .as_ref()
                        .is_some_and(|dialog| dialog.submitting)
                    {
                        self.state.user_role_dialog = None;
                    }
                }
                UiAction::SaveUserRoles => {
                    let Some(dialog) = self.state.user_role_dialog.as_mut() else {
                        continue;
                    };
                    if dialog.selected.is_empty() {
                        dialog.error = Some("A subject must keep at least one role.".into());
                        continue;
                    }
                    dialog.submitting = true;
                    dialog.error = None;
                    self.worker.send(Command::SetSubjectRoles {
                        connection_id: dialog.connection_id.clone(),
                        subject_uuid: dialog.subject_uuid.clone(),
                        roles: dialog.selected.iter().cloned().collect(),
                    });
                }
                UiAction::LoadPoolStatus => {
                    let Some(connection_id) = self
                        .state
                        .selection
                        .as_ref()
                        .and_then(|tab| tab.key.connection_id())
                        .map(str::to_owned)
                    else {
                        continue;
                    };
                    self.state.pool_status_seq += 1;
                    self.state.pool_status_state = LoadState::Loading;
                    self.worker.send(Command::PoolStatus {
                        seq: self.state.pool_status_seq,
                        connection_id,
                    });
                }
                UiAction::LoadAuth => {
                    let Some(connection_id) = self
                        .state
                        .selection
                        .as_ref()
                        .and_then(|tab| tab.key.connection_id())
                        .map(str::to_owned)
                    else {
                        continue;
                    };
                    self.state.auth_seq += 1;
                    self.state.auth_state = LoadState::Loading;
                    self.worker.send(Command::AuthInfo {
                        seq: self.state.auth_seq,
                        connection_id,
                    });
                }
                UiAction::LoadPbdCatalog {
                    seq,
                    connection_id,
                    sr_uuid,
                } => {
                    self.worker.send(Command::PbdCatalog {
                        seq,
                        connection_id,
                        sr_uuid,
                    });
                }
                UiAction::SetPbdPlugged {
                    connection_id,
                    pbd_uuid,
                    plugged,
                } => {
                    self.worker.send(Command::SetPbdPlugged {
                        connection_id,
                        pbd_uuid,
                        plugged,
                    });
                }
                UiAction::RepairSr => {
                    let Some(ObjectKey::Storage {
                        connection_id,
                        sr_uuid,
                    }) = self.state.selection.as_ref().map(|tab| tab.key.clone())
                    else {
                        continue;
                    };
                    self.worker.send(Command::RepairSr {
                        connection_id,
                        sr_uuid,
                    });
                }
                UiAction::OpenSrProbe => {
                    let Some(connection_id) = self.state.scope.connection_id().map(str::to_owned)
                    else {
                        self.state.toasts.push(Toast::new(
                            "Select a pool before probing storage.",
                            ToastKind::Info,
                        ));
                        continue;
                    };
                    let hosts: Vec<(String, String)> =
                        find_connection(&self.workspace, &connection_id)
                            .and_then(crate::state::connection_inventory)
                            .map(|inventory| {
                                inventory
                                    .hosts
                                    .iter()
                                    .map(|host| (host.uuid.clone(), host.name.clone()))
                                    .collect()
                            })
                            .unwrap_or_default();
                    if hosts.is_empty() {
                        continue;
                    }
                    self.state.sr_probe_dialog = Some(crate::state::SrProbeDialog {
                        connection_id,
                        host_uuid: hosts.first().map(|(uuid, _)| uuid.clone()),
                        hosts,
                        location: String::new(),
                        submitting: false,
                        error: None,
                        result: None,
                    });
                }
                UiAction::CloseSrProbe => {
                    if !self
                        .state
                        .sr_probe_dialog
                        .as_ref()
                        .is_some_and(|dialog| dialog.submitting)
                    {
                        self.state.sr_probe_dialog = None;
                    }
                }
                UiAction::SaveSrProbe => {
                    let Some(dialog) = self.state.sr_probe_dialog.as_mut() else {
                        continue;
                    };
                    let Some(host_uuid) = dialog.host_uuid.clone() else {
                        dialog.error = Some("Select a host.".into());
                        continue;
                    };
                    if dialog.location.trim().is_empty() {
                        dialog.error = Some("Enter a host path to probe.".into());
                        continue;
                    }
                    self.state.sr_probe_seq += 1;
                    dialog.submitting = true;
                    dialog.error = None;
                    dialog.result = None;
                    self.worker.send(Command::ProbeSr {
                        seq: self.state.sr_probe_seq,
                        connection_id: dialog.connection_id.clone(),
                        host_uuid,
                        location: dialog.location.trim().to_owned(),
                    });
                }
                UiAction::LoadPifCatalog { seq, connection_id } => {
                    self.worker.send(Command::PifCatalog { seq, connection_id });
                }
                UiAction::SetPifPlugged {
                    connection_id,
                    pif_uuid,
                    plugged,
                } => {
                    self.worker.send(Command::SetPifPlugged {
                        connection_id,
                        pif_uuid,
                        plugged,
                    });
                }
                UiAction::OpenBondCreate => {
                    let Some(connection_id) = self.state.scope.connection_id().map(str::to_owned)
                    else {
                        self.state.toasts.push(Toast::new(
                            "Select a pool before creating a bond.",
                            ToastKind::Info,
                        ));
                        continue;
                    };
                    if !self
                        .state
                        .pif_catalog
                        .iter()
                        .any(|pif| pif.bond_candidate())
                    {
                        self.state.toasts.push(Toast::new(
                            "No spare physical interfaces are available for a bond.",
                            ToastKind::Info,
                        ));
                        continue;
                    }
                    self.state.bond_create_dialog = Some(crate::state::BondCreateDialog {
                        connection_id,
                        name: "bond0".into(),
                        mode: "balance-slb".into(),
                        mtu: "1500".into(),
                        autoplug: true,
                        members: HashSet::new(),
                        submitting: false,
                        error: None,
                    });
                }
                UiAction::CloseBondCreate => {
                    if !self
                        .state
                        .bond_create_dialog
                        .as_ref()
                        .is_some_and(|dialog| dialog.submitting)
                    {
                        self.state.bond_create_dialog = None;
                    }
                }
                UiAction::SaveBondCreate => {
                    let Some(dialog) = self.state.bond_create_dialog.as_mut() else {
                        continue;
                    };
                    let Some(mtu) = dialog.mtu.trim().parse::<u64>().ok() else {
                        dialog.error = Some("Enter a whole-number MTU.".into());
                        continue;
                    };
                    if dialog.members.len() < 2 {
                        dialog.error = Some("Select at least two interfaces.".into());
                        continue;
                    }
                    dialog.submitting = true;
                    dialog.error = None;
                    self.worker.send(Command::CreateBond {
                        connection_id: dialog.connection_id.clone(),
                        name: dialog.name.trim().to_owned(),
                        members: dialog.members.iter().cloned().collect(),
                        mode: dialog.mode.clone(),
                        mtu,
                        autoplug: dialog.autoplug,
                    });
                }
                UiAction::OpenVlanCreate => {
                    let Some(connection_id) = self.state.scope.connection_id().map(str::to_owned)
                    else {
                        self.state.toasts.push(Toast::new(
                            "Select a pool before creating a VLAN.",
                            ToastKind::Info,
                        ));
                        continue;
                    };
                    let parent = self
                        .state
                        .pif_catalog
                        .iter()
                        .find(|pif| pif.vlan_candidate())
                        .map(|pif| pif.uuid.clone());
                    self.state.vlan_create_dialog = Some(crate::state::VlanCreateDialog {
                        connection_id,
                        pif_uuid: parent,
                        tag: "100".into(),
                        name: "VLAN 100".into(),
                        description: String::new(),
                        mtu: "1500".into(),
                        autoplug: true,
                        submitting: false,
                        error: None,
                    });
                }
                UiAction::CloseVlanCreate => {
                    if !self
                        .state
                        .vlan_create_dialog
                        .as_ref()
                        .is_some_and(|dialog| dialog.submitting)
                    {
                        self.state.vlan_create_dialog = None;
                    }
                }
                UiAction::SaveVlanCreate => {
                    let Some(dialog) = self.state.vlan_create_dialog.as_mut() else {
                        continue;
                    };
                    let Some(tag) = dialog.tag.trim().parse::<i64>().ok() else {
                        dialog.error = Some("Enter a whole-number VLAN tag.".into());
                        continue;
                    };
                    let Some(mtu) = dialog.mtu.trim().parse::<u64>().ok() else {
                        dialog.error = Some("Enter a whole-number MTU.".into());
                        continue;
                    };
                    let Some(pif_uuid) = dialog.pif_uuid.clone() else {
                        dialog.error = Some("Select a parent interface.".into());
                        continue;
                    };
                    dialog.submitting = true;
                    dialog.error = None;
                    self.worker.send(Command::CreateVlan {
                        connection_id: dialog.connection_id.clone(),
                        pif_uuid,
                        tag,
                        name: dialog.name.trim().to_owned(),
                        description: dialog.description.clone(),
                        mtu,
                        autoplug: dialog.autoplug,
                    });
                }
                UiAction::OpenNetworkChooser => {
                    let Some(connection_id) = self.state.scope.connection_id().map(str::to_owned)
                    else {
                        continue;
                    };
                    if !find_connection(&self.workspace, &connection_id).is_some_and(|connection| {
                        crate::state::is_online(connection)
                            && !crate::state::is_read_only(connection)
                    }) {
                        continue;
                    }
                    let connection_id_for_pifs = connection_id.clone();
                    self.state.pif_catalog = Vec::new();
                    self.state.pif_catalog_error = None;
                    self.state.pif_catalog_connection = Some(connection_id_for_pifs.clone());
                    self.state.pif_catalog_loading = true;
                    self.state.pif_catalog_seq += 1;
                    self.worker.send(Command::PifCatalog {
                        seq: self.state.pif_catalog_seq,
                        connection_id: connection_id_for_pifs,
                    });
                    self.state.network_chooser = Some(false);
                }
                UiAction::CloseNetworkChooser => self.state.network_chooser = None,
                UiAction::OpenNetworkCreate => {
                    let Some(connection_id) = self.state.scope.connection_id().map(str::to_owned)
                    else {
                        self.state.toasts.push(Toast::new(
                            "Select a pool in the Infrastructure tree before adding a network.",
                            ToastKind::Info,
                        ));
                        continue;
                    };
                    let connection_id_for_pifs = connection_id.clone();
                    self.state.pif_catalog = Vec::new();
                    self.state.pif_catalog_error = None;
                    self.state.pif_catalog_connection = Some(connection_id_for_pifs.clone());
                    self.state.pif_catalog_loading = true;
                    self.state.pif_catalog_seq += 1;
                    self.worker.send(Command::PifCatalog {
                        seq: self.state.pif_catalog_seq,
                        connection_id: connection_id_for_pifs,
                    });
                    self.state.network_create_dialog = Some(crate::state::NetworkCreateDialog {
                        connection_id,
                        kind: "internal".into(),
                        name: String::new(),
                        description: String::new(),
                        mtu: "1500".into(),
                        pif_uuid: None,
                        protocol: "gre".into(),
                        autoplug: true,
                        submitting: false,
                        error: None,
                    });
                }
                UiAction::CloseNetworkCreate => {
                    if !self
                        .state
                        .network_create_dialog
                        .as_ref()
                        .is_some_and(|dialog| dialog.submitting)
                    {
                        self.state.network_create_dialog = None;
                    }
                }
                UiAction::SaveNetwork => {
                    let Some(dialog) = self.state.network_create_dialog.as_mut() else {
                        continue;
                    };
                    if dialog.name.trim().is_empty() {
                        dialog.error = Some("Enter a network name.".into());
                        continue;
                    }
                    let Some(mtu) = dialog.mtu.trim().parse::<u64>().ok() else {
                        dialog.error = Some("Enter a whole-number MTU.".into());
                        continue;
                    };
                    if !(1500..=9000).contains(&mtu) {
                        dialog.error = Some("MTU must be between 1500 and 9000.".into());
                        continue;
                    }
                    if dialog.kind != "internal" && dialog.pif_uuid.is_none() {
                        dialog.error = Some("Select a physical interface.".into());
                        continue;
                    }
                    let pif_uuids: Vec<String> = dialog.pif_uuid.iter().cloned().collect();
                    if dialog.kind == "sriov"
                        && let Err(error) = xen_core::network::validate_sriov_network(
                            &dialog.name,
                            &dialog.description,
                            mtu,
                            &pif_uuids,
                        )
                    {
                        dialog.error = Some(error.to_string());
                        continue;
                    }
                    if dialog.kind == "chin"
                        && let Err(error) = xen_core::network::validate_chin_network(
                            &dialog.name,
                            &dialog.description,
                            mtu,
                            &dialog.protocol,
                            &pif_uuids,
                        )
                    {
                        dialog.error = Some(error.to_string());
                        continue;
                    }
                    dialog.submitting = true;
                    dialog.error = None;
                    let connection_id = dialog.connection_id.clone();
                    let name = dialog.name.trim().to_owned();
                    let description = dialog.description.clone();
                    match dialog.kind.as_str() {
                        "sriov" => {
                            self.worker.send(Command::CreateSriovNetwork {
                                connection_id,
                                name,
                                description,
                                mtu,
                                autoplug: dialog.autoplug,
                                pif_uuids,
                            });
                        }
                        "chin" => {
                            self.worker.send(Command::CreateChinNetwork {
                                connection_id,
                                name,
                                description,
                                mtu,
                                autoplug: dialog.autoplug,
                                protocol: dialog.protocol.clone(),
                                pif_uuids,
                            });
                        }
                        _ => {
                            self.worker.send(Command::CreateNetwork {
                                connection_id,
                                name,
                                description,
                                mtu,
                            });
                        }
                    }
                }
                UiAction::PromptDestroyNetwork {
                    connection_id,
                    network_uuid,
                    name,
                } => {
                    self.state.network_destroy_confirm =
                        Some(crate::state::NetworkDestroyConfirm {
                            connection_id,
                            network_uuid,
                            name,
                            acknowledged: false,
                            submitting: false,
                            error: None,
                        });
                }
                UiAction::CancelNetworkDestroy => {
                    if !self
                        .state
                        .network_destroy_confirm
                        .as_ref()
                        .is_some_and(|dialog| dialog.submitting)
                    {
                        self.state.network_destroy_confirm = None;
                    }
                }
                UiAction::ConfirmNetworkDestroy => {
                    let Some(dialog) = self.state.network_destroy_confirm.as_mut() else {
                        continue;
                    };
                    if !dialog.acknowledged {
                        dialog.error = Some("Confirm that you understand this action.".into());
                        continue;
                    }
                    dialog.submitting = true;
                    dialog.error = None;
                    self.worker.send(Command::DestroyNetwork {
                        connection_id: dialog.connection_id.clone(),
                        network_uuid: dialog.network_uuid.clone(),
                    });
                }
                UiAction::OpenSrIntroduce => {
                    let Some(connection_id) = self.state.scope.connection_id().map(str::to_owned)
                    else {
                        self.state.toasts.push(Toast::new(
                            "Select a pool before introducing storage.",
                            ToastKind::Info,
                        ));
                        continue;
                    };
                    self.state.sr_introduce_dialog = Some(crate::state::SrIntroduceDialog {
                        connection_id,
                        uuid: String::new(),
                        name: String::new(),
                        description: String::new(),
                        submitting: false,
                        error: None,
                    });
                }
                UiAction::CloseSrIntroduce => {
                    if !self
                        .state
                        .sr_introduce_dialog
                        .as_ref()
                        .is_some_and(|dialog| dialog.submitting)
                    {
                        self.state.sr_introduce_dialog = None;
                    }
                }
                UiAction::SaveSrIntroduce => {
                    let Some(dialog) = self.state.sr_introduce_dialog.as_mut() else {
                        continue;
                    };
                    if dialog.uuid.trim().is_empty() || dialog.name.trim().is_empty() {
                        dialog.error = Some("Enter the UUID and name.".into());
                        continue;
                    }
                    dialog.submitting = true;
                    dialog.error = None;
                    self.worker.send(Command::IntroduceSr {
                        connection_id: dialog.connection_id.clone(),
                        uuid: dialog.uuid.trim().to_owned(),
                        name: dialog.name.trim().to_owned(),
                        description: dialog.description.clone(),
                    });
                }
                UiAction::OpenSrCreate => {
                    let Some(connection_id) = self.state.scope.connection_id().map(str::to_owned)
                    else {
                        self.state.toasts.push(Toast::new(
                            "Select a pool before creating a storage repository.",
                            ToastKind::Info,
                        ));
                        continue;
                    };
                    let hosts: Vec<(String, String)> =
                        find_connection(&self.workspace, &connection_id)
                            .and_then(crate::state::connection_inventory)
                            .map(|inventory| {
                                inventory
                                    .hosts
                                    .iter()
                                    .map(|host| (host.uuid.clone(), host.name.clone()))
                                    .collect()
                            })
                            .unwrap_or_default();
                    if hosts.is_empty() {
                        self.state.toasts.push(Toast::new(
                            "No host is available for the new storage repository.",
                            ToastKind::Error,
                        ));
                        continue;
                    }
                    self.state.sr_create_dialog = Some(crate::state::SrCreateDialog {
                        connection_id,
                        host_uuid: hosts.first().map(|(uuid, _)| uuid.clone()),
                        hosts,
                        name: String::new(),
                        description: String::new(),
                        location: String::new(),
                        submitting: false,
                        error: None,
                    });
                }
                UiAction::CloseSrCreate => {
                    if !self
                        .state
                        .sr_create_dialog
                        .as_ref()
                        .is_some_and(|dialog| dialog.submitting)
                    {
                        self.state.sr_create_dialog = None;
                    }
                }
                UiAction::SaveSrCreate => {
                    let Some(dialog) = self.state.sr_create_dialog.as_mut() else {
                        continue;
                    };
                    if dialog.name.trim().is_empty() {
                        dialog.error = Some("Enter a storage repository name.".into());
                        continue;
                    }
                    let Some(host_uuid) = dialog.host_uuid.clone() else {
                        dialog.error = Some("Select a host.".into());
                        continue;
                    };
                    dialog.submitting = true;
                    dialog.error = None;
                    self.worker.send(Command::CreateSr {
                        connection_id: dialog.connection_id.clone(),
                        host_uuid,
                        name: dialog.name.trim().to_owned(),
                        description: dialog.description.clone(),
                        location: dialog.location.trim().to_owned(),
                    });
                }
                UiAction::OpenSrReattach => {
                    let Some(ObjectKey::Storage {
                        connection_id,
                        sr_uuid,
                    }) = self.state.selection.as_ref().map(|tab| tab.key.clone())
                    else {
                        continue;
                    };
                    let Some(sr) = find_connection(&self.workspace, &connection_id)
                        .and_then(crate::state::connection_inventory)
                        .and_then(|inventory| {
                            inventory.storage.iter().find(|sr| sr.uuid == sr_uuid)
                        })
                    else {
                        continue;
                    };
                    let hosts: Vec<(String, String)> =
                        find_connection(&self.workspace, &connection_id)
                            .and_then(crate::state::connection_inventory)
                            .map(|inventory| {
                                inventory
                                    .hosts
                                    .iter()
                                    .map(|host| (host.uuid.clone(), host.name.clone()))
                                    .collect()
                            })
                            .unwrap_or_default();
                    if hosts.is_empty() {
                        continue;
                    }
                    self.state.sr_reattach_dialog = Some(crate::state::SrReattachDialog {
                        connection_id,
                        sr_uuid: sr.uuid.clone(),
                        sr_name: sr.name.clone(),
                        host_uuid: hosts.first().map(|(uuid, _)| uuid.clone()),
                        hosts,
                        location: String::new(),
                        submitting: false,
                        error: None,
                    });
                }
                UiAction::CloseSrReattach => {
                    if !self
                        .state
                        .sr_reattach_dialog
                        .as_ref()
                        .is_some_and(|dialog| dialog.submitting)
                    {
                        self.state.sr_reattach_dialog = None;
                    }
                }
                UiAction::SaveSrReattach => {
                    let Some(dialog) = self.state.sr_reattach_dialog.as_mut() else {
                        continue;
                    };
                    let Some(host_uuid) = dialog.host_uuid.clone() else {
                        dialog.error = Some("Select a host.".into());
                        continue;
                    };
                    dialog.submitting = true;
                    dialog.error = None;
                    self.worker.send(Command::ReattachSr {
                        connection_id: dialog.connection_id.clone(),
                        sr_uuid: dialog.sr_uuid.clone(),
                        host_uuid,
                        location: dialog.location.trim().to_owned(),
                    });
                }
                UiAction::PromptSrAction {
                    connection_id,
                    sr_uuid,
                    sr_name,
                    action,
                } => {
                    if action.destructive() {
                        self.state.sr_action_confirm = Some(crate::state::SrActionConfirm {
                            connection_id,
                            sr_uuid,
                            sr_name,
                            action,
                            acknowledged: false,
                            submitting: false,
                            error: None,
                        });
                    } else {
                        self.worker.send(Command::SrAction {
                            connection_id,
                            sr_uuid,
                            action,
                        });
                    }
                }
                UiAction::CancelSrAction => {
                    if !self
                        .state
                        .sr_action_confirm
                        .as_ref()
                        .is_some_and(|confirm| confirm.submitting)
                    {
                        self.state.sr_action_confirm = None;
                    }
                }
                UiAction::ConfirmSrAction => {
                    let Some(confirm) = self.state.sr_action_confirm.as_mut() else {
                        continue;
                    };
                    if !confirm.acknowledged {
                        confirm.error = Some("Confirm that you understand this action.".into());
                        continue;
                    }
                    confirm.submitting = true;
                    confirm.error = None;
                    self.worker.send(Command::SrAction {
                        connection_id: confirm.connection_id.clone(),
                        sr_uuid: confirm.sr_uuid.clone(),
                        action: confirm.action,
                    });
                }
                UiAction::PromptDeleteSnapshots => {
                    let Some(key) = self.state.selection.as_ref().map(|tab| tab.key.clone()) else {
                        continue;
                    };
                    let Some((connection_id, vm_uuid)) = vm_key(&key) else {
                        continue;
                    };
                    let Some((connection, vm)) = find_vm(&self.workspace, &connection_id, &vm_uuid)
                    else {
                        continue;
                    };
                    let Some(inventory) = crate::state::connection_inventory(connection) else {
                        continue;
                    };
                    let snapshots: Vec<(String, String)> = inventory
                        .snapshots
                        .iter()
                        .filter(|snapshot| {
                            snapshot.snapshot_of == vm.reference
                                && self.state.snapshot_selection.contains(&snapshot.uuid)
                        })
                        .map(|snapshot| (snapshot.uuid.clone(), snapshot.name.clone()))
                        .collect();
                    if snapshots.is_empty() {
                        self.state.toasts.push(Toast::new(
                            "Select at least one snapshot to delete.",
                            ToastKind::Info,
                        ));
                        continue;
                    }
                    self.state.snapshot_confirm = Some(crate::state::SnapshotDeleteConfirm {
                        connection_id,
                        vm_uuid,
                        vm_name: vm.name.clone(),
                        snapshots,
                        submitting: false,
                        error: None,
                    });
                }
                UiAction::CloseSnapshotDelete => {
                    if !self
                        .state
                        .snapshot_confirm
                        .as_ref()
                        .is_some_and(|confirm| confirm.submitting)
                    {
                        self.state.snapshot_confirm = None;
                    }
                }
                UiAction::ConfirmSnapshotDelete => {
                    let Some(confirm) = self.state.snapshot_confirm.as_mut() else {
                        continue;
                    };
                    confirm.submitting = true;
                    confirm.error = None;
                    self.worker.send(Command::DeleteSnapshots {
                        connection_id: confirm.connection_id.clone(),
                        vm_uuid: confirm.vm_uuid.clone(),
                        snapshot_uuids: confirm
                            .snapshots
                            .iter()
                            .map(|(uuid, _)| uuid.clone())
                            .collect(),
                    });
                }
                UiAction::DismissAlert {
                    connection_id,
                    message_uuid,
                } => {
                    self.worker.send(Command::DismissAlert {
                        connection_id,
                        message_uuid,
                    });
                }
                UiAction::PromptHostAction {
                    host_uuid,
                    host_name,
                    action,
                } => {
                    let Some(route) = self.state.host_route.clone() else {
                        continue;
                    };
                    self.state.host_action_confirm = Some(crate::state::HostActionConfirm {
                        connection_id: route.connection_id,
                        host_uuid,
                        host_name,
                        action,
                        acknowledged: !action.destructive(),
                        submitting: false,
                        error: None,
                    });
                }
                UiAction::CancelHostAction => {
                    if !self
                        .state
                        .host_action_confirm
                        .as_ref()
                        .is_some_and(|confirm| confirm.submitting)
                    {
                        self.state.host_action_confirm = None;
                    }
                }
                UiAction::ConfirmHostAction => {
                    let Some(confirm) = self.state.host_action_confirm.as_mut() else {
                        continue;
                    };
                    if confirm.action.destructive() && !confirm.acknowledged {
                        confirm.error = Some("Confirm that you understand this action.".into());
                        continue;
                    }
                    confirm.submitting = true;
                    confirm.error = None;
                    self.worker.send(Command::HostAction {
                        connection_id: confirm.connection_id.clone(),
                        host_uuid: confirm.host_uuid.clone(),
                        action: confirm.action,
                    });
                }
                UiAction::LoadSrVdis {
                    seq,
                    connection_id,
                    sr_uuid,
                } => {
                    self.worker.send(Command::SrVdis {
                        seq,
                        connection_id,
                        sr_uuid,
                    });
                }
                UiAction::PromptDeleteVdi {
                    vdi_uuid,
                    name,
                    size,
                } => {
                    let Some(ObjectKey::Storage {
                        connection_id,
                        sr_uuid,
                    }) = self.state.selection.as_ref().map(|tab| tab.key.clone())
                    else {
                        continue;
                    };
                    self.state.vdi_delete_confirm = Some(crate::state::VdiDeleteConfirm {
                        connection_id,
                        sr_uuid,
                        vdi_uuid,
                        name,
                        size,
                        submitting: false,
                        error: None,
                    });
                }
                UiAction::CancelDeleteVdi => {
                    if !self
                        .state
                        .vdi_delete_confirm
                        .as_ref()
                        .is_some_and(|confirm| confirm.submitting)
                    {
                        self.state.vdi_delete_confirm = None;
                    }
                }
                UiAction::ConfirmDeleteVdi => {
                    let Some(confirm) = self.state.vdi_delete_confirm.as_mut() else {
                        continue;
                    };
                    confirm.submitting = true;
                    confirm.error = None;
                    self.worker.send(Command::DeleteVdi {
                        connection_id: confirm.connection_id.clone(),
                        vdi_uuid: confirm.vdi_uuid.clone(),
                    });
                }
                UiAction::OpenHaEdit => {
                    let Some(key) = self.state.selection.as_ref().map(|tab| tab.key.clone()) else {
                        continue;
                    };
                    let Some((connection_id, vm_uuid)) = vm_key(&key) else {
                        continue;
                    };
                    let Some(details) = self.state.vm_state.ready() else {
                        continue;
                    };
                    let hosts: Vec<(String, String)> =
                        find_connection(&self.workspace, &connection_id)
                            .and_then(crate::state::connection_inventory)
                            .map(|inventory| {
                                inventory
                                    .hosts
                                    .iter()
                                    .map(|host| (host.uuid.clone(), host.name.clone()))
                                    .collect()
                            })
                            .unwrap_or_default();
                    self.state.ha_edit_dialog = Some(crate::state::HaEditDialog {
                        connection_id,
                        vm_uuid,
                        hosts,
                        restart_priority: details.ha_restart_priority.clone(),
                        start_delay: details.start_delay.to_string(),
                        shutdown_delay: details.shutdown_delay.to_string(),
                        affinity_host_uuid: (!details.affinity_ref.is_empty()
                            && details.affinity_ref != "OpaqueRef:NULL")
                            .then(|| details.affinity_ref.clone()),
                        submitting: false,
                        error: None,
                    });
                }
                UiAction::CloseHaEdit => {
                    if !self
                        .state
                        .ha_edit_dialog
                        .as_ref()
                        .is_some_and(|dialog| dialog.submitting)
                    {
                        self.state.ha_edit_dialog = None;
                    }
                }
                UiAction::SaveHaEdit => {
                    let Some(dialog) = self.state.ha_edit_dialog.as_mut() else {
                        continue;
                    };
                    let Some(start_delay) = dialog.start_delay.trim().parse::<u64>().ok() else {
                        dialog.error =
                            Some("Start delay must be a whole number of seconds.".into());
                        continue;
                    };
                    let Some(shutdown_delay) = dialog.shutdown_delay.trim().parse::<u64>().ok()
                    else {
                        dialog.error =
                            Some("Shutdown delay must be a whole number of seconds.".into());
                        continue;
                    };
                    if start_delay > 7 * 24 * 60 * 60 || shutdown_delay > 7 * 24 * 60 * 60 {
                        dialog.error = Some("Delays must be 7 days or fewer.".into());
                        continue;
                    }
                    dialog.submitting = true;
                    dialog.error = None;
                    self.worker.send(Command::UpdateVmHa {
                        connection_id: dialog.connection_id.clone(),
                        vm_uuid: dialog.vm_uuid.clone(),
                        restart_priority: dialog.restart_priority.clone(),
                        start_delay,
                        shutdown_delay,
                        affinity_host_uuid: dialog.affinity_host_uuid.clone(),
                    });
                }
                UiAction::OpenAttachVdi => {
                    let Some(key) = self.state.selection.as_ref().map(|tab| tab.key.clone()) else {
                        continue;
                    };
                    let Some((connection_id, vm_uuid)) = vm_key(&key) else {
                        continue;
                    };
                    self.state.vdi_catalog_seq += 1;
                    self.state.attach_vdi_dialog = Some(crate::state::AttachVdiDialog {
                        connection_id: connection_id.clone(),
                        vm_uuid,
                        vdis: Vec::new(),
                        selected: None,
                        search: String::new(),
                        bootable: false,
                        loading: true,
                        submitting: false,
                        error: None,
                    });
                    self.worker.send(Command::VdiCatalog {
                        seq: self.state.vdi_catalog_seq,
                        connection_id,
                    });
                }
                UiAction::CloseAttachVdi => {
                    if !self
                        .state
                        .attach_vdi_dialog
                        .as_ref()
                        .is_some_and(|dialog| dialog.loading || dialog.submitting)
                    {
                        self.state.attach_vdi_dialog = None;
                    }
                }
                UiAction::SaveAttachVdi => {
                    let Some(dialog) = self.state.attach_vdi_dialog.as_mut() else {
                        continue;
                    };
                    let Some(vdi_uuid) = dialog.selected.clone() else {
                        dialog.error = Some("Select a disk to attach.".into());
                        continue;
                    };
                    dialog.submitting = true;
                    dialog.error = None;
                    self.worker.send(Command::AttachVdi {
                        connection_id: dialog.connection_id.clone(),
                        vm_uuid: dialog.vm_uuid.clone(),
                        vdi_uuid,
                        bootable: dialog.bootable,
                    });
                }
                UiAction::OpenMoveDisk {
                    vbd_uuid,
                    name,
                    size,
                    source_sr_uuid,
                } => {
                    let Some(key) = self.state.selection.as_ref().map(|tab| tab.key.clone()) else {
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
                                    .filter(|sr| {
                                        sr.content_type != "iso"
                                            && sr.uuid != source_sr_uuid
                                            && sr.total > sr.used
                                    })
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
                            "No other writable storage repository is available.",
                            ToastKind::Info,
                        ));
                        continue;
                    }
                    let live = self
                        .state
                        .vm_state
                        .ready()
                        .is_some_and(|details| details.power_state == "Running");
                    self.state.move_disk_dialog = Some(crate::state::MoveDiskDialog {
                        connection_id,
                        vm_uuid,
                        vbd_uuid,
                        name,
                        size,
                        live,
                        selected: srs.first().map(|(uuid, _, _)| uuid.clone()),
                        srs,
                        submitting: false,
                        error: None,
                    });
                }
                UiAction::CloseMoveDisk => {
                    if !self
                        .state
                        .move_disk_dialog
                        .as_ref()
                        .is_some_and(|dialog| dialog.submitting)
                    {
                        self.state.move_disk_dialog = None;
                    }
                }
                UiAction::SaveMoveDisk => {
                    let Some(dialog) = self.state.move_disk_dialog.as_mut() else {
                        continue;
                    };
                    let Some(sr_uuid) = dialog.selected.clone() else {
                        dialog.error = Some("Select a target storage repository.".into());
                        continue;
                    };
                    dialog.submitting = true;
                    dialog.error = None;
                    self.worker.send(Command::MoveDisk {
                        connection_id: dialog.connection_id.clone(),
                        vm_uuid: dialog.vm_uuid.clone(),
                        vbd_uuid: dialog.vbd_uuid.clone(),
                        sr_uuid,
                    });
                }
                UiAction::SetVbdMode { vbd_uuid, mode } => {
                    let Some(key) = self.state.selection.as_ref().map(|tab| tab.key.clone()) else {
                        continue;
                    };
                    let Some((connection_id, vm_uuid)) = vm_key(&key) else {
                        continue;
                    };
                    self.worker.send(Command::SetVbdMode {
                        connection_id,
                        vm_uuid,
                        vbd_uuid,
                        mode,
                    });
                }
                UiAction::SetVbdPlugged { vbd_uuid, plugged } => {
                    let Some(key) = self.state.selection.as_ref().map(|tab| tab.key.clone()) else {
                        continue;
                    };
                    let Some((connection_id, vm_uuid)) = vm_key(&key) else {
                        continue;
                    };
                    self.worker.send(Command::SetVbdPlugged {
                        connection_id,
                        vm_uuid,
                        vbd_uuid,
                        plugged,
                    });
                }
                UiAction::SetVbdBootable { vbd_uuid, bootable } => {
                    let Some(key) = self.state.selection.as_ref().map(|tab| tab.key.clone()) else {
                        continue;
                    };
                    let Some((connection_id, vm_uuid)) = vm_key(&key) else {
                        continue;
                    };
                    self.worker.send(Command::SetVbdBootable {
                        connection_id,
                        vm_uuid,
                        vbd_uuid,
                        bootable,
                    });
                }
                UiAction::ApplySavedSearch(index) => {
                    let Some(saved) = self.state.saved_searches.get(index).cloned() else {
                        continue;
                    };
                    self.state.query = saved.query;
                    self.state.group_by = saved.group_by;
                    self.state.page = 0;
                    self.state.selected.clear();
                }
                UiAction::DeleteSavedSearch(index) => {
                    if index < self.state.saved_searches.len() {
                        self.state.saved_searches.remove(index);
                        crate::config::save_saved_searches(&self.state.saved_searches);
                        self.state
                            .toasts
                            .push(Toast::new("Saved search deleted.", ToastKind::Info));
                    }
                }
                UiAction::EditSavedSearch(index) => {
                    if let Some(saved) = self.state.saved_searches.get(index) {
                        self.state.save_search = Some(crate::state::SaveSearchState {
                            name: saved.name.clone(),
                            query: saved.query.clone(),
                            group_by: saved.group_by,
                            editing: Some(index),
                            error: None,
                        });
                    }
                }
                UiAction::OpenSaveSearch => {
                    let suggested = if self.state.query.trim().is_empty() {
                        "All objects".to_owned()
                    } else {
                        self.state.query.trim().chars().take(48).collect()
                    };
                    self.state.save_search = Some(crate::state::SaveSearchState {
                        name: suggested,
                        query: self.state.query.clone(),
                        group_by: self.state.group_by,
                        editing: None,
                        error: None,
                    });
                }
                UiAction::CloseSaveSearch => self.state.save_search = None,
                UiAction::SaveCurrentSearch => {
                    let Some(mut dialog) = self.state.save_search.take() else {
                        continue;
                    };
                    let name = dialog.name.trim().to_owned();
                    if name.is_empty() {
                        dialog.error = Some("Enter a name for the saved search.".into());
                        self.state.save_search = Some(dialog);
                        continue;
                    }
                    let saved = crate::config::SavedSearch {
                        name: name.clone(),
                        query: dialog.query.trim().to_owned(),
                        group_by: dialog.group_by,
                    };
                    crate::config::upsert_saved_search(
                        &mut self.state.saved_searches,
                        saved,
                        dialog.editing,
                    );
                    crate::config::save_saved_searches(&self.state.saved_searches);
                    self.state.toasts.push(Toast::new(
                        format!("Saved search “{name}”."),
                        ToastKind::Success,
                    ));
                }
                UiAction::OpenAddDisk => {
                    let Some(key) = self.state.selection.as_ref().map(|tab| tab.key.clone()) else {
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
                    let Some(key) = self.state.selection.as_ref().map(|tab| tab.key.clone()) else {
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
                    let Some(key) = self.state.selection.as_ref().map(|tab| tab.key.clone()) else {
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
                UiAction::OpenVifDialog(vif_uuid) => {
                    let Some(key) = self.state.selection.as_ref().map(|tab| tab.key.clone()) else {
                        continue;
                    };
                    let Some((connection_id, vm_uuid)) = vm_key(&key) else {
                        continue;
                    };
                    let existing = vif_uuid.as_ref().and_then(|vif_uuid| {
                        self.state.vm_state.ready().and_then(|details| {
                            details
                                .interfaces
                                .iter()
                                .find(|interface| &interface.uuid == vif_uuid)
                        })
                    });
                    let networks: Vec<(String, String)> =
                        find_connection(&self.workspace, &connection_id)
                            .and_then(crate::state::connection_inventory)
                            .map(|inventory| {
                                inventory
                                    .networks
                                    .iter()
                                    .filter(|network| {
                                        network.bridge != "xenapi"
                                            || existing.is_some_and(|interface| {
                                                interface.network_uuid == network.uuid
                                            })
                                    })
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
                        vif_uuid,
                        network_uuid: existing
                            .map(|interface| interface.network_uuid.clone())
                            .filter(|uuid| networks.iter().any(|(candidate, _)| candidate == uuid)),
                        networks,
                        mac: existing
                            .map(|interface| interface.mac.clone())
                            .unwrap_or_default(),
                        locking_mode: existing
                            .map(|interface| interface.locking_mode.clone())
                            .filter(|mode| !mode.is_empty())
                            .unwrap_or_else(|| "network_default".into()),
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
                    if let Some(vif_uuid) = dialog.vif_uuid.clone() {
                        self.worker.send(Command::UpdateVif {
                            connection_id: dialog.connection_id.clone(),
                            vm_uuid: dialog.vm_uuid.clone(),
                            vif_uuid,
                            network_uuid,
                            locking_mode: dialog.locking_mode.clone(),
                        });
                    } else {
                        self.worker.send(Command::AddVif {
                            connection_id: dialog.connection_id.clone(),
                            vm_uuid: dialog.vm_uuid.clone(),
                            network_uuid,
                            mac: (!dialog.mac.trim().is_empty())
                                .then(|| dialog.mac.trim().to_owned()),
                        });
                    }
                }
                UiAction::PromptRemoveVif { vif_uuid, mac } => {
                    let Some(key) = self.state.selection.as_ref().map(|tab| tab.key.clone()) else {
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
                    let Some(key) = self.state.selection.as_ref().map(|tab| tab.key.clone()) else {
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
                UiAction::LaunchRdp(address) => {
                    if address.parse::<std::net::IpAddr>().is_err() {
                        self.state.toasts.push(Toast::new(
                            "The guest did not report a valid IPv4 address.",
                            ToastKind::Error,
                        ));
                        continue;
                    }
                    match std::process::Command::new("xfreerdp")
                        .args([
                            format!("/v:{address}"),
                            "/cert:ignore".into(),
                            "/dynamic-resolution".into(),
                            "/network:auto".into(),
                        ])
                        .spawn()
                    {
                        Ok(_) => self
                            .state
                            .toasts
                            .push(Toast::new("Launching RDP with xfreerdp.", ToastKind::Info)),
                        Err(error) => self.state.toasts.push(Toast::new(
                            format!("Cannot launch xfreerdp: {error}"),
                            ToastKind::Error,
                        )),
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

        views::shell::menubar(ui, &mut self.state, &self.workspace, &mut actions);
        views::shell::toolbar(ui, &mut self.state, &self.workspace, &mut actions);
        views::shell::statusbar(ui, &mut self.state, &self.workspace, &mut actions);
        views::sidebar::show(ui, &mut self.state, &self.workspace, &mut actions);

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

fn pvs_site_list(state: &crate::state::UiState) -> Vec<(String, String)> {
    match &state.pool_status_state {
        LoadState::Ready(status) => status
            .pvs
            .sites
            .iter()
            .map(|site| (site.uuid.clone(), site.name.clone()))
            .collect(),
        _ => Vec::new(),
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
