use crate::{
    state::{
        ListKind, ObjectKey, UiAction, UiState, connection_id, connection_inventory,
        connection_name, is_online,
    },
    theme::*,
    widgets,
};
use egui::{
    Align, Align2, Color32, CornerRadius, FontId, Frame, Layout, Margin, Rect, RichText, Sense,
    Stroke, Ui, Vec2, pos2, vec2,
};
use std::{cmp::Ordering, collections::BTreeMap};
use xen_core::{workspace::WorkspaceView, xapi::NavigationMetadata};

const TREE_WIDTH: f32 = 310.0;
const ROW_HEIGHT: f32 = 24.0;

#[derive(Clone, Copy)]
pub(super) enum TreeIcon {
    Root,
    Pool,
    Host,
    Vm,
    Template,
    Snapshot,
    Disk,
    Storage,
    Network,
    Running,
    Halted,
    Paused,
    Suspended,
    Maintenance,
    Disconnected,
    DefaultStorage,
    DetachedStorage,
    BrokenStorage,
}

#[derive(Clone, Copy, Default, PartialEq, Eq)]
pub enum NavigationMode {
    #[default]
    Infrastructure,
    Objects,
    Tags,
    Folders,
    CustomFields,
    Vapps,
    SavedSearches,
    Notifications,
}

#[derive(Clone)]
pub(super) struct Node {
    pub(super) id: String,
    pub(super) name: String,
    pub(super) key: Option<ObjectKey>,
    pub(super) icon: TreeIcon,
    rank: u8,
    metadata: NavigationMetadata,
    pub(super) children: Vec<Node>,
}
impl Node {
    fn new(id: String, name: String, key: Option<ObjectKey>, icon: TreeIcon, rank: u8) -> Self {
        Self {
            id,
            name,
            key,
            icon,
            rank,
            metadata: Default::default(),
            children: vec![],
        }
    }
    pub(super) fn group(name: &str) -> Self {
        Self::new(
            format!("group:{name}"),
            name.into(),
            None,
            TreeIcon::Root,
            4,
        )
    }
    fn sort(&mut self) {
        self.children.sort_by(|a, b| {
            a.rank
                .cmp(&b.rank)
                .then_with(|| natural_cmp(&a.name, &b.name))
                .then(a.id.cmp(&b.id))
        });
        for child in &mut self.children {
            child.sort();
        }
    }
    fn filter(&mut self, needle: &str) -> bool {
        // A name match keeps the whole matching subtree; descendants retain ancestors.
        if self.name.to_lowercase().contains(needle) {
            return true;
        }
        self.children.retain_mut(|child| child.filter(needle));
        !self.children.is_empty()
    }
}

fn natural_cmp(a: &str, b: &str) -> Ordering {
    let a = a.to_lowercase();
    let b = b.to_lowercase();
    let mut a = a.chars().peekable();
    let mut b = b.chars().peekable();
    loop {
        match (a.peek(), b.peek()) {
            (Some(x), Some(y)) if x.is_ascii_digit() && y.is_ascii_digit() => {
                let x: String = std::iter::from_fn(|| a.next_if(|c| c.is_ascii_digit())).collect();
                let y: String = std::iter::from_fn(|| b.next_if(|c| c.is_ascii_digit())).collect();
                let x = x.trim_start_matches('0');
                let y = y.trim_start_matches('0');
                let order = x.len().cmp(&y.len()).then_with(|| x.cmp(y));
                if order != Ordering::Equal {
                    return order;
                }
            }
            _ => match (a.next(), b.next()) {
                (None, None) => return Ordering::Equal,
                (None, _) => return Ordering::Less,
                (_, None) => return Ordering::Greater,
                (Some(x), Some(y)) if x != y => return x.cmp(&y),
                _ => {}
            },
        }
    }
}

pub(super) fn objects(workspace: &WorkspaceView, state: &UiState) -> Vec<(String, Vec<Node>)> {
    let mut result = vec![];
    for connection in &workspace.connections {
        let id = connection_id(connection).to_owned();
        let Some(inv) = connection_inventory(connection).filter(|_| is_online(connection)) else {
            result.push((
                id.clone(),
                vec![Node::new(
                    format!("offline:{id}"),
                    connection_name(connection),
                    Some(ObjectKey::Pool(id)),
                    TreeIcon::Disconnected,
                    1,
                )],
            ));
            continue;
        };
        let mut nodes = vec![];
        let mut add = |uuid: &str, name: &str, key: ObjectKey, icon: TreeIcon, rank: u8| {
            let metadata = inv.navigation.get(uuid).cloned().unwrap_or_default();
            if metadata.hidden && !state.tree_preferences.hidden_objects {
                return;
            }
            let mut node = Node::new(
                format!("object:{id}:{uuid}"),
                name.into(),
                Some(key),
                icon,
                rank,
            );
            node.metadata = metadata;
            nodes.push(node);
        };
        if inv.named_pool || inv.hosts.len() > 1 {
            add(
                &inv.pool_id,
                &inv.pool_name,
                ObjectKey::Pool(id.clone()),
                TreeIcon::Pool,
                0,
            );
        }
        for host in &inv.hosts {
            let live = inv.navigation.get(&host.uuid).is_some_and(|m| m.live);
            add(
                &host.uuid,
                &host.name,
                ObjectKey::Host {
                    connection_id: id.clone(),
                    host_uuid: host.uuid.clone(),
                },
                if !live {
                    TreeIcon::Disconnected
                } else if !host.enabled {
                    TreeIcon::Maintenance
                } else {
                    TreeIcon::Host
                },
                1,
            );
        }
        for vm in inv.vms.iter().chain(&inv.templates).chain(&inv.snapshots) {
            let (key, icon, rank) = match vm.kind.as_str() {
                "template" => (
                    ObjectKey::Template {
                        connection_id: id.clone(),
                        vm_uuid: vm.uuid.clone(),
                    },
                    TreeIcon::Template,
                    4,
                ),
                "snapshot" => (
                    ObjectKey::Snapshot {
                        connection_id: id.clone(),
                        vm_uuid: vm.uuid.clone(),
                    },
                    TreeIcon::Snapshot,
                    5,
                ),
                _ => (
                    ObjectKey::Vm {
                        connection_id: id.clone(),
                        vm_uuid: vm.uuid.clone(),
                    },
                    match vm.power_state.as_str() {
                        "Running" => TreeIcon::Running,
                        "Paused" => TreeIcon::Paused,
                        "Suspended" => TreeIcon::Suspended,
                        _ => TreeIcon::Halted,
                    },
                    2,
                ),
            };
            add(&vm.uuid, &vm.name, key, icon, rank);
        }
        for sr in &inv.storage {
            if inv.navigation.get(&sr.uuid).is_some_and(|m| m.tools_sr)
                || matches!(sr.name.as_str(), "XenServer Tools" | "XCP-ng Tools")
            {
                continue;
            }
            let icon = if sr.pbd_count == 0 {
                TreeIcon::DetachedStorage
            } else if sr.pbd_attached < sr.pbd_count {
                TreeIcon::BrokenStorage
            } else if sr.is_default {
                TreeIcon::DefaultStorage
            } else {
                TreeIcon::Storage
            };
            add(
                &sr.uuid,
                &sr.name,
                ObjectKey::Storage {
                    connection_id: id.clone(),
                    sr_uuid: sr.uuid.clone(),
                },
                icon,
                6,
            );
        }
        for disk in &inv.virtual_disks {
            add(
                &disk.uuid,
                &disk.name,
                ObjectKey::Vdi {
                    connection_id: id.clone(),
                    vdi_uuid: disk.uuid.clone(),
                },
                TreeIcon::Disk,
                8,
            );
        }
        for net in &inv.networks {
            add(
                &net.uuid,
                &net.name,
                ObjectKey::Network {
                    connection_id: id.clone(),
                    network_uuid: net.uuid.clone(),
                },
                TreeIcon::Network,
                7,
            );
        }
        result.push((id, nodes));
    }
    result
}

pub(super) fn build_tree(workspace: &WorkspaceView, state: &UiState) -> Node {
    build_tree_mode(workspace, state, state.navigation_mode)
}

pub(super) fn build_tree_mode(
    workspace: &WorkspaceView,
    state: &UiState,
    mode: NavigationMode,
) -> Node {
    let title = match mode {
        NavigationMode::Objects => "Objects",
        NavigationMode::Tags => "Tags",
        NavigationMode::Folders => "Folders",
        NavigationMode::CustomFields => "Custom Fields",
        NavigationMode::Vapps => "vApps",
        NavigationMode::SavedSearches => "Saved Searches",
        _ => "Xen Desktop",
    };
    let mut root = Node::new(
        format!("root:{title}"),
        title.into(),
        Some(ObjectKey::Home),
        TreeIcon::Root,
        0,
    );
    let connections = objects(workspace, state);
    if mode == NavigationMode::Infrastructure || mode == NavigationMode::Notifications {
        for (id, nodes) in connections {
            let inv = workspace
                .connections
                .iter()
                .find(|c| connection_id(c) == id)
                .and_then(connection_inventory);
            let Some(inv) = inv else {
                root.children.extend(nodes);
                continue;
            };
            let mut pool = nodes
                .iter()
                .find(|n| matches!(n.key, Some(ObjectKey::Pool(_))) && n.rank == 0)
                .cloned();
            let mut hosts: Vec<_> = nodes.iter().filter(|n| n.rank == 1).cloned().collect();
            let mut unhomed = vec![];
            for node in nodes.into_iter().filter(|n| n.rank > 1) {
                match &node.key {
                    Some(
                        ObjectKey::Snapshot { .. }
                        | ObjectKey::Network { .. }
                        | ObjectKey::Vdi { .. },
                    ) => continue,
                    Some(ObjectKey::Template { .. })
                        if if node.metadata.default_template {
                            !state.tree_preferences.default_templates
                        } else {
                            !state.tree_preferences.custom_templates
                        } =>
                    {
                        continue;
                    }
                    Some(ObjectKey::Storage { sr_uuid, .. })
                        if !state.tree_preferences.local_storage
                            && inv
                                .storage
                                .iter()
                                .any(|sr| sr.uuid == *sr_uuid && !sr.shared) =>
                    {
                        continue;
                    }
                    _ => {}
                }
                let host = inv.hosts.iter().find(|h| h.reference == node.metadata.home);
                if host.is_some_and(|h| !inv.navigation.get(&h.uuid).is_some_and(|m| m.live)) {
                    continue;
                }
                let host_uuid = host.map(|h| h.uuid.as_str()).or_else(|| {
                    (pool.is_none() && hosts.len() == 1)
                        .then(|| inv.hosts.first().map(|h| h.uuid.as_str()))
                        .flatten()
                });
                if let Some(parent) = hosts.iter_mut().find(|n| matches!(&n.key, Some(ObjectKey::Host { host_uuid: uuid, .. }) if Some(uuid.as_str()) == host_uuid)) { parent.children.push(node); }
                else { unhomed.push(node); }
            }
            if let Some(pool) = &mut pool {
                pool.children.extend(hosts);
                pool.children.extend(unhomed);
                root.children.push(pool.clone());
            } else {
                root.children.extend(hosts);
                root.children.extend(unhomed);
            }
        }
    } else if mode == NavigationMode::Objects {
        for (rank, name) in [
            (0, "Pools"),
            (1, "Servers"),
            (2, "VMs"),
            (4, "Templates"),
            (5, "Snapshots"),
            (6, "Storage"),
            (8, "Virtual Disks"),
            (7, "Networks"),
        ] {
            let mut group = Node::group(name);
            group.icon = match rank {
                0 => TreeIcon::Pool,
                1 => TreeIcon::Host,
                2 => TreeIcon::Vm,
                4 => TreeIcon::Template,
                5 => TreeIcon::Snapshot,
                6 => TreeIcon::Storage,
                8 => TreeIcon::Disk,
                _ => TreeIcon::Network,
            };
            group.rank = root.children.len() as u8;
            group.children = connections
                .iter()
                .flat_map(|(_, ns)| ns.iter())
                .filter(|n| n.rank == rank)
                .cloned()
                .collect();
            root.children.push(group);
        }
    } else if matches!(
        mode,
        NavigationMode::Tags | NavigationMode::Folders | NavigationMode::CustomFields
    ) {
        let mut groups: BTreeMap<Vec<String>, Vec<Node>> = BTreeMap::new();
        for (_, nodes) in connections {
            for node in nodes {
                let paths: Vec<Vec<String>> = match mode {
                    NavigationMode::Tags => node
                        .metadata
                        .tags
                        .iter()
                        .map(|tag| vec![tag.clone()])
                        .collect(),
                    NavigationMode::Folders => {
                        let path: Vec<_> = node
                            .metadata
                            .folder
                            .split('/')
                            .filter(|part| !part.is_empty())
                            .map(str::to_owned)
                            .collect();
                        if path.is_empty() { vec![] } else { vec![path] }
                    }
                    _ => node
                        .metadata
                        .custom_fields
                        .iter()
                        .map(|(key, value)| vec![key.clone(), value.clone()])
                        .collect(),
                };
                for path in paths {
                    groups.entry(path).or_default().push(node.clone());
                }
            }
        }
        for (path, nodes) in groups {
            let mut parent = &mut root;
            for name in path {
                let index = parent
                    .children
                    .iter()
                    .position(|n| n.key.is_none() && n.name == name)
                    .unwrap_or_else(|| {
                        parent.children.push(Node::group(&name));
                        parent.children.len() - 1
                    });
                parent = &mut parent.children[index];
            }
            parent.children.extend(nodes);
        }
    }
    fn assign_groups(node: &mut Node, parent: &str) {
        let path = format!("{parent}/{}", node.id);
        if node.key.is_none() || (parent.is_empty() && node.name != "Xen Desktop") {
            node.key = Some(ObjectKey::Group(path.clone()));
        }
        for child in &mut node.children {
            assign_groups(child, &path);
        }
    }
    assign_groups(&mut root, "");
    root.sort();
    let needle = state.tree_query.trim().to_lowercase();
    if !needle.is_empty() {
        root.children.retain_mut(|n| n.filter(&needle));
    }
    root
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
            let response = widgets::search_field(ui, &mut state.tree_query, "Search tree…");
            if state.focus_search {
                response.request_focus();
                state.focus_search = false;
            }
            ui.add_space(4.0);
            ui.spacing_mut().item_spacing.y = 0.0;
            let tree = build_tree(workspace, state);
            let height = (ui.available_height() - 192.0).max(80.0);
            egui::ScrollArea::vertical()
                .id_salt("navigation-tree")
                .max_height(height)
                .min_scrolled_height(height)
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    draw_node(ui, state, workspace, &tree, &[], 0, "", actions);
                    if tree.children.is_empty()
                        && !matches!(
                            state.navigation_mode,
                            NavigationMode::Infrastructure
                                | NavigationMode::Objects
                                | NavigationMode::Notifications
                        )
                    {
                        ui.label(
                            RichText::new("No objects available in this view.")
                                .size(11.0)
                                .color(dim_fg()),
                        );
                    }
                });
            ui.with_layout(Layout::top_down(Align::Min), |ui| {
                ui.spacing_mut().item_spacing.y = 4.0;
                ui.separator();
                for (mode, label) in [
                    (NavigationMode::Infrastructure, "▣  Infrastructure"),
                    (NavigationMode::Objects, "▦  Objects"),
                ] {
                    if ui
                        .add(nav_button(ui, label, state.navigation_mode == mode))
                        .clicked()
                    {
                        state.navigation_mode = mode;
                    }
                }
                let org = matches!(
                    state.navigation_mode,
                    NavigationMode::Tags
                        | NavigationMode::Folders
                        | NavigationMode::CustomFields
                        | NavigationMode::Vapps
                );
                egui::containers::menu::MenuButton::from_button(nav_button(
                    ui,
                    "▤  Organization Views ▾",
                    org,
                ))
                .ui(ui, |ui| {
                    for (mode, label) in [
                        (NavigationMode::Tags, "Objects by Tag"),
                        (NavigationMode::Folders, "Objects by Folder"),
                        (NavigationMode::CustomFields, "Objects by Custom Field"),
                        (NavigationMode::Vapps, "vApps"),
                    ] {
                        if ui
                            .selectable_label(state.navigation_mode == mode, label)
                            .clicked()
                        {
                            state.navigation_mode = mode;
                            ui.close();
                        }
                    }
                });
                egui::containers::menu::MenuButton::from_button(nav_button(
                    ui,
                    "⌕  Saved Searches ▾",
                    state.navigation_mode == NavigationMode::SavedSearches,
                ))
                .ui(ui, |ui| {
                    let mut searches: Vec<_> = state.saved_searches.iter().enumerate().collect();
                    searches.sort_by(|(_, a), (_, b)| natural_cmp(&a.name, &b.name));
                    if searches.is_empty() {
                        ui.label("No saved searches.");
                    }
                    for (index, saved) in searches {
                        if ui.button(&saved.name).clicked() {
                            state.navigation_mode = NavigationMode::SavedSearches;
                            actions.push(UiAction::OpenObject {
                                key: ObjectKey::List {
                                    connection_id: None,
                                    kind: ListKind::Vms,
                                    host_ref: None,
                                },
                                title: saved.name.clone(),
                            });
                            actions.push(UiAction::ApplySavedSearch(index));
                            ui.close();
                        }
                    }
                });
                let failed = workspace
                    .tasks
                    .iter()
                    .filter(|t| t.status == xen_core::operations::TaskStatus::Failed)
                    .count();
                let label = if failed > 0 {
                    format!("Notifications ({failed})")
                } else {
                    "Notifications".into()
                };
                if ui
                    .add(nav_button(
                        ui,
                        &format!("!  {label}"),
                        state.navigation_mode == NavigationMode::Notifications,
                    ))
                    .clicked()
                {
                    state.navigation_mode = NavigationMode::Notifications;
                    actions.push(UiAction::OpenObject {
                        key: ObjectKey::Tasks,
                        title: "Activity & tasks".into(),
                    });
                }
            });
        });
}

#[allow(clippy::too_many_arguments)]
fn draw_node(
    ui: &mut Ui,
    state: &mut UiState,
    workspace: &WorkspaceView,
    node: &Node,
    siblings: &[crate::state::ObjectSelection],
    depth: usize,
    parent_id: &str,
    actions: &mut Vec<UiAction>,
) {
    let path_id = format!("{parent_id}/{}", node.id);
    if state.tree_initialized.insert(path_id.clone()) && (depth == 0 || node.rank <= 1) {
        state.tree_expanded.insert(path_id.clone());
    }
    fn contains_selection(node: &Node, key: &ObjectKey) -> bool {
        node.key.as_ref() == Some(key) || node.children.iter().any(|n| contains_selection(n, key))
    }
    if state
        .selection
        .as_ref()
        .is_some_and(|s| node.children.iter().any(|n| contains_selection(n, &s.key)))
    {
        state.tree_expanded.insert(path_id.clone());
    }
    let expanded = !state.tree_query.trim().is_empty() || state.tree_expanded.contains(&path_id);
    let selected = node
        .key
        .as_ref()
        .is_some_and(|key| super::context_menu::contains(state, key));
    let (main, chevron) = tree_row_colored(
        ui,
        &path_id,
        depth,
        &node.name,
        node.icon,
        selected,
        !node.children.is_empty(),
        expanded,
        None,
        match node.icon {
            TreeIcon::Running => green(),
            TreeIcon::Disconnected | TreeIcon::DetachedStorage => dim_fg(),
            TreeIcon::BrokenStorage => red(),
            TreeIcon::Paused | TreeIcon::Suspended | TreeIcon::Maintenance => amber(),
            _ => muted_fg(),
        },
        false,
    );
    if chevron.is_some_and(|c| c.clicked()) {
        actions.push(UiAction::ToggleTreeNode(path_id.clone()));
    }
    if let Some(key) = &node.key {
        super::context_menu::row(
            &main,
            state,
            workspace,
            crate::state::ObjectSelection {
                key: key.clone(),
                title: node.name.clone(),
            },
            siblings,
            actions,
        );
    } else if main.clicked() {
        actions.push(UiAction::ToggleTreeNode(path_id.clone()));
    }
    if expanded {
        let siblings: Vec<_> = node
            .children
            .iter()
            .filter_map(|n| {
                n.key.clone().map(|key| crate::state::ObjectSelection {
                    key,
                    title: n.name.clone(),
                })
            })
            .collect();
        for child in &node.children {
            draw_node(
                ui,
                state,
                workspace,
                child,
                &siblings,
                depth + 1,
                &path_id,
                actions,
            );
        }
    }
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
    let text_color = if matches!(icon, TreeIcon::Disconnected) {
        dim_fg()
    } else if selected {
        fg()
    } else {
        muted_fg()
    };
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

pub(super) fn paint_icon(painter: &egui::Painter, rect: Rect, icon: TreeIcon, color: Color32) {
    let stroke = Stroke::new(1.1, color);
    let center = rect.center();
    match icon {
        TreeIcon::Root => {
            painter.line_segment(
                [pos2(rect.left(), center.y), pos2(center.x, rect.top())],
                stroke,
            );
            painter.line_segment(
                [pos2(center.x, rect.top()), pos2(rect.right(), center.y)],
                stroke,
            );
            painter.rect_stroke(
                Rect::from_min_max(
                    pos2(rect.left() + 2.0, center.y),
                    pos2(rect.right() - 2.0, rect.bottom()),
                ),
                CornerRadius::ZERO,
                stroke,
                egui::StrokeKind::Inside,
            );
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
        TreeIcon::Host | TreeIcon::Maintenance | TreeIcon::Disconnected => {
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
        TreeIcon::Vm
        | TreeIcon::Running
        | TreeIcon::Halted
        | TreeIcon::Paused
        | TreeIcon::Suspended => {
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
        TreeIcon::Snapshot => {
            painter.rect_stroke(
                rect.shrink(1.0),
                CornerRadius::same(2),
                stroke,
                egui::StrokeKind::Inside,
            );
            painter.circle_stroke(center, 3.0, stroke);
        }
        TreeIcon::Disk => {
            painter.circle_stroke(center, 6.0, stroke);
            painter.circle_stroke(center, 2.0, stroke);
        }
        TreeIcon::Storage
        | TreeIcon::DefaultStorage
        | TreeIcon::DetachedStorage
        | TreeIcon::BrokenStorage => {
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
    }
    let symbol = match icon {
        TreeIcon::Running => "▶",
        TreeIcon::Halted => "■",
        TreeIcon::Paused => "Ⅱ",
        TreeIcon::Suspended => "z",
        TreeIcon::Maintenance => "!",
        TreeIcon::Disconnected => "×",
        TreeIcon::DefaultStorage => "★",
        TreeIcon::DetachedStorage => "−",
        TreeIcon::BrokenStorage => "!",
        _ => "",
    };
    if !symbol.is_empty() {
        let position = pos2(rect.right(), rect.bottom());
        painter.circle_filled(position - vec2(2.0, 3.0), 5.0, panel());
        painter.text(
            position,
            Align2::RIGHT_BOTTOM,
            symbol,
            FontId::proportional(9.0),
            color,
        );
    }
}

fn nav_button(ui: &Ui, label: &str, selected: bool) -> egui::Button<'static> {
    egui::Button::new("")
        .left_text(label.to_owned())
        .min_size(vec2(ui.available_width(), 26.0))
        .selected(selected)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn natural_names_and_parent_preserving_search() {
        let mut root = Node::group("root");
        let mut host = Node::group("server");
        host.children.push(Node::group("VM 10"));
        host.children.push(Node::group("vm 2"));
        root.children.push(host);
        root.sort();
        assert_eq!(root.children[0].children[0].name, "vm 2");
        assert!(root.filter("vm 10"));
        assert_eq!(root.children[0].name, "server");
        assert_eq!(root.children[0].children.len(), 1);
        assert_eq!(natural_cmp("VM 0002", "vm 2"), Ordering::Equal);
    }
}
