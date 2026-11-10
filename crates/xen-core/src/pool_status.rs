//! Read-only pool, HA, WLB, PVS, VMSS and DR status.
use std::collections::HashMap;

use serde::Serialize;
use serde_json::Value;

use crate::xapi::{ApiError, Connection};

#[derive(Clone, Debug, Serialize)]
pub struct PoolConfigEntry {
    pub key: String,
    pub value: String,
    pub sensitive: bool,
}

#[derive(Clone, Debug, Serialize)]
pub struct HaStatus {
    pub enabled: bool,
    pub failures_to_tolerate: u64,
    pub overcommitted: bool,
    pub allow_overcommit: bool,
    pub plan_exists_for: u64,
    pub cluster_stack: String,
    pub statefiles: Vec<String>,
    pub redo_log_enabled: bool,
    pub redo_log_vdi: String,
    pub reboot_vm_on_internal_shutdown: bool,
    pub configuration: Vec<PoolConfigEntry>,
}

#[derive(Clone, Debug, Serialize)]
pub struct WlbStatus {
    pub enabled: bool,
    pub url: String,
    pub username: String,
    pub verify_cert: bool,
    pub recommendations: Vec<PoolConfigEntry>,
}

#[derive(Clone, Debug, Serialize)]
pub struct PvsSite {
    pub uuid: String,
    pub name: String,
    pub pvs_uuid: String,
    pub servers: usize,
    pub proxies: usize,
    pub cache_storages: usize,
}

#[derive(Clone, Debug, Serialize)]
pub struct PvsServer {
    pub uuid: String,
    pub site_name: String,
    pub addresses: String,
    pub port_range: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct PvsProxy {
    pub uuid: String,
    pub site_name: String,
    pub vif: String,
    pub attached: bool,
    pub status: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct PvsCache {
    pub uuid: String,
    pub site_name: String,
    pub host_name: String,
    pub sr_name: String,
    pub size: u64,
}

#[derive(Clone, Debug, Serialize)]
pub struct PvsStatus {
    pub sites: Vec<PvsSite>,
    pub servers: Vec<PvsServer>,
    pub proxies: Vec<PvsProxy>,
    pub cache_storages: Vec<PvsCache>,
}

#[derive(Clone, Debug, Serialize)]
pub struct VmssStatus {
    pub uuid: String,
    pub name: String,
    pub description: String,
    pub enabled: bool,
    pub schedule_type: String,
    pub frequency: String,
    pub retained_snapshots: u64,
    pub schedule: String,
    pub minute: u8,
    pub hour: u8,
    pub days: Vec<String>,
    pub last_run_time: String,
    pub vms: Vec<String>,
}

#[derive(Clone, Debug, Serialize)]
pub struct DrStatus {
    pub uuid: String,
    pub introduced_srs: Vec<String>,
}

#[derive(Clone, Debug, Serialize)]
pub struct PoolStatus {
    pub name: String,
    pub uuid: String,
    pub master_name: String,
    pub ha: HaStatus,
    pub wlb: WlbStatus,
    pub pvs: PvsStatus,
    pub vmss: Vec<VmssStatus>,
    pub dr: Vec<DrStatus>,
}

impl Connection {
    pub async fn pool_status(&self) -> Result<PoolStatus, ApiError> {
        let (pools, hosts, vms, vdis, srs) = tokio::join!(
            self.records("pool"),
            self.records("host"),
            self.records("VM"),
            self.records("VDI"),
            self.records("SR")
        );
        let pools = pools?;
        let hosts = hosts?;
        let vms = vms?;
        let vdis = vdis?;
        let srs = srs?;
        let (vmss, pvs_sites, pvs_servers, pvs_proxies, pvs_cache, dr_tasks) = tokio::join!(
            self.records("VMSS"),
            self.records("PVS_site"),
            self.records("PVS_server"),
            self.records("PVS_proxy"),
            self.records("PVS_cache_storage"),
            self.records("DR_task")
        );
        let vmss = vmss.unwrap_or_default();
        let pvs_sites = pvs_sites.unwrap_or_default();
        let pvs_servers = pvs_servers.unwrap_or_default();
        let pvs_proxies = pvs_proxies.unwrap_or_default();
        let pvs_cache = pvs_cache.unwrap_or_default();
        let dr_tasks = dr_tasks.unwrap_or_default();
        let pool = pools
            .values()
            .next()
            .ok_or_else(|| ApiError::Xen("No pool record is available.".into()))?;
        let master_name = lookup(&hosts, &field(pool, "master"));
        let ha_statefiles = pool["ha_statefiles"]
            .as_array()
            .map(|refs| {
                refs.iter()
                    .filter_map(Value::as_str)
                    .map(|reference| {
                        vdis.get(reference)
                            .map(|vdi| field(vdi, "name_label"))
                            .filter(|name| !name.is_empty())
                            .unwrap_or_else(|| reference.to_owned())
                    })
                    .collect()
            })
            .unwrap_or_default();
        let redo_ref = field(pool, "redo_log_vdi");
        let redo_log_vdi = if redo_ref.is_empty() || redo_ref == "OpaqueRef:NULL" {
            String::new()
        } else {
            vdis.get(&redo_ref)
                .map(|vdi| field(vdi, "name_label"))
                .filter(|name| !name.is_empty())
                .unwrap_or(redo_ref)
        };
        let site_names: HashMap<&str, String> = pvs_sites
            .iter()
            .map(|(reference, site)| (reference.as_str(), field(site, "name_label")))
            .collect();
        let sites = pvs_sites
            .values()
            .map(|site| PvsSite {
                uuid: field(site, "uuid"),
                name: field(site, "name_label"),
                pvs_uuid: field(site, "PVS_uuid"),
                servers: site["servers"].as_array().map_or(0, Vec::len),
                proxies: site["proxies"].as_array().map_or(0, Vec::len),
                cache_storages: site["cache_storage"].as_array().map_or(0, Vec::len),
            })
            .collect();
        let servers = pvs_servers
            .values()
            .map(|server| {
                let site_ref = field(server, "site");
                PvsServer {
                    uuid: field(server, "uuid"),
                    site_name: site_names
                        .get(site_ref.as_str())
                        .cloned()
                        .unwrap_or_default(),
                    addresses: server["addresses"]
                        .as_array()
                        .map(|values| {
                            values
                                .iter()
                                .filter_map(Value::as_str)
                                .collect::<Vec<_>>()
                                .join(", ")
                        })
                        .unwrap_or_default(),
                    port_range: format!(
                        "{}-{}",
                        number(server, "first_port"),
                        number(server, "last_port")
                    ),
                }
            })
            .collect();
        let proxies = pvs_proxies
            .values()
            .map(|proxy| {
                let site_ref = field(proxy, "site");
                PvsProxy {
                    uuid: field(proxy, "uuid"),
                    site_name: site_names
                        .get(site_ref.as_str())
                        .cloned()
                        .unwrap_or_default(),
                    vif: field(proxy, "VIF"),
                    attached: proxy["currently_attached"].as_bool().unwrap_or(false),
                    status: field(proxy, "status"),
                }
            })
            .collect();
        let cache_storages = pvs_cache
            .values()
            .map(|cache| {
                let site_ref = field(cache, "site");
                PvsCache {
                    uuid: field(cache, "uuid"),
                    site_name: site_names
                        .get(site_ref.as_str())
                        .cloned()
                        .unwrap_or_default(),
                    host_name: lookup(&hosts, &field(cache, "host")),
                    sr_name: lookup(&srs, &field(cache, "SR")),
                    size: number(cache, "size"),
                }
            })
            .collect();
        let mut vmss_list: Vec<VmssStatus> = vmss
            .values()
            .map(|schedule| VmssStatus {
                uuid: field(schedule, "uuid"),
                name: field(schedule, "name_label"),
                description: field(schedule, "name_description"),
                enabled: schedule["enabled"].as_bool().unwrap_or(false),
                schedule_type: field(schedule, "type"),
                frequency: field(schedule, "frequency"),
                retained_snapshots: number(schedule, "retained_snapshots"),
                schedule: schedule_label(&schedule["schedule"]),
                minute: schedule_number(&schedule["schedule"], "min"),
                hour: schedule_number(&schedule["schedule"], "hour"),
                days: schedule["schedule"]["days"]
                    .as_str()
                    .map(|days| {
                        days.split(',')
                            .map(str::trim)
                            .filter(|day| !day.is_empty())
                            .map(str::to_lowercase)
                            .collect()
                    })
                    .unwrap_or_default(),
                last_run_time: field(schedule, "last_run_time"),
                vms: schedule["VMs"]
                    .as_array()
                    .map(|refs| {
                        refs.iter()
                            .filter_map(Value::as_str)
                            .map(|reference| {
                                vms.get(reference)
                                    .map(|vm| field(vm, "name_label"))
                                    .unwrap_or_else(|| reference.to_owned())
                            })
                            .collect()
                    })
                    .unwrap_or_default(),
            })
            .collect();
        vmss_list.sort_by(|left, right| left.name.cmp(&right.name));
        let dr = dr_tasks
            .values()
            .map(|task| DrStatus {
                uuid: field(task, "uuid"),
                introduced_srs: task["introduced_SRs"]
                    .as_array()
                    .map(|refs| {
                        refs.iter()
                            .filter_map(Value::as_str)
                            .map(|reference| {
                                srs.get(reference)
                                    .map(|sr| field(sr, "name_label"))
                                    .unwrap_or_else(|| reference.to_owned())
                            })
                            .collect()
                    })
                    .unwrap_or_default(),
            })
            .collect();
        Ok(PoolStatus {
            name: field(pool, "name_label"),
            uuid: field(pool, "uuid"),
            master_name,
            ha: HaStatus {
                enabled: pool["ha_enabled"].as_bool().unwrap_or(false),
                failures_to_tolerate: number(pool, "ha_host_failures_to_tolerate"),
                overcommitted: pool["ha_overcommitted"].as_bool().unwrap_or(false),
                allow_overcommit: pool["ha_allow_overcommit"].as_bool().unwrap_or(false),
                plan_exists_for: number(pool, "ha_plan_exists_for"),
                cluster_stack: field(pool, "ha_cluster_stack"),
                statefiles: ha_statefiles,
                redo_log_enabled: pool["redo_log_enabled"].as_bool().unwrap_or(false),
                redo_log_vdi,
                reboot_vm_on_internal_shutdown: pool["ha_reboot_vm_on_internal_shutdown"]
                    .as_bool()
                    .unwrap_or(false),
                configuration: config_entries(&pool["ha_configuration"]),
            },
            wlb: WlbStatus {
                enabled: pool["wlb_enabled"].as_bool().unwrap_or(false),
                url: field(pool, "wlb_url"),
                username: field(pool, "wlb_username"),
                verify_cert: pool["wlb_verify_cert"].as_bool().unwrap_or(false),
                recommendations: config_entries(&pool["recommendations"]),
            },
            pvs: PvsStatus {
                sites,
                servers,
                proxies,
                cache_storages,
            },
            vmss: vmss_list,
            dr,
        })
    }
}

fn lookup(records: &HashMap<String, Value>, reference: &str) -> String {
    records
        .get(reference)
        .map(|record| field(record, "name_label"))
        .unwrap_or_default()
}

fn field(record: &Value, key: &str) -> String {
    record[key].as_str().unwrap_or_default().to_owned()
}

fn number(record: &Value, key: &str) -> u64 {
    record[key]
        .as_u64()
        .or_else(|| record[key].as_str().and_then(|value| value.parse().ok()))
        .unwrap_or(0)
}

fn schedule_number(schedule: &Value, key: &str) -> u8 {
    schedule[key]
        .as_str()
        .and_then(|value| value.parse().ok())
        .or_else(|| schedule[key].as_u64().map(|value| value as u8))
        .unwrap_or(0)
}

fn schedule_label(schedule: &Value) -> String {
    let Some(schedule) = schedule.as_object() else {
        return String::new();
    };
    let mut entries: Vec<String> = schedule
        .iter()
        .map(|(key, value)| {
            format!(
                "{key}={}",
                value
                    .as_str()
                    .map(str::to_owned)
                    .unwrap_or_else(|| value.to_string())
            )
        })
        .collect();
    entries.sort();
    entries.join(", ")
}

fn config_entries(value: &Value) -> Vec<PoolConfigEntry> {
    let Some(values) = value.as_object() else {
        return Vec::new();
    };
    let mut entries: Vec<PoolConfigEntry> = values
        .iter()
        .map(|(key, value)| {
            let sensitive = sensitive_key(key);
            PoolConfigEntry {
                key: key.clone(),
                value: if sensitive {
                    "••••".into()
                } else {
                    value
                        .as_str()
                        .map(str::to_owned)
                        .unwrap_or_else(|| value.to_string())
                },
                sensitive,
            }
        })
        .collect();
    entries.sort_by(|left, right| left.key.cmp(&right.key));
    entries
}

fn sensitive_key(key: &str) -> bool {
    let key = key.to_ascii_lowercase();
    ["password", "passwd", "secret", "credential", "keytab"]
        .iter()
        .any(|needle| key.contains(needle))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn schedules_are_rendered_deterministically() {
        assert_eq!(
            schedule_label(&json!({"hour": "3", "minute": "15"})),
            "hour=3, minute=15"
        );
        assert_eq!(schedule_label(&json!({})), "");
        assert_eq!(schedule_label(&Value::Null), "");
    }

    #[test]
    fn schedule_fields_are_parsed_for_editing() {
        let schedule = json!({"hour": "3", "min": "15", "days": "Monday,Friday"});
        assert_eq!(schedule_number(&schedule, "hour"), 3);
        assert_eq!(schedule_number(&schedule, "min"), 15);
        assert_eq!(schedule_number(&schedule, "missing"), 0);
        assert_eq!(
            schedule["days"].as_str().map(|days| days
                .split(',')
                .map(str::trim)
                .map(str::to_lowercase)
                .collect::<Vec<_>>()),
            Some(vec!["monday".to_string(), "friday".to_string()])
        );
    }

    #[test]
    fn config_values_are_redacted_sorted_and_bool_rendered() {
        let entries = config_entries(&json!({
            "z_password": "secret",
            "enabled": true,
            "name": "pool"
        }));
        assert_eq!(entries.len(), 3);
        assert_eq!(entries[0].key, "enabled");
        assert_eq!(entries[0].value, "true");
        assert_eq!(entries[1].key, "name");
        assert_eq!(entries[1].value, "pool");
        assert_eq!(entries[2].key, "z_password");
        assert!(entries[2].sensitive);
        assert_eq!(entries[2].value, "••••");
    }

    #[test]
    fn pvs_ports_and_reference_labels_are_friendly() {
        assert_eq!(number(&json!({"first_port": "6900"}), "first_port"), 6900);
        assert_eq!(number(&json!({"last_port": 6910}), "last_port"), 6910);
        assert!(sensitive_key("AD_PASSWORD"));
        assert!(!sensitive_key("wlb_username"));
    }
}
