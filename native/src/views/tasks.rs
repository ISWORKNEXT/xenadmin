use std::collections::HashSet;

use egui::{Align, Frame, Layout, RichText, Stroke, Ui};
use xen_core::{operations::TaskStatus, workspace::WorkspaceView, xapi::ExternalTask};

use crate::format;
use crate::state::{
    UiAction, UiState, connection_id, connection_inventory, connection_name, scoped_tasks,
    task_status_label,
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
    let tasks = scoped_tasks(workspace, &state.scope);
    let owned: HashSet<&str> = tasks
        .iter()
        .filter(|task| !task.remote_ref.is_empty())
        .map(|task| task.remote_ref.as_str())
        .collect();
    let mut external: Vec<(&ExternalTask, String)> = workspace
        .connections
        .iter()
        .filter(|connection| {
            state
                .scope
                .connection_id()
                .is_none_or(|id| id == connection_id(connection))
        })
        .filter_map(|connection| {
            let name = connection_name(connection).to_owned();
            connection_inventory(connection).map(|inventory| {
                inventory
                    .tasks
                    .iter()
                    .filter(|task| !owned.contains(task.reference.as_str()))
                    .map(move |task| (task, name.clone()))
            })
        })
        .flatten()
        .collect();
    external.sort_by_key(|entry| std::cmp::Reverse(entry.0.started_at));
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
        } else {
            for task in tasks {
                task_row(ui, task, actions);
            }
        }

        if !external.is_empty() {
            ui.add_space(12.0);
            widgets::panel_heading(
                ui,
                &format!("Host tasks  {}", external.len()),
                "XenAPI tasks reported by the host, including other clients",
                |_ui| {},
            );
            ui.add_space(4.0);
            for (task, connection) in external.iter().take(60) {
                external_task_row(ui, task, connection);
            }
        }
    });
}

fn external_task_row(ui: &mut Ui, task: &ExternalTask, connection: &str) {
    let (label, color) = match task.status.as_str() {
        "pending" => ("running", accent()),
        "success" => ("succeeded", green()),
        "failure" => ("failed", red()),
        "cancelled" => ("cancelled", dim_fg()),
        other => (other, muted_fg()),
    };
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
                        ui.label(RichText::new(&task.name).size(11.5).strong());
                        ui.label(RichText::new(label).size(9.5).color(color));
                        if task.status == "pending" {
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
                            "{} · {}{}",
                            connection,
                            if task.host_name.is_empty() {
                                "unknown host"
                            } else {
                                &task.host_name
                            },
                            if task.started_at > 0 {
                                format!(" · started {}", format::short_time(task.started_at))
                            } else {
                                String::new()
                            }
                        ))
                        .size(9.0)
                        .color(dim_fg()),
                    );
                    if task.status == "pending" {
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
                    if !task.error.is_empty() {
                        ui.label(RichText::new(&task.error).size(9.5).color(red()));
                    }
                });
            });
        });
    ui.add_space(6.0);
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
