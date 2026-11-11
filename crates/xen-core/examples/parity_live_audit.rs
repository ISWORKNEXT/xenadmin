//! Opt-in destructive audit restricted to the explicitly authorized dev host.
//! Credentials arrive on stdin; output contains selected evidence only.
use std::io::{self, BufRead};

use serde::Deserialize;
use serde_json::json;
use xen_core::operations::{SrAction, VmAction};
use xen_core::performance::HostPerformance;
use xen_core::vmss::VmssSchedule;
use xen_core::xapi::{ApiError, Connection, ConnectionSettings};

#[derive(Deserialize)]
struct Input {
    settings: ConnectionSettings,
    password: String,
    #[serde(default)]
    services: bool,
    #[serde(default)]
    vmss_only: bool,
}

fn evidence(name: &str, result: Result<(), ApiError>) -> bool {
    match result {
        Ok(()) => {
            println!("{}", json!({"check": name, "status": "PASS"}));
            true
        }
        Err(error) => {
            println!(
                "{}",
                json!({"check": name, "status": "FAIL", "error": error.to_string()})
            );
            false
        }
    }
}

async fn action(c: &Connection, vm: &str, action: VmAction) -> Result<(), ApiError> {
    let task = c.begin_vm_action(vm, &action).await?;
    c.wait_task(&task).await
}

async fn state(c: &Connection, vm: &str, expected: &str) -> Result<(), ApiError> {
    let actual = c.vm_details(vm).await?.power_state;
    if actual == expected {
        Ok(())
    } else {
        Err(ApiError::Xen(format!(
            "Expected {expected}, read back {actual}"
        )))
    }
}

async fn exercise(
    c: &Connection,
    vm: &str,
    network: &str,
    sr: &str,
    name: &str,
) -> Result<(), ApiError> {
    let target = xen_core::properties::ObjectTarget {
        kind: xen_core::properties::ObjectKind::Vm,
        uuid: vm.to_owned(),
    };
    let original = c.object_properties(&target).await?;
    let mut edited = original.clone();
    edited.name = name.to_owned();
    edited.description = "Live parity audit temporary VM".into();
    evidence(
        "VM name/description write",
        c.update_object_properties(&xen_core::properties::ObjectPropertiesEdit {
            original,
            edited,
        })
        .await
        .map(|_| ()),
    );
    evidence(
        "VM edit CPU/memory",
        c.update_vm_resources(
            vm,
            &xen_core::operations::VmResourceUpdate {
                vcpus_max: 1,
                vcpus_at_startup: 1,
                memory_static_max_mib: 256,
                memory_dynamic_min_mib: 256,
                memory_dynamic_max_mib: 256,
                boot_order: None,
                firmware: None,
                secure_boot: None,
                boot_policy: None,
                after_crash: None,
            },
        )
        .await,
    );
    let detail = c.vm_details(vm).await?;
    evidence(
        "VM resource readback",
        if detail.vcpus_max == 1 && detail.memory_static_max == 256 * 1024 * 1024 {
            Ok(())
        } else {
            Err(ApiError::Protocol)
        },
    );
    evidence(
        "VM tags write",
        async {
            let original = c.object_properties(&target).await?;
            let mut edited = original.clone();
            edited.tags = vec!["zz-parity-audit".into()];
            c.update_object_properties(&xen_core::properties::ObjectPropertiesEdit {
                original,
                edited,
            })
            .await
            .map(|_| ())
        }
        .await,
    );
    evidence(
        "VM tags readback",
        if c.vm_details(vm)
            .await?
            .tags
            .contains(&"zz-parity-audit".into())
        {
            Ok(())
        } else {
            Err(ApiError::Protocol)
        },
    );
    evidence(
        "VM HA property edit",
        c.update_vm_ha(vm, "", 1, 1, None).await,
    );
    evidence(
        "Add 64 MiB VDI/VBD",
        c.add_disk(vm, sr, 64 * 1024 * 1024, Some(name)).await,
    );
    let detail = c.vm_details(vm).await?;
    if let Some(disk) = detail.disks.iter().find(|d| !d.is_cd) {
        evidence(
            "VDI resize to 128 MiB",
            c.resize_disk(vm, &disk.vdi_uuid, 128 * 1024 * 1024).await,
        );
        evidence(
            "VDI resize readback",
            if c.vm_details(vm)
                .await?
                .disks
                .iter()
                .any(|d| d.vdi_uuid == disk.vdi_uuid && d.virtual_size == 128 * 1024 * 1024)
            {
                Ok(())
            } else {
                Err(ApiError::Protocol)
            },
        );
        evidence(
            "VBD mode RO",
            c.set_vbd_mode(vm, &disk.vbd_uuid, "RO").await,
        );
        evidence(
            "VBD mode RW",
            c.set_vbd_mode(vm, &disk.vbd_uuid, "RW").await,
        );
        evidence(
            "VBD bootable flag",
            c.set_vbd_bootable(vm, &disk.vbd_uuid, true).await,
        );
    }
    evidence("Add empty CD drive", c.add_cd_drive(vm).await);
    evidence(
        "Add VIF to isolated network",
        c.add_vif(vm, network, None).await,
    );
    if let Some(vif) = c.vm_details(vm).await?.interfaces.first() {
        evidence(
            "VIF locking edit",
            c.update_vif(vm, &vif.uuid, network, "unlocked").await,
        );
        evidence("Remove halted VIF", c.remove_vif(vm, &vif.uuid).await);
    }
    evidence(
        "Snapshot halted VM",
        action(
            c,
            vm,
            VmAction::Snapshot {
                name: format!("{name}-snapshot"),
            },
        )
        .await,
    );
    if let Some(snapshot) = c.vm_details(vm).await?.snapshots.first() {
        evidence(
            "Revert halted snapshot",
            action(c, &snapshot.uuid, VmAction::Revert).await,
        );
        evidence("Revert power-state readback", state(c, vm, "Halted").await);
        evidence(
            "Delete snapshot",
            c.destroy_vm(&snapshot.uuid, true, false).await,
        );
    }
    let cloned = c.clone_vm(vm, &format!("{name}-clone")).await;
    match cloned {
        Ok(uuid) => {
            evidence("Clone temporary VM", Ok(()));
            evidence(
                "Delete temporary clone",
                c.destroy_vm(&uuid, true, true).await,
            );
        }
        Err(error) => {
            evidence("Clone temporary VM", Err(error));
        }
    }
    if evidence("Start temporary VM", action(c, vm, VmAction::Start).await) {
        evidence("Start readback", state(c, vm, "Running").await);
        if evidence("Pause VM", action(c, vm, VmAction::Pause).await) {
            evidence("Pause readback", state(c, vm, "Paused").await);
            evidence("Unpause VM", action(c, vm, VmAction::Unpause).await);
            evidence("Unpause readback", state(c, vm, "Running").await);
        }
        if evidence("Suspend VM", action(c, vm, VmAction::Suspend).await) {
            evidence("Suspend readback", state(c, vm, "Suspended").await);
            evidence("Resume VM", action(c, vm, VmAction::Resume).await);
            evidence("Resume readback", state(c, vm, "Running").await);
        }
        if c.vm_details(vm).await?.power_state == "Running" {
            evidence(
                "Force reboot temporary VM",
                action(c, vm, VmAction::ForceReboot).await,
            );
            evidence("Reboot readback", state(c, vm, "Running").await);
        }
        evidence(
            "Force shutdown temporary VM",
            action(c, vm, VmAction::ForceShutdown).await,
        );
        evidence("Shutdown readback", state(c, vm, "Halted").await);
    }
    if let Some(disk) = c.vm_details(vm).await?.disks.iter().find(|d| !d.is_cd) {
        evidence(
            "Remove temporary disk and delete VDI",
            c.remove_disk(vm, &disk.vbd_uuid, true).await,
        );
    }
    Ok(())
}

async fn services(c: &Connection, vmss_only: bool) -> Result<(), ApiError> {
    let baseline = c.inventory().await?;
    let host = baseline.hosts.first().ok_or(ApiError::Protocol)?;
    let name = format!("zz-parity-services-{}", uuid::Uuid::now_v7());
    let location = "/var/tmp/zz-parity-root-sr-20261111";
    if !vmss_only
        && evidence(
            "Create file-backed temporary SR",
            c.create_sr(&host.uuid, &name, "Temporary parity audit", location)
                .await,
        )
    {
        let sr = c
            .inventory()
            .await?
            .storage
            .into_iter()
            .find(|s| s.name == name)
            .ok_or(ApiError::Protocol)?;
        evidence("SR scan", c.sr_action(&sr.uuid, SrAction::Scan).await);
        if let Some(pbd) = c.pbd_catalog(&sr.uuid).await?.first() {
            evidence(
                "Unplug temporary PBD",
                c.set_pbd_plugged(&pbd.uuid, false).await,
            );
            evidence(
                "PBD detached readback",
                if c.pbd_catalog(&sr.uuid).await?.iter().all(|p| !p.attached) {
                    Ok(())
                } else {
                    Err(ApiError::Protocol)
                },
            );
            evidence(
                "Repair temporary SR",
                c.repair_sr(&sr.uuid).await.map(|_| ()),
            );
            evidence(
                "PBD repaired readback",
                if c.pbd_catalog(&sr.uuid).await?.iter().all(|p| p.attached) {
                    Ok(())
                } else {
                    Err(ApiError::Protocol)
                },
            );
        }
        evidence(
            "Detach temporary SR",
            c.sr_action(&sr.uuid, SrAction::Detach).await,
        );
        evidence(
            "Reattach temporary SR",
            c.reattach_sr(&sr.uuid, &host.uuid, location).await,
        );
        evidence(
            "Probe file-backed SR",
            c.probe_sr(&host.uuid, location).await.map(|_| ()),
        );
        evidence(
            "TRIM file-backed SR",
            c.sr_action(&sr.uuid, SrAction::Trim).await,
        );
        evidence(
            "Detach SR before forget",
            c.sr_action(&sr.uuid, SrAction::Detach).await,
        );
        if evidence(
            "Forget temporary SR",
            c.sr_action(&sr.uuid, SrAction::Forget).await,
        ) {
            evidence(
                "Reintroduce temporary SR",
                c.introduce_sr(
                    &sr.uuid,
                    &name,
                    "Temporary parity audit",
                    "file",
                    "user",
                    false,
                )
                .await,
            );
            evidence(
                "Reattach reintroduced SR",
                c.reattach_sr(&sr.uuid, &host.uuid, location).await,
            );
        }
        evidence(
            "Destroy temporary SR",
            c.sr_action(&sr.uuid, SrAction::Destroy).await,
        );
    }
    let schedule = VmssSchedule {
        frequency: "daily".into(),
        minute: 15,
        hour: 3,
        days: vec![],
    };
    if evidence(
        "Create disabled empty VMSS",
        c.create_vmss(
            &name,
            "Temporary parity audit",
            "snapshot",
            &schedule,
            2,
            false,
            vec![],
        )
        .await,
    ) && let Some(policy) = c.pool_status().await?.vmss.iter().find(|p| p.name == name)
    {
        evidence(
            "Edit disabled empty VMSS",
            c.update_vmss(
                &policy.uuid,
                &name,
                "Updated parity audit",
                "snapshot",
                &schedule,
                3,
                false,
                vec![],
            )
            .await,
        );
        evidence("Delete temporary VMSS", c.delete_vmss(&policy.uuid).await);
    }
    if !vmss_only
        && evidence(
            "Introduce temporary PVS site",
            c.introduce_pvs_site(&name, "Temporary parity audit").await,
        )
        && let Some(site) = c
            .pool_status()
            .await?
            .pvs
            .sites
            .iter()
            .find(|p| p.name == name)
    {
        evidence(
            "Introduce temporary PVS server record",
            c.introduce_pvs_server(&site.uuid, &["192.0.2.123".into()], 6910, 6930)
                .await,
        );
        if let Some(server) = c
            .pool_status()
            .await?
            .pvs
            .servers
            .iter()
            .find(|p| p.site_name == site.name)
        {
            evidence(
                "Forget temporary PVS server",
                c.forget_pvs_server(&server.uuid).await,
            );
        }
        evidence(
            "Forget temporary PVS site",
            c.forget_pvs_site(&site.uuid).await,
        );
    }
    let inventory = c.inventory().await?;
    let status = c.pool_status().await?;
    println!(
        "{}",
        json!({"check": "Services cleanup readback", "temporary_srs": inventory.storage.iter().filter(|s| s.name == name).count(), "temporary_vmss": status.vmss.iter().filter(|s| s.name == name).count(), "temporary_pvs_sites": status.pvs.sites.iter().filter(|s| s.name == name).count()})
    );
    Ok(())
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut line = String::new();
    io::stdin().lock().read_line(&mut line)?;
    let input: Input = serde_json::from_str(&line)?;
    drop(line);
    if input.settings.host != "192.168.8.170" || input.settings.read_only {
        return Err(
            "This audit requires the explicitly authorized dev host and write profile".into(),
        );
    }
    let c = Connection::login(input.settings, &input.password).await?;
    drop(input.password);
    if input.services {
        let result = services(&c, input.vmss_only).await;
        evidence("Service sequence finished", result);
        c.logout().await?;
        return Ok(());
    }
    let baseline = c.inventory().await?;
    let name = format!("zz-parity-root-{}", uuid::Uuid::now_v7());
    println!(
        "{}",
        json!({"baseline": {"hosts": baseline.hosts.len(), "vms": baseline.vms.len(), "srs": baseline.storage.len(), "networks": baseline.networks.len()}, "temporary_prefix": name})
    );
    let result: Result<(), ApiError> = async {
        for host in &baseline.hosts {
            evidence("Host details live", c.host_details(&host.uuid).await.map(|_| ()));
            for range in [900, 3600, 86400] {
                let performance = c.host_performance(&host.uuid, range).await?;
                match performance {
                    HostPerformance::Ready { series, .. } => {
                        println!("{}", json!({"check": "Host RRD live", "status": "PASS", "range_seconds": range, "metric_series": series.len(), "samples": series.iter().map(|s| s.points.len()).sum::<usize>()}));
                    }
                    HostPerformance::Unsupported { message } | HostPerformance::Unavailable { message } => {
                        println!("{}", json!({"check": "Host RRD live", "status": "UNAVAILABLE", "range_seconds": range, "message": message}));
                    }
                }
            }
        }
        evidence("Authentication/RBAC live", c.auth_info().await.map(|_| ()));
        evidence("Pool services/status live", c.pool_status().await.map(|_| ()));
        evidence("PIF inventory live", c.pif_catalog().await.map(|_| ()));
        for sr in &baseline.storage {
            evidence("PBD inventory live", c.pbd_catalog(&sr.uuid).await.map(|_| ()));
        }
        let template = baseline.templates.iter().find(|t| t.name == "Other install media")
            .or_else(|| baseline.templates.iter().find(|t| t.name.contains("Other install")))
            .ok_or_else(|| ApiError::Xen("No diskless Other install media template; mutation phase skipped".into()))?;
        let sr = baseline.storage.iter().find(|sr| matches!(sr.storage_type.as_str(), "ext" | "lvm" | "lvmthin" | "file") && sr.total.saturating_sub(sr.used) > 1024 * 1024 * 1024)
            .ok_or_else(|| ApiError::Xen("No writable SR with 1 GiB headroom".into()))?;
        evidence("Create isolated internal network", c.create_network(&name, "Temporary live audit network", 1500).await);
        let network = c.inventory().await?.networks.into_iter().find(|n| n.name == name)
            .ok_or(ApiError::Protocol)?;
        let vm_result = c.clone_vm(&template.uuid, &name).await;
        match vm_result {
            Ok(vm) => {
                evidence("Create VM from template/provision", Ok(()));
                println!("{}", json!({"temporary_vm_uuid": vm}));
                let run = exercise(&c, &vm, &network.uuid, &sr.uuid, &name).await;
                evidence("Mutation sequence completed", run);
                if c.vm_details(&vm).await.is_ok_and(|v| v.power_state != "Halted") {
                    evidence("Cleanup force stop", action(&c, &vm, VmAction::ForceShutdown).await);
                }
                evidence("Cleanup delete VM/disks/snapshots", c.destroy_vm(&vm, true, true).await);
            }
            Err(error) => { evidence("Create VM from template/provision", Err(error)); }
        }
        evidence("Cleanup isolated network", c.destroy_network(&network.uuid).await);
        let after = c.inventory().await?;
        let leftovers: Vec<_> = after.vms.iter().chain(after.snapshots.iter()).filter(|vm| vm.name.starts_with(&name)).map(|vm| vm.name.clone()).collect();
        let baseline_unchanged = baseline.vms.iter().all(|before| after.vms.iter().any(|after| after.uuid == before.uuid && after.name == before.name && after.power_state == before.power_state));
        println!("{}", json!({"check": "Final inventory readback", "temporary_vm_leftovers": leftovers, "temporary_network_leftovers": after.networks.iter().filter(|n| n.name == name).count(), "existing_vm_names_and_power_states_unchanged": baseline_unchanged}));
        Ok(())
    }.await;
    evidence("Audit result", result);
    c.logout().await?;
    Ok(())
}
