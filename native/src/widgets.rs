use egui::{
    Align2, Button, Color32, CornerRadius, FontId, Frame, Label, Margin, Rect, Response, RichText,
    Sense, Stroke, Ui, Vec2,
};

use crate::state::{ToastKind, UiAction, UiState};
use crate::theme::*;

pub fn badge(ui: &mut Ui, text: impl Into<String>, color: Color32) -> Response {
    let text = text.into();
    let font = FontId::proportional(11.0);
    let galley = ui.painter().layout_no_wrap(text, font, color);
    let size = Vec2::new(galley.size().x + 18.0, galley.size().y + 6.0);
    let (rect, response) = ui.allocate_exact_size(size, Sense::hover());
    ui.painter()
        .rect_filled(rect, CornerRadius::same(20), with_alpha(color, 26));
    ui.painter()
        .galley(rect.min + Vec2::new(9.0, 3.0), galley, color);
    response
}

pub fn dot(ui: &mut Ui, color: Color32, size: f32) {
    let (rect, _) = ui.allocate_exact_size(Vec2::splat(size), Sense::hover());
    ui.painter().circle_filled(rect.center(), size / 2.0, color);
}

pub fn power_badge(ui: &mut Ui, state: &str) -> Response {
    let (label, color) = power_style(state);
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 4.0;
        dot(ui, color, 6.0);
        ui.label(RichText::new(label).size(12.5).color(color));
    })
    .response
}

pub fn power_style(state: &str) -> (&'static str, Color32) {
    match state {
        "Running" => ("Running", green()),
        "Halted" => ("Stopped", muted_fg()),
        "Paused" => ("Paused", amber()),
        "Suspended" => ("Suspended", purple()),
        "Unassigned" => ("Unassigned", dim_fg()),
        _ => ("Unknown", dim_fg()),
    }
}

pub fn connection_color(status: &str) -> Color32 {
    match status {
        "online" => green(),
        "stale" => amber(),
        _ => dim_fg(),
    }
}

pub fn task_color(status: &str) -> Color32 {
    match status {
        "running" => accent(),
        "succeeded" => green(),
        "failed" => red(),
        "cancelled" => amber(),
        _ => purple(),
    }
}

pub fn resource_bar(ui: &mut Ui, used: u64, total: u64, color: Color32, height: f32) {
    let width = ui.available_width().max(10.0);
    let (rect, _) = ui.allocate_exact_size(Vec2::new(width, height), Sense::hover());
    let painter = ui.painter();
    painter.rect_filled(rect, CornerRadius::same((height / 2.0) as u8), panel_3());
    let fraction = if total == 0 {
        0.0
    } else {
        (used as f64 / total as f64).clamp(0.0, 1.0) as f32
    };
    if fraction > 0.0 {
        let fill = Rect::from_min_size(
            rect.min,
            Vec2::new((rect.width() * fraction).max(height), height),
        );
        painter.rect_filled(fill, CornerRadius::same((height / 2.0) as u8), color);
    }
}

pub fn search_field(ui: &mut Ui, value: &mut String, hint: &str) -> Response {
    ui.add(
        egui::TextEdit::singleline(value)
            .hint_text(RichText::new(hint).color(dim_fg()))
            .desired_width(f32::INFINITY)
            .margin(Margin::symmetric(8, 5)),
    )
}

pub fn section_title(ui: &mut Ui, title: &str, subtitle: Option<&str>) {
    ui.label(RichText::new(title).size(15.0).strong().color(fg()));
    if let Some(subtitle) = subtitle {
        ui.label(RichText::new(subtitle).size(12.0).color(muted_fg()));
    }
}

pub fn panel_heading(ui: &mut Ui, title: &str, subtitle: &str, right: impl FnOnce(&mut Ui)) {
    ui.horizontal(|ui| {
        ui.vertical(|ui| {
            ui.label(RichText::new(title).size(15.0).strong());
            ui.label(RichText::new(subtitle).size(12.0).color(muted_fg()));
        });
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            right(ui);
        });
    });
}

pub fn kv(ui: &mut Ui, label: &str, value: &str, mono: bool) {
    ui.horizontal(|ui| {
        ui.add_sized(
            [130.0, 18.0],
            Label::new(RichText::new(label).size(12.5).color(muted_fg())),
        );
        let text = if mono {
            RichText::new(value).monospace().size(12.5)
        } else {
            RichText::new(value).size(13.0)
        };
        ui.add(Label::new(text).wrap());
    });
}

pub fn info_groups(ui: &mut Ui, groups: &[xen_core::host::InfoGroup], columns: usize) {
    if groups.is_empty() {
        empty_state(
            ui,
            "No information",
            "The host did not return information for this section.",
        );
        return;
    }
    let columns = columns.max(1).min(groups.len());
    let mut index = 0;
    while index < groups.len() {
        let end = (index + columns).min(groups.len());
        ui.columns(columns, |cols| {
            for (offset, col) in cols.iter_mut().enumerate() {
                if let Some(group) = groups.get(index + offset) {
                    group_card(col, group);
                }
            }
        });
        index = end;
        ui.add_space(GAP);
    }
}

fn group_card(ui: &mut Ui, group: &xen_core::host::InfoGroup) {
    card_frame().inner_margin(Margin::same(12)).show(ui, |ui| {
        ui.set_width(ui.available_width());
        if !group.title.is_empty() {
            ui.label(RichText::new(&group.title).size(12.5).strong().color(fg()));
            ui.add_space(4.0);
        }
        egui::Grid::new(("info-group", &group.title))
            .num_columns(2)
            .spacing([14.0, 5.0])
            .show(ui, |ui| {
                for field in &group.fields {
                    ui.label(RichText::new(&field.label).size(11.0).color(muted_fg()));
                    ui.add(Label::new(RichText::new(&field.value).size(11.5)).wrap());
                    ui.end_row();
                }
            });
    });
}

pub fn empty_state(ui: &mut Ui, title: &str, description: &str) {
    ui.add_space(26.0);
    ui.vertical_centered(|ui| {
        let (rect, _) = ui.allocate_exact_size(Vec2::splat(44.0), Sense::hover());
        let painter = ui.painter();
        painter.rect_filled(rect, CornerRadius::same(12), panel_2());
        painter.rect_stroke(
            rect,
            CornerRadius::same(12),
            Stroke::new(1.0, border()),
            egui::StrokeKind::Inside,
        );
        let center = rect.center();
        painter.circle_stroke(center, 8.5, Stroke::new(1.2, dim_fg()));
        painter.line_segment(
            [center - Vec2::new(0.0, 4.0), center + Vec2::new(0.0, 4.0)],
            Stroke::new(1.2, dim_fg()),
        );
        painter.line_segment(
            [center - Vec2::new(4.0, 0.0), center + Vec2::new(4.0, 0.0)],
            Stroke::new(1.2, dim_fg()),
        );
        ui.add_space(6.0);
        ui.label(RichText::new(title).size(13.5).strong());
        ui.label(RichText::new(description).size(11.5).color(muted_fg()));
    });
    ui.add_space(26.0);
}

/// Small painted server mark used on host cards and host detail headers.
pub fn server_mark(ui: &mut Ui, size: f32) {
    let (rect, _) = ui.allocate_exact_size(Vec2::splat(size), Sense::hover());
    let painter = ui.painter();
    painter.rect_filled(rect, CornerRadius::same((size * 0.24) as u8), panel_2());
    painter.rect_stroke(
        rect,
        CornerRadius::same((size * 0.24) as u8),
        Stroke::new(1.0, border()),
        egui::StrokeKind::Inside,
    );
    let unit = size / 34.0;
    let left = rect.left() + 8.0 * unit;
    for row in 0..3 {
        let y = rect.top() + (10.0 + row as f32 * 6.0) * unit;
        painter.line_segment(
            [egui::pos2(left, y), egui::pos2(left + 17.0 * unit, y)],
            Stroke::new(1.4 * unit, muted_fg()),
        );
        painter.circle_filled(egui::pos2(left + 20.5 * unit, y), 1.4 * unit, accent());
    }
}

/// Painted offline indicator (circle with a diagonal slash).
pub fn offline_mark(ui: &mut Ui, size: f32) {
    let (rect, _) = ui.allocate_exact_size(Vec2::splat(size), Sense::hover());
    let painter = ui.painter();
    painter.rect_filled(rect, CornerRadius::same((size * 0.25) as u8), panel_2());
    painter.rect_stroke(
        rect,
        CornerRadius::same((size * 0.25) as u8),
        Stroke::new(1.0, border()),
        egui::StrokeKind::Inside,
    );
    let center = rect.center();
    let radius = size * 0.20;
    painter.circle_stroke(center, radius, Stroke::new(1.4, dim_fg()));
    let offset = radius * 0.70;
    painter.line_segment(
        [
            center + Vec2::new(-offset, offset),
            center + Vec2::new(offset, -offset),
        ],
        Stroke::new(1.4, dim_fg()),
    );
}

pub fn primary_button(text: impl Into<String>) -> Button<'static> {
    Button::new(RichText::new(text.into()).color(Color32::WHITE).size(12.0))
        .fill(accent())
        .stroke(Stroke::NONE)
}

pub fn field_label(ui: &mut Ui, text: &str) {
    ui.label(RichText::new(text).size(11.0).color(muted_fg()));
}

pub fn toasts(ctx: &egui::Context, state: &UiState, actions: &mut Vec<UiAction>) {
    if state.toasts.is_empty() {
        return;
    }
    egui::Area::new(egui::Id::unique("toast-area"))
        .anchor(Align2::RIGHT_BOTTOM, [-18.0, -42.0])
        .order(egui::Order::Foreground)
        .show(ctx, |ui| {
            ui.set_max_width(430.0);
            for (index, toast) in state.toasts.iter().enumerate() {
                let (color, icon) = match toast.kind {
                    ToastKind::Info => (accent(), "i"),
                    ToastKind::Success => (green(), "✓"),
                    ToastKind::Error => (red(), "!"),
                };
                Frame::NONE
                    .fill(panel())
                    .stroke(Stroke::new(1.0, color.gamma_multiply(0.55)))
                    .corner_radius(CornerRadius::same(9))
                    .inner_margin(Margin::symmetric(12, 9))
                    .shadow(egui::epaint::Shadow {
                        offset: [0, 6],
                        blur: 18,
                        spread: 0,
                        color: Color32::from_black_alpha(120),
                    })
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.label(RichText::new(icon).size(11.0).strong().color(color));
                            ui.add(
                                Label::new(RichText::new(&toast.text).size(11.5))
                                    .wrap()
                                    .selectable(false),
                            );
                            if ui
                                .add(
                                    Button::new(RichText::new("×").size(13.0).color(muted_fg()))
                                        .frame(false),
                                )
                                .clicked()
                            {
                                actions.push(UiAction::DismissToast(index));
                            }
                        });
                    });
                ui.add_space(6.0);
            }
        });
}

pub fn tab_bar(ui: &mut Ui, tabs: &[&str], current: &mut usize) {
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing.x = 4.0;
        for (index, label) in tabs.iter().enumerate() {
            let selected = index == *current;
            let response = ui.add(
                Button::new(RichText::new(*label).size(11.5).color(if selected {
                    fg()
                } else {
                    muted_fg()
                }))
                .frame(false)
                .corner_radius(CornerRadius::same(6)),
            );
            if selected {
                let rect = response.rect;
                ui.painter().hline(
                    rect.x_range(),
                    rect.bottom() + 1.0,
                    Stroke::new(1.5, accent()),
                );
            }
            if response.clicked() {
                *current = index;
            }
        }
    });
}
