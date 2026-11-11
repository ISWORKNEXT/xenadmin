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
        Ok(properties_from_record(target, &record))
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
        self.object_properties(&edited.target).await
    }
}

fn properties_from_record(target: &ObjectTarget, record: &Value) -> ObjectProperties {
    ObjectProperties {
        target: target.clone(),
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
