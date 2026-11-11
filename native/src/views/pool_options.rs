use crate::{
    state::{LoadState, UiAction, UiState, find_connection, is_online, is_read_only},
    theme, widgets,
};
use xen_core::workspace::WorkspaceView;

pub fn show(
    ctx: &egui::Context,
    state: &mut UiState,
    workspace: &WorkspaceView,
    actions: &mut Vec<UiAction>,
) {
    let Some(dialog) = state.pool_options.as_mut() else {
        return;
    };
    let connection = find_connection(workspace, &dialog.connection_id);
    let writable = connection.is_some_and(|c| is_online(c) && !is_read_only(c));
    let mut close = false;
    let response = egui::Modal::new(egui::Id::unique("pool-options"))
        .frame(theme::modal_frame())
        .show(ctx, |ui| {
            ui.set_width(540.0);
            ui.heading("Pool options");
            ui.label(egui::RichText::new(&dialog.uuid).monospace().size(10.0));
            ui.add_space(12.0);
            egui::ScrollArea::vertical()
                .max_height((ctx.content_rect().height() - 260.0).max(100.0))
                .show(ui, |ui| {
                    match &mut dialog.data {
                        LoadState::Ready(data) => {
                            ui.add_enabled_ui(writable && !dialog.submitting, |ui| {
                                flag(
                                    ui,
                                    &mut data.migration_compression,
                                    "Compress migration traffic",
                                );
                                flag(ui, &mut data.igmp_snooping_enabled, "Enable IGMP snooping");
                                if let Some(disabled) = data.live_patching_disabled.as_mut() {
                                    let mut enabled = !*disabled;
                                    if ui.checkbox(&mut enabled, "Enable live patching").changed() {
                                        *disabled = !enabled;
                                    }
                                } else {
                                    ui.label("Live patching policy is unavailable on this server.");
                                }
                            })
                            .response
                        }
                        LoadState::Error(message) => {
                            ui.colored_label(theme::red(), message.as_str())
                        }
                        _ => {
                            ui.spinner();
                            ui.label("Reading pool options…")
                        }
                    };
                });
            if let Some(error) = &dialog.error {
                egui::ScrollArea::vertical()
                    .id_salt("pool-options-error")
                    .max_height(90.0)
                    .show(ui, |ui| {
                        ui.colored_label(theme::red(), error);
                    });
            }
            if !writable {
                ui.label("This connection is offline or read-only.");
            }
            ui.add_space(12.0);
            ui.horizontal(|ui| {
                if ui
                    .add_enabled(!dialog.submitting, egui::Button::new("Close"))
                    .clicked()
                {
                    close = true;
                }
                if ui
                    .add_enabled(!dialog.submitting, egui::Button::new("Reload"))
                    .clicked()
                {
                    actions.push(UiAction::ReloadPoolOptions);
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let ready =
                        matches!(dialog.data, LoadState::Ready(_)) && dialog.original.is_some();
                    if ui
                        .add_enabled(
                            writable && ready && !dialog.submitting,
                            widgets::primary_button(if dialog.submitting {
                                "Saving…"
                            } else {
                                "Save changes"
                            }),
                        )
                        .clicked()
                    {
                        actions.push(UiAction::SavePoolOptions);
                    }
                });
            });
        });
    if (response.should_close() || close) && !dialog.submitting {
        actions.push(UiAction::ClosePoolOptions);
    }
}

fn flag(ui: &mut egui::Ui, value: &mut Option<bool>, label: &str) {
    if let Some(value) = value {
        ui.checkbox(value, label);
    } else {
        ui.label(format!("{label}: unavailable on this server."));
    }
}
