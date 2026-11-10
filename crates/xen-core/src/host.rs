//! Read-only, host-scoped views. Maps returned by XenAPI are never exported wholesale:
//! each display field below is deliberately selected so configuration secrets stay private.
use std::{
    collections::{BTreeMap, HashMap},
    time::{SystemTime, UNIX_EPOCH},
};

use serde::Serialize;
use serde_json::{Value, json};

use crate::{
    performance::parse_server_epoch,
    xapi::{ApiError, Connection},
};

type Records = HashMap<String, Value>;
type RecordRead = Result<Records, ApiError>;

#[derive(Clone, Debug, Serialize)]
pub struct InfoField {
    pub label: String,
    pub value: String,
}
#[derive(Clone, Debug, Serialize)]
pub struct InfoGroup {
    pub title: String,
    pub fields: Vec<InfoField>,
}
#[derive(Clone, Debug, Serialize)]
pub struct DetailRecord {
    pub id: String,
    pub title: String,
    pub values: BTreeMap<String, String>,
    pub groups: Vec<InfoGroup>,
}
#[derive(Clone, Debug, Serialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum DataSection {
    Ready {
        columns: Vec<String>,
        rows: Vec<DetailRecord>,
    },
    Unsupported {
        message: String,
    },
    Unavailable {
        message: String,
    },
}
#[derive(Clone, Debug, Serialize)]
pub struct SectionWarning {
    pub section: String,
    pub message: String,
}
#[derive(Clone, Debug, Serialize)]
pub struct HostDetails {
    pub host_uuid: String,
    pub name: String,
    pub address: String,
    pub enabled: bool,
    pub power_on: bool,
    pub collected_at: u64,
    pub general: Vec<InfoGroup>,
    pub memory: Vec<InfoGroup>,
    pub cpu: DataSection,
    pub nics: DataSection,
    pub storage: DataSection,
    pub gpus: DataSection,
    pub pci: DataSection,
    pub usb: DataSection,
    pub updates: DataSection,
    pub certificates: DataSection,
    pub alerts: DataSection,
    pub crashdumps: DataSection,
    pub warnings: Vec<SectionWarning>,
}

impl Connection {
    pub async fn host_details(&self, host_uuid: &str) -> Result<HostDetails, ApiError> {
        // Resolve through this session; never accept an opaque reference from another connection.
        let host_ref: String = self
            .request("host.get_by_uuid", json!([self.session, host_uuid]))
            .await?;
        let host: Value = self
            .request("host.get_record", json!([self.session, host_ref]))
            .await?;
        if host["uuid"].as_str() != Some(host_uuid) {
            return Err(ApiError::Protocol);
        }

        // Bound fan-out to four calls, including ancillary reads on older servers.
        let (metrics, pools, cpus, server_time) = tokio::join!(
            self.records("host_metrics"),
            self.records("pool"),
            self.records("host_cpu"),
            self.request::<Value>("host.get_servertime", json!([self.session, host_ref]))
        );
        let (pifs, pif_metrics, networks, bonds) = tokio::join!(
            self.records("PIF"),
            self.records("PIF_metrics"),
            self.records("network"),
            self.records("Bond")
        );
        let (pbds, srs, pgpus, pci) = tokio::join!(
            self.records("PBD"),
            self.records("SR"),
            self.records("PGPU"),
            self.records("PCI")
        );
        let (vgpu_types, usb, certificates, messages) = tokio::join!(
            self.records("VGPU_type"),
            self.records("PUSB"),
            self.records("Certificate"),
            self.records("message")
        );
        let (host_patches, pool_patches, updates, crashdumps) = tokio::join!(
            self.records("host_patch"),
            self.records("pool_patch"),
            self.records("pool_update"),
            self.records("host_crashdump")
        );
        let (vms, vm_metrics, guest_metrics, vlans) = tokio::join!(
            self.records("VM"),
            self.records("VM_metrics"),
            self.records("VM_guest_metrics"),
            self.records("VLAN")
        );
        let sriov = self.records("network_sriov").await;
        let mut capacities = HashMap::new();
        if let (Ok(sriov), Ok(pifs)) = (&sriov, &pifs) {
            for (reference, r) in sriov {
                if local_sriov(r, pifs, &host_ref) {
                    capacities.insert(
                        reference.clone(),
                        self.request::<Value>(
                            "network_sriov.get_remaining_capacity",
                            json!([self.session, reference]),
                        )
                        .await,
                    );
                }
            }
        }
        let mut warnings = Vec::new();
        let cpus = checked_host_records(cpus, &host, "host_CPUs", &host_ref, "CPU", &mut warnings);
        let pifs =
            checked_host_records(pifs, &host, "PIFs", &host_ref, "Networking", &mut warnings);
        let pbds = checked_host_records(pbds, &host, "PBDs", &host_ref, "Storage", &mut warnings);
        let pgpus = checked_host_records(pgpus, &host, "PGPUs", &host_ref, "GPUs", &mut warnings);
        let usb = checked_host_records(usb, &host, "PUSBs", &host_ref, "USB", &mut warnings);
        let general = general_groups(&host, &host_ref, &pools, &server_time, &mut warnings);
        let memory = memory_groups(
            &host,
            &host_ref,
            &metrics,
            &vms,
            &vm_metrics,
            &guest_metrics,
            &mut warnings,
        );
        let mut cpu = simple_section(
            cpus,
            &host_ref,
            "CPU",
            &["CPU", "Model", "Speed (MHz)", "Vendor", "Utilisation"],
            |reference, r| {
                let mut row = record(reference, &format!("CPU {}", display(&r["number"])));
                row.values = values(
                    r,
                    &[
                        ("number", "CPU"),
                        ("modelname", "Model"),
                        ("speed", "Speed (MHz)"),
                        ("vendor", "Vendor"),
                        ("utilisation", "Utilisation"),
                    ],
                );
                row.groups.push(group(
                    "Processor",
                    r,
                    &[
                        ("uuid", "UUID"),
                        ("number", "Number"),
                        ("modelname", "Model"),
                        ("vendor", "Vendor"),
                        ("family", "Family"),
                        ("model", "Model ID"),
                        ("stepping", "Stepping"),
                        ("speed", "Speed (MHz)"),
                        ("flags", "Flags"),
                        ("features", "Features"),
                        ("utilisation", "Utilisation (fraction)"),
                    ],
                ));
                row
            },
        );
        if let DataSection::Ready { rows, .. } = &mut cpu {
            rows.sort_by_key(|r| {
                r.values
                    .get("CPU")
                    .and_then(|s| s.parse::<u64>().ok())
                    .unwrap_or(u64::MAX)
            });
        }
        let nics = network_section(
            pifs,
            NetworkReads {
                metrics: &pif_metrics,
                networks: &networks,
                bonds: &bonds,
                pci: &pci,
                vlans: &vlans,
                sriov: &sriov,
                capacities: &capacities,
            },
            &host_ref,
            &mut warnings,
        );
        let storage = storage_section(pbds, &srs, &host_ref, &host, &metrics, &mut warnings);
        let gpus = gpu_section(pgpus, &pci, &vgpu_types, &host_ref, &mut warnings);
        let pci_section = simple_section(
            pci,
            &host_ref,
            "PCI",
            &["Address", "Vendor", "Device", "Class", "Functions"],
            |reference, r| {
                let mut row = record(reference, &display(&r["pci_id"]));
                row.values = values(
                    r,
                    &[
                        ("pci_id", "Address"),
                        ("vendor_name", "Vendor"),
                        ("device_name", "Device"),
                        ("class_name", "Class"),
                        ("functions", "Functions"),
                    ],
                );
                row.groups.push(pci_group(r));
                row
            },
        );
        let usb = simple_section(
            usb,
            &host_ref,
            "USB",
            &["Path", "Vendor", "Product", "Version", "Passthrough"],
            |reference, r| {
                let mut row = record(reference, &display(&r["product_desc"]));
                row.values = values(
                    r,
                    &[
                        ("path", "Path"),
                        ("vendor_desc", "Vendor"),
                        ("product_desc", "Product"),
                        ("version", "Version"),
                        ("passthrough_enabled", "Passthrough"),
                    ],
                );
                row.groups.push(group(
                    "USB device",
                    r,
                    &[
                        ("uuid", "UUID"),
                        ("path", "Path"),
                        ("vendor_id", "Vendor ID"),
                        ("vendor_desc", "Vendor"),
                        ("product_id", "Product ID"),
                        ("product_desc", "Product"),
                        ("serial", "Serial number"),
                        ("version", "Version"),
                        ("description", "Description"),
                        ("speed", "Speed (Mbps)"),
                        ("passthrough_enabled", "Passthrough enabled"),
                    ],
                ));
                row
            },
        );
        let updates = update_section(
            host_patches,
            &pool_patches,
            updates,
            &host_ref,
            &host,
            &mut warnings,
        );
        let certificates = certificate_section(certificates, &host_ref, &host, &mut warnings);
        if let (DataSection::Ready { rows, .. }, Some(now)) =
            (&certificates, server_time.as_ref().ok().and_then(timestamp))
        {
            for row in rows {
                if row
                    .values
                    .get("Expires")
                    .and_then(|v| timestamp(&json!(v)))
                    .is_some_and(|expiry| expiry <= now)
                {
                    warn("Health", "A host certificate has expired.", &mut warnings);
                }
                if row
                    .values
                    .get("Valid from")
                    .and_then(|v| timestamp(&json!(v)))
                    .is_some_and(|start| start > now)
                {
                    warn(
                        "Health",
                        "A host certificate is not yet valid.",
                        &mut warnings,
                    );
                }
            }
        }
        let alerts = match messages {
            Err(e) => failed_section("Alerts", e),
            Ok(records) => ready(
                &["Name", "Priority", "Timestamp"],
                records
                    .into_iter()
                    .filter(|(_, r)| {
                        r["obj_uuid"].as_str() == Some(host_uuid)
                            && r["cls"]
                                .as_str()
                                .is_some_and(|s| s.eq_ignore_ascii_case("host"))
                    })
                    .map(|(reference, r)| {
                        let mut row = record(&reference, &display(&r["name"]));
                        row.values = values(
                            &r,
                            &[
                                ("name", "Name"),
                                ("priority", "Priority"),
                                ("timestamp", "Timestamp"),
                            ],
                        );
                        row.groups.push(group(
                            "Host alert",
                            &r,
                            &[
                                ("uuid", "UUID"),
                                ("name", "Name"),
                                ("priority", "Priority"),
                                ("timestamp", "Timestamp"),
                                ("body", "Message"),
                            ],
                        ));
                        row
                    })
                    .collect(),
            ),
        };
        let crashdumps = simple_section(
            crashdumps,
            &host_ref,
            "Crash dumps",
            &["Timestamp", "Size", "UUID"],
            |reference, r| {
                let mut row = record(reference, &display(&r["timestamp"]));
                row.values = values(r, &[("timestamp", "Timestamp"), ("uuid", "UUID")]);
                row.values.insert("Size".into(), bytes(&r["size"]));
                row.groups.push(group(
                    "Crash dump",
                    r,
                    &[
                        ("uuid", "UUID"),
                        ("timestamp", "Timestamp"),
                        ("size", "Size (bytes)"),
                    ],
                ));
                row
            },
        );
        Ok(HostDetails {
            host_uuid: host_uuid.into(),
            name: display(&host["name_label"]),
            address: display(&host["address"]),
            enabled: bool_value(&host["enabled"]).unwrap_or(false),
            power_on: bool_value(&host["power_on"]).unwrap_or(false),
            collected_at: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs(),
            general,
            memory,
            cpu,
            nics,
            storage,
            gpus,
            pci: pci_section,
            usb,
            updates,
            certificates,
            alerts,
            crashdumps,
            warnings,
        })
    }
}

fn display(value: &Value) -> String {
    match value {
        Value::String(v) if v == "OpaqueRef:NULL" || v.is_empty() => "—".into(),
        Value::String(v) => v.clone(),
        Value::Bool(v) => if *v { "Yes" } else { "No" }.into(),
        Value::Number(v) => v.to_string(),
        Value::Array(v) => {
            let items: Vec<_> = v
                .iter()
                .filter(|v| !v.is_object() && !v.is_array() && !v.is_null())
                .map(display)
                .collect();
            if items.is_empty() {
                "None".into()
            } else {
                items.join(", ")
            }
        }
        _ => "—".into(),
    }
}
fn text<'a>(r: &'a Value, key: &str) -> &'a str {
    r[key].as_str().unwrap_or_default()
}
fn numeric(value: &Value) -> Option<f64> {
    value
        .as_f64()
        .or_else(|| value.as_str().and_then(|v| v.parse::<f64>().ok()))
        .filter(|v| v.is_finite() && *v >= 0.0)
}
fn bytes(value: &Value) -> String {
    let Some(mut n) = numeric(value) else {
        return "Unavailable".into();
    };
    let mut unit = "B";
    for candidate in ["KiB", "MiB", "GiB", "TiB", "PiB"] {
        if n < 1024.0 {
            break;
        }
        n /= 1024.0;
        unit = candidate;
    }
    format!("{n:.2} {unit}")
}
fn field(label: &str, value: impl Into<String>) -> InfoField {
    InfoField {
        label: label.into(),
        value: value.into(),
    }
}
fn group(title: &str, r: &Value, keys: &[(&str, &str)]) -> InfoGroup {
    InfoGroup {
        title: title.into(),
        fields: keys
            .iter()
            .filter(|(k, _)| !r[*k].is_null())
            .map(|(k, label)| field(label, display(&r[*k])))
            .collect(),
    }
}
fn values(r: &Value, keys: &[(&str, &str)]) -> BTreeMap<String, String> {
    keys.iter()
        .map(|(k, label)| ((*label).into(), display(&r[*k])))
        .collect()
}
fn record(id: &str, title: &str) -> DetailRecord {
    DetailRecord {
        id: id.into(),
        title: title.into(),
        values: BTreeMap::new(),
        groups: vec![],
    }
}
fn ready(columns: &[&str], mut rows: Vec<DetailRecord>) -> DataSection {
    rows.sort_by(|a, b| a.title.cmp(&b.title).then(a.id.cmp(&b.id)));
    DataSection::Ready {
        columns: columns.iter().map(|s| (*s).into()).collect(),
        rows,
    }
}
fn failed_section(section: &str, error: ApiError) -> DataSection {
    if matches!(error, ApiError::Unsupported) {
        DataSection::Unsupported {
            message: format!("{section} is not supported by this server's XenAPI."),
        }
    } else {
        DataSection::Unavailable {
            message: format!("Could not read {section}: {error}"),
        }
    }
}
fn warn(section: &str, message: impl Into<String>, warnings: &mut Vec<SectionWarning>) {
    warnings.push(SectionWarning {
        section: section.into(),
        message: message.into(),
    });
}
fn dependency<'a>(
    read: &'a RecordRead,
    section: &str,
    resource: &str,
    warnings: &mut Vec<SectionWarning>,
) -> Option<&'a Records> {
    match read {
        Ok(v) => Some(v),
        Err(e) => {
            warn(section, format!("{resource}: {e}"), warnings);
            None
        }
    }
}
fn related<'a>(records: Option<&'a Records>, reference: &str) -> &'a Value {
    records
        .and_then(|r| r.get(reference))
        .unwrap_or(&Value::Null)
}
fn includes(record: &Value, key: &str, reference: &str) -> bool {
    record[key]
        .as_array()
        .is_some_and(|v| v.iter().any(|item| item.as_str() == Some(reference)))
}
fn on_host(record: &Value, host_ref: &str) -> bool {
    record["host"].as_str() == Some(host_ref)
}
fn checked_host_records(
    read: RecordRead,
    host: &Value,
    key: &str,
    host_ref: &str,
    section: &str,
    warnings: &mut Vec<SectionWarning>,
) -> RecordRead {
    let records = read?;
    let missing = host[key].as_array().is_some_and(|refs| {
        refs.iter().any(|reference| {
            reference.as_str().is_none_or(|reference| {
                records
                    .get(reference)
                    .is_none_or(|record| !on_host(record, host_ref))
            })
        })
    });
    if missing {
        warn(
            section,
            "Some host-advertised records are unavailable or belong to another host.",
            warnings,
        );
        if !records.values().any(|record| on_host(record, host_ref)) {
            return Err(ApiError::Xen(
                "Host-advertised records could not be resolved.".into(),
            ));
        }
    }
    Ok(records)
}
fn simple_section(
    read: RecordRead,
    host_ref: &str,
    name: &str,
    columns: &[&str],
    build: impl Fn(&str, &Value) -> DetailRecord,
) -> DataSection {
    match read {
        Err(e) => failed_section(name, e),
        Ok(records) => ready(
            columns,
            records
                .into_iter()
                .filter(|(_, r)| on_host(r, host_ref))
                .map(|(reference, r)| build(&reference, &r))
                .collect(),
        ),
    }
}

fn timestamp(value: &Value) -> Option<u64> {
    parse_server_epoch(value)
}
fn uptime(now: Option<u64>, start: &Value) -> String {
    match (now, timestamp(start)) {
        (Some(now), Some(start)) if start > 0 && now >= start => {
            let elapsed = now - start;
            format!(
                "{}d {}h {}m",
                elapsed / 86400,
                elapsed % 86400 / 3600,
                elapsed % 3600 / 60
            )
        }
        _ => "Unavailable".into(),
    }
}
fn general_groups(
    host: &Value,
    host_ref: &str,
    pools: &RecordRead,
    server_time: &Result<Value, ApiError>,
    warnings: &mut Vec<SectionWarning>,
) -> Vec<InfoGroup> {
    let config = &host["other_config"];
    let mut groups = vec![group(
        "Identity",
        host,
        &[
            ("name_label", "Name"),
            ("name_description", "Description"),
            ("hostname", "Hostname"),
            ("address", "Management address"),
            ("uuid", "UUID"),
            ("enabled", "Enabled"),
            ("tags", "Tags"),
            ("capabilities", "Capabilities"),
        ],
    )];
    groups.push(group(
        "Host configuration",
        config,
        &[
            ("auto_poweron", "Host autostart flag"),
            ("multipathed", "Multipathing"),
            ("mpath-boot", "Boot multipathing"),
            ("folder", "Folder"),
        ],
    ));
    groups[1].fields.push(field(
        "Maintenance mode",
        bool_value(&config["MAINTENANCE_MODE"])
            .map_or("Unavailable", |value| if value { "Yes" } else { "No" }),
    ));
    groups[1].fields.push(field(
        "iSCSI IQN",
        display(host.get("iscsi_iqn").unwrap_or(&config["iscsi_iqn"])),
    ));
    if let Some((current, maximum)) = path_counts(&config["mpath-boot"]) {
        groups[1].fields.push(field(
            "Boot path health",
            path_health(current, maximum, None),
        ));
    }
    let now = match server_time {
        Ok(v) => {
            let parsed = timestamp(v);
            if parsed.is_none() {
                warn(
                    "General",
                    "Server time could not be parsed; uptime is unavailable.",
                    warnings,
                );
            }
            parsed
        }
        Err(e) => {
            warn(
                "General",
                format!("Server time: {e}; uptime is unavailable."),
                warnings,
            );
            None
        }
    };
    let mut runtime = group(
        "Runtime",
        config,
        &[
            ("boot_time", "Boot time (Unix seconds)"),
            ("agent_start_time", "Agent start time (Unix seconds)"),
        ],
    );
    runtime
        .fields
        .push(field("Host uptime", uptime(now, &config["boot_time"])));
    runtime.fields.push(field(
        "Agent uptime",
        uptime(now, &config["agent_start_time"]),
    ));
    if let Ok(time) = server_time {
        runtime.fields.push(field("Server time", display(time)));
    }
    runtime.fields.extend(
        group(
            "",
            host,
            &[
                ("power_on_mode", "Power-on mode"),
                ("pending_guidances", "Pending guidance"),
                ("pending_guidances_recommended", "Recommended guidance"),
                ("pending_guidances_full", "All pending guidance"),
            ],
        )
        .fields,
    );
    groups.push(runtime);
    groups.push(group(
        "Software",
        &host["software_version"],
        &[
            ("product_brand", "Product"),
            ("product_version", "Version"),
            ("product_version_text", "Display version"),
            ("product_version_text_short", "Short version"),
            ("platform_name", "Platform"),
            ("platform_version", "Platform version"),
            ("build_number", "Build"),
            ("date", "Build date"),
            ("xapi", "XAPI"),
            ("xapi_build", "XAPI build"),
            ("xen", "Xen"),
            ("linux", "Linux kernel"),
            ("xencenter_min", "Minimum client version"),
            ("xencenter_max", "Maximum client version"),
            ("dbv", "Database version"),
            ("network_backend", "Network backend"),
            ("hostname", "Build hostname"),
            ("company_name", "Company"),
        ],
    ));
    groups.push(group(
        "BIOS and hardware",
        &host["bios_strings"],
        &[
            ("bios-vendor", "BIOS vendor"),
            ("bios-version", "BIOS version"),
            ("bios-release-date", "BIOS release date"),
            ("system-manufacturer", "System manufacturer"),
            ("system-product-name", "System product"),
            ("system-version", "System version"),
            ("system-serial-number", "System serial"),
            ("system-uuid", "System UUID"),
            ("system-sku-number", "System SKU"),
            ("system-family", "System family"),
            ("baseboard-manufacturer", "Baseboard manufacturer"),
            ("baseboard-product-name", "Baseboard product"),
            ("baseboard-version", "Baseboard version"),
            ("baseboard-serial-number", "Baseboard serial"),
            ("enclosure-asset-tag", "Asset tag"),
        ],
    ));
    groups.push(group(
        "CPU summary",
        &host["cpu_info"],
        &[
            ("cpu_count", "Logical CPUs"),
            ("socket_count", "Sockets"),
            ("cores_per_socket", "Cores per socket"),
            ("threads_per_core", "Threads per core"),
            ("vendor", "Vendor"),
            ("modelname", "Model"),
            ("speed", "Speed (MHz)"),
            ("family", "Family"),
            ("model", "Model ID"),
            ("stepping", "Stepping"),
            ("flags", "Flags"),
            ("features", "Features"),
            ("features_pv", "PV features"),
            ("features_hvm", "HVM features"),
            ("physical_features", "Physical features"),
            ("maskable", "Feature masking"),
            ("numa_nodes", "NUMA nodes"),
        ],
    ));
    groups.push(group(
        "CPU configuration",
        &host["cpu_configuration"],
        &[
            ("scheduler-granularity", "Scheduler granularity"),
            ("nr-sockets", "Sockets"),
            ("nr-cores", "Cores"),
            ("nr-threads", "Threads"),
        ],
    ));
    let mut licensing = group("License", host, &[("edition", "Edition")]);
    licensing.fields.extend(
        group(
            "",
            &host["license_params"],
            &[
                ("edition", "Licensed edition"),
                ("sku_type", "SKU"),
                ("expiry", "Expiry"),
                ("expires", "Expires"),
                ("sockets", "Licensed sockets"),
                ("sockets-per-host", "Sockets per host"),
                ("socket_count", "Socket count"),
            ],
        )
        .fields,
    );
    groups.push(licensing);
    let mut security = group(
        "Authentication and TLS",
        host,
        &[
            ("external_auth_type", "Authentication type"),
            ("external_auth_service_name", "Authentication service"),
            ("tls_verification_enabled", "TLS verification enabled"),
            ("ssl_legacy", "Legacy SSL enabled"),
            ("https_only", "HTTPS only"),
            ("secure_boot", "Secure boot"),
        ],
    );
    security.fields.extend(
        group(
            "",
            &host["logging"],
            &[("syslog_destination", "Syslog destination")],
        )
        .fields,
    );
    if host["logging"].is_object() && text(&host["logging"], "syslog_destination").is_empty() {
        security.fields.retain(|f| f.label != "Syslog destination");
        security
            .fields
            .push(field("Syslog destination", "Local only"));
    }
    groups.push(security);
    if let Some(pools) = dependency(pools, "General", "Pool / HA", warnings) {
        // A XenAPI session has one pool; selecting any pool record is safe only when unambiguous.
        if pools.len() == 1 {
            let pool = pools.values().next().unwrap();
            let mut g = group(
                "Pool and high availability",
                pool,
                &[
                    ("name_label", "Pool"),
                    ("uuid", "Pool UUID"),
                    ("ha_enabled", "HA enabled"),
                    ("ha_host_failures_to_tolerate", "Host failures to tolerate"),
                    ("ha_plan_exists_for", "HA plan tolerates"),
                    ("ha_overcommitted", "HA overcommitted"),
                    ("ha_allow_overcommit", "HA allows overcommit"),
                ],
            );
            g.fields.push(field(
                "VM autostart",
                display(&pool["other_config"]["auto_poweron"]),
            ));
            g.fields.push(field(
                "Pool coordinator",
                if text(pool, "master") == host_ref {
                    "Yes"
                } else {
                    "No"
                },
            ));
            g.fields.extend(
                group(
                    "",
                    host,
                    &[
                        ("ha_statefiles", "HA state files"),
                        ("ha_network_peers", "HA network peers"),
                    ],
                )
                .fields,
            );
            groups.push(g);
        } else {
            warn(
                "General",
                "Pool / HA record was not uniquely available.",
                warnings,
            );
        }
    }
    let mut custom = InfoGroup {
        title: "Custom fields".into(),
        fields: vec![],
    };
    if let Some(config) = config.as_object() {
        for (key, value) in config {
            if let Some(label) = key.strip_prefix("XenCenter.CustomFields.") {
                custom.fields.push(field(label, display(value)));
            }
        }
    }
    if !custom.fields.is_empty() {
        groups.push(custom);
    }
    groups.push(InfoGroup { title:"Health coverage".into(),fields:vec![field("Checks","Host liveness, maintenance, storage attachment, certificate validity and update guidance; host alerts and crash dump metadata. Other legacy diagnostics are not evaluated.")] });
    if bool_value(&config["MAINTENANCE_MODE"]) == Some(true) {
        warn("Health", "Host is in maintenance mode.", warnings);
    }
    for key in ["pending_guidances", "pending_guidances_recommended"] {
        if let Some(guidances) = host[key].as_array() {
            for guidance in guidances {
                if guidance.as_str().is_some() {
                    warn(
                        "Updates",
                        format!("Pending host guidance: {}", display(guidance)),
                        warnings,
                    );
                }
            }
        }
    }
    groups.retain(|g| !g.fields.is_empty());
    groups
}
#[derive(Debug, PartialEq)]
struct MemoryTotals {
    xen: Option<u64>,
    dom0: Option<u64>,
    resident: Option<u64>,
    reservation: Option<u64>,
    available: Option<u64>,
}
fn integer(value: &Value) -> Option<u64> {
    value
        .as_u64()
        .or_else(|| value.as_str().and_then(|v| v.parse().ok()))
}
fn bool_value(value: &Value) -> Option<bool> {
    value.as_bool().or_else(|| match value.as_str()? {
        "true" => Some(true),
        "false" => Some(false),
        _ => None,
    })
}
fn aggregate_memory(
    host: &Value,
    host_ref: &str,
    metric: &Value,
    vms: &Records,
    vm_metrics: Option<&Records>,
    guests: Option<&Records>,
) -> MemoryTotals {
    let resident: Vec<_> = vms
        .iter()
        .filter(|(_, r)| text(r, "resident_on") == host_ref)
        .collect();
    // A missing advertised resident record could otherwise silently undercount memory.
    let complete = host["resident_VMs"].as_array().is_none_or(|refs| {
        refs.iter().all(|r| {
            r.as_str()
                .is_some_and(|r| resident.iter().any(|(key, _)| key.as_str() == r))
        })
    });
    let mut xen = integer(&host["memory_overhead"]);
    let mut dom0 = Some(0_u64);
    let mut used = Some(0_u64);
    let mut reservation = Some(0_u64);
    for (_, vm) in resident {
        xen = xen
            .zip(integer(&vm["memory_overhead"]))
            .and_then(|(a, b)| a.checked_add(b));
        let metric = related(vm_metrics, text(vm, "metrics"));
        let actual = integer(&metric["memory_actual"]);
        match vm["is_control_domain"].as_bool() {
            Some(true) => {
                xen = xen.zip(actual).and_then(|(a, b)| a.checked_add(b));
                dom0 = dom0
                    .zip(actual.or_else(|| integer(&vm["memory_dynamic_min"])))
                    .and_then(|(a, b)| a.checked_add(b));
            }
            Some(false) => {
                used = used.zip(actual).and_then(|(a, b)| a.checked_add(b));
                let guest_ref = text(vm, "guest_metrics");
                let guest = related(guests, guest_ref);
                let balloon = if vm["is_a_template"] == true {
                    integer(&vm["memory_dynamic_min"])
                        .zip(integer(&vm["memory_static_max"]))
                        .map(|(a, b)| a != b)
                } else if guest_ref == "OpaqueRef:NULL" {
                    Some(false)
                } else if guest.is_null() {
                    None
                } else {
                    guest["other"]
                        .as_object()
                        .map(|other| other.contains_key("feature-balloon"))
                };
                let minimum = balloon.and_then(|balloon| {
                    integer(
                        &vm[if balloon {
                            "memory_dynamic_min"
                        } else {
                            "memory_static_max"
                        }],
                    )
                });
                reservation = reservation.zip(minimum).and_then(|(a, b)| a.checked_add(b));
            }
            None => {
                xen = None;
                dom0 = None;
                used = None;
                reservation = None;
            }
        }
    }
    if vms
        .values()
        .filter(|vm| text(vm, "resident_on") == host_ref && vm["is_control_domain"] == true)
        .count()
        != 1
    {
        xen = None;
        dom0 = None;
    }
    if !complete {
        xen = None;
        dom0 = None;
        used = None;
        reservation = None;
    }
    let available = integer(&metric["memory_total"])
        .zip(reservation)
        .zip(xen)
        .map(|((total, min), xen)| total.saturating_sub(min).saturating_sub(xen));
    MemoryTotals {
        xen,
        dom0,
        resident: used,
        reservation,
        available,
    }
}
fn optional_bytes(value: Option<u64>) -> String {
    value.map_or_else(|| "Unavailable".into(), |v| bytes(&json!(v)))
}
fn memory_groups(
    host: &Value,
    host_ref: &str,
    metrics: &RecordRead,
    vms: &RecordRead,
    vm_metrics: &RecordRead,
    guests: &RecordRead,
    warnings: &mut Vec<SectionWarning>,
) -> Vec<InfoGroup> {
    let read = dependency(metrics, "Memory", "Host metrics", warnings);
    let metric = related(read, text(host, "metrics"));
    if read.is_some() && metric.is_null() {
        warn(
            "Memory",
            "The selected host's metrics record is unavailable.",
            warnings,
        );
    }
    let mut fields = vec![
        field("Total memory", bytes(&metric["memory_total"])),
        field("Free memory", bytes(&metric["memory_free"])),
        field("Host overhead", bytes(&host["memory_overhead"])),
    ];
    if let (Some(total), Some(free)) = (
        integer(&metric["memory_total"]),
        integer(&metric["memory_free"]),
    ) {
        if total >= free {
            fields.push(field("Used memory", bytes(&json!(total - free))));
            fields.push(field(
                "Memory utilisation",
                if total > 0 {
                    format!("{:.1}%", (total - free) as f64 / total as f64 * 100.0)
                } else {
                    "Unavailable".into()
                },
            ));
        } else {
            warn(
                "Memory",
                "Free memory exceeds total memory; physical usage is unavailable.",
                warnings,
            );
        }
    }
    fields.extend(
        group(
            "",
            metric,
            &[("live", "Host live"), ("last_updated", "Metrics updated")],
        )
        .fields,
    );
    if metric["live"] == false {
        warn(
            "Health",
            "Host is not live according to its metrics.",
            warnings,
        );
    }
    let vms = dependency(vms, "Memory", "Resident memory records", warnings);
    let vm_metrics = dependency(vm_metrics, "Memory", "Resident memory metrics", warnings);
    let guests = dependency(guests, "Memory", "Ballooning capability", warnings);
    let totals = vms
        .map(|vms| aggregate_memory(host, host_ref, metric, vms, vm_metrics, guests))
        .unwrap_or(MemoryTotals {
            xen: None,
            dom0: None,
            resident: None,
            reservation: None,
            available: None,
        });
    if [
        totals.xen,
        totals.dom0,
        totals.resident,
        totals.reservation,
        totals.available,
    ]
    .iter()
    .any(Option::is_none)
    {
        warn(
            "Memory",
            "Some aggregate memory inputs are missing; affected totals are unavailable.",
            warnings,
        );
    }
    let aggregate = InfoGroup {
        title: "Host memory allocation".into(),
        fields: vec![
            field("Available for placement", optional_bytes(totals.available)),
            field("Xen and control domain", optional_bytes(totals.xen)),
            field("Control domain memory", optional_bytes(totals.dom0)),
            field("Resident guest memory", optional_bytes(totals.resident)),
            field(
                "Resident minimum reservation",
                optional_bytes(totals.reservation),
            ),
        ],
    };
    vec![
        InfoGroup {
            title: "Physical host memory".into(),
            fields,
        },
        aggregate,
    ]
}

struct NetworkReads<'a> {
    metrics: &'a RecordRead,
    networks: &'a RecordRead,
    bonds: &'a RecordRead,
    pci: &'a RecordRead,
    vlans: &'a RecordRead,
    sriov: &'a RecordRead,
    capacities: &'a HashMap<String, Result<Value, ApiError>>,
}
fn local_pif(reference: &str, pifs: &Records, host_ref: &str) -> bool {
    pifs.get(reference)
        .is_some_and(|pif| on_host(pif, host_ref))
}
fn local_vlan(vlan: &Value, pifs: &Records, host_ref: &str) -> bool {
    local_pif(text(vlan, "tagged_PIF"), pifs, host_ref)
        && local_pif(text(vlan, "untagged_PIF"), pifs, host_ref)
}
fn local_sriov(config: &Value, pifs: &Records, host_ref: &str) -> bool {
    local_pif(text(config, "physical_PIF"), pifs, host_ref)
        && local_pif(text(config, "logical_PIF"), pifs, host_ref)
}
fn local_bond(bond: &Value, pifs: &Records, host_ref: &str) -> bool {
    local_pif(text(bond, "master"), pifs, host_ref)
        && bond["slaves"].as_array().is_some_and(|members| {
            members.iter().all(|member| {
                member
                    .as_str()
                    .is_some_and(|reference| local_pif(reference, pifs, host_ref))
            })
        })
        && (text(bond, "primary_slave").is_empty()
            || text(bond, "primary_slave") == "OpaqueRef:NULL"
            || local_pif(text(bond, "primary_slave"), pifs, host_ref))
}
// Resolve the same chain as PIF.NetworkSriov(), while rejecting cross-host
// relationships before any parent SR-IOV metadata can reach the selected host.
fn vlan_sriov_parent<'a>(
    reference: &str,
    pif: &Value,
    pifs: &'a Records,
    vlans: Option<&Records>,
    host_ref: &str,
) -> Result<Option<(&'a str, &'a Value)>, ()> {
    if integer(&pif["VLAN"]).is_none() {
        return Ok(None);
    }
    let vlan = vlans
        .and_then(|records| records.get(text(pif, "VLAN_master_of")))
        .ok_or(())?;
    if text(vlan, "untagged_PIF") != reference || !local_vlan(vlan, pifs, host_ref) {
        return Err(());
    }
    let (parent_ref, parent) = pifs.get_key_value(text(vlan, "tagged_PIF")).ok_or(())?;
    if !on_host(parent, host_ref) {
        return Err(());
    }
    if parent["sriov_logical_PIF_of"]
        .as_array()
        .is_some_and(|refs| !refs.is_empty())
    {
        Ok(Some((parent_ref.as_str(), parent)))
    } else {
        Ok(None)
    }
}
fn network_section(
    pifs: RecordRead,
    reads: NetworkReads<'_>,
    host_ref: &str,
    warnings: &mut Vec<SectionWarning>,
) -> DataSection {
    let NetworkReads {
        metrics,
        networks,
        bonds,
        pci,
        vlans,
        sriov,
        capacities,
    } = reads;
    let pifs = match pifs {
        Ok(r) => r,
        Err(e) => return failed_section("Networking", e),
    };
    let metrics = dependency(metrics, "Networking", "Interface metrics", warnings);
    let networks = dependency(networks, "Networking", "Networks", warnings);
    let bonds = dependency(bonds, "Networking", "Bonds", warnings);
    let pci = dependency(pci, "Networking", "PCI devices", warnings);
    let vlans = dependency(vlans, "Networking", "VLAN relationships", warnings);
    let sriov = dependency(sriov, "Networking", "SR-IOV configuration", warnings);
    let rows = pifs
        .iter()
        .filter(|(_, r)| on_host(r, host_ref))
        .map(|(reference, r)| {
            let metric = related(metrics, text(r, "metrics"));
            let network = related(networks, text(r, "network"));
            if metric.is_null() {
                warn(
                    "Networking",
                    "An interface metrics record is unavailable.",
                    warnings,
                );
            }
            if network.is_null() {
                warn(
                    "Networking",
                    "An interface network record is unavailable.",
                    warnings,
                );
            }
            let mut row = record(reference, &display(&r["device"]));
            row.values = values(
                r,
                &[
                    ("device", "Interface"),
                    ("MAC", "MAC"),
                    ("management", "Management"),
                    ("IP", "IPv4"),
                    ("IPv6", "IPv6"),
                    ("MTU", "MTU"),
                    ("VLAN", "VLAN"),
                    ("currently_attached", "Attached"),
                ],
            );
            row.values
                .insert("Network".into(), display(&network["name_label"]));
            row.values
                .insert("Link".into(), display(&metric["carrier"]));
            row.values
                .insert("Speed (Mbps)".into(), display(&metric["speed"]));
            row.groups.push(group(
                "Interface",
                r,
                &[
                    ("uuid", "UUID"),
                    ("device", "Device"),
                    ("device_name", "Device name"),
                    ("physical", "Physical"),
                    ("MAC", "MAC"),
                    ("management", "Management interface"),
                    ("currently_attached", "Attached"),
                    ("disallow_unplug", "Unplug prohibited"),
                    ("MTU", "MTU"),
                    ("VLAN", "VLAN"),
                    ("primary_address_type", "Primary address type"),
                    ("igmp_snooping_status", "IGMP snooping"),
                    ("managed", "Managed"),
                    ("capabilities", "Capabilities"),
                ],
            ));
            row.groups.push(group(
                "IPv4",
                r,
                &[
                    ("ip_configuration_mode", "Configuration"),
                    ("IP", "Address"),
                    ("netmask", "Netmask"),
                    ("gateway", "Gateway"),
                    ("DNS", "DNS"),
                ],
            ));
            row.groups.push(group(
                "IPv6",
                r,
                &[
                    ("ipv6_configuration_mode", "Configuration"),
                    ("IPv6", "Addresses"),
                    ("ipv6_gateway", "Gateway"),
                ],
            ));
            row.groups.push(group(
                "Network",
                network,
                &[
                    ("uuid", "UUID"),
                    ("name_label", "Name"),
                    ("name_description", "Description"),
                    ("bridge", "Bridge"),
                    ("MTU", "MTU"),
                    ("managed", "Managed"),
                ],
            ));
            row.groups.push(group(
                "Link and hardware",
                metric,
                &[
                    ("carrier", "Link connected"),
                    ("speed", "Speed (Mbps)"),
                    ("duplex", "Full duplex"),
                    ("vendor_id", "Vendor ID"),
                    ("vendor_name", "Vendor"),
                    ("device_id", "Device ID"),
                    ("device_name", "Model"),
                    ("pci_bus_path", "PCI address"),
                    ("last_updated", "Metrics updated"),
                ],
            ));
            row.groups.push(group(
                "Driver",
                &metric["other_config"],
                &[
                    ("driver_name", "Driver"),
                    ("driver_version", "Driver version"),
                    ("firmware_version", "Firmware version"),
                ],
            ));
            row.groups.push(group(
                "Offload and capabilities",
                &r["properties"],
                &[
                    ("gro", "Generic receive offload"),
                    ("lro", "Large receive offload"),
                    ("tso", "TCP segmentation offload"),
                    ("gso", "Generic segmentation offload"),
                    ("sg", "Scatter gather"),
                    ("tx", "TX checksum"),
                    ("rx", "RX checksum"),
                ],
            ));
            row.groups.push(group(
                "Network purpose",
                &r["other_config"],
                &[
                    ("management_purpose", "Purpose"),
                    ("management", "Secondary management"),
                    ("storage", "Storage traffic"),
                ],
            ));
            let mut capabilities = InfoGroup {
                title: "FCoE and SR-IOV".into(),
                fields: vec![],
            };
            for (key, label) in [("fcoe", "FCoE capable"), ("sriov", "SR-IOV capable")] {
                capabilities.fields.push(field(
                    label,
                    if r["capabilities"].is_array() {
                        if includes(r, "capabilities", key) {
                            "Yes"
                        } else {
                            "No"
                        }
                    } else {
                        "Unavailable"
                    },
                ));
            }
            if let Some(vlans) = vlans {
                for vlan in vlans.values().filter(|v| {
                    text(v, "tagged_PIF") == reference || text(v, "untagged_PIF") == reference
                }) {
                    if !local_vlan(vlan,&pifs,host_ref) {
                        warn("Networking","A VLAN relationship has missing or foreign-host endpoints; its metadata is unavailable.",warnings);
                        continue;
                    }
                    let mut g = group(
                        "VLAN relationship",
                        vlan,
                        &[("uuid", "UUID"), ("tag", "VLAN tag")],
                    );
                    for (key, label) in [
                        ("tagged_PIF", "Parent interface"),
                        ("untagged_PIF", "VLAN interface"),
                    ] {
                        g.fields.push(field(
                            label,
                            pifs.get(text(vlan, key))
                                .filter(|p| on_host(p, host_ref))
                                .map_or_else(|| "Unavailable".into(), |p| display(&p["device"])),
                        ));
                    }
                    row.groups.push(g);
                }
            }
            let vlan_parent = match vlan_sriov_parent(reference,r,&pifs,vlans,host_ref) {
                Ok(parent) => parent,
                Err(()) => {
                    warn("Networking","A VLAN parent relationship is unavailable or belongs to another host; inherited link status is unavailable.",warnings);
                    row.values.insert("Link".into(),"Unavailable".into());
                    None
                }
            };
            if let Some(sriov) = sriov {
                let mut configured = false;
                let mut invalid_configuration = false;
                for (sriov_ref, config) in sriov {
                    let inherited = vlan_parent.is_some_and(|(parent_ref,parent)| {
                        text(config,"logical_PIF") == parent_ref
                            && includes(parent,"sriov_logical_PIF_of",sriov_ref)
                    });
                    if text(config, "physical_PIF") != reference
                        && text(config, "logical_PIF") != reference
                        && !inherited
                    {
                        continue;
                    }
                    if !local_sriov(config,&pifs,host_ref) {
                        invalid_configuration = true;
                        warn("Networking","An SR-IOV relationship has missing or foreign-host endpoints; its metadata is unavailable.",warnings);
                        row.values.insert("Link".into(),"Unavailable".into());
                        continue;
                    }
                    configured = true;
                    let mut g = group(
                        "SR-IOV configuration",
                        config,
                        &[
                            ("uuid", "UUID"),
                            ("requires_reboot", "Host reboot required"),
                            ("configuration_mode", "Configuration mode"),
                        ],
                    );
                    if inherited { g.fields.push(field("Applies through","VLAN parent interface")); }
                    let logical = pifs
                        .get(text(config, "logical_PIF"))
                        .filter(|p| on_host(p, host_ref));
                    g.fields.push(field(
                        "Logical interface",
                        logical.map_or_else(|| "Unavailable".into(), |p| display(&p["device"])),
                    ));
                    g.fields.push(field(
                        "Logical interface attached",
                        logical.map_or_else(
                            || "Unavailable".into(),
                            |p| display(&p["currently_attached"]),
                        ),
                    ));
                    let capacity = match capacities.get(sriov_ref) {
                        Some(Ok(v)) => {
                            integer(v).map_or_else(|| "Unavailable".into(), |n| n.to_string())
                        }
                        Some(Err(e)) => {
                            warn(
                                "Networking",
                                format!("SR-IOV remaining capacity: {e}"),
                                warnings,
                            );
                            "Unavailable".into()
                        }
                        None => "Unavailable".into(),
                    };
                    g.fields
                        .push(field("Remaining virtual functions", capacity));
                    if (config["requires_reboot"] == true
                        || text(config, "configuration_mode") == "unknown"
                        || logical.is_some_and(|p| p["currently_attached"] == false))
                        && (text(config, "logical_PIF") == reference || inherited)
                    {
                        row.values.insert("Link".into(), "No".into());
                    }
                    row.groups.push(g);
                }
                if vlan_parent.is_some() && !configured && !invalid_configuration {
                    invalid_configuration = true;
                    warn("Networking","The VLAN parent's advertised SR-IOV configuration is unavailable.",warnings);
                    row.values.insert("Link".into(),"Unavailable".into());
                }
                capabilities.fields.push(field(
                    "SR-IOV configured",
                    if invalid_configuration { "Unavailable" } else if configured { "Yes" } else { "No" },
                ));
            } else {
                if vlan_parent.is_some() { row.values.insert("Link".into(),"Unavailable".into()); }
                capabilities.fields.push(field("SR-IOV configured", "Unavailable"));
            }
            row.groups.push(capabilities);
            let pci_record = related(pci, text(r, "PCI"));
            if !pci_record.is_null() && on_host(pci_record, host_ref) {
                row.groups.push(pci_group(pci_record));
            }
            if let Some(bonds) = bonds {
                for (bond_ref, bond) in bonds {
                    if text(bond, "master") != reference && !includes(bond, "slaves", reference) {
                        continue;
                    }
                    if !local_bond(bond,&pifs,host_ref) {
                        warn("Networking","A bond relationship has missing or foreign-host endpoints; its metadata is unavailable.",warnings);
                        continue;
                    }
                    let mut g = group(
                        "Bond",
                        bond,
                        &[("uuid", "UUID"), ("mode", "Mode"), ("links_up", "Links up")],
                    );
                    g.fields.push(field(
                        "Role",
                        if text(bond, "master") == reference {
                            "Master"
                        } else {
                            "Member"
                        },
                    ));
                    g.fields.push(field(
                        "Master",
                        pifs.get(text(bond, "master"))
                            .filter(|p| on_host(p, host_ref))
                            .map(|p| display(&p["device"]))
                            .unwrap_or_else(|| "Unavailable".into()),
                    ));
                    let members = bond["slaves"]
                        .as_array()
                        .into_iter()
                        .flatten()
                        .filter_map(|r| r.as_str())
                        .filter_map(|r| pifs.get(r))
                        .filter(|p| on_host(p, host_ref))
                        .map(|p| display(&p["device"]))
                        .collect::<Vec<_>>();
                    g.fields.push(field("Members", members.join(", ")));
                    g.fields.extend(
                        group(
                            "",
                            &bond["properties"],
                            &[
                                ("mode", "Mode"),
                                ("hashing_algorithm", "Hashing algorithm"),
                                ("lacp-time", "LACP time"),
                                ("lacp-fallback-ab", "LACP fallback"),
                            ],
                        )
                        .fields,
                    );
                    if text(bond, "uuid").is_empty() {
                        g.fields.push(field("Reference", bond_ref));
                    }
                    row.groups.push(g);
                }
            }
            row.groups.retain(|g| !g.fields.is_empty());
            row
        })
        .collect();
    ready(
        &[
            "Interface",
            "Network",
            "Management",
            "MAC",
            "IPv4",
            "IPv6",
            "MTU",
            "VLAN",
            "Attached",
            "Link",
            "Speed (Mbps)",
        ],
        rows,
    )
}

const SAFE_DEVICE_CONFIG: &[(&str, &str)] = &[
    ("server", "Server"),
    ("serverpath", "Server path"),
    ("device", "Device"),
    ("SCSIid", "SCSI ID"),
    ("target", "Target"),
    ("targetIQN", "Target IQN"),
    ("port", "Port"),
];
fn storage_section(
    pbds: RecordRead,
    srs: &RecordRead,
    host_ref: &str,
    host: &Value,
    metrics: &RecordRead,
    warnings: &mut Vec<SectionWarning>,
) -> DataSection {
    let pbds = match pbds {
        Ok(v) => v,
        Err(e) => return failed_section("Storage", e),
    };
    let srs = match srs {
        Ok(records) => records,
        Err(e) => {
            return DataSection::Unavailable {
                message: format!("Storage repository metadata is unavailable: {e}"),
            };
        }
    };
    let host_live = metrics
        .as_ref()
        .ok()
        .and_then(|r| r.get(text(host, "metrics")))
        .and_then(|m| m["live"].as_bool());
    let mut missing = false;
    let rows: Vec<_> = pbds
        .iter()
        .filter(|(_, r)| on_host(r, host_ref))
        .filter_map(|(reference, pbd)| {
            let Some(sr) = srs.get(text(pbd, "SR")) else {
                missing = true;
                warn(
                    "Storage",
                    "A host attachment references unavailable repository metadata.",
                    warnings,
                );
                return None;
            };
            if hidden_sr(sr) {
                return None;
            }
            let name = if sr.is_null() {
                "Unavailable storage repository".into()
            } else {
                display(&sr["name_label"])
            };
            let mut row = record(reference, &name);
            row.values = values(
                sr,
                &[
                    ("name_label", "Name"),
                    ("type", "Type"),
                    ("shared", "Shared"),
                ],
            );
            row.values
                .insert("Attached".into(), display(&pbd["currently_attached"]));
            let status = match (pbd["currently_attached"].as_bool(), host_live) {
                (Some(false), _) => "Unplugged",
                (Some(true), Some(false)) => "Host not live",
                (Some(true), Some(true)) => "Connected",
                _ => "Unavailable",
            };
            row.values.insert("Status".into(), status.into());
            if status == "Unplugged" {
                warn(
                    "Health",
                    "A host storage attachment is unplugged.",
                    warnings,
                );
            }
            row.values
                .insert("Capacity".into(), bytes(&sr["physical_size"]));
            row.values
                .insert("Used".into(), bytes(&sr["physical_utilisation"]));
            row.values.insert(
                "Virtual allocation".into(),
                bytes(&sr["virtual_allocation"]),
            );
            if let (Some(total), Some(used)) = (
                numeric(&sr["physical_size"]),
                numeric(&sr["physical_utilisation"]),
            ) && total >= used
            {
                row.values
                    .insert("Free".into(), bytes(&json!(total - used)));
            }
            row.groups.push(group(
                "Storage repository",
                sr,
                &[
                    ("uuid", "UUID"),
                    ("name_label", "Name"),
                    ("name_description", "Description"),
                    ("type", "Type"),
                    ("content_type", "Content type"),
                    ("shared", "Shared"),
                    ("physical_size", "Capacity (bytes)"),
                    ("physical_utilisation", "Used (bytes)"),
                    ("virtual_allocation", "Virtual allocation (bytes)"),
                    ("clustered", "Clustered"),
                    ("is_tools_sr", "Guest tools repository"),
                ],
            ));
            row.groups.push(group(
                "Host attachment",
                pbd,
                &[("uuid", "PBD UUID"), ("currently_attached", "Attached")],
            ));
            row.groups.push(group(
                "Connection",
                &pbd["device_config"],
                SAFE_DEVICE_CONFIG,
            ));
            row.groups.push(group(
                "Multipathing",
                &pbd["other_config"],
                &[
                    ("multipathed", "Multipathed"),
                    ("multipath", "Multipath"),
                    ("iscsi_sessions", "iSCSI sessions"),
                ],
            ));
            row.groups.push(multipath_group(&pbd["other_config"]));
            row.groups.retain(|g| !g.fields.is_empty());
            Some(row)
        })
        .collect();
    if missing && rows.is_empty() {
        return DataSection::Unavailable {
            message: "Host storage references could not be resolved.".into(),
        };
    }
    ready(
        &[
            "Name",
            "Type",
            "Shared",
            "Attached",
            "Status",
            "Capacity",
            "Used",
            "Free",
            "Virtual allocation",
        ],
        rows,
    )
}
fn pci_group(r: &Value) -> InfoGroup {
    group(
        "PCI device",
        r,
        &[
            ("uuid", "UUID"),
            ("pci_id", "Address"),
            ("vendor_id", "Vendor ID"),
            ("vendor_name", "Vendor"),
            ("device_id", "Device ID"),
            ("device_name", "Device"),
            ("class_id", "Class ID"),
            ("class_name", "Class"),
            ("subsystem_vendor_id", "Subsystem vendor ID"),
            ("subsystem_vendor_name", "Subsystem vendor"),
            ("subsystem_device_id", "Subsystem device ID"),
            ("subsystem_device_name", "Subsystem device"),
            ("functions", "Functions"),
            ("driver_name", "Driver"),
            ("dependencies", "Dependent PCI references"),
        ],
    )
}
fn gpu_section(
    pgpus: RecordRead,
    pci: &RecordRead,
    types: &RecordRead,
    host_ref: &str,
    warnings: &mut Vec<SectionWarning>,
) -> DataSection {
    let pgpus = match pgpus {
        Ok(v) => v,
        Err(e) => return failed_section("GPUs", e),
    };
    let pci = dependency(pci, "GPUs", "PCI devices", warnings);
    let types = dependency(types, "GPUs", "Virtual GPU types", warnings);
    let rows = pgpus
        .iter()
        .filter(|(_, r)| on_host(r, host_ref))
        .map(|(reference, r)| {
            let device = related(pci, text(r, "PCI"));
            let device = if on_host(device, host_ref) {
                device
            } else {
                &Value::Null
            };
            if device.is_null() {
                warn(
                    "GPUs",
                    "A physical GPU references unavailable PCI metadata.",
                    warnings,
                );
            }
            let mut row = record(reference, &display(&device["device_name"]));
            row.values = values(
                device,
                &[
                    ("pci_id", "Address"),
                    ("vendor_name", "Vendor"),
                    ("device_name", "Device"),
                ],
            );
            row.values
                .insert("Dom0 access".into(), display(&r["dom0_access"]));
            row.values.insert(
                "System display".into(),
                display(&r["is_system_display_device"]),
            );
            row.groups.push(group(
                "Physical GPU",
                r,
                &[
                    ("uuid", "UUID"),
                    ("dom0_access", "Dom0 access"),
                    ("is_system_display_device", "System display device"),
                    ("size", "Size"),
                ],
            ));
            row.groups.push(pci_group(device));
            let mut enabled_names = Vec::new();
            if let Some(types) = types {
                for (type_ref, t) in types {
                    if !includes(r, "supported_VGPU_types", type_ref)
                        && !includes(r, "enabled_VGPU_types", type_ref)
                    {
                        continue;
                    }
                    let enabled = includes(r, "enabled_VGPU_types", type_ref);
                    if enabled {
                        enabled_names.push(display(&t["model_name"]));
                    }
                    let mut g = group(
                        &format!("vGPU type: {}", display(&t["model_name"])),
                        t,
                        &[
                            ("uuid", "UUID"),
                            ("vendor_name", "Vendor"),
                            ("model_name", "Model"),
                            ("framebuffer_size", "Framebuffer (bytes)"),
                            ("max_heads", "Maximum heads"),
                            ("max_resolution_x", "Maximum width"),
                            ("max_resolution_y", "Maximum height"),
                            ("size", "Size"),
                            ("implementation", "Implementation"),
                            ("experimental", "Experimental"),
                        ],
                    );
                    g.fields
                        .push(field("Enabled", if enabled { "Yes" } else { "No" }));
                    if let Some(capacity) = r["supported_VGPU_max_capacities"].get(type_ref) {
                        g.fields.push(field("Maximum capacity", display(capacity)));
                    }
                    row.groups.push(g);
                }
            }
            let missing_type = r["enabled_VGPU_types"].as_array().is_none_or(|refs| {
                refs.iter().any(|r| {
                    r.as_str()
                        .is_none_or(|r| types.is_none_or(|types| !types.contains_key(r)))
                })
            });
            if missing_type {
                warn(
                    "GPUs",
                    "Enabled virtual GPU type information is incomplete.",
                    warnings,
                );
            }
            enabled_names.sort();
            row.values.insert(
                "Enabled vGPU types".into(),
                if types.is_none() || missing_type {
                    "Unavailable".into()
                } else if enabled_names.is_empty() {
                    "None".into()
                } else {
                    enabled_names.join(", ")
                },
            );
            row.groups.retain(|g| !g.fields.is_empty());
            row
        })
        .collect();
    ready(
        &[
            "Address",
            "Vendor",
            "Device",
            "Dom0 access",
            "System display",
            "Enabled vGPU types",
        ],
        rows,
    )
}
fn certificate_section(
    read: RecordRead,
    host_ref: &str,
    host: &Value,
    warnings: &mut Vec<SectionWarning>,
) -> DataSection {
    match read {
        Err(e) => failed_section("Certificates", e),
        Ok(records) => {
            let belongs = |reference: &str, r: &Value| {
                on_host(r, host_ref)
                    || ((text(r, "host").is_empty() || text(r, "host") == "OpaqueRef:NULL")
                        && includes(host, "certificates", reference))
            };
            let missing = host["certificates"].as_array().is_some_and(|refs| {
                refs.iter().any(|r| {
                    r.as_str().is_none_or(|reference| {
                        records
                            .get(reference)
                            .is_none_or(|record| !belongs(reference, record))
                    })
                })
            });
            if missing {
                warn(
                    "Certificates",
                    "A host certificate reference could not be resolved.",
                    warnings,
                );
            }
            let section = ready(
                &["Name", "Type", "Valid from", "Expires", "Fingerprint"],
                records
                    .iter()
                    .filter(|(reference, r)| belongs(reference, r))
                    .map(|(reference, r)| {
                        let mut row = record(reference, &display(&r["type"]));
                        row.values = values(
                            r,
                            &[
                                ("name", "Name"),
                                ("type", "Type"),
                                ("not_before", "Valid from"),
                                ("not_after", "Expires"),
                                ("fingerprint", "Fingerprint"),
                            ],
                        );
                        row.groups.push(group(
                            "Public certificate metadata",
                            r,
                            &[
                                ("uuid", "UUID"),
                                ("name", "Name"),
                                ("type", "Type"),
                                ("not_before", "Valid from"),
                                ("not_after", "Expires"),
                                ("fingerprint", "Fingerprint"),
                                ("fingerprint_sha256", "SHA-256 fingerprint"),
                                ("fingerprint_sha1", "SHA-1 fingerprint"),
                                ("subject", "Subject"),
                                ("issuer", "Issuer"),
                                ("serial_number", "Serial number"),
                            ],
                        ));
                        row
                    })
                    .collect(),
            );
            if missing && matches!(&section,DataSection::Ready{rows,..} if rows.is_empty()) {
                DataSection::Unavailable {
                    message: "Host certificate references could not be resolved.".into(),
                }
            } else {
                section
            }
        }
    }
}
fn update_section(
    host_patches: RecordRead,
    pool_patches: &RecordRead,
    updates: RecordRead,
    host_ref: &str,
    host: &Value,
    warnings: &mut Vec<SectionWarning>,
) -> DataSection {
    // Old servers expose host_patch; newer ones expose pool_update. A failed source
    // is always visible even when the other source succeeds with no applied updates.
    if let (Err(patches_error), Err(updates_error)) = (&host_patches, &updates) {
        if matches!(patches_error, ApiError::Unsupported)
            && matches!(updates_error, ApiError::Unsupported)
        {
            return failed_section("Updates", ApiError::Unsupported);
        }
        return DataSection::Unavailable {
            message: format!(
                "Could not read update sources. Host patches: {patches_error}; pool updates: {updates_error}"
            ),
        };
    }
    let mut rows = Vec::new();
    match host_patches {
        Err(e) => warn("Updates", format!("Host patches: {e}"), warnings),
        Ok(patches) => {
            let patches: Vec<_> = patches
                .iter()
                .filter(|(_, r)| on_host(r, host_ref) && r["applied"] == true)
                .collect();
            let definitions = if patches.is_empty() {
                None
            } else {
                dependency(pool_patches, "Updates", "Patch definitions", warnings)
            };
            for (reference, r) in patches {
                let patch = related(definitions, text(r, "pool_patch"));
                if patch.is_null() {
                    warn(
                        "Updates",
                        "An applied patch definition is unavailable; its guidance cannot be evaluated.",
                        warnings,
                    );
                }
                let mut row = record(reference, &display(&patch["name_label"]));
                row.values = values(
                    patch,
                    &[
                        ("name_label", "Name"),
                        ("version", "Version"),
                        ("after_apply_guidance", "Guidance"),
                    ],
                );
                row.values.insert("Source".into(), "Host patch".into());
                row.values.insert("Applied".into(), display(&r["applied"]));
                row.values
                    .insert("Timestamp".into(), display(&r["timestamp_applied"]));
                row.groups.push(group(
                    "Patch",
                    patch,
                    &[
                        ("uuid", "UUID"),
                        ("name_label", "Name"),
                        ("name_description", "Description"),
                        ("version", "Version"),
                        ("size", "Size (bytes)"),
                        ("after_apply_guidance", "After-apply guidance"),
                    ],
                ));
                row.groups.push(group(
                    "Host application",
                    r,
                    &[
                        ("uuid", "Application UUID"),
                        ("applied", "Applied"),
                        ("timestamp_applied", "Applied at"),
                        ("size", "Size (bytes)"),
                    ],
                ));
                if ["restartHost", "restartXAPI"]
                    .iter()
                    .any(|g| includes(patch, "after_apply_guidance", g))
                    && (timestamp(&r["timestamp_applied"]).is_none()
                        || timestamp(&host["other_config"]["boot_time"]).is_none()
                        || timestamp(&host["other_config"]["agent_start_time"]).is_none())
                {
                    warn(
                        "Updates",
                        "Patch restart guidance cannot be fully evaluated because application or host start time is unavailable.",
                        warnings,
                    );
                }
                let pending = pending_patch_guidance(patch, r, host);
                for message in &pending {
                    warn("Updates", *message, warnings);
                }
                row.groups.push(InfoGroup {
                    title: "Pending action".into(),
                    fields: pending
                        .into_iter()
                        .map(|p| field("Required action", p))
                        .collect(),
                });
                rows.push(row);
            }
        }
    }
    match updates {
        Err(e) => warn("Updates", format!("Pool updates: {e}"), warnings),
        Ok(updates) => {
            if host["updates_requiring_reboot"]
                .as_array()
                .is_some_and(|refs| {
                    refs.iter()
                        .any(|r| r.as_str().is_none_or(|r| !updates.contains_key(r)))
                })
            {
                warn(
                    "Updates",
                    "Host restart is required, but some pending update metadata is unavailable.",
                    warnings,
                );
            }
            for (reference, r) in updates {
                if !includes(&r, "hosts", host_ref)
                    && !includes(host, "updates", &reference)
                    && !includes(host, "updates_requiring_reboot", &reference)
                {
                    continue;
                }
                let mut row = record(&reference, &display(&r["name_label"]));
                row.values = values(
                    &r,
                    &[
                        ("name_label", "Name"),
                        ("version", "Version"),
                        ("after_apply_guidance", "Guidance"),
                    ],
                );
                row.values.insert("Source".into(), "Pool update".into());
                row.values.insert(
                    "Applied".into(),
                    if includes(&r, "hosts", host_ref) || includes(host, "updates", &reference) {
                        "Yes"
                    } else {
                        "Unknown"
                    }
                    .into(),
                );
                row.values.insert("Timestamp".into(), "—".into());
                let mut g = group(
                    "Update",
                    &r,
                    &[
                        ("uuid", "UUID"),
                        ("name_label", "Name"),
                        ("name_description", "Description"),
                        ("version", "Version"),
                        ("installation_size", "Installation size (bytes)"),
                        ("key", "Signing key identifier"),
                        ("after_apply_guidance", "After-apply guidance"),
                        ("enforce_homogeneity", "Enforce homogeneity"),
                    ],
                );
                g.fields.push(field(
                    "Host reboot required",
                    if includes(host, "updates_requiring_reboot", &reference) {
                        "Yes"
                    } else {
                        "No"
                    },
                ));
                if includes(host, "updates_requiring_reboot", &reference) {
                    warn(
                        "Updates",
                        "Host restart required by an applied update.",
                        warnings,
                    );
                }
                row.groups.push(g);
                rows.push(row);
            }
        }
    }
    ready(
        &[
            "Name",
            "Version",
            "Source",
            "Applied",
            "Timestamp",
            "Guidance",
        ],
        rows,
    )
}

fn hidden_sr(sr: &Value) -> bool {
    bool_value(&sr["is_tools_sr"]) == Some(true)
        || text(sr, "name_label") == "XenServer Tools"
        || text(sr, "name_label").starts_with("__gui__")
        || matches!(text(sr, "type"), "local" | "tmpfs")
        || bool_value(&sr["other_config"]["HideFromXenCenter"]) == Some(true)
        || (!text(sr, "introduced_by").is_empty() && text(sr, "introduced_by") != "OpaqueRef:NULL")
}
fn path_counts(value: &Value) -> Option<(u64, u64)> {
    let value = value.as_str()?.trim().strip_prefix('[')?;
    let mut parts = value.split(',');
    let current = parts.next()?.trim().trim_end_matches('L').parse().ok()?;
    let maximum = parts
        .next()?
        .trim()
        .trim_end_matches(']')
        .trim()
        .trim_end_matches('L')
        .parse()
        .ok()?;
    Some((current, maximum))
}
fn path_health(current: u64, maximum: u64, sessions: Option<u64>) -> String {
    if maximum == 0 || current > maximum {
        return "Unavailable".into();
    }
    format!(
        "{current} / {maximum} paths ({})",
        if current < maximum || sessions.is_some_and(|s| maximum < s) {
            "Degraded"
        } else {
            "Healthy"
        }
    )
}
fn multipath_group(config: &Value) -> InfoGroup {
    let mut g = InfoGroup {
        title: "Path health".into(),
        fields: vec![],
    };
    if bool_value(&config["multipathed"]) == Some(false) {
        g.fields
            .push(field("Storage paths", "Multipathing inactive"));
        return g;
    }
    if let Some(map) = config.as_object() {
        // Never export arbitrary backend metadata: only parsed path counts.
        for (key, value) in map {
            if key.starts_with("mpath")
                && let Some((current, maximum)) = path_counts(value)
            {
                g.fields.push(field(
                    "Storage paths",
                    path_health(current, maximum, integer(&config["iscsi_sessions"])),
                ));
            }
        }
    }
    if g.fields.is_empty() {
        g.fields.push(field("Storage paths", "Unavailable"));
    }
    g
}
fn pending_patch_guidance(patch: &Value, application: &Value, host: &Value) -> Vec<&'static str> {
    if application["applied"] != true {
        return vec![];
    }
    let applied = timestamp(&application["timestamp_applied"]);
    let mut pending = Vec::new();
    for (guidance, key, label) in [
        ("restartHost", "boot_time", "Host restart required"),
        (
            "restartXAPI",
            "agent_start_time",
            "Toolstack restart required",
        ),
    ] {
        // Modern hosts explicitly list pending reboot updates. Keep the legacy path
        // for pre-pool_update servers and for toolstack restart checks on all versions.
        if guidance == "restartHost" && host.get("updates_requiring_reboot").is_some() {
            continue;
        }
        if includes(patch, "after_apply_guidance", guidance)
            && applied
                .zip(timestamp(&host["other_config"][key]))
                .is_some_and(|(applied, start)| start > 0 && applied > start)
        {
            pending.push(label);
        }
    }
    pending
}

#[cfg(test)]
mod tests {
    use super::*;

    fn records(values: Value) -> Records {
        serde_json::from_value(values).unwrap()
    }
    fn memory_fixture() -> (Value, Value, Records, Records, Records) {
        (
            json!({"memory_overhead":100,"resident_VMs":["dom0","balloon","fixed"]}),
            json!({"memory_total":10000}),
            records(json!({
                "dom0":{"resident_on":"host-a","is_control_domain":true,"memory_overhead":10,"metrics":"m0","memory_dynamic_min":900},
                "balloon":{"resident_on":"host-a","is_control_domain":false,"memory_overhead":20,"metrics":"m1","guest_metrics":"g1","memory_dynamic_min":1000,"memory_static_max":3000},
                "fixed":{"resident_on":"host-a","is_control_domain":false,"memory_overhead":30,"metrics":"m2","guest_metrics":"g2","memory_dynamic_min":500,"memory_static_max":2000},
                "foreign":{"resident_on":"host-b","is_control_domain":true,"memory_overhead":99999,"metrics":"foreign"}
            })),
            records(
                json!({"m0":{"memory_actual":1000},"m1":{"memory_actual":2500},"m2":{"memory_actual":1500},"foreign":{"memory_actual":99999}}),
            ),
            records(json!({"g1":{"other":{"feature-balloon":"0"}},"g2":{"other":{}}})),
        )
    }
    #[test]
    fn memory_matches_legacy_formulas_without_exposing_guest_rows() {
        let (host, metric, vms, metrics, guests) = memory_fixture();
        assert_eq!(
            aggregate_memory(
                &host,
                "host-a",
                &metric,
                &vms,
                Some(&metrics),
                Some(&guests)
            ),
            MemoryTotals {
                xen: Some(1160),
                dom0: Some(1000),
                resident: Some(4000),
                reservation: Some(3000),
                available: Some(5840),
            }
        );
        let mut warnings = vec![];
        let groups = memory_groups(
            &json!({"metrics":"hm","memory_overhead":100,"resident_VMs":["dom0","balloon","fixed"]}),
            "host-a",
            &Ok(records(json!({"hm":metric}))),
            &Ok(vms),
            &Ok(metrics),
            &Ok(guests),
            &mut warnings,
        );
        let output = serde_json::to_string(&groups).unwrap();
        for identity in ["balloon", "fixed", "foreign", "host-b", "guest_metrics"] {
            assert!(!output.contains(identity));
        }
    }
    #[test]
    fn absent_memory_inputs_cannot_inflate_available_capacity() {
        let (host, metric, mut vms, mut metrics, guests) = memory_fixture();
        metrics.remove("m0");
        let totals = aggregate_memory(
            &host,
            "host-a",
            &metric,
            &vms,
            Some(&metrics),
            Some(&guests),
        );
        assert_eq!(totals.xen, None);
        assert_eq!(totals.available, None);
        assert_eq!(totals.dom0, Some(900)); // Original dom0_memory fallback only.
        vms.remove("balloon");
        assert_eq!(
            aggregate_memory(
                &host,
                "host-a",
                &metric,
                &vms,
                Some(&metrics),
                Some(&guests)
            )
            .resident,
            None
        );
        let (host, metric, vms, metrics, _) = memory_fixture();
        let totals = aggregate_memory(&host, "host-a", &metric, &vms, Some(&metrics), None);
        assert_eq!(totals.reservation, None);
        assert_eq!(totals.available, None);
        assert_eq!(bytes(&json!(-1)), "Unavailable");
        assert_eq!(bytes(&Value::Null), "Unavailable");
        assert_eq!(bytes(&json!("NaN")), "Unavailable");
        assert_eq!(bytes(&json!(0)), "0.00 B");
    }
    #[test]
    fn placement_headroom_clamps_at_zero() {
        let (host, _, vms, metrics, guests) = memory_fixture();
        assert_eq!(
            aggregate_memory(
                &host,
                "host-a",
                &json!({"memory_total":1000}),
                &vms,
                Some(&metrics),
                Some(&guests)
            )
            .available,
            Some(0)
        );
    }
    #[test]
    fn general_allowlist_redacts_secrets_and_uses_server_clock_and_modern_iqn() {
        let host = json!({"iscsi_iqn":"iqn:modern","power_on_mode":"wake-on-lan","power_on_config":{"password":"PRIVATE_POWER"},"external_auth_configuration":{"password":"PRIVATE_AUTH"},"license_params":{"edition":"standard","license_key":"PRIVATE_LICENSE"},"logging":{},"other_config":{"iscsi_iqn":"iqn:legacy","boot_time":"1000.0","agent_start_time":"1100","secret":"PRIVATE_OTHER","XenCenter.CustomFields.Rack":"R1"}});
        let groups = general_groups(
            &host,
            "host-a",
            &Ok(records(
                json!({"pool":{"master":"host-a","other_config":{"auto_poweron":"true"}}}),
            )),
            &Ok(json!(2000)),
            &mut vec![],
        );
        let output = serde_json::to_string(&groups).unwrap();
        for secret in [
            "PRIVATE_POWER",
            "PRIVATE_AUTH",
            "PRIVATE_LICENSE",
            "PRIVATE_OTHER",
            "iqn:legacy",
        ] {
            assert!(!output.contains(secret));
        }
        for expected in ["iqn:modern", "R1", "0d 0h 16m", "Local only"] {
            assert!(output.contains(expected), "{expected}");
        }
        assert_eq!(uptime(Some(999), &json!(1000)), "Unavailable");
        assert_eq!(uptime(None, &json!(1000)), "Unavailable");
    }
    #[test]
    fn storage_filters_hidden_foreign_and_tools_and_redacts_attachment_config() {
        let pbds = records(json!({
            "a":{"host":"host-a","SR":"sr-a","currently_attached":true,"device_config":{"server":"192.0.2.1","password":"PRIVATE_PASSWORD","chapuser":"PRIVATE_CHAP"}},
            "b":{"host":"host-b","SR":"sr-b"},"tools":{"host":"host-a","SR":"tools"},"hidden":{"host":"host-a","SR":"hidden"}
        }));
        let srs = records(
            json!({"sr-a":{"name_label":"Data","physical_size":"10000","physical_utilisation":"1000","virtual_allocation":"20000"},"sr-b":{"name_label":"FOREIGN"},"tools":{"is_tools_sr":true},"hidden":{"other_config":{"HideFromXenCenter":"true"}}}),
        );
        let section = storage_section(
            Ok(pbds),
            &Ok(srs),
            "host-a",
            &Value::Null,
            &Ok(Records::new()),
            &mut vec![],
        );
        let DataSection::Ready { rows, .. } = &section else {
            panic!()
        };
        assert_eq!(rows.len(), 1);
        assert_ne!(rows[0].values["Used"], rows[0].values["Virtual allocation"]);
        let output = serde_json::to_string(&section).unwrap();
        for secret in ["PRIVATE_PASSWORD", "PRIVATE_CHAP", "FOREIGN"] {
            assert!(!output.contains(secret));
        }
        assert!(output.contains("192.0.2.1"));
    }
    #[test]
    fn missing_storage_dependency_is_not_a_phantom_or_an_empty_success() {
        let pbds = || Ok(records(json!({"a":{"host":"host-a","SR":"missing"}})));
        assert!(matches!(
            storage_section(
                pbds(),
                &Err(ApiError::Connection),
                "host-a",
                &Value::Null,
                &Ok(Records::new()),
                &mut vec![]
            ),
            DataSection::Unavailable { .. }
        ));
        assert!(matches!(
            storage_section(
                pbds(),
                &Ok(Records::new()),
                "host-a",
                &Value::Null,
                &Ok(Records::new()),
                &mut vec![]
            ),
            DataSection::Unavailable { .. }
        ));
    }
    #[test]
    fn method_absence_permission_denial_and_empty_success_are_distinct() {
        assert!(matches!(
            failed_section("USB", ApiError::Unsupported),
            DataSection::Unsupported { .. }
        ));
        assert!(matches!(
            failed_section("USB", ApiError::Xen("Permission denied".into())),
            DataSection::Unavailable { .. }
        ));
        assert!(
            matches!(simple_section(Ok(Records::new()),"host-a","USB",&[],|_,_|unreachable!()),DataSection::Ready {rows,..} if rows.is_empty())
        );
        assert!(
            matches!(simple_section(Ok(records(json!({"b":{"host":"host-b"}}))),"host-a","USB",&[],|_,_|panic!("foreign host must never map")),DataSection::Ready {rows,..} if rows.is_empty())
        );
    }
    #[test]
    fn patch_guidance_compares_application_time_to_boot_and_agent_start() {
        let patch = json!({"after_apply_guidance":["restartHost","restartXAPI"]});
        let application = json!({"applied":true,"timestamp_applied":"2000"});
        let mut host = json!({"other_config":{"boot_time":"1000","agent_start_time":"1500"}});
        assert_eq!(
            pending_patch_guidance(&patch, &application, &host),
            vec!["Host restart required", "Toolstack restart required"]
        );
        host["updates_requiring_reboot"] = json!([]);
        assert_eq!(
            pending_patch_guidance(&patch, &application, &host),
            vec!["Toolstack restart required"]
        );
        host["other_config"]["agent_start_time"] = json!(2500);
        assert!(pending_patch_guidance(&patch, &application, &host).is_empty());
        assert!(
            pending_patch_guidance(
                &patch,
                &json!({"applied":false,"timestamp_applied":2000}),
                &host
            )
            .is_empty()
        );
    }
    #[test]
    fn multipath_metadata_exports_only_valid_counts() {
        assert_eq!(path_counts(&json!("[2L, 4L, 99]")), Some((2, 4)));
        assert_eq!(path_counts(&json!("[2, 4]")), Some((2, 4)));
        assert_eq!(path_counts(&json!("PRIVATE")), None);
        assert_eq!(path_health(0, 0, None), "Unavailable");
        assert_eq!(path_health(4, 2, None), "Unavailable");
        assert!(path_health(0, 2, None).contains("Degraded"));
        let output = serde_json::to_string(&multipath_group(
            &json!({"mpath-secret":"PRIVATE","mpath-good":"[2, 4]","password":"PRIVATE_PASSWORD"}),
        ))
        .unwrap();
        assert!(output.contains("Degraded"));
        assert!(!output.contains("PRIVATE"));
    }
    #[test]
    fn certificates_are_host_scoped_public_only_and_missing_refs_are_not_empty() {
        let host = json!({"certificates":["mine","foreign"]});
        let mut warnings = vec![];
        let section = certificate_section(
            Ok(records(
                json!({"mine":{"host":"host-a","type":"host","fingerprint":"PUBLIC_SHA256","private_key":"PRIVATE_KEY"},"foreign":{"host":"host-b","fingerprint":"FOREIGN"}}),
            )),
            "host-a",
            &host,
            &mut warnings,
        );
        let DataSection::Ready { rows, .. } = &section else {
            panic!()
        };
        assert_eq!(rows.len(), 1);
        let output = serde_json::to_string(&section).unwrap();
        assert!(output.contains("PUBLIC_SHA256"));
        assert!(!output.contains("PRIVATE_KEY"));
        assert!(!output.contains("FOREIGN"));
        assert!(!warnings.is_empty());
        assert!(matches!(
            certificate_section(Ok(Records::new()), "host-a", &host, &mut vec![]),
            DataSection::Unavailable { .. }
        ));
    }
    #[test]
    fn maintenance_uses_original_uppercase_key_and_distinguishes_missing() {
        for (value, expected, warning) in [
            (json!("true"), "Yes", true),
            (json!("false"), "No", false),
            (Value::Null, "Unavailable", false),
        ] {
            let host = json!({"other_config":{"MAINTENANCE_MODE":value,"maintenance_mode":"true"}});
            let mut warnings = vec![];
            let groups = general_groups(
                &host,
                "host-a",
                &Ok(Records::new()),
                &Ok(json!(2000)),
                &mut warnings,
            );
            let actual = groups
                .iter()
                .flat_map(|g| &g.fields)
                .find(|f| f.label == "Maintenance mode")
                .unwrap();
            assert_eq!(actual.value, expected);
            assert_eq!(
                warnings
                    .iter()
                    .any(|w| w.message == "Host is in maintenance mode."),
                warning
            );
        }
    }
    #[test]
    fn vlan_inherits_same_host_sriov_reboot_and_unknown_mode_link_state() {
        for (requires_reboot, mode, expected) in [
            (true, "sysfs", "No"),
            (false, "unknown", "No"),
            (false, "sysfs", "Yes"),
        ] {
            let section = vlan_sriov_fixture("host-a", requires_reboot, mode);
            let DataSection::Ready { rows, .. } = section else {
                panic!()
            };
            let vlan = rows.iter().find(|r| r.id == "vlan-pif").unwrap();
            assert_eq!(vlan.values["Link"], expected);
            assert!(
                vlan.groups
                    .iter()
                    .any(|g| g.title == "SR-IOV configuration")
            );
            assert!(
                vlan.groups
                    .iter()
                    .flat_map(|g| &g.fields)
                    .any(|f| f.label == "Applies through" && f.value == "VLAN parent interface")
            );
        }
    }
    #[test]
    fn vlan_never_inherits_cross_host_sriov_metadata_or_link_state() {
        let section = vlan_sriov_fixture("host-b", true, "unknown");
        let DataSection::Ready { rows, .. } = &section else {
            panic!()
        };
        let vlan = rows.iter().find(|r| r.id == "vlan-pif").unwrap();
        assert_eq!(vlan.values["Link"], "Unavailable");
        assert!(
            !vlan
                .groups
                .iter()
                .any(|g| g.title == "SR-IOV configuration")
        );
        assert!(
            !serde_json::to_string(&section)
                .unwrap()
                .contains("FOREIGN_SRIOV")
        );
    }
    fn vlan_sriov_fixture(parent_host: &str, requires_reboot: bool, mode: &str) -> DataSection {
        let pifs = Ok(records(json!({
            "vlan-pif":{"host":"host-a","device":"eth0.100","VLAN":"100","VLAN_master_of":"vlan","metrics":"metric","network":"network"},
            "logical":{"host":parent_host,"device":"logical-device","VLAN":"-1","sriov_logical_PIF_of":["sriov"],"currently_attached":true,"metrics":"metric","network":"network"},
            "physical":{"host":parent_host,"device":"physical-device","VLAN":"-1","metrics":"metric","network":"network"}
        })));
        let empty = Ok(Records::new());
        let metrics = Ok(records(json!({"metric":{"carrier":true}})));
        let networks = Ok(records(json!({"network":{"name_label":"Network"}})));
        let vlans = Ok(records(
            json!({"vlan":{"tagged_PIF":"logical","untagged_PIF":"vlan-pif","tag":100}}),
        ));
        let sriov = Ok(records(
            json!({"sriov":{"uuid":if parent_host=="host-a" {"LOCAL_SRIOV"} else {"FOREIGN_SRIOV"},"physical_PIF":"physical","logical_PIF":"logical","requires_reboot":requires_reboot,"configuration_mode":mode}}),
        ));
        network_section(
            pifs,
            NetworkReads {
                metrics: &metrics,
                networks: &networks,
                bonds: &empty,
                pci: &empty,
                vlans: &vlans,
                sriov: &sriov,
                capacities: &HashMap::new(),
            },
            "host-a",
            &mut vec![],
        )
    }
    #[test]
    fn advertised_primary_references_distinguish_missing_foreign_partial_and_empty() {
        for key in ["host_CPUs", "PIFs", "PBDs", "PGPUs", "PUSBs"] {
            let host = json!({key:["expected"]});
            let mut warnings = vec![];
            assert!(
                checked_host_records(
                    Ok(Records::new()),
                    &host,
                    key,
                    "host-a",
                    "Section",
                    &mut warnings
                )
                .is_err()
            );
            assert_eq!(warnings.len(), 1);
            assert!(
                checked_host_records(
                    Ok(records(
                        json!({"expected":{"host":"host-b","name_label":"FOREIGN"}})
                    )),
                    &host,
                    key,
                    "host-a",
                    "Section",
                    &mut vec![]
                )
                .is_err()
            );
            let mut warnings = vec![];
            let available = checked_host_records(
                Ok(records(json!({"another":{"host":"host-a"}}))),
                &host,
                key,
                "host-a",
                "Section",
                &mut warnings,
            )
            .unwrap();
            assert_eq!(available.len(), 1);
            assert_eq!(warnings.len(), 1);
            let mut warnings = vec![];
            let empty = checked_host_records(
                Ok(Records::new()),
                &json!({key:[]}),
                key,
                "host-a",
                "Section",
                &mut warnings,
            )
            .unwrap();
            assert!(empty.is_empty());
            assert!(warnings.is_empty());
        }
    }
    #[test]
    fn nested_network_relations_never_export_foreign_metadata() {
        let pifs = Ok(records(json!({
            "a":{"host":"host-a","device":"eth0","metrics":"m","network":"n","VLAN":-1},
            "b":{"host":"host-b","device":"FOREIGN_DEVICE","VLAN":-1}
        })));
        let metrics = Ok(records(json!({"m":{"carrier":true}})));
        let networks = Ok(records(json!({"n":{"name_label":"Network"}})));
        let bonds = Ok(records(json!({
            "foreign-master":{"master":"b","slaves":["a"],"uuid":"FOREIGN_BOND","mode":"FOREIGN_MODE"},
            "foreign-slave":{"master":"a","slaves":["b"],"uuid":"FOREIGN_SLAVE_BOND"},
            "missing":{"master":"a","slaves":["missing"],"uuid":"MISSING_BOND"}
        })));
        let vlans = Ok(records(
            json!({"v":{"tagged_PIF":"b","untagged_PIF":"a","uuid":"FOREIGN_VLAN","tag":123}}),
        ));
        let sriov = Ok(records(
            json!({"s":{"physical_PIF":"b","logical_PIF":"a","uuid":"FOREIGN_SRIOV","configuration_mode":"FOREIGN_CONFIG","requires_reboot":true}}),
        ));
        let mut warnings = vec![];
        let section = network_section(
            pifs,
            NetworkReads {
                metrics: &metrics,
                networks: &networks,
                bonds: &bonds,
                pci: &Ok(Records::new()),
                vlans: &vlans,
                sriov: &sriov,
                capacities: &HashMap::new(),
            },
            "host-a",
            &mut warnings,
        );
        let output = serde_json::to_string(&section).unwrap();
        assert!(!output.contains("FOREIGN"));
        assert!(!output.contains("MISSING_BOND"));
        assert_eq!(warnings.len(), 5);
        let DataSection::Ready { rows, .. } = section else {
            panic!()
        };
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].values["Link"], "Unavailable");
    }
}
