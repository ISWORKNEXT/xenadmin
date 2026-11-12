//! The editable identity fields shared by managed XenAPI objects.
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::xapi::{ApiError, Connection};

pub const MAX_DESCRIPTION_CHARS: usize = 32767;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ObjectKind {
    Vm,
    Host,
    Pool,
    Storage,
    Disk,
    Network,
}

impl ObjectKind {
    pub fn label(self) -> &'static str {
        match self {
            Self::Vm => "Virtual machine / template / snapshot",
            Self::Host => "Host",
            Self::Pool => "Pool",
            Self::Storage => "Storage repository",
            Self::Disk => "Virtual disk",
            Self::Network => "Network",
        }
    }

    fn class(self) -> &'static str {
        match self {
            Self::Vm => "VM",
            Self::Host => "host",
            Self::Pool => "pool",
            Self::Storage => "SR",
            Self::Disk => "VDI",
            Self::Network => "network",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ObjectTarget {
    pub kind: ObjectKind,
    pub uuid: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ObjectProperties {
    pub target: ObjectTarget,
    pub name: String,
    pub description: String,
    pub tags: Vec<String>,
    pub folder: String,
    #[serde(default)]
    pub custom_fields: std::collections::BTreeMap<String, String>,
    #[serde(default)]
    pub custom_definitions: Vec<crate::organization::CustomFieldDefinition>,
    #[serde(default)]
    pub definition_xml: String,
    #[serde(default)]
    pub organization_pool: String,
    #[serde(default)]
    pub page_summaries: std::collections::BTreeMap<String, String>,
    pub disk: Option<DiskProperties>,
    #[serde(default)]
    pub network: Option<NetworkProperties>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct NetworkProperties {
    pub mtu: u64,
    pub autoplug: bool,
    pub interfaces: Vec<String>,
    pub external: bool,
    pub parent_uuid: String,
    pub vlan: u16,
    pub can_reconfigure: bool,
    pub parents: Vec<(String, String)>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DiskProperties {
    pub size_bytes: u64,
    pub sr_name: String,
    pub devices: Vec<DiskDevice>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DiskDevice {
    pub vbd_uuid: String,
    pub vm_uuid: String,
    pub vm_name: String,
    pub mode: String,
    pub bootable: bool,
    pub position: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ObjectPropertiesEdit {
    pub original: ObjectProperties,
    pub edited: ObjectProperties,
}

impl Connection {
    async fn property_record(&self, target: &ObjectTarget) -> Result<(String, Value), ApiError> {
        if uuid::Uuid::parse_str(&target.uuid).is_err() {
            return Err(ApiError::Xen("Select an object with a valid UUID.".into()));
        }
        let class = target.kind.class();
        let reference: String = self
            .request(
                &format!("{class}.get_by_uuid"),
                json!([self.session, target.uuid]),
            )
            .await?;
        let record: Value = self
            .request(
                &format!("{class}.get_record"),
                json!([self.session, reference]),
            )
            .await?;
        if record["uuid"].as_str() != Some(&target.uuid) {
            return Err(ApiError::Protocol);
        }
        if target.kind == ObjectKind::Vm && record["is_control_domain"].as_bool() != Some(false) {
            return Err(ApiError::Xen(
                "Control-domain properties cannot be edited here.".into(),
            ));
        }
        Ok((reference, record))
    }

    pub async fn object_properties(
        &self,
        target: &ObjectTarget,
    ) -> Result<ObjectProperties, ApiError> {
        let (_, record) = self.property_record(target).await?;
        let mut properties = properties_from_record(target, &record);
        let pools: std::collections::BTreeMap<String, Value> = self
            .request("pool.get_all_records", json!([self.session]))
            .await?;
        if let Some((pool, record)) = pools.into_iter().next() {
            properties.definition_xml = record["gui_config"][crate::organization::CUSTOM_FIELDS]
                .as_str()
                .unwrap_or_default()
                .into();
            properties.custom_definitions =
                crate::organization::decode_definitions(&properties.definition_xml)?;
            properties.organization_pool = pool;
        }
        if target.kind == ObjectKind::Disk {
            let mut devices = Vec::new();
            for reference in record["VBDs"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(Value::as_str)
            {
                let vbd: Value = self
                    .request("VBD.get_record", json!([self.session, reference]))
                    .await?;
                let vm: Value = self
                    .request("VM.get_record", json!([self.session, vbd["VM"]]))
                    .await?;
                devices.push(DiskDevice {
                    vbd_uuid: vbd["uuid"].as_str().unwrap_or_default().into(),
                    vm_uuid: vm["uuid"].as_str().unwrap_or_default().into(),
                    vm_name: vm["name_label"].as_str().unwrap_or_default().into(),
                    mode: vbd["mode"].as_str().unwrap_or("RW").into(),
                    bootable: vbd["bootable"].as_bool().unwrap_or(false),
                    position: vbd["userdevice"].as_str().unwrap_or_default().into(),
                });
            }
            let sr: Value = self
                .request("SR.get_record", json!([self.session, record["SR"]]))
                .await?;
            properties.disk = Some(DiskProperties {
                size_bytes: record["virtual_size"]
                    .as_str()
                    .and_then(|v| v.parse().ok())
                    .or_else(|| record["virtual_size"].as_u64())
                    .unwrap_or_default(),
                sr_name: sr["name_label"].as_str().unwrap_or_default().into(),
                devices,
            });
        }
        if target.kind == ObjectKind::Network {
            let mut interfaces = Vec::new();
            for pif in self.network_pifs(&target.uuid).await? {
                interfaces.push(format!(
                    "{} — {} — VLAN {}",
                    pif.host_name, pif.device, pif.vlan
                ));
            }
            let catalog = self.pif_catalog().await?;
            let connected: Vec<_> = catalog
                .iter()
                .filter(|p| p.network_uuid == target.uuid)
                .collect();
            let mut parent_uuid = String::new();
            if let Some(pif) = connected.first()
                && pif.vlan >= 0
            {
                let reference: String = self
                    .request("PIF.get_by_uuid", json!([self.session, pif.uuid]))
                    .await?;
                let pif: Value = self
                    .request("PIF.get_record", json!([self.session, reference]))
                    .await?;
                let vlan: Value = self
                    .request(
                        "VLAN.get_record",
                        json!([self.session, pif["VLAN_master_of"]]),
                    )
                    .await?;
                let parent: Value = self
                    .request("PIF.get_record", json!([self.session, vlan["tagged_PIF"]]))
                    .await?;
                parent_uuid = parent["uuid"].as_str().unwrap_or_default().into();
            }
            properties.network = Some(NetworkProperties {
                mtu: record["MTU"]
                    .as_u64()
                    .or_else(|| record["MTU"].as_str()?.parse().ok())
                    .unwrap_or(1500),
                autoplug: record["other_config"]["automatic"].as_str() != Some("false"),
                external: !connected.is_empty(),
                vlan: connected.first().map(|p| p.vlan.max(0) as u16).unwrap_or(0),
                can_reconfigure: connected
                    .iter()
                    .all(|p| !p.physical && !p.management && p.vlan >= 0),
                parent_uuid,
                parents: catalog
                    .iter()
                    .filter(|p| p.physical && p.vlan == -1 && p.bond_network_uuid.is_empty())
                    .map(|p| (p.uuid.clone(), format!("{} — {}", p.host_name, p.device)))
                    .collect(),
                interfaces,
            });
        }
        Ok(properties)
    }

    pub async fn update_object_properties(
        &self,
        edit: &ObjectPropertiesEdit,
    ) -> Result<ObjectProperties, ApiError> {
        if self.settings.read_only {
            return Err(ApiError::Xen("This connection is read-only.".into()));
        }
        let original = &edit.original;
        let edited = &edit.edited;
        if original.target != edited.target {
            return Err(ApiError::Xen(
                "The property editor target changed. Reopen the editor.".into(),
            ));
        }
        let name = if edited.name == original.name {
            original.name.as_str()
        } else {
            edited.name.trim()
        };
        let name_changed = name != original.name;
        let description_changed = edited.description != original.description;
        let tags = canonical_tags(&edited.tags);
        let old_tags = canonical_tags(&original.tags);
        let tags_changed = tags != old_tags;
        if name_changed
            && ((name.is_empty() && edited.target.kind != ObjectKind::Pool)
                || name.chars().count() > 255)
        {
            return Err(ApiError::Xen(
                "Enter a name of up to 255 characters. Only a pool may have an empty name.".into(),
            ));
        }
        validate_description_change(&original.description, &edited.description)?;
        if tags_changed {
            for tag in tags.iter().filter(|tag| !old_tags.contains(tag)) {
                if tag.trim().is_empty()
                    || tag.chars().count() > 255
                    || tag.chars().any(char::is_control)
                {
                    return Err(ApiError::Xen(
                        "Each new tag must contain 1–255 characters without control characters."
                            .into(),
                    ));
                }
            }
        }
        let folder = crate::organization::normalize_folder(&edited.folder)?;
        let definitions = crate::organization::encode_definitions(&edited.custom_definitions)?;
        for definition in &edited.custom_definitions {
            if definition.date
                && edited.custom_fields.get(&definition.name)
                    != original.custom_fields.get(&definition.name)
                && let Some(value) = edited.custom_fields.get(&definition.name)
                && !value.is_empty()
                && chrono::DateTime::parse_from_rfc3339(value).is_err()
            {
                return Err(ApiError::Xen(format!(
                    "{} must be an ISO 8601 date and time (e.g. 2026-11-12T00:00:00Z).",
                    definition.name
                )));
            }
        }
        let definitions_changed = edited.custom_definitions != original.custom_definitions;
        if definitions_changed {
            let actual: Value = self
                .request(
                    "pool.get_gui_config",
                    json!([self.session, original.organization_pool]),
                )
                .await?;
            if actual[crate::organization::CUSTOM_FIELDS]
                .as_str()
                .unwrap_or_default()
                != original.definition_xml
            {
                return Err(ApiError::Xen(
                    "Custom field definitions changed. Reopen Properties.".into(),
                ));
            }
        }
        let (reference, record) = self.property_record(&edited.target).await?;
        let current = properties_from_record(&edited.target, &record);
        let fields = [
            (
                "name_label",
                name_changed,
                json!(original.name),
                json!(name),
                json!(current.name),
            ),
            (
                "name_description",
                description_changed,
                json!(original.description),
                json!(edited.description),
                json!(current.description),
            ),
            (
                "tags",
                tags_changed,
                json!(old_tags),
                json!(tags),
                json!(canonical_tags(&current.tags)),
            ),
        ];
        for (field, changed, before, desired, actual) in &fields {
            if *changed && actual != before && actual != desired {
                let label = match *field {
                    "name_label" => "name",
                    "name_description" => "description",
                    _ => "tags",
                };
                return Err(ApiError::Xen(format!(
                    "The {label} changed on the host since this editor opened. Reload before saving."
                )));
            }
        }
        let class = edited.target.kind.class();
        for (field, changed, _, desired, actual) in fields {
            if changed && actual != desired {
                self.request::<Value>(
                    &format!("{class}.set_{field}"),
                    json!([self.session, reference, desired]),
                )
                .await?;
            }
        }
        let mut changes = std::collections::BTreeMap::new();
        if folder != original.folder {
            changes.insert("folder".to_owned(), (original.folder.clone(), folder));
        }
        for key in original
            .custom_fields
            .keys()
            .chain(edited.custom_fields.keys())
        {
            let before = original.custom_fields.get(key).cloned().unwrap_or_default();
            let after = edited.custom_fields.get(key).cloned().unwrap_or_default();
            if before != after {
                changes.insert(
                    format!("{}{key}", crate::organization::CUSTOM_PREFIX),
                    (before, after),
                );
            }
        }
        for (key, (before, after)) in &changes {
            let actual = record["other_config"][key].as_str().unwrap_or_default();
            if actual != before && actual != after {
                return Err(ApiError::Xen(format!(
                    "{key} changed on the host. Reopen Properties."
                )));
            }
        }
        for (key, (_, after)) in changes {
            self.request::<Value>(
                &format!("{class}.remove_from_other_config"),
                json!([self.session, reference, key]),
            )
            .await?;
            if !after.is_empty() {
                self.request::<Value>(
                    &format!("{class}.add_to_other_config"),
                    json!([self.session, reference, key, after]),
                )
                .await?;
            }
        }
        if definitions_changed {
            let removed: Vec<_> = original
                .custom_definitions
                .iter()
                .filter(|d| !edited.custom_definitions.iter().any(|e| e.name == d.name))
                .collect();
            if !removed.is_empty() {
                for class in ["pool", "host", "VM", "SR", "VDI", "network"] {
                    for (reference, record) in self.records(class).await? {
                        for definition in &removed {
                            let key = format!(
                                "{}{}",
                                crate::organization::CUSTOM_PREFIX,
                                definition.name
                            );
                            if record["other_config"].get(&key).is_some() {
                                self.request::<Value>(
                                    &format!("{class}.remove_from_other_config"),
                                    json!([self.session, reference, key]),
                                )
                                .await?;
                            }
                        }
                    }
                }
            }
        }
        if definitions_changed {
            self.request::<Value>(
                "pool.remove_from_gui_config",
                json!([
                    self.session,
                    original.organization_pool,
                    crate::organization::CUSTOM_FIELDS
                ]),
            )
            .await?;
            self.request::<Value>(
                "pool.add_to_gui_config",
                json!([
                    self.session,
                    original.organization_pool,
                    crate::organization::CUSTOM_FIELDS,
                    definitions
                ]),
            )
            .await?;
        }
        if let (Some(before), Some(network)) = (&original.network, &edited.network) {
            if !(1500..=9000).contains(&network.mtu) {
                return Err(ApiError::Xen("MTU must be between 1500 and 9000.".into()));
            }
            if before.external != network.external
                || before.parent_uuid != network.parent_uuid
                || before.vlan != network.vlan
            {
                self.reconfigure_network_external(
                    &edited.target.uuid,
                    network.external,
                    &network.parent_uuid,
                    network.vlan,
                )
                .await?;
            }
            if before.mtu != network.mtu {
                self.request::<Value>(
                    "network.set_MTU",
                    json!([self.session, reference, network.mtu]),
                )
                .await?;
            }
            if before.autoplug != network.autoplug {
                self.request::<Value>(
                    "network.remove_from_other_config",
                    json!([self.session, reference, "automatic"]),
                )
                .await?;
                self.request::<Value>(
                    "network.add_to_other_config",
                    json!([
                        self.session,
                        reference,
                        "automatic",
                        network.autoplug.to_string()
                    ]),
                )
                .await?;
            }
        }
        self.object_properties(&edited.target).await
    }
}

fn properties_from_record(target: &ObjectTarget, record: &Value) -> ObjectProperties {
    ObjectProperties {
        custom_fields: record["other_config"]
            .as_object()
            .into_iter()
            .flatten()
            .filter_map(|(k, v)| {
                Some((
                    k.strip_prefix(crate::organization::CUSTOM_PREFIX)?
                        .to_owned(),
                    v.as_str()?.to_owned(),
                ))
            })
            .collect(),
        custom_definitions: Vec::new(),
        definition_xml: String::new(),
        organization_pool: String::new(),
        page_summaries: {
            let mut summaries = std::collections::BTreeMap::new();
            let alarms = record["other_config"]["perfmon"]
                .as_str()
                .unwrap_or_default();
            summaries.insert(
                "Alerts".into(),
                if alarms.is_empty() {
                    "None configured".into()
                } else {
                    "Configured".into()
                },
            );
            let log = record["logging"]["syslog_destination"]
                .as_str()
                .unwrap_or_default();
            summaries.insert(
                "Log Destination".into(),
                if log.is_empty() {
                    "Local".into()
                } else {
                    log.into()
                },
            );
            let count = record["other_config"].as_object().map_or(0, |m| {
                m.keys()
                    .filter(|k| k.starts_with("XenCenter.CustomFields."))
                    .count()
            });
            summaries.insert(
                "Custom Fields".into(),
                if count == 0 {
                    "None configured".into()
                } else {
                    format!("{count} custom fields")
                },
            );
            summaries
        },
        target: target.clone(),
        folder: record["other_config"]["folder"]
            .as_str()
            .unwrap_or_default()
            .into(),
        disk: None,
        network: None,
        name: record["name_label"].as_str().unwrap_or_default().to_owned(),
        description: record["name_description"]
            .as_str()
            .unwrap_or_default()
            .to_owned(),
        tags: record["tags"]
            .as_array()
            .map(|tags| {
                tags.iter()
                    .filter_map(Value::as_str)
                    .map(str::to_owned)
                    .collect()
            })
            .unwrap_or_default(),
    }
}

fn canonical_tags(tags: &[String]) -> Vec<String> {
    let mut tags = tags.to_vec();
    tags.sort();
    tags.dedup();
    tags
}

pub(crate) fn validate_description_change(original: &str, desired: &str) -> Result<(), ApiError> {
    if original != desired && desired.chars().count() > MAX_DESCRIPTION_CHARS {
        return Err(ApiError::Xen(format!(
            "The description must be {MAX_DESCRIPTION_CHARS} characters or fewer."
        )));
    }
    Ok(())
}
