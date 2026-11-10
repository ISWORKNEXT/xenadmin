//! PVS site, server, proxy and cache-storage write actions.
use std::collections::HashSet;

use serde::Serialize;
use serde_json::{Value, json};

use crate::operations::wait_task_value;
use crate::xapi::{ApiError, Connection};

/// 1 GiB in MiB.
pub const MIN_CACHE_MIB: u64 = 1024;
/// 2 TiB in MiB.
pub const MAX_CACHE_MIB: u64 = 2 * 1024 * 1024;

#[derive(Clone, Debug, Serialize)]
pub struct PvsVif {
    pub uuid: String,
    pub vm_name: String,
    pub network_name: String,
    pub mac: String,
}

impl Connection {
    pub async fn introduce_pvs_site(&self, name: &str, description: &str) -> Result<(), ApiError> {
        if self.settings.read_only {
            return Err(ApiError::Xen(
                "This connection is read-only. Changes are blocked by the Rust backend.".into(),
            ));
        }
        validate_pvs_site(name, description)?;
        let task: String = self
            .request(
                "Async.PVS_site.introduce",
                json!([self.session, name.trim(), description, ""]),
            )
            .await?;
        let _ = wait_task_value(self, &task).await?;
        Ok(())
    }

    pub async fn forget_pvs_site(&self, uuid: &str) -> Result<(), ApiError> {
        if self.settings.read_only {
            return Err(ApiError::Xen(
                "This connection is read-only. Changes are blocked by the Rust backend.".into(),
            ));
        }
        let site_ref = self.pvs_ref("PVS_site", uuid).await?;
        let (proxies, servers) =
            tokio::join!(self.records("PVS_proxy"), self.records("PVS_server"));
        let proxies = proxies.unwrap_or_default();
        if proxies
            .values()
            .any(|proxy| proxy["site"].as_str() == Some(site_ref.as_str()))
        {
            return Err(ApiError::Xen(
                "This PVS site still has proxies. Remove them before deleting the site.".into(),
            ));
        }
        let servers = servers.unwrap_or_default();
        for (server_ref, server) in &servers {
            if server["site"].as_str() == Some(site_ref.as_str()) {
                self.async_task("Async.PVS_server.forget", json!([self.session, server_ref]))
                    .await?;
            }
        }
        self.async_task("Async.PVS_site.forget", json!([self.session, site_ref]))
            .await
    }

    pub async fn introduce_pvs_server(
        &self,
        site_uuid: &str,
        addresses: &[String],
        first_port: u16,
        last_port: u16,
    ) -> Result<(), ApiError> {
        if self.settings.read_only {
            return Err(ApiError::Xen(
                "This connection is read-only. Changes are blocked by the Rust backend.".into(),
            ));
        }
        let addresses = validate_pvs_addresses(addresses)?;
        validate_pvs_ports(first_port, last_port)?;
        let site_ref = self.pvs_ref("PVS_site", site_uuid).await?;
        self.async_task(
            "Async.PVS_server.introduce",
            json!([self.session, addresses, first_port, last_port, site_ref]),
        )
        .await
    }

    pub async fn forget_pvs_server(&self, uuid: &str) -> Result<(), ApiError> {
        if self.settings.read_only {
            return Err(ApiError::Xen(
                "This connection is read-only. Changes are blocked by the Rust backend.".into(),
            ));
        }
        let server_ref = self.pvs_ref("PVS_server", uuid).await?;
        self.async_task("Async.PVS_server.forget", json!([self.session, server_ref]))
            .await
    }

    pub async fn create_pvs_proxy(&self, site_uuid: &str, vif_uuid: &str) -> Result<(), ApiError> {
        if self.settings.read_only {
            return Err(ApiError::Xen(
                "This connection is read-only. Changes are blocked by the Rust backend.".into(),
            ));
        }
        let site_ref = self.pvs_ref("PVS_site", site_uuid).await?;
        let vif_ref: String = self
            .request("VIF.get_by_uuid", json!([self.session, vif_uuid]))
            .await?;
        let vif: Value = self
            .request("VIF.get_record", json!([self.session, vif_ref]))
            .await?;
        if vif["uuid"].as_str() != Some(vif_uuid) {
            return Err(ApiError::Protocol);
        }
        if vif["currently_attached"].as_bool() != Some(true) {
            return Err(ApiError::Xen(
                "The selected VM interface must be connected before enabling PVS read caching."
                    .into(),
            ));
        }
        let proxies = self.records("PVS_proxy").await.unwrap_or_default();
        if proxies.values().any(|proxy| {
            proxy["VIF"].as_str() == Some(vif_ref.as_str())
                && proxy["site"].as_str() == Some(site_ref.as_str())
        }) {
            return Err(ApiError::Xen(
                "This interface already has PVS read caching for the selected site.".into(),
            ));
        }
        self.async_task(
            "Async.PVS_proxy.create",
            json!([self.session, site_ref, vif_ref]),
        )
        .await
    }

    pub async fn destroy_pvs_proxy(&self, uuid: &str) -> Result<(), ApiError> {
        if self.settings.read_only {
            return Err(ApiError::Xen(
                "This connection is read-only. Changes are blocked by the Rust backend.".into(),
            ));
        }
        let proxy_ref = self.pvs_ref("PVS_proxy", uuid).await?;
        self.async_task("Async.PVS_proxy.destroy", json!([self.session, proxy_ref]))
            .await
    }

    pub async fn create_pvs_cache(
        &self,
        site_uuid: &str,
        host_uuid: &str,
        sr_uuid: &str,
        size_mib: u64,
    ) -> Result<(), ApiError> {
        if self.settings.read_only {
            return Err(ApiError::Xen(
                "This connection is read-only. Changes are blocked by the Rust backend.".into(),
            ));
        }
        validate_pvs_cache_size(size_mib)?;
        let site_ref = self.pvs_ref("PVS_site", site_uuid).await?;
        let host_ref: String = self
            .request("host.get_by_uuid", json!([self.session, host_uuid]))
            .await?;
        let sr_ref: String = self
            .request("SR.get_by_uuid", json!([self.session, sr_uuid]))
            .await?;
        let sr: Value = self
            .request("SR.get_record", json!([self.session, sr_ref]))
            .await?;
        if sr["content_type"].as_str() == Some("iso") {
            return Err(ApiError::Xen(
                "PVS cache storage needs a writable user SR.".into(),
            ));
        }
        let (caches, hosts) = tokio::join!(self.records("PVS_cache_storage"), self.records("host"));
        let caches = caches.unwrap_or_default();
        if caches.values().any(|cache| {
            cache["site"].as_str() == Some(site_ref.as_str())
                && cache["host"].as_str() == Some(host_ref.as_str())
        }) {
            return Err(ApiError::Xen(
                "This host already has cache storage for the selected PVS site. Remove it first."
                    .into(),
            ));
        }
        let _ = hosts?;
        let record = json!({
            "uuid": uuid::Uuid::now_v7().to_string(),
            "host": host_ref,
            "SR": sr_ref,
            "site": site_ref,
            "size": (size_mib.saturating_mul(1024 * 1024)).to_string(),
            "VDI": "OpaqueRef:NULL"
        });
        self.async_task(
            "Async.PVS_cache_storage.create",
            json!([self.session, record]),
        )
        .await
    }

    pub async fn destroy_pvs_cache(&self, uuid: &str) -> Result<(), ApiError> {
        if self.settings.read_only {
            return Err(ApiError::Xen(
                "This connection is read-only. Changes are blocked by the Rust backend.".into(),
            ));
        }
        let cache_ref = self.pvs_ref("PVS_cache_storage", uuid).await?;
        self.async_task(
            "Async.PVS_cache_storage.destroy",
            json!([self.session, cache_ref]),
        )
        .await
    }

    pub async fn pvs_vif_catalog(&self) -> Result<Vec<PvsVif>, ApiError> {
        let (vifs, vms, networks) = tokio::join!(
            self.records("VIF"),
            self.records("VM"),
            self.records("network")
        );
        let (vifs, vms, networks) = (vifs?, vms?, networks?);
        let mut catalog: Vec<PvsVif> = vifs
            .values()
            .filter(|vif| vif["currently_attached"].as_bool() == Some(true))
            .filter_map(|vif| {
                let vm = vms.get(&text(vif, "VM"))?;
                if vm["is_a_template"].as_bool() == Some(true)
                    || vm["is_snapshot"].as_bool() == Some(true)
                {
                    return None;
                }
                Some(PvsVif {
                    uuid: text(vif, "uuid"),
                    vm_name: text(vm, "name_label"),
                    network_name: networks
                        .get(&text(vif, "network"))
                        .map(|network| text(network, "name_label"))
                        .unwrap_or_default(),
                    mac: text(vif, "MAC"),
                })
            })
            .collect();
        catalog.sort_by(|left, right| {
            left.vm_name
                .cmp(&right.vm_name)
                .then(left.network_name.cmp(&right.network_name))
        });
        catalog.truncate(500);
        Ok(catalog)
    }

    async fn pvs_ref(&self, class: &str, uuid: &str) -> Result<String, ApiError> {
        let reference: String = self
            .request(&format!("{class}.get_by_uuid"), json!([self.session, uuid]))
            .await?;
        let record: Value = self
            .request(
                &format!("{class}.get_record"),
                json!([self.session, reference]),
            )
            .await?;
        if record["uuid"].as_str() != Some(uuid) {
            return Err(ApiError::Protocol);
        }
        Ok(reference)
    }
}

fn text(record: &Value, key: &str) -> String {
    record[key].as_str().unwrap_or_default().to_owned()
}

pub fn validate_pvs_site(name: &str, description: &str) -> Result<(), ApiError> {
    let name = name.trim();
    if name.is_empty() || name.chars().count() > 255 {
        return Err(ApiError::Xen(
            "Enter a PVS site name between 1 and 255 characters.".into(),
        ));
    }
    if description.chars().count() > 1024 {
        return Err(ApiError::Xen(
            "The description must be 1024 characters or fewer.".into(),
        ));
    }
    Ok(())
}

pub fn validate_pvs_addresses(addresses: &[String]) -> Result<Vec<String>, ApiError> {
    let mut unique = HashSet::new();
    let mut cleaned = Vec::new();
    for address in addresses {
        let address = address.trim();
        if address.is_empty() {
            continue;
        }
        if address.chars().count() > 255
            || address.chars().any(char::is_whitespace)
            || address.chars().any(char::is_control)
            || !valid_host(address)
        {
            return Err(ApiError::Xen(format!(
                "'{address}' is not a valid PVS server address."
            )));
        }
        if unique.insert(address.to_owned()) {
            cleaned.push(address.to_owned());
        }
    }
    if cleaned.is_empty() {
        return Err(ApiError::Xen(
            "Enter at least one PVS server address.".into(),
        ));
    }
    if cleaned.len() > 16 {
        return Err(ApiError::Xen(
            "A PVS server can have at most 16 addresses.".into(),
        ));
    }
    Ok(cleaned)
}

fn valid_host(address: &str) -> bool {
    // The pool validates PVS server addresses as IP addresses; host names are
    // rejected by XAPI with INVALID_IP_ADDRESS_SPECIFIED.
    if address.parse::<std::net::IpAddr>().is_ok() {
        return true;
    }
    address.starts_with('[')
        && address.ends_with(']')
        && address[1..address.len() - 1]
            .parse::<std::net::IpAddr>()
            .is_ok()
}

pub fn validate_pvs_ports(first_port: u16, last_port: u16) -> Result<(), ApiError> {
    if first_port == 0 || last_port == 0 {
        return Err(ApiError::Xen(
            "PVS server ports must be between 1 and 65535.".into(),
        ));
    }
    if first_port > last_port {
        return Err(ApiError::Xen(
            "The first PVS port must not be higher than the last port.".into(),
        ));
    }
    Ok(())
}

pub fn validate_pvs_cache_size(size_mib: u64) -> Result<(), ApiError> {
    if !(MIN_CACHE_MIB..=MAX_CACHE_MIB).contains(&size_mib) {
        return Err(ApiError::Xen(
            "PVS cache size must be between 1 GiB and 2 TiB.".into(),
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn site_fields_are_validated() {
        assert!(validate_pvs_site("site", "").is_ok());
        assert!(validate_pvs_site("", "").is_err());
        assert!(validate_pvs_site(&"x".repeat(256), "").is_err());
        assert!(validate_pvs_site("site", &"x".repeat(1025)).is_err());
    }

    #[test]
    fn server_addresses_are_normalized_and_validated() {
        let addresses = validate_pvs_addresses(&[
            " 10.0.0.1 ".into(),
            "10.0.0.1".into(),
            "192.0.2.20".into(),
            "[2001:db8::1]".into(),
            "".into(),
        ])
        .unwrap();
        assert_eq!(addresses, vec!["10.0.0.1", "192.0.2.20", "[2001:db8::1]"]);
        assert!(validate_pvs_addresses(&[]).is_err());
        assert!(validate_pvs_addresses(&["bad address".into()]).is_err());
        assert!(validate_pvs_addresses(&["pvs.example.com".into()]).is_err());
        assert!(validate_pvs_addresses(&["[not-ip]".into()]).is_err());
        assert!(validate_pvs_addresses(&[format!("h{}", "x".repeat(300))]).is_err());
    }

    #[test]
    fn ports_and_cache_sizes_are_validated() {
        assert!(validate_pvs_ports(6901, 6910).is_ok());
        assert!(validate_pvs_ports(6910, 6901).is_err());
        assert!(validate_pvs_ports(0, 6900).is_err());
        assert!(validate_pvs_cache_size(1024).is_ok());
        assert!(validate_pvs_cache_size(MAX_CACHE_MIB).is_ok());
        assert!(validate_pvs_cache_size(1023).is_err());
        assert!(validate_pvs_cache_size(MAX_CACHE_MIB + 1).is_err());
        assert!(validate_pvs_cache_size(1024).is_ok());
    }
}
