//! Organization metadata shared with XenCenter and XCP-ng Center.
use crate::xapi::ApiError;
use serde::{Deserialize, Serialize};

pub const CUSTOM_FIELDS: &str = "XenCenter.CustomFields";
pub const CUSTOM_PREFIX: &str = "XenCenter.CustomFields.";

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CustomFieldDefinition {
    pub name: String,
    pub date: bool,
}

pub fn decode_definitions(xml: &str) -> Result<Vec<CustomFieldDefinition>, ApiError> {
    if xml.trim().is_empty() {
        return Ok(Vec::new());
    }
    let doc = roxmltree::Document::parse(xml).map_err(|e| ApiError::Xen(e.to_string()))?;
    if doc.root_element().tag_name().name() != "CustomFieldDefinitions" {
        return Err(ApiError::Xen(
            "Invalid custom field definitions root.".into(),
        ));
    }
    doc.root_element()
        .children()
        .filter(|n| n.is_element())
        .map(|n| {
            if n.tag_name().name() != "CustomFieldDefinition" {
                return Err(ApiError::Xen("Unknown custom field definition.".into()));
            }
            let name = n.attribute("name").ok_or(ApiError::Protocol)?.to_owned();
            let date = match n.attribute("type") {
                Some("Date") => true,
                Some("String") => false,
                _ => return Err(ApiError::Xen("Unknown custom field type.".into())),
            };
            Ok(CustomFieldDefinition { name, date })
        })
        .collect()
}

pub fn encode_definitions(fields: &[CustomFieldDefinition]) -> Result<String, ApiError> {
    let mut names = std::collections::HashSet::new();
    let mut xml = String::from("<CustomFieldDefinitions>");
    for f in fields {
        if f.name.trim().is_empty()
            || f.name.chars().any(char::is_control)
            || !names.insert(&f.name)
        {
            return Err(ApiError::Xen(
                "Custom field names must be nonempty and unique.".into(),
            ));
        }
        xml += &format!(
            "<CustomFieldDefinition name=\"{}\" type=\"{}\" defaultValue=\"\"/>",
            quick_xml::escape::escape(&f.name),
            if f.date { "Date" } else { "String" }
        );
    }
    xml += "</CustomFieldDefinitions>";
    Ok(xml)
}

pub fn normalize_folder(path: &str) -> Result<String, ApiError> {
    let parts: Vec<_> = path
        .split('/')
        .map(str::trim)
        .filter(|p| !p.is_empty())
        .collect();
    if parts
        .iter()
        .any(|p| p.contains(';') || p.chars().any(char::is_control))
    {
        return Err(ApiError::Xen(
            "Folder names cannot contain semicolons or control characters.".into(),
        ));
    }
    Ok(if parts.is_empty() {
        String::new()
    } else {
        format!("/{}", parts.join("/"))
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn upstream_definition_format_round_trip() {
        let xml = r#"<CustomFieldDefinitions><CustomFieldDefinition name="Owner &amp; team" type="String" defaultValue=""/><CustomFieldDefinition name="Due" type="Date" defaultValue=""/></CustomFieldDefinitions>"#;
        let fields = decode_definitions(xml).unwrap();
        assert_eq!(fields[0].name, "Owner & team");
        assert!(fields[1].date);
        assert_eq!(
            decode_definitions(&encode_definitions(&fields).unwrap()).unwrap(),
            fields
        );
        assert!(encode_definitions(&[fields[0].clone(), fields[0].clone()]).is_err());
    }
    #[test]
    fn upstream_folder_paths() {
        assert_eq!(normalize_folder(" /one/ two // ").unwrap(), "/one/two");
        assert_eq!(normalize_folder("/").unwrap(), "");
        assert!(normalize_folder("/bad;folder").is_err());
    }
}

#[derive(Clone, Debug)]
pub enum OrganizationChange {
    Folder { before: String, after: String },
    Tag { before: String, after: String },
}
impl crate::xapi::Connection {
    /// Folder/tag rename and deletion follow the upstream global operation.
    pub async fn change_organization(&self, change: &OrganizationChange) -> Result<(), ApiError> {
        use serde_json::{Value, json};
        if self.settings.read_only {
            return Err(ApiError::Xen("This connection is read-only.".into()));
        }
        let (before, after, folder) = match change {
            OrganizationChange::Folder { before, after } => {
                (normalize_folder(before)?, normalize_folder(after)?, true)
            }
            OrganizationChange::Tag { before, after } => {
                if before.is_empty()
                    || after.chars().any(char::is_control)
                    || after.chars().count() > 255
                {
                    return Err(ApiError::Xen("Enter a valid tag name.".into()));
                }
                (before.clone(), after.trim().to_owned(), false)
            }
        };
        if folder && before.is_empty() && after.is_empty() {
            return Err(ApiError::Xen("Select a folder.".into()));
        }
        if !before.is_empty() && before == after {
            return Ok(());
        }
        let mut pools = std::collections::BTreeMap::new();
        for class in ["pool", "host", "VM", "SR", "VDI", "network"] {
            let records: std::collections::BTreeMap<String, Value> = self
                .request(&format!("{class}.get_all_records"), json!([self.session]))
                .await?;
            if class == "pool" {
                pools = records.clone();
            }
            for (reference, record) in records {
                if folder {
                    let path = record["other_config"]["folder"]
                        .as_str()
                        .unwrap_or_default();
                    if !before.is_empty()
                        && (path == before || path.starts_with(&format!("{before}/")))
                    {
                        let desired = if after.is_empty() {
                            String::new()
                        } else {
                            format!("{after}{}", &path[before.len()..])
                        };
                        self.request::<Value>(
                            &format!("{class}.remove_from_other_config"),
                            json!([self.session, reference, "folder"]),
                        )
                        .await?;
                        if !desired.is_empty() {
                            self.request::<Value>(
                                &format!("{class}.add_to_other_config"),
                                json!([self.session, reference, "folder", desired]),
                            )
                            .await?;
                        }
                    }
                } else if record["tags"]
                    .as_array()
                    .is_some_and(|tags| tags.iter().any(|tag| tag.as_str() == Some(&before)))
                {
                    self.request::<Value>(
                        &format!("{class}.remove_tags"),
                        json!([self.session, reference, before]),
                    )
                    .await?;
                    if !after.is_empty() {
                        self.request::<Value>(
                            &format!("{class}.add_tags"),
                            json!([self.session, reference, after]),
                        )
                        .await?;
                    }
                }
            }
        }
        if folder {
            for (reference, record) in pools {
                let mut folders = std::collections::BTreeSet::new();
                for path in record["other_config"]["EMPTY_FOLDERS"]
                    .as_str()
                    .unwrap_or_default()
                    .split(';')
                    .filter(|s| s.starts_with('/'))
                {
                    if !before.is_empty()
                        && (path == before || path.starts_with(&format!("{before}/")))
                    {
                        if !after.is_empty() {
                            folders.insert(format!("{after}{}", &path[before.len()..]));
                        }
                    } else {
                        folders.insert(path.to_owned());
                    }
                }
                if !after.is_empty() {
                    folders.insert(after.clone());
                }
                self.request::<Value>(
                    "pool.remove_from_other_config",
                    json!([self.session, reference, "EMPTY_FOLDERS"]),
                )
                .await?;
                if !folders.is_empty() {
                    self.request::<Value>(
                        "pool.add_to_other_config",
                        json!([
                            self.session,
                            reference,
                            "EMPTY_FOLDERS",
                            folders.into_iter().collect::<Vec<_>>().join(";")
                        ]),
                    )
                    .await?;
                }
            }
        }
        Ok(())
    }
}

#[derive(Clone, Debug)]
pub enum ExtraAction {
    Recovery(String),
    DisableCbt(String),
    QuickCreate(String),
    DestroyHost(String),
}
impl crate::xapi::Connection {
    pub async fn extra_action(&self, action: &ExtraAction) -> Result<(), ApiError> {
        use serde_json::{Value, json};
        if self.settings.read_only {
            return Err(ApiError::Xen("This connection is read-only.".into()));
        }
        if let ExtraAction::DestroyHost(uuid) = action {
            let reference: String = self
                .request("host.get_by_uuid", json!([self.session, uuid]))
                .await?;
            let record: Value = self
                .request("host.get_record", json!([self.session, reference]))
                .await?;
            let pools: std::collections::BTreeMap<String, Value> = self
                .request("pool.get_all_records", json!([self.session]))
                .await?;
            let metrics: Value = self
                .request(
                    "host_metrics.get_record",
                    json!([self.session, record["metrics"]]),
                )
                .await?;
            if pools.values().any(|p| p["master"] == reference)
                || metrics["live"].as_bool() != Some(false)
            {
                return Err(ApiError::Xen(
                    "Only an offline pool member that is not the coordinator can be destroyed."
                        .into(),
                ));
            }
            self.request::<Value>("host.destroy", json!([self.session, reference]))
                .await?;
            return Ok(());
        }
        let uuid = match action {
            ExtraAction::Recovery(id)
            | ExtraAction::DisableCbt(id)
            | ExtraAction::QuickCreate(id) => id,
            _ => unreachable!(),
        };
        let reference: String = self
            .request("VM.get_by_uuid", json!([self.session, uuid]))
            .await?;
        let record: Value = self
            .request("VM.get_record", json!([self.session, reference]))
            .await?;
        if record["is_control_domain"].as_bool() != Some(false)
            || record["is_a_snapshot"].as_bool() != Some(false)
        {
            return Err(ApiError::Xen(
                "Select a VM or template, not a control domain or snapshot.".into(),
            ));
        }
        let template = record["is_a_template"].as_bool() == Some(true);
        match action {
            ExtraAction::Recovery(_) => {
                if template
                    || !record["allowed_operations"]
                        .as_array()
                        .is_some_and(|ops| ops.iter().any(|op| op == "start"))
                {
                    return Err(ApiError::Xen(
                        "The VM cannot be started in its current state.".into(),
                    ));
                }
                let mut params = record["HVM_boot_params"]
                    .as_object()
                    .cloned()
                    .unwrap_or_default();
                params.insert("order".into(), json!("DN"));
                self.request::<Value>(
                    "VM.set_HVM_boot_params",
                    json!([self.session, reference, params]),
                )
                .await?;
                let start = async {
                    self.request::<Value>(
                        "VM.set_HVM_boot_policy",
                        json!([self.session, reference, "BIOS order"]),
                    )
                    .await?;
                    self.request::<Value>(
                        "VM.start",
                        json!([self.session, reference, false, false]),
                    )
                    .await
                }
                .await;
                let restore_policy = self
                    .request::<Value>(
                        "VM.set_HVM_boot_policy",
                        json!([self.session, reference, record["HVM_boot_policy"]]),
                    )
                    .await;
                let restore_params = self
                    .request::<Value>(
                        "VM.set_HVM_boot_params",
                        json!([self.session, reference, record["HVM_boot_params"]]),
                    )
                    .await;
                restore_policy?;
                restore_params?;
                start?;
            }
            ExtraAction::DisableCbt(_) => {
                if template {
                    return Err(ApiError::Xen("Select a VM, not a template.".into()));
                }
                let mut disks = std::collections::BTreeSet::new();
                for vbd in record["VBDs"].as_array().into_iter().flatten() {
                    let vbd: Value = self
                        .request("VBD.get_record", json!([self.session, vbd]))
                        .await?;
                    if vbd["empty"].as_bool() == Some(true) {
                        continue;
                    }
                    let vdi: Value = self
                        .request("VDI.get_record", json!([self.session, vbd["VDI"]]))
                        .await?;
                    if vdi["cbt_enabled"].as_bool() == Some(true) {
                        disks.insert(vbd["VDI"].as_str().ok_or(ApiError::Protocol)?.to_owned());
                    }
                }
                if disks.is_empty() {
                    return Err(ApiError::Xen(
                        "No disks have Changed Block Tracking enabled.".into(),
                    ));
                }
                for disk in disks {
                    self.request::<Value>("VDI.disable_cbt", json!([self.session, disk]))
                        .await?;
                }
            }
            ExtraAction::QuickCreate(_) => {
                if !template
                    || record["other_config"]["instant"].as_str() != Some("true")
                    || !record["current_operations"]
                        .as_object()
                        .is_some_and(|ops| ops.is_empty())
                {
                    return Err(ApiError::Xen(
                        "Quick Create requires an unlocked instant template.".into(),
                    ));
                }
                let name = format!(
                    "{} ({})",
                    record["name_label"].as_str().unwrap_or("VM"),
                    uuid::Uuid::now_v7()
                );
                let vm: String = self
                    .request("VM.clone", json!([self.session, reference, name]))
                    .await?;
                self.request::<Value>("VM.provision", json!([self.session, vm]))
                    .await?;
                self.request::<Value>("VM.start", json!([self.session, vm, false, false]))
                    .await?;
            }
            _ => unreachable!(),
        }
        Ok(())
    }
}

#[cfg(test)]
mod live_tests {
    use super::*;
    use crate::{
        properties::{ObjectKind, ObjectPropertiesEdit, ObjectTarget},
        provisioning::{CreateVmRequest, VmCreationSource},
        transport::TlsOptions,
        xapi::{Connection, ConnectionSettings},
    };
    use serde_json::{Value, json};

    #[tokio::test]
    #[ignore = "Requires the authorized .170 lab and XEN_DEV_* environment"]
    async fn m6_live_organization() {
        let settings = ConnectionSettings {
            host: std::env::var("XEN_DEV_HOST").unwrap(),
            username: std::env::var("XEN_DEV_USER").unwrap(),
            label: "M6 qualification".into(),
            read_only: false,
            tls: TlsOptions {
                bypass_certificate_verification: true,
            },
            proxy: None,
            timeout_seconds: 45,
        };
        assert_eq!(settings.host, "192.168.8.170");
        let c = Connection::login(settings, &std::env::var("XEN_DEV_PASSWORD").unwrap())
            .await
            .unwrap();
        let initial = c.records("VM").await.unwrap();
        assert!(
            !initial.values().any(|v| v["name_label"]
                .as_str()
                .is_some_and(|n| n.starts_with("zz-parity-M6-"))),
            "An M6 VM already exists; leave it untouched."
        );
        let initial_pools = c.records("pool").await.unwrap();
        let (pool_ref, pool) = initial_pools.iter().next().unwrap();
        let definition_xml = pool["gui_config"][CUSTOM_FIELDS]
            .as_str()
            .unwrap_or_default();
        assert!(
            !decode_definitions(definition_xml)
                .unwrap()
                .iter()
                .any(|d| d.name == "zz-parity-field")
        );
        for class in ["VM", "host", "pool", "SR", "VDI", "network"] {
            for record in c.records(class).await.unwrap().values() {
                assert!(
                    !record["tags"]
                        .as_array()
                        .is_some_and(|tags| tags.iter().any(|t| t == "zz-parity-tag"))
                );
                assert!(
                    !record["other_config"]["folder"]
                        .as_str()
                        .is_some_and(|p| p == "/zz-parity" || p.starts_with("/zz-parity/"))
                );
                assert!(
                    record["other_config"]
                        .get("XenCenter.CustomFields.zz-parity-field")
                        .is_none()
                );
            }
        }
        assert!(
            !pool["other_config"]["EMPTY_FOLDERS"]
                .as_str()
                .unwrap_or_default()
                .split(';')
                .any(|p| p.starts_with("/zz-parity"))
        );
        let mut owned = Vec::<String>::new();
        let result: Result<(), ApiError> = async {
            let inventory = c.inventory().await?;
            let template = inventory.templates.iter().find(|v| v.name == "Other install media").ok_or(ApiError::Protocol)?;
            let sr = inventory.storage.iter().find(|s| s.name == "Local storage").ok_or(ApiError::Protocol)?;
            let uuid = c.create_configured_vm(&CreateVmRequest { guest_setup: None, start_automatically: false, source: VmCreationSource::Template(template.uuid.clone()), name: "zz-parity-M6-vm".into(), description: "M6 disposable organization verification".into(), vcpus_max: 1, vcpus_at_startup: 1, cores_per_socket: None, memory_mib: 256, sr_uuid: sr.uuid.clone(), disks: Some(vec![]), interfaces: Some(vec![]), media: Default::default(), pv_args: None, copy_bios_from: None, new_disk_gib: None, network_uuid: None, iso_uuid: None, affinity_host_uuid: None }).await?;
            owned.push(uuid.clone());
            let target = ObjectTarget { kind: ObjectKind::Vm, uuid: uuid.clone() };
            let original = c.object_properties(&target).await?;
            let mut edited = original.clone();
            edited.tags.push("zz-parity-tag".into()); edited.folder = "/zz-parity".into();
            edited.custom_definitions.push(CustomFieldDefinition { name: "zz-parity-field".into(), date: false });
            edited.custom_fields.insert("zz-parity-field".into(), "M6 value".into());
            let saved = c.update_object_properties(&ObjectPropertiesEdit { original, edited }).await?;
            let reference: String = c.request("VM.get_by_uuid", json!([c.session, uuid])).await?;
            let record: Value = c.request("VM.get_record", json!([c.session, reference])).await?;
            if record["tags"] != json!(["zz-parity-tag"]) || record["other_config"]["folder"] != "/zz-parity" || record["other_config"]["XenCenter.CustomFields.zz-parity-field"] != "M6 value" { return Err(ApiError::Xen("Organization readback mismatch".into())); }
            let mut rejected = saved.clone(); rejected.tags.push("\n".into());
            if c.update_object_properties(&ObjectPropertiesEdit { original: saved.clone(), edited: rejected }).await.is_ok() { return Err(ApiError::Xen("Invalid tag was accepted".into())); }
            let evidence = std::path::Path::new("../../docs/supervision/evidence/M6");
            std::fs::write(evidence.join("organization-readback.json"), serde_json::to_vec_pretty(&json!({"vm_uuid":uuid,"tags":record["tags"],"folder":record["other_config"]["folder"],"custom_field":record["other_config"]["XenCenter.CustomFields.zz-parity-field"],"definitions":saved.custom_definitions,"invalid_tag":"rejected"})).unwrap()).map_err(|e| ApiError::Xen(e.to_string()))?;
            println!("M6 organization readback verified; UI READY: {uuid}");
            if std::env::var_os("M6_WAIT_FOR_UI").is_some() {
                let marker = std::path::Path::new("/tmp/zz-parity-M6-ui-done");
                for _ in 0..1200 {
                    if marker.exists() { break; }
                    tokio::time::sleep(std::time::Duration::from_millis(500)).await;
                }
            }
            c.change_organization(&OrganizationChange::Tag { before: "zz-parity-tag".into(), after: "zz-parity-M6-tag-renamed".into() }).await?;
            let tags: Vec<String> = c.request("VM.get_tags", json!([c.session, reference])).await?;
            if tags != ["zz-parity-M6-tag-renamed"] { return Err(ApiError::Protocol); }
            c.change_organization(&OrganizationChange::Tag { before: "zz-parity-M6-tag-renamed".into(), after: String::new() }).await?;
            c.change_organization(&OrganizationChange::Folder { before: "/zz-parity".into(), after: "/zz-parity-M6-renamed".into() }).await?;
            if c.object_properties(&target).await?.folder != "/zz-parity-M6-renamed" { return Err(ApiError::Protocol); }
            c.change_organization(&OrganizationChange::Folder { before: "/zz-parity-M6-renamed".into(), after: String::new() }).await?;
            c.change_organization(&OrganizationChange::Folder { before: String::new(), after: "/zz-parity-M6-empty".into() }).await?;
            if !c.inventory().await?.empty_folders.contains(&"/zz-parity-M6-empty".into()) { return Err(ApiError::Protocol); }
            c.change_organization(&OrganizationChange::Folder { before: "/zz-parity-M6-empty".into(), after: String::new() }).await?;
            c.extra_action(&ExtraAction::Recovery(uuid.clone())).await?;
            let power: String = c.request("VM.get_power_state", json!([c.session, reference])).await?;
            if power != "Running" { return Err(ApiError::Protocol); }
            if c.extra_action(&ExtraAction::Recovery(uuid.clone())).await.is_ok() { return Err(ApiError::Xen("Recovery start accepted a running VM".into())); }
            c.request::<Value>("VM.hard_shutdown", json!([c.session, reference])).await?;
            if c.extra_action(&ExtraAction::DisableCbt(uuid.clone())).await.is_ok() { return Err(ApiError::Xen("CBT disable accepted a VM with no CBT disks".into())); }
            if c.extra_action(&ExtraAction::QuickCreate(uuid.clone())).await.is_ok() { return Err(ApiError::Xen("Quick Create accepted a non-template".into())); }
            println!("M6 global folder/tag operations and Recovery Mode readbacks verified; CBT/Quick Create rejection verified");
            Ok(())
        }.await;
        // Only UUIDs created above can be removed. Never operate on pre-existing VMs.
        for uuid in owned {
            let reference: String = c
                .request("VM.get_by_uuid", json!([c.session, uuid]))
                .await
                .unwrap();
            let record: Value = c
                .request("VM.get_record", json!([c.session, reference]))
                .await
                .unwrap();
            assert_eq!(record["name_label"], "zz-parity-M6-vm");
            if record["power_state"] != "Halted" {
                c.request::<Value>("VM.hard_shutdown", json!([c.session, reference]))
                    .await
                    .unwrap();
            }
            c.request::<Value>("VM.destroy", json!([c.session, reference]))
                .await
                .unwrap();
        }
        // Restore only keys touched by this unit, preserving the exact original presence/value.
        c.request::<Value>(
            "pool.remove_from_gui_config",
            json!([c.session, pool_ref, CUSTOM_FIELDS]),
        )
        .await
        .unwrap();
        if let Some(value) = pool["gui_config"].get(CUSTOM_FIELDS) {
            c.request::<Value>(
                "pool.add_to_gui_config",
                json!([c.session, pool_ref, CUSTOM_FIELDS, value]),
            )
            .await
            .unwrap();
        }
        c.request::<Value>(
            "pool.remove_from_other_config",
            json!([c.session, pool_ref, "EMPTY_FOLDERS"]),
        )
        .await
        .unwrap();
        if let Some(value) = pool["other_config"].get("EMPTY_FOLDERS") {
            c.request::<Value>(
                "pool.add_to_other_config",
                json!([c.session, pool_ref, "EMPTY_FOLDERS", value]),
            )
            .await
            .unwrap();
        }
        let after = c.records("VM").await.unwrap();
        assert_eq!(
            initial.keys().collect::<std::collections::BTreeSet<_>>(),
            after.keys().collect()
        );
        let pools = c.records("pool").await.unwrap();
        assert_eq!(pools[pool_ref]["gui_config"], pool["gui_config"]);
        assert_eq!(pools[pool_ref]["other_config"], pool["other_config"]);
        c.logout().await.unwrap();
        println!("M6 cleanup verified: VM set and original pool metadata restored");
        assert!(result.is_ok(), "M6 live failure: {:?}", result.err());
    }
}

#[cfg(test)]
mod live_menu_tests {
    use super::*;
    use crate::{
        provisioning::{CreateVmRequest, VmCreationSource},
        transport::TlsOptions,
        xapi::{Connection, ConnectionSettings},
    };
    use serde_json::{Value, json};
    #[tokio::test]
    #[ignore = "Requires authorized .170 lab and XEN_DEV_* environment"]
    async fn m6_live_menu_actions() {
        let settings = ConnectionSettings {
            host: std::env::var("XEN_DEV_HOST").unwrap(),
            username: std::env::var("XEN_DEV_USER").unwrap(),
            label: "M6 menu qualification".into(),
            read_only: false,
            tls: TlsOptions {
                bypass_certificate_verification: true,
            },
            proxy: None,
            timeout_seconds: 45,
        };
        assert_eq!(settings.host, "192.168.8.170");
        let c = Connection::login(settings, &std::env::var("XEN_DEV_PASSWORD").unwrap())
            .await
            .unwrap();
        let initial = c.records("VM").await.unwrap();
        let initial_disks = c.records("VDI").await.unwrap();
        assert!(!initial.values().any(|v| {
            v["name_label"]
                .as_str()
                .is_some_and(|n| n.starts_with("zz-parity-M6-"))
        }));
        let temp = tempfile::tempdir().unwrap();
        let mut evidence = serde_json::Map::new();
        let run: Result<(), ApiError> = async {
            let inventory = c.inventory().await?;
            let template = inventory.templates.iter().find(|t| t.name == "Other install media").ok_or(ApiError::Protocol)?;
            let sr = inventory.storage.iter().find(|s| s.name == "Local storage").ok_or(ApiError::Protocol)?;
            let uuid = c.create_configured_vm(&CreateVmRequest { guest_setup: None, start_automatically: false, source: VmCreationSource::Template(template.uuid.clone()), name: "zz-parity-M6-extra".into(), description: "Disposable M6 menu qualification".into(), vcpus_max: 1, vcpus_at_startup: 1, cores_per_socket: None, memory_mib: 512, sr_uuid: sr.uuid.clone(), disks: Some(vec![]), interfaces: Some(vec![]), media: Default::default(), pv_args: None, copy_bios_from: None, new_disk_gib: None, network_uuid: None, iso_uuid: None, affinity_host_uuid: None }).await?;
            let reference: String = c.request("VM.get_by_uuid", json!([c.session, uuid])).await?;
            let describe = |error: ApiError| error.to_string();
            c.add_disk(&uuid, &sr.uuid, 64 * 1024 * 1024, Some("zz-parity-M6-cbt")).await?;
            let disks = c.records("VDI").await?;
            let disk = disks.iter().find(|(_,v)| v["name_label"] == "zz-parity-M6-cbt" && !initial_disks.values().any(|old| old["uuid"] == v["uuid"])).ok_or(ApiError::Protocol)?.0.clone();
            match c.request::<Value>("VDI.enable_cbt", json!([c.session, disk])).await {
                Ok(_) => {
                    c.extra_action(&ExtraAction::DisableCbt(uuid.clone())).await?;
                    let enabled: bool = c.request("VDI.get_cbt_enabled", json!([c.session, disk])).await?;
                    if enabled { return Err(ApiError::Xen("CBT remained enabled".into())); }
                    evidence.insert("disable_cbt".into(), json!("verified: VDI.get_cbt_enabled=false"));
                }
                Err(error) => { evidence.insert("disable_cbt".into(), json!(format!("not qualified: enable_cbt prerequisite failed: {}", describe(error)))); }
            }
            // The instant-template rejection is exercised on our ordinary VM.
            if c.extra_action(&ExtraAction::QuickCreate(uuid.clone())).await.is_ok() { return Err(ApiError::Xen("Quick Create accepted an ordinary VM".into())); }
            c.request::<Value>("VM.set_is_a_template", json!([c.session, reference, true])).await?;
            c.request::<Value>("VM.add_to_other_config", json!([c.session, reference, "instant", "true"])).await?;
            let quick = c.extra_action(&ExtraAction::QuickCreate(uuid.clone())).await;
            let clones: Vec<_> = c.records("VM").await?.into_iter().filter(|(_,v)| v["name_label"].as_str().is_some_and(|n| n.starts_with("zz-parity-M6-extra ("))).collect();
            if clones.len() != 1 { return Err(ApiError::Xen("Expected one Quick Create clone".into())); }
            evidence.insert("quick_create".into(), json!({"clone_count":clones.len(),"power_state":clones[0].1["power_state"],"result":quick.map(|()| "verified clone/provision/start".to_owned()).unwrap_or_else(describe)}));
            let path = temp.path().join("status.tar.bz2");
            let host = &inventory.hosts[0].uuid;
            match c.server_change(host, crate::server_admin::ServerChange::StatusReport(path.clone())).await {
                Ok(()) => {
                    let bytes = std::fs::read(&path).map_err(|e| ApiError::Xen(e.to_string()))?;
                    evidence.insert("system_status".into(), json!({"bytes":bytes.len(),"bzip2_magic":bytes.starts_with(b"BZh")}));
                    if !bytes.starts_with(b"BZh") { return Err(ApiError::Xen("Status report is not a bzip2 archive".into())); }
                }
                Err(error) => { evidence.insert("system_status".into(), json!(describe(error))); }
            }
            match crate::updates::latest_release(env!("CARGO_PKG_VERSION")).await {
                Ok((message,url)) => { evidence.insert("update_check".into(), json!({"message":message,"url":url})); }
                Err(error) => { evidence.insert("update_check".into(),json!(error)); }
            }
            Ok(())
        }.await;
        evidence.insert("run_result".into(), json!(format!("{run:?}")));
        std::fs::write(
            "../../docs/supervision/evidence/M6/menu-readback.json",
            serde_json::to_vec_pretty(&evidence).unwrap(),
        )
        .unwrap();
        // Reconcile new UUIDs only, and require the unit prefix before deletion.
        for (reference, record) in c.records("VM").await.unwrap() {
            if !initial.contains_key(&reference)
                && record["name_label"]
                    .as_str()
                    .is_some_and(|n| n.starts_with("zz-parity-M6-"))
            {
                if record["power_state"] != "Halted" {
                    c.request::<Value>("VM.hard_shutdown", json!([c.session, reference]))
                        .await
                        .unwrap();
                }
                c.request::<Value>("VM.destroy", json!([c.session, reference]))
                    .await
                    .unwrap();
            }
        }
        for (reference, record) in c.records("VDI").await.unwrap() {
            if !initial_disks.contains_key(&reference)
                && record["name_label"]
                    .as_str()
                    .is_some_and(|n| n.starts_with("zz-parity-M6-"))
            {
                c.request::<Value>("VDI.destroy", json!([c.session, reference]))
                    .await
                    .unwrap();
            }
        }
        let after = c.records("VM").await.unwrap();
        assert_eq!(
            initial.keys().collect::<std::collections::BTreeSet<_>>(),
            after.keys().collect()
        );
        // XenAPI may briefly retain a VDI while backend deletion completes.
        let mut after = c.records("VDI").await.unwrap();
        for _ in 0..20 {
            if initial_disks
                .keys()
                .collect::<std::collections::BTreeSet<_>>()
                == after.keys().collect()
            {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(250)).await;
            after = c.records("VDI").await.unwrap();
        }
        assert_eq!(
            initial_disks
                .keys()
                .collect::<std::collections::BTreeSet<_>>(),
            after.keys().collect()
        );
        evidence.insert("cleanup".into(), json!("original VM and VDI sets restored"));
        std::fs::write(
            "../../docs/supervision/evidence/M6/menu-readback.json",
            serde_json::to_vec_pretty(&evidence).unwrap(),
        )
        .unwrap();
        c.logout().await.unwrap();
        assert!(run.is_ok(), "Menu qualification failed: {:?}", run.err());
    }
}
