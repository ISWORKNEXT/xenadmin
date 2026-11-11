//! Main-window layout. All command decisions live in `menubar`.
use super::menubar::{self as model, CommandAction, MenuItem};
use crate::{
    config,
    state::{ConnectState, UiAction, UiState, connection_id, is_online},
    theme,
};
use egui::{Align, Button, Frame, Layout, RichText, Ui, ViewportCommand};
use xen_core::workspace::WorkspaceView;

pub fn menubar(
    ui: &mut Ui,
    state: &mut UiState,
    workspace: &WorkspaceView,
    actions: &mut Vec<UiAction>,
) {
    if !ui.ctx().egui_wants_keyboard_input() {
        let key = model::selection(state);
        let command = if ui.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Delete)) {
            Some(match key {
                crate::state::ObjectKey::Template { .. } => "DeleteTemplateCommand",
                crate::state::ObjectKey::Snapshot { .. } => "DeleteSnapshotCommand",
                _ => "DeleteVMsAndTemplatesCommand",
            })
        } else if state.tree_selection.len() <= 1
            && ui.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::F2))
        {
            Some(match key {
                crate::state::ObjectKey::Host { .. } => "HostPropertiesCommand",
                crate::state::ObjectKey::Storage { .. } => "SRPropertiesCommand",
                crate::state::ObjectKey::Network { .. } => "PropertiesCommand",
                crate::state::ObjectKey::Template { .. } => "TemplatePropertiesCommand",
                _ => "VMPropertiesCommand",
            })
        } else {
            None
        };
        if let Some(command) = command
            && let Ok(action) = model::resolve(command, state, workspace)
        {
            perform(action, ui.ctx(), state, workspace, actions);
        }
    }
    egui::Panel::top("menu-bar")
        .frame(Frame::NONE.fill(theme::panel()).inner_margin(egui::Margin {
            left: 8,
            ..Default::default()
        }))
        .show(ui, |ui| {
            ui.style_mut().override_font_id = Some(egui::FontId::proportional(11.0));
            egui::MenuBar::new().ui(ui, |ui| {
                for menu in model::menus(state, workspace) {
                    ui.menu_button(&menu.label, |ui| {
                        render_items(ui, &menu.children, state, workspace, actions);
                    });
                }
            });
        });
}
pub(super) fn compact_menu(ui: &mut Ui) {
    ui.style_mut().override_font_id = Some(egui::FontId::proportional(11.0));
    ui.spacing_mut().interact_size.y = 22.0;
    ui.spacing_mut().button_padding = egui::vec2(4.0, 0.0);
    ui.spacing_mut().item_spacing.y = 0.0;
}
pub(super) fn render_items(
    ui: &mut Ui,
    items: &[MenuItem],
    state: &mut UiState,
    workspace: &WorkspaceView,
    actions: &mut Vec<UiAction>,
) {
    compact_menu(ui);
    let max_height = (ui.ctx().content_rect().height() - 20.0).max(22.0);
    ui.set_max_height(max_height);
    egui::ScrollArea::vertical()
        .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::VisibleWhenNeeded)
        .max_height(max_height)
        .show(ui, |ui| {
            for item in items {
                if item.label == "---" {
                    ui.add(egui::Separator::default().spacing(7.0));
                    continue;
                }
                if !item.children.is_empty() {
                    // A submenu is navigable when at least one descendant can run.
                    let reason = model::submenu_reason(item, state, workspace);
                    let response = ui
                        .add_enabled_ui(reason.is_none(), |ui| {
                            ui.menu_button(RichText::new(&item.label).size(11.0), |ui| {
                                render_items(ui, &item.children, state, workspace, actions)
                            });
                        })
                        .response;
                    if let Some(reason) = reason {
                        response.on_disabled_hover_text(reason);
                    }
                } else {
                    let resolution = model::resolve_item(item, state, workspace);
                    let checked = match item.command {
                        "CustomTemplates" => Some(state.tree_preferences.custom_templates),
                        "XenDesktopTemplates" => Some(state.tree_preferences.default_templates),
                        "LocalStorage" => Some(state.tree_preferences.local_storage),
                        "HiddenObjects" => Some(state.tree_preferences.hidden_objects),
                        _ => None,
                    };
                    let label = if let Some(checked) = checked {
                        format!("{} {}", if checked { "✓" } else { "  " }, item.label)
                    } else if item.command == "Toolbar" {
                        format!("{} Toolbar", if state.toolbar_visible { "✓" } else { "  " })
                    } else {
                        item.label.clone()
                    };
                    let response = ui.add_enabled(
                        resolution.is_ok(),
                        Button::new(RichText::new(label).size(11.0)),
                    );
                    match resolution {
                        Ok(action) if response.clicked() => {
                            perform(action, ui.ctx(), state, workspace, actions);
                            ui.close();
                        }
                        Err(reason) => {
                            response.on_disabled_hover_text(reason);
                        }
                        _ => {}
                    }
                }
            }
        });
}

fn perform(
    action: CommandAction,
    ctx: &egui::Context,
    state: &mut UiState,
    workspace: &WorkspaceView,
    actions: &mut Vec<UiAction>,
) {
    match action {
        CommandAction::Dispatch(action) => actions.push(action),
        CommandAction::DispatchMany(batch) => {
            let console_keys = batch
                .iter()
                .any(|action| matches!(action, UiAction::ConsoleKey { .. }));
            actions.extend(batch);
            if console_keys && let Some(console) = &mut state.console {
                console.modifiers = (false, false, false, false);
            }
        }
        CommandAction::Connect => state.connect = Some(ConnectState::empty()),
        CommandAction::Exit => ctx.send_viewport_cmd(ViewportCommand::Close),
        CommandAction::Toolbar => state.toolbar_visible = !state.toolbar_visible,
        CommandAction::TreeView(command) => {
            let setting = match command {
                "CustomTemplates" => &mut state.tree_preferences.custom_templates,
                "XenDesktopTemplates" => &mut state.tree_preferences.default_templates,
                "LocalStorage" => &mut state.tree_preferences.local_storage,
                _ => &mut state.tree_preferences.hidden_objects,
            };
            *setting = !*setting;
            config::save_ui_preferences(&config::UiPreferences {
                light: theme::is_light(),
                zoom: ctx.zoom_factor(),
                tree: state.tree_preferences.clone(),
            });
        }
        CommandAction::Theme => {
            theme::toggle();
            theme::apply(ctx);
            save_preferences(ctx);
        }
        CommandAction::Zoom(factor) => {
            ctx.set_zoom_factor(if factor == 1.0 {
                1.0
            } else {
                (ctx.zoom_factor() * factor).clamp(0.7, 2.0)
            });
            save_preferences(ctx);
        }
        CommandAction::AllConnections(connect) => {
            for connection in &workspace.connections {
                if connect != is_online(connection) {
                    let id = connection_id(connection).to_owned();
                    actions.push(if connect {
                        UiAction::Reconnect(id)
                    } else {
                        UiAction::Disconnect(id)
                    });
                }
            }
        }
    }
}
fn save_preferences(ctx: &egui::Context) {
    config::save_ui_preferences(&config::UiPreferences {
        light: theme::is_light(),
        zoom: ctx.zoom_factor(),
        tree: config::load_ui_preferences().tree,
    });
}

pub fn toolbar(
    ui: &mut Ui,
    state: &mut UiState,
    workspace: &WorkspaceView,
    actions: &mut Vec<UiAction>,
) {
    if !state.toolbar_visible {
        return;
    }
    egui::Panel::top("toolbar")
        .exact_size(42.0)
        .frame(Frame::NONE.fill(theme::panel()))
        .show(ui, |ui| {
            egui::ScrollArea::horizontal().show(ui, |ui| {
                ui.horizontal_centered(|ui| {
                    for (label, command) in [
                        ("‹ Back", "Back"),
                        ("Forward ›", "Forward"),
                        ("|", ""),
                        ("Add New Server", "AddHostCommand"),
                        ("|", ""),
                        ("New Pool", "NewPoolCommand"),
                        ("New Storage", "NewSRCommand"),
                        ("New VM", "NewVMCommand"),
                        ("|", ""),
                    ] {
                        if label == "|" {
                            ui.separator();
                        } else {
                            toolbar_button(ui, label, command, state, workspace, actions);
                        }
                    }
                    let can = |command| model::resolve(command, state, workspace).is_ok();
                    let start = matches!(model::selection(state), crate::state::ObjectKey::Vm { connection_id, vm_uuid }
                        if crate::state::find_vm(workspace, connection_id, vm_uuid).is_some_and(|(_, vm)| vm.power_state == "Halted"));
                    let power = can("PowerOnHostCommand");
                    let resume = can("ResumeVMCommand");
                    let unpause = can("UnPauseVMCommand");
                    let host = matches!(
                        model::selection(state),
                        crate::state::ObjectKey::Host { .. }
                    );
                    let shutdown = if host {
                        "ShutDownHostCommand"
                    } else {
                        "ShutDownVMCommand"
                    };
                    let reboot = if host {
                        "RebootHostCommand"
                    } else {
                        "RebootVMCommand"
                    };
                    let force_shutdown = can("ForceVMShutDownCommand") && !can("ShutDownVMCommand");
                    let force_reboot = can("ForceVMRebootCommand") && !can("RebootVMCommand");
                    for (label, command, visible) in [
                        ("Shut Down", shutdown, can(shutdown) || (!start && !power)),
                        ("Power On", "PowerOnHostCommand", power),
                        ("Start", "StartVMCommand", start),
                        ("Reboot", reboot, true),
                        ("Resume", "ResumeVMCommand", resume),
                        ("Suspend", "SuspendVMCommand", !resume),
                        ("Pause", "PauseVMCommand", matches!(model::selection(state), crate::state::ObjectKey::Vm { connection_id, vm_uuid } if crate::state::find_vm(workspace,connection_id,vm_uuid).is_some_and(|(_, vm)| vm.power_state == "Running"))),
                        ("Unpause", "UnPauseVMCommand", unpause),
                        ("Force Shut Down", "ForceVMShutDownCommand", force_shutdown),
                        ("Force Reboot", "ForceVMRebootCommand", force_reboot),
                    ] {
                        if visible {
                            toolbar_button(ui, label, command, state, workspace, actions);
                        }
                    }
                });
            });
        });
}
fn toolbar_button(
    ui: &mut Ui,
    label: &str,
    command: &str,
    state: &mut UiState,
    workspace: &WorkspaceView,
    actions: &mut Vec<UiAction>,
) {
    let resolution = model::resolve(command, state, workspace);
    let icon = match command {
        "AddHostCommand" | "PowerOnHostCommand" => Some(super::sidebar::TreeIcon::Host),
        "NewPoolCommand" => Some(super::sidebar::TreeIcon::Pool),
        "NewSRCommand" => Some(super::sidebar::TreeIcon::Storage),
        "Back" | "Forward" => None,
        _ => Some(super::sidebar::TreeIcon::Vm),
    };
    let text = if icon.is_some() {
        format!("     {label}")
    } else {
        label.into()
    };
    let response = ui.add_enabled(
        resolution.is_ok(),
        Button::new(RichText::new(text).size(11.0)),
    );
    if let Some(icon) = icon {
        let rect = egui::Rect::from_center_size(
            egui::pos2(response.rect.left() + 10.0, response.rect.center().y),
            egui::Vec2::splat(14.0),
        );
        super::sidebar::paint_icon(
            ui.painter(),
            rect,
            icon,
            if resolution.is_ok() {
                theme::muted_fg()
            } else {
                theme::dim_fg()
            },
        );
    }
    match resolution {
        Ok(action) if response.clicked() => perform(action, ui.ctx(), state, workspace, actions),
        Err(reason) => {
            response.on_disabled_hover_text(reason);
        }
        _ => {}
    }
}
pub fn statusbar(
    ui: &mut Ui,
    state: &mut UiState,
    workspace: &WorkspaceView,
    actions: &mut Vec<UiAction>,
) {
    egui::Panel::bottom("status-bar")
        .exact_size(26.0)
        .frame(Frame::NONE.fill(theme::panel()))
        .show(ui, |ui| {
            ui.horizontal_centered(|ui| {
                let latest = workspace.tasks.iter().max_by_key(|task| task.started_at);
                let refresh_newer = state.updated.is_some_and(|t| {
                    latest.is_none_or(|task| t.timestamp() as u64 > task.started_at)
                });
                let status = if !state.busy.is_empty() {
                    "Refreshing inventory…".into()
                } else if refresh_newer {
                    state
                        .updated
                        .map(|t| format!("Inventory refreshed {}", t.format("%H:%M:%S")))
                        .unwrap_or_default()
                } else if let Some(task) = latest {
                    format!(
                        "{} · {} · {}{}",
                        task.object_name,
                        task.action,
                        crate::state::task_status_label(task.status),
                        if task.message.is_empty() {
                            String::new()
                        } else {
                            format!(" · {}", task.message)
                        }
                    )
                } else {
                    "Ready".into()
                };
                let status_width = (ui.available_width() - 165.0).max(0.0);
                ui.allocate_ui_with_layout(
                    egui::vec2(status_width, 20.0),
                    Layout::left_to_right(Align::Center),
                    |ui| {
                        ui.set_min_width(status_width);
                        ui.add(egui::Label::new(RichText::new(status).size(10.5)).truncate());
                    },
                );
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if let Some((title, progress)) = super::tasks::running(workspace) {
                        let response = ui.add(
                            egui::ProgressBar::new(progress)
                                .desired_width(160.0)
                                .text(format!("{} {:.0}%", title, progress * 100.0)),
                        );
                        if response.interact(egui::Sense::click()).clicked() {
                            super::tasks::open(state, false, actions);
                        }
                        ui.spinner();
                    } else if !state.busy.is_empty() {
                        ui.spinner();
                    }
                });
            });
        });
}
