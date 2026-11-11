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
    connection_inventory, connection_name, find_connection, find_vm, is_read_only, running_task,
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
    let Some(key) = state.selection.as_ref().map(|tab| tab.key.clone()) else {
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

    let tabs: &[VmTab] = if is_template || is_snapshot {
        &VmTab::ALL[..6]
    } else {
        &VmTab::ALL
    };
    let labels: Vec<&str> = tabs.iter().map(|tab| tab.label()).collect();
    let mut index = tabs
        .iter()
        .position(|tab| *tab == state.vm_subtab)
        .unwrap_or(0);
    widgets::tab_bar(ui, &labels, &mut index);
    ui.add_space(4.0);
    let tab = tabs[index];
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
                general(
                    ui,
                    connection,
                    vm,
                    &details,
                    is_template,
                    is_snapshot,
                    read_only,
                    actions,
                );
            }
        }
        VmTab::Performance => performance(ui, state, actions),
        VmTab::Storage => {
            if let Some(details) = details_state(ui, state, actions) {
                storage(
                    ui,
                    read_only,
                    is_template,
                    is_snapshot,
                    vm.power_state == "Halted",
                    &details,
                    actions,
                );
            }
        }
        VmTab::Network => {
            if let Some(details) = details_state(ui, state, actions) {
                let editable = !read_only && !is_template && !is_snapshot;
                network(ui, &details.interfaces, editable, actions);
            }
        }
        VmTab::Ha => {
            if let Some(details) = details_state(ui, state, actions) {
                let editable = !read_only && !is_template && !is_snapshot;
                ha(ui, &details, editable, vm.power_state == "Running", actions);
            }
        }
        VmTab::Snapshots => snapshots(ui, state, workspace, connection, vm, read_only, actions),
        VmTab::Console => console(ui, state, &connection_id, &vm_uuid, vm, actions),
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
    _state: &UiState,
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
            if !is_template && !is_snapshot {
                if ui
                    .add(widgets::primary_button("Console"))
                    .on_hover_text("Open the VNC console (connects automatically when running)")
                    .clicked()
                {
                    actions.push(UiAction::SetVmTab(VmTab::Console));
                }
                if ui
                    .add(egui::Button::new(RichText::new("Disks").size(11.5)))
                    .on_hover_text("Add, resize, move, plug or remove virtual disks")
                    .clicked()
                {
                    actions.push(UiAction::SetVmTab(VmTab::Storage));
                }
                if ui
                    .add(egui::Button::new(RichText::new("Network").size(11.5)))
                    .on_hover_text("Add, edit, plug or remove virtual network interfaces")
                    .clicked()
                {
                    actions.push(UiAction::SetVmTab(VmTab::Network));
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
    ui.horizontal(|ui| {
        if ui
            .add_enabled(
                vm.power_state == "Halted"
                    && action_allowed(workspace, connection_id, vm, "export"),
                egui::Button::new("Export XVA…"),
            )
            .clicked()
        {
            actions.push(UiAction::OpenXvaExport {
                connection_id: connection_id.to_owned(),
                vm_uuid: vm_uuid.to_owned(),
            });
        }
    });
    if !is_snapshot
        && ui
            .add_enabled(
                vm.power_state == "Halted" && action_allowed(workspace, connection_id, vm, "copy"),
                egui::Button::new("Full copy…"),
            )
            .on_hover_text("Copy all disks to a selected storage repository, preserving the source VM or template")
            .clicked()
    {
        actions.push(UiAction::OpenVmCopy {
            connection_id: connection_id.to_owned(),
            vm_uuid: vm_uuid.to_owned(),
        });
    }
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
        ui.horizontal_wrapped(|ui| {
            for kind in [ActionKind::SnapshotToVm, ActionKind::SnapshotToTemplate] {
                if ui
                    .add_enabled(
                        action_allowed(workspace, connection_id, vm, "clone"),
                        egui::Button::new(kind.label()),
                    )
                    .clicked()
                {
                    actions.push(UiAction::Propose(
                        kind,
                        vec![(connection_id.to_owned(), vm_uuid.to_owned())],
                    ));
                }
            }
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
                "Revert replaces the parent VM's disk state and restores a running power state. Deleting a snapshot frees its disks.",
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
        let start_label = if vm.power_state == "Suspended" {
            "Resume"
        } else {
            "Start"
        };
        let start_button = if can_start {
            widgets::primary_button(start_label)
        } else {
            egui::Button::new(RichText::new(start_label).size(11.5))
        };
        if ui.add_enabled(can_start, start_button).clicked() {
            actions.push(UiAction::Propose(
                start_kind,
                vec![(connection_id.to_owned(), vm_uuid.to_owned())],
            ));
        }
        let host_operation = if vm.power_state == "Suspended" {
            "resume_on"
        } else {
            "start_on"
        };
        ui.menu_button(
            RichText::new(if vm.power_state == "Suspended" {
                "Resume on…"
            } else {
                "Start on…"
            })
            .size(11.0)
            .color(muted_fg()),
            |ui| {
                let hosts = find_connection(workspace, connection_id)
                    .and_then(connection_inventory)
                    .map(|inventory| inventory.hosts.as_slice())
                    .unwrap_or_default();
                if hosts.is_empty() {
                    ui.label(
                        RichText::new("No hosts available")
                            .size(10.0)
                            .color(dim_fg()),
                    );
                }
                for host in hosts {
                    let allowed = action_allowed(workspace, connection_id, vm, host_operation)
                        && host.enabled;
                    if ui
                        .add_enabled(
                            allowed,
                            egui::Button::new(RichText::new(&host.name).size(11.0).color(fg()))
                                .frame(false),
                        )
                        .on_hover_text(&host.address)
                        .clicked()
                    {
                        actions.push(UiAction::ProposeOnHost(
                            start_kind,
                            vec![(connection_id.to_owned(), vm_uuid.to_owned())],
                            host.reference.clone(),
                            host.name.clone(),
                        ));
                        ui.close();
                    }
                    if vm.power_state == "Halted"
                        && ui
                            .add_enabled(
                                allowed,
                                egui::Button::new(
                                    RichText::new(format!("{} (paused)", host.name)).size(11.0),
                                )
                                .frame(false),
                            )
                            .clicked()
                    {
                        actions.push(UiAction::ProposeOnHost(
                            ActionKind::StartPaused,
                            vec![(connection_id.to_owned(), vm_uuid.to_owned())],
                            host.reference.clone(),
                            host.name.clone(),
                        ));
                        ui.close();
                    }
                }
            },
        );
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
                for kind in ActionKind::MENU.into_iter().filter(|kind| {
                    matches!(
                        kind,
                        ActionKind::StartPaused
                            | ActionKind::Checkpoint
                            | ActionKind::SnapshotWithQuiesce
                            | ActionKind::Clone
                    )
                }) {
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
        let can_edit = !read_only && !running_task(workspace, connection_id, vm_uuid);
        if ui
            .add_enabled(
                can_edit,
                egui::Button::new(RichText::new("Resources…").size(11.0)),
            )
            .on_hover_text("Edit vCPUs, memory and boot settings")
            .clicked()
        {
            actions.push(UiAction::OpenVmResources);
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

#[allow(clippy::too_many_arguments)]
fn general(
    ui: &mut Ui,
    connection: &xen_core::workspace::ConnectionView,
    vm: &xen_core::xapi::VirtualMachine,
    details: &VmDetails,
    is_template: bool,
    is_snapshot: bool,
    read_only: bool,
    actions: &mut Vec<UiAction>,
) {
    if !is_template && !is_snapshot {
        ui.add_space(8.0);
        theme::card_frame().show(ui, |ui| {
        ui.set_width(ui.available_width());
        widgets::section_title(ui, "Quick actions", None);
        ui.add_space(4.0);
        ui.horizontal_wrapped(|ui| {
            if ui
                .add(widgets::primary_button("VNC console"))
                .on_hover_text("Open the VNC console; it connects automatically when the VM is running")
                .clicked()
            {
                actions.push(UiAction::SetVmTab(VmTab::Console));
            }
            if ui
                .add(egui::Button::new(RichText::new("Disks").size(11.0)))
                .on_hover_text("Add, resize, move, plug or remove virtual disks")
                .clicked()
            {
                actions.push(UiAction::SetVmTab(VmTab::Storage));
            }
            if ui
                .add(egui::Button::new(RichText::new("Network").size(11.0)))
                .on_hover_text("Add, edit, plug or remove virtual interfaces")
                .clicked()
            {
                actions.push(UiAction::SetVmTab(VmTab::Network));
            }
            if ui
                .add(egui::Button::new(RichText::new("Snapshots").size(11.0)))
                .clicked()
            {
                actions.push(UiAction::SetVmTab(VmTab::Snapshots));
            }
            if ui
                .add_enabled(
                    !read_only,
                    egui::Button::new(RichText::new("Resources…").size(11.0)),
                )
                .on_hover_text("Edit vCPUs, memory and boot settings")
                .clicked()
            {
                actions.push(UiAction::OpenVmResources);
            }
        });
        ui.label(
            RichText::new(
                "Edit resources to change memory or add supported CPUs while running; other changes require shutdown.",
            )
            .size(9.0)
            .color(dim_fg()),
        );
    });
        ui.add_space(8.0);
    }
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

fn console(
    ui: &mut Ui,
    state: &mut UiState,
    connection_id: &str,
    vm_uuid: &str,
    vm: &xen_core::xapi::VirtualMachine,
    actions: &mut Vec<UiAction>,
) {
    theme::card_frame().show(ui, |ui| {
        ui.set_width(ui.available_width());
        let matching = state.console.as_ref().is_some_and(|console| {
            console.connection_id == connection_id && console.vm_uuid == vm_uuid
        });
        ui.horizontal(|ui| {
            ui.label(RichText::new("Virtual console").size(15.0).strong());
            widgets::badge(ui, "VNC", accent());
            if let Some(console) = state.console.as_ref().filter(|_| matching) {
                let status = match console.status {
                    crate::state::ConsoleStatus::Connecting => "Connecting".to_owned(),
                    crate::state::ConsoleStatus::Connected => {
                        if console.width > 0 {
                            format!("{}×{}", console.width, console.height)
                        } else {
                            "Connected".to_owned()
                        }
                    }
                    crate::state::ConsoleStatus::Closed => "Disconnected".to_owned(),
                };
                ui.label(RichText::new(status).size(10.0).color(muted_fg()));
            }
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if ui
                    .add_enabled(
                        vm.power_state == "Running",
                        egui::Button::new(RichText::new("SSH…").size(11.0)),
                    )
                    .on_hover_text("Open the embedded SSH text console")
                    .clicked()
                {
                    actions.push(UiAction::OpenSshDialog);
                }
                if matching {
                    if ui
                        .add(egui::Button::new(RichText::new("Disconnect").size(11.0)))
                        .clicked()
                    {
                        actions.push(UiAction::CloseConsole);
                    }
                    if ui
                        .add(egui::Button::new(
                            RichText::new("Ctrl+Alt+Del").size(11.0),
                        ))
                        .on_hover_text("Send the secure attention sequence to the guest")
                        .clicked()
                    {
                        let console_id = state
                            .console
                            .as_ref()
                            .map(|console| console.console_id.clone())
                            .unwrap_or_default();
                        for (keycode, down) in [
                            (0xffe3, true),
                            (0xffe9, true),
                            (0xffff, true),
                            (0xffff, false),
                            (0xffe9, false),
                            (0xffe3, false),
                        ] {
                            actions.push(UiAction::ConsoleKey {
                                console_id: console_id.clone(),
                                keycode,
                                down,
                            });
                        }
                        if let Some(console) = state.console.as_mut() {
                            console.modifiers = (false, false, false, false);
                        }
                    }
                    let clipboard = state
                        .console
                        .as_ref()
                        .map(|console| console.clipboard.clone())
                        .unwrap_or_default();
                    if ui
                        .add_enabled(
                            !clipboard.is_empty(),
                            egui::Button::new(RichText::new("Copy from guest").size(11.0)),
                        )
                        .on_hover_text("Copy the guest clipboard to this desktop")
                        .clicked()
                    {
                        ui.ctx().copy_text(clipboard);
                    }
                    let guest_ip = state
                        .vm_state
                        .ready()
                        .and_then(|details| details.guest_ips.first())
                        .cloned()
                        .unwrap_or_default();
                    ui.menu_button(RichText::new("RDP").size(11.0), |ui| {
                        if ui
                            .button(RichText::new("Embedded RDP…").size(10.5))
                            .clicked()
                        {
                            actions.push(UiAction::OpenRdpDialog);
                            ui.close();
                        }
                        if !guest_ip.is_empty()
                            && ui
                                .button(
                                    RichText::new(format!("External xfreerdp ({guest_ip})"))
                                        .size(10.5),
                                )
                                .clicked()
                        {
                            actions.push(UiAction::LaunchRdp(guest_ip.clone()));
                            ui.close();
                        }
                    });
                    if ui
                        .add(egui::Button::new(
                            RichText::new("Paste to guest").size(11.0),
                        ))
                        .on_hover_text("Send this desktop's clipboard to the guest")
                        .clicked()
                    {
                        let console_id = state
                            .console
                            .as_ref()
                            .map(|console| console.console_id.clone())
                            .unwrap_or_default();
                        if let Ok(mut clipboard) = arboard::Clipboard::new()
                            && let Ok(text) = clipboard.get_text()
                        {
                            let text: String = text.chars().take(65535).collect();
                            if !text.is_empty() {
                                actions.push(UiAction::ConsoleClipboard {
                                    console_id,
                                    text,
                                });
                            }
                        }
                    }
                } else if ui
                    .add_enabled(
                        vm.power_state == "Running",
                        widgets::primary_button("Open console"),
                    )
                    .clicked()
                {
                    actions.push(UiAction::OpenConsole(
                        connection_id.to_owned(),
                        vm_uuid.to_owned(),
                    ));
                }
            });
        });
        ui.label(
            RichText::new(
                "The console is a direct VNC tunnel through the host and stays open when you switch tabs.",
            )
            .size(10.0)
            .color(dim_fg()),
        );
        ui.add_space(6.0);

        if !matching {
            if let Some(console) = state.console.as_ref() {
                ui.label(
                    RichText::new(format!(
                        "A console session is already open for another VM ({}).",
                        console.vm_uuid
                    ))
                    .size(10.5)
                    .color(amber()),
                );
            }
            if vm.power_state == "Running" {
                ui.label(
                    RichText::new("Open the console to view and control this VM.")
                        .size(11.0)
                        .color(muted_fg()),
                );
            } else {
                ui.label(
                    RichText::new("Start the VM before opening its console.")
                        .size(11.0)
                        .color(dim_fg()),
                );
            }
            return;
        }

        let Some(console) = state.console.as_mut() else {
            return;
        };
        if console.status == crate::state::ConsoleStatus::Connecting {
            ui.horizontal(|ui| {
                ui.spinner();
                ui.label(
                    RichText::new("Connecting to the console…")
                        .size(11.0)
                        .color(muted_fg()),
                );
            });
            return;
        }
        if let crate::state::ConsoleStatus::Closed = console.status {
            ui.label(
                RichText::new(if console.message.is_empty() {
                    "The console connection closed."
                } else {
                    &console.message
                })
                .size(11.0)
                .color(red()),
            );
            if ui.button("Reconnect").clicked() {
                actions.push(UiAction::OpenConsole(
                    connection_id.to_owned(),
                    vm_uuid.to_owned(),
                ));
            }
            return;
        }
        if console.width == 0 || console.height == 0 || console.pixels.is_empty() {
            ui.horizontal(|ui| {
                ui.spinner();
                ui.label(
                    RichText::new("Waiting for the first frame…")
                        .size(11.0)
                        .color(muted_fg()),
                );
            });
            return;
        }

        let size = [console.width as usize, console.height as usize];
        let needs_texture = console
            .texture
            .as_ref()
            .is_none_or(|texture| texture.size() != size);
        if needs_texture {
            let image = egui::ColorImage::from_rgba_unmultiplied(size, &console.pixels);
            console.texture = Some(ui.ctx().load_texture(
                format!("vm-console-{}", console.console_id),
                image,
                egui::TextureOptions::LINEAR,
            ));
            console.dirty.clear();
            console.dirty_full = false;
        } else if console.dirty_full {
            let image = egui::ColorImage::from_rgba_unmultiplied(size, &console.pixels);
            if let Some(texture) = console.texture.as_mut() {
                texture.set(image, egui::TextureOptions::LINEAR);
            }
            console.dirty.clear();
            console.dirty_full = false;
        } else if !console.dirty.is_empty() {
            let dirty = std::mem::take(&mut console.dirty);
            if let Some(texture) = console.texture.as_mut() {
                for (x, y, width, height) in dirty {
                    if width == 0 || height == 0 {
                        continue;
                    }
                    let image = console_region(
                        &console.pixels,
                        console.width,
                        x,
                        y,
                        width,
                        height,
                    );
                    texture.set_partial(
                        [x as usize, y as usize],
                        image,
                        egui::TextureOptions::LINEAR,
                    );
                }
            }
        }
        let Some(texture) = console.texture.as_ref().map(|texture| texture.id()) else {
            return;
        };

        let max_size = Vec2::new(ui.available_width(), 680.0);
        let scale = (max_size.x / console.width as f32)
            .min(max_size.y / console.height as f32)
            .max(0.05);
        let display = Vec2::new(
            console.width as f32 * scale,
            console.height as f32 * scale,
        );
        let console_id = console.console_id.clone();
        let mut input_actions = Vec::new();
        let mut console_focused = false;
        ui.vertical_centered(|ui| {
            let (rect, response) = ui.allocate_exact_size(display, Sense::click_and_drag());
            ui.painter().rect_filled(rect, 0.0, egui::Color32::BLACK);
            ui.painter().image(
                texture,
                rect,
                egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
                egui::Color32::WHITE,
            );
            if response.clicked() {
                response.request_focus();
            }
            let focused = ui.memory(|memory| memory.has_focus(response.id));
            console_focused = focused;
            let modifiers = ui.input(|input| input.modifiers);
            sync_console_modifiers(
                console,
                modifiers,
                focused,
                &console_id,
                &mut input_actions,
            );
            if focused {
                let events = ui.input(|input| input.events.clone());
                let command = ui.input(|input| {
                    input.modifiers.ctrl || input.modifiers.alt || input.modifiers.command
                });
                for event in events {
                    match event {
                        egui::Event::Key {
                            key,
                            pressed,
                            repeat,
                            ..
                        } => {
                            if repeat {
                                continue;
                            }
                            if !command && is_printable_key(key) {
                                continue;
                            }
                            if let Some(keycode) = console_keysym(key) {
                                input_actions.push(UiAction::ConsoleKey {
                                    console_id: console_id.clone(),
                                    keycode,
                                    down: pressed,
                                });
                            }
                        }
                        egui::Event::Text(text) if !command => {
                            for character in text.chars() {
                                if let Some(keycode) = unicode_keysym(character) {
                                    input_actions.push(UiAction::ConsoleKey {
                                        console_id: console_id.clone(),
                                        keycode,
                                        down: true,
                                    });
                                    input_actions.push(UiAction::ConsoleKey {
                                        console_id: console_id.clone(),
                                        keycode,
                                        down: false,
                                    });
                                }
                            }
                        }
                        _ => {}
                    }
                }
            }

            let pointer = ui.input(|input| input.pointer.interact_pos());
            if let Some(pos) = pointer
                && (rect.contains(pos) || response.dragged())
            {
                let x = (((pos.x - rect.left()) / rect.width()) * console.width as f32)
                    .clamp(0.0, (console.width.saturating_sub(1)) as f32)
                    as u16;
                let y = (((pos.y - rect.top()) / rect.height()) * console.height as f32)
                    .clamp(0.0, (console.height.saturating_sub(1)) as f32)
                    as u16;
                let mut buttons = 0u8;
                ui.input(|input| {
                    if input.pointer.button_down(egui::PointerButton::Primary) {
                        buttons |= 1;
                    }
                    if input.pointer.button_down(egui::PointerButton::Middle) {
                        buttons |= 2;
                    }
                    if input.pointer.button_down(egui::PointerButton::Secondary) {
                        buttons |= 4;
                    }
                });
                if console.last_pointer != Some((x, y, buttons)) {
                    console.last_pointer = Some((x, y, buttons));
                    input_actions.push(UiAction::ConsolePointer {
                        console_id: console_id.clone(),
                        x,
                        y,
                        buttons,
                    });
                }
                let wheel = ui.input(|input| input.smooth_scroll_delta.y);
                if wheel != 0.0 {
                    let bit = if wheel > 0.0 { 8 } else { 16 };
                    input_actions.push(UiAction::ConsolePointer {
                        console_id: console_id.clone(),
                        x,
                        y,
                        buttons: buttons | bit,
                    });
                    input_actions.push(UiAction::ConsolePointer {
                        console_id: console_id.clone(),
                        x,
                        y,
                        buttons,
                    });
                }
            }
        });
        state.console_focus = console_focused;
        actions.extend(input_actions);
    });
    ssh_console_section(ui, state, connection_id, vm, actions);
    rdp_console_section(ui, state, connection_id, vm, actions);
}

fn rdp_console_section(
    ui: &mut Ui,
    state: &mut UiState,
    connection_id: &str,
    vm: &xen_core::xapi::VirtualMachine,
    actions: &mut Vec<UiAction>,
) {
    theme::card_frame().show(ui, |ui| {
        ui.set_width(ui.available_width());
        let matching = state.rdp_console.as_ref().is_some_and(|console| {
            console.vm_uuid == vm.uuid && console.connection_id == connection_id
        });
        ui.horizontal(|ui| {
            ui.label(RichText::new("RDP console").size(15.0).strong());
            widgets::badge(ui, "RDP", accent());
            if let Some(console) = state.rdp_console.as_ref().filter(|_| matching) {
                let (label, color) = match console.status {
                    crate::state::RdpStatus::Connecting => ("Connecting", amber()),
                    crate::state::RdpStatus::Connected => ("Connected", green()),
                    crate::state::RdpStatus::Closed => ("Disconnected", dim_fg()),
                };
                ui.label(RichText::new(label).size(10.0).color(color));
                if console.width > 0 {
                    ui.label(
                        RichText::new(format!("{}×{}", console.width, console.height))
                            .size(9.0)
                            .color(dim_fg()),
                    );
                }
            }
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if matching {
                    if ui
                        .add(egui::Button::new(RichText::new("Disconnect").size(10.5)))
                        .clicked()
                    {
                        actions.push(UiAction::CloseRdpConsole);
                    }
                    if ui
                        .add(egui::Button::new(RichText::new("Close").size(10.5)))
                        .clicked()
                    {
                        actions.push(UiAction::ClearRdpConsole);
                    }
                    if ui
                        .add(egui::Button::new(RichText::new("Ctrl+Alt+Del").size(10.5)))
                        .clicked()
                        && let Some(console) = state.rdp_console.as_ref()
                    {
                        actions.push(UiAction::RdpInput {
                            console_id: console.console_id.clone(),
                            input: crate::rdp::RdpInput::CtrlAltDel,
                        });
                    }
                } else if ui
                    .add_enabled(
                        vm.power_state == "Running",
                        egui::Button::new(RichText::new("Open RDP…").size(10.5)),
                    )
                    .on_hover_text(if vm.power_state == "Running" {
                        "Connect to the guest RDP server"
                    } else {
                        "Start the VM before opening RDP"
                    })
                    .clicked()
                {
                    actions.push(UiAction::OpenRdpDialog);
                }
            });
        });
        ui.add_space(4.0);
        let Some(console) = state.rdp_console.as_mut().filter(|_| matching) else {
            ui.label(
                RichText::new("Open RDP to enter the guest address and credentials.")
                    .size(10.0)
                    .color(dim_fg()),
            );
            return;
        };
        if !console.message.is_empty() {
            ui.label(RichText::new(&console.message).size(9.5).color(
                if console.status == crate::state::RdpStatus::Closed {
                    amber()
                } else {
                    muted_fg()
                },
            ));
        }
        if console.width == 0 || console.height == 0 {
            ui.horizontal(|ui| {
                ui.spinner();
                ui.label(
                    RichText::new("Waiting for the RDP framebuffer…")
                        .size(10.0)
                        .color(muted_fg()),
                );
            });
            return;
        }
        let width = console.width;
        let height = console.height;
        if console.dirty || console.texture.is_none() {
            let pixels: Vec<egui::Color32> = console
                .pixels
                .iter()
                .map(|pixel| {
                    egui::Color32::from_rgb(
                        ((pixel >> 16) & 0xff) as u8,
                        ((pixel >> 8) & 0xff) as u8,
                        (pixel & 0xff) as u8,
                    )
                })
                .collect();
            let image = egui::ColorImage {
                size: [width as usize, height as usize],
                source_size: egui::Vec2::new(width as f32, height as f32),
                pixels,
            };
            if let Some(texture) = console.texture.as_mut() {
                texture.set(image, egui::TextureOptions::NEAREST);
            } else {
                console.texture = Some(ui.ctx().load_texture(
                    format!("rdp-{}", console.console_id),
                    image,
                    egui::TextureOptions::NEAREST,
                ));
            }
            console.dirty = false;
        }
        let Some(texture) = console.texture.as_ref().map(|texture| texture.id()) else {
            return;
        };
        let max_size = egui::Vec2::new(ui.available_width(), 680.0);
        let scale = (max_size.x / width as f32)
            .min(max_size.y / height as f32)
            .max(0.05);
        let display = egui::Vec2::new(width as f32 * scale, height as f32 * scale);
        ui.vertical_centered(|ui| {
            let (rect, response) = ui.allocate_exact_size(display, Sense::click_and_drag());
            ui.painter().rect_filled(rect, 0.0, egui::Color32::BLACK);
            ui.painter().image(
                texture,
                rect,
                egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
                egui::Color32::WHITE,
            );
            if response.clicked() {
                response.request_focus();
            }
            let focused = ui.memory(|memory| memory.has_focus(response.id));
            if let Some(pos) = ui.input(|input| input.pointer.interact_pos())
                && (rect.contains(pos) || response.dragged())
            {
                let x = (((pos.x - rect.left()) / rect.width()) * width as f32)
                    .clamp(0.0, (width.saturating_sub(1)) as f32) as u16;
                let y = (((pos.y - rect.top()) / rect.height()) * height as f32)
                    .clamp(0.0, (height.saturating_sub(1)) as f32) as u16;
                actions.push(UiAction::RdpInput {
                    console_id: console.console_id.clone(),
                    input: crate::rdp::RdpInput::MouseMove { x, y },
                });
                let mut buttons = 0u8;
                ui.input(|input| {
                    if input.pointer.button_down(egui::PointerButton::Primary) {
                        buttons |= 1;
                    }
                    if input.pointer.button_down(egui::PointerButton::Middle) {
                        buttons |= 2;
                    }
                    if input.pointer.button_down(egui::PointerButton::Secondary) {
                        buttons |= 4;
                    }
                });
                for (bit, button) in [(1u8, 0u8), (2, 1), (4, 2)] {
                    if (buttons & bit) != (console.buttons & bit) {
                        actions.push(UiAction::RdpInput {
                            console_id: console.console_id.clone(),
                            input: crate::rdp::RdpInput::MouseButton {
                                button,
                                pressed: buttons & bit != 0,
                            },
                        });
                    }
                }
                console.buttons = buttons;
                let wheel = ui.input(|input| input.smooth_scroll_delta.y);
                if wheel != 0.0 {
                    actions.push(UiAction::RdpInput {
                        console_id: console.console_id.clone(),
                        input: crate::rdp::RdpInput::Wheel {
                            vertical: true,
                            units: (wheel / 40.0).clamp(-10.0, 10.0) as i16,
                        },
                    });
                }
            }
            if focused {
                let modifiers = ui.input(|input| input.modifiers);
                for event in ui.input(|input| input.events.clone()) {
                    match event {
                        egui::Event::Text(text)
                            if !modifiers.ctrl && !modifiers.alt && !modifiers.command =>
                        {
                            actions.push(UiAction::RdpInput {
                                console_id: console.console_id.clone(),
                                input: crate::rdp::RdpInput::Text(text),
                            });
                        }
                        egui::Event::Key {
                            key,
                            pressed,
                            repeat,
                            modifiers,
                            ..
                        } => {
                            if repeat {
                                continue;
                            }
                            let command = modifiers.ctrl || modifiers.alt || modifiers.command;
                            if !command && is_printable_key(key) {
                                continue;
                            }
                            actions.push(UiAction::RdpInput {
                                console_id: console.console_id.clone(),
                                input: crate::rdp::RdpInput::Key {
                                    key,
                                    pressed,
                                    ctrl: modifiers.ctrl,
                                    alt: modifiers.alt,
                                    shift: modifiers.shift,
                                },
                            });
                        }
                        _ => {}
                    }
                }
            }
        });
        ui.label(
            RichText::new(
                "Click the framebuffer to focus. Keyboard, mouse clicks and wheel are forwarded.",
            )
            .size(9.0)
            .color(dim_fg()),
        );
    });
}

fn ssh_console_section(
    ui: &mut Ui,
    state: &mut UiState,
    connection_id: &str,
    vm: &xen_core::xapi::VirtualMachine,
    actions: &mut Vec<UiAction>,
) {
    theme::card_frame().show(ui, |ui| {
        ui.set_width(ui.available_width());
        let matching = state.ssh_console.as_ref().is_some_and(|console| {
            console.vm_uuid == vm.uuid && console.connection_id == connection_id
        });
        ui.horizontal(|ui| {
            ui.label(RichText::new("SSH text console").size(15.0).strong());
            widgets::badge(ui, "SSH", accent());
            if let Some(console) = state.ssh_console.as_ref().filter(|_| matching) {
                let (label, color) = match console.status {
                    crate::state::SshStatus::Connecting => ("Connecting", amber()),
                    crate::state::SshStatus::Connected => ("Connected", green()),
                    crate::state::SshStatus::Closed => ("Disconnected", dim_fg()),
                };
                ui.label(RichText::new(label).size(10.0).color(color));
                if !console.host.is_empty() {
                    ui.label(
                        RichText::new(format!("{}@{}", console.username, console.host))
                            .monospace()
                            .size(9.0)
                            .color(dim_fg()),
                    );
                }
            }
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if matching {
                    if ui
                        .add(egui::Button::new(RichText::new("Disconnect").size(10.5)))
                        .clicked()
                    {
                        actions.push(UiAction::CloseSshConsole);
                    }
                    if ui
                        .add(egui::Button::new(RichText::new("Clear").size(10.5)))
                        .clicked()
                    {
                        actions.push(UiAction::ClearSshConsole);
                    }
                    if ui
                        .add(egui::Button::new(RichText::new("Ctrl+C").size(10.5)))
                        .clicked()
                        && let Some(console) = state.ssh_console.as_ref()
                    {
                        actions.push(UiAction::SshInput {
                            console_id: console.console_id.clone(),
                            data: vec![0x03],
                        });
                    }
                } else if ui
                    .add_enabled(
                        vm.power_state == "Running",
                        egui::Button::new(RichText::new("Open SSH…").size(10.5)),
                    )
                    .on_hover_text(if vm.power_state == "Running" {
                        "Connect to the guest SSH server"
                    } else {
                        "Start the VM before opening SSH"
                    })
                    .clicked()
                {
                    actions.push(UiAction::OpenSshDialog);
                }
            });
        });
        ui.add_space(4.0);
        let Some(console) = state.ssh_console.as_mut().filter(|_| matching) else {
            let guest = state
                .vm_state
                .ready()
                .and_then(|details| details.guest_ips.first())
                .cloned()
                .unwrap_or_default();
            ui.label(
                RichText::new(if guest.is_empty() {
                    "Open SSH to enter the guest address and credentials.".to_owned()
                } else {
                    format!("Guest address: {guest}. Open SSH to enter credentials.")
                })
                .size(10.0)
                .color(dim_fg()),
            );
            return;
        };
        if !console.message.is_empty() {
            ui.label(RichText::new(&console.message).size(9.5).color(
                if console.status == crate::state::SshStatus::Closed {
                    amber()
                } else {
                    muted_fg()
                },
            ));
        }
        egui::Frame::NONE
            .fill(egui::Color32::from_rgb(18, 20, 24))
            .corner_radius(egui::CornerRadius::same(6))
            .inner_margin(egui::Margin::same(8))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                let scroll = egui::ScrollArea::vertical()
                    .max_height(360.0)
                    .stick_to_bottom(true)
                    .auto_shrink([false, false]);
                scroll.show(ui, |ui| {
                    let response = ui.add(
                        egui::Label::new(
                            RichText::new(console.buffer.display())
                                .monospace()
                                .size(10.0)
                                .color(egui::Color32::from_rgb(220, 225, 230)),
                        )
                        .sense(egui::Sense::click())
                        .wrap_mode(egui::TextWrapMode::Extend),
                    );
                    if response.clicked() {
                        response.request_focus();
                    }
                    let focused = ui.memory(|memory| memory.has_focus(response.id));
                    if focused {
                        let mut data = Vec::new();
                        let modifiers = ui.input(|input| input.modifiers);
                        for event in ui.input(|input| input.events.clone()) {
                            match event {
                                egui::Event::Text(text)
                                    if !modifiers.ctrl && !modifiers.alt && !modifiers.command =>
                                {
                                    data.extend_from_slice(text.as_bytes());
                                }
                                egui::Event::Key {
                                    key,
                                    pressed: true,
                                    repeat: false,
                                    modifiers,
                                    ..
                                } => {
                                    if modifiers.ctrl {
                                        match key {
                                            egui::Key::C => data.push(0x03),
                                            egui::Key::D => data.push(0x04),
                                            egui::Key::L => data.push(0x0c),
                                            egui::Key::Z => data.push(0x1a),
                                            _ => {}
                                        }
                                    } else {
                                        match key {
                                            egui::Key::Enter => data.push(b'\r'),
                                            egui::Key::Backspace => data.push(0x7f),
                                            egui::Key::Tab => data.push(b'\t'),
                                            egui::Key::Escape => data.push(0x1b),
                                            egui::Key::ArrowUp => data.extend_from_slice(b"\x1b[A"),
                                            egui::Key::ArrowDown => {
                                                data.extend_from_slice(b"\x1b[B")
                                            }
                                            egui::Key::ArrowRight => {
                                                data.extend_from_slice(b"\x1b[C")
                                            }
                                            egui::Key::ArrowLeft => {
                                                data.extend_from_slice(b"\x1b[D")
                                            }
                                            egui::Key::Home => data.extend_from_slice(b"\x1b[H"),
                                            egui::Key::End => data.extend_from_slice(b"\x1b[F"),
                                            egui::Key::Delete => data.extend_from_slice(b"\x1b[3~"),
                                            egui::Key::PageUp => data.extend_from_slice(b"\x1b[5~"),
                                            egui::Key::PageDown => {
                                                data.extend_from_slice(b"\x1b[6~")
                                            }
                                            _ => {}
                                        }
                                    }
                                }
                                _ => {}
                            }
                        }
                        if !data.is_empty() {
                            actions.push(UiAction::SshInput {
                                console_id: console.console_id.clone(),
                                data,
                            });
                        }
                    }
                });
            });
        ui.label(
            RichText::new(
                "Click the terminal to type. Ctrl+C, arrows, Tab and Enter are forwarded.",
            )
            .size(9.0)
            .color(dim_fg()),
        );
    });
}

fn console_region(
    pixels: &[u8],
    stride_width: u16,
    x: u16,
    y: u16,
    width: u16,
    height: u16,
) -> egui::ColorImage {
    let row_bytes = width as usize * 4;
    let stride = stride_width as usize * 4;
    let mut data = Vec::with_capacity(row_bytes * height as usize);
    for row in 0..height as usize {
        let start = (y as usize + row) * stride + x as usize * 4;
        if start + row_bytes <= pixels.len() {
            data.extend_from_slice(&pixels[start..start + row_bytes]);
        } else {
            data.resize(data.len() + row_bytes, 0);
        }
    }
    egui::ColorImage::from_rgba_unmultiplied([width as usize, height as usize], &data)
}

fn sync_console_modifiers(
    console: &mut crate::state::ConsoleState,
    modifiers: egui::Modifiers,
    focused: bool,
    console_id: &str,
    actions: &mut Vec<UiAction>,
) {
    let desired = (
        focused && modifiers.shift,
        focused && modifiers.ctrl,
        focused && modifiers.alt,
        focused && modifiers.command && !modifiers.ctrl,
    );
    let current = console.modifiers;
    for (index, keycode) in [0xffe1u32, 0xffe3, 0xffe9, 0xffeb].into_iter().enumerate() {
        let wanted = match index {
            0 => desired.0,
            1 => desired.1,
            2 => desired.2,
            _ => desired.3,
        };
        let was = match index {
            0 => current.0,
            1 => current.1,
            2 => current.2,
            _ => current.3,
        };
        if wanted != was {
            actions.push(UiAction::ConsoleKey {
                console_id: console_id.to_owned(),
                keycode,
                down: wanted,
            });
        }
    }
    console.modifiers = desired;
}

fn is_printable_key(key: egui::Key) -> bool {
    use egui::Key;
    matches!(
        key,
        Key::Space
            | Key::Colon
            | Key::Comma
            | Key::Backslash
            | Key::Slash
            | Key::Pipe
            | Key::Questionmark
            | Key::Exclamationmark
            | Key::OpenBracket
            | Key::CloseBracket
            | Key::OpenCurlyBracket
            | Key::CloseCurlyBracket
            | Key::Backtick
            | Key::Minus
            | Key::Period
            | Key::Plus
            | Key::Equals
            | Key::Semicolon
            | Key::Quote
            | Key::Num0
            | Key::Num1
            | Key::Num2
            | Key::Num3
            | Key::Num4
            | Key::Num5
            | Key::Num6
            | Key::Num7
            | Key::Num8
            | Key::Num9
            | Key::A
            | Key::B
            | Key::C
            | Key::D
            | Key::E
            | Key::F
            | Key::G
            | Key::H
            | Key::I
            | Key::J
            | Key::K
            | Key::L
            | Key::M
            | Key::N
            | Key::O
            | Key::P
            | Key::Q
            | Key::R
            | Key::S
            | Key::T
            | Key::U
            | Key::V
            | Key::W
            | Key::X
            | Key::Y
            | Key::Z
    )
}

fn unicode_keysym(character: char) -> Option<u32> {
    let code = character as u32;
    match code {
        0x20..=0x7e | 0xa0..=0xff => Some(code),
        code if code > 0xff => Some(0x0100_0000 + code),
        _ => None,
    }
}

fn console_keysym(key: egui::Key) -> Option<u32> {
    use egui::Key;
    Some(match key {
        Key::ArrowDown => 0xff54,
        Key::ArrowLeft => 0xff51,
        Key::ArrowRight => 0xff53,
        Key::ArrowUp => 0xff52,
        Key::Escape => 0xff1b,
        Key::Tab => 0xff09,
        Key::Backspace => 0xff08,
        Key::Enter => 0xff0d,
        Key::Space => 0x20,
        Key::Insert => 0xff63,
        Key::Delete => 0xffff,
        Key::Home => 0xff50,
        Key::End => 0xff57,
        Key::PageUp => 0xff55,
        Key::PageDown => 0xff56,
        Key::Colon => 0x3a,
        Key::Comma => 0x2c,
        Key::Backslash => 0x5c,
        Key::Slash => 0x2f,
        Key::Pipe => 0x7c,
        Key::Questionmark => 0x3f,
        Key::Exclamationmark => 0x21,
        Key::OpenBracket => 0x5b,
        Key::CloseBracket => 0x5d,
        Key::OpenCurlyBracket => 0x7b,
        Key::CloseCurlyBracket => 0x7d,
        Key::Backtick => 0x60,
        Key::Minus => 0x2d,
        Key::Period => 0x2e,
        Key::Plus => 0x2b,
        Key::Equals => 0x3d,
        Key::Semicolon => 0x3b,
        Key::Quote => 0x27,
        Key::Num0 => 0x30,
        Key::Num1 => 0x31,
        Key::Num2 => 0x32,
        Key::Num3 => 0x33,
        Key::Num4 => 0x34,
        Key::Num5 => 0x35,
        Key::Num6 => 0x36,
        Key::Num7 => 0x37,
        Key::Num8 => 0x38,
        Key::Num9 => 0x39,
        Key::A => 0x61,
        Key::B => 0x62,
        Key::C => 0x63,
        Key::D => 0x64,
        Key::E => 0x65,
        Key::F => 0x66,
        Key::G => 0x67,
        Key::H => 0x68,
        Key::I => 0x69,
        Key::J => 0x6a,
        Key::K => 0x6b,
        Key::L => 0x6c,
        Key::M => 0x6d,
        Key::N => 0x6e,
        Key::O => 0x6f,
        Key::P => 0x70,
        Key::Q => 0x71,
        Key::R => 0x72,
        Key::S => 0x73,
        Key::T => 0x74,
        Key::U => 0x75,
        Key::V => 0x76,
        Key::W => 0x77,
        Key::X => 0x78,
        Key::Y => 0x79,
        Key::Z => 0x7a,
        Key::F1 => 0xffbe,
        Key::F2 => 0xffbf,
        Key::F3 => 0xffc0,
        Key::F4 => 0xffc1,
        Key::F5 => 0xffc2,
        Key::F6 => 0xffc3,
        Key::F7 => 0xffc4,
        Key::F8 => 0xffc5,
        Key::F9 => 0xffc6,
        Key::F10 => 0xffc7,
        Key::F11 => 0xffc8,
        Key::F12 => 0xffc9,
        Key::F13 => 0xffca,
        Key::F14 => 0xffcb,
        Key::F15 => 0xffcc,
        Key::F16 => 0xffcd,
        Key::F17 => 0xffce,
        Key::F18 => 0xffcf,
        Key::F19 => 0xffd0,
        Key::F20 => 0xffd1,
        Key::F21 => 0xffd2,
        Key::F22 => 0xffd3,
        Key::F23 => 0xffd4,
        Key::F24 => 0xffd5,
        Key::F25 => 0xffd6,
        Key::F26 => 0xffd7,
        Key::F27 => 0xffd8,
        Key::F28 => 0xffd9,
        Key::F29 => 0xffda,
        Key::F30 => 0xffdb,
        Key::F31 => 0xffdc,
        Key::F32 => 0xffdd,
        Key::F33 => 0xffde,
        Key::F34 => 0xffdf,
        Key::F35 => 0xffe0,
        _ => return None,
    })
}

fn storage(
    ui: &mut Ui,
    read_only: bool,
    is_template: bool,
    is_snapshot: bool,
    halted: bool,
    details: &VmDetails,
    actions: &mut Vec<UiAction>,
) {
    let disks = &details.disks;
    theme::card_frame().show(ui, |ui| {
        ui.set_width(ui.available_width());
        let editable = !read_only && !is_template && !is_snapshot;
        ui.horizontal(|ui| {
            ui.label(RichText::new("Virtual disks").size(15.0).strong());
            widgets::badge(ui, disks.len().to_string(), muted_fg());
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if ui
                    .add_enabled(editable && halted, widgets::primary_button("Attach disk"))
                    .on_hover_text(if halted {
                        "Attach an existing unattached VDI"
                    } else {
                        "Shut down the VM to attach a disk"
                    })
                    .clicked()
                {
                    actions.push(UiAction::OpenAttachVdi);
                }
                if ui
                    .add_enabled(editable && halted, widgets::primary_button("Add disk"))
                    .on_hover_text(if halted {
                        "Create a new virtual disk"
                    } else {
                        "Shut down the VM to add a disk"
                    })
                    .clicked()
                {
                    actions.push(UiAction::OpenAddDisk);
                }
                if editable && !halted {
                    ui.label(
                        RichText::new("Shut down to edit disks")
                            .size(9.0)
                            .color(dim_fg()),
                    );
                }
            });
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
            .column(Column::remainder().at_least(160.0).clip(true))
            .column(Column::initial(120.0).clip(true))
            .column(Column::exact(80.0))
            .column(Column::exact(55.0))
            .column(Column::exact(65.0))
            .column(Column::exact(65.0))
            .column(Column::exact(150.0))
            .header(28.0, |mut header| {
                for title in [
                    "Device", "VDI", "SR", "Size", "Mode", "Attached", "Bootable", "Actions",
                ] {
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
                        let (label, color) = match disk.mode.as_str() {
                            "RW" => ("RW", green()),
                            "RO" => ("RO", amber()),
                            _ => ("—", dim_fg()),
                        };
                        ui.label(RichText::new(label).monospace().size(10.5).color(color));
                    });
                    row.col(|ui| {
                        ui.label(
                            RichText::new(if disk.attached { "Yes" } else { "No" })
                                .size(10.5)
                                .color(if disk.attached { green() } else { dim_fg() }),
                        );
                    });
                    row.col(|ui| {
                        if editable {
                            let mut bootable = disk.bootable;
                            if ui
                                .checkbox(&mut bootable, "")
                                .on_hover_text("Bootable flag (takes effect on next start)")
                                .changed()
                            {
                                actions.push(UiAction::SetVbdBootable {
                                    vbd_uuid: disk.vbd_uuid.clone(),
                                    bootable,
                                });
                            }
                        } else {
                            ui.label(
                                RichText::new(if disk.bootable { "Yes" } else { "No" })
                                    .size(10.5)
                                    .color(muted_fg()),
                            );
                        }
                    });
                    row.col(|ui| {
                        if disk.is_cd {
                            ui.label(RichText::new("CD media below").size(9.0).color(dim_fg()));
                            return;
                        }
                        if !editable {
                            ui.label(RichText::new("—").size(10.0).color(dim_fg()));
                            return;
                        }
                        let gib = disk.virtual_size.div_ceil(1024 * 1024 * 1024);
                        ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing.x = 4.0;
                            if halted {
                                ui.menu_button(RichText::new("Manage…").size(9.5), |ui| {
                                    let target = if disk.mode == "RO" { "RW" } else { "RO" };
                                    if ui
                                        .button(
                                            RichText::new(if target == "RW" {
                                                "Make read-write"
                                            } else {
                                                "Make read-only"
                                            })
                                            .size(10.5),
                                        )
                                        .clicked()
                                    {
                                        actions.push(UiAction::SetVbdMode {
                                            vbd_uuid: disk.vbd_uuid.clone(),
                                            mode: target.into(),
                                        });
                                        ui.close();
                                    }
                                    if ui
                                        .button(RichText::new("Move to another SR…").size(10.5))
                                        .clicked()
                                    {
                                        actions.push(UiAction::OpenMoveDisk {
                                            vbd_uuid: disk.vbd_uuid.clone(),
                                            name: if disk.vdi_name.is_empty() {
                                                format!("Disk {}", disk.device)
                                            } else {
                                                disk.vdi_name.clone()
                                            },
                                            size: disk.virtual_size,
                                            source_sr_uuid: disk.sr_uuid.clone(),
                                        });
                                        ui.close();
                                    }
                                    if ui
                                        .add_enabled(
                                            !disk.read_only
                                                && disk.mode == "RW"
                                                && disk.allowed_operations.iter().any(|op| {
                                                    op == "resize" || op == "resize_online"
                                                }),
                                            egui::Button::new(RichText::new("Resize…").size(10.5)),
                                        )
                                        .clicked()
                                    {
                                        actions.push(UiAction::OpenDiskResize {
                                            vdi_uuid: disk.vdi_uuid.clone(),
                                            name: if disk.vdi_name.is_empty() {
                                                format!("Disk {}", disk.device)
                                            } else {
                                                disk.vdi_name.clone()
                                            },
                                            current_gib: gib,
                                        });
                                        ui.close();
                                    }
                                    ui.separator();
                                    if ui
                                        .button(
                                            RichText::new("Remove disk…").size(10.5).color(red()),
                                        )
                                        .clicked()
                                    {
                                        actions.push(UiAction::PromptRemoveDisk {
                                            vbd_uuid: disk.vbd_uuid.clone(),
                                            name: if disk.vdi_name.is_empty() {
                                                format!("Disk {}", disk.device)
                                            } else {
                                                disk.vdi_name.clone()
                                            },
                                            owner: disk.owner,
                                        });
                                        ui.close();
                                    }
                                });
                            } else {
                                ui.menu_button(RichText::new("Manage…").size(9.5), |ui| {
                                    if ui
                                        .add(
                                            egui::Button::new(
                                                RichText::new(if disk.attached {
                                                    "Unplug"
                                                } else {
                                                    "Plug"
                                                })
                                                .size(9.5),
                                            )
                                            .small(),
                                        )
                                        .on_hover_text(
                                            "Hot-plug or unplug this disk from the running VM",
                                        )
                                        .clicked()
                                    {
                                        actions.push(UiAction::SetVbdPlugged {
                                            vbd_uuid: disk.vbd_uuid.clone(),
                                            plugged: !disk.attached,
                                        });
                                    }
                                    if ui
                                        .add_enabled(
                                            details.power_state == "Running"
                                                && !disk.read_only
                                                && disk.mode == "RW"
                                                && disk.allowed_operations.iter().any(|op| {
                                                    op == "resize" || op == "resize_online"
                                                }),
                                            egui::Button::new(RichText::new("Resize…").size(9.5))
                                                .small(),
                                        )
                                        .clicked()
                                    {
                                        actions.push(UiAction::OpenDiskResize {
                                            vdi_uuid: disk.vdi_uuid.clone(),
                                            name: disk.vdi_name.clone(),
                                            current_gib: gib,
                                        });
                                    }
                                    if ui
                                        .add(
                                            egui::Button::new(RichText::new("Move").size(9.5))
                                                .small(),
                                        )
                                        .on_hover_text("Move this disk to another SR")
                                        .clicked()
                                    {
                                        actions.push(UiAction::OpenMoveDisk {
                                            vbd_uuid: disk.vbd_uuid.clone(),
                                            name: if disk.vdi_name.is_empty() {
                                                format!("Disk {}", disk.device)
                                            } else {
                                                disk.vdi_name.clone()
                                            },
                                            size: disk.virtual_size,
                                            source_sr_uuid: disk.sr_uuid.clone(),
                                        });
                                    }
                                });
                            }
                        });
                    });
                });
            });
    });

    ui.add_space(8.0);
    let cds: Vec<&VmDisk> = details.disks.iter().filter(|disk| disk.is_cd).collect();
    theme::card_frame().show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.horizontal(|ui| {
            ui.label(RichText::new("CD/DVD drives").size(15.0).strong());
            widgets::badge(ui, cds.len().to_string(), muted_fg());
            ui.label(
                RichText::new(format!("{} ISO images available", details.isos.len()))
                    .size(9.5)
                    .color(dim_fg()),
            );
        });
        ui.add_space(4.0);
        let editable = !read_only && !is_template && !is_snapshot;
        if cds.is_empty() {
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new("No CD/DVD drive is attached to this VM.")
                        .size(11.0)
                        .color(dim_fg()),
                );
                if ui
                    .add_enabled(
                        editable && halted,
                        egui::Button::new(RichText::new("Add CD drive").size(10.5)),
                    )
                    .on_hover_text(if halted {
                        "Attach an empty CD/DVD drive"
                    } else {
                        "Shut down the VM to add a CD drive"
                    })
                    .clicked()
                {
                    actions.push(UiAction::AddCdDrive);
                }
            });
            return;
        }
        for disk in cds {
            ui.horizontal(|ui| {
                widgets::dot(ui, purple(), 6.0);
                ui.vertical(|ui| {
                    ui.spacing_mut().item_spacing.y = 0.0;
                    ui.label(
                        RichText::new(format!(
                            "CD drive {}",
                            if disk.device.is_empty() {
                                "—"
                            } else {
                                &disk.device
                            }
                        ))
                        .size(11.0)
                        .strong(),
                    );
                    ui.label(
                        RichText::new(if disk.vdi_name.is_empty() {
                            "Empty".to_owned()
                        } else {
                            format!(
                                "{} · {}",
                                disk.vdi_name,
                                if disk.sr_name.is_empty() {
                                    "Unknown SR"
                                } else {
                                    &disk.sr_name
                                }
                            )
                        })
                        .size(10.0)
                        .color(if disk.vdi_name.is_empty() {
                            dim_fg()
                        } else {
                            muted_fg()
                        }),
                    );
                });
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if ui
                        .add_enabled(
                            editable,
                            egui::Button::new(RichText::new("Change media…").size(10.5)),
                        )
                        .clicked()
                    {
                        actions.push(UiAction::OpenIsoDialog {
                            vbd_uuid: disk.vbd_uuid.clone(),
                            drive: format!("CD drive {}", disk.device),
                        });
                    }
                });
            });
            ui.add_space(4.0);
        }
        if !editable {
            ui.label(
                RichText::new("Read-only connection or non-VM object: media cannot be changed.")
                    .size(9.0)
                    .color(dim_fg()),
            );
        }
    });
}

fn network(ui: &mut Ui, interfaces: &[VmInterface], editable: bool, actions: &mut Vec<UiAction>) {
    theme::card_frame().show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.horizontal(|ui| {
            ui.label(
                RichText::new("Virtual network interfaces")
                    .size(15.0)
                    .strong(),
            );
            widgets::badge(ui, interfaces.len().to_string(), muted_fg());
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if ui
                    .add_enabled(editable, widgets::primary_button("Add interface"))
                    .clicked()
                {
                    actions.push(UiAction::OpenVifDialog(None));
                }
            });
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
            .column(Column::exact(60.0))
            .column(Column::remainder().at_least(130.0))
            .column(Column::initial(120.0))
            .column(Column::initial(110.0))
            .column(Column::initial(110.0))
            .column(Column::exact(70.0))
            .column(Column::exact(140.0))
            .header(28.0, |mut header| {
                for title in [
                    "Device", "Network", "MAC", "IPv4", "IPv6", "Attached", "Actions",
                ] {
                    header.col(|ui| {
                        ui.label(RichText::new(title).size(10.0).color(muted_fg()));
                    });
                }
            })
            .body(|body| {
                body.rows(32.0, interfaces.len(), |mut row| {
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
                        ui.add(
                            egui::Label::new(
                                RichText::new(&interface.mac)
                                    .monospace()
                                    .size(9.0)
                                    .color(muted_fg()),
                            )
                            .truncate(),
                        );
                    });
                    row.col(|ui| {
                        ui.add(
                            egui::Label::new(
                                RichText::new(interface.ipv4.join(", "))
                                    .size(9.0)
                                    .color(muted_fg()),
                            )
                            .truncate(),
                        );
                    });
                    row.col(|ui| {
                        ui.add(
                            egui::Label::new(
                                RichText::new(interface.ipv6.join(", "))
                                    .size(9.0)
                                    .color(dim_fg()),
                            )
                            .truncate(),
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
                    row.col(|ui| {
                        if !editable {
                            ui.label(RichText::new("—").size(10.0).color(dim_fg()));
                            return;
                        }
                        ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing.x = 4.0;
                            if ui
                                .add(egui::Button::new(RichText::new("Edit").size(9.5)).small())
                                .clicked()
                            {
                                actions.push(UiAction::OpenVifDialog(Some(interface.uuid.clone())));
                            }
                            if ui
                                .add(
                                    egui::Button::new(
                                        RichText::new(if interface.attached {
                                            "Unplug"
                                        } else {
                                            "Plug"
                                        })
                                        .size(9.5),
                                    )
                                    .small(),
                                )
                                .clicked()
                            {
                                actions.push(UiAction::SetVifPlugged {
                                    vif_uuid: interface.uuid.clone(),
                                    plugged: !interface.attached,
                                });
                            }
                            if ui
                                .add(
                                    egui::Button::new(
                                        RichText::new("Remove").size(9.5).color(red()),
                                    )
                                    .small(),
                                )
                                .clicked()
                            {
                                actions.push(UiAction::PromptRemoveVif {
                                    vif_uuid: interface.uuid.clone(),
                                    mac: interface.mac.clone(),
                                });
                            }
                        });
                    });
                });
            });
    });
}

fn ha(
    ui: &mut Ui,
    details: &VmDetails,
    editable: bool,
    running: bool,
    actions: &mut Vec<UiAction>,
) {
    theme::card_frame().show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.horizontal(|ui| {
            ui.label(RichText::new("High availability").size(15.0).strong());
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if ui
                    .add_enabled(editable, widgets::primary_button("Edit HA settings"))
                    .on_hover_text(if running {
                        "Some changes take effect on the next start"
                    } else {
                        "Change restart priority, always-run, delays and affinity"
                    })
                    .clicked()
                {
                    actions.push(UiAction::OpenHaEdit);
                }
            });
        });
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
        widgets::kv(
            ui,
            "Start delay",
            &format!("{} seconds", details.start_delay),
            false,
        );
        widgets::kv(
            ui,
            "Shutdown delay",
            &format!("{} seconds", details.shutdown_delay),
            false,
        );
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
    state: &mut UiState,
    workspace: &WorkspaceView,
    connection: &xen_core::workspace::ConnectionView,
    vm: &xen_core::xapi::VirtualMachine,
    read_only: bool,
    actions: &mut Vec<UiAction>,
) {
    theme::card_frame().show(ui, |ui| {
        ui.set_width(ui.available_width());
        let Some(inventory) = connection_inventory(connection) else {
            return;
        };
        let snapshots: Vec<_> = inventory
            .snapshots
            .iter()
            .filter(|snapshot| snapshot.snapshot_of == vm.reference)
            .collect();
        let available: std::collections::HashSet<&str> =
            snapshots.iter().map(|snapshot| snapshot.uuid.as_str()).collect();
        state
            .snapshot_selection
            .retain(|uuid| available.contains(uuid.as_str()));

        ui.horizontal(|ui| {
            widgets::section_title(ui, "Disk snapshots", None);
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                let selected = state.snapshot_selection.len();
                if ui
                    .add_enabled(
                        !read_only && selected > 0,
                        egui::Button::new(
                            RichText::new(if selected > 0 {
                                format!("Delete selected ({selected})")
                            } else {
                                "Delete selected".into()
                            })
                            .size(10.5)
                            .color(if selected > 0 { red() } else { dim_fg() }),
                        ),
                    )
                    .clicked()
                {
                    actions.push(UiAction::PromptDeleteSnapshots);
                }
                let all_selected = !snapshots.is_empty()
                    && snapshots
                        .iter()
                        .all(|snapshot| state.snapshot_selection.contains(&snapshot.uuid));
                let mut select_all = all_selected;
                if ui
                    .add_enabled(
                        !snapshots.is_empty(),
                        egui::Checkbox::new(&mut select_all, RichText::new("Select all").size(9.5)),
                    )
                    .changed()
                {
                    if select_all {
                        for snapshot in &snapshots {
                            state.snapshot_selection.insert(snapshot.uuid.clone());
                        }
                    } else {
                        for snapshot in &snapshots {
                            state.snapshot_selection.remove(&snapshot.uuid);
                        }
                    }
                }
                ui.label(
                    RichText::new(format!("{} snapshots", snapshots.len()))
                        .size(9.5)
                        .color(dim_fg()),
                );
            });
        });
        ui.add_space(4.0);
        if snapshots.is_empty() {
            ui.label(
                RichText::new("No snapshots for this VM.")
                    .size(11.0)
                    .color(dim_fg()),
            );
        }
        for snapshot in &snapshots {
            ui.horizontal(|ui| {
                let mut checked = state.snapshot_selection.contains(&snapshot.uuid);
                if ui
                    .add_enabled(
                        !read_only,
                        egui::Checkbox::without_text(&mut checked),
                    )
                    .changed()
                {
                    if checked {
                        state.snapshot_selection.insert(snapshot.uuid.clone());
                    } else {
                        state.snapshot_selection.remove(&snapshot.uuid);
                    }
                }
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
                    ui.menu_button("Create…", |ui| {
                        for kind in [ActionKind::SnapshotToVm, ActionKind::SnapshotToTemplate] {
                            if ui.add_enabled(action_allowed(workspace, connection_id(connection), snapshot, "clone"), egui::Button::new(kind.label())).clicked() {
                                actions.push(UiAction::Propose(kind, vec![(connection_id(connection).to_owned(), snapshot.uuid.clone())]));
                                ui.close();
                            }
                        }
                    });
                    if ui
                        .add_enabled(
                            !read_only,
                            egui::Button::new(RichText::new("Delete").size(10.0).color(red())),
                        )
                        .clicked()
                    {
                        state.snapshot_selection.clear();
                        state.snapshot_selection.insert(snapshot.uuid.clone());
                        actions.push(UiAction::PromptDeleteSnapshots);
                    }
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
                "Reverting replaces disk state. If the snapshot was running, the VM is started again.",
            )
            .size(9.5)
            .color(dim_fg()),
        );
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn console_keysyms_use_x11_values() {
        assert_eq!(console_keysym(egui::Key::A), Some(0x61));
        assert_eq!(console_keysym(egui::Key::Enter), Some(0xff0d));
        assert_eq!(console_keysym(egui::Key::F5), Some(0xffc2));
        assert_eq!(console_keysym(egui::Key::ArrowLeft), Some(0xff51));
        assert_eq!(console_keysym(egui::Key::MediaPlayPause), None);
        assert!(is_printable_key(egui::Key::A));
        assert!(!is_printable_key(egui::Key::Enter));
        assert_eq!(unicode_keysym('a'), Some(0x61));
        assert_eq!(unicode_keysym('ก'), Some(0x0100_0e01));
        assert_eq!(unicode_keysym('\n'), None);
    }
}
