use egui::{Align, Layout, RichText, Ui};
use xen_core::workspace::WorkspaceView;

use crate::state::{
    UiAction, UiState, connection_name, connection_settings, find_connection, find_vm,
    normalized_host,
};
use crate::theme;
use crate::theme::*;
use crate::widgets;

pub fn show(
    ctx: &egui::Context,
    state: &mut UiState,
    workspace: &WorkspaceView,
    actions: &mut Vec<UiAction>,
) {
    if state.help {
        help(ctx, state, actions);
    }
    if state.resource.is_some() {
        resource(ctx, state, actions);
    }
    if state.record_dialog.is_some() {
        record(ctx, state);
    }
    if state.vm_edit.is_some() {
        vm_edit(ctx, state, actions);
    }
    if state.confirm_remove.is_some() {
        confirm_remove(ctx, state, workspace, actions);
    }
    if state.connect.is_some() {
        connect(ctx, state, actions);
    }
    if state.intent.is_some() {
        intent(ctx, state, workspace, actions);
    }
}

fn modal_title(ui: &mut Ui, title: &str, subtitle: &str) {
    ui.label(RichText::new(title).size(17.0).strong());
    ui.label(RichText::new(subtitle).size(10.5).color(muted_fg()));
    ui.add_space(12.0);
}

fn connect(ctx: &egui::Context, state: &mut UiState, actions: &mut Vec<UiAction>) {
    let mut close = false;
    let mut submit = false;
    {
        let Some(connect) = state.connect.as_mut() else {
            return;
        };
        if connect.protected_host() {
            connect.settings.read_only = true;
        }
        let protected = connect.protected_host();
        let response = egui::Modal::new(egui::Id::unique("connect-modal"))
            .frame(theme::modal_frame())
            .show(ctx, |ui| {
                ui.set_width(500.0);
                modal_title(
                    ui,
                    if connect.profile_id.is_some() {
                        "Reconnect to a host"
                    } else {
                        "Add a connection"
                    },
                    "Other hosts stay connected while you add this one.",
                );

                widgets::field_label(ui, "Connection name");
                ui.add(
                    egui::TextEdit::singleline(&mut connect.settings.label)
                        .hint_text("e.g. Production · Bangkok")
                        .desired_width(f32::INFINITY)
                        .margin(egui::Margin::symmetric(8, 5)),
                );
                ui.add_space(6.0);

                widgets::field_label(ui, "Host address");
                if ui
                    .add(
                        egui::TextEdit::singleline(&mut connect.settings.host)
                            .hint_text("192.168.8.170")
                            .desired_width(f32::INFINITY)
                            .margin(egui::Margin::symmetric(8, 5)),
                    )
                    .changed()
                    && normalized_host(&connect.settings.host) == "10.100.1.11" {
                        connect.settings.read_only = true;
                    }
                ui.add_space(6.0);

                ui.columns(2, |columns| {
                    widgets::field_label(&mut columns[0], "Username");
                    columns[0].add(
                        egui::TextEdit::singleline(&mut connect.settings.username)
                            .desired_width(f32::INFINITY)
                            .margin(egui::Margin::symmetric(8, 5)),
                    );
                    widgets::field_label(&mut columns[1], "Password");
                    columns[1].add(
                        egui::TextEdit::singleline(&mut connect.password)
                            .password(true)
                            .hint_text("Never saved")
                            .desired_width(f32::INFINITY)
                            .margin(egui::Margin::symmetric(8, 5)),
                    );
                });
                ui.add_space(8.0);

                ui.add_enabled_ui(!protected, |ui| {
                    ui.checkbox(
                        &mut connect.settings.read_only,
                        RichText::new("Read-only connection").size(11.0),
                    );
                });
                ui.label(
                    RichText::new(if protected {
                        "Production host. Changes are blocked by the Rust backend."
                    } else {
                        "Block VM actions while allowing inventory and monitoring."
                    })
                    .size(9.5)
                    .color(if protected { amber() } else { dim_fg() }),
                );
                ui.add_space(4.0);
                ui.checkbox(
                    &mut connect.settings.tls.bypass_certificate_verification,
                    RichText::new("Bypass TLS certificate verification").size(11.0),
                );
                ui.label(
                    RichText::new(
                        "Applies to this connection only. HTTPS stays enabled, but certificate identity is not verified.",
                    )
                    .size(9.5)
                    .color(dim_fg()),
                );

                if let Some(error) = &connect.error {
                    ui.add_space(6.0);
                    ui.label(RichText::new(error).size(10.5).color(red()));
                }

                ui.add_space(14.0);
                ui.horizontal(|ui| {
                    if ui
                        .add(egui::Button::new(RichText::new("Cancel").size(11.0)))
                        .clicked()
                    {
                        close = true;
                    }
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        let enabled = !connect.submitting
                            && !connect.settings.host.trim().is_empty()
                            && !connect.settings.username.trim().is_empty();
                        if ui
                            .add_enabled(
                                enabled,
                                widgets::primary_button(if connect.submitting {
                                    "Connecting…"
                                } else {
                                    "Connect to host"
                                }),
                            )
                            .clicked()
                        {
                            submit = true;
                        }
                        if connect.submitting {
                            ui.spinner();
                        }
                    });
                });
            });
        if response.should_close() && !connect.submitting {
            close = true;
        }
        if submit && !connect.submitting {
            if connect.password.is_empty() {
                connect.error = Some("Enter the host password.".into());
            } else {
                connect.submitting = true;
                connect.error = None;
            }
        }
    }

    if submit
        && let Some(connect) = state.connect.as_mut()
        && connect.submitting
        && connect.error.is_none()
    {
        actions.push(UiAction::Connect {
            settings: connect.settings.clone(),
            password: connect.password.clone(),
            profile_id: connect.profile_id.clone(),
        });
    }
    if close {
        state.connect = None;
    }
}

fn intent(
    ctx: &egui::Context,
    state: &mut UiState,
    workspace: &WorkspaceView,
    actions: &mut Vec<UiAction>,
) {
    let mut close = false;
    let mut submit = false;
    let Some(intent) = state.intent.as_mut() else {
        return;
    };
    let targets: Vec<(String, String, String)> = intent
        .targets
        .iter()
        .map(|(connection_id, vm_uuid)| {
            let name = find_vm(workspace, connection_id, vm_uuid)
                .map_or_else(|| vm_uuid.clone(), |(_, vm)| vm.name.clone());
            let connection = find_connection(workspace, connection_id);
            (
                name,
                connection.map_or_else(|| connection_id.clone(), connection_name),
                connection
                    .map(|connection| connection_settings(connection).host.clone())
                    .unwrap_or_default(),
            )
        })
        .collect();
    let count = intent.targets.len();
    let has_vm_target = intent.targets.iter().any(|(connection_id, vm_uuid)| {
        find_vm(workspace, connection_id, vm_uuid).is_some_and(|(_, vm)| vm.kind == "vm")
    });
    let response = egui::Modal::new(egui::Id::unique("intent-modal"))
        .frame(theme::modal_frame())
        .show(ctx, |ui| {
            ui.set_width(520.0);
            let title = intent
                .host
                .as_ref()
                .map(|(_, host)| format!("{} on {}", intent.kind.label(), host))
                .unwrap_or_else(|| intent.kind.label().to_owned());
            modal_title(
                ui,
                &title,
                "Review the exact targets before submitting to XenAPI.",
            );
            theme::inset_frame().show(ui, |ui| {
                ui.set_width(ui.available_width());
                for (name, connection, address) in &targets {
                    ui.horizontal(|ui| {
                        widgets::dot(ui, accent(), 5.0);
                        ui.vertical(|ui| {
                            ui.spacing_mut().item_spacing.y = 0.0;
                            ui.label(RichText::new(name).size(11.0).strong());
                            ui.label(
                                RichText::new(format!("{connection} · {address}"))
                                    .size(9.0)
                                    .color(dim_fg()),
                            );
                        });
                    });
                }
            });
            if let Some((_, host)) = &intent.host {
                ui.add_space(6.0);
                ui.label(
                    RichText::new(format!("Target host: {host}"))
                        .size(10.0)
                        .color(accent()),
                );
            }
            if intent.kind.needs_name() {
                ui.add_space(8.0);
                widgets::field_label(
                    ui,
                    if intent.kind == crate::state::ActionKind::Clone {
                        "New machine name"
                    } else {
                        "Snapshot name"
                    },
                );
                ui.add(
                    egui::TextEdit::singleline(&mut intent.name)
                        .desired_width(f32::INFINITY)
                        .char_limit(255)
                        .margin(egui::Margin::symmetric(8, 5)),
                );
            }
            if intent.kind == crate::state::ActionKind::Revert {
                ui.add_space(8.0);
                ui.label(
                    RichText::new(
                        "Reverting replaces the parent VM's disk state. This does not automatically restart or resume the VM.",
                    )
                    .size(9.5)
                    .color(amber()),
                );
            }
            if intent.kind == crate::state::ActionKind::Destroy {
                ui.add_space(8.0);
                ui.label(
                    RichText::new(
                        "The object is removed from the host. This cannot be undone.",
                    )
                    .size(9.5)
                    .color(red()),
                );
                ui.add_space(4.0);
                ui.checkbox(
                    &mut intent.delete_disks,
                    RichText::new("Delete attached disks owned by this object")
                        .size(10.0),
                );
                ui.label(
                    RichText::new(
                        "Disks shared with another VM are never removed.",
                    )
                    .size(9.0)
                    .color(dim_fg()),
                );
                if has_vm_target {
                    ui.add_space(4.0);
                    ui.checkbox(
                        &mut intent.delete_snapshots,
                        RichText::new("Delete snapshots of this VM").size(10.0),
                    );
                    ui.label(
                        RichText::new(
                            "Snapshots left behind stay in the Snapshots list and keep their disks.",
                        )
                        .size(9.0)
                        .color(dim_fg()),
                    );
                }
            } else if intent.kind.destructive()
                && intent.kind != crate::state::ActionKind::Revert
            {
                ui.add_space(8.0);
                ui.label(
                    RichText::new(
                        "This bypasses guest shutdown. Unsaved data can be lost.",
                    )
                    .size(9.5)
                    .color(red()),
                );
            }
            ui.add_space(14.0);
            ui.horizontal(|ui| {
                if ui
                    .add(egui::Button::new(RichText::new("Cancel").size(11.0)))
                    .clicked()
                {
                    close = true;
                }
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    let valid_name = !intent.kind.needs_name() || !intent.name.trim().is_empty();
                    if ui
                        .add_enabled(
                            valid_name,
                            widgets::primary_button(format!(
                                "Submit {count} task{}",
                                if count == 1 { "" } else { "s" }
                            )),
                        )
                        .clicked()
                    {
                        submit = true;
                    }
                });
            });
        });
    if response.should_close() {
        close = true;
    }
    if submit {
        actions.push(UiAction::SubmitIntent);
    }
    if close {
        actions.push(UiAction::CloseIntent);
    }
}

fn vm_edit(ctx: &egui::Context, state: &mut UiState, actions: &mut Vec<UiAction>) {
    let mut close = false;
    let mut save = false;
    let Some(edit) = state.vm_edit.as_mut() else {
        return;
    };
    let response = egui::Modal::new(egui::Id::unique("vm-edit-modal"))
        .frame(theme::modal_frame())
        .show(ctx, |ui| {
            ui.set_width(520.0);
            modal_title(
                ui,
                "Edit virtual machine",
                "Name and description can change while the VM is running.",
            );
            widgets::field_label(ui, "Name");
            ui.add_enabled(
                !edit.submitting,
                egui::TextEdit::singleline(&mut edit.name)
                    .desired_width(f32::INFINITY)
                    .char_limit(255)
                    .margin(egui::Margin::symmetric(8, 5)),
            );
            ui.add_space(8.0);
            widgets::field_label(ui, "Description");
            ui.add_enabled(
                !edit.submitting,
                egui::TextEdit::multiline(&mut edit.description)
                    .desired_width(f32::INFINITY)
                    .desired_rows(3)
                    .char_limit(1024)
                    .margin(egui::Margin::symmetric(8, 5)),
            );
            ui.add_space(10.0);
            ui.columns(2, |columns| {
                resources_field(
                    &mut columns[0],
                    "vCPUs maximum",
                    &mut edit.vcpus_max,
                    !edit.running && !edit.submitting,
                );
                resources_field(
                    &mut columns[1],
                    "vCPUs at startup",
                    &mut edit.vcpus_at_startup,
                    !edit.running && !edit.submitting,
                );
            });
            ui.add_space(8.0);
            resources_field(
                ui,
                "Static memory maximum (MiB)",
                &mut edit.memory_mib,
                !edit.running && !edit.submitting,
            );
            ui.label(
                RichText::new("CPU and memory changes require the VM to be shut down. Dynamic memory limits are clamped to the new maximum.")
                    .size(9.0)
                    .color(if edit.running { amber() } else { dim_fg() }),
            );
            if let Some(error) = &edit.error {
                ui.add_space(8.0);
                ui.label(RichText::new(error).size(10.0).color(red()));
            }
            ui.add_space(14.0);
            ui.horizontal(|ui| {
                if ui
                    .add_enabled(
                        !edit.submitting,
                        egui::Button::new(RichText::new("Cancel").size(11.0)),
                    )
                    .clicked()
                {
                    close = true;
                }
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if ui
                        .add_enabled(
                            !edit.submitting && !edit.name.trim().is_empty(),
                            widgets::primary_button(if edit.submitting {
                                "Saving…"
                            } else {
                                "Save changes"
                            }),
                        )
                        .clicked()
                    {
                        save = true;
                    }
                });
            });
        });
    if response.should_close() && !edit.submitting {
        close = true;
    }
    if save {
        actions.push(UiAction::SaveVmEdit);
    }
    if close {
        actions.push(UiAction::CloseVmEdit);
    }
}

fn resources_field(ui: &mut Ui, label: &str, value: &mut String, enabled: bool) {
    widgets::field_label(ui, label);
    ui.add_enabled(
        enabled,
        egui::TextEdit::singleline(value)
            .desired_width(f32::INFINITY)
            .char_limit(8)
            .margin(egui::Margin::symmetric(8, 5)),
    );
}

fn help(ctx: &egui::Context, state: &mut UiState, actions: &mut Vec<UiAction>) {
    let mut close = false;
    let response = egui::Modal::new(egui::Id::unique("help-modal"))
        .frame(theme::modal_frame())
        .show(ctx, |ui| {
            ui.set_width(520.0);
            modal_title(
                ui,
                "Migration status",
                "Native egui capabilities and remaining work",
            );
            for line in [
                "Multiple simultaneous pools, host navigation and saved profiles",
                "VM lifecycle actions, bulk operations, clone and disk snapshots",
                "Task progress, results and cancellation where the host allows it",
            ] {
                ui.label(RichText::new(format!("✓  {line}")).size(11.0).color(green()));
                ui.add_space(3.0);
            }
            ui.label(
                RichText::new("•  Production 10.100.1.11 is read-only")
                    .size(11.0)
                    .color(amber()),
            );
            ui.add_space(8.0);
            ui.label(
                RichText::new(
                    "Still pending: new VM provisioning, console, migration, disk/network editing, snapshot disk cleanup, import/export, alerts, host updates and HA/DR workflows. This app does not yet replace every feature of XCP-ng Center.",
                )
                .size(10.0)
                .color(muted_fg()),
            );
            ui.add_space(14.0);
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if ui
                    .add(widgets::primary_button("Close"))
                    .clicked()
                {
                    close = true;
                }
            });
        });
    if response.should_close() || close {
        state.help = false;
    }
    let _ = actions;
}

fn resource(ctx: &egui::Context, state: &mut UiState, actions: &mut Vec<UiAction>) {
    let Some(resource) = state.resource.as_ref() else {
        return;
    };
    let mut close = false;
    let response = egui::Modal::new(egui::Id::unique("resource-modal"))
        .frame(theme::modal_frame())
        .show(ctx, |ui| {
            ui.set_width(480.0);
            modal_title(
                ui,
                &resource.title,
                "Resource information from the selected connection",
            );
            for (label, value) in &resource.rows {
                widgets::kv(ui, label, value, false);
            }
            ui.add_space(12.0);
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if ui.add(widgets::primary_button("Close")).clicked() {
                    close = true;
                }
            });
        });
    if response.should_close() || close {
        actions.push(UiAction::CloseResource);
    }
}

fn record(ctx: &egui::Context, state: &mut UiState) {
    let Some(record) = state.record_dialog.as_ref() else {
        return;
    };
    let mut close = false;
    let response = egui::Modal::new(egui::Id::unique("record-modal"))
        .frame(theme::modal_frame())
        .show(ctx, |ui| {
            ui.set_width(620.0);
            modal_title(ui, &record.title, &record.subtitle);
            ui.label(RichText::new("Summary").size(11.5).strong());
            ui.add_space(2.0);
            for (label, value) in &record.values {
                widgets::kv(ui, label, value, false);
            }
            ui.add_space(6.0);
            egui::ScrollArea::vertical()
                .max_height(420.0)
                .auto_shrink([false, true])
                .show(ui, |ui| {
                    widgets::info_groups(ui, &record.groups, 1);
                });
            ui.add_space(10.0);
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if ui.add(widgets::primary_button("Close")).clicked() {
                    close = true;
                }
            });
        });
    if response.should_close() || close {
        state.record_dialog = None;
    }
}

fn confirm_remove(
    ctx: &egui::Context,
    state: &mut UiState,
    workspace: &WorkspaceView,
    actions: &mut Vec<UiAction>,
) {
    let Some(id) = state.confirm_remove.clone() else {
        return;
    };
    let name = find_connection(workspace, &id).map_or_else(|| id.clone(), connection_name);
    let mut close = false;
    let mut remove = false;
    let response = egui::Modal::new(egui::Id::unique("remove-modal"))
        .frame(theme::modal_frame())
        .show(ctx, |ui| {
            ui.set_width(430.0);
            modal_title(
                ui,
                "Remove saved connection",
                "This only removes the saved profile. It does not change the host.",
            );
            ui.label(RichText::new(name.clone()).size(12.0).strong());
            ui.label(
                RichText::new("Saved addresses, usernames and preferences will be deleted.")
                    .size(10.0)
                    .color(muted_fg()),
            );
            ui.add_space(14.0);
            ui.horizontal(|ui| {
                if ui
                    .add(egui::Button::new(RichText::new("Cancel").size(11.0)))
                    .clicked()
                {
                    close = true;
                }
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if ui
                        .add(
                            egui::Button::new(
                                RichText::new("Remove")
                                    .size(11.0)
                                    .color(egui::Color32::WHITE),
                            )
                            .fill(red()),
                        )
                        .clicked()
                    {
                        remove = true;
                    }
                });
            });
        });
    if response.should_close() {
        close = true;
    }
    if remove {
        actions.push(UiAction::Remove(id));
        state.confirm_remove = None;
    } else if close {
        actions.push(UiAction::CancelRemove);
    }
}
