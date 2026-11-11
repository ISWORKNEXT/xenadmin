use crate::{
    state::{
        LoadState, PropertiesDialog, UiAction, UiState, find_connection, is_online, is_read_only,
    },
    theme, widgets,
};
use egui::RichText;
use xen_core::{properties::ObjectKind, workspace::WorkspaceView};

fn pages(dialog: &PropertiesDialog) -> Vec<String> {
    let mut pages = vec!["General", "Custom Fields"];
    match dialog.target.kind {
        ObjectKind::Vm => pages.extend([
            "CPU",
            "Boot Options",
            "High Availability",
            "Alerts",
            "Home Server",
            "GPU",
            "USB",
            "Advanced",
        ]),
        ObjectKind::Host => {
            pages.extend([
                "Alerts",
                "Multipathing",
                "Log Destination",
                "Power On",
                "Automatic Start",
            ]);
            if dialog.pool.is_some() {
                pages.extend([
                    "GPU",
                    "Live Patching",
                    "Network Options",
                    "Clustering",
                    "Advanced",
                ]);
            }
        }
        ObjectKind::Pool => pages.extend([
            "Alerts",
            "Power On",
            "GPU",
            "Security",
            "Live Patching",
            "Network Options",
            "Clustering",
            "Advanced",
        ]),
        ObjectKind::Storage => pages.extend(["Alerts", "Read Caching"]),
        ObjectKind::Disk => pages.push("Size and Location"),
        ObjectKind::Network => pages.push("Network Settings"),
    }
    if dialog.guest_setup.is_some() {
        pages.push("Guest Setup");
    }
    let mut pages: Vec<String> = pages.into_iter().map(str::to_owned).collect();
    if let LoadState::Ready(properties) = &dialog.properties
        && let Some(disk) = &properties.disk
    {
        pages.extend(
            disk.devices
                .iter()
                .map(|device| format!("device-{}", device.vbd_uuid)),
        );
    }
    pages
}

fn page_title(dialog: &PropertiesDialog, page: &str) -> String {
    dialog
        .properties
        .ready()
        .and_then(|p| p.disk.as_ref())
        .and_then(|d| {
            d.devices
                .iter()
                .find(|v| page == format!("device-{}", v.vbd_uuid))
        })
        .map(|v| v.vm_name.clone())
        .unwrap_or_else(|| page.to_owned())
}

fn summary(dialog: &PropertiesDialog, page: &str) -> String {
    if let Some(summary) = dialog
        .properties
        .ready()
        .and_then(|p| p.page_summaries.get(page))
    {
        return summary.clone();
    }
    match page {
        "General" => dialog
            .properties
            .ready()
            .map(|p| p.name.clone())
            .unwrap_or_else(|| "Loading…".into()),
        "CPU" => dialog
            .vm
            .as_ref()
            .map(|v| {
                format!(
                    "{} vCPUs · {} MiB",
                    v.vcpus_at_startup, v.memory_dynamic_max_mib
                )
            })
            .unwrap_or_default(),
        "Boot Options" => dialog
            .vm
            .as_ref()
            .map(|v| {
                if v.is_hvm {
                    format!(
                        "{} · {}",
                        if v.firmware == "uefi" { "UEFI" } else { "BIOS" },
                        boot_labels(&v.boot_order)
                    )
                } else {
                    v.pv_args.clone()
                }
            })
            .unwrap_or_default(),
        "High Availability" => {
            if dialog.restart_priority.is_empty() {
                "Do not restart".into()
            } else {
                dialog.restart_priority.clone()
            }
        }
        "Live Patching" => dialog
            .pool
            .as_ref()
            .and_then(|p| p.live_patching_disabled)
            .map(|v| if v { "Disabled" } else { "Enabled" }.into())
            .unwrap_or_else(|| "Unavailable".into()),
        "Advanced" if dialog.pool.is_some() => {
            if dialog.pool.as_ref().is_some_and(|p| p.vm_autostart) {
                "VM autostart enabled"
            } else {
                "VM autostart disabled"
            }
            .into()
        }
        "Custom Fields" => "Custom field values".into(),
        "Size and Location" => dialog
            .properties
            .ready()
            .and_then(|p| p.disk.as_ref())
            .map(|d| format!("{} · {}", crate::format::bytes(d.size_bytes), d.sr_name))
            .unwrap_or_default(),
        "Alerts" => "Not implemented yet".into(),
        "Home Server" => dialog
            .original_vm
            .as_ref()
            .map(|v| {
                if dialog.affinity.is_none() {
                    "Unassigned".into()
                } else {
                    v.affinity.clone()
                }
            })
            .unwrap_or_else(|| "Loading…".into()),
        "Log Destination" | "Power On" | "GPU" | "USB" | "Multipathing" | "Network Settings"
        | "Clustering" | "Security" | "Read Caching" | "Advanced" => "Not implemented yet".into(),
        "Automatic Start" => if dialog.pool.as_ref().is_some_and(|p| p.vm_autostart) {
            "Enabled"
        } else {
            "Disabled"
        }
        .into(),
        _ => String::new(),
    }
}

pub fn show(
    ctx: &egui::Context,
    state: &mut UiState,
    workspace: &WorkspaceView,
    actions: &mut Vec<UiAction>,
) {
    let Some(dialog) = state.properties.as_mut() else {
        return;
    };
    let connection = find_connection(workspace, &dialog.connection_id);
    let writable = connection.is_some_and(|c| is_online(c) && !is_read_only(c));
    let name = dialog
        .original
        .as_ref()
        .map(|p| p.name.as_str())
        .unwrap_or("Loading…");
    let mut open = true;
    egui::Window::new(RichText::new(format!("'{name}' Properties")).size(15.0))
        .id(egui::Id::unique(("properties", state.properties_seq)))
        .open(&mut open)
        .resizable(true)
        .collapsible(false)
        .default_size([760.0, 560.0])
        .default_pos(ctx.content_rect().center() - egui::vec2(380.0, 280.0))
        .max_size([
            ctx.content_rect().width() - 40.0,
            ctx.content_rect().height() - 80.0,
        ])
        .min_size([620.0, 420.0])
        .show(ctx, |ui| {
            let height = (ui.available_height() - 72.0)
                .clamp(300.0, (ctx.content_rect().height() - 150.0).max(300.0));
            ui.horizontal_top(|ui| {
                ui.allocate_ui_with_layout(
                    egui::vec2(240.0, height),
                    egui::Layout::top_down(egui::Align::Min),
                    |ui| {
                        egui::ScrollArea::vertical()
                            .max_height(height - 10.0)
                            .id_salt("property-tabs")
                            .show(ui, |ui| {
                                ui.spacing_mut().item_spacing.y = 0.0;
                                for page in pages(dialog) {
                                    let selected = dialog.page == page;
                                    let subtext = summary(dialog, &page);
                                    let title = page_title(dialog, &page);
                                    let response = egui::Frame::new()
                                        .fill(if selected {
                                            ui.visuals().selection.bg_fill
                                        } else {
                                            egui::Color32::TRANSPARENT
                                        })
                                        .inner_margin(egui::Margin::symmetric(8, 3))
                                        .show(ui, |ui| {
                                            ui.set_min_width(210.0);
                                            ui.set_height(32.0);
                                            ui.spacing_mut().item_spacing.y = 0.0;
                                            ui.label(
                                                RichText::new(format!(
                                                    "{}  {title}",
                                                    page_icon(&page)
                                                ))
                                                .strong(),
                                            );
                                            ui.add(
                                                egui::Label::new(
                                                    RichText::new(subtext)
                                                        .small()
                                                        .color(theme::muted_fg()),
                                                )
                                                .truncate(),
                                            );
                                        })
                                        .response;
                                    if ui
                                        .interact(
                                            response.rect,
                                            response.id.with("select"),
                                            egui::Sense::click(),
                                        )
                                        .clicked()
                                        && !dialog.submitting
                                    {
                                        dialog.page = page;
                                    }
                                }
                            });
                    },
                );
                let (separator, _) =
                    ui.allocate_exact_size(egui::vec2(1.0, height), egui::Sense::hover());
                ui.painter().vline(
                    separator.center().x,
                    separator.y_range(),
                    ui.visuals().widgets.noninteractive.bg_stroke,
                );
                ui.allocate_ui_with_layout(
                    egui::vec2(ui.available_width(), height),
                    egui::Layout::top_down(egui::Align::Min),
                    |ui| {
                        ui.label(
                            RichText::new(page_title(dialog, &dialog.page))
                                .size(15.0)
                                .strong(),
                        );
                        ui.separator();
                        egui::ScrollArea::vertical()
                            .max_height(height - 45.0)
                            .id_salt("property-content")
                            .show(ui, |ui| {
                                ui.set_min_width(310.0);
                                ui.add_enabled_ui(writable && !dialog.submitting, |ui| {
                                    content(ui, dialog, workspace);
                                });
                                if let Some(error) = &dialog.error {
                                    ui.colored_label(theme::red(), error);
                                }
                            });
                    },
                );
            });
            ui.separator();
            ui.allocate_ui_with_layout(
                egui::vec2(ui.available_width(), ui.spacing().interact_size.y),
                egui::Layout::right_to_left(egui::Align::Center),
                |ui| {
                    ui.add_space(8.0);
                    if ui
                        .add_enabled(
                            writable
                                && dialog.extras_ready
                                && dialog.original.is_some()
                                && !dialog.submitting,
                            widgets::primary_button(if dialog.submitting {
                                "Saving…"
                            } else {
                                "OK"
                            }),
                        )
                        .clicked()
                    {
                        actions.push(UiAction::SaveProperties);
                    }
                    if ui
                        .add_enabled(!dialog.submitting, egui::Button::new("Cancel"))
                        .clicked()
                    {
                        actions.push(UiAction::CloseProperties);
                    }
                    if !writable {
                        ui.label("This connection is offline or read-only.");
                    }
                },
            );
            ui.add_space(8.0);
        });
    if !open && !dialog.submitting {
        actions.push(UiAction::CloseProperties);
    }
}

fn field(ui: &mut egui::Ui, label: &str, value: &mut String) {
    widgets::field_label(ui, label);
    ui.add(egui::TextEdit::singleline(value).desired_width(f32::INFINITY));
    ui.add_space(6.0);
}

fn unavailable(ui: &mut egui::Ui, labels: &[&str]) {
    ui.label(RichText::new("Not implemented yet").color(theme::muted_fg()));
    ui.add_enabled_ui(false, |ui| {
        for label in labels {
            ui.label(*label);
            ui.add(egui::TextEdit::singleline(&mut String::new()).desired_width(f32::INFINITY));
        }
    });
}

fn content(ui: &mut egui::Ui, dialog: &mut PropertiesDialog, workspace: &WorkspaceView) {
    let page = dialog.page.clone();
    if page == "General" {
        match &mut dialog.properties {
            LoadState::Ready(p) => {
                field(ui, "Name", &mut p.name);
                widgets::field_label(ui, "Description");
                ui.add(
                    egui::TextEdit::multiline(&mut p.description)
                        .desired_width(f32::INFINITY)
                        .desired_rows(4)
                        .char_limit(xen_core::properties::MAX_DESCRIPTION_CHARS),
                );
                field(ui, "Folder", &mut p.folder);
                widgets::field_label(ui, "Tags");
                let mut remove = None;
                for (i, tag) in p.tags.iter_mut().enumerate() {
                    ui.horizontal(|ui| {
                        ui.text_edit_singleline(tag);
                        if ui.small_button("Remove").clicked() {
                            remove = Some(i);
                        }
                    });
                }
                if let Some(i) = remove {
                    p.tags.remove(i);
                }
                ui.horizontal(|ui| {
                    ui.add(egui::TextEdit::singleline(&mut dialog.new_tag).hint_text("New tag"));
                    if ui.button("Add").clicked() && !dialog.new_tag.trim().is_empty() {
                        p.tags.push(std::mem::take(&mut dialog.new_tag));
                    }
                });
            }
            LoadState::Error(e) => {
                ui.colored_label(theme::red(), e);
            }
            _ => {
                ui.spinner();
            }
        }
        return;
    }
    if page == "Guest Setup" {
        if let (Some(setup), Some(before)) = (&mut dialog.guest_setup, &dialog.original_vm) {
            let editable =
                before.power_state == "Halted" && !before.is_template && !before.is_snapshot;
            if !editable {
                ui.label("Halt the VM to edit Guest Setup.");
            }
            ui.add_enabled_ui(editable, |ui| {
                ui.checkbox(&mut dialog.reset_password, "Reset password…");
                if dialog.reset_password {
                    super::guest_setup::password(ui, setup);
                    ui.label("The guest applies the new password on next boot.");
                    if let Err(e) = setup.validate_password() {
                        ui.colored_label(theme::red(), e);
                    }
                } else {
                    super::guest_setup::fields(ui, setup, &before.guest_support);
                    if let Err(e) = setup.validate(&before.guest_support) {
                        ui.colored_label(theme::red(), e);
                    }
                }
            });
        }
        return;
    }
    if let Some(vm) = dialog.vm.as_mut() {
        match page.as_str() {
            "CPU" => {
                ui.add_enabled_ui(!vm.running, |ui| {
                    field(ui, "Number of vCPUs", &mut vm.vcpus_max);
                });
                ui.add_enabled_ui(!vm.running || vm.can_hotplug_vcpus, |ui| {
                    field(ui, "vCPUs at startup / online", &mut vm.vcpus_at_startup);
                });
                ui.add_enabled_ui(!vm.running, |ui| {
                    field(ui, "Cores per socket", &mut vm.cores_per_socket);
                    field(ui, "Priority (CPU weight)", &mut vm.cpu_weight);
                });
                ui.separator();
                ui.label("Memory");
                ui.add_enabled_ui(!vm.running, |ui| {
                    field(ui, "Static maximum (MiB)", &mut vm.memory_static_max_mib);
                });
                field(ui, "Dynamic minimum (MiB)", &mut vm.memory_dynamic_min_mib);
                field(ui, "Dynamic maximum (MiB)", &mut vm.memory_dynamic_max_mib);
                if vm.running {
                    ui.label(
                        "Stop the VM to change CPU maximum, topology, priority or static memory.",
                    );
                }
                return;
            }
            "Boot Options" => {
                if vm.is_hvm {
                    widgets::field_label(ui, "Boot order");
                    let selection_id = ui.scope_id().with("boot-order-selection");
                    let mut selected = ui
                        .ctx()
                        .data_mut(|d| d.get_temp::<char>(selection_id))
                        .unwrap_or('c');
                    let mut order: Vec<char> = vm
                        .boot_order
                        .chars()
                        .filter(|c| "cdn".contains(*c))
                        .collect();
                    for code in ['c', 'd', 'n'] {
                        if !order.contains(&code) {
                            order.push(code);
                        }
                    }
                    for code in &order {
                        ui.horizontal(|ui| {
                            let mut enabled = vm.boot_order.contains(*code);
                            if ui.checkbox(&mut enabled, "").changed() {
                                if enabled {
                                    vm.boot_order.push(*code);
                                } else {
                                    vm.boot_order.retain(|c| c != *code);
                                }
                            }
                            if ui
                                .selectable_label(selected == *code, boot_label(*code))
                                .clicked()
                            {
                                selected = *code;
                            }
                        });
                    }
                    ui.horizontal(|ui| {
                        let index = order.iter().position(|c| *c == selected).unwrap_or(0);
                        if ui
                            .add_enabled(index > 0, egui::Button::new("Move Up"))
                            .clicked()
                        {
                            order.swap(index, index - 1);
                        }
                        if ui
                            .add_enabled(index + 1 < order.len(), egui::Button::new("Move Down"))
                            .clicked()
                        {
                            order.swap(index, index + 1);
                        }
                    });
                    vm.boot_order = order
                        .into_iter()
                        .filter(|c| vm.boot_order.contains(*c))
                        .collect();
                    ui.ctx().data_mut(|d| d.insert_temp(selection_id, selected));
                    widgets::field_label(ui, "Firmware");
                    egui::ComboBox::from_id_salt("properties-firmware")
                        .selected_text(if vm.firmware == "uefi" {
                            "UEFI"
                        } else {
                            "BIOS"
                        })
                        .show_ui(ui, |ui| {
                            ui.selectable_value(&mut vm.firmware, "bios".into(), "BIOS");
                            ui.selectable_value(&mut vm.firmware, "uefi".into(), "UEFI");
                        });
                    ui.add_enabled(
                        vm.firmware == "uefi",
                        egui::Checkbox::new(&mut vm.secure_boot, "Secure boot"),
                    );
                    field(ui, "Boot policy", &mut vm.boot_policy);
                } else {
                    field(ui, "OS boot parameters", &mut vm.pv_args);
                }
                widgets::field_label(ui, "After a crash");
                egui::ComboBox::from_id_salt("properties-after-crash")
                    .selected_text(crash_label(&vm.after_crash))
                    .show_ui(ui, |ui| {
                        for code in [
                            "restart",
                            "destroy",
                            "preserve",
                            "coredump_and_restart",
                            "coredump_and_destroy",
                        ] {
                            ui.selectable_value(
                                &mut vm.after_crash,
                                code.into(),
                                crash_label(code),
                            );
                        }
                    });
                ui.checkbox(&mut vm.auto_poweron, "Auto-start VM when the host starts");
                return;
            }
            "High Availability" => {
                egui::ComboBox::from_id_salt("properties-ha")
                    .selected_text(if dialog.restart_priority.is_empty() {
                        "Do not restart"
                    } else {
                        &dialog.restart_priority
                    })
                    .show_ui(ui, |ui| {
                        for (v, label) in [
                            ("", "Do not restart"),
                            ("restart", "Restart"),
                            ("best-effort", "Restart if possible"),
                        ] {
                            ui.selectable_value(&mut dialog.restart_priority, v.into(), label);
                        }
                    });
                ui.label("Start delay (seconds)");
                ui.add(egui::DragValue::new(&mut dialog.start_delay).range(0..=604800));
                ui.label("Shutdown delay (seconds)");
                ui.add(egui::DragValue::new(&mut dialog.shutdown_delay).range(0..=604800));
                return;
            }
            "Home Server" => {
                let hosts = find_connection(workspace, &dialog.connection_id)
                    .and_then(crate::state::connection_inventory)
                    .map(|i| i.hosts.as_slice())
                    .unwrap_or_default();
                let label = dialog
                    .affinity
                    .as_ref()
                    .and_then(|id| hosts.iter().find(|h| h.uuid == *id || h.reference == *id))
                    .map(|h| h.name.as_str())
                    .unwrap_or("No preference");
                egui::ComboBox::from_id_salt("properties-affinity")
                    .selected_text(label)
                    .show_ui(ui, |ui| {
                        ui.selectable_value(&mut dialog.affinity, None, "No preference");
                        for host in hosts {
                            ui.selectable_value(
                                &mut dialog.affinity,
                                Some(host.uuid.clone()),
                                &host.name,
                            );
                        }
                    });
                return;
            }
            _ => {}
        }
    }
    if let Some(pool) = dialog.pool.as_mut() {
        match page.as_str() {
            "Live Patching" => {
                if let Some(disabled) = pool.live_patching_disabled.as_mut() {
                    let mut enabled = !*disabled;
                    if ui.checkbox(&mut enabled, "Enable live patching").changed() {
                        *disabled = !enabled;
                    }
                } else {
                    unavailable(ui, &["Enable live patching"]);
                }
                return;
            }
            "Network Options" => {
                if let Some(enabled) = pool.igmp_snooping_enabled.as_mut() {
                    ui.checkbox(enabled, "Enable IGMP snooping");
                } else {
                    unavailable(ui, &["Enable IGMP snooping"]);
                }
                return;
            }
            "Advanced" | "Automatic Start" => {
                if let Some(enabled) = pool.migration_compression.as_mut() {
                    ui.checkbox(enabled, "Compress migration traffic");
                }
                ui.checkbox(&mut pool.vm_autostart, "Enable VM autostart for this pool");
                return;
            }
            _ => {}
        }
    }
    if let LoadState::Ready(p) = &mut dialog.properties
        && let Some(disk) = &mut p.disk
    {
        if page == "Size and Location" {
            ui.label(format!("Storage repository: {}", disk.sr_name));
            ui.label("Size (GiB)");
            let mut gib = disk.size_bytes as f64 / (1024.0 * 1024.0 * 1024.0);
            if ui
                .add_enabled(
                    !disk.devices.is_empty(),
                    egui::DragValue::new(&mut gib).range(0.001..=1_048_576.0),
                )
                .changed()
            {
                disk.size_bytes = (gib * 1024.0 * 1024.0 * 1024.0) as u64;
            }
            return;
        }
        if let Some(device) = disk
            .devices
            .iter_mut()
            .find(|d| format!("device-{}", d.vbd_uuid) == page)
        {
            widgets::field_label(ui, "Mode");
            egui::ComboBox::from_id_salt(("properties-mode", &device.vbd_uuid))
                .selected_text(&device.mode)
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut device.mode, "RW".into(), "Read/write");
                    ui.selectable_value(&mut device.mode, "RO".into(), "Read-only");
                });
            ui.checkbox(&mut device.bootable, "Bootable");
            ui.add_enabled(
                false,
                egui::Label::new(format!("Position: {}", device.position)),
            )
            .on_disabled_hover_text("Not implemented yet");
            return;
        }
    }
    let labels: &[&str] = match page.as_str() {
        "Custom Fields" => &["Custom fields"],
        "Alerts" => &["CPU usage", "Memory usage", "Disk usage"],
        "Multipathing" => &["Enable multipathing"],
        "Log Destination" => &["System log server"],
        "Power On" => &["Power-on mode"],
        "GPU" => &["GPU assignment"],
        "USB" => &["USB devices"],
        "Advanced" => &["Optimization", "Shadow memory multiplier"],
        "Clustering" => &["Enable clustering"],
        "Security" => &["SSL legacy policy"],
        "Read Caching" => &["Enable read caching"],
        "Network Settings" => &["MTU", "Automatically add to new servers"],
        _ => &["Settings"],
    };
    unavailable(ui, labels);
}

fn boot_label(code: char) -> &'static str {
    match code {
        'c' => "Hard Disk",
        'd' => "DVD-Drive",
        'n' => "Network",
        _ => "Unknown",
    }
}
fn boot_labels(order: &str) -> String {
    order.chars().map(boot_label).collect::<Vec<_>>().join(", ")
}
fn crash_label(code: &str) -> &str {
    match code {
        "restart" => "Restart",
        "destroy" => "Shut down",
        "preserve" => "Preserve",
        "coredump_and_restart" => "Restart, recording a core dump",
        "coredump_and_destroy" => "Shut down, recording a core dump",
        _ => code,
    }
}
fn page_icon(page: &str) -> &'static str {
    match page {
        "General" => "▤",
        "Custom Fields" => "▦",
        "CPU" => "▣",
        "Boot Options" => "↥",
        "High Availability" => "♥",
        "Alerts" => "⚠",
        "Home Server" => "⌂",
        "GPU" => "▧",
        "USB" => "⑂",
        "Advanced" => "⚙",
        "Log Destination" => "≡",
        "Power On" => "⏻",
        "Multipathing" => "⇉",
        _ => "▥",
    }
}
