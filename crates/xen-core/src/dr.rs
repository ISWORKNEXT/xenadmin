//! Disaster-recovery task write actions.
use std::collections::HashSet;

use serde_json::{Value, json};

use crate::xapi::{ApiError, Connection};

const DR_TYPES: [&str; 5] = ["nfs", "lvmoiscsi", "lvmohba", "lvmofcoe", "hba"];

impl Connection {
    /// Introduce an unmanaged storage repository into the pool as a DR task.
    pub async fn create_dr_task(
        &self,
        sr_type: &str,
        device_config: Vec<(String, String)>,
        whitelist: Vec<String>,
    ) -> Result<(), ApiError> {
        if self.settings.read_only {
            return Err(ApiError::Xen(
                "This connection is read-only. Changes are blocked by the Rust backend.".into(),
            ));
        }
        let config = validate_dr_task(sr_type, &device_config, &whitelist)?;
        self.async_task(
            "Async.DR_task.create",
            json!([self.session, sr_type, config, whitelist]),
        )
        .await
    }

    /// Destroy a DR task. Introduced SRs stay managed until they are forgotten.
    pub async fn destroy_dr_task(&self, uuid: &str) -> Result<(), ApiError> {
        if self.settings.read_only {
            return Err(ApiError::Xen(
                "This connection is read-only. Changes are blocked by the Rust backend.".into(),
            ));
        }
        validate_dr_uuid(uuid)?;
        let task_ref: String = self
            .request("DR_task.get_by_uuid", json!([self.session, uuid]))
            .await?;
        let record: Value = self
            .request("DR_task.get_record", json!([self.session, task_ref]))
            .await?;
        if record["uuid"].as_str() != Some(uuid) {
            return Err(ApiError::Protocol);
        }
        self.async_task("Async.DR_task.destroy", json!([self.session, task_ref]))
            .await
    }
}

pub fn validate_dr_task(
    sr_type: &str,
    device_config: &[(String, String)],
    whitelist: &[String],
) -> Result<serde_json::Map<String, Value>, ApiError> {
    if !DR_TYPES.contains(&sr_type) {
        return Err(ApiError::Xen(
            "Choose a supported DR storage type: NFS, iSCSI, HBA or FCoE.".into(),
        ));
    }
    let mut config = serde_json::Map::new();
    for (key, value) in device_config {
        let key = key.trim();
        if key.is_empty()
            || key.chars().count() > 128
            || key.chars().any(char::is_control)
            || value.chars().count() > 2048
            || value.chars().any(char::is_control)
        {
            return Err(ApiError::Xen(
                "DR device settings must have a short key and a plain value.".into(),
            ));
        }
        if config
            .insert(key.to_owned(), Value::String(value.trim().to_owned()))
            .is_some()
        {
            return Err(ApiError::Xen(
                "DR device setting keys must be unique.".into(),
            ));
        }
    }
    if config.len() > 32 {
        return Err(ApiError::Xen(
            "A DR device can have at most 32 settings.".into(),
        ));
    }
    for key in required_config_keys(sr_type) {
        if config
            .get(*key)
            .and_then(Value::as_str)
            .is_none_or(str::is_empty)
        {
            return Err(ApiError::Xen(format!(
                "The {key} setting is required for {sr_type} storage."
            )));
        }
    }
    let mut unique = HashSet::new();
    for uuid in whitelist {
        let uuid = uuid.trim();
        validate_dr_uuid(uuid)?;
        if !unique.insert(uuid.to_owned()) {
            return Err(ApiError::Xen(
                "Each DR SR UUID can only appear once.".into(),
            ));
        }
    }
    if whitelist.is_empty() {
        return Err(ApiError::Xen(
            "Enter at least one SR UUID discovered on the DR storage.".into(),
        ));
    }
    if whitelist.len() > 32 {
        return Err(ApiError::Xen(
            "A DR task can introduce at most 32 SRs.".into(),
        ));
    }
    Ok(config)
}

fn required_config_keys(sr_type: &str) -> &'static [&'static str] {
    match sr_type {
        "nfs" => &["server", "serverpath"],
        "lvmoiscsi" => &["target", "targetIQN"],
        _ => &[],
    }
}

fn validate_dr_uuid(uuid: &str) -> Result<(), ApiError> {
    if uuid::Uuid::parse_str(uuid.trim()).is_err() {
        return Err(ApiError::Xen(
            "Enter a standard SR UUID, for example 12345678-1234-1234-1234-123456789abc.".into(),
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const UUID_A: &str = "11111111-2222-3333-4444-555555555555";
    const UUID_B: &str = "aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee";

    #[test]
    fn dr_types_and_required_settings_are_validated() {
        let config = vec![
            ("server".into(), "192.0.2.1".into()),
            ("serverpath".into(), "/export/dr".into()),
        ];
        assert!(validate_dr_task("nfs", &config, &[UUID_A.into()]).is_ok());
        assert!(validate_dr_task("file", &config, &[UUID_A.into()]).is_err());
        assert!(validate_dr_task("", &config, &[UUID_A.into()]).is_err());
        assert!(validate_dr_task("nfs", &[], &[UUID_A.into()]).is_err());
        assert!(
            validate_dr_task(
                "nfs",
                &[("server".into(), "192.0.2.1".into())],
                &[UUID_A.into()]
            )
            .is_err()
        );
        assert!(
            validate_dr_task(
                "lvmoiscsi",
                &[
                    ("target".into(), "192.0.2.2".into()),
                    ("targetIQN".into(), "iqn.2026-01.example".into()),
                ],
                &[UUID_A.into()]
            )
            .is_ok()
        );
        assert!(validate_dr_task("hba", &[], &[UUID_A.into()]).is_ok());
    }

    #[test]
    fn dr_device_settings_are_sane_and_unique() {
        let duplicate = vec![
            ("server".into(), "192.0.2.1".into()),
            ("server".into(), "192.0.2.2".into()),
            ("serverpath".into(), "/export/dr".into()),
        ];
        assert!(validate_dr_task("nfs", &duplicate, &[UUID_A.into()]).is_err());
        let control = vec![
            ("server".into(), "192.0.2.1\n".into()),
            ("serverpath".into(), "/export/dr".into()),
        ];
        assert!(validate_dr_task("nfs", &control, &[UUID_A.into()]).is_err());
        let long_value = vec![
            ("server".into(), "1".repeat(2049)),
            ("serverpath".into(), "/export/dr".into()),
        ];
        assert!(validate_dr_task("nfs", &long_value, &[UUID_A.into()]).is_err());
    }

    #[test]
    fn dr_whitelist_requires_valid_unique_uuids() {
        assert!(validate_dr_task("hba", &[], &[]).is_err());
        assert!(validate_dr_task("hba", &[], &["not-a-uuid".into()]).is_err());
        assert!(validate_dr_task("hba", &[], &[UUID_A.into(), UUID_A.into()]).is_err());
        assert!(validate_dr_task("hba", &[], &[UUID_A.into(), UUID_B.into()]).is_ok());
        let many: Vec<String> = (0..33)
            .map(|index| format!("{:08x}-0000-0000-0000-000000000000", index))
            .collect();
        assert!(validate_dr_task("hba", &[], &many).is_err());
    }
}
