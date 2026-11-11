//! Owner template vm-data contract. Passwords never enter detail/debug records.
use crate::xapi::{ApiError, Connection};
use base64::Engine;
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    net::{IpAddr, Ipv4Addr, Ipv6Addr},
};

#[derive(Clone, Default, PartialEq, Eq)]
pub struct GuestSetup {
    pub dual_stack: bool,
    pub ipv4: String,
    pub prefix: String,
    pub gateway: String,
    pub dns1: String,
    pub dns2: String,
    pub ipv6: String,
    pub ipv6_prefix: String,
    pub ipv6_gateway: String,
    pub username: String,
    pub ssh_keys: String,
    pub password: String,
    pub confirm: String,
}

#[derive(Clone, Debug, Default, serde::Serialize)]
pub struct GuestSupport {
    pub enabled: bool,
    pub username: bool,
    pub ssh_keys: bool,
}
fn valid_public_key(kind: &str, data: &str) -> bool {
    let Ok(bytes) = base64::engine::general_purpose::STANDARD.decode(data) else {
        return false;
    };
    let mut rest = bytes.as_slice();
    fn field<'a>(rest: &mut &'a [u8]) -> Option<&'a [u8]> {
        let length = u32::from_be_bytes(rest.get(..4)?.try_into().ok()?) as usize;
        *rest = &rest[4..];
        let value = rest.get(..length)?;
        *rest = &rest[length..];
        Some(value)
    }
    if field(&mut rest) != Some(kind.as_bytes()) {
        return false;
    }
    let valid = match kind {
        "ssh-ed25519" => field(&mut rest).is_some_and(|v| v.len() == 32),
        "ssh-rsa" => {
            field(&mut rest).is_some_and(|v| !v.is_empty())
                && field(&mut rest).is_some_and(|v| v.len() >= 128)
        }
        "ecdsa-sha2-nistp256" | "ecdsa-sha2-nistp384" | "ecdsa-sha2-nistp521" => {
            field(&mut rest) == kind.strip_prefix("ecdsa-sha2-").map(str::as_bytes)
                && field(&mut rest).is_some_and(|v| {
                    v.first() == Some(&4)
                        && v.len()
                            == match kind {
                                "ecdsa-sha2-nistp256" => 65,
                                "ecdsa-sha2-nistp384" => 97,
                                _ => 133,
                            }
                })
        }
        _ => false,
    };
    valid && rest.is_empty()
}
impl GuestSupport {
    pub fn from_record(record: &Value) -> Self {
        let marker = |key: &str| record["other_config"].get(key).is_some();
        Self {
            enabled: record["other_config"]
                .as_object()
                .is_some_and(|m| m.keys().any(|k| k.starts_with("noname-")))
                || record["xenstore_data"]
                    .as_object()
                    .is_some_and(|m| m.keys().any(|k| k.starts_with("vm-data/"))),
            username: marker("noname-custom-username"),
            ssh_keys: marker("noname-ssh-public-keys"),
        }
    }
}
impl GuestSetup {
    pub fn from_data(data: &BTreeMap<String, String>) -> Self {
        let get = |key: &str| {
            data.get(&format!("vm-data/{key}"))
                .cloned()
                .unwrap_or_default()
        };
        Self {
            dual_stack: !get("ipv6").is_empty(),
            ipv4: get("ipv4"),
            prefix: get("subnet"),
            gateway: get("gateway"),
            dns1: get("dns1"),
            dns2: get("dns2"),
            ipv6: get("ipv6"),
            ipv6_prefix: get("ipv6-prefix-length"),
            ipv6_gateway: get("ipv6-gateway"),
            username: get("login-username"),
            ssh_keys: serde_json::from_str::<Vec<String>>(&get("ssh-public-keys"))
                .unwrap_or_default()
                .join("\n"),
            ..Self::default()
        }
    }
    pub fn validate(&self, support: &GuestSupport) -> Result<(), &'static str> {
        if !support.enabled {
            return Err("This template does not support Guest Setup.");
        }
        if self.ipv4.parse::<Ipv4Addr>().is_err() || self.gateway.parse::<Ipv4Addr>().is_err() {
            return Err("Enter valid IPv4 address and gateway.");
        }
        if self.prefix.parse::<u8>().ok().is_none_or(|p| p > 32) {
            return Err("IPv4 prefix must be 0–32.");
        }
        for dns in [&self.dns1, &self.dns2] {
            if !dns.is_empty() && dns.parse::<IpAddr>().is_err() {
                return Err("Enter valid DNS addresses.");
            }
        }
        let any =
            !self.ipv6.is_empty() || !self.ipv6_prefix.is_empty() || !self.ipv6_gateway.is_empty();
        if any
            && (!self.dual_stack
                || self.ipv6.parse::<Ipv6Addr>().is_err()
                || self.ipv6_gateway.parse::<Ipv6Addr>().is_err()
                || self.ipv6_prefix.parse::<u8>().ok().is_none_or(|p| p > 128))
        {
            return Err(
                "Supply IPv6 address, prefix (0–128) and gateway together in dual stack mode.",
            );
        }
        if support.username && !self.username.is_empty() {
            let mut chars = self.username.chars();
            if !chars
                .next()
                .is_some_and(|c| c.is_ascii_lowercase() || c == '_')
                || !chars
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_' || c == '-')
                || self.username.len() > 32
                || self.username == "root"
            {
                return Err(
                    "Enter a non-root Linux login name (up to 32 lowercase letters, digits, _ or -).",
                );
            }
        }
        if support.ssh_keys {
            for line in self.ssh_keys.lines().filter(|s| !s.trim().is_empty()) {
                let mut parts = line.split_whitespace();
                let kind = parts.next().unwrap_or_default();
                let data = parts.next().unwrap_or_default();
                if !matches!(
                    kind,
                    "ssh-ed25519"
                        | "ssh-rsa"
                        | "ecdsa-sha2-nistp256"
                        | "ecdsa-sha2-nistp384"
                        | "ecdsa-sha2-nistp521"
                ) || !valid_public_key(kind, data)
                    || data.len() < 32
                    || !data
                        .bytes()
                        .all(|b| b.is_ascii_alphanumeric() || b"+/=".contains(&b))
                {
                    return Err("Enter one OpenSSH public key per line.");
                }
            }
        }
        self.validate_password()
    }
    pub fn validate_password(&self) -> Result<(), &'static str> {
        if self.password != self.confirm {
            return Err("Passwords do not match.");
        }
        if self.password.contains(['\n', '\r', '\0']) {
            return Err("Password contains an invalid control character.");
        }
        Ok(())
    }
    pub fn apply(&self, data: &mut BTreeMap<String, String>, support: &GuestSupport) {
        for key in [
            "password",
            "root-password",
            "xpwd",
            "ipv6",
            "ipv6-gateway",
            "ipv6-prefix-length",
        ] {
            data.remove(&format!("vm-data/{key}"));
        }
        let mut put = |key: &str, value: &str| {
            data.insert(format!("vm-data/{key}"), value.into());
        };
        for (key, value) in [
            ("ip-family", "ipv4"),
            ("ipv4", &self.ipv4),
            ("subnet", &self.prefix),
            ("submark", &self.prefix),
            ("gateway", &self.gateway),
            ("dns1", &self.dns1),
            ("dns2", &self.dns2),
        ] {
            put(key, value);
        }
        if self.dual_stack && !self.ipv6.is_empty() {
            put("ipv6", &self.ipv6);
            put("ipv6-prefix-length", &self.ipv6_prefix);
            put("ipv6-gateway", &self.ipv6_gateway);
        }
        if support.username {
            put("login-username", &self.username);
        }
        if support.ssh_keys {
            let keys: Vec<_> = self
                .ssh_keys
                .lines()
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .collect();
            put(
                "ssh-public-keys",
                &serde_json::to_string(&keys).expect("string array"),
            );
        }
        if !self.password.is_empty() {
            put("xpwd", &self.password);
        }
    }
}
impl Connection {
    pub async fn install_guest_tools(&self, uuid: &str) -> Result<(), ApiError> {
        if self.settings.read_only {
            return Err(ApiError::Xen("This connection is read-only.".into()));
        }
        let reference: String = self
            .request("VM.get_by_uuid", json!([self.session, uuid]))
            .await?;
        let record: Value = self
            .request("VM.get_record", json!([self.session, reference]))
            .await?;
        if record["is_a_template"].as_bool() != Some(false)
            || record["is_a_snapshot"].as_bool() != Some(false)
            || record["is_control_domain"].as_bool() != Some(false)
            || !matches!(record["power_state"].as_str(), Some("Halted" | "Running"))
        {
            return Err(ApiError::Xen(
                "Select a running or halted VM to install Guest Tools.".into(),
            ));
        }
        let details = self.vm_details(uuid).await?;
        let cd =
            details.disks.iter().find(|d| d.is_cd).ok_or_else(|| {
                ApiError::Xen("Could not find a CD drive on the selected VM.".into())
            })?;
        let srs = self.records("SR").await?;
        let vdis = self.records("VDI").await?;
        let iso = vdis
            .values()
            .find(|v| {
                (v["location"].as_str() == Some("guest-tools.iso")
                    || v["name_label"].as_str() == Some("guest-tools.iso"))
                    && v["SR"].as_str().and_then(|r| srs.get(r)).is_some_and(|sr| {
                        sr["content_type"].as_str() == Some("iso")
                            && (sr["other_config"]["xenserver_tools_sr"].as_str() == Some("true")
                                || sr["name_label"]
                                    .as_str()
                                    .is_some_and(|n| n.contains("Tools")))
                    })
            })
            .ok_or_else(|| ApiError::Xen("Could not find the Guest Tools install disk.".into()))?;
        let iso_uuid = iso["uuid"].as_str().ok_or(ApiError::Protocol)?;
        self.change_cd(uuid, &cd.vbd_uuid, Some(iso_uuid)).await?;
        let actual = self.vm_details(uuid).await?;
        if !actual
            .disks
            .iter()
            .any(|d| d.vbd_uuid == cd.vbd_uuid && !d.empty && d.vdi_uuid == iso_uuid)
        {
            return Err(ApiError::Xen(
                "Guest Tools CD readback did not match.".into(),
            ));
        }
        Ok(())
    }

    pub async fn update_guest_setup(
        &self,
        uuid: &str,
        setup: &GuestSetup,
        password_only: bool,
    ) -> Result<(), ApiError> {
        if self.settings.read_only {
            return Err(ApiError::Xen("This connection is read-only.".into()));
        }
        let reference: String = self
            .request("VM.get_by_uuid", json!([self.session, uuid]))
            .await?;
        let record: Value = self
            .request("VM.get_record", json!([self.session, reference]))
            .await?;
        let support = GuestSupport::from_record(&record);
        if record["power_state"].as_str() != Some("Halted")
            || record["is_a_template"].as_bool() != Some(false)
            || record["is_a_snapshot"].as_bool() != Some(false)
            || record["is_control_domain"].as_bool() != Some(false)
            || !support.enabled
        {
            return Err(ApiError::Xen(
                "Guest Setup requires a halted, supported VM.".into(),
            ));
        }
        let mut data: BTreeMap<String, String> =
            serde_json::from_value(record["xenstore_data"].clone())
                .map_err(|_| ApiError::Protocol)?;
        if password_only {
            setup
                .validate_password()
                .map_err(|e| ApiError::Xen(e.into()))?;
            if setup.password.is_empty() {
                return Err(ApiError::Xen("Enter a new password.".into()));
            }
            data.remove("vm-data/password");
            data.remove("vm-data/root-password");
            data.insert("vm-data/xpwd".into(), setup.password.clone());
        } else {
            setup
                .validate(&support)
                .map_err(|e| ApiError::Xen(e.into()))?;
            setup.apply(&mut data, &support);
        }
        // Errors are deliberately generic: XAPI error parameters can contain the password.
        self.request::<Value>(
            "VM.set_xenstore_data",
            json!([self.session, reference, data]),
        )
        .await
        .map_err(|_| ApiError::Xen("Guest Setup write failed; the VM remains halted.".into()))?;
        let actual: BTreeMap<String, String> = self
            .request("VM.get_xenstore_data", json!([self.session, reference]))
            .await
            .map_err(|_| {
                ApiError::Xen("Guest Setup readback failed; the VM remains halted.".into())
            })?;
        if data != actual {
            return Err(ApiError::Xen(
                "Guest Setup readback did not match; the VM remains halted.".into(),
            ));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn valid() -> GuestSetup {
        GuestSetup {
            ipv4: "192.168.8.225".into(),
            gateway: "192.168.8.1".into(),
            prefix: "24".into(),
            ..Default::default()
        }
    }
    fn support() -> GuestSupport {
        GuestSupport {
            enabled: true,
            username: true,
            ssh_keys: true,
        }
    }
    #[test]
    fn validation() {
        let mut s = valid();
        assert!(s.validate(&support()).is_ok());
        s.ipv4 = "invalid".into();
        assert!(s.validate(&support()).is_err());
        s = valid();
        s.prefix = "33".into();
        assert!(s.validate(&support()).is_err());
        s = valid();
        s.dns1 = "invalid".into();
        assert!(s.validate(&support()).is_err());
        s = valid();
        s.username = "root".into();
        assert!(s.validate(&support()).is_err());
        s = valid();
        s.ssh_keys = "private key".into();
        assert!(s.validate(&support()).is_err());
        s = valid();
        s.password = "secret".into();
        assert!(s.validate(&support()).is_err());
    }
    #[test]
    fn ipv6_all_or_none() {
        for mask in 0..8 {
            let mut s = valid();
            s.dual_stack = true;
            if mask & 1 != 0 {
                s.ipv6 = "2001:db8::2".into();
            }
            if mask & 2 != 0 {
                s.ipv6_prefix = "64".into();
            }
            if mask & 4 != 0 {
                s.ipv6_gateway = "fe80::1".into();
            }
            assert_eq!(s.validate(&support()).is_ok(), mask == 0 || mask == 7);
        }
    }
    #[test]
    fn map_and_stale_removal() {
        let mut data = BTreeMap::from([("unrelated".into(), "keep".into())]);
        for key in [
            "password",
            "root-password",
            "xpwd",
            "ipv6",
            "ipv6-gateway",
            "ipv6-prefix-length",
        ] {
            data.insert(format!("vm-data/{key}"), "stale".into());
        }
        let mut s = valid();
        s.username = "paritytester".into();
        s.apply(&mut data, &support());
        assert_eq!(data["unrelated"], "keep");
        assert_eq!(data["vm-data/submark"], "24");
        assert_eq!(data["vm-data/ip-family"], "ipv4");
        assert_eq!(data["vm-data/login-username"], "paritytester");
        assert_eq!(data["vm-data/ssh-public-keys"], "[]");
        for key in [
            "password",
            "root-password",
            "xpwd",
            "ipv6",
            "ipv6-gateway",
            "ipv6-prefix-length",
        ] {
            assert!(!data.contains_key(&format!("vm-data/{key}")));
        }
        s.password = "test".into();
        s.apply(&mut data, &support());
        assert_eq!(data["vm-data/xpwd"], "test");
    }
}
