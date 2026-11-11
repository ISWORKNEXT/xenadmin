use crate::{
    state::{
        LoadState, UiAction, UiState, connection_name, find_connection, is_online, is_read_only,
    },
    theme, widgets,
};
use egui::RichText;

pub fn show(
    ctx: &egui::Context,
    state: &mut UiState,
    workspace: &xen_core::workspace::WorkspaceView,
    actions: &mut Vec<UiAction>,
) {
    let sequence = state.properties_seq;
    let disks = state
        .vm_state
        .ready()
        .map(|vm| vm.disks.clone())
        .unwrap_or_default();
    let Some(dialog) = state.properties.as_mut() else {
        return;
    };
    let connection = find_connection(workspace, &dialog.connection_id);
    let read_only = connection.is_none_or(is_read_only);
    let available = connection.is_some_and(is_online);
    let writable = available && !read_only;
    let pool_options = dialog.target.kind == xen_core::properties::ObjectKind::Pool
        || (dialog.target.kind == xen_core::properties::ObjectKind::Host
            && connection
                .and_then(crate::state::connection_inventory)
                .is_some_and(|inventory| inventory.hosts.len() == 1));
    let mut close = false;
    let response = egui::Modal::new(egui::Id::unique("object-properties"))
        .frame(theme::modal_frame())
        .show(ctx, |ui| {
            ui.set_width(530.0);
            ui.label(RichText::new("Properties").size(17.0).strong());
            ui.label(RichText::new(format!("{} · {} · {}", connection.map(connection_name).unwrap_or_default(), dialog.target.kind.label(), dialog.target.uuid)).size(9.5).color(theme::muted_fg()));
            ui.add_space(12.0);
            let reserve = if dialog.error.is_some() { 310.0 } else { 210.0 };
            egui::ScrollArea::vertical().id_salt(("property-fields", sequence)).max_height((ctx.content_rect().height()-reserve).max(80.0)).show(ui, |ui| {
                match &mut dialog.properties {
                    LoadState::Ready(properties) => {
                        ui.add_enabled_ui(writable && !dialog.submitting, |ui| {
                            widgets::field_label(ui, if properties.target.kind == xen_core::properties::ObjectKind::Pool { "Name (may be empty for an unnamed pool)" } else { "Name" });
                            ui.add(egui::TextEdit::singleline(&mut properties.name).desired_width(f32::INFINITY).char_limit(255));
                            ui.add_space(8.0);
                            widgets::field_label(ui, "Description");
                            ui.add(egui::TextEdit::multiline(&mut properties.description).desired_width(f32::INFINITY).desired_rows(4).char_limit(xen_core::properties::MAX_DESCRIPTION_CHARS));
                            ui.add_space(8.0);
                            if let Some(disk) = disks.iter().find(|d| d.vdi_uuid == dialog.target.uuid) {
                                widgets::field_label(ui, "Size and Location");
                                ui.horizontal(|ui| {
                                    ui.label(format!("{} · {}", crate::format::bytes(disk.virtual_size), disk.sr_name));
                                    if ui.button("Resize…").clicked() {
                                        actions.push(UiAction::CloseProperties);
                                        actions.push(UiAction::OpenDiskResize {vdi_uuid: disk.vdi_uuid.clone(), name: disk.vdi_name.clone(), current_gib: disk.virtual_size.div_ceil(1024*1024*1024)});
                                    }
                                });
                                widgets::field_label(ui, "Device options for this VM");
                                let mut mode = disk.mode.clone();
                                egui::ComboBox::from_id_salt("disk-properties-mode").selected_text(if mode == "RO" {"Read-only"} else {"Read/write"}).show_ui(ui,|ui| {
                                    ui.selectable_value(&mut mode, "RO".into(), "Read-only");
                                    ui.selectable_value(&mut mode, "RW".into(), "Read/write");
                                });
                                if mode != disk.mode {actions.push(UiAction::SetVbdMode {vbd_uuid:disk.vbd_uuid.clone(),mode});}
                                let mut bootable = disk.bootable;
                                if ui.checkbox(&mut bootable,"Bootable").changed() {actions.push(UiAction::SetVbdBootable {vbd_uuid:disk.vbd_uuid.clone(),bootable});}
                                ui.add_enabled(false,egui::Label::new(format!("Position: {}",disk.userdevice))).on_disabled_hover_text("Not implemented yet");
                                ui.add_space(8.0);
                            }
                            widgets::field_label(ui, "Tags");
                            let mut remove = None;
                            for (index, tag) in properties.tags.iter_mut().enumerate() {
                                ui.horizontal(|ui| {
                                    ui.add(egui::TextEdit::singleline(tag).desired_width(420.0).char_limit(255));
                                    if ui.small_button("Remove").clicked() { remove = Some(index); }
                                });
                            }
                            if let Some(index) = remove { properties.tags.remove(index); }
                            ui.horizontal(|ui| {
                                ui.add(egui::TextEdit::singleline(&mut dialog.new_tag).desired_width(420.0).char_limit(255).hint_text("New tag"));
                                if ui.add_enabled(!dialog.new_tag.trim().is_empty(), egui::Button::new("Add tag")).clicked() {
                                    properties.tags.push(std::mem::take(&mut dialog.new_tag));
                                }
                            });
                        });
                        if read_only { ui.label("This connection is read-only."); }
                        else if !available { ui.label("This connection is offline."); }
                    }
                    LoadState::Error(message) => { ui.label(RichText::new(message.as_str()).color(theme::red())); }
                    _ => { ui.spinner(); ui.label("Reading properties from the host…"); }
                }
            });
            if let Some(error) = &dialog.error {
                ui.add_space(8.0);
                egui::ScrollArea::vertical().id_salt(("property-error", sequence)).max_height(80.0).show(ui, |ui| { ui.label(RichText::new(error).color(theme::red())); });
            }
            if pool_options && ui.add_enabled(available && !dialog.submitting, egui::Button::new("Pool options… (VM autostart)")).clicked() {
                actions.push(UiAction::OpenPoolOptions { connection_id: dialog.connection_id.clone() });
            }
            ui.add_space(14.0);
            ui.horizontal(|ui| {
                if ui.add_enabled(!dialog.submitting, egui::Button::new("Close")).clicked() { close = true; }
                if ui.add_enabled(!dialog.submitting, egui::Button::new("Reload")).clicked() { actions.push(UiAction::ReloadProperties); }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let ready = matches!(&dialog.properties, LoadState::Ready(properties) if !properties.name.trim().is_empty() || properties.target.kind == xen_core::properties::ObjectKind::Pool || dialog.original.as_ref().is_some_and(|original| original.name == properties.name));
                    if ui.add_enabled(writable && ready && !dialog.submitting, widgets::primary_button(if dialog.submitting { "Saving…" } else { "Save changes" })).clicked() { actions.push(UiAction::SaveProperties); }
                });
            });
        });
    if (response.should_close() || close) && !dialog.submitting {
        actions.push(UiAction::CloseProperties);
    }
}
