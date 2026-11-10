use egui::{Align, Layout, RichText, Sense, Ui};
use xen_core::workspace::{ConnectionView, WorkspaceView};

use crate::format;
use crate::state::{
    ObjectKey, UiAction, UiState, connection_inventory, connection_name, connection_settings,
    connection_status, is_read_only,
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
    let Some(ObjectKey::Pool(connection_id)) = state
        .object_tabs
        .get(state.active_tab)
        .map(|tab| tab.key.clone())
    else {
        return;
    };
    let Some(connection) = workspace
        .connections
        .iter()
        .find(|connection| crate::state::connection_id(connection) == connection_id)
    else {
        widgets::empty_state(ui, "Pool unavailable", "The saved connection was removed.");
        return;
    };
    let name = connection_name(connection);
    let settings = connection_settings(connection);
    let status = connection_status(connection);
    let read_only = is_read_only(connection);

    if matches!(connection, ConnectionView::Offline { .. }) {
        ui.add_space(50.0);
        ui.vertical_centered(|ui| {
            widgets::offline_mark(ui, 64.0);
            ui.add_space(10.0);
            ui.label(
                RichText::new(format!("{name} is offline"))
                    .size(20.0)
                    .strong(),
            );
            ui.label(
                RichText::new(format!("{} · {}", settings.host, settings.username))
                    .size(12.0)
                    .color(muted_fg()),
            );
            ui.add_space(10.0);
            if ui
                .add(widgets::primary_button("Connect to this host"))
                .clicked()
            {
                state.connect = Some(crate::state::ConnectState::from_profile(
                    connection_id.clone(),
                    settings.clone(),
                ));
            }
        });
        return;
    }

    let Some(inventory) = connection_inventory(connection) else {
        widgets::empty_state(ui, "Inventory unavailable", "Refresh this connection.");
        return;
    };

    ui.horizontal(|ui| {
        widgets::server_mark(ui, 44.0);
        ui.vertical(|ui| {
            ui.spacing_mut().item_spacing.y = 0.0;
            ui.label(RichText::new("POOL").size(9.5).color(dim_fg()));
            ui.label(RichText::new(&name).size(24.0).strong());
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new(&settings.host)
                        .monospace()
                        .size(10.5)
                        .color(muted_fg()),
                );
                ui.label(RichText::new("·").size(10.5).color(dim_fg()));
                ui.label(
                    RichText::new(format!("Pool UUID {}", inventory.pool_id))
                        .monospace()
                        .size(9.5)
                        .color(dim_fg()),
                );
            });
        });
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            if ui
                .add(egui::Button::new(RichText::new("Refresh").size(11.5)))
                .clicked()
            {
                actions.push(UiAction::Refresh(Some(connection_id.clone())));
            }
            widgets::badge(
                ui,
                match status {
                    "online" => "Connected",
                    "stale" => "Needs attention",
                    _ => "Offline",
                },
                widgets::connection_color(status),
            );
            if read_only {
                widgets::badge(ui, "Read-only", amber());
            }
        });
    });
    ui.add_space(10.0);

    let storage_total: u64 = inventory.storage.iter().map(|sr| sr.total).sum();
    let storage_used: u64 = inventory.storage.iter().map(|sr| sr.used).sum();
    stat_grid(
        ui,
        &[
            (
                "Hosts",
                inventory.hosts.len().to_string(),
                format::plural(inventory.hosts.len(), "host", "hosts"),
                cyan(),
            ),
            (
                "Virtual machines",
                inventory.vms.len().to_string(),
                format!(
                    "{} running",
                    inventory
                        .vms
                        .iter()
                        .filter(|vm| vm.power_state == "Running")
                        .count()
                ),
                green(),
            ),
            (
                "Storage",
                format::bytes(storage_used),
                format!("/ {}", format::bytes(storage_total)),
                accent(),
            ),
            (
                "Networks",
                inventory
                    .networks
                    .iter()
                    .filter(|network| network.bridge != "xenapi")
                    .count()
                    .to_string(),
                "pool networks".into(),
                purple(),
            ),
        ],
    );
    ui.add_space(GAP);
    if !inventory.alerts.is_empty() {
        alerts_card(ui, inventory, &connection_id, read_only, actions);
        ui.add_space(GAP);
    }

    ui.columns(2, |columns| {
        hosts_card(&mut columns[0], inventory, &connection_id, actions);
        storage_card(&mut columns[1], inventory, &connection_id, actions);
    });
    ui.add_space(GAP);
    ui.columns(2, |columns| {
        networks_card(&mut columns[0], inventory, &connection_id, actions);
        vms_card(&mut columns[1], inventory, &connection_id, actions);
    });
}

fn alerts_card(
    ui: &mut Ui,
    inventory: &xen_core::xapi::Inventory,
    connection_id: &str,
    read_only: bool,
    actions: &mut Vec<UiAction>,
) {
    theme::card_frame().show(ui, |ui| {
        ui.set_width(ui.available_width());
        card_heading(ui, "Alerts", inventory.alerts.len(), |ui| {
            if inventory.alerts.len() > 6 {
                ui.label(
                    RichText::new(format!("showing 6 of {}", inventory.alerts.len()))
                        .size(9.5)
                        .color(dim_fg()),
                );
            }
        });
        for alert in inventory.alerts.iter().take(6) {
            ui.horizontal(|ui| {
                let (label, color) = match alert.priority {
                    1 => ("Critical", red()),
                    2 => ("Major", amber()),
                    3 => ("Minor", amber()),
                    _ => ("Warning", accent()),
                };
                widgets::dot(ui, color, 7.0);
                ui.vertical(|ui| {
                    ui.spacing_mut().item_spacing.y = 1.0;
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(&alert.name).size(11.0).strong());
                        ui.label(RichText::new(label).size(9.0).color(color));
                        if alert.time > 0 {
                            ui.label(
                                RichText::new(format::short_time(alert.time))
                                    .size(9.0)
                                    .color(dim_fg()),
                            );
                        }
                    });
                    let body = if alert.body.chars().count() > 180 {
                        format!("{}…", alert.body.chars().take(180).collect::<String>())
                    } else {
                        alert.body.clone()
                    };
                    ui.label(RichText::new(body).size(9.5).color(muted_fg()));
                    ui.label(
                        RichText::new(format!(
                            "{} · {}",
                            alert.class,
                            if alert.obj_uuid.is_empty() {
                                "unknown object"
                            } else {
                                &alert.obj_uuid
                            }
                        ))
                        .monospace()
                        .size(8.5)
                        .color(dim_fg()),
                    );
                });
                if !read_only {
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if ui
                            .add(egui::Button::new(RichText::new("Dismiss").size(9.5)))
                            .clicked()
                        {
                            actions.push(UiAction::DismissAlert {
                                connection_id: connection_id.to_owned(),
                                message_uuid: alert.uuid.clone(),
                            });
                        }
                    });
                }
            });
            ui.add_space(4.0);
        }
        if read_only {
            ui.label(
                RichText::new("Read-only connection: alerts cannot be dismissed.")
                    .size(9.0)
                    .color(dim_fg()),
            );
        }
    });
}

fn stat_grid(ui: &mut Ui, cards: &[(&str, String, String, egui::Color32)]) {
    ui.columns(cards.len().max(1), |columns| {
        for (index, (title, value, sub, color)) in cards.iter().enumerate() {
            if let Some(column) = columns.get_mut(index) {
                theme::card_frame()
                    .inner_margin(egui::Margin::same(14))
                    .show(column, |ui| {
                        ui.set_width(ui.available_width());
                        ui.horizontal(|ui| {
                            ui.label(RichText::new(*title).size(10.5).color(muted_fg()));
                            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                widgets::dot(ui, *color, 6.0);
                            });
                        });
                        ui.label(RichText::new(value).size(24.0).strong());
                        ui.label(RichText::new(sub).size(9.5).color(dim_fg()));
                    });
            }
        }
    });
}

fn hosts_card(
    ui: &mut Ui,
    inventory: &xen_core::xapi::Inventory,
    connection_id: &str,
    actions: &mut Vec<UiAction>,
) {
    theme::card_frame().show(ui, |ui| {
        ui.set_width(ui.available_width());
        card_heading(ui, "Hosts", inventory.hosts.len(), |ui| {
            if ui.button(RichText::new("View all ›").size(10.0)).clicked() {
                actions.push(UiAction::OpenObject {
                    key: ObjectKey::List {
                        connection_id: Some(connection_id.to_owned()),
                        kind: crate::state::ListKind::Hosts,
                        host_ref: None,
                    },
                    title: "Hosts".into(),
                });
            }
        });
        for host in &inventory.hosts {
            ui.horizontal(|ui| {
                widgets::dot(ui, if host.enabled { green() } else { amber() }, 7.0);
                if ui
                    .add(
                        egui::Label::new(RichText::new(&host.name).size(12.0))
                            .sense(Sense::click()),
                    )
                    .clicked()
                {
                    actions.push(UiAction::OpenObject {
                        key: ObjectKey::Host {
                            connection_id: connection_id.to_owned(),
                            host_ref: host.reference.clone(),
                        },
                        title: host.name.clone(),
                    });
                }
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    ui.label(RichText::new(&host.version).size(9.5).color(dim_fg()));
                    ui.label(
                        RichText::new(format!(
                            "{} CPUs · {}",
                            host.cpu_count,
                            format::bytes(host.memory_total)
                        ))
                        .size(9.5)
                        .color(muted_fg()),
                    );
                    ui.label(
                        RichText::new(&host.address)
                            .monospace()
                            .size(9.0)
                            .color(dim_fg()),
                    );
                });
            });
        }
    });
}

fn storage_card(
    ui: &mut Ui,
    inventory: &xen_core::xapi::Inventory,
    connection_id: &str,
    actions: &mut Vec<UiAction>,
) {
    theme::card_frame().show(ui, |ui| {
        ui.set_width(ui.available_width());
        let visible: Vec<_> = inventory
            .storage
            .iter()
            .filter(|sr| {
                !(sr.storage_type == "udev"
                    || sr.storage_type == "tmpfs"
                    || sr.name.starts_with("__gui__")
                    || sr.name == "XenServer Tools"
                    || sr.name == "XCP-ng Tools")
            })
            .collect();
        card_heading(ui, "Storage", visible.len(), |ui| {
            if ui.button(RichText::new("View all ›").size(10.0)).clicked() {
                actions.push(UiAction::OpenObject {
                    key: ObjectKey::List {
                        connection_id: Some(connection_id.to_owned()),
                        kind: crate::state::ListKind::Storage,
                        host_ref: None,
                    },
                    title: "Storage".into(),
                });
            }
        });
        for sr in visible {
            ui.horizontal(|ui| {
                if ui
                    .add(egui::Label::new(RichText::new(&sr.name).size(11.5)).sense(Sense::click()))
                    .clicked()
                {
                    actions.push(UiAction::OpenObject {
                        key: ObjectKey::Storage {
                            connection_id: connection_id.to_owned(),
                            sr_uuid: sr.uuid.clone(),
                        },
                        title: sr.name.clone(),
                    });
                }
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    ui.label(
                        RichText::new(if sr.total == 0 {
                            "Not reported".into()
                        } else {
                            format::bytes(sr.total)
                        })
                        .monospace()
                        .size(9.5)
                        .color(muted_fg()),
                    );
                    ui.scope(|ui| {
                        ui.set_width(90.0);
                        widgets::resource_bar(ui, sr.used, sr.total, green(), 5.0);
                    });
                });
            });
        }
    });
}

fn networks_card(
    ui: &mut Ui,
    inventory: &xen_core::xapi::Inventory,
    connection_id: &str,
    actions: &mut Vec<UiAction>,
) {
    theme::card_frame().show(ui, |ui| {
        ui.set_width(ui.available_width());
        let visible: Vec<_> = inventory
            .networks
            .iter()
            .filter(|network| network.bridge != "xenapi")
            .collect();
        card_heading(ui, "Networks", visible.len(), |ui| {
            if ui.button(RichText::new("View all ›").size(10.0)).clicked() {
                actions.push(UiAction::OpenObject {
                    key: ObjectKey::List {
                        connection_id: Some(connection_id.to_owned()),
                        kind: crate::state::ListKind::Networks,
                        host_ref: None,
                    },
                    title: "Networks".into(),
                });
            }
        });
        for network in visible {
            ui.horizontal(|ui| {
                if ui
                    .add(
                        egui::Label::new(RichText::new(&network.name).size(11.5))
                            .sense(Sense::click()),
                    )
                    .clicked()
                {
                    actions.push(UiAction::OpenObject {
                        key: ObjectKey::Network {
                            connection_id: connection_id.to_owned(),
                            network_uuid: network.uuid.clone(),
                        },
                        title: network.name.clone(),
                    });
                }
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    ui.label(
                        RichText::new(format!(
                            "{} PIF / {} VIF",
                            network.interface_count, network.virtual_interface_count
                        ))
                        .size(9.5)
                        .color(dim_fg()),
                    );
                    ui.label(
                        RichText::new(&network.bridge)
                            .monospace()
                            .size(9.0)
                            .color(muted_fg()),
                    );
                });
            });
        }
    });
}

fn vms_card(
    ui: &mut Ui,
    inventory: &xen_core::xapi::Inventory,
    connection_id: &str,
    actions: &mut Vec<UiAction>,
) {
    theme::card_frame().show(ui, |ui| {
        ui.set_width(ui.available_width());
        card_heading(ui, "Virtual machines", inventory.vms.len(), |ui| {
            if ui.button(RichText::new("View all ›").size(10.0)).clicked() {
                actions.push(UiAction::OpenObject {
                    key: ObjectKey::List {
                        connection_id: Some(connection_id.to_owned()),
                        kind: crate::state::ListKind::Vms,
                        host_ref: None,
                    },
                    title: "Virtual machines".into(),
                });
            }
        });
        for vm in inventory.vms.iter().take(7) {
            ui.horizontal(|ui| {
                widgets::dot(ui, widgets::power_style(&vm.power_state).1, 6.0);
                if ui
                    .add(egui::Label::new(RichText::new(&vm.name).size(11.5)).sense(Sense::click()))
                    .clicked()
                {
                    actions.push(UiAction::OpenObject {
                        key: ObjectKey::Vm {
                            connection_id: connection_id.to_owned(),
                            vm_uuid: vm.uuid.clone(),
                        },
                        title: vm.name.clone(),
                    });
                }
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    ui.label(
                        RichText::new(widgets::power_style(&vm.power_state).0)
                            .size(9.5)
                            .color(widgets::power_style(&vm.power_state).1),
                    );
                    ui.label(RichText::new(&vm.host_name).size(9.0).color(dim_fg()));
                });
            });
        }
    });
}

fn card_heading(ui: &mut Ui, title: &str, count: usize, right: impl FnOnce(&mut Ui)) {
    ui.horizontal(|ui| {
        ui.label(RichText::new(title).size(14.0).strong());
        widgets::badge(ui, count.to_string(), muted_fg());
        ui.with_layout(Layout::right_to_left(Align::Center), right);
    });
    ui.add_space(4.0);
}
