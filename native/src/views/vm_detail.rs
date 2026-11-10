use egui::{Align, Color32, Layout, RichText, Sense, Ui, Vec2};
use egui_extras::{Column, TableBuilder};
use xen_core::{
    performance::HostPerformance,
    vm::{VmDetails, VmDisk, VmInterface},
    workspace::WorkspaceView,
};

use super::host_details::{chart, is_cpu_core, is_memory_metric, is_network_metric, multi_chart};
use crate::format;
use crate::state::{
    ActionKind, LoadState, ObjectKey, UiAction, UiState, VmTab, action_allowed, connection_id,
    connection_inventory, connection_name, find_vm, is_read_only, running_task,
};
use crate::theme;
use crate::theme::*;
use crate::widgets;

#[allow(clippy::too_many_lines)]
pub fn show(
    ui: &mut Ui,
    state: &mut UiState,
    workspace: &WorkspaceView,
    actions: &mut Vec<UiAction>,
) {
    let Some(key) = state
        .object_tabs
        .get(state.active_tab)
        .map(|tab| tab.key.clone())
    else {
        return;
    };
    let (connection_id, vm_uuid, kind, is_template, is_snapshot) = match key {
        ObjectKey::Vm {
            connection_id,
            vm_uuid,
        } => (connection_id, vm_uuid, "Virtual machine", false, false),
        ObjectKey::Template {
            connection_id,
            vm_uuid,
        } => (connection_id, vm_uuid, "Template", true, false),
        ObjectKey::Snapshot {
            connection_id,
            vm_uuid,
        } => (connection_id, vm_uuid, "Snapshot", false, true),
        _ => return,
    };
    let Some((connection, vm)) = find_vm(workspace, &connection_id, &vm_uuid) else {
        widgets::empty_state(
            ui,
            "Object unavailable",
            "This virtual machine is no longer present in the selected connection.",
        );
        return;
    };
    let read_only = is_read_only(connection);

    header(
        ui,
        connection,
        vm,
        kind,
        is_template,
        is_snapshot,
        read_only,
        state,
        actions,
    );

    let mut index = VmTab::ALL
        .iter()
        .position(|tab| *tab == state.vm_subtab)
        .unwrap_or(0);
    widgets::tab_bar(ui, &VmTab::ALL.map(VmTab::label), &mut index);
    ui.add_space(4.0);
    let tab = VmTab::ALL[index];
    if tab != state.vm_subtab {
        actions.push(UiAction::SetVmTab(tab));
    }
    state.vm_subtab = tab;

    action_bar(
        ui,
        workspace,
        &connection_id,
        &vm_uuid,
        vm,
        read_only,
        is_template,
        is_snapshot,
        actions,
    );
    ui.add_space(8.0);

    match tab {
        VmTab::General => {
            if let Some(details) = details_state(ui, state, actions) {
                general(ui, connection, vm, &details, is_template, is_snapshot);
            }
        }
        VmTab::Performance => performance(ui, state, actions),
        VmTab::Storage => {
            if let Some(details) = details_state(ui, state, actions) {
                storage(ui, &details.disks);
            }
        }
        VmTab::Network => {
            if let Some(details) = details_state(ui, state, actions) {
                network(ui, &details.interfaces);
            }
        }
        VmTab::Ha => {
            if let Some(details) = details_state(ui, state, actions) {
                ha(ui, &details);
            }
        }
        VmTab::Snapshots => snapshots(ui, workspace, connection, vm, read_only, actions),
    }
}

#[allow(clippy::too_many_arguments)]
fn header(
    ui: &mut Ui,
    connection: &xen_core::workspace::ConnectionView,
    vm: &xen_core::xapi::VirtualMachine,
    kind: &str,
    is_template: bool,
    is_snapshot: bool,
    read_only: bool,
    state: &UiState,
    actions: &mut Vec<UiAction>,
) {
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
            if is_template {
                "T"
            } else if is_snapshot {
                "S"
            } else {
                "V"
            },
            egui::FontId::proportional(17.0),
            accent(),
        );
        ui.vertical(|ui| {
            ui.spacing_mut().item_spacing.y = 0.0;
            ui.label(RichText::new(kind.to_uppercase()).size(9.5).color(dim_fg()));
            ui.label(RichText::new(&vm.name).size(24.0).strong());
            ui.horizontal(|ui| {
                widgets::power_badge(ui, &vm.power_state);
                ui.label(RichText::new("·").size(10.0).color(dim_fg()));
                ui.label(
                    RichText::new(connection_name(connection))
                        .size(10.5)
                        .color(muted_fg()),
                );
                ui.label(RichText::new("·").size(10.0).color(dim_fg()));
                ui.label(RichText::new(&vm.host_name).size(10.5).color(muted_fg()));
                if read_only {
                    widgets::badge(ui, "Read-only", amber());
                }
            });
        });
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            if ui
                .add(egui::Button::new(RichText::new("Refresh").size(11.5)))
                .clicked()
            {
                actions.push(UiAction::ReloadVmDetails);
                if state.vm_subtab == VmTab::Performance {
                    actions.push(UiAction::LoadVmPerformance);
                }
            }
        });
    });
    ui.label(
        RichText::new(&vm.uuid)
            .monospace()
            .size(9.0)
            .color(dim_fg()),
    );
    ui.add_space(8.0);
}

fn details_state(
    ui: &mut Ui,
    state: &mut UiState,
    actions: &mut Vec<UiAction>,
) -> Option<VmDetails> {
    match &state.vm_state {
        LoadState::Ready(details) => Some(details.clone()),
        LoadState::Error(message) => {
            let message = message.clone();
            theme::card_frame().show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.label(RichText::new("Could not load virtual machine details").strong());
                ui.label(RichText::new(message).size(11.0).color(red()));
                if ui.button("Retry").clicked() {
                    actions.push(UiAction::ReloadVmDetails);
                }
            });
            None
        }
        LoadState::Idle | LoadState::Loading => {
            ui.horizontal(|ui| {
                ui.spinner();
                ui.label(
                    RichText::new("Loading virtual machine details…")
                        .size(11.5)
                        .color(muted_fg()),
                );
            });
            None
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn action_bar(
    ui: &mut Ui,
    workspace: &WorkspaceView,
    connection_id: &str,
    vm_uuid: &str,
    vm: &xen_core::xapi::VirtualMachine,
    read_only: bool,
    is_template: bool,
    is_snapshot: bool,
    actions: &mut Vec<UiAction>,
) {
    if is_template {
        ui.horizontal(|ui| {
            let can_clone = action_allowed(workspace, connection_id, vm, "clone");
            if ui
                .add_enabled(can_clone, widgets::primary_button("New VM from template"))
                .clicked()
            {
                actions.push(UiAction::Propose(
                    ActionKind::Clone,
                    vec![(connection_id.to_owned(), vm_uuid.to_owned())],
                ));
            }
            let can_delete = action_allowed(workspace, connection_id, vm, "destroy");
            if ui
                .add_enabled(
                    can_delete,
                    egui::Button::new(RichText::new("Delete template…").size(11.0)),
                )
                .clicked()
            {
                actions.push(UiAction::Propose(
                    ActionKind::Destroy,
                    vec![(connection_id.to_owned(), vm_uuid.to_owned())],
                ));
            }
            ui.label(
                RichText::new("Creating a VM copies the template's configuration and disks.")
                    .size(10.0)
                    .color(dim_fg()),
            );
            if read_only {
                widgets::badge(ui, "Read-only", amber());
            }
        });
        return;
    }
    if is_snapshot {
        ui.horizontal(|ui| {
            let allowed =
                action_allowed(workspace, connection_id, vm, ActionKind::Revert.operation());
            if ui
                .add_enabled(allowed, widgets::primary_button("Revert to this snapshot"))
                .clicked()
            {
                actions.push(UiAction::Propose(
                    ActionKind::Revert,
                    vec![(connection_id.to_owned(), vm_uuid.to_owned())],
                ));
            }
            let can_delete = action_allowed(workspace, connection_id, vm, "destroy");
            if ui
                .add_enabled(
                    can_delete,
                    egui::Button::new(RichText::new("Delete snapshot…").size(11.0).color(red())),
                )
                .clicked()
            {
                actions.push(UiAction::Propose(
                    ActionKind::Destroy,
                    vec![(connection_id.to_owned(), vm_uuid.to_owned())],
                ));
            }
        });
        ui.label(
            RichText::new(
                "Revert replaces the parent VM's disk state. Deleting a snapshot frees its disks.",
            )
            .size(10.0)
            .color(dim_fg()),
        );
        return;
    }

    ui.horizontal(|ui| {
        let start_kind = if vm.power_state == "Suspended" {
            ActionKind::Resume
        } else {
            ActionKind::Start
        };
        let can_start = action_allowed(workspace, connection_id, vm, start_kind.operation());
        if ui
            .add_enabled(
                can_start,
                widgets::primary_button(if vm.power_state == "Suspended" {
                    "Resume"
                } else {
                    "Start"
                }),
            )
            .clicked()
        {
            actions.push(UiAction::Propose(
                start_kind,
                vec![(connection_id.to_owned(), vm_uuid.to_owned())],
            ));
        }
        let can_shutdown = action_allowed(
            workspace,
            connection_id,
            vm,
            ActionKind::Shutdown.operation(),
        );
        if ui
            .add_enabled(
                can_shutdown,
                egui::Button::new(RichText::new("Shut down").size(11.5)),
            )
            .clicked()
        {
            actions.push(UiAction::Propose(
                ActionKind::Shutdown,
                vec![(connection_id.to_owned(), vm_uuid.to_owned())],
            ));
        }
        ui.menu_button(
            RichText::new("More ...").size(11.0).color(muted_fg()),
            |ui| {
                if read_only {
                    ui.label(
                        RichText::new("Read-only connection")
                            .size(10.0)
                            .color(amber()),
                    );
                    ui.separator();
                }
                for kind in ActionKind::MENU {
                    let allowed = action_allowed(workspace, connection_id, vm, kind.operation());
                    let color = if kind.destructive() { red() } else { fg() };
                    if ui
                        .add_enabled(
                            allowed,
                            egui::Button::new(RichText::new(kind.label()).size(11.0).color(color))
                                .frame(false),
                        )
                        .clicked()
                    {
                        actions.push(UiAction::Propose(
                            kind,
                            vec![(connection_id.to_owned(), vm_uuid.to_owned())],
                        ));
                        ui.close();
                    }
                }
            },
        );
        let can_snapshot = action_allowed(
            workspace,
            connection_id,
            vm,
            ActionKind::Snapshot.operation(),
        );
        if ui
            .add_enabled(
                can_snapshot,
                egui::Button::new(RichText::new("Take snapshot").size(11.0)),
            )
            .clicked()
        {
            actions.push(UiAction::Propose(
                ActionKind::Snapshot,
                vec![(connection_id.to_owned(), vm_uuid.to_owned())],
            ));
        }
        if running_task(workspace, connection_id, vm_uuid) {
            ui.label(
                RichText::new("A task is active for this object.")
                    .size(10.0)
                    .color(amber()),
            );
        }
    });
}

fn general(
    ui: &mut Ui,
    connection: &xen_core::workspace::ConnectionView,
    vm: &xen_core::xapi::VirtualMachine,
    details: &VmDetails,
    is_template: bool,
    is_snapshot: bool,
) {
    ui.columns(2, |columns| {
        theme::card_frame().show(&mut columns[0], |ui| {
            ui.set_width(ui.available_width());
            widgets::section_title(
                ui,
                if is_template {
                    "Template"
                } else if is_snapshot {
                    "Snapshot"
                } else {
                    "Configuration"
                },
                None,
            );
            ui.add_space(4.0);
            widgets::kv(ui, "Connection", &connection_name(connection), false);
            widgets::kv(
                ui,
                "Resident host",
                if details.resident_on.is_empty() {
                    &vm.host_name
                } else {
                    &details.resident_on
                },
                false,
            );
            widgets::kv(ui, "Affinity", &details.affinity, false);
            widgets::kv(ui, "Power state", &details.power_state, false);
            widgets::kv(
                ui,
                "vCPUs",
                &format!(
                    "{} / {} at startup",
                    details.vcpus_max, details.vcpus_at_startup
                ),
                false,
            );
            widgets::kv(
                ui,
                "Static memory",
                &format!(
                    "{} max / {} min",
                    format::bytes(details.memory_static_max),
                    format::bytes(details.memory_static_min)
                ),
                false,
            );
            widgets::kv(
                ui,
                "Dynamic memory",
                &format!(
                    "{} max / {} target",
                    format::bytes(details.memory_dynamic_max),
                    format::bytes(details.memory_target)
                ),
                false,
            );
            widgets::kv(
                ui,
                "HA restart priority",
                if details.ha_restart_priority.is_empty() {
                    "Not set"
                } else {
                    &details.ha_restart_priority
                },
                false,
            );
            widgets::kv(
                ui,
                "HA always run",
                if details.ha_always_run { "Yes" } else { "No" },
                false,
            );
            widgets::kv(
                ui,
                "OS version",
                if details.os_version.is_empty() {
                    "Not reported"
                } else {
                    &details.os_version
                },
                false,
            );
            widgets::kv(
                ui,
                "PV drivers",
                if details.pv_drivers_version.is_empty() {
                    "Not reported"
                } else {
                    &details.pv_drivers_version
                },
                false,
            );
            ui.add_space(2.0);
            widgets::kv(ui, "UUID", &details.uuid, true);
        });

        theme::card_frame().show(&mut columns[1], |ui| {
            ui.set_width(ui.available_width());
            widgets::section_title(ui, "Description", None);
            ui.add_space(4.0);
            ui.label(
                RichText::new(if details.description.is_empty() {
                    "No description"
                } else {
                    &details.description
                })
                .size(11.5)
                .color(if details.description.is_empty() {
                    dim_fg()
                } else {
                    fg()
                }),
            );
            if !details.platform.is_empty() {
                ui.add_space(8.0);
                widgets::section_title(ui, "Platform", None);
                ui.add_space(4.0);
                for field in &details.platform {
                    widgets::kv(ui, &field.label, &field.value, false);
                }
            }
            if !details.other_config.is_empty() {
                ui.add_space(8.0);
                widgets::section_title(ui, "Other config", None);
                ui.add_space(4.0);
                for field in &details.other_config {
                    widgets::kv(ui, &field.label, &field.value, false);
                }
            }
        });
    });
}

fn performance(ui: &mut Ui, state: &mut UiState, actions: &mut Vec<UiAction>) {
    theme::card_frame().show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.horizontal(|ui| {
            ui.vertical(|ui| {
                ui.label(RichText::new("VM performance").size(15.0).strong());
                ui.label(
                    RichText::new(
                        "VM RRD samples · one graph per resource · gaps indicate missing data",
                    )
                    .size(10.0)
                    .color(muted_fg()),
                );
            });
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if ui
                    .add_enabled(
                        !state.vm_perf_state.is_loading(),
                        egui::Button::new(RichText::new("Refresh").size(11.0)),
                    )
                    .clicked()
                {
                    actions.push(UiAction::LoadVmPerformance);
                }
                for (seconds, label) in [(86400u64, "24 hours"), (3600, "1 hour"), (900, "15 min")]
                {
                    let selected = state.vm_perf_range == seconds;
                    if ui
                        .add(
                            egui::Button::new(RichText::new(label).size(10.5).color(if selected {
                                fg()
                            } else {
                                muted_fg()
                            }))
                            .fill(if selected {
                                panel_3()
                            } else {
                                Color32::TRANSPARENT
                            })
                            .frame(selected),
                        )
                        .clicked()
                        && !selected
                    {
                        actions.push(UiAction::SetVmPerfRange(seconds));
                    }
                }
            });
        });
        ui.add_space(8.0);

        let perf = state.vm_perf_state.ready().cloned();
        match perf {
            None if state.vm_perf_state.is_loading() => {
                ui.horizontal(|ui| {
                    ui.spinner();
                    ui.label(
                        RichText::new("Loading VM performance…")
                            .size(11.0)
                            .color(muted_fg()),
                    );
                });
            }
            None => {
                if matches!(state.vm_perf_state, LoadState::Idle) {
                    actions.push(UiAction::LoadVmPerformance);
                }
                if let LoadState::Error(message) = &state.vm_perf_state {
                    ui.label(RichText::new(message).size(11.0).color(red()));
                    if ui.button("Retry").clicked() {
                        actions.push(UiAction::LoadVmPerformance);
                    }
                } else {
                    ui.label(
                        RichText::new("Loading VM performance…")
                            .size(11.0)
                            .color(muted_fg()),
                    );
                }
            }
            Some(HostPerformance::Unsupported { message })
            | Some(HostPerformance::Unavailable { message }) => {
                ui.label(RichText::new(message).size(11.0).color(muted_fg()));
            }
            Some(HostPerformance::Ready {
                start,
                end,
                step,
                series,
            }) => {
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new(format!(
                            "{} — {}",
                            format::timestamp(start),
                            format::timestamp(end)
                        ))
                        .size(10.0)
                        .color(muted_fg()),
                    );
                    ui.label(
                        RichText::new(format!(
                            "{step}s sample interval · {} metrics",
                            series.len()
                        ))
                        .size(10.0)
                        .color(dim_fg()),
                    );
                });
                ui.add_space(6.0);
                let cpu: Vec<&_> = series
                    .iter()
                    .filter(|metric| is_cpu_core(&metric.name))
                    .collect();
                let memory: Vec<&_> = series
                    .iter()
                    .filter(|metric| is_memory_metric(&metric.name))
                    .collect();
                let network: Vec<&_> = series
                    .iter()
                    .filter(|metric| is_network_metric(&metric.name))
                    .collect();
                let used: std::collections::HashSet<&str> = cpu
                    .iter()
                    .chain(&memory)
                    .chain(&network)
                    .map(|metric| metric.name.as_str())
                    .collect();
                let other: Vec<&_> = series
                    .iter()
                    .filter(|metric| !used.contains(metric.name.as_str()))
                    .collect();
                for (title, group) in [
                    ("CPU Performance", cpu),
                    ("Memory Performance", memory),
                    ("Network Performance", network),
                ] {
                    if !group.is_empty() {
                        multi_chart(ui, title, &group, start, end, step);
                        ui.add_space(8.0);
                    }
                }
                if !other.is_empty() {
                    ui.add_space(2.0);
                    ui.label(RichText::new("Other VM metrics").size(14.0).strong());
                    ui.add_space(4.0);
                    let gap = 10.0;
                    let width = ((ui.available_width() - gap) / 2.0).max(220.0);
                    for chunk in other.chunks(2) {
                        ui.horizontal_top(|ui| {
                            ui.spacing_mut().item_spacing.x = 0.0;
                            ui.allocate_ui_with_layout(
                                Vec2::new(width, 0.0),
                                Layout::top_down(Align::LEFT),
                                |ui| {
                                    ui.set_width(width);
                                    chart(ui, chunk[0], start, end, step);
                                },
                            );
                            if chunk.len() > 1 {
                                ui.add_space(gap);
                                ui.allocate_ui_with_layout(
                                    Vec2::new(width, 0.0),
                                    Layout::top_down(Align::LEFT),
                                    |ui| {
                                        ui.set_width(width);
                                        chart(ui, chunk[1], start, end, step);
                                    },
                                );
                            }
                        });
                        ui.add_space(8.0);
                    }
                }
            }
        }
    });
}

fn storage(ui: &mut Ui, disks: &[VmDisk]) {
    theme::card_frame().show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.horizontal(|ui| {
            ui.label(RichText::new("Virtual disks").size(15.0).strong());
            widgets::badge(ui, disks.len().to_string(), muted_fg());
        });
        ui.add_space(6.0);
        if disks.is_empty() {
            ui.label(
                RichText::new("No virtual disks reported.")
                    .size(11.0)
                    .color(dim_fg()),
            );
            return;
        }
        TableBuilder::new(ui)
            .id_salt("vm-disks")
            .striped(true)
            .cell_layout(Layout::left_to_right(Align::Center))
            .column(Column::exact(70.0))
            .column(Column::remainder().at_least(160.0))
            .column(Column::initial(150.0))
            .column(Column::exact(95.0))
            .column(Column::exact(80.0))
            .column(Column::exact(80.0))
            .header(28.0, |mut header| {
                for title in ["Device", "VDI", "SR", "Size", "Attached", "Bootable"] {
                    header.col(|ui| {
                        ui.label(RichText::new(title).size(10.0).color(muted_fg()));
                    });
                }
            })
            .body(|body| {
                body.rows(30.0, disks.len(), |mut row| {
                    let disk = &disks[row.index()];
                    row.col(|ui| {
                        ui.label(
                            RichText::new(if disk.device.is_empty() {
                                "—"
                            } else {
                                &disk.device
                            })
                            .monospace()
                            .size(10.5),
                        );
                    });
                    row.col(|ui| {
                        ui.label(
                            RichText::new(if disk.vdi_name.is_empty() {
                                if disk.empty {
                                    "Empty drive"
                                } else {
                                    "Unavailable"
                                }
                            } else {
                                &disk.vdi_name
                            })
                            .size(11.0),
                        );
                    });
                    row.col(|ui| {
                        ui.label(RichText::new(&disk.sr_name).size(10.5).color(muted_fg()));
                    });
                    row.col(|ui| {
                        ui.label(
                            RichText::new(format::bytes(disk.virtual_size))
                                .monospace()
                                .size(10.5),
                        );
                    });
                    row.col(|ui| {
                        ui.label(
                            RichText::new(if disk.attached { "Yes" } else { "No" })
                                .size(10.5)
                                .color(if disk.attached { green() } else { dim_fg() }),
                        );
                    });
                    row.col(|ui| {
                        ui.label(
                            RichText::new(if disk.bootable { "Yes" } else { "No" })
                                .size(10.5)
                                .color(muted_fg()),
                        );
                    });
                });
            });
    });
}

fn network(ui: &mut Ui, interfaces: &[VmInterface]) {
    theme::card_frame().show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.horizontal(|ui| {
            ui.label(
                RichText::new("Virtual network interfaces")
                    .size(15.0)
                    .strong(),
            );
            widgets::badge(ui, interfaces.len().to_string(), muted_fg());
        });
        ui.add_space(6.0);
        if interfaces.is_empty() {
            ui.label(
                RichText::new("No virtual interfaces reported.")
                    .size(11.0)
                    .color(dim_fg()),
            );
            return;
        }
        TableBuilder::new(ui)
            .id_salt("vm-vifs")
            .striped(true)
            .cell_layout(Layout::left_to_right(Align::Center))
            .column(Column::exact(65.0))
            .column(Column::remainder().at_least(140.0))
            .column(Column::initial(110.0))
            .column(Column::initial(130.0))
            .column(Column::initial(130.0))
            .column(Column::exact(80.0))
            .header(28.0, |mut header| {
                for title in ["Device", "Network", "MAC", "IPv4", "IPv6", "Attached"] {
                    header.col(|ui| {
                        ui.label(RichText::new(title).size(10.0).color(muted_fg()));
                    });
                }
            })
            .body(|body| {
                body.rows(30.0, interfaces.len(), |mut row| {
                    let interface = &interfaces[row.index()];
                    row.col(|ui| {
                        ui.label(
                            RichText::new(format!("eth{}", interface.device))
                                .monospace()
                                .size(10.5),
                        );
                    });
                    row.col(|ui| {
                        ui.label(
                            RichText::new(if interface.network.is_empty() {
                                &interface.bridge
                            } else {
                                &interface.network
                            })
                            .size(11.0),
                        );
                    });
                    row.col(|ui| {
                        ui.label(
                            RichText::new(&interface.mac)
                                .monospace()
                                .size(9.5)
                                .color(muted_fg()),
                        );
                    });
                    row.col(|ui| {
                        ui.label(
                            RichText::new(interface.ipv4.join(", "))
                                .size(9.5)
                                .color(muted_fg()),
                        );
                    });
                    row.col(|ui| {
                        ui.label(
                            RichText::new(interface.ipv6.join(", "))
                                .size(9.5)
                                .color(dim_fg()),
                        );
                    });
                    row.col(|ui| {
                        ui.label(
                            RichText::new(if interface.attached { "Yes" } else { "No" })
                                .size(10.5)
                                .color(if interface.attached {
                                    green()
                                } else {
                                    dim_fg()
                                }),
                        );
                    });
                });
            });
    });
}

fn ha(ui: &mut Ui, details: &VmDetails) {
    theme::card_frame().show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.label(RichText::new("High availability").size(15.0).strong());
        ui.label(
            RichText::new("HA failover tolerance and the HA plan are configured at pool level.")
                .size(10.0)
                .color(dim_fg()),
        );
        ui.add_space(6.0);
        widgets::kv(
            ui,
            "Restart priority",
            if details.ha_restart_priority.is_empty() {
                "Not set"
            } else {
                &details.ha_restart_priority
            },
            false,
        );
        widgets::kv(
            ui,
            "Always run",
            if details.ha_always_run { "Yes" } else { "No" },
            false,
        );
        widgets::kv(ui, "Resident host", &details.resident_on, false);
        widgets::kv(ui, "Affinity", &details.affinity, false);
        ui.add_space(4.0);
        widgets::kv(
            ui,
            "Static memory",
            &format!(
                "{} max / {} min",
                format::bytes(details.memory_static_max),
                format::bytes(details.memory_static_min)
            ),
            false,
        );
        widgets::kv(
            ui,
            "Dynamic memory",
            &format!(
                "{} max / {} min",
                format::bytes(details.memory_dynamic_max),
                format::bytes(details.memory_dynamic_min)
            ),
            false,
        );
        widgets::kv(
            ui,
            "Memory target",
            &format::bytes(details.memory_target),
            false,
        );
    });
}

fn snapshots(
    ui: &mut Ui,
    workspace: &WorkspaceView,
    connection: &xen_core::workspace::ConnectionView,
    vm: &xen_core::xapi::VirtualMachine,
    read_only: bool,
    actions: &mut Vec<UiAction>,
) {
    theme::card_frame().show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.horizontal(|ui| {
            widgets::section_title(ui, "Disk snapshots", None);
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                let count = connection_inventory(connection)
                    .map(|inventory| {
                        inventory
                            .snapshots
                            .iter()
                            .filter(|snapshot| snapshot.snapshot_of == vm.reference)
                            .count()
                    })
                    .unwrap_or(0);
                ui.label(
                    RichText::new(format!("{count} snapshots"))
                        .size(9.5)
                        .color(dim_fg()),
                );
            });
        });
        ui.add_space(4.0);
        let Some(inventory) = connection_inventory(connection) else {
            return;
        };
        let snapshots: Vec<_> = inventory
            .snapshots
            .iter()
            .filter(|snapshot| snapshot.snapshot_of == vm.reference)
            .collect();
        if snapshots.is_empty() {
            ui.label(
                RichText::new("No snapshots for this VM.")
                    .size(11.0)
                    .color(dim_fg()),
            );
        }
        for snapshot in &snapshots {
            ui.horizontal(|ui| {
                widgets::dot(ui, purple(), 6.0);
                ui.vertical(|ui| {
                    ui.spacing_mut().item_spacing.y = 0.0;
                    if ui
                        .add(
                            egui::Label::new(RichText::new(&snapshot.name).size(11.5))
                                .sense(Sense::click()),
                        )
                        .clicked()
                    {
                        actions.push(UiAction::OpenObject {
                            key: ObjectKey::Snapshot {
                                connection_id: connection_id(connection).to_owned(),
                                vm_uuid: snapshot.uuid.clone(),
                            },
                            title: snapshot.name.clone(),
                        });
                    }
                    ui.label(
                        RichText::new(if snapshot.snapshot_time.is_empty() {
                            "Unknown time"
                        } else {
                            &snapshot.snapshot_time
                        })
                        .size(9.0)
                        .color(dim_fg()),
                    );
                });
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    let allowed = action_allowed(
                        workspace,
                        connection_id(connection),
                        snapshot,
                        ActionKind::Revert.operation(),
                    );
                    if ui
                        .add_enabled(
                            allowed,
                            egui::Button::new(RichText::new("Revert").size(10.0)),
                        )
                        .clicked()
                    {
                        actions.push(UiAction::Propose(
                            ActionKind::Revert,
                            vec![(connection_id(connection).to_owned(), snapshot.uuid.clone())],
                        ));
                    }
                });
            });
            ui.add_space(3.0);
        }
        ui.add_space(4.0);
        let can_snapshot = action_allowed(workspace, connection_id(connection), vm, "snapshot");
        if ui
            .add_enabled(
                can_snapshot && !read_only,
                egui::Button::new(RichText::new("Take snapshot").size(11.0)),
            )
            .clicked()
        {
            actions.push(UiAction::Propose(
                ActionKind::Snapshot,
                vec![(connection_id(connection).to_owned(), vm.uuid.clone())],
            ));
        }
        ui.add_space(2.0);
        ui.label(
            RichText::new(
                "Reverting replaces disk state; it does not restore the previous power state.",
            )
            .size(9.5)
            .color(dim_fg()),
        );
    });
}
