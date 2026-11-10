use std::path::PathBuf;

use serde::{Deserialize, Serialize};

pub fn config_dir() -> PathBuf {
    if let Some(dir) = std::env::var_os("XDG_CONFIG_HOME").filter(|value| !value.is_empty()) {
        return PathBuf::from(dir).join("xen-desktop");
    }
    #[cfg(windows)]
    if let Some(dir) = std::env::var_os("APPDATA") {
        return PathBuf::from(dir).join("xen-desktop");
    }
    #[cfg(target_os = "macos")]
    if let Some(home) = std::env::var_os("HOME") {
        return PathBuf::from(home).join("Library/Application Support/xen-desktop");
    }
    let home = std::env::var_os("HOME").map_or_else(|| PathBuf::from("."), PathBuf::from);
    home.join(".config/xen-desktop")
}

pub fn profiles_path() -> PathBuf {
    config_dir().join("connections.json")
}

pub fn ui_path() -> PathBuf {
    config_dir().join("ui.json")
}

#[derive(Deserialize, Serialize)]
struct UiPreferences {
    #[serde(default = "default_light")]
    light: bool,
}

fn default_light() -> bool {
    true
}

impl Default for UiPreferences {
    fn default() -> Self {
        Self {
            light: default_light(),
        }
    }
}

/// The white theme is the default; a saved preference wins on later launches.
pub fn load_light_preference() -> bool {
    std::fs::read(ui_path())
        .ok()
        .and_then(|bytes| serde_json::from_slice::<UiPreferences>(&bytes).ok())
        .unwrap_or_default()
        .light
}

pub fn save_light_preference(light: bool) {
    let path = ui_path();
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let Ok(bytes) = serde_json::to_vec_pretty(&UiPreferences { light }) else {
        return;
    };
    let temporary = path.with_extension("tmp");
    if std::fs::write(&temporary, bytes).is_ok() {
        let _ = std::fs::rename(temporary, path);
    }
}
