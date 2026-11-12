//! Global folder and tag commands, with explicit connection and item selection.
use crate::state::{
    UiAction, UiState, connection_id, connection_inventory, connection_name, is_online,
    is_read_only,
};
use xen_core::{organization::OrganizationChange, workspace::WorkspaceView};
pub struct OrganizationDialog {
    command: &'static str,
    pub connection_id: String,
    before: String,
    after: String,
    pub submitting: bool,
    pub error: Option<String>,
}
impl OrganizationDialog {
    pub fn new(command: &'static str, workspace: &WorkspaceView) -> Self {
        Self {
            command,
            connection_id: workspace
                .connections
                .iter()
                .find(|c| is_online(c) && !is_read_only(c))
                .map(|c| connection_id(c).to_owned())
                .unwrap_or_default(),
            before: String::new(),
            after: String::new(),
            submitting: false,
            error: None,
        }
    }
    pub fn change(&self) -> OrganizationChange {
        if self.command.contains("Tag") {
            OrganizationChange::Tag {
                before: self.before.clone(),
                after: self.after.clone(),
            }
        } else {
            OrganizationChange::Folder {
                before: self.before.clone(),
                after: self.after.clone(),
            }
        }
    }
}
pub fn show(
    ctx: &egui::Context,
    state: &mut UiState,
    workspace: &WorkspaceView,
    actions: &mut Vec<UiAction>,
) {
    if let Some((_, action, busy, error)) = &mut state.extra {
        let mut close = false;
        let response = egui::Modal::new(egui::Id::unique("extra-confirm")).show(ctx, |ui| {
            ui.set_width(480.0);
            ui.heading("Confirm command");
            ui.label(match action {
                xen_core::organization::ExtraAction::Recovery(_) => "Start the VM in Recovery Mode using CD/DVD and network boot?",
                xen_core::organization::ExtraAction::DisableCbt(_) => "Disable Changed Block Tracking on this VM's disks? Existing backup chains may need a full backup.",
                xen_core::organization::ExtraAction::QuickCreate(_) => "Clone, provision and start a VM from this instant template?",
                xen_core::organization::ExtraAction::DestroyHost(_) => "Permanently remove this offline pool member? This operation is untested-lab.",
            });
            if let Some(error) = error { ui.colored_label(crate::theme::red(), error); }
            if ui.add_enabled(!*busy, egui::Button::new("Continue")).clicked() { actions.push(UiAction::SubmitExtra); }
            if ui.add_enabled(!*busy, egui::Button::new("Cancel")).clicked() { close = true; }
            if *busy { ui.spinner(); }
        });
        if (close || response.should_close()) && !*busy {
            state.extra = None;
        }
        return;
    }
    let Some(dialog) = &mut state.organization else {
        return;
    };
    let new = dialog.command == "NewFolderCommand";
    let tag = dialog.command.contains("Tag");
    let delete = dialog.command.starts_with("Delete");
    let title = if new {
        "New Folder"
    } else if tag && delete {
        "Delete Tag"
    } else if tag {
        "Rename Tag"
    } else if delete {
        "Delete Folder"
    } else {
        "Rename Folder"
    };
    let response = egui::Modal::new(egui::Id::unique("organization-dialog")).show(ctx, |ui| {
        ui.set_width(480.0);
        ui.heading(title);
        ui.add_enabled_ui(!dialog.submitting, |ui| {
            egui::ComboBox::from_id_salt("organization-connection")
                .selected_text(
                    crate::state::find_connection(workspace, &dialog.connection_id)
                        .map(connection_name)
                        .unwrap_or_default(),
                )
                .show_ui(ui, |ui| {
                    for c in workspace
                        .connections
                        .iter()
                        .filter(|c| is_online(c) && !is_read_only(c))
                    {
                        ui.selectable_value(
                            &mut dialog.connection_id,
                            connection_id(c).to_owned(),
                            connection_name(c),
                        );
                    }
                });
            if !new {
                let mut names = std::collections::BTreeSet::new();
                if let Some(inv) = crate::state::find_connection(workspace, &dialog.connection_id)
                    .and_then(connection_inventory)
                {
                    if !tag {
                        names.extend(inv.empty_folders.iter().cloned());
                    }
                    for node in inv.navigation.values() {
                        if tag {
                            names.extend(node.tags.iter().cloned());
                        } else if !node.folder.is_empty() {
                            names.insert(node.folder.clone());
                        }
                    }
                }
                egui::ComboBox::from_id_salt("organization-item")
                    .selected_text(if dialog.before.is_empty() {
                        "Select…"
                    } else {
                        &dialog.before
                    })
                    .show_ui(ui, |ui| {
                        for name in names {
                            ui.selectable_value(&mut dialog.before, name.clone(), name);
                        }
                    });
            }
            if !delete {
                ui.label(if tag { "New tag name" } else { "Folder path" });
                ui.text_edit_singleline(&mut dialog.after);
            }
            if !new {
                ui.label("This changes all affected objects in the selected connection.");
            }
            if let Some(error) = &dialog.error {
                ui.colored_label(crate::theme::red(), error);
            }
            if ui
                .add_enabled(
                    !dialog.connection_id.is_empty()
                        && (new || !dialog.before.is_empty())
                        && (delete || !dialog.after.trim().is_empty()),
                    egui::Button::new(if delete { "Delete" } else { "Save" }),
                )
                .clicked()
            {
                actions.push(UiAction::SubmitOrganization);
            }
        });
        if dialog.submitting {
            ui.spinner();
        }
        if ui
            .add_enabled(!dialog.submitting, egui::Button::new("Cancel"))
            .clicked()
        {
            return true;
        }
        false
    });
    if (response.should_close() || response.inner) && !dialog.submitting {
        state.organization = None;
    }
}
