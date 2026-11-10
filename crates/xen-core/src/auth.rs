//! Read-only authentication and RBAC information.
use std::collections::HashMap;

use serde::Serialize;
use serde_json::Value;

use crate::xapi::{ApiError, Connection};

#[derive(Clone, Debug, Serialize)]
pub struct AuthConfigEntry {
    pub key: String,
    pub value: String,
    pub sensitive: bool,
}

#[derive(Clone, Debug, Serialize)]
pub struct AuthUser {
    pub uuid: String,
    pub identifier: String,
    pub display_name: String,
    pub email: String,
    pub roles: Vec<String>,
    pub local_root: bool,
}

#[derive(Clone, Debug, Serialize)]
pub struct AuthRole {
    pub uuid: String,
    pub name: String,
    pub description: String,
    pub permissions: usize,
    pub internal: bool,
}

#[derive(Clone, Debug, Serialize)]
pub struct AuthInfo {
    pub auth_type: String,
    pub service_name: String,
    pub config: Vec<AuthConfigEntry>,
    pub users: Vec<AuthUser>,
    pub roles: Vec<AuthRole>,
    pub local_auth_max_threads: u64,
    pub ext_auth_cache_enabled: bool,
}

impl AuthInfo {
    pub fn auth_label(&self) -> &'static str {
        match self.auth_type.as_str() {
            "AD" => "Active Directory",
            "" => "Local authentication",
            _ => "External authentication",
        }
    }
}

impl Connection {
    /// Enable AD authentication for the pool. The password is never logged.
    pub async fn enable_ad(
        &self,
        domain: &str,
        user: &str,
        password: &str,
    ) -> Result<(), ApiError> {
        if self.settings.read_only {
            return Err(ApiError::Xen(
                "This connection is read-only. Changes are blocked by the Rust backend.".into(),
            ));
        }
        let domain = validate_ad_domain(domain)?;
        let user = validate_ad_user(user)?;
        validate_ad_password(password)?;
        let pool = self.pool_record().await?;
        let (pool_ref, pool) = pool;
        if pool["external_auth_type"]
            .as_str()
            .is_some_and(|value| !value.is_empty())
        {
            // XenCenter disables the previous configuration before enabling a
            // fresh one. Ignore failures so the enable attempt can still report
            // the authoritative error from the backend.
            let _ = self
                .request::<Value>(
                    "Pool.disable_external_auth",
                    serde_json::json!([self.session, pool_ref, {}]),
                )
                .await;
        }
        self.request::<Value>(
            "Pool.enable_external_auth",
            serde_json::json!([
                self.session,
                pool_ref,
                {"domain": domain, "user": user, "pass": password},
                domain,
                "AD"
            ]),
        )
        .await?;
        Ok(())
    }

    /// Disable AD authentication. Supplying a user disables the machine account.
    pub async fn disable_ad(
        &self,
        user: Option<&str>,
        password: Option<&str>,
    ) -> Result<(), ApiError> {
        if self.settings.read_only {
            return Err(ApiError::Xen(
                "This connection is read-only. Changes are blocked by the Rust backend.".into(),
            ));
        }
        let config = validate_disable_credentials(user, password)?;
        let (pool_ref, _) = self.pool_record().await?;
        self.request::<Value>(
            "Pool.disable_external_auth",
            serde_json::json!([self.session, pool_ref, config]),
        )
        .await?;
        Ok(())
    }

    async fn pool_record(&self) -> Result<(String, Value), ApiError> {
        let pools = self.records("pool").await?;
        let Some((reference, pool)) = pools.iter().next() else {
            return Err(ApiError::Xen("No pool record is available.".into()));
        };
        Ok((reference.clone(), pool.clone()))
    }

    pub async fn set_subject_roles(
        &self,
        subject_uuid: &str,
        requested_roles: Vec<String>,
    ) -> Result<(), ApiError> {
        if self.settings.read_only {
            return Err(ApiError::Xen(
                "This connection is read-only. Changes are blocked by the Rust backend.".into(),
            ));
        }
        let subject_ref: String = self
            .request(
                "Subject.get_by_uuid",
                serde_json::json!([self.session, subject_uuid]),
            )
            .await?;
        let subject: Value = self
            .request(
                "Subject.get_record",
                serde_json::json!([self.session, subject_ref]),
            )
            .await?;
        if subject["uuid"].as_str() != Some(subject_uuid) {
            return Err(ApiError::Protocol);
        }
        let roles = self.records("role").await?;
        let desired = resolve_role_refs(&requested_roles, &roles)?;
        let current: Vec<String> = subject["roles"]
            .as_array()
            .map(|refs| {
                refs.iter()
                    .filter_map(Value::as_str)
                    .map(str::to_owned)
                    .collect()
            })
            .unwrap_or_default();
        let (add, remove) = role_diff(&current, &desired);
        for role in add {
            self.request::<Value>(
                "Subject.add_to_roles",
                serde_json::json!([self.session, subject_ref, role]),
            )
            .await?;
        }
        for role in remove {
            self.request::<Value>(
                "Subject.remove_from_roles",
                serde_json::json!([self.session, subject_ref, role]),
            )
            .await?;
        }
        Ok(())
    }

    pub async fn auth_info(&self) -> Result<AuthInfo, ApiError> {
        let (pools, hosts, subjects, roles) = tokio::join!(
            self.records("pool"),
            self.records("host"),
            self.records("subject"),
            self.records("role")
        );
        let (pools, hosts, subjects, roles) = (pools?, hosts?, subjects?, roles?);
        let pool = pools.values().next();
        let host = hosts.values().next();
        let auth_type = pool
            .and_then(|pool| pool["external_auth_type"].as_str())
            .filter(|value| !value.is_empty())
            .or_else(|| {
                host.and_then(|host| host["external_auth_type"].as_str())
                    .filter(|value| !value.is_empty())
            })
            .unwrap_or_default()
            .to_owned();
        let service_name = pool
            .and_then(|pool| pool["external_auth_service_name"].as_str())
            .filter(|value| !value.is_empty())
            .or_else(|| {
                host.and_then(|host| host["external_auth_service_name"].as_str())
                    .filter(|value| !value.is_empty())
            })
            .unwrap_or_default()
            .to_owned();
        let mut config = HashMap::new();
        for record in pool.into_iter().chain(host) {
            if let Some(values) = record["external_auth_configuration"].as_object() {
                for (key, value) in values {
                    config
                        .entry(key.clone())
                        .or_insert_with(|| value.as_str().unwrap_or_default().to_owned());
                }
            }
        }
        let config = redact_config(config);
        let role_map: HashMap<String, String> = roles
            .iter()
            .filter_map(|(reference, role)| {
                role["name_label"]
                    .as_str()
                    .map(|name| (reference.clone(), name.to_owned()))
            })
            .collect();
        let mut users = parse_subjects(&subjects, &role_map);
        if !users.iter().any(|user| user.identifier == "root") {
            users.push(AuthUser {
                uuid: "local-root".into(),
                identifier: "root".into(),
                display_name: "Local root account".into(),
                email: String::new(),
                roles: vec!["pool-admin".into()],
                local_root: true,
            });
        }
        users.sort_by(|left, right| left.identifier.cmp(&right.identifier));
        let mut role_list: Vec<AuthRole> = roles
            .values()
            .filter(|role| role["is_internal"].as_bool() != Some(true))
            .map(|role| AuthRole {
                uuid: text(role, "uuid"),
                name: text(role, "name_label"),
                description: text(role, "name_description"),
                permissions: role["subroles"].as_array().map_or(0, Vec::len),
                internal: false,
            })
            .collect();
        role_list.sort_by(|left, right| left.name.cmp(&right.name));
        let local_auth_max_threads = pool
            .and_then(|pool| pool["local_auth_max_threads"].as_u64())
            .or_else(|| {
                pool.and_then(|pool| {
                    pool["local_auth_max_threads"]
                        .as_str()
                        .and_then(|value| value.parse().ok())
                })
            })
            .unwrap_or(0);
        let ext_auth_cache_enabled = pool
            .and_then(|pool| pool["ext_auth_cache_enabled"].as_bool())
            .unwrap_or(false);
        Ok(AuthInfo {
            auth_type,
            service_name,
            config,
            users,
            roles: role_list,
            local_auth_max_threads,
            ext_auth_cache_enabled,
        })
    }
}

fn validate_ad_domain(domain: &str) -> Result<String, ApiError> {
    let domain = domain.trim();
    if domain.is_empty()
        || domain.chars().count() > 255
        || domain.chars().any(char::is_control)
        || domain.chars().any(char::is_whitespace)
    {
        return Err(ApiError::Xen(
            "Enter an AD domain name such as example.com.".into(),
        ));
    }
    Ok(domain.to_owned())
}

fn validate_ad_user(user: &str) -> Result<String, ApiError> {
    let user = user.trim();
    if user.is_empty() || user.chars().count() > 255 || user.chars().any(char::is_control) {
        return Err(ApiError::Xen(
            "Enter the AD service account user name.".into(),
        ));
    }
    Ok(user.to_owned())
}

fn validate_ad_password(password: &str) -> Result<(), ApiError> {
    if password.is_empty() || password.chars().count() > 1024 || password.contains('\0') {
        return Err(ApiError::Xen(
            "Enter a non-empty AD service account password.".into(),
        ));
    }
    Ok(())
}

fn validate_disable_credentials(
    user: Option<&str>,
    password: Option<&str>,
) -> Result<serde_json::Map<String, Value>, ApiError> {
    let user = user.map(str::trim).filter(|user| !user.is_empty());
    let password = password.unwrap_or_default();
    match (user, password.is_empty()) {
        (None, true) => Ok(serde_json::Map::new()),
        (None, false) => Err(ApiError::Xen(
            "Enter the AD service account user when supplying a password.".into(),
        )),
        (Some(user), false) => {
            let user = validate_ad_user(user)?;
            validate_ad_password(password)?;
            let mut config = serde_json::Map::new();
            config.insert("user".into(), Value::String(user));
            config.insert("pass".into(), Value::String(password.to_owned()));
            Ok(config)
        }
        (Some(_), true) => Err(ApiError::Xen(
            "Enter the service account password, or leave both fields empty to keep the machine account.".into(),
        )),
    }
}

fn resolve_role_refs(
    requested: &[String],
    roles: &HashMap<String, Value>,
) -> Result<Vec<String>, ApiError> {
    let mut resolved = Vec::new();
    for requested in requested {
        let requested = requested.trim();
        let Some((reference, role)) = roles.iter().find(|(reference, role)| {
            role["uuid"].as_str() == Some(requested)
                || role["name_label"].as_str() == Some(requested)
                || reference.as_str() == requested
        }) else {
            return Err(ApiError::Xen(format!(
                "Unknown role '{requested}'. Refresh and choose a role from the list."
            )));
        };
        if role["is_internal"].as_bool() == Some(true) {
            return Err(ApiError::Xen(
                "Internal permission records cannot be assigned as roles.".into(),
            ));
        }
        if !resolved.contains(reference) {
            resolved.push(reference.clone());
        }
    }
    if resolved.is_empty() {
        return Err(ApiError::Xen(
            "A subject must keep at least one role.".into(),
        ));
    }
    Ok(resolved)
}

fn role_diff(current: &[String], desired: &[String]) -> (Vec<String>, Vec<String>) {
    let add = desired
        .iter()
        .filter(|role| !current.contains(role))
        .cloned()
        .collect();
    let remove = current
        .iter()
        .filter(|role| !desired.contains(role))
        .cloned()
        .collect();
    (add, remove)
}

fn parse_subjects(
    subjects: &HashMap<String, Value>,
    role_map: &HashMap<String, String>,
) -> Vec<AuthUser> {
    subjects
        .values()
        .map(|subject| {
            let other = &subject["other_config"];
            let identifier = text(subject, "subject_identifier");
            let display_name = ["subject-displayname", "subject-name"]
                .iter()
                .find_map(|key| other[*key].as_str().filter(|value| !value.is_empty()))
                .unwrap_or(&identifier)
                .to_owned();
            let roles = subject["roles"]
                .as_array()
                .map(|refs| {
                    refs.iter()
                        .filter_map(Value::as_str)
                        .map(|reference| {
                            role_map
                                .get(reference)
                                .cloned()
                                .unwrap_or_else(|| reference.to_owned())
                        })
                        .collect()
                })
                .unwrap_or_default();
            AuthUser {
                uuid: text(subject, "uuid"),
                identifier,
                display_name,
                email: text(other, "subject-email"),
                roles,
                local_root: false,
            }
        })
        .collect()
}

fn redact_config(config: HashMap<String, String>) -> Vec<AuthConfigEntry> {
    let mut entries: Vec<AuthConfigEntry> = config
        .into_iter()
        .map(|(key, value)| {
            let sensitive = sensitive_key(&key);
            AuthConfigEntry {
                key,
                value: if sensitive { "••••" } else { &value }.to_owned(),
                sensitive,
            }
        })
        .collect();
    entries.sort_by(|left, right| left.key.cmp(&right.key));
    entries
}

fn sensitive_key(key: &str) -> bool {
    let key = key.to_ascii_lowercase();
    ["password", "passwd", "secret", "credential", "keytab"]
        .iter()
        .any(|needle| key.contains(needle))
}

fn text(record: &Value, key: &str) -> String {
    record[key].as_str().unwrap_or_default().to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn redacts_secret_values_only() {
        let entries = redact_config(HashMap::from([
            ("password".into(), "hunter2".into()),
            ("service-name".into(), "xenad".into()),
            ("keytab".into(), "secret".into()),
        ]));
        assert_eq!(entries[0].key, "keytab");
        assert!(entries[0].sensitive);
        assert_eq!(entries[0].value, "••••");
        assert_eq!(entries[1].key, "password");
        assert_eq!(entries[1].value, "••••");
        assert!(!entries[2].sensitive);
        assert_eq!(entries[2].value, "xenad");
        assert!(sensitive_key("AD_PASSWORD"));
        assert!(!sensitive_key("AD_USER"));
    }

    #[test]
    fn parses_subjects_and_resolves_roles() {
        let subjects = HashMap::from([(
            "subject-ref".to_owned(),
            json!({
                "uuid": "subject-uuid",
                "subject_identifier": "alice",
                "other_config": {
                    "subject-displayname": "Alice Example",
                    "subject-email": "alice@example.test"
                },
                "roles": ["role-ref"]
            }),
        )]);
        let roles = HashMap::from([("role-ref".to_owned(), "vm-operator".to_owned())]);
        let users = parse_subjects(&subjects, &roles);
        assert_eq!(users.len(), 1);
        assert_eq!(users[0].identifier, "alice");
        assert_eq!(users[0].display_name, "Alice Example");
        assert_eq!(users[0].email, "alice@example.test");
        assert_eq!(users[0].roles, vec!["vm-operator"]);
        assert!(!users[0].local_root);
    }

    #[test]
    fn ad_enable_fields_are_validated() {
        assert!(validate_ad_domain("example.com").is_ok());
        assert!(validate_ad_domain("").is_err());
        assert!(validate_ad_domain("bad domain").is_err());
        assert!(validate_ad_domain("bad\ndomain").is_err());
        assert!(validate_ad_user("svc-xen").is_ok());
        assert!(validate_ad_user("").is_err());
        assert!(validate_ad_password("secret").is_ok());
        assert!(validate_ad_password("").is_err());
    }

    #[test]
    fn ad_disable_credentials_are_paired() {
        let empty = validate_disable_credentials(None, None).unwrap();
        assert!(empty.is_empty());
        assert!(validate_disable_credentials(None, Some("password")).is_err());
        assert!(validate_disable_credentials(Some("svc"), Some("")).is_err());
        let config = validate_disable_credentials(Some("svc"), Some("password")).unwrap();
        assert_eq!(config.get("user").and_then(Value::as_str), Some("svc"));
        assert_eq!(config.get("pass").and_then(Value::as_str), Some("password"));
    }

    #[test]
    fn role_resolution_accepts_uuid_and_name_and_rejects_internal_or_unknown() {
        let roles = HashMap::from([
            (
                "role-ref".to_owned(),
                json!({"uuid":"role-uuid","name_label":"vm-operator","is_internal":false}),
            ),
            (
                "perm-ref".to_owned(),
                json!({"uuid":"perm-uuid","name_label":"vm.get_record","is_internal":true}),
            ),
        ]);
        let resolved = resolve_role_refs(&["role-uuid".into()], &roles).unwrap();
        assert_eq!(resolved, vec!["role-ref"]);
        let resolved = resolve_role_refs(&["vm-operator".into()], &roles).unwrap();
        assert_eq!(resolved, vec!["role-ref"]);
        assert!(resolve_role_refs(&["missing".into()], &roles).is_err());
        assert!(resolve_role_refs(&["vm.get_record".into()], &roles).is_err());
        assert!(resolve_role_refs(&[], &roles).is_err());
    }

    #[test]
    fn role_diff_adds_and_removes_only_changes() {
        let (add, remove) = role_diff(&["a".into(), "b".into()], &["b".into(), "c".into()]);
        assert_eq!(add, vec!["c"]);
        assert_eq!(remove, vec!["a"]);
        let (add, remove) = role_diff(&["a".into()], &["a".into()]);
        assert!(add.is_empty());
        assert!(remove.is_empty());
    }

    #[test]
    fn auth_labels_cover_local_and_ad() {
        let base = AuthInfo {
            auth_type: String::new(),
            service_name: String::new(),
            config: Vec::new(),
            users: Vec::new(),
            roles: Vec::new(),
            local_auth_max_threads: 8,
            ext_auth_cache_enabled: false,
        };
        assert_eq!(base.auth_label(), "Local authentication");
        let ad = AuthInfo {
            auth_type: "AD".into(),
            ..base
        };
        assert_eq!(ad.auth_label(), "Active Directory");
    }
}
