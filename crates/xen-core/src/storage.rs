//! PBD lifecycle and SR probe helpers.
use serde::Serialize;
use serde_json::{Value, json};

use crate::operations::wait_task_value;
use crate::xapi::{ApiError, Connection};

#[derive(Clone, Debug, Serialize)]
pub struct PbdSummary {
    pub uuid: String,
    pub host_uuid: String,
    pub host_name: String,
    pub attached: bool,
    pub location: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct ProbedSr {
    pub uuid: String,
    pub name: String,
    pub description: String,
    pub shared: bool,
    pub sr_type: String,
    pub content_type: String,
    pub physical_size: u64,
    pub physical_utilisation: u64,
}

#[derive(Clone, Debug, Serialize)]
pub struct SrProbe {
    pub raw: String,
    pub found: Vec<ProbedSr>,
}

impl Connection {
    pub async fn pbd_catalog(&self, sr_uuid: &str) -> Result<Vec<PbdSummary>, ApiError> {
        let sr_ref: String = self
            .request("SR.get_by_uuid", json!([self.session, sr_uuid]))
            .await?;
        let (pbds, hosts) = tokio::join!(self.records("PBD"), self.records("host"));
        let (pbds, hosts) = (pbds?, hosts?);
        let mut catalog: Vec<PbdSummary> = pbds
            .values()
            .filter(|pbd| pbd["SR"].as_str() == Some(sr_ref.as_str()))
            .map(|pbd| {
                let host_ref = text(pbd, "host");
                let host = hosts.get(&host_ref);
                PbdSummary {
                    uuid: text(pbd, "uuid"),
                    host_uuid: text(host.unwrap_or(&Value::Null), "uuid"),
                    host_name: text(host.unwrap_or(&Value::Null), "name_label"),
                    attached: pbd["currently_attached"].as_bool().unwrap_or(false),
                    location: pbd["device_config"]
                        .as_object()
                        .and_then(|config| {
                            config
                                .get("location")
                                .or_else(|| config.get("device"))
                                .or_else(|| config.get("target"))
                                .and_then(Value::as_str)
                        })
                        .unwrap_or_default()
                        .to_owned(),
                }
            })
            .collect();
        catalog.sort_by(|left, right| left.host_name.cmp(&right.host_name));
        Ok(catalog)
    }

    pub async fn set_pbd_plugged(&self, pbd_uuid: &str, plugged: bool) -> Result<(), ApiError> {
        if self.settings.read_only {
            return Err(ApiError::Xen(
                "This connection is read-only. Changes are blocked by the Rust backend.".into(),
            ));
        }
        let pbd_ref: String = self
            .request("PBD.get_by_uuid", json!([self.session, pbd_uuid]))
            .await?;
        let pbd: Value = self
            .request("PBD.get_record", json!([self.session, pbd_ref]))
            .await?;
        if pbd["uuid"].as_str() != Some(pbd_uuid) {
            return Err(ApiError::Protocol);
        }
        if pbd["currently_attached"].as_bool().unwrap_or(false) == plugged {
            return Ok(());
        }
        if !plugged {
            let sr_ref = text(&pbd, "SR");
            let (vbds, vdis, vms) =
                tokio::join!(self.records("VBD"), self.records("VDI"), self.records("VM"));
            let (vbds, vdis, vms) = (vbds?, vdis?, vms?);
            if vbds.values().any(|vbd| {
                let vdi_ref = text(vbd, "VDI");
                vdis.get(&vdi_ref)
                    .is_some_and(|vdi| text(vdi, "SR") == sr_ref)
                    && vms
                        .get(&text(vbd, "VM"))
                        .is_some_and(|vm| vm["power_state"].as_str() == Some("Running"))
            }) {
                return Err(ApiError::Xen(
                    "A running VM is using this storage. Shut it down before unplugging the PBD."
                        .into(),
                ));
            }
        }
        let method = if plugged {
            "Async.PBD.plug"
        } else {
            "Async.PBD.unplug"
        };
        self.async_task(method, json!([self.session, pbd_ref]))
            .await
    }

    /// Plug every detached PBD belonging to this SR and return the number repaired.
    pub async fn repair_sr(&self, sr_uuid: &str) -> Result<usize, ApiError> {
        if self.settings.read_only {
            return Err(ApiError::Xen(
                "This connection is read-only. Changes are blocked by the Rust backend.".into(),
            ));
        }
        let sr_ref: String = self
            .request("SR.get_by_uuid", json!([self.session, sr_uuid]))
            .await?;
        let pbds = self.records("PBD").await?;
        let mut repaired = 0usize;
        let mut found = 0usize;
        for (pbd_ref, pbd) in pbds
            .iter()
            .filter(|(_, pbd)| pbd["SR"].as_str() == Some(sr_ref.as_str()))
        {
            found += 1;
            if pbd["currently_attached"].as_bool() != Some(true) {
                self.async_task("Async.PBD.plug", json!([self.session, pbd_ref]))
                    .await?;
                repaired += 1;
            }
        }
        if found == 0 {
            return Err(ApiError::Xen(
                "This storage repository has no PBDs; reattach it with a host path first.".into(),
            ));
        }
        Ok(repaired)
    }

    /// Probe a host-local file path for an existing SR.
    pub async fn probe_sr(&self, host_uuid: &str, location: &str) -> Result<SrProbe, ApiError> {
        if location.trim().is_empty() || !location.trim().starts_with('/') {
            return Err(ApiError::Xen(
                "Enter an absolute host path such as /var/tmp/xen-storage.".into(),
            ));
        }
        let host_ref: String = self
            .request("host.get_by_uuid", json!([self.session, host_uuid]))
            .await?;
        let task: String = self
            .request(
                "Async.SR.probe",
                json!([self.session, host_ref, {"location": location}, "file", {}]),
            )
            .await?;
        let raw = wait_task_value(self, &task).await?;
        let raw = raw.as_str().unwrap_or_default().to_owned();
        Ok(SrProbe {
            found: parse_probe_xml(&raw),
            raw,
        })
    }
}

fn parse_probe_xml(xml: &str) -> Vec<ProbedSr> {
    let mut results = Vec::new();
    for block in xml.split("<SR>").skip(1) {
        let block = block.split("</SR>").next().unwrap_or_default();
        let uuid = tag(block, "UUID");
        if uuid.is_empty() {
            continue;
        }
        results.push(ProbedSr {
            uuid,
            name: tag(block, "Name"),
            description: tag(block, "Description"),
            shared: tag(block, "IsShared").eq_ignore_ascii_case("true"),
            sr_type: tag(block, "Type"),
            content_type: tag(block, "ContentType"),
            physical_size: tag(block, "PhysicalSize").parse().unwrap_or(0),
            physical_utilisation: tag(block, "PhysicalUtilisation").parse().unwrap_or(0),
        });
    }
    results
}

fn tag(block: &str, name: &str) -> String {
    let open = format!("<{name}>");
    let close = format!("</{name}>");
    block
        .split(&open)
        .nth(1)
        .and_then(|rest| rest.split(&close).next())
        .unwrap_or_default()
        .trim()
        .to_owned()
}

fn text(record: &Value, key: &str) -> String {
    record[key].as_str().unwrap_or_default().to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_probe_xml_records() {
        let xml = "<SRlist><SR><UUID>abc</UUID><Name>storage</Name><Description>test</Description><IsShared>false</IsShared><Type>file</Type><ContentType>user</ContentType><PhysicalSize>100</PhysicalSize><PhysicalUtilisation>20</PhysicalUtilisation></SR></SRlist>";
        let found = parse_probe_xml(xml);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].uuid, "abc");
        assert_eq!(found[0].name, "storage");
        assert!(!found[0].shared);
        assert_eq!(found[0].physical_size, 100);
        assert!(parse_probe_xml("<SRlist/>").is_empty());
    }
}
