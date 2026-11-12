//! Tools and Help dialogs.
use crate::{config, state::UiState};
use serde::{Deserialize, Serialize};

#[derive(Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Options {
    pub timestamps: bool,
    pub graph_lines: bool,
    pub clipboard: bool,
    pub scale: bool,
    pub windows_key: bool,
    pub confirm_alerts: bool,
    pub confirm_events: bool,
    pub proxy: String,
    pub timeout: u64,
    pub save_servers: bool,
    pub rdp_program: String,
}
impl Default for Options {
    fn default() -> Self {
        Self {
            timestamps: true,
            graph_lines: true,
            clipboard: true,
            scale: true,
            windows_key: true,
            confirm_alerts: true,
            confirm_events: true,
            proxy: String::new(),
            timeout: 45,
            save_servers: true,
            rdp_program: "xfreerdp".into(),
        }
    }
}
pub fn load_options() -> Options {
    std::fs::read(config::config_dir().join("options.json"))
        .ok()
        .and_then(|v| serde_json::from_slice(&v).ok())
        .unwrap_or_default()
}
pub struct UtilityDialog {
    command: &'static str,
    page: &'static str,
    options: Options,
    message: String,
    update: Option<std::sync::mpsc::Receiver<Result<(String, String), String>>>,
    release_url: Option<String>,
}
impl UtilityDialog {
    pub fn new(command: &'static str) -> Self {
        let mut result = Self {
            command,
            page: "Display",
            options: load_options(),
            message: String::new(),
            update: None,
            release_url: None,
        };
        if command == "CheckforXenDesktopUpdates" {
            let (send, receive) = std::sync::mpsc::channel();
            result.update = Some(receive);
            result.message = "Checking GitHub releases…".into();
            std::thread::spawn(move || {
                let value = tokio::runtime::Runtime::new()
                    .map_err(|e| e.to_string())
                    .and_then(|runtime| {
                        runtime
                            .block_on(xen_core::updates::latest_release(env!("CARGO_PKG_VERSION")))
                    });
                let _ = send.send(value);
            });
        }
        result
    }
}
pub fn show(ctx: &egui::Context, state: &mut UiState) {
    let Some(dialog) = state.utility.as_mut() else {
        return;
    };
    if let Some(receiver) = &dialog.update
        && let Ok(result) = receiver.try_recv()
    {
        match result {
            Ok((message, url)) => {
                dialog.message = message;
                dialog.release_url = Some(url);
            }
            Err(error) => dialog.message = format!("Update check failed: {error}"),
        }
        dialog.update = None;
    }
    if dialog.update.is_some() {
        ctx.request_repaint_after(std::time::Duration::from_millis(200));
    }
    let title = match dialog.command {
        "Options" => "Options",
        "About" => "About Xen Desktop",
        "CheckforXenDesktopUpdates" => "Check for Updates",
        _ => "Help",
    };
    let mut open = true;
    let mut close = false;
    egui::Window::new(title).open(&mut open).default_width(710.0).collapsible(false).show(ctx, |ui| {
        match dialog.command {
            "Options" => {
                ui.allocate_ui_with_layout(egui::vec2(ui.available_width(), 330.0), egui::Layout::left_to_right(egui::Align::Min), |ui| {
                    ui.vertical(|ui| {
                        ui.set_min_width(165.0);
                        for page in ["Security", "Display", "Confirmations", "Consoles", "Connection", "Save and Restore", "External Tools"] {
                            if ui.selectable_label(dialog.page == page, page).clicked() { dialog.page = page; }
                        }
                    });
                    ui.separator();
                    ui.vertical(|ui| {
                        ui.set_min_size(egui::vec2(460.0, 300.0));
                        ui.heading(dialog.page);
                        ui.separator();
                        match dialog.page {
                            "Security" => { ui.add_enabled(false, egui::Checkbox::new(&mut true, "Check server certificates")); ui.label("Certificate verification is configured per connection. Certificate management is excluded from this port."); }
                            "Display" => {
                                ui.checkbox(&mut dialog.options.timestamps, "Show timestamps in notifications");
                                ui.checkbox(&mut dialog.options.graph_lines, "Use line graphs (uncheck for filled graphs)");
                            }
                            "Consoles" => {
                                ui.checkbox(&mut dialog.options.windows_key, "Send Windows key to the console");
                                ui.checkbox(&mut dialog.options.clipboard, "Enable clipboard transfer");
                                ui.checkbox(&mut dialog.options.scale, "Scale console to fit");
                            }
                            "Confirmations" => { ui.checkbox(&mut dialog.options.confirm_alerts, "Confirm before dismissing alerts");
                                ui.checkbox(&mut dialog.options.confirm_events, "Confirm before dismissing events"); }
                            "Connection" => { ui.label("Defaults for new connections; reconnect to apply timeout changes.");
                                ui.label("Proxy URL (empty uses system settings)");
                                ui.text_edit_singleline(&mut dialog.options.proxy);
                                ui.horizontal(|ui| { ui.label("Request timeout (seconds)"); ui.add(egui::DragValue::new(&mut dialog.options.timeout).range(5..=600)); }); }
                            "Save and Restore" => {
                                ui.checkbox(&mut dialog.options.save_servers, "Save server list on exit and connection changes");
                                ui.label("Server lists can also be imported and exported from the File menu.");
                                ui.add_enabled(false, egui::Button::new("Set master password…")).on_disabled_hover_text("Encrypted credential storage is not available.");
                            }
                            "External Tools" => { ui.label("SSH uses the embedded console.");
                                ui.label("External RDP executable"); ui.text_edit_singleline(&mut dialog.options.rdp_program);
                                ui.label("Choose executable names or paths. Arguments are passed directly without a shell."); }
                            _ => {}
                        }
                    });
                });
                ui.separator();
                ui.horizontal(|ui| {
                    if ui.button("Cancel").clicked() { close = true; }
                    if ui.button("OK").clicked() {
                        let path = config::config_dir().join("options.json");
                        let result = std::fs::create_dir_all(config::config_dir()).and_then(|()| std::fs::write(path, serde_json::to_vec_pretty(&dialog.options)?));
                        match result { Ok(()) => close = true, Err(error) => dialog.message = format!("Could not save options: {error}") }
                    }
                });
            }
            "About" => {
                ui.heading("Xen Desktop");
                ui.label(format!("Version {}", env!("CARGO_PKG_VERSION")));
                ui.label(format!("Commit: {}", option_env!("XEN_BUILD_COMMIT").unwrap_or("not embedded in this build")));
                ui.label("BSD 2-Clause License");
                ui.label("Based on XCP-ng Center and Citrix XenCenter. Copyright Citrix Systems, Inc. and contributors. Xen Desktop contributors.");
                ui.hyperlink_to("Source, licence and credits", "https://github.com/ISWORKNEXT/xenadmin");
            }
            "CheckforXenDesktopUpdates" => { if let Some(url) = &dialog.release_url { ui.hyperlink_to("Open release page", url); } }
            "ViewXenDesktopLogFiles" => {
                let folder = config::config_dir().join("logs");
                ui.label(folder.display().to_string());
                if ui.button("Open log folder").clicked() {
                    #[cfg(target_os = "linux")]
                    let result = std::process::Command::new("xdg-open").arg(&folder).spawn();
                    #[cfg(target_os = "macos")]
                    let result = std::process::Command::new("open").arg(&folder).spawn();
                    #[cfg(windows)]
                    let result = std::process::Command::new("explorer").arg(&folder).spawn();
                    if let Err(error) = result { dialog.message = format!("Could not open log folder: {error}"); }
                }
            }
            "HelpTopics" => { ui.hyperlink_to("Help Topics — Xen Desktop README", "https://github.com/ISWORKNEXT/xenadmin#readme"); }
            "XenDesktopontheWeb" => { ui.hyperlink_to("Xen Desktop on the Web", "https://github.com/ISWORKNEXT/xenadmin"); }
            _ => {}
        }
        if !dialog.message.is_empty() { ui.label(&dialog.message); }
        if dialog.command != "Options" && ui.button("Close").clicked() { close = true; }
    });
    if !open || close {
        state.options = load_options();
        state.utility = None;
    }
}
