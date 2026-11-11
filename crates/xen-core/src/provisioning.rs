//! Configured VM creation, template preparation and independent disk copies.
use quick_xml::{
    Reader, Writer,
    events::{BytesStart, Event},
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::xapi::{ApiError, Connection};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum VmCreationSource {
    Template(String),
    Snapshot(String),
}

impl VmCreationSource {
    pub fn uuid(&self) -> &str {
        match self {
            Self::Template(uuid) | Self::Snapshot(uuid) => uuid,
        }
    }
    pub fn is_snapshot(&self) -> bool {
        matches!(self, Self::Snapshot(_))
    }
}

#[derive(Clone)]
pub struct CreateVmRequest {
    pub guest_setup: Option<Box<crate::guest_setup::GuestSetup>>,
    pub start_automatically: bool,
    pub source: VmCreationSource,
    pub name: String,
    pub description: String,
    pub vcpus_max: u64,
    pub vcpus_at_startup: u64,
    pub cores_per_socket: Option<u64>,
    pub memory_mib: u64,
    pub sr_uuid: String,
    // Legacy single-device callers inherit the source unless these lists are supplied.
    pub disks: Option<Vec<CreateDisk>>,
    pub interfaces: Option<Vec<CreateVif>>,
    pub media: InstallationMedia,
    pub pv_args: Option<String>,
    pub copy_bios_from: Option<String>,
    pub new_disk_gib: Option<u64>,
    pub network_uuid: Option<String>,
    pub iso_uuid: Option<String>,
    pub affinity_host_uuid: Option<String>,
}

/// Source device identifies a cloned disk; None creates a new disk.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CreateDisk {
    pub source_device: Option<String>,
    pub name: String,
    pub size_bytes: u64,
    pub sr_uuid: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CreateVif {
    pub network_uuid: String,
    /// Empty means generate a MAC.
    pub mac: String,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum InstallationMedia {
    #[default]
    Inherit,
    Iso(String),
    PhysicalDvd(String),
    Network,
}

impl InstallationMedia {
    pub fn boot_order(&self, existing: &str, diskless: bool) -> String {
        let first = if diskless || matches!(self, Self::Network) {
            'n'
        } else if matches!(self, Self::Iso(_) | Self::PhysicalDvd(_)) {
            'd'
        } else {
            return existing.into();
        };
        format!(
            "{first}{}",
            existing.chars().filter(|c| *c != first).collect::<String>()
        )
    }
}

impl CreateVmRequest {
    pub fn disk_list(&self, template: &crate::vm::VmDetails) -> Vec<CreateDisk> {
        if let Some(disks) = &self.disks {
            return disks.clone();
        }
        let mut disks: Vec<_> = template
            .disks
            .iter()
            .filter(|d| !d.is_cd)
            .map(|d| CreateDisk {
                source_device: Some(d.userdevice.clone()),
                name: d.vdi_name.clone(),
                size_bytes: d.virtual_size,
                sr_uuid: self.sr_uuid.clone(),
            })
            .collect();
        if disks.is_empty() && !template.provision_disks.is_empty() {
            disks.extend(
                template
                    .provision_disks
                    .iter()
                    .enumerate()
                    .map(|(i, d)| CreateDisk {
                        source_device: Some(d.device.clone()),
                        name: format!("{} disk {}", self.name, d.device),
                        size_bytes: if i == 0 {
                            self.new_disk_gib.map_or(d.size, |v| v * 1024 * 1024 * 1024)
                        } else {
                            d.size
                        },
                        sr_uuid: self.sr_uuid.clone(),
                    }),
            );
        } else if let Some(size) = self.new_disk_gib {
            disks.push(CreateDisk {
                source_device: None,
                name: format!("{} disk", self.name),
                size_bytes: size * 1024 * 1024 * 1024,
                sr_uuid: self.sr_uuid.clone(),
            });
        }
        disks
    }

    pub fn vif_list(&self) -> Vec<CreateVif> {
        self.interfaces.clone().unwrap_or_else(|| {
            self.network_uuid
                .iter()
                .map(|uuid| CreateVif {
                    network_uuid: uuid.clone(),
                    mac: String::new(),
                })
                .collect()
        })
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CopyVmRequest {
    pub name: String,
    pub description: String,
    pub sr_uuid: String,
}

/// Retained only for reconciling an interrupted copy; never resubmits VM.copy.
#[derive(Clone, Debug)]
pub struct CopyRecovery {
    pub(crate) source: crate::vm::VmDetails,
    host: String,
    pub(crate) request: CopyVmRequest,
    pub(crate) task_ref: Option<String>,
    pub(crate) copied_uuid: Option<String>,
    pub(crate) message: String,
}

impl CopyRecovery {
    pub fn task_reference(&self) -> Option<&str> {
        self.task_ref.as_deref()
    }

    fn interrupted(&self, stage: &str, error: impl std::fmt::Display) -> ApiError {
        let mut recovery = self.clone();
        recovery.message = format!(
            "Full copy outcome is unknown ({stage}): {error}. Do not submit another copy. Source: {}; destination name: {}; task: {}; copy UUID: {}. Check the existing task and inventory first.",
            self.source.uuid,
            self.request.name,
            self.task_ref.as_deref().unwrap_or("response not received"),
            self.copied_uuid.as_deref().unwrap_or("not yet read")
        );
        ApiError::CopyInterrupted(Box::new(recovery))
    }
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

/// Replace the provisioning disk list before VM.provision, retaining the XML root
/// and all attributes (including bootable/type) of retained template disks.
fn configure_blueprint(xml: &str, disks: &[CreateDisk]) -> Result<String, ApiError> {
    provision_disks(xml)?;
    let mut reader = Reader::from_str(xml);
    let mut originals = std::collections::HashMap::new();
    loop {
        match reader.read_event().map_err(|_| invalid_blueprint())? {
            Event::Start(e) | Event::Empty(e) if e.name().as_ref() == b"disk" => {
                let device = e
                    .try_get_attribute("device")
                    .map_err(|_| invalid_blueprint())?
                    .ok_or_else(invalid_blueprint)?
                    .decoded_and_normalized_value(
                        quick_xml::XmlVersion::Implicit1_0,
                        reader.decoder(),
                    )
                    .map_err(|_| invalid_blueprint())?
                    .into_owned();
                originals.insert(device, e.into_owned());
            }
            Event::Eof => break,
            _ => {}
        }
    }
    let mut writer = Writer::new(Vec::new());
    writer
        .write_event(Event::Start(BytesStart::new("provision")))
        .map_err(|_| invalid_blueprint())?;
    for (index, disk) in disks.iter().enumerate() {
        let original = disk
            .source_device
            .as_ref()
            .and_then(|device| originals.get(device));
        let mut element = BytesStart::new("disk");
        if let Some(original) = original {
            for attr in original.attributes() {
                let attr = attr.map_err(|_| invalid_blueprint())?;
                if !matches!(attr.key.as_ref(), b"device" | b"size" | b"sr") {
                    element.push_attribute(attr);
                }
            }
        } else {
            element.push_attribute(("bootable", if index == 0 { "true" } else { "false" }));
            element.push_attribute(("type", "system"));
        }
        element.push_attribute(("device", index.to_string().as_str()));
        element.push_attribute(("size", disk.size_bytes.to_string().as_str()));
        element.push_attribute(("sr", disk.sr_uuid.as_str()));
        writer
            .write_event(Event::Empty(element))
            .map_err(|_| invalid_blueprint())?;
    }
    writer
        .write_event(Event::End(quick_xml::events::BytesEnd::new("provision")))
        .map_err(|_| invalid_blueprint())?;
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

    /// Full independent copy on a selected SR, retaining VM/template identity.
    pub async fn copy_vm(&self, uuid: &str, request: &CopyVmRequest) -> Result<String, ApiError> {
        if self.settings.read_only {
            return Err(ApiError::Xen("This connection is read-only.".into()));
        }
        let name = request.name.trim();
        if name.is_empty() || name.chars().count() > 255 {
            return Err(ApiError::Xen(
                "Enter a name between 1 and 255 characters.".into(),
            ));
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
            || record["power_state"].as_str() != Some("Halted")
            || !record["allowed_operations"]
                .as_array()
                .is_some_and(|ops| ops.iter().any(|v| v.as_str() == Some("copy")))
        {
            return Err(ApiError::Xen(
                "Select a halted VM or template that the host allows copying.".into(),
            ));
        }
        let source = self.vm_details(uuid).await?;
        crate::properties::validate_description_change(&source.description, &request.description)?;
        let sr_ref: String = self
            .request("SR.get_by_uuid", json!([self.session, request.sr_uuid]))
            .await?;
        let sr: Value = self
            .request("SR.get_record", json!([self.session, sr_ref]))
            .await?;
        if sr["uuid"].as_str() != Some(request.sr_uuid.as_str())
            || sr["content_type"].as_str() == Some("iso")
            || matches!(sr["type"].as_str(), Some("iso" | "udev"))
            || !sr["allowed_operations"]
                .as_array()
                .is_some_and(|ops| ops.iter().any(|v| v.as_str() == Some("vdi_create")))
        {
            return Err(ApiError::Xen(
                "Select a writable storage repository for the copy.".into(),
            ));
        }
        let pbds = self.records("PBD").await?;
        if !pbds.values().any(|p| {
            p["SR"].as_str() == Some(sr_ref.as_str())
                && p["currently_attached"].as_bool() == Some(true)
        }) {
            return Err(ApiError::Xen(
                "The selected storage repository is detached.".into(),
            ));
        }
        let number = |v: &Value| {
            v.as_u64()
                .or_else(|| v.as_str().and_then(|s| s.parse().ok()))
                .unwrap_or(0)
        };
        let required = source
            .disks
            .iter()
            .filter(|d| !d.is_cd)
            .try_fold(0u64, |total, d| total.checked_add(d.virtual_size))
            .ok_or_else(|| ApiError::Xen("The disk sizes are too large.".into()))?;
        if number(&sr["physical_size"]).saturating_sub(number(&sr["physical_utilisation"]))
            < required
        {
            return Err(ApiError::Xen(
                "The selected storage repository does not have enough free space for the copy."
                    .into(),
            ));
        }
        let mut recovery = CopyRecovery {
            host: self.settings.host.clone(),
            source,
            request: request.clone(),
            task_ref: None,
            copied_uuid: None,
            message: String::new(),
        };
        let task: String = match self
            .request(
                "Async.VM.copy",
                json!([self.session, reference, name, sr_ref]),
            )
            .await
        {
            Ok(task) => task,
            Err(error @ (ApiError::XenCode(..) | ApiError::Xen(..) | ApiError::Unsupported)) => {
                return Err(error);
            }
            Err(error) => return Err(recovery.interrupted("submission response lost", error)),
        };
        recovery.task_ref = Some(task);
        self.resume_copy_vm(&recovery).await
    }

    pub async fn resume_copy_vm(&self, recovery: &CopyRecovery) -> Result<String, ApiError> {
        if self.settings.host != recovery.host {
            return Err(recovery.interrupted(
                "checking recovery connection",
                "Reconnect to the original copy host to reconcile this operation",
            ));
        }
        if self.settings.read_only {
            return Err(recovery.interrupted("checking recovery connection", "This connection is read-only; use the original writable connection to check the existing copy"));
        }
        let mut recovery = recovery.clone();
        if recovery.copied_uuid.is_none() {
            let Some(task_ref) = recovery.task_ref.as_deref() else {
                return Err(recovery.interrupted("submission response lost", "No remote task reference was received; manually inspect the host task list and destination name before resolving this operation"));
            };
            let result = self.wait_copy_result(&recovery, task_ref).await?;
            let copied_ref = crate::operations::decode_ref(&result).ok_or_else(|| {
                recovery.interrupted(
                    "reading copy result",
                    "The host returned an invalid object reference",
                )
            })?;
            let uuid: String = self
                .request("VM.get_uuid", json!([self.session, copied_ref]))
                .await
                .map_err(|error| recovery.interrupted("reading copy UUID", error))?;
            if uuid == recovery.source.uuid {
                return Err(
                    recovery.interrupted("reading copy UUID", "The host returned the source UUID")
                );
            }
            recovery.copied_uuid = Some(uuid);
        }
        let new_uuid = recovery.copied_uuid.clone().ok_or(ApiError::Protocol)?;
        let source = &recovery.source;
        let request = &recovery.request;
        let configured=async {
            let copied=self.vm_details(&new_uuid).await?;
            if copied.is_snapshot || copied.is_template!=source.is_template || copied.power_state!="Halted" {
                return Err(ApiError::Xen("The full copy has an unexpected type or power state.".into()));
            }
            if copied.disks.iter().filter(|d| !d.is_cd).count()
                != source.disks.iter().filter(|d| !d.is_cd).count()
            {
                return Err(ApiError::Xen("The full copy is missing source disks.".into()));
            }
            for disk in copied.disks.iter().filter(|d|!d.is_cd) {
                if disk.sr_uuid!=request.sr_uuid
                    || source.disks.iter().any(|d|!d.is_cd && d.vdi_uuid==disk.vdi_uuid)
                    || !source.disks.iter().any(|d| !d.is_cd && d.device == disk.device && d.virtual_size == disk.virtual_size)
                {
                    return Err(ApiError::Xen("The copied disks are not independent disks on the selected storage repository.".into()));
                }

            }
            let current=self.object_properties(&crate::properties::ObjectTarget{kind:crate::properties::ObjectKind::Vm,uuid:new_uuid.clone()}).await?;
            let mut original=current.clone();original.description=source.description.clone();
            let mut edited=current;edited.description=request.description.clone();
            self.update_object_properties(&crate::properties::ObjectPropertiesEdit{original,edited}).await?;
            Ok::<(),ApiError>(())
        }.await;
        match configured {
            Ok(()) => {
                if let Some(reference) = &recovery.task_ref {
                    self.destroy_task(reference).await;
                }
                Ok(new_uuid)
            }
            Err(error) => {
                Err(recovery.interrupted("configuring or validating the completed copy", error))
            }
        }
    }

    async fn wait_copy_result(
        &self,
        recovery: &CopyRecovery,
        reference: &str,
    ) -> Result<Value, ApiError> {
        for _ in 0..7200 {
            let task = self
                .task_record(reference)
                .await
                .map_err(|error| recovery.interrupted("polling existing task", error))?;
            match task.status {
                crate::operations::TaskStatus::Succeeded => {
                    let record: Value = self
                        .request("task.get_record", json!([self.session, reference]))
                        .await
                        .map_err(|error| {
                            recovery.interrupted("reading existing task result", error)
                        })?;
                    return Ok(record["result"].clone());
                }
                crate::operations::TaskStatus::Failed
                | crate::operations::TaskStatus::Cancelled => {
                    self.destroy_task(reference).await;
                    return Err(ApiError::Xen(format!(
                        "The full copy ended without success: {}",
                        task.message
                    )));
                }
                crate::operations::TaskStatus::Running
                | crate::operations::TaskStatus::Unreachable => {
                    tokio::time::sleep(std::time::Duration::from_millis(500)).await
                }
            }
        }
        Err(recovery.interrupted(
            "waiting for existing task",
            "The host task is still pending",
        ))
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
            Err(error) => Err(ApiError::Xen(format!(
                "Snapshot output {uuid} was created but configuration failed: {error}. The object and its disks were retained. Review this UUID and its task before creating another."
            ))),
        }
    }

    pub async fn create_configured_vm(
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
            || request.vcpus_max == 0
            || request.vcpus_at_startup == 0
            || request.vcpus_at_startup > request.vcpus_max
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
        let candidates = match &request.source {
            VmCreationSource::Template(_) => &inventory.templates,
            VmCreationSource::Snapshot(_) => &inventory.snapshots,
        };
        let source = candidates
            .iter()
            .find(|vm| vm.uuid == request.source.uuid())
            .ok_or_else(|| {
                ApiError::Xen(
                    "The selected source is no longer available. Refresh the inventory.".into(),
                )
            })?;
        if !source
            .allowed_operations
            .iter()
            .any(|operation| operation == "clone")
        {
            return Err(ApiError::Xen(
                "The host does not currently allow cloning this source.".into(),
            ));
        }
        let details = self.vm_details(request.source.uuid()).await?;
        if details.is_snapshot != request.source.is_snapshot()
            || (!request.source.is_snapshot() && !details.is_template)
        {
            return Err(ApiError::Xen(
                "The creation source type changed. Select the source again.".into(),
            ));
        }
        if let Some(setup) = &request.guest_setup {
            setup
                .validate(&details.guest_support)
                .map_err(|e| ApiError::Xen(e.into()))?;
        }
        crate::properties::validate_description_change(&details.description, &request.description)?;
        if request.vcpus_at_startup != request.vcpus_max && !details.supports_vcpu_hotplug {
            return Err(ApiError::Xen("This source or host does not support separate initial and maximum vCPU counts. Use equal counts.".into()));
        }
        if request.cores_per_socket.is_some() || request.vcpus_max != details.vcpus_max {
            let cores = match request.cores_per_socket {
                Some(cores) => cores,
                None => details.cores_per_socket.parse().map_err(|_| ApiError::Xen(
                    "The source CPU topology is invalid. Select a valid topology for the new VM.".into()
                ))?,
            };
            // Physical limits are checked by Resources against final placement below.
            crate::vm::validate_cpu_topology(request.vcpus_max, cores, 0)?;
        }
        if request.memory_mib * 1024 * 1024 < details.memory_static_min {
            return Err(ApiError::Xen(
                "The memory is below the source's minimum.".into(),
            ));
        }
        let requested_disks = request.disk_list(&details);
        let mut required = std::collections::HashMap::<&str, u64>::new();
        let mut sources = std::collections::HashSet::new();
        for disk in &requested_disks {
            if disk.size_bytes == 0 || disk.name.chars().count() > 255 {
                return Err(ApiError::Xen("Enter a valid disk name and size.".into()));
            }
            if let Some(device) = &disk.source_device {
                if !sources.insert(device) {
                    return Err(ApiError::Xen(
                        "A source disk can only be selected once.".into(),
                    ));
                }
                let installed = details
                    .disks
                    .iter()
                    .find(|d| !d.is_cd && &d.userdevice == device);
                if installed.is_none()
                    && !details.provision_disks.iter().any(|d| &d.device == device)
                {
                    return Err(ApiError::Xen(
                        "The source disk is no longer available.".into(),
                    ));
                }
                if installed.is_some_and(|d| disk.size_bytes < d.virtual_size) {
                    return Err(ApiError::Xen(
                        "A cloned disk can only be increased in size.".into(),
                    ));
                }
            }
            let size = required.entry(&disk.sr_uuid).or_default();
            *size = size
                .checked_add(disk.size_bytes)
                .ok_or_else(|| ApiError::Xen("The selected disks are too large.".into()))?;
        }
        for (uuid, bytes) in required {
            let sr = inventory
                .storage
                .iter()
                .find(|sr| {
                    sr.uuid == uuid
                        && sr.content_type != "iso"
                        && !matches!(sr.storage_type.as_str(), "udev" | "iso")
                        && sr.pbd_attached > 0
                })
                .ok_or_else(|| ApiError::Xen("Select attached storage for every disk.".into()))?;
            if bytes > sr.total.saturating_sub(sr.used) {
                return Err(ApiError::Xen(
                    "The selected storage does not have enough free space for these disks.".into(),
                ));
            }
        }
        for vif in request.vif_list() {
            if !inventory
                .networks
                .iter()
                .any(|n| n.uuid == vif.network_uuid && n.bridge != "xenapi")
            {
                return Err(ApiError::Xen("Select an available guest network.".into()));
            }
            validate_creation_mac(&vif.mac)?;
        }
        let media = if request.media == InstallationMedia::Inherit {
            request
                .iso_uuid
                .clone()
                .map(InstallationMedia::Iso)
                .unwrap_or_default()
        } else {
            request.media.clone()
        };
        match &media {
            InstallationMedia::Iso(uuid) if !details.isos.iter().any(|iso| &iso.uuid == uuid) => {
                return Err(ApiError::Xen(
                    "The selected installation ISO is no longer available.".into(),
                ));
            }
            InstallationMedia::PhysicalDvd(uuid)
                if !details.physical_dvds.iter().any(|dvd| {
                    &dvd.uuid == uuid && Some(&dvd.host_uuid) == request.affinity_host_uuid.as_ref()
                }) =>
            {
                return Err(ApiError::Xen(
                    "Select a physical DVD on the home server.".into(),
                ));
            }
            _ => {}
        }
        if let Some(host) = &request.copy_bios_from
            && (!details.is_default_template
                || details.hvm_boot_policy.is_empty()
                || !inventory.hosts.iter().any(|h| &h.uuid == host && h.enabled))
        {
            return Err(ApiError::Xen(
                "BIOS strings can only be copied from an enabled server to a default HVM template."
                    .into(),
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
            .clone_vm_unprovisioned(request.source.uuid(), &request.name)
            .await?;
        match self.configure_new_vm(&uuid, request, &details).await {
            Ok(()) => Ok(uuid),
            Err(error) => Err(ApiError::Xen(format!(
                "VM {uuid} was created but configuration failed: {error}. The VM and its disks were retained. Review this UUID and its task before creating another."
            ))),
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
        if disks.is_snapshot || disks.power_state != "Halted" {
            return Err(ApiError::Xen(
                "The source clone is not an independent halted object.".into(),
            ));
        }
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
            let configured = configure_blueprint(xml, &request.disk_list(template))?;
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
        self.update_vm_ha(uuid, "", 0, 0, request.affinity_host_uuid.as_deref())
            .await?;
        if let Some(host) = &request.copy_bios_from {
            let host_ref: String = self
                .request("host.get_by_uuid", json!([self.session, host]))
                .await?;
            self.request::<Value>(
                "VM.copy_bios_strings",
                json!([self.session, reference, host_ref]),
            )
            .await?;
        }
        let requested = request.disk_list(template);
        let details = self.vm_details(uuid).await?;
        if !installation {
            for disk in details.disks.iter().filter(|d| !d.is_cd) {
                let target = requested
                    .iter()
                    .find(|d| d.source_device.as_ref() == Some(&disk.userdevice));
                if let Some(target) = target {
                    if target.size_bytes > disk.virtual_size {
                        self.resize_disk(uuid, &disk.vdi_uuid, target.size_bytes)
                            .await?;
                    }
                    if target.sr_uuid != disk.sr_uuid {
                        self.move_disk(uuid, &disk.vbd_uuid, &target.sr_uuid)
                            .await?;
                    }
                } else {
                    self.remove_disk(uuid, &disk.vbd_uuid, true).await?;
                }
            }
            for disk in requested.iter().filter(|d| d.source_device.is_none()) {
                self.add_disk(uuid, &disk.sr_uuid, disk.size_bytes, Some(&disk.name))
                    .await?;
            }
        }
        let current = self.vm_details(uuid).await?;
        for (index, target) in requested.iter().enumerate() {
            let disk = if installation {
                current
                    .disks
                    .iter()
                    .find(|d| !d.is_cd && d.userdevice == index.to_string())
            } else if let Some(device) = &target.source_device {
                current
                    .disks
                    .iter()
                    .find(|d| !d.is_cd && &d.userdevice == device)
            } else {
                None
            };
            if let Some(disk) = disk {
                let vdi: String = self
                    .request("VDI.get_by_uuid", json!([self.session, disk.vdi_uuid]))
                    .await?;
                self.request::<Value>(
                    "VDI.set_name_label",
                    json!([self.session, vdi, target.name]),
                )
                .await?;
            }
        }
        if let Some(first) = current.disks.iter().find(|d| !d.is_cd) {
            self.set_vbd_bootable(uuid, &first.vbd_uuid, true).await?;
        }
        for interface in &details.interfaces {
            self.remove_vif(uuid, &interface.uuid).await?;
        }
        for vif in request.vif_list() {
            self.add_vif(uuid, &vif.network_uuid, Some(&vif.mac))
                .await?;
        }
        let media = if request.media == InstallationMedia::Inherit {
            request
                .iso_uuid
                .clone()
                .map(InstallationMedia::Iso)
                .unwrap_or_default()
        } else {
            request.media.clone()
        };
        if let InstallationMedia::Iso(iso) | InstallationMedia::PhysicalDvd(iso) = &media {
            if current.disks.iter().all(|d| !d.is_cd) {
                self.add_cd_drive(uuid).await?;
            }
            let current = self.vm_details(uuid).await?;
            let cd = current
                .disks
                .iter()
                .find(|d| d.is_cd)
                .ok_or(ApiError::Protocol)?;
            self.change_cd(uuid, &cd.vbd_uuid, Some(iso)).await?;
        } else if media == InstallationMedia::Network {
            for cd in current.disks.iter().filter(|d| d.is_cd && !d.empty) {
                self.change_cd(uuid, &cd.vbd_uuid, None).await?;
            }
        }
        if !current.hvm_boot_policy.is_empty() {
            let mut boot = current.hvm_boot_params.clone();
            let existing = boot.get("order").cloned().unwrap_or_else(|| "cdn".into());
            boot.insert(
                "order".into(),
                media.boot_order(&existing, requested.is_empty()),
            );
            self.request::<Value>(
                "VM.set_HVM_boot_params",
                json!([self.session, reference, boot]),
            )
            .await?;
        }
        self.update_vm_resources(
            uuid,
            &crate::operations::VmResourceUpdate {
                pv_args: template
                    .hvm_boot_policy
                    .is_empty()
                    .then(|| request.pv_args.clone())
                    .flatten(),
                auto_poweron: None,
                vcpus_max: request.vcpus_max,
                vcpus_at_startup: request.vcpus_at_startup,
                memory_static_max_mib: request.memory_mib,
                memory_dynamic_min_mib: request.memory_mib,
                memory_dynamic_max_mib: request.memory_mib,
                boot_order: None,
                firmware: None,
                secure_boot: None,
                boot_policy: None,
                after_crash: None,
                cpu_weight: None,
                cores_per_socket: request.cores_per_socket,
            },
        )
        .await?;
        if let Some(setup) = &request.guest_setup {
            self.update_guest_setup(uuid, setup, false).await?;
        }
        if request.start_automatically {
            self.preflight_boot(&reference).await?;
            self.async_task(
                "Async.VM.start",
                json!([self.session, reference, false, false]),
            )
            .await?;
        }
        Ok(())
    }
}

fn validate_creation_mac(mac: &str) -> Result<(), ApiError> {
    if mac.is_empty() {
        return Ok(());
    }
    let octets: Option<Vec<u8>> = mac
        .split(':')
        .map(|part| {
            if part.len() == 2 {
                u8::from_str_radix(part, 16).ok()
            } else {
                None
            }
        })
        .collect();
    if !octets.is_some_and(|v| v.len() == 6 && v[0] & 1 == 0 && v.iter().any(|b| *b != 0)) {
        return Err(ApiError::Xen("Enter a unicast MAC address like 02:00:00:00:00:01, or leave it empty to generate one.".into()));
    }
    Ok(())
}

#[cfg(test)]
mod creation_tests {
    use super::*;

    #[test]
    fn provision_xml_edits_delete_resize_move_and_add_disks() {
        let xml = r#"<provision><disk device="0" size="100" sr="old" bootable="true" type="system"/><disk device="1" size="200" sr="old" bootable="false" type="user"/></provision>"#;
        let disks = vec![
            CreateDisk {
                source_device: Some("1".into()),
                name: "Renamed".into(),
                size_bytes: 300,
                sr_uuid: "a&b".into(),
            },
            CreateDisk {
                source_device: None,
                name: "Added".into(),
                size_bytes: 400,
                sr_uuid: "second".into(),
            },
        ];
        let result = configure_blueprint(xml, &disks).unwrap();
        let parsed = provision_disks(&result).unwrap();
        assert_eq!(
            parsed
                .iter()
                .map(|d| (d.device.as_str(), d.size))
                .collect::<Vec<_>>(),
            vec![("0", 300), ("1", 400)]
        );
        assert!(result.contains("sr=\"a&amp;b\""));
        assert!(result.contains("type=\"user\""));
        assert!(!result.contains("size=\"100\""));
        assert!(
            provision_disks(&configure_blueprint(xml, &[]).unwrap())
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn installation_media_boot_orders() {
        assert_eq!(InstallationMedia::Network.boot_order("cdn", false), "ncd");
        assert_eq!(
            InstallationMedia::Iso("iso".into()).boot_order("cdn", false),
            "dcn"
        );
        assert_eq!(
            InstallationMedia::PhysicalDvd("dvd".into()).boot_order("ncd", false),
            "dnc"
        );
        assert_eq!(InstallationMedia::Inherit.boot_order("cdn", true), "ncd");
        assert_eq!(InstallationMedia::Inherit.boot_order("cdn", false), "cdn");
    }

    #[test]
    fn vif_request_preserves_order_and_manual_or_automatic_mac() {
        let vifs = vec![
            CreateVif {
                network_uuid: "same".into(),
                mac: String::new(),
            },
            CreateVif {
                network_uuid: "same".into(),
                mac: "02:00:00:00:00:01".into(),
            },
        ];
        let request = CreateVmRequest {
            guest_setup: None,
            start_automatically: false,
            source: VmCreationSource::Template("template".into()),
            name: "VM".into(),
            description: String::new(),
            vcpus_max: 1,
            vcpus_at_startup: 1,
            cores_per_socket: None,
            memory_mib: 1024,
            sr_uuid: "sr".into(),
            disks: Some(Vec::new()),
            interfaces: Some(vifs.clone()),
            media: InstallationMedia::Network,
            pv_args: None,
            copy_bios_from: None,
            new_disk_gib: None,
            network_uuid: None,
            iso_uuid: None,
            affinity_host_uuid: None,
        };
        assert_eq!(request.vif_list(), vifs);
        let mut allowed = vec!["10".into(), "1".into(), "0".into(), "2".into()];
        for (index, _) in request.vif_list().iter().enumerate() {
            assert_eq!(
                crate::operations::next_vif_device(&mut allowed).unwrap(),
                &index.to_string()
            );
            allowed.remove(0);
        }
        assert!(crate::operations::next_vif_device(&mut []).is_none());
        for vif in request.vif_list() {
            validate_creation_mac(&vif.mac).unwrap();
        }
        assert!(validate_creation_mac("01:00:00:00:00:01").is_err());
        assert!(validate_creation_mac("02:00:00:00:00:xx").is_err());
    }
}
