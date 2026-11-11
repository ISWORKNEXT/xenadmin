//! Configured template creation used by the native New VM wizard.
use quick_xml::{Reader, Writer, events::Event};
use serde::Serialize;
use serde_json::{Value, json};

use crate::xapi::{ApiError, Connection};

#[derive(Clone)]
pub struct CreateVmRequest {
    pub template_uuid: String,
    pub name: String,
    pub description: String,
    pub vcpus: u64,
    pub memory_mib: u64,
    pub sr_uuid: String,
    pub new_disk_gib: Option<u64>,
    pub network_uuid: Option<String>,
    pub iso_uuid: Option<String>,
    pub affinity_host_uuid: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
pub struct ProvisionDisk {
    pub device: String,
    pub size: u64,
}

fn invalid_blueprint() -> ApiError {
    ApiError::Xen("The template has an invalid disk provisioning specification.".into())
}

pub(crate) fn provision_disks(xml: &str) -> Result<Vec<ProvisionDisk>, ApiError> {
    let mut reader = Reader::from_str(xml);
    let mut disks = Vec::new();
    loop {
        match reader.read_event().map_err(|_| invalid_blueprint())? {
            Event::Start(element) | Event::Empty(element) if element.name().as_ref() == b"disk" => {
                let mut device = None;
                let mut size = None;
                for attribute in element.attributes() {
                    let attribute = attribute.map_err(|_| invalid_blueprint())?;
                    match attribute.key.as_ref() {
                        b"device" => {
                            device = Some(
                                attribute
                                    .decoded_and_normalized_value(
                                        quick_xml::XmlVersion::Implicit1_0,
                                        reader.decoder(),
                                    )
                                    .map_err(|_| invalid_blueprint())?
                                    .into_owned(),
                            )
                        }
                        b"size" => {
                            size = Some(
                                attribute
                                    .decoded_and_normalized_value(
                                        quick_xml::XmlVersion::Implicit1_0,
                                        reader.decoder(),
                                    )
                                    .map_err(|_| invalid_blueprint())?
                                    .parse::<u64>()
                                    .map_err(|_| invalid_blueprint())?,
                            )
                        }
                        _ => {}
                    }
                }
                let disk = ProvisionDisk {
                    device: device.ok_or_else(invalid_blueprint)?,
                    size: size.ok_or_else(invalid_blueprint)?,
                };
                if disk.size == 0
                    || disks
                        .iter()
                        .any(|other: &ProvisionDisk| other.device == disk.device)
                {
                    return Err(invalid_blueprint());
                }
                disks.push(disk);
            }
            Event::Eof => break,
            _ => {}
        }
    }
    Ok(disks)
}

fn configure_blueprint(
    xml: &str,
    sr_uuid: &str,
    first_disk_size: Option<u64>,
) -> Result<String, ApiError> {
    provision_disks(xml)?;
    let mut reader = Reader::from_str(xml);
    let mut writer = Writer::new(Vec::new());
    let mut first = true;
    loop {
        let event = reader.read_event().map_err(|_| invalid_blueprint())?;
        if event == Event::Eof {
            break;
        }
        let is_start = matches!(&event, Event::Start(_));
        let event = match event {
            Event::Start(element) | Event::Empty(element) if element.name().as_ref() == b"disk" => {
                let mut configured = element.to_owned();
                configured.clear_attributes();
                for attribute in element.attributes() {
                    let attribute = attribute.map_err(|_| invalid_blueprint())?;
                    if attribute.key.as_ref() != b"sr"
                        && !(first
                            && first_disk_size.is_some()
                            && attribute.key.as_ref() == b"size")
                    {
                        configured.push_attribute(attribute);
                    }
                }
                configured.push_attribute(("sr", sr_uuid));
                if first && let Some(size) = first_disk_size {
                    configured.push_attribute(("size", size.to_string().as_str()));
                }
                first = false;
                if is_start {
                    Event::Start(configured)
                } else {
                    Event::Empty(configured)
                }
            }
            other => other.into_owned(),
        };
        writer.write_event(event).map_err(|_| invalid_blueprint())?;
    }
    String::from_utf8(writer.into_inner()).map_err(|_| invalid_blueprint())
}

impl Connection {
    pub async fn convert_vm_to_template(&self, uuid: &str) -> Result<(), ApiError> {
        if self.settings.read_only {
            return Err(ApiError::Xen("This connection is read-only.".into()));
        }
        let reference: String = self
            .request("VM.get_by_uuid", json!([self.session, uuid]))
            .await?;
        let record: Value = self
            .request("VM.get_record", json!([self.session, reference]))
            .await?;
        if record["uuid"].as_str() != Some(uuid)
            || record["is_control_domain"].as_bool() != Some(false)
            || record["is_a_snapshot"].as_bool() != Some(false)
        {
            return Err(ApiError::Xen(
                "Only a normal VM can be converted to a template.".into(),
            ));
        }
        if record["is_a_template"].as_bool() == Some(true) {
            self.request::<Value>(
                "VM.update_allowed_operations",
                json!([self.session, reference]),
            )
            .await?;
            return Ok(());
        }
        if record["power_state"].as_str() != Some("Halted")
            || !record["allowed_operations"].as_array().is_some_and(|ops| {
                ops.iter()
                    .any(|op| op.as_str() == Some("make_into_template"))
            })
        {
            return Err(ApiError::Xen(
                "Halt the VM and wait for its active tasks before converting it to a template."
                    .into(),
            ));
        }
        let mut config = record["other_config"]
            .as_object()
            .cloned()
            .unwrap_or_default();
        config.insert("instant".into(), Value::String("true".into()));
        self.request::<Value>(
            "VM.set_other_config",
            json!([self.session, reference, config]),
        )
        .await?;
        self.request::<Value>(
            "VM.set_is_a_template",
            json!([self.session, reference, true]),
        )
        .await?;
        self.request::<Value>(
            "VM.update_allowed_operations",
            json!([self.session, reference]),
        )
        .await?;
        Ok(())
    }

    pub async fn create_from_snapshot(
        &self,
        snapshot_uuid: &str,
        name: &str,
        as_template: bool,
    ) -> Result<String, ApiError> {
        if self.settings.read_only {
            return Err(ApiError::Xen(
                "This connection is read-only. Changes are blocked by the Rust backend.".into(),
            ));
        }
        let source = self.vm_details(snapshot_uuid).await?;
        if !source.is_snapshot {
            return Err(ApiError::Xen(
                "Select a snapshot to create a VM or template.".into(),
            ));
        }
        let uuid = self.clone_vm_unprovisioned(snapshot_uuid, name).await?;
        let result = async {
            let details = self.vm_details(&uuid).await?;
            if details.is_snapshot || details.power_state != "Halted" {
                return Err(ApiError::Xen(
                    "The snapshot clone is not a halted, independent object.".into(),
                ));
            }
            for disk in details.disks.iter().filter(|disk| !disk.is_cd) {
                if source
                    .disks
                    .iter()
                    .any(|original| !original.is_cd && original.vdi_uuid == disk.vdi_uuid)
                {
                    return Err(ApiError::Xen(
                        "The new object still references a source snapshot disk.".into(),
                    ));
                }
                if !disk.owner {
                    let vbd: String = self
                        .request("VBD.get_by_uuid", json!([self.session, disk.vbd_uuid]))
                        .await?;
                    self.request::<Value>(
                        "VBD.add_to_other_config",
                        json!([self.session, vbd, "owner", "true"]),
                    )
                    .await?;
                }
            }
            let reference: String = self
                .request("VM.get_by_uuid", json!([self.session, uuid]))
                .await?;
            if details.is_template != as_template {
                self.request::<Value>(
                    "VM.set_is_a_template",
                    json!([self.session, reference, as_template]),
                )
                .await?;
            }
            if as_template {
                let mut config: std::collections::BTreeMap<String, String> = self
                    .request("VM.get_other_config", json!([self.session, reference]))
                    .await?;
                config.insert("instant".into(), "true".into());
                self.request::<Value>(
                    "VM.set_other_config",
                    json!([self.session, reference, config]),
                )
                .await?;
            }
            self.request::<Value>(
                "VM.update_allowed_operations",
                json!([self.session, reference]),
            )
            .await?;
            Ok::<(), ApiError>(())
        }
        .await;
        match result {
            Ok(()) => Ok(uuid),
            Err(error) => match self.cleanup_new_vm(&uuid, &source).await {
                Ok(()) => Err(ApiError::Xen(format!(
                    "Snapshot creation failed and the temporary object was removed: {error}"
                ))),
                Err(cleanup) => Err(ApiError::Xen(format!(
                    "Snapshot creation failed: {error}. Review object {uuid} and its disks because cleanup failed: {cleanup}"
                ))),
            },
        }
    }

    pub async fn create_vm_from_template(
        &self,
        request: &CreateVmRequest,
    ) -> Result<String, ApiError> {
        if self.settings.read_only {
            return Err(ApiError::Xen(
                "This connection is read-only. Changes are blocked by the Rust backend.".into(),
            ));
        }
        if request.name.trim().is_empty()
            || request.name.chars().count() > 255
            || request.vcpus == 0
            || request.memory_mib == 0
            || request.memory_mib.checked_mul(1024 * 1024).is_none()
            || request
                .new_disk_gib
                .is_some_and(|size| size == 0 || size.checked_mul(1024 * 1024 * 1024).is_none())
        {
            return Err(ApiError::Xen(
                "Enter a valid VM name, description, CPU, memory and disk size.".into(),
            ));
        }
        let inventory = self.inventory().await?;
        let template = inventory
            .templates
            .iter()
            .find(|vm| vm.uuid == request.template_uuid)
            .ok_or_else(|| ApiError::Xen("The selected template is no longer available.".into()))?;
        if !template
            .allowed_operations
            .iter()
            .any(|operation| operation == "clone")
        {
            return Err(ApiError::Xen(
                "The host does not currently allow cloning this template.".into(),
            ));
        }
        let details = self.vm_details(&request.template_uuid).await?;
        crate::properties::validate_description_change(&details.description, &request.description)?;
        if !details.is_template || request.memory_mib * 1024 * 1024 < details.memory_static_min {
            return Err(ApiError::Xen(
                "The memory is below the template's minimum.".into(),
            ));
        }
        let sr = inventory
            .storage
            .iter()
            .find(|sr| {
                sr.uuid == request.sr_uuid
                    && sr.content_type != "iso"
                    && !matches!(sr.storage_type.as_str(), "udev" | "iso")
                    && sr.pbd_attached > 0
            })
            .ok_or_else(|| {
                ApiError::Xen("Select an attached storage repository for VM disks.".into())
            })?;
        let disk_bytes = request.new_disk_gib.unwrap_or(0) * 1024 * 1024 * 1024;
        let installation =
            details.disks.iter().all(|disk| disk.is_cd) && !details.provision_disks.is_empty();
        let blueprint_bytes = details
            .provision_disks
            .iter()
            .enumerate()
            .try_fold(0_u64, |total, (index, disk)| {
                total.checked_add(if index == 0 {
                    request.new_disk_gib.map_or(disk.size, |_| disk_bytes)
                } else {
                    disk.size
                })
            })
            .ok_or_else(|| ApiError::Xen("The template disks are too large.".into()))?;
        let required_bytes = details
            .disks
            .iter()
            .filter(|disk| !disk.is_cd)
            .try_fold(
                if installation {
                    blueprint_bytes
                } else {
                    disk_bytes
                },
                |total, disk| total.checked_add(disk.virtual_size),
            )
            .ok_or_else(|| ApiError::Xen("The selected disks are too large.".into()))?;
        if required_bytes > sr.total.saturating_sub(sr.used) {
            return Err(ApiError::Xen(
                "The selected storage does not have enough free space for these disks.".into(),
            ));
        }
        if let Some(uuid) = &request.network_uuid
            && !inventory
                .networks
                .iter()
                .any(|network| &network.uuid == uuid && network.bridge != "xenapi")
        {
            return Err(ApiError::Xen("Select an available guest network.".into()));
        }
        if let Some(uuid) = &request.iso_uuid
            && !details.isos.iter().any(|iso| &iso.uuid == uuid)
        {
            return Err(ApiError::Xen(
                "The selected installation ISO is no longer available.".into(),
            ));
        }
        if let Some(uuid) = &request.affinity_host_uuid
            && !inventory
                .hosts
                .iter()
                .any(|host| &host.uuid == uuid && host.enabled)
        {
            return Err(ApiError::Xen("Select an enabled home server.".into()));
        }
        let uuid = self
            .clone_vm_unprovisioned(&request.template_uuid, &request.name)
            .await?;
        match self.configure_new_vm(&uuid, request, &details).await {
            Ok(()) => Ok(uuid),
            Err(error) => match self.cleanup_new_vm(&uuid, &details).await {
                Ok(()) => Err(ApiError::Xen(format!(
                    "VM creation failed and the temporary VM was removed: {error}"
                ))),
                Err(cleanup) => Err(ApiError::Xen(format!(
                    "VM {uuid} was created but configuration failed: {error}. Cleanup also failed: {cleanup}. Review this VM before creating another."
                ))),
            },
        }
    }

    async fn configure_new_vm(
        &self,
        uuid: &str,
        request: &CreateVmRequest,
        template: &crate::vm::VmDetails,
    ) -> Result<(), ApiError> {
        let reference: String = self
            .request("VM.get_by_uuid", json!([self.session, uuid]))
            .await?;
        let record: Value = self
            .request("VM.get_record", json!([self.session, reference]))
            .await?;
        let disks = self.vm_details(uuid).await?;
        for disk in disks.disks.iter().filter(|disk| !disk.is_cd) {
            if template
                .disks
                .iter()
                .any(|source| !source.is_cd && source.vdi_uuid == disk.vdi_uuid)
            {
                return Err(ApiError::Xen(
                    "The cloned VM still references a template disk. Creation stopped.".into(),
                ));
            }
        }
        let xml = record["other_config"]["disks"].as_str().unwrap_or_default();
        let installation =
            disks.disks.iter().all(|disk| disk.is_cd) && !provision_disks(xml)?.is_empty();
        if installation {
            let configured = configure_blueprint(
                xml,
                &request.sr_uuid,
                request.new_disk_gib.map(|size| size * 1024 * 1024 * 1024),
            )?;
            let mut config = record["other_config"]
                .as_object()
                .cloned()
                .unwrap_or_default();
            config.insert("disks".into(), Value::String(configured));
            self.request::<Value>(
                "VM.set_other_config",
                json!([self.session, reference, config]),
            )
            .await?;
            self.async_task("Async.VM.provision", json!([self.session, reference]))
                .await?;
            self.request::<Value>(
                "VM.remove_from_other_config",
                json!([self.session, reference, "disks"]),
            )
            .await?;
        } else {
            self.request::<Value>(
                "VM.set_is_a_template",
                json!([self.session, reference, false]),
            )
            .await?;
            self.request::<Value>(
                "VM.update_allowed_operations",
                json!([self.session, reference]),
            )
            .await?;
        }
        // Newly cloned/provisioned disks belong to this VM, never to its source.
        for disk in self
            .vm_details(uuid)
            .await?
            .disks
            .iter()
            .filter(|disk| !disk.is_cd)
        {
            if template
                .disks
                .iter()
                .any(|source| !source.is_cd && source.vdi_uuid == disk.vdi_uuid)
            {
                return Err(ApiError::Xen(
                    "The new VM references a template disk.".into(),
                ));
            }
            let vbd: String = self
                .request("VBD.get_by_uuid", json!([self.session, disk.vbd_uuid]))
                .await?;
            if !disk.owner {
                self.request::<Value>(
                    "VBD.add_to_other_config",
                    json!([self.session, vbd, "owner", "true"]),
                )
                .await?;
            }
        }
        let original = self
            .object_properties(&crate::properties::ObjectTarget {
                kind: crate::properties::ObjectKind::Vm,
                uuid: uuid.to_owned(),
            })
            .await?;
        let mut edited = original.clone();
        edited.name = request.name.clone();
        edited.description = request.description.clone();
        self.update_object_properties(&crate::properties::ObjectPropertiesEdit {
            original,
            edited,
        })
        .await?;
        self.update_vm_resources(
            uuid,
            &crate::operations::VmResourceUpdate {
                vcpus_max: request.vcpus,
                vcpus_at_startup: request.vcpus,
                memory_static_max_mib: request.memory_mib,
                memory_dynamic_min_mib: request.memory_mib,
                memory_dynamic_max_mib: request.memory_mib,
                boot_order: None,
                firmware: None,
                secure_boot: None,
                boot_policy: None,
                after_crash: None,
            },
        )
        .await?;
        self.update_vm_ha(uuid, "", 0, 0, request.affinity_host_uuid.as_deref())
            .await?;
        let details = self.vm_details(uuid).await?;
        for disk in details.disks.iter().filter(|disk| !disk.is_cd) {
            if disk.sr_uuid != request.sr_uuid {
                self.move_disk(uuid, &disk.vbd_uuid, &request.sr_uuid)
                    .await?;
            }
        }
        if let Some(size) = request.new_disk_gib.filter(|_| !installation) {
            self.add_disk(uuid, &request.sr_uuid, size * 1024 * 1024 * 1024, None)
                .await?;
            if details.disks.iter().all(|disk| disk.is_cd)
                && let Some(disk) = self
                    .vm_details(uuid)
                    .await?
                    .disks
                    .iter()
                    .find(|disk| !disk.is_cd)
            {
                self.set_vbd_bootable(uuid, &disk.vbd_uuid, true).await?;
            }
        }
        for interface in &details.interfaces {
            self.remove_vif(uuid, &interface.uuid).await?;
        }
        if let Some(network) = &request.network_uuid {
            self.add_vif(uuid, network, None).await?;
        }
        if let Some(iso) = &request.iso_uuid {
            if details.disks.iter().all(|disk| !disk.is_cd) {
                self.add_cd_drive(uuid).await?;
            }
            let current = self.vm_details(uuid).await?;
            let cd = current
                .disks
                .iter()
                .find(|disk| disk.is_cd)
                .ok_or(ApiError::Protocol)?;
            self.change_cd(uuid, &cd.vbd_uuid, Some(iso)).await?;
            if !current.hvm_boot_policy.is_empty() {
                let mut boot = current.hvm_boot_params.clone();
                let existing = boot.get("order").cloned().unwrap_or_else(|| "cdn".into());
                boot.insert(
                    "order".into(),
                    format!(
                        "d{}",
                        existing
                            .chars()
                            .filter(|key| *key != 'd')
                            .collect::<String>()
                    ),
                );
                self.request::<Value>(
                    "VM.set_HVM_boot_params",
                    json!([self.session, reference, boot]),
                )
                .await?;
            }
        }
        Ok(())
    }
    async fn cleanup_new_vm(
        &self,
        uuid: &str,
        template: &crate::vm::VmDetails,
    ) -> Result<(), ApiError> {
        let current = self.vm_details(uuid).await?;
        let owned: Vec<String> = current
            .disks
            .iter()
            .filter(|disk| {
                !disk.is_cd
                    && !template
                        .disks
                        .iter()
                        .any(|original| !original.is_cd && original.vdi_uuid == disk.vdi_uuid)
            })
            .map(|disk| disk.vdi_uuid.clone())
            .collect();
        self.destroy_vm(uuid, false, false).await?;
        let mut failure = None;
        for vdi in owned {
            // VM.provision creates writable `system` disks as well as `user`
            // disks. These UUIDs came exclusively from this new VM, with all
            // source-template disks excluded above.
            let result = async {
                let reference: String = self
                    .request("VDI.get_by_uuid", json!([self.session, vdi]))
                    .await?;
                let record: Value = self
                    .request("VDI.get_record", json!([self.session, reference]))
                    .await?;
                if record["uuid"].as_str() != Some(vdi.as_str())
                    || record["read_only"].as_bool() != Some(false)
                    || !matches!(record["type"].as_str(), Some("user" | "system"))
                    || !record["VBDs"].as_array().is_some_and(Vec::is_empty)
                {
                    return Err(ApiError::Xen(format!(
                        "Temporary disk {vdi} could not be safely removed."
                    )));
                }
                self.request::<Value>("VDI.destroy", json!([self.session, reference]))
                    .await?;
                Ok::<(), ApiError>(())
            }
            .await;
            if let Err(error) = result {
                failure.get_or_insert(error);
            }
        }
        if let Some(error) = failure {
            return Err(error);
        }
        Ok(())
    }
}
