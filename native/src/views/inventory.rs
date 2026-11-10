use egui::{Align, CornerRadius, Frame, Label, Layout, Margin, RichText, Sense, Stroke, Ui};
use egui_extras::{Column, TableBuilder};
use xen_core::workspace::WorkspaceView;

use crate::config::GroupBy;
use crate::format;
use crate::search::SearchQuery;
use crate::state::{
    ActionKind, PowerFilter, Scope, Target, UiAction, UiState, action_allowed, connection_id,
    connection_name, find_connection, is_read_only, scoped_hosts, scoped_targets, scoped_templates,
};
use crate::theme;
use crate::theme::*;
use crate::widgets;

const PAGE_SIZE: usize = 25;

pub fn vms(
    ui: &mut Ui,
    state: &mut UiState,
    workspace: &WorkspaceView,
    actions: &mut Vec<UiAction>,
    templates: bool,
) {
    let mut targets = if templates {
        scoped_templates(workspace, &state.scope)
    } else {
        scoped_targets(workspace, &state.scope)
    };
    let query = SearchQuery::parse(&state.query);
    targets.retain(|target| {
        let connection = connection_name(target.connection);
        query.matches(target.vm, &connection) && state.power.matches(&target.vm.power_state)
    });

    toolbar(ui, state, actions, targets.len(), templates);
    if !templates {
        bulk_bar(ui, state, workspace, actions, &targets);
    }
    if state.group_by != GroupBy::None {
        grouped_targets(ui, state, workspace, actions, &targets, templates);
        return;
    }

    let total = targets.len();
    let page_count = total.div_ceil(PAGE_SIZE).max(1);
    if state.page >= page_count {
        state.page = page_count - 1;
    }
    let page = state.page;
    let start = page * PAGE_SIZE;
    let page_targets: Vec<Target<'_>> = targets
        .iter()
        .skip(start)
        .take(PAGE_SIZE)
        .copied()
        .collect();

    let selected_on_page = !page_targets.is_empty()
        && page_targets
            .iter()
            .all(|target| state.selected.contains(&key(target)));

    Frame::NONE
        .fill(panel())
        .stroke(Stroke::new(1.0, border()))
        .corner_radius(CornerRadius::same(RADIUS))
        .inner_margin(Margin::symmetric(6, 6))
        .show(ui, |ui| {
            let mut builder = TableBuilder::new(ui)
                .id_salt(("vm-table", templates))
                .striped(true)
                .cell_layout(Layout::left_to_right(Align::Center))
                .vscroll(false)
                .sense(Sense::click());
            if !templates {
                builder = builder.column(Column::exact(30.0));
            }
            builder = builder
                .column(Column::remainder().at_least(190.0))
                .column(Column::initial(180.0).at_least(120.0))
                .column(Column::initial(104.0))
                .column(Column::exact(58.0))
                .column(Column::exact(86.0));
            if !templates {
                builder = builder.column(Column::exact(40.0));
            }

            builder
                .header(30.0, |mut header| {
                    if !templates {
                        header.col(|ui| {
                            select_all(ui, state, workspace, &page_targets, selected_on_page);
                        });
                    }
                    for title in [
                        "Name",
                        "Connection / host",
                        "Power state",
                        "vCPUs",
                        "Memory",
                    ] {
                        header.col(|ui| {
                            ui.label(RichText::new(title).size(10.5).color(dim_fg()));
                        });
                    }
                    if !templates {
                        header.col(|ui| {
                            ui.label(RichText::new("").size(9.5));
                        });
                    }
                })
                .body(|body| {
                    body.rows(38.0, page_targets.len(), |mut row| {
                        let target = page_targets[row.index()];
                        let id = key(&target);
                        if !templates {
                            row.set_selected(state.selected.contains(&id));
                            row.col(|ui| {
                                let mut checked = state.selected.contains(&id);
                                if ui.add(egui::Checkbox::without_text(&mut checked)).changed() {
                                    toggle(state, &id, checked);
                                }
                            });
                        }
                        row.col(|ui| {
                            if ui
                                .add(
                                    Label::new(
                                        RichText::new(&target.vm.name).size(12.5).color(fg()),
                                    )
                                    .truncate()
                                    .sense(Sense::click()),
                                )
                                .on_hover_text(format!("{}\n{}", target.vm.name, target.vm.uuid))
                                .clicked()
                            {
                                actions.push(UiAction::OpenVm(
                                    connection_id(target.connection).to_owned(),
                                    target.vm.uuid.clone(),
                                ));
                            }
                        });
                        row.col(|ui| {
                            ui.vertical(|ui| {
                                ui.spacing_mut().item_spacing.y = 0.0;
                                ui.horizontal(|ui| {
                                    ui.spacing_mut().item_spacing.x = 3.0;
                                    ui.label(
                                        RichText::new(connection_name(target.connection))
                                            .size(10.5)
                                            .color(muted_fg()),
                                    );
                                    if is_read_only(target.connection) {
                                        ui.label(RichText::new("RO").size(7.5).color(dim_fg()));
                                    }
                                });
                                ui.label(
                                    RichText::new(&target.vm.host_name)
                                        .size(10.0)
                                        .color(dim_fg()),
                                );
                            });
                        });
                        row.col(|ui| {
                            widgets::power_badge(ui, &target.vm.power_state);
                        });
                        row.col(|ui| {
                            ui.label(
                                RichText::new(target.vm.vcpus.to_string())
                                    .monospace()
                                    .size(10.5)
                                    .color(fg()),
                            );
                        });
                        row.col(|ui| {
                            ui.label(
                                RichText::new(format::bytes(target.vm.memory))
                                    .monospace()
                                    .size(10.5)
                                    .color(fg()),
                            );
                        });
                        if !templates {
                            row.col(|ui| {
                                vm_menu(ui, workspace, target, actions);
                            });
                        }
                    });
                });
        });

    if total == 0 {
        widgets::empty_state(
            ui,
            if templates {
                "No templates in this scope"
            } else {
                "No virtual machines in this scope"
            },
            "Connect a pool or change the selected scope and filters.",
        );
    } else {
        pagination(ui, state, total, page, page_count);
        if page_targets.is_empty() {
            widgets::empty_state(ui, "No matches", "No objects match the current filters.");
        }
    }
}

fn grouped_targets(
    ui: &mut Ui,
    state: &mut UiState,
    workspace: &WorkspaceView,
    actions: &mut Vec<UiAction>,
    targets: &[Target<'_>],
    templates: bool,
) {
    use std::collections::BTreeMap;
    let mut groups: BTreeMap<String, Vec<Target<'_>>> = BTreeMap::new();
    for target in targets {
        groups
            .entry(group_value(
                target.vm,
                &connection_name(target.connection),
                state.group_by,
            ))
            .or_default()
            .push(*target);
    }
    for (label, group) in groups {
        theme::card_frame().show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                ui.label(RichText::new(&label).size(13.5).strong());
                widgets::badge(ui, group.len().to_string(), accent());
            });
            ui.add_space(4.0);
            for target in group {
                ui.horizontal(|ui| {
                    if !templates {
                        let id = key(&target);
                        let mut checked = state.selected.contains(&id);
                        if ui.add(egui::Checkbox::without_text(&mut checked)).changed() {
                            toggle(state, &id, checked);
                        }
                    }
                    widgets::dot(
                        ui,
                        if target.vm.power_state == "Running" {
                            green()
                        } else {
                            dim_fg()
                        },
                        6.0,
                    );
                    if ui
                        .add(
                            Label::new(RichText::new(&target.vm.name).size(11.5).color(fg()))
                                .truncate()
                                .sense(Sense::click()),
                        )
                        .clicked()
                    {
                        actions.push(UiAction::OpenVm(
                            connection_id(target.connection).to_owned(),
                            target.vm.uuid.clone(),
                        ));
                    }
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if !templates {
                            vm_menu(ui, workspace, target, actions);
                        }
                        ui.label(
                            RichText::new(format::bytes(target.vm.memory))
                                .monospace()
                                .size(9.5)
                                .color(muted_fg()),
                        );
                        ui.label(
                            RichText::new(format!("{} vCPU", target.vm.vcpus))
                                .monospace()
                                .size(9.5)
                                .color(muted_fg()),
                        );
                        widgets::power_badge(ui, &target.vm.power_state);
                        ui.label(
                            RichText::new(format!(
                                "{} · {}",
                                connection_name(target.connection),
                                target.vm.host_name
                            ))
                            .size(9.0)
                            .color(dim_fg()),
                        );
                    });
                });
                ui.separator();
            }
        });
        ui.add_space(6.0);
    }
}

fn group_value(vm: &xen_core::xapi::VirtualMachine, connection: &str, group_by: GroupBy) -> String {
    match group_by {
        GroupBy::None => String::new(),
        GroupBy::Connection => connection.to_owned(),
        GroupBy::Host => {
            if vm.host_name.is_empty() || vm.host_name == "Unassigned" {
                "Unassigned".into()
            } else {
                vm.host_name.clone()
            }
        }
        GroupBy::PowerState => {
            if vm.power_state.is_empty() {
                "Unknown".into()
            } else {
                vm.power_state.clone()
            }
        }
        GroupBy::Tag => vm
            .tags
            .first()
            .cloned()
            .unwrap_or_else(|| "Untagged".into()),
    }
}

fn key(target: &Target<'_>) -> (String, String) {
    (
        connection_id(target.connection).to_owned(),
        target.vm.uuid.clone(),
    )
}

fn toggle(state: &mut UiState, id: &(String, String), checked: bool) {
    if checked {
        state.selected.insert(id.clone());
    } else {
        state.selected.remove(id);
    }
}

fn select_all(
    ui: &mut Ui,
    state: &mut UiState,
    _workspace: &WorkspaceView,
    page_targets: &[Target<'_>],
    selected: bool,
) {
    let mut value = selected;
    if ui
        .add(egui::Checkbox::without_text(&mut value))
        .on_hover_text("Select this page")
        .changed()
    {
        for target in page_targets {
            toggle(state, &key(target), value);
        }
    }
}

fn toolbar(
    ui: &mut Ui,
    state: &mut UiState,
    actions: &mut Vec<UiAction>,
    count: usize,
    templates: bool,
) {
    ui.horizontal(|ui| {
        ui.scope(|ui| {
            ui.set_width(300.0);
            let response = widgets::search_field(
                ui,
                &mut state.query,
                "Search name:/host:/tag:/memory:/power_state: …",
            );
            if state.focus_search {
                response.request_focus();
                state.focus_search = false;
            }
        });
        let mut group_by = state.group_by;
        egui::ComboBox::from_id_salt("group-by")
            .selected_text(RichText::new(group_by.label()).size(10.5))
            .width(130.0)
            .show_ui(ui, |ui| {
                for option in GroupBy::ALL {
                    ui.selectable_value(&mut group_by, option, option.label());
                }
            });
        if group_by != state.group_by {
            state.group_by = group_by;
            state.page = 0;
        }
        ui.menu_button(RichText::new("Saved searches").size(10.5), |ui| {
            if state.saved_searches.is_empty() {
                ui.label(
                    RichText::new("No saved searches yet.")
                        .size(10.0)
                        .color(dim_fg()),
                );
            }
            for (index, saved) in state.saved_searches.iter().enumerate() {
                ui.horizontal(|ui| {
                    if ui
                        .selectable_label(false, RichText::new(&saved.name).size(10.5))
                        .on_hover_text(format!(
                            "{}\nGroup: {}",
                            if saved.query.is_empty() {
                                "(no query)"
                            } else {
                                &saved.query
                            },
                            saved.group_by.label()
                        ))
                        .clicked()
                    {
                        actions.push(UiAction::ApplySavedSearch(index));
                        ui.close();
                    }
                    if ui
                        .add(egui::Button::new(RichText::new("×").size(10.0)).frame(false))
                        .on_hover_text("Delete saved search")
                        .clicked()
                    {
                        actions.push(UiAction::DeleteSavedSearch(index));
                    }
                });
            }
            ui.separator();
            if ui
                .add(egui::Button::new(
                    RichText::new("Save current search…").size(10.5),
                ))
                .clicked()
            {
                actions.push(UiAction::OpenSaveSearch);
                ui.close();
            }
        });
        let mut power = state.power;
        egui::ComboBox::from_id_salt("power-filter")
            .selected_text(RichText::new(power.label()).size(10.5))
            .width(145.0)
            .show_ui(ui, |ui| {
                for option in PowerFilter::ALL {
                    ui.selectable_value(&mut power, option, option.label());
                }
            });
        if power != state.power {
            state.power = power;
            state.page = 0;
        }
        ui.label(
            RichText::new(if templates {
                format::plural(count, "template", "templates")
            } else {
                format::plural(count, "machine", "machines")
            })
            .size(10.0)
            .color(dim_fg()),
        );
    });
    ui.add_space(6.0);
}

fn bulk_bar(
    ui: &mut Ui,
    state: &mut UiState,
    workspace: &WorkspaceView,
    actions: &mut Vec<UiAction>,
    targets: &[Target<'_>],
) {
    let selected: Vec<Target<'_>> = targets
        .iter()
        .filter(|target| state.selected.contains(&key(target)))
        .copied()
        .collect();
    if selected.is_empty() {
        return;
    }
    let connections: std::collections::BTreeSet<_> = selected
        .iter()
        .map(|target| connection_id(target.connection))
        .collect();
    let read_only = selected
        .iter()
        .any(|target| is_read_only(target.connection));

    Frame::NONE
        .fill(with_alpha(accent(), 16))
        .stroke(Stroke::new(1.0, with_alpha(accent(), 70)))
        .corner_radius(CornerRadius::same(8))
        .inner_margin(Margin::symmetric(12, 7))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new(format!(
                        "{} selected across {} connection(s)",
                        selected.len(),
                        connections.len()
                    ))
                    .size(10.5)
                    .color(fg()),
                );
                if read_only {
                    ui.label(
                        RichText::new("· includes read-only targets")
                            .size(10.0)
                            .color(amber()),
                    );
                }
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if ui
                        .add(egui::Button::new(RichText::new("Clear").size(10.5)).frame(false))
                        .clicked()
                    {
                        state.selected.clear();
                    }
                    for kind in ActionKind::BULK.iter().rev() {
                        let allowed = selected.iter().all(|target| {
                            action_allowed(
                                workspace,
                                connection_id(target.connection),
                                target.vm,
                                kind.operation(),
                            )
                        });
                        if ui
                            .add_enabled(
                                allowed,
                                egui::Button::new(RichText::new(kind.label()).size(10.5)),
                            )
                            .clicked()
                        {
                            actions
                                .push(UiAction::Propose(*kind, selected.iter().map(key).collect()));
                        }
                    }
                });
            });
        });
    ui.add_space(6.0);
}

fn vm_menu(
    ui: &mut Ui,
    workspace: &WorkspaceView,
    target: Target<'_>,
    actions: &mut Vec<UiAction>,
) {
    ui.menu_button(RichText::new("...").size(11.0).color(muted_fg()), |ui| {
        ui.label(
            RichText::new(connection_name(target.connection))
                .size(9.5)
                .color(dim_fg()),
        );
        if is_read_only(target.connection) {
            ui.label(
                RichText::new("RO · Read-only connection")
                    .size(10.0)
                    .color(amber()),
            );
        }
        ui.separator();
        let id = key(&target);
        for kind in ActionKind::MENU {
            let allowed = action_allowed(
                workspace,
                connection_id(target.connection),
                target.vm,
                kind.operation(),
            );
            let color = if kind.destructive() { red() } else { fg() };
            if ui
                .add_enabled(
                    allowed,
                    egui::Button::new(RichText::new(kind.label()).size(11.0).color(color))
                        .frame(false),
                )
                .clicked()
            {
                actions.push(UiAction::Propose(kind, vec![id.clone()]));
                ui.close();
            }
        }
        ui.separator();
        if ui
            .add(
                egui::Button::new(RichText::new("Open details").size(11.0).color(muted_fg()))
                    .frame(false),
            )
            .clicked()
        {
            actions.push(UiAction::OpenVm(
                connection_id(target.connection).to_owned(),
                target.vm.uuid.clone(),
            ));
            ui.close();
        }
    });
}

fn pagination(ui: &mut Ui, state: &mut UiState, total: usize, page: usize, page_count: usize) {
    ui.add_space(4.0);
    ui.horizontal(|ui| {
        let first = page * PAGE_SIZE + 1;
        let last = ((page + 1) * PAGE_SIZE).min(total);
        ui.label(
            RichText::new(format!("{first}–{last} of {total}"))
                .size(10.0)
                .color(dim_fg()),
        );
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            if ui
                .add_enabled(
                    page + 1 < page_count,
                    egui::Button::new(RichText::new("Next").size(10.0)),
                )
                .clicked()
            {
                state.page += 1;
            }
            ui.label(
                RichText::new(format!("Page {} of {page_count}", page + 1))
                    .size(10.0)
                    .color(dim_fg()),
            );
            if ui
                .add_enabled(
                    page > 0,
                    egui::Button::new(RichText::new("Previous").size(10.0)),
                )
                .clicked()
            {
                state.page = state.page.saturating_sub(1);
            }
        });
    });
}

pub fn hosts(
    ui: &mut Ui,
    state: &mut UiState,
    workspace: &WorkspaceView,
    actions: &mut Vec<UiAction>,
) {
    let hosts = scoped_hosts(workspace, &state.scope);
    if hosts.is_empty() {
        widgets::empty_state(
            ui,
            "No hosts in this scope",
            "Connect a pool or select another connection.",
        );
        return;
    }
    for chunk in hosts.chunks(2) {
        ui.columns(2, |columns| {
            for (index, (connection, host)) in chunk.iter().enumerate() {
                if let Some(column) = columns.get_mut(index) {
                    host_card(column, connection, host, actions);
                }
            }
        });
        ui.add_space(GAP);
    }
}

fn host_card(
    ui: &mut Ui,
    connection: &xen_core::workspace::ConnectionView,
    host: &xen_core::xapi::Host,
    actions: &mut Vec<UiAction>,
) {
    crate::theme::card_frame().show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.horizontal(|ui| {
            widgets::server_mark(ui, 34.0);
            ui.vertical(|ui| {
                ui.spacing_mut().item_spacing.y = 0.0;
                ui.label(RichText::new(&host.name).size(13.0).strong());
                ui.label(
                    RichText::new(&host.address)
                        .monospace()
                        .size(9.5)
                        .color(dim_fg()),
                );
            });
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                widgets::badge(
                    ui,
                    if host.enabled { "Enabled" } else { "Disabled" },
                    if host.enabled { green() } else { amber() },
                );
            });
        });
        ui.add_space(6.0);
        let platform = format!("{} {}", host.brand, host.version);
        let platform = platform.trim();
        let platform = if platform.is_empty() {
            connection_name(connection)
        } else {
            format!("{} · {platform}", connection_name(connection))
        };
        if ui
            .add(
                egui::Button::new(RichText::new(platform).size(10.0).color(muted_fg()))
                    .frame(false),
            )
            .clicked()
        {
            actions.push(UiAction::SetScope(Scope::Connection(
                connection_id(connection).to_owned(),
            )));
        }
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            spec(ui, "Logical processors", &host.cpu_count.to_string());
            spec(ui, "Physical memory", &format::bytes(host.memory_total));
        });
        ui.add_space(4.0);
        widgets::resource_bar(
            ui,
            host.memory_total.saturating_sub(host.memory_free),
            host.memory_total,
            accent(),
            6.0,
        );
        ui.horizontal(|ui| {
            ui.label(RichText::new("Memory in use").size(9.0).color(dim_fg()));
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                ui.label(
                    RichText::new(format!("{} free", format::bytes(host.memory_free)))
                        .size(9.0)
                        .color(dim_fg()),
                );
            });
        });
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            if ui
                .add(
                    egui::Button::new(RichText::new("View this host's VMs").size(10.5))
                        .frame(false),
                )
                .clicked()
            {
                actions.push(UiAction::SetScope(Scope::Host {
                    connection_id: connection_id(connection).to_owned(),
                    host_ref: host.reference.clone(),
                }));
                actions.push(UiAction::SetView(crate::state::View::Vms));
            }
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if ui
                    .add(egui::Button::new(RichText::new("Host details").size(10.5)))
                    .clicked()
                {
                    actions.push(UiAction::OpenHostDetails {
                        connection_id: connection_id(connection).to_owned(),
                        host_uuid: host.uuid.clone(),
                        host_name: host.name.clone(),
                    });
                }
            });
        });
    });
}

fn spec(ui: &mut Ui, label: &str, value: &str) {
    ui.vertical(|ui| {
        ui.spacing_mut().item_spacing.y = 0.0;
        ui.label(RichText::new(label).size(9.0).color(dim_fg()));
        ui.label(RichText::new(value).size(12.0).monospace().color(fg()));
    });
    ui.add_space(12.0);
}

pub fn storage(
    ui: &mut Ui,
    state: &mut UiState,
    workspace: &WorkspaceView,
    actions: &mut Vec<UiAction>,
) {
    let mut rows = Vec::new();
    for connection in crate::state::visible_connections(workspace, &state.scope) {
        if let Some(inventory) = crate::state::connection_inventory(connection) {
            for sr in &inventory.storage {
                let hidden = sr.storage_type == "udev"
                    || sr.storage_type == "tmpfs"
                    || sr.name.starts_with("__gui__")
                    || sr.name == "XenServer Tools"
                    || sr.name == "XCP-ng Tools";
                if !hidden {
                    rows.push((connection, sr));
                }
            }
        }
    }
    if rows.is_empty() {
        widgets::empty_state(
            ui,
            "No storage repositories",
            "Connect a pool to list its SRs.",
        );
        return;
    }
    Frame::NONE
        .fill(panel())
        .stroke(Stroke::new(1.0, border()))
        .corner_radius(CornerRadius::same(RADIUS))
        .inner_margin(Margin::symmetric(6, 6))
        .show(ui, |ui| {
            TableBuilder::new(ui)
                .id_salt("storage-table")
                .striped(true)
                .cell_layout(Layout::left_to_right(Align::Center))
                .column(Column::remainder().at_least(180.0))
                .column(Column::initial(150.0))
                .column(Column::exact(90.0))
                .column(Column::exact(100.0))
                .column(Column::initial(170.0))
                .header(30.0, |mut header| {
                    for title in ["Name", "Connection", "Type", "Capacity", "Usage"] {
                        header.col(|ui| {
                            ui.label(RichText::new(title).size(10.5).color(dim_fg()));
                        });
                    }
                })
                .body(|body| {
                    body.rows(36.0, rows.len(), |mut row| {
                        let (connection, sr) = rows[row.index()];
                        row.col(|ui| {
                            if ui
                                .add(
                                    Label::new(RichText::new(&sr.name).size(12.0))
                                        .truncate()
                                        .sense(Sense::click()),
                                )
                                .clicked()
                            {
                                actions.push(UiAction::OpenResource {
                                    title: sr.name.clone(),
                                    rows: vec![
                                        ("Connection".into(), connection_name(connection)),
                                        ("Type".into(), sr.storage_type.clone()),
                                        (
                                            "Scope".into(),
                                            if sr.shared { "Shared" } else { "Local" }.into(),
                                        ),
                                        (
                                            "Capacity".into(),
                                            if sr.total == 0 {
                                                "Not reported".into()
                                            } else {
                                                format::bytes(sr.total)
                                            },
                                        ),
                                        ("Used".into(), format::bytes(sr.used)),
                                        (
                                            "Description".into(),
                                            if sr.description.is_empty() {
                                                "No description".into()
                                            } else {
                                                sr.description.clone()
                                            },
                                        ),
                                        ("UUID".into(), sr.uuid.clone()),
                                    ],
                                });
                            }
                        });
                        row.col(|ui| {
                            ui.label(
                                RichText::new(connection_name(connection))
                                    .size(10.5)
                                    .color(muted_fg()),
                            );
                        });
                        row.col(|ui| {
                            widgets::badge(ui, &sr.storage_type, cyan());
                        });
                        row.col(|ui| {
                            ui.label(
                                RichText::new(if sr.total == 0 {
                                    "Not reported".into()
                                } else {
                                    format::bytes(sr.total)
                                })
                                .monospace()
                                .size(10.5),
                            );
                        });
                        row.col(|ui| {
                            if sr.total == 0 {
                                ui.label(RichText::new("—").color(dim_fg()));
                            } else {
                                ui.horizontal(|ui| {
                                    ui.scope(|ui| {
                                        ui.set_width(92.0);
                                        widgets::resource_bar(ui, sr.used, sr.total, green(), 5.0);
                                    });
                                    ui.label(
                                        RichText::new(format!(
                                            "{:.0}%",
                                            format::percent(sr.used, sr.total)
                                        ))
                                        .size(9.5)
                                        .color(muted_fg()),
                                    );
                                });
                            }
                        });
                    });
                });
        });
}

pub fn networks(
    ui: &mut Ui,
    state: &mut UiState,
    workspace: &WorkspaceView,
    actions: &mut Vec<UiAction>,
) {
    let mut rows = Vec::new();
    for connection in crate::state::visible_connections(workspace, &state.scope) {
        if let Some(inventory) = crate::state::connection_inventory(connection) {
            for network in &inventory.networks {
                if network.bridge != "xenapi" {
                    rows.push((connection, network));
                }
            }
        }
    }
    ui.horizontal(|ui| {
        ui.label(
            RichText::new(format::plural(rows.len(), "network", "networks"))
                .size(10.0)
                .color(dim_fg()),
        );
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            if let Some(id) = state.scope.connection_id()
                && find_connection(workspace, id)
                    .is_some_and(|connection| !is_read_only(connection))
                && ui.add(widgets::primary_button("Add network")).clicked()
            {
                actions.push(UiAction::OpenNetworkCreate);
            }
        });
    });
    ui.add_space(6.0);
    if rows.is_empty() {
        widgets::empty_state(ui, "No networks", "Connect a pool to list its networks.");
        return;
    }
    Frame::NONE
        .fill(panel())
        .stroke(Stroke::new(1.0, border()))
        .corner_radius(CornerRadius::same(RADIUS))
        .inner_margin(Margin::symmetric(6, 6))
        .show(ui, |ui| {
            TableBuilder::new(ui)
                .id_salt("network-table")
                .striped(true)
                .cell_layout(Layout::left_to_right(Align::Center))
                .column(Column::remainder().at_least(180.0))
                .column(Column::initial(160.0))
                .column(Column::initial(120.0))
                .column(Column::exact(70.0))
                .column(Column::initial(150.0))
                .header(30.0, |mut header| {
                    for title in [
                        "Name",
                        "Connection",
                        "Bridge",
                        "MTU",
                        "Physical / virtual interfaces",
                    ] {
                        header.col(|ui| {
                            ui.label(RichText::new(title).size(10.5).color(dim_fg()));
                        });
                    }
                })
                .body(|body| {
                    body.rows(36.0, rows.len(), |mut row| {
                        let (connection, network) = rows[row.index()];
                        row.col(|ui| {
                            if ui
                                .add(
                                    Label::new(RichText::new(&network.name).size(12.0))
                                        .truncate()
                                        .sense(Sense::click()),
                                )
                                .clicked()
                            {
                                actions.push(UiAction::OpenResource {
                                    title: network.name.clone(),
                                    rows: vec![
                                        ("Connection".into(), connection_name(connection)),
                                        ("Bridge".into(), network.bridge.clone()),
                                        ("MTU".into(), network.mtu.to_string()),
                                        (
                                            "Description".into(),
                                            if network.description.is_empty() {
                                                "No description".into()
                                            } else {
                                                network.description.clone()
                                            },
                                        ),
                                        ("UUID".into(), network.uuid.clone()),
                                    ],
                                });
                            }
                        });
                        row.col(|ui| {
                            ui.label(
                                RichText::new(connection_name(connection))
                                    .size(10.5)
                                    .color(muted_fg()),
                            );
                        });
                        row.col(|ui| {
                            ui.label(
                                RichText::new(&network.bridge)
                                    .monospace()
                                    .size(10.0)
                                    .color(muted_fg()),
                            );
                        });
                        row.col(|ui| {
                            ui.label(
                                RichText::new(network.mtu.to_string())
                                    .monospace()
                                    .size(10.5),
                            );
                        });
                        row.col(|ui| {
                            ui.label(
                                RichText::new(format!(
                                    "{} / {}",
                                    network.interface_count, network.virtual_interface_count
                                ))
                                .monospace()
                                .size(10.5)
                                .color(muted_fg()),
                            );
                        });
                    });
                });
        });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_vm(host: &str, power: &str, tags: &[&str]) -> xen_core::xapi::VirtualMachine {
        xen_core::xapi::VirtualMachine {
            reference: "ref".into(),
            kind: "vm".into(),
            allowed_operations: Vec::new(),
            host_name: host.into(),
            resident_on: String::new(),
            snapshot_of: String::new(),
            snapshot_time: String::new(),
            disk_count: 0,
            interface_count: 0,
            uuid: "uuid".into(),
            name: "name".into(),
            description: String::new(),
            power_state: power.into(),
            vcpus: 2,
            memory: 1024,
            os: String::new(),
            tags: tags.iter().map(|tag| (*tag).to_owned()).collect(),
        }
    }

    #[test]
    fn groups_vms_by_connection_host_power_and_first_tag() {
        let vm = sample_vm("host-a", "Running", &["prod", "web"]);
        assert_eq!(group_value(&vm, "conn", GroupBy::Connection), "conn");
        assert_eq!(group_value(&vm, "conn", GroupBy::Host), "host-a");
        assert_eq!(group_value(&vm, "conn", GroupBy::PowerState), "Running");
        assert_eq!(group_value(&vm, "conn", GroupBy::Tag), "prod");
        assert_eq!(
            group_value(&sample_vm("Unassigned", "", &[]), "conn", GroupBy::Host),
            "Unassigned"
        );
        assert_eq!(
            group_value(
                &sample_vm("Unassigned", "", &[]),
                "conn",
                GroupBy::PowerState
            ),
            "Unknown"
        );
        assert_eq!(
            group_value(&sample_vm("Unassigned", "", &[]), "conn", GroupBy::Tag),
            "Untagged"
        );
        assert_eq!(group_value(&vm, "conn", GroupBy::None), "");
    }
}
