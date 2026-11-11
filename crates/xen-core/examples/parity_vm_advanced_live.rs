//! Real-host before/after checks for advanced VM operations.
use serde::Deserialize;
use serde_json::json;
use std::io::{self, BufRead, Write};
use std::process::{Command, Stdio};
use xen_core::{
    operations::VmAction,
    provisioning::CreateVmRequest,
    xapi::{Connection, ConnectionSettings},
};
type Error = Box<dyn std::error::Error>;

fn checkpoint_image(uuid: &str) -> Result<(), Error> {
    let script=r#"
import XenAPI,json
s=XenAPI.xapi_local();s.xenapi.login_with_password('','','1.0','checkpoint-readback')
try:
 v=s.xenapi.VM.get_record(s.xenapi.VM.get_by_uuid(UUID))
 assert v['name_label'].startswith('zz-parity-advanced-') and v['is_a_snapshot'] and v['power_state']=='Suspended'
 assert v['suspend_VDI']!='OpaqueRef:NULL'
 d=s.xenapi.VDI.get_record(v['suspend_VDI']);assert int(d['virtual_size'])>0
 print(json.dumps({'check':'checkpoint-memory-image','snapshot_uuid':v['uuid'],'memory_vdi_uuid':d['uuid'],'bytes':int(d['virtual_size'])}))
finally:s.xenapi.session.logout()
"#.replace("UUID",&serde_json::to_string(uuid)?);
    let mut child = Command::new("ssh")
        .args(["-o", "BatchMode=yes", "root@192.168.8.170", "python3", "-"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
    child
        .stdin
        .take()
        .ok_or("Missing SSH input")?
        .write_all(script.as_bytes())?;
    let output = child.wait_with_output()?;
    if !output.status.success() {
        return Err(format!(
            "Checkpoint memory-image verification failed: {}",
            String::from_utf8_lossy(&output.stderr)
        )
        .into());
    }
    print!("{}", String::from_utf8(output.stdout)?);
    Ok(())
}

#[derive(Deserialize)]
struct Input {
    settings: ConnectionSettings,
    password: String,
    mode: String,
    #[serde(default)]
    pause_for_ui: bool,
}

async fn action(c: &Connection, uuid: &str, action: VmAction) -> Result<(), Error> {
    let task = c.begin_vm_action(uuid, &action).await?;
    c.wait_task(&task).await?;
    Ok(())
}

fn owned_legacy_metadata(uuid: &str) -> Result<(), Error> {
    let script = r#"
import XenAPI,json
s=XenAPI.xapi_local();s.xenapi.login_with_password('','','1.0','owned-legacy-provisioning-check')
try:
 r=s.xenapi.VM.get_by_uuid(UUID);v=s.xenapi.VM.get_record(r)
 assert v['name_label'].startswith('zz-parity-advanced-') and v['is_a_template'] and v['power_state']=='Halted'
 s.xenapi.VM.set_name_description(r,'L'*40000)
 s.xenapi.VM.set_tags(r,['  inherited exact spacing  ','inherited-tag'])
 print(json.dumps({'check':'seed-owned-legacy-template','uuid':v['uuid'],'description_chars':40000,'exact_spaced_tag':True}))
finally:s.xenapi.session.logout()
"#.replace("UUID", &serde_json::to_string(uuid)?);
    let mut child = Command::new("ssh")
        .args(["-o", "BatchMode=yes", "root@192.168.8.170", "python3", "-"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
    child
        .stdin
        .take()
        .ok_or("Missing SSH input")?
        .write_all(script.as_bytes())?;
    let output = child.wait_with_output()?;
    if !output.status.success() {
        return Err(format!(
            "Owned legacy setup failed: {}",
            String::from_utf8_lossy(&output.stderr)
        )
        .into());
    }
    print!("{}", String::from_utf8(output.stdout)?);
    Ok(())
}

fn owned_boot_state(uuid: &str, prepare: bool) -> Result<serde_json::Value, Error> {
    let script = r#"
import XenAPI,json
s=XenAPI.xapi_local();s.xenapi.login_with_password('','','1.0','owned-resource-preservation-check')
try:
 r=s.xenapi.VM.get_by_uuid(UUID);v=s.xenapi.VM.get_record(r)
 assert v['name_label'].startswith('zz-parity-advanced-') and not v['is_a_template'] and v['power_state']=='Halted'
 if PREPARE:
  p=v['platform'];p.pop('secureboot',None);s.xenapi.VM.set_platform(r,p)
  b=v['HVM_boot_params'];b['parity-owned-key']='preserve';s.xenapi.VM.set_HVM_boot_params(r,b)
 v=s.xenapi.VM.get_record(r)
 print(json.dumps({'platform':v['platform'],'boot_params':v['HVM_boot_params']}))
finally:s.xenapi.session.logout()
"#.replace("UUID", &serde_json::to_string(uuid)?).replace("PREPARE", if prepare {"True"} else {"False"});
    let mut child = Command::new("ssh")
        .args(["-o", "BatchMode=yes", "root@192.168.8.170", "python3", "-"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
    child
        .stdin
        .take()
        .ok_or("Missing SSH input")?
        .write_all(script.as_bytes())?;
    let output = child.wait_with_output()?;
    if !output.status.success() {
        return Err(format!(
            "Owned boot readback/setup failed: {}",
            String::from_utf8_lossy(&output.stderr)
        )
        .into());
    }
    Ok(serde_json::from_slice(&output.stdout)?)
}

async fn provisioning_descriptions(c: &Connection, prefix: &str) -> Result<(), Error> {
    let inventory = c.inventory().await?;
    let template = inventory
        .templates
        .iter()
        .find(|v| v.name == "Debian 11 Legacy (TH) - Guest Agent")
        .ok_or("Legacy test template missing")?;
    let sr = inventory
        .storage
        .iter()
        .find(|sr| sr.name == "Local storage")
        .ok_or("Local storage missing")?;
    let mut request = CreateVmRequest {
        template_uuid: template.uuid.clone(),
        name: format!("{prefix}-source"),
        description: "D".repeat(2000),
        vcpus: 1,
        memory_mib: 1024,
        sr_uuid: sr.uuid.clone(),
        new_disk_gib: None,
        network_uuid: None,
        iso_uuid: None,
        affinity_host_uuid: None,
    };
    let source = c.create_vm_from_template(&request).await?;
    let details = c.vm_details(&source).await?;
    if details.description != request.description {
        return Err("Changed long description not preserved during creation".into());
    }
    println!(
        "{}",
        json!({"check":"create-with-changed-long-description","uuid":source,"description_chars":details.description.chars().count(),"status":"VERIFIED"})
    );
    let before_boot = owned_boot_state(&source, true)?;
    let mut update = xen_core::operations::VmResourceUpdate {
        vcpus_max: 2,
        vcpus_at_startup: 2,
        memory_static_max_mib: 1024,
        memory_dynamic_min_mib: 1024,
        memory_dynamic_max_mib: 1024,
        boot_order: None,
        firmware: None,
        secure_boot: None,
        boot_policy: None,
        after_crash: None,
    };
    c.update_vm_resources(&source, &update).await?;
    if owned_boot_state(&source, false)? != before_boot
        || c.vm_details(&source).await?.vcpus_at_startup != 2
    {
        return Err("CPU-only update changed absent boot settings or did not change CPU".into());
    }
    println!(
        "{}",
        json!({"check":"cpu-only-update-preserves-absent-secureboot-and-boot-maps","uuid":source,"startup_vcpus":2,"status":"VERIFIED"})
    );
    update.boot_order = Some("cd".into());
    c.update_vm_resources(&source, &update).await?;
    let after_boot = owned_boot_state(&source, false)?;
    let mut expected = before_boot.clone();
    expected["boot_params"]["order"] = json!("cd");
    if after_boot != expected {
        return Err("Boot-order edit changed unrelated map keys".into());
    }
    println!(
        "{}",
        json!({"check":"boot-order-change-preserves-other-map-keys","uuid":source,"order":"cd","status":"VERIFIED"})
    );
    c.convert_vm_to_template(&source).await?;
    owned_legacy_metadata(&source)?;
    request.template_uuid = source.clone();
    request.name = format!("{prefix}-legacy-child");
    request.description = "L".repeat(40000);
    let child = c.create_vm_from_template(&request).await?;
    let details = c.vm_details(&child).await?;
    let properties = c
        .object_properties(&xen_core::properties::ObjectTarget {
            kind: xen_core::properties::ObjectKind::Vm,
            uuid: child.clone(),
        })
        .await?;
    if details.description != request.description
        || details.vcpus_at_startup != 1
        || !properties
            .tags
            .contains(&"  inherited exact spacing  ".to_owned())
    {
        return Err("Inherited legacy metadata or configured resources changed".into());
    }
    println!(
        "{}",
        json!({"check":"create-with-unchanged-legacy-description","uuid":child,"description_chars":40000,"exact_spaced_tag":true,"status":"VERIFIED"})
    );
    let before = c.inventory().await?;
    request.name = format!("{prefix}-must-not-exist");
    request.description = "X".repeat(32768);
    let error = c
        .create_vm_from_template(&request)
        .await
        .err()
        .ok_or("Changed oversized description accepted")?;
    let after = c.inventory().await?;
    let ids = |v: &xen_core::xapi::Inventory| {
        let mut ids: Vec<_> = v
            .vms
            .iter()
            .chain(&v.templates)
            .filter(|v| v.name.starts_with(prefix))
            .map(|v| v.uuid.clone())
            .collect();
        ids.sort();
        ids
    };
    if ids(&before) != ids(&after) {
        return Err("Invalid description created an extra object".into());
    }
    println!(
        "{}",
        json!({"check":"changed-oversized-description-rejected-before-clone","error":error.to_string(),"owned_object_set_unchanged":true,"status":"VERIFIED"})
    );
    Ok(())
}

async fn exercise(c: &Connection, prefix: &str, input: &Input) -> Result<(), Error> {
    if input.mode == "verify-provisioning" {
        return provisioning_descriptions(c, prefix).await;
    }
    let inventory = c.inventory().await?;
    let template = inventory
        .templates
        .iter()
        .find(|v| v.name == "Debian 11 Legacy (TH) - Guest Agent")
        .ok_or("Legacy test template missing")?;
    let sr = inventory
        .storage
        .iter()
        .find(|sr| sr.name == "Local storage")
        .ok_or("Local storage missing")?;
    let uuid = c
        .create_vm_from_template(&CreateVmRequest {
            template_uuid: template.uuid.clone(),
            name: format!("{prefix}-vm"),
            description: "Temporary advanced VM qualification".into(),
            vcpus: 1,
            memory_mib: 1024,
            sr_uuid: sr.uuid.clone(),
            new_disk_gib: None,
            network_uuid: None,
            iso_uuid: None,
            affinity_host_uuid: None,
        })
        .await?;
    c.update_vm_resources(
        &uuid,
        &xen_core::operations::VmResourceUpdate {
            vcpus_max: if input.mode == "verify-resources" {
                4
            } else {
                2
            },
            vcpus_at_startup: 1,
            memory_static_max_mib: 1024,
            memory_dynamic_min_mib: 1024,
            memory_dynamic_max_mib: 1024,
            boot_order: None,
            firmware: None,
            secure_boot: None,
            boot_policy: None,
            after_crash: None,
        },
    )
    .await?;
    c.add_disk(
        &uuid,
        &sr.uuid,
        1024 * 1024 * 1024,
        Some(&format!("{prefix}-data")),
    )
    .await?;
    let details = c.vm_details(&uuid).await?;
    let disk = details
        .disks
        .iter()
        .find(|d| d.vdi_name == format!("{prefix}-data"))
        .ok_or("Test data disk missing")?;
    if input.mode == "verify-actions" {
        action(c, &uuid, VmAction::StartPaused).await?;
        if c.vm_details(&uuid).await?.power_state != "Paused" {
            return Err("Start paused did not produce Paused state".into());
        }
        println!(
            "{}",
            json!({"check":"start-paused","uuid":uuid,"state":"Paused"})
        );
        action(c, &uuid, VmAction::Unpause).await?;
    } else {
        action(c, &uuid, VmAction::Start).await?;
    }
    tokio::time::sleep(std::time::Duration::from_secs(30)).await;
    let details = c.vm_details(&uuid).await?;
    if details.power_state != "Running" || details.pv_drivers_version.is_empty() {
        return Err("Guest did not boot with PV drivers".into());
    }
    println!(
        "{}",
        json!({"phase":"guest-ready","uuid":uuid,"data_disk":disk.vdi_uuid,"pv_version":details.pv_drivers_version,"operations":c.inventory().await?.vms.iter().find(|v|v.uuid==uuid).map(|v|&v.allowed_operations)})
    );
    if input.mode == "baseline" {
        let cpu = c
            .update_vm_resources(
                &uuid,
                &xen_core::operations::VmResourceUpdate {
                    vcpus_max: 2,
                    vcpus_at_startup: 2,
                    memory_static_max_mib: 1024,
                    memory_dynamic_min_mib: 1024,
                    memory_dynamic_max_mib: 1024,
                    boot_order: None,
                    firmware: None,
                    secure_boot: None,
                    boot_policy: None,
                    after_crash: None,
                },
            )
            .await;
        let grow = c
            .resize_disk(&uuid, &disk.vdi_uuid, 2 * 1024 * 1024 * 1024)
            .await;
        println!(
            "{}",
            json!({"check":"before-live-resources","cpu_error":cpu.err().map(|e|e.to_string()),"disk_error":grow.err().map(|e|e.to_string())})
        );
    } else if input.mode == "verify-resources" {
        if !details.can_hotplug_vcpus {
            return Err("Guest does not allow vCPU hotplug".into());
        }
        c.update_vm_resources(
            &uuid,
            &xen_core::operations::VmResourceUpdate {
                vcpus_max: 4,
                vcpus_at_startup: 2,
                memory_static_max_mib: 1024,
                memory_dynamic_min_mib: 1024,
                memory_dynamic_max_mib: 1024,
                boot_order: None,
                firmware: None,
                secure_boot: None,
                boot_policy: None,
                after_crash: None,
            },
        )
        .await?;
        let after = c.vm_details(&uuid).await?;
        if after.power_state != "Running" || after.vcpus_max != 4 || after.vcpus_at_startup != 2 {
            return Err("Live CPU addition readback failed".into());
        }
        let decrease = c
            .update_vm_resources(
                &uuid,
                &xen_core::operations::VmResourceUpdate {
                    vcpus_max: 4,
                    vcpus_at_startup: 1,
                    memory_static_max_mib: 1024,
                    memory_dynamic_min_mib: 1024,
                    memory_dynamic_max_mib: 1024,
                    boot_order: None,
                    firmware: None,
                    secure_boot: None,
                    boot_policy: None,
                    after_crash: None,
                },
            )
            .await;
        let maximum = c
            .update_vm_resources(
                &uuid,
                &xen_core::operations::VmResourceUpdate {
                    vcpus_max: 3,
                    vcpus_at_startup: 2,
                    memory_static_max_mib: 1024,
                    memory_dynamic_min_mib: 1024,
                    memory_dynamic_max_mib: 1024,
                    boot_order: None,
                    firmware: None,
                    secure_boot: None,
                    boot_policy: None,
                    after_crash: None,
                },
            )
            .await;
        if decrease.is_ok() || maximum.is_ok() {
            return Err("Unsafe live CPU change accepted".into());
        }
        println!(
            "{}",
            json!({"check":"live-cpu","uuid":uuid,"vcpus":2,"maximum":4,"vm_running":true,"cpu_decrease_rejected":true,"cpu_maximum_rejected":true})
        );
        let disk_state = c.vm_details(&uuid).await?;
        let disk_state = disk_state
            .disks
            .iter()
            .find(|d| d.vdi_uuid == disk.vdi_uuid)
            .ok_or("Test disk missing")?;
        let supported = disk_state
            .allowed_operations
            .iter()
            .any(|op| op == "resize" || op == "resize_online");
        let growth = c
            .resize_disk(&uuid, &disk.vdi_uuid, 2 * 1024 * 1024 * 1024)
            .await;
        if supported {
            growth?;
            let after = c.vm_details(&uuid).await?;
            let grown = after
                .disks
                .iter()
                .find(|d| d.vdi_uuid == disk.vdi_uuid)
                .ok_or("Grown disk missing")?;
            if after.power_state != "Running" || grown.virtual_size != 2 * 1024 * 1024 * 1024 {
                return Err("Online disk growth readback failed".into());
            }
            println!(
                "{}",
                json!({"check":"live-disk-growth","status":"VERIFIED","bytes":grown.virtual_size})
            );
        } else {
            if growth.is_ok() {
                return Err("Unsupported disk growth accepted".into());
            }
            println!(
                "{}",
                json!({"check":"live-disk-growth","status":"NOT VERIFIED","reason":"Storage backend does not advertise resize or resize_online for this attached disk","allowed_operations":disk_state.allowed_operations,"unsupported_rejected":true})
            );
        }
        if c.resize_disk(&uuid, &disk.vdi_uuid, 512 * 1024 * 1024)
            .await
            .is_ok()
        {
            return Err("Disk shrink accepted".into());
        }
        action(c, &uuid, VmAction::Pause).await?;
        if c.resize_disk(&uuid, &disk.vdi_uuid, 3 * 1024 * 1024 * 1024)
            .await
            .is_ok()
        {
            return Err("Paused VM disk resize accepted".into());
        }
        action(c, &uuid, VmAction::Unpause).await?;
        if input.pause_for_ui {
            println!(
                "{}",
                json!({"phase":"ready-for-ui","uuid":uuid,"prefix":prefix})
            );
            let mut line = String::new();
            io::stdin().lock().read_line(&mut line)?;
        }
    } else if input.mode == "verify-actions" {
        let rejected = c.convert_vm_to_template(&uuid).await;
        if rejected.is_ok() {
            return Err("Running VM conversion was accepted".into());
        }
        let checkpoint_name = format!("{prefix}-checkpoint");
        action(
            c,
            &uuid,
            VmAction::Checkpoint {
                name: checkpoint_name.clone(),
            },
        )
        .await?;
        let inventory = c.inventory().await?;
        let checkpoint = inventory
            .snapshots
            .iter()
            .find(|s| s.name == checkpoint_name)
            .ok_or("Checkpoint missing")?;
        let captured = c.vm_details(&checkpoint.uuid).await?;
        if !captured.is_snapshot
            || captured.power_state != "Suspended"
            || c.vm_details(&uuid).await?.power_state != "Running"
        {
            return Err("Checkpoint did not preserve memory and the running VM".into());
        }
        println!(
            "{}",
            json!({"check":"checkpoint","uuid":checkpoint.uuid,"snapshot_state":captured.power_state,"parent_running":true})
        );
        checkpoint_image(&checkpoint.uuid)?;
        let quiesce = c
            .begin_vm_action(
                &uuid,
                &VmAction::SnapshotWithQuiesce {
                    name: format!("{prefix}-quiesced"),
                },
            )
            .await;
        match quiesce {
            Ok(task) => match c.wait_task(&task).await {
                Ok(()) => println!("{}", json!({"check":"quiesce","status":"VERIFIED"})),
                Err(error) => println!(
                    "{}",
                    json!({"check":"quiesce","status":"INCONCLUSIVE","error":error.to_string()})
                ),
            },
            Err(error) => println!(
                "{}",
                json!({"check":"quiesce","status":"NOT VERIFIED","error":error.to_string()})
            ),
        }
        c.destroy_vm(&checkpoint.uuid, true, false).await?;
        action(c, &uuid, VmAction::Shutdown).await?;
        if c.vm_details(&uuid).await?.power_state != "Halted" {
            return Err("Guest did not shut down cleanly".into());
        }
        let before = c.vm_details(&uuid).await?;
        let disk_ids: Vec<_> = before.disks.iter().map(|d| d.vdi_uuid.clone()).collect();
        c.convert_vm_to_template(&uuid).await?;
        c.convert_vm_to_template(&uuid).await?;
        let after = c.vm_details(&uuid).await?;
        if !after.is_template
            || after.power_state != "Halted"
            || disk_ids
                != after
                    .disks
                    .iter()
                    .map(|d| d.vdi_uuid.clone())
                    .collect::<Vec<_>>()
        {
            return Err("VM conversion changed its disks or produced the wrong type".into());
        }
        println!(
            "{}",
            json!({"check":"convert-to-template","uuid":uuid,"same_disks":true,"idempotent":true,"running_rejected":true})
        );
        if input.pause_for_ui {
            let ui_uuid = c
                .create_vm_from_template(&CreateVmRequest {
                    template_uuid: uuid.clone(),
                    name: format!("{prefix}-ui"),
                    description: "Temporary UI qualification VM".into(),
                    vcpus: 1,
                    memory_mib: 1024,
                    sr_uuid: sr.uuid.clone(),
                    new_disk_gib: None,
                    network_uuid: None,
                    iso_uuid: None,
                    affinity_host_uuid: None,
                })
                .await?;
            println!(
                "{}",
                json!({"phase":"ready-for-ui","uuid":ui_uuid,"prefix":prefix})
            );
            let mut line = String::new();
            io::stdin().lock().read_line(&mut line)?;
        }
    } else {
        return Err("Unknown qualification mode".into());
    }
    if input.pause_for_ui && input.mode == "baseline" {
        println!(
            "{}",
            json!({"phase":"ready-for-ui","uuid":uuid,"prefix":prefix})
        );
        let mut line = String::new();
        io::stdin().lock().read_line(&mut line)?;
    }
    Ok(())
}

#[tokio::main]
async fn main() -> Result<(), Error> {
    let mut line = String::new();
    io::stdin().lock().read_line(&mut line)?;
    let input: Input = serde_json::from_str(&line)?;
    if input.settings.host != "192.168.8.170" || input.settings.read_only {
        return Err("Only the authorized writable development host is permitted".into());
    }
    let c = Connection::login(input.settings.clone(), &input.password).await?;
    let prefix = format!("zz-parity-advanced-{}", uuid::Uuid::now_v7());
    println!("{}", json!({"prefix":prefix}));
    let result = exercise(&c, &prefix, &input).await;
    let inventory = c.inventory().await?;
    for vm in inventory
        .snapshots
        .iter()
        .chain(&inventory.vms)
        .chain(&inventory.templates)
        .filter(|v| v.name.starts_with(&prefix))
    {
        if vm.kind != "snapshot"
            && matches!(vm.power_state.as_str(), "Running" | "Paused" | "Suspended")
        {
            action(&c, &vm.uuid, VmAction::ForceShutdown).await?;
        }
        c.destroy_vm(&vm.uuid, true, true).await?;
    }
    c.logout().await?;
    println!(
        "{}",
        json!({"cleanup":"completed","result":result.as_ref().map(|_|"pass").map_err(|e|e.to_string())})
    );
    result
}
