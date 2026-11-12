use super::wizard::{Frame, Outcome, Step};
use crate::{
    state::{UiAction, UiState, connection_inventory, find_connection, is_read_only},
    theme, widgets,
};
use xen_core::{
    appliance::{Format, source_kind},
    workspace::WorkspaceView,
};

pub fn show(
    ctx: &egui::Context,
    state: &mut UiState,
    workspace: &WorkspaceView,
    actions: &mut Vec<UiAction>,
) {
    if let Some(dialog) = state.file_interchange.as_mut() {
        egui::Modal::new(egui::Id::unique("file-interchange"))
            .frame(theme::modal_frame())
            .show(ctx, |ui| {
                ui.set_width(600.0);
                ui.heading(match dialog.command {
                    "ImportServerList" => "Import Server List",
                    "ExportServerList" => "Export Server List",
                    "ImportSearchCommand" => "Import Search",
                    _ => "Export Search",
                });
                ui.label(if dialog.command.contains("ServerList") {
                    "XCP-ng Center XML connection list. Passwords are never included."
                } else {
                    "Xen Desktop saved searches (.xensearch)."
                });
                widgets::field_label(ui, "File name on this computer");
                ui.add(egui::TextEdit::singleline(&mut dialog.path).desired_width(f32::INFINITY));
                if let Some(error) = &dialog.error {
                    ui.colored_label(theme::red(), error);
                }
                ui.horizontal(|ui| {
                    if ui.button("Cancel").clicked() {
                        actions.push(UiAction::CloseFileInterchange);
                    }
                    if ui
                        .add_enabled(
                            !dialog.path.trim().is_empty(),
                            widgets::primary_button("Finish"),
                        )
                        .clicked()
                    {
                        actions.push(UiAction::SubmitFileInterchange);
                    }
                });
            });
    }
    let Some(d) = state.xva_dialog.as_mut() else {
        return;
    };
    let connection = find_connection(workspace, &d.connection_id);
    let inventory = connection.and_then(connection_inventory);
    let available = inventory.is_some() && connection.is_some_and(|c| !is_read_only(c));
    let export = d.vm_uuid.is_some();
    let kind = source_kind(std::path::Path::new(&d.path));
    let ovf = kind == Some("OVF/OVA");
    let image = kind == Some("Disk image");
    let mut titles = if export {
        vec!["Select VMs", "Format", "Location", "Options", "Finish"]
    } else {
        vec!["Source"]
    };
    if !export {
        if ovf {
            titles.push("Security");
            if d.envelope.as_ref().is_some_and(|e| !e.eulas.is_empty()) {
                titles.push("EULA");
            }
        }
        if image {
            titles.push("VM configuration");
        }
        titles.extend(["Home server", "Storage", "Networking"]);
        if image {
            titles.push("Boot options");
        } else if ovf {
            titles.push("Options");
        }
        titles.push("Finish");
    }
    let steps: Vec<_> = titles
        .iter()
        .map(|title| Step {
            title,
            description: match *title {
                "Source" => "Select an XVA, OVF/OVA, VHD, VMDK or raw disk image.",
                "Storage" => "Choose the storage repository for each disk.",
                "Networking" => "Map the imported virtual interfaces to guest networks.",
                _ => "",
            },
        })
        .collect();
    let mut frame = std::mem::take(&mut d.frame);
    let outcome = Frame {
        id: "import-export-wizard",
        title: if export { "Export" } else { "Import" },
        steps: &steps,
        busy: d.submitting,
        dirty: !d.path.is_empty(),
        error: d.error.as_deref(),
    }
    .show(ctx, &mut frame, |ui, step| {
        if !available {
            return Some("Connect with write access to transfer a VM.");
        }
        let title = titles[step];
        match title {
            "Source" => {
                widgets::field_label(ui, "File name on this computer");
                if ui
                    .add(egui::TextEdit::singleline(&mut d.path).desired_width(f32::INFINITY))
                    .changed()
                {
                    d.envelope = None;
                    d.accepted_eula = false;
                    d.disk_storage.clear();
                    d.networks.clear();
                }
                let kind = source_kind(std::path::Path::new(&d.path));
                ui.label(format!("Detected format: {}", kind.unwrap_or("Unknown")));
                widgets::field_label(ui, "Imported name");
                ui.text_edit_singleline(&mut d.name);
                if kind == Some("OVF/OVA") {
                    if ui.button("Read appliance").clicked() {
                        match xen_core::appliance::inspect(std::path::Path::new(&d.path)) {
                            Ok(e) => {
                                if let Some(vm) = e.machines.first() {
                                    d.name = vm.name.clone();
                                }
                                d.envelope = Some(e);
                            }
                            Err(e) => {
                                ui.colored_label(theme::red(), e.to_string());
                            }
                        }
                    }
                    if let Some(e) = &d.envelope {
                        ui.label(format!(
                            "{} virtual machine(s), {} disk(s), {} interface(s)",
                            e.machines.len(),
                            e.machines.iter().map(|v| v.disks.len()).sum::<usize>(),
                            e.machines.iter().map(|v| v.networks.len()).sum::<usize>()
                        ));
                    } else {
                        return Some("Read the appliance before continuing.");
                    }
                }
                if kind.is_none()
                    || !std::path::Path::new(&d.path).is_file()
                    || d.name.trim().is_empty()
                {
                    return Some("Select an existing supported file and enter an imported name.");
                }
            }
            "Select VMs" => {
                if let Some(inventory) = inventory {
                    if d.export_targets.is_empty()
                        && let Some(uuid) = &d.vm_uuid
                        && let Some(vm) = inventory
                            .vms
                            .iter()
                            .chain(&inventory.templates)
                            .chain(&inventory.snapshots)
                            .find(|v| &v.uuid == uuid)
                    {
                        d.export_targets.push((
                            d.connection_id.clone(),
                            uuid.clone(),
                            vm.name.clone(),
                        ));
                    }
                    for vm in inventory
                        .vms
                        .iter()
                        .chain(&inventory.templates)
                        .chain(&inventory.snapshots)
                    {
                        let mut selected =
                            d.export_targets.iter().any(|(_, uuid, _)| uuid == &vm.uuid);
                        if ui
                            .add_enabled(
                                vm.power_state == "Halted"
                                    && vm.allowed_operations.iter().any(|o| o == "export"),
                                egui::Checkbox::new(&mut selected, &vm.name),
                            )
                            .changed()
                        {
                            d.export_targets.retain(|(_, uuid, _)| uuid != &vm.uuid);
                            if selected {
                                d.export_targets.push((
                                    d.connection_id.clone(),
                                    vm.uuid.clone(),
                                    vm.name.clone(),
                                ));
                            }
                        }
                    }
                }
                if d.export_targets.is_empty() {
                    return Some("Select at least one halted VM.");
                }
            }
            "Format" => {
                ui.add_enabled_ui(d.export_targets.len() == 1, |ui| {
                    ui.radio_value(&mut d.format, Format::Xva, "XVA (single VM)");
                });
                ui.radio_value(&mut d.format, Format::Ovf, "OVF (descriptor and VHD files)");
                ui.radio_value(&mut d.format, Format::Ova, "OVA (single archive)");
                if d.export_targets.len() > 1 && d.format == Format::Xva {
                    d.format = Format::Ova;
                }
                let path = std::path::Path::new(&d.path).with_extension(d.format.extension());
                d.path = path.to_string_lossy().into_owned();
            }
            "Location" => {
                widgets::field_label(ui, "Destination file on this computer");
                ui.add(egui::TextEdit::singleline(&mut d.path).desired_width(f32::INFINITY));
                ui.label("Choose a new filename. Existing files are preserved.");
                if d.path.trim().is_empty() || std::path::Path::new(&d.path).exists() {
                    return Some("Choose a new destination file.");
                }
            }
            "Security" => {
                ui.checkbox(
                    &mut d.manifest,
                    "Verify manifest when present (SHA-1 / SHA-256)",
                );
                ui.add_enabled(
                    false,
                    egui::Checkbox::new(&mut false, "Verify signature — Excluded from this port"),
                );
            }
            "EULA" => {
                if let Some(e) = &d.envelope {
                    for eula in &e.eulas {
                        ui.label(eula);
                    }
                }
                ui.checkbox(&mut d.accepted_eula, "I accept the license agreement(s)");
                if !d.accepted_eula {
                    return Some("Accept the license agreement(s) to continue.");
                }
            }
            "VM configuration" => {
                widgets::field_label(ui, "VM name");
                ui.text_edit_singleline(&mut d.name);
                ui.horizontal(|ui| {
                    ui.label("vCPUs");
                    ui.add(egui::DragValue::new(&mut d.vcpus).range(1..=128));
                });
                ui.horizontal(|ui| {
                    ui.label("Memory (MiB)");
                    ui.add(egui::DragValue::new(&mut d.memory_mib).range(16..=1048576));
                });
                if d.name.trim().is_empty() {
                    return Some("Enter a VM name.");
                }
            }
            "Home server" => {
                if let Some(inventory) = inventory {
                    for host in inventory.hosts.iter().filter(|h| h.enabled) {
                        ui.radio_value(&mut d.host_uuid, Some(host.uuid.clone()), &host.name);
                    }
                }
                if d.host_uuid.is_none() {
                    return Some("Select a home server.");
                }
            }
            "Storage" => {
                let count = d.envelope.as_ref().map_or(1, |e| {
                    e.machines
                        .iter()
                        .map(|v| v.disks.len())
                        .sum::<usize>()
                        .max(1)
                });
                d.disk_storage
                    .resize(count, d.sr_uuid.clone().unwrap_or_default());
                for (i, sr) in d.disk_storage.iter_mut().enumerate() {
                    ui.label(format!("Disk {}", i + 1));
                    egui::ComboBox::from_id_salt(("import-sr", i))
                        .selected_text(
                            inventory
                                .and_then(|inv| inv.storage.iter().find(|s| &s.uuid == sr))
                                .map_or("Select storage…", |s| s.name.as_str()),
                        )
                        .width(400.0)
                        .show_ui(ui, |ui| {
                            if let Some(inv) = inventory {
                                for s in inv.storage.iter().filter(|s| {
                                    s.content_type != "iso"
                                        && !matches!(s.storage_type.as_str(), "iso" | "udev")
                                        && s.pbd_attached > 0
                                }) {
                                    ui.selectable_value(sr, s.uuid.clone(), &s.name);
                                }
                            }
                        });
                }
                d.sr_uuid = d.disk_storage.first().cloned().filter(|s| !s.is_empty());
                if d.disk_storage.iter().any(|s| s.is_empty()) {
                    return Some("Choose storage for every disk.");
                }
            }
            "Networking" => {
                let count = d
                    .envelope
                    .as_ref()
                    .map_or(1, |e| e.machines.iter().map(|v| v.networks.len()).sum());
                d.networks.resize(count, d.network_uuid.clone());
                for (i, network) in d.networks.iter_mut().enumerate() {
                    ui.label(format!("Interface {}", i + 1));
                    egui::ComboBox::from_id_salt(("import-network", i))
                        .selected_text(
                            inventory
                                .and_then(|inv| {
                                    inv.networks
                                        .iter()
                                        .find(|n| Some(&n.uuid) == network.as_ref())
                                })
                                .map_or("None", |n| n.name.as_str()),
                        )
                        .width(400.0)
                        .show_ui(ui, |ui| {
                            ui.selectable_value(network, None, "None");
                            if let Some(inv) = inventory {
                                for n in inv.networks.iter().filter(|n| n.bridge != "xenapi") {
                                    ui.selectable_value(network, Some(n.uuid.clone()), &n.name);
                                }
                            }
                        });
                }
                d.network_uuid = d.networks.first().cloned().flatten();
            }
            "Boot options" => {
                ui.radio_value(&mut d.uefi, false, "BIOS");
                ui.radio_value(&mut d.uefi, true, "UEFI");
            }
            "Options" => {
                if export {
                    ui.add_enabled_ui(d.format != Format::Xva, |ui| {
                        ui.checkbox(&mut d.manifest, "Create manifest (SHA-256)");
                    });
                    ui.add_enabled(
                        false,
                        egui::Checkbox::new(&mut false, "Sign — Excluded from this port"),
                    );
                    ui.add_enabled(
                        false,
                        egui::Checkbox::new(&mut false, "Encrypt — Excluded from this port"),
                    );
                } else {
                    ui.add_enabled(
                        false,
                        egui::Checkbox::new(&mut false, "Run OS fixup — Not implemented yet"),
                    );
                }
            }
            "Finish" => {
                ui.label(format!("File: {}", d.path));
                if export {
                    for (_, _, name) in &d.export_targets {
                        ui.label(name);
                    }
                    ui.label(format!("Format: {}", d.format.extension().to_uppercase()));
                } else {
                    ui.label(format!("Imported name: {}", d.name));
                    ui.checkbox(&mut d.start, "Start VM(s) after import");
                }
                ui.label("Transfer progress and Cancel are available in Events.");
            }
            _ => {}
        }
        None
    });
    d.frame = frame;
    match outcome {
        Outcome::Finish => actions.push(UiAction::SubmitXvaTransfer),
        Outcome::Cancel => actions.push(UiAction::CloseXvaDialog),
        Outcome::None => {}
    }
}
