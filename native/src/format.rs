use chrono::{Local, TimeZone};

/// Human readable binary size, matching the previous desktop client.
pub fn bytes(value: u64) -> String {
    if value == 0 {
        return "0 B".into();
    }
    const UNITS: [&str; 5] = ["B", "KiB", "MiB", "GiB", "TiB"];
    let mut size = value as f64;
    let mut unit = 0;
    while size >= 1024.0 && unit < UNITS.len() - 1 {
        size /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{value} B")
    } else {
        format!("{size:.1} {}", UNITS[unit])
    }
}

pub fn percent(used: u64, total: u64) -> f32 {
    if total == 0 {
        0.0
    } else {
        (used as f64 / total as f64 * 100.0).clamp(0.0, 100.0) as f32
    }
}

pub fn timestamp(seconds: u64) -> String {
    Local
        .timestamp_opt(seconds as i64, 0)
        .single()
        .map(|value| value.format("%Y-%m-%d %H:%M:%S").to_string())
        .unwrap_or_else(|| "—".into())
}

pub fn short_time(seconds: u64) -> String {
    Local
        .timestamp_opt(seconds as i64, 0)
        .single()
        .map(|value| value.format("%H:%M:%S").to_string())
        .unwrap_or_else(|| "—".into())
}

pub fn clock(seconds: u64) -> String {
    Local
        .timestamp_opt(seconds as i64, 0)
        .single()
        .map(|value| value.format("%H:%M").to_string())
        .unwrap_or_else(|| "—".into())
}

pub fn plural(count: usize, singular: &str, plural: &str) -> String {
    format!("{count} {}", if count == 1 { singular } else { plural })
}

pub fn truncate(value: &str, max: usize) -> String {
    let count = value.chars().count();
    if count <= max {
        return value.to_owned();
    }
    let mut out: String = value.chars().take(max.saturating_sub(1)).collect();
    out.push('…');
    out
}

/// XenAPI date strings denote UTC, including compact ISO-8601 values.
pub fn xapi_time(value: &str) -> String {
    xen_core::performance::parse_server_epoch(&serde_json::Value::String(value.into()))
        .map(timestamp)
        .unwrap_or_else(|| value.into())
}
