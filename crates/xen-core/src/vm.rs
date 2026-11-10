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
    pub memory_static_max: u64,
    pub memory_static_min: u64,
    pub memory_dynamic_max: u64,
    pub memory_dynamic_min: u64,
    pub memory_target: u64,
    pub hvm_boot_policy: String,
    pub hvm_boot_params: BTreeMap<String, String>,
    pub ha_restart_priority: String,
    pub ha_always_run: bool,
    pub platform: Vec<InfoField>,
    pub other_config: Vec<InfoField>,
    pub os_version: String,
    pub pv_drivers_version: String,
    pub pv_drivers_up_to_date: bool,
    pub disks: Vec<VmDisk>,
    pub isos: Vec<VmIso>,
    pub interfaces: Vec<VmInterface>,
    pub snapshots: Vec<VmSnapshot>,
    pub warnings: Vec<String>,
}

#[derive(Clone, Debug, Serialize)]
pub struct VmDisk {
    pub vbd_uuid: String,
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
    pub virtual_size: u64,
    pub physical_utilisation: u64,
    pub read_only: bool,
    pub owner: bool,
}

#[derive(Clone, Debug, Serialize)]
pub struct VmIso {
    pub uuid: String,
    pub name: String,
    pub sr_name: String,
    pub size: u64,
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
    ("secure_boot", "Secure boot"),
];

const OTHER_CONFIG_FIELDS: &[(&str, &str)] = &[
    ("os_type", "OS type"),
    ("base_template_name", "Base template"),
    ("linux_template", "Linux template"),
    ("mac_seed", "MAC seed"),
    ("install-repository", "Install repository"),
];

impl Connection {
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

        let (vbds, vdis, srs, vifs, networks, guests, hosts, all_vms) = tokio::join!(
            self.records("VBD"),
            self.records("VDI"),
            self.records("SR"),
            self.records("VIF"),
            self.records("network"),
            self.records("VM_guest_metrics"),
            self.records("host"),
            self.records("VM")
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
                    virtual_size: number(vdi, "virtual_size"),
                    physical_utilisation: number(vdi, "physical_utilisation"),
                    read_only: vdi["read_only"].as_bool().unwrap_or(false),
                    owner: vbd["other_config"].get("owner").is_some(),
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
        if let Some(vdis) = vdis.as_ref() {
            for vdi in vdis.values() {
                if vdi["is_a_snapshot"].as_bool() == Some(true)
                    || vdi["is_a_template"].as_bool() == Some(true)
                {
                    continue;
                }
                let sr = related(srs.as_ref(), &text(vdi, "SR"));
                let is_iso = text(vdi, "type") == "iso" || text(sr, "content_type") == "iso";
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
        let platform = fields(&vm["platform"], PLATFORM_FIELDS);
        let other_config = fields(&vm["other_config"], OTHER_CONFIG_FIELDS);

        Ok(VmDetails {
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
            memory_static_max: number(&vm, "memory_static_max"),
            memory_static_min: number(&vm, "memory_static_min"),
            memory_dynamic_max: number(&vm, "memory_dynamic_max"),
            memory_dynamic_min: number(&vm, "memory_dynamic_min"),
            memory_target: number(&vm, "memory_target"),
            hvm_boot_policy: text(&vm, "HVM_boot_policy"),
            hvm_boot_params: string_map(&vm["HVM_boot_params"]),
            ha_restart_priority: text(&vm, "ha_restart_priority"),
            ha_always_run: vm["ha_always_run"].as_bool().unwrap_or(false),
            platform,
            other_config,
            os_version: text(guest, "os_version"),
            pv_drivers_version: text(guest, "PV_drivers_version"),
            pv_drivers_up_to_date: guest["PV_drivers_up_to_date"].as_bool().unwrap_or(false),
            disks,
            isos,
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
