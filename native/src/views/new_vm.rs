use egui::RichText;
use xen_core::provisioning::{CreateDisk, CreateVif, InstallationMedia};
use xen_core::workspace::WorkspaceView;

use crate::state::{LoadState, UiAction, UiState, connection_inventory, find_connection};
use crate::{format, theme, widgets};

use super::wizard::{Frame, Outcome, Step};

#[derive(Clone, Copy, PartialEq, Eq)]
enum Page {
    Template,
    Name,
    CopyBios,
    InstallationMedia,
    HomeServer,
    CpuMemory,
    Storage,
    Networking,
    GuestSetup,
    Finish,
}

impl Page {
    fn step(self) -> Step {
        let (title, description) = match self {
            Self::Template => ("Template", "Select a template on which to base the new VM."),
            Self::CopyBios => (
                "Copy BIOS strings",
                "Select the server whose BIOS strings will be copied to the VM.",
            ),
            Self::Name => ("Name", "Enter a name and description for the new VM."),
            Self::InstallationMedia => (
                "Installation Media",
                "Choose the installation media for the operating system.",
            ),
            Self::HomeServer => ("Home Server", "Choose a home server for the new VM."),
            Self::CpuMemory => (
                "CPU & Memory",
                "Allocate processors and memory to the new VM.",
            ),
            Self::Storage => ("Storage", "Configure the virtual disks for the new VM."),
            Self::Networking => ("Networking", "Configure the virtual network interfaces."),
            Self::GuestSetup => (
                "Guest Setup",
                "Configure networking and login inside the supported guest.",
            ),
            Self::Finish => ("Finish", "Review the settings and create the new VM."),
        };
        Step { title, description }
    }
}

fn pages(installed: bool, guest: bool) -> Vec<Page> {
    let mut pages = vec![Page::Template, Page::Name];
    if !installed {
        pages.push(Page::InstallationMedia);
    }
    pages.extend([
        Page::HomeServer,
        Page::CpuMemory,
        Page::Storage,
        Page::Networking,
    ]);
    if guest {
        pages.push(Page::GuestSetup);
    }
    pages.push(Page::Finish);
    pages
}

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
    if wizard.draft.sr_uuid.is_empty() {
        wizard.draft.sr_uuid = inventory
            .storage
            .iter()
            .find(|sr| {
                sr.content_type != "iso"
                    && !matches!(sr.storage_type.as_str(), "udev" | "iso")
                    && sr.pbd_attached > 0
            })
            .map(|sr| sr.uuid.clone())
            .unwrap_or_default();
    }
    if let Some(disks) = &mut wizard.draft.disks {
        for disk in disks.iter_mut().filter(|disk| disk.sr_uuid.is_empty()) {
            disk.sr_uuid = wizard.draft.sr_uuid.clone();
        }
    }
    let template = wizard.template.ready();
    let mut pages = pages(
        template.is_some_and(|vm| vm.disks.iter().any(|disk| !disk.is_cd)),
        template.is_some_and(|vm| vm.guest_support.enabled),
    );
    if !wizard.copy_bios {
        wizard.draft.copy_bios_from = None;
    }
    if wizard.copy_bios {
        pages.insert(1, Page::CopyBios);
    }
    let steps: Vec<_> = pages.iter().map(|page| page.step()).collect();
    let mut frame = std::mem::take(&mut wizard.frame);
    let old_step = frame.step;
    let error = wizard.error.clone();
    let outcome = Frame {
        id: "new-vm-wizard",
        title: "New VM",
        steps: &steps,
        busy: wizard.submitting,
        dirty: !wizard.draft.name.is_empty() || frame.step > 0,
        error: error.as_deref(),
    }
    .show(ctx, &mut frame, |ui, step| {
        page(ui, wizard, pages[step], inventory, actions)
    });
    if frame.step != old_step {
        wizard.error = None;
    }
    wizard.frame = frame;
    match outcome {
        Outcome::Cancel => actions.push(UiAction::CloseNewVmWizard),
        Outcome::Finish => actions.push(UiAction::CreateNewVm),
        Outcome::None => {}
    }
}

fn page(
    ui: &mut egui::Ui,
    wizard: &mut crate::state::NewVmWizard,
    current: Page,
    inventory: &xen_core::xapi::Inventory,
    actions: &mut Vec<UiAction>,
) -> Option<&'static str> {
    let mut error = None;
    let draft = &mut wizard.draft;
    match current {
        Page::Template => {
            ui.add(
                egui::TextEdit::singleline(&mut wizard.template_search)
                    .hint_text("Search templates")
                    .desired_width(f32::INFINITY),
            );
            let search = wizard.template_search.to_lowercase();
            egui::ScrollArea::vertical()
                .id_salt("template-list")
                .max_height(210.0)
                .show(ui, |ui| {
                    for group in ["Windows", "Linux", "Other", "Snapshots"] {
                        let mut templates: Vec<_> = inventory
                            .templates
                            .iter()
                            .chain(&inventory.snapshots)
                            .filter(|vm| vm.allowed_operations.iter().any(|op| op == "clone"))
                            .filter(|vm| template_group(&vm.name, &vm.kind) == group)
                            .filter(|vm| vm.name.to_lowercase().contains(&search))
                            .collect();
                        templates.sort_by_key(|vm| vm.name.to_lowercase());
                        if templates.is_empty() {
                            continue;
                        }
                        ui.label(RichText::new(group).strong());
                        for template in templates {
                            if ui
                                .selectable_label(
                                    template.uuid == draft.source.uuid(),
                                    &template.name,
                                )
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
                    }
                });
            ui.separator();
            match &wizard.template {
                LoadState::Ready(details) => {
                    ui.add_space(12.0);
                    ui.label(&details.description);
                    ui.add_enabled(
                        details.is_default_template && !details.hvm_boot_policy.is_empty(),
                        egui::Checkbox::new(&mut wizard.copy_bios, "Copy host BIOS strings to VM"),
                    );
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
        Page::Name => {
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
        Page::CpuMemory => {
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
        }
        Page::CopyBios => {
            let editable = wizard
                .template
                .ready()
                .is_some_and(|vm| vm.is_default_template);
            ui.add_enabled_ui(editable, |ui| {
                for host in inventory.hosts.iter().filter(|h| h.enabled) {
                    ui.selectable_value(
                        &mut draft.copy_bios_from,
                        Some(host.uuid.clone()),
                        &host.name,
                    );
                }
            });
            if !editable {
                ui.label("The BIOS strings of this template cannot be changed.");
            }
            if editable && draft.copy_bios_from.is_none() {
                error = Some("Select a server to copy BIOS strings from.");
            }
            if editable && let Some(host) = &draft.copy_bios_from {
                draft.affinity_host_uuid = Some(host.clone());
            }
        }
        Page::InstallationMedia => {
            ui.add_space(12.0);
            let mut kind = match draft.media {
                InstallationMedia::Network => 2,
                InstallationMedia::PhysicalDvd(_) => 1,
                _ => 0,
            };
            ui.radio_value(&mut kind, 0, "Install from ISO library");
            let dvds: Vec<_> = wizard
                .template
                .ready()
                .into_iter()
                .flat_map(|vm| &vm.physical_dvds)
                .filter(|dvd| {
                    draft
                        .affinity_host_uuid
                        .as_ref()
                        .is_none_or(|host| &dvd.host_uuid == host)
                })
                .collect();
            ui.add_enabled_ui(!dvds.is_empty(), |ui| {
                ui.radio_value(&mut kind, 1, "Physical DVD drive on the home server");
            });
            ui.radio_value(&mut kind, 2, "Boot from network (PXE)");
            match kind {
                2 => {
                    draft.media = InstallationMedia::Network;
                }
                1 => {
                    if !matches!(draft.media, InstallationMedia::PhysicalDvd(_))
                        && let Some(dvd) = dvds.first()
                    {
                        draft.media = InstallationMedia::PhysicalDvd(dvd.uuid.clone());
                        draft.affinity_host_uuid = Some(dvd.host_uuid.clone());
                    }
                    for dvd in dvds {
                        if ui
                            .selectable_label(
                                draft.media == InstallationMedia::PhysicalDvd(dvd.uuid.clone()),
                                &dvd.name,
                            )
                            .clicked()
                        {
                            draft.media = InstallationMedia::PhysicalDvd(dvd.uuid.clone());
                            draft.affinity_host_uuid = Some(dvd.host_uuid.clone());
                        }
                    }
                }
                _ => {
                    if matches!(
                        draft.media,
                        InstallationMedia::Network | InstallationMedia::PhysicalDvd(_)
                    ) {
                        draft.media = InstallationMedia::Inherit;
                    }
                    let selected = wizard
                        .template
                        .ready()
                        .and_then(|vm| {
                            vm.isos
                                .iter()
                                .find(|iso| draft.media == InstallationMedia::Iso(iso.uuid.clone()))
                        })
                        .map_or("Select ISO", |iso| iso.name.as_str());
                    egui::ComboBox::from_id_salt("new-vm-iso")
                        .selected_text(selected)
                        .width(ui.available_width())
                        .show_ui(ui, |ui| {
                            ui.selectable_value(
                                &mut draft.media,
                                InstallationMedia::Inherit,
                                "Keep source media",
                            );
                            if let Some(details) = wizard.template.ready() {
                                for iso in &details.isos {
                                    ui.selectable_value(
                                        &mut draft.media,
                                        InstallationMedia::Iso(iso.uuid.clone()),
                                        format!("{} · {}", iso.name, iso.sr_name),
                                    );
                                }
                            }
                        });
                }
            }
            if wizard
                .template
                .ready()
                .is_some_and(|vm| vm.hvm_boot_policy.is_empty())
            {
                widgets::field_label(ui, "Boot parameters");
                ui.text_edit_singleline(draft.pv_args.get_or_insert_default());
            }
        }
        Page::HomeServer => {
            ui.add_space(12.0);
            widgets::field_label(ui, "Home server");
            let dvd_host = if let InstallationMedia::PhysicalDvd(uuid) = &draft.media {
                wizard
                    .template
                    .ready()
                    .and_then(|vm| vm.physical_dvds.iter().find(|dvd| &dvd.uuid == uuid))
                    .map(|dvd| dvd.host_uuid.clone())
            } else {
                None
            };
            if let Some(host) = &dvd_host {
                draft.affinity_host_uuid = Some(host.clone());
            }
            let mut assigned = draft.affinity_host_uuid.is_some();
            ui.add_enabled_ui(dvd_host.is_none(), |ui| {
                ui.radio_value(&mut assigned, false, "Don't assign this VM a home server");
            });
            ui.radio_value(&mut assigned, true, "Place the VM on this server");
            if !assigned {
                draft.affinity_host_uuid = None;
            } else if draft.affinity_host_uuid.is_none() {
                draft.affinity_host_uuid = inventory
                    .hosts
                    .iter()
                    .find(|host| host.enabled)
                    .map(|host| host.uuid.clone());
            }
            ui.add_space(12.0);
            ui.add_enabled_ui(assigned, |ui| {
                egui::Grid::new("home-server-list")
                    .striped(true)
                    .show(ui, |ui| {
                        ui.strong("Server");
                        ui.strong("Free memory");
                        ui.end_row();
                        for host in &inventory.hosts {
                            ui.add_enabled_ui(
                                host.enabled
                                    && dvd_host.as_ref().is_none_or(|uuid| uuid == &host.uuid),
                                |ui| {
                                    ui.selectable_value(
                                        &mut draft.affinity_host_uuid,
                                        Some(host.uuid.clone()),
                                        &host.name,
                                    );
                                },
                            );
                            ui.label(format::bytes(host.memory_free));
                            ui.end_row();
                        }
                    });
            });
            if assigned && draft.affinity_host_uuid.is_none() {
                error = Some("Select an available home server.");
            }
        }

        Page::Storage => {
            widgets::field_label(ui, "Virtual disks");
            let disks = draft.disks.get_or_insert_default();
            let mut remove = None;
            egui::ScrollArea::vertical()
                .max_height(300.0)
                .show(ui, |ui| {
                    for (index, disk) in disks.iter_mut().enumerate() {
                        ui.push_id(index, |ui| {
                            ui.horizontal(|ui| {
                                ui.label(format!("Disk {index}"));
                                ui.add(
                                    egui::TextEdit::singleline(&mut disk.name)
                                        .desired_width(180.0)
                                        .char_limit(255),
                                );
                                if ui.button("Delete").clicked() {
                                    remove = Some(index);
                                }
                            });
                            ui.horizontal(|ui| {
                                ui.label("Size (GiB)");
                                let minimum = wizard
                                    .template
                                    .ready()
                                    .and_then(|vm| {
                                        vm.disks.iter().find(|d| {
                                            !d.is_cd
                                                && Some(&d.userdevice)
                                                    == disk.source_device.as_ref()
                                        })
                                    })
                                    .map_or(1, |d| d.virtual_size.div_ceil(1024 * 1024 * 1024));
                                let mut size = disk.size_bytes as f64 / (1024.0 * 1024.0 * 1024.0);
                                if ui
                                    .add(
                                        egui::DragValue::new(&mut size)
                                            .range(minimum as f64..=1_048_576.0)
                                            .speed(1),
                                    )
                                    .changed()
                                {
                                    disk.size_bytes = (size * 1024.0 * 1024.0 * 1024.0) as u64;
                                }
                                let selected = inventory
                                    .storage
                                    .iter()
                                    .find(|sr| sr.uuid == disk.sr_uuid)
                                    .map_or("Select storage", |sr| sr.name.as_str());
                                egui::ComboBox::from_id_salt("sr")
                                    .selected_text(selected)
                                    .show_ui(ui, |ui| {
                                        for sr in inventory.storage.iter().filter(|sr| {
                                            sr.content_type != "iso"
                                                && !matches!(
                                                    sr.storage_type.as_str(),
                                                    "iso" | "udev"
                                                )
                                                && sr.pbd_attached > 0
                                        }) {
                                            ui.selectable_value(
                                                &mut disk.sr_uuid,
                                                sr.uuid.clone(),
                                                &sr.name,
                                            );
                                        }
                                    });
                            });
                            ui.separator();
                        });
                    }
                });
            if let Some(index) = remove {
                disks.remove(index);
            }
            if ui.button("Add…").clicked() {
                disks.push(CreateDisk {
                    source_device: None,
                    name: format!("{} disk {}", draft.name, disks.len()),
                    size_bytes: 20 * 1024 * 1024 * 1024,
                    sr_uuid: disks
                        .last()
                        .map_or_else(|| draft.sr_uuid.clone(), |disk| disk.sr_uuid.clone()),
                });
            }
            if disks.is_empty() {
                ui.label("This VM has no virtual disks and will boot from the network.");
            }
            if disks.iter().any(|disk| disk.sr_uuid.is_empty()) {
                error = Some("Select storage for every disk.");
            }
        }
        Page::Networking => {
            widgets::field_label(ui, "Virtual network interfaces");
            let interfaces = draft.interfaces.get_or_insert_default();
            let mut remove = None;
            egui::ScrollArea::vertical()
                .max_height(300.0)
                .show(ui, |ui| {
                    for (index, vif) in interfaces.iter_mut().enumerate() {
                        ui.push_id(index, |ui| {
                            ui.horizontal(|ui| {
                                ui.label(format!("Interface {index}"));
                                let selected = inventory
                                    .networks
                                    .iter()
                                    .find(|n| n.uuid == vif.network_uuid)
                                    .map_or("Select network", |n| n.name.as_str());
                                egui::ComboBox::from_id_salt("network")
                                    .selected_text(selected)
                                    .show_ui(ui, |ui| {
                                        for network in inventory
                                            .networks
                                            .iter()
                                            .filter(|n| n.bridge != "xenapi")
                                        {
                                            ui.selectable_value(
                                                &mut vif.network_uuid,
                                                network.uuid.clone(),
                                                &network.name,
                                            );
                                        }
                                    });
                                if ui.button("Delete").clicked() {
                                    remove = Some(index);
                                }
                            });
                            ui.horizontal(|ui| {
                                ui.label("MAC address");
                                ui.add(
                                    egui::TextEdit::singleline(&mut vif.mac)
                                        .hint_text("Auto-generated")
                                        .desired_width(180.0),
                                );
                            });
                            ui.separator();
                        });
                    }
                });
            if let Some(index) = remove {
                interfaces.remove(index);
            }
            if ui.button("Add…").clicked()
                && let Some(network) = inventory.networks.iter().find(|n| n.bridge != "xenapi")
            {
                interfaces.push(CreateVif {
                    network_uuid: network.uuid.clone(),
                    mac: String::new(),
                });
            }
        }
        Page::GuestSetup => {
            let support = &wizard.template.ready().unwrap().guest_support;
            let setup = draft.guest_setup.get_or_insert_with(Default::default);
            super::guest_setup::fields(ui, setup, support);
            error = setup.validate(support).err();
        }
        Page::Finish => {
            ui.checkbox(
                &mut draft.start_automatically,
                "Start the new VM automatically",
            );
            let template = wizard.template.ready();
            let disks = format!("{} virtual disks", draft.disks.as_ref().map_or(0, Vec::len));
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
                    "Networking",
                    format!(
                        "{} interfaces",
                        draft.interfaces.as_ref().map_or(0, Vec::len)
                    ),
                ),
                (
                    "Installation media",
                    match &draft.media {
                        InstallationMedia::Network => "Network (PXE)".into(),
                        InstallationMedia::PhysicalDvd(_) => "Physical DVD".into(),
                        InstallationMedia::Iso(uuid) => template
                            .and_then(|vm| vm.isos.iter().find(|iso| &iso.uuid == uuid))
                            .map_or_else(|| "Unavailable ISO".into(), |iso| iso.name.clone()),
                        InstallationMedia::Inherit => "Keep source media".into(),
                    },
                ),
                (
                    "Home server",
                    draft
                        .affinity_host_uuid
                        .as_ref()
                        .and_then(|uuid| inventory.hosts.iter().find(|host| &host.uuid == uuid))
                        .map_or_else(|| "Automatic placement".into(), |host| host.name.clone()),
                ),
                (
                    "Power state after creation",
                    if draft.start_automatically {
                        "Running"
                    } else {
                        "Halted"
                    }
                    .into(),
                ),
            ] {
                widgets::kv(ui, label, &value, false);
            }
            ui.add_space(8.0);
            ui.label("Create the VM with these settings. Follow its progress in Events.");
        }
    }
    error
}

fn template_group(name: &str, kind: &str) -> &'static str {
    if kind == "snapshot" {
        return "Snapshots";
    }
    let name = name.to_lowercase();
    if name.contains("windows") {
        "Windows"
    } else if [
        "debian", "ubuntu", "centos", "fedora", "rhel", "red hat", "suse", "oracle", "linux",
        "rocky", "alma",
    ]
    .iter()
    .any(|os| name.contains(os))
    {
        "Linux"
    } else {
        "Other"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn installed_guests_skip_media_and_keep_guest_setup_before_finish() {
        let steps = pages(true, true);
        assert!(!steps.contains(&Page::InstallationMedia));
        assert!(steps[2] == Page::HomeServer);
        assert!(steps[steps.len() - 2] == Page::GuestSetup);
        assert!(steps[steps.len() - 1] == Page::Finish);
        let stock = pages(false, false);
        assert!(stock[2] == Page::InstallationMedia);
        assert!(!stock.contains(&Page::GuestSetup));
    }
}
