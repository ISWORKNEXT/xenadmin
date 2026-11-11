use egui::{Align, Layout, RichText, Sense, Ui, Vec2};
use xen_core::health::{DiagnosticCheck, HealthReport, Severity};
use xen_core::workspace::WorkspaceView;

use crate::format;
use crate::state::{
    ObjectKey, UiAction, UiState, connection_inventory, connection_name, find_connection,
    is_read_only,
};
use crate::theme;
use crate::theme::*;
use crate::widgets;

pub fn show(
    ui: &mut Ui,
    state: &mut UiState,
    workspace: &WorkspaceView,
    actions: &mut Vec<UiAction>,
) {
    let Some(ObjectKey::Health { connection_id }) =
        state.selection.as_ref().map(|tab| tab.key.clone())
    else {
        return;
    };
    let Some(connection) = find_connection(workspace, &connection_id) else {
        widgets::empty_state(
            ui,
            "Connection unavailable",
            "The saved connection was removed.",
        );
        return;
    };
    let Some(inventory) = connection_inventory(connection) else {
        widgets::empty_state(
            ui,
            "Health data unavailable",
            "Connect to this host to run diagnostics.",
        );
        return;
    };
    let report: &HealthReport = &inventory.health;
    let overall = report.worst();

    ui.horizontal(|ui| {
        let (rect, _) = ui.allocate_exact_size(Vec2::splat(42.0), Sense::hover());
        ui.painter()
            .rect_filled(rect, egui::CornerRadius::same(11), panel_2());
        ui.painter().rect_stroke(
            rect,
            egui::CornerRadius::same(11),
            egui::Stroke::new(1.0, border()),
            egui::StrokeKind::Inside,
        );
        ui.painter().text(
            rect.center(),
            egui::Align2::CENTER_CENTER,
            "H",
            egui::FontId::proportional(17.0),
            severity_color(overall),
        );
        ui.vertical(|ui| {
            ui.spacing_mut().item_spacing.y = 0.0;
            ui.label(
                RichText::new("HEALTH & DIAGNOSTICS")
                    .size(8.5)
                    .color(dim_fg()),
            );
            ui.label(
                RichText::new(connection_name(connection))
                    .size(23.0)
                    .strong(),
            );
            ui.label(
                RichText::new(format!(
                    "Checked {} · read-only checks",
                    if report.checked_at == 0 {
                        "—".to_owned()
                    } else {
                        format::short_time(report.checked_at)
                    }
                ))
                .size(9.5)
                .color(muted_fg()),
            );
        });
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            if ui
                .add(egui::Button::new(RichText::new("Refresh").size(11.0)))
                .clicked()
            {
                actions.push(UiAction::Refresh(Some(connection_id.clone())));
            }
            widgets::badge(ui, overall.label(), severity_color(overall));
            if is_read_only(connection) {
                widgets::badge(ui, "Read-only", amber());
            }
        });
    });
    ui.add_space(10.0);

    summary(ui, report);
    ui.add_space(8.0);

    for check in &report.checks {
        check_card(ui, check);
        ui.add_space(6.0);
    }
}

fn summary(ui: &mut Ui, report: &HealthReport) {
    ui.columns(4, |columns| {
        for (index, (label, severity)) in [
            ("Critical", Severity::Critical),
            ("Warning", Severity::Warning),
            ("Info", Severity::Info),
            ("OK", Severity::Ok),
        ]
        .into_iter()
        .enumerate()
        {
            let column = &mut columns[index];
            theme::card_frame()
                .inner_margin(egui::Margin::same(12))
                .show(column, |ui| {
                    ui.set_width(ui.available_width());
                    ui.horizontal(|ui| {
                        widgets::dot(ui, severity_color(severity), 7.0);
                        ui.label(RichText::new(label).size(10.0).color(muted_fg()));
                    });
                    ui.label(
                        RichText::new(report.count(severity).to_string())
                            .size(21.0)
                            .strong(),
                    );
                    ui.label(RichText::new("checks").size(9.0).color(dim_fg()));
                });
        }
    });
}

fn check_card(ui: &mut Ui, check: &DiagnosticCheck) {
    theme::card_frame().show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.horizontal(|ui| {
            widgets::dot(ui, severity_color(check.severity), 8.0);
            ui.vertical(|ui| {
                ui.spacing_mut().item_spacing.y = 1.0;
                ui.horizontal(|ui| {
                    ui.label(RichText::new(&check.title).size(12.5).strong());
                    ui.label(
                        RichText::new(check.severity.label())
                            .size(9.0)
                            .color(severity_color(check.severity)),
                    );
                });
                ui.label(RichText::new(&check.summary).size(10.5).color(muted_fg()));
            });
        });
        if !check.details.is_empty() {
            ui.add_space(5.0);
            for detail in &check.details {
                ui.horizontal(|ui| {
                    ui.add_space(16.0);
                    ui.label(RichText::new(format!("• {detail}")).size(10.0).color(
                        if check.severity.rank() >= Severity::Warning.rank() {
                            fg()
                        } else {
                            muted_fg()
                        },
                    ));
                });
            }
        }
    });
}

fn severity_color(severity: Severity) -> egui::Color32 {
    match severity {
        Severity::Critical => red(),
        Severity::Warning => amber(),
        Severity::Info => accent(),
        Severity::Ok => green(),
    }
}
