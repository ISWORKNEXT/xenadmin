use egui::{
    Align, Align2, Button, Color32, CornerRadius, CursorIcon, FontId, Frame, Layout, Margin,
    Rangef, Rect, RichText, Sense, Stroke, Ui, Vec2, ViewportCommand, pos2, vec2,
};
use xen_core::workspace::WorkspaceView;

use crate::config;
use crate::state::{
    ActionKind, ObjectKey, Scope, UiAction, UiState, action_allowed, connected_connections,
    connection_inventory, find_connection, find_vm, is_read_only,
};
use crate::theme;
use crate::theme::*;
use crate::widgets;

const TITLEBAR_HEIGHT: f32 = 50.0;

pub fn titlebar(
    ui: &mut Ui,
    state: &mut UiState,
    workspace: &WorkspaceView,
    actions: &mut Vec<UiAction>,
) {
    egui::Panel::top("titlebar")
        .exact_size(TITLEBAR_HEIGHT)
        .resizable(false)
        .frame(
            Frame::NONE
                .fill(panel())
                .stroke(Stroke::new(1.0, border_soft()))
                .inner_margin(Margin::symmetric(12, 0)),
        )
        .show(ui, |ui| {
            let bar_rect = ui.max_rect();
            let ctx = ui.ctx().clone();

            let drag = ui.interact(
                bar_rect,
                egui::Id::unique("titlebar-drag"),
                Sense::click_and_drag(),
            );
            if drag.double_clicked() {
                toggle_maximized(&ctx);
            } else if drag.drag_started() {
                ctx.send_viewport_cmd(ViewportCommand::StartDrag);
            }

            ui.horizontal_centered(|ui| {
                ui.spacing_mut().item_spacing.x = 5.0;
                brand(ui);
                ui.add_space(6.0);
                separator(ui);
                ui.label(
                    RichText::new("Infrastructure manager")
                        .size(9.0)
                        .color(dim_fg()),
                );

                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    ui.spacing_mut().item_spacing.x = 0.0;
                    window_controls(ui, &ctx);
                    ui.add_space(8.0);
                    theme_toggle(ui, &ctx);
                    ui.add_space(6.0);
                    zoom_control(ui, &ctx);
                    ui.add_space(6.0);

                    let running = workspace
                        .tasks
                        .iter()
                        .filter(|task| {
                            matches!(task.status, xen_core::operations::TaskStatus::Running)
                        })
                        .count();
                    if running > 0
                        && ui
                            .add(
                                Button::new(
                                    RichText::new(format!("{running} running"))
                                        .size(10.0)
                                        .color(amber()),
                                )
                                .frame(false),
                            )
                            .on_hover_text("Open Activity & tasks")
                            .clicked()
                    {
                        actions.push(UiAction::OpenObject {
                            key: ObjectKey::Tasks,
                            title: "Activity & tasks".into(),
                        });
                    }
                    ui.add_space(8.0);
                    scope_pill(ui, state, workspace);
                });
            });
        });
}

pub fn toolbar(
    ui: &mut Ui,
    state: &mut UiState,
    workspace: &WorkspaceView,
    actions: &mut Vec<UiAction>,
) {
    egui::Panel::top("toolbar")
        .exact_size(42.0)
        .resizable(false)
        .frame(
            Frame::NONE
                .fill(panel())
                .stroke(Stroke::new(1.0, border_soft()))
                .inner_margin(Margin::symmetric(12, 0)),
        )
        .show(ui, |ui| {
            ui.horizontal_centered(|ui| {
                let (home_rect, home_response) =
                    ui.allocate_exact_size(vec2(26.0, 26.0), Sense::click());
                if home_response.hovered() {
                    ui.painter()
                        .rect_filled(home_rect, CornerRadius::same(6), panel_3());
                }
                let color = if home_response.hovered() {
                    fg()
                } else {
                    muted_fg()
                };
                let center = home_rect.center();
                ui.painter().rect_stroke(
                    Rect::from_center_size(center + vec2(0.0, 2.0), vec2(10.0, 8.0)),
                    CornerRadius::same(1),
                    Stroke::new(1.2, color),
                    egui::StrokeKind::Inside,
                );
                ui.painter().line_segment(
                    [center + vec2(-6.0, -1.0), center + vec2(0.0, -6.0)],
                    Stroke::new(1.2, color),
                );
                ui.painter().line_segment(
                    [center + vec2(0.0, -6.0), center + vec2(6.0, -1.0)],
                    Stroke::new(1.2, color),
                );
                if home_response.on_hover_text("Home").clicked() {
                    actions.push(UiAction::OpenObject {
                        key: ObjectKey::Home,
                        title: "Home".into(),
                    });
                }
                ui.add_space(6.0);
                ui.label(RichText::new("Infrastructure").size(11.0).color(dim_fg()));
                ui.label(RichText::new("›").size(11.0).color(dim_fg()));
                let full_title = state
                    .object_tabs
                    .get(state.active_tab)
                    .map_or("Home", |tab| tab.title.as_str());
                let title = crate::format::truncate(full_title, 52);
                ui.label(RichText::new(title).size(11.5).strong().color(fg()))
                    .on_hover_text(full_title);
                if ui.available_width() > 520.0 {
                    ui.add_space(6.0);
                    ui.separator();
                    ui.add_space(2.0);
                    context_actions(ui, state, workspace, actions);
                }

                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if ui
                        .add(widgets::primary_button("+  Add connection"))
                        .clicked()
                    {
                        state.connect = Some(crate::state::ConnectState::empty());
                    }
                    ui.add_space(6.0);
                    let busy = state
                        .scope
                        .connection_id()
                        .is_some_and(|id| state.busy.contains(id));
                    let has_connections = !workspace.connections.is_empty();
                    if ui
                        .add_enabled(
                            has_connections && !busy,
                            egui::Button::new(
                                RichText::new(if busy { "Refreshing…" } else { "Refresh" })
                                    .size(11.5)
                                    .color(if busy { dim_fg() } else { fg() }),
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
        });
}

pub fn object_tabs(
    ui: &mut Ui,
    state: &mut UiState,
    _workspace: &WorkspaceView,
    actions: &mut Vec<UiAction>,
) {
    egui::Panel::top("object-tabs")
        .exact_size(42.0)
        .resizable(false)
        .frame(
            Frame::NONE
                .fill(panel_2())
                .stroke(Stroke::new(1.0, border_soft()))
                .inner_margin(Margin::symmetric(8, 4)),
        )
        .show(ui, |ui| {
            egui::ScrollArea::horizontal()
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 2.0;
                        let mut activate = None;
                        let mut close = None;
                        for (index, tab) in state.object_tabs.iter().enumerate() {
                            let active = index == state.active_tab;
                            let (response, close_clicked) =
                                object_tab(ui, &tab.title, active, index != 0);
                            if close_clicked {
                                close = Some(index);
                            } else if response.clicked() {
                                activate = Some(index);
                            }
                        }
                        if let Some(index) = close {
                            actions.push(UiAction::CloseTab(index));
                        } else if let Some(index) = activate {
                            actions.push(UiAction::ActivateTab(index));
                        }
                    });
                });
        });
}

fn object_tab(ui: &mut Ui, title: &str, active: bool, closable: bool) -> (egui::Response, bool) {
    let full_title = title.to_owned();
    let title = crate::format::truncate(title, 30);
    let font = egui::FontId::proportional(11.5);
    let galley = ui
        .painter()
        .layout_no_wrap(title.clone(), font.clone(), fg());
    let text_width = galley.size().x.min(220.0);
    let width = text_width + if closable { 42.0 } else { 22.0 };
    let (rect, response) = ui.allocate_exact_size(vec2(width, 32.0), Sense::click());
    let fill = if active { panel() } else { panel_2() };
    let stroke = if active {
        Stroke::new(1.0, border())
    } else {
        Stroke::new(1.0, Color32::TRANSPARENT)
    };
    ui.painter().rect_filled(rect, CornerRadius::same(6), fill);
    if active {
        ui.painter().rect_stroke(
            rect,
            CornerRadius::same(6),
            stroke,
            egui::StrokeKind::Inside,
        );
        ui.painter().hline(
            Rangef::new(rect.left() + 6.0, rect.right() - 6.0),
            rect.bottom() - 1.0,
            Stroke::new(2.0, accent()),
        );
    } else if response.hovered() {
        ui.painter()
            .rect_filled(rect, CornerRadius::same(6), panel_2());
    }
    let color = if active { fg() } else { muted_fg() };
    ui.painter().text(
        egui::pos2(rect.left() + 10.0, rect.center().y),
        Align2::LEFT_CENTER,
        &title,
        font,
        color,
    );
    let mut close_clicked = false;
    if closable {
        let close_rect = Rect::from_center_size(
            egui::pos2(rect.right() - 14.0, rect.center().y),
            Vec2::splat(18.0),
        );
        let close = ui.interact(
            close_rect,
            egui::Id::unique(("tab-close", title, rect.left() as i32)),
            Sense::click(),
        );
        if close.hovered() {
            ui.painter()
                .rect_filled(close_rect, CornerRadius::same(4), panel_3());
        }
        let x = close_rect.center();
        ui.painter().line_segment(
            [x + vec2(-3.0, -3.0), x + vec2(3.0, 3.0)],
            Stroke::new(1.1, muted_fg()),
        );
        ui.painter().line_segment(
            [x + vec2(3.0, -3.0), x + vec2(-3.0, 3.0)],
            Stroke::new(1.1, muted_fg()),
        );
        close_clicked = close.clicked();
    }
    (response.on_hover_text(full_title), close_clicked)
}

fn brand(ui: &mut Ui) {
    let (rect, _) = ui.allocate_exact_size(Vec2::splat(28.0), Sense::hover());
    let painter = ui.painter();
    painter.rect_filled(rect, CornerRadius::same(7), with_alpha(accent(), 30));
    painter.rect_stroke(
        rect,
        CornerRadius::same(7),
        Stroke::new(1.0, with_alpha(accent(), 110)),
        egui::StrokeKind::Inside,
    );
    painter.text(
        rect.center(),
        Align2::CENTER_CENTER,
        "X",
        FontId::proportional(14.0),
        accent(),
    );
    ui.label(RichText::new("Xen Desktop").size(13.5).strong().color(fg()));
    ui.label(RichText::new("XCP-ng").size(9.5).color(dim_fg()));
}

fn separator(ui: &mut Ui) {
    let (rect, _) = ui.allocate_exact_size(vec2(1.0, 18.0), Sense::hover());
    ui.painter()
        .vline(rect.center().x, rect.y_range(), Stroke::new(1.0, border()));
}

fn context_actions(
    ui: &mut Ui,
    state: &UiState,
    workspace: &WorkspaceView,
    actions: &mut Vec<UiAction>,
) {
    let Some(key) = state.object_tabs.get(state.active_tab).map(|tab| &tab.key) else {
        return;
    };
    match key {
        ObjectKey::Vm {
            connection_id,
            vm_uuid,
        } => {
            let Some((_, vm)) = find_vm(workspace, connection_id, vm_uuid) else {
                return;
            };
            let (start_kind, start_label) = if vm.power_state == "Suspended" {
                (ActionKind::Resume, "Resume")
            } else {
                (ActionKind::Start, "Start")
            };
            if ui
                .add_enabled(
                    action_allowed(workspace, connection_id, vm, start_kind.operation()),
                    Button::new(RichText::new(start_label).size(11.0).color(accent())).frame(false),
                )
                .clicked()
            {
                actions.push(UiAction::Propose(
                    start_kind,
                    vec![(connection_id.clone(), vm_uuid.clone())],
                ));
            }
            if ui
                .add_enabled(
                    action_allowed(
                        workspace,
                        connection_id,
                        vm,
                        ActionKind::Shutdown.operation(),
                    ),
                    Button::new(RichText::new("Shut down").size(11.0).color(muted_fg()))
                        .frame(false),
                )
                .clicked()
            {
                actions.push(UiAction::Propose(
                    ActionKind::Shutdown,
                    vec![(connection_id.clone(), vm_uuid.clone())],
                ));
            }
            if ui
                .add_enabled(
                    action_allowed(
                        workspace,
                        connection_id,
                        vm,
                        ActionKind::Snapshot.operation(),
                    ),
                    Button::new(RichText::new("Snapshot").size(11.0).color(muted_fg()))
                        .frame(false),
                )
                .clicked()
            {
                actions.push(UiAction::Propose(
                    ActionKind::Snapshot,
                    vec![(connection_id.clone(), vm_uuid.clone())],
                ));
            }
            ui.menu_button(RichText::new("More").size(11.0).color(muted_fg()), |ui| {
                for kind in ActionKind::MENU {
                    let allowed = action_allowed(workspace, connection_id, vm, kind.operation());
                    let color = if kind.destructive() { red() } else { fg() };
                    if ui
                        .add_enabled(
                            allowed,
                            Button::new(RichText::new(kind.label()).size(11.0).color(color))
                                .frame(false),
                        )
                        .clicked()
                    {
                        actions.push(UiAction::Propose(
                            kind,
                            vec![(connection_id.clone(), vm_uuid.clone())],
                        ));
                        ui.close();
                    }
                }
            });
        }
        ObjectKey::Host { .. } => {
            if ui
                .add(
                    Button::new(
                        RichText::new("Refresh details")
                            .size(11.0)
                            .color(muted_fg()),
                    )
                    .frame(false),
                )
                .clicked()
            {
                actions.push(UiAction::ReloadHostDetails);
            }
        }
        ObjectKey::Snapshot { .. } => {
            if let ObjectKey::Snapshot {
                connection_id,
                vm_uuid,
            } = key
                && let Some((_, snapshot)) = find_vm(workspace, connection_id, vm_uuid)
                && ui
                    .add_enabled(
                        action_allowed(
                            workspace,
                            connection_id,
                            snapshot,
                            ActionKind::Revert.operation(),
                        ),
                        Button::new(RichText::new("Revert").size(11.0).color(amber())).frame(false),
                    )
                    .clicked()
            {
                actions.push(UiAction::Propose(
                    ActionKind::Revert,
                    vec![(connection_id.clone(), vm_uuid.clone())],
                ));
            }
        }
        _ => {}
    }
    use xen_core::properties::{ObjectKind, ObjectTarget};
    let target = match key {
        ObjectKey::Vm { vm_uuid, .. }
        | ObjectKey::Template { vm_uuid, .. }
        | ObjectKey::Snapshot { vm_uuid, .. } => Some(ObjectTarget {
            kind: ObjectKind::Vm,
            uuid: vm_uuid.clone(),
        }),
        ObjectKey::Host { host_uuid, .. } => Some(ObjectTarget {
            kind: ObjectKind::Host,
            uuid: host_uuid.clone(),
        }),
        ObjectKey::Storage { sr_uuid, .. } => Some(ObjectTarget {
            kind: ObjectKind::Storage,
            uuid: sr_uuid.clone(),
        }),
        ObjectKey::Network { network_uuid, .. } => Some(ObjectTarget {
            kind: ObjectKind::Network,
            uuid: network_uuid.clone(),
        }),
        ObjectKey::Pool(connection_id) => find_connection(workspace, connection_id)
            .and_then(connection_inventory)
            .map(|inventory| ObjectTarget {
                kind: ObjectKind::Pool,
                uuid: inventory.pool_id.clone(),
            }),
        _ => None,
    };
    if let Some(target) = target
        && let Some(connection_id) = key.connection_id()
        && ui.button(RichText::new("Properties…").size(11.0)).clicked()
    {
        actions.push(UiAction::OpenProperties {
            connection_id: connection_id.to_owned(),
            target,
        });
    }
}

fn scope_pill(ui: &mut Ui, state: &UiState, workspace: &WorkspaceView) {
    let name = scope_name(workspace, &state.scope);
    let connection = state
        .scope
        .connection_id()
        .and_then(|id| find_connection(workspace, id));
    let label = if connection.is_some_and(is_read_only) {
        format!("{name} · RO")
    } else {
        name
    };
    let status_color = match connection {
        Some(connection) => widgets::connection_color(crate::state::connection_status(connection)),
        None if !connected_connections(workspace).is_empty() => green(),
        None => dim_fg(),
    };
    Frame::NONE
        .fill(panel_2())
        .stroke(Stroke::new(1.0, border()))
        .corner_radius(CornerRadius::same(20))
        .inner_margin(Margin::symmetric(10, 3))
        .show(ui, |ui| {
            ui.set_max_width(230.0);
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 5.0;
                widgets::dot(ui, status_color, 5.0);
                ui.add(
                    egui::Label::new(RichText::new(label).size(10.5).color(muted_fg())).truncate(),
                );
            });
        });
}

fn window_controls(ui: &mut Ui, ctx: &egui::Context) {
    let maximized = ctx.input(|input| input.viewport().maximized.unwrap_or(false));
    if window_button(ui, WindowButton::Close, false) {
        ctx.send_viewport_cmd(ViewportCommand::Close);
    }
    if window_button(ui, WindowButton::Maximize, maximized) {
        toggle_maximized(ctx);
    }
    if window_button(ui, WindowButton::Minimize, false) {
        ctx.send_viewport_cmd(ViewportCommand::Minimized(true));
    }
}

#[derive(Clone, Copy)]
enum WindowButton {
    Minimize,
    Maximize,
    Close,
}

fn window_button(ui: &mut Ui, kind: WindowButton, maximized: bool) -> bool {
    let (rect, response) = ui.allocate_exact_size(vec2(34.0, 30.0), Sense::click());
    let hovered = response.hovered();
    let close = matches!(kind, WindowButton::Close);
    if hovered {
        let fill = if close { red() } else { panel_3() };
        ui.painter().rect_filled(rect, CornerRadius::same(6), fill);
    }
    let icon = if hovered && close {
        Color32::WHITE
    } else if hovered {
        fg()
    } else {
        muted_fg()
    };
    let painter = ui.painter();
    let center = rect.center();
    match kind {
        WindowButton::Minimize => {
            painter.hline(
                Rangef::new(center.x - 4.0, center.x + 4.0),
                center.y + 3.0,
                Stroke::new(1.2, icon),
            );
        }
        WindowButton::Close => {
            for (a, b) in [
                (vec2(-4.0, -4.0), vec2(4.0, 4.0)),
                (vec2(4.0, -4.0), vec2(-4.0, 4.0)),
            ] {
                painter.line_segment([center + a, center + b], Stroke::new(1.2, icon));
            }
        }
        WindowButton::Maximize => {
            if maximized {
                painter.rect_stroke(
                    Rect::from_center_size(center + vec2(-1.5, 1.5), Vec2::splat(7.0)),
                    CornerRadius::ZERO,
                    Stroke::new(1.1, icon),
                    egui::StrokeKind::Inside,
                );
                painter.rect_stroke(
                    Rect::from_center_size(center + vec2(1.5, -1.5), Vec2::splat(7.0)),
                    CornerRadius::ZERO,
                    Stroke::new(1.1, icon),
                    egui::StrokeKind::Inside,
                );
            } else {
                painter.rect_stroke(
                    Rect::from_center_size(center, Vec2::splat(9.0)),
                    CornerRadius::ZERO,
                    Stroke::new(1.1, icon),
                    egui::StrokeKind::Inside,
                );
            }
        }
    }
    response
        .on_hover_text(match kind {
            WindowButton::Minimize => "Minimize",
            WindowButton::Maximize => {
                if maximized {
                    "Restore"
                } else {
                    "Maximize"
                }
            }
            WindowButton::Close => "Close",
        })
        .clicked()
}

fn toggle_maximized(ctx: &egui::Context) {
    let maximized = ctx.input(|input| input.viewport().maximized.unwrap_or(false));
    ctx.send_viewport_cmd(ViewportCommand::Maximized(!maximized));
}

fn theme_toggle(ui: &mut Ui, ctx: &egui::Context) {
    let (rect, response) = ui.allocate_exact_size(vec2(30.0, 30.0), Sense::click());
    let hovered = response.hovered();
    let background = if hovered { panel_3() } else { panel() };
    if hovered {
        ui.painter()
            .rect_filled(rect, CornerRadius::same(6), background);
    }
    let painter = ui.painter();
    let center = rect.center();
    let color = if hovered { fg() } else { muted_fg() };
    if theme::is_light() {
        // Moon: a filled disc with an offset cut-out.
        painter.circle_filled(center + vec2(0.5, 0.0), 5.0, color);
        painter.circle_filled(center + vec2(2.8, -1.8), 4.4, background);
    } else {
        // Sun.
        painter.circle_stroke(center, 3.8, Stroke::new(1.2, color));
        for index in 0..8 {
            let angle = index as f32 * std::f32::consts::TAU / 8.0;
            let direction = vec2(angle.cos(), angle.sin());
            painter.line_segment(
                [center + direction * 5.6, center + direction * 7.4],
                Stroke::new(1.2, color),
            );
        }
    }
    if response
        .on_hover_text(if theme::is_light() {
            "Switch to dark theme"
        } else {
            "Switch to light theme"
        })
        .clicked()
    {
        let light = theme::toggle();
        theme::apply(ctx);
        config::save_ui_preferences(&config::UiPreferences {
            light,
            zoom: ctx.zoom_factor(),
        });
    }
}

fn zoom_control(ui: &mut Ui, ctx: &egui::Context) {
    let current = ctx.zoom_factor();
    let label = format!("{:.0}%", current * 100.0);
    if ui
        .add(Button::new(RichText::new(label).size(10.5).color(muted_fg())).frame(false))
        .on_hover_text("UI scale — click to cycle · Ctrl+Plus / Ctrl+Minus / Ctrl+0")
        .clicked()
    {
        let next = if current < 0.95 {
            1.0
        } else if current < 1.05 {
            1.1
        } else if current < 1.2 {
            1.25
        } else if current < 1.35 {
            1.4
        } else {
            0.9
        };
        ctx.set_zoom_factor(next);
        config::save_ui_preferences(&config::UiPreferences {
            light: theme::is_light(),
            zoom: next,
        });
    }
}

pub fn footer(ui: &mut Ui, state: &mut UiState, workspace: &WorkspaceView) {
    egui::Panel::bottom("footer")
        .exact_size(30.0)
        .resizable(false)
        .frame(
            Frame::NONE
                .fill(bg())
                .stroke(Stroke::new(1.0, border_soft()))
                .inner_margin(Margin::symmetric(18, 0)),
        )
        .show(ui, |ui| {
            ui.horizontal_centered(|ui| {
                widgets::dot(
                    ui,
                    if connected_connections(workspace).is_empty() {
                        dim_fg()
                    } else {
                        green()
                    },
                    5.0,
                );
                let text = state.updated.map_or_else(
                    || "Task status updates automatically · Inventory refresh is manual".to_owned(),
                    |updated| format!("Inventory refreshed {}", updated.format("%H:%M:%S")),
                );
                ui.label(RichText::new(text).size(9.5).color(dim_fg()));
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    ui.label(
                        RichText::new("Rust + egui")
                            .monospace()
                            .size(9.0)
                            .color(dim_fg()),
                    );
                    ui.label(RichText::new("·").size(9.0).color(dim_fg()));
                    ui.label(RichText::new("Xen Desktop").size(9.5).color(dim_fg()));
                });
            });
        });
}

/// Invisible edge handles for the undecorated window.
pub fn resize_edges(ui: &mut Ui) {
    let ctx = ui.ctx().clone();
    let rect = ctx.content_rect();
    let edge = 4.0;
    let corner = 10.0;
    let handles: [(Rect, egui::viewport::ResizeDirection, CursorIcon); 8] = [
        (
            Rect::from_min_max(
                pos2(rect.left() + corner, rect.top()),
                pos2(rect.right() - corner, rect.top() + edge),
            ),
            egui::viewport::ResizeDirection::North,
            CursorIcon::ResizeVertical,
        ),
        (
            Rect::from_min_max(
                pos2(rect.left() + corner, rect.bottom() - edge),
                pos2(rect.right() - corner, rect.bottom()),
            ),
            egui::viewport::ResizeDirection::South,
            CursorIcon::ResizeVertical,
        ),
        (
            Rect::from_min_max(
                pos2(rect.left(), rect.top() + corner),
                pos2(rect.left() + edge, rect.bottom() - corner),
            ),
            egui::viewport::ResizeDirection::West,
            CursorIcon::ResizeHorizontal,
        ),
        (
            Rect::from_min_max(
                pos2(rect.right() - edge, rect.top() + corner),
                pos2(rect.right(), rect.bottom() - corner),
            ),
            egui::viewport::ResizeDirection::East,
            CursorIcon::ResizeHorizontal,
        ),
        (
            Rect::from_min_max(rect.min, pos2(rect.left() + corner, rect.top() + corner)),
            egui::viewport::ResizeDirection::NorthWest,
            CursorIcon::ResizeNwSe,
        ),
        (
            Rect::from_min_max(
                pos2(rect.right() - corner, rect.top()),
                pos2(rect.right(), rect.top() + corner),
            ),
            egui::viewport::ResizeDirection::NorthEast,
            CursorIcon::ResizeNeSw,
        ),
        (
            Rect::from_min_max(
                pos2(rect.left(), rect.bottom() - corner),
                pos2(rect.left() + corner, rect.bottom()),
            ),
            egui::viewport::ResizeDirection::SouthWest,
            CursorIcon::ResizeNeSw,
        ),
        (
            Rect::from_min_max(
                pos2(rect.right() - corner, rect.bottom() - corner),
                rect.max,
            ),
            egui::viewport::ResizeDirection::SouthEast,
            CursorIcon::ResizeNwSe,
        ),
    ];
    for (index, (rect, direction, cursor)) in handles.into_iter().enumerate() {
        let response = ui.interact(
            rect,
            egui::Id::unique(("resize-handle", index)),
            Sense::drag(),
        );
        if response.hovered() {
            ctx.set_cursor_icon(cursor);
        }
        if response.drag_started() {
            ctx.send_viewport_cmd(ViewportCommand::BeginResize(direction));
        }
    }
}

pub fn scope_name(workspace: &WorkspaceView, scope: &Scope) -> String {
    match scope {
        Scope::All => "All connections".into(),
        Scope::Connection(id) => {
            find_connection(workspace, id).map_or_else(|| id.clone(), crate::state::connection_name)
        }
        Scope::Host {
            connection_id,
            host_ref,
        } => find_connection(workspace, connection_id)
            .and_then(|connection| {
                connection_inventory(connection).and_then(|inventory| {
                    inventory
                        .hosts
                        .iter()
                        .find(|host| &host.reference == host_ref)
                        .map(|host| host.name.clone())
                })
            })
            .unwrap_or_else(|| "Host".into()),
    }
}

pub fn eyebrow(ui: &mut Ui, text: &str) {
    ui.label(
        RichText::new(text.to_uppercase())
            .size(9.0)
            .color(with_alpha(accent(), 220)),
    );
}

pub fn error_banner(ui: &mut Ui, text: &str) {
    Frame::NONE
        .fill(with_alpha(red(), 18))
        .stroke(Stroke::new(1.0, with_alpha(red(), 90)))
        .corner_radius(CornerRadius::same(8))
        .inner_margin(Margin::symmetric(12, 8))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(RichText::new("!").size(11.0).strong().color(red()));
                ui.add(egui::Label::new(RichText::new(text).size(11.0).color(fg())).wrap());
            });
        });
}
