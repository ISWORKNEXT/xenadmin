//! Read-only health diagnostics computed from the cached XenAPI records.
use std::time::{SystemTime, UNIX_EPOCH};

use serde::Serialize;
use serde_json::Value;

use crate::xapi::RawInventory;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Severity {
    Ok,
    Info,
    Warning,
    Critical,
}

impl Severity {
    pub fn label(self) -> &'static str {
        match self {
            Self::Ok => "OK",
            Self::Info => "Info",
            Self::Warning => "Warning",
            Self::Critical => "Critical",
        }
    }

    pub fn rank(self) -> u8 {
        match self {
            Self::Ok => 0,
            Self::Info => 1,
            Self::Warning => 2,
            Self::Critical => 3,
        }
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct DiagnosticCheck {
    pub id: String,
    pub title: String,
    pub severity: Severity,
    pub summary: String,
    pub details: Vec<String>,
}

#[derive(Clone, Debug, Serialize)]
pub struct HealthReport {
    pub checks: Vec<DiagnosticCheck>,
    pub checked_at: u64,
}

impl HealthReport {
    pub fn worst(&self) -> Severity {
        self.checks
            .iter()
            .map(|check| check.severity)
            .max_by_key(|severity| severity.rank())
            .unwrap_or(Severity::Ok)
    }

    pub fn count(&self, severity: Severity) -> usize {
        self.checks
            .iter()
            .filter(|check| check.severity == severity)
            .count()
    }
}

pub(crate) fn evaluate(raw: &RawInventory) -> HealthReport {
    let checks = vec![
        alerts(raw),
        hosts(raw),
        storage(raw),
        pool_hosts(raw),
        ha(raw),
        vm_disks(raw),
    ];
    HealthReport {
        checks,
        checked_at: SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs(),
    }
}

fn check(
    id: &str,
    title: &str,
    severity: Severity,
    summary: impl Into<String>,
    details: Vec<String>,
) -> DiagnosticCheck {
    DiagnosticCheck {
        id: id.into(),
        title: title.into(),
        severity,
        summary: summary.into(),
        details,
    }
}

fn text(record: &Value, key: &str) -> String {
    record[key].as_str().unwrap_or_default().to_owned()
}

fn alerts(raw: &RawInventory) -> DiagnosticCheck {
    let mut details = Vec::new();
    let mut severity = Severity::Ok;
    for message in raw.messages.values() {
        let priority = message["priority"]
            .as_u64()
            .or_else(|| message["priority"].as_str().and_then(|p| p.parse().ok()))
            .unwrap_or(5);
        if priority > 3 {
            continue;
        }
        let name = text(message, "name");
        let body = text(message, "body");
        let lead = if body.is_empty() {
            name.clone()
        } else {
            format!("{name}: {}", body.chars().take(140).collect::<String>())
        };
        details.push(lead);
        severity = match (severity, priority) {
            (Severity::Critical, _) | (_, 1) => Severity::Critical,
            (_, 2 | 3) => max_severity(severity, Severity::Warning),
            _ => severity,
        };
    }
    let severity = max_severity(severity, Severity::Ok);
    check(
        "alerts",
        "Alerts",
        severity,
        if details.is_empty() {
            "No priority 1-3 alerts.".into()
        } else {
            format!("{} active priority alert(s).", details.len())
        },
        details,
    )
}

fn max_severity(left: Severity, right: Severity) -> Severity {
    if left.rank() >= right.rank() {
        left
    } else {
        right
    }
}

fn hosts(raw: &RawInventory) -> DiagnosticCheck {
    let mut details = Vec::new();
    let mut severity = Severity::Ok;
    for host in raw.hosts.values() {
        let name = text(host, "name_label");
        let enabled = host["enabled"].as_bool().unwrap_or(true);
        let power_on = host["power_on"].as_bool().unwrap_or(true);
        if !power_on {
            details.push(format!("{name}: powered off"));
            severity = Severity::Critical;
        } else if !enabled {
            details.push(format!("{name}: disabled"));
            severity = max_severity(severity, Severity::Warning);
        }
    }
    check(
        "hosts",
        "Host availability",
        severity,
        if details.is_empty() {
            "All hosts are enabled and powered on.".into()
        } else {
            format!("{} host issue(s).", details.len())
        },
        details,
    )
}

fn storage(raw: &RawInventory) -> DiagnosticCheck {
    let mut details = Vec::new();
    let mut severity = Severity::Ok;
    for sr in raw.storage.values() {
        let name = text(sr, "name_label");
        if sr["PBDs"].as_array().is_none_or(Vec::is_empty) {
            details.push(format!("{name}: no attached PBD"));
            severity = Severity::Critical;
        }
        let total = number(sr, "physical_size");
        let used = number(sr, "physical_utilisation");
        if total == 0 {
            continue;
        }
        let ratio = used as f64 / total as f64;
        if ratio >= 0.95 {
            details.push(format!("{name}: {:.0}% used", ratio * 100.0));
            severity = Severity::Critical;
        } else if ratio >= 0.85 {
            details.push(format!("{name}: {:.0}% used", ratio * 100.0));
            severity = max_severity(severity, Severity::Warning);
        }
    }
    check(
        "storage",
        "Storage capacity",
        severity,
        if details.is_empty() {
            "Storage repositories have capacity and attached PBDs.".into()
        } else {
            format!("{} storage issue(s).", details.len())
        },
        details,
    )
}

fn pool_hosts(raw: &RawInventory) -> DiagnosticCheck {
    let total = raw.hosts.len();
    let enabled = raw
        .hosts
        .values()
        .filter(|host| host["enabled"].as_bool().unwrap_or(true))
        .count();
    let (severity, summary) = if total == 0 {
        (Severity::Critical, "The pool has no hosts.".into())
    } else if enabled == 0 {
        (
            Severity::Critical,
            "No host is enabled; VMs cannot start.".into(),
        )
    } else if enabled == 1 {
        (
            Severity::Warning,
            "Single enabled host; there is no HA failover capacity.".into(),
        )
    } else {
        (
            Severity::Ok,
            format!("{enabled} enabled host(s) provide failover capacity."),
        )
    };
    check("pool-hosts", "Pool capacity", severity, summary, Vec::new())
}

fn ha(raw: &RawInventory) -> DiagnosticCheck {
    let enabled = raw
        .pools
        .values()
        .next()
        .and_then(|pool| pool["ha_enabled"].as_bool())
        .unwrap_or(false);
    if enabled {
        check(
            "ha",
            "High availability",
            Severity::Ok,
            "Pool HA is enabled.",
            Vec::new(),
        )
    } else {
        check(
            "ha",
            "High availability",
            Severity::Info,
            "Pool HA is disabled.",
            Vec::new(),
        )
    }
}

fn vm_disks(raw: &RawInventory) -> DiagnosticCheck {
    let mut details = Vec::new();
    let mut severity = Severity::Ok;
    for (vm_ref, vm) in &raw.vms {
        if vm["is_control_domain"].as_bool() == Some(true)
            || vm["is_a_template"].as_bool() == Some(true)
            || vm["is_a_snapshot"].as_bool() == Some(true)
        {
            continue;
        }
        let vm_name = text(vm, "name_label");
        for vbd in raw
            .vbds
            .values()
            .filter(|vbd| vbd["VM"].as_str() == Some(vm_ref.as_str()))
        {
            if vbd["empty"].as_bool() == Some(true) {
                continue;
            }
            let vdi_ref = vbd["VDI"].as_str().unwrap_or_default();
            let device = text(vbd, "device");
            if vdi_ref.is_empty() || vdi_ref == "OpaqueRef:NULL" {
                if text(vbd, "type") == "Disk" {
                    details.push(format!("{vm_name}: disk {device} has no VDI"));
                    severity = Severity::Critical;
                }
                continue;
            }
            let Some(vdi) = raw.vdis.get(vdi_ref) else {
                details.push(format!("{vm_name}: disk {device} points to a missing VDI"));
                severity = Severity::Critical;
                continue;
            };
            if vdi["missing"].as_bool() == Some(true) {
                details.push(format!(
                    "{vm_name}: {} is marked missing",
                    text(vdi, "name_label")
                ));
                severity = Severity::Critical;
                continue;
            }
            let sr_ref = text(vdi, "SR");
            let Some(sr) = raw.storage.get(&sr_ref) else {
                details.push(format!("{vm_name}: disk {device} SR is unavailable"));
                severity = Severity::Critical;
                continue;
            };
            if sr["PBDs"].as_array().is_none_or(Vec::is_empty) {
                details.push(format!(
                    "{vm_name}: disk {device} SR {} has no PBD",
                    text(sr, "name_label")
                ));
                severity = Severity::Critical;
            }
        }
    }
    check(
        "vm-disks",
        "VM disk references",
        severity,
        if details.is_empty() {
            "No broken VM disk or ISO references were found.".into()
        } else {
            format!("{} broken disk/ISO reference(s).", details.len())
        },
        details,
    )
}

fn number(record: &Value, key: &str) -> u64 {
    record[key]
        .as_u64()
        .or_else(|| record[key].as_str().and_then(|value| value.parse().ok()))
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn base() -> RawInventory {
        let mut raw = RawInventory::default();
        raw.hosts.insert(
            "h1".into(),
            json!({"name_label":"h1","enabled":true,"power_on":true}),
        );
        raw.hosts.insert(
            "h2".into(),
            json!({"name_label":"h2","enabled":true,"power_on":true}),
        );
        raw.pools.insert("p".into(), json!({"ha_enabled":true}));
        raw.storage.insert(
            "sr".into(),
            json!({"name_label":"sr","PBDs":["pbd"],"physical_size":1000,"physical_utilisation":100}),
        );
        raw.vms.insert(
            "vm".into(),
            json!({"name_label":"vm","is_control_domain":false,"is_a_template":false,"is_a_snapshot":false}),
        );
        raw.vbds.insert(
            "vbd".into(),
            json!({"VM":"vm","VDI":"vdi","device":"xvda","type":"Disk","empty":false}),
        );
        raw.vdis.insert(
            "vdi".into(),
            json!({"name_label":"root","SR":"sr","missing":false}),
        );
        raw
    }

    #[test]
    fn healthy_inventory_has_no_warnings() {
        let report = evaluate(&base());
        assert!(report.worst().rank() <= Severity::Info.rank());
        assert_eq!(report.count(Severity::Critical), 0);
    }

    #[test]
    fn detects_broken_disks_alerts_capacity_and_hosts() {
        let mut raw = base();
        raw.hosts.retain(|reference, _| reference == "h1");
        raw.hosts.get_mut("h1").unwrap()["enabled"] = json!(false);
        raw.storage.get_mut("sr").unwrap()["physical_utilisation"] = json!(960);
        raw.vdis.remove("vdi");
        raw.messages.insert(
            "m".into(),
            json!({"name":"FAIL","priority":1,"body":"boom"}),
        );
        let report = evaluate(&raw);
        assert_eq!(report.worst(), Severity::Critical);
        assert!(report.count(Severity::Critical) >= 3);
        assert!(report.count(Severity::Warning) >= 1);
        assert!(
            report
                .checks
                .iter()
                .any(|check| check.id == "vm-disks" && !check.details.is_empty())
        );
    }

    #[test]
    fn warns_at_eighty_five_percent_storage() {
        let mut raw = base();
        raw.storage.get_mut("sr").unwrap()["physical_utilisation"] = json!(890);
        let report = evaluate(&raw);
        let storage = report
            .checks
            .iter()
            .find(|check| check.id == "storage")
            .unwrap();
        assert_eq!(storage.severity, Severity::Warning);
        assert!(storage.details[0].contains("89%"));
    }
}
