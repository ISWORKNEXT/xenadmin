use egui::{
    Align, Align2, Button, Color32, CornerRadius, CursorIcon, FontId, Frame, Layout, Margin,
    Rangef, Rect, RichText, Sense, Stroke, Ui, Vec2, ViewportCommand, pos2, vec2,
};
use xen_core::workspace::WorkspaceView;

use crate::state::{
    Scope, UiAction, UiState, View, connected_connections, connection_inventory, find_connection,
    is_read_only,
};
use crate::theme;
use crate::theme::*;
use crate::widgets;

const TITLEBAR_HEIGHT: f32 = 46.0;

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

            // The whole strip is draggable; buttons added later take precedence.
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

            let mut selected_rect: Option<Rect> = None;
            ui.horizontal_centered(|ui| {
                ui.spacing_mut().item_spacing.x = 5.0;
                brand(ui);
                ui.add_space(3.0);
                separator(ui);
                ui.add_space(3.0);

                for (index, view) in View::ALL.iter().enumerate() {
                    let selected = if state.host_route.is_some() {
                        *view == View::Hosts
                    } else {
                        state.view == *view
                    };
                    let color = if selected { fg() } else { muted_fg() };
                    let response = ui
                        .add(
                            Button::new(RichText::new(view.tab_label()).size(11.5).color(color))
                                .frame(false)
                                .corner_radius(CornerRadius::same(6)),
                        )
                        .on_hover_text(format!("{}  ·  Ctrl+{}", view.title(), index + 1));
                    if selected {
                        selected_rect = Some(response.rect);
                    }
                    if response.clicked() {
                        actions.push(UiAction::SetView(*view));
                    }
                }

                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    ui.spacing_mut().item_spacing.x = 0.0;
                    window_controls(ui, &ctx);
                    ui.add_space(8.0);
                    theme_toggle(ui, &ctx);
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
                        actions.push(UiAction::SetView(View::Tasks));
                    }
                    ui.add_space(8.0);
                    if ui.available_width() > 260.0 {
                        scope_pill(ui, state, workspace);
                    }
                });
            });

            if let Some(rect) = selected_rect {
                ui.painter().hline(
                    Rangef::new(rect.left() + 2.0, rect.right() - 2.0),
                    bar_rect.bottom() - 1.0,
                    Stroke::new(2.0, accent()),
                );
            }
        });
}

fn brand(ui: &mut Ui) {
    let (rect, _) = ui.allocate_exact_size(Vec2::splat(26.0), Sense::hover());
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
        FontId::proportional(13.0),
        accent(),
    );
    ui.label(RichText::new("Xen Desktop").size(12.5).strong().color(fg()));
    ui.label(RichText::new("XCP-ng").size(9.0).color(dim_fg()));
}

fn separator(ui: &mut Ui) {
    let (rect, _) = ui.allocate_exact_size(vec2(1.0, 18.0), Sense::hover());
    ui.painter()
        .vline(rect.center().x, rect.y_range(), Stroke::new(1.0, border()));
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
                    egui::Label::new(RichText::new(label).size(9.5).color(muted_fg())).truncate(),
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
        crate::config::save_light_preference(light);
    }
}

pub fn footer(ui: &mut Ui, state: &mut UiState, workspace: &WorkspaceView) {
    egui::Panel::bottom("footer")
        .exact_size(28.0)
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
