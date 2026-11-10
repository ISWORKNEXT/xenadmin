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
    if state.iso_dialog.is_some() {
        iso_dialog(ctx, state, actions);
    }
    if state.disk_dialog.is_some() {
        disk_dialog(ctx, state, actions);
    }
    if state.attach_vdi_dialog.is_some() {
        attach_vdi_dialog(ctx, state, actions);
    }
    if state.ha_edit_dialog.is_some() {
        ha_edit_dialog(ctx, state, actions);
    }
    if state.vdi_delete_confirm.is_some() {
        vdi_delete_confirm(ctx, state, actions);
    }
    if state.move_disk_dialog.is_some() {
        move_disk_dialog(ctx, state, actions);
    }
    if state.sr_create_dialog.is_some() {
        sr_create_dialog(ctx, state, actions);
    }
    if state.sr_introduce_dialog.is_some() {
        sr_introduce_dialog(ctx, state, actions);
    }
    if state.sr_reattach_dialog.is_some() {
        sr_reattach_dialog(ctx, state, actions);
    }
    if state.bond_create_dialog.is_some() {
        bond_create_dialog(ctx, state, actions);
    }
    if state.vlan_create_dialog.is_some() {
        vlan_create_dialog(ctx, state, actions);
    }
    if state.sr_probe_dialog.is_some() {
        sr_probe_dialog(ctx, state, actions);
    }
    if state.user_role_dialog.is_some() {
        user_roles_dialog(ctx, state, actions);
    }
    if state.ad_enable_dialog.is_some() {
        ad_enable_dialog(ctx, state, actions);
    }
    if state.ad_disable_dialog.is_some() {
        ad_disable_dialog(ctx, state, actions);
    }
    if state.host_action_confirm.is_some() {
        host_action_confirm(ctx, state, actions);
    }
    if state.snapshot_confirm.is_some() {
        snapshot_delete(ctx, state, actions);
    }
    if state.sr_action_confirm.is_some() {
        sr_action_confirm(ctx, state, actions);
    }
    if state.network_create_dialog.is_some() {
        network_create(ctx, state, actions);
    }
    if state.save_search.is_some() {
        save_search_dialog(ctx, state, actions);
    }
    if state.network_destroy_confirm.is_some() {
        network_destroy(ctx, state, actions);
    }
    if state.vif_dialog.is_some() {
        vif_dialog(ctx, state, actions);
    }
    if state.vif_confirm.is_some() {
        vif_confirm(ctx, state, actions);
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
                ui.add_space(6.0);
                widgets::field_label(ui, "HTTP proxy (optional)");
                ui.add(
                    egui::TextEdit::singleline(&mut connect.proxy)
                        .hint_text("http://proxy:3128 or socks5://proxy:1080")
                        .desired_width(f32::INFINITY)
                        .margin(egui::Margin::symmetric(8, 5)),
                );
                ui.label(
                    RichText::new("Used for XenAPI and RRD traffic on this connection only.")
                        .size(9.0)
                        .color(dim_fg()),
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
            connect.settings.proxy =
                (!connect.proxy.trim().is_empty()).then(|| connect.proxy.trim().to_owned());
            if let Err(error) = connect.settings.proxy_url() {
                connect.error = Some(error);
            } else if connect.password.is_empty() {
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
                        "Reverting replaces the parent VM's disk state. If the snapshot was taken while running, the VM is started again afterwards.",
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
            ui.add_space(8.0);
            widgets::field_label(ui, "Tags (comma separated)");
            ui.add_enabled(
                !edit.submitting,
                egui::TextEdit::singleline(&mut edit.tags)
                    .desired_width(f32::INFINITY)
                    .hint_text("e.g. production, web, xensearch")
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
                &mut edit.memory_static_max_mib,
                !edit.running && !edit.submitting,
            );
            ui.add_space(8.0);
            ui.columns(2, |columns| {
                resources_field(
                    &mut columns[0],
                    "Dynamic memory minimum (MiB)",
                    &mut edit.memory_dynamic_min_mib,
                    !edit.submitting,
                );
                resources_field(
                    &mut columns[1],
                    "Dynamic memory maximum (MiB)",
                    &mut edit.memory_dynamic_max_mib,
                    !edit.submitting,
                );
            });
            if edit.is_hvm {
                ui.add_space(8.0);
                widgets::field_label(ui, "Boot order");
                let current = edit.boot_order.clone();
                let label = match current.as_str() {
                    "dc" => "Disk first",
                    "cd" => "CD first",
                    "ncd" => "Network first",
                    _ => "Custom",
                };
                ui.add_enabled_ui(!edit.submitting, |ui| {
                    egui::ComboBox::from_id_salt("vm-edit-boot-order")
                        .selected_text(RichText::new(label).size(11.0))
                        .width(ui.available_width())
                        .show_ui(ui, |ui| {
                            for (value, text) in [
                                ("dc", "Disk first"),
                                ("cd", "CD first"),
                                ("ncd", "Network first"),
                            ] {
                                if ui
                                    .selectable_label(
                                        edit.boot_order == value,
                                        RichText::new(text).size(11.0),
                                    )
                                    .clicked()
                                {
                                    edit.boot_order = value.into();
                                }
                            }
                            if !matches!(edit.boot_order.as_str(), "dc" | "cd" | "ncd") {
                                ui.label(
                                    RichText::new(format!("Custom: {}", edit.boot_order))
                                        .size(10.0)
                                        .color(muted_fg()),
                                );
                            }
                        });
                });
                ui.add_space(8.0);
                widgets::field_label(ui, "Firmware");
                let firmware_label = if edit.firmware.eq_ignore_ascii_case("uefi") {
                    "UEFI"
                } else {
                    "BIOS"
                };
                ui.add_enabled_ui(!edit.submitting, |ui| {
                    egui::ComboBox::from_id_salt("vm-edit-firmware")
                        .selected_text(RichText::new(firmware_label).size(11.0))
                        .width(ui.available_width())
                        .show_ui(ui, |ui| {
                            for (value, label) in [("bios", "BIOS"), ("uefi", "UEFI")] {
                                if ui
                                    .selectable_label(
                                        edit.firmware.eq_ignore_ascii_case(value),
                                        RichText::new(label).size(11.0),
                                    )
                                    .clicked()
                                {
                                    edit.firmware = value.into();
                                    if value == "bios" {
                                        edit.secure_boot = false;
                                    }
                                }
                            }
                        });
                });
                ui.add_enabled(
                    !edit.submitting
                        && edit.firmware.eq_ignore_ascii_case("uefi"),
                    egui::Checkbox::new(
                        &mut edit.secure_boot,
                        RichText::new("Secure boot (requires UEFI)").size(10.5),
                    ),
                );
                ui.add_space(8.0);
                widgets::field_label(ui, "Boot policy");
                let policy_label = if edit.boot_policy.is_empty() {
                    "Default (empty)".to_owned()
                } else {
                    edit.boot_policy.clone()
                };
                ui.add_enabled_ui(!edit.submitting, |ui| {
                    egui::ComboBox::from_id_salt("vm-edit-boot-policy")
                        .selected_text(RichText::new(policy_label).size(11.0))
                        .width(ui.available_width())
                        .show_ui(ui, |ui| {
                            for (value, label) in [
                                ("BIOS order", "BIOS order"),
                                ("", "Default (empty)"),
                            ] {
                                if ui
                                    .selectable_label(
                                        edit.boot_policy == value,
                                        RichText::new(label).size(11.0),
                                    )
                                    .clicked()
                                {
                                    edit.boot_policy = value.into();
                                }
                            }
                        });
                });
                ui.add_space(8.0);
                widgets::field_label(ui, "After guest crash");
                let crash_label = match edit.after_crash.as_str() {
                    "destroy" => "Destroy",
                    "preserve" => "Preserve",
                    "rename_restart" => "Rename and restart",
                    _ => "Restart",
                };
                ui.add_enabled_ui(!edit.submitting, |ui| {
                    egui::ComboBox::from_id_salt("vm-edit-after-crash")
                        .selected_text(RichText::new(crash_label).size(11.0))
                        .width(ui.available_width())
                        .show_ui(ui, |ui| {
                            for (value, label) in [
                                ("restart", "Restart"),
                                ("destroy", "Destroy"),
                                ("preserve", "Preserve"),
                                ("rename_restart", "Rename and restart"),
                            ] {
                                if ui
                                    .selectable_label(
                                        edit.after_crash == value,
                                        RichText::new(label).size(11.0),
                                    )
                                    .clicked()
                                {
                                    edit.after_crash = value.into();
                                }
                            }
                        });
                });
            }
            ui.label(
                RichText::new("vCPU and static memory changes require the VM to be shut down. Dynamic memory can change while running when the host allows it. Firmware, secure boot and boot options take effect on the next start.")
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

fn save_search_dialog(ctx: &egui::Context, state: &mut UiState, actions: &mut Vec<UiAction>) {
    let mut close = false;
    let mut save = false;
    let Some(dialog) = state.save_search.as_mut() else {
        return;
    };
    let response = egui::Modal::new(egui::Id::unique("save-search-modal"))
        .frame(theme::modal_frame())
        .show(ctx, |ui| {
            ui.set_width(540.0);
            modal_title(
                ui,
                if dialog.editing.is_some() {
                    "Edit saved search"
                } else {
                    "Save search"
                },
                "Saves the query and grouping for this workspace.",
            );
            widgets::field_label(ui, "Search name");
            ui.add(
                egui::TextEdit::singleline(&mut dialog.name)
                    .desired_width(f32::INFINITY)
                    .char_limit(80)
                    .margin(egui::Margin::symmetric(8, 5)),
            );
            ui.add_space(8.0);
            widgets::field_label(ui, "Query");
            ui.add(
                egui::TextEdit::singleline(&mut dialog.query)
                    .desired_width(f32::INFINITY)
                    .hint_text("e.g. power_state:running tag:prod")
                    .char_limit(512)
                    .margin(egui::Margin::symmetric(8, 5)),
            );
            ui.add_space(8.0);
            widgets::field_label(ui, "Grouping");
            egui::ComboBox::from_id_salt("save-search-group")
                .selected_text(RichText::new(dialog.group_by.label()).size(11.0))
                .width(ui.available_width())
                .show_ui(ui, |ui| {
                    for group in crate::config::GroupBy::ALL {
                        ui.selectable_value(&mut dialog.group_by, group, group.label());
                    }
                });
            if let Some(error) = &dialog.error {
                ui.add_space(8.0);
                ui.label(RichText::new(error).size(10.0).color(red()));
            }
            ui.add_space(12.0);
            ui.horizontal(|ui| {
                if ui
                    .add(egui::Button::new(RichText::new("Cancel").size(11.0)))
                    .clicked()
                {
                    close = true;
                }
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if ui
                        .add_enabled(
                            !dialog.name.trim().is_empty(),
                            widgets::primary_button(if dialog.editing.is_some() {
                                "Update search"
                            } else {
                                "Save search"
                            }),
                        )
                        .clicked()
                    {
                        save = true;
                    }
                });
            });
        });
    if response.should_close() {
        close = true;
    }
    if save {
        actions.push(UiAction::SaveCurrentSearch);
    }
    if close {
        actions.push(UiAction::CloseSaveSearch);
    }
}

fn network_create(ctx: &egui::Context, state: &mut UiState, actions: &mut Vec<UiAction>) {
    let mut close = false;
    let mut save = false;
    let Some(dialog) = state.network_create_dialog.as_mut() else {
        return;
    };
    let response = egui::Modal::new(egui::Id::unique("network-create-modal"))
        .frame(theme::modal_frame())
        .show(ctx, |ui| {
            ui.set_width(520.0);
            modal_title(
                ui,
                "Add network",
                "Creates an internal XenAPI network. Attach a PIF or VIF afterwards.",
            );
            widgets::field_label(ui, "Name");
            ui.add_enabled(
                !dialog.submitting,
                egui::TextEdit::singleline(&mut dialog.name)
                    .desired_width(f32::INFINITY)
                    .char_limit(255)
                    .margin(egui::Margin::symmetric(8, 5)),
            );
            ui.add_space(8.0);
            widgets::field_label(ui, "Description");
            ui.add_enabled(
                !dialog.submitting,
                egui::TextEdit::multiline(&mut dialog.description)
                    .desired_width(f32::INFINITY)
                    .desired_rows(2)
                    .char_limit(1024)
                    .margin(egui::Margin::symmetric(8, 5)),
            );
            ui.add_space(8.0);
            widgets::field_label(ui, "MTU");
            ui.add_enabled(
                !dialog.submitting,
                egui::TextEdit::singleline(&mut dialog.mtu)
                    .desired_width(f32::INFINITY)
                    .char_limit(4)
                    .margin(egui::Margin::symmetric(8, 5)),
            );
            if let Some(error) = &dialog.error {
                ui.add_space(8.0);
                ui.label(RichText::new(error).size(10.0).color(red()));
            }
            ui.add_space(12.0);
            ui.horizontal(|ui| {
                if ui
                    .add_enabled(
                        !dialog.submitting,
                        egui::Button::new(RichText::new("Cancel").size(11.0)),
                    )
                    .clicked()
                {
                    close = true;
                }
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if ui
                        .add_enabled(
                            !dialog.submitting && !dialog.name.trim().is_empty(),
                            widgets::primary_button(if dialog.submitting {
                                "Creating…"
                            } else {
                                "Create network"
                            }),
                        )
                        .clicked()
                    {
                        save = true;
                    }
                });
            });
        });
    if response.should_close() && !dialog.submitting {
        close = true;
    }
    if save {
        actions.push(UiAction::SaveNetwork);
    }
    if close {
        actions.push(UiAction::CloseNetworkCreate);
    }
}

fn network_destroy(ctx: &egui::Context, state: &mut UiState, actions: &mut Vec<UiAction>) {
    let mut close = false;
    let mut destroy = false;
    let Some(dialog) = state.network_destroy_confirm.as_mut() else {
        return;
    };
    let response = egui::Modal::new(egui::Id::unique("network-destroy-modal"))
        .frame(theme::modal_frame())
        .show(ctx, |ui| {
            ui.set_width(480.0);
            modal_title(
                ui,
                "Destroy network",
                "This removes the XenAPI network record.",
            );
            ui.horizontal(|ui| {
                widgets::dot(ui, red(), 6.0);
                ui.vertical(|ui| {
                    ui.spacing_mut().item_spacing.y = 0.0;
                    ui.label(RichText::new(&dialog.name).size(11.5).strong());
                    ui.label(
                        RichText::new(&dialog.network_uuid)
                            .monospace()
                            .size(9.0)
                            .color(dim_fg()),
                    );
                });
            });
            ui.add_space(8.0);
            ui.label(
                RichText::new("The network must have no PIFs or VIFs. This cannot be undone.")
                    .size(9.5)
                    .color(red()),
            );
            ui.add_enabled(
                !dialog.submitting,
                egui::Checkbox::new(
                    &mut dialog.acknowledged,
                    RichText::new("I understand this network operation").size(10.0),
                ),
            );
            if let Some(error) = &dialog.error {
                ui.add_space(8.0);
                ui.label(RichText::new(error).size(10.0).color(red()));
            }
            ui.add_space(12.0);
            ui.horizontal(|ui| {
                if ui
                    .add_enabled(
                        !dialog.submitting,
                        egui::Button::new(RichText::new("Cancel").size(11.0)),
                    )
                    .clicked()
                {
                    close = true;
                }
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if ui
                        .add_enabled(
                            !dialog.submitting && dialog.acknowledged,
                            egui::Button::new(
                                RichText::new(if dialog.submitting {
                                    "Destroying…"
                                } else {
                                    "Destroy network"
                                })
                                .size(11.0)
                                .color(red()),
                            )
                            .stroke(egui::Stroke::new(1.0, with_alpha(red(), 120))),
                        )
                        .clicked()
                    {
                        destroy = true;
                    }
                });
            });
        });
    if response.should_close() && !dialog.submitting {
        close = true;
    }
    if destroy {
        actions.push(UiAction::ConfirmNetworkDestroy);
    }
    if close {
        actions.push(UiAction::CancelNetworkDestroy);
    }
}

fn sr_action_confirm(ctx: &egui::Context, state: &mut UiState, actions: &mut Vec<UiAction>) {
    let mut close = false;
    let mut confirm = false;
    let Some(dialog) = state.sr_action_confirm.as_mut() else {
        return;
    };
    let response = egui::Modal::new(egui::Id::unique("sr-action-modal"))
        .frame(theme::modal_frame())
        .show(ctx, |ui| {
            ui.set_width(500.0);
            modal_title(
                ui,
                dialog.action.title(),
                "Review the storage repository before submitting to XenAPI.",
            );
            ui.horizontal(|ui| {
                widgets::dot(ui, accent(), 6.0);
                ui.vertical(|ui| {
                    ui.spacing_mut().item_spacing.y = 0.0;
                    ui.label(RichText::new(&dialog.sr_name).size(11.5).strong());
                    ui.label(
                        RichText::new(&dialog.sr_uuid)
                            .monospace()
                            .size(9.0)
                            .color(dim_fg()),
                    );
                });
            });
            ui.add_space(8.0);
            if dialog.action.destructive() {
                ui.label(
                    RichText::new(match dialog.action {
                        xen_core::operations::SrAction::Detach => {
                            "Detaching disconnects this storage from the host. Data on the storage remains, but VMs using it must be stopped."
                        }
                        xen_core::operations::SrAction::Forget => {
                            "Forgetting removes only the SR record and its PBDs. Data on the storage is left untouched; use Introduce to add it back."
                        }
                        _ => {
                            "Destroying an SR removes its record. The SR must be empty; data loss is possible."
                        }
                    })
                    .size(9.5)
                    .color(red()),
                );
                ui.add_space(4.0);
                ui.add_enabled(
                    !dialog.submitting,
                    egui::Checkbox::new(
                        &mut dialog.acknowledged,
                        RichText::new("I understand this storage operation").size(10.0),
                    ),
                );
            }
            if let Some(error) = &dialog.error {
                ui.add_space(8.0);
                ui.label(RichText::new(error).size(10.0).color(red()));
            }
            ui.add_space(12.0);
            ui.horizontal(|ui| {
                if ui
                    .add_enabled(
                        !dialog.submitting,
                        egui::Button::new(RichText::new("Cancel").size(11.0)),
                    )
                    .clicked()
                {
                    close = true;
                }
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    let destructive = dialog.action.destructive();
                    let button = if destructive {
                        egui::Button::new(
                            RichText::new(if dialog.submitting {
                                "Running…"
                            } else {
                                dialog.action.title()
                            })
                            .size(11.0)
                            .color(red()),
                        )
                        .stroke(egui::Stroke::new(1.0, with_alpha(red(), 120)))
                    } else {
                        widgets::primary_button(if dialog.submitting {
                            "Running…"
                        } else {
                            dialog.action.title()
                        })
                    };
                    if ui
                        .add_enabled(
                            !dialog.submitting && (!destructive || dialog.acknowledged),
                            button,
                        )
                        .clicked()
                    {
                        confirm = true;
                    }
                });
            });
        });
    if response.should_close() && !dialog.submitting {
        close = true;
    }
    if confirm {
        actions.push(UiAction::ConfirmSrAction);
    }
    if close {
        actions.push(UiAction::CancelSrAction);
    }
}

fn snapshot_delete(ctx: &egui::Context, state: &mut UiState, actions: &mut Vec<UiAction>) {
    let mut close = false;
    let mut delete = false;
    let Some(dialog) = state.snapshot_confirm.as_mut() else {
        return;
    };
    let response = egui::Modal::new(egui::Id::unique("snapshot-delete-modal"))
        .frame(theme::modal_frame())
        .show(ctx, |ui| {
            ui.set_width(500.0);
            modal_title(
                ui,
                "Delete snapshots",
                &format!(
                    "{} snapshot{} from {}",
                    dialog.snapshots.len(),
                    if dialog.snapshots.len() == 1 { "" } else { "s" },
                    dialog.vm_name
                ),
            );
            egui::ScrollArea::vertical()
                .max_height(220.0)
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    for (_, name) in &dialog.snapshots {
                        ui.horizontal(|ui| {
                            widgets::dot(ui, purple(), 6.0);
                            ui.label(RichText::new(name).size(11.0));
                        });
                    }
                });
            ui.add_space(8.0);
            ui.label(
                RichText::new(
                    "Snapshot disks owned by these snapshots are deleted. This cannot be undone.",
                )
                .size(9.5)
                .color(red()),
            );
            if let Some(error) = &dialog.error {
                ui.add_space(8.0);
                ui.label(RichText::new(error).size(10.0).color(red()));
            }
            ui.add_space(12.0);
            ui.horizontal(|ui| {
                if ui
                    .add_enabled(
                        !dialog.submitting,
                        egui::Button::new(RichText::new("Cancel").size(11.0)),
                    )
                    .clicked()
                {
                    close = true;
                }
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if ui
                        .add_enabled(
                            !dialog.submitting,
                            egui::Button::new(
                                RichText::new(if dialog.submitting {
                                    "Deleting…"
                                } else {
                                    "Delete snapshots"
                                })
                                .size(11.0)
                                .color(red()),
                            )
                            .stroke(egui::Stroke::new(1.0, with_alpha(red(), 120))),
                        )
                        .clicked()
                    {
                        delete = true;
                    }
                });
            });
        });
    if response.should_close() && !dialog.submitting {
        close = true;
    }
    if delete {
        actions.push(UiAction::ConfirmSnapshotDelete);
    }
    if close {
        actions.push(UiAction::CloseSnapshotDelete);
    }
}

fn host_action_confirm(ctx: &egui::Context, state: &mut UiState, actions: &mut Vec<UiAction>) {
    let mut close = false;
    let mut confirm = false;
    let Some(dialog) = state.host_action_confirm.as_mut() else {
        return;
    };
    let response = egui::Modal::new(egui::Id::unique("host-action-modal"))
        .frame(theme::modal_frame())
        .show(ctx, |ui| {
            ui.set_width(500.0);
            modal_title(
                ui,
                dialog.action.title(),
                "Review the host target before submitting to XenAPI.",
            );
            ui.horizontal(|ui| {
                widgets::dot(ui, accent(), 6.0);
                ui.vertical(|ui| {
                    ui.spacing_mut().item_spacing.y = 0.0;
                    ui.label(RichText::new(&dialog.host_name).size(11.5).strong());
                    ui.label(
                        RichText::new(&dialog.host_uuid)
                            .monospace()
                            .size(9.0)
                            .color(dim_fg()),
                    );
                });
            });
            if dialog.action.destructive() {
                ui.add_space(8.0);
                ui.label(
                    RichText::new(match dialog.action {
                        xen_core::operations::HostAction::Reboot => {
                            "All VMs on this host will be rebooted or shut down."
                        }
                        xen_core::operations::HostAction::Shutdown => {
                            "The host and all VMs on it will be shut down."
                        }
                        _ => "This action changes host availability.",
                    })
                    .size(9.5)
                    .color(red()),
                );
                ui.add_space(4.0);
                ui.add_enabled(
                    !dialog.submitting,
                    egui::Checkbox::new(
                        &mut dialog.acknowledged,
                        RichText::new("I understand the impact on running VMs").size(10.0),
                    ),
                );
            }
            if let Some(error) = &dialog.error {
                ui.add_space(8.0);
                ui.label(RichText::new(error).size(10.0).color(red()));
            }
            ui.add_space(12.0);
            ui.horizontal(|ui| {
                if ui
                    .add_enabled(
                        !dialog.submitting,
                        egui::Button::new(RichText::new("Cancel").size(11.0)),
                    )
                    .clicked()
                {
                    close = true;
                }
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    let destructive = dialog.action.destructive();
                    let button = if destructive {
                        egui::Button::new(
                            RichText::new(if dialog.submitting {
                                "Running…"
                            } else {
                                dialog.action.title()
                            })
                            .size(11.0)
                            .color(red()),
                        )
                        .stroke(egui::Stroke::new(1.0, with_alpha(red(), 120)))
                    } else {
                        widgets::primary_button(if dialog.submitting {
                            "Running…"
                        } else {
                            dialog.action.title()
                        })
                    };
                    if ui
                        .add_enabled(
                            !dialog.submitting && (!destructive || dialog.acknowledged),
                            button,
                        )
                        .clicked()
                    {
                        confirm = true;
                    }
                });
            });
        });
    if response.should_close() && !dialog.submitting {
        close = true;
    }
    if confirm {
        actions.push(UiAction::ConfirmHostAction);
    }
    if close {
        actions.push(UiAction::CancelHostAction);
    }
}

fn ad_enable_dialog(ctx: &egui::Context, state: &mut UiState, actions: &mut Vec<UiAction>) {
    let mut close = false;
    let mut save = false;
    let Some(dialog) = state.ad_enable_dialog.as_mut() else {
        return;
    };
    let response = egui::Modal::new(egui::Id::unique("ad-enable-modal"))
        .frame(theme::modal_frame())
        .show(ctx, |ui| {
            ui.set_width(560.0);
            modal_title(
                ui,
                "Enable Active Directory authentication",
                "Pool users will authenticate against the configured domain.",
            );
            widgets::field_label(ui, "Domain");
            ui.add_enabled(
                !dialog.submitting,
                egui::TextEdit::singleline(&mut dialog.domain)
                    .desired_width(f32::INFINITY)
                    .hint_text("example.com")
                    .char_limit(255)
                    .margin(egui::Margin::symmetric(8, 5)),
            );
            ui.add_space(8.0);
            widgets::field_label(ui, "Service account user");
            ui.add_enabled(
                !dialog.submitting,
                egui::TextEdit::singleline(&mut dialog.user)
                    .desired_width(f32::INFINITY)
                    .hint_text("svc-xen or DOMAIN\\svc-xen")
                    .char_limit(255)
                    .margin(egui::Margin::symmetric(8, 5)),
            );
            ui.add_space(8.0);
            widgets::field_label(ui, "Service account password");
            ui.add_enabled(
                !dialog.submitting,
                egui::TextEdit::singleline(&mut dialog.password)
                    .desired_width(f32::INFINITY)
                    .password(true)
                    .char_limit(1024)
                    .margin(egui::Margin::symmetric(8, 5)),
            );
            ui.add_space(8.0);
            ui.label(
                RichText::new(
                    "The password is sent only to the pool enable call and is never saved in this app.",
                )
                .size(9.0)
                .color(dim_fg()),
            );
            ui.add_enabled(
                !dialog.submitting,
                egui::Checkbox::new(
                    &mut dialog.acknowledged,
                    RichText::new("I understand users will authenticate through AD").size(10.0),
                ),
            );
            if let Some(error) = &dialog.error {
                ui.add_space(8.0);
                ui.label(RichText::new(error).size(10.0).color(red()));
            }
            ui.add_space(12.0);
            ui.horizontal(|ui| {
                if ui
                    .add_enabled(
                        !dialog.submitting,
                        egui::Button::new(RichText::new("Cancel").size(11.0)),
                    )
                    .clicked()
                {
                    close = true;
                }
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if ui
                        .add_enabled(
                            !dialog.submitting
                                && dialog.acknowledged
                                && !dialog.domain.trim().is_empty()
                                && !dialog.user.trim().is_empty()
                                && !dialog.password.is_empty(),
                            widgets::primary_button(if dialog.submitting {
                                "Enabling…"
                            } else {
                                "Enable AD"
                            }),
                        )
                        .clicked()
                    {
                        save = true;
                    }
                });
            });
        });
    if response.should_close() && !dialog.submitting {
        close = true;
    }
    if save {
        actions.push(UiAction::SaveAdEnable);
    }
    if close {
        actions.push(UiAction::CloseAdEnable);
    }
}

fn ad_disable_dialog(ctx: &egui::Context, state: &mut UiState, actions: &mut Vec<UiAction>) {
    let mut close = false;
    let mut save = false;
    let Some(dialog) = state.ad_disable_dialog.as_mut() else {
        return;
    };
    let response = egui::Modal::new(egui::Id::unique("ad-disable-modal"))
        .frame(theme::modal_frame())
        .show(ctx, |ui| {
            ui.set_width(560.0);
            modal_title(
                ui,
                "Disable Active Directory authentication",
                "The pool returns to local authentication. Local root remains available.",
            );
            ui.add_enabled(
                !dialog.submitting,
                egui::Checkbox::new(
                    &mut dialog.disable_machine_account,
                    RichText::new("Also disable the machine account in AD").size(10.5),
                ),
            );
            ui.add_space(8.0);
            widgets::field_label(ui, "Service account user");
            ui.add_enabled(
                !dialog.submitting && dialog.disable_machine_account,
                egui::TextEdit::singleline(&mut dialog.user)
                    .desired_width(f32::INFINITY)
                    .char_limit(255)
                    .margin(egui::Margin::symmetric(8, 5)),
            );
            ui.add_space(8.0);
            widgets::field_label(ui, "Service account password");
            ui.add_enabled(
                !dialog.submitting && dialog.disable_machine_account,
                egui::TextEdit::singleline(&mut dialog.password)
                    .desired_width(f32::INFINITY)
                    .password(true)
                    .char_limit(1024)
                    .margin(egui::Margin::symmetric(8, 5)),
            );
            ui.add_enabled(
                !dialog.submitting,
                egui::Checkbox::new(
                    &mut dialog.acknowledged,
                    RichText::new("I understand authentication will switch to local").size(10.0),
                ),
            );
            if let Some(error) = &dialog.error {
                ui.add_space(8.0);
                ui.label(RichText::new(error).size(10.0).color(red()));
            }
            ui.add_space(12.0);
            ui.horizontal(|ui| {
                if ui
                    .add_enabled(
                        !dialog.submitting,
                        egui::Button::new(RichText::new("Cancel").size(11.0)),
                    )
                    .clicked()
                {
                    close = true;
                }
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if ui
                        .add_enabled(
                            !dialog.submitting && dialog.acknowledged,
                            widgets::primary_button(if dialog.submitting {
                                "Disabling…"
                            } else {
                                "Disable AD"
                            }),
                        )
                        .clicked()
                    {
                        save = true;
                    }
                });
            });
        });
    if response.should_close() && !dialog.submitting {
        close = true;
    }
    if save {
        actions.push(UiAction::SaveAdDisable);
    }
    if close {
        actions.push(UiAction::CloseAdDisable);
    }
}

fn user_roles_dialog(ctx: &egui::Context, state: &mut UiState, actions: &mut Vec<UiAction>) {
    let roles = match &state.auth_state {
        crate::state::LoadState::Ready(info) => info.roles.clone(),
        _ => Vec::new(),
    };
    let mut close = false;
    let mut save = false;
    let Some(dialog) = state.user_role_dialog.as_mut() else {
        return;
    };
    let response = egui::Modal::new(egui::Id::unique("user-roles-modal"))
        .frame(theme::modal_frame())
        .show(ctx, |ui| {
            ui.set_width(540.0);
            modal_title(
                ui,
                "Assign subject roles",
                "Changes apply immediately to the selected directory subject.",
            );
            ui.label(
                RichText::new(format!("{} · {}", dialog.identifier, dialog.display_name))
                    .size(11.0)
                    .strong(),
            );
            ui.add_space(8.0);
            egui::ScrollArea::vertical()
                .max_height(260.0)
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    for role in &roles {
                        let mut checked = dialog.selected.contains(&role.uuid);
                        if ui
                            .add_enabled(
                                !dialog.submitting,
                                egui::Checkbox::new(
                                    &mut checked,
                                    RichText::new(&role.name).size(10.5),
                                ),
                            )
                            .on_hover_text(&role.description)
                            .changed()
                        {
                            if checked {
                                dialog.selected.insert(role.uuid.clone());
                            } else {
                                dialog.selected.remove(&role.uuid);
                            }
                        }
                    }
                });
            ui.label(
                RichText::new("At least one role is required. Local root cannot be changed here.")
                    .size(9.0)
                    .color(dim_fg()),
            );
            if let Some(error) = &dialog.error {
                ui.add_space(8.0);
                ui.label(RichText::new(error).size(10.0).color(red()));
            }
            ui.add_space(12.0);
            ui.horizontal(|ui| {
                if ui
                    .add_enabled(
                        !dialog.submitting,
                        egui::Button::new(RichText::new("Cancel").size(11.0)),
                    )
                    .clicked()
                {
                    close = true;
                }
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if ui
                        .add_enabled(
                            !dialog.submitting && !dialog.selected.is_empty(),
                            widgets::primary_button(if dialog.submitting {
                                "Saving…"
                            } else {
                                "Save roles"
                            }),
                        )
                        .clicked()
                    {
                        save = true;
                    }
                });
            });
        });
    if response.should_close() && !dialog.submitting {
        close = true;
    }
    if save {
        actions.push(UiAction::SaveUserRoles);
    }
    if close {
        actions.push(UiAction::CloseUserRoles);
    }
}

fn sr_introduce_dialog(ctx: &egui::Context, state: &mut UiState, actions: &mut Vec<UiAction>) {
    let mut close = false;
    let mut save = false;
    let Some(dialog) = state.sr_introduce_dialog.as_mut() else {
        return;
    };
    let response = egui::Modal::new(egui::Id::unique("sr-introduce-modal"))
        .frame(theme::modal_frame())
        .show(ctx, |ui| {
            ui.set_width(600.0);
            modal_title(
                ui,
                "Introduce storage repository",
                "Adds an existing SR record without modifying its data.",
            );
            widgets::field_label(ui, "SR UUID");
            ui.add_enabled(
                !dialog.submitting,
                egui::TextEdit::singleline(&mut dialog.uuid)
                    .desired_width(f32::INFINITY)
                    .hint_text("12345678-1234-1234-1234-123456789abc")
                    .char_limit(36)
                    .margin(egui::Margin::symmetric(8, 5)),
            );
            ui.add_space(8.0);
            widgets::field_label(ui, "Name");
            ui.add_enabled(
                !dialog.submitting,
                egui::TextEdit::singleline(&mut dialog.name)
                    .desired_width(f32::INFINITY)
                    .char_limit(255)
                    .margin(egui::Margin::symmetric(8, 5)),
            );
            ui.add_space(8.0);
            widgets::field_label(ui, "Description");
            ui.add_enabled(
                !dialog.submitting,
                egui::TextEdit::multiline(&mut dialog.description)
                    .desired_width(f32::INFINITY)
                    .desired_rows(2)
                    .char_limit(1024)
                    .margin(egui::Margin::symmetric(8, 5)),
            );
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                ui.label(RichText::new("Type").size(9.5).color(muted_fg()));
                widgets::badge(ui, "file", accent());
                ui.label(RichText::new("Content").size(9.5).color(muted_fg()));
                widgets::badge(ui, "user", muted_fg());
                ui.label(RichText::new("Local only").size(9.5).color(muted_fg()));
            });
            ui.label(
                RichText::new(
                    "Use Reattach storage repository on the new SR page to attach a host path.",
                )
                .size(9.0)
                .color(dim_fg()),
            );
            if let Some(error) = &dialog.error {
                ui.add_space(8.0);
                ui.label(RichText::new(error).size(10.0).color(red()));
            }
            ui.add_space(12.0);
            ui.horizontal(|ui| {
                if ui
                    .add_enabled(
                        !dialog.submitting,
                        egui::Button::new(RichText::new("Cancel").size(11.0)),
                    )
                    .clicked()
                {
                    close = true;
                }
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if ui
                        .add_enabled(
                            !dialog.submitting
                                && !dialog.uuid.trim().is_empty()
                                && !dialog.name.trim().is_empty(),
                            widgets::primary_button(if dialog.submitting {
                                "Introducing…"
                            } else {
                                "Introduce storage"
                            }),
                        )
                        .clicked()
                    {
                        save = true;
                    }
                });
            });
        });
    if response.should_close() && !dialog.submitting {
        close = true;
    }
    if save {
        actions.push(UiAction::SaveSrIntroduce);
    }
    if close {
        actions.push(UiAction::CloseSrIntroduce);
    }
}

fn sr_probe_dialog(ctx: &egui::Context, state: &mut UiState, actions: &mut Vec<UiAction>) {
    let mut close = false;
    let mut probe = false;
    let Some(dialog) = state.sr_probe_dialog.as_mut() else {
        return;
    };
    let response = egui::Modal::new(egui::Id::unique("sr-probe-modal"))
        .frame(theme::modal_frame())
        .show(ctx, |ui| {
            ui.set_width(620.0);
            modal_title(
                ui,
                "Probe storage path",
                "Asks the host storage backend what SR records exist at a path.",
            );
            ui.columns(2, |columns| {
                widgets::field_label(&mut columns[0], "Host");
                let host_label = dialog
                    .host_uuid
                    .as_ref()
                    .and_then(|uuid| dialog.hosts.iter().find(|(id, _)| id == uuid))
                    .map(|(_, name)| name.as_str())
                    .unwrap_or("Select a host");
                egui::ComboBox::from_id_salt("sr-probe-host")
                    .selected_text(RichText::new(host_label).size(11.0))
                    .width(columns[0].available_width())
                    .show_ui(&mut columns[0], |ui| {
                        for (uuid, name) in &dialog.hosts {
                            if ui
                                .selectable_label(
                                    dialog.host_uuid.as_deref() == Some(uuid.as_str()),
                                    RichText::new(name).size(11.0),
                                )
                                .clicked()
                            {
                                dialog.host_uuid = Some(uuid.clone());
                            }
                        }
                    });
                widgets::field_label(&mut columns[1], "Location on host");
                columns[1].add_enabled(
                    !dialog.submitting,
                    egui::TextEdit::singleline(&mut dialog.location)
                        .hint_text("/var/tmp/xen-storage")
                        .char_limit(255)
                        .margin(egui::Margin::symmetric(8, 5)),
                );
            });
            if let Some(error) = &dialog.error {
                ui.add_space(8.0);
                ui.label(RichText::new(error).size(10.0).color(red()));
            }
            if let Some(result) = &dialog.result {
                ui.add_space(8.0);
                if result.found.is_empty() {
                    ui.label(
                        RichText::new("No SR records were found at this path.")
                            .size(10.5)
                            .color(muted_fg()),
                    );
                    if !result.raw.trim().is_empty() {
                        ui.label(
                            RichText::new(result.raw.chars().take(300).collect::<String>())
                                .monospace()
                                .size(8.5)
                                .color(dim_fg()),
                        );
                    }
                } else {
                    for probed in &result.found {
                        ui.horizontal(|ui| {
                            widgets::dot(ui, accent(), 6.0);
                            ui.vertical(|ui| {
                                ui.label(
                                    RichText::new(if probed.name.is_empty() {
                                        "(unnamed SR)"
                                    } else {
                                        &probed.name
                                    })
                                    .size(11.0)
                                    .strong(),
                                );
                                ui.label(
                                    RichText::new(format!(
                                        "{} · {} · {} · {}",
                                        probed.uuid,
                                        probed.sr_type,
                                        probed.content_type,
                                        crate::format::bytes(probed.physical_size)
                                    ))
                                    .monospace()
                                    .size(8.5)
                                    .color(dim_fg()),
                                );
                            });
                        });
                    }
                }
            }
            ui.add_space(12.0);
            ui.horizontal(|ui| {
                if ui
                    .add_enabled(
                        !dialog.submitting,
                        egui::Button::new(RichText::new("Close").size(11.0)),
                    )
                    .clicked()
                {
                    close = true;
                }
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if ui
                        .add_enabled(
                            !dialog.submitting
                                && !dialog.location.trim().is_empty()
                                && dialog.host_uuid.is_some(),
                            widgets::primary_button(if dialog.submitting {
                                "Probing…"
                            } else {
                                "Probe"
                            }),
                        )
                        .clicked()
                    {
                        probe = true;
                    }
                });
            });
        });
    if response.should_close() && !dialog.submitting {
        close = true;
    }
    if probe {
        actions.push(UiAction::SaveSrProbe);
    }
    if close {
        actions.push(UiAction::CloseSrProbe);
    }
}

fn bond_create_dialog(ctx: &egui::Context, state: &mut UiState, actions: &mut Vec<UiAction>) {
    let candidates: Vec<(String, String, String, String)> = state
        .pif_catalog
        .iter()
        .filter(|pif| pif.bond_candidate())
        .map(|pif| {
            (
                pif.uuid.clone(),
                pif.host_name.clone(),
                pif.device.clone(),
                pif.mac.clone(),
            )
        })
        .collect();
    let mut close = false;
    let mut save = false;
    let Some(dialog) = state.bond_create_dialog.as_mut() else {
        return;
    };
    let response = egui::Modal::new(egui::Id::unique("bond-create-modal"))
        .frame(theme::modal_frame())
        .show(ctx, |ui| {
            ui.set_width(600.0);
            modal_title(
                ui,
                "Create bond",
                "Combines two or more physical interfaces into one network.",
            );
            widgets::field_label(ui, "Network name");
            ui.add_enabled(
                !dialog.submitting,
                egui::TextEdit::singleline(&mut dialog.name)
                    .desired_width(f32::INFINITY)
                    .char_limit(255)
                    .margin(egui::Margin::symmetric(8, 5)),
            );
            ui.add_space(8.0);
            ui.columns(2, |columns| {
                columns[0].label(RichText::new("Mode").size(9.5).color(muted_fg()));
                let mode_label = match dialog.mode.as_str() {
                    "active-backup" => "Active-backup",
                    "lacp" => "LACP",
                    _ => "Balance SLB",
                };
                egui::ComboBox::from_id_salt("bond-mode")
                    .selected_text(RichText::new(mode_label).size(11.0))
                    .width(columns[0].available_width())
                    .show_ui(&mut columns[0], |ui| {
                        for (value, label) in [
                            ("balance-slb", "Balance SLB"),
                            ("active-backup", "Active-backup"),
                            ("lacp", "LACP"),
                        ] {
                            if ui
                                .selectable_label(
                                    dialog.mode == value,
                                    RichText::new(label).size(11.0),
                                )
                                .clicked()
                            {
                                dialog.mode = value.into();
                            }
                        }
                    });
                widgets::field_label(&mut columns[1], "MTU");
                columns[1].add_enabled(
                    !dialog.submitting,
                    egui::TextEdit::singleline(&mut dialog.mtu)
                        .char_limit(4)
                        .margin(egui::Margin::symmetric(8, 5)),
                );
            });
            ui.add_space(6.0);
            ui.add_enabled(
                !dialog.submitting,
                egui::Checkbox::new(
                    &mut dialog.autoplug,
                    RichText::new("Automatically plug the new bond network").size(10.5),
                ),
            );
            ui.add_space(8.0);
            widgets::field_label(ui, "Member interfaces");
            if candidates.is_empty() {
                ui.label(
                    RichText::new("No spare physical interfaces are available.")
                        .size(10.0)
                        .color(amber()),
                );
            }
            egui::ScrollArea::vertical()
                .max_height(180.0)
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    for (uuid, host, device, mac) in &candidates {
                        let mut checked = dialog.members.contains(uuid);
                        if ui
                            .add_enabled(
                                !dialog.submitting,
                                egui::Checkbox::new(
                                    &mut checked,
                                    RichText::new(format!("{host} · {device} · {mac}")).size(10.5),
                                ),
                            )
                            .changed()
                        {
                            if checked {
                                dialog.members.insert(uuid.clone());
                            } else {
                                dialog.members.remove(uuid);
                            }
                        }
                    }
                });
            ui.label(
                RichText::new("Management interfaces are excluded from bonding.")
                    .size(9.0)
                    .color(dim_fg()),
            );
            if let Some(error) = &dialog.error {
                ui.add_space(8.0);
                ui.label(RichText::new(error).size(10.0).color(red()));
            }
            ui.add_space(12.0);
            ui.horizontal(|ui| {
                if ui
                    .add_enabled(
                        !dialog.submitting,
                        egui::Button::new(RichText::new("Cancel").size(11.0)),
                    )
                    .clicked()
                {
                    close = true;
                }
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if ui
                        .add_enabled(
                            !dialog.submitting
                                && dialog.members.len() >= 2
                                && !dialog.name.trim().is_empty(),
                            widgets::primary_button(if dialog.submitting {
                                "Creating…"
                            } else {
                                "Create bond"
                            }),
                        )
                        .clicked()
                    {
                        save = true;
                    }
                });
            });
        });
    if response.should_close() && !dialog.submitting {
        close = true;
    }
    if save {
        actions.push(UiAction::SaveBondCreate);
    }
    if close {
        actions.push(UiAction::CloseBondCreate);
    }
}

fn vlan_create_dialog(ctx: &egui::Context, state: &mut UiState, actions: &mut Vec<UiAction>) {
    let candidates: Vec<(String, String, String)> = state
        .pif_catalog
        .iter()
        .filter(|pif| pif.vlan_candidate())
        .map(|pif| (pif.uuid.clone(), pif.host_name.clone(), pif.device.clone()))
        .collect();
    let mut close = false;
    let mut save = false;
    let Some(dialog) = state.vlan_create_dialog.as_mut() else {
        return;
    };
    let response = egui::Modal::new(egui::Id::unique("vlan-create-modal"))
        .frame(theme::modal_frame())
        .show(ctx, |ui| {
            ui.set_width(600.0);
            modal_title(
                ui,
                "Create VLAN",
                "Creates a tagged network on a physical interface.",
            );
            widgets::field_label(ui, "Parent interface");
            let parent_label = dialog
                .pif_uuid
                .as_ref()
                .and_then(|uuid| candidates.iter().find(|(id, _, _)| id == uuid))
                .map(|(_, host, device)| format!("{host} · {device}"))
                .unwrap_or_else(|| "Select an interface".into());
            egui::ComboBox::from_id_salt("vlan-parent")
                .selected_text(RichText::new(parent_label).size(11.0))
                .width(ui.available_width())
                .show_ui(ui, |ui| {
                    for (uuid, host, device) in &candidates {
                        if ui
                            .selectable_label(
                                dialog.pif_uuid.as_deref() == Some(uuid.as_str()),
                                RichText::new(format!("{host} · {device}")).size(11.0),
                            )
                            .clicked()
                        {
                            dialog.pif_uuid = Some(uuid.clone());
                        }
                    }
                });
            if candidates.is_empty() {
                ui.label(
                    RichText::new("No spare physical interface is available for a VLAN.")
                        .size(10.0)
                        .color(amber()),
                );
            }
            ui.add_space(8.0);
            ui.columns(2, |columns| {
                widgets::field_label(&mut columns[0], "VLAN tag");
                columns[0].add_enabled(
                    !dialog.submitting,
                    egui::TextEdit::singleline(&mut dialog.tag)
                        .char_limit(4)
                        .margin(egui::Margin::symmetric(8, 5)),
                );
                widgets::field_label(&mut columns[1], "MTU");
                columns[1].add_enabled(
                    !dialog.submitting,
                    egui::TextEdit::singleline(&mut dialog.mtu)
                        .char_limit(4)
                        .margin(egui::Margin::symmetric(8, 5)),
                );
            });
            ui.add_space(8.0);
            widgets::field_label(ui, "Network name");
            ui.add_enabled(
                !dialog.submitting,
                egui::TextEdit::singleline(&mut dialog.name)
                    .desired_width(f32::INFINITY)
                    .char_limit(255)
                    .margin(egui::Margin::symmetric(8, 5)),
            );
            ui.add_space(8.0);
            widgets::field_label(ui, "Description");
            ui.add_enabled(
                !dialog.submitting,
                egui::TextEdit::multiline(&mut dialog.description)
                    .desired_width(f32::INFINITY)
                    .desired_rows(2)
                    .char_limit(1024)
                    .margin(egui::Margin::symmetric(8, 5)),
            );
            ui.add_space(6.0);
            ui.add_enabled(
                !dialog.submitting,
                egui::Checkbox::new(
                    &mut dialog.autoplug,
                    RichText::new("Automatically plug the new VLAN network").size(10.5),
                ),
            );
            if let Some(error) = &dialog.error {
                ui.add_space(8.0);
                ui.label(RichText::new(error).size(10.0).color(red()));
            }
            ui.add_space(12.0);
            ui.horizontal(|ui| {
                if ui
                    .add_enabled(
                        !dialog.submitting,
                        egui::Button::new(RichText::new("Cancel").size(11.0)),
                    )
                    .clicked()
                {
                    close = true;
                }
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if ui
                        .add_enabled(
                            !dialog.submitting
                                && dialog.pif_uuid.is_some()
                                && !dialog.name.trim().is_empty()
                                && !dialog.tag.trim().is_empty(),
                            widgets::primary_button(if dialog.submitting {
                                "Creating…"
                            } else {
                                "Create VLAN"
                            }),
                        )
                        .clicked()
                    {
                        save = true;
                    }
                });
            });
        });
    if response.should_close() && !dialog.submitting {
        close = true;
    }
    if save {
        actions.push(UiAction::SaveVlanCreate);
    }
    if close {
        actions.push(UiAction::CloseVlanCreate);
    }
}

fn sr_create_dialog(ctx: &egui::Context, state: &mut UiState, actions: &mut Vec<UiAction>) {
    let mut close = false;
    let mut save = false;
    let Some(dialog) = state.sr_create_dialog.as_mut() else {
        return;
    };
    let response = egui::Modal::new(egui::Id::unique("sr-create-modal"))
        .frame(theme::modal_frame())
        .show(ctx, |ui| {
            ui.set_width(600.0);
            modal_title(
                ui,
                "Create storage repository",
                "Creates a host-local, file-backed SR at an existing absolute path.",
            );
            widgets::field_label(ui, "Name");
            ui.add_enabled(
                !dialog.submitting,
                egui::TextEdit::singleline(&mut dialog.name)
                    .desired_width(f32::INFINITY)
                    .char_limit(255)
                    .margin(egui::Margin::symmetric(8, 5)),
            );
            ui.add_space(8.0);
            widgets::field_label(ui, "Description");
            ui.add_enabled(
                !dialog.submitting,
                egui::TextEdit::multiline(&mut dialog.description)
                    .desired_width(f32::INFINITY)
                    .desired_rows(2)
                    .char_limit(1024)
                    .margin(egui::Margin::symmetric(8, 5)),
            );
            ui.add_space(8.0);
            widgets::field_label(ui, "Host");
            let host_label = dialog
                .host_uuid
                .as_ref()
                .and_then(|uuid| dialog.hosts.iter().find(|(id, _)| id == uuid))
                .map(|(_, name)| name.as_str())
                .unwrap_or("Select a host");
            ui.add_enabled_ui(!dialog.submitting, |ui| {
                egui::ComboBox::from_id_salt("sr-create-host")
                    .selected_text(RichText::new(host_label).size(11.0))
                    .width(ui.available_width())
                    .show_ui(ui, |ui| {
                        for (uuid, name) in &dialog.hosts {
                            if ui
                                .selectable_label(
                                    dialog.host_uuid.as_deref() == Some(uuid.as_str()),
                                    RichText::new(name).size(11.0),
                                )
                                .clicked()
                            {
                                dialog.host_uuid = Some(uuid.clone());
                            }
                        }
                    });
            });
            ui.add_space(8.0);
            widgets::field_label(ui, "Location on host");
            ui.add_enabled(
                !dialog.submitting,
                egui::TextEdit::singleline(&mut dialog.location)
                    .desired_width(f32::INFINITY)
                    .hint_text("/var/tmp/xen-storage")
                    .char_limit(255)
                    .margin(egui::Margin::symmetric(8, 5)),
            );
            ui.label(
                RichText::new("The path must be absolute and should be dedicated to this SR.")
                    .size(9.0)
                    .color(dim_fg()),
            );
            if let Some(error) = &dialog.error {
                ui.add_space(8.0);
                ui.label(RichText::new(error).size(10.0).color(red()));
            }
            ui.add_space(12.0);
            ui.horizontal(|ui| {
                if ui
                    .add_enabled(
                        !dialog.submitting,
                        egui::Button::new(RichText::new("Cancel").size(11.0)),
                    )
                    .clicked()
                {
                    close = true;
                }
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if ui
                        .add_enabled(
                            !dialog.submitting
                                && !dialog.name.trim().is_empty()
                                && !dialog.location.trim().is_empty(),
                            widgets::primary_button(if dialog.submitting {
                                "Creating…"
                            } else {
                                "Create storage"
                            }),
                        )
                        .clicked()
                    {
                        save = true;
                    }
                });
            });
        });
    if response.should_close() && !dialog.submitting {
        close = true;
    }
    if save {
        actions.push(UiAction::SaveSrCreate);
    }
    if close {
        actions.push(UiAction::CloseSrCreate);
    }
}

fn sr_reattach_dialog(ctx: &egui::Context, state: &mut UiState, actions: &mut Vec<UiAction>) {
    let mut close = false;
    let mut save = false;
    let Some(dialog) = state.sr_reattach_dialog.as_mut() else {
        return;
    };
    let response = egui::Modal::new(egui::Id::unique("sr-reattach-modal"))
        .frame(theme::modal_frame())
        .show(ctx, |ui| {
            ui.set_width(580.0);
            modal_title(
                ui,
                "Reattach storage repository",
                "Creates and plugs a PBD using the host-local path.",
            );
            ui.label(RichText::new(&dialog.sr_name).size(11.5).strong());
            ui.label(
                RichText::new(&dialog.sr_uuid)
                    .monospace()
                    .size(9.0)
                    .color(dim_fg()),
            );
            ui.add_space(8.0);
            widgets::field_label(ui, "Host");
            let host_label = dialog
                .host_uuid
                .as_ref()
                .and_then(|uuid| dialog.hosts.iter().find(|(id, _)| id == uuid))
                .map(|(_, name)| name.as_str())
                .unwrap_or("Select a host");
            ui.add_enabled_ui(!dialog.submitting, |ui| {
                egui::ComboBox::from_id_salt("sr-reattach-host")
                    .selected_text(RichText::new(host_label).size(11.0))
                    .width(ui.available_width())
                    .show_ui(ui, |ui| {
                        for (uuid, name) in &dialog.hosts {
                            if ui
                                .selectable_label(
                                    dialog.host_uuid.as_deref() == Some(uuid.as_str()),
                                    RichText::new(name).size(11.0),
                                )
                                .clicked()
                            {
                                dialog.host_uuid = Some(uuid.clone());
                            }
                        }
                    });
            });
            ui.add_space(8.0);
            widgets::field_label(ui, "Location on host");
            ui.add_enabled(
                !dialog.submitting,
                egui::TextEdit::singleline(&mut dialog.location)
                    .desired_width(f32::INFINITY)
                    .hint_text("/var/tmp/xen-storage")
                    .char_limit(255)
                    .margin(egui::Margin::symmetric(8, 5)),
            );
            if let Some(error) = &dialog.error {
                ui.add_space(8.0);
                ui.label(RichText::new(error).size(10.0).color(red()));
            }
            ui.add_space(12.0);
            ui.horizontal(|ui| {
                if ui
                    .add_enabled(
                        !dialog.submitting,
                        egui::Button::new(RichText::new("Cancel").size(11.0)),
                    )
                    .clicked()
                {
                    close = true;
                }
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if ui
                        .add_enabled(
                            !dialog.submitting && !dialog.location.trim().is_empty(),
                            widgets::primary_button(if dialog.submitting {
                                "Attaching…"
                            } else {
                                "Reattach storage"
                            }),
                        )
                        .clicked()
                    {
                        save = true;
                    }
                });
            });
        });
    if response.should_close() && !dialog.submitting {
        close = true;
    }
    if save {
        actions.push(UiAction::SaveSrReattach);
    }
    if close {
        actions.push(UiAction::CloseSrReattach);
    }
}

fn move_disk_dialog(ctx: &egui::Context, state: &mut UiState, actions: &mut Vec<UiAction>) {
    let mut close = false;
    let mut save = false;
    let Some(dialog) = state.move_disk_dialog.as_mut() else {
        return;
    };
    let response = egui::Modal::new(egui::Id::unique("move-disk-modal"))
        .frame(theme::modal_frame())
        .show(ctx, |ui| {
            ui.set_width(540.0);
            modal_title(
                ui,
                "Move virtual disk",
                if dialog.live {
                    "The disk is migrated live; the VM stays running."
                } else {
                    "The disk is copied to the target SR and then re-attached."
                },
            );
            ui.label(
                RichText::new(format!("{} · {}", dialog.name, crate::format::bytes(dialog.size)))
                    .size(11.0)
                    .color(muted_fg()),
            );
            ui.add_space(8.0);
            widgets::field_label(ui, "Target storage repository");
            let selected = dialog
                .selected
                .as_ref()
                .and_then(|uuid| dialog.srs.iter().find(|(sr_uuid, _, _)| sr_uuid == uuid))
                .map(|(_, name, free)| format!("{name} · {} free", crate::format::bytes(*free)))
                .unwrap_or_else(|| "Select a storage repository".into());
            ui.add_enabled_ui(!dialog.submitting, |ui| {
                egui::ComboBox::from_id_salt("move-disk-sr")
                    .selected_text(RichText::new(selected).size(11.0))
                    .width(ui.available_width())
                    .show_ui(ui, |ui| {
                        for (uuid, name, free) in &dialog.srs {
                            let label =
                                format!("{name} · {} free", crate::format::bytes(*free));
                            if ui
                                .selectable_label(
                                    dialog.selected.as_deref() == Some(uuid.as_str()),
                                    RichText::new(label).size(11.0),
                                )
                                .clicked()
                            {
                                dialog.selected = Some(uuid.clone());
                            }
                        }
                    });
            });
            ui.add_space(8.0);
            ui.label(
                RichText::new(if dialog.live {
                    "The destination SR must be visible to the running guest. The VM stays running during the move."
                } else {
                    "The VM must be shut down. The copy uses the target SR's free space until the source is deleted."
                })
                .size(9.0)
                .color(dim_fg()),
            );
            if let Some(error) = &dialog.error {
                ui.add_space(8.0);
                ui.label(RichText::new(error).size(10.0).color(red()));
            }
            ui.add_space(12.0);
            ui.horizontal(|ui| {
                if ui
                    .add_enabled(
                        !dialog.submitting,
                        egui::Button::new(RichText::new("Cancel").size(11.0)),
                    )
                    .clicked()
                {
                    close = true;
                }
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if ui
                        .add_enabled(
                            !dialog.submitting && dialog.selected.is_some(),
                            widgets::primary_button(if dialog.submitting {
                                "Moving…"
                            } else {
                                "Move disk"
                            }),
                        )
                        .clicked()
                    {
                        save = true;
                    }
                });
            });
        });
    if response.should_close() && !dialog.submitting {
        close = true;
    }
    if save {
        actions.push(UiAction::SaveMoveDisk);
    }
    if close {
        actions.push(UiAction::CloseMoveDisk);
    }
}

fn vdi_delete_confirm(ctx: &egui::Context, state: &mut UiState, actions: &mut Vec<UiAction>) {
    let mut close = false;
    let mut delete = false;
    let Some(dialog) = state.vdi_delete_confirm.as_mut() else {
        return;
    };
    let response = egui::Modal::new(egui::Id::unique("vdi-delete-modal"))
        .frame(theme::modal_frame())
        .show(ctx, |ui| {
            ui.set_width(500.0);
            modal_title(
                ui,
                "Delete virtual disk",
                "This permanently removes the disk and its data from storage.",
            );
            ui.horizontal(|ui| {
                widgets::dot(ui, red(), 6.0);
                ui.vertical(|ui| {
                    ui.spacing_mut().item_spacing.y = 0.0;
                    ui.label(
                        RichText::new(if dialog.name.is_empty() {
                            "(unnamed disk)"
                        } else {
                            &dialog.name
                        })
                        .size(11.5)
                        .strong(),
                    );
                    ui.label(
                        RichText::new(format!(
                            "{} · {}",
                            crate::format::bytes(dialog.size),
                            dialog.vdi_uuid
                        ))
                        .monospace()
                        .size(9.0)
                        .color(dim_fg()),
                    );
                });
            });
            ui.add_space(8.0);
            ui.label(
                RichText::new("The disk must remain detached. Deletion cannot be undone.")
                    .size(9.5)
                    .color(red()),
            );
            if let Some(error) = &dialog.error {
                ui.add_space(8.0);
                ui.label(RichText::new(error).size(10.0).color(red()));
            }
            ui.add_space(12.0);
            ui.horizontal(|ui| {
                if ui
                    .add_enabled(
                        !dialog.submitting,
                        egui::Button::new(RichText::new("Cancel").size(11.0)),
                    )
                    .clicked()
                {
                    close = true;
                }
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if ui
                        .add_enabled(
                            !dialog.submitting,
                            egui::Button::new(
                                RichText::new(if dialog.submitting {
                                    "Deleting…"
                                } else {
                                    "Delete disk"
                                })
                                .size(11.0)
                                .color(red()),
                            )
                            .stroke(egui::Stroke::new(1.0, with_alpha(red(), 120))),
                        )
                        .clicked()
                    {
                        delete = true;
                    }
                });
            });
        });
    if response.should_close() && !dialog.submitting {
        close = true;
    }
    if delete {
        actions.push(UiAction::ConfirmDeleteVdi);
    }
    if close {
        actions.push(UiAction::CancelDeleteVdi);
    }
}

fn ha_edit_dialog(ctx: &egui::Context, state: &mut UiState, actions: &mut Vec<UiAction>) {
    let mut close = false;
    let mut save = false;
    let Some(dialog) = state.ha_edit_dialog.as_mut() else {
        return;
    };
    let response = egui::Modal::new(egui::Id::unique("vm-ha-modal"))
        .frame(theme::modal_frame())
        .show(ctx, |ui| {
            ui.set_width(540.0);
            modal_title(
                ui,
                "HA and startup settings",
                "Restart priority is used when the pool must recover guests.",
            );
            widgets::field_label(ui, "Restart priority");
            let priority_label = match dialog.restart_priority.as_str() {
                "restart" => "Restart if possible".to_owned(),
                "best-effort" => "Always restart".to_owned(),
                "" => "Never restart".to_owned(),
                other => other.to_owned(),
            };
            ui.add_enabled_ui(!dialog.submitting, |ui| {
                egui::ComboBox::from_id_salt("vm-ha-priority")
                    .selected_text(RichText::new(priority_label).size(11.0))
                    .width(ui.available_width())
                    .show_ui(ui, |ui| {
                        for (value, label) in [
                            ("", "Never restart"),
                            ("restart", "Restart if possible"),
                            ("best-effort", "Always restart"),
                        ] {
                            if ui
                                .selectable_label(
                                    dialog.restart_priority == value,
                                    RichText::new(label).size(11.0),
                                )
                                .clicked()
                            {
                                dialog.restart_priority = value.into();
                            }
                        }
                    });
            });
            ui.add_space(8.0);
            widgets::field_label(ui, "Start delay (seconds)");
            ui.add_enabled(
                !dialog.submitting,
                egui::TextEdit::singleline(&mut dialog.start_delay)
                    .desired_width(f32::INFINITY)
                    .char_limit(7)
                    .margin(egui::Margin::symmetric(8, 5)),
            );
            ui.add_space(8.0);
            widgets::field_label(ui, "Shutdown delay (seconds)");
            ui.add_enabled(
                !dialog.submitting,
                egui::TextEdit::singleline(&mut dialog.shutdown_delay)
                    .desired_width(f32::INFINITY)
                    .char_limit(7)
                    .margin(egui::Margin::symmetric(8, 5)),
            );
            ui.add_space(8.0);
            widgets::field_label(ui, "Affinity host");
            let affinity_label = dialog
                .affinity_host_uuid
                .as_ref()
                .and_then(|uuid| {
                    dialog
                        .hosts
                        .iter()
                        .find(|(host_uuid, _)| host_uuid == uuid)
                        .map(|(_, name)| name.clone())
                })
                .unwrap_or_else(|| "No affinity".into());
            ui.add_enabled_ui(!dialog.submitting, |ui| {
                egui::ComboBox::from_id_salt("vm-ha-affinity")
                    .selected_text(RichText::new(affinity_label).size(11.0))
                    .width(ui.available_width())
                    .show_ui(ui, |ui| {
                        if ui
                            .selectable_label(
                                dialog.affinity_host_uuid.is_none(),
                                RichText::new("No affinity").size(11.0),
                            )
                            .clicked()
                        {
                            dialog.affinity_host_uuid = None;
                        }
                        for (uuid, name) in &dialog.hosts {
                            if ui
                                .selectable_label(
                                    dialog.affinity_host_uuid.as_deref() == Some(uuid.as_str()),
                                    RichText::new(name).size(11.0),
                                )
                                .clicked()
                            {
                                dialog.affinity_host_uuid = Some(uuid.clone());
                            }
                        }
                    });
            });
            if let Some(error) = &dialog.error {
                ui.add_space(8.0);
                ui.label(RichText::new(error).size(10.0).color(red()));
            }
            ui.add_space(12.0);
            ui.horizontal(|ui| {
                if ui
                    .add_enabled(
                        !dialog.submitting,
                        egui::Button::new(RichText::new("Cancel").size(11.0)),
                    )
                    .clicked()
                {
                    close = true;
                }
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if ui
                        .add_enabled(
                            !dialog.submitting,
                            widgets::primary_button(if dialog.submitting {
                                "Saving…"
                            } else {
                                "Save HA settings"
                            }),
                        )
                        .clicked()
                    {
                        save = true;
                    }
                });
            });
        });
    if response.should_close() && !dialog.submitting {
        close = true;
    }
    if save {
        actions.push(UiAction::SaveHaEdit);
    }
    if close {
        actions.push(UiAction::CloseHaEdit);
    }
}

fn attach_vdi_dialog(ctx: &egui::Context, state: &mut UiState, actions: &mut Vec<UiAction>) {
    let mut close = false;
    let mut attach = false;
    let Some(dialog) = state.attach_vdi_dialog.as_mut() else {
        return;
    };
    let response = egui::Modal::new(egui::Id::unique("attach-vdi-modal"))
        .frame(theme::modal_frame())
        .show(ctx, |ui| {
            ui.set_width(600.0);
            modal_title(
                ui,
                "Attach existing disk",
                "Choose an unattached user VDI from this pool.",
            );
            widgets::field_label(ui, "Search disks");
            ui.add_enabled(
                !dialog.submitting && !dialog.loading,
                egui::TextEdit::singleline(&mut dialog.search)
                    .desired_width(f32::INFINITY)
                    .hint_text("Filter by disk name or storage repository")
                    .margin(egui::Margin::symmetric(8, 5)),
            );
            ui.add_space(8.0);
            if dialog.loading {
                ui.horizontal(|ui| {
                    ui.spinner();
                    ui.label(
                        RichText::new("Loading available disks…")
                            .size(10.5)
                            .color(muted_fg()),
                    );
                });
            } else {
                let query = dialog.search.trim().to_lowercase();
                let mut visible = 0usize;
                egui::ScrollArea::vertical()
                    .max_height(300.0)
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        for vdi in &dialog.vdis {
                            if !query.is_empty()
                                && !vdi.name.to_lowercase().contains(&query)
                                && !vdi.sr_name.to_lowercase().contains(&query)
                            {
                                continue;
                            }
                            visible += 1;
                            let selected = dialog.selected.as_deref() == Some(vdi.uuid.as_str());
                            ui.horizontal(|ui| {
                                if ui
                                    .selectable_label(
                                        selected,
                                        RichText::new(if vdi.name.is_empty() {
                                            "(unnamed disk)"
                                        } else {
                                            &vdi.name
                                        })
                                        .size(11.0),
                                    )
                                    .clicked()
                                {
                                    dialog.selected = Some(vdi.uuid.clone());
                                }
                                ui.label(
                                    RichText::new(format!(
                                        "{} · {}",
                                        vdi.sr_name,
                                        crate::format::bytes(vdi.virtual_size)
                                    ))
                                    .size(9.0)
                                    .color(dim_fg()),
                                );
                            });
                        }
                        if visible == 0 {
                            ui.label(
                                RichText::new(if dialog.vdis.is_empty() {
                                    "No unattached writable disks were found."
                                } else {
                                    "No disks match the search."
                                })
                                .size(10.5)
                                .color(dim_fg()),
                            );
                        }
                    });
            }
            ui.add_space(8.0);
            ui.add_enabled(
                !dialog.submitting && !dialog.loading,
                egui::Checkbox::new(
                    &mut dialog.bootable,
                    RichText::new("Mark the attached disk as bootable").size(10.5),
                ),
            );
            if let Some(error) = &dialog.error {
                ui.add_space(8.0);
                ui.label(RichText::new(error).size(10.0).color(red()));
            }
            ui.add_space(12.0);
            ui.horizontal(|ui| {
                if ui
                    .add_enabled(
                        !dialog.submitting,
                        egui::Button::new(RichText::new("Cancel").size(11.0)),
                    )
                    .clicked()
                {
                    close = true;
                }
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if ui
                        .add_enabled(
                            !dialog.submitting && !dialog.loading && dialog.selected.is_some(),
                            widgets::primary_button(if dialog.submitting {
                                "Attaching…"
                            } else {
                                "Attach disk"
                            }),
                        )
                        .clicked()
                    {
                        attach = true;
                    }
                });
            });
        });
    if response.should_close() && !dialog.submitting && !dialog.loading {
        close = true;
    }
    if attach {
        actions.push(UiAction::SaveAttachVdi);
    }
    if close {
        actions.push(UiAction::CloseAttachVdi);
    }
}

fn disk_dialog(ctx: &egui::Context, state: &mut UiState, actions: &mut Vec<UiAction>) {
    let mut close = false;
    let mut save = false;
    let Some(dialog) = state.disk_dialog.as_mut() else {
        return;
    };
    let response = egui::Modal::new(egui::Id::unique("disk-modal"))
        .frame(theme::modal_frame())
        .show(ctx, |ui| {
            ui.set_width(540.0);
            match dialog {
                crate::state::DiskDialog::Add(dialog) => {
                    modal_title(
                        ui,
                        "Add virtual disk",
                        "Create a new disk in a storage repository. The VM must be shut down.",
                    );
                    widgets::field_label(ui, "Storage repository");
                    let selected = dialog
                        .sr_uuid
                        .as_ref()
                        .and_then(|uuid| dialog.srs.iter().find(|(sr_uuid, _, _)| sr_uuid == uuid))
                        .map(|(_, name, free)| {
                            format!("{name} · {} free", crate::format::bytes(*free))
                        })
                        .unwrap_or_else(|| "Select a storage repository".into());
                    ui.add_enabled_ui(!dialog.submitting, |ui| {
                        egui::ComboBox::from_id_salt("disk-sr")
                            .selected_text(RichText::new(selected).size(11.0))
                            .width(ui.available_width())
                            .show_ui(ui, |ui| {
                                for (uuid, name, free) in &dialog.srs {
                                    let text =
                                        format!("{name} · {} free", crate::format::bytes(*free));
                                    let checked = dialog.sr_uuid.as_deref() == Some(uuid.as_str());
                                    if ui
                                        .selectable_label(checked, RichText::new(text).size(11.0))
                                        .clicked()
                                    {
                                        dialog.sr_uuid = Some(uuid.clone());
                                    }
                                }
                            });
                    });
                    ui.add_space(8.0);
                    disk_size_field(ui, "Size (GiB)", &mut dialog.size_gib, !dialog.submitting);
                    ui.add_space(8.0);
                    widgets::field_label(ui, "Disk name (optional)");
                    ui.add_enabled(
                        !dialog.submitting,
                        egui::TextEdit::singleline(&mut dialog.name)
                            .desired_width(f32::INFINITY)
                            .hint_text("Generated from the VM name")
                            .char_limit(255)
                            .margin(egui::Margin::symmetric(8, 5)),
                    );
                    if let Some(error) = &dialog.error {
                        ui.add_space(8.0);
                        ui.label(RichText::new(error).size(10.0).color(red()));
                    }
                    dialog_footer(ui, dialog.submitting, "Add disk", &mut close, &mut save);
                }
                crate::state::DiskDialog::Resize(dialog) => {
                    modal_title(
                        ui,
                        "Resize virtual disk",
                        "Disks can be increased while the VM is shut down.",
                    );
                    ui.label(
                        RichText::new(format!(
                            "{} · current size {} GiB",
                            dialog.name, dialog.current_gib
                        ))
                        .size(11.0)
                        .color(muted_fg()),
                    );
                    ui.add_space(8.0);
                    disk_size_field(
                        ui,
                        "New size (GiB)",
                        &mut dialog.size_gib,
                        !dialog.submitting,
                    );
                    if let Some(error) = &dialog.error {
                        ui.add_space(8.0);
                        ui.label(RichText::new(error).size(10.0).color(red()));
                    }
                    dialog_footer(ui, dialog.submitting, "Resize disk", &mut close, &mut save);
                }
                crate::state::DiskDialog::Remove(dialog) => {
                    modal_title(
                        ui,
                        "Remove virtual disk",
                        "Detach the disk from this VM. The VM must be shut down.",
                    );
                    ui.label(RichText::new(&dialog.name).size(11.5).strong());
                    ui.add_space(8.0);
                    ui.add_enabled(
                        dialog.owner && !dialog.submitting,
                        egui::Checkbox::new(
                            &mut dialog.delete_vdi,
                            RichText::new("Delete the disk from storage").size(10.5),
                        ),
                    );
                    if !dialog.owner {
                        ui.label(
                            RichText::new("This disk is shared; only the attachment is removed.")
                                .size(9.0)
                                .color(amber()),
                        );
                    }
                    ui.label(
                        RichText::new("Deleting the disk cannot be undone.")
                            .size(9.0)
                            .color(red()),
                    );
                    if let Some(error) = &dialog.error {
                        ui.add_space(8.0);
                        ui.label(RichText::new(error).size(10.0).color(red()));
                    }
                    ui.add_space(12.0);
                    ui.horizontal(|ui| {
                        if ui
                            .add_enabled(
                                !dialog.submitting,
                                egui::Button::new(RichText::new("Cancel").size(11.0)),
                            )
                            .clicked()
                        {
                            close = true;
                        }
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            if ui
                                .add_enabled(
                                    !dialog.submitting,
                                    egui::Button::new(
                                        RichText::new(if dialog.submitting {
                                            "Removing…"
                                        } else {
                                            "Remove disk"
                                        })
                                        .size(11.0)
                                        .color(red()),
                                    )
                                    .stroke(egui::Stroke::new(1.0, with_alpha(red(), 120))),
                                )
                                .clicked()
                            {
                                save = true;
                            }
                        });
                    });
                }
            }
        });
    let submitting = match dialog {
        crate::state::DiskDialog::Add(dialog) => dialog.submitting,
        crate::state::DiskDialog::Resize(dialog) => dialog.submitting,
        crate::state::DiskDialog::Remove(dialog) => dialog.submitting,
    };
    if response.should_close() && !submitting {
        close = true;
    }
    if save {
        actions.push(UiAction::SaveDiskDialog);
    }
    if close {
        actions.push(UiAction::CloseDiskDialog);
    }
}

fn disk_size_field(ui: &mut Ui, label: &str, value: &mut String, enabled: bool) {
    widgets::field_label(ui, label);
    ui.add_enabled(
        enabled,
        egui::TextEdit::singleline(value)
            .desired_width(f32::INFINITY)
            .char_limit(8)
            .margin(egui::Margin::symmetric(8, 5)),
    );
}

fn dialog_footer(
    ui: &mut Ui,
    submitting: bool,
    save_label: &str,
    close: &mut bool,
    save: &mut bool,
) {
    ui.add_space(12.0);
    ui.horizontal(|ui| {
        if ui
            .add_enabled(
                !submitting,
                egui::Button::new(RichText::new("Cancel").size(11.0)),
            )
            .clicked()
        {
            *close = true;
        }
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            if ui
                .add_enabled(
                    !submitting,
                    widgets::primary_button(if submitting { "Working…" } else { save_label }),
                )
                .clicked()
            {
                *save = true;
            }
        });
    });
}

fn vif_dialog(ctx: &egui::Context, state: &mut UiState, actions: &mut Vec<UiAction>) {
    let mut close = false;
    let mut save = false;
    let Some(dialog) = state.vif_dialog.as_mut() else {
        return;
    };
    let response = egui::Modal::new(egui::Id::unique("vif-modal"))
        .frame(theme::modal_frame())
        .show(ctx, |ui| {
            ui.set_width(520.0);
            let editing = dialog.vif_uuid.is_some();
            modal_title(
                ui,
                if editing {
                    "Edit network interface"
                } else {
                    "Add network interface"
                },
                if editing {
                    "Move the interface to another network or change its locking mode."
                } else {
                    "The interface is created with the next free device slot."
                },
            );
            widgets::field_label(ui, "Network");
            let selected = dialog
                .network_uuid
                .as_ref()
                .and_then(|uuid| {
                    dialog
                        .networks
                        .iter()
                        .find(|(network_uuid, _)| network_uuid == uuid)
                })
                .map(|(_, name)| name.as_str())
                .unwrap_or("Select a network");
            ui.add_enabled_ui(!dialog.submitting, |ui| {
                egui::ComboBox::from_id_salt("vif-network")
                    .selected_text(RichText::new(selected).size(11.0))
                    .width(ui.available_width())
                    .show_ui(ui, |ui| {
                        for (uuid, name) in &dialog.networks {
                            let checked = dialog.network_uuid.as_deref() == Some(uuid.as_str());
                            if ui
                                .selectable_label(checked, RichText::new(name).size(11.0))
                                .clicked()
                            {
                                dialog.network_uuid = Some(uuid.clone());
                            }
                        }
                    });
            });
            ui.add_space(8.0);
            widgets::field_label(ui, "MAC address (optional)");
            ui.add_enabled(
                !dialog.submitting && !editing,
                egui::TextEdit::singleline(&mut dialog.mac)
                    .desired_width(f32::INFINITY)
                    .hint_text("Leave empty to generate automatically")
                    .char_limit(17)
                    .margin(egui::Margin::symmetric(8, 5)),
            );
            if editing {
                ui.label(
                    RichText::new("The MAC cannot be changed; remove and recreate the interface.")
                        .size(9.0)
                        .color(dim_fg()),
                );
            }
            ui.add_space(8.0);
            widgets::field_label(ui, "Locking mode");
            let locking_label = match dialog.locking_mode.as_str() {
                "locked" => "Locked",
                "unlocked" => "Unlocked",
                _ => "Network default",
            };
            ui.add_enabled_ui(!dialog.submitting, |ui| {
                egui::ComboBox::from_id_salt("vif-locking")
                    .selected_text(RichText::new(locking_label).size(11.0))
                    .width(ui.available_width())
                    .show_ui(ui, |ui| {
                        for (value, label) in [
                            ("network_default", "Network default"),
                            ("locked", "Locked"),
                            ("unlocked", "Unlocked"),
                        ] {
                            if ui
                                .selectable_label(
                                    dialog.locking_mode == value,
                                    RichText::new(label).size(11.0),
                                )
                                .clicked()
                            {
                                dialog.locking_mode = value.into();
                            }
                        }
                    });
            });
            if let Some(error) = &dialog.error {
                ui.add_space(8.0);
                ui.label(RichText::new(error).size(10.0).color(red()));
            }
            ui.add_space(12.0);
            ui.horizontal(|ui| {
                if ui
                    .add_enabled(
                        !dialog.submitting,
                        egui::Button::new(RichText::new("Cancel").size(11.0)),
                    )
                    .clicked()
                {
                    close = true;
                }
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if ui
                        .add_enabled(
                            !dialog.submitting && dialog.network_uuid.is_some(),
                            widgets::primary_button(if dialog.submitting {
                                "Saving…"
                            } else if editing {
                                "Save changes"
                            } else {
                                "Add interface"
                            }),
                        )
                        .clicked()
                    {
                        save = true;
                    }
                });
            });
        });
    if response.should_close() && !dialog.submitting {
        close = true;
    }
    if save {
        actions.push(UiAction::SaveVif);
    }
    if close {
        actions.push(UiAction::CloseVifDialog);
    }
}

fn vif_confirm(ctx: &egui::Context, state: &mut UiState, actions: &mut Vec<UiAction>) {
    let mut close = false;
    let mut remove = false;
    let Some(confirm) = state.vif_confirm.as_ref() else {
        return;
    };
    let response = egui::Modal::new(egui::Id::unique("vif-confirm-modal"))
        .frame(theme::modal_frame())
        .show(ctx, |ui| {
            ui.set_width(460.0);
            modal_title(
                ui,
                "Remove network interface",
                "This detaches the interface and deletes its MAC configuration.",
            );
            ui.label(
                RichText::new(format!("Interface MAC: {}", confirm.mac))
                    .size(11.0)
                    .color(muted_fg()),
            );
            ui.add_space(8.0);
            ui.label(
                RichText::new("Running VMs can lose connectivity immediately.")
                    .size(9.5)
                    .color(red()),
            );
            ui.add_space(12.0);
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
                                RichText::new("Remove interface").size(11.0).color(red()),
                            )
                            .stroke(egui::Stroke::new(1.0, with_alpha(red(), 120))),
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
        actions.push(UiAction::RemoveVif);
    }
    if close {
        actions.push(UiAction::CancelRemoveVif);
    }
}

fn iso_dialog(ctx: &egui::Context, state: &mut UiState, actions: &mut Vec<UiAction>) {
    let mut close = false;
    let mut save = false;
    let Some(dialog) = state.iso_dialog.as_mut() else {
        return;
    };
    let response = egui::Modal::new(egui::Id::unique("iso-modal"))
        .frame(theme::modal_frame())
        .show(ctx, |ui| {
            ui.set_width(560.0);
            modal_title(
                ui,
                &format!("CD/DVD media · {}", dialog.drive),
                "Choose an ISO image from the pool, or eject the current media.",
            );
            widgets::field_label(ui, "Search ISO images");
            ui.add_enabled(
                !dialog.submitting,
                egui::TextEdit::singleline(&mut dialog.search)
                    .desired_width(f32::INFINITY)
                    .hint_text("Filter by name or storage repository")
                    .margin(egui::Margin::symmetric(8, 5)),
            );
            ui.add_space(6.0);
            let query = dialog.search.trim().to_lowercase();
            let mut visible = 0usize;
            egui::ScrollArea::vertical()
                .max_height(280.0)
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    let empty_selected = dialog.selected.is_none();
                    if ui
                        .selectable_label(
                            empty_selected,
                            RichText::new("Empty / eject media").size(11.0),
                        )
                        .clicked()
                    {
                        dialog.selected = None;
                    }
                    for iso in &dialog.isos {
                        if !query.is_empty()
                            && !iso.name.to_lowercase().contains(&query)
                            && !iso.sr_name.to_lowercase().contains(&query)
                        {
                            continue;
                        }
                        visible += 1;
                        let selected = dialog.selected.as_deref() == Some(iso.uuid.as_str());
                        ui.horizontal(|ui| {
                            if ui
                                .selectable_label(
                                    selected,
                                    RichText::new(if iso.name.is_empty() {
                                        "(unnamed ISO)"
                                    } else {
                                        &iso.name
                                    })
                                    .size(11.0),
                                )
                                .clicked()
                            {
                                dialog.selected = Some(iso.uuid.clone());
                            }
                            ui.label(
                                RichText::new(format!(
                                    "{} · {}",
                                    iso.sr_name,
                                    crate::format::bytes(iso.size)
                                ))
                                .size(9.0)
                                .color(dim_fg()),
                            );
                        });
                    }
                    if visible == 0 {
                        ui.label(
                            RichText::new(if dialog.isos.is_empty() {
                                "No ISO images were found in this pool."
                            } else {
                                "No ISO images match the search."
                            })
                            .size(10.5)
                            .color(dim_fg()),
                        );
                    }
                });
            if let Some(error) = &dialog.error {
                ui.add_space(8.0);
                ui.label(RichText::new(error).size(10.0).color(red()));
            }
            ui.add_space(12.0);
            ui.horizontal(|ui| {
                if ui
                    .add_enabled(
                        !dialog.submitting,
                        egui::Button::new(RichText::new("Cancel").size(11.0)),
                    )
                    .clicked()
                {
                    close = true;
                }
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if ui
                        .add_enabled(
                            !dialog.submitting,
                            widgets::primary_button(if dialog.submitting {
                                "Applying…"
                            } else if dialog.selected.is_some() {
                                "Insert media"
                            } else {
                                "Eject media"
                            }),
                        )
                        .clicked()
                    {
                        save = true;
                    }
                });
            });
        });
    if response.should_close() && !dialog.submitting {
        close = true;
    }
    if save {
        actions.push(UiAction::SaveIso);
    }
    if close {
        actions.push(UiAction::CloseIsoDialog);
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
