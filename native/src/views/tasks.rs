//! Notifications pages, backed by the workspace history and XenAPI inventory.
use std::collections::HashSet;

use egui::{RichText, Ui};
use xen_core::{operations::TaskStatus, workspace::WorkspaceView};

use crate::state::{
    ObjectKey, UiAction, UiState, connection_id, connection_inventory, is_read_only,
};

#[derive(Default)]
pub struct Notifications {
    pub alerts: bool,
    severity: String,
    status: String,
    server: String,
    days: u64,
    selected: HashSet<String>,
    expanded: HashSet<String>,
    dismissed: HashSet<String>,
    export: Option<String>,
    export_filtered: bool,
    export_error: String,
    overwrite: bool,
    help: Option<(String, String)>,
    pending_dismiss: Vec<(Entry, bool)>,
}

#[derive(Clone)]
enum NotificationAction {
    Cancel(String),
    Check(String),
    Dismiss {
        connection_id: String,
        message_uuid: String,
    },
    CancelExternal {
        connection_id: String,
        reference: String,
    },
}
impl NotificationAction {
    fn action(&self) -> UiAction {
        match self {
            Self::Cancel(id) => UiAction::CancelTask(id.clone()),
            Self::Check(id) => UiAction::CheckTask(id.clone()),
            Self::Dismiss {
                connection_id,
                message_uuid,
            } => UiAction::DismissAlert {
                connection_id: connection_id.clone(),
                message_uuid: message_uuid.clone(),
            },
            Self::CancelExternal {
                connection_id,
                reference,
            } => UiAction::CancelExternalTask {
                connection_id: connection_id.clone(),
                reference: reference.clone(),
            },
        }
    }
}

#[derive(Clone)]
struct Entry {
    id: String,
    connection: String,
    server: String,
    location: String,
    host: String,
    title: String,
    body: String,
    status: String,
    time: u64,
    progress: f32,
    cancel: Option<NotificationAction>,
    dismiss: Option<NotificationAction>,
    target: Option<ObjectKey>,
    running: bool,
    local: bool,
}

fn severity(priority: u64) -> &'static str {
    match priority {
        1 => "Critical",
        2 => "Major",
        3 => "Warning",
        4 => "Minor",
        5 => "Information",
        _ => "Unknown",
    }
}

fn target(workspace: &WorkspaceView, id: &str, uuid: &str) -> Option<ObjectKey> {
    let inv = crate::state::find_connection(workspace, id).and_then(connection_inventory)?;
    let connection_id = id.to_owned();
    if inv.vms.iter().any(|v| v.uuid == uuid) {
        Some(ObjectKey::Vm {
            connection_id,
            vm_uuid: uuid.into(),
        })
    } else if inv.hosts.iter().any(|h| h.uuid == uuid) {
        Some(ObjectKey::Host {
            connection_id,
            host_uuid: uuid.into(),
        })
    } else if inv.storage.iter().any(|s| s.uuid == uuid) {
        Some(ObjectKey::Storage {
            connection_id,
            sr_uuid: uuid.into(),
        })
    } else if inv.pool_id == uuid {
        Some(ObjectKey::Pool(connection_id))
    } else {
        None
    }
}

fn server_name(workspace: &WorkspaceView, id: &str) -> String {
    crate::state::find_connection(workspace, id)
        .and_then(connection_inventory)
        .map(|inv| {
            if inv.named_pool && !inv.pool_name.is_empty() {
                inv.pool_name.clone()
            } else {
                inv.hosts
                    .first()
                    .map(|h| h.name.clone())
                    .unwrap_or_default()
            }
        })
        .unwrap_or_default()
}

fn entries(workspace: &WorkspaceView, alerts: bool) -> Vec<Entry> {
    let mut result = Vec::new();
    let owned: HashSet<_> = workspace
        .tasks
        .iter()
        .map(|t| (t.connection_id.as_str(), t.remote_ref.as_str()))
        .collect();
    if !alerts {
        for t in &workspace.tasks {
            let running = matches!(t.status, TaskStatus::Running | TaskStatus::Unreachable);
            result.push(Entry {
                id: format!("local:{}", t.id),
                connection: t.connection_id.clone(),
                server: server_name(workspace, &t.connection_id),
                location: server_name(workspace, &t.connection_id),
                host: crate::state::find_vm(workspace, &t.connection_id, &t.vm_uuid)
                    .map(|(_, vm)| vm.host_name.clone())
                    .unwrap_or_default(),
                title: format!("{} · {}", t.action, t.object_name),
                body: t.message.clone(),
                status: match t.status {
                    TaskStatus::Running | TaskStatus::Unreachable => "In progress",
                    TaskStatus::Succeeded => "Completed",
                    TaskStatus::Failed => "Failed",
                    TaskStatus::Cancelled => "Cancelled",
                }
                .into(),
                time: t.started_at,
                progress: t.progress as f32,
                running,
                local: true,
                cancel: (t.can_cancel && t.status == TaskStatus::Running)
                    .then(|| NotificationAction::Cancel(t.id.clone())),
                dismiss: (t.status == TaskStatus::Unreachable)
                    .then(|| NotificationAction::Check(t.id.clone())),
                target: target(workspace, &t.connection_id, &t.vm_uuid),
            });
        }
    }
    for c in &workspace.connections {
        let Some(inv) = connection_inventory(c) else {
            continue;
        };
        let id = connection_id(c);
        let name = server_name(workspace, id);
        if alerts {
            for a in &inv.alerts {
                result.push(Entry {
                    id: format!("alert:{id}:{}", a.uuid),
                    connection: id.into(),
                    server: name.clone(),
                    location: name.clone(),
                    host: inv
                        .hosts
                        .iter()
                        .find(|h| h.uuid == a.obj_uuid)
                        .map(|h| h.name.clone())
                        .or_else(|| {
                            inv.vms
                                .iter()
                                .find(|v| v.uuid == a.obj_uuid)
                                .map(|v| v.host_name.clone())
                        })
                        .unwrap_or_default(),
                    title: super::alert_titles::title(&a.name).into(),
                    body: a.body.clone(),
                    status: severity(a.priority).into(),
                    time: a.time,
                    progress: 0.0,
                    running: false,
                    local: false,
                    cancel: None,
                    dismiss: (!is_read_only(c)).then(|| NotificationAction::Dismiss {
                        connection_id: id.into(),
                        message_uuid: a.uuid.clone(),
                    }),
                    target: target(workspace, id, &a.obj_uuid),
                });
            }
        } else {
            for t in &inv.tasks {
                if owned.contains(&(id, t.reference.as_str())) {
                    continue;
                }
                let running = t.status == "pending";
                result.push(Entry {
                    id: format!("remote:{id}:{}", t.uuid),
                    connection: id.into(),
                    server: name.clone(),
                    location: name.clone(),
                    host: t.host_name.clone(),
                    title: t.name.clone(),
                    body: if t.error.is_empty() {
                        t.task_type.clone()
                    } else {
                        t.error.clone()
                    },
                    status: match t.status.as_str() {
                        "pending" => "In progress",
                        "success" => "Completed",
                        "failure" => "Failed",
                        "cancelled" => "Cancelled",
                        other => other,
                    }
                    .into(),
                    time: t.started_at,
                    progress: t.progress as f32,
                    running,
                    local: false,
                    cancel: (running && t.can_cancel && !is_read_only(c)).then(|| {
                        NotificationAction::CancelExternal {
                            connection_id: id.into(),
                            reference: t.reference.clone(),
                        }
                    }),
                    dismiss: None,
                    target: Some(ObjectKey::Pool(id.into())),
                });
            }
        }
    }
    result.sort_by_key(|e| std::cmp::Reverse(e.time));
    result
}

pub fn running(workspace: &WorkspaceView) -> Option<(String, f32)> {
    entries(workspace, false)
        .into_iter()
        .find(|e| e.running)
        .map(|e| (e.title, e.progress))
}

pub fn counts(state: &UiState, workspace: &WorkspaceView) -> (usize, usize) {
    let count = |alerts| {
        entries(workspace, alerts)
            .iter()
            .filter(|e| !state.notifications.dismissed.contains(&e.id))
            .count()
    };
    (count(true), count(false))
}

pub fn open(state: &mut UiState, alerts: bool, actions: &mut Vec<UiAction>) {
    state.navigation_mode = super::sidebar::NavigationMode::Notifications;
    state.notifications.alerts = alerts;
    actions.push(UiAction::OpenObject {
        key: ObjectKey::Tasks,
        title: if alerts { "Alerts" } else { "Events" }.into(),
    });
}

fn matches(e: &Entry, n: &Notifications, alerts: bool) -> bool {
    let status = if alerts { &n.severity } else { &n.status };
    (status.is_empty() || status == &e.status)
        && (n.server.is_empty()
            || n.server == e.connection
            || n.server
                .split_once('\n')
                .is_some_and(|(connection, host)| connection == e.connection && host == e.host))
        && (n.days == 0
            || e.time >= (chrono::Utc::now().timestamp() as u64).saturating_sub(n.days * 86400))
}

fn dismiss(e: &Entry, n: &mut Notifications, alerts: bool, actions: &mut Vec<UiAction>) {
    let options = super::utilities::load_options();
    if if alerts {
        options.confirm_alerts
    } else {
        options.confirm_events
    } {
        n.pending_dismiss.push((e.clone(), alerts));
    } else {
        dismiss_now(e, n, alerts, actions);
    }
}
fn dismiss_now(e: &Entry, n: &mut Notifications, alerts: bool, actions: &mut Vec<UiAction>) {
    if alerts {
        if let Some(action) = &e.dismiss {
            actions.push(action.action());
        }
    } else if !e.running {
        n.dismissed.insert(e.id.clone());
    }
}

pub fn show(
    ui: &mut Ui,
    state: &mut UiState,
    workspace: &WorkspaceView,
    actions: &mut Vec<UiAction>,
) {
    let n = &mut state.notifications;
    if !n.pending_dismiss.is_empty() {
        let response = egui::Modal::new(egui::Id::unique("dismiss-confirm")).show(ui.ctx(), |ui| {
            ui.heading("Confirm Dismiss");
            ui.label(format!(
                "Dismiss {} selected notification(s)?",
                n.pending_dismiss.len()
            ));
            if ui.button("Dismiss").clicked() {
                return Some(true);
            }
            if ui.button("Cancel").clicked() {
                return Some(false);
            }
            None
        });
        if let Some(confirm) = response.inner {
            let pending = std::mem::take(&mut n.pending_dismiss);
            if confirm {
                for (entry, alerts) in pending {
                    dismiss_now(&entry, n, alerts, actions);
                }
            }
        } else if response.should_close() {
            n.pending_dismiss.clear();
        }
    }
    let alerts = n.alerts;
    let all = entries(workspace, alerts);
    let visible: Vec<_> = all
        .iter()
        .filter(|e| !n.dismissed.contains(&e.id) && matches(e, n, alerts))
        .collect();
    ui.heading(if alerts { "Alerts" } else { "Events" });
    ui.horizontal_wrapped(|ui| {
        ui.menu_button(
            if alerts {
                "Filter by Severity ▾"
            } else {
                "Filter by Status ▾"
            },
            |ui| {
                super::shell::compact_menu(ui);
                let filter = if alerts {
                    &mut n.severity
                } else {
                    &mut n.status
                };
                ui.selectable_value(filter, String::new(), "All");
                for label in if alerts {
                    &["Critical", "Major", "Warning", "Minor", "Information"][..]
                } else {
                    &["In progress", "Completed", "Failed", "Cancelled"][..]
                } {
                    ui.selectable_value(filter, (*label).into(), *label);
                }
            },
        );
        ui.menu_button("Filter by Server ▾", |ui| {
            super::shell::compact_menu(ui);
            ui.selectable_value(&mut n.server, String::new(), "All Servers");
            for c in &workspace.connections {
                ui.selectable_value(
                    &mut n.server,
                    connection_id(c).into(),
                    server_name(workspace, connection_id(c)),
                );
                if let Some(inv) = connection_inventory(c) {
                    for host in &inv.hosts {
                        ui.selectable_value(
                            &mut n.server,
                            format!("{}\n{}", connection_id(c), host.name),
                            &host.name,
                        );
                    }
                }
            }
        });
        ui.menu_button("Filter by Date ▾", |ui| {
            super::shell::compact_menu(ui);
            for (days, label) in [
                (0, "All Dates"),
                (3, "Last 3 days"),
                (7, "Last 7 days"),
                (30, "Last 30 days"),
            ] {
                ui.selectable_value(&mut n.days, days, label);
            }
        });
        if alerts {
            if ui.button("Refresh").clicked() {
                actions.push(UiAction::Refresh(None));
            }
            if ui
                .add_enabled(!all.is_empty(), egui::Button::new("Export"))
                .clicked()
            {
                n.export = Some("alerts.csv".into());
                n.overwrite = false;
                n.export_error.clear();
            }
        }
        ui.menu_button("Dismiss ▾", |ui| {
            super::shell::compact_menu(ui);
            let eligible = |e: &&Entry| !e.running && (!alerts || e.dismiss.is_some());
            if ui
                .add_enabled(
                    visible.iter().any(eligible),
                    egui::Button::new("Dismiss All"),
                )
                .clicked()
            {
                for e in &visible {
                    dismiss(e, n, alerts, actions);
                }
                ui.close();
            }
            if ui
                .add_enabled(
                    visible
                        .iter()
                        .any(|e| eligible(e) && n.selected.contains(&e.id)),
                    egui::Button::new("Dismiss Selected"),
                )
                .clicked()
            {
                for e in &visible {
                    if n.selected.contains(&e.id) {
                        dismiss(e, n, alerts, actions);
                    }
                }
                ui.close();
            }
        });
    });
    let filter = if alerts { &n.severity } else { &n.status };
    if !filter.is_empty() || !n.server.is_empty() || n.days != 0 {
        ui.label(format!(
            "{} · {} · {}",
            if filter.is_empty() { "All" } else { filter },
            if n.server.is_empty() {
                "All Servers"
            } else {
                "Selected Server"
            },
            if n.days == 0 {
                "All Dates".into()
            } else {
                format!("Last {} days", n.days)
            }
        ));
    }
    ui.separator();
    if visible.len() > 1000 {
        ui.label("Showing first 1000 entries");
    }
    ui.scope(|ui| {
        super::shell::compact_menu(ui);
        ui.spacing_mut().item_spacing.y = 0.0;
        let message_width =
            (ui.available_width() - 368.0 - ui.spacing().item_spacing.x * 5.0).max(1.0);
        let body_height = |e: &Entry| {
            ui.fonts_mut(|fonts| {
                fonts.layout(
                    e.body.clone(),
                    egui::FontId::proportional(11.0),
                    crate::theme::fg(),
                    message_width,
                )
            })
            .size()
            .y
        };
        let heights: Vec<_> = visible
            .iter()
            .take(1000)
            .map(|e| {
                if n.expanded.contains(&e.id) {
                    24.0 + body_height(e) + if e.running { 24.0 } else { 0.0 }
                } else {
                    24.0
                }
            })
            .collect();
        use egui_extras::{Column, TableBuilder};
        TableBuilder::new(ui)
            .id_salt("notifications-table")
            .striped(true)
            .cell_layout(egui::Layout::left_to_right(egui::Align::Center))
            .column(Column::exact(20.0))
            .column(Column::exact(18.0))
            .column(Column::remainder().clip(true))
            .column(Column::exact(130.0).clip(true))
            .column(Column::exact(125.0))
            .column(Column::exact(75.0))
            .header(24.0, |mut row| {
                for text in ["", "", "Message", "Server / Pool", "Date", "Actions"] {
                    row.col(|ui| {
                        ui.strong(text);
                    });
                }
            })
            .body(|mut body| {
                for (e, height) in visible.iter().take(1000).zip(heights) {
                    body.row(height, |mut row| {
                        row.col(|ui| {
                            let expanded = n.expanded.contains(&e.id);
                            if ui.small_button(if expanded { "▾" } else { "▸" }).clicked() {
                                if !expanded {
                                    n.expanded.insert(e.id.clone());
                                } else {
                                    n.expanded.remove(&e.id);
                                }
                            }
                        });
                        row.col(|ui| {
                            let color = match e.status.as_str() {
                                "Critical" | "Major" | "Failed" => crate::theme::red(),
                                "Completed" => crate::theme::green(),
                                "In progress" | "Information" => crate::theme::accent(),
                                "Warning" | "Minor" => crate::theme::amber(),
                                _ => crate::theme::muted_fg(),
                            };
                            ui.label(
                                RichText::new(if alerts {
                                    match e.status.as_str() {
                                        "Critical" | "Major" => "⛔",
                                        "Information" => "ℹ",
                                        _ => "⚠",
                                    }
                                } else {
                                    "●"
                                })
                                .color(color),
                            )
                            .on_hover_text(&e.status);
                        });
                        row.col(|ui| {
                            ui.vertical(|ui| {
                                if ui
                                    .add(
                                        egui::Button::selectable(
                                            n.selected.contains(&e.id),
                                            RichText::new(&e.title).strong(),
                                        )
                                        .truncate(),
                                    )
                                    .clicked()
                                {
                                    let selected = n.selected.contains(&e.id);
                                    if !ui.input(|i| i.modifiers.ctrl || i.modifiers.command) {
                                        n.selected.clear();
                                    }
                                    if selected {
                                        n.selected.remove(&e.id);
                                    } else {
                                        n.selected.insert(e.id.clone());
                                    }
                                }
                                if n.expanded.contains(&e.id) {
                                    ui.add(egui::Label::new(&e.body).wrap());
                                }
                                if e.running && n.expanded.contains(&e.id) {
                                    ui.add(egui::ProgressBar::new(e.progress).show_percentage());
                                }
                            });
                        });
                        row.col(|ui| {
                            ui.add(egui::Label::new(&e.location).truncate())
                                .on_hover_text(&e.server);
                        });
                        row.col(|ui| {
                            ui.label(date(e.time));
                        });
                        row.col(|ui| {
                            ui.push_id(&e.id, |ui| {
                                ui.menu_button("Actions ▾", |ui| {
                                    super::shell::compact_menu(ui);
                                    if alerts && ui.button("Help").clicked() {
                                        n.help = Some((e.title.clone(), e.body.clone()));
                                        ui.close();
                                    }
                                    if let Some(action) = &e.cancel
                                        && ui.button("Cancel").clicked()
                                    {
                                        actions.push(action.action());
                                        ui.close();
                                    }
                                    if e.local
                                        && e.running
                                        && let Some(action) = &e.dismiss
                                        && ui.button("Check again").clicked()
                                    {
                                        actions.push(action.action());
                                        ui.close();
                                    }
                                    if ui
                                        .add_enabled(
                                            !e.running && (!alerts || e.dismiss.is_some()),
                                            egui::Button::new("Dismiss"),
                                        )
                                        .clicked()
                                    {
                                        dismiss(e, n, alerts, actions);
                                        ui.close();
                                    }
                                    if ui
                                        .add_enabled(e.target.is_some(), egui::Button::new("Go To"))
                                        .clicked()
                                    {
                                        if let Some(key) = &e.target {
                                            actions.push(UiAction::OpenObject {
                                                key: key.clone(),
                                                title: e.title.clone(),
                                            });
                                        }
                                        ui.close();
                                    }
                                });
                            });
                        });
                    });
                }
            });
    });
    if visible.is_empty() {
        ui.label(if alerts {
            "No alerts to display."
        } else {
            "No events to display."
        });
    }
    if let Some((title, body)) = &n.help {
        let mut open = true;
        egui::Window::new("Alert Help").open(&mut open).show(ui.ctx(), |ui| { ui.heading(title); ui.label(body); ui.label("Select Go To to inspect the affected object. Dismiss removes this alert from the server."); });
        if !open {
            n.help = None;
        }
    }
    if let Some(path) = &mut n.export {
        let mut close = false;
        egui::Window::new("Export Alerts")
            .collapsible(false)
            .show(ui.ctx(), |ui| {
                ui.label("CSV file");
                ui.text_edit_singleline(path);
                ui.checkbox(&mut n.export_filtered, "Export filtered alerts only");
                if std::path::Path::new(path).exists() {
                    ui.checkbox(&mut n.overwrite, "Replace existing file");
                }
                ui.label(&n.export_error);
                ui.horizontal(|ui| {
                    if ui.button("Save").clicked() {
                        let rows: Vec<_> = all
                            .iter()
                            .filter(|e| !n.export_filtered || visible.iter().any(|v| v.id == e.id))
                            .collect();
                        let data = csv(&rows);
                        use std::io::Write;
                        let result = std::fs::OpenOptions::new()
                            .write(true)
                            .create(true)
                            .truncate(n.overwrite)
                            .create_new(!n.overwrite)
                            .open(path.as_str())
                            .and_then(|mut f| f.write_all(data.as_bytes()));
                        match result {
                            Ok(()) => close = true,
                            Err(e) => n.export_error = e.to_string(),
                        }
                    }
                    if ui.button("Cancel").clicked() {
                        close = true;
                    }
                });
            });
        if close {
            n.export = None;
        }
    }
}

fn date(time: u64) -> String {
    if super::utilities::load_options().timestamps {
        crate::format::timestamp(time)
    } else {
        String::new()
    }
}

fn csv(entries: &[&Entry]) -> String {
    let mut result = "Title,Severity,Description,Applies To,Timestamp\r\n".to_owned();
    for e in entries {
        let date = date(e.time);
        let row = [&e.title, &e.status, &e.body, &e.location, &date]
            .map(|s| format!("\"{}\"", s.replace('"', "\"\"")));
        result.push_str(&row.join(","));
        result.push_str("\r\n");
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    fn workspace() -> WorkspaceView {
        WorkspaceView {
            connections: vec![],
            tasks: vec![xen_core::workspace::Operation {
                id: "one".into(),
                connection_id: "lab".into(),
                connection_name: "Lab".into(),
                vm_uuid: "vm".into(),
                remote_ref: "task".into(),
                object_name: "VM".into(),
                action: "Snapshot".into(),
                status: TaskStatus::Running,
                progress: 0.5,
                message: "Details, with \"quotes\"\nand a newline".into(),
                can_cancel: true,
                started_at: chrono::Utc::now().timestamp() as u64,
            }],
        }
    }
    #[test]
    fn events_filter_and_dismiss_preserve_running_work() {
        let mut workspace = workspace();
        let mut n = Notifications {
            status: "Failed".into(),
            ..Default::default()
        };
        let rows = entries(&workspace, false);
        assert!(!matches(&rows[0], &n, false));
        let mut actions = vec![];
        dismiss_now(&rows[0], &mut n, false, &mut actions);
        assert!(n.dismissed.is_empty());
        assert!(rows[0].cancel.is_some());
        workspace.tasks[0].status = TaskStatus::Failed;
        let rows = entries(&workspace, false);
        assert!(matches(&rows[0], &n, false));
        assert!(rows[0].cancel.is_none());
        dismiss_now(&rows[0], &mut n, false, &mut actions);
        assert!(n.dismissed.contains(&rows[0].id));
        n.server = "another-server".into();
        assert!(!matches(&rows[0], &n, false));
    }
    #[test]
    fn alert_csv_quotes_multiline_bodies_and_uses_upstream_severities() {
        let rows = entries(&workspace(), false);
        let exported = csv(&[&rows[0]]);
        assert!(exported.starts_with("Title,Severity,Description,Applies To,Timestamp\r\n"));
        assert!(exported.contains("\"Details, with \"\"quotes\"\"\nand a newline\""));
        assert_eq!(severity(3), "Warning");
        assert_eq!(severity(4), "Minor");
        assert_eq!(severity(5), "Information");
    }
}
