//! Field-limited pool options, independent from identity and credentials.
use crate::xapi::{ApiError, Connection};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PoolOptions {
    pub uuid: String,
    pub migration_compression: Option<bool>,
    pub igmp_snooping_enabled: Option<bool>,
    pub live_patching_disabled: Option<bool>,
}

#[derive(Clone, Debug)]
pub struct PoolOptionsEdit {
    pub original: PoolOptions,
    pub edited: PoolOptions,
}

impl Connection {
    pub async fn pool_options(&self, uuid: &str) -> Result<PoolOptions, ApiError> {
        let (_, record) = self.pool_options_record(uuid).await?;
        Ok(options(uuid, &record))
    }

    async fn pool_options_record(&self, uuid: &str) -> Result<(String, Value), ApiError> {
        if uuid::Uuid::parse_str(uuid).is_err() {
            return Err(ApiError::Xen("Select a pool with a valid UUID.".into()));
        }
        let reference: String = self
            .request("pool.get_by_uuid", json!([self.session, uuid]))
            .await?;
        let record: Value = self
            .request("pool.get_record", json!([self.session, reference]))
            .await?;
        if record["uuid"].as_str() != Some(uuid) {
            return Err(ApiError::Protocol);
        }
        Ok((reference, record))
    }

    pub async fn update_pool_options(
        &self,
        edit: &PoolOptionsEdit,
    ) -> Result<PoolOptions, ApiError> {
        if self.settings.read_only {
            return Err(ApiError::Xen("This connection is read-only.".into()));
        }
        if edit.original.uuid != edit.edited.uuid {
            return Err(ApiError::Xen(
                "The selected pool changed; reload its options.".into(),
            ));
        }
        let (reference, record) = self.pool_options_record(&edit.original.uuid).await?;
        let current = options(&edit.original.uuid, &record);
        let fields = [
            (
                "migration compression",
                "pool.set_migration_compression",
                edit.original.migration_compression,
                edit.edited.migration_compression,
                current.migration_compression,
            ),
            (
                "IGMP snooping",
                "pool.set_igmp_snooping_enabled",
                edit.original.igmp_snooping_enabled,
                edit.edited.igmp_snooping_enabled,
                current.igmp_snooping_enabled,
            ),
            (
                "live patching",
                "pool.set_live_patching_disabled",
                edit.original.live_patching_disabled,
                edit.edited.live_patching_disabled,
                current.live_patching_disabled,
            ),
        ];
        let mut writes = Vec::new();
        for (label, method, original, desired, current) in fields {
            if original == desired {
                continue;
            }
            let (Some(_), Some(desired), Some(current)) = (original, desired, current) else {
                return Err(ApiError::Xen(format!(
                    "The {label} option is unavailable or cannot be cleared; reload the pool options."
                )));
            };
            if current != desired {
                writes.push((label, method, desired));
            }
        }
        for (label, method, desired) in writes {
            self.request::<Value>(method, json!([self.session, reference, desired])).await.map_err(|error|ApiError::Xen(format!("Could not save {label}: {error}. Some settings may already have changed; reload before retrying.")))?;
        }
        let saved = self.pool_options(&edit.original.uuid).await?;
        for ((label, _, original, desired, _), actual) in fields.into_iter().zip([
            saved.migration_compression,
            saved.igmp_snooping_enabled,
            saved.live_patching_disabled,
        ]) {
            if original != desired && actual != desired {
                return Err(ApiError::Xen(format!(
                    "The host did not retain the requested {label} setting; reload the pool options."
                )));
            }
        }
        Ok(saved)
    }
}

fn options(uuid: &str, record: &Value) -> PoolOptions {
    PoolOptions {
        uuid: uuid.to_owned(),
        migration_compression: record["migration_compression"].as_bool(),
        igmp_snooping_enabled: record["igmp_snooping_enabled"].as_bool(),
        live_patching_disabled: record["live_patching_disabled"].as_bool(),
    }
}
