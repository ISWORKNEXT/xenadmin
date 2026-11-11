//! Read-only VM detail model. Every field is explicitly selected; `other_config`
//! and `platform` are allowlisted so configuration secrets never leave the core.
use std::collections::{BTreeMap, HashMap};

use serde::Serialize;
use serde_json::{Value, json};

use crate::host::InfoField;
use crate::xapi::{ApiError, Connection};

type Records = HashMap<String, Value>;

#[derive(Clone, Debug, Serialize)]
pub struct VmDetails {
    pub guest_support: crate::guest_setup::GuestSupport,
    pub guest_data: BTreeMap<String, String>,
    pub uuid: String,
    pub name: String,
    pub description: String,
    pub power_state: String,
    pub is_template: bool,
    pub is_snapshot: bool,
    pub snapshot_of: String,
    pub snapshot_time: String,
    pub resident_on: String,
    pub affinity: String,
    pub vcpus_max: u64,
    pub vcpus_at_startup: u64,
    pub cpu_weight: String,
    pub cores_per_socket: String,
    pub max_cores_per_socket: u64,
    pub supports_vcpu_hotplug: bool,
    pub can_hotplug_vcpus: bool,
    pub memory_static_max: u64,
    pub memory_static_min: u64,
    pub memory_dynamic_max: u64,
    pub memory_dynamic_min: u64,
    pub memory_target: u64,
    pub hvm_boot_policy: String,
    pub pv_args: String,
    pub auto_poweron: bool,
    pub hvm_boot_params: BTreeMap<String, String>,
    pub firmware: String,
    pub secure_boot: String,
    pub after_crash: String,
    pub ha_restart_priority: String,
    pub ha_always_run: bool,
    pub start_delay: u64,
    pub shutdown_delay: u64,
    pub affinity_ref: String,
    pub platform: Vec<InfoField>,
    pub other_config: Vec<InfoField>,
    pub os_version: String,
    pub pv_drivers_version: String,
    pub pv_drivers_up_to_date: bool,
    pub guest_ips: Vec<String>,
    pub tags: Vec<String>,
    pub disks: Vec<VmDisk>,
    pub provision_disks: Vec<crate::provisioning::ProvisionDisk>,
    pub isos: Vec<VmIso>,
    pub physical_dvds: Vec<PhysicalDvd>,
    pub is_default_template: bool,
    pub bios_strings_copied: bool,
    pub interfaces: Vec<VmInterface>,
    pub snapshots: Vec<VmSnapshot>,
    pub warnings: Vec<String>,
}

#[derive(Clone, Debug, Serialize)]
pub struct VmDisk {
    pub vbd_uuid: String,
    pub userdevice: String,
    pub priority: String,
    pub description: String,
    pub device: String,
    pub disk_type: String,
    pub is_cd: bool,
    pub mode: String,
    pub bootable: bool,
    pub empty: bool,
    pub attached: bool,
    pub vdi_uuid: String,
    pub vdi_name: String,
    pub sr_name: String,
    pub sr_uuid: String,
    pub virtual_size: u64,
    pub physical_utilisation: u64,
    pub read_only: bool,
    pub owner: bool,
    pub shared: bool,
    pub allowed_operations: Vec<String>,
}

#[derive(Clone, Debug, Serialize)]
pub struct VmIso {
    pub uuid: String,
    pub name: String,
    pub sr_name: String,
    pub size: u64,
}

#[derive(Clone, Debug, Serialize)]
pub struct PhysicalDvd {
    pub uuid: String,
    pub name: String,
    pub host_uuid: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct VmInterface {
    pub uuid: String,
    pub device: String,
    pub network: String,
    pub network_uuid: String,
    pub bridge: String,
    pub mac: String,
    pub mtu: u64,
    pub attached: bool,
    pub locking_mode: String,
    pub ipv4: Vec<String>,
    pub ipv6: Vec<String>,
}

#[derive(Clone, Debug, Serialize)]
pub struct VdiSummary {
    pub uuid: String,
    pub name: String,
    pub sr_name: String,
    pub virtual_size: u64,
    pub read_only: bool,
}

#[derive(Clone, Debug, Serialize)]
pub struct SrVdi {
    pub description: String,
    pub uuid: String,
    pub name: String,
    pub virtual_size: u64,
    pub physical_utilisation: u64,
    pub read_only: bool,
    pub missing: bool,
    pub vdi_type: String,
    pub attached_to: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct VmSnapshot {
    pub uuid: String,
    pub name: String,
    pub time: String,
}

const PLATFORM_FIELDS: &[(&str, &str)] = &[
    ("acpi", "ACPI"),
    ("apic", "APIC"),
    ("pae", "PAE"),
    ("nx", "NX"),
    ("viridian", "Viridian"),
    ("timeoffset", "Time offset"),
    ("vga", "VGA"),
    ("videoram", "Video RAM"),
    ("boot", "Boot order"),
    ("device_id", "Device ID"),
    ("PCI_bus", "PCI bus"),
    ("usb", "USB"),
    ("usb_tablet", "USB tablet"),
    ("secureboot", "Secure boot"),
];

const OTHER_CONFIG_FIELDS: &[(&str, &str)] = &[
    ("os_type", "OS type"),
    ("base_template_name", "Base template"),
    ("linux_template", "Linux template"),
    ("mac_seed", "MAC seed"),
    ("install-repository", "Install repository"),
];

impl Connection {
    /// List unattached, writable user VDIs available for attach.
    pub async fn vdi_catalog(&self) -> Result<Vec<VdiSummary>, ApiError> {
        let (vdis, srs, vbds) =
            tokio::join!(self.records("VDI"), self.records("SR"), self.records("VBD"));
        let vdis = vdis?;
        let srs = srs?;
        let vbds = vbds?;
        let mut attached = std::collections::HashSet::new();
        for vbd in vbds.values() {
            if let Some(vdi) = vbd["VDI"].as_str() {
                attached.insert(vdi.to_owned());
            }
        }
        let mut catalog: Vec<VdiSummary> = vdis
            .iter()
            .filter(|(reference, vdi)| {
                text(vdi, "type") == "user"
                    && vdi["missing"].as_bool() != Some(true)
                    && vdi["is_a_snapshot"].as_bool() != Some(true)
                    && !attached.contains(reference.as_str())
                    && vdi["read_only"].as_bool() != Some(true)
            })
            .map(|(_, vdi)| {
                let sr_ref = text(vdi, "SR");
                let sr = srs.get(&sr_ref).unwrap_or(&Value::Null);
                VdiSummary {
                    uuid: text(vdi, "uuid"),
                    name: text(vdi, "name_label"),
                    sr_name: text(sr, "name_label"),
                    virtual_size: number(vdi, "virtual_size"),
                    read_only: vdi["read_only"].as_bool().unwrap_or(false),
                }
            })
            .collect();
        catalog.sort_by(|left, right| {
            left.name
                .cmp(&right.name)
                .then(left.sr_name.cmp(&right.sr_name))
        });
        catalog.truncate(500);
        Ok(catalog)
    }

    /// List VDIs in a storage repository with their attachment state.
    pub async fn sr_vdis(&self, sr_uuid: &str) -> Result<Vec<SrVdi>, ApiError> {
        let sr_ref: String = self
            .request("SR.get_by_uuid", json!([self.session, sr_uuid]))
            .await?;
        let sr: Value = self
            .request("SR.get_record", json!([self.session, sr_ref]))
            .await?;
        if sr["uuid"].as_str() != Some(sr_uuid) {
            return Err(ApiError::Protocol);
        }
        let (vdis, vbds, vms) =
            tokio::join!(self.records("VDI"), self.records("VBD"), self.records("VM"));
        let (vdis, vbds, vms) = (vdis?, vbds?, vms?);
        let mut attached: HashMap<&str, String> = HashMap::new();
        for vbd in vbds.values() {
            let Some(vdi) = vbd["VDI"].as_str() else {
                continue;
            };
            let vm_ref = text(vbd, "VM");
            let vm_name = vms
                .get(vm_ref.as_str())
                .map(|vm| text(vm, "name_label"))
                .unwrap_or(vm_ref);
            attached.entry(vdi).or_insert(vm_name);
        }
        let mut catalog: Vec<SrVdi> = vdis
            .iter()
            .filter(|(_, vdi)| text(vdi, "SR") == sr_ref && text(vdi, "type") == "user")
            .map(|(reference, vdi)| SrVdi {
                description: text(vdi, "name_description"),
                uuid: text(vdi, "uuid"),
                name: text(vdi, "name_label"),
                virtual_size: number(vdi, "virtual_size"),
                physical_utilisation: number(vdi, "physical_utilisation"),
                read_only: vdi["read_only"].as_bool().unwrap_or(false),
                missing: vdi["missing"].as_bool().unwrap_or(false),
                vdi_type: text(vdi, "type"),
                attached_to: attached
                    .get(reference.as_str())
                    .cloned()
                    .unwrap_or_default(),
            })
            .collect();
        catalog.sort_by(|left, right| left.name.cmp(&right.name).then(left.uuid.cmp(&right.uuid)));
        catalog.truncate(2000);
        Ok(catalog)
    }

    pub async fn vm_details(&self, vm_uuid: &str) -> Result<VmDetails, ApiError> {
        let reference: String = self
            .request("VM.get_by_uuid", json!([self.session, vm_uuid]))
            .await?;
        let vm: Value = self
            .request("VM.get_record", json!([self.session, reference]))
            .await?;
        if vm["uuid"].as_str() != Some(vm_uuid) {
            return Err(ApiError::Protocol);
        }
        if vm["is_control_domain"].as_bool() == Some(true) {
            return Err(ApiError::Xen(
                "Control domains cannot be opened as virtual machines.".into(),
            ));
        }

        let (vbds, vdis, srs, vifs, networks, guests, hosts, all_vms, pbds, host_metrics) = tokio::join!(
            self.records("VBD"),
            self.records("VDI"),
            self.records("SR"),
            self.records("VIF"),
            self.records("network"),
            self.records("VM_guest_metrics"),
            self.records("host"),
            self.records("VM"),
            self.records("PBD"),
            self.records("host_metrics")
        );

        let mut warnings = Vec::new();
        let vbds = dependency(vbds, "Storage", "VBD records", &mut warnings);
        let vdis = dependency(vdis, "Storage", "VDI records", &mut warnings);
        let srs = dependency(srs, "Storage", "SR records", &mut warnings);
        let vifs = dependency(vifs, "Networking", "VIF records", &mut warnings);
        let networks = dependency(networks, "Networking", "network records", &mut warnings);
        let guests = dependency(guests, "Guest tools", "guest metrics", &mut warnings);
        let hosts = dependency(hosts, "General", "host records", &mut warnings);
        let all_vms = dependency(all_vms, "Snapshots", "VM records", &mut warnings);
        let pbds = dependency(pbds, "CPU topology", "PBD records", &mut warnings);
        let host_metrics = dependency(host_metrics, "CPU topology", "host metrics", &mut warnings);
        let cpu_topology_limit = match (
            hosts.as_ref(),
            vbds.as_ref(),
            vdis.as_ref(),
            srs.as_ref(),
            pbds.as_ref(),
            host_metrics.as_ref(),
            all_vms.as_ref(),
        ) {
            (
                Some(hosts),
                Some(vbds),
                Some(vdis),
                Some(srs),
                Some(pbds),
                Some(metrics),
                Some(vms),
            ) => max_cores_per_socket(
                &vm,
                &CpuTopologyRecords {
                    hosts,
                    vbds,
                    vdis,
                    srs,
                    pbds,
                    metrics,
                    vms,
                },
            ),
            _ => 0,
        };

        let resident_ref = text(&vm, "resident_on");
        let affinity_ref = text(&vm, "affinity");
        let resident_on = host_name(hosts.as_ref(), &resident_ref);
        let affinity = host_name(hosts.as_ref(), &affinity_ref);

        let mut disks = Vec::new();
        if let Some(vbds) = vbds {
            for vbd in vbds.values() {
                if text(vbd, "VM") != reference {
                    continue;
                }
                let vdi_ref = text(vbd, "VDI");
                let vdi = related(vdis.as_ref(), &vdi_ref);
                let sr_ref = text(vdi, "SR");
                let sr = related(srs.as_ref(), &sr_ref);
                let disk_type = text(vbd, "type");
                disks.push(VmDisk {
                    vbd_uuid: text(vbd, "uuid"),
                    userdevice: text(vbd, "userdevice"),
                    priority: text(&vbd["qos_algorithm_params"], "class"),
                    description: text(vdi, "name_description"),
                    device: text(vbd, "device"),
                    is_cd: disk_type == "CD",
                    disk_type,
                    mode: text(vbd, "mode"),
                    bootable: vbd["bootable"].as_bool().unwrap_or(false),
                    empty: vbd["empty"].as_bool().unwrap_or(false),
                    attached: vbd["currently_attached"].as_bool().unwrap_or(false),
                    vdi_uuid: text(vdi, "uuid"),
                    vdi_name: text(vdi, "name_label"),
                    sr_name: text(sr, "name_label"),
                    sr_uuid: text(sr, "uuid"),
                    virtual_size: number(vdi, "virtual_size"),
                    physical_utilisation: number(vdi, "physical_utilisation"),
                    read_only: vdi["read_only"].as_bool().unwrap_or(false),
                    owner: vbd["other_config"].get("owner").is_some(),
                    shared: vbds
                        .values()
                        .any(|other| other["VDI"] == vbd["VDI"] && other["VM"] != vbd["VM"]),
                    allowed_operations: strings(&vdi["allowed_operations"]),
                });
            }
        }

        let mut interfaces = Vec::new();
        if let Some(vifs) = vifs {
            for vif in vifs.values() {
                if text(vif, "VM") != reference {
                    continue;
                }
                let network = related(networks.as_ref(), &text(vif, "network"));
                interfaces.push(VmInterface {
                    uuid: text(vif, "uuid"),
                    device: text(vif, "device"),
                    network: text(network, "name_label"),
                    network_uuid: text(network, "uuid"),
                    bridge: text(network, "bridge"),
                    mac: text(vif, "MAC"),
                    mtu: number(vif, "MTU"),
                    attached: vif["currently_attached"].as_bool().unwrap_or(false),
                    locking_mode: text(vif, "locking_mode"),
                    ipv4: strings(&vif["ipv4_addresses"]),
                    ipv6: strings(&vif["ipv6_addresses"]),
                });
            }
        }

        let mut snapshots = Vec::new();
        if let Some(all_vms) = all_vms {
            for snapshot in all_vms.values() {
                if text(snapshot, "snapshot_of") != reference {
                    continue;
                }
                snapshots.push(VmSnapshot {
                    uuid: text(snapshot, "uuid"),
                    name: text(snapshot, "name_label"),
                    time: text(snapshot, "snapshot_time"),
                });
            }
            snapshots.sort_by(|left, right| right.time.cmp(&left.time));
        }

        let mut isos = Vec::new();
        let mut physical_dvds = Vec::new();
        if let Some(vdis) = vdis.as_ref() {
            for vdi in vdis.values() {
                if vdi["is_a_snapshot"].as_bool() == Some(true)
                    || vdi["is_a_template"].as_bool() == Some(true)
                {
                    continue;
                }
                let sr = related(srs.as_ref(), &text(vdi, "SR"));
                let is_iso = text(vdi, "type") == "iso" || text(sr, "content_type") == "iso";
                if text(sr, "type") == "udev" && text(sr, "content_type") == "iso" {
                    if let (Some(pbds), Some(hosts)) = (pbds.as_ref(), hosts.as_ref()) {
                        for pbd in pbds.values().filter(|pbd| {
                            text(pbd, "SR") == text(vdi, "SR")
                                && pbd["currently_attached"].as_bool() == Some(true)
                        }) {
                            if let Some(host) = hosts.get(&text(pbd, "host")) {
                                physical_dvds.push(PhysicalDvd {
                                    uuid: text(vdi, "uuid"),
                                    name: text(vdi, "name_label"),
                                    host_uuid: text(host, "uuid"),
                                });
                            }
                        }
                    }
                    continue;
                }
                if !is_iso {
                    continue;
                }
                isos.push(VmIso {
                    uuid: text(vdi, "uuid"),
                    name: text(vdi, "name_label"),
                    sr_name: text(sr, "name_label"),
                    size: number(vdi, "virtual_size"),
                });
            }
            isos.sort_by(|left, right| left.name.cmp(&right.name));
        }

        let guest = related(guests.as_ref(), &text(&vm, "guest_metrics"));
        let mut guest_ips: Vec<String> = guest["networks"]
            .as_object()
            .map(|networks| {
                networks
                    .iter()
                    .filter(|(key, _)| key.ends_with("/ip") || key.ends_with("/ipv4"))
                    .filter_map(|(_, value)| value.as_str())
                    .filter(|value| !value.is_empty())
                    .map(str::to_owned)
                    .collect()
            })
            .unwrap_or_default();
        guest_ips.sort();
        guest_ips.dedup();
        let platform = fields(&vm["platform"], PLATFORM_FIELDS);
        let other_config = fields(&vm["other_config"], OTHER_CONFIG_FIELDS);
        let firmware = vm["HVM_boot_params"]["firmware"]
            .as_str()
            .filter(|value| !value.is_empty())
            .unwrap_or("bios")
            .to_owned();
        let secure_boot = vm["platform"]["secureboot"]
            .as_str()
            .unwrap_or("false")
            .to_owned();

        let provision_disks = match crate::provisioning::provision_disks(
            vm["other_config"]["disks"].as_str().unwrap_or_default(),
        ) {
            Ok(disks) => disks,
            Err(error) => {
                warnings.push(error.to_string());
                Vec::new()
            }
        };

        let guest_data = vm["xenstore_data"]
            .as_object()
            .into_iter()
            .flatten()
            .filter(|(k, _)| k.starts_with("vm-data/"))
            .map(|(k, v)| {
                (
                    k.clone(),
                    if matches!(
                        k.as_str(),
                        "vm-data/xpwd" | "vm-data/password" | "vm-data/root-password"
                    ) {
                        "•••• (pending guest pickup)".into()
                    } else {
                        v.as_str().unwrap_or_default().to_owned()
                    },
                )
            })
            .collect();
        Ok(VmDetails {
            guest_support: crate::guest_setup::GuestSupport::from_record(&vm),
            guest_data,
            uuid: vm_uuid.into(),
            name: text(&vm, "name_label"),
            description: text(&vm, "name_description"),
            power_state: text(&vm, "power_state"),
            is_template: vm["is_a_template"].as_bool().unwrap_or(false),
            is_snapshot: vm["is_a_snapshot"].as_bool().unwrap_or(false),
            snapshot_of: text(&vm, "snapshot_of"),
            snapshot_time: text(&vm, "snapshot_time"),
            resident_on,
            affinity,
            vcpus_max: number(&vm, "VCPUs_max"),
            vcpus_at_startup: number(&vm, "VCPUs_at_startup"),
            cpu_weight: vm["VCPUs_params"]["weight"]
                .as_str()
                .unwrap_or("256")
                .to_owned(),
            cores_per_socket: vm["platform"]["cores-per-socket"]
                .as_str()
                .unwrap_or("1")
                .to_owned(),
            max_cores_per_socket: cpu_topology_limit,
            supports_vcpu_hotplug: !is_windows_guest(&vm, guests.as_ref())
                && hosts.as_ref().is_some_and(|hosts| {
                    !hosts.is_empty()
                        && hosts.values().all(|host| {
                            matches!(
                                host["license_params"]["restrict_set_vcpus_number_live"].as_str(),
                                Some("false")
                            )
                        })
                }),
            can_hotplug_vcpus: vcpu_hotplug_allowed(&vm, guests.as_ref()),
            memory_static_max: number(&vm, "memory_static_max"),
            memory_static_min: number(&vm, "memory_static_min"),
            memory_dynamic_max: number(&vm, "memory_dynamic_max"),
            memory_dynamic_min: number(&vm, "memory_dynamic_min"),
            memory_target: number(&vm, "memory_target"),
            hvm_boot_policy: text(&vm, "HVM_boot_policy"),
            pv_args: text(&vm, "PV_args"),
            auto_poweron: vm["other_config"]["auto_poweron"].as_str() == Some("true"),
            hvm_boot_params: string_map(&vm["HVM_boot_params"]),
            firmware,
            secure_boot,
            after_crash: text(&vm, "actions_after_crash"),
            ha_restart_priority: text(&vm, "ha_restart_priority"),
            ha_always_run: vm["ha_always_run"].as_bool().unwrap_or(false),
            start_delay: number(&vm, "start_delay"),
            shutdown_delay: number(&vm, "shutdown_delay"),
            affinity_ref,
            platform,
            other_config,
            os_version: if guest["os_version"].is_object() {
                text(&guest["os_version"], "name")
            } else {
                text(guest, "os_version")
            },
            pv_drivers_version: if guest["PV_drivers_version"].is_object() {
                ["major", "minor", "micro", "build"]
                    .iter()
                    .filter_map(|key| guest["PV_drivers_version"][key].as_str())
                    .collect::<Vec<_>>()
                    .join(".")
            } else {
                text(guest, "PV_drivers_version")
            },
            pv_drivers_up_to_date: guest["PV_drivers_up_to_date"].as_bool().unwrap_or(false),
            guest_ips,
            tags: vm["tags"]
                .as_array()
                .map(|tags| {
                    tags.iter()
                        .filter_map(Value::as_str)
                        .map(str::to_owned)
                        .collect()
                })
                .unwrap_or_default(),
            disks,
            provision_disks,
            isos,
            physical_dvds,
            is_default_template: vm["other_config"].get("default_template").is_some(),
            bios_strings_copied: vm["other_config"].get("default_template").is_none()
                && vm["bios_strings"]
                    .as_object()
                    .is_some_and(|bios| !bios.is_empty())
                && !(vm["bios_strings"]["bios-vendor"].as_str() == Some("Xen")
                    && vm["bios_strings"]["system-manufacturer"].as_str() == Some("Xen")),
            interfaces,
            snapshots,
            warnings,
        })
    }
}

fn dependency(
    read: Result<Records, ApiError>,
    section: &str,
    resource: &str,
    warnings: &mut Vec<String>,
) -> Option<Records> {
    match read {
        Ok(records) => Some(records),
        Err(error) => {
            warnings.push(format!("{section}: {resource}: {error}"));
            None
        }
    }
}

fn related<'a>(records: Option<&'a Records>, reference: &str) -> &'a Value {
    records
        .and_then(|records| records.get(reference))
        .unwrap_or(&Value::Null)
}

fn host_name(records: Option<&Records>, reference: &str) -> String {
    if reference.is_empty() || reference == "OpaqueRef:NULL" {
        return "Unassigned".into();
    }
    records
        .and_then(|records| records.get(reference))
        .map(|host| text(host, "name_label"))
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| "Unavailable".into())
}

fn fields(record: &Value, allowed: &[(&str, &str)]) -> Vec<InfoField> {
    allowed
        .iter()
        .filter(|(key, _)| !record[*key].is_null())
        .map(|(key, label)| InfoField {
            label: (*label).into(),
            value: display(&record[*key]),
        })
        .collect()
}

fn string_map(value: &Value) -> BTreeMap<String, String> {
    value
        .as_object()
        .map(|values| {
            values
                .iter()
                .filter_map(|(key, value)| Some((key.clone(), value.as_str()?.to_owned())))
                .collect()
        })
        .unwrap_or_default()
}

fn strings(value: &Value) -> Vec<String> {
    value
        .as_array()
        .map(|values| {
            values
                .iter()
                .filter_map(Value::as_str)
                .filter(|value| !value.is_empty())
                .map(str::to_owned)
                .collect()
        })
        .unwrap_or_default()
}

fn text(record: &Value, key: &str) -> String {
    record[key].as_str().unwrap_or_default().to_owned()
}

fn number(record: &Value, key: &str) -> u64 {
    let value = &record[key];
    value
        .as_u64()
        .or_else(|| value.as_str().and_then(|value| value.parse().ok()))
        .unwrap_or(0)
}

fn display(value: &Value) -> String {
    match value {
        Value::String(value) if value.is_empty() => "—".into(),
        Value::String(value) => value.clone(),
        Value::Bool(value) => if *value { "Yes" } else { "No" }.into(),
        Value::Number(value) => value.to_string(),
        Value::Array(values) => {
            let values: Vec<String> = values
                .iter()
                .filter(|value| !value.is_object() && !value.is_array())
                .map(display)
                .collect();
            if values.is_empty() {
                "None".into()
            } else {
                values.join(", ")
            }
        }
        _ => "—".into(),
    }
}

fn is_windows_guest(vm: &Value, guests: Option<&Records>) -> bool {
    let current = related(guests, &text(vm, "guest_metrics"));
    let previous_ref = vm["last_booted_record"]
        .as_str()
        .and_then(|record| record.split_once("'guest_metrics'"))
        .and_then(|(_, rest)| rest.trim_start().strip_prefix("'"))
        .and_then(|rest| rest.split_once("'"))
        .map(|(reference, _)| reference)
        .filter(|reference| {
            reference
                .strip_prefix("OpaqueRef:")
                .is_some_and(|id| uuid::Uuid::parse_str(id).is_ok())
        });
    let previous = related(guests, previous_ref.unwrap_or_default());
    let known_non_windows = [current, previous].iter().any(|guest| {
        matches!(
            guest["os_version"]["distro"]
                .as_str()
                .unwrap_or_default()
                .to_ascii_lowercase()
                .as_str(),
            "debian"
                | "rhel"
                | "fedora"
                | "centos"
                | "scientific"
                | "oracle"
                | "sles"
                | "lsb"
                | "boot2docker"
                | "freebsd"
                | "ubuntu"
                | "neokylin"
                | "gooroom"
                | "rocky"
        ) || guest["os_version"]["uname"]
            .as_str()
            .unwrap_or_default()
            .to_ascii_lowercase()
            .contains("netscaler")
    });
    !known_non_windows
        && !text(vm, "HVM_boot_policy").is_empty()
        && matches!(vm["platform"]["viridian"].as_str(), Some("true" | "1"))
}

pub(crate) fn vcpu_hotplug_allowed(vm: &Value, guests: Option<&Records>) -> bool {
    vm["power_state"].as_str() == Some("Running")
        && !is_windows_guest(vm, guests)
        && strings(&vm["allowed_operations"])
            .iter()
            .any(|op| op == "changing_VCPUs_live")
}

pub(crate) struct CpuTopologyRecords<'a> {
    pub hosts: &'a Records,
    pub vbds: &'a Records,
    pub vdis: &'a Records,
    pub srs: &'a Records,
    pub pbds: &'a Records,
    pub metrics: &'a Records,
    pub vms: &'a Records,
}

pub(crate) fn host_cores_per_socket(host: &Value) -> u64 {
    let sockets = number(&host["cpu_info"], "socket_count");
    number(&host["cpu_info"], "cpu_count")
        .checked_div(sockets)
        .unwrap_or(0)
}

/// Same home precedence as upstream VM.Home: resident, local storage, live affinity.
pub(crate) fn max_cores_per_socket(vm: &Value, records: &CpuTopologyRecords<'_>) -> u64 {
    let mut source = vm;
    let mut visited = std::collections::HashSet::new();
    while source["is_a_snapshot"].as_bool() == Some(true) {
        let parent = text(source, "snapshot_of");
        if !visited.insert(parent.clone()) {
            return 0;
        }
        let Some(parent) = records.vms.get(&parent) else {
            return records
                .hosts
                .values()
                .map(host_cores_per_socket)
                .max()
                .unwrap_or(0);
        };
        source = parent;
    }
    let home = if source["is_a_template"].as_bool() == Some(true) {
        None
    } else if matches!(source["power_state"].as_str(), Some("Running" | "Paused")) {
        records.hosts.get(&text(source, "resident_on"))
    } else {
        let storage_home = source["VBDs"]
            .as_array()
            .into_iter()
            .flatten()
            .find_map(|reference| {
                let vbd = records.vbds.get(reference.as_str()?)?;
                let vdi = records.vdis.get(&text(vbd, "VDI"))?;
                if vdi["missing"].as_bool() == Some(true) || vdi["managed"].as_bool() != Some(true)
                {
                    return None;
                }
                let sr = records.srs.get(&text(vdi, "SR"))?;
                if sr["shared"].as_bool() == Some(true) {
                    return None;
                }
                let pbds = sr["PBDs"].as_array()?;
                if pbds.len() != 1 {
                    return None;
                }
                let pbd = records.pbds.get(pbds[0].as_str()?)?;
                records.hosts.get(&text(pbd, "host"))
            });
        storage_home.or_else(|| {
            let host = records.hosts.get(&text(source, "affinity"))?;
            let metric = records.metrics.get(&text(host, "metrics"))?;
            (metric["live"].as_bool() == Some(true)).then_some(host)
        })
    };
    home.map(host_cores_per_socket).unwrap_or_else(|| {
        records
            .hosts
            .values()
            .map(host_cores_per_socket)
            .max()
            .unwrap_or(0)
    })
}

pub fn validate_cpu_topology(vcpus: u64, cores: u64, host_limit: u64) -> Result<(), ApiError> {
    if cores == 0
        || vcpus == 0
        || !vcpus.is_multiple_of(cores)
        || vcpus / cores > 16
        || (host_limit > 0 && cores > host_limit)
    {
        return Err(ApiError::Xen("Choose cores per socket that divide the vCPU maximum, use at most 16 sockets and fit the home server's CPU topology.".into()));
    }
    Ok(())
}
