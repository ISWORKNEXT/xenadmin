use egui::RichText;
use xen_core::workspace::WorkspaceView;

use crate::state::{LoadState, UiAction, UiState, connection_inventory, find_connection};
use crate::{format, theme, widgets};

use super::wizard::{Frame, Outcome, Step};

#[derive(Clone, Copy, PartialEq, Eq)]
enum Page {
    Template,
    Name,
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
    let template = wizard.template.ready();
    let pages = pages(
        template.is_some_and(|vm| vm.disks.iter().any(|disk| !disk.is_cd)),
        template.is_some_and(|vm| vm.guest_support.enabled),
    );
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
        Page::InstallationMedia => {
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
                .width(ui.available_width())
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
        }
        Page::HomeServer => {
            ui.add_space(12.0);
            widgets::field_label(ui, "Home server");
            let mut assigned = draft.affinity_host_uuid.is_some();
            ui.radio_value(&mut assigned, false, "Don't assign this VM a home server");
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
                            ui.add_enabled_ui(host.enabled, |ui| {
                                ui.selectable_value(
                                    &mut draft.affinity_host_uuid,
                                    Some(host.uuid.clone()),
                                    &host.name,
                                );
                            });
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
            widgets::field_label(ui, "Storage repository");
            let selected = inventory
                .storage
                .iter()
                .find(|sr| sr.uuid == draft.sr_uuid)
                .map_or("Select storage", |sr| sr.name.as_str());
            egui::ComboBox::from_id_salt("new-vm-sr")
                .selected_text(selected)
                .width(ui.available_width())
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
            let blueprint = wizard
                .template
                .ready()
                .map_or(&[][..], |vm| vm.provision_disks.as_slice());
            let installation = existing == 0 && !blueprint.is_empty();
            egui::Grid::new("new-vm-disks")
                .striped(true)
                .show(ui, |ui| {
                    ui.strong("Disk");
                    ui.strong("Size");
                    ui.end_row();
                    if let Some(template) = wizard.template.ready() {
                        for disk in template.disks.iter().filter(|disk| !disk.is_cd) {
                            ui.label(format!("Source disk {}", disk.userdevice));
                            ui.label(format::bytes(disk.virtual_size));
                            ui.end_row();
                        }
                    }
                    for (index, disk) in blueprint.iter().enumerate().filter(|_| existing == 0) {
                        ui.label(format!("Installation disk {}", disk.device));
                        ui.label(if index == 0 {
                            draft.new_disk_gib.map_or_else(
                                || format::bytes(disk.size),
                                |size| format!("{size} GiB"),
                            )
                        } else {
                            format::bytes(disk.size)
                        });
                        ui.end_row();
                    }
                    if !installation && let Some(size) = draft.new_disk_gib {
                        ui.label("New disk");
                        ui.label(format!("{size} GiB"));
                        ui.end_row();
                    }
                });
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                if ui
                    .add_enabled(
                        !installation && draft.new_disk_gib.is_none(),
                        egui::Button::new("Add…"),
                    )
                    .clicked()
                {
                    draft.new_disk_gib = Some(20);
                    wizard.edit_disk = true;
                }
                if ui
                    .add_enabled(
                        installation || draft.new_disk_gib.is_some(),
                        egui::Button::new("Edit…"),
                    )
                    .clicked()
                {
                    if draft.new_disk_gib.is_none() {
                        draft.new_disk_gib = blueprint
                            .first()
                            .map(|disk| disk.size.div_ceil(1024 * 1024 * 1024));
                    }
                    wizard.edit_disk = true;
                }
                if ui
                    .add_enabled(
                        !installation && draft.new_disk_gib.is_some(),
                        egui::Button::new("Delete"),
                    )
                    .clicked()
                {
                    draft.new_disk_gib = None;
                    wizard.edit_disk = false;
                }
            });
            if wizard.edit_disk
                && let Some(size) = draft.new_disk_gib.as_mut()
            {
                ui.horizontal(|ui| {
                    ui.label(if installation {
                        "First installation disk size"
                    } else {
                        "New disk size"
                    });
                    ui.add(egui::DragValue::new(size).range(1..=1_048_576));
                    ui.label("GiB");
                    if ui.button("Apply").clicked() {
                        wizard.edit_disk = false;
                    }
                });
            }
            let mut diskless = !installation && existing == 0 && draft.new_disk_gib.is_none();
            if ui
                .add_enabled(
                    !installation && existing == 0,
                    egui::Checkbox::new(&mut diskless, "Create a diskless VM"),
                )
                .changed()
            {
                draft.new_disk_gib = if diskless { None } else { Some(20) };
            }
            if draft.sr_uuid.is_empty() {
                error = Some("Select storage for the VM disks.");
            }
        }
        Page::Networking => {
            widgets::field_label(ui, "Virtual network interfaces");
            let network = draft.network_uuid.as_ref().and_then(|uuid| {
                inventory
                    .networks
                    .iter()
                    .find(|network| &network.uuid == uuid)
            });
            egui::Grid::new("new-vm-vifs").striped(true).show(ui, |ui| {
                ui.strong("Interface");
                ui.strong("Network");
                ui.end_row();
                if let Some(network) = network {
                    ui.label("0");
                    ui.label(&network.name);
                    ui.end_row();
                }
            });
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                if ui
                    .add_enabled(draft.network_uuid.is_none(), egui::Button::new("Add…"))
                    .clicked()
                {
                    draft.network_uuid = inventory
                        .networks
                        .iter()
                        .find(|network| network.bridge != "xenapi")
                        .map(|network| network.uuid.clone());
                    wizard.edit_network = true;
                }
                if ui
                    .add_enabled(
                        draft.network_uuid.is_some(),
                        egui::Button::new("Properties…"),
                    )
                    .clicked()
                {
                    wizard.edit_network = true;
                }
                if ui
                    .add_enabled(draft.network_uuid.is_some(), egui::Button::new("Delete"))
                    .clicked()
                {
                    draft.network_uuid = None;
                    wizard.edit_network = false;
                }
            });
            if wizard.edit_network {
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
                    .width(ui.available_width())
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
                if ui.button("Apply").clicked() {
                    wizard.edit_network = false;
                }
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
                    "Networking",
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
