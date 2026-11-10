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

#[derive(Clone, Deserialize, Serialize)]
pub struct UiPreferences {
    #[serde(default = "default_light")]
    pub light: bool,
    #[serde(default = "default_zoom")]
    pub zoom: f32,
}

fn default_light() -> bool {
    true
}

pub fn default_zoom() -> f32 {
    1.1
}

impl Default for UiPreferences {
    fn default() -> Self {
        Self {
            light: default_light(),
            zoom: default_zoom(),
        }
    }
}

impl UiPreferences {
    pub fn normalized(mut self) -> Self {
        self.zoom = if self.zoom.is_finite() {
            self.zoom.clamp(0.8, 1.6)
        } else {
            default_zoom()
        };
        self
    }
}

/// The white theme is the default; a saved preference wins on later launches.
pub fn load_ui_preferences() -> UiPreferences {
    std::fs::read(ui_path())
        .ok()
        .and_then(|bytes| serde_json::from_slice::<UiPreferences>(&bytes).ok())
        .unwrap_or_default()
        .normalized()
}

pub fn save_ui_preferences(preferences: &UiPreferences) {
    let path = ui_path();
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let Ok(bytes) = serde_json::to_vec_pretty(preferences) else {
        return;
    };
    let temporary = path.with_extension("tmp");
    if std::fs::write(&temporary, bytes).is_ok() {
        let _ = std::fs::rename(temporary, path);
    }
}
