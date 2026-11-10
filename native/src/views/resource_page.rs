use egui::{Align, Layout, RichText, Sense, Ui, Vec2};
use xen_core::{operations::SrAction, workspace::WorkspaceView};

use crate::format;
use crate::state::{
    ObjectKey, UiAction, UiState, connection_name, find_network, find_storage, is_read_only,
};
use crate::theme;
use crate::theme::*;
use crate::widgets;

pub fn storage(
    ui: &mut Ui,
    state: &mut UiState,
    workspace: &WorkspaceView,
    actions: &mut Vec<UiAction>,
) {
    let Some(ObjectKey::Storage {
        connection_id,
        sr_uuid,
    }) = active_key(state)
    else {
        return;
    };
    let Some((connection, sr)) = find_storage(workspace, &connection_id, &sr_uuid) else {
        missing(ui, "Storage repository is no longer present.");
        return;
    };
    header(
        ui,
        "S",
        &sr.name,
        &format!("{} · {}", connection_name(connection), sr.storage_type),
        &sr.uuid,
        is_read_only(connection),
        &connection_id,
        actions,
    );

    ui.horizontal(|ui| {
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            ui.menu_button(RichText::new("Manage storage…").size(10.5), |ui| {
                if ui
                    .add_enabled(
                        !is_read_only(connection),
                        egui::Button::new(RichText::new("Scan storage").size(11.0)),
                    )
                    .clicked()
                {
                    actions.push(UiAction::PromptSrAction {
                        connection_id: connection_id.clone(),
                        sr_uuid: sr.uuid.clone(),
                        sr_name: sr.name.clone(),
                        action: SrAction::Scan,
                    });
                    ui.close();
                }
                ui.separator();
                if ui
                    .add_enabled(
                        !is_read_only(connection),
                        egui::Button::new(
                            RichText::new("Destroy storage repository…")
                                .size(11.0)
                                .color(red()),
                        ),
                    )
                    .clicked()
                {
                    actions.push(UiAction::PromptSrAction {
                        connection_id: connection_id.clone(),
                        sr_uuid: sr.uuid.clone(),
                        sr_name: sr.name.clone(),
                        action: SrAction::Destroy,
                    });
                    ui.close();
                }
            });
        });
    });
    ui.add_space(4.0);

    ui.columns(2, |columns| {
        theme::card_frame().show(&mut columns[0], |ui| {
            ui.set_width(ui.available_width());
            widgets::section_title(ui, "Storage repository", None);
            ui.add_space(4.0);
            widgets::kv(ui, "Name", &sr.name, false);
            widgets::kv(ui, "Type", &sr.storage_type, false);
            widgets::kv(
                ui,
                "Scope",
                if sr.shared { "Shared" } else { "Local" },
                false,
            );
            widgets::kv(
                ui,
                "Description",
                if sr.description.is_empty() {
                    "No description"
                } else {
                    &sr.description
                },
                false,
            );
            ui.add_space(2.0);
            widgets::kv(ui, "UUID", &sr.uuid, true);
        });
        theme::card_frame().show(&mut columns[1], |ui| {
            ui.set_width(ui.available_width());
            widgets::section_title(ui, "Capacity", None);
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                ui.label(RichText::new(format::bytes(sr.used)).size(23.0).strong());
                ui.label(
                    RichText::new(if sr.total == 0 {
                        "capacity not reported".into()
                    } else {
                        format!("/ {}", format::bytes(sr.total))
                    })
                    .size(11.0)
                    .color(muted_fg()),
                );
            });
            ui.add_space(4.0);
            widgets::resource_bar(ui, sr.used, sr.total, green(), 6.0);
            ui.horizontal(|ui| {
                ui.label(RichText::new("Used").size(9.0).color(dim_fg()));
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    ui.label(
                        RichText::new(if sr.total == 0 {
                            "—".into()
                        } else {
                            format!(
                                "{:.0}% · {} free",
                                format::percent(sr.used, sr.total),
                                format::bytes(sr.total.saturating_sub(sr.used))
                            )
                        })
                        .size(9.0)
                        .color(dim_fg()),
                    );
                });
            });
        });
    });
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
    }) = active_key(state)
    else {
        return;
    };
    let Some((connection, network)) = find_network(workspace, &connection_id, &network_uuid) else {
        missing(ui, "Network is no longer present.");
        return;
    };
    header(
        ui,
        "N",
        &network.name,
        &format!("{} · {}", connection_name(connection), network.bridge),
        &network.uuid,
        is_read_only(connection),
        &connection_id,
        actions,
    );

    ui.horizontal(|ui| {
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            ui.menu_button(RichText::new("Manage network…").size(10.5), |ui| {
                if ui
                    .add_enabled(
                        !is_read_only(connection),
                        egui::Button::new(
                            RichText::new("Destroy network…").size(11.0).color(red()),
                        ),
                    )
                    .clicked()
                {
                    actions.push(UiAction::PromptDestroyNetwork {
                        connection_id: connection_id.clone(),
                        network_uuid: network.uuid.clone(),
                        name: network.name.clone(),
                    });
                    ui.close();
                }
            });
        });
    });
    ui.add_space(4.0);

    ui.columns(2, |columns| {
        theme::card_frame().show(&mut columns[0], |ui| {
            ui.set_width(ui.available_width());
            widgets::section_title(ui, "Network", None);
            ui.add_space(4.0);
            widgets::kv(ui, "Name", &network.name, false);
            widgets::kv(ui, "Bridge", &network.bridge, true);
            widgets::kv(ui, "MTU", &network.mtu.to_string(), false);
            widgets::kv(
                ui,
                "Description",
                if network.description.is_empty() {
                    "No description"
                } else {
                    &network.description
                },
                false,
            );
            ui.add_space(2.0);
            widgets::kv(ui, "UUID", &network.uuid, true);
        });
        theme::card_frame().show(&mut columns[1], |ui| {
            ui.set_width(ui.available_width());
            widgets::section_title(ui, "Attachments", None);
            ui.add_space(4.0);
            widgets::kv(
                ui,
                "Physical interfaces",
                &network.interface_count.to_string(),
                false,
            );
            widgets::kv(
                ui,
                "Virtual interfaces",
                &network.virtual_interface_count.to_string(),
                false,
            );
        });
    });
}

fn active_key(state: &UiState) -> Option<ObjectKey> {
    state
        .object_tabs
        .get(state.active_tab)
        .map(|tab| tab.key.clone())
}

#[allow(clippy::too_many_arguments)]
fn header(
    ui: &mut Ui,
    mark: &str,
    title: &str,
    subtitle: &str,
    uuid: &str,
    read_only: bool,
    connection_id: &str,
    actions: &mut Vec<UiAction>,
) {
    ui.horizontal(|ui| {
        let (rect, _) = ui.allocate_exact_size(Vec2::splat(42.0), Sense::hover());
        ui.painter()
            .rect_filled(rect, egui::CornerRadius::same(11), panel_2());
        ui.painter().rect_stroke(
            rect,
            egui::CornerRadius::same(11),
            egui::Stroke::new(1.0, border()),
            egui::StrokeKind::Inside,
        );
        ui.painter().text(
            rect.center(),
            egui::Align2::CENTER_CENTER,
            mark,
            egui::FontId::proportional(17.0),
            accent(),
        );
        ui.vertical(|ui| {
            ui.spacing_mut().item_spacing.y = 0.0;
            ui.label(RichText::new("RESOURCE").size(8.5).color(dim_fg()));
            ui.label(RichText::new(title).size(21.0).strong());
            ui.label(RichText::new(subtitle).size(9.5).color(muted_fg()));
        });
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            if ui
                .add(egui::Button::new(RichText::new("Refresh").size(10.5)))
                .clicked()
            {
                actions.push(UiAction::Refresh(Some(connection_id.to_owned())));
            }
            if read_only {
                widgets::badge(ui, "Read-only", amber());
            }
        });
    });
    ui.label(RichText::new(uuid).monospace().size(8.5).color(dim_fg()));
    ui.add_space(8.0);
}

fn missing(ui: &mut Ui, message: &str) {
    widgets::empty_state(ui, "Object unavailable", message);
}
