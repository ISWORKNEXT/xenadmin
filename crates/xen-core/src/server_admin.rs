//! Server administration. Dialog data deliberately excludes credentials and sessions.
use crate::xapi::{ApiError, Connection};
use serde_json::{Value, json};

#[derive(Clone)]
pub struct Interface {
    pub uuid: String,
    pub name: String,
    pub management: bool,
    pub mode: String,
    pub ip: String,
    pub mask: String,
    pub gateway: String,
    pub dns: String,
}
#[derive(Clone, Default)]
pub struct ServerInfo {
    pub name: String,
    pub pool_name: String,
    pub hosts: Vec<(String, String, bool)>,
    pub maintenance: bool,
    pub memory_mib: u64,
    pub min_memory_mib: u64,
    pub max_memory_mib: u64,
    pub interfaces: Vec<Interface>,
    pub running_vms: Vec<(String, String)>,
}
#[derive(Clone)]
pub enum ServerChange {
    Memory(u64),
    Password {
        old: String,
        new: String,
    },
    Ip(Interface),
    RemoveCrashDumps,
    NewPool(String),
    JoinPool {
        address: String,
        username: String,
        password: String,
    },
    Eject(String),
    Standalone,
    RotateSecret,
    Backup(std::path::PathBuf),
    StatusReport(std::path::PathBuf),
    Restore(std::path::PathBuf),
}
fn text(v: &Value, key: &str) -> String {
    v[key].as_str().unwrap_or_default().into()
}
fn number(v: &Value, key: &str) -> u64 {
    v[key]
        .as_u64()
        .or_else(|| v[key].as_str()?.parse().ok())
        .unwrap_or_default()
}
impl Connection {
    pub(crate) async fn host_boot_time(&self, uuid: &str) -> Result<String, ApiError> {
        let reference: String = self
            .request("host.get_by_uuid", json!([self.session, uuid]))
            .await?;
        let host: Value = self
            .request("host.get_record", json!([self.session, reference]))
            .await?;
        Ok(host["other_config"]["boot_time"]
            .as_str()
            .unwrap_or_default()
            .into())
    }
    pub async fn server_info(&self, uuid: &str) -> Result<ServerInfo, ApiError> {
        let host: String = self
            .request("host.get_by_uuid", json!([self.session, uuid]))
            .await?;
        let record: Value = self
            .request("host.get_record", json!([self.session, host]))
            .await?;
        let (vms, pifs) = tokio::try_join!(self.records("VM"), self.records("PIF"))?;
        let mut info = ServerInfo {
            name: text(&record, "name_label"),
            maintenance: record["other_config"]["MAINTENANCE_MODE"].as_str() == Some("true"),
            ..Default::default()
        };
        let pools = self.records("pool").await?;
        let pool = pools.values().next().ok_or(ApiError::Protocol)?;
        info.pool_name = text(pool, "name_label");
        info.hosts = self
            .records("host")
            .await?
            .into_iter()
            .map(|(reference, v)| {
                (
                    text(&v, "uuid"),
                    text(&v, "name_label"),
                    pool["master"].as_str() == Some(&reference),
                )
            })
            .collect();
        for vm in vms
            .values()
            .filter(|v| v["resident_on"].as_str() == Some(&host))
        {
            if vm["is_control_domain"].as_bool() == Some(true) {
                info.memory_mib = number(vm, "memory_dynamic_max") / 1048576;
                info.min_memory_mib = number(vm, "memory_static_min").div_ceil(1048576);
                let metrics: Value = self
                    .request(
                        "host_metrics.get_record",
                        json!([self.session, record["metrics"]]),
                    )
                    .await?;
                info.max_memory_mib =
                    (info.memory_mib + number(&metrics, "memory_free") / 1048576).min(256 * 1024);
            } else if matches!(vm["power_state"].as_str(), Some("Running" | "Paused")) {
                info.running_vms.push((
                    text(vm, "name_label"),
                    if vm["allowed_operations"]
                        .as_array()
                        .is_some_and(|ops| ops.iter().any(|op| op == "suspend"))
                    {
                        "Suspend or shut down"
                    } else {
                        "Shut down"
                    }
                    .into(),
                ));
            }
        }
        for pif in pifs.values().filter(|p| p["host"].as_str() == Some(&host)) {
            info.interfaces.push(Interface {
                uuid: text(pif, "uuid"),
                name: text(pif, "device"),
                management: pif["management"].as_bool() == Some(true),
                mode: text(pif, "ip_configuration_mode"),
                ip: text(pif, "IP"),
                mask: text(pif, "netmask"),
                gateway: text(pif, "gateway"),
                dns: text(pif, "DNS"),
            });
        }
        info.interfaces.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(info)
    }
    pub async fn server_change(&self, uuid: &str, change: ServerChange) -> Result<(), ApiError> {
        if self.settings.read_only {
            return Err(ApiError::Xen("This connection is read-only.".into()));
        }
        let host: String = self
            .request("host.get_by_uuid", json!([self.session, uuid]))
            .await?;
        let status_report = matches!(&change, ServerChange::StatusReport(_));
        match change {
            ServerChange::Memory(mib) => {
                let info = self.server_info(uuid).await?;
                if !info.maintenance {
                    return Err(ApiError::Xen(
                        "Enter maintenance mode before changing control domain memory.".into(),
                    ));
                }
                if mib < info.min_memory_mib || mib > info.max_memory_mib {
                    return Err(ApiError::Xen(
                        "Memory must be within the available control domain range.".into(),
                    ));
                }
                let vms = self.records("VM").await?;
                let (dom0, _) = vms
                    .iter()
                    .find(|(_, v)| {
                        v["is_control_domain"].as_bool() == Some(true)
                            && v["resident_on"].as_str() == Some(&host)
                    })
                    .ok_or(ApiError::Protocol)?;
                self.request::<Value>("VM.set_memory", json!([self.session, dom0, mib * 1048576]))
                    .await?;
                self.async_task("Async.host.reboot", json!([self.session, host]))
                    .await?;
            }
            ServerChange::Password { old, new } => {
                if old.is_empty() || new.is_empty() {
                    return Err(ApiError::Xen("Enter the current and new passwords.".into()));
                }
                self.request::<Value>("session.change_password", json!([self.session, old, new]))
                    .await?;
            }
            ServerChange::Ip(pif) => {
                validate_ip(&pif)?;
                let reference: String = self
                    .request("PIF.get_by_uuid", json!([self.session, pif.uuid]))
                    .await?;
                let record: Value = self
                    .request("PIF.get_record", json!([self.session, reference]))
                    .await?;
                if record["host"].as_str() != Some(&host) {
                    return Err(ApiError::Xen("Interface belongs to another server.".into()));
                }
                self.async_task(
                    "Async.PIF.reconfigure_ip",
                    json!([
                        self.session,
                        reference,
                        pif.mode,
                        pif.ip,
                        pif.mask,
                        pif.gateway,
                        pif.dns
                    ]),
                )
                .await?;
                if pif.management && record["management"].as_bool() != Some(true) {
                    self.async_task(
                        "Async.host.management_reconfigure",
                        json!([self.session, reference]),
                    )
                    .await?;
                }
            }
            ServerChange::NewPool(name) => {
                if name.trim().is_empty() || name.chars().count() > 255 {
                    return Err(ApiError::Xen(
                        "Enter a pool name between 1 and 255 characters.".into(),
                    ));
                }
                if self.records("host").await?.len() != 1 {
                    return Err(ApiError::Xen(
                        "Create a new pool from a standalone server.".into(),
                    ));
                }
                let pools = self.records("pool").await?;
                let (reference, _) = pools.iter().next().ok_or(ApiError::Protocol)?;
                self.request::<Value>(
                    "pool.set_name_label",
                    json!([self.session, reference, name.trim()]),
                )
                .await?;
            }
            ServerChange::JoinPool {
                address,
                username,
                password,
            } => {
                if self.records("host").await?.len() != 1 {
                    return Err(ApiError::Xen(
                        "Only a standalone server can join another pool.".into(),
                    ));
                }
                if address.trim().is_empty() || username.trim().is_empty() || password.is_empty() {
                    return Err(ApiError::Xen(
                        "Enter the coordinator address and credentials.".into(),
                    ));
                }
                self.async_task(
                    "Async.pool.join",
                    json!([self.session, address, username, password]),
                )
                .await?;
            }
            ServerChange::Eject(uuid) => {
                let reference: String = self
                    .request("host.get_by_uuid", json!([self.session, uuid]))
                    .await?;

                let pools = self.records("pool").await?;
                let pool = pools.values().next().ok_or(ApiError::Protocol)?;
                if pool["master"].as_str() == Some(&reference) {
                    return Err(ApiError::Xen(
                        "The coordinator cannot be removed from its pool.".into(),
                    ));
                }
                if pool["ha_enabled"].as_bool() == Some(true) {
                    return Err(ApiError::Xen(
                        "Disable High Availability before removing a server.".into(),
                    ));
                }
                self.async_task("Async.pool.eject", json!([self.session, reference]))
                    .await?;
            }
            ServerChange::Standalone => {
                let pools = self.records("pool").await?;
                let (reference, pool) = pools.iter().next().ok_or(ApiError::Protocol)?;
                if pool["ha_enabled"].as_bool() == Some(true) {
                    return Err(ApiError::Xen("Disable High Availability first.".into()));
                }
                for (member, _) in self
                    .records("host")
                    .await?
                    .into_iter()
                    .filter(|(member, _)| pool["master"].as_str() != Some(member))
                {
                    self.async_task("Async.pool.eject", json!([self.session, member]))
                        .await?;
                }
                self.request::<Value>("pool.set_name_label", json!([self.session, reference, ""]))
                    .await?;
            }
            ServerChange::RotateSecret => {
                let pools = self.records("pool").await?;
                let pool = pools.values().next().ok_or(ApiError::Protocol)?;
                if pool["ha_enabled"].as_bool() == Some(true)
                    || pool["other_config"]
                        .as_object()
                        .is_some_and(|config| config.contains_key("rolling_upgrade_in_progress"))
                {
                    return Err(ApiError::Xen(
                        "Pool secret rotation is unavailable during HA or a rolling upgrade."
                            .into(),
                    ));
                }
                self.async_task("Async.pool.rotate_secret", json!([self.session]))
                    .await?;
            }
            ServerChange::Backup(path) | ServerChange::StatusReport(path) => {
                use tokio::io::AsyncWriteExt;
                let record: Value = self
                    .request("host.get_record", json!([self.session, host]))
                    .await?;
                let transport = self
                    .transport
                    .for_member(
                        record["address"].as_str().ok_or(ApiError::Protocol)?,
                        self.settings.tls,
                    )
                    .map_err(|_| ApiError::Connection)?;
                let mut response = if status_report {
                    transport.system_status(&self.session).await
                } else {
                    transport.host_backup(&self.session).await
                }
                .map_err(|_| ApiError::Connection)?;
                if !response.status().is_success() {
                    return Err(ApiError::Http(response.status().as_u16()));
                }
                let mut options = tokio::fs::OpenOptions::new();
                options.write(true).create_new(true);
                #[cfg(unix)]
                options.mode(0o600);
                let mut file = options
                    .open(&path)
                    .await
                    .map_err(|e| ApiError::Xen(e.to_string()))?;
                let result = async {
                    let mut bytes = 0;
                    while let Some(chunk) =
                        response.chunk().await.map_err(|_| ApiError::Connection)?
                    {
                        file.write_all(&chunk)
                            .await
                            .map_err(|e| ApiError::Xen(e.to_string()))?;
                        bytes += chunk.len();
                    }
                    file.flush()
                        .await
                        .map_err(|e| ApiError::Xen(e.to_string()))?;
                    if bytes == 0 {
                        return Err(ApiError::Xen("The server returned an empty backup.".into()));
                    }
                    Ok(())
                }
                .await;
                if result.is_err() {
                    drop(file);
                    let _ = tokio::fs::remove_file(path).await;
                }
                result?;
            }
            ServerChange::Restore(path) => {
                let record: Value = self
                    .request("host.get_record", json!([self.session, host]))
                    .await?;
                let transport = self
                    .transport
                    .for_member(
                        record["address"].as_str().ok_or(ApiError::Protocol)?,
                        self.settings.tls,
                    )
                    .map_err(|_| ApiError::Connection)?;
                let file = tokio::fs::File::open(path)
                    .await
                    .map_err(|e| ApiError::Xen(e.to_string()))?;
                let length = file
                    .metadata()
                    .await
                    .map_err(|e| ApiError::Xen(e.to_string()))?
                    .len();
                if length == 0 {
                    return Err(ApiError::Xen("Select a non-empty server backup.".into()));
                }
                let response = transport
                    .host_restore(&self.session, file, length)
                    .await
                    .map_err(|_| ApiError::Connection)?;
                if !response.status().is_success() {
                    return Err(ApiError::Http(response.status().as_u16()));
                }
            }
            ServerChange::RemoveCrashDumps => {
                for (reference, dump) in self.records("host_crashdump").await? {
                    if dump["host"].as_str() == Some(&host) {
                        self.async_task(
                            "Async.host_crashdump.destroy",
                            json!([self.session, reference]),
                        )
                        .await?;
                    }
                }
            }
        }
        Ok(())
    }
}
pub fn validate_ip(pif: &Interface) -> Result<(), ApiError> {
    if !matches!(pif.mode.as_str(), "DHCP" | "Static" | "None") {
        return Err(ApiError::Xen("Select an IP configuration mode.".into()));
    }
    if pif.management && pif.mode == "None" {
        return Err(ApiError::Xen(
            "The management interface requires an IP address.".into(),
        ));
    }
    if pif.mode == "Static" {
        for (label, value) in [("IP address", &pif.ip), ("Subnet mask", &pif.mask)] {
            if value.parse::<std::net::Ipv4Addr>().is_err() {
                return Err(ApiError::Xen(format!("Enter a valid {label}.")));
            }
        }
        if !pif.gateway.is_empty() && pif.gateway.parse::<std::net::Ipv4Addr>().is_err() {
            return Err(ApiError::Xen("Enter a valid gateway.".into()));
        }
    }
    for dns in pif.dns.split([',', ' ']).filter(|s| !s.is_empty()) {
        if dns.parse::<std::net::IpAddr>().is_err() {
            return Err(ApiError::Xen("Enter valid DNS server addresses.".into()));
        }
    }
    Ok(())
}
