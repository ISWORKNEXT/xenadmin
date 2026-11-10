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

pub fn saved_searches_path() -> PathBuf {
    config_dir().join("saved_searches.json")
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GroupBy {
    #[default]
    None,
    Connection,
    Host,
    PowerState,
    Tag,
}

impl GroupBy {
    pub const ALL: [GroupBy; 5] = [
        GroupBy::None,
        GroupBy::Connection,
        GroupBy::Host,
        GroupBy::PowerState,
        GroupBy::Tag,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::None => "No grouping",
            Self::Connection => "Connection",
            Self::Host => "Host",
            Self::PowerState => "Power state",
            Self::Tag => "Tag",
        }
    }

    pub fn from_key(key: &str) -> Option<Self> {
        match key.to_ascii_lowercase().replace([' ', '-'], "_").as_str() {
            "none" | "off" | "no_grouping" => Some(Self::None),
            "connection" | "conn" => Some(Self::Connection),
            "host" => Some(Self::Host),
            "power" | "power_state" | "state" => Some(Self::PowerState),
            "tag" => Some(Self::Tag),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SavedSearch {
    pub name: String,
    pub query: String,
    #[serde(default)]
    pub group_by: GroupBy,
}

pub fn load_saved_searches() -> Vec<SavedSearch> {
    read_saved_searches(&saved_searches_path())
}

pub fn save_saved_searches(searches: &[SavedSearch]) {
    write_saved_searches(&saved_searches_path(), searches);
}

fn read_saved_searches(path: &std::path::Path) -> Vec<SavedSearch> {
    std::fs::read(path)
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .unwrap_or_default()
}

fn write_saved_searches(path: &std::path::Path, searches: &[SavedSearch]) {
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let Ok(bytes) = serde_json::to_vec_pretty(searches) else {
        return;
    };
    let temporary = path.with_extension("tmp");
    if std::fs::write(&temporary, bytes).is_ok() {
        let _ = std::fs::rename(temporary, path);
    }
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

#[cfg(test)]
mod saved_search_tests {
    use super::*;

    #[test]
    fn saved_searches_round_trip_and_default_grouping() {
        let path = std::env::temp_dir().join(format!(
            "xen-saved-searches-{}-{}.json",
            std::process::id(),
            std::thread::current().name().unwrap_or("test")
        ));
        let searches = vec![
            SavedSearch {
                name: "Running".into(),
                query: "power_state:running".into(),
                group_by: GroupBy::PowerState,
            },
            SavedSearch {
                name: "Templates".into(),
                query: "name:ubuntu".into(),
                group_by: GroupBy::None,
            },
        ];
        write_saved_searches(&path, &searches);
        let read = read_saved_searches(&path);
        assert_eq!(read.len(), 2);
        assert_eq!(read[0].group_by, GroupBy::PowerState);
        assert_eq!(read[1].query, "name:ubuntu");
        std::fs::write(&path, br#"[{"name":"Legacy","query":"name:web"}]"#).unwrap();
        let legacy = read_saved_searches(&path);
        assert_eq!(legacy[0].group_by, GroupBy::None);
        std::fs::write(&path, b"not json").unwrap();
        assert!(read_saved_searches(&path).is_empty());
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn group_by_keys_are_stable() {
        for group in GroupBy::ALL {
            assert_eq!(GroupBy::from_key(group.label()), Some(group));
        }
        assert_eq!(GroupBy::from_key("power"), Some(GroupBy::PowerState));
        assert_eq!(GroupBy::from_key("unknown"), None);
    }
}
