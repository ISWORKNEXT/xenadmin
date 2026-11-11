//! Upstream ContextMenuBuilder selection shapes; commands share the main menu resolver.
use super::menubar::{self, MenuItem};
use crate::state::{
    ObjectKey, ObjectSelection, UiAction, UiState, connection_inventory, find_connection, is_online,
};
use xen_core::workspace::WorkspaceView;

pub fn contains(state: &UiState, key: &ObjectKey) -> bool {
    if state.tree_selection.is_empty() {
        state.selection.as_ref().is_some_and(|s| &s.key == key)
    } else {
        state.tree_selection.iter().any(|s| &s.key == key)
    }
}

pub fn select(
    state: &mut UiState,
    item: ObjectSelection,
    siblings: &[ObjectSelection],
    modifiers: egui::Modifiers,
) {
    let mut selection = if state.tree_selection.is_empty() {
        state.selection.iter().cloned().collect()
    } else {
        state.tree_selection.clone()
    };
    if modifiers.shift {
        let anchor = state
            .tree_anchor
            .as_ref()
            .or_else(|| state.selection.as_ref().map(|s| &s.key));
        if let Some(a) = anchor.and_then(|key| siblings.iter().position(|s| &s.key == key))
            && let Some(b) = siblings.iter().position(|s| s.key == item.key)
        {
            selection = siblings[a.min(b)..=a.max(b)].to_vec();
        } else {
            selection = vec![item.clone()];
        }
    } else if modifiers.ctrl || modifiers.command {
        if let Some(index) = selection.iter().position(|s| s.key == item.key) {
            selection.remove(index);
        } else {
            selection.push(item.clone());
        }
        state.tree_anchor = Some(item.key.clone());
    } else {
        selection = vec![item.clone()];
        state.tree_anchor = Some(item.key.clone());
    }
    state.selection = selection.last().cloned();
    state.tree_selection = selection;
}

pub fn row(
    response: &egui::Response,
    state: &mut UiState,
    workspace: &WorkspaceView,
    item: ObjectSelection,
    siblings: &[ObjectSelection],
    actions: &mut Vec<UiAction>,
) {
    let keyboard = contains(state, &item.key)
        && state.selection.as_ref().is_some_and(|s| s.key == item.key)
        && !response.ctx.egui_wants_keyboard_input()
        && response.ctx.input_mut(|i| {
            i.consume_key(egui::Modifiers::SHIFT, egui::Key::F10)
                || i.consume_key(egui::Modifiers::NONE, egui::Key::Menu)
        });
    if response.clicked() {
        select(
            state,
            item.clone(),
            siblings,
            response.ctx.input(|i| i.modifiers),
        );
        actions.push(UiAction::SelectObjects(state.tree_selection.clone()));
    } else if response.secondary_clicked() && !contains(state, &item.key) {
        select(state, item, siblings, egui::Modifiers::NONE);
        actions.push(UiAction::SelectObjects(state.tree_selection.clone()));
    }
    let popup = egui::Popup::context_menu(response);
    let popup = if keyboard {
        popup
            .open_memory(egui::SetOpenCommand::Bool(true))
            .at_position(response.rect.left_bottom())
    } else {
        popup
    };
    popup.show(|ui| {
        let items = items(state, workspace);
        super::shell::render_items(ui, &items, state, workspace, actions);
    });
}

pub fn items(state: &UiState, workspace: &WorkspaceView) -> Vec<MenuItem> {
    let key = menubar::selection(state);
    let multi = state.tree_selection.len() > 1;
    let same = |test: fn(&ObjectKey) -> bool| state.tree_selection.iter().all(|s| test(&s.key));
    let shape = if multi {
        if same(|k| matches!(k, ObjectKey::Vm { .. })) {
            "MultipleVMsInPool"
        } else if same(|k| matches!(k, ObjectKey::Template { .. })) {
            "MultipleTemplates"
        } else if same(|k| matches!(k, ObjectKey::Snapshot { .. })) {
            "MultipleSnapshots"
        } else if same(|k| matches!(k, ObjectKey::Storage { .. })) {
            "MultipleSRs"
        } else if same(|k| matches!(k, ObjectKey::Host { .. })) {
            "MultipleAliveHosts"
        } else if same(|k| matches!(k, ObjectKey::Vm { .. } | ObjectKey::Template { .. })) {
            "MixedVMsAndTemplates"
        } else {
            "MultipleDifferentXenObjectTypes"
        }
    } else {
        match key {
            ObjectKey::Home => "Root",
            ObjectKey::Pool(id)
            | ObjectKey::Host {
                connection_id: id, ..
            } if find_connection(workspace, id).is_some_and(|c| !is_online(c)) => {
                "DisconnectedHosts"
            }
            ObjectKey::Pool(_) => "SinglePool",
            ObjectKey::Host { connection_id, .. } => {
                if find_connection(workspace, connection_id)
                    .and_then(connection_inventory)
                    .is_some_and(|i| i.named_pool || i.hosts.len() > 1)
                {
                    "SingleAliveHostInPool"
                } else {
                    "SingleAliveStandaloneHost"
                }
            }
            ObjectKey::Vm { .. } => "SingleVM",
            ObjectKey::Template { .. } => "SingleTemplate",
            ObjectKey::Snapshot { .. } => "SingleSnapshot",
            ObjectKey::Storage { .. } => "SingleSR",
            ObjectKey::Vdi { .. } => "SingleVDI",
            ObjectKey::Network { .. } => "SingleNetwork",
            ObjectKey::Group(_)
                if state.navigation_mode == super::sidebar::NavigationMode::Tags =>
            {
                "SingleTag"
            }
            ObjectKey::Group(_) => "SingleFolder",
            _ => "MultipleDifferentXenObjectTypes",
        }
    };
    let commands: &[&str] = match shape {
        "Root" => &[
            "AddHostCommand",
            "ConnectAllHostsCommand",
            "DisconnectAllHostsCommand",
        ],
        "SinglePool" => &[
            "NewVMCommand",
            "NewSRCommand",
            "ImportCommand",
            "",
            "HACommand",
            "VMGroupCommandVMSS",
            "VMGroupCommandVM_appliance",
            "DRCommand",
            "LaunchConversionManagerCommand",
            "",
            "AddHostToSelectedPoolToolStripMenuItem",
            "DisconnectPoolCommand",
            "PoolReconnectAsCommand",
            "",
            "",
            "PoolPropertiesCommand",
        ],
        "SingleAliveStandaloneHost" => &[
            "NewVMCommand",
            "NewSRCommand",
            "ImportCommand",
            "",
            "AddSelectedHostToPoolToolStripMenuItem",
            "",
            "CertificateCommand",
            "HostMaintenanceModeCommand",
            "",
            "RebootHostCommand",
            "ShutDownHostCommand",
            "PowerOnHostCommand",
            "RestartToolstackCommand",
            "",
            "DisconnectHostCommand",
            "HostReconnectAsCommand",
            "",
            "HostPropertiesCommand",
        ],
        "SingleAliveHostInPool" => &[
            "NewVMCommand",
            "NewSRCommand",
            "ImportCommand",
            "",
            "CertificateCommand",
            "HostMaintenanceModeCommand",
            "",
            "RebootHostCommand",
            "ShutDownHostCommand",
            "RestartToolstackCommand",
            "",
            "RemoveHostFromPoolCommand",
            "",
            "HostPropertiesCommand",
        ],
        "DisconnectedHosts" => &[
            "CancelHostConnectionCommand",
            "DisconnectHostsAndPoolsCommand",
            "ReconnectHostCommand",
            "ForgetSavedPasswordCommand",
            "RemoveHostCommand",
            "RestartToolstackCommand",
        ],
        "SingleVM" => &[
            "ShutDownVMCommand",
            "StartVMCommand",
            "ResumeVMCommand",
            "SuspendVMCommand",
            "RebootVMCommand",
            "",
            "ForceVMShutDownCommand",
            "ForceVMRebootCommand",
            "",
            "VappStartCommand",
            "VappShutDownCommand",
            "",
            "StartVMOnHostToolStripMenuItem",
            "ResumeVMOnHostToolStripMenuItem",
            "MigrateVMToolStripMenuItem",
            "",
            "CopyVMCommand",
            "MoveVMCommand",
            "ExportCommand",
            "TakeSnapshotCommand",
            "ConvertVMToTemplateCommand",
            "AssignGroupToolStripMenuItemVMSS",
            "AssignGroupToolStripMenuItemVM_appliance",
            "",
            "VtpmCommand",
            "",
            "DeleteVMCommand",
            "",
            "PropertiesCommand",
        ],
        "SingleTemplate" => &[
            "NewVMFromTemplateCommand",
            "InstantVMFromTemplateCommand",
            "",
            "ExportTemplateCommand",
            "CopyTemplateCommand",
            "",
            "DeleteTemplateCommand",
            "",
            "PropertiesCommand",
        ],
        "SingleSnapshot" => &[
            "NewVMFromSnapshotCommand",
            "NewTemplateFromSnapshotCommand",
            "ExportSnapshotAsTemplateCommand",
            "DeleteSnapshotCommand",
            "",
            "PropertiesCommand",
        ],
        "SingleSR" => &[
            "RepairSRCommand",
            "SetAsDefaultSRCommand",
            "",
            "DetachSRCommand",
            "ReattachSRCommand",
            "ForgetSRCommand",
            "DestroySRCommand",
            "",
            "PropertiesCommand",
        ],
        "SingleVDI" => &["DeleteVirtualDiskCommand", "", "PropertiesCommand"],
        "SingleNetwork" => &["PropertiesCommand"],
        "SingleFolder" => &[
            "NewFolderCommand",
            "PutFolderIntoRenameModeCommand",
            "DeleteFolderCommand",
        ],
        "SingleTag" => &["DeleteTagCommand", "PutTagIntoRenameModeCommand"],
        "MultipleVMsInPool" => &[
            "ShutDownVMCommand",
            "StartVMCommand",
            "ResumeVMCommand",
            "SuspendVMCommand",
            "RebootVMCommand",
            "",
            "ForceVMShutDownCommand",
            "ForceVMRebootCommand",
            "",
            "VappStartCommand",
            "VappShutDownCommand",
            "",
            "StartVMOnHostToolStripMenuItem",
            "ResumeVMOnHostToolStripMenuItem",
            "MoveVMCommand",
            "MigrateVMToolStripMenuItem",
            "ExportCommand",
            "AssignGroupToolStripMenuItemVMSS",
            "AssignGroupToolStripMenuItemVM_appliance",
            "",
            "DeleteVMCommand",
            "EditTagsCommand",
        ],
        "MultipleTemplates" => &["DeleteTemplateCommand", "EditTagsCommand"],
        "MultipleSnapshots" => &["EditTagsCommand", "DeleteSnapshotCommand"],
        "MultipleSRs" => &[
            "RepairSRCommand",
            "DetachSRCommand",
            "ForgetSRCommand",
            "DestroySRCommand",
            "EditTagsCommand",
        ],
        "MultipleAliveHosts" => &[
            "AddSelectedHostToPoolToolStripMenuItem",
            "DisconnectHostCommand",
            "RebootHostCommand",
            "ShutDownHostCommand",
            "RestartToolstackCommand",
            "",
            "EditTagsCommand",
            "",
            "",
        ],
        "MixedVMsAndTemplates" => &["DeleteVMsAndTemplatesCommand", "EditTagsCommand"],
        "MultipleDifferentXenObjectTypes" => &["EditTagsCommand"],
        _ => &[],
    };
    fn find(items: Vec<MenuItem>, command: &str) -> Option<MenuItem> {
        for item in items {
            if item.command == command {
                return Some(item);
            }
            if let Some(found) = find(item.children, command) {
                return Some(found);
            }
        }
        None
    }
    commands
        .iter()
        .map(|command| {
            let command = match *command {
                "PropertiesCommand" => match key {
                    ObjectKey::Vm { .. } | ObjectKey::Snapshot { .. } => "VMPropertiesCommand",
                    ObjectKey::Template { .. } => "TemplatePropertiesCommand",
                    ObjectKey::Storage { .. } => "SRPropertiesCommand",
                    _ => "PropertiesCommand",
                },
                "StartVMOnHostToolStripMenuItem" => "StartVMOnHostCommand",
                "ResumeVMOnHostToolStripMenuItem" => "ResumeVMOnHostCommand",
                "MigrateVMToolStripMenuItem" => "MigratetoServer",
                _ => command,
            };
            if command.is_empty() {
                return MenuItem::separator();
            }
            find(menubar::menus(state, workspace), command).unwrap_or_else(|| {
                MenuItem::item(
                    match command {
                        "DeleteVMsAndTemplatesCommand"
                        | "DeleteSnapshotCommand"
                        | "DeleteVirtualDiskCommand"
                        | "DeleteFolderCommand"
                        | "DeleteTagCommand" => "Delete".into(),
                        "PropertiesCommand" => "Properties".into(),
                        "EditTagsCommand" => "Edit Tags…".into(),
                        "PutTagIntoRenameModeCommand" | "PutFolderIntoRenameModeCommand" => {
                            "Rename".into()
                        }
                        "NewFolderCommand" => "New Folder…".into(),
                        "AddSelectedHostToPoolToolStripMenuItem" => "Add to Pool".into(),
                        "AddHostToSelectedPoolToolStripMenuItem" => "Add Server".into(),
                        "StartVMCommand" => "Start".into(),
                        "ShutDownVMCommand" => "Shut Down".into(),
                        "SuspendVMCommand" => "Suspend".into(),
                        "ResumeVMCommand" => "Resume".into(),
                        "NewVMFromSnapshotCommand" => "New VM from Snapshot…".into(),
                        "NewTemplateFromSnapshotCommand" => "New Template from Snapshot…".into(),
                        "ExportSnapshotAsTemplateCommand" => "Export Snapshot as Template…".into(),
                        "InstantVMFromTemplateCommand" => "Instant VM from Template…".into(),
                        "AssignGroupToolStripMenuItemVMSS" => "Assign to Snapshot Schedule".into(),
                        "AssignGroupToolStripMenuItemVM_appliance" => "Assign to vApp".into(),
                        "VtpmCommand" => "Virtual TPM…".into(),
                        "DisconnectHostsAndPoolsCommand" => "Disconnect".into(),
                        "CancelHostConnectionCommand" => "Cancel Connection".into(),
                        _ => command
                            .strip_suffix("Command")
                            .unwrap_or(command)
                            .chars()
                            .fold(String::new(), |mut s, c| {
                                if c.is_uppercase() && !s.is_empty() {
                                    s.push(' ');
                                }
                                s.push(c);
                                s
                            }),
                    },
                    command,
                )
            })
        })
        .collect()
}

/// Preserve the table button order while laying out popup commands vertically.
pub(super) fn command_strip(ui: &mut egui::Ui, body: impl FnOnce(&mut egui::Ui)) {
    if ui.stack().contained_in(egui::UiKind::Menu) || ui.stack().contained_in(egui::UiKind::Popup) {
        ui.vertical(body);
    } else {
        ui.horizontal_wrapped(body);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn vm(name: &str) -> ObjectSelection {
        ObjectSelection {
            key: ObjectKey::Vm {
                connection_id: "lab".into(),
                vm_uuid: name.into(),
            },
            title: name.into(),
        }
    }
    #[test]
    fn ctrl_toggles_and_shift_ranges_only_across_siblings() {
        let siblings = vec![vm("a"), vm("b"), vm("c")];
        let mut state = UiState::default();
        select(
            &mut state,
            siblings[0].clone(),
            &siblings,
            egui::Modifiers::NONE,
        );
        select(
            &mut state,
            siblings[2].clone(),
            &siblings,
            egui::Modifiers::SHIFT,
        );
        assert_eq!(
            state
                .tree_selection
                .iter()
                .map(|s| s.title.as_str())
                .collect::<Vec<_>>(),
            vec!["a", "b", "c"]
        );
        select(
            &mut state,
            siblings[1].clone(),
            &siblings,
            egui::Modifiers::CTRL,
        );
        assert_eq!(state.tree_selection.len(), 2);
        assert!(!contains(&state, &siblings[1].key));
        select(
            &mut state,
            vm("other-parent"),
            &[vm("other-parent")],
            egui::Modifiers::SHIFT,
        );
        assert_eq!(state.tree_selection.len(), 1);
        assert_eq!(state.tree_selection[0].title, "other-parent");
    }
    #[test]
    fn root_and_multiple_snapshot_menus_keep_upstream_order_and_disabled_items() {
        let workspace = WorkspaceView {
            connections: vec![],
            tasks: vec![],
        };
        let mut state = UiState::default();
        assert_eq!(
            items(&state, &workspace)
                .iter()
                .map(|i| i.command)
                .collect::<Vec<_>>(),
            vec![
                "AddHostCommand",
                "ConnectAllHostsCommand",
                "DisconnectAllHostsCommand"
            ]
        );
        state.tree_selection = vec!["one", "two"]
            .into_iter()
            .map(|name| ObjectSelection {
                key: ObjectKey::Snapshot {
                    connection_id: "lab".into(),
                    vm_uuid: name.into(),
                },
                title: name.into(),
            })
            .collect();
        state.selection = state.tree_selection.last().cloned();
        let menu = items(&state, &workspace);
        assert_eq!(
            menu.iter().map(|i| i.command).collect::<Vec<_>>(),
            vec!["EditTagsCommand", "DeleteSnapshotCommand"]
        );
        assert!(menubar::resolve("DeleteSnapshotCommand", &state, &workspace).is_err());
    }
}
