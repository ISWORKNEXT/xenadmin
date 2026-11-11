use crate::{
    state::{UiAction, UiState, connection_inventory, find_connection, is_read_only},
    theme, widgets,
};
use egui::{Align, Layout};
use xen_core::workspace::WorkspaceView;

pub fn show(
    ctx: &egui::Context,
    state: &mut UiState,
    workspace: &WorkspaceView,
    actions: &mut Vec<UiAction>,
) {
    let Some(dialog) = state.xva_dialog.as_mut() else {
        return;
    };
    let connection = find_connection(workspace, &dialog.connection_id);
    let inventory = connection.and_then(connection_inventory);
    let available = inventory.is_some() && connection.is_some_and(|c| !is_read_only(c));
    let export = dialog.vm_uuid.is_some();
    let response=egui::Modal::new(egui::Id::unique("xva-transfer")).frame(theme::modal_frame()).show(ctx,|ui| {
        ui.set_width(600.0);
        ui.heading(if export {"Export XVA"} else {"Import XVA"});
        ui.add_space(8.0);
        if !available {ui.colored_label(theme::red(),"Connect with write access to transfer a VM.");}
        ui.add_enabled_ui(!dialog.submitting && available,|ui| {
            if export {ui.label(format!("Export {} as a complete XVA archive. The VM must be halted.",dialog.name));}
            else {ui.label("Import one VM or template from an XVA archive. The imported object stays halted.");}
            ui.add_space(8.0);
            widgets::field_label(ui,if export {"Destination file on this computer"} else {"XVA file on this computer"});
            ui.add(egui::TextEdit::singleline(&mut dialog.path).desired_width(f32::INFINITY));
            if export {ui.label("Choose a new file name. Existing files are preserved.");}
            else {
                ui.add_space(8.0);
                widgets::field_label(ui,"Imported name");
                ui.add(egui::TextEdit::singleline(&mut dialog.name).char_limit(255).desired_width(f32::INFINITY));
                widgets::field_label(ui,"Storage repository");
                egui::ComboBox::from_id_salt("xva-sr").selected_text(inventory.and_then(|i|i.storage.iter().find(|s|Some(&s.uuid)==dialog.sr_uuid.as_ref())).map(|s|s.name.as_str()).unwrap_or("Select storage…")).width(ui.available_width()).show_ui(ui,|ui| {
                    if let Some(inventory)=inventory {
                        for sr in inventory.storage.iter().filter(|sr|sr.content_type!="iso" && !matches!(sr.storage_type.as_str(),"udev"|"iso") && sr.pbd_attached>0) {
                            ui.selectable_value(&mut dialog.sr_uuid,Some(sr.uuid.clone()),&sr.name);
                        }
                    }
                });
                widgets::field_label(ui,"Guest network");
                egui::ComboBox::from_id_salt("xva-network").selected_text(inventory.and_then(|i|i.networks.iter().find(|n|Some(&n.uuid)==dialog.network_uuid.as_ref())).map(|n|n.name.as_str()).unwrap_or("None")).width(ui.available_width()).show_ui(ui,|ui| {
                    ui.selectable_value(&mut dialog.network_uuid,None,"None");
                    if let Some(inventory)=inventory {
                        for network in inventory.networks.iter().filter(|n|n.bridge!="xenapi") {
                            ui.selectable_value(&mut dialog.network_uuid,Some(network.uuid.clone()),&network.name);
                        }
                    }
                });
                ui.label("The imported VM gets one interface on the selected network. Choose None to import without networking.");
            }
        });
        if let Some(error)=&dialog.error {ui.colored_label(theme::red(),error);}
        ui.add_space(16.0);
        ui.horizontal(|ui| {
            if ui.add_enabled(!dialog.submitting,egui::Button::new("Cancel")).clicked() {actions.push(UiAction::CloseXvaDialog);}
            ui.with_layout(Layout::right_to_left(Align::Center),|ui| {
                let valid=available && !dialog.submitting && !dialog.path.trim().is_empty() && (export || (dialog.sr_uuid.is_some() && !dialog.name.trim().is_empty()));
                if ui.add_enabled(valid,widgets::primary_button(if export {"Export"} else {"Import"})).clicked() {actions.push(UiAction::SubmitXvaTransfer);}
            });
        });
    });
    if response.should_close() && !dialog.submitting {
        actions.push(UiAction::CloseXvaDialog);
    }
}
