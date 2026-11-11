use egui::{Align, Color32, Layout, RichText, Sense, Ui, Vec2};
use egui_extras::{Column, TableBuilder};
use xen_core::{
    performance::HostPerformance,
    vm::{VmDetails, VmInterface},
    workspace::WorkspaceView,
};

use super::host_details::{chart, is_cpu_core, is_memory_metric, is_network_metric, multi_chart};
use super::sidebar::TreeIcon;
use crate::format;
use crate::state::{
    ActionKind, LoadState, ObjectKey, UiAction, UiState, VmTab, action_allowed, connection_id,
    connection_inventory, find_vm, is_read_only,
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
        } => (connection_id, vm_uuid, TreeIcon::Vm, false, false),
        ObjectKey::Template {
            connection_id,
            vm_uuid,
        } => (connection_id, vm_uuid, TreeIcon::Template, true, false),
        ObjectKey::Snapshot {
            connection_id,
            vm_uuid,
        } => (connection_id, vm_uuid, TreeIcon::Snapshot, false, true),
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

    let object_type = if is_template {
        1
    } else if is_snapshot {
        2
    } else {
        0
    };
    super::object_page::title_strip(ui, connection, &vm.name, kind, &vm.power_state);
    let has_provision_xml = connection_inventory(connection)
        .and_then(|inventory| inventory.navigation.get(&vm.uuid))
        .is_some_and(|metadata| metadata.default_template);
    let selection = if is_snapshot {
        super::object_page::Selection::Snapshot
    } else if is_template {
        super::object_page::Selection::Template { has_provision_xml }
    } else {
        super::object_page::Selection::Vm
    };
    let tabs = super::object_page::tabs_for(selection);
    let mut tab = state.vm_subtab;
    super::object_page::tab_strip(ui, &tabs, &mut tab);
    if tab != state.vm_subtab {
        actions.push(UiAction::SetVmTab(tab));
    }
    state.vm_subtab = tab;
    state.vm_last_tabs[object_type] = tab;
    ui.add_space(4.0);

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
                    &connection_id,
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
                network(ui, &connection_id, &details.interfaces, editable, actions);
            }
        }
        VmTab::Memory => {
            if let Some(details) = details_state(ui, state, actions) {
                let editable = !read_only && !is_template && !is_snapshot;
                memory(ui, &details, editable, actions);
            }
        }
        VmTab::Snapshots => snapshots(ui, state, workspace, connection, vm, read_only, actions),
        VmTab::Console => console(ui, state, &connection_id, &vm_uuid, vm, actions),
        VmTab::Search => super::search_page::show(ui, state, workspace, actions),
        _ => unreachable!("VM tabs are selected by tabs_for"),
    }
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
    use super::object_page::value_row as row;
    let mut expand = None;
    ui.horizontal(|ui| {
        if ui
            .add_enabled(!read_only && !is_snapshot, egui::Button::new("Properties"))
            .clicked()
        {
            actions.push(UiAction::OpenProperties {
                connection_id: connection_id(connection).to_owned(),
                target: xen_core::properties::ObjectTarget {
                    kind: xen_core::properties::ObjectKind::Vm,
                    uuid: vm.uuid.clone(),
                },
            });
        }
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            if ui.button("Collapse all").clicked() {
                expand = Some(false);
            }
            if ui.button("Expand all").clicked() {
                expand = Some(true);
            }
        });
    });
    ui.spacing_mut().item_spacing.y = 0.0;
    let section = |ui: &mut Ui, name: &str, body: &mut dyn FnMut(&mut Ui)| {
        egui::CollapsingHeader::new(name)
            .id_salt(("vm-general", name))
            .default_open(true)
            .open(expand)
            .show(ui, body);
    };
    section(ui, "General", &mut |ui| {
        row(ui, "Name", &details.name);
        row(ui, "Description", &details.description);
        row(ui, "Tags", &details.tags.join(", "));
        if let Some(metadata) =
            connection_inventory(connection).and_then(|inv| inv.navigation.get(&vm.uuid))
        {
            row(ui, "Folder", &metadata.folder);
        }
        if !details.os_version.is_empty() {
            row(ui, "Operating system", &details.os_version);
        }
        row(
            ui,
            "Virtualization mode",
            if details.hvm_boot_policy.is_empty() {
                "Paravirtualization (PV)"
            } else {
                "Hardware-assisted virtualization (HVM)"
            },
        );
        if !details.pv_drivers_version.is_empty() {
            row(ui, "Virtualization state", &details.pv_drivers_version);
        }
        row(ui, "UUID", &details.uuid);
    });
    section(ui, "Boot Options", &mut |ui| {
        row(
            ui,
            "Auto-boot",
            if details.auto_poweron { "Yes" } else { "No" },
        );
        if let Some(order) = details.hvm_boot_params.get("order") {
            row(ui, "Boot order", order);
        }
        if details.hvm_boot_policy.is_empty() {
            row(ui, "OS boot parameters", &details.pv_args);
        }
        if ui
            .add_enabled(!read_only && !is_snapshot, egui::Button::new("Edit…"))
            .clicked()
        {
            actions.push(UiAction::OpenVmResources);
            actions.push(UiAction::SelectPropertyPage("Boot Options"));
        }
    });
    section(ui, "CPUs", &mut |ui| {
        row(ui, "vCPUs", &details.vcpus_max.to_string());
        if !details.cores_per_socket.is_empty() {
            row(
                ui,
                "Topology",
                &format!("{} cores per socket", details.cores_per_socket),
            );
        }
        if !details.cpu_weight.is_empty() {
            row(ui, "Priority", &details.cpu_weight);
        }
        if ui
            .add_enabled(
                !read_only && !is_template && !is_snapshot,
                egui::Button::new("Edit…"),
            )
            .clicked()
        {
            actions.push(UiAction::OpenVmResources);
        }
    });
    // Read caching is omitted: the existing model has no caching status.
    if !is_template && !is_snapshot {
        section(ui, "High Availability", &mut |ui| {
            row(
                ui,
                "Restart priority",
                if details.ha_restart_priority.is_empty() {
                    "Do not restart"
                } else {
                    &details.ha_restart_priority
                },
            );
            if ui
                .add_enabled(!read_only, egui::Button::new("Edit…"))
                .clicked()
            {
                actions.push(UiAction::OpenHaEdit);
            }
        });
    }
    section(ui, "Memory", &mut |ui| {
        row(ui, "Memory", &format::bytes(details.memory_dynamic_max));
    });
    if !is_template && !is_snapshot {
        section(ui, "Home Server", &mut |ui| {
            row(ui, "Home server", &details.affinity);
            if ui
                .add_enabled(!read_only && !is_snapshot, egui::Button::new("Edit…"))
                .clicked()
            {
                actions.push(UiAction::OpenVmResources);
                actions.push(UiAction::SelectPropertyPage("Home Server"));
            }
        });
    }
}

fn memory(ui: &mut Ui, details: &VmDetails, editable: bool, actions: &mut Vec<UiAction>) {
    ui.horizontal(|ui| {
        ui.label("Memory");
        if ui
            .add_enabled(editable, egui::Button::new("Edit…"))
            .on_disabled_hover_text("Not implemented yet for templates or snapshots")
            .clicked()
        {
            actions.push(UiAction::OpenVmResources);
        }
    });
    // memory_target is allocation, not observed guest memory use.
    widgets::resource_bar(
        ui,
        details.memory_target,
        details.memory_static_max,
        accent(),
        24.0,
    );
    for (label, value) in [
        ("Allocated", details.memory_target),
        ("Dynamic minimum", details.memory_dynamic_min),
        ("Dynamic maximum", details.memory_dynamic_max),
        ("Static maximum", details.memory_static_max),
    ] {
        super::object_page::value_row(ui, label, &format::bytes(value));
    }
}

fn performance(ui: &mut Ui, state: &mut UiState, actions: &mut Vec<UiAction>) {
    theme::card_frame().show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.menu_button("Actions ▾", |ui| {
            for label in [
                "New Graph…",
                "Edit Graph…",
                "Delete Graph",
                "Restore Default Graphs",
            ] {
                ui.add_enabled(false, egui::Button::new(label))
                    .on_disabled_hover_text("Not implemented yet");
            }
        });
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

#[allow(clippy::too_many_arguments)]
fn storage(
    ui: &mut Ui,
    connection_id: &str,
    read_only: bool,
    is_template: bool,
    is_snapshot: bool,
    halted: bool,
    details: &VmDetails,
    actions: &mut Vec<UiAction>,
) {
    let disks: Vec<_> = details.disks.iter().filter(|disk| !disk.is_cd).collect();
    let editable = !read_only && !is_template && !is_snapshot;
    dvd_drives(ui, details, editable, halted, actions);
    let selection_id = ui.scope_id().with(("disk-selection", &details.uuid));
    let mut selected = ui
        .ctx()
        .data_mut(|data| data.get_temp::<String>(selection_id))
        .unwrap_or_default();
    egui::ScrollArea::horizontal()
        .id_salt("disk-scroll")
        .show(ui, |ui| {
            TableBuilder::new(ui)
                .id_salt("vm-disks")
                .striped(true)
                .sense(Sense::click())
                .cell_layout(Layout::left_to_right(Align::Center))
                .columns(Column::initial(100.0).at_least(70.0).clip(true), 9)
                .header(24.0, |mut header| {
                    for title in [
                        "Position",
                        "Name",
                        "Description",
                        "SR",
                        "SR Volume / Size",
                        "Read Only",
                        "Priority",
                        "Active",
                        "Device Path",
                    ] {
                        header.col(|ui| {
                            ui.label(RichText::new(title).size(11.0));
                        });
                    }
                })
                .body(|body| {
                    body.rows(24.0, disks.len(), |mut row| {
                        let disk = disks[row.index()];
                        row.set_selected(selected == disk.vbd_uuid);
                        for value in [
                            disk.userdevice.clone(),
                            disk.vdi_name.clone(),
                            disk.description.clone(),
                            disk.sr_name.clone(),
                            format::bytes(disk.virtual_size),
                            if disk.mode == "RO" { "Yes" } else { "No" }.into(),
                            match disk.priority.as_str() {
                                "0" => "0 (Lowest)".into(),
                                "7" => "7 (Highest)".into(),
                                _ => disk.priority.clone(),
                            },
                            if disk.attached { "Yes" } else { "No" }.into(),
                            if disk.device.is_empty() {
                                "Unknown".into()
                            } else {
                                format!("/dev/{}", disk.device)
                            },
                        ] {
                            row.col(|ui| {
                                if ui
                                    .add(
                                        egui::Label::new(RichText::new(value).size(11.0))
                                            .sense(Sense::click()),
                                    )
                                    .clicked()
                                {
                                    selected = disk.vbd_uuid.clone();
                                }
                            });
                        }
                        if row.response().clicked() || row.response().secondary_clicked() {
                            selected = disk.vbd_uuid.clone();
                        }
                        row.response().context_menu(|ui| {
                            storage_commands(
                                ui,
                                connection_id,
                                editable,
                                halted,
                                Some(disk),
                                actions,
                            )
                        });
                    });
                });
        });
    ui.ctx()
        .data_mut(|data| data.insert_temp(selection_id, selected.clone()));
    let disk = disks.iter().copied().find(|disk| disk.vbd_uuid == selected);
    storage_commands(ui, connection_id, editable, halted, disk, actions);
}

fn storage_commands(
    ui: &mut Ui,
    connection_id: &str,
    editable: bool,
    halted: bool,
    disk: Option<&xen_core::vm::VmDisk>,
    actions: &mut Vec<UiAction>,
) {
    super::context_menu::command_strip(ui, |ui| {
        if ui
            .add_enabled(editable && halted, egui::Button::new("Add…"))
            .on_disabled_hover_text("The object state or connection does not allow this operation.")
            .clicked()
        {
            actions.push(UiAction::OpenAddDisk);
        }
        if ui
            .add_enabled(editable && halted, egui::Button::new("Attach Disk…"))
            .on_disabled_hover_text("The object state or connection does not allow this operation.")
            .clicked()
        {
            actions.push(UiAction::OpenAttachVdi);
        }
        let active = disk.is_some_and(|disk| disk.attached);
        if ui
            .add_enabled(
                editable && !halted && disk.is_some(),
                egui::Button::new(if active { "Deactivate" } else { "Activate" }),
            )
            .on_disabled_hover_text("The object state or connection does not allow this operation.")
            .clicked()
        {
            let disk = disk.unwrap();
            actions.push(UiAction::SetVbdPlugged {
                vbd_uuid: disk.vbd_uuid.clone(),
                plugged: !active,
            });
        }
        if ui
            .add_enabled(editable && disk.is_some(), egui::Button::new("Move…"))
            .on_disabled_hover_text("The object state or connection does not allow this operation.")
            .clicked()
        {
            let disk = disk.unwrap();
            actions.push(UiAction::OpenMoveDisk {
                vbd_uuid: disk.vbd_uuid.clone(),
                name: disk.vdi_name.clone(),
                size: disk.virtual_size,
                source_sr_uuid: disk.sr_uuid.clone(),
            });
        }
        for (label, delete) in [("Detach", false), ("Delete", true)] {
            if ui
                .add_enabled(
                    editable && halted && disk.is_some_and(|disk| !delete || disk.owner),
                    egui::Button::new(label),
                )
                .on_disabled_hover_text(
                    "The object state or connection does not allow this operation.",
                )
                .clicked()
            {
                let disk = disk.unwrap();
                actions.push(UiAction::PromptRemoveDisk {
                    vbd_uuid: disk.vbd_uuid.clone(),
                    name: disk.vdi_name.clone(),
                    owner: delete && disk.owner,
                });
            }
        }
        if ui
            .add_enabled(editable && disk.is_some(), egui::Button::new("Properties"))
            .on_disabled_hover_text("The object state or connection does not allow this operation.")
            .clicked()
        {
            actions.push(UiAction::OpenProperties {
                connection_id: connection_id.to_owned(),
                target: xen_core::properties::ObjectTarget {
                    kind: xen_core::properties::ObjectKind::Disk,
                    uuid: disk.unwrap().vdi_uuid.clone(),
                },
            });
        }
    });
}

fn dvd_drives(
    ui: &mut Ui,
    details: &VmDetails,
    editable: bool,
    halted: bool,
    actions: &mut Vec<UiAction>,
) {
    for (index, disk) in details.disks.iter().filter(|disk| disk.is_cd).enumerate() {
        ui.horizontal(|ui| {
            ui.label(format!("DVD Drive {}:", index + 1));
            let mut iso = (!disk.empty).then(|| disk.vdi_uuid.clone());
            let before = iso.clone();
            ui.add_enabled_ui(editable, |ui| {
                egui::ComboBox::from_id_salt(("dvd", &disk.vbd_uuid))
                    .selected_text(if disk.empty {
                        "<empty>"
                    } else {
                        &disk.vdi_name
                    })
                    .width(260.0)
                    .show_ui(ui, |ui| {
                        ui.selectable_value(&mut iso, None, "<empty>");
                        for media in &details.isos {
                            ui.selectable_value(&mut iso, Some(media.uuid.clone()), &media.name);
                        }
                    });
            });
            if iso != before {
                actions.push(UiAction::SetDvdMedia {
                    vbd_uuid: disk.vbd_uuid.clone(),
                    iso_uuid: iso,
                });
            }
            if ui
                .add_enabled(editable && !disk.empty, egui::Button::new("Eject"))
                .clicked()
            {
                actions.push(UiAction::SetDvdMedia {
                    vbd_uuid: disk.vbd_uuid.clone(),
                    iso_uuid: None,
                });
            }
        });
    }
    if ui
        .add_enabled(editable && halted, egui::Button::new("New CD-DVD Drive"))
        .clicked()
    {
        actions.push(UiAction::AddCdDrive);
    }
    ui.add_space(8.0);
}

fn network(
    ui: &mut Ui,
    connection_id: &str,
    interfaces: &[VmInterface],
    editable: bool,
    actions: &mut Vec<UiAction>,
) {
    let selection_id = ui.scope_id().with("vif-selection");
    let mut selected = ui
        .ctx()
        .data_mut(|data| data.get_temp::<String>(selection_id))
        .unwrap_or_default();
    TableBuilder::new(ui)
        .id_salt("vm-vifs")
        .striped(true)
        .sense(Sense::click())
        .cell_layout(Layout::left_to_right(Align::Center))
        .column(Column::exact(60.0))
        .column(Column::initial(140.0))
        .column(Column::exact(65.0))
        .column(Column::remainder())
        .column(Column::initial(160.0))
        .column(Column::exact(65.0))
        .header(24.0, |mut header| {
            for title in ["Device", "MAC", "Limit", "Network", "IP Address", "Active"] {
                header.col(|ui| {
                    ui.label(RichText::new(title).size(11.0));
                });
            }
        })
        .body(|body| {
            body.rows(24.0, interfaces.len(), |mut row| {
                let interface = &interfaces[row.index()];
                row.set_selected(selected == interface.uuid);
                for value in [
                    interface.device.clone(),
                    interface.mac.clone(),
                    String::new(),
                    interface.network.clone(),
                    interface
                        .ipv4
                        .iter()
                        .chain(&interface.ipv6)
                        .cloned()
                        .collect::<Vec<_>>()
                        .join(", "),
                    if interface.attached { "Yes" } else { "No" }.into(),
                ] {
                    row.col(|ui| {
                        ui.label(RichText::new(value).size(11.0));
                    });
                }
                if row.response().clicked() || row.response().secondary_clicked() {
                    selected = interface.uuid.clone();
                }
                row.response().context_menu(|ui| {
                    network_commands(ui, connection_id, editable, Some(interface), actions)
                });
            });
        });
    ui.ctx()
        .data_mut(|data| data.insert_temp(selection_id, selected.clone()));
    let interface = interfaces
        .iter()
        .find(|interface| interface.uuid == selected);
    network_commands(ui, connection_id, editable, interface, actions);
}

fn network_commands(
    ui: &mut Ui,
    connection_id: &str,
    editable: bool,
    interface: Option<&VmInterface>,
    actions: &mut Vec<UiAction>,
) {
    super::context_menu::command_strip(ui, |ui| {
        if ui
            .add_enabled(editable, egui::Button::new("Add Interface…"))
            .on_disabled_hover_text("The object state or connection does not allow this operation.")
            .clicked()
        {
            actions.push(UiAction::OpenVifDialog(None));
        }
        if ui
            .add_enabled(
                editable && interface.is_some(),
                egui::Button::new("Properties"),
            )
            .on_disabled_hover_text("The object state or connection does not allow this operation.")
            .clicked()
        {
            actions.push(UiAction::OpenProperties {
                connection_id: connection_id.into(),
                target: xen_core::properties::ObjectTarget {
                    kind: xen_core::properties::ObjectKind::Network,
                    uuid: interface.unwrap().network_uuid.clone(),
                },
            });
            actions.push(UiAction::SelectPropertyPage("Network Settings"));
        }
        if ui
            .add_enabled(editable && interface.is_some(), egui::Button::new("Remove"))
            .on_disabled_hover_text("The object state or connection does not allow this operation.")
            .clicked()
        {
            let interface = interface.unwrap();
            actions.push(UiAction::PromptRemoveVif {
                vif_uuid: interface.uuid.clone(),
                mac: interface.mac.clone(),
            });
        }
        let active = interface.is_some_and(|interface| interface.attached);
        if ui
            .add_enabled(
                editable && interface.is_some(),
                egui::Button::new(if active { "Deactivate" } else { "Activate" }),
            )
            .on_disabled_hover_text("The object state or connection does not allow this operation.")
            .clicked()
        {
            actions.push(UiAction::SetVifPlugged {
                vif_uuid: interface.unwrap().uuid.clone(),
                plugged: !active,
            });
        }
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
    let Some(inventory) = connection_inventory(connection) else {
        return;
    };
    let snapshots: Vec<_> = inventory
        .snapshots
        .iter()
        .filter(|snapshot| snapshot.snapshot_of == vm.reference)
        .collect();
    let available: std::collections::HashSet<&str> = snapshots
        .iter()
        .map(|snapshot| snapshot.uuid.as_str())
        .collect();
    state
        .snapshot_selection
        .retain(|uuid| available.contains(uuid.as_str()));

    let selected: Vec<_> = snapshots
        .iter()
        .copied()
        .filter(|snapshot| state.snapshot_selection.contains(&snapshot.uuid))
        .collect();
    let single = (selected.len() == 1).then(|| selected[0]);
    ui.horizontal(|ui| {
        if ui
            .add_enabled(
                !read_only && action_allowed(workspace, connection_id(connection), vm, "snapshot"),
                egui::Button::new("Take Snapshot…"),
            )
            .clicked()
        {
            actions.push(UiAction::Propose(
                ActionKind::Snapshot,
                vec![(connection_id(connection).to_owned(), vm.uuid.clone())],
            ));
        }
        if ui
            .add_enabled(
                single.is_some_and(|snapshot| {
                    action_allowed(workspace, connection_id(connection), snapshot, "revert")
                }),
                egui::Button::new("Revert To…"),
            )
            .clicked()
        {
            actions.push(UiAction::Propose(
                ActionKind::Revert,
                vec![(
                    connection_id(connection).to_owned(),
                    single.unwrap().uuid.clone(),
                )],
            ));
        }
        ui.add_enabled_ui(single.is_some(), |ui| {
            ui.menu_button("Actions ▾", |ui| {
                if let Some(snapshot) = single {
                    for (label, kind) in [
                        ("New VM from Snapshot…", ActionKind::SnapshotToVm),
                        (
                            "New Template from Snapshot…",
                            ActionKind::SnapshotToTemplate,
                        ),
                    ] {
                        if ui
                            .add_enabled(
                                action_allowed(
                                    workspace,
                                    connection_id(connection),
                                    snapshot,
                                    "clone",
                                ),
                                egui::Button::new(label),
                            )
                            .clicked()
                        {
                            actions.push(UiAction::Propose(
                                kind,
                                vec![(connection_id(connection).to_owned(), snapshot.uuid.clone())],
                            ));
                            ui.close();
                        }
                    }
                    if ui
                        .add_enabled(
                            action_allowed(
                                workspace,
                                connection_id(connection),
                                snapshot,
                                "export",
                            ),
                            egui::Button::new("Export Snapshot as Template…"),
                        )
                        .clicked()
                    {
                        actions.push(UiAction::OpenXvaExport {
                            connection_id: connection_id(connection).to_owned(),
                            vm_uuid: snapshot.uuid.clone(),
                        });
                        ui.close();
                    }
                    if ui.button("Properties").clicked() {
                        actions.push(UiAction::OpenProperties {
                            connection_id: connection_id(connection).to_owned(),
                            target: xen_core::properties::ObjectTarget {
                                kind: xen_core::properties::ObjectKind::Vm,
                                uuid: snapshot.uuid.clone(),
                            },
                        });
                        ui.close();
                    }
                }
            });
        });
        if ui
            .add_enabled(
                !read_only
                    && !selected.is_empty()
                    && selected.iter().all(|snapshot| {
                        action_allowed(workspace, connection_id(connection), snapshot, "destroy")
                    }),
                egui::Button::new("Delete"),
            )
            .clicked()
        {
            actions.push(UiAction::PromptDeleteSnapshots);
        }
    });
    ui.add_space(4.0);
    let view_id = ui.scope_id().with(("snapshot-tree", &vm.uuid));
    let mut tree = ui
        .ctx()
        .data_mut(|d| d.get_temp::<bool>(view_id))
        .unwrap_or(true);
    ui.horizontal(|ui| {
        ui.selectable_value(&mut tree, true, "Tree view");
        ui.selectable_value(&mut tree, false, "List view");
    });
    ui.ctx().data_mut(|d| d.insert_temp(view_id, tree));
    if tree {
        ui.columns(2, |columns| {
            let mut visited = std::collections::HashSet::new();
            // Include the current VM as a leaf at its actual VM.parent link.
            let mut nodes = snapshots.clone();
            nodes.push(vm);
            for root in nodes
                .iter()
                .filter(|s| !nodes.iter().any(|p| p.reference == s.parent))
            {
                snapshot_branch(
                    &mut columns[0],
                    root,
                    vm,
                    &nodes,
                    0,
                    &mut visited,
                    &mut state.snapshot_selection,
                );
            }
            for node in &nodes {
                if !visited.contains(&node.uuid) {
                    snapshot_branch(
                        &mut columns[0],
                        node,
                        vm,
                        &nodes,
                        0,
                        &mut visited,
                        &mut state.snapshot_selection,
                    );
                }
            }
            let panel = &mut columns[1];
            let selected = snapshots
                .iter()
                .copied()
                .find(|s| state.snapshot_selection.contains(&s.uuid))
                .unwrap_or(vm);
            panel.label(RichText::new(&selected.name).size(15.0).strong());
            panel.separator();
            for (key, value) in [
                ("Name", selected.name.clone()),
                ("Description", selected.description.clone()),
                (
                    "Created",
                    if selected.uuid == vm.uuid || selected.snapshot_time.is_empty() {
                        "—".into()
                    } else {
                        format::xapi_time(&selected.snapshot_time)
                    },
                ),
                ("Tags", selected.tags.join(", ")),
                ("Folder", selected.folder.clone()),
                ("Memory", format::bytes(selected.memory)),
                ("Disks", selected.disk_count.to_string()),
            ] {
                panel.strong(key);
                panel.label(if value.is_empty() { "—" } else { &value });
            }
            panel.strong("Custom Fields");
            if selected.custom_fields.is_empty() {
                panel.label("None configured");
            }
            for (name, value) in &selected.custom_fields {
                panel.label(format!("{name}: {value}"));
            }
        });
        return;
    }
    if snapshots.is_empty() {
        ui.label(
            RichText::new("No snapshots for this VM.")
                .size(11.0)
                .color(dim_fg()),
        );
    }
    for snapshot in &snapshots {
        let response = ui.horizontal(|ui| {
            let mut checked = state.snapshot_selection.contains(&snapshot.uuid);
            if ui
                .add_enabled(!read_only, egui::Checkbox::without_text(&mut checked))
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
                        "—".into()
                    } else {
                        format::xapi_time(&snapshot.snapshot_time)
                    })
                    .size(9.0)
                    .color(dim_fg()),
                );
            });
        });
        let row_response = response.response.interact(Sense::click());
        if row_response.secondary_clicked() && !state.snapshot_selection.contains(&snapshot.uuid) {
            state.snapshot_selection.clear();
            state.snapshot_selection.insert(snapshot.uuid.clone());
        }
        row_response.context_menu(|ui| {
            for (label, kind, operation) in [
                ("Take Snapshot…", ActionKind::Snapshot, "snapshot"),
                ("Revert To…", ActionKind::Revert, "revert"),
                ("New VM from Snapshot…", ActionKind::SnapshotToVm, "clone"),
                (
                    "New Template from Snapshot…",
                    ActionKind::SnapshotToTemplate,
                    "clone",
                ),
            ] {
                let target = if kind == ActionKind::Snapshot {
                    vm
                } else {
                    snapshot
                };
                if ui
                    .add_enabled(
                        !read_only
                            && action_allowed(
                                workspace,
                                connection_id(connection),
                                target,
                                operation,
                            ),
                        egui::Button::new(label),
                    )
                    .on_disabled_hover_text("The VM state does not allow this operation.")
                    .clicked()
                {
                    actions.push(UiAction::Propose(
                        kind,
                        vec![(connection_id(connection).into(), target.uuid.clone())],
                    ));
                    ui.close();
                }
            }
            if ui
                .add_enabled(
                    action_allowed(workspace, connection_id(connection), snapshot, "export"),
                    egui::Button::new("Export Snapshot as Template…"),
                )
                .on_disabled_hover_text("The snapshot state does not allow export.")
                .clicked()
            {
                actions.push(UiAction::OpenXvaExport {
                    connection_id: connection_id(connection).into(),
                    vm_uuid: snapshot.uuid.clone(),
                });
                ui.close();
            }
            if ui.button("Properties").clicked() {
                actions.push(UiAction::OpenProperties {
                    connection_id: connection_id(connection).into(),
                    target: xen_core::properties::ObjectTarget {
                        kind: xen_core::properties::ObjectKind::Vm,
                        uuid: snapshot.uuid.clone(),
                    },
                });
                ui.close();
            }
            if ui
                .add_enabled(
                    !read_only
                        && state.snapshot_selection.iter().all(|uuid| {
                            snapshots.iter().find(|s| &s.uuid == uuid).is_some_and(|s| {
                                action_allowed(workspace, connection_id(connection), s, "destroy")
                            })
                        }),
                    egui::Button::new("Delete"),
                )
                .on_disabled_hover_text("The snapshot state does not allow deletion.")
                .clicked()
            {
                actions.push(UiAction::PromptDeleteSnapshots);
                ui.close();
            }
        });
        ui.add_space(3.0);
    }
}

fn snapshot_branch(
    ui: &mut Ui,
    node: &xen_core::xapi::VirtualMachine,
    current: &xen_core::xapi::VirtualMachine,
    nodes: &[&xen_core::xapi::VirtualMachine],
    depth: usize,
    visited: &mut std::collections::HashSet<String>,
    selection: &mut std::collections::HashSet<String>,
) {
    if !visited.insert(node.uuid.clone()) {
        return;
    }
    ui.horizontal(|ui| {
        ui.add_space(depth as f32 * 20.0);
        ui.label(if depth == 0 { "▣" } else { "└─" });
        let now = node.uuid == current.uuid;
        if ui
            .selectable_label(
                if now {
                    selection.is_empty()
                } else {
                    selection.contains(&node.uuid)
                },
                if now { "Now" } else { &node.name },
            )
            .clicked()
        {
            if !ui.input(|i| i.modifiers.command) {
                selection.clear();
            }
            if !now && !selection.remove(&node.uuid) {
                selection.insert(node.uuid.clone());
            }
        }
    });
    for child in nodes.iter().filter(|child| child.parent == node.reference) {
        snapshot_branch(ui, child, current, nodes, depth + 1, visited, selection);
    }
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
