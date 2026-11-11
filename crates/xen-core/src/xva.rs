//! Streaming XVA transfers. Authenticated URLs never leave the transport.
use std::{
    collections::HashSet,
    path::{Path, PathBuf},
    sync::atomic::{AtomicBool, Ordering},
    time::Duration,
};

use quick_xml::{Reader, events::Event};
use serde::Serialize;
use serde_json::{Value, json};
use tokio::io::{AsyncReadExt, AsyncSeekExt, AsyncWriteExt};

use crate::{
    operations::TaskStatus,
    xapi::{ApiError, Connection},
};

#[derive(Clone)]
pub enum XvaRequest {
    Export {
        vm_uuid: String,
        path: PathBuf,
    },
    Import {
        path: PathBuf,
        sr_uuid: String,
        name: String,
        network_uuid: Option<String>,
    },
}

#[derive(Debug, Serialize)]
pub enum XvaOutcome {
    Exported { bytes: u64 },
    Imported { uuid: String },
}

struct PartialExport(PathBuf);
impl Drop for PartialExport {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

fn io_error(error: std::io::Error) -> ApiError {
    ApiError::Xen(format!("Local file operation failed: {error}"))
}

fn cancelled() -> ApiError {
    ApiError::XenCode(
        "TASK_CANCELLED".into(),
        "The transfer was cancelled.".into(),
    )
}

async fn local_cancellation(cancel: &AtomicBool) -> ApiError {
    while !cancel.load(Ordering::Relaxed) {
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    cancelled()
}

fn result_refs(value: &Value) -> Result<Vec<String>, ApiError> {
    let text = value.as_str().ok_or(ApiError::Protocol)?;
    let mut refs = Vec::new();
    let mut add = |value: &str| {
        let value = value.trim();
        if value.starts_with("OpaqueRef:")
            && value != "OpaqueRef:NULL"
            && !value
                .chars()
                .any(|c| c.is_whitespace() || c == '<' || c == '>')
            && !refs.iter().any(|r| r == value)
        {
            refs.push(value.to_owned());
        }
    };
    if !text.starts_with('<') {
        add(text);
    } else {
        let mut reader = Reader::from_str(text);
        loop {
            match reader.read_event().map_err(|_| ApiError::Protocol)? {
                Event::Text(value) => add(&value.decode().map_err(|_| ApiError::Protocol)?),
                Event::Eof => break,
                _ => {}
            }
        }
    }
    if refs.is_empty() {
        return Err(ApiError::Protocol);
    }
    Ok(refs)
}

async fn open_xva(path: &Path) -> Result<(tokio::fs::File, u64), ApiError> {
    let mut file = tokio::fs::File::open(path).await.map_err(io_error)?;
    let metadata = file.metadata().await.map_err(io_error)?;
    if !metadata.is_file() || metadata.len() < 1024 {
        return Err(ApiError::Xen("Select a complete XVA archive file.".into()));
    }
    let invalid = || {
        ApiError::Xen(
            "The XVA archive is incomplete or has invalid tar headers. Select a complete XVA file."
                .into(),
        )
    };
    let mut offset = 0_u64;
    loop {
        if offset
            .checked_add(512)
            .is_none_or(|end| end > metadata.len())
        {
            return Err(invalid());
        }
        let mut header = [0_u8; 512];
        file.read_exact(&mut header).await.map_err(io_error)?;
        if header.iter().all(|byte| *byte == 0) {
            let mut footer = [0_u8; 512];
            file.read_exact(&mut footer).await.map_err(|_| invalid())?;
            if offset == 0 || footer.iter().any(|byte| *byte != 0) {
                return Err(invalid());
            }
            break;
        }
        let name = std::str::from_utf8(&header[..100])
            .unwrap_or_default()
            .trim_end_matches('\0');
        if offset == 0 && !matches!(name, "ova.xml" | "./ova.xml") {
            return Err(ApiError::Xen(
                "The archive is not an XVA file. OVF and OVA require a separate import workflow."
                    .into(),
            ));
        }
        let octal = |bytes: &[u8]| {
            u64::from_str_radix(
                std::str::from_utf8(bytes)
                    .unwrap_or_default()
                    .trim_matches(['\0', ' ']),
                8,
            )
            .map_err(|_| invalid())
        };
        let expected = octal(&header[148..156])?;
        let actual: u64 = header
            .iter()
            .enumerate()
            .map(|(i, b)| {
                if (148..156).contains(&i) {
                    32
                } else {
                    u64::from(*b)
                }
            })
            .sum();
        if expected != actual {
            return Err(invalid());
        }
        let size = octal(&header[124..136])?;
        let padded = size
            .checked_add(511)
            .and_then(|n| (n / 512).checked_mul(512))
            .ok_or_else(invalid)?;
        offset = offset
            .checked_add(512)
            .and_then(|n| n.checked_add(padded))
            .filter(|end| *end <= metadata.len())
            .ok_or_else(invalid)?;
        file.seek(std::io::SeekFrom::Start(offset))
            .await
            .map_err(io_error)?;
    }
    file.rewind().await.map_err(io_error)?;
    Ok((file, metadata.len()))
}

impl Connection {
    pub(crate) async fn create_xva_task(&self, name: &str) -> Result<String, ApiError> {
        self.request(
            "task.create",
            json!([self.session, name, "XVA file transfer"]),
        )
        .await
    }

    pub async fn transfer_xva(&self, request: &XvaRequest) -> Result<XvaOutcome, ApiError> {
        if self.settings.read_only {
            return Err(ApiError::Xen("This connection is read-only.".into()));
        }
        let task = self.create_xva_task("XVA transfer").await?;
        self.run_xva_transfer(request, &task, &AtomicBool::new(false))
            .await
    }

    pub(crate) async fn run_xva_transfer(
        &self,
        request: &XvaRequest,
        task: &str,
        cancel: &AtomicBool,
    ) -> Result<XvaOutcome, ApiError> {
        let mut submitted = false;
        let result = if self.settings.read_only {
            Err(ApiError::Xen("This connection is read-only.".into()))
        } else {
            match request {
                XvaRequest::Export { vm_uuid, path } => {
                    self.export_xva(vm_uuid, path, task, &mut submitted, cancel)
                        .await
                }
                XvaRequest::Import {
                    path,
                    sr_uuid,
                    name,
                    network_uuid,
                } => {
                    self.import_xva(
                        path,
                        sr_uuid,
                        name,
                        network_uuid.as_deref(),
                        task,
                        &mut submitted,
                        cancel,
                    )
                    .await
                }
            }
        };
        if matches!(&result,Err(ApiError::XenCode(code,_)) if code=="TASK_CANCELLED") && submitted {
            self.settle_cancelled_xva(task).await;
        }
        if !submitted {
            // The client created this task, but no HTTP operation has started.
            // XAPI can reject destroying a still-pending client task.
            let _ = self
                .request::<Value>(
                    "task.set_error_info",
                    json!([self.session, task, ["CLIENT_TRANSFER_REJECTED"]]),
                )
                .await;
            let _ = self
                .request::<Value>("task.set_status", json!([self.session, task, "failure"]))
                .await;
        }
        if !submitted
            || self
                .task_record(task)
                .await
                .is_ok_and(|r| r.status.terminal())
        {
            self.destroy_task(task).await;
        }
        result
    }

    async fn settle_cancelled_xva(&self, task: &str) -> bool {
        let _ = self.cancel_task(task).await;
        for _ in 0..30 {
            if let Ok(Ok(record)) =
                tokio::time::timeout(Duration::from_secs(2), self.task_record(task)).await
                && record.status.terminal()
            {
                return true;
            }
            tokio::time::sleep(Duration::from_millis(200)).await;
        }
        false
    }

    async fn xva_task_result(&self, task: &str) -> Result<Value, ApiError> {
        for _ in 0..86400 {
            let record = self.task_record(task).await?;
            match record.status {
                TaskStatus::Succeeded => {
                    let value: Value = self
                        .request("task.get_record", json!([self.session, task]))
                        .await?;
                    return Ok(value["result"].clone());
                }
                TaskStatus::Cancelled => return Err(cancelled()),
                TaskStatus::Failed => {
                    if record.message == "TASK_CANCELLED" {
                        return Err(cancelled());
                    }
                    return Err(ApiError::Xen(format!(
                        "The host rejected the transfer: {}.",
                        record.message
                    )));
                }
                _ => tokio::time::sleep(Duration::from_secs(1)).await,
            }
        }
        Err(ApiError::Xen(
            "The transfer is still pending on the host. Check its tasks before retrying.".into(),
        ))
    }

    async fn xva_failure(&self, task: &str) -> ApiError {
        loop {
            if let Ok(record) = self.task_record(task).await {
                match record.status {
                    TaskStatus::Cancelled => return cancelled(),
                    TaskStatus::Failed => {
                        if record.message == "TASK_CANCELLED" {
                            return cancelled();
                        }
                        return ApiError::Xen(format!(
                            "The host rejected the transfer: {}.",
                            record.message
                        ));
                    }
                    _ => {}
                }
            }
            tokio::time::sleep(Duration::from_secs(1)).await;
        }
    }

    async fn export_xva(
        &self,
        uuid: &str,
        path: &Path,
        task: &str,
        submitted: &mut bool,
        cancel: &AtomicBool,
    ) -> Result<XvaOutcome, ApiError> {
        let reference: String = self
            .request("VM.get_by_uuid", json!([self.session, uuid]))
            .await?;
        let record: Value = self
            .request("VM.get_record", json!([self.session, reference]))
            .await?;
        if record["uuid"].as_str() != Some(uuid)
            || record["is_control_domain"].as_bool() != Some(false)
            || record["power_state"].as_str() != Some("Halted")
            || !record["allowed_operations"]
                .as_array()
                .is_some_and(|ops| ops.iter().any(|op| op.as_str() == Some("export")))
        {
            return Err(ApiError::Xen("Export requires an available halted VM, template or snapshot with the export operation allowed.".into()));
        }
        if tokio::fs::try_exists(path).await.map_err(io_error)? {
            return Err(ApiError::Xen(
                "The destination already exists. Choose a new file name.".into(),
            ));
        }
        let parent = path
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or_else(|| Path::new("."));
        let partial =
            PartialExport(parent.join(format!(".xen-export-{}.part", uuid::Uuid::now_v7())));
        let mut options = tokio::fs::OpenOptions::new();
        options.create_new(true).write(true);
        #[cfg(unix)]
        options.mode(0o600);
        let mut file = options.open(&partial.0).await.map_err(io_error)?;
        *submitted = true;
        let transfer = async {
            let mut response = self
                .transport
                .export_xva(&self.session, uuid, task)
                .await
                .map_err(|_| ApiError::Connection)?;
            if !response.status().is_success() {
                return Err(ApiError::Http(response.status().as_u16()));
            }
            let expected = response.content_length();
            let mut bytes = 0_u64;
            while let Some(chunk) = response.chunk().await.map_err(|_| ApiError::Connection)? {
                file.write_all(&chunk).await.map_err(io_error)?;
                bytes += chunk.len() as u64;
            }
            if bytes < 1024 || expected.is_some_and(|n| bytes != n) {
                return Err(ApiError::Xen("The export download is incomplete.".into()));
            }
            self.xva_task_result(task).await?;
            file.sync_all().await.map_err(io_error)?;
            drop(file);
            open_xva(&partial.0).await?;
            Ok(bytes)
        };
        let bytes = tokio::select! {
            result = transfer => result,
            error = self.xva_failure(task) => Err(error),
            error = local_cancellation(cancel) => Err(error),
        }?;
        if cancel.load(Ordering::Relaxed) {
            return Err(cancelled());
        }
        // Publishing is the commit step after the cancellable download.
        tokio::fs::hard_link(&partial.0, path)
            .await
            .map_err(io_error)?;
        Ok(XvaOutcome::Exported { bytes })
    }

    #[allow(clippy::too_many_arguments)]
    async fn import_xva(
        &self,
        path: &Path,
        sr_uuid: &str,
        name: &str,
        network: Option<&str>,
        task: &str,
        submitted: &mut bool,
        cancel: &AtomicBool,
    ) -> Result<XvaOutcome, ApiError> {
        if name.trim().is_empty() || name.chars().count() > 255 {
            return Err(ApiError::Xen(
                "Enter a VM name between 1 and 255 characters.".into(),
            ));
        }
        let (file, length) = open_xva(path).await?;
        let inventory = self.inventory().await?;
        if !inventory.storage.iter().any(|sr| {
            sr.uuid == sr_uuid
                && sr.content_type != "iso"
                && !matches!(sr.storage_type.as_str(), "udev" | "iso")
                && sr.pbd_attached > 0
        }) {
            return Err(ApiError::Xen(
                "Choose an attached storage repository for VM disks.".into(),
            ));
        }
        if let Some(uuid) = network
            && !inventory
                .networks
                .iter()
                .any(|n| n.uuid == uuid && n.bridge != "xenapi")
        {
            return Err(ApiError::Xen("Choose an available guest network.".into()));
        }
        let sr: String = self
            .request("SR.get_by_uuid", json!([self.session, sr_uuid]))
            .await?;
        let (vms, vdis, networks) = tokio::try_join!(
            self.records("VM"),
            self.records("VDI"),
            self.records("network")
        )?;
        let before_vms: HashSet<String> = vms.into_keys().collect();
        let before_vdis: HashSet<String> = vdis.into_keys().collect();
        let before_networks: HashSet<String> = networks.into_keys().collect();
        *submitted = true;
        let transfer = async {
            let response = self
                .transport
                .import_xva(&self.session, &sr, task, file, length)
                .await
                .map_err(|_| ApiError::Connection)?;
            if !response.status().is_success() {
                return Err(ApiError::Http(response.status().as_u16()));
            }
            let result = self.xva_task_result(task).await?;
            let references = result_refs(&result)?;
            if references.len() != 1 {
                return Err(ApiError::Xen("This XVA workflow expects one VM or template. The imported objects will be removed.".into()));
            }
            let reference = &references[0];
            let vm: Value = self
                .request("VM.get_record", json!([self.session, reference]))
                .await?;
            if before_vms.contains(reference)
                || vm["other_config"]["import_task"].as_str() != Some(task)
                || vm["is_control_domain"].as_bool() != Some(false)
                || vm["power_state"].as_str() != Some("Halted")
            {
                return Err(ApiError::Xen(
                    "The imported object could not be verified as a new halted VM or template."
                        .into(),
                ));
            }
            let uuid = vm["uuid"].as_str().ok_or(ApiError::Protocol)?.to_owned();
            self.request::<Value>(
                "VM.set_name_label",
                json!([self.session, reference, name.trim()]),
            )
            .await?;
            let details = self.vm_details(&uuid).await?;
            for vif in &details.interfaces {
                self.remove_vif(&uuid, &vif.uuid).await?;
            }
            if let Some(network) = network {
                self.add_imported_vif(&uuid, network).await?;
            }
            for disk in details.disks.iter().filter(|d| !d.is_cd) {
                let vdi: String = self
                    .request("VDI.get_by_uuid", json!([self.session, disk.vdi_uuid]))
                    .await?;
                if before_vdis.contains(&vdi) {
                    return Err(ApiError::Xen(
                        "The import unexpectedly references a pre-existing disk.".into(),
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
            self.cleanup_import_networks(task, &before_networks).await?;
            Ok(XvaOutcome::Imported { uuid })
        };
        let result = tokio::select! {
            result = transfer => result,
            error = self.xva_failure(task) => Err(error),
            error = local_cancellation(cancel) => Err(error),
        };
        if let Err(error) = result {
            if matches!(&error,ApiError::XenCode(code,_) if code=="TASK_CANCELLED") {
                self.settle_cancelled_xva(task).await;
            }
            if self
                .task_record(task)
                .await
                .is_ok_and(|r| r.status.terminal())
            {
                return match self
                    .cleanup_import(task, &before_vms, &before_vdis, &before_networks)
                    .await
                {
                    Ok(()) => Err(error),
                    Err(cleanup) => Err(ApiError::Xen(format!(
                        "Import failed: {error}. Cleanup also failed: {cleanup}. Inspect the imported objects before retrying."
                    ))),
                };
            }
            let _ = self.cancel_task(task).await;
            return Err(ApiError::Xen(format!(
                "Import interrupted: {error}. Its outcome is not confirmed. Check host tasks and inventory before importing again."
            )));
        }
        result
    }

    async fn cleanup_import_networks(
        &self,
        task: &str,
        before_networks: &HashSet<String>,
    ) -> Result<(), ApiError> {
        for (reference, network) in self.records("network").await? {
            if !before_networks.contains(&reference)
                && network["other_config"]["import_task"].as_str() == Some(task)
                && network["PIFs"].as_array().is_some_and(Vec::is_empty)
                && network["VIFs"].as_array().is_some_and(Vec::is_empty)
            {
                self.request::<Value>("network.destroy", json!([self.session, reference]))
                    .await?;
            }
        }
        Ok(())
    }

    async fn cleanup_import(
        &self,
        task: &str,
        before_vms: &HashSet<String>,
        before_vdis: &HashSet<String>,
        before_networks: &HashSet<String>,
    ) -> Result<(), ApiError> {
        let records = self.records("VM").await?;
        let owned: HashSet<String> = records
            .iter()
            .filter(|(r, v)| {
                !before_vms.contains(*r)
                    && v["other_config"]["import_task"].as_str() == Some(task)
                    && v["is_control_domain"].as_bool() == Some(false)
            })
            .map(|(r, _)| r.clone())
            .collect();
        let mut disks: HashSet<String> = self
            .records("VBD")
            .await?
            .values()
            .filter(|v| {
                v["VM"].as_str().is_some_and(|r| owned.contains(r))
                    && v["type"].as_str() == Some("Disk")
            })
            .filter_map(|v| v["VDI"].as_str().map(str::to_owned))
            .filter(|r| r != "OpaqueRef:NULL" && !before_vdis.contains(r))
            .collect();
        for (r, v) in self.records("VDI").await? {
            if !before_vdis.contains(&r) && v["other_config"]["import_task"].as_str() == Some(task)
            {
                disks.insert(r);
            }
        }
        let mut failure = None;
        for reference in &owned {
            if let Err(error) = self
                .async_task("Async.VM.destroy", json!([self.session, reference]))
                .await
            {
                failure.get_or_insert(error);
            }
        }
        for reference in disks {
            let vdi: Value = self
                .request("VDI.get_record", json!([self.session, reference]))
                .await?;
            if !vdi["VBDs"].as_array().is_some_and(Vec::is_empty) {
                failure.get_or_insert(ApiError::Xen("An imported disk is still attached.".into()));
                continue;
            }
            if let Err(error) = self
                .request::<Value>("VDI.destroy", json!([self.session, reference]))
                .await
            {
                failure.get_or_insert(error);
            }
        }
        if let Err(error) = self.cleanup_import_networks(task, before_networks).await {
            failure.get_or_insert(error);
        }
        match failure {
            Some(error) => Err(error),
            None => Ok(()),
        }
    }
}
