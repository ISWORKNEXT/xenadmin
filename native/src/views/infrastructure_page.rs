//! Host and pool tabs use the same bodies, with host-scoped records supplied per host.
use super::{
    object_page::{self, Selection, value_row},
    sidebar::TreeIcon,
};
use crate::{
    format,
    state::{self, HostTab, LoadState, ObjectKey, UiAction, UiState, VmTab},
    widgets,
};
use egui::{Align, Layout, RichText, Sense, Ui};
use egui_extras::{Column, TableBuilder};
use xen_core::{
    host::{DataSection, DetailRecord, HostDetails, InfoGroup},
    properties::{ObjectKind, ObjectTarget},
    workspace::WorkspaceView,
};

pub fn show(
    ui: &mut Ui,
    state: &mut UiState,
    workspace: &WorkspaceView,
    actions: &mut Vec<UiAction>,
) {
    let Some(key) = state.selection.as_ref().map(|s| s.key.clone()) else {
        return;
    };
    let Some(id) = key.connection_id() else {
        return;
    };
    let Some(connection) = state::find_connection(workspace, id) else {
        return;
    };
    let inv = state::connection_inventory(connection);
    let host = if let ObjectKey::Host { host_uuid, .. } = &key {
        inv.and_then(|inv| inv.hosts.iter().find(|h| &h.uuid == host_uuid))
    } else {
        None
    };
    let name = host.map_or_else(|| state::connection_name(connection), |h| h.name.clone());
    let live = state::is_online(connection)
        && host.is_none_or(|h| {
            inv.and_then(|i| i.navigation.get(&h.uuid))
                .is_some_and(|m| m.live)
        });
    object_page::title_strip(
        ui,
        connection,
        &name,
        if host.is_some() {
            TreeIcon::Host
        } else {
            TreeIcon::Pool
        },
        if live { "Running" } else { "Halted" },
    );
    let details: Vec<HostDetails> = inv
        .map(|inv| {
            inv.hosts
                .iter()
                .filter(|h| host.is_none_or(|selected| selected.uuid == h.uuid))
                .filter_map(|h| {
                    let cache_key = (id.to_owned(), h.uuid.clone());
                    if let Some(ready) = state.host_state.ready().filter(|d| d.host_uuid == h.uuid)
                        && state
                            .host_route
                            .as_ref()
                            .is_some_and(|r| r.connection_id == id)
                    {
                        return Some(ready.clone());
                    }
                    match state.page_hosts.get(&cache_key) {
                        Some(LoadState::Ready(details)) => Some(details.clone()),
                        Some(LoadState::Error(_)) | Some(LoadState::Loading) => None,
                        _ => {
                            if live {
                                actions.push(UiAction::LoadPageHost {
                                    connection_id: id.into(),
                                    host_uuid: h.uuid.clone(),
                                });
                            }
                            None
                        }
                    }
                })
                .collect()
        })
        .unwrap_or_default();
    let usb = details
        .iter()
        .any(|d| matches!(&d.usb, DataSection::Ready { rows, .. } if !rows.is_empty()));
    let tabs = object_page::tabs_for(if matches!(key, ObjectKey::Host { .. }) {
        Selection::Host { live, usb }
    } else if live {
        Selection::Pool
    } else {
        Selection::Host {
            live: false,
            usb: false,
        }
    });
    let mut tab = state.object_tab;
    object_page::tab_strip(ui, &tabs, &mut tab);
    state.object_tab = tab;
    ui.add_space(6.0);
    if tab == VmTab::Search {
        super::search_page::show(ui, state, workspace, actions);
        return;
    }
    if tab == VmTab::Performance {
        if state.host_tab != HostTab::Performance {
            actions.push(UiAction::SetHostTab(HostTab::Performance));
        }
        super::host_details::performance(ui, state, actions);
        return;
    }
    if tab == VmTab::Console {
        // The current console transport resolves VM UUIDs; inventory excludes dom0.
        ui.label("Host console is not implemented yet.");
        return;
    }
    let Some(inv) = inv else {
        ui.label("Server disconnected.");
        return;
    };
    let read_only = state::is_read_only(connection);
    for h in inv
        .hosts
        .iter()
        .filter(|h| host.is_none_or(|selected| selected.uuid == h.uuid))
    {
        if let Some(LoadState::Error(error)) = state.page_hosts.get(&(id.into(), h.uuid.clone())) {
            ui.colored_label(crate::theme::red(), format!("{}: {error}", h.name));
            if ui.button("Retry").clicked() {
                actions.push(UiAction::LoadPageHost {
                    connection_id: id.into(),
                    host_uuid: h.uuid.clone(),
                });
            }
        }
    }
    match tab {
        VmTab::General => {
            if ui.button("Properties").clicked() {
                let (kind, uuid) = host.map_or((ObjectKind::Pool, inv.pool_id.as_str()), |host| {
                    (ObjectKind::Host, host.uuid.as_str())
                });
                properties(actions, id, kind, uuid);
            }
            if let Some(host) = host {
                if let Some(details) = details.first() {
                    host_general(ui, details);
                } else {
                    value_row(ui, "Name", &host.name);
                    value_row(ui, "Address", &host.address);
                    ui.spinner();
                }
            } else {
                section(ui, "General", |ui| {
                    value_row(ui, "Name", &inv.pool_name);
                    value_row(ui, "UUID", &inv.pool_id);
                    value_row(ui, "Servers", &inv.hosts.len().to_string());
                });
                for details in &details {
                    section(ui, &details.name, |ui| host_general(ui, details));
                }
            }
        }
        VmTab::Memory => {
            for h in inv
                .hosts
                .iter()
                .filter(|h| host.is_none_or(|selected| selected.uuid == h.uuid))
            {
                section(ui, &h.name, |ui| {
                    let used = h.memory_total.saturating_sub(h.memory_free);
                    value_row(ui, "Total", &format::bytes(h.memory_total));
                    value_row(ui, "Used", &format::bytes(used));
                    if let Some(d) = details.iter().find(|d| d.host_uuid == h.uuid) {
                        for group in &d.memory {
                            for field in &group.fields {
                                if field.label == "Control domain memory" {
                                    value_row(ui, "Control domain", &field.value);
                                }
                            }
                        }
                    }
                    widgets::resource_bar(ui, used, h.memory_total, crate::theme::green(), 14.0);
                    for vm in inv.vms.iter().filter(|v| {
                        inv.navigation
                            .get(&v.uuid)
                            .is_some_and(|m| m.home == h.reference)
                    }) {
                        value_row(ui, &vm.name, &format::bytes(vm.memory));
                        widgets::resource_bar(
                            ui,
                            vm.memory,
                            h.memory_total,
                            crate::theme::accent(),
                            10.0,
                        );
                    }
                });
            }
        }
        VmTab::Storage => {
            let rows: Vec<_> = inv
                .storage
                .iter()
                .filter(|sr| {
                    host.is_none()
                        || details.iter().any(|d| {
                            records(&d.storage)
                                .iter()
                                .any(|r| field(r, "Storage repository", "UUID") == sr.uuid)
                        })
                })
                .map(|sr| {
                    let allocation = details
                        .iter()
                        .flat_map(|d| records(&d.storage))
                        .find(|r| field(r, "Storage repository", "UUID") == sr.uuid)
                        .and_then(|r| r.values.get("Virtual allocation"))
                        .cloned()
                        .unwrap_or_else(|| "—".into());
                    (
                        sr.uuid.clone(),
                        vec![
                            sr.name.clone(),
                            sr.description.clone(),
                            sr.storage_type.clone(),
                            yes(sr.shared),
                            format::bytes(sr.used),
                            format::bytes(sr.total),
                            allocation,
                        ],
                    )
                })
                .collect();
            let selected = table(
                ui,
                "physical-storage",
                &[
                    "Name",
                    "Description",
                    "Type",
                    "Shared",
                    "Usage",
                    "Size",
                    "Virtual allocation",
                ],
                &rows,
            );
            ui.horizontal(|ui| {
                if ui
                    .add_enabled(!read_only, egui::Button::new("New SR…"))
                    .clicked()
                {
                    actions.push(UiAction::OpenSrCreate);
                }
                let sr = inv.storage.iter().find(|s| s.uuid == selected);
                // Use the same command resolver as the menu for backend capability and selection guards.
                let trim = sr.and_then(|sr| {
                    let selected_state = UiState {
                        selection: Some(state::ObjectSelection {
                            key: ObjectKey::Storage {
                                connection_id: id.into(),
                                sr_uuid: sr.uuid.clone(),
                            },
                            title: sr.name.clone(),
                        }),
                        ..Default::default()
                    };
                    super::menubar::resolve("TrimSRCommand", &selected_state, workspace).ok()
                });
                if ui
                    .add_enabled(trim.is_some(), egui::Button::new("Trim… / Reclaim"))
                    .clicked()
                    && let Some(super::menubar::CommandAction::Dispatch(action)) = trim
                {
                    actions.push(action);
                }
                if ui
                    .add_enabled(sr.is_some(), egui::Button::new("Properties"))
                    .clicked()
                {
                    properties(actions, id, ObjectKind::Storage, &selected);
                }
            });
        }
        VmTab::Network => networking(ui, state, id, inv, &details, read_only, actions),
        VmTab::Nics => {
            let rows: Vec<_> = details
                .iter()
                .flat_map(|d| records(&d.nics))
                .filter(|r| field(r, "Interface", "Physical") == "Yes")
                .map(|r| {
                    (
                        r.id.clone(),
                        vec![
                            value(r, "Interface"),
                            value(r, "MAC"),
                            value(r, "Link"),
                            value(r, "Speed (Mbps)"),
                            field(r, "Link and hardware", "Full duplex"),
                            field(r, "Link and hardware", "Vendor"),
                            field(r, "Link and hardware", "Model"),
                            field(r, "Link and hardware", "PCI address"),
                            field(r, "FCoE and SR-IOV", "FCoE capable"),
                            field(r, "FCoE and SR-IOV", "SR-IOV capable"),
                        ],
                    )
                })
                .collect();
            let selected = table_with_menu(
                ui,
                "nics",
                &[
                    "NIC",
                    "MAC",
                    "Link Status",
                    "Speed",
                    "Duplex",
                    "Vendor",
                    "Device",
                    "PCI Bus Path",
                    "FCoE",
                    "SR-IOV",
                ],
                &rows,
                |ui, key| {
                    let action = delete_bond_action(state, id, key, read_only, inv);
                    if ui
                        .add_enabled(action.is_some(), egui::Button::new("Delete Bond"))
                        .clicked()
                        && let Some(action) = action
                    {
                        actions.push(action);
                        ui.close();
                    }
                    ui.separator();
                    if ui.button("Copy").clicked() {
                        if let Some((_, values)) = rows.iter().find(|(id, _)| id == key) {
                            ui.ctx().copy_text(values.join("\t"));
                        }
                        ui.close();
                    }
                },
            );
            load_pifs(state, id, actions);
            ui.horizontal(|ui| {
                let can_bond = !read_only
                    && state
                        .pif_catalog
                        .iter()
                        .filter(|p| {
                            host.is_none_or(|h| p.host_uuid == h.uuid) && p.bond_candidate()
                        })
                        .count()
                        >= 2;
                if ui
                    .add_enabled(can_bond, egui::Button::new("Create Bond…"))
                    .clicked()
                {
                    actions.push(UiAction::OpenBondWizard);
                }
                let action = delete_bond_action(state, id, &selected, read_only, inv);
                if ui
                    .add_enabled(action.is_some(), egui::Button::new("Delete Bond"))
                    .clicked()
                    && let Some(action) = action
                {
                    actions.push(action);
                }
                if ui
                    .add_enabled(!read_only && host.is_some(), egui::Button::new("Rescan"))
                    .clicked()
                    && let Some(host) = host
                {
                    actions.push(UiAction::PromptHostAction {
                        host_uuid: host.uuid.clone(),
                        host_name: host.name.clone(),
                        action: xen_core::operations::HostAction::RescanPifs,
                    });
                }
            });
        }
        VmTab::Gpu | VmTab::Usb => {
            for d in &details {
                let (title, data) = if tab == VmTab::Gpu {
                    ("GPU", &d.gpus)
                } else {
                    ("USB", &d.usb)
                };
                inventory_table(ui, &format!("{}-{title}", d.host_uuid), data);
            }
            ui.horizontal(|ui| {
                disabled(ui, "Properties");
            });
        }
        _ => {}
    }
}

fn host_general(ui: &mut Ui, details: &HostDetails) {
    section(ui, "General", |ui| {
        for name in ["Identity", "Host configuration", "Custom fields"] {
            if let Some(g) = details.general.iter().find(|g| g.title == name) {
                fields(ui, g);
            }
        }
    });
    section(ui, "Management Interfaces", |ui| {
        for r in records(&details.nics)
            .iter()
            .filter(|r| value(r, "Management") == "Yes")
        {
            value_row(ui, &value(r, "Interface"), &value(r, "IPv4"));
        }
    });
    section(ui, "Memory", |ui| {
        for g in &details.memory {
            fields(ui, g);
        }
    });
    section(ui, "Version Details", |ui| {
        for g in details.general.iter().filter(|g| g.title == "Software") {
            fields(ui, g);
        }
    });
    section(ui, "CPUs", |ui| {
        for g in details.general.iter().filter(|g| g.title == "CPU summary") {
            fields(ui, g);
        }
    });
    section(ui, "Updates", |ui| {
        inventory_table(ui, "general-updates", &details.updates);
    });
    if let Some(g) = details.general.iter().find(|g| g.title == "License") {
        section(ui, "License Details", |ui| fields(ui, g));
    }
}
fn fields(ui: &mut Ui, group: &InfoGroup) {
    for f in &group.fields {
        value_row(ui, &f.label, &f.value);
    }
}
fn yes(value: bool) -> String {
    if value { "Yes" } else { "No" }.into()
}
fn value(row: &DetailRecord, label: &str) -> String {
    row.values.get(label).cloned().unwrap_or_else(|| "—".into())
}
fn field(row: &DetailRecord, group: &str, label: &str) -> String {
    row.groups
        .iter()
        .find(|g| g.title == group)
        .and_then(|g| g.fields.iter().find(|f| f.label == label))
        .map(|f| f.value.clone())
        .unwrap_or_else(|| "—".into())
}
fn records(section: &DataSection) -> &[DetailRecord] {
    match section {
        DataSection::Ready { rows, .. } => rows,
        _ => &[],
    }
}
pub(super) fn section(ui: &mut Ui, name: &str, body: impl FnOnce(&mut Ui)) {
    egui::CollapsingHeader::new(name)
        .id_salt(name)
        .default_open(true)
        .show(ui, body);
}
pub(super) fn disabled(ui: &mut Ui, name: &str) {
    ui.add_enabled(false, egui::Button::new(name))
        .on_disabled_hover_text("Not implemented yet");
}
pub(super) fn properties(actions: &mut Vec<UiAction>, id: &str, kind: ObjectKind, uuid: &str) {
    actions.push(UiAction::OpenProperties {
        connection_id: id.into(),
        target: ObjectTarget {
            kind,
            uuid: uuid.into(),
        },
    });
}
pub(super) fn table(
    ui: &mut Ui,
    id: &str,
    columns: &[&str],
    rows: &[(String, Vec<String>)],
) -> String {
    table_with_menu(ui, id, columns, rows, |_, _| {})
}

pub(super) fn table_with_menu(
    ui: &mut Ui,
    id: &str,
    columns: &[&str],
    rows: &[(String, Vec<String>)],
    mut menu: impl FnMut(&mut Ui, &str),
) -> String {
    let selection_id = ui.scope_id().with(("selection", id));
    let mut selected = ui
        .ctx()
        .data_mut(|d| d.get_temp::<String>(selection_id))
        .unwrap_or_default();
    egui::ScrollArea::horizontal().id_salt(id).show(ui, |ui| {
        TableBuilder::new(ui)
            .id_salt(id)
            .striped(true)
            .sense(Sense::click())
            .vscroll(true)
            .min_scrolled_height(0.0)
            .max_scroll_height(460.0)
            .cell_layout(Layout::left_to_right(Align::Center))
            .columns(Column::remainder().at_least(96.0).clip(true), columns.len())
            .header(24.0, |mut header| {
                for name in columns {
                    header.col(|ui| {
                        ui.label(RichText::new(*name).size(11.0));
                    });
                }
            })
            .body(|body| {
                body.rows(26.0, rows.len(), |mut row| {
                    let (key, values) = &rows[row.index()];
                    row.set_selected(key == &selected);
                    for value in values {
                        row.col(|ui| {
                            ui.add(
                                egui::Label::new(RichText::new(value).size(11.0))
                                    .truncate()
                                    .selectable(false),
                            );
                        });
                    }
                    if row.response().clicked() || row.response().secondary_clicked() {
                        selected = key.clone();
                    }
                    row.response().context_menu(|ui| menu(ui, key));
                });
            });
    });
    if !rows.iter().any(|(key, _)| key == &selected) {
        selected.clear();
    }
    ui.ctx()
        .data_mut(|d| d.insert_temp(selection_id, selected.clone()));
    selected
}
fn inventory_table(ui: &mut Ui, id: &str, section: &DataSection) {
    match section {
        DataSection::Ready { columns, rows } => {
            let cells = rows
                .iter()
                .map(|r| (r.id.clone(), columns.iter().map(|c| value(r, c)).collect()))
                .collect::<Vec<_>>();
            table(
                ui,
                id,
                &columns.iter().map(String::as_str).collect::<Vec<_>>(),
                &cells,
            );
            if rows.is_empty() {
                ui.label("No devices or records reported.");
            }
        }
        DataSection::Unsupported { message } | DataSection::Unavailable { message } => {
            ui.label(message);
        }
    }
}
fn load_pifs(state: &mut UiState, id: &str, actions: &mut Vec<UiAction>) {
    if state.pif_catalog_connection.as_deref() != Some(id) {
        state.pif_catalog.clear();
        state.pif_catalog_error = None;
        state.pif_catalog_connection = Some(id.into());
        state.pif_catalog_loading = true;
        state.pif_catalog_seq += 1;
        actions.push(UiAction::LoadPifCatalog {
            seq: state.pif_catalog_seq,
            connection_id: id.into(),
        });
    }
}
fn network_commands(
    ui: &mut Ui,
    id: &str,
    inv: &xen_core::xapi::Inventory,
    read_only: bool,
    selected: &str,
    actions: &mut Vec<UiAction>,
) {
    super::context_menu::command_strip(ui, |ui| {
        if ui
            .add_enabled(!read_only, egui::Button::new("Add Network…"))
            .on_disabled_hover_text("The object state or connection does not allow this operation.")
            .clicked()
        {
            actions.push(UiAction::OpenNetworkChooser);
        }
        if ui
            .add_enabled(!selected.is_empty(), egui::Button::new("Properties"))
            .on_disabled_hover_text("The object state or connection does not allow this operation.")
            .clicked()
        {
            properties(actions, id, ObjectKind::Network, selected);
        }
        let removable = !read_only
            && inv
                .networks
                .iter()
                .find(|n| n.uuid == selected)
                .is_some_and(|n| n.interface_count == 0 && n.virtual_interface_count == 0);
        if ui
            .add_enabled(removable, egui::Button::new("Remove"))
            .on_disabled_hover_text("The object state or connection does not allow this operation.")
            .clicked()
        {
            actions.push(UiAction::PromptDestroyNetwork {
                connection_id: id.into(),
                network_uuid: selected.to_owned(),
                name: inv
                    .networks
                    .iter()
                    .find(|n| n.uuid == selected)
                    .unwrap()
                    .name
                    .clone(),
            });
        }
    });
}

fn networking(
    ui: &mut Ui,
    state: &mut UiState,
    id: &str,
    inv: &xen_core::xapi::Inventory,
    details: &[HostDetails],
    read_only: bool,
    actions: &mut Vec<UiAction>,
) {
    load_pifs(state, id, actions);
    let pool = matches!(
        state.selection.as_ref().map(|s| &s.key),
        Some(ObjectKey::Pool(_))
    );
    let rows: Vec<_> = inv
        .networks
        .iter()
        .filter_map(|net| {
            let pifs: Vec<_> = details
                .iter()
                .flat_map(|d| records(&d.nics))
                .filter(|r| field(r, "Network", "UUID") == net.uuid)
                .collect();
            if !pool && pifs.is_empty() && net.interface_count > 0 {
                return None;
            }
            let join = |label| {
                pifs.iter()
                    .map(|r| value(r, label))
                    .collect::<Vec<_>>()
                    .join(", ")
            };
            Some((
                net.uuid.clone(),
                vec![
                    net.name.clone(),
                    net.description.clone(),
                    join("Interface"),
                    join("VLAN"),
                    yes(net.automatic),
                    join("Link"),
                    join("MAC"),
                    net.mtu.to_string(),
                ],
            ))
        })
        .collect();
    let selected = table_with_menu(
        ui,
        "host-networks",
        &[
            "Name",
            "Description",
            "NIC",
            "VLAN",
            "Auto",
            "Link Status",
            "MAC",
            "MTU",
        ],
        &rows,
        |ui, key| network_commands(ui, id, inv, read_only, key, actions),
    );
    network_commands(ui, id, inv, read_only, &selected, actions);
    ui.add_space(10.0);
    ui.label(RichText::new("IP Address Configuration").strong());
    let ips: Vec<_> = details
        .iter()
        .flat_map(|d| records(&d.nics))
        .filter(|r| {
            value(r, "Management") == "Yes" || !matches!(value(r, "IPv4").as_str(), "" | "—")
        })
        .map(|r| {
            (
                r.id.clone(),
                vec![
                    value(r, "Interface"),
                    field(r, "IPv4", "Configuration"),
                    value(r, "IPv4"),
                    field(r, "IPv4", "Netmask"),
                    field(r, "IPv4", "Gateway"),
                    field(r, "IPv4", "DNS"),
                ],
            )
        })
        .collect();
    table(
        ui,
        "host-ip",
        &[
            "Name",
            "Mode",
            "IP Address",
            "Subnet Mask",
            "Gateway",
            "DNS Servers",
        ],
        &ips,
    );
    if ui
        .add_enabled(!read_only, egui::Button::new("Configure…"))
        .clicked()
        && let Some(detail) = details.first()
    {
        actions.push(UiAction::OpenServerAdmin {
            connection_id: id.into(),
            host_uuid: detail.host_uuid.clone(),
            page: super::server_admin::Page::Ip,
        });
    }
}

fn delete_bond_action(
    state: &UiState,
    id: &str,
    pif_uuid: &str,
    read_only: bool,
    inv: &xen_core::xapi::Inventory,
) -> Option<UiAction> {
    if read_only {
        return None;
    }
    let pif = state.pif_catalog.iter().find(|p| p.uuid == pif_uuid)?;
    let network = inv
        .networks
        .iter()
        .find(|n| n.uuid == pif.bond_network_uuid && n.virtual_interface_count == 0)?;
    if state
        .pif_catalog
        .iter()
        .any(|p| p.network_uuid == network.uuid && p.management)
    {
        return None;
    }
    Some(UiAction::PromptDestroyNetwork {
        connection_id: id.into(),
        network_uuid: network.uuid.clone(),
        name: format!("Bond: {}", network.name),
    })
}
