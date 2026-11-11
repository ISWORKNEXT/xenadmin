use super::{
    infrastructure_page::{disabled, properties, section, table},
    object_page::{self, Selection, value_row},
    sidebar::TreeIcon,
};
use crate::{
    format,
    state::{self, ObjectKey, UiAction, UiState, VmTab},
};
use egui::Ui;
use xen_core::{operations::SrAction, properties::ObjectKind, workspace::WorkspaceView};

pub fn storage(
    ui: &mut Ui,
    state: &mut UiState,
    workspace: &WorkspaceView,
    actions: &mut Vec<UiAction>,
) {
    let Some(ObjectKey::Storage {
        connection_id: id,
        sr_uuid,
    }) = state.selection.as_ref().map(|s| s.key.clone())
    else {
        return;
    };
    let Some((connection, sr)) = state::find_storage(workspace, &id, &sr_uuid) else {
        ui.label("Storage repository unavailable.");
        return;
    };
    object_page::title_strip(
        ui,
        connection,
        &sr.name,
        TreeIcon::Storage,
        if sr.pbd_attached > 0 {
            "Running"
        } else {
            "Halted"
        },
    );
    object_page::tab_strip(
        ui,
        &object_page::tabs_for(Selection::Sr),
        &mut state.object_tab,
    );
    ui.add_space(6.0);
    match state.object_tab {
        VmTab::Search => super::search_page::show(ui, state, workspace, actions),
        VmTab::General => {
            section(ui, "General", |ui| {
                value_row(ui, "Name", &sr.name);
                value_row(ui, "Description", &sr.description);
                value_row(ui, "Type", &sr.storage_type);
                value_row(ui, "Shared", if sr.shared { "Yes" } else { "No" });
                value_row(ui, "Size", &format::bytes(sr.total));
                value_row(ui, "Usage", &format::bytes(sr.used));
                value_row(ui, "UUID", &sr.uuid);
            });
            section(ui, "Status", |ui| {
                value_row(
                    ui,
                    "Connection",
                    if sr.pbd_attached > 0 {
                        "Connected"
                    } else {
                        "Detached"
                    },
                );
                value_row(
                    ui,
                    "Attachments",
                    &format!("{} / {}", sr.pbd_attached, sr.pbd_count),
                );
            });
            section(ui, "Multipathing", |ui| {
                // No new reads: retain the host collector's explicit availability values.
                let mut found = false;
                for ((cid, _), details) in &state.page_hosts {
                    if cid != &id {
                        continue;
                    }
                    if let Some(details) = details.ready()
                        && let xen_core::host::DataSection::Ready { rows, .. } = &details.storage
                    {
                        for r in rows {
                            let is_sr = r.groups.iter().any(|g| {
                                g.title == "Storage repository"
                                    && g.fields
                                        .iter()
                                        .any(|f| f.label == "UUID" && f.value == sr_uuid)
                            });
                            if is_sr {
                                for g in &r.groups {
                                    if g.title == "Multipathing" || g.title == "Path health" {
                                        for f in &g.fields {
                                            value_row(ui, &f.label, &f.value);
                                            found = true;
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
                if !found {
                    ui.label("Multipathing information unavailable.");
                }
            });
        }
        VmTab::Storage => {
            if state.sr_vdis_sr.as_deref() != Some(&sr_uuid) {
                state.sr_vdis.clear();
                state.sr_vdis_error = None;
                state.sr_vdis_sr = Some(sr_uuid.clone());
                state.sr_vdis_loading = true;
                state.sr_vdis_seq += 1;
                actions.push(UiAction::LoadSrVdis {
                    seq: state.sr_vdis_seq,
                    connection_id: id.clone(),
                    sr_uuid: sr_uuid.clone(),
                });
            }
            if state.sr_vdis_loading {
                ui.spinner();
            }
            if let Some(error) = &state.sr_vdis_error {
                ui.colored_label(crate::theme::red(), error);
            }
            let rows = state
                .sr_vdis
                .iter()
                .map(|d| {
                    (
                        d.uuid.clone(),
                        vec![
                            d.name.clone(),
                            d.description.clone(),
                            format::bytes(d.virtual_size),
                            d.attached_to.clone(),
                        ],
                    )
                })
                .collect::<Vec<_>>();
            let selected = table(
                ui,
                "sr-disks",
                &["Name", "Description", "Size", "VM"],
                &rows,
            );
            let disk = state.sr_vdis.iter().find(|d| d.uuid == selected);
            ui.horizontal(|ui| {
                disabled(ui, "Add…");
                if ui
                    .add_enabled(
                        !state::is_read_only(connection),
                        egui::Button::new("Rescan"),
                    )
                    .clicked()
                {
                    actions.push(UiAction::PromptSrAction {
                        connection_id: id.clone(),
                        sr_uuid: sr_uuid.clone(),
                        sr_name: sr.name.clone(),
                        action: SrAction::Scan,
                    });
                }
                disabled(ui, "Move…");
                let deletable = !state::is_read_only(connection)
                    && sr.content_type != "iso"
                    && disk.is_some_and(|d| d.attached_to.is_empty() && !d.missing && !d.read_only);
                if ui
                    .add_enabled(deletable, egui::Button::new("Delete"))
                    .on_disabled_hover_text("Select an unattached, writable disk.")
                    .clicked()
                {
                    let d = disk.unwrap();
                    actions.push(UiAction::PromptDeleteVdi {
                        vdi_uuid: d.uuid.clone(),
                        name: d.name.clone(),
                        size: d.virtual_size,
                    });
                }
                if ui
                    .add_enabled(disk.is_some(), egui::Button::new("Properties"))
                    .clicked()
                {
                    properties(actions, &id, ObjectKind::Disk, &selected);
                }
            });
            if let Some(disk) = disk
                && ui.link(format!("Open {}", disk.name)).clicked()
            {
                actions.push(UiAction::OpenObject {
                    key: ObjectKey::Vdi {
                        connection_id: id,
                        vdi_uuid: disk.uuid.clone(),
                    },
                    title: disk.name.clone(),
                });
            }
        }
        _ => {}
    }
}

pub fn network(
    ui: &mut Ui,
    state: &mut UiState,
    workspace: &WorkspaceView,
    actions: &mut Vec<UiAction>,
) {
    let Some(ObjectKey::Network {
        connection_id,
        network_uuid,
    }) = state.selection.as_ref().map(|s| s.key.clone())
    else {
        return;
    };
    let Some((connection, network)) = state::find_network(workspace, &connection_id, &network_uuid)
    else {
        return;
    };
    object_page::title_strip(ui, connection, &network.name, TreeIcon::Network, "Running");
    object_page::tab_strip(
        ui,
        &object_page::tabs_for(Selection::Network),
        &mut state.object_tab,
    );
    super::search_page::show(ui, state, workspace, actions);
}
pub fn vdi(
    ui: &mut Ui,
    state: &mut UiState,
    workspace: &WorkspaceView,
    actions: &mut Vec<UiAction>,
) {
    let Some(ObjectKey::Vdi {
        connection_id: id,
        vdi_uuid,
    }) = state.selection.as_ref().map(|s| s.key.clone())
    else {
        return;
    };
    let Some(connection) = state::find_connection(workspace, &id) else {
        return;
    };
    let Some(disk) = state::connection_inventory(connection)
        .and_then(|i| i.virtual_disks.iter().find(|d| d.uuid == vdi_uuid))
    else {
        ui.label("Virtual disk unavailable.");
        return;
    };
    object_page::title_strip(ui, connection, &disk.name, TreeIcon::Disk, "Halted");
    object_page::tab_strip(
        ui,
        &object_page::tabs_for(Selection::Vdi),
        &mut state.object_tab,
    );
    if state.object_tab == VmTab::Search {
        super::search_page::show(ui, state, workspace, actions);
        return;
    }
    section(ui, "General", |ui| {
        value_row(ui, "Name", &disk.name);
        value_row(ui, "Description", &disk.description);
        value_row(ui, "Size", &format::bytes(disk.size));
        value_row(ui, "SR", &disk.sr_name);
        value_row(ui, "Read only", if disk.read_only { "Yes" } else { "No" });
        value_row(ui, "UUID", &disk.uuid);
    });
    if ui.button("Properties").clicked() {
        properties(actions, &id, ObjectKind::Disk, &disk.uuid);
    }
}
