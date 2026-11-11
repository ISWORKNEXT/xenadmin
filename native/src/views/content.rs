use egui::{Align, Layout, RichText, Ui};
use xen_core::workspace::{ConnectionView, WorkspaceView};

use crate::state::{
    ObjectKey, Scope, UiAction, UiState, View, connection_name, find_connection, is_read_only,
    visible_connections,
};
use crate::theme::*;
use crate::widgets;

pub fn show(
    ui: &mut Ui,
    state: &mut UiState,
    workspace: &WorkspaceView,
    actions: &mut Vec<UiAction>,
) {
    if state.tree_selection.len() > 1 {
        super::object_page::tab_strip(ui, &[crate::state::VmTab::Search], &mut state.object_tab);
        super::search_page::show(ui, state, workspace, actions);
        return;
    }
    let key = state
        .selection
        .as_ref()
        .map(|tab| tab.key.clone())
        .unwrap_or(ObjectKey::Home);

    match key {
        ObjectKey::Home => {
            super::object_page::tab_strip(
                ui,
                &super::object_page::tabs_for(super::object_page::Selection::Home),
                &mut state.object_tab,
            );
            if state.object_tab == crate::state::VmTab::Search {
                super::search_page::show(ui, state, workspace, actions);
            } else {
                workspace_page(ui, state, workspace, actions);
            }
        }
        ObjectKey::Group(_) | ObjectKey::List { .. } => {
            super::object_page::tab_strip(
                ui,
                &super::object_page::tabs_for(super::object_page::Selection::Group),
                &mut state.object_tab,
            );
            super::search_page::show(ui, state, workspace, actions);
        }
        ObjectKey::Vdi { .. } => super::resource_page::vdi(ui, state, workspace, actions),
        ObjectKey::Tasks => super::tasks::show(ui, state, workspace, actions),
        ObjectKey::Pool(_) => super::pool_page::show(ui, state, workspace, actions),
        ObjectKey::Host { .. } => super::infrastructure_page::show(ui, state, workspace, actions),
        ObjectKey::Vm { .. } | ObjectKey::Template { .. } | ObjectKey::Snapshot { .. } => {
            super::vm_detail::show(ui, state, workspace, actions);
        }
        ObjectKey::Storage { .. } => super::resource_page::storage(ui, state, workspace, actions),
        ObjectKey::Network { .. } => super::resource_page::network(ui, state, workspace, actions),
        ObjectKey::Health { .. } => super::health::show(ui, state, workspace, actions),
        ObjectKey::Auth { .. } => super::auth::show(ui, state, workspace, actions),
        ObjectKey::PoolStatus { .. } => super::pool_status::show(ui, state, workspace, actions),
    }
}

fn workspace_page(
    ui: &mut Ui,
    state: &mut UiState,
    workspace: &WorkspaceView,
    actions: &mut Vec<UiAction>,
) {
    if let Scope::Connection(id) = state.scope.clone() {
        match find_connection(workspace, &id) {
            None => {
                actions.push(UiAction::SetScope(Scope::All));
                return;
            }
            Some(ConnectionView::Offline { .. }) => {
                offline_page(ui, state, workspace, actions);
                return;
            }
            _ => {}
        }
    }

    if !state.loaded {
        widgets::empty_state(
            ui,
            "Opening your workspace",
            "Loading saved connections and task state…",
        );
        return;
    }

    if state.view != View::Overview {
        page_header(ui, state, workspace);
        ui.add_space(10.0);
    }

    let stale: Vec<(String, String)> = visible_connections(workspace, &state.scope)
        .into_iter()
        .filter_map(|connection| match connection {
            ConnectionView::Stale { error, .. } => {
                Some((connection_name(connection), error.clone()))
            }
            _ => None,
        })
        .collect();
    for (name, error) in &stale {
        super::chrome::error_banner(
            ui,
            &format!("{name}: {error} Last known inventory is shown."),
        );
        ui.add_space(6.0);
    }
    if !stale.is_empty() {
        ui.add_space(4.0);
    }

    match state.view {
        View::Overview => super::overview::show(ui, state, workspace, actions),
        View::Vms => super::inventory::vms(ui, state, workspace, actions, false),
        View::Templates => super::inventory::vms(ui, state, workspace, actions, true),
        View::Hosts => super::inventory::hosts(ui, state, workspace, actions),
        View::Storage => super::inventory::storage(ui, state, workspace, actions),
        View::Networks => super::inventory::networks(ui, state, workspace, actions),
        View::Tasks => super::tasks::show(ui, state, workspace, actions),
    }
}

fn page_header(ui: &mut Ui, state: &mut UiState, workspace: &WorkspaceView) {
    let scope_name = super::chrome::scope_name(workspace, &state.scope);
    ui.horizontal(|ui| {
        ui.vertical(|ui| {
            super::chrome::eyebrow(
                ui,
                if state.scope == Scope::All {
                    "YOUR ENTIRE INFRASTRUCTURE"
                } else {
                    &scope_name
                },
            );
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 2.0;
                ui.label(
                    RichText::new(state.view.title())
                        .size(28.0)
                        .strong()
                        .color(fg()),
                );
                ui.label(RichText::new(".").size(28.0).color(accent()));
            });
            ui.label(
                RichText::new(state.view.subtitle())
                    .size(12.5)
                    .color(muted_fg()),
            );
        });
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            if let Some(id) = state.scope.connection_id()
                && let Some(connection) = find_connection(workspace, id)
                && is_read_only(connection)
            {
                ui.label(RichText::new("Read-only").size(9.5).strong().color(amber()));
            }
        });
    });
}

fn offline_page(
    ui: &mut Ui,
    state: &mut UiState,
    workspace: &WorkspaceView,
    _actions: &mut Vec<UiAction>,
) {
    let Some(id) = state.scope.connection_id().map(str::to_owned) else {
        return;
    };
    let Some(connection) = find_connection(workspace, &id) else {
        return;
    };
    let settings = crate::state::connection_settings(connection);
    ui.add_space(40.0);
    ui.vertical_centered(|ui| {
        widgets::offline_mark(ui, 64.0);
        ui.add_space(10.0);
        ui.label(
            RichText::new(format!("{} is offline", connection_name(connection)))
                .size(19.0)
                .strong(),
        );
        ui.label(
            RichText::new(format!("{} · {}", settings.host, settings.username))
                .size(11.5)
                .color(muted_fg()),
        );
        if is_read_only(connection) {
            ui.add_space(4.0);
            widgets::badge(ui, "Read-only profile", amber());
        }
        ui.add_space(10.0);
        if ui
            .add(widgets::primary_button("Connect to this host"))
            .clicked()
        {
            state.connect = Some(crate::state::ConnectState::from_profile(
                id.clone(),
                settings.clone(),
            ));
        }
        ui.add_space(6.0);
        ui.label(
            RichText::new("Passwords are requested again and never saved.")
                .size(10.0)
                .color(dim_fg()),
        );
    });
}
