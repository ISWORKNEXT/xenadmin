use egui::{Align, Layout, RichText, Ui};
use egui_extras::{Column, TableBuilder};
use xen_core::auth::{AuthInfo, AuthRole, AuthUser};
use xen_core::workspace::WorkspaceView;

use crate::state::{
    LoadState, ObjectKey, UiAction, UiState, connection_name, find_connection, is_read_only,
};
use crate::theme;
use crate::theme::*;
use crate::widgets;

pub fn show(
    ui: &mut Ui,
    state: &mut UiState,
    workspace: &WorkspaceView,
    actions: &mut Vec<UiAction>,
) {
    let Some(ObjectKey::Auth { connection_id }) = state
        .object_tabs
        .get(state.active_tab)
        .map(|tab| tab.key.clone())
    else {
        return;
    };
    let Some(connection) = find_connection(workspace, &connection_id) else {
        widgets::empty_state(
            ui,
            "Connection unavailable",
            "The saved connection was removed.",
        );
        return;
    };
    if matches!(state.auth_state, LoadState::Idle) {
        actions.push(UiAction::LoadAuth);
    }
    ui.horizontal(|ui| {
        ui.vertical(|ui| {
            ui.spacing_mut().item_spacing.y = 0.0;
            ui.label(
                RichText::new("AUTHENTICATION & RBAC")
                    .size(8.5)
                    .color(dim_fg()),
            );
            ui.label(
                RichText::new(connection_name(connection))
                    .size(23.0)
                    .strong(),
            );
            ui.label(
                RichText::new("Read-only external authentication, subjects and roles")
                    .size(9.5)
                    .color(muted_fg()),
            );
        });
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            if ui
                .add(egui::Button::new(RichText::new("Refresh").size(11.0)))
                .clicked()
            {
                actions.push(UiAction::LoadAuth);
            }
            if is_read_only(connection) {
                widgets::badge(ui, "Read-only", amber());
            }
        });
    });
    ui.add_space(10.0);

    match &state.auth_state {
        LoadState::Idle | LoadState::Loading => {
            ui.horizontal(|ui| {
                ui.spinner();
                ui.label(
                    RichText::new("Loading authentication and RBAC data…")
                        .size(10.5)
                        .color(muted_fg()),
                );
            });
        }
        LoadState::Error(message) => {
            theme::card_frame().show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.label(RichText::new("Could not load authentication data").strong());
                ui.label(RichText::new(message).size(10.0).color(red()));
            });
        }
        LoadState::Ready(info) => {
            summary(ui, info);
            ui.add_space(8.0);
            config_card(ui, info);
            ui.add_space(8.0);
            users_card(ui, &info.users, actions);
            ui.add_space(8.0);
            roles_card(ui, &info.roles);
        }
    }
}

fn summary(ui: &mut Ui, info: &AuthInfo) {
    ui.columns(4, |columns| {
        for (index, (label, value, color)) in [
            (
                "Authentication",
                info.auth_label().to_owned(),
                if info.auth_type.is_empty() {
                    amber()
                } else {
                    green()
                },
            ),
            (
                "Service",
                if info.service_name.is_empty() {
                    "Not configured".into()
                } else {
                    info.service_name.clone()
                },
                accent(),
            ),
            ("Subjects", info.users.len().to_string(), cyan()),
            ("Roles", info.roles.len().to_string(), purple()),
        ]
        .into_iter()
        .enumerate()
        {
            theme::card_frame()
                .inner_margin(egui::Margin::same(12))
                .show(&mut columns[index], |ui| {
                    ui.set_width(ui.available_width());
                    ui.horizontal(|ui| {
                        widgets::dot(ui, color, 7.0);
                        ui.label(RichText::new(label).size(10.0).color(muted_fg()));
                    });
                    ui.label(RichText::new(value).size(16.0).strong());
                });
        }
    });
}

fn config_card(ui: &mut Ui, info: &AuthInfo) {
    theme::card_frame().show(ui, |ui| {
        ui.set_width(ui.available_width());
        widgets::section_title(ui, "External authentication", None);
        ui.add_space(4.0);
        widgets::kv(ui, "Type", info.auth_label(), false);
        widgets::kv(
            ui,
            "Service name",
            if info.service_name.is_empty() {
                "—"
            } else {
                &info.service_name
            },
            false,
        );
        widgets::kv(
            ui,
            "Local auth threads",
            &info.local_auth_max_threads.to_string(),
            false,
        );
        widgets::kv(
            ui,
            "External auth cache",
            if info.ext_auth_cache_enabled {
                "Enabled"
            } else {
                "Disabled"
            },
            false,
        );
        if info.config.is_empty() {
            ui.label(
                RichText::new("No external auth configuration is present (local authentication).")
                    .size(10.0)
                    .color(dim_fg()),
            );
            return;
        }
        ui.add_space(6.0);
        TableBuilder::new(ui)
            .id_salt("auth-config")
            .striped(true)
            .cell_layout(Layout::left_to_right(Align::Center))
            .column(Column::remainder().at_least(160.0))
            .column(Column::remainder().at_least(220.0))
            .header(26.0, |mut header| {
                for title in ["Setting", "Value"] {
                    header.col(|ui| {
                        ui.label(RichText::new(title).size(9.5).color(muted_fg()));
                    });
                }
            })
            .body(|body| {
                body.rows(26.0, info.config.len(), |mut row| {
                    let entry = &info.config[row.index()];
                    row.col(|ui| {
                        ui.label(RichText::new(&entry.key).monospace().size(9.5));
                    });
                    row.col(|ui| {
                        ui.horizontal(|ui| {
                            ui.label(
                                RichText::new(&entry.value)
                                    .monospace()
                                    .size(9.5)
                                    .color(if entry.sensitive { amber() } else { fg() }),
                            );
                            if entry.sensitive {
                                ui.label(RichText::new("redacted").size(8.5).color(dim_fg()));
                            }
                        });
                    });
                });
            });
    });
}

fn users_card(ui: &mut Ui, users: &[AuthUser], actions: &mut Vec<UiAction>) {
    theme::card_frame().show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.horizontal(|ui| {
            widgets::section_title(ui, "Subjects", None);
            widgets::badge(ui, users.len().to_string(), muted_fg());
        });
        ui.add_space(4.0);
        if users.is_empty() {
            ui.label(
                RichText::new("No subjects are reported.")
                    .size(10.5)
                    .color(dim_fg()),
            );
            return;
        }
        TableBuilder::new(ui)
            .id_salt("auth-users")
            .striped(true)
            .cell_layout(Layout::left_to_right(Align::Center))
            .column(Column::initial(130.0))
            .column(Column::remainder().at_least(150.0))
            .column(Column::initial(180.0))
            .column(Column::remainder().at_least(160.0))
            .column(Column::exact(100.0))
            .header(26.0, |mut header| {
                for title in ["Identifier", "Display name", "Email", "Roles", "Actions"] {
                    header.col(|ui| {
                        ui.label(RichText::new(title).size(9.5).color(muted_fg()));
                    });
                }
            })
            .body(|body| {
                body.rows(28.0, users.len(), |mut row| {
                    let user = &users[row.index()];
                    row.col(|ui| {
                        ui.horizontal(|ui| {
                            if user.local_root {
                                widgets::dot(ui, amber(), 6.0);
                            }
                            ui.label(RichText::new(&user.identifier).monospace().size(9.5));
                        });
                    });
                    row.col(|ui| {
                        ui.add(
                            egui::Label::new(
                                RichText::new(if user.display_name.is_empty() {
                                    "—"
                                } else {
                                    &user.display_name
                                })
                                .size(10.5),
                            )
                            .truncate(),
                        );
                    });
                    row.col(|ui| {
                        ui.add(
                            egui::Label::new(
                                RichText::new(if user.email.is_empty() {
                                    "—"
                                } else {
                                    &user.email
                                })
                                .size(9.5)
                                .color(muted_fg()),
                            )
                            .truncate(),
                        );
                    });
                    row.col(|ui| {
                        ui.add(
                            egui::Label::new(
                                RichText::new(if user.roles.is_empty() {
                                    "No roles".to_owned()
                                } else {
                                    user.roles.join(", ")
                                })
                                .size(9.5)
                                .color(muted_fg()),
                            )
                            .truncate(),
                        );
                    });
                    row.col(|ui| {
                        if user.local_root {
                            ui.label(RichText::new("Protected").size(9.0).color(amber()));
                        } else if ui
                            .add(egui::Button::new(RichText::new("Edit roles").size(9.5)).small())
                            .clicked()
                        {
                            actions.push(UiAction::OpenUserRoles(user.uuid.clone()));
                        }
                    });
                });
            });
    });
}

fn roles_card(ui: &mut Ui, roles: &[AuthRole]) {
    theme::card_frame().show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.horizontal(|ui| {
            widgets::section_title(ui, "Roles", None);
            widgets::badge(ui, roles.len().to_string(), muted_fg());
        });
        ui.add_space(4.0);
        if roles.is_empty() {
            ui.label(
                RichText::new("No public roles are reported.")
                    .size(10.5)
                    .color(dim_fg()),
            );
            return;
        }
        TableBuilder::new(ui)
            .id_salt("auth-roles")
            .striped(true)
            .cell_layout(Layout::left_to_right(Align::Center))
            .column(Column::exact(150.0))
            .column(Column::remainder().at_least(260.0))
            .column(Column::exact(90.0))
            .header(26.0, |mut header| {
                for title in ["Role", "Description", "Permissions"] {
                    header.col(|ui| {
                        ui.label(RichText::new(title).size(9.5).color(muted_fg()));
                    });
                }
            })
            .body(|body| {
                body.rows(28.0, roles.len(), |mut row| {
                    let role = &roles[row.index()];
                    row.col(|ui| {
                        ui.label(RichText::new(&role.name).monospace().size(9.5));
                    });
                    row.col(|ui| {
                        ui.add(
                            egui::Label::new(
                                RichText::new(&role.description).size(9.5).color(muted_fg()),
                            )
                            .truncate(),
                        );
                    });
                    row.col(|ui| {
                        ui.label(
                            RichText::new(role.permissions.to_string())
                                .monospace()
                                .size(9.5)
                                .color(accent()),
                        );
                    });
                });
            });
    });
}
