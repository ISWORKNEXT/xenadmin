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
        network(raw),
        pool_hosts(raw),
        ha(raw),
        guest_tools(raw),
        vm_disks(raw),
        vbd_consistency(raw),
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
    let multi_host = raw
        .hosts
        .values()
        .filter(|host| host["enabled"].as_bool().unwrap_or(true))
        .count()
        > 1;
    for (sr_ref, sr) in &raw.storage {
        let name = text(sr, "name_label");
        if sr["PBDs"].as_array().is_none_or(Vec::is_empty) {
            details.push(format!("{name}: no attached PBD"));
            severity = Severity::Critical;
        }
        let related_pbds: Vec<&Value> = raw
            .pbds
            .values()
            .filter(|pbd| pbd["SR"].as_str() == Some(sr_ref.as_str()))
            .collect();
        if !related_pbds.is_empty() {
            let attached = related_pbds
                .iter()
                .filter(|pbd| pbd["currently_attached"].as_bool() != Some(false))
                .count();
            if attached == 0 {
                details.push(format!("{name}: no PBD is attached"));
                severity = Severity::Critical;
            } else if attached < related_pbds.len() {
                details.push(format!(
                    "{name}: {}/{} PBDs attached",
                    attached,
                    related_pbds.len()
                ));
                severity = max_severity(severity, Severity::Warning);
            }
        }
        if multi_host && sr["shared"].as_bool() == Some(false) {
            details.push(format!("{name}: local storage cannot serve other hosts"));
            severity = max_severity(severity, Severity::Warning);
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
        "Storage health",
        severity,
        if details.is_empty() {
            "Storage repositories have capacity, shared storage and attached PBDs.".into()
        } else {
            format!("{} storage issue(s).", details.len())
        },
        details,
    )
}

fn network(raw: &RawInventory) -> DiagnosticCheck {
    let mut details = Vec::new();
    let mut severity = Severity::Ok;
    if raw.pifs.is_empty() {
        return check(
            "network",
            "Network health",
            Severity::Info,
            "PIF data is not reported by this host.",
            Vec::new(),
        );
    }
    let mut management_hosts = std::collections::HashSet::new();
    for pif in raw.pifs.values() {
        if pif["management"].as_bool() != Some(true) {
            continue;
        }
        let host_ref = pif["host"]
            .as_str()
            .or_else(|| pif["Host"].as_str())
            .unwrap_or_default()
            .to_owned();
        management_hosts.insert(host_ref.clone());
        if pif["currently_attached"].as_bool() == Some(false) {
            let host_name = raw
                .hosts
                .get(&host_ref)
                .map(|host| text(host, "name_label"))
                .unwrap_or_else(|| host_ref.clone());
            details.push(format!(
                "{host_name}: management PIF {} is not attached",
                text(pif, "device")
            ));
            severity = Severity::Critical;
        }
    }
    for (host_ref, host) in &raw.hosts {
        if !management_hosts.contains(host_ref) {
            details.push(format!(
                "{}: no management PIF reported",
                text(host, "name_label")
            ));
            severity = max_severity(severity, Severity::Warning);
        }
    }
    let mut bridges = std::collections::HashMap::new();
    for network in raw.networks.values() {
        let bridge = text(network, "bridge");
        if !bridge.is_empty() {
            bridges
                .entry(bridge)
                .or_insert_with(Vec::new)
                .push(text(network, "name_label"));
        }
    }
    for (bridge, networks) in bridges {
        if networks.len() > 1 {
            details.push(format!(
                "bridge {bridge} is used by {} networks: {}",
                networks.len(),
                networks.join(", ")
            ));
            severity = max_severity(severity, Severity::Warning);
        }
    }
    check(
        "network",
        "Network health",
        severity,
        if details.is_empty() {
            "Management interfaces are attached and bridges are unique.".into()
        } else {
            format!("{} network issue(s).", details.len())
        },
        details,
    )
}

fn guest_tools(raw: &RawInventory) -> DiagnosticCheck {
    let mut details = Vec::new();
    let mut severity = Severity::Ok;
    for vm in raw.vms.values() {
        if vm["is_control_domain"].as_bool() == Some(true)
            || vm["is_a_template"].as_bool() == Some(true)
            || vm["is_a_snapshot"].as_bool() == Some(true)
            || vm["power_state"].as_str() != Some("Running")
        {
            continue;
        }
        let name = text(vm, "name_label");
        let guest_ref = text(vm, "guest_metrics");
        if guest_ref.is_empty() || guest_ref == "OpaqueRef:NULL" {
            details.push(format!("{name}: guest tools/metrics are not reporting"));
            severity = max_severity(severity, Severity::Info);
            continue;
        }
        let Some(guest) = raw.guest_metrics.get(&guest_ref) else {
            details.push(format!("{name}: guest metrics are unavailable"));
            severity = max_severity(severity, Severity::Info);
            continue;
        };
        if guest["PV_drivers_up_to_date"].as_bool() == Some(false) {
            details.push(format!("{name}: PV drivers/tools are out of date"));
            severity = max_severity(severity, Severity::Warning);
        } else if text(guest, "PV_drivers_version").is_empty() {
            details.push(format!("{name}: PV driver version is not reported"));
            severity = max_severity(severity, Severity::Info);
        }
    }
    check(
        "guest-tools",
        "Guest tools",
        severity,
        if details.is_empty() {
            "Running guests report current tools or no issues.".into()
        } else {
            format!("{} guest tools issue(s).", details.len())
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
    let Some(pool) = raw.pools.values().next() else {
        return check(
            "ha",
            "High availability",
            Severity::Info,
            "No pool record is available.",
            Vec::new(),
        );
    };
    let enabled = pool["ha_enabled"].as_bool().unwrap_or(false);
    if !enabled {
        return check(
            "ha",
            "High availability",
            Severity::Info,
            "Pool HA is disabled.",
            Vec::new(),
        );
    }
    let enabled_hosts = raw
        .hosts
        .values()
        .filter(|host| host["enabled"].as_bool().unwrap_or(true))
        .count() as u64;
    let failures = number(pool, "ha_host_failures_to_tolerate");
    let statefiles = pool["ha_statefiles"].as_array().map_or(0, Vec::len);
    let overcommitted = pool["ha_overcommitted"].as_bool().unwrap_or(false);
    let mut details = Vec::new();
    let mut severity = Severity::Ok;
    if statefiles == 0 {
        details.push("No HA statefile is configured.".into());
        severity = Severity::Critical;
    }
    if overcommitted {
        details.push("HA is overcommitted for the current host capacity.".into());
        severity = Severity::Critical;
    }
    if failures >= enabled_hosts.max(1) {
        details.push(format!(
            "HA tolerates {failures} host failure(s) with only {enabled_hosts} enabled host(s)."
        ));
        severity = max_severity(severity, Severity::Warning);
    }
    check(
        "ha",
        "High availability",
        severity,
        if details.is_empty() {
            format!("HA is enabled and tolerates {failures} host failure(s).")
        } else {
            format!(
                "HA is enabled with {} configuration issue(s).",
                details.len()
            )
        },
        details,
    )
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

fn vbd_consistency(raw: &RawInventory) -> DiagnosticCheck {
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
        let mut devices = std::collections::HashMap::new();
        let mut bootable = 0usize;
        for vbd in raw
            .vbds
            .values()
            .filter(|vbd| vbd["VM"].as_str() == Some(vm_ref.as_str()))
        {
            let device = text(vbd, "device");
            if !device.is_empty() {
                *devices.entry(device.clone()).or_insert(0usize) += 1;
            }
            if vbd["bootable"].as_bool() == Some(true) {
                bootable += 1;
            }
            if vbd["empty"].as_bool() == Some(true) {
                continue;
            }
            let Some(vdi) = raw.vdis.get(text(vbd, "VDI").as_str()) else {
                continue;
            };
            if text(vbd, "type") == "CD" {
                let sr = raw.storage.get(&text(vdi, "SR"));
                let is_iso = text(vdi, "type") == "iso"
                    || sr.is_some_and(|sr| text(sr, "content_type") == "iso");
                if !is_iso {
                    details.push(format!(
                        "{vm_name}: CD drive {device} contains non-ISO media"
                    ));
                    severity = max_severity(severity, Severity::Warning);
                }
            }
            if text(vbd, "type") == "Disk"
                && vdi["read_only"].as_bool() == Some(true)
                && text(vbd, "mode") != "RO"
            {
                details.push(format!(
                    "{vm_name}: read-only disk {device} is attached read-write"
                ));
                severity = max_severity(severity, Severity::Warning);
            }
        }
        for (device, count) in devices {
            if count > 1 {
                details.push(format!(
                    "{vm_name}: device slot {device} is used by {count} VBDs"
                ));
                severity = max_severity(severity, Severity::Warning);
            }
        }
        if bootable > 1 {
            details.push(format!(
                "{vm_name}: {bootable} bootable VBDs; boot order chooses the device"
            ));
            severity = max_severity(severity, Severity::Warning);
        }
    }
    check(
        "vbd-consistency",
        "VBD / CD consistency",
        severity,
        if details.is_empty() {
            "VBD slots, boot flags and CD media types are consistent.".into()
        } else {
            format!("{} VBD consistency issue(s).", details.len())
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
        raw.pools.insert(
            "p".into(),
            json!({"ha_enabled":true,"ha_statefiles":["sf"],"ha_host_failures_to_tolerate":0,"ha_overcommitted":false}),
        );
        raw.storage.insert(
            "sr".into(),
            json!({"name_label":"sr","shared":true,"PBDs":["pbd"],"physical_size":1000,"physical_utilisation":100}),
        );
        raw.pbds.insert(
            "pbd".into(),
            json!({"SR":"sr","device":"0","currently_attached":true}),
        );
        raw.vms.insert(
            "vm".into(),
            json!({"name_label":"vm","power_state":"Halted","is_control_domain":false,"is_a_template":false,"is_a_snapshot":false}),
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
    fn detects_ha_and_guest_tool_issues() {
        let mut raw = base();
        let pool = raw.pools.get_mut("p").unwrap();
        pool["ha_statefiles"] = json!([]);
        pool["ha_host_failures_to_tolerate"] = json!(2);
        let vm = raw.vms.get_mut("vm").unwrap();
        vm["power_state"] = json!("Running");
        vm["guest_metrics"] = json!("gm");
        raw.guest_metrics.insert(
            "gm".into(),
            json!({"PV_drivers_up_to_date":false,"PV_drivers_version":"7.0"}),
        );
        let report = evaluate(&raw);
        let ha = report.checks.iter().find(|check| check.id == "ha").unwrap();
        assert_eq!(ha.severity, Severity::Critical);
        let tools = report
            .checks
            .iter()
            .find(|check| check.id == "guest-tools")
            .unwrap();
        assert_eq!(tools.severity, Severity::Warning);
        assert!(tools.details[0].contains("out of date"));
    }

    #[test]
    fn detects_management_pif_and_local_storage() {
        let mut raw = base();
        raw.hosts.get_mut("h1").unwrap()["PIFs"] = json!(["pif1"]);
        raw.pifs.insert(
            "pif1".into(),
            json!({"Host":"h1","device":"eth0","management":true,"currently_attached":false}),
        );
        raw.storage.get_mut("sr").unwrap()["shared"] = json!(false);
        let report = evaluate(&raw);
        let network = report
            .checks
            .iter()
            .find(|check| check.id == "network")
            .unwrap();
        assert_eq!(network.severity, Severity::Critical);
        let storage = report
            .checks
            .iter()
            .find(|check| check.id == "storage")
            .unwrap();
        assert_eq!(storage.severity, Severity::Warning);
        assert!(
            storage
                .details
                .iter()
                .any(|detail| detail.contains("local storage"))
        );
    }

    #[test]
    fn detects_unattached_pbd() {
        let mut raw = base();
        raw.pbds.get_mut("pbd").unwrap()["currently_attached"] = json!(false);
        let report = evaluate(&raw);
        let storage = report
            .checks
            .iter()
            .find(|check| check.id == "storage")
            .unwrap();
        assert_eq!(storage.severity, Severity::Critical);
        assert!(
            storage
                .details
                .iter()
                .any(|detail| detail.contains("no PBD"))
        );
    }

    #[test]
    fn detects_vbd_slot_boot_and_cd_consistency_issues() {
        let mut raw = base();
        raw.vbds.insert(
            "vbd2".into(),
            json!({"VM":"vm","VDI":"vdi","device":"xvda","type":"Disk","empty":false,"bootable":true,"mode":"RW"}),
        );
        raw.vbds.get_mut("vbd").unwrap()["bootable"] = json!(true);
        raw.vdis.get_mut("vdi").unwrap()["read_only"] = json!(true);
        raw.vbds.get_mut("vbd").unwrap()["mode"] = json!("RW");
        raw.vbds.insert(
            "cd".into(),
            json!({"VM":"vm","VDI":"vdi","device":"xvdd","type":"CD","empty":false,"bootable":false,"mode":"RO"}),
        );
        let report = evaluate(&raw);
        let check = report
            .checks
            .iter()
            .find(|check| check.id == "vbd-consistency")
            .unwrap();
        assert_eq!(check.severity, Severity::Warning);
        assert!(
            check
                .details
                .iter()
                .any(|detail| detail.contains("device slot"))
        );
        assert!(
            check
                .details
                .iter()
                .any(|detail| detail.contains("bootable VBDs"))
        );
        assert!(
            check
                .details
                .iter()
                .any(|detail| detail.contains("non-ISO"))
        );
        assert!(
            check
                .details
                .iter()
                .any(|detail| detail.contains("read-only disk"))
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
