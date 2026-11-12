#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod config;
mod console;
mod format;
mod rdp;
mod search;
mod ssh;
mod state;
mod theme;
mod views;
mod widgets;
mod worker;

use app::App;
use serde::Deserialize;
use xen_core::xapi::ConnectionSettings;

#[derive(Deserialize)]
pub(crate) struct Startup {
    settings: ConnectionSettings,
    password: String,
}

fn argument(flag: &str) -> Option<String> {
    let args: Vec<String> = std::env::args().collect();
    args.iter()
        .position(|arg| arg == flag)
        .and_then(|index| args.get(index + 1))
        .cloned()
}

fn startup_entries() -> Vec<Startup> {
    if !std::env::args().any(|arg| arg == "--connect-stdin") {
        return Vec::new();
    }
    use std::io::BufRead;
    let mut line = String::new();
    let _ = std::io::stdin().lock().read_line(&mut line);
    let parsed = serde_json::from_str::<Vec<Startup>>(&line)
        .or_else(|_| serde_json::from_str::<Startup>(&line).map(|entry| vec![entry]));
    drop(line);
    parsed.unwrap_or_else(|_| {
        eprintln!("Invalid startup connection input.");
        Vec::new()
    })
}

fn main() -> eframe::Result {
    let log_folder = config::config_dir().join("logs");
    let _ = std::fs::create_dir_all(&log_folder);
    let log_path = log_folder.join("xen-desktop.log");
    use std::io::Write;
    if let Ok(mut file) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_path)
    {
        let _ = writeln!(
            file,
            "{} Xen Desktop {} starting",
            chrono::Utc::now().to_rfc3339(),
            env!("CARGO_PKG_VERSION")
        );
    }
    std::panic::set_hook(Box::new(move |info| {
        // Panic payloads may contain host data; record only source location.
        if let Ok(mut file) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&log_path)
        {
            let _ = writeln!(
                file,
                "{} panic at {:?}",
                chrono::Utc::now().to_rfc3339(),
                info.location()
            );
        }
    }));
    let startup = startup_entries();
    let open_connect = std::env::args().any(|arg| arg == "--connect");
    let startup_view = argument("--view").and_then(|value| state::View::from_key(&value));
    let startup_host = argument("--host");
    let startup_vm = argument("--vm");
    let startup_health = std::env::args().any(|arg| arg == "--health");
    let startup_auth = std::env::args().any(|arg| arg == "--auth");
    let startup_pool_status = std::env::args().any(|arg| arg == "--pool-status");
    let startup_sr = argument("--sr");
    let startup_network = argument("--network");
    let startup_search = argument("--search");
    let startup_group_by =
        argument("--group-by").and_then(|value| config::GroupBy::from_key(&value));
    let startup_saved_search = argument("--saved-search");
    let startup_vm_tab = argument("--vm-tab").and_then(|value| state::VmTab::from_key(&value));
    let startup_light =
        argument("--theme").and_then(|value| match value.to_ascii_lowercase().as_str() {
            "light" => Some(true),
            "dark" => Some(false),
            _ => None,
        });
    let startup_host_tab =
        argument("--host-tab").and_then(|value| state::HostTab::from_key(&value));
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1440.0, 940.0])
            .with_min_inner_size([980.0, 660.0])
            .with_decorations(true)
            .with_title("Xen Desktop"),
        ..Default::default()
    };
    eframe::run_native(
        "Xen Desktop",
        options,
        Box::new(move |cc| {
            Ok(Box::new(App::new(
                cc,
                app::StartupOptions {
                    startup,
                    open_connect,
                    view: startup_view,
                    host: startup_host,
                    host_tab: startup_host_tab,
                    vm: startup_vm,
                    vm_tab: startup_vm_tab,
                    health: startup_health,
                    auth: startup_auth,
                    pool_status: startup_pool_status,
                    sr: startup_sr,
                    network: startup_network,
                    search: startup_search,
                    group_by: startup_group_by,
                    saved_search: startup_saved_search,
                    light: startup_light,
                },
            )))
        }),
    )
}
