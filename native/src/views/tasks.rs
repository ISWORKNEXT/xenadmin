use egui::{Align, Frame, Layout, RichText, Stroke, Ui};
use xen_core::{operations::TaskStatus, workspace::WorkspaceView};

use crate::format;
use crate::state::{UiAction, UiState, scoped_tasks, task_status_label};
use crate::theme;
use crate::theme::*;
use crate::widgets;

pub fn show(
    ui: &mut Ui,
    state: &mut UiState,
    workspace: &WorkspaceView,
    actions: &mut Vec<UiAction>,
) {
    let tasks = scoped_tasks(workspace, &state.scope);
    let mut card = theme::card_frame();
    card = card.inner_margin(egui::Margin::same(14));
    card.show(ui, |ui| {
        ui.set_width(ui.available_width());
        widgets::panel_heading(
            ui,
            &format!("Activity & tasks  {}", tasks.len()),
            "XenAPI task results, not assumed success from a button click",
            |_ui| {},
        );
        ui.add_space(4.0);
        if tasks.is_empty() {
            widgets::empty_state(
                ui,
                "No operations yet",
                "VM actions appear here with their connection, progress and result.",
            );
            return;
        }
        for task in tasks {
            task_row(ui, task, actions);
        }
    });
}

fn task_row(ui: &mut Ui, task: &xen_core::workspace::Operation, actions: &mut Vec<UiAction>) {
    let label = task_status_label(task.status);
    let color = widgets::task_color(label);
    Frame::NONE
        .fill(panel_2())
        .stroke(Stroke::new(1.0, border_soft()))
        .corner_radius(egui::CornerRadius::same(8))
        .inner_margin(egui::Margin::symmetric(12, 9))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                widgets::dot(ui, color, 8.0);
                ui.vertical(|ui| {
                    ui.spacing_mut().item_spacing.y = 2.0;
                    ui.horizontal(|ui| {
                        ui.label(
                            RichText::new(format!("{} · {}", task.action, task.object_name))
                                .size(11.5)
                                .strong(),
                        );
                        ui.label(RichText::new(label).size(9.5).color(color));
                        if task.status == TaskStatus::Running {
                            ui.label(
                                RichText::new(format!("{:.0}%", task.progress * 100.0))
                                    .monospace()
                                    .size(9.0)
                                    .color(muted_fg()),
                            );
                        }
                    });
                    ui.label(
                        RichText::new(format!(
                            "{} · started {}",
                            task.connection_name,
                            format::short_time(task.started_at)
                        ))
                        .size(9.0)
                        .color(dim_fg()),
                    );
                    if task.status == TaskStatus::Running {
                        ui.scope(|ui| {
                            ui.set_width((ui.available_width() - 8.0).max(120.0));
                            widgets::resource_bar(
                                ui,
                                (task.progress.clamp(0.0, 1.0) * 1000.0) as u64,
                                1000,
                                accent(),
                                4.0,
                            );
                        });
                    }
                    if !task.message.is_empty() {
                        ui.label(RichText::new(&task.message).size(9.5).color(
                            if task.status == TaskStatus::Failed {
                                red()
                            } else {
                                muted_fg()
                            },
                        ));
                    }
                });
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if task.status == TaskStatus::Unreachable
                        && ui
                            .add(egui::Button::new(RichText::new("Check again").size(10.0)))
                            .clicked()
                    {
                        actions.push(UiAction::CheckTask(task.id.clone()));
                    }
                    if task.can_cancel
                        && task.status == TaskStatus::Running
                        && ui
                            .add(
                                egui::Button::new(RichText::new("Cancel task").size(10.0))
                                    .stroke(Stroke::new(1.0, with_alpha(red(), 120))),
                            )
                            .clicked()
                    {
                        actions.push(UiAction::CancelTask(task.id.clone()));
                    }
                });
            });
        });
    ui.add_space(6.0);
}
