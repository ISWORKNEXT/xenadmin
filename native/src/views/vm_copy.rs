use crate::{
    state::{UiAction, UiState, connection_inventory, find_connection, is_read_only},
    theme, widgets,
};
use xen_core::workspace::WorkspaceView;

pub fn show(
    ctx: &egui::Context,
    state: &mut UiState,
    workspace: &WorkspaceView,
    actions: &mut Vec<UiAction>,
) {
    let Some(dialog) = state.vm_copy.as_mut() else {
        return;
    };
    let connection = find_connection(workspace, &dialog.connection_id);
    let inventory = connection.and_then(connection_inventory);
    let writable = inventory.is_some() && connection.is_some_and(|c| !is_read_only(c));
    let mut close = false;
    let response=egui::Modal::new(egui::Id::unique("full-copy-vm")).frame(theme::modal_frame()).show(ctx,|ui| {
        ui.set_width(540.0);
        ui.heading("Full copy VM / template");
        ui.label("Creates an independent halted copy with all disks on the selected storage repository.");
        egui::ScrollArea::vertical().max_height((ctx.content_rect().height()-250.0).max(100.0)).show(ui,|ui| {
            ui.add_enabled_ui(writable,|ui| {
                widgets::field_label(ui,"Copy name");
                ui.add(egui::TextEdit::singleline(&mut dialog.request.name).desired_width(f32::INFINITY).char_limit(255));
                widgets::field_label(ui,"Description");
                ui.add(egui::TextEdit::multiline(&mut dialog.request.description).desired_width(f32::INFINITY).desired_rows(4).char_limit(xen_core::properties::MAX_DESCRIPTION_CHARS));
                widgets::field_label(ui,"Destination storage repository");
                let selected=inventory.and_then(|i|i.storage.iter().find(|s|s.uuid==dialog.request.sr_uuid));
                egui::ComboBox::from_id_salt("copy-destination-sr").selected_text(selected.map(|s|s.name.as_str()).unwrap_or("Choose storage…")).width(ui.available_width()).show_ui(ui,|ui| {
                    if let Some(inventory)=inventory {
                        for sr in inventory.storage.iter().filter(|s|s.pbd_attached>0 && s.content_type!="iso" && !matches!(s.storage_type.as_str(),"iso"|"udev")) {
                            ui.selectable_value(&mut dialog.request.sr_uuid,sr.uuid.clone(),format!("{} · {} free",sr.name,crate::format::bytes(sr.total.saturating_sub(sr.used))));
                        }
                    }
                });
            });
        });
        if !writable {ui.colored_label(theme::amber(),"Reconnect with a writable connection to copy this object.");}
        ui.add_space(12.0);
        ui.horizontal(|ui| {
            if ui.button("Cancel").clicked() {close=true;}
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center),|ui| {
                let destination_available=inventory.is_some_and(|i|i.storage.iter().any(|s|s.uuid==dialog.request.sr_uuid && s.pbd_attached>0 && s.content_type!="iso" && !matches!(s.storage_type.as_str(),"iso"|"udev")));
                if ui.add_enabled(writable && destination_available && !dialog.request.name.trim().is_empty(),widgets::primary_button("Create full copy")).clicked() {actions.push(UiAction::SubmitVmCopy);}
            });
        });
    });
    if response.should_close() || close {
        actions.push(UiAction::CloseVmCopy);
    }
}
