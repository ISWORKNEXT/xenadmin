use egui::{
    Align, Align2, Color32, CornerRadius, FontId, Frame, Layout, Margin, RichText, Sense, Stroke,
    Ui, Vec2,
};
use xen_core::workspace::{ConnectionView, WorkspaceView};

use crate::state::{
    ConnectState, Scope, UiAction, UiState, connected_connections, connection_id, connection_name,
    connection_settings, connection_status, is_read_only,
};
use crate::theme::*;
use crate::widgets;

pub fn show(
    ui: &mut Ui,
    state: &mut UiState,
    workspace: &WorkspaceView,
    actions: &mut Vec<UiAction>,
) {
    egui::Panel::left("sidebar")
        .exact_size(244.0)
        .resizable(false)
        .frame(
            Frame::NONE
                .fill(panel())
                .stroke(Stroke::new(1.0, border_soft()))
                .inner_margin(Margin::symmetric(12, 10)),
        )
        .show(ui, |ui| {
            ui.add_space(2.0);
            workspace_label(ui);
            ui.add_space(12.0);
            connections_header(ui, state, actions);
            ui.add_space(4.0);
            all_connections_row(ui, state, workspace, actions);
            ui.add_space(2.0);

            egui::ScrollArea::vertical()
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    for connection in &workspace.connections {
                        connection_row(ui, state, connection, actions);
                    }
                });

            ui.with_layout(Layout::bottom_up(Align::LEFT), |ui| {
                ui.add_space(2.0);
                ui.label(
                    RichText::new("Ctrl+K search   ·   F5 refresh")
                        .monospace()
                        .size(9.0)
                        .color(dim_fg()),
                );
            });
        });
}

fn workspace_label(ui: &mut Ui) {
    ui.horizontal(|ui| {
        ui.label(RichText::new("WORKSPACE").size(9.0).color(dim_fg()));
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            widgets::badge(ui, "MULTI-HOST", cyan());
        });
    });
}

fn connections_header(ui: &mut Ui, state: &mut UiState, _actions: &mut Vec<UiAction>) {
    ui.horizontal(|ui| {
        ui.label(RichText::new("CONNECTIONS").size(9.0).color(dim_fg()));
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            if ui
                .add(
                    egui::Button::new(RichText::new("+").size(15.0).color(muted_fg())).frame(false),
                )
                .on_hover_text("Add connection")
                .clicked()
            {
                state.connect = Some(ConnectState::empty());
            }
        });
    });
}

fn all_connections_row(
    ui: &mut Ui,
    state: &mut UiState,
    workspace: &WorkspaceView,
    actions: &mut Vec<UiAction>,
) {
    let connected = connected_connections(workspace).len();
    let total = workspace.connections.len();
    let selected = state.scope == Scope::All;

    let (rect, response) =
        ui.allocate_exact_size(Vec2::new(ui.available_width(), 30.0), Sense::click());
    let hovered = response.hovered();
    if selected || hovered {
        ui.painter().rect_filled(
            rect,
            CornerRadius::same(7),
            if selected { panel_3() } else { panel_2() },
        );
    }
    ui.painter().text(
        egui::pos2(rect.left() + 14.0, rect.center().y),
        Align2::LEFT_CENTER,
        "All connections",
        FontId::proportional(12.0),
        if selected || hovered {
            fg()
        } else {
            muted_fg()
        },
    );
    ui.painter().text(
        egui::pos2(rect.right() - 10.0, rect.center().y),
        Align2::RIGHT_CENTER,
        format!("{connected}/{total}"),
        FontId::monospace(9.5),
        dim_fg(),
    );
    if response.clicked() {
        actions.push(UiAction::SetScope(Scope::All));
    }
}

fn connection_row(
    ui: &mut Ui,
    state: &mut UiState,
    connection: &ConnectionView,
    actions: &mut Vec<UiAction>,
) {
    let id = connection_id(connection).to_owned();
    let selected = state.scope == Scope::Connection(id.clone());
    let expanded = state.expanded.contains(&id);
    let status = connection_status(connection);
    let read_only = is_read_only(connection);
    let name = connection_name(connection);

    let frame = Frame::NONE
        .fill(if selected {
            panel_3()
        } else {
            Color32::TRANSPARENT
        })
        .corner_radius(CornerRadius::same(7))
        .inner_margin(Margin::symmetric(6, 4));
    frame.show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 4.0;
            let (chevron_rect, chevron) =
                ui.allocate_exact_size(Vec2::new(14.0, 18.0), Sense::click());
            let center = chevron_rect.center();
            let color = if chevron.hovered() { fg() } else { dim_fg() };
            let stroke = Stroke::new(1.3, color);
            if expanded {
                ui.painter().line_segment(
                    [
                        egui::pos2(center.x - 3.0, center.y - 1.5),
                        egui::pos2(center.x, center.y + 1.5),
                    ],
                    stroke,
                );
                ui.painter().line_segment(
                    [
                        egui::pos2(center.x, center.y + 1.5),
                        egui::pos2(center.x + 3.0, center.y - 1.5),
                    ],
                    stroke,
                );
            } else {
                ui.painter().line_segment(
                    [
                        egui::pos2(center.x - 1.5, center.y - 3.0),
                        egui::pos2(center.x + 1.5, center.y),
                    ],
                    stroke,
                );
                ui.painter().line_segment(
                    [
                        egui::pos2(center.x + 1.5, center.y),
                        egui::pos2(center.x - 1.5, center.y + 3.0),
                    ],
                    stroke,
                );
            }
            if chevron.clicked() {
                actions.push(UiAction::ToggleExpand(id.clone()));
            }
            widgets::dot(ui, widgets::connection_color(status), 7.0);
            let label =
                ui.add(
                    egui::Label::new(RichText::new(&name).size(11.5).color(
                        if status == "offline" {
                            muted_fg()
                        } else {
                            fg()
                        },
                    ))
                    .truncate()
                    .sense(Sense::click()),
                );
            if label.clicked() {
                actions.push(UiAction::SetScope(Scope::Connection(id.clone())));
            }
            if read_only {
                ui.label(RichText::new("RO").size(7.5).color(dim_fg()))
                    .on_hover_text("Read-only connection");
            }
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                connection_menu(ui, state, connection, actions);
            });
        });
    });

    if expanded {
        if let Some(inventory) = crate::state::connection_inventory(connection) {
            for host in &inventory.hosts {
                host_row(ui, state, &id, &host.reference, &host.name, actions);
            }
        }
        if matches!(connection, ConnectionView::Offline { .. }) {
            ui.horizontal(|ui| {
                ui.add_space(24.0);
                ui.label(
                    RichText::new("Reconnect to load hosts")
                        .size(10.0)
                        .color(dim_fg()),
                );
            });
        }
    }
}

fn host_row(
    ui: &mut Ui,
    state: &mut UiState,
    connection_id: &str,
    host_ref: &str,
    name: &str,
    actions: &mut Vec<UiAction>,
) {
    let selected = matches!(&state.scope, Scope::Host { connection_id: current, host_ref: current_host }
        if current == connection_id && current_host == host_ref);
    let width = ui.available_width();
    let (rect, response) = ui.allocate_exact_size(Vec2::new(width, 26.0), Sense::click());
    if selected || response.hovered() {
        ui.painter().rect_filled(
            rect,
            CornerRadius::same(6),
            if selected { panel_3() } else { panel_2() },
        );
    }
    let dot_center = egui::pos2(rect.left() + 28.0, rect.center().y);
    ui.painter()
        .circle_filled(dot_center, 2.5, if selected { accent() } else { dim_fg() });
    ui.painter().text(
        egui::pos2(rect.left() + 38.0, rect.center().y),
        Align2::LEFT_CENTER,
        name,
        FontId::proportional(11.0),
        if selected || response.hovered() {
            fg()
        } else {
            muted_fg()
        },
    );
    if response.clicked() {
        actions.push(UiAction::SetScope(Scope::Host {
            connection_id: connection_id.to_owned(),
            host_ref: host_ref.to_owned(),
        }));
    }
}

fn connection_menu(
    ui: &mut Ui,
    state: &mut UiState,
    connection: &ConnectionView,
    actions: &mut Vec<UiAction>,
) {
    ui.menu_button(RichText::new("...").size(11.0).color(dim_fg()), |ui| {
        connection_menu_items(ui, state, connection, actions);
    });
}

fn connection_menu_items(
    ui: &mut Ui,
    state: &mut UiState,
    connection: &ConnectionView,
    actions: &mut Vec<UiAction>,
) {
    let id = connection_id(connection).to_owned();
    let status = connection_status(connection);
    if ui.button("Refresh inventory").clicked() {
        actions.push(UiAction::Refresh(Some(id.clone())));
        ui.close();
    }
    let label = if status == "offline" {
        "Connect to this host"
    } else {
        "Reconnect / edit"
    };
    if ui.button(label).clicked() {
        state.connect = Some(ConnectState::from_profile(
            id.clone(),
            connection_settings(connection).clone(),
        ));
        ui.close();
    }
    if status != "offline" && ui.button("Disconnect").clicked() {
        actions.push(UiAction::Disconnect(id.clone()));
        ui.close();
    }
    ui.separator();
    if status == "offline" {
        if ui.button("Remove saved connection").clicked() {
            actions.push(UiAction::ConfirmRemove(id.clone()));
            ui.close();
        }
    } else {
        ui.add_enabled(false, egui::Button::new("Disconnect before removing"));
    }
}
