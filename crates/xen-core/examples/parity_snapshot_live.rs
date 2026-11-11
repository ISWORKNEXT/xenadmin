//! Opt-in live qualification. Creates and removes only uniquely named lab objects.
use std::io::{self, BufRead};

use serde::Deserialize;
use serde_json::json;
use xen_core::{
    operations::VmAction,
    provisioning::CreateVmRequest,
    vmss::{VmssSchedule, validate_vmss_fields},
    xapi::{Connection, ConnectionSettings},
};

type Error = Box<dyn std::error::Error>;

#[derive(Deserialize)]
struct Input {
    settings: ConnectionSettings,
    password: String,
    #[serde(default)]
    pause_for_ui: bool,
    #[serde(default)]
    vif_only: bool,
}

async fn action(c: &Connection, vm: &str, action: VmAction) -> Result<(), Error> {
    let task = c.begin_vm_action(vm, &action).await?;
    c.wait_task(&task).await?;
    Ok(())
}

async fn exercise(
    c: &Connection,
    prefix: &str,
    pause_for_ui: bool,
    vif_only: bool,
) -> Result<(), Error> {
    let inventory = c.inventory().await?;
    let template = inventory
        .templates
        .iter()
        .find(|vm| vm.name == "Debian 11 Legacy (TH) - Guest Agent")
        .ok_or("Debian 11 Legacy template missing")?;
    let sr = inventory
        .storage
        .iter()
        .find(|sr| sr.name == "Local storage")
        .ok_or("Local storage missing")?;
    let network = inventory
        .networks
        .iter()
        .find(|n| n.name == "Pool-wide network 0" && n.bridge != "xenapi")
        .ok_or("Guest network missing")?;
    let source = c.vm_details(&template.uuid).await?;
    let uuid = c
        .create_vm_from_template(&CreateVmRequest {
            template_uuid: template.uuid.clone(),
            name: format!("{prefix}-source"),
            description: "Temporary real-host snapshot qualification".into(),
            vcpus: 1,
            memory_mib: source.memory_static_min.div_ceil(1024 * 1024).max(1024),
            sr_uuid: sr.uuid.clone(),
            new_disk_gib: None,
            network_uuid: Some(network.uuid.clone()),
            iso_uuid: None,
            affinity_host_uuid: None,
        })
        .await?;
    let created = c.vm_details(&uuid).await?;
    println!(
        "{}",
        json!({"phase":"source-created","uuid":uuid,"template":created.is_template,"power":created.power_state,"allowed":c.inventory().await?.vms.iter().find(|vm| vm.uuid == uuid).map(|vm| &vm.allowed_operations)})
    );
    if !vif_only {
        action(
            c,
            &uuid,
            VmAction::Snapshot {
                name: format!("{prefix}-snapshot"),
            },
        )
        .await?;
        let inventory = c.inventory().await?;
        let snapshot = inventory
            .snapshots
            .iter()
            .find(|s| s.name == format!("{prefix}-snapshot"))
            .ok_or("Created snapshot missing")?;
        let original = c.vm_details(&snapshot.uuid).await?;
        if pause_for_ui {
            println!(
                "{}",
                json!({"phase":"ready-for-ui", "snapshot_uuid":snapshot.uuid,"source_uuid":uuid})
            );
            let mut resume = String::new();
            io::stdin().lock().read_line(&mut resume)?;
        }
        let original_disks: Vec<_> = original
            .disks
            .iter()
            .filter(|d| !d.is_cd)
            .map(|d| d.vdi_uuid.clone())
            .collect();
        if original_disks.is_empty() {
            return Err("Snapshot has no disk to qualify".into());
        }
        for as_template in [false, true] {
            let created = c
                .create_from_snapshot(
                    &snapshot.uuid,
                    &format!("{prefix}-output-{as_template}"),
                    as_template,
                )
                .await?;
            let details = c.vm_details(&created).await?;
            assert_eq!(details.is_template, as_template);
            assert!(!details.is_snapshot && details.power_state == "Halted");
            let disks: Vec<_> = details.disks.iter().filter(|d| !d.is_cd).collect();
            assert_eq!(disks.len(), original_disks.len());
            assert!(
                disks
                    .iter()
                    .all(|d| d.owner && !original_disks.contains(&d.vdi_uuid))
            );
            assert_eq!(details.firmware, original.firmware);
            let disk_ids: Vec<_> = disks.iter().map(|d| d.vdi_uuid.clone()).collect();
            c.destroy_vm(&created, true, false).await?;
            assert!(
                !c.vdi_catalog()
                    .await?
                    .iter()
                    .any(|d| disk_ids.contains(&d.uuid))
            );
            let unchanged = c.vm_details(&snapshot.uuid).await?;
            assert_eq!(
                original_disks,
                unchanged
                    .disks
                    .iter()
                    .filter(|d| !d.is_cd)
                    .map(|d| d.vdi_uuid.clone())
                    .collect::<Vec<_>>()
            );
            println!(
                "{}",
                json!({"check":"snapshot-output", "as_template":as_template,"uuid":created,"independent_disks":true,"source_unchanged":true,"output_cleanup":true})
            );
        }
    }
    action(c, &uuid, VmAction::Start).await?;
    println!("{}", json!({"phase":"vm-started","uuid":uuid}));
    tokio::time::sleep(std::time::Duration::from_secs(30)).await;
    let details = c.vm_details(&uuid).await?;
    if details.power_state != "Running" {
        return Err("The test VM did not remain Running".into());
    }
    println!(
        "{}",
        json!({"check":"guest-metrics", "os":details.os_version,"pv_version":details.pv_drivers_version})
    );
    if details.os_version.is_empty() || details.pv_drivers_version.is_empty() {
        return Err("Guest OS or PV-driver version was not reported".into());
    }
    let interface = details
        .interfaces
        .iter()
        .find(|v| v.attached)
        .ok_or("No attached VIF to qualify")?;
    c.remove_vif(&uuid, &interface.uuid).await?;
    assert!(
        !c.vm_details(&uuid)
            .await?
            .interfaces
            .iter()
            .any(|v| v.uuid == interface.uuid)
    );
    println!(
        "{}",
        json!({"check":"remove-attached-vif","removed":true,"vm_power":c.vm_details(&uuid).await?.power_state})
    );
    let invalid = VmssSchedule {
        frequency: "daily".into(),
        minute: 7,
        hour: 3,
        days: vec![],
    };
    assert!(validate_vmss_fields("validation", "", "snapshot", &invalid, 1).is_err());
    let valid = VmssSchedule {
        minute: 15,
        ..invalid
    };
    c.create_vmss(
        &format!("{prefix}-schedule"),
        "",
        "snapshot",
        &valid,
        1,
        false,
        vec![],
    )
    .await?;
    println!(
        "{}",
        json!({"check":"vmss-quarter-hour","invalid_rejected":true,"valid_created":true})
    );
    Ok(())
}

#[tokio::main]
async fn main() -> Result<(), Error> {
    let mut line = String::new();
    io::stdin().lock().read_line(&mut line)?;
    let input: Input = serde_json::from_str(&line)?;
    if input.settings.host != "192.168.8.170" || input.settings.read_only {
        return Err("This opt-in mutation test requires the authorized writable dev host".into());
    }
    let c = Connection::login(input.settings, &input.password).await?;
    let prefix = format!("zz-parity-snapshot-{}", uuid::Uuid::now_v7());
    println!("{}", json!({"prefix":prefix}));
    let result = exercise(&c, &prefix, input.pause_for_ui, input.vif_only).await;
    let inventory = c.inventory().await?;
    for vm in inventory
        .snapshots
        .iter()
        .chain(&inventory.vms)
        .chain(&inventory.templates)
        .filter(|v| v.name.starts_with(&prefix))
    {
        if vm.power_state == "Running" || vm.power_state == "Paused" {
            action(&c, &vm.uuid, VmAction::ForceShutdown).await?;
        }
        c.destroy_vm(&vm.uuid, true, true).await?;
    }
    // Policy records remain disabled and have no VM assignments.
    let status = c.pool_status().await?;
    for schedule in status.vmss.iter().filter(|s| s.name.starts_with(&prefix)) {
        c.delete_vmss(&schedule.uuid).await?;
    }
    println!(
        "{}",
        json!({"cleanup":"completed","result":result.as_ref().map(|_| "pass").map_err(|e| e.to_string())})
    );
    c.logout().await?;
    result
}
