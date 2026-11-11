use egui::{Align, Layout, RichText};
use xen_core::workspace::WorkspaceView;

use crate::state::{LoadState, UiAction, UiState, connection_inventory, find_connection};
use crate::{format, theme, widgets};

const STEPS: [&str; 6] = [
    "Source",
    "Name",
    "Resources",
    "Storage & media",
    "Network",
    "Review",
];

pub fn show(
    ctx: &egui::Context,
    state: &mut UiState,
    workspace: &WorkspaceView,
    actions: &mut Vec<UiAction>,
) {
    let Some(wizard) = state.new_vm.as_mut() else {
        return;
    };
    let Some(inventory) =
        find_connection(workspace, &wizard.connection_id).and_then(connection_inventory)
    else {
        egui::Modal::new(egui::Id::unique("new-vm-offline")).show(ctx, |ui| {
            ui.label("The connection is offline. Reconnect to create a VM.");
            if ui.button("Close").clicked() {
                actions.push(UiAction::CloseNewVmWizard);
            }
        });
        return;
    };
    let mut close = false;
    let response = egui::Modal::new(egui::Id::unique("new-vm-wizard"))
        .frame(theme::modal_frame())
        .show(ctx, |ui| {
            ui.set_width(680.0);
            ui.heading("New virtual machine");
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                for (index, label) in STEPS.iter().enumerate() {
                    if index > 0 {
                        ui.label("›");
                    }
                    if index == wizard.step {
                        ui.label(RichText::new(*label).strong().color(theme::accent()));
                    } else {
                        ui.label(RichText::new(*label).color(theme::muted_fg()));
                    }
                }
            });
            ui.separator();
            let mut error = None;
            egui::ScrollArea::vertical()
                .max_height((ctx.content_rect().height() - 250.0).max(180.0))
                .show(ui, |ui| {
                    ui.add_enabled_ui(!wizard.submitting, |ui| {
                        error = page(ui, wizard, inventory, actions);
                    });
                });
            ui.add_space(10.0);
            if let Some(message) = &wizard.error {
                ui.colored_label(theme::red(), message);
            }
            if let Some(message) = error {
                ui.label(RichText::new(message).color(theme::muted_fg()));
            }
            ui.separator();
            ui.horizontal(|ui| {
                if ui
                    .add_enabled(!wizard.submitting, egui::Button::new("Cancel"))
                    .clicked()
                {
                    close = true;
                }
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if wizard.step == STEPS.len() - 1 {
                        if ui
                            .add_enabled(!wizard.submitting, widgets::primary_button("Create VM"))
                            .clicked()
                        {
                            actions.push(UiAction::CreateNewVm);
                        }
                    } else if ui
                        .add_enabled(
                            !wizard.submitting && error.is_none(),
                            widgets::primary_button("Next"),
                        )
                        .clicked()
                    {
                        wizard.step += 1;
                        wizard.error = None;
                    }
                    if ui
                        .add_enabled(
                            !wizard.submitting && wizard.step > 0,
                            egui::Button::new("Back"),
                        )
                        .clicked()
                    {
                        wizard.step -= 1;
                        wizard.error = None;
                    }
                    if wizard.submitting {
                        ui.spinner();
                        ui.label("Queuing creation…");
                    }
                });
            });
        });
    if (close || response.should_close()) && !wizard.submitting {
        actions.push(UiAction::CloseNewVmWizard);
    }
}

fn page(
    ui: &mut egui::Ui,
    wizard: &mut crate::state::NewVmWizard,
    inventory: &xen_core::xapi::Inventory,
    actions: &mut Vec<UiAction>,
) -> Option<&'static str> {
    let mut error = None;
    let draft = &mut wizard.draft;
    match wizard.step {
        0 => {
            widgets::field_label(ui, "Source");
            let selected = inventory
                .templates
                .iter()
                .chain(&inventory.snapshots)
                .find(|vm| vm.uuid == draft.source.uuid())
                .map_or("Select a template or snapshot", |vm| vm.name.as_str());
            egui::ComboBox::from_id_salt("new-vm-template")
                .selected_text(selected)
                .width(620.0)
                .show_ui(ui, |ui| {
                    for template in inventory
                        .templates
                        .iter()
                        .chain(&inventory.snapshots)
                        .filter(|vm| vm.allowed_operations.iter().any(|op| op == "clone"))
                    {
                        if ui
                            .selectable_label(template.uuid == draft.source.uuid(), &template.name)
                            .clicked()
                        {
                            actions.push(UiAction::SelectNewVmSource(
                                if template.kind == "snapshot" {
                                    xen_core::provisioning::VmCreationSource::Snapshot(
                                        template.uuid.clone(),
                                    )
                                } else {
                                    xen_core::provisioning::VmCreationSource::Template(
                                        template.uuid.clone(),
                                    )
                                },
                            ));
                        }
                    }
                });
            match &wizard.template {
                LoadState::Ready(details) => {
                    ui.add_space(12.0);
                    ui.label(&details.description);
                    ui.label(format!(
                        "{} vCPUs · {} memory · {} existing disks",
                        details.vcpus_at_startup,
                        format::bytes(details.memory_dynamic_max),
                        details.disks.iter().filter(|disk| !disk.is_cd).count()
                    ));
                }
                LoadState::Error(message) => {
                    ui.colored_label(theme::red(), message);
                    if ui.button("Retry source").clicked() {
                        actions.push(UiAction::SelectNewVmSource(draft.source.clone()));
                    }
                    error = Some("Load the source before continuing.");
                }
                _ => {
                    ui.spinner();
                    ui.label("Loading source…");
                    error = Some("Wait for the source to load.");
                }
            }
        }
        1 => {
            widgets::field_label(ui, "VM name");
            ui.add(
                egui::TextEdit::singleline(&mut draft.name)
                    .char_limit(255)
                    .desired_width(f32::INFINITY),
            );
            ui.add_space(12.0);
            widgets::field_label(ui, "Description");
            ui.add(
                egui::TextEdit::multiline(&mut draft.description)
                    .char_limit(xen_core::properties::MAX_DESCRIPTION_CHARS)
                    .desired_rows(4)
                    .desired_width(f32::INFINITY),
            );
            if draft.name.trim().is_empty() {
                error = Some("Enter a VM name.");
            }
        }
        2 => {
            widgets::field_label(ui, "Maximum vCPUs");
            let previous_max = draft.vcpus_max;
            ui.add(egui::DragValue::new(&mut draft.vcpus_max).range(1..=256));
            if draft.vcpus_max != previous_max {
                draft.vcpus_at_startup = if draft.vcpus_at_startup == previous_max {
                    draft.vcpus_max
                } else {
                    draft.vcpus_at_startup.min(draft.vcpus_max)
                };
            }
            ui.add_space(8.0);
            widgets::field_label(ui, "Initial vCPUs");
            let hotplug = wizard
                .template
                .ready()
                .is_some_and(|source| source.supports_vcpu_hotplug);
            if !hotplug {
                draft.vcpus_at_startup = draft.vcpus_max;
            }
            ui.add_enabled(
                hotplug,
                egui::DragValue::new(&mut draft.vcpus_at_startup).range(1..=draft.vcpus_max),
            );
            ui.add_space(8.0);
            widgets::field_label(ui, "Cores per socket");
            let cores = draft.cores_per_socket.unwrap_or(1);
            egui::ComboBox::from_id_salt("new-vm-topology")
                .selected_text(cores.to_string())
                .show_ui(ui, |ui| {
                    for value in 1..=draft.vcpus_max {
                        if xen_core::vm::validate_cpu_topology(draft.vcpus_max, value, 0).is_ok() {
                            ui.selectable_value(
                                &mut draft.cores_per_socket,
                                Some(value),
                                format!(
                                    "{value} cores / socket · {} sockets",
                                    draft.vcpus_max / value
                                ),
                            );
                        }
                    }
                });
            if xen_core::vm::validate_cpu_topology(draft.vcpus_max, cores, 0).is_err() {
                error = Some(
                    "Choose a topology that divides the CPU maximum and uses at most 16 sockets.",
                );
            }
            ui.label(RichText::new("Physical host limits are checked for the selected storage and home server when creating the VM. Runtime CPU hot-add requires guest support.").small().color(theme::muted_fg()));
            ui.add_space(12.0);
            widgets::field_label(ui, "Memory (MiB)");
            let minimum = wizard
                .template
                .ready()
                .map_or(64, |vm| vm.memory_static_min.div_ceil(1024 * 1024).max(1));
            let maximum = inventory
                .hosts
                .iter()
                .map(|host| host.memory_total / (1024 * 1024))
                .max()
                .unwrap_or(minimum)
                .max(minimum);
            ui.add(
                egui::DragValue::new(&mut draft.memory_mib)
                    .range(minimum..=maximum)
                    .speed(64),
            );
            ui.add_space(12.0);
            widgets::field_label(ui, "Home server");
            let selected = draft
                .affinity_host_uuid
                .as_ref()
                .and_then(|uuid| inventory.hosts.iter().find(|host| &host.uuid == uuid))
                .map_or("Automatic placement", |host| host.name.as_str());
            egui::ComboBox::from_id_salt("new-vm-home")
                .selected_text(selected)
                .width(620.0)
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut draft.affinity_host_uuid, None, "Automatic placement");
                    for host in inventory.hosts.iter().filter(|host| host.enabled) {
                        ui.selectable_value(
                            &mut draft.affinity_host_uuid,
                            Some(host.uuid.clone()),
                            &host.name,
                        );
                    }
                });
        }
        3 => {
            widgets::field_label(ui, "Storage repository");
            let selected = inventory
                .storage
                .iter()
                .find(|sr| sr.uuid == draft.sr_uuid)
                .map_or("Select storage", |sr| sr.name.as_str());
            egui::ComboBox::from_id_salt("new-vm-sr")
                .selected_text(selected)
                .width(620.0)
                .show_ui(ui, |ui| {
                    for sr in inventory.storage.iter().filter(|sr| {
                        sr.content_type != "iso"
                            && !matches!(sr.storage_type.as_str(), "iso" | "udev")
                            && sr.pbd_attached > 0
                    }) {
                        ui.selectable_value(
                            &mut draft.sr_uuid,
                            sr.uuid.clone(),
                            format!(
                                "{} · {} free",
                                sr.name,
                                format::bytes(sr.total.saturating_sub(sr.used))
                            ),
                        );
                    }
                });
            ui.add_space(12.0);
            let existing = wizard
                .template
                .ready()
                .map_or(0, |vm| vm.disks.iter().filter(|disk| !disk.is_cd).count());
            if existing > 0 {
                ui.label(format!("Keep the source's {existing} existing disks and place their copies on the selected storage."));
            }
            if let Some(template) = wizard.template.ready()
                && existing == 0
            {
                for disk in &template.provision_disks {
                    ui.label(format!(
                        "Installation disk {} · {}",
                        disk.device,
                        format::bytes(disk.size)
                    ));
                }
            }
            let mut add = draft.new_disk_gib.is_some();
            let installation = wizard.template.ready().is_some_and(|vm| {
                vm.disks.iter().all(|disk| disk.is_cd) && !vm.provision_disks.is_empty()
            });
            if ui
                .checkbox(
                    &mut add,
                    if installation {
                        "Customize first installation disk size"
                    } else {
                        "Add a new disk"
                    },
                )
                .changed()
            {
                draft.new_disk_gib = add.then_some(20);
            }
            if let Some(size) = draft.new_disk_gib.as_mut() {
                ui.horizontal(|ui| {
                    ui.add(egui::DragValue::new(size).range(1..=1_048_576));
                    ui.label("GiB");
                });
            }
            ui.add_space(12.0);
            widgets::field_label(ui, "Installation ISO");
            let selected = wizard
                .template
                .ready()
                .and_then(|vm| {
                    vm.isos
                        .iter()
                        .find(|iso| Some(&iso.uuid) == draft.iso_uuid.as_ref())
                })
                .map_or("Keep source media", |iso| iso.name.as_str());
            egui::ComboBox::from_id_salt("new-vm-iso")
                .selected_text(selected)
                .width(620.0)
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut draft.iso_uuid, None, "Keep source media");
                    if let Some(details) = wizard.template.ready() {
                        for iso in &details.isos {
                            ui.selectable_value(
                                &mut draft.iso_uuid,
                                Some(iso.uuid.clone()),
                                format!("{} · {}", iso.name, iso.sr_name),
                            );
                        }
                    }
                });
            if draft.sr_uuid.is_empty() {
                error = Some("Select storage for the VM disks.");
            }
        }
        4 => {
            widgets::field_label(ui, "Guest network");
            let selected = draft
                .network_uuid
                .as_ref()
                .and_then(|uuid| {
                    inventory
                        .networks
                        .iter()
                        .find(|network| &network.uuid == uuid)
                })
                .map_or("No network interfaces", |network| network.name.as_str());
            egui::ComboBox::from_id_salt("new-vm-network")
                .selected_text(selected)
                .width(620.0)
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut draft.network_uuid, None, "No network interfaces");
                    for network in inventory
                        .networks
                        .iter()
                        .filter(|network| network.bridge != "xenapi")
                    {
                        ui.selectable_value(
                            &mut draft.network_uuid,
                            Some(network.uuid.clone()),
                            &network.name,
                        );
                    }
                });
            ui.add_space(12.0);
            ui.label("Choose one guest network or create the VM without network interfaces. This selection replaces the source's interfaces.");
        }
        _ => {
            let template = wizard.template.ready();
            let disk_count =
                template.map_or(0, |vm| vm.disks.iter().filter(|disk| !disk.is_cd).count());
            let disks = if let Some(template) = template.filter(|vm| {
                vm.disks.iter().all(|disk| disk.is_cd) && !vm.provision_disks.is_empty()
            }) {
                format!(
                    "{} installation disks{}",
                    template.provision_disks.len(),
                    draft
                        .new_disk_gib
                        .map_or_else(String::new, |size| format!(", first disk {size} GiB"))
                )
            } else {
                format!(
                    "{disk_count} existing source disks{}",
                    draft
                        .new_disk_gib
                        .map_or_else(String::new, |size| format!(" + {size} GiB new disk"))
                )
            };
            for (label, value) in [
                (
                    "Source",
                    template.map_or_else(String::new, |vm| vm.name.clone()),
                ),
                ("Name", draft.name.clone()),
                ("Description", draft.description.clone()),
                ("Maximum vCPUs", draft.vcpus_max.to_string()),
                ("Initial vCPUs", draft.vcpus_at_startup.to_string()),
                (
                    "Cores per socket",
                    draft
                        .cores_per_socket
                        .map_or_else(|| "Inherit source".into(), |cores| cores.to_string()),
                ),
                ("Memory", format!("{} MiB", draft.memory_mib)),
                (
                    "Storage",
                    inventory
                        .storage
                        .iter()
                        .find(|sr| sr.uuid == draft.sr_uuid)
                        .map_or_else(|| "Unavailable".into(), |sr| sr.name.clone()),
                ),
                ("Disks", disks),
                (
                    "Network",
                    draft
                        .network_uuid
                        .as_ref()
                        .and_then(|uuid| {
                            inventory
                                .networks
                                .iter()
                                .find(|network| &network.uuid == uuid)
                        })
                        .map_or_else(
                            || "No network interfaces".into(),
                            |network| network.name.clone(),
                        ),
                ),
                (
                    "Installation ISO",
                    template
                        .and_then(|vm| {
                            vm.isos
                                .iter()
                                .find(|iso| Some(&iso.uuid) == draft.iso_uuid.as_ref())
                        })
                        .map_or_else(|| "Keep source media".into(), |iso| iso.name.clone()),
                ),
                (
                    "Home server",
                    draft
                        .affinity_host_uuid
                        .as_ref()
                        .and_then(|uuid| inventory.hosts.iter().find(|host| &host.uuid == uuid))
                        .map_or_else(|| "Automatic placement".into(), |host| host.name.clone()),
                ),
                ("Power state after creation", "Halted".into()),
            ] {
                widgets::kv(ui, label, &value, false);
            }
            ui.add_space(8.0);
            ui.label("Create the VM with these settings. Follow its progress in Activity & tasks.");
        }
    }
    error
}
