//! Pool HA and workload-balancing write actions.
use std::collections::HashSet;

use serde_json::{Value, json};

use crate::xapi::{ApiError, Connection};

impl Connection {
    /// Enable HA with one to three shared heartbeat SRs.
    pub async fn enable_ha(
        &self,
        heartbeat_sr_uuids: Vec<String>,
        failures_to_tolerate: u64,
    ) -> Result<(), ApiError> {
        if self.settings.read_only {
            return Err(ApiError::Xen(
                "This connection is read-only. Changes are blocked by the Rust backend.".into(),
            ));
        }
        let (pool_ref, pool) = self.pool_action_record().await?;
        if pool["ha_enabled"].as_bool() == Some(true) {
            return Err(ApiError::Xen(
                "High availability is already enabled.".into(),
            ));
        }
        let hosts = self.records("host").await?;
        let enabled_hosts = hosts
            .values()
            .filter(|host| host["enabled"].as_bool().unwrap_or(true))
            .count() as u64;
        validate_ha_failures(failures_to_tolerate, enabled_hosts)?;
        let srs = self.records("SR").await?;
        let heartbeat_refs = validate_ha_heartbeats(&heartbeat_sr_uuids, &srs)?;
        self.request::<Value>(
            "Pool.set_ha_host_failures_to_tolerate",
            json!([self.session, pool_ref, failures_to_tolerate]),
        )
        .await?;
        if let Err(error) = self
            .async_task(
                "Async.Pool.enable_ha",
                json!([self.session, heartbeat_refs, {}]),
            )
            .await
        {
            let _ = self
                .request::<Value>(
                    "Pool.set_ha_host_failures_to_tolerate",
                    json!([self.session, pool_ref, 0u64]),
                )
                .await;
            return Err(error);
        }
        Ok(())
    }

    pub async fn disable_ha(&self) -> Result<(), ApiError> {
        if self.settings.read_only {
            return Err(ApiError::Xen(
                "This connection is read-only. Changes are blocked by the Rust backend.".into(),
            ));
        }
        let (pool_ref, pool) = self.pool_action_record().await?;
        if pool["ha_enabled"].as_bool() != Some(true) {
            return Ok(());
        }
        self.async_task("Async.Pool.disable_ha", json!([self.session]))
            .await?;
        let _ = self
            .request::<Value>(
                "Pool.set_ha_host_failures_to_tolerate",
                json!([self.session, pool_ref, 0u64]),
            )
            .await;
        Ok(())
    }

    /// Initialize workload balancing against a WLB server.
    pub async fn configure_wlb(
        &self,
        wlb_url: &str,
        wlb_username: &str,
        wlb_password: &str,
        xenserver_username: &str,
        xenserver_password: &str,
    ) -> Result<(), ApiError> {
        if self.settings.read_only {
            return Err(ApiError::Xen(
                "This connection is read-only. Changes are blocked by the Rust backend.".into(),
            ));
        }
        validate_wlb_config(
            wlb_url,
            wlb_username,
            wlb_password,
            xenserver_username,
            xenserver_password,
        )?;
        self.async_task(
            "Async.Pool.initialize_wlb",
            json!([
                self.session,
                wlb_url.trim(),
                wlb_username.trim(),
                wlb_password,
                xenserver_username.trim(),
                xenserver_password
            ]),
        )
        .await
    }

    pub async fn set_wlb_enabled(&self, enabled: bool) -> Result<(), ApiError> {
        if self.settings.read_only {
            return Err(ApiError::Xen(
                "This connection is read-only. Changes are blocked by the Rust backend.".into(),
            ));
        }
        let (pool_ref, pool) = self.pool_action_record().await?;
        if enabled && text(&pool, "wlb_url").is_empty() {
            return Err(ApiError::Xen(
                "Configure the WLB server before enabling workload balancing.".into(),
            ));
        }
        self.request::<Value>(
            "Pool.set_wlb_enabled",
            json!([self.session, pool_ref, enabled]),
        )
        .await?;
        Ok(())
    }

    pub async fn deconfigure_wlb(&self) -> Result<(), ApiError> {
        if self.settings.read_only {
            return Err(ApiError::Xen(
                "This connection is read-only. Changes are blocked by the Rust backend.".into(),
            ));
        }
        let (_, pool) = self.pool_action_record().await?;
        if pool["wlb_enabled"].as_bool() == Some(true) {
            let _ = self.set_wlb_enabled(false).await;
        }
        self.async_task("Async.Pool.deconfigure_wlb", json!([self.session]))
            .await
    }

    async fn pool_action_record(&self) -> Result<(String, Value), ApiError> {
        let pools = self.records("pool").await?;
        let Some((reference, pool)) = pools.iter().next() else {
            return Err(ApiError::Xen("No pool record is available.".into()));
        };
        Ok((reference.clone(), pool.clone()))
    }
}

fn validate_ha_heartbeats(
    selected: &[String],
    srs: &std::collections::HashMap<String, Value>,
) -> Result<Vec<String>, ApiError> {
    if selected.is_empty() {
        return Err(ApiError::Xen(
            "Select at least one shared heartbeat SR.".into(),
        ));
    }
    if selected.len() > 3 {
        return Err(ApiError::Xen("Select at most three heartbeat SRs.".into()));
    }
    let mut unique = HashSet::new();
    let mut refs = Vec::new();
    for uuid in selected {
        if !unique.insert(uuid.clone()) {
            return Err(ApiError::Xen("Heartbeat SRs must be unique.".into()));
        }
        let Some((reference, sr)) = srs
            .iter()
            .find(|(_, sr)| sr["uuid"].as_str() == Some(uuid.as_str()))
        else {
            return Err(ApiError::Xen(format!(
                "Heartbeat SR {uuid} is not present in this pool."
            )));
        };
        if sr["shared"].as_bool() != Some(true) || text(sr, "content_type") != "user" {
            return Err(ApiError::Xen(
                "Heartbeat SRs must be shared user storage, not local or ISO storage.".into(),
            ));
        }
        refs.push(reference.clone());
    }
    Ok(refs)
}

fn validate_ha_failures(failures: u64, enabled_hosts: u64) -> Result<(), ApiError> {
    if enabled_hosts == 0 {
        return Err(ApiError::Xen(
            "HA cannot be enabled without an enabled host.".into(),
        ));
    }
    if failures >= enabled_hosts {
        return Err(ApiError::Xen(format!(
            "HA cannot tolerate {failures} failure(s) with only {enabled_hosts} enabled host(s)."
        )));
    }
    Ok(())
}

fn validate_wlb_config(
    wlb_url: &str,
    wlb_username: &str,
    wlb_password: &str,
    xenserver_username: &str,
    xenserver_password: &str,
) -> Result<(), ApiError> {
    let url = wlb_url.trim();
    if url.is_empty()
        || url.chars().count() > 255
        || url.chars().any(char::is_whitespace)
        || url.chars().any(char::is_control)
    {
        return Err(ApiError::Xen(
            "Enter the WLB server URL, for example https://wlb.example.com:443.".into(),
        ));
    }
    if let Some((scheme, rest)) = url.split_once("://")
        && (!matches!(scheme, "http" | "https") || rest.is_empty())
    {
        return Err(ApiError::Xen(
            "The WLB URL scheme must be http or https.".into(),
        ));
    }
    for (label, value) in [
        ("WLB user name", wlb_username),
        ("WLB password", wlb_password),
        ("XenServer user name", xenserver_username),
        ("XenServer password", xenserver_password),
    ] {
        if value.is_empty() || value.chars().count() > 255 {
            return Err(ApiError::Xen(format!(
                "Enter the {label} (1–255 characters)."
            )));
        }
    }
    Ok(())
}

fn text(record: &Value, key: &str) -> String {
    record[key].as_str().unwrap_or_default().to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn srs() -> std::collections::HashMap<String, Value> {
        std::collections::HashMap::from([
            (
                "shared-ref".into(),
                json!({"uuid":"shared-uuid","shared":true,"content_type":"user"}),
            ),
            (
                "local-ref".into(),
                json!({"uuid":"local-uuid","shared":false,"content_type":"user"}),
            ),
            (
                "iso-ref".into(),
                json!({"uuid":"iso-uuid","shared":true,"content_type":"iso"}),
            ),
        ])
    }

    #[test]
    fn ha_heartbeats_require_shared_user_storage() {
        assert_eq!(
            validate_ha_heartbeats(&["shared-uuid".into()], &srs()).unwrap(),
            vec!["shared-ref"]
        );
        assert!(validate_ha_heartbeats(&[], &srs()).is_err());
        assert!(validate_ha_heartbeats(&["local-uuid".into()], &srs()).is_err());
        assert!(validate_ha_heartbeats(&["iso-uuid".into()], &srs()).is_err());
        assert!(
            validate_ha_heartbeats(&["shared-uuid".into(), "shared-uuid".into()], &srs()).is_err()
        );
    }

    #[test]
    fn ha_failure_tolerance_must_leave_a_host() {
        assert!(validate_ha_failures(0, 1).is_ok());
        assert!(validate_ha_failures(1, 2).is_ok());
        assert!(validate_ha_failures(1, 1).is_err());
        assert!(validate_ha_failures(0, 0).is_err());
    }

    #[test]
    fn wlb_configuration_is_validated() {
        assert!(
            validate_wlb_config(
                "https://wlb.example.com:443",
                "wlb-user",
                "wlb-pass",
                "root",
                "xen-pass"
            )
            .is_ok()
        );
        assert!(validate_wlb_config("", "u", "p", "root", "p").is_err());
        assert!(validate_wlb_config("ftp://wlb", "u", "p", "root", "p").is_err());
        assert!(validate_wlb_config("wlb url", "u", "p", "root", "p").is_err());
        assert!(validate_wlb_config("wlb:443", "u", "p", "root", "").is_err());
    }
}
