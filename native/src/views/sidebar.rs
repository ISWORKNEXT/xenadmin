use egui::{
    Align, Align2, Color32, CornerRadius, FontId, Frame, Layout, Margin, Rect, RichText, Sense,
    Stroke, Ui, Vec2, pos2, vec2,
};
use xen_core::workspace::{ConnectionView, WorkspaceView};
use xen_core::xapi::{Host, Network, Storage, VirtualMachine};

use crate::format;
use crate::state::{
    ActionKind, ConnectState, ListKind, ObjectKey, UiAction, UiState, action_allowed,
    connected_connections, connection_id, connection_inventory, connection_name,
    connection_settings, connection_status, is_read_only, running_task,
};
use crate::theme::*;
use crate::widgets;

const TREE_WIDTH: f32 = 310.0;
const ROW_HEIGHT: f32 = 28.0;

#[derive(Clone, Copy)]
enum TreeIcon {
    Root,
    Pool,
    Host,
    Vm,
    Template,
    Storage,
    Network,
    Tasks,
}

pub fn show(
    ui: &mut Ui,
    state: &mut UiState,
    workspace: &WorkspaceView,
    actions: &mut Vec<UiAction>,
) {
    egui::Panel::left("sidebar")
        .exact_size(TREE_WIDTH)
        .resizable(false)
        .frame(
            Frame::NONE
                .fill(panel())
                .stroke(Stroke::new(1.0, border_soft()))
                .inner_margin(Margin::symmetric(10, 8)),
        )
        .show(ui, |ui| {
            ui.spacing_mut().item_spacing.y = 1.0;
            header(ui, state, workspace);
            ui.add_space(4.0);
            let response =
                widgets::search_field(ui, &mut state.tree_query, "Search infrastructure…");
            if state.focus_search {
                response.request_focus();
                state.focus_search = false;
            }
            ui.add_space(4.0);

            egui::ScrollArea::vertical()
                .id_salt("infrastructure-tree")
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    let running = workspace
                        .tasks
                        .iter()
                        .filter(|task| {
                            matches!(task.status, xen_core::operations::TaskStatus::Running)
                        })
                        .count();
                    root_row(ui, state, workspace, actions, running);
                    ui.add_space(2.0);
                    for connection in &workspace.connections {
                        pool_node(ui, state, workspace, connection, actions);
                    }
                });

            ui.with_layout(Layout::bottom_up(Align::LEFT), |ui| {
                ui.add_space(2.0);
                ui.label(
                    RichText::new("Ctrl+K search · F5 refresh · Ctrl+W close tab")
                        .monospace()
                        .size(9.5)
                        .color(dim_fg()),
                );
            });
        });
}

fn header(ui: &mut Ui, state: &mut UiState, workspace: &WorkspaceView) {
    ui.horizontal(|ui| {
        ui.label(RichText::new("INFRASTRUCTURE").size(10.0).color(dim_fg()));
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            if ui
                .add(
                    egui::Button::new(RichText::new("+").size(17.0).color(muted_fg())).frame(false),
                )
                .on_hover_text("Add connection")
                .clicked()
            {
                state.connect = Some(ConnectState::empty());
            }
            ui.label(
                RichText::new(format!(
                    "{}/{}",
                    connected_connections(workspace).len(),
                    workspace.connections.len()
                ))
                .monospace()
                .size(10.0)
                .color(dim_fg()),
            );
        });
    });
}

fn root_row(
    ui: &mut Ui,
    state: &mut UiState,
    workspace: &WorkspaceView,
    actions: &mut Vec<UiAction>,
    running: usize,
) {
    let selected = state
        .object_tabs
        .get(state.active_tab)
        .is_some_and(|tab| tab.key == ObjectKey::Home);
    let (main, _) = tree_row(
        ui,
        "root",
        0,
        "Infrastructure",
        TreeIcon::Root,
        selected,
        false,
        true,
        Some(format!(
            "{}/{}",
            connected_connections(workspace).len(),
            workspace.connections.len()
        )),
    );
    if main.clicked() {
        actions.push(UiAction::OpenObject {
            key: ObjectKey::Home,
            title: "Home".into(),
        });
    }
    main.context_menu(|ui| {
        if ui.button("Home").clicked() {
            actions.push(UiAction::OpenObject {
                key: ObjectKey::Home,
                title: "Home".into(),
            });
            ui.close();
        }
        if ui.button("Activity & tasks").clicked() {
            actions.push(UiAction::OpenObject {
                key: ObjectKey::Tasks,
                title: "Activity & tasks".into(),
            });
            ui.close();
        }
        ui.separator();
        if ui.button("Refresh all").clicked() {
            actions.push(UiAction::Refresh(None));
            ui.close();
        }
        if ui.button("Add connection").clicked() {
            state.connect = Some(ConnectState::empty());
            ui.close();
        }
    });

    let tasks_selected = state
        .object_tabs
        .get(state.active_tab)
        .is_some_and(|tab| tab.key == ObjectKey::Tasks);
    let (tasks, _) = tree_row(
        ui,
        "tasks",
        0,
        "Activity & tasks",
        TreeIcon::Tasks,
        tasks_selected,
        false,
        true,
        (running > 0).then(|| running.to_string()),
    );
    if tasks.clicked() {
        actions.push(UiAction::OpenObject {
            key: ObjectKey::Tasks,
            title: "Activity & tasks".into(),
        });
    }
}

fn pool_node(
    ui: &mut Ui,
    state: &mut UiState,
    workspace: &WorkspaceView,
    connection: &ConnectionView,
    actions: &mut Vec<UiAction>,
) {
    let id = connection_id(connection).to_owned();
    let name = connection_name(connection);
    let status = connection_status(connection);
    let read_only = is_read_only(connection);
    let node_id = format!("pool:{id}");
    let expanded = tree_open(state, &node_id);
    let selected = state
        .object_tabs
        .get(state.active_tab)
        .is_some_and(|tab| matches!(&tab.key, ObjectKey::Pool(cid) if cid == &id));
    let needle = state.tree_query.trim().to_lowercase();
    let pool_matches = needle.is_empty() || name.to_lowercase().contains(&needle);

    let inventory = connection_inventory(connection);
    let hosts: Vec<&Host> = inventory
        .map(|inventory| {
            inventory
                .hosts
                .iter()
                .filter(|host| {
                    pool_matches
                        || needle.is_empty()
                        || host.name.to_lowercase().contains(&needle)
                        || host.address.to_lowercase().contains(&needle)
                })
                .collect()
        })
        .unwrap_or_default();
    let vms: Vec<&VirtualMachine> = inventory
        .map(|inventory| {
            inventory
                .vms
                .iter()
                .filter(|vm| {
                    pool_matches
                        || needle.is_empty()
                        || vm.name.to_lowercase().contains(&needle)
                        || vm.uuid.to_lowercase().contains(&needle)
                        || vm.host_name.to_lowercase().contains(&needle)
                })
                .collect()
        })
        .unwrap_or_default();
    let templates: Vec<&VirtualMachine> = inventory
        .map(|inventory| {
            inventory
                .templates
                .iter()
                .filter(|vm| {
                    pool_matches || needle.is_empty() || vm.name.to_lowercase().contains(&needle)
                })
                .collect()
        })
        .unwrap_or_default();
    let storage: Vec<&Storage> = inventory
        .map(|inventory| {
            inventory
                .storage
                .iter()
                .filter(|sr| {
                    let hidden = sr.storage_type == "udev"
                        || sr.storage_type == "tmpfs"
                        || sr.name.starts_with("__gui__")
                        || sr.name == "XenServer Tools"
                        || sr.name == "XCP-ng Tools";
                    !hidden
                        && (pool_matches
                            || needle.is_empty()
                            || sr.name.to_lowercase().contains(&needle)
                            || sr.storage_type.to_lowercase().contains(&needle))
                })
                .collect()
        })
        .unwrap_or_default();
    let networks: Vec<&Network> = inventory
        .map(|inventory| {
            inventory
                .networks
                .iter()
                .filter(|network| {
                    network.bridge != "xenapi"
                        && (pool_matches
                            || needle.is_empty()
                            || network.name.to_lowercase().contains(&needle))
                })
                .collect()
        })
        .unwrap_or_default();

    let has_match = pool_matches
        || !needle.is_empty()
            && (!hosts.is_empty()
                || !vms.is_empty()
                || !templates.is_empty()
                || !storage.is_empty()
                || !networks.is_empty());
    if !needle.is_empty() && !has_match {
        return;
    }

    let (main, chevron) = tree_row(
        ui,
        &node_id,
        0,
        &name,
        TreeIcon::Pool,
        selected,
        true,
        expanded,
        Some(if status == "offline" {
            "offline".into()
        } else if read_only {
            "RO".into()
        } else {
            String::new()
        }),
    );
    if let Some(chevron) = chevron
        && chevron.clicked()
    {
        actions.push(UiAction::ToggleTreeNode(node_id.clone()));
    }
    if main.clicked() {
        actions.push(UiAction::OpenObject {
            key: ObjectKey::Pool(id.clone()),
            title: name.clone(),
        });
    }
    main.context_menu(|ui| {
        if status == "offline" {
            if ui.button("Connect to this host").clicked() {
                state.connect = Some(ConnectState::from_profile(
                    id.clone(),
                    connection_settings(connection).clone(),
                ));
                ui.close();
            }
        } else {
            if ui.button("Refresh inventory").clicked() {
                actions.push(UiAction::Refresh(Some(id.clone())));
                ui.close();
            }
            if ui.button("Reconnect / edit").clicked() {
                state.connect = Some(ConnectState::from_profile(
                    id.clone(),
                    connection_settings(connection).clone(),
                ));
                ui.close();
            }
            if ui.button("Disconnect").clicked() {
                actions.push(UiAction::Disconnect(id.clone()));
                ui.close();
            }
            ui.separator();
            ui.add_enabled(false, egui::Button::new("Disconnect before removing"));
        }
        if status == "offline" {
            ui.separator();
            if ui.button("Remove saved connection").clicked() {
                actions.push(UiAction::ConfirmRemove(id.clone()));
                ui.close();
            }
        }
    });

    if !expanded {
        return;
    }

    let groups = [
        (
            ListKind::Hosts,
            "Hosts",
            hosts.len(),
            TreeIcon::Host,
            hosts.is_empty(),
        ),
        (
            ListKind::Vms,
            "Virtual machines",
            vms.len(),
            TreeIcon::Vm,
            vms.is_empty(),
        ),
        (
            ListKind::Templates,
            "Templates",
            templates.len(),
            TreeIcon::Template,
            templates.is_empty(),
        ),
        (
            ListKind::Storage,
            "Storage",
            storage.len(),
            TreeIcon::Storage,
            storage.is_empty(),
        ),
        (
            ListKind::Networks,
            "Networks",
            networks.len(),
            TreeIcon::Network,
            networks.is_empty(),
        ),
    ];

    for (kind, title, count, icon, empty) in groups {
        if !needle.is_empty() && empty {
            continue;
        }
        let group_id = format!("group:{kind:?}:{id}");
        let group_open = tree_open(state, &group_id);
        let group_selected = state.object_tabs.get(state.active_tab).is_some_and(|tab| {
            matches!(
                &tab.key,
                ObjectKey::List {
                    connection_id: Some(cid),
                    kind: list_kind,
                    host_ref: None,
                } if cid == &id && *list_kind == kind
            )
        });
        let (group_main, group_chevron) = tree_row(
            ui,
            &group_id,
            1,
            title,
            icon,
            group_selected,
            true,
            group_open,
            Some(count.to_string()),
        );
        if let Some(chevron) = group_chevron
            && chevron.clicked()
        {
            actions.push(UiAction::ToggleTreeNode(group_id.clone()));
        }
        if group_main.clicked() {
            actions.push(UiAction::OpenObject {
                key: ObjectKey::List {
                    connection_id: Some(id.clone()),
                    kind,
                    host_ref: None,
                },
                title: title.into(),
            });
        }
        if !group_open {
            continue;
        }

        match kind {
            ListKind::Hosts => {
                for host in &hosts {
                    host_leaf(ui, state, workspace, &id, host, actions);
                }
            }
            ListKind::Vms => {
                for vm in &vms {
                    vm_leaf(ui, state, workspace, &id, vm, actions);
                }
            }
            ListKind::Templates => {
                for template in &templates {
                    template_leaf(ui, state, workspace, &id, template, actions);
                }
            }
            ListKind::Storage => {
                for sr in &storage {
                    storage_leaf(ui, state, &id, sr, actions);
                }
            }
            ListKind::Networks => {
                for network in &networks {
                    network_leaf(ui, state, &id, network, actions);
                }
            }
        }
    }
}

fn host_leaf(
    ui: &mut Ui,
    state: &mut UiState,
    workspace: &WorkspaceView,
    connection_id: &str,
    host: &Host,
    actions: &mut Vec<UiAction>,
) {
    let key = ObjectKey::Host {
        connection_id: connection_id.to_owned(),
        host_uuid: host.uuid.clone(),
    };
    let selected = active_key(state).is_some_and(|active| *active == key);
    let (main, _) = tree_row(
        ui,
        &format!("host:{connection_id}:{}", host.reference),
        2,
        &host.name,
        TreeIcon::Host,
        selected,
        false,
        true,
        (!host.enabled).then(|| "disabled".into()),
    );
    if main.clicked() {
        actions.push(UiAction::OpenObject {
            key,
            title: host.name.clone(),
        });
    }
    main.context_menu(|ui| {
        if ui.button("Open host details").clicked() {
            actions.push(UiAction::OpenObject {
                key: ObjectKey::Host {
                    connection_id: connection_id.to_owned(),
                    host_uuid: host.uuid.clone(),
                },
                title: host.name.clone(),
            });
            ui.close();
        }
        if ui.button("Refresh inventory").clicked() {
            actions.push(UiAction::Refresh(Some(connection_id.to_owned())));
            ui.close();
        }
        ui.separator();
        ui.label(
            RichText::new(format!("{} · {}", host.address, host.version))
                .size(10.0)
                .color(dim_fg()),
        );
        let _ = workspace;
    });
}

fn vm_leaf(
    ui: &mut Ui,
    state: &mut UiState,
    workspace: &WorkspaceView,
    connection_id: &str,
    vm: &VirtualMachine,
    actions: &mut Vec<UiAction>,
) {
    let key = ObjectKey::Vm {
        connection_id: connection_id.to_owned(),
        vm_uuid: vm.uuid.clone(),
    };
    let selected = active_key(state).is_some_and(|active| *active == key);
    let power = widgets::power_style(&vm.power_state).1;
    let task = running_task(workspace, connection_id, &vm.uuid);
    let (main, _) = tree_row_colored(
        ui,
        &format!("vm:{connection_id}:{}", vm.uuid),
        2,
        &vm.name,
        TreeIcon::Vm,
        selected,
        false,
        true,
        None,
        power,
        task,
    );
    if main.clicked() {
        actions.push(UiAction::OpenObject {
            key,
            title: vm.name.clone(),
        });
    }
    main.context_menu(|ui| {
        if ui.button("Open").clicked() {
            actions.push(UiAction::OpenObject {
                key: ObjectKey::Vm {
                    connection_id: connection_id.to_owned(),
                    vm_uuid: vm.uuid.clone(),
                },
                title: vm.name.clone(),
            });
            ui.close();
        }
        ui.separator();
        let read_only = workspace
            .connections
            .iter()
            .find(|connection| crate::state::connection_id(connection) == connection_id)
            .is_some_and(is_read_only);
        if read_only {
            ui.label(RichText::new("Read-only").size(10.0).color(amber()));
        }
        for kind in ActionKind::MENU {
            let allowed = action_allowed(workspace, connection_id, vm, kind.operation());
            let color = if kind.destructive() { red() } else { fg() };
            if ui
                .add_enabled(
                    allowed,
                    egui::Button::new(RichText::new(kind.label()).size(11.5).color(color))
                        .frame(false),
                )
                .clicked()
            {
                actions.push(UiAction::Propose(
                    kind,
                    vec![(connection_id.to_owned(), vm.uuid.clone())],
                ));
                ui.close();
            }
        }
    });
}

fn template_leaf(
    ui: &mut Ui,
    state: &mut UiState,
    _workspace: &WorkspaceView,
    connection_id: &str,
    template: &VirtualMachine,
    actions: &mut Vec<UiAction>,
) {
    let key = ObjectKey::Template {
        connection_id: connection_id.to_owned(),
        vm_uuid: template.uuid.clone(),
    };
    let selected = active_key(state).is_some_and(|active| *active == key);
    let (main, _) = tree_row(
        ui,
        &format!("template:{connection_id}:{}", template.uuid),
        2,
        &template.name,
        TreeIcon::Template,
        selected,
        false,
        true,
        None,
    );
    if main.clicked() {
        actions.push(UiAction::OpenObject {
            key,
            title: template.name.clone(),
        });
    }
}

fn storage_leaf(
    ui: &mut Ui,
    state: &mut UiState,
    connection_id: &str,
    sr: &Storage,
    actions: &mut Vec<UiAction>,
) {
    let key = ObjectKey::Storage {
        connection_id: connection_id.to_owned(),
        sr_uuid: sr.uuid.clone(),
    };
    let selected = active_key(state).is_some_and(|active| *active == key);
    let badge = if sr.total == 0 {
        None
    } else {
        Some(format!("{:.0}%", format::percent(sr.used, sr.total)))
    };
    let (main, _) = tree_row(
        ui,
        &format!("sr:{connection_id}:{}", sr.uuid),
        2,
        &sr.name,
        TreeIcon::Storage,
        selected,
        false,
        true,
        badge,
    );
    if main.clicked() {
        actions.push(UiAction::OpenObject {
            key,
            title: sr.name.clone(),
        });
    }
}

fn network_leaf(
    ui: &mut Ui,
    state: &mut UiState,
    connection_id: &str,
    network: &Network,
    actions: &mut Vec<UiAction>,
) {
    let key = ObjectKey::Network {
        connection_id: connection_id.to_owned(),
        network_uuid: network.uuid.clone(),
    };
    let selected = active_key(state).is_some_and(|active| *active == key);
    let (main, _) = tree_row(
        ui,
        &format!("net:{connection_id}:{}", network.uuid),
        2,
        &network.name,
        TreeIcon::Network,
        selected,
        false,
        true,
        None,
    );
    if main.clicked() {
        actions.push(UiAction::OpenObject {
            key,
            title: network.name.clone(),
        });
    }
}

fn active_key(state: &UiState) -> Option<&ObjectKey> {
    state.object_tabs.get(state.active_tab).map(|tab| &tab.key)
}

fn tree_open(state: &UiState, id: &str) -> bool {
    !state.tree_query.trim().is_empty() || state.tree_expanded.contains(id)
}

#[allow(clippy::too_many_arguments)]
fn tree_row(
    ui: &mut Ui,
    id: &str,
    depth: usize,
    label: &str,
    icon: TreeIcon,
    selected: bool,
    has_children: bool,
    expanded: bool,
    badge: Option<String>,
) -> (egui::Response, Option<egui::Response>) {
    tree_row_colored(
        ui,
        id,
        depth,
        label,
        icon,
        selected,
        has_children,
        expanded,
        badge,
        muted_fg(),
        false,
    )
}

#[allow(clippy::too_many_arguments)]
fn tree_row_colored(
    ui: &mut Ui,
    id: &str,
    depth: usize,
    label: &str,
    icon: TreeIcon,
    selected: bool,
    has_children: bool,
    expanded: bool,
    badge: Option<String>,
    icon_color: Color32,
    task_dot: bool,
) -> (egui::Response, Option<egui::Response>) {
    let width = ui.available_width().max(120.0);
    let (rect, _) = ui.allocate_exact_size(Vec2::new(width, ROW_HEIGHT), Sense::hover());
    let indent = depth as f32 * 16.0;
    let chevron_rect = Rect::from_min_size(
        pos2(rect.left() + indent, rect.top()),
        vec2(18.0, ROW_HEIGHT),
    );
    let main_rect = Rect::from_min_max(
        pos2(chevron_rect.right(), rect.top()),
        pos2(rect.right(), rect.bottom()),
    );
    let chevron = has_children.then(|| {
        ui.interact(
            chevron_rect,
            egui::Id::unique(("tree-chevron", id)),
            Sense::click(),
        )
    });
    let response = ui.interact(
        main_rect,
        egui::Id::unique(("tree-row", id)),
        Sense::click(),
    );
    let hovered = response.hovered() || chevron.as_ref().is_some_and(|c| c.hovered());
    let painter = ui.painter();
    if selected {
        painter.rect_filled(rect, CornerRadius::same(5), panel_3());
        painter.rect_filled(
            Rect::from_min_size(
                pos2(rect.left() + indent - 4.0, rect.top() + 4.0),
                vec2(2.0, ROW_HEIGHT - 8.0),
            ),
            CornerRadius::same(1),
            accent(),
        );
    } else if hovered {
        painter.rect_filled(rect, CornerRadius::same(5), panel_2());
    }

    if let Some(chevron) = &chevron {
        let center = chevron_rect.center();
        let color = if chevron.hovered() { fg() } else { dim_fg() };
        let stroke = Stroke::new(1.2, color);
        if expanded {
            painter.line_segment(
                [
                    pos2(center.x - 3.0, center.y - 1.5),
                    pos2(center.x, center.y + 1.5),
                ],
                stroke,
            );
            painter.line_segment(
                [
                    pos2(center.x, center.y + 1.5),
                    pos2(center.x + 3.0, center.y - 1.5),
                ],
                stroke,
            );
        } else {
            painter.line_segment(
                [
                    pos2(center.x - 1.5, center.y - 3.0),
                    pos2(center.x + 1.5, center.y),
                ],
                stroke,
            );
            painter.line_segment(
                [
                    pos2(center.x + 1.5, center.y),
                    pos2(center.x - 1.5, center.y + 3.0),
                ],
                stroke,
            );
        }
    }

    let icon_rect = Rect::from_min_size(
        pos2(rect.left() + indent + 19.0, rect.top() + 7.0),
        Vec2::splat(14.0),
    );
    paint_icon(
        painter,
        icon_rect,
        icon,
        if selected { accent() } else { icon_color },
    );
    let text_color = if selected { fg() } else { muted_fg() };
    let text_x = icon_rect.right() + 6.0;
    let badge_width = badge
        .as_ref()
        .map_or(0.0, |value| value.chars().count() as f32 * 6.0 + 18.0);
    let max_chars = (((rect.right() - text_x - badge_width) / 6.6).max(8.0)) as usize;
    let label = crate::format::truncate(label, max_chars);
    painter.text(
        pos2(text_x, rect.center().y),
        Align2::LEFT_CENTER,
        label,
        FontId::proportional(12.0),
        text_color,
    );
    if task_dot {
        painter.circle_filled(pos2(rect.right() - 8.0, rect.center().y), 2.5, amber());
    } else if let Some(badge) = badge
        && !badge.is_empty()
    {
        painter.text(
            pos2(rect.right() - 8.0, rect.center().y),
            Align2::RIGHT_CENTER,
            badge,
            FontId::monospace(9.5),
            if selected { muted_fg() } else { dim_fg() },
        );
    }
    (response, chevron)
}

fn paint_icon(painter: &egui::Painter, rect: Rect, icon: TreeIcon, color: Color32) {
    let stroke = Stroke::new(1.1, color);
    let center = rect.center();
    match icon {
        TreeIcon::Root => {
            for offset in [-4.0, 0.0, 4.0] {
                painter.line_segment(
                    [
                        pos2(rect.left() + 1.0, center.y + offset),
                        pos2(rect.right() - 1.0, center.y + offset),
                    ],
                    stroke,
                );
            }
        }
        TreeIcon::Pool => {
            painter.rect_stroke(
                rect.shrink(1.5),
                CornerRadius::same(3),
                stroke,
                egui::StrokeKind::Inside,
            );
            painter.circle_filled(center, 1.6, color);
        }
        TreeIcon::Host => {
            for offset in [-3.5, 0.0, 3.5] {
                painter.line_segment(
                    [
                        pos2(rect.left() + 2.0, center.y + offset),
                        pos2(rect.right() - 3.0, center.y + offset),
                    ],
                    stroke,
                );
                painter.circle_filled(pos2(rect.right() - 1.5, center.y + offset), 0.9, color);
            }
        }
        TreeIcon::Vm => {
            painter.rect_stroke(
                Rect::from_min_max(
                    pos2(rect.left() + 1.0, rect.top() + 2.0),
                    pos2(rect.right() - 1.0, rect.bottom() - 4.0),
                ),
                CornerRadius::same(2),
                stroke,
                egui::StrokeKind::Inside,
            );
            painter.line_segment(
                [
                    pos2(center.x - 2.5, rect.bottom() - 1.0),
                    pos2(center.x + 2.5, rect.bottom() - 1.0),
                ],
                stroke,
            );
        }
        TreeIcon::Template => {
            painter.rect_stroke(
                Rect::from_min_max(
                    pos2(rect.left() + 3.0, rect.top() + 1.0),
                    pos2(rect.right() - 1.0, rect.bottom() - 3.0),
                ),
                CornerRadius::same(2),
                stroke,
                egui::StrokeKind::Inside,
            );
            painter.rect_stroke(
                Rect::from_min_max(
                    pos2(rect.left() + 1.0, rect.top() + 3.0),
                    pos2(rect.right() - 3.0, rect.bottom() - 1.0),
                ),
                CornerRadius::same(2),
                stroke,
                egui::StrokeKind::Inside,
            );
        }
        TreeIcon::Storage => {
            painter.rect_stroke(
                Rect::from_min_max(
                    pos2(rect.left() + 1.0, rect.top() + 1.0),
                    pos2(rect.right() - 1.0, rect.bottom() - 1.0),
                ),
                CornerRadius::same(3),
                stroke,
                egui::StrokeKind::Inside,
            );
            for offset in [-2.0, 2.0] {
                painter.line_segment(
                    [
                        pos2(rect.left() + 2.0, center.y + offset),
                        pos2(rect.right() - 2.0, center.y + offset),
                    ],
                    stroke,
                );
            }
        }
        TreeIcon::Network => {
            for (dx, dy) in [(-4.0, 3.0), (4.0, 3.0), (0.0, -4.0)] {
                painter.circle_filled(pos2(center.x + dx, center.y + dy), 1.5, color);
            }
            painter.line_segment(
                [
                    pos2(center.x - 3.0, center.y + 2.0),
                    pos2(center.x, center.y - 3.0),
                ],
                stroke,
            );
            painter.line_segment(
                [
                    pos2(center.x, center.y - 3.0),
                    pos2(center.x + 3.0, center.y + 2.0),
                ],
                stroke,
            );
            painter.line_segment(
                [
                    pos2(center.x - 4.0, center.y + 3.0),
                    pos2(center.x + 4.0, center.y + 3.0),
                ],
                stroke,
            );
        }
        TreeIcon::Tasks => {
            painter.circle_stroke(center, 4.5, stroke);
            painter.line_segment([center, pos2(center.x, center.y - 2.2)], stroke);
            painter.line_segment([center, pos2(center.x + 2.0, center.y + 1.0)], stroke);
        }
    }
}
