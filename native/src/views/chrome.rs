use crate::state::{Scope, connection_inventory, find_connection};
use crate::theme::*;
use egui::{CornerRadius, Frame, Margin, RichText, Stroke, Ui};
use xen_core::workspace::WorkspaceView;

pub fn scope_name(workspace: &WorkspaceView, scope: &Scope) -> String {
    match scope {
        Scope::All => "All connections".into(),
        Scope::Connection(id) => {
            find_connection(workspace, id).map_or_else(|| id.clone(), crate::state::connection_name)
        }
        Scope::Host {
            connection_id,
            host_ref,
        } => find_connection(workspace, connection_id)
            .and_then(|connection| {
                connection_inventory(connection).and_then(|inventory| {
                    inventory
                        .hosts
                        .iter()
                        .find(|host| &host.reference == host_ref)
                        .map(|host| host.name.clone())
                })
            })
            .unwrap_or_else(|| "Host".into()),
    }
}

pub fn eyebrow(ui: &mut Ui, text: &str) {
    ui.label(
        RichText::new(text.to_uppercase())
            .size(9.0)
            .color(with_alpha(accent(), 220)),
    );
}

pub fn error_banner(ui: &mut Ui, text: &str) {
    Frame::NONE
        .fill(with_alpha(red(), 18))
        .stroke(Stroke::new(1.0, with_alpha(red(), 90)))
        .corner_radius(CornerRadius::same(8))
        .inner_margin(Margin::symmetric(12, 8))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(RichText::new("!").size(11.0).strong().color(red()));
                ui.add(egui::Label::new(RichText::new(text).size(11.0).color(fg())).wrap());
            });
        });
}
