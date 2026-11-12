//! Physical interface, bond and VLAN management.
use serde::Serialize;
use serde_json::{Value, json};

use crate::xapi::{ApiError, Connection};

#[derive(Clone, Debug, Serialize)]
pub struct PifSummary {
    pub uuid: String,
    pub host_uuid: String,
    pub host_name: String,
    pub device: String,
    pub network_uuid: String,
    pub network_name: String,
    pub mac: String,
    pub mtu: u64,
    pub vlan: i64,
    pub physical: bool,
    pub sriov_capable: bool,
    pub management: bool,
    pub attached: bool,
    pub disallow_unplug: bool,
    pub bond_master: String,
    pub bond_network_uuid: String,
    pub ip: String,
    pub primary_address_type: String,
}

impl PifSummary {
    pub fn bond_candidate(&self) -> bool {
        self.bond_network_uuid.is_empty()
            && self.physical
            && !self.management
            && self.attached
            && self.vlan == -1
            && self.bond_master.is_empty()
    }

    pub fn vlan_candidate(&self) -> bool {
        self.physical
            && !self.management
            && self.attached
            && self.vlan == -1
            && self.bond_master.is_empty()
    }

    pub fn sriov_candidate(&self) -> bool {
        self.sriov_capable
            && self.physical
            && !self.management
            && self.attached
            && self.vlan == -1
            && self.bond_master.is_empty()
    }

    pub fn chin_candidate(&self) -> bool {
        self.management && self.attached && self.vlan == -1
    }
}

impl Connection {
    pub async fn pif_catalog(&self) -> Result<Vec<PifSummary>, ApiError> {
        let (pifs, hosts, networks, bonds) = tokio::join!(
            self.records("PIF"),
            self.records("host"),
            self.records("network"),
            self.records("Bond")
        );
        let (pifs, hosts, networks, bonds) = (pifs?, hosts?, networks?, bonds?);
        let mut catalog: Vec<PifSummary> = pifs
            .iter()
            .map(|(reference, pif)| {
                let host_ref = text(pif, "host");
                let network_ref = text(pif, "network");
                let host = hosts.get(&host_ref);
                let network = networks.get(&network_ref);
                PifSummary {
                    uuid: text(pif, "uuid"),
                    host_uuid: text(host.unwrap_or(&Value::Null), "uuid"),
                    host_name: text(host.unwrap_or(&Value::Null), "name_label"),
                    device: text(pif, "device"),
                    network_uuid: text(network.unwrap_or(&Value::Null), "uuid"),
                    network_name: text(network.unwrap_or(&Value::Null), "name_label"),
                    mac: text(pif, "MAC"),
                    mtu: number(pif, "MTU"),
                    vlan: number_i64(pif, "VLAN"),
                    physical: pif["physical"].as_bool().unwrap_or(false),
                    sriov_capable: pif["capabilities"]
                        .as_array()
                        .is_some_and(|caps| caps.iter().any(|c| c == "sriov")),
                    management: pif["management"].as_bool().unwrap_or(false),
                    attached: pif["currently_attached"].as_bool().unwrap_or(false),
                    disallow_unplug: pif["disallow_unplug"].as_bool().unwrap_or(false),
                    bond_network_uuid: bonds
                        .values()
                        .find(|bond| {
                            bond["master"].as_str() == Some(reference)
                                || bond["slaves"].as_array().is_some_and(|slaves| {
                                    slaves.iter().any(|s| s.as_str() == Some(reference))
                                })
                        })
                        .and_then(|bond| bond["master"].as_str())
                        .and_then(|master| pifs.get(master))
                        .and_then(|master| master["network"].as_str())
                        .and_then(|network| networks.get(network))
                        .map(|network| text(network, "uuid"))
                        .unwrap_or_default(),
                    bond_master: pif["bond_master_of"]
                        .as_array()
                        .and_then(|bonds| bonds.first())
                        .and_then(Value::as_str)
                        .and_then(|bond_ref| bonds.get(bond_ref))
                        .and_then(|bond| bond["master"].as_str())
                        .and_then(|master_ref| pifs.get(master_ref))
                        .map(|master| text(master, "uuid"))
                        .unwrap_or_default(),
                    ip: text(pif, "IP"),
                    primary_address_type: text(pif, "primary_address_type"),
                }
            })
            .collect();
        catalog.sort_by(|left, right| {
            left.host_name
                .cmp(&right.host_name)
                .then(left.device.cmp(&right.device))
        });
        Ok(catalog)
    }

    pub async fn network_pifs(&self, network_uuid: &str) -> Result<Vec<PifSummary>, ApiError> {
        let network_ref: String = self
            .request("network.get_by_uuid", json!([self.session, network_uuid]))
            .await?;
        Ok(self
            .pif_catalog()
            .await?
            .into_iter()
            .filter(|pif| {
                pif.network_uuid == network_uuid && {
                    let _ = &network_ref;
                    true
                }
            })
            .collect())
    }

    pub async fn set_pif_plugged(&self, pif_uuid: &str, plugged: bool) -> Result<(), ApiError> {
        if self.settings.read_only {
            return Err(ApiError::Xen(
                "This connection is read-only. Changes are blocked by the Rust backend.".into(),
            ));
        }
        let pif_ref: String = self
            .request("PIF.get_by_uuid", json!([self.session, pif_uuid]))
            .await?;
        let pif: Value = self
            .request("PIF.get_record", json!([self.session, pif_ref]))
            .await?;
        if pif["uuid"].as_str() != Some(pif_uuid) {
            return Err(ApiError::Protocol);
        }
        if pif["management"].as_bool() == Some(true) {
            return Err(ApiError::Xen(
                "The management interface is protected and cannot be plugged or unplugged here."
                    .into(),
            ));
        }
        if !plugged && pif["disallow_unplug"].as_bool() == Some(true) {
            return Err(ApiError::Xen(
                "This interface is marked as not unpluggable by the host.".into(),
            ));
        }
        if pif["currently_attached"].as_bool().unwrap_or(false) == plugged {
            return Ok(());
        }
        let method = if plugged {
            "Async.PIF.plug"
        } else {
            "Async.PIF.unplug"
        };
        self.async_task(method, json!([self.session, pif_ref]))
            .await
    }

    /// Create an SR-IOV logical network and enable it on the selected PFs.
    pub async fn create_sriov_network(
        &self,
        name: &str,
        description: &str,
        mtu: u64,
        autoplug: bool,
        pif_uuids: Vec<String>,
    ) -> Result<(), ApiError> {
        if self.settings.read_only {
            return Err(ApiError::Xen(
                "This connection is read-only. Changes are blocked by the Rust backend.".into(),
            ));
        }
        validate_sriov_network(name, description, mtu, &pif_uuids)?;
        let mut members = Vec::new();
        for uuid in &pif_uuids {
            let pif_ref: String = self
                .request("PIF.get_by_uuid", json!([self.session, uuid]))
                .await?;
            let pif: Value = self
                .request("PIF.get_record", json!([self.session, pif_ref]))
                .await?;
            if pif["uuid"].as_str() != Some(uuid.as_str()) || !sriov_pif_eligible(&pif) {
                return Err(ApiError::Xen(
                    "SR-IOV can only be enabled on attached physical interfaces that are not management, VLAN or bond interfaces."
                        .into(),
                ));
            }
            members.push(pif_ref);
        }
        let network_ref: String = self
            .request(
                "network.create",
                json!([self.session, {
                    "name_label": name.trim(),
                    "name_description": description,
                    "MTU": mtu,
                    "other_config": {"autoplug": autoplug.to_string(), "automatic": autoplug.to_string()},
                    "tags": [],
                    "bridge": "",
                    "managed": true
                }]),
            )
            .await?;
        let mut created = Vec::new();
        for pif_ref in &members {
            match self
                .async_task(
                    "Async.Network_sriov.create",
                    json!([self.session, pif_ref, network_ref]),
                )
                .await
            {
                Ok(()) => created.push(pif_ref.clone()),
                Err(error) => {
                    for pif in created {
                        let _ = self.destroy_sriov_for_pif(&pif).await;
                    }
                    let _ = self
                        .request::<Value>("network.destroy", json!([self.session, network_ref]))
                        .await;
                    return Err(error);
                }
            }
        }
        Ok(())
    }

    /// Create a cross-host internal network (CHIN) over management PIFs.
    pub async fn create_chin_network(
        &self,
        name: &str,
        description: &str,
        mtu: u64,
        autoplug: bool,
        protocol: &str,
        pif_uuids: Vec<String>,
    ) -> Result<(), ApiError> {
        if self.settings.read_only {
            return Err(ApiError::Xen(
                "This connection is read-only. Changes are blocked by the Rust backend.".into(),
            ));
        }
        validate_chin_network(name, description, mtu, protocol, &pif_uuids)?;
        let mut members = Vec::new();
        for uuid in &pif_uuids {
            let pif_ref: String = self
                .request("PIF.get_by_uuid", json!([self.session, uuid]))
                .await?;
            let pif: Value = self
                .request("PIF.get_record", json!([self.session, pif_ref]))
                .await?;
            if pif["uuid"].as_str() != Some(uuid.as_str()) || !chin_pif_eligible(&pif) {
                return Err(ApiError::Xen(
                    "Cross-host networks can only use attached management interfaces.".into(),
                ));
            }
            members.push(pif_ref);
        }
        let network_ref: String = self
            .request(
                "network.create",
                json!([self.session, {
                    "name_label": name.trim(),
                    "name_description": description,
                    "MTU": mtu,
                    "other_config": {"autoplug": autoplug.to_string(), "automatic": autoplug.to_string()},
                    "tags": [],
                    "bridge": "",
                    "managed": true
                }]),
            )
            .await?;
        let network_uuid: String = self
            .request("network.get_uuid", json!([self.session, network_ref]))
            .await?;
        for pif_ref in &members {
            if let Err(error) = self
                .async_task(
                    "Async.Tunnel.create",
                    json!([self.session, pif_ref, network_ref, protocol]),
                )
                .await
            {
                let _ = self.destroy_network(&network_uuid).await;
                return Err(error);
            }
        }
        Ok(())
    }

    async fn destroy_sriov_for_pif(&self, pif_ref: &str) -> Result<(), ApiError> {
        let pif: Value = self
            .request("PIF.get_record", json!([self.session, pif_ref]))
            .await?;
        if let Some(refs) = pif["sriov_physical_PIF_of"].as_array() {
            for reference in refs.iter().filter_map(Value::as_str) {
                self.async_task(
                    "Async.Network_sriov.destroy",
                    json!([self.session, reference]),
                )
                .await?;
            }
        }
        Ok(())
    }

    pub async fn create_bond(
        &self,
        name: &str,
        member_uuids: Vec<String>,
        mode: &str,
        mtu: u64,
        autoplug: bool,
    ) -> Result<(), ApiError> {
        if self.settings.read_only {
            return Err(ApiError::Xen(
                "This connection is read-only. Changes are blocked by the Rust backend.".into(),
            ));
        }
        validate_bond_fields(name, &member_uuids, mode, mtu)?;
        let mut members = Vec::new();
        let mut host_ref = String::new();
        for uuid in &member_uuids {
            let pif_ref: String = self
                .request("PIF.get_by_uuid", json!([self.session, uuid]))
                .await?;
            let pif: Value = self
                .request("PIF.get_record", json!([self.session, pif_ref]))
                .await?;
            if pif["uuid"].as_str() != Some(uuid) || !bond_member_eligible(&pif) {
                return Err(ApiError::Xen(
                    "Bond members must be attached physical interfaces that are not management interfaces, bonds or VLANs."
                        .into(),
                ));
            }
            let member_host = text(&pif, "host");
            if host_ref.is_empty() {
                host_ref = member_host;
            } else if host_ref != member_host {
                return Err(ApiError::Xen(
                    "All bond members must be on the same host.".into(),
                ));
            }
            members.push(pif_ref);
        }
        let network_ref: String = self
            .request(
                "network.create",
                json!([self.session, {
                    "name_label": name.trim(),
                    "name_description": "",
                    "MTU": mtu,
                    "other_config": {"autoplug": autoplug.to_string(), "automatic": autoplug.to_string()},
                    "tags": [],
                    "bridge": "",
                    "managed": true
                }]),
            )
            .await?;
        let properties = if mode == "lacp" {
            json!({"hashing_algorithm": "l3"})
        } else {
            json!({})
        };
        if let Err(error) = self
            .async_task(
                "Async.Bond.create",
                json!([self.session, network_ref, members, "", mode, properties]),
            )
            .await
        {
            let _ = self
                .request::<Value>("network.destroy", json!([self.session, network_ref]))
                .await;
            return Err(error);
        }
        Ok(())
    }

    pub async fn reconfigure_network_external(
        &self,
        uuid: &str,
        external: bool,
        parent_uuid: &str,
        tag: u16,
    ) -> Result<(), ApiError> {
        if self.settings.read_only {
            return Err(ApiError::Xen("This connection is read-only.".into()));
        }
        if tag > 4094 {
            return Err(ApiError::Xen("Enter a VLAN from 0 to 4094.".into()));
        }
        let network: String = self
            .request("network.get_by_uuid", json!([self.session, uuid]))
            .await?;
        let record: Value = self
            .request("network.get_record", json!([self.session, network]))
            .await?;
        if record["VIFs"].as_array().is_some_and(|v| !v.is_empty()) {
            return Err(ApiError::Xen(
                "Remove virtual interfaces before changing this network's NIC or VLAN.".into(),
            ));
        }
        let pifs = self.records("PIF").await?;
        let connected: Vec<_> = pifs
            .values()
            .filter(|p| p["network"].as_str() == Some(&network))
            .collect();
        if connected.iter().any(|p| {
            p["management"].as_bool() == Some(true)
                || p["physical"].as_bool() == Some(true)
                || number_i64(p, "VLAN") < 0
        }) {
            return Err(ApiError::Xen(
                "Only private networks and VLAN networks can change NIC or VLAN here.".into(),
            ));
        }
        let parent = if external {
            let (reference, pif) = pifs
                .iter()
                .find(|(_, p)| {
                    p["uuid"].as_str() == Some(parent_uuid)
                        && p["physical"].as_bool() == Some(true)
                        && number_i64(p, "VLAN") == -1
                })
                .ok_or_else(|| ApiError::Xen("Select a physical NIC.".into()))?;
            if pifs.values().any(|p| {
                p["device"] == pif["device"]
                    && number_i64(p, "VLAN") == i64::from(tag)
                    && p["network"].as_str() != Some(&network)
            }) {
                return Err(ApiError::Xen(
                    "This VLAN is already configured on the selected NIC.".into(),
                ));
            }
            Some(reference)
        } else {
            None
        };
        for pif in connected {
            self.request::<Value>("VLAN.destroy", json!([self.session, pif["VLAN_master_of"]]))
                .await?;
        }
        if let Some(parent) = parent {
            self.request::<Value>(
                "pool.create_VLAN_from_PIF",
                json!([self.session, parent, network, tag]),
            )
            .await?;
        }
        Ok(())
    }

    pub async fn create_vlan(
        &self,
        pif_uuid: &str,
        tag: i64,
        name: &str,
        description: &str,
        mtu: u64,
        autoplug: bool,
    ) -> Result<(), ApiError> {
        if self.settings.read_only {
            return Err(ApiError::Xen(
                "This connection is read-only. Changes are blocked by the Rust backend.".into(),
            ));
        }
        validate_vlan_fields(name, description, tag, mtu)?;
        let pif_ref: String = self
            .request("PIF.get_by_uuid", json!([self.session, pif_uuid]))
            .await?;
        let pif: Value = self
            .request("PIF.get_record", json!([self.session, pif_ref]))
            .await?;
        if pif["uuid"].as_str() != Some(pif_uuid) || !vlan_parent_eligible(&pif) {
            return Err(ApiError::Xen(
                "VLANs can only be created on an attached physical interface that is not a management interface."
                    .into(),
            ));
        }
        let device = text(&pif, "device");
        let pifs = self.records("PIF").await?;
        if pifs.values().any(|candidate| {
            text(candidate, "device") == device && number_i64(candidate, "VLAN") == tag
        }) {
            return Err(ApiError::Xen(format!(
                "VLAN {tag} is already configured on {device}."
            )));
        }
        let network_ref: String = self
            .request(
                "network.create",
                json!([self.session, {
                    "name_label": name.trim(),
                    "name_description": description,
                    "MTU": mtu,
                    "other_config": {"autoplug": autoplug.to_string(), "automatic": autoplug.to_string()},
                    "tags": [],
                    "bridge": "",
                    "managed": true
                }]),
            )
            .await?;
        if let Err(error) = self
            .request::<Value>(
                "Pool.create_VLAN_from_PIF",
                json!([self.session, pif_ref, network_ref, tag]),
            )
            .await
        {
            let _ = self
                .request::<Value>("network.destroy", json!([self.session, network_ref]))
                .await;
            return Err(error);
        }
        Ok(())
    }
}

fn text(record: &Value, key: &str) -> String {
    record[key].as_str().unwrap_or_default().to_owned()
}

fn number(record: &Value, key: &str) -> u64 {
    record[key]
        .as_u64()
        .or_else(|| record[key].as_str().and_then(|value| value.parse().ok()))
        .unwrap_or(0)
}

fn number_i64(record: &Value, key: &str) -> i64 {
    record[key]
        .as_i64()
        .or_else(|| record[key].as_str().and_then(|value| value.parse().ok()))
        .unwrap_or(-1)
}

pub fn validate_sriov_network(
    name: &str,
    description: &str,
    mtu: u64,
    pif_uuids: &[String],
) -> Result<(), ApiError> {
    validate_network_common(name, description, mtu, pif_uuids, "SR-IOV")
}

pub fn validate_chin_network(
    name: &str,
    description: &str,
    mtu: u64,
    protocol: &str,
    pif_uuids: &[String],
) -> Result<(), ApiError> {
    if !matches!(protocol, "gre" | "vxlan") {
        return Err(ApiError::Xen(
            "Tunnel protocol must be GRE or VXLAN.".into(),
        ));
    }
    validate_network_common(name, description, mtu, pif_uuids, "cross-host")
}

fn validate_network_common(
    name: &str,
    description: &str,
    mtu: u64,
    pif_uuids: &[String],
    label: &str,
) -> Result<(), ApiError> {
    let name = name.trim();
    if name.is_empty() || name.chars().count() > 255 {
        return Err(ApiError::Xen(format!(
            "Enter a {label} network name between 1 and 255 characters."
        )));
    }
    if description.chars().count() > 1024 {
        return Err(ApiError::Xen(
            "The description must be 1024 characters or fewer.".into(),
        ));
    }
    validate_mtu(mtu)?;
    if pif_uuids.is_empty() {
        return Err(ApiError::Xen(
            "Select at least one physical interface.".into(),
        ));
    }
    let mut unique = std::collections::HashSet::new();
    for uuid in pif_uuids {
        if !unique.insert(uuid) {
            return Err(ApiError::Xen(
                "Each interface can only be selected once.".into(),
            ));
        }
    }
    Ok(())
}

pub fn sriov_pif_eligible(pif: &Value) -> bool {
    pif["physical"].as_bool() == Some(true)
        && pif["management"].as_bool() != Some(true)
        && pif["currently_attached"].as_bool() == Some(true)
        && number_i64(pif, "VLAN") == -1
        && pif["bond_master_of"].as_array().is_none_or(Vec::is_empty)
        && pif["sriov_physical_PIF_of"]
            .as_array()
            .is_none_or(Vec::is_empty)
}

pub fn chin_pif_eligible(pif: &Value) -> bool {
    pif["management"].as_bool() == Some(true)
        && pif["currently_attached"].as_bool() == Some(true)
        && number_i64(pif, "VLAN") == -1
}

fn validate_bond_fields(
    name: &str,
    members: &[String],
    mode: &str,
    mtu: u64,
) -> Result<(), ApiError> {
    if name.trim().is_empty() || name.chars().count() > 255 {
        return Err(ApiError::Xen(
            "Enter a bond network name between 1 and 255 characters.".into(),
        ));
    }
    if members.len() < 2 {
        return Err(ApiError::Xen(
            "Select at least two physical interfaces.".into(),
        ));
    }
    let mut unique = members.to_vec();
    unique.sort();
    unique.dedup();
    if unique.len() != members.len() {
        return Err(ApiError::Xen("Bond members must be unique.".into()));
    }
    if !matches!(mode, "balance-slb" | "active-backup" | "lacp") {
        return Err(ApiError::Xen(
            "Bond mode must be balance-slb, active-backup or lacp.".into(),
        ));
    }
    validate_mtu(mtu)
}

fn validate_vlan_fields(name: &str, description: &str, tag: i64, mtu: u64) -> Result<(), ApiError> {
    if name.trim().is_empty() || name.chars().count() > 255 {
        return Err(ApiError::Xen(
            "Enter a VLAN network name between 1 and 255 characters.".into(),
        ));
    }
    if description.chars().count() > 1024 {
        return Err(ApiError::Xen(
            "The description must be 1024 characters or fewer.".into(),
        ));
    }
    if !(1..=4094).contains(&tag) {
        return Err(ApiError::Xen("VLAN tag must be between 1 and 4094.".into()));
    }
    validate_mtu(mtu)
}

fn validate_mtu(mtu: u64) -> Result<(), ApiError> {
    if !(1500..=9000).contains(&mtu) {
        return Err(ApiError::Xen("MTU must be between 1500 and 9000.".into()));
    }
    Ok(())
}

fn bond_member_eligible(pif: &Value) -> bool {
    pif["physical"].as_bool() == Some(true)
        && pif["management"].as_bool() != Some(true)
        && pif["currently_attached"].as_bool() == Some(true)
        && number_i64(pif, "VLAN") == -1
        && pif["bond_master_of"].as_array().is_none_or(Vec::is_empty)
}

fn vlan_parent_eligible(pif: &Value) -> bool {
    pif["physical"].as_bool() == Some(true)
        && pif["management"].as_bool() != Some(true)
        && pif["currently_attached"].as_bool() == Some(true)
        && number_i64(pif, "VLAN") == -1
        && pif["bond_master_of"].as_array().is_none_or(Vec::is_empty)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn physical() -> Value {
        json!({"physical":true,"management":false,"currently_attached":true,"VLAN":-1,"bond_master_of":[]})
    }

    #[test]
    fn bond_and_vlan_fields_are_validated() {
        assert!(
            validate_bond_fields("bond0", &["a".into(), "b".into()], "balance-slb", 1500).is_ok()
        );
        assert!(validate_bond_fields("", &["a".into(), "b".into()], "balance-slb", 1500).is_err());
        assert!(validate_bond_fields("bond0", &["a".into()], "balance-slb", 1500).is_err());
        assert!(
            validate_bond_fields("bond0", &["a".into(), "a".into()], "balance-slb", 1500).is_err()
        );
        assert!(
            validate_bond_fields("bond0", &["a".into(), "b".into()], "round-robin", 1500).is_err()
        );
        assert!(validate_bond_fields("bond0", &["a".into(), "b".into()], "lacp", 9001).is_err());
        assert!(validate_vlan_fields("vlan100", "", 100, 1500).is_ok());
        assert!(validate_vlan_fields("vlan100", "", 0, 1500).is_err());
        assert!(validate_vlan_fields("vlan100", "", 4095, 1500).is_err());
        assert!(validate_vlan_fields("vlan100", &"x".repeat(1025), 100, 1500).is_err());
    }

    #[test]
    fn sriov_and_chin_fields_are_validated() {
        assert!(validate_sriov_network("sriov", "", 1500, &["pif".into()]).is_ok());
        assert!(validate_sriov_network("", "", 1500, &["pif".into()]).is_err());
        assert!(validate_sriov_network("sriov", "", 1499, &["pif".into()]).is_err());
        assert!(validate_sriov_network("sriov", "", 1500, &[]).is_err());
        assert!(validate_sriov_network("sriov", "", 1500, &["a".into(), "a".into()]).is_err());
        assert!(validate_chin_network("chin", "", 1500, "gre", &["pif".into()]).is_ok());
        assert!(validate_chin_network("chin", "", 1500, "vxlan", &["pif".into()]).is_ok());
        assert!(validate_chin_network("chin", "", 1500, "ipip", &["pif".into()]).is_err());
        assert!(validate_chin_network("chin", "", 1500, "gre", &[]).is_err());
    }

    #[test]
    fn sriov_and_chin_pif_eligibility_is_enforced() {
        let physical = json!({
            "physical": true,
            "management": false,
            "currently_attached": true,
            "VLAN": -1,
            "bond_master_of": [],
            "sriov_physical_PIF_of": []
        });
        assert!(sriov_pif_eligible(&physical));
        assert!(!chin_pif_eligible(&physical));
        let mut management = physical.clone();
        management["management"] = json!(true);
        assert!(!sriov_pif_eligible(&management));
        assert!(chin_pif_eligible(&management));
        let mut enabled = physical.clone();
        enabled["sriov_physical_PIF_of"] = json!(["sriov-ref"]);
        assert!(!sriov_pif_eligible(&enabled));
        let mut vlan = management.clone();
        vlan["VLAN"] = json!(100);
        assert!(!chin_pif_eligible(&vlan));
        let mut detached = management.clone();
        detached["currently_attached"] = json!(false);
        assert!(!chin_pif_eligible(&detached));
    }

    #[test]
    fn management_interfaces_are_never_bond_or_vlan_members() {
        assert!(bond_member_eligible(&physical()));
        assert!(vlan_parent_eligible(&physical()));
        let mut management = physical();
        management["management"] = json!(true);
        assert!(!bond_member_eligible(&management));
        assert!(!vlan_parent_eligible(&management));
        let mut vlan = physical();
        vlan["VLAN"] = json!(100);
        assert!(!bond_member_eligible(&vlan));
        assert!(!vlan_parent_eligible(&vlan));
        let mut bonded = physical();
        bonded["bond_master_of"] = json!(["bond-ref"]);
        assert!(!bond_member_eligible(&bonded));
        assert!(!vlan_parent_eligible(&bonded));
        let mut detached = physical();
        detached["currently_attached"] = json!(false);
        assert!(!bond_member_eligible(&detached));
        assert!(!vlan_parent_eligible(&detached));
    }
}
