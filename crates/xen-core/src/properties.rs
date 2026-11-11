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
    pub disk: Option<DiskProperties>,
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
        if edited.folder != original.folder {
            let actual = record["other_config"]["folder"]
                .as_str()
                .unwrap_or_default();
            if actual != original.folder && actual != edited.folder {
                return Err(ApiError::Xen(
                    "The folder changed on the host. Reopen Properties.".into(),
                ));
            }
            let mut other = record["other_config"]
                .as_object()
                .cloned()
                .unwrap_or_default();
            if edited.folder.is_empty() {
                other.remove("folder");
            } else {
                other.insert("folder".into(), json!(edited.folder));
            }
            self.request::<Value>(
                &format!("{class}.set_other_config"),
                json!([self.session, reference, other]),
            )
            .await?;
        }
        self.object_properties(&edited.target).await
    }
}

fn properties_from_record(target: &ObjectTarget, record: &Value) -> ObjectProperties {
    ObjectProperties {
        target: target.clone(),
        folder: record["other_config"]["folder"]
            .as_str()
            .unwrap_or_default()
            .into(),
        disk: None,
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
