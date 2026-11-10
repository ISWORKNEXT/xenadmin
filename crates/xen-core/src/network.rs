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
    pub management: bool,
    pub attached: bool,
    pub disallow_unplug: bool,
    pub bond_master: String,
    pub ip: String,
    pub primary_address_type: String,
}

impl PifSummary {
    pub fn bond_candidate(&self) -> bool {
        self.physical
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
            .values()
            .map(|pif| {
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
                    management: pif["management"].as_bool().unwrap_or(false),
                    attached: pif["currently_attached"].as_bool().unwrap_or(false),
                    disallow_unplug: pif["disallow_unplug"].as_bool().unwrap_or(false),
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
                    "other_config": {"autoplug": autoplug.to_string()},
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
                    "other_config": {"autoplug": autoplug.to_string()},
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
