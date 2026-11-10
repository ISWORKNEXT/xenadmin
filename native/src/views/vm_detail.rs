use egui::{Align, Layout, RichText, Sense, Ui, Vec2};
use xen_core::workspace::WorkspaceView;

use crate::format;
use crate::state::{
    ActionKind, ObjectKey, UiAction, UiState, VmTab, action_allowed, connection_inventory,
    connection_name, find_vm, is_read_only, running_task,
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
            ui.label(RichText::new(kind.to_uppercase()).size(8.5).color(dim_fg()));
            ui.label(RichText::new(&vm.name).size(22.0).strong());
            ui.horizontal(|ui| {
                widgets::power_badge(ui, &vm.power_state);
                ui.label(RichText::new("·").size(10.0).color(dim_fg()));
                ui.label(
                    RichText::new(connection_name(connection))
                        .size(10.0)
                        .color(muted_fg()),
                );
                ui.label(RichText::new("·").size(10.0).color(dim_fg()));
                ui.label(RichText::new(&vm.host_name).size(10.0).color(muted_fg()));
                if read_only {
                    widgets::badge(ui, "Read-only", amber());
                }
            });
        });
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            if ui
                .add(egui::Button::new(RichText::new("Refresh").size(10.5)))
                .clicked()
            {
                actions.push(UiAction::Refresh(Some(connection_id.clone())));
            }
        });
    });
    ui.label(
        RichText::new(&vm.uuid)
            .monospace()
            .size(8.5)
            .color(dim_fg()),
    );
    ui.add_space(8.0);

    if !is_template && !is_snapshot {
        let mut index = match state.vm_subtab {
            VmTab::General => 0,
            VmTab::Snapshots => 1,
        };
        widgets::tab_bar(ui, &VmTab::ALL.map(VmTab::label), &mut index);
        ui.add_space(4.0);
        state.vm_subtab = if index == 0 {
            VmTab::General
        } else {
            VmTab::Snapshots
        };
    }

    action_bar(
        ui,
        state,
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

    if state.vm_subtab == VmTab::Snapshots && !is_template && !is_snapshot {
        snapshots(ui, workspace, connection, vm, read_only, actions);
    } else {
        general(ui, connection, vm, is_template, is_snapshot, workspace);
    }
}

#[allow(clippy::too_many_arguments)]
fn action_bar(
    ui: &mut Ui,
    _state: &mut UiState,
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
            ui.add_enabled(false, widgets::primary_button("New VM from template"));
            ui.label(
                RichText::new("Template provisioning is still being migrated.")
                    .size(9.5)
                    .color(dim_fg()),
            );
            if read_only {
                widgets::badge(ui, "Read-only", amber());
            }
        });
        return;
    }
    if is_snapshot {
        let allowed = action_allowed(workspace, connection_id, vm, ActionKind::Revert.operation());
        if ui
            .add_enabled(allowed, widgets::primary_button("Revert to this snapshot"))
            .clicked()
        {
            actions.push(UiAction::Propose(
                ActionKind::Revert,
                vec![(connection_id.to_owned(), vm_uuid.to_owned())],
            ));
        }
        ui.label(
            RichText::new("Revert replaces the parent VM's disk state.")
                .size(9.5)
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
                            .size(9.5)
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
                egui::Button::new(RichText::new("Take snapshot").size(10.5)),
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
                    .size(9.5)
                    .color(amber()),
            );
        }
    });
}

fn general(
    ui: &mut Ui,
    connection: &xen_core::workspace::ConnectionView,
    vm: &xen_core::xapi::VirtualMachine,
    is_template: bool,
    is_snapshot: bool,
    workspace: &WorkspaceView,
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
                    "General"
                },
                None,
            );
            ui.add_space(4.0);
            widgets::kv(ui, "Connection", &connection_name(connection), false);
            widgets::kv(ui, "Host", &vm.host_name, false);
            widgets::kv(ui, "Virtual CPUs", &vm.vcpus.to_string(), false);
            widgets::kv(ui, "Configured memory", &format::bytes(vm.memory), false);
            widgets::kv(ui, "Block devices", &vm.disk_count.to_string(), false);
            widgets::kv(
                ui,
                "Network interfaces",
                &vm.interface_count.to_string(),
                false,
            );
            widgets::kv(
                ui,
                "Operating system",
                if vm.os.is_empty() {
                    "Not reported"
                } else {
                    &vm.os
                },
                false,
            );
            ui.add_space(2.0);
            widgets::kv(ui, "UUID", &vm.uuid, true);
        });
        theme::card_frame().show(&mut columns[1], |ui| {
            ui.set_width(ui.available_width());
            widgets::section_title(ui, "Description", None);
            ui.add_space(4.0);
            ui.label(
                RichText::new(if vm.description.is_empty() {
                    "No description"
                } else {
                    &vm.description
                })
                .size(11.0)
                .color(if vm.description.is_empty() {
                    dim_fg()
                } else {
                    fg()
                }),
            );
            if is_snapshot {
                ui.add_space(8.0);
                widgets::section_title(ui, "Snapshot parent", None);
                let parent = workspace
                    .connections
                    .iter()
                    .find(|candidate| {
                        crate::state::connection_id(candidate)
                            == crate::state::connection_id(connection)
                    })
                    .and_then(connection_inventory)
                    .and_then(|inventory| {
                        inventory
                            .vms
                            .iter()
                            .find(|candidate| candidate.reference == vm.snapshot_of)
                    })
                    .map(|parent| parent.name.clone())
                    .unwrap_or_else(|| "Not available".into());
                ui.label(RichText::new(parent).size(11.0).color(muted_fg()));
                if !vm.snapshot_time.is_empty() {
                    ui.add_space(4.0);
                    ui.label(
                        RichText::new(format!("Taken {}", vm.snapshot_time))
                            .size(9.5)
                            .color(dim_fg()),
                    );
                }
            }
        });
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
                        .size(9.0)
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
                    .size(10.0)
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
                            egui::Label::new(RichText::new(&snapshot.name).size(11.0))
                                .sense(Sense::click()),
                        )
                        .clicked()
                    {
                        actions.push(UiAction::OpenObject {
                            key: ObjectKey::Snapshot {
                                connection_id: crate::state::connection_id(connection).to_owned(),
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
                        .size(8.5)
                        .color(dim_fg()),
                    );
                });
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    let allowed = action_allowed(
                        workspace,
                        crate::state::connection_id(connection),
                        snapshot,
                        ActionKind::Revert.operation(),
                    );
                    if ui
                        .add_enabled(
                            allowed,
                            egui::Button::new(RichText::new("Revert").size(9.5)),
                        )
                        .clicked()
                    {
                        actions.push(UiAction::Propose(
                            ActionKind::Revert,
                            vec![(
                                crate::state::connection_id(connection).to_owned(),
                                snapshot.uuid.clone(),
                            )],
                        ));
                    }
                });
            });
            ui.add_space(3.0);
        }
        ui.add_space(4.0);
        let can_snapshot = action_allowed(
            workspace,
            crate::state::connection_id(connection),
            vm,
            "snapshot",
        );
        if ui
            .add_enabled(
                can_snapshot && !read_only,
                egui::Button::new(RichText::new("Take snapshot").size(10.5)),
            )
            .clicked()
        {
            actions.push(UiAction::Propose(
                ActionKind::Snapshot,
                vec![(
                    crate::state::connection_id(connection).to_owned(),
                    vm.uuid.clone(),
                )],
            ));
        }
        ui.add_space(2.0);
        ui.label(
            RichText::new(
                "Reverting replaces disk state; it does not restore the previous power state.",
            )
            .size(9.0)
            .color(dim_fg()),
        );
    });
}
