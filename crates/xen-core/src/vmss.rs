//! VM snapshot schedule (VMSS) write actions.
use std::collections::{HashMap, HashSet};

use serde_json::{Value, json};

use crate::operations::wait_task_value;
use crate::xapi::{ApiError, Connection};

const MAX_RETENTION: u64 = 1000;
const SNAPSHOT_TYPES: [&str; 2] = ["snapshot", "checkpoint"];
const FREQUENCIES: [&str; 3] = ["hourly", "daily", "weekly"];
const DAYS: [&str; 7] = [
    "monday",
    "tuesday",
    "wednesday",
    "thursday",
    "friday",
    "saturday",
    "sunday",
];

#[derive(Clone, Debug)]
pub struct VmssSchedule {
    pub frequency: String,
    pub minute: u8,
    pub hour: u8,
    pub days: Vec<String>,
}

impl VmssSchedule {
    pub fn summary(&self) -> String {
        match self.frequency.as_str() {
            "hourly" => format!("Every hour at minute {}", self.minute),
            "daily" => format!("Every day at {:02}:{:02}", self.hour, self.minute),
            _ => format!(
                "Weekly on {} at {:02}:{:02}",
                self.days.join(", "),
                self.hour,
                self.minute
            ),
        }
    }
}

impl Connection {
    #[allow(clippy::too_many_arguments)]
    pub async fn create_vmss(
        &self,
        name: &str,
        description: &str,
        snapshot_type: &str,
        schedule: &VmssSchedule,
        retained_snapshots: u64,
        enabled: bool,
        vm_uuids: Vec<String>,
    ) -> Result<(), ApiError> {
        if self.settings.read_only {
            return Err(ApiError::Xen(
                "This connection is read-only. Changes are blocked by the Rust backend.".into(),
            ));
        }
        validate_vmss_fields(
            name,
            description,
            snapshot_type,
            schedule,
            retained_snapshots,
        )?;
        let vm_refs = self.resolve_vmss_targets(&vm_uuids).await?;
        let record = json!({
            "uuid": uuid::Uuid::now_v7().to_string(),
            "name_label": name.trim(),
            "name_description": description,
            "enabled": enabled && !vm_refs.is_empty(),
            "type": snapshot_type,
            "retained_snapshots": retained_snapshots.to_string(),
            "frequency": schedule.frequency,
            "schedule": schedule_map(schedule),
            "last_run_time": "19700101T00:00:00Z",
            "VMs": vm_refs,
        });
        let task: String = self
            .request("Async.VMSS.create", json!([self.session, record]))
            .await?;
        let created = match wait_task_value(self, &task).await {
            Ok(value) => value,
            Err(error) => return Err(error),
        };
        let Some(vmss_ref) = created.as_str() else {
            return Err(ApiError::Protocol);
        };
        self.assign_vmss_vms(vmss_ref, &vm_refs).await
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn update_vmss(
        &self,
        uuid: &str,
        name: &str,
        description: &str,
        snapshot_type: &str,
        schedule: &VmssSchedule,
        retained_snapshots: u64,
        enabled: bool,
        vm_uuids: Vec<String>,
    ) -> Result<(), ApiError> {
        if self.settings.read_only {
            return Err(ApiError::Xen(
                "This connection is read-only. Changes are blocked by the Rust backend.".into(),
            ));
        }
        validate_vmss_fields(
            name,
            description,
            snapshot_type,
            schedule,
            retained_snapshots,
        )?;
        let (vmss_ref, current) = self.vmss_record(uuid).await?;
        let vm_refs = self.resolve_vmss_targets(&vm_uuids).await?;
        let setters: [(&str, &str, Value); 4] = [
            (
                "VMSS.set_name_label",
                "name_label",
                Value::String(name.trim().to_owned()),
            ),
            (
                "VMSS.set_name_description",
                "name_description",
                Value::String(description.to_owned()),
            ),
            (
                "VMSS.set_type",
                "type",
                Value::String(snapshot_type.to_owned()),
            ),
            (
                "VMSS.set_frequency",
                "frequency",
                Value::String(schedule.frequency.clone()),
            ),
        ];
        for (method, key, value) in setters {
            if current[key].as_str() != value.as_str() {
                self.request::<Value>(method, json!([self.session, vmss_ref, value]))
                    .await?;
            }
        }
        if current["retained_snapshots"].as_u64().or_else(|| {
            current["retained_snapshots"]
                .as_str()
                .and_then(|value| value.parse().ok())
        }) != Some(retained_snapshots)
        {
            self.request::<Value>(
                "VMSS.set_retained_snapshots",
                json!([self.session, vmss_ref, retained_snapshots.to_string()]),
            )
            .await?;
        }
        let desired_schedule: serde_json::Map<String, Value> = schedule_map(schedule)
            .into_iter()
            .map(|(key, value)| (key, Value::String(value)))
            .collect();
        if current["schedule"] != Value::Object(desired_schedule) {
            self.request::<Value>(
                "VMSS.set_schedule",
                json!([self.session, vmss_ref, schedule_map(schedule)]),
            )
            .await?;
        }
        let current_vms: Vec<String> = current["VMs"]
            .as_array()
            .map(|refs| {
                refs.iter()
                    .filter_map(Value::as_str)
                    .map(str::to_owned)
                    .collect()
            })
            .unwrap_or_default();
        let removed: Vec<&String> = current_vms
            .iter()
            .filter(|reference| !vm_refs.contains(reference))
            .collect();
        for reference in removed {
            self.request::<Value>(
                "VM.set_snapshot_schedule",
                json!([self.session, reference, "OpaqueRef:NULL"]),
            )
            .await?;
        }
        self.assign_vmss_vms(&vmss_ref, &vm_refs).await?;
        let desired_enabled = enabled && !vm_refs.is_empty();
        if current["enabled"].as_bool().unwrap_or(false) != desired_enabled {
            self.request::<Value>(
                "VMSS.set_enabled",
                json!([self.session, vmss_ref, desired_enabled]),
            )
            .await?;
        }
        Ok(())
    }

    pub async fn delete_vmss(&self, uuid: &str) -> Result<(), ApiError> {
        if self.settings.read_only {
            return Err(ApiError::Xen(
                "This connection is read-only. Changes are blocked by the Rust backend.".into(),
            ));
        }
        let (vmss_ref, current) = self.vmss_record(uuid).await?;
        let current_vms: Vec<String> = current["VMs"]
            .as_array()
            .map(|refs| {
                refs.iter()
                    .filter_map(Value::as_str)
                    .map(str::to_owned)
                    .collect()
            })
            .unwrap_or_default();
        for reference in current_vms {
            let _ = self
                .request::<Value>(
                    "VM.set_snapshot_schedule",
                    json!([self.session, reference, "OpaqueRef:NULL"]),
                )
                .await;
        }
        self.request::<Value>("VMSS.destroy", json!([self.session, vmss_ref]))
            .await?;
        Ok(())
    }

    async fn vmss_record(&self, uuid: &str) -> Result<(String, Value), ApiError> {
        let vmss_ref: String = self
            .request("VMSS.get_by_uuid", json!([self.session, uuid]))
            .await?;
        let record: Value = self
            .request("VMSS.get_record", json!([self.session, vmss_ref]))
            .await?;
        if record["uuid"].as_str() != Some(uuid) {
            return Err(ApiError::Protocol);
        }
        Ok((vmss_ref, record))
    }

    async fn resolve_vmss_targets(&self, vm_uuids: &[String]) -> Result<Vec<String>, ApiError> {
        let mut unique = HashSet::new();
        let mut refs = Vec::new();
        for uuid in vm_uuids {
            if !unique.insert(uuid.clone()) {
                continue;
            }
            let reference: String = self
                .request("VM.get_by_uuid", json!([self.session, uuid]))
                .await?;
            let vm: Value = self
                .request("VM.get_record", json!([self.session, reference]))
                .await?;
            if vm["uuid"].as_str() != Some(uuid.as_str()) || !vmss_target_vm(&vm) {
                return Err(ApiError::Xen(
                    "Snapshot schedules can only include real virtual machines, not templates, snapshots or control domains."
                        .into(),
                ));
            }
            refs.push(reference);
        }
        if refs.len() > 100 {
            return Err(ApiError::Xen(
                "A snapshot schedule can include at most 100 VMs.".into(),
            ));
        }
        Ok(refs)
    }

    async fn assign_vmss_vms(&self, vmss_ref: &str, vm_refs: &[String]) -> Result<(), ApiError> {
        for reference in vm_refs {
            self.request::<Value>(
                "VM.set_snapshot_schedule",
                json!([self.session, reference, vmss_ref]),
            )
            .await?;
        }
        Ok(())
    }
}

fn vmss_target_vm(vm: &Value) -> bool {
    vm["is_control_domain"].as_bool() != Some(true)
        && vm["is_a_template"].as_bool() != Some(true)
        && vm["is_a_snapshot"].as_bool() != Some(true)
}

pub fn validate_vmss_fields(
    name: &str,
    description: &str,
    snapshot_type: &str,
    schedule: &VmssSchedule,
    retained_snapshots: u64,
) -> Result<(), ApiError> {
    let name = name.trim();
    if name.is_empty() || name.chars().count() > 255 {
        return Err(ApiError::Xen(
            "Enter a snapshot schedule name between 1 and 255 characters.".into(),
        ));
    }
    if description.chars().count() > 1024 {
        return Err(ApiError::Xen(
            "The description must be 1024 characters or fewer.".into(),
        ));
    }
    if !SNAPSHOT_TYPES.contains(&snapshot_type) {
        return Err(ApiError::Xen(
            "Snapshot type must be a disk snapshot or a checkpoint.".into(),
        ));
    }
    if !FREQUENCIES.contains(&schedule.frequency.as_str()) {
        return Err(ApiError::Xen(
            "Frequency must be hourly, daily or weekly.".into(),
        ));
    }
    if ![0, 15, 30, 45].contains(&schedule.minute) || schedule.hour > 23 {
        return Err(ApiError::Xen(
            "Select minute 0, 15, 30 or 45 and an hour between 0 and 23.".into(),
        ));
    }
    if schedule.frequency == "weekly" {
        if schedule.days.is_empty() {
            return Err(ApiError::Xen(
                "Select at least one day for a weekly schedule.".into(),
            ));
        }
        let mut unique = HashSet::new();
        for day in &schedule.days {
            if !DAYS.contains(&day.as_str()) {
                return Err(ApiError::Xen("Unknown day of week.".into()));
            }
            if !unique.insert(day) {
                return Err(ApiError::Xen("Days of week must be unique.".into()));
            }
        }
    }
    if !(1..=MAX_RETENTION).contains(&retained_snapshots) {
        return Err(ApiError::Xen(format!(
            "Snapshots to retain must be between 1 and {MAX_RETENTION}."
        )));
    }
    Ok(())
}

fn schedule_map(schedule: &VmssSchedule) -> HashMap<String, String> {
    let mut map = HashMap::new();
    match schedule.frequency.as_str() {
        "hourly" => {
            map.insert("min".into(), schedule.minute.to_string());
        }
        "daily" => {
            map.insert("hour".into(), schedule.hour.to_string());
            map.insert("min".into(), schedule.minute.to_string());
        }
        _ => {
            map.insert("hour".into(), schedule.hour.to_string());
            map.insert("min".into(), schedule.minute.to_string());
            map.insert("days".into(), schedule.days.join(","));
        }
    }
    map
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn hourly() -> VmssSchedule {
        VmssSchedule {
            frequency: "hourly".into(),
            minute: 15,
            hour: 0,
            days: Vec::new(),
        }
    }

    fn weekly() -> VmssSchedule {
        VmssSchedule {
            frequency: "weekly".into(),
            minute: 30,
            hour: 3,
            days: vec!["monday".into(), "friday".into()],
        }
    }

    #[test]
    fn schedules_are_rendered_per_frequency() {
        assert_eq!(hourly().summary(), "Every hour at minute 15");
        assert_eq!(
            VmssSchedule {
                frequency: "daily".into(),
                minute: 5,
                hour: 2,
                days: Vec::new(),
            }
            .summary(),
            "Every day at 02:05"
        );
        assert_eq!(weekly().summary(), "Weekly on monday, friday at 03:30");
        let map = schedule_map(&hourly());
        assert_eq!(map.get("min"), Some(&"15".to_string()));
        assert!(!map.contains_key("hour"));
        let map = schedule_map(&weekly());
        assert_eq!(map.get("days"), Some(&"monday,friday".to_string()));
    }

    #[test]
    fn vmss_fields_are_validated() {
        assert!(validate_vmss_fields("policy", "", "snapshot", &hourly(), 1).is_ok());
        assert!(validate_vmss_fields("", "", "snapshot", &hourly(), 1).is_err());
        assert!(validate_vmss_fields(&"x".repeat(256), "", "snapshot", &hourly(), 1).is_err());
        assert!(
            validate_vmss_fields("policy", &"x".repeat(1025), "snapshot", &hourly(), 1).is_err()
        );
        assert!(validate_vmss_fields("policy", "", "vss", &hourly(), 1).is_err());
        assert!(validate_vmss_fields("policy", "", "snapshot", &hourly(), 0).is_err());
        assert!(validate_vmss_fields("policy", "", "snapshot", &hourly(), 1001).is_err());
        assert!(
            validate_vmss_fields(
                "policy",
                "",
                "snapshot",
                &VmssSchedule {
                    frequency: "monthly".into(),
                    ..hourly()
                },
                1
            )
            .is_err()
        );
        assert!(
            validate_vmss_fields(
                "policy",
                "",
                "snapshot",
                &VmssSchedule {
                    frequency: "daily".into(),
                    hour: 24,
                    ..hourly()
                },
                1
            )
            .is_err()
        );
    }

    #[test]
    fn weekly_schedules_need_valid_unique_days() {
        assert!(validate_vmss_fields("policy", "", "snapshot", &weekly(), 7).is_ok());
        let no_days = VmssSchedule {
            days: Vec::new(),
            ..weekly()
        };
        assert!(validate_vmss_fields("policy", "", "snapshot", &no_days, 7).is_err());
        let bad_day = VmssSchedule {
            days: vec!["funday".into()],
            ..weekly()
        };
        assert!(validate_vmss_fields("policy", "", "snapshot", &bad_day, 7).is_err());
        let duplicates = VmssSchedule {
            days: vec!["monday".into(), "monday".into()],
            ..weekly()
        };
        assert!(validate_vmss_fields("policy", "", "snapshot", &duplicates, 7).is_err());
    }

    #[test]
    fn vmss_targets_exclude_templates_snapshots_and_dom0() {
        assert!(vmss_target_vm(&json!({
            "is_control_domain": false,
            "is_a_template": false,
            "is_a_snapshot": false
        })));
        for record in [
            json!({"is_control_domain": true, "is_a_template": false, "is_a_snapshot": false}),
            json!({"is_control_domain": false, "is_a_template": true, "is_a_snapshot": false}),
            json!({"is_control_domain": false, "is_a_template": false, "is_a_snapshot": true}),
        ] {
            assert!(!vmss_target_vm(&record));
        }
    }
}
