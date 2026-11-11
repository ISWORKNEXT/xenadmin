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
    if state.vm_copy.is_some() {
        super::vm_copy::show(ctx, state, workspace, actions);
    }
    if state.properties.is_some() {
        super::properties::show(ctx, state, workspace, actions);
    }
    if state.xva_dialog.is_some() {
        super::xva::show(ctx, state, workspace, actions);
    }
    if state.new_vm.is_some() {
        super::new_vm::show(ctx, state, workspace, actions);
    }
    if state.help {
        help(ctx, state, actions);
    }
    if state.resource.is_some() {
        resource(ctx, state, actions);
    }
    if state.record_dialog.is_some() {
        record(ctx, state);
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
    if state.ssh_credentials.is_some() {
        ssh_credentials_dialog(ctx, state, actions);
    }
    if state.rdp_credentials.is_some() {
        rdp_credentials_dialog(ctx, state, actions);
    }
    if state.ha_enable_dialog.is_some() {
        ha_enable_dialog(ctx, state, actions);
    }
    if state.wlb_config_dialog.is_some() {
        wlb_config_dialog(ctx, state, actions);
    }
    if state.pool_confirm.is_some() {
        pool_confirm_dialog(ctx, state, actions);
    }
    if state.vmss_dialog.is_some() {
        vmss_dialog(ctx, state, actions);
    }
    if state.vmss_delete_confirm.is_some() {
        vmss_delete_dialog(ctx, state, actions);
    }
    if state.pvs_site_dialog.is_some() {
        pvs_site_dialog(ctx, state, actions);
    }
    if state.pvs_server_dialog.is_some() {
        pvs_server_dialog(ctx, state, actions);
    }
    if state.pvs_cache_dialog.is_some() {
        pvs_cache_dialog(ctx, state, actions);
    }
    if state.pvs_proxy_dialog.is_some() {
        pvs_proxy_dialog(ctx, state, actions);
    }
    if state.pvs_delete_confirm.is_some() {
        pvs_delete_dialog(ctx, state, actions);
    }
    if state.dr_task_dialog.is_some() {
        dr_task_dialog(ctx, state, actions);
    }
    if state.dr_task_delete_confirm.is_some() {
        dr_task_delete_dialog(ctx, state, actions);
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
    if state.network_chooser.is_some() {
        network_chooser(ctx, state, actions);
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
                let host = ui.add(
                    egui::TextEdit::singleline(&mut connect.settings.host)
                        .hint_text("192.168.8.170")
                        .desired_width(f32::INFINITY)
                        .margin(egui::Margin::symmetric(8, 5)),
                );
                if connect.focus_host {
                    host.request_focus();
                    connect.focus_host = false;
                }
                if host.changed() && normalized_host(&connect.settings.host) == "10.100.1.11" {
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
    let snapshot_dialog = matches!(
        intent.kind,
        crate::state::ActionKind::Snapshot
            | crate::state::ActionKind::Checkpoint
            | crate::state::ActionKind::SnapshotWithQuiesce
    );
    let response = egui::Modal::new(egui::Id::unique("intent-modal"))
        .frame(theme::modal_frame())
        .show(ctx, |ui| {
            ui.set_width(520.0);
            let title = intent
                .host
                .as_ref()
                .map(|(_, host)| format!("{} on {}", intent.kind.label(), host))
                .unwrap_or_else(|| if snapshot_dialog { "Take Snapshot".into() } else if intent.kind == crate::state::ActionKind::Destroy { "Delete VM".into() } else { intent.kind.label().to_owned() });
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
                    match intent.kind {
                        crate::state::ActionKind::Snapshot | crate::state::ActionKind::Checkpoint | crate::state::ActionKind::SnapshotWithQuiesce => "Snapshot name",
                        crate::state::ActionKind::SnapshotToTemplate => "New template name",
                        _ => "New machine name",
                    },
                );
                if matches!(intent.kind, crate::state::ActionKind::SnapshotToVm | crate::state::ActionKind::SnapshotToTemplate) {
                    ui.label("Creates an independent halted object. The source VM and snapshot remain unchanged.");
                }
                ui.add(
                    egui::TextEdit::singleline(&mut intent.name)
                        .desired_width(f32::INFINITY)
                        .char_limit(255)
                        .margin(egui::Margin::symmetric(8, 5)),
                );
            }
            if intent.kind==crate::state::ActionKind::ConvertToTemplate {
                ui.label("Converts this halted VM into a template. Its disks stay attached and the object appears under Templates.");
            }
            if snapshot_dialog {
                widgets::field_label(ui, "Description");
                ui.add(egui::TextEdit::multiline(&mut intent.description).desired_rows(3).desired_width(f32::INFINITY).char_limit(xen_core::properties::MAX_DESCRIPTION_CHARS));
                for (kind, label, help, unavailable) in [
                    (crate::state::ActionKind::Snapshot, "Snapshot the virtual machine's disks", "A snapshot of the virtual machine's disks.", "Disk snapshots are not currently available for this VM"),
                    (crate::state::ActionKind::SnapshotWithQuiesce, "Quiesced snapshot", "Quiesced snapshots need the VSS service to be enabled on the VM", "Quiesced snapshots need Management Agent and VSS to be installed and enabled"),
                    (crate::state::ActionKind::Checkpoint, "Snapshot the virtual machine's disks and memory", "A snapshot of the virtual machine's disks and memory.", "Disk and memory snapshots are not currently available for this VM"),
                ] {
                    let running_required = kind != crate::state::ActionKind::Snapshot;
                    let enabled = intent.targets.iter().all(|(id, uuid)| find_vm(workspace, id, uuid).is_some_and(|(_, vm)| (!running_required || vm.power_state == "Running") && crate::state::action_allowed(workspace, id, vm, kind.operation())));
                    ui.add_enabled_ui(enabled, |ui| { ui.radio_value(&mut intent.kind, kind, label); });
                    let running = intent.targets.iter().all(|(id, uuid)| find_vm(workspace, id, uuid).is_some_and(|(_, vm)| vm.power_state == "Running"));
                    ui.label(RichText::new(if enabled { help } else if running_required && !running { if kind == crate::state::ActionKind::Checkpoint { "Disk and memory snapshots need the VM to be running" } else { "Quiesced snapshots need the VM to be running" } } else { unavailable }).small().color(dim_fg()));
                }
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
                ui.colored_label(red(), "The selected VMs and checked resources will be permanently deleted.");
                egui::ScrollArea::vertical().max_height(280.0).show(ui, |ui| {
                    match &mut intent.deletion {
                        crate::state::LoadState::Ready(items) => for item in items {
                            ui.strong(&item.name);
                            ui.horizontal(|ui| {
                                ui.label("Attached disks");
                                if ui.small_button("Select All").clicked() { for (d, checked) in &mut item.disks { *checked = d.owner && !d.shared && !d.is_cd; } }
                                if ui.small_button("Clear All").clicked() { for (_, checked) in &mut item.disks { *checked = false; } }
                            });
                            for (disk, checked) in &mut item.disks {
                                let eligible = disk.owner && !disk.shared && !disk.is_cd;
                                ui.add_enabled(eligible, egui::Checkbox::new(checked, format!("{} · {} · {}{}", disk.vdi_name, disk.sr_name, crate::format::bytes(disk.virtual_size), if disk.is_cd { " (ISO)" } else if disk.shared { " (shared)" } else if !disk.owner { " (not owned)" } else { "" })));
                            }
                            ui.horizontal(|ui| {
                                ui.label("Snapshots");
                                if ui.small_button("Select All").clicked() { for (_, _, checked) in &mut item.snapshots { *checked = true; } }
                                if ui.small_button("Clear All").clicked() { for (_, _, checked) in &mut item.snapshots { *checked = false; } }
                            });
                            for (_, name, checked) in &mut item.snapshots { ui.checkbox(checked, name.as_str()); }
                            ui.separator();
                        },
                        crate::state::LoadState::Error(e) => { ui.colored_label(red(), e.as_str()); },
                        _ => { ui.spinner(); ui.label("Loading disks and snapshots…"); }
                    }
                });
            } else if intent.kind.destructive()
                && !matches!(intent.kind, crate::state::ActionKind::Revert | crate::state::ActionKind::ConvertToTemplate)
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
                    let valid_name = (!intent.kind.needs_name() || !intent.name.trim().is_empty())
                        && (intent.kind != crate::state::ActionKind::Destroy || matches!(intent.deletion, crate::state::LoadState::Ready(_)))
                        && (!snapshot_dialog || intent.targets.iter().all(|(id, uuid)| find_vm(workspace, id, uuid).is_some_and(|(_, vm)| crate::state::action_allowed(workspace, id, vm, intent.kind.operation()))));
                    if ui
                        .add_enabled(
                            valid_name,
                            widgets::primary_button(if snapshot_dialog { "Take Snapshot".to_owned() } else if intent.kind == crate::state::ActionKind::Destroy { "Delete".to_owned() } else { format!("Submit {count} task{}", if count == 1 { "" } else { "s" }) }),
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

fn network_chooser(ctx: &egui::Context, state: &mut UiState, actions: &mut Vec<UiAction>) {
    let Some(bonded) = state.network_chooser.as_mut() else {
        return;
    };
    let mut close = false;
    let response = egui::Modal::new(egui::Id::unique("network-type"))
        .frame(theme::modal_frame())
        .show(ctx, |ui| {
            ui.set_width(480.0);
            ui.heading("New Network");
            ui.label("Select network type");
            ui.add_space(12.0);
            ui.radio_value(bonded, false, "External Network");
            ui.label("Connect virtual machines to an external network using a VLAN.");
            ui.radio_value(bonded, true, "Bonded Network");
            ui.label("Combine physical NICs into a bond.");
            ui.add_enabled(
                false,
                egui::RadioButton::new(false, "Single-Server Private Network"),
            )
            .on_disabled_hover_text("Not implemented yet");
            ui.label("Not implemented yet");
            ui.add_space(12.0);
            if state.pif_catalog_loading {
                ui.spinner();
                ui.label("Reading physical interfaces…");
            }
            if let Some(error) = &state.pif_catalog_error {
                ui.colored_label(theme::red(), error);
            }
            ui.horizontal(|ui| {
                if ui.button("Cancel").clicked() {
                    close = true;
                }
                if ui
                    .add_enabled(
                        !state.pif_catalog_loading && state.pif_catalog_error.is_none(),
                        widgets::primary_button("Next >"),
                    )
                    .clicked()
                {
                    close = true;
                    actions.push(if *bonded {
                        UiAction::OpenBondCreate
                    } else {
                        UiAction::OpenVlanCreate
                    });
                }
            });
        });
    if close || response.should_close() {
        actions.push(UiAction::CloseNetworkChooser);
    }
}

fn network_create(ctx: &egui::Context, state: &mut UiState, actions: &mut Vec<UiAction>) {
    let mut close = false;
    let mut save = false;
    let pifs: Vec<(String, String, String)> = state
        .pif_catalog
        .iter()
        .map(|pif| (pif.uuid.clone(), pif.host_name.clone(), pif.device.clone()))
        .collect();
    let Some(dialog) = state.network_create_dialog.as_mut() else {
        return;
    };
    let response = egui::Modal::new(egui::Id::unique("network-create-modal"))
        .frame(theme::modal_frame())
        .show(ctx, |ui| {
            ui.set_width(560.0);
            modal_title(
                ui,
                "Add network",
                match dialog.kind.as_str() {
                    "sriov" => "Creates an SR-IOV logical network on a physical function.",
                    "chin" => "Creates a cross-host internal network tunnelled over management.",
                    _ => "Creates an internal XenAPI network. Attach a PIF or VIF afterwards.",
                },
            );
            widgets::field_label(ui, "Type");
            egui::ComboBox::from_id_salt("network-kind")
                .selected_text(
                    RichText::new(match dialog.kind.as_str() {
                        "sriov" => "SR-IOV logical network",
                        "chin" => "Cross-host internal (CHIN)",
                        _ => "Internal network",
                    })
                    .size(11.0),
                )
                .width(ui.available_width())
                .show_ui(ui, |ui| {
                    for (value, label) in [
                        ("internal", "Internal network"),
                        ("sriov", "SR-IOV logical network"),
                        ("chin", "Cross-host internal (CHIN)"),
                    ] {
                        if ui
                            .selectable_label(
                                dialog.kind == value,
                                RichText::new(label).size(11.0),
                            )
                            .clicked()
                        {
                            dialog.kind = value.into();
                            dialog.pif_uuid = None;
                        }
                    }
                });
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
            ui.columns(2, |columns| {
                widgets::field_label(&mut columns[0], "MTU");
                columns[0].add_enabled(
                    !dialog.submitting,
                    egui::TextEdit::singleline(&mut dialog.mtu)
                        .char_limit(4)
                        .margin(egui::Margin::symmetric(8, 5)),
                );
                columns[1].add_enabled(
                    !dialog.submitting,
                    egui::Checkbox::new(
                        &mut dialog.autoplug,
                        RichText::new("Automatically plug new VIFs").size(10.0),
                    ),
                );
            });
            if dialog.kind != "internal" {
                ui.add_space(8.0);
                widgets::field_label(
                    ui,
                    if dialog.kind == "sriov" {
                        "Physical function (PF)"
                    } else {
                        "Transport (management) interface"
                    },
                );
                let candidates: Vec<&(String, String, String)> = pifs
                    .iter()
                    .filter(|(uuid, host, device)| {
                        let Some(pif) = state.pif_catalog.iter().find(|pif| pif.uuid == *uuid)
                        else {
                            return false;
                        };
                        let _ = (host, device);
                        if dialog.kind == "sriov" {
                            pif.sriov_candidate()
                        } else {
                            pif.chin_candidate()
                        }
                    })
                    .collect();
                let selected = dialog
                    .pif_uuid
                    .as_ref()
                    .and_then(|uuid| candidates.iter().find(|(id, _, _)| id == uuid))
                    .map(|(_, host, device)| format!("{host} · {device}"))
                    .unwrap_or_else(|| {
                        if state.pif_catalog_loading {
                            "Loading interfaces…".into()
                        } else {
                            "Select an interface".into()
                        }
                    });
                egui::ComboBox::from_id_salt("network-pif")
                    .selected_text(RichText::new(selected).size(11.0))
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
                if candidates.is_empty() && !state.pif_catalog_loading {
                    ui.label(
                        RichText::new(if dialog.kind == "sriov" {
                            "No spare physical function is available; management, VLAN and bonded interfaces are excluded."
                        } else {
                            "No attached management interface is available."
                        })
                        .size(9.0)
                        .color(amber()),
                    );
                }
                if dialog.kind == "chin" {
                    ui.add_space(8.0);
                    widgets::field_label(ui, "Tunnel protocol");
                    egui::ComboBox::from_id_salt("chin-protocol")
                        .selected_text(
                            RichText::new(if dialog.protocol == "vxlan" {
                                "VXLAN"
                            } else {
                                "GRE"
                            })
                            .size(11.0),
                        )
                        .width(ui.available_width())
                        .show_ui(ui, |ui| {
                            for (value, label) in [("gre", "GRE"), ("vxlan", "VXLAN")] {
                                if ui
                                    .selectable_label(
                                        dialog.protocol == value,
                                        RichText::new(label).size(11.0),
                                    )
                                    .clicked()
                                {
                                    dialog.protocol = value.into();
                                }
                            }
                        });
                }
                ui.label(
                    RichText::new(if dialog.kind == "sriov" {
                        "SR-IOV on the management interface is blocked to protect connectivity."
                    } else {
                        "CHIN requires a management interface on every pool host; add hosts before creating it."
                    })
                    .size(9.0)
                    .color(dim_fg()),
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
                    if ui
                        .add_enabled(
                            !dialog.submitting
                                && !dialog.name.trim().is_empty()
                                && (dialog.kind == "internal" || dialog.pif_uuid.is_some()),
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

fn dr_task_dialog(ctx: &egui::Context, state: &mut UiState, actions: &mut Vec<UiAction>) {
    let mut close = false;
    let mut save = false;
    let Some(dialog) = state.dr_task_dialog.as_mut() else {
        return;
    };
    let response = egui::Modal::new(egui::Id::unique("dr-task-modal"))
        .frame(theme::modal_frame())
        .show(ctx, |ui| {
            ui.set_width(600.0);
            modal_title(
                ui,
                "Create disaster recovery task",
                "Introduces unmanaged SRs discovered on shared storage into this pool.",
            );
            widgets::field_label(ui, "Storage type");
            egui::ComboBox::from_id_salt("dr-task-type")
                .selected_text(
                    RichText::new(match dialog.sr_type.as_str() {
                        "lvmoiscsi" => "iSCSI",
                        "lvmohba" | "hba" => "HBA / FCoE",
                        "lvmofcoe" => "FCoE",
                        _ => "NFS",
                    })
                    .size(11.0),
                )
                .width(ui.available_width())
                .show_ui(ui, |ui| {
                    for (value, label) in [
                        ("nfs", "NFS"),
                        ("lvmoiscsi", "iSCSI"),
                        ("lvmohba", "HBA"),
                        ("lvmofcoe", "FCoE"),
                        ("hba", "HBA (shared)"),
                    ] {
                        if ui
                            .selectable_label(
                                dialog.sr_type == value,
                                RichText::new(label).size(11.0),
                            )
                            .clicked()
                        {
                            dialog.sr_type = value.into();
                        }
                    }
                });
            ui.add_space(8.0);
            match dialog.sr_type.as_str() {
                "nfs" => {
                    ui.columns(2, |columns| {
                        widgets::field_label(&mut columns[0], "NFS server");
                        columns[0].add_enabled(
                            !dialog.submitting,
                            egui::TextEdit::singleline(&mut dialog.server)
                                .char_limit(255)
                                .margin(egui::Margin::symmetric(8, 5)),
                        );
                        widgets::field_label(&mut columns[1], "Server path");
                        columns[1].add_enabled(
                            !dialog.submitting,
                            egui::TextEdit::singleline(&mut dialog.serverpath)
                                .char_limit(255)
                                .margin(egui::Margin::symmetric(8, 5)),
                        );
                    });
                }
                "lvmoiscsi" | "lvmofcoe" => {
                    ui.columns(2, |columns| {
                        widgets::field_label(&mut columns[0], "Target");
                        columns[0].add_enabled(
                            !dialog.submitting,
                            egui::TextEdit::singleline(&mut dialog.target)
                                .char_limit(255)
                                .margin(egui::Margin::symmetric(8, 5)),
                        );
                        widgets::field_label(&mut columns[1], "Target IQN");
                        columns[1].add_enabled(
                            !dialog.submitting,
                            egui::TextEdit::singleline(&mut dialog.target_iqn)
                                .char_limit(255)
                                .margin(egui::Margin::symmetric(8, 5)),
                        );
                    });
                }
                _ => {
                    ui.label(
                        RichText::new("HBA/FCoE storage is discovered from the host adapter.")
                            .size(9.5)
                            .color(muted_fg()),
                    );
                }
            }
            ui.add_space(8.0);
            widgets::field_label(ui, "SR UUIDs to introduce (comma or newline separated)");
            ui.add_enabled(
                !dialog.submitting,
                egui::TextEdit::multiline(&mut dialog.whitelist)
                    .desired_width(f32::INFINITY)
                    .desired_rows(3)
                    .hint_text("11111111-2222-3333-4444-555555555555")
                    .char_limit(4096)
                    .margin(egui::Margin::symmetric(8, 5)),
            );
            ui.label(
                RichText::new(
                    "Use SR.probe or the storage's existing SR UUIDs. Only unmanaged SRs should be listed.",
                )
                .size(9.0)
                .color(dim_fg()),
            );
            ui.add_space(6.0);
            ui.add_enabled(
                !dialog.submitting,
                egui::Checkbox::new(
                    &mut dialog.acknowledged,
                    RichText::new("I understand these SRs will be introduced into this pool")
                        .size(10.0),
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
                                && !dialog.whitelist.trim().is_empty(),
                            widgets::primary_button(if dialog.submitting {
                                "Creating…"
                            } else {
                                "Create DR task"
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
        actions.push(UiAction::SaveDrTaskDialog);
    }
    if close {
        actions.push(UiAction::CloseDrTaskDialog);
    }
}

fn dr_task_delete_dialog(ctx: &egui::Context, state: &mut UiState, actions: &mut Vec<UiAction>) {
    let mut close = false;
    let mut confirm = false;
    let Some(dialog) = state.dr_task_delete_confirm.as_mut() else {
        return;
    };
    let response = egui::Modal::new(egui::Id::unique("dr-task-delete-modal"))
        .frame(theme::modal_frame())
        .show(ctx, |ui| {
            ui.set_width(520.0);
            modal_title(
                ui,
                "Destroy DR task",
                "Stops managing this DR task. Introduced SRs stay in the pool until they are forgotten.",
            );
            ui.horizontal(|ui| {
                widgets::dot(ui, amber(), 6.0);
                ui.label(RichText::new(&dialog.uuid).monospace().size(9.5));
            });
            if !dialog.introduced_srs.is_empty() {
                ui.label(
                    RichText::new(format!(
                        "Introduced SRs: {}",
                        dialog.introduced_srs.join(", ")
                    ))
                    .size(9.5)
                    .color(muted_fg()),
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
                    if ui
                        .add_enabled(
                            !dialog.submitting,
                            egui::Button::new(
                                RichText::new(if dialog.submitting {
                                    "Destroying…"
                                } else {
                                    "Destroy DR task"
                                })
                                .size(11.0)
                                .color(red()),
                            )
                            .stroke(egui::Stroke::new(1.0, with_alpha(red(), 120))),
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
        actions.push(UiAction::ConfirmDrTaskDestroy);
    }
    if close {
        actions.push(UiAction::CancelDrTaskDestroy);
    }
}

fn pvs_site_dialog(ctx: &egui::Context, state: &mut UiState, actions: &mut Vec<UiAction>) {
    let mut close = false;
    let mut save = false;
    let Some(dialog) = state.pvs_site_dialog.as_mut() else {
        return;
    };
    let response = egui::Modal::new(egui::Id::unique("pvs-site-modal"))
        .frame(theme::modal_frame())
        .show(ctx, |ui| {
            ui.set_width(540.0);
            modal_title(
                ui,
                "Add PVS site",
                "Registers a Provisioning Services site with this pool.",
            );
            widgets::field_label(ui, "Site name");
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
                                "Adding…"
                            } else {
                                "Add site"
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
        actions.push(UiAction::SavePvsSiteDialog);
    }
    if close {
        actions.push(UiAction::ClosePvsSiteDialog);
    }
}

fn pvs_server_dialog(ctx: &egui::Context, state: &mut UiState, actions: &mut Vec<UiAction>) {
    let mut close = false;
    let mut save = false;
    let Some(dialog) = state.pvs_server_dialog.as_mut() else {
        return;
    };
    let response = egui::Modal::new(egui::Id::unique("pvs-server-modal"))
        .frame(theme::modal_frame())
        .show(ctx, |ui| {
            ui.set_width(560.0);
            modal_title(
                ui,
                "Add PVS server",
                "Registers one or more PVS server addresses for a site.",
            );
            widgets::field_label(ui, "Site");
            let site_label = dialog
                .site_uuid
                .as_ref()
                .and_then(|uuid| dialog.sites.iter().find(|(id, _)| id == uuid))
                .map(|(_, name)| name.as_str())
                .unwrap_or("Select a site");
            egui::ComboBox::from_id_salt("pvs-server-site")
                .selected_text(RichText::new(site_label).size(11.0))
                .width(ui.available_width())
                .show_ui(ui, |ui| {
                    for (uuid, name) in &dialog.sites {
                        if ui
                            .selectable_label(
                                dialog.site_uuid.as_deref() == Some(uuid.as_str()),
                                RichText::new(name).size(11.0),
                            )
                            .clicked()
                        {
                            dialog.site_uuid = Some(uuid.clone());
                        }
                    }
                });
            ui.add_space(8.0);
            widgets::field_label(ui, "Addresses (comma or newline separated)");
            ui.add_enabled(
                !dialog.submitting,
                egui::TextEdit::multiline(&mut dialog.addresses)
                    .desired_width(f32::INFINITY)
                    .desired_rows(3)
                    .hint_text("10.0.0.10, pvs2.example.com")
                    .char_limit(2048)
                    .margin(egui::Margin::symmetric(8, 5)),
            );
            ui.add_space(8.0);
            ui.columns(2, |columns| {
                widgets::field_label(&mut columns[0], "First port");
                columns[0].add_enabled(
                    !dialog.submitting,
                    egui::TextEdit::singleline(&mut dialog.first_port)
                        .char_limit(5)
                        .margin(egui::Margin::symmetric(8, 5)),
                );
                widgets::field_label(&mut columns[1], "Last port");
                columns[1].add_enabled(
                    !dialog.submitting,
                    egui::TextEdit::singleline(&mut dialog.last_port)
                        .char_limit(5)
                        .margin(egui::Margin::symmetric(8, 5)),
                );
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
                            !dialog.submitting && !dialog.addresses.trim().is_empty(),
                            widgets::primary_button(if dialog.submitting {
                                "Adding…"
                            } else {
                                "Add server"
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
        actions.push(UiAction::SavePvsServerDialog);
    }
    if close {
        actions.push(UiAction::ClosePvsServerDialog);
    }
}

fn pvs_cache_dialog(ctx: &egui::Context, state: &mut UiState, actions: &mut Vec<UiAction>) {
    let mut close = false;
    let mut save = false;
    let Some(dialog) = state.pvs_cache_dialog.as_mut() else {
        return;
    };
    let response = egui::Modal::new(egui::Id::unique("pvs-cache-modal"))
        .frame(theme::modal_frame())
        .show(ctx, |ui| {
            ui.set_width(580.0);
            modal_title(
                ui,
                "Add PVS cache storage",
                "Allocates cache space on a host-local or shared SR for a PVS site.",
            );
            widgets::field_label(ui, "Site");
            let site_label = dialog
                .site_uuid
                .as_ref()
                .and_then(|uuid| dialog.sites.iter().find(|(id, _)| id == uuid))
                .map(|(_, name)| name.as_str())
                .unwrap_or("Select a site");
            egui::ComboBox::from_id_salt("pvs-cache-site")
                .selected_text(RichText::new(site_label).size(11.0))
                .width(ui.available_width())
                .show_ui(ui, |ui| {
                    for (uuid, name) in &dialog.sites {
                        if ui
                            .selectable_label(
                                dialog.site_uuid.as_deref() == Some(uuid.as_str()),
                                RichText::new(name).size(11.0),
                            )
                            .clicked()
                        {
                            dialog.site_uuid = Some(uuid.clone());
                        }
                    }
                });
            ui.add_space(8.0);
            ui.columns(2, |columns| {
                widgets::field_label(&mut columns[0], "Host");
                let host_label = dialog
                    .host_uuid
                    .as_ref()
                    .and_then(|uuid| dialog.hosts.iter().find(|(id, _)| id == uuid))
                    .map(|(_, name)| name.as_str())
                    .unwrap_or("Select a host");
                egui::ComboBox::from_id_salt("pvs-cache-host")
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
                widgets::field_label(&mut columns[1], "Cache SR");
                let sr_label = dialog
                    .sr_uuid
                    .as_ref()
                    .and_then(|uuid| dialog.srs.iter().find(|(id, _)| id == uuid))
                    .map(|(_, name)| name.as_str())
                    .unwrap_or("Select an SR");
                egui::ComboBox::from_id_salt("pvs-cache-sr")
                    .selected_text(RichText::new(sr_label).size(11.0))
                    .width(columns[1].available_width())
                    .show_ui(&mut columns[1], |ui| {
                        for (uuid, name) in &dialog.srs {
                            if ui
                                .selectable_label(
                                    dialog.sr_uuid.as_deref() == Some(uuid.as_str()),
                                    RichText::new(name).size(11.0),
                                )
                                .clicked()
                            {
                                dialog.sr_uuid = Some(uuid.clone());
                            }
                        }
                    });
            });
            ui.add_space(8.0);
            widgets::field_label(ui, "Cache size (MiB, 1024–2097152)");
            ui.add_enabled(
                !dialog.submitting,
                egui::TextEdit::singleline(&mut dialog.size_mib)
                    .desired_width(f32::INFINITY)
                    .char_limit(7)
                    .margin(egui::Margin::symmetric(8, 5)),
            );
            ui.label(
                RichText::new(
                    "The SR is not created automatically; add or choose existing storage.",
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
                                && dialog.site_uuid.is_some()
                                && dialog.host_uuid.is_some()
                                && dialog.sr_uuid.is_some(),
                            widgets::primary_button(if dialog.submitting {
                                "Adding…"
                            } else {
                                "Add cache"
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
        actions.push(UiAction::SavePvsCacheDialog);
    }
    if close {
        actions.push(UiAction::ClosePvsCacheDialog);
    }
}

fn pvs_proxy_dialog(ctx: &egui::Context, state: &mut UiState, actions: &mut Vec<UiAction>) {
    let mut close = false;
    let mut save = false;
    let Some(dialog) = state.pvs_proxy_dialog.as_mut() else {
        return;
    };
    let response = egui::Modal::new(egui::Id::unique("pvs-proxy-modal"))
        .frame(theme::modal_frame())
        .show(ctx, |ui| {
            ui.set_width(580.0);
            modal_title(
                ui,
                "Enable PVS read caching",
                "Creates a PVS proxy on a connected VM interface.",
            );
            widgets::field_label(ui, "Site");
            let site_label = dialog
                .site_uuid
                .as_ref()
                .and_then(|uuid| dialog.sites.iter().find(|(id, _)| id == uuid))
                .map(|(_, name)| name.as_str())
                .unwrap_or("Select a site");
            egui::ComboBox::from_id_salt("pvs-proxy-site")
                .selected_text(RichText::new(site_label).size(11.0))
                .width(ui.available_width())
                .show_ui(ui, |ui| {
                    for (uuid, name) in &dialog.sites {
                        if ui
                            .selectable_label(
                                dialog.site_uuid.as_deref() == Some(uuid.as_str()),
                                RichText::new(name).size(11.0),
                            )
                            .clicked()
                        {
                            dialog.site_uuid = Some(uuid.clone());
                        }
                    }
                });
            ui.add_space(8.0);
            widgets::field_label(ui, "VM interface");
            if dialog.loading {
                ui.horizontal(|ui| {
                    ui.spinner();
                    ui.label(
                        RichText::new("Loading connected interfaces…")
                            .size(10.0)
                            .color(muted_fg()),
                    );
                });
            } else if dialog.vifs.is_empty() {
                ui.label(
                    RichText::new("No connected VM interfaces are available.")
                        .size(10.0)
                        .color(amber()),
                );
            } else {
                let vif_label = dialog
                    .vif_uuid
                    .as_ref()
                    .and_then(|uuid| dialog.vifs.iter().find(|vif| vif.uuid == *uuid))
                    .map(|vif| format!("{} · {} · {}", vif.vm_name, vif.network_name, vif.mac))
                    .unwrap_or_else(|| "Select an interface".into());
                egui::ComboBox::from_id_salt("pvs-proxy-vif")
                    .selected_text(RichText::new(vif_label).size(11.0))
                    .width(ui.available_width())
                    .show_ui(ui, |ui| {
                        for vif in &dialog.vifs {
                            let label =
                                format!("{} · {} · {}", vif.vm_name, vif.network_name, vif.mac);
                            if ui
                                .selectable_label(
                                    dialog.vif_uuid.as_deref() == Some(vif.uuid.as_str()),
                                    RichText::new(label).size(11.0),
                                )
                                .clicked()
                            {
                                dialog.vif_uuid = Some(vif.uuid.clone());
                            }
                        }
                    });
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
                    if ui
                        .add_enabled(
                            !dialog.submitting
                                && !dialog.loading
                                && dialog.site_uuid.is_some()
                                && dialog.vif_uuid.is_some(),
                            widgets::primary_button(if dialog.submitting {
                                "Enabling…"
                            } else {
                                "Enable caching"
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
        actions.push(UiAction::SavePvsProxyDialog);
    }
    if close {
        actions.push(UiAction::ClosePvsProxyDialog);
    }
}

fn pvs_delete_dialog(ctx: &egui::Context, state: &mut UiState, actions: &mut Vec<UiAction>) {
    let mut close = false;
    let mut confirm = false;
    let Some(dialog) = state.pvs_delete_confirm.as_mut() else {
        return;
    };
    let (title, detail) = match dialog.kind {
        crate::state::PvsDeleteKind::Site => (
            "Delete PVS site",
            "Forgets the site and its servers. Proxies must be removed first.",
        ),
        crate::state::PvsDeleteKind::Server => (
            "Remove PVS server",
            "The server address is forgotten from this site. Cache storage is unaffected.",
        ),
        crate::state::PvsDeleteKind::Proxy => (
            "Remove PVS read caching",
            "Destroys the PVS proxy on this VM interface.",
        ),
        crate::state::PvsDeleteKind::Cache => (
            "Remove PVS cache storage",
            "Destroys the cache storage record. Data on the SR is left untouched.",
        ),
    };
    let response = egui::Modal::new(egui::Id::unique("pvs-delete-modal"))
        .frame(theme::modal_frame())
        .show(ctx, |ui| {
            ui.set_width(500.0);
            modal_title(ui, title, detail);
            ui.horizontal(|ui| {
                widgets::dot(ui, red(), 6.0);
                ui.label(RichText::new(&dialog.name).size(11.5).strong());
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
                            egui::Button::new(
                                RichText::new(if dialog.submitting {
                                    "Removing…"
                                } else {
                                    "Remove"
                                })
                                .size(11.0)
                                .color(red()),
                            )
                            .stroke(egui::Stroke::new(1.0, with_alpha(red(), 120))),
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
        actions.push(UiAction::ConfirmPvsDelete);
    }
    if close {
        actions.push(UiAction::CancelPvsDelete);
    }
}

fn vmss_dialog(ctx: &egui::Context, state: &mut UiState, actions: &mut Vec<UiAction>) {
    let mut close = false;
    let mut save = false;
    let Some(dialog) = state.vmss_dialog.as_mut() else {
        return;
    };
    let editing = dialog.uuid.is_some();
    let response = egui::Modal::new(egui::Id::unique("vmss-modal"))
        .frame(theme::modal_frame())
        .show(ctx, |ui| {
            ui.set_width(640.0);
            modal_title(
                ui,
                if editing {
                    "Edit snapshot schedule"
                } else {
                    "New snapshot schedule"
                },
                "Scheduled snapshots are taken automatically and pruned to the retention count.",
            );
            ui.columns(2, |columns| {
                widgets::field_label(&mut columns[0], "Name");
                columns[0].add_enabled(
                    !dialog.submitting,
                    egui::TextEdit::singleline(&mut dialog.name)
                        .char_limit(255)
                        .margin(egui::Margin::symmetric(8, 5)),
                );
                widgets::field_label(&mut columns[1], "Snapshot type");
                egui::ComboBox::from_id_salt("vmss-type")
                    .selected_text(
                        RichText::new(match dialog.snapshot_type.as_str() {
                            "checkpoint" => "Checkpoint",
                            _ => "Disk snapshot",
                        })
                        .size(11.0),
                    )
                    .width(columns[1].available_width())
                    .show_ui(&mut columns[1], |ui| {
                        for (value, label) in
                            [("snapshot", "Disk snapshot"), ("checkpoint", "Checkpoint")]
                        {
                            if ui
                                .selectable_label(
                                    dialog.snapshot_type == value,
                                    RichText::new(label).size(11.0),
                                )
                                .clicked()
                            {
                                dialog.snapshot_type = value.into();
                            }
                        }
                    });
            });
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
            ui.columns(3, |columns| {
                widgets::field_label(&mut columns[0], "Frequency");
                egui::ComboBox::from_id_salt("vmss-frequency")
                    .selected_text(
                        RichText::new(match dialog.frequency.as_str() {
                            "hourly" => "Hourly",
                            "weekly" => "Weekly",
                            _ => "Daily",
                        })
                        .size(11.0),
                    )
                    .width(columns[0].available_width())
                    .show_ui(&mut columns[0], |ui| {
                        for (value, label) in
                            [("hourly", "Hourly"), ("daily", "Daily"), ("weekly", "Weekly")]
                        {
                            if ui
                                .selectable_label(
                                    dialog.frequency == value,
                                    RichText::new(label).size(11.0),
                                )
                                .clicked()
                            {
                                dialog.frequency = value.into();
                            }
                        }
                    });
                if dialog.frequency != "hourly" {
                    widgets::field_label(&mut columns[1], "Hour (0–23)");
                    columns[1].add_enabled(
                        !dialog.submitting,
                        egui::TextEdit::singleline(&mut dialog.hour)
                            .char_limit(2)
                            .margin(egui::Margin::symmetric(8, 5)),
                    );
                }
                widgets::field_label(
                    &mut columns[2],
                    if dialog.frequency == "hourly" {
                        "Minute"
                    } else {
                        "Minute · at"
                    },
                );
                columns[2].add_enabled_ui(!dialog.submitting, |ui| {
                    egui::ComboBox::from_id_salt("vmss-minute")
                        .selected_text(&dialog.minute)
                        .show_ui(ui, |ui| {
                            for minute in ["0", "15", "30", "45"] {
                                ui.selectable_value(&mut dialog.minute, minute.to_owned(), minute);
                            }
                        });
                });
            });
            if dialog.frequency == "weekly" {
                ui.add_space(6.0);
                widgets::field_label(ui, "Days");
                ui.horizontal_wrapped(|ui| {
                    for day in [
                        "monday",
                        "tuesday",
                        "wednesday",
                        "thursday",
                        "friday",
                        "saturday",
                        "sunday",
                    ] {
                        let mut checked = dialog.days.contains(day);
                        let label = {
                            let mut chars = day.chars();
                            chars.next().map(|first| first.to_uppercase().collect::<String>())
                                .unwrap_or_default()
                                + chars.as_str()
                        };
                        if ui
                            .add_enabled(
                                !dialog.submitting,
                                egui::Checkbox::new(&mut checked, RichText::new(label).size(10.0)),
                            )
                            .changed()
                        {
                            if checked {
                                dialog.days.insert(day.into());
                            } else {
                                dialog.days.remove(day);
                            }
                        }
                    }
                });
            }
            ui.add_space(8.0);
            ui.columns(2, |columns| {
                widgets::field_label(&mut columns[0], "Snapshots to retain");
                columns[0].add_enabled(
                    !dialog.submitting,
                    egui::TextEdit::singleline(&mut dialog.retained)
                        .char_limit(4)
                        .margin(egui::Margin::symmetric(8, 5)),
                );
                widgets::field_label(&mut columns[1], "Enabled");
                columns[1].add_enabled(
                    !dialog.submitting,
                    egui::Checkbox::new(
                        &mut dialog.enabled,
                        RichText::new("Run this schedule").size(10.5),
                    ),
                );
            });
            ui.add_space(8.0);
            widgets::field_label(ui, "Virtual machines");
            if dialog.vms.is_empty() {
                ui.label(
                    RichText::new("No virtual machines are available in this pool.")
                        .size(10.0)
                        .color(amber()),
                );
            }
            egui::ScrollArea::vertical()
                .max_height(150.0)
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    for (uuid, name) in &dialog.vms {
                        let mut checked = dialog.selected_vms.contains(uuid);
                        if ui
                            .add_enabled(
                                !dialog.submitting,
                                egui::Checkbox::new(&mut checked, RichText::new(name).size(10.5)),
                            )
                            .changed()
                        {
                            if checked {
                                dialog.selected_vms.insert(uuid.clone());
                            } else {
                                dialog.selected_vms.remove(uuid);
                            }
                        }
                    }
                });
            ui.label(
                RichText::new(
                    "A schedule only runs after it is enabled and has at least one VM. Existing VMs keep their other settings.",
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
                            !dialog.submitting && !dialog.name.trim().is_empty(),
                            widgets::primary_button(if dialog.submitting {
                                "Saving…"
                            } else if editing {
                                "Save schedule"
                            } else {
                                "Create schedule"
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
        actions.push(UiAction::SaveVmssDialog);
    }
    if close {
        actions.push(UiAction::CloseVmssDialog);
    }
}

fn vmss_delete_dialog(ctx: &egui::Context, state: &mut UiState, actions: &mut Vec<UiAction>) {
    let mut close = false;
    let mut confirm = false;
    let Some(dialog) = state.vmss_delete_confirm.as_mut() else {
        return;
    };
    let response = egui::Modal::new(egui::Id::unique("vmss-delete-modal"))
        .frame(theme::modal_frame())
        .show(ctx, |ui| {
            ui.set_width(500.0);
            modal_title(
                ui,
                "Delete snapshot schedule",
                "The schedule is removed and member VMs no longer take scheduled snapshots. Existing snapshots are kept.",
            );
            ui.horizontal(|ui| {
                widgets::dot(ui, red(), 6.0);
                ui.label(RichText::new(&dialog.name).size(11.5).strong());
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
                            egui::Button::new(
                                RichText::new(if dialog.submitting {
                                    "Deleting…"
                                } else {
                                    "Delete schedule"
                                })
                                .size(11.0)
                                .color(red()),
                            )
                            .stroke(egui::Stroke::new(1.0, with_alpha(red(), 120))),
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
        actions.push(UiAction::ConfirmVmssDelete);
    }
    if close {
        actions.push(UiAction::CancelVmssDelete);
    }
}

fn ha_enable_dialog(ctx: &egui::Context, state: &mut UiState, actions: &mut Vec<UiAction>) {
    let mut close = false;
    let mut save = false;
    let Some(dialog) = state.ha_enable_dialog.as_mut() else {
        return;
    };
    let response = egui::Modal::new(egui::Id::unique("ha-enable-modal"))
        .frame(theme::modal_frame())
        .show(ctx, |ui| {
            ui.set_width(580.0);
            modal_title(
                ui,
                "Enable high availability",
                "HA restarts protected VMs when a host fails. It needs shared heartbeat storage.",
            );
            widgets::field_label(ui, "Heartbeat SRs (up to three)");
            egui::ScrollArea::vertical()
                .max_height(160.0)
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    for (uuid, name) in &dialog.srs {
                        let mut checked = dialog.selected.contains(uuid);
                        if ui
                            .add_enabled(
                                !dialog.submitting,
                                egui::Checkbox::new(
                                    &mut checked,
                                    RichText::new(format!("{name} · {uuid}")).size(10.5),
                                ),
                            )
                            .changed()
                        {
                            if checked {
                                dialog.selected.insert(uuid.clone());
                            } else {
                                dialog.selected.remove(uuid);
                            }
                        }
                    }
                });
            ui.add_space(8.0);
            widgets::field_label(ui, "Host failures to tolerate");
            ui.add_enabled(
                !dialog.submitting,
                egui::TextEdit::singleline(&mut dialog.failures)
                    .desired_width(f32::INFINITY)
                    .char_limit(3)
                    .margin(egui::Margin::symmetric(8, 5)),
            );
            ui.label(
                RichText::new(
                    "Must be lower than the number of enabled hosts. The HA plan and VM restart priorities decide which guests restart.",
                )
                .size(9.0)
                .color(dim_fg()),
            );
            ui.add_space(6.0);
            ui.add_enabled(
                !dialog.submitting,
                egui::Checkbox::new(
                    &mut dialog.acknowledged,
                    RichText::new("I understand HA reserves capacity and restarts VMs").size(10.0),
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
                                && !dialog.selected.is_empty(),
                            widgets::primary_button(if dialog.submitting {
                                "Enabling…"
                            } else {
                                "Enable HA"
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
        actions.push(UiAction::SaveHaEnable);
    }
    if close {
        actions.push(UiAction::CloseHaEnable);
    }
}

fn wlb_config_dialog(ctx: &egui::Context, state: &mut UiState, actions: &mut Vec<UiAction>) {
    let mut close = false;
    let mut save = false;
    let Some(dialog) = state.wlb_config_dialog.as_mut() else {
        return;
    };
    let response = egui::Modal::new(egui::Id::unique("wlb-config-modal"))
        .frame(theme::modal_frame())
        .show(ctx, |ui| {
            ui.set_width(600.0);
            modal_title(
                ui,
                "Configure workload balancing",
                "Registers this pool with a Workload Balancing server.",
            );
            widgets::field_label(ui, "WLB server URL");
            ui.add_enabled(
                !dialog.submitting,
                egui::TextEdit::singleline(&mut dialog.url)
                    .desired_width(f32::INFINITY)
                    .hint_text("https://wlb.example.com:443")
                    .char_limit(255)
                    .margin(egui::Margin::symmetric(8, 5)),
            );
            ui.add_space(8.0);
            ui.columns(2, |columns| {
                widgets::field_label(&mut columns[0], "WLB user name");
                columns[0].add_enabled(
                    !dialog.submitting,
                    egui::TextEdit::singleline(&mut dialog.wlb_username)
                        .char_limit(255)
                        .margin(egui::Margin::symmetric(8, 5)),
                );
                widgets::field_label(&mut columns[1], "WLB password");
                columns[1].add_enabled(
                    !dialog.submitting,
                    egui::TextEdit::singleline(&mut dialog.wlb_password)
                        .password(true)
                        .char_limit(255)
                        .margin(egui::Margin::symmetric(8, 5)),
                );
            });
            ui.add_space(8.0);
            ui.columns(2, |columns| {
                widgets::field_label(&mut columns[0], "XenServer user name");
                columns[0].add_enabled(
                    !dialog.submitting,
                    egui::TextEdit::singleline(&mut dialog.xenserver_username)
                        .char_limit(255)
                        .margin(egui::Margin::symmetric(8, 5)),
                );
                widgets::field_label(&mut columns[1], "XenServer password");
                columns[1].add_enabled(
                    !dialog.submitting,
                    egui::TextEdit::singleline(&mut dialog.xenserver_password)
                        .password(true)
                        .char_limit(255)
                        .margin(egui::Margin::symmetric(8, 5)),
                );
            });
            ui.label(
                RichText::new(
                    "Passwords are sent only to the pool initialization call and are never saved by the app.",
                )
                .size(9.0)
                .color(dim_fg()),
            );
            ui.add_space(6.0);
            ui.add_enabled(
                !dialog.submitting,
                egui::Checkbox::new(
                    &mut dialog.acknowledged,
                    RichText::new("I understand workload balancing will contact this server")
                        .size(10.0),
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
                                && !dialog.url.trim().is_empty(),
                            widgets::primary_button(if dialog.submitting {
                                "Configuring…"
                            } else {
                                "Configure WLB"
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
        actions.push(UiAction::SaveWlbConfig);
    }
    if close {
        actions.push(UiAction::CloseWlbConfig);
    }
}

fn pool_confirm_dialog(ctx: &egui::Context, state: &mut UiState, actions: &mut Vec<UiAction>) {
    let mut close = false;
    let mut confirm = false;
    let Some(dialog) = state.pool_confirm.as_mut() else {
        return;
    };
    let (title, detail) = match dialog.action {
        crate::state::PoolConfirmAction::DisableHa => (
            "Disable high availability",
            "VMs will no longer be restarted after a host failure. The HA plan is discarded.",
        ),
        crate::state::PoolConfirmAction::EnableWlb => (
            "Enable workload balancing",
            "The pool will start sending recommendations and optimizations to the configured WLB server.",
        ),
        crate::state::PoolConfirmAction::DisableWlb => (
            "Disable workload balancing",
            "Monitoring stops. The WLB configuration is kept so it can be enabled again.",
        ),
        crate::state::PoolConfirmAction::DeconfigureWlb => (
            "Deconfigure workload balancing",
            "Disables WLB and removes the stored WLB server credentials from the pool.",
        ),
    };
    let response = egui::Modal::new(egui::Id::unique("pool-confirm-modal"))
        .frame(theme::modal_frame())
        .show(ctx, |ui| {
            ui.set_width(520.0);
            modal_title(ui, title, detail);
            ui.add_enabled(
                !dialog.submitting,
                egui::Checkbox::new(
                    &mut dialog.acknowledged,
                    RichText::new("I understand this pool-wide change").size(10.0),
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
                                "Applying…"
                            } else {
                                "Confirm"
                            }),
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
        actions.push(UiAction::ConfirmPoolConfirm);
    }
    if close {
        actions.push(UiAction::CancelPoolConfirm);
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

fn rdp_credentials_dialog(ctx: &egui::Context, state: &mut UiState, actions: &mut Vec<UiAction>) {
    let mut close = false;
    let mut connect = false;
    let Some(dialog) = state.rdp_credentials.as_mut() else {
        return;
    };
    let response = egui::Modal::new(egui::Id::unique("rdp-credentials-modal"))
        .frame(theme::modal_frame())
        .show(ctx, |ui| {
            ui.set_width(580.0);
            modal_title(
                ui,
                "Open embedded RDP console",
                "Connects directly to the guest RDP server.",
            );
            widgets::field_label(ui, "Host");
            ui.add(
                egui::TextEdit::singleline(&mut dialog.host)
                    .desired_width(f32::INFINITY)
                    .hint_text("Guest IP address")
                    .char_limit(255)
                    .margin(egui::Margin::symmetric(8, 5)),
            );
            ui.add_space(8.0);
            ui.columns(2, |columns| {
                widgets::field_label(&mut columns[0], "Port");
                columns[0].add(
                    egui::TextEdit::singleline(&mut dialog.port)
                        .char_limit(5)
                        .margin(egui::Margin::symmetric(8, 5)),
                );
                widgets::field_label(&mut columns[1], "User");
                columns[1].add(
                    egui::TextEdit::singleline(&mut dialog.username)
                        .char_limit(255)
                        .margin(egui::Margin::symmetric(8, 5)),
                );
            });
            ui.add_space(8.0);
            widgets::field_label(ui, "Domain (optional)");
            ui.add(
                egui::TextEdit::singleline(&mut dialog.domain)
                    .desired_width(f32::INFINITY)
                    .char_limit(255)
                    .margin(egui::Margin::symmetric(8, 5)),
            );
            ui.add_space(8.0);
            widgets::field_label(ui, "Password");
            ui.add(
                egui::TextEdit::singleline(&mut dialog.password)
                    .desired_width(f32::INFINITY)
                    .password(true)
                    .char_limit(1024)
                    .margin(egui::Margin::symmetric(8, 5)),
            );
            ui.label(
                RichText::new(
                    "The password is held only for this RDP session and is never saved by the app. The server certificate is accepted for this guest connection.",
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
                    .add(egui::Button::new(RichText::new("Cancel").size(11.0)))
                    .clicked()
                {
                    close = true;
                }
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if ui
                        .add_enabled(
                            !dialog.host.trim().is_empty()
                                && !dialog.username.trim().is_empty()
                                && !dialog.password.is_empty(),
                            widgets::primary_button("Connect"),
                        )
                        .clicked()
                    {
                        connect = true;
                    }
                });
            });
        });
    if response.should_close() {
        close = true;
    }
    if connect {
        actions.push(UiAction::ConnectRdp);
    }
    if close {
        actions.push(UiAction::CloseRdpDialog);
    }
}

fn ssh_credentials_dialog(ctx: &egui::Context, state: &mut UiState, actions: &mut Vec<UiAction>) {
    let mut close = false;
    let mut connect = false;
    let Some(dialog) = state.ssh_credentials.as_mut() else {
        return;
    };
    let response = egui::Modal::new(egui::Id::unique("ssh-credentials-modal"))
        .frame(theme::modal_frame())
        .show(ctx, |ui| {
            ui.set_width(560.0);
            modal_title(
                ui,
                "Open SSH text console",
                "Connects directly to the guest over SSH, separately from the VNC console.",
            );
            widgets::field_label(ui, "Host");
            ui.add(
                egui::TextEdit::singleline(&mut dialog.host)
                    .desired_width(f32::INFINITY)
                    .hint_text("Guest IP address")
                    .char_limit(255)
                    .margin(egui::Margin::symmetric(8, 5)),
            );
            ui.add_space(8.0);
            ui.columns(2, |columns| {
                widgets::field_label(&mut columns[0], "Port");
                columns[0].add(
                    egui::TextEdit::singleline(&mut dialog.port)
                        .char_limit(5)
                        .margin(egui::Margin::symmetric(8, 5)),
                );
                widgets::field_label(&mut columns[1], "User");
                columns[1].add(
                    egui::TextEdit::singleline(&mut dialog.username)
                        .char_limit(255)
                        .margin(egui::Margin::symmetric(8, 5)),
                );
            });
            ui.add_space(8.0);
            widgets::field_label(ui, "Password");
            ui.add(
                egui::TextEdit::singleline(&mut dialog.password)
                    .desired_width(f32::INFINITY)
                    .password(true)
                    .char_limit(1024)
                    .margin(egui::Margin::symmetric(8, 5)),
            );
            ui.label(
                RichText::new(
                    "The password is held only for this SSH session and is never saved by the app.",
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
                    .add(egui::Button::new(RichText::new("Cancel").size(11.0)))
                    .clicked()
                {
                    close = true;
                }
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if ui
                        .add_enabled(
                            !dialog.host.trim().is_empty()
                                && !dialog.username.trim().is_empty()
                                && !dialog.password.is_empty(),
                            widgets::primary_button("Connect"),
                        )
                        .clicked()
                    {
                        connect = true;
                    }
                });
            });
        });
    if response.should_close() {
        close = true;
    }
    if connect {
        actions.push(UiAction::ConnectSsh);
    }
    if close {
        actions.push(UiAction::CloseSshDialog);
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
                    let free_gib = dialog
                        .sr_uuid
                        .as_ref()
                        .and_then(|uuid| dialog.srs.iter().find(|(sr_uuid, _, _)| sr_uuid == uuid))
                        .map(|(_, _, free)| (*free / (1024 * 1024 * 1024)).max(1))
                        .unwrap_or(1024)
                        .min(65536);
                    let mut add_size = dialog
                        .size_gib
                        .trim()
                        .parse::<u64>()
                        .unwrap_or(10)
                        .clamp(1, free_gib);
                    if ui
                        .add_enabled(
                            !dialog.submitting,
                            egui::Slider::new(&mut add_size, 1..=free_gib)
                                .suffix(" GiB")
                                .logarithmic(true),
                        )
                        .changed()
                    {
                        dialog.size_gib = add_size.to_string();
                    }
                    ui.add_space(4.0);
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
