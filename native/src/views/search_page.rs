//! Scoped overview shared by every object page and navigation grouping.
use egui::{Align, Layout, RichText, Sense, Ui};
use egui_extras::{Column, TableBuilder};
use xen_core::workspace::WorkspaceView;

use super::sidebar::{NavigationMode, Node};
use crate::{
    format,
    search::SearchQuery,
    state::{self, ListKind, ObjectKey, UiAction, UiState},
};

pub fn show(
    ui: &mut Ui,
    state: &mut UiState,
    workspace: &WorkspaceView,
    actions: &mut Vec<UiAction>,
) {
    let key = state
        .selection
        .as_ref()
        .map(|s| s.key.clone())
        .unwrap_or(ObjectKey::Home);
    let mut roots = regroup(scoped_roots(state, workspace, &key), state, workspace);
    let query = SearchQuery::parse(&state.query);
    fn filter(
        node: &mut Node,
        query: &SearchQuery,
        state: &UiState,
        workspace: &WorkspaceView,
    ) -> bool {
        node.children
            .retain_mut(|child| filter(child, query, state, workspace));
        if let Some(
            ObjectKey::Vm {
                connection_id,
                vm_uuid,
            }
            | ObjectKey::Template {
                connection_id,
                vm_uuid,
            }
            | ObjectKey::Snapshot {
                connection_id,
                vm_uuid,
            },
        ) = &node.key
            && let Some((connection, vm)) = state::find_vm(workspace, connection_id, vm_uuid)
        {
            return (query.matches(vm, &state::connection_name(connection))
                && state.power.matches(&vm.power_state))
                || !node.children.is_empty();
        }
        state.query.trim().is_empty()
            || node
                .name
                .to_lowercase()
                .contains(&state.query.to_lowercase())
            || !node.children.is_empty()
    }
    roots.retain_mut(|node| filter(node, &query, state, workspace));
    ui.horizontal(|ui| {
        if ui.button("New Search…").clicked() {
            state.query.clear();
            state.power = Default::default();
            state.focus_search = true;
        }
        for label in ["Export…", "Import…"] {
            ui.add_enabled(false, egui::Button::new(label))
                .on_disabled_hover_text("Not implemented yet");
        }
    });
    fn count(nodes: &[Node]) -> usize {
        nodes
            .iter()
            .map(|node| usize::from(node.key.is_some()) + count(&node.children))
            .sum()
    }
    super::inventory::toolbar(ui, state, actions, count(&roots), "objects");
    let mut rows = Vec::new();
    flatten(ui, &roots, 0, &mut rows);
    egui::ScrollArea::horizontal()
        .id_salt("overview-scroll")
        .show(ui, |ui| {
            TableBuilder::new(ui)
                .id_salt("search-results")
                .striped(true)
                .sense(Sense::click())
                .cell_layout(Layout::left_to_right(Align::Center))
                .column(Column::initial(300.0).at_least(180.0))
                .columns(Column::initial(100.0).at_least(75.0), 6)
                .header(24.0, |mut header| {
                    for name in [
                        "Name",
                        "CPU Usage",
                        "Used Memory",
                        "Disks",
                        "Network",
                        "Address",
                        "Uptime",
                    ] {
                        header.col(|ui| {
                            ui.label(RichText::new(name).size(11.0));
                        });
                    }
                })
                .body(|body| {
                    body.rows(38.0, rows.len(), |mut row| {
                        let (node, depth) = rows[row.index()];
                        let (description, values) = values(node, workspace);
                        row.col(|ui| {
                            ui.add_space(depth as f32 * 16.0);
                            if !node.children.is_empty()
                                && ui
                                    .small_button(if expanded(ui, node) { "▾" } else { "▸" })
                                    .clicked()
                            {
                                let id = egui::Id::unique(("search-expand", &node.id));
                                let value = !expanded(ui, node);
                                ui.ctx().data_mut(|data| data.insert_temp(id, value));
                            }
                            let (rect, _) =
                                ui.allocate_exact_size(egui::vec2(14.0, 14.0), Sense::hover());
                            super::sidebar::paint_icon(
                                ui.painter(),
                                rect,
                                node.icon,
                                crate::theme::muted_fg(),
                            );
                            ui.vertical(|ui| {
                                ui.spacing_mut().item_spacing.y = 0.0;
                                ui.add(
                                    egui::Label::new(RichText::new(&node.name).size(11.0))
                                        .truncate()
                                        .selectable(false),
                                );
                                if !description.is_empty() {
                                    ui.add(
                                        egui::Label::new(
                                            RichText::new(description)
                                                .size(11.0)
                                                .color(crate::theme::muted_fg()),
                                        )
                                        .truncate()
                                        .selectable(false),
                                    );
                                }
                            });
                        });
                        for value in values {
                            row.col(|ui| {
                                ui.add(
                                    egui::Label::new(RichText::new(value).size(11.0))
                                        .truncate()
                                        .selectable(false),
                                );
                            });
                        }
                        if row.response().double_clicked()
                            && let Some(key) = &node.key
                        {
                            actions.push(UiAction::OpenObject {
                                key: key.clone(),
                                title: node.name.clone(),
                            });
                        }
                    });
                });
        });
}

fn expanded(ui: &Ui, node: &Node) -> bool {
    // Stable across table cells and the flatten pass.
    ui.ctx()
        .data_mut(|data| data.get_temp::<bool>(egui::Id::unique(("search-expand", &node.id))))
        .unwrap_or(true)
}
fn flatten<'a>(ui: &Ui, nodes: &'a [Node], depth: usize, rows: &mut Vec<(&'a Node, usize)>) {
    for node in nodes {
        rows.push((node, depth));
        if expanded(ui, node) {
            flatten(ui, &node.children, depth + 1, rows);
        }
    }
}
fn find(node: &Node, key: &ObjectKey) -> Option<Node> {
    if node.key.as_ref() == Some(key) {
        return Some(node.clone());
    }
    node.children.iter().find_map(|child| find(child, key))
}
fn scoped_roots(state: &UiState, workspace: &WorkspaceView, key: &ObjectKey) -> Vec<Node> {
    let tree = super::sidebar::build_tree(workspace, state);
    if matches!(key, ObjectKey::Group(_)) {
        return find(&tree, key).map(|n| n.children).unwrap_or_default();
    }
    if let ObjectKey::List {
        connection_id,
        kind,
        host_ref,
    } = key
    {
        return super::sidebar::objects(workspace, state)
            .into_iter()
            .filter(|(id, _)| connection_id.as_ref().is_none_or(|selected| selected == id))
            .flat_map(|(_, nodes)| nodes)
            .filter(|n| {
                let kind_matches = matches!(
                    (kind, &n.key),
                    (ListKind::Vms, Some(ObjectKey::Vm { .. }))
                        | (ListKind::Templates, Some(ObjectKey::Template { .. }))
                        | (ListKind::Hosts, Some(ObjectKey::Host { .. }))
                        | (ListKind::Storage, Some(ObjectKey::Storage { .. }))
                        | (ListKind::Networks, Some(ObjectKey::Network { .. }))
                );
                let host_matches = host_ref.as_ref().is_none_or(|host| {
                    n.key
                        .as_ref()
                        .and_then(ObjectKey::connection_id)
                        .and_then(|id| state::find_connection(workspace, id))
                        .and_then(state::connection_inventory)
                        .and_then(|inv| {
                            n.key
                                .as_ref()
                                .and_then(|key| object_uuid(key))
                                .and_then(|uuid| inv.navigation.get(uuid))
                        })
                        .is_some_and(|m| &m.home == host)
                });
                kind_matches && host_matches
            })
            .collect();
    }
    // The default overview groups pools, hosts and VMs like the Infrastructure tree.
    let overview =
        super::sidebar::build_tree_mode(workspace, state, NavigationMode::Infrastructure);
    if *key == ObjectKey::Home {
        return overview.children;
    }
    if let Some(node) = find(&overview, key) {
        return vec![node];
    }
    let all = super::sidebar::objects(workspace, state);
    if let ObjectKey::Pool(id) = key {
        return all
            .into_iter()
            .filter(|(cid, _)| cid == id)
            .flat_map(|(_, nodes)| nodes)
            .collect();
    }
    all.into_iter()
        .flat_map(|(_, nodes)| nodes)
        .filter(|n| n.key.as_ref() == Some(key))
        .collect()
}
fn object_uuid(key: &ObjectKey) -> Option<&str> {
    match key {
        ObjectKey::Host { host_uuid, .. } => Some(host_uuid),
        ObjectKey::Vm { vm_uuid, .. }
        | ObjectKey::Template { vm_uuid, .. }
        | ObjectKey::Snapshot { vm_uuid, .. } => Some(vm_uuid),
        ObjectKey::Storage { sr_uuid, .. } => Some(sr_uuid),
        ObjectKey::Network { network_uuid, .. } => Some(network_uuid),
        ObjectKey::Vdi { vdi_uuid, .. } => Some(vdi_uuid),
        _ => None,
    }
}
fn values(node: &Node, workspace: &WorkspaceView) -> (String, [String; 6]) {
    let mut values = std::array::from_fn(|_| "—".into());
    let mut description = String::new();
    if let Some(key) = &node.key
        && let Some(id) = key.connection_id()
        && let Some(connection) = state::find_connection(workspace, id)
        && let Some(inv) = state::connection_inventory(connection)
    {
        match key {
            ObjectKey::Host { host_uuid, .. } => {
                if let Some(host) = inv.hosts.iter().find(|h| &h.uuid == host_uuid) {
                    values[1] = format::bytes(host.memory_total.saturating_sub(host.memory_free));
                    values[4] = host.address.clone();
                    description = format!("{} {}", host.brand, host.version);
                }
            }
            ObjectKey::Vm { vm_uuid, .. }
            | ObjectKey::Template { vm_uuid, .. }
            | ObjectKey::Snapshot { vm_uuid, .. } => {
                if let Some((_, vm)) = state::find_vm(workspace, id, vm_uuid) {
                    description = vm.description.clone();
                    values[2] = vm.disk_count.to_string();
                    values[3] = vm.interface_count.to_string();
                }
            }
            ObjectKey::Storage { sr_uuid, .. } => {
                if let Some(sr) = inv.storage.iter().find(|s| &s.uuid == sr_uuid) {
                    description = sr.description.clone();
                    values[2] = format::bytes(sr.used);
                }
            }
            ObjectKey::Network { network_uuid, .. } => {
                if let Some(net) = inv.networks.iter().find(|n| &n.uuid == network_uuid) {
                    description = net.description.clone();
                }
            }
            ObjectKey::Vdi { vdi_uuid, .. } => {
                if let Some(disk) = inv.virtual_disks.iter().find(|d| &d.uuid == vdi_uuid) {
                    description = disk.description.clone();
                    values[2] = format::bytes(disk.size);
                }
            }
            _ => {}
        }
    }
    (description, values)
}

fn regroup(roots: Vec<Node>, state: &UiState, workspace: &WorkspaceView) -> Vec<Node> {
    use crate::config::GroupBy;
    if state.group_by == GroupBy::None {
        return roots;
    }
    fn collect(mut node: Node, output: &mut Vec<Node>) {
        for child in std::mem::take(&mut node.children) {
            collect(child, output);
        }
        if node
            .key
            .as_ref()
            .is_some_and(|key| !matches!(key, ObjectKey::Group(_) | ObjectKey::Home))
        {
            output.push(node);
        }
    }
    let mut nodes = Vec::new();
    for root in roots {
        collect(root, &mut nodes);
    }
    let mut groups = std::collections::BTreeMap::<String, Vec<Node>>::new();
    for node in nodes {
        let connection = node
            .key
            .as_ref()
            .and_then(ObjectKey::connection_id)
            .and_then(|id| state::find_connection(workspace, id));
        let vm = node.key.as_ref().and_then(|key| match key {
            ObjectKey::Vm {
                connection_id,
                vm_uuid,
            }
            | ObjectKey::Template {
                connection_id,
                vm_uuid,
            }
            | ObjectKey::Snapshot {
                connection_id,
                vm_uuid,
            } => state::find_vm(workspace, connection_id, vm_uuid).map(|(_, vm)| vm),
            _ => None,
        });
        let labels = match state.group_by {
            GroupBy::Connection => vec![
                connection
                    .map(state::connection_name)
                    .unwrap_or_else(|| "Disconnected".into()),
            ],
            GroupBy::Host => vec![
                connection
                    .and_then(state::connection_inventory)
                    .and_then(|inv| {
                        let key = node.key.as_ref()?;
                        let home = &inv.navigation.get(object_uuid(key)?)?.home;
                        inv.hosts
                            .iter()
                            .find(|host| &host.reference == home)
                            .map(|host| host.name.clone())
                    })
                    .unwrap_or_else(|| "No home server".into()),
            ],
            GroupBy::PowerState => vec![
                vm.map(|vm| vm.power_state.clone())
                    .unwrap_or_else(|| "Other objects".into()),
            ],
            GroupBy::Tag => {
                let tags = vm.map(|vm| vm.tags.clone()).unwrap_or_default();
                if tags.is_empty() {
                    vec!["Untagged".into()]
                } else {
                    tags
                }
            }
            GroupBy::None => unreachable!(),
        };
        for label in labels {
            groups.entry(label).or_default().push(node.clone());
        }
    }
    groups
        .into_iter()
        .map(|(label, children)| {
            let mut node = Node::group(&label);
            node.id = format!("search-group:{label}");
            node.children = children;
            node
        })
        .collect()
}
