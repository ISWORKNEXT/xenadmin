//! Disk image and OVF/OVA transfers. Rollback only uses UUIDs created by this operation.
use crate::{
    ovf::{self, Disk, Envelope, Machine, error, io},
    xapi::{ApiError, Connection},
    xva::XvaOutcome,
};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::atomic::{AtomicBool, Ordering},
};
use tokio::io::AsyncWriteExt;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Format {
    #[default]
    Xva,
    Ovf,
    Ova,
}
impl Format {
    pub fn extension(self) -> &'static str {
        match self {
            Self::Xva => "xva",
            Self::Ovf => "ovf",
            Self::Ova => "ova",
        }
    }
}
#[derive(Clone)]
pub enum Request {
    Import(Import),
    Export {
        vm_uuids: Vec<String>,
        path: PathBuf,
        format: Format,
        manifest: bool,
    },
}
impl Request {
    pub fn is_import(&self) -> bool {
        matches!(self, Self::Import(_))
    }
    pub fn label(&self) -> String {
        match self {
            Self::Import(r) => r.name.clone(),
            Self::Export { path, .. } => path
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned(),
        }
    }
}
#[derive(Clone)]
pub struct Import {
    pub path: PathBuf,
    pub name: String,
    pub vcpus: u64,
    pub memory_mib: u64,
    pub host_uuid: Option<String>,
    pub sr_uuid: String,
    pub network_uuid: Option<String>,
    pub disk_storage: Vec<String>,
    pub networks: Vec<Option<String>>,
    pub uefi: bool,
    pub start: bool,
    pub verify_manifest: bool,
}
fn check(cancel: &AtomicBool) -> Result<(), ApiError> {
    if cancel.load(Ordering::Relaxed) {
        Err(ApiError::XenCode(
            "TASK_CANCELLED".into(),
            "The transfer was cancelled.".into(),
        ))
    } else {
        Ok(())
    }
}
pub fn source_kind(path: &Path) -> Option<&'static str> {
    match path.extension()?.to_str()?.to_ascii_lowercase().as_str() {
        "xva" => Some("XVA"),
        "ovf" | "ova" => Some("OVF/OVA"),
        "vhd" | "vmdk" | "img" | "raw" => Some("Disk image"),
        _ => None,
    }
}
pub fn inspect(path: &Path) -> Result<Envelope, ApiError> {
    if path
        .extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("ova"))
    {
        let mut archive = tar::Archive::new(std::fs::File::open(path).map_err(io)?);
        for entry in archive.entries().map_err(io)? {
            let mut entry = entry.map_err(io)?;
            if entry
                .path()
                .map_err(io)?
                .extension()
                .is_some_and(|e| e.eq_ignore_ascii_case("ovf"))
            {
                let mut xml = String::new();
                entry.read_to_string(&mut xml).map_err(io)?;
                return ovf::parse(&xml);
            }
        }
        Err(error("The OVA contains no OVF descriptor."))
    } else {
        ovf::parse(&std::fs::read_to_string(path).map_err(io)?)
    }
}
fn unpack(path: &Path, root: &Path, cancel: &AtomicBool) -> Result<PathBuf, ApiError> {
    let mut archive = tar::Archive::new(std::fs::File::open(path).map_err(io)?);
    let mut descriptor = None;
    for entry in archive.entries().map_err(io)? {
        check(cancel)?;
        let mut entry = entry.map_err(io)?;
        let name = entry.path().map_err(io)?.into_owned();
        let target = ovf::relative_file(root, &name.to_string_lossy())?;
        if entry.header().entry_type().is_dir() {
            std::fs::create_dir_all(target).map_err(io)?;
            continue;
        }
        if !entry.header().entry_type().is_file() {
            return Err(error("OVA links and special files are unsupported."));
        }
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent).map_err(io)?;
        }
        let mut file = std::fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&target)
            .map_err(io)?;
        let mut buffer = [0u8; 65536];
        loop {
            check(cancel)?;
            let n = entry.read(&mut buffer).map_err(io)?;
            if n == 0 {
                break;
            }
            file.write_all(&buffer[..n]).map_err(io)?;
        }
        if name
            .extension()
            .is_some_and(|e| e.eq_ignore_ascii_case("ovf"))
        {
            if descriptor.is_some() {
                return Err(error("The OVA has multiple descriptors."));
            }
            descriptor = Some(target);
        }
    }
    descriptor.ok_or_else(|| error("The OVA contains no OVF descriptor."))
}
fn digest(path: &Path, sha1: bool, cancel: &AtomicBool) -> Result<String, ApiError> {
    let mut file = std::fs::File::open(path).map_err(io)?;
    let mut buffer = [0u8; 65536];
    let mut a = Sha256::new();
    let mut b = sha1::Sha1::new();
    loop {
        check(cancel)?;
        let n = file.read(&mut buffer).map_err(io)?;
        if n == 0 {
            break;
        }
        if sha1 {
            b.update(&buffer[..n]);
        } else {
            a.update(&buffer[..n]);
        }
    }
    Ok(if sha1 {
        format!("{:x}", b.finalize())
    } else {
        format!("{:x}", a.finalize())
    })
}
fn verify_manifest(path: &Path, cancel: &AtomicBool) -> Result<(), ApiError> {
    let manifest = path.with_extension("mf");
    if !manifest.exists() {
        return Ok(());
    }
    for line in std::fs::read_to_string(manifest)
        .map_err(io)?
        .lines()
        .filter(|s| !s.trim().is_empty())
    {
        let (algorithm, rest) = line
            .split_once('(')
            .ok_or_else(|| error("Malformed OVF manifest."))?;
        let (name, expected) = rest
            .split_once(")=")
            .or_else(|| rest.split_once(") ="))
            .ok_or_else(|| error("Malformed OVF manifest."))?;
        let sha1 = match algorithm.trim() {
            "SHA1" => true,
            "SHA256" => false,
            _ => return Err(error("Unsupported manifest digest algorithm.")),
        };
        let file = ovf::relative_file(path.parent().unwrap_or(Path::new(".")), name)?;
        if digest(&file, sha1, cancel)? != expected.trim().to_ascii_lowercase() {
            return Err(error(format!("Manifest verification failed for {name}.")));
        }
    }
    Ok(())
}
impl Connection {
    pub(crate) async fn run_appliance(
        &self,
        request: &Request,
        task: &str,
        cancel: &AtomicBool,
    ) -> Result<XvaOutcome, ApiError> {
        if self.settings.read_only {
            return Err(error("This connection is read-only."));
        }
        check(cancel)?;
        match request {
            Request::Import(r) => self.import_appliance(r, task, cancel).await,
            Request::Export {
                vm_uuids,
                path,
                format,
                manifest,
            } => {
                self.export_appliance(vm_uuids, path, *format, *manifest, task, cancel)
                    .await
            }
        }
    }
    async fn progress(&self, task: &str, progress: f64) {
        let _ = self
            .request::<Value>("task.set_progress", json!([self.session, task, progress]))
            .await;
    }
    async fn raw_transfer(
        &self,
        vdi: &str,
        path: &Path,
        upload: Option<&str>,
        cancel: &AtomicBool,
        parent_task: &str,
    ) -> Result<u64, ApiError> {
        let task = self.create_xva_task("Virtual disk transfer").await?;
        let result = {
            let transfer = async {
                let input = if let Some(format) = upload {
                    let file = tokio::fs::File::open(path).await.map_err(io)?;
                    let length = file.metadata().await.map_err(io)?.len();
                    Some((file, length, format))
                } else {
                    None
                };
                let mut response = self
                    .transport
                    .raw_vdi(&self.session, vdi, &task, input)
                    .await
                    .map_err(|_| ApiError::Connection)?;
                if !response.status().is_success() {
                    return Err(ApiError::Http(response.status().as_u16()));
                }
                let mut bytes = 0;
                if upload.is_none() {
                    let mut file = tokio::fs::OpenOptions::new()
                        .create_new(true)
                        .write(true)
                        .open(path)
                        .await
                        .map_err(io)?;
                    while let Some(chunk) =
                        response.chunk().await.map_err(|_| ApiError::Connection)?
                    {
                        check(cancel)?;
                        file.write_all(&chunk).await.map_err(io)?;
                        bytes += chunk.len() as u64;
                    }
                    file.sync_all().await.map_err(io)?;
                }
                self.xva_task_result(&task).await?;
                Ok(bytes)
            };
            tokio::pin!(transfer);
            let mut interval = tokio::time::interval(std::time::Duration::from_millis(200));
            loop {
                tokio::select! {
                    result=&mut transfer=>break result,
                    _=interval.tick()=>{
                        if let Err(error)=check(cancel) {break Err(error);}
                        if let Ok(remote)=self.task_record(&task).await {self.progress(parent_task,remote.progress).await;}
                    }
                }
            }
        };
        if result.is_err() {
            let _ = self.cancel_task(&task).await;
            // Do not destroy a disk until the host has stopped writing it.
            for _ in 0..100 {
                if self
                    .task_record(&task)
                    .await
                    .is_ok_and(|t| t.status.terminal())
                {
                    break;
                }
                tokio::time::sleep(std::time::Duration::from_millis(100)).await;
            }
        }
        if self
            .task_record(&task)
            .await
            .is_ok_and(|t| t.status.terminal())
        {
            self.destroy_task(&task).await;
        }
        result
    }
    async fn import_appliance(
        &self,
        r: &Import,
        task: &str,
        cancel: &AtomicBool,
    ) -> Result<XvaOutcome, ApiError> {
        if r.name.trim().is_empty()
            || r.name.chars().count() > 255
            || r.vcpus == 0
            || r.memory_mib < 16
        {
            return Err(error(
                "Enter a VM name, positive CPU count and at least 16 MiB memory.",
            ));
        }
        let temporary = tempfile::tempdir().map_err(io)?;
        let descriptor = if r
            .path
            .extension()
            .is_some_and(|e| e.eq_ignore_ascii_case("ova"))
        {
            unpack(&r.path, temporary.path(), cancel)?
        } else {
            r.path.clone()
        };
        let image = source_kind(&r.path) == Some("Disk image");
        let mut envelope = if image {
            Envelope {
                machines: vec![Machine {
                    name: r.name.clone(),
                    vcpus: r.vcpus,
                    memory_mib: r.memory_mib,
                    disks: vec![Disk {
                        id: "disk0".into(),
                        file: r
                            .path
                            .file_name()
                            .ok_or_else(|| error("Missing image filename."))?
                            .to_string_lossy()
                            .into_owned(),
                        capacity: 0,
                    }],
                    networks: r.network_uuid.iter().cloned().collect(),
                }],
                eulas: vec![],
            }
        } else {
            if r.verify_manifest {
                verify_manifest(&descriptor, cancel)?;
            }
            inspect(&descriptor)?
        };
        if envelope.machines.len() == 1 {
            envelope.machines[0].name = r.name.clone();
        }
        let root = descriptor.parent().unwrap_or(Path::new("."));
        // Validate and convert all local disks before creating any host objects.
        let mut inputs = Vec::new();
        let mut index = 0;
        for vm in &mut envelope.machines {
            for disk in &mut vm.disks {
                check(cancel)?;
                let mut path = ovf::relative_file(root, &disk.file)?;
                if path
                    .extension()
                    .is_some_and(|e| e.eq_ignore_ascii_case("vmdk"))
                {
                    let converted = temporary.path().join(format!("converted-{index}.raw"));
                    let status = std::process::Command::new("qemu-img")
                        .args(["convert", "-f", "vmdk", "-O", "raw"])
                        .arg(&path)
                        .arg(&converted)
                        .stdout(std::process::Stdio::null())
                        .stderr(std::process::Stdio::null())
                        .status()
                        .map_err(|_| error("VMDK import requires qemu-img on this computer."))?;
                    if !status.success() {
                        return Err(error("VMDK conversion failed."));
                    }
                    path = converted;
                }
                let mut file = std::fs::File::open(&path).map_err(io)?;
                let footer = ovf::vhd_size(&mut file)?;
                if footer.is_none()
                    && path
                        .extension()
                        .is_some_and(|e| e.eq_ignore_ascii_case("vhd"))
                {
                    return Err(error("The VHD footer is missing."));
                }
                let capacity = footer.unwrap_or(file.metadata().map_err(io)?.len());
                if capacity == 0 || (disk.capacity != 0 && capacity > disk.capacity) {
                    return Err(error(
                        "Disk capacity is invalid or exceeds the OVF allocation.",
                    ));
                }
                disk.capacity = disk.capacity.max(capacity);
                inputs.push((path, if footer.is_some() { "vhd" } else { "raw" }));
                index += 1;
            }
        }
        let inventory = self.inventory().await?;
        let template = inventory
            .templates
            .iter()
            .find(|v| v.name == "Other install media")
            .ok_or_else(|| error("The Other install media template is required."))?;
        let template_details = self.vm_details(&template.uuid).await?;
        if template_details.disks.iter().any(|d| !d.is_cd) {
            return Err(error("The import base template must have no disks."));
        }
        for sr in std::iter::once(&r.sr_uuid).chain(&r.disk_storage) {
            if !inventory.storage.iter().any(|s| {
                &s.uuid == sr
                    && s.content_type != "iso"
                    && s.pbd_attached > 0
                    && !matches!(s.storage_type.as_str(), "iso" | "udev")
            }) {
                return Err(error("Select attached storage for every disk."));
            }
        }
        if let Some(host) = &r.host_uuid
            && !inventory.hosts.iter().any(|h| &h.uuid == host && h.enabled)
        {
            return Err(error("Select an enabled home server."));
        }
        for network in r.network_uuid.iter().chain(r.networks.iter().flatten()) {
            if !inventory
                .networks
                .iter()
                .any(|n| &n.uuid == network && n.bridge != "xenapi")
            {
                return Err(error("Select an available guest network."));
            }
        }
        let mut owned = Vec::new();
        let result = async {
            let mut disk_index = 0;
            let mut nic_index = 0;
            for vm in &envelope.machines {
                check(cancel)?;
                let uuid = self
                    .clone_vm_unprovisioned(&template.uuid, &vm.name)
                    .await?;
                owned.push(uuid.clone());
                let reference: String = self
                    .request("VM.get_by_uuid", json!([self.session, uuid]))
                    .await?;
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
                if let Some(host) = &r.host_uuid {
                    let host: String = self
                        .request("host.get_by_uuid", json!([self.session, host]))
                        .await?;
                    self.request::<Value>(
                        "VM.set_affinity",
                        json!([self.session, reference, host]),
                    )
                    .await?;
                }
                self.update_vm_resources(
                    &uuid,
                    &crate::operations::VmResourceUpdate {
                        vcpus_max: vm.vcpus,
                        vcpus_at_startup: vm.vcpus,
                        memory_static_max_mib: vm.memory_mib,
                        memory_dynamic_min_mib: vm.memory_mib,
                        memory_dynamic_max_mib: vm.memory_mib,
                        boot_order: Some("cdn".into()),
                        firmware: Some(if r.uefi { "uefi" } else { "bios" }.into()),
                        secure_boot: Some(false),
                        boot_policy: None,
                        after_crash: None,
                        cpu_weight: None,
                        cores_per_socket: None,
                        pv_args: None,
                        auto_poweron: None,
                    },
                )
                .await?;
                for (i, disk) in vm.disks.iter().enumerate() {
                    check(cancel)?;
                    let sr = r.disk_storage.get(disk_index).unwrap_or(&r.sr_uuid);
                    let name = format!("{} disk {i}", vm.name);
                    self.add_disk(&uuid, sr, disk.capacity, Some(&name)).await?;
                    let details = self.vm_details(&uuid).await?;
                    let disk = details
                        .disks
                        .iter()
                        .find(|d| !d.is_cd && d.vdi_name == name)
                        .ok_or(ApiError::Protocol)?;
                    let vdi: String = self
                        .request("VDI.get_by_uuid", json!([self.session, disk.vdi_uuid]))
                        .await?;
                    self.raw_transfer(
                        &vdi,
                        &inputs[disk_index].0,
                        Some(inputs[disk_index].1),
                        cancel,
                        task,
                    )
                    .await?;
                    if i == 0 {
                        self.set_vbd_bootable(&uuid, &disk.vbd_uuid, true).await?;
                    }
                    disk_index += 1;
                    self.progress(task, disk_index as f64 / (inputs.len() + 1) as f64)
                        .await;
                }
                for vif in self.vm_details(&uuid).await?.interfaces {
                    self.remove_vif(&uuid, &vif.uuid).await?;
                }
                for _ in &vm.networks {
                    if let Some(network) = r.networks.get(nic_index).unwrap_or(&r.network_uuid) {
                        self.add_vif(&uuid, network, None).await?;
                    }
                    nic_index += 1;
                }
            }
            check(cancel)?;
            if r.start {
                for uuid in &owned {
                    let task = self
                        .begin_vm_action(uuid, &crate::operations::VmAction::Start)
                        .await?;
                    self.wait_task(&task).await?;
                }
            }
            Ok(XvaOutcome::Imported {
                uuid: owned.join(", "),
            })
        }
        .await;
        if result.is_err() {
            let mut failures = Vec::new();
            for uuid in &owned {
                if let Err(e) = self.destroy_vm(uuid, true, false).await {
                    failures.push(format!("{uuid}: {e}"));
                }
            }
            if !failures.is_empty() {
                return Err(error(format!(
                    "Import failed; cleanup requires attention: {}",
                    failures.join("; ")
                )));
            }
        }
        result
    }
    async fn export_appliance(
        &self,
        uuids: &[String],
        path: &Path,
        format: Format,
        manifest: bool,
        task: &str,
        cancel: &AtomicBool,
    ) -> Result<XvaOutcome, ApiError> {
        if uuids.is_empty() || format == Format::Xva {
            return Err(error("Select VMs and an OVF or OVA format."));
        }
        if path.exists() {
            return Err(error(
                "The destination already exists. Choose a new filename.",
            ));
        }
        let parent = path
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or(Path::new("."));
        let staging = tempfile::tempdir_in(parent).map_err(io)?;
        let mut envelope = Envelope::default();
        let mut files = Vec::new();
        let mut bytes = 0;
        for (v, uuid) in uuids.iter().enumerate() {
            check(cancel)?;
            let details = self.vm_details(uuid).await?;
            if details.power_state != "Halted" {
                return Err(error("Shut down the selected VMs before exporting."));
            }
            let reference: String = self
                .request("VM.get_by_uuid", json!([self.session, uuid]))
                .await?;
            let record: Value = self
                .request("VM.get_record", json!([self.session, reference]))
                .await?;
            if record["is_control_domain"].as_bool() != Some(false)
                || !record["allowed_operations"]
                    .as_array()
                    .is_some_and(|ops| ops.iter().any(|o| o == "export"))
            {
                return Err(error("This VM does not allow export."));
            }
            let mut vm = Machine {
                name: details.name.clone(),
                vcpus: details.vcpus_at_startup,
                memory_mib: details.memory_static_max / (1024 * 1024),
                disks: Vec::new(),
                networks: details
                    .interfaces
                    .iter()
                    .map(|n| n.network.clone())
                    .collect(),
            };
            for (d, disk) in details.disks.iter().filter(|d| !d.is_cd).enumerate() {
                let id = format!("vm{v}-disk{d}");
                let file = format!(
                    "{}-{id}.vhd",
                    path.file_stem().unwrap_or_default().to_string_lossy()
                );
                let destination = staging.path().join(&file);
                let vdi: String = self
                    .request("VDI.get_by_uuid", json!([self.session, disk.vdi_uuid]))
                    .await?;
                bytes += self
                    .raw_transfer(&vdi, &destination, None, cancel, task)
                    .await?;
                if ovf::vhd_size(&mut std::fs::File::open(&destination).map_err(io)?)?
                    != Some(disk.virtual_size)
                {
                    return Err(error(
                        "The exported VHD capacity differs from the source disk.",
                    ));
                }
                vm.disks.push(Disk {
                    id,
                    file: file.clone(),
                    capacity: disk.virtual_size,
                });
                files.push(file);
            }
            envelope.machines.push(vm);
            self.progress(task, (v + 1) as f64 / (uuids.len() + 1) as f64)
                .await;
        }
        let descriptor = format!(
            "{}.ovf",
            path.file_stem().unwrap_or_default().to_string_lossy()
        );
        std::fs::write(staging.path().join(&descriptor), ovf::write(&envelope)).map_err(io)?;
        files.insert(0, descriptor.clone());
        if manifest {
            let mut text = String::new();
            for file in &files {
                text += &format!(
                    "SHA256({file})= {}\n",
                    digest(&staging.path().join(file), false, cancel)?
                );
            }
            let name = Path::new(&descriptor)
                .with_extension("mf")
                .to_string_lossy()
                .into_owned();
            std::fs::write(staging.path().join(&name), text).map_err(io)?;
            files.push(name);
        }
        check(cancel)?;
        if format == Format::Ova {
            let output = staging.path().join("archive.part");
            let mut archive = tar::Builder::new(std::fs::File::create(&output).map_err(io)?);
            for file in &files {
                check(cancel)?;
                archive
                    .append_path_with_name(staging.path().join(file), file)
                    .map_err(io)?;
            }
            archive.finish().map_err(io)?;
            drop(archive);
            check(cancel)?;
            std::fs::hard_link(output, path).map_err(io)?;
        } else {
            let mut published = Vec::new();
            let result = (|| {
                for file in files
                    .iter()
                    .filter(|f| **f != descriptor)
                    .chain(std::iter::once(&descriptor))
                {
                    check(cancel)?;
                    let target = parent.join(file);
                    std::fs::hard_link(staging.path().join(file), &target).map_err(io)?;
                    published.push(target);
                }
                Ok::<(), ApiError>(())
            })();
            if let Err(e) = result {
                for file in published {
                    let _ = std::fs::remove_file(file);
                }
                return Err(e);
            }
        }
        Ok(XvaOutcome::Exported { bytes })
    }
}

#[cfg(test)]
mod live_tests {
    use super::*;
    use crate::{
        provisioning::{CreateDisk, CreateVif, CreateVmRequest, VmCreationSource},
        transport::TlsOptions,
        xapi::ConnectionSettings,
        xva::XvaRequest,
    };
    #[tokio::test]
    #[ignore = "Requires the explicitly authorized .170 lab and XEN_DEV_* environment"]
    async fn m5_live_round_trip() {
        let settings = ConnectionSettings {
            host: std::env::var("XEN_DEV_HOST").unwrap(),
            username: std::env::var("XEN_DEV_USER").unwrap(),
            label: "M5 qualification".into(),
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
        let before_vm = c.records("VM").await.unwrap();
        let before_vdi = c.records("VDI").await.unwrap();
        assert!(!before_vm.values().any(|v| {
            v["name_label"]
                .as_str()
                .is_some_and(|n| n.starts_with("zz-parity-M5-"))
        }));
        let temp = tempfile::tempdir().unwrap();
        let run=async {
            let inventory=c.inventory().await?;
            let template=inventory.templates.iter().find(|v|v.name=="Other install media").unwrap();
            let sr=inventory.storage.iter().find(|s|s.name=="Local storage").unwrap();
            let network=inventory.networks.iter().find(|n|n.bridge!="xenapi").unwrap();
            let host=inventory.hosts.iter().find(|h|h.enabled).unwrap();
            let source=c.create_configured_vm(&CreateVmRequest{guest_setup:None,start_automatically:false,source:VmCreationSource::Template(template.uuid.clone()),name:"zz-parity-M5-src".into(),description:"M5 temporary source".into(),vcpus_max:1,vcpus_at_startup:1,cores_per_socket:None,memory_mib:256,sr_uuid:sr.uuid.clone(),disks:Some(vec![CreateDisk{source_device:None,name:"zz-parity-M5-src disk".into(),size_bytes:1073741824,sr_uuid:sr.uuid.clone()}]),interfaces:Some(vec![CreateVif{network_uuid:network.uuid.clone(),mac:String::new()}]),media:Default::default(),pv_args:None,copy_bios_from:None,new_disk_gib:None,network_uuid:None,iso_uuid:None,affinity_host_uuid:Some(host.uuid.clone())}).await?;
            let xva=temp.path().join("source.xva");let ova=temp.path().join("source.ova");
            println!("M5 source created: {source}");
            c.transfer_xva(&XvaRequest::Export{vm_uuid:source.clone(),path:xva.clone()}).await?;
            println!("M5 XVA export complete");
            c.transfer_xva(&XvaRequest::Appliance(Box::new(Request::Export{vm_uuids:vec![source.clone()],path:ova.clone(),format:Format::Ova,manifest:true}))).await?;
            println!("M5 OVA export complete");
            if c.transfer_xva(&XvaRequest::Appliance(Box::new(Request::Export{vm_uuids:vec![source],path:ova.clone(),format:Format::Ova,manifest:true}))).await.is_ok() {return Err(error("Existing export path was overwritten."));}
            let base=Import{path:xva.clone(),name:"zz-parity-M5-xva".into(),vcpus:1,memory_mib:256,host_uuid:Some(host.uuid.clone()),sr_uuid:sr.uuid.clone(),network_uuid:Some(network.uuid.clone()),disk_storage:vec![],networks:vec![Some(network.uuid.clone())],uefi:false,start:false,verify_manifest:true};
            for (name,path,size) in [("xva",xva,1073741824),("ova",ova,1073741824),("img",temp.path().join("image.raw"),67108864)] {
                if name=="img" {let status=std::process::Command::new("truncate").args(["-s","64M"]).arg(&path).status().map_err(io)?;assert!(status.success());}
                let mut import=base.clone();import.path=path;import.name=format!("zz-parity-M5-{name}");
                let request=if name=="xva" {XvaRequest::ConfiguredImport(import)} else {XvaRequest::Appliance(Box::new(Request::Import(import)))};
                let outcome=c.transfer_xva(&request).await?;
                let XvaOutcome::Imported{uuid}=outcome else {panic!("Expected import");};
                let details=c.vm_details(&uuid).await?;let sizes:Vec<_>=details.disks.iter().filter(|d|!d.is_cd).map(|d|d.virtual_size).collect();
                println!("{}",json!({"name":details.name,"uuid":uuid,"disk_sizes":sizes,"vif_count":details.interfaces.len()}));
                if sizes!=vec![size] || details.interfaces.len()!=1 {return Err(error("Imported disk size or VIF count mismatch."));}
            }
            let mut import=base;import.name="zz-parity-M5-cancel".into();import.path=temp.path().join("cancel.raw");std::fs::File::create(&import.path).map_err(io)?.set_len(1073741824).map_err(io)?;
            let cancel=AtomicBool::new(false);let task=c.create_xva_task("M5 cancellation test").await?;
            let request=XvaRequest::Appliance(Box::new(Request::Import(import)));
            let transfer=c.run_xva_transfer(&request,&task,&cancel);tokio::pin!(transfer);
            let mut tick=tokio::time::interval(std::time::Duration::from_millis(50));let mut cancelled=false;
            let result=loop {tokio::select! {result=&mut transfer=>break result,_=tick.tick()=>{
                if !cancelled && c.records("task").await?.values().any(|t|t["name_label"]=="Virtual disk transfer" && t["status"]=="pending" && t["progress"].as_f64().is_some_and(|p|p>0.0 && p<1.0)) {cancel.store(true,Ordering::Relaxed);cancelled=true;}
            }}};
            if !cancelled || result.is_ok() {return Err(error("Import was not cancelled at the transfer checkpoint."));}
            if c.records("VM").await?.values().any(|v|v["name_label"]=="zz-parity-M5-cancel") {return Err(error("Partial cancelled VM remains."));}
            if c.records("VDI").await?.values().any(|v|v["name_label"].as_str().is_some_and(|n|n.starts_with("zz-parity-M5-cancel"))) {return Err(error("Partial cancelled disk remains."));}
            println!("M5 cancelled mid disk transfer; no partial VM or VDI remains");
            Ok::<(),ApiError>(())
        }.await;
        let mut cleanup_errors = Vec::new();
        for (reference, vm) in c.records("VM").await.unwrap() {
            if !before_vm.contains_key(&reference)
                && vm["name_label"]
                    .as_str()
                    .is_some_and(|n| n.starts_with("zz-parity-M5-"))
                && let Err(e) = c
                    .destroy_vm(vm["uuid"].as_str().unwrap(), true, false)
                    .await
            {
                cleanup_errors.push(e.to_string());
            }
        }
        for (reference, vdi) in c.records("VDI").await.unwrap() {
            if !before_vdi.contains_key(&reference)
                && vdi["name_label"]
                    .as_str()
                    .is_some_and(|n| n.starts_with("zz-parity-M5-"))
                && vdi["VBDs"].as_array().is_some_and(Vec::is_empty)
                && let Err(e) = c
                    .request::<Value>("VDI.destroy", json!([c.session, reference]))
                    .await
            {
                cleanup_errors.push(e.to_string());
            }
        }
        let after_vm = c.records("VM").await.unwrap();
        let after_vdi = c.records("VDI").await.unwrap();
        assert!(before_vm.keys().all(|id| after_vm.contains_key(id)));
        assert!(before_vdi.keys().all(|id| after_vdi.contains_key(id)));
        assert!(!after_vm.values().any(|v| {
            v["name_label"]
                .as_str()
                .is_some_and(|n| n.starts_with("zz-parity-M5-"))
        }));
        assert!(
            cleanup_errors.is_empty(),
            "Cleanup failed: {cleanup_errors:?}"
        );
        c.logout().await.unwrap();
        println!("M5 cleanup and pre-existing VM/VDI preservation confirmed");
        run.unwrap();
    }
}
