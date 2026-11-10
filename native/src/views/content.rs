use egui::{Align, Layout, RichText, Ui};
use xen_core::workspace::{ConnectionView, WorkspaceView};

use crate::state::{
    Scope, UiAction, UiState, View, connection_name, find_connection, scoped_hosts, scoped_targets,
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
    if state.host_route.is_some() {
        super::host_details::show(ui, state, workspace, actions);
        return;
    }

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

    page_header(ui, state, workspace, actions);
    ui.add_space(10.0);
    scope_toolbar(ui, state, workspace, actions);
    ui.add_space(12.0);

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

fn page_header(
    ui: &mut Ui,
    state: &mut UiState,
    workspace: &WorkspaceView,
    actions: &mut Vec<UiAction>,
) {
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
                        .size(26.0)
                        .strong()
                        .color(fg()),
                );
                ui.label(RichText::new(".").size(26.0).color(accent()));
            });
            ui.label(
                RichText::new(state.view.subtitle())
                    .size(11.5)
                    .color(muted_fg()),
            );
        });
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            if ui
                .add(widgets::primary_button("+  Add connection"))
                .clicked()
            {
                state.connect = Some(crate::state::ConnectState::empty());
            }
            let busy = !state.busy.is_empty();
            let has_connections = !visible_connections(workspace, &state.scope).is_empty();
            if ui
                .add_enabled(
                    has_connections && !busy,
                    egui::Button::new(
                        RichText::new(if busy {
                            "Refreshing…".to_owned()
                        } else if state.scope.connection_id().is_some() {
                            "Refresh".to_owned()
                        } else {
                            "Refresh all".to_owned()
                        })
                        .size(12.0)
                        .color(if busy { muted_fg() } else { fg() }),
                    ),
                )
                .clicked()
            {
                actions.push(UiAction::Refresh(
                    state.scope.connection_id().map(str::to_owned),
                ));
            }
            if busy {
                ui.spinner();
            }
        });
    });
}

fn scope_toolbar(
    ui: &mut Ui,
    state: &mut UiState,
    workspace: &WorkspaceView,
    actions: &mut Vec<UiAction>,
) {
    let hosts = scoped_hosts(workspace, &state.scope).len();
    let vms = scoped_targets(workspace, &state.scope).len();
    ui.horizontal(|ui| {
        egui::ComboBox::from_id_salt("scope-selector")
            .selected_text(
                RichText::new(super::chrome::scope_name(workspace, &state.scope))
                    .size(11.0)
                    .color(fg()),
            )
            .width(220.0)
            .show_ui(ui, |ui| {
                let mut chosen: Option<Scope> = None;
                if ui
                    .selectable_label(state.scope == Scope::All, "All connections")
                    .clicked()
                {
                    chosen = Some(Scope::All);
                }
                for connection in &workspace.connections {
                    let id = crate::state::connection_id(connection);
                    let selected = state.scope == Scope::Connection(id.to_owned());
                    let label = format!(
                        "{}{}",
                        connection_name(connection),
                        if matches!(connection, ConnectionView::Offline { .. }) {
                            "  (offline)"
                        } else {
                            ""
                        }
                    );
                    if ui.selectable_label(selected, label).clicked() {
                        chosen = Some(Scope::Connection(id.to_owned()));
                    }
                }
                if let Some(scope) = chosen {
                    actions.push(UiAction::SetScope(scope));
                }
            });
        ui.label(
            RichText::new(crate::format::plural(hosts, "host", "hosts"))
                .size(10.5)
                .color(muted_fg()),
        );
        ui.label(RichText::new("·").size(10.5).color(dim_fg()));
        ui.label(
            RichText::new(crate::format::plural(vms, "guest VM", "guest VMs"))
                .size(10.5)
                .color(muted_fg()),
        );
        if let Some(id) = state.scope.connection_id()
            && let Some(connection) = find_connection(workspace, id)
            && crate::state::is_read_only(connection)
        {
            let production =
                crate::state::normalized_host(&crate::state::connection_settings(connection).host)
                    == "10.100.1.11";
            widgets::badge(
                ui,
                if production {
                    "Production · read-only"
                } else {
                    "Read-only"
                },
                amber(),
            );
        }
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
    ui.add_space(60.0);
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
        if settings.read_only {
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
