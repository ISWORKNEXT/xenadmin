use egui::{Align, Color32, Label, Layout, Margin, RichText, Sense, Ui};
use xen_core::workspace::{ConnectionView, WorkspaceView};

use crate::format;
use crate::state::{
    Scope, Target, UiAction, UiState, View, connection_id, connection_name, connection_settings,
    connection_status, is_read_only, scoped_hosts, scoped_targets, scoped_tasks,
    visible_connections,
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
    let connections = visible_connections(workspace, &state.scope);
    let hosts = scoped_hosts(workspace, &state.scope);
    let vms = scoped_targets(workspace, &state.scope);
    let tasks = scoped_tasks(workspace, &state.scope);
    let running = tasks
        .iter()
        .filter(|task| matches!(task.status, xen_core::operations::TaskStatus::Running))
        .count();
    let offline = workspace
        .connections
        .iter()
        .filter(|connection| matches!(connection, ConnectionView::Offline { .. }))
        .count();

    ui.columns(4, |columns| {
        stat_card(
            &mut columns[0],
            "Connected pools",
            connections.len().to_string(),
            &format!("{offline} saved offline"),
            accent(),
        );
        stat_card(
            &mut columns[1],
            "Hosts",
            hosts.len().to_string(),
            &format!(
                "{} enabled",
                hosts.iter().filter(|(_, host)| host.enabled).count()
            ),
            cyan(),
        );
        stat_card(
            &mut columns[2],
            "Virtual machines",
            vms.len().to_string(),
            &format!(
                "{} running",
                vms.iter()
                    .filter(|target| target.vm.power_state == "Running")
                    .count()
            ),
            green(),
        );
        stat_card(
            &mut columns[3],
            "Active tasks",
            running.to_string(),
            "Tracked per connection",
            amber(),
        );
    });

    ui.add_space(GAP);
    connections_panel(ui, state, workspace, actions);
    ui.add_space(GAP);
    resource_panel(ui, workspace, &state.scope);
    ui.add_space(GAP);
    recent_vms(ui, state, workspace, actions);
}

fn stat_card(ui: &mut Ui, title: &str, value: String, sub: &str, color: Color32) {
    let response = theme::card_frame()
        .inner_margin(Margin::same(14))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                ui.label(RichText::new(title).size(10.0).color(muted_fg()));
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    widgets::dot(ui, color, 6.0);
                });
            });
            ui.add_space(2.0);
            ui.label(RichText::new(value).size(27.0).strong().color(fg()));
            ui.add_space(2.0);
            ui.label(RichText::new(sub).size(9.5).color(dim_fg()));
        })
        .response;
    let _ = response;
    ui.add_space(4.0);
}

fn connections_panel(
    ui: &mut Ui,
    state: &mut UiState,
    workspace: &WorkspaceView,
    actions: &mut Vec<UiAction>,
) {
    theme::card_frame().show(ui, |ui| {
        ui.set_width(ui.available_width());
        widgets::panel_heading(
            ui,
            &format!("Connections {}", workspace.connections.len()),
            "Select a pool to narrow the workspace. Each host stays connected independently.",
            |_ui| {},
        );
        ui.add_space(10.0);
        if workspace.connections.is_empty() {
            widgets::empty_state(
                ui,
                "Connect your first host",
                "XCP-ng and XenServer pools appear here with their own inventory.",
            );
            return;
        }
        for chunk in workspace.connections.chunks(2) {
            ui.columns(2, |columns| {
                for (index, connection) in chunk.iter().enumerate() {
                    if let Some(column) = columns.get_mut(index) {
                        connection_card(column, state, connection, actions);
                    }
                }
            });
            ui.add_space(GAP);
        }
    });
}

fn connection_card(
    ui: &mut Ui,
    state: &mut UiState,
    connection: &ConnectionView,
    actions: &mut Vec<UiAction>,
) {
    let status = connection_status(connection);
    let settings = connection_settings(connection);
    let id = connection_id(connection).to_owned();
    let name = connection_name(connection);
    let inventory = crate::state::connection_inventory(connection);

    theme::inset_frame().show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.horizontal(|ui| {
            widgets::dot(ui, widgets::connection_color(status), 7.0);
            ui.label(
                RichText::new(match status {
                    "online" => "Connected",
                    "stale" => "Needs attention",
                    _ => "Offline",
                })
                .size(9.5)
                .color(widgets::connection_color(status)),
            );
            if is_read_only(connection) {
                ui.label(RichText::new("RO").size(7.5).color(amber()));
            }
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                ui.label(
                    RichText::new(if settings.read_only {
                        "Read-only"
                    } else if settings.tls.bypass_certificate_verification {
                        "TLS bypass"
                    } else {
                        "Certificate verified"
                    })
                    .size(8.5)
                    .color(dim_fg()),
                );
            });
        });
        ui.add_space(2.0);
        if ui
            .add(egui::Button::new(RichText::new(&name).size(13.0).strong()).frame(false))
            .clicked()
        {
            actions.push(UiAction::SetScope(Scope::Connection(id.clone())));
        }
        ui.label(
            RichText::new(&settings.host)
                .monospace()
                .size(9.5)
                .color(dim_fg()),
        );
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            if let Some(inventory) = inventory {
                for text in [
                    format::plural(inventory.hosts.len(), "host", "hosts"),
                    format::plural(inventory.vms.len(), "VM", "VMs"),
                    format::plural(inventory.storage.len(), "SR", "SRs"),
                ] {
                    ui.label(RichText::new(text).size(9.0).color(muted_fg()));
                    ui.label(RichText::new("·").size(9.0).color(dim_fg()));
                }
            } else {
                ui.label(
                    RichText::new("Credentials are not saved")
                        .size(9.0)
                        .color(dim_fg()),
                );
            }
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if status == "offline" {
                    if ui
                        .add(egui::Button::new(RichText::new("Connect").size(10.0)))
                        .clicked()
                    {
                        state.connect = Some(crate::state::ConnectState::from_profile(
                            id.clone(),
                            settings.clone(),
                        ));
                    }
                } else if ui
                    .add(egui::Button::new(RichText::new("Open ›").size(10.0)).frame(false))
                    .clicked()
                {
                    actions.push(UiAction::SetScope(Scope::Connection(id.clone())));
                }
            });
        });
    });
    ui.add_space(2.0);
}

fn resource_panel(ui: &mut Ui, workspace: &WorkspaceView, scope: &Scope) {
    let hosts = scoped_hosts(workspace, scope);
    let total_memory: u64 = hosts.iter().map(|(_, host)| host.memory_total).sum();
    let free_memory: u64 = hosts.iter().map(|(_, host)| host.memory_free).sum();
    let mut total_storage = 0u64;
    let mut used_storage = 0u64;
    for connection in visible_connections(workspace, scope) {
        if let Some(inventory) = crate::state::connection_inventory(connection) {
            for sr in &inventory.storage {
                total_storage += sr.total;
                used_storage += sr.used;
            }
        }
    }
    if hosts.is_empty() && total_storage == 0 {
        return;
    }
    ui.columns(2, |columns| {
        theme::card_frame().show(&mut columns[0], |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                ui.label(RichText::new("Host memory").size(12.0).strong());
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    ui.label(RichText::new("selected scope").size(9.0).color(dim_fg()));
                });
            });
            ui.add_space(2.0);
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new(format::bytes(total_memory.saturating_sub(free_memory)))
                        .size(23.0)
                        .strong(),
                );
                ui.label(
                    RichText::new(format!("/ {}", format::bytes(total_memory)))
                        .size(11.0)
                        .color(muted_fg()),
                );
            });
            ui.add_space(4.0);
            widgets::resource_bar(
                ui,
                total_memory.saturating_sub(free_memory),
                total_memory,
                accent(),
                6.0,
            );
            ui.horizontal(|ui| {
                ui.label(RichText::new("Used memory").size(9.0).color(dim_fg()));
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    ui.label(
                        RichText::new(format!("{} available", format::bytes(free_memory)))
                            .size(9.0)
                            .color(dim_fg()),
                    );
                });
            });
        });
        theme::card_frame().show(&mut columns[1], |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                ui.label(RichText::new("Storage usage").size(12.0).strong());
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    widgets::dot(ui, green(), 6.0);
                });
            });
            ui.add_space(2.0);
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new(format::bytes(used_storage))
                        .size(23.0)
                        .strong(),
                );
                ui.label(
                    RichText::new(format!("/ {}", format::bytes(total_storage)))
                        .size(11.0)
                        .color(muted_fg()),
                );
            });
            ui.add_space(4.0);
            widgets::resource_bar(ui, used_storage, total_storage, green(), 6.0);
            ui.horizontal(|ui| {
                ui.label(RichText::new("Used storage").size(9.0).color(dim_fg()));
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    ui.label(
                        RichText::new(format!(
                            "{} available",
                            format::bytes(total_storage.saturating_sub(used_storage))
                        ))
                        .size(9.0)
                        .color(dim_fg()),
                    );
                });
            });
        });
    });
}

fn recent_vms(
    ui: &mut Ui,
    state: &mut UiState,
    workspace: &WorkspaceView,
    actions: &mut Vec<UiAction>,
) {
    let targets: Vec<Target<'_>> = scoped_targets(workspace, &state.scope)
        .into_iter()
        .take(6)
        .collect();
    if targets.is_empty() {
        return;
    }
    theme::card_frame().show(ui, |ui| {
        ui.set_width(ui.available_width());
        widgets::panel_heading(
            ui,
            "Virtual machines",
            "Connection and host are shown for every object",
            |ui| {
                if ui
                    .add(egui::Button::new(RichText::new("View all ›").size(10.0)).frame(false))
                    .clicked()
                {
                    actions.push(UiAction::SetView(View::Vms));
                }
            },
        );
        ui.add_space(6.0);
        for target in targets {
            let id = (
                connection_id(target.connection).to_owned(),
                target.vm.uuid.clone(),
            );
            let response = ui
                .horizontal(|ui| {
                    widgets::dot(ui, widgets::power_style(&target.vm.power_state).1, 6.0);
                    if ui
                        .add(
                            Label::new(RichText::new(&target.vm.name).size(11.5))
                                .truncate()
                                .sense(Sense::click()),
                        )
                        .clicked()
                    {
                        actions.push(UiAction::OpenVm(id.0.clone(), id.1.clone()));
                    }
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        ui.label(
                            RichText::new(format::bytes(target.vm.memory))
                                .monospace()
                                .size(9.5)
                                .color(muted_fg()),
                        );
                        ui.label(
                            RichText::new(format!(
                                "{} · {}",
                                connection_name(target.connection),
                                target.vm.host_name
                            ))
                            .size(9.0)
                            .color(dim_fg()),
                        );
                        ui.label(
                            RichText::new(widgets::power_style(&target.vm.power_state).0)
                                .size(9.0)
                                .color(widgets::power_style(&target.vm.power_state).1),
                        );
                    });
                })
                .response;
            let _ = response;
            ui.add_space(2.0);
        }
    });
}
