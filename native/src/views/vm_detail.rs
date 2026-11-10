use egui::{Align, Layout, RichText, Ui};
use xen_core::workspace::WorkspaceView;

use crate::format;
use crate::state::{
    ActionKind, UiAction, UiState, action_allowed, connection_name, find_vm, is_read_only,
    running_task,
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
    let Some((connection_id, vm_uuid)) = state.detail.clone() else {
        return;
    };
    let Some((connection, vm)) = find_vm(workspace, &connection_id, &vm_uuid) else {
        state.detail = None;
        return;
    };
    let read_only = is_read_only(connection);
    let conn_name = connection_name(connection);

    egui::Panel::right("vm-detail")
        .exact_size(438.0)
        .resizable(false)
        .frame(
            egui::Frame::NONE
                .fill(panel())
                .stroke(egui::Stroke::new(1.0, border()))
                .inner_margin(egui::Margin::same(16)),
        )
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(RichText::new(&conn_name).size(9.5).color(dim_fg()));
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if ui
                        .add(
                            egui::Button::new(RichText::new("×").size(15.0).color(muted_fg()))
                                .frame(false),
                        )
                        .clicked()
                    {
                        actions.push(UiAction::CloseVm);
                    }
                });
            });
            ui.add_space(2.0);
            ui.label(RichText::new(&vm.name).size(20.0).strong());
            ui.horizontal(|ui| {
                widgets::power_badge(ui, &vm.power_state);
                ui.label(RichText::new("·").size(10.0).color(dim_fg()));
                ui.label(RichText::new(&vm.host_name).size(10.0).color(muted_fg()));
                if read_only {
                    widgets::badge(ui, "Read-only", amber());
                }
            });
            ui.add_space(8.0);

            let start_kind = if vm.power_state == "Suspended" {
                ActionKind::Resume
            } else {
                ActionKind::Start
            };
            ui.horizontal(|ui| {
                let can_start =
                    action_allowed(workspace, &connection_id, vm, start_kind.operation());
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
                        vec![(connection_id.clone(), vm_uuid.clone())],
                    ));
                }
                let can_shutdown = action_allowed(
                    workspace,
                    &connection_id,
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
                        vec![(connection_id.clone(), vm_uuid.clone())],
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
                            let allowed =
                                action_allowed(workspace, &connection_id, vm, kind.operation());
                            let color = if kind.destructive() { red() } else { fg() };
                            if ui
                                .add_enabled(
                                    allowed,
                                    egui::Button::new(
                                        RichText::new(kind.label()).size(11.0).color(color),
                                    )
                                    .frame(false),
                                )
                                .clicked()
                            {
                                actions.push(UiAction::Propose(
                                    kind,
                                    vec![(connection_id.clone(), vm_uuid.clone())],
                                ));
                                ui.close();
                            }
                        }
                    },
                );
            });

            if running_task(workspace, &connection_id, &vm_uuid) {
                ui.add_space(4.0);
                ui.label(
                    RichText::new("A task is active for this object.")
                        .size(9.5)
                        .color(amber()),
                );
            }

            ui.add_space(10.0);
            egui::ScrollArea::vertical()
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    theme::inset_frame().show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        widgets::section_title(ui, "General", None);
                        ui.add_space(4.0);
                        let rows = [
                            ("Connection", conn_name.clone()),
                            ("Host", vm.host_name.clone()),
                            ("Virtual CPUs", vm.vcpus.to_string()),
                            ("Configured memory", format::bytes(vm.memory)),
                            ("Block devices", vm.disk_count.to_string()),
                            ("Network interfaces", vm.interface_count.to_string()),
                            (
                                "Operating system",
                                if vm.os.is_empty() {
                                    "Not reported".into()
                                } else {
                                    vm.os.clone()
                                },
                            ),
                            (
                                "Description",
                                if vm.description.is_empty() {
                                    "No description".into()
                                } else {
                                    vm.description.clone()
                                },
                            ),
                        ];
                        for (label, value) in rows {
                            widgets::kv(ui, label, &value, false);
                        }
                        ui.add_space(2.0);
                        widgets::kv(ui, "UUID", &vm.uuid, true);
                    });

                    ui.add_space(8.0);
                    theme::inset_frame().show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        ui.horizontal(|ui| {
                            widgets::section_title(ui, "Disk snapshots", None);
                            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                let mut count = 0;
                                if let Some(inventory) =
                                    crate::state::connection_inventory(connection)
                                {
                                    count = inventory
                                        .snapshots
                                        .iter()
                                        .filter(|snapshot| snapshot.snapshot_of == vm.reference)
                                        .count();
                                }
                                ui.label(
                                    RichText::new(format!("{count} snapshots"))
                                        .size(9.0)
                                        .color(dim_fg()),
                                );
                            });
                        });
                        ui.add_space(2.0);
                        if let Some(inventory) = crate::state::connection_inventory(connection) {
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
                                    ui.vertical(|ui| {
                                        ui.spacing_mut().item_spacing.y = 0.0;
                                        ui.label(RichText::new(&snapshot.name).size(10.5));
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
                                            &connection_id,
                                            snapshot,
                                            ActionKind::Revert.operation(),
                                        );
                                        if ui
                                            .add_enabled(
                                                allowed,
                                                egui::Button::new(
                                                    RichText::new("Revert").size(9.5),
                                                ),
                                            )
                                            .clicked()
                                        {
                                            actions.push(UiAction::Propose(
                                                ActionKind::Revert,
                                                vec![(
                                                    connection_id.clone(),
                                                    snapshot.uuid.clone(),
                                                )],
                                            ));
                                        }
                                    });
                                });
                                ui.add_space(2.0);
                            }
                        }
                        ui.add_space(4.0);
                        if ui
                            .add_enabled(
                                action_allowed(
                                    workspace,
                                    &connection_id,
                                    vm,
                                    ActionKind::Snapshot.operation(),
                                ),
                                egui::Button::new(RichText::new("Take snapshot").size(10.5)),
                            )
                            .clicked()
                        {
                            actions.push(UiAction::Propose(
                                ActionKind::Snapshot,
                                vec![(connection_id.clone(), vm_uuid.clone())],
                            ));
                        }
                    });
                    ui.add_space(8.0);
                    ui.label(
                        RichText::new("Power state changes are reported by XenAPI tasks.")
                            .size(9.0)
                            .color(dim_fg()),
                    );
                });
        });
}
