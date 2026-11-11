use crate::state::{Scope, UiAction, UiState, connection_inventory, find_connection};
use crate::theme::*;
use egui::{
    Align2, Color32, CornerRadius, Frame, Margin, Rangef, Rect, RichText, Sense, Stroke, Ui, Vec2,
    vec2,
};
use xen_core::workspace::WorkspaceView;

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
