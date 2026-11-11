//! Opt-in temporary file-SR and disk workflows on the authorized .170 host.
use serde::Deserialize;
use serde_json::{Value, json};
use std::{
    io::{self, BufRead, Write},
    process::{Command, Stdio},
};
use xen_core::{
    operations::{SrAction, VmAction},
    provisioning::{CopyVmRequest, CreateVmRequest},
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
    snapshot_wizard: bool,
    #[serde(default)]
    qualify_retained_failure: bool,
    #[serde(default)]
    qualify_default_sr: bool,
    #[serde(default)]
    qualify_cpu_settings: bool,
    #[serde(default)]
    qualify_creation_cpu: bool,
}

fn ssh_json(script: &str) -> Result<Value, Error> {
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
            "Owned storage probe failed: {}",
            String::from_utf8_lossy(&output.stderr)
        )
        .into());
    }
    Ok(serde_json::from_slice(&output.stdout)?)
}
fn snapshot() -> Result<Value, Error> {
    ssh_json(
        r#"import XenAPI,json
s=XenAPI.xapi_local();s.xenapi.login_with_password('','','1.0','storage-inventory-baseline')
try:
 print(json.dumps({k:sorted(s.xenapi.__getattr__(c).get_uuid(r) for r in s.xenapi.__getattr__(c).get_all()) for k,c in [('vm_uuids','VM'),('vdi_uuids','VDI'),('sr_uuids','SR'),('pbd_uuids','PBD'),('network_uuids','network')]}))
finally:s.xenapi.session.logout()
"#,
    )
}
fn cpu_state(uuid: &str, prefix: &str, seed: bool) -> Result<Value, Error> {
    let helper =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tools/audit_vm_cpu_state.py");
    let mut child = Command::new("python3")
        .arg(helper)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
    child
        .stdin
        .take()
        .ok_or("Missing CPU probe input")?
        .write_all(
            json!({"vm_uuid":uuid,"prefix":prefix,"seed":seed})
                .to_string()
                .as_bytes(),
        )?;
    let output = child.wait_with_output()?;
    if !output.status.success() {
        return Err(format!(
            "CPU probe failed: {}",
            String::from_utf8_lossy(&output.stderr)
        )
        .into());
    }
    Ok(serde_json::from_slice(&output.stdout)?)
}
fn default_sr_state() -> Result<Value, Error> {
    ssh_json(
        r#"import XenAPI,json
s=XenAPI.xapi_local();s.xenapi.login_with_password('','','1.0','default-sr-independent-read')
try:
 pools=s.xenapi.pool.get_all();assert len(pools)==1;p=pools[0];sr=s.xenapi.pool.get_default_SR(p)
 print(json.dumps({'pool_uuid':s.xenapi.pool.get_uuid(p),'sr_ref':sr,'sr_uuid':s.xenapi.SR.get_uuid(sr)}))
finally:s.xenapi.session.logout()
"#,
    )
}
fn default_sr_journal(prefix: &str, state: &Value, create: bool) -> Result<Value, Error> {
    ssh_json(&r#"import json
from pathlib import Path
c=json.loads(CONFIG);parent=Path('/var/tmp')/c['prefix'];assert c['prefix'].startswith('zz-parity-storage-') and '/' not in c['prefix'] and not parent.is_symlink()
assert json.loads((parent/'owner.json').read_text())=={'prefix':c['prefix']}
p=parent/'default-sr-original.json'
if c['create']:
 with p.open('x') as f:json.dump(c['state'],f)
 p.chmod(0o600)
else:
 assert not p.is_symlink() and json.loads(p.read_text())==c['state'];p.unlink()
print(json.dumps({'journal_created':c['create'],'state':c['state']}))
"#.replace("CONFIG",&serde_json::to_string(&json!({"prefix":prefix,"state":state,"create":create}).to_string())?))
}
fn private_path(prefix: &str, prepare: bool) -> Result<Value, Error> {
    ssh_json(&r#"import json,os
from pathlib import Path
c=json.loads(CONFIG);parent=Path('/var/tmp')/c['prefix'];location=parent/'sr';marker=parent/'owner.json'
assert c['prefix'].startswith('zz-parity-storage-') and '/' not in c['prefix'] and not parent.is_symlink()
if c['prepare']:
 v=os.statvfs('/var/tmp');free=v.f_bavail*v.f_frsize
 assert free>=12*1024**3,'Less than 12 GiB free; no test directory created'
 parent.mkdir(mode=0o700);marker.write_text(json.dumps({'prefix':c['prefix']}));marker.chmod(0o600)
 print(json.dumps({'parent':str(parent),'location':str(location),'free_bytes':free}))
else:
 assert json.loads(marker.read_text())=={'prefix':c['prefix']}
 if location.exists():
  assert location.is_dir() and not location.is_symlink() and not os.path.ismount(str(location)) and not list(location.iterdir())
  location.rmdir()
 marker.unlink();parent.rmdir();print(json.dumps({'private_path_removed':True}))
"#.replace("CONFIG",&serde_json::to_string(&json!({"prefix":prefix,"prepare":prepare}).to_string())?))
}
fn disk_probe(vm_uuid: &str, disk_uuid: &str, prefix: &str, seed: bool) -> Result<Value, Error> {
    ssh_json(&r#"import XenAPI,json,os,time,hashlib
c=json.loads(CONFIG);s=XenAPI.xapi_local();s.xenapi.login_with_password('','','1.0','owned-storage-disk-probe');temporary=None
try:
 vm=s.xenapi.VM.get_by_uuid(c['vm']);v=s.xenapi.VM.get_record(vm)
 assert v['name_label'].startswith(c['prefix']) and v['power_state']=='Halted' and not v['is_control_domain']
 disk=s.xenapi.VDI.get_by_uuid(c['disk']);d=s.xenapi.VDI.get_record(disk)
 assert d['name_label'].startswith(c['prefix']) and 0<int(d['virtual_size'])<=1024**3
 vbds=[s.xenapi.VBD.get_record(r) for r in v['VBDs'] if s.xenapi.VBD.get_VDI(r)==disk]
 assert len(vbds)==1 and vbds[0]['other_config'].get('owner') is not None and not vbds[0]['currently_attached']
 if c['seed']:assert int(d['virtual_size'])==16777216
 dom0=next(r for r,v in s.xenapi.VM.get_all_records().items() if v['is_control_domain']);device=s.xenapi.VM.get_allowed_VBD_devices(dom0)[0]
 temporary=s.xenapi.VBD.create({'VM':dom0,'VDI':disk,'userdevice':device,'bootable':False,'mode':'RW' if c['seed'] else 'RO','type':'Disk','empty':False,'unpluggable':True,'other_config':{},'qos_algorithm_type':'','qos_algorithm_params':{}})
 s.xenapi.VBD.plug(temporary);path='/dev/'+s.xenapi.VBD.get_device(temporary)
 for _ in range(20):
  if os.path.exists(path):break
  time.sleep(.1)
 fd=os.open(path,os.O_RDWR if c['seed'] else os.O_RDONLY)
 try:
  if c['seed']:
   marker=(b'XEN-DESKTOP-STORAGE-PARITY\n'*4096)[:65536];os.lseek(fd,1048576,os.SEEK_SET);assert os.write(fd,marker)==len(marker);os.fsync(fd)
  os.lseek(fd,0,os.SEEK_SET);whole=hashlib.sha256();prefix_hash=hashlib.sha256();size=int(d['virtual_size']);read=0;zero_tail=True
  while read<size:
   chunk=os.read(fd,min(size-read,1048576));assert chunk;whole.update(chunk)
   take=max(0,min(len(chunk),16777216-read));prefix_hash.update(chunk[:take])
   if chunk[take:].strip(b'\0'):zero_tail=False
   read+=len(chunk)
  print(json.dumps({'vm_uuid':v['uuid'],'disk_uuid':d['uuid'],'bytes':size,'sha256':whole.hexdigest(),'prefix_16m_sha256':prefix_hash.hexdigest(),'zero_tail':zero_tail,'sr_uuid':s.xenapi.SR.get_uuid(d['SR'])}))
 finally:os.close(fd)
finally:
 if temporary:
  if s.xenapi.VBD.get_currently_attached(temporary):s.xenapi.VBD.unplug(temporary)
  s.xenapi.VBD.destroy(temporary)
 s.xenapi.session.logout()
"#.replace("CONFIG",&serde_json::to_string(&json!({"vm":vm_uuid,"disk":disk_uuid,"prefix":prefix,"seed":seed}).to_string())?))
}
async fn exercise(
    c: &Connection,
    prefix: &str,
    location: &str,
    input: &Input,
) -> Result<(), Error> {
    let inventory = c.inventory().await?;
    let host = inventory
        .hosts
        .iter()
        .find(|h| h.address == "192.168.8.170")
        .ok_or("Dev host missing")?;
    if inventory.hosts.len() != 1 {
        return Err("This qualification requires the known single-host development pool".into());
    }
    let original_sr = inventory
        .storage
        .iter()
        .find(|s| s.name == "Local storage" && s.pbd_attached > 0)
        .ok_or("Original SR missing")?;
    c.create_sr(
        &host.uuid,
        &format!("{prefix}-sr"),
        "Temporary owned storage qualification",
        location,
    )
    .await?;
    let created = c.inventory().await?;
    let target = created
        .storage
        .iter()
        .find(|s| {
            s.name == format!("{prefix}-sr") && s.storage_type == "file" && s.pbd_attached == 1
        })
        .ok_or("New file SR did not attach")?;
    println!(
        "{}",
        json!({"check":"create-file-sr","uuid":target.uuid,"location":location,"host_uuid":host.uuid,"status":"VERIFIED"})
    );
    let template = inventory
        .templates
        .iter()
        .find(|v| v.name == "Other install media")
        .ok_or("Blank template missing")?;
    let source = c
        .create_configured_vm(&CreateVmRequest {
            guest_setup: None,
            start_automatically: false,
            source: xen_core::provisioning::VmCreationSource::Template(template.uuid.clone()),
            name: format!("{prefix}-source"),
            description: "Owned storage qualification VM".into(),
            vcpus_max: 1,
            vcpus_at_startup: 1,
            cores_per_socket: None,
            memory_mib: 256,
            sr_uuid: original_sr.uuid.clone(),
            new_disk_gib: None,
            network_uuid: None,
            iso_uuid: None,
            affinity_host_uuid: None,
        })
        .await?;
    c.add_disk(
        &source,
        &original_sr.uuid,
        16777216,
        Some(&format!("{prefix}-marker")),
    )
    .await?;
    let details = c.vm_details(&source).await?;
    let data = details
        .disks
        .iter()
        .find(|d| !d.is_cd)
        .ok_or("Marker disk missing")?;
    let baseline = disk_probe(&source, &data.vdi_uuid, prefix, true)?;
    let copy = c
        .copy_vm(
            &source,
            &CopyVmRequest {
                name: format!("{prefix}-copy"),
                description: "Independent copy on another SR".into(),
                sr_uuid: target.uuid.clone(),
            },
        )
        .await?;
    let copied = c.vm_details(&copy).await?;
    let disk = copied
        .disks
        .iter()
        .find(|d| !d.is_cd)
        .ok_or("Copied disk missing")?;
    let readback = disk_probe(&copy, &disk.vdi_uuid, prefix, false)?;
    if disk.vdi_uuid == data.vdi_uuid
        || readback["sha256"] != baseline["sha256"]
        || readback["sr_uuid"] != target.uuid
    {
        return Err("Full copy to another SR differs".into());
    }
    println!(
        "{}",
        json!({"check":"full-copy-vm-other-sr","source_uuid":source,"copy_uuid":copy,"readback":readback,"status":"VERIFIED"})
    );
    c.convert_vm_to_template(&copy).await?;
    let template_copy = c
        .copy_vm(
            &copy,
            &CopyVmRequest {
                name: format!("{prefix}-template-copy"),
                description: "Template copied back to original SR".into(),
                sr_uuid: original_sr.uuid.clone(),
            },
        )
        .await?;
    let template_details = c.vm_details(&template_copy).await?;
    let disk = template_details
        .disks
        .iter()
        .find(|d| !d.is_cd)
        .ok_or("Template copy disk missing")?;
    let readback = disk_probe(&template_copy, &disk.vdi_uuid, prefix, false)?;
    if !template_details.is_template
        || readback["sha256"] != baseline["sha256"]
        || readback["sr_uuid"] != original_sr.uuid
    {
        return Err("Template copy to another SR differs".into());
    }
    println!(
        "{}",
        json!({"check":"full-copy-template-other-sr","uuid":template_copy,"readback":readback,"status":"VERIFIED"})
    );
    let mut disk = data.clone();
    for destination in [&target.uuid, &original_sr.uuid] {
        c.move_disk(&source, &disk.vbd_uuid, destination).await?;
        let details = c.vm_details(&source).await?;
        let moved = details
            .disks
            .iter()
            .find(|d| !d.is_cd)
            .ok_or("Moved disk missing")?;
        let readback = disk_probe(&source, &moved.vdi_uuid, prefix, false)?;
        if moved.vdi_uuid == disk.vdi_uuid
            || readback["sha256"] != baseline["sha256"]
            || &moved.sr_uuid != destination
            || moved.device != disk.device
            || moved.bootable != disk.bootable
            || moved.mode != disk.mode
            || !moved.owner
        {
            return Err("Offline move lost data or VBD properties".into());
        }
        println!(
            "{}",
            json!({"check":"offline-disk-move","uuid":source,"old_disk_uuid":disk.vdi_uuid,"readback":readback,"status":"VERIFIED"})
        );
        disk = moved.clone();
    }
    c.resize_disk(&source, &disk.vdi_uuid, 33554432).await?;
    let readback = disk_probe(&source, &disk.vdi_uuid, prefix, false)?;
    if readback["bytes"] != 33554432
        || readback["prefix_16m_sha256"] != baseline["sha256"]
        || readback["zero_tail"] != true
    {
        return Err("Offline resize did not preserve prefix and zero-fill growth".into());
    }
    println!(
        "{}",
        json!({"check":"offline-resize","readback":readback,"status":"VERIFIED"})
    );
    let pbd = c
        .pbd_catalog(&target.uuid)
        .await?
        .into_iter()
        .next()
        .ok_or("File SR PBD missing")?;
    c.set_pbd_plugged(&pbd.uuid, false).await?;
    if c.pbd_catalog(&target.uuid)
        .await?
        .iter()
        .any(|p| p.attached)
    {
        return Err("PBD did not detach".into());
    }
    if input.qualify_default_sr
        && c.sr_action(&target.uuid, SrAction::SetDefault)
            .await
            .is_ok()
    {
        return Err("Detached storage was admitted as default".into());
    }
    if c.repair_sr(&target.uuid).await? != 1
        || c.pbd_catalog(&target.uuid)
            .await?
            .iter()
            .any(|p| !p.attached)
    {
        return Err("SR repair did not reattach PBD".into());
    }
    c.sr_action(&target.uuid, SrAction::Scan).await?;
    println!(
        "{}",
        json!({"check":"pbd-detach-repair-scan","sr_uuid":target.uuid,"pbd_uuid":pbd.uuid,"status":"VERIFIED"})
    );
    if input.qualify_cpu_settings {
        let original = cpu_state(&source, prefix, true)?;
        let mut update = xen_core::operations::VmResourceUpdate {
            pv_args: None,
            auto_poweron: None,
            vcpus_max: 4,
            vcpus_at_startup: 2,
            memory_static_max_mib: 256,
            memory_dynamic_min_mib: 256,
            memory_dynamic_max_mib: 256,
            boot_order: None,
            firmware: None,
            secure_boot: None,
            boot_policy: None,
            after_crash: None,
            cpu_weight: Some(512),
            cores_per_socket: Some(2),
        };
        c.update_vm_resources(&source, &update).await?;
        let actual = cpu_state(&source, prefix, false)?;
        let mut expected = original.clone();
        expected["VCPUs_max"] = json!("4");
        expected["VCPUs_at_startup"] = json!("2");
        expected["VCPUs_params"]["weight"] = json!("512");
        expected["platform"]["cores-per-socket"] = json!("2");
        if actual != expected {
            return Err("CPU settings changed unrelated fields or did not apply".into());
        }
        let mut invalid = update.clone();
        invalid.vcpus_max = 8;
        invalid.cpu_weight = Some(0);
        if c.update_vm_resources(&source, &invalid).await.is_ok()
            || cpu_state(&source, prefix, false)? != actual
        {
            return Err("Invalid weight was applied or other fields changed".into());
        }
        invalid.cpu_weight = Some(512);
        invalid.cores_per_socket = Some(3);
        if c.update_vm_resources(&source, &invalid).await.is_ok()
            || cpu_state(&source, prefix, false)? != actual
        {
            return Err("Invalid topology was applied or other fields changed".into());
        }
        invalid.vcpus_max = 3;
        invalid.cpu_weight = None;
        invalid.cores_per_socket = None;
        if c.update_vm_resources(&source, &invalid).await.is_ok()
            || cpu_state(&source, prefix, false)? != actual
        {
            return Err("CPU maximum bypassed unchanged topology validation".into());
        }
        update.cpu_weight = None;
        update.cores_per_socket = None;
        c.update_vm_resources(&source, &update).await?;
        if cpu_state(&source, prefix, false)? != actual {
            return Err("Omitted CPU options changed maps".into());
        }
        update.cpu_weight = Some(1024);
        c.update_vm_resources(&source, &update).await?;
        let mut expected = actual;
        expected["VCPUs_params"]["weight"] = json!("1024");
        if cpu_state(&source, prefix, false)? != expected {
            return Err("Weight-only update changed topology or other fields".into());
        }
        let mut readonly_settings = input.settings.clone();
        readonly_settings.read_only = true;
        let readonly = Connection::login(readonly_settings, &input.password).await?;
        let rejected = readonly
            .update_vm_resources(&source, &update)
            .await
            .is_err();
        readonly.logout().await?;
        if !rejected {
            return Err("Read-only CPU settings update admitted".into());
        }
        let start = c.begin_vm_action(&source, &VmAction::StartPaused).await?;
        c.wait_task(&start).await?;
        let unpause = c.begin_vm_action(&source, &VmAction::Unpause).await?;
        c.wait_task(&unpause).await?;
        let running = cpu_state(&source, prefix, false)?;
        if running["power_state"] != "Running" {
            return Err("Owned CPU test VM is not Running".into());
        }
        let mut expected_running = expected.clone();
        expected_running["power_state"] = json!("Running");
        let mut host_added_platform = serde_json::Map::new();
        for key in ["device-model", "timeoffset"] {
            if expected_running["platform"].get(key).is_none()
                && let Some(value) = running["platform"].get(key)
            {
                expected_running["platform"][key] = value.clone();
                host_added_platform.insert(key.into(), value.clone());
            }
        }
        if running != expected_running {
            println!(
                "{}",
                json!({"phase":"cpu-state-after-start-difference","expected":expected_running,"actual":running})
            );
            return Err("Starting the VM changed fields beyond the observed host defaults".into());
        }
        update.cpu_weight = Some(2048);
        if c.update_vm_resources(&source, &update).await.is_ok()
            || cpu_state(&source, prefix, false)? != running
        {
            return Err("Running CPU weight update changed fields".into());
        }
        update.cpu_weight = None;
        update.cores_per_socket = Some(1);
        if c.update_vm_resources(&source, &update).await.is_ok()
            || cpu_state(&source, prefix, false)? != running
        {
            return Err("Running CPU topology update changed fields".into());
        }
        let stop = c.begin_vm_action(&source, &VmAction::ForceShutdown).await?;
        c.wait_task(&stop).await?;
        let final_state = cpu_state(&source, prefix, false)?;
        let mut expected = expected_running;
        expected["power_state"] = json!("Halted");
        if final_state != expected {
            println!(
                "{}",
                json!({"phase":"cpu-state-after-power-cycle-difference","expected":expected,"actual":final_state})
            );
            return Err("Halted CPU settings differ after Running rejection".into());
        }
        println!(
            "{}",
            json!({"check":"cpu-topology-weight","uuid":source,"initial":original,"final":final_state,"invalid_weight_rejected":true,"invalid_topology_rejected":true,"readonly_rejected":true,"running_changes_rejected":true,"omitted_options_preserved":true,"unrelated_maps_metadata_preserved":true,"host_added_platform_on_start":host_added_platform,"inherited_topology_checked_on_cpu_change":true,"status":"VERIFIED"})
        );
    }
    if input.qualify_default_sr {
        if let Some(iso) = inventory.storage.iter().find(|sr| sr.content_type == "iso")
            && c.sr_action(&iso.uuid, SrAction::SetDefault).await.is_ok()
        {
            return Err("ISO storage was admitted as default".into());
        }
        c.sr_action(&target.uuid, SrAction::SetDefault).await?;
        c.sr_action(&target.uuid, SrAction::SetDefault).await?;
        let mut readonly_settings = input.settings.clone();
        readonly_settings.read_only = true;
        let readonly = Connection::login(readonly_settings, &input.password).await?;
        let rejected = readonly
            .sr_action(&original_sr.uuid, SrAction::SetDefault)
            .await
            .is_err();
        readonly.logout().await?;
        if !rejected {
            return Err("Read-only default change was admitted".into());
        }
        let actual = default_sr_state()?;
        let refreshed = c.inventory().await?;
        if actual["sr_uuid"] != target.uuid
            || !refreshed
                .storage
                .iter()
                .any(|sr| sr.uuid == target.uuid && sr.is_default)
        {
            return Err("Pool default SR readback differs".into());
        }
        println!(
            "{}",
            json!({"check":"set-default-sr","actual":actual,"idempotent_retry":true,"readonly_rejected":true,"detached_rejected":true,"iso_rejected":inventory.storage.iter().any(|sr|sr.content_type=="iso"),"status":"VERIFIED"})
        );
    }
    let mut snapshot_uuid = None;
    if input.snapshot_wizard {
        let creation_cpu_source = if input.qualify_creation_cpu {
            c.update_vm_resources(
                &source,
                &xen_core::operations::VmResourceUpdate {
                    pv_args: None,
                    auto_poweron: None,
                    vcpus_max: 4,
                    vcpus_at_startup: 2,
                    memory_static_max_mib: 256,
                    memory_dynamic_min_mib: 256,
                    memory_dynamic_max_mib: 256,
                    boot_order: None,
                    firmware: None,
                    secure_boot: None,
                    boot_policy: None,
                    after_crash: None,
                    cpu_weight: None,
                    cores_per_socket: Some(2),
                },
            )
            .await?;
            Some(cpu_state(&source, prefix, false)?)
        } else {
            None
        };
        let task = c
            .begin_vm_action(
                &source,
                &VmAction::Snapshot {
                    name: format!("{prefix}-snapshot"),
                },
            )
            .await?;
        c.wait_task(&task).await?;
        let state = c.inventory().await?;
        let snapshot = state
            .snapshots
            .iter()
            .find(|vm| vm.name == format!("{prefix}-snapshot"))
            .ok_or("Snapshot missing")?;
        let mut request = CreateVmRequest {
            guest_setup: None,
            start_automatically: false,
            source: xen_core::provisioning::VmCreationSource::Snapshot(snapshot.uuid.clone()),
            name: format!("{prefix}-configured"),
            description: "Configured from an owned snapshot".into(),
            vcpus_max: if input.qualify_creation_cpu { 6 } else { 2 },
            vcpus_at_startup: if input.qualify_creation_cpu { 3 } else { 2 },
            cores_per_socket: input.qualify_creation_cpu.then_some(1),
            memory_mib: 384,
            sr_uuid: target.uuid.clone(),
            new_disk_gib: None,
            network_uuid: inventory
                .networks
                .iter()
                .find(|n| n.bridge != "xenapi")
                .map(|n| n.uuid.clone()),
            iso_uuid: None,
            affinity_host_uuid: Some(host.uuid.clone()),
        };
        // A template-typed UUID must never silently admit a snapshot.
        request.source = xen_core::provisioning::VmCreationSource::Template(snapshot.uuid.clone());
        if c.create_configured_vm(&request).await.is_ok() {
            return Err("Wrong source kind was admitted".into());
        }
        request.source = xen_core::provisioning::VmCreationSource::Snapshot(snapshot.uuid.clone());
        if input.qualify_creation_cpu {
            let before = crate::snapshot()?;
            for (maximum, initial, cores) in [
                (6, 0, Some(1)),
                (6, 7, Some(1)),
                (6, 3, Some(4)),
                (3, 2, None),
            ] {
                request.vcpus_max = maximum;
                request.vcpus_at_startup = initial;
                request.cores_per_socket = cores;
                if c.create_configured_vm(&request).await.is_ok() || crate::snapshot()? != before {
                    return Err("Invalid creation CPU request wrote objects".into());
                }
            }
            request.vcpus_max = 6;
            request.vcpus_at_startup = 3;
            request.cores_per_socket = Some(1);
            println!(
                "{}",
                json!({"check":"creation-cpu-invalid-preclone","zero_initial_rejected":true,"initial_above_max_rejected":true,"nondividing_topology_rejected":true,"invalid_inherited_topology_rejected":true,"all_inventory_sets_unchanged":true,"status":"VERIFIED"})
            );
        }
        let output = c.create_configured_vm(&request).await?;
        let details = c.vm_details(&output).await?;
        let output_disk = details
            .disks
            .iter()
            .find(|d| !d.is_cd)
            .ok_or("Configured disk missing")?;
        let probe = disk_probe(&output, &output_disk.vdi_uuid, prefix, false)?;
        if details.is_template
            || details.is_snapshot
            || details.power_state != "Halted"
            || details.vcpus_at_startup != request.vcpus_at_startup
            || details.vcpus_max != request.vcpus_max
            || request
                .cores_per_socket
                .is_some_and(|cores| details.cores_per_socket != cores.to_string())
            || details.memory_static_max != 384 * 1024 * 1024
            || details.memory_dynamic_min != 384 * 1024 * 1024
            || details.memory_dynamic_max != 384 * 1024 * 1024
            || details.affinity_ref != host.reference
            || details
                .interfaces
                .iter()
                .any(|interface| Some(&interface.network_uuid) != request.network_uuid.as_ref())
            || details.description != request.description
            || probe["sha256"] != readback["sha256"]
            || probe["sr_uuid"] != target.uuid
            || details.interfaces.len() != usize::from(request.network_uuid.is_some())
        {
            return Err(
                "Configured snapshot output does not match resources, destination or data".into(),
            );
        }
        println!(
            "{}",
            json!({"check":"configured-snapshot-vm","source_snapshot_uuid":snapshot.uuid,"uuid":output,"readback":probe,"vcpus_max":request.vcpus_max,"vcpus_at_startup":request.vcpus_at_startup,"cores_per_socket":details.cores_per_socket,"memory_mib":384,"network_uuid":request.network_uuid,"affinity_host_uuid":request.affinity_host_uuid,"wrong_source_kind_rejected":true,"status":"VERIFIED"})
        );
        if let Some(before) = creation_cpu_source {
            if cpu_state(&source, prefix, false)? != before {
                return Err("Creation changed source CPU/maps/metadata".into());
            }
            println!(
                "{}",
                json!({"check":"creation-cpu-source-preserved","state":before,"exact_match":true,"status":"VERIFIED"})
            );
        }
        if input.qualify_retained_failure {
            request.name = format!("{prefix}-failed-config");
            request.vcpus_max = u64::MAX;
            let failure = c
                .create_configured_vm(&request)
                .await
                .err()
                .ok_or("Oversized CPU count unexpectedly accepted")?
                .to_string();
            let after = c.inventory().await?;
            let retained = after
                .vms
                .iter()
                .find(|vm| vm.name == request.name)
                .ok_or_else(||format!("No failed output was found. Creation returned: {failure}. If preflight rejected the request before cloning, output retention was not exercised."))?;
            if !failure.contains(&retained.uuid) || !failure.contains("retained") {
                return Err("Failure did not identify retained VM".into());
            }
            let retained_details = c.vm_details(&retained.uuid).await?;
            let retained_disk = retained_details
                .disks
                .iter()
                .find(|d| !d.is_cd)
                .ok_or("Failed output lost its disk")?;
            let retained_probe =
                disk_probe(&retained.uuid, &retained_disk.vdi_uuid, prefix, false)?;
            if retained_probe["sha256"] != readback["sha256"] {
                return Err("Retained output lost data".into());
            }
            println!(
                "{}",
                json!({"check":"post-clone-failure-retains-output","uuid":retained.uuid,"message":failure,"readback":retained_probe,"status":"VERIFIED"})
            );
        }
        snapshot_uuid = Some(snapshot.uuid.clone());
    }
    if input.pause_for_ui {
        println!(
            "{}",
            json!({"phase":"ready-for-ui","prefix":prefix,"vm_uuid":source,"snapshot_uuid":snapshot_uuid,"target_sr_uuid":target.uuid,"target_sr_name":target.name,"location":location,"source_sr_uuid":original_sr.uuid})
        );
        let mut line = String::new();
        io::stdin().lock().read_line(&mut line)?;
    }
    Ok(())
}
fn cleanup_graph(
    prefix: &str,
    location: &str,
    baseline: &Value,
    allow_snapshots: bool,
) -> Result<Value, Error> {
    let helper = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tools/audit_storage_cleanup_graph.py");
    let mut child = Command::new("python3")
        .arg(helper)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
    child
        .stdin
        .take()
        .ok_or("Missing cleanup admission input")?
        .write_all(
            json!({"prefix":prefix,"location":location,"baseline":baseline,"allow_snapshots":allow_snapshots})
                .to_string()
                .as_bytes(),
        )?;
    let output = child.wait_with_output()?;
    if !output.status.success() {
        return Err(format!(
            "Cleanup graph was not admitted: {}",
            String::from_utf8_lossy(&output.stderr)
        )
        .into());
    }
    Ok(serde_json::from_slice(&output.stdout)?)
}

async fn cleanup(
    c: &Connection,
    prefix: &str,
    location: &str,
    baseline: &Value,
    allow_snapshots: bool,
) -> Result<(), Error> {
    let graph = cleanup_graph(prefix, location, baseline, allow_snapshots)?;
    println!("{}", json!({"phase":"cleanup-admission","graph":graph}));
    let disk_ids = graph["admitted_vdi_uuids"]
        .as_array()
        .ok_or("Missing admitted disks")?;
    for vm in graph["vms"].as_array().ok_or("Missing admitted VMs")? {
        let uuid = vm["uuid"].as_str().ok_or("Missing admitted VM UUID")?;
        if matches!(
            vm["power_state"].as_str(),
            Some("Running" | "Paused" | "Suspended")
        ) {
            let task = c.begin_vm_action(uuid, &VmAction::ForceShutdown).await?;
            c.wait_task(&task).await?;
        }
        c.destroy_vm(uuid, true, false).await?;
    }
    for sr in graph["srs"].as_array().ok_or("Missing admitted SRs")? {
        let uuid = sr["uuid"].as_str().ok_or("Missing admitted SR UUID")?;
        for disk in c.sr_vdis(uuid).await? {
            if !disk_ids.iter().any(|id| id == &disk.uuid) {
                return Err("An unadmitted disk appeared; no automatic SR cleanup".into());
            }
            c.delete_vdi(&disk.uuid).await?;
        }
        c.sr_action(uuid, SrAction::Destroy).await?;
    }
    private_path(prefix, false)?;
    Ok(())
}
#[tokio::main]
async fn main() -> Result<(), Error> {
    let mut line = String::new();
    io::stdin().lock().read_line(&mut line)?;
    let input: Input = serde_json::from_str(&line)?;
    if input.settings.host != "192.168.8.170" || input.settings.read_only {
        return Err("Only authorized writable .170 is permitted".into());
    }
    let prefix = format!("zz-parity-storage-{}", uuid::Uuid::now_v7());
    let baseline = snapshot()?;
    println!(
        "{}",
        json!({"phase":"baseline","prefix":prefix,"inventory":baseline})
    );
    let path = private_path(&prefix, true)?;
    let location = path["location"]
        .as_str()
        .ok_or("Missing location")?
        .to_owned();
    println!("{}", json!({"phase":"private-path-ready","path":path}));
    let c = Connection::login(input.settings.clone(), &input.password).await?;
    let original_default = if input.qualify_default_sr {
        let state = default_sr_state()?;
        default_sr_journal(&prefix, &state, true)?;
        println!("{}", json!({"phase":"original-default-sr","state":state}));
        Some(state)
    } else {
        None
    };
    let result = exercise(&c, &prefix, &location, &input).await;
    if let Some(original) = original_default {
        c.sr_action(
            original["sr_uuid"]
                .as_str()
                .ok_or("Original default missing")?,
            SrAction::SetDefault,
        )
        .await?;
        if default_sr_state()? != original {
            return Err("Default SR restore differs; journal retained; no cleanup".into());
        }
        default_sr_journal(&prefix, &original, false)?;
        println!(
            "{}",
            json!({"phase":"default-sr-restored","state":original,"exact_match":true})
        );
    }
    let clean = cleanup(&c, &prefix, &location, &baseline, input.snapshot_wizard).await;
    c.logout().await?;
    clean?;
    let mut final_inventory = snapshot()?;
    // Snapshot GC can briefly leave new, unattached VDI records after destroy.
    // Only wait for extra VDIs: missing baseline disks or other object drift fails.
    for attempt in 0..20 {
        if final_inventory == baseline {
            break;
        }
        let only_extra_vdis = ["vm_uuids", "sr_uuids", "pbd_uuids", "network_uuids"]
            .iter()
            .all(|key| final_inventory[key] == baseline[key])
            && baseline["vdi_uuids"].as_array().is_some_and(|original| {
                original.iter().all(|id| {
                    final_inventory["vdi_uuids"]
                        .as_array()
                        .is_some_and(|actual| actual.contains(id))
                })
            });
        if !only_extra_vdis {
            break;
        }
        println!(
            "{}",
            json!({"phase":"waiting-for-vdi-record-settle","attempt":attempt+1,"actual":final_inventory["vdi_uuids"]})
        );
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
        final_inventory = snapshot()?;
    }
    if final_inventory != baseline {
        println!(
            "{}",
            json!({"phase":"cleanup-inventory-mismatch","baseline":baseline,"actual":final_inventory})
        );
        return Err("Storage qualification cleanup inventory differs".into());
    }
    println!(
        "{}",
        json!({"phase":"cleanup","all_inventory_sets_match":true,"vm_count":baseline["vm_uuids"].as_array().map(Vec::len),"vdi_count":baseline["vdi_uuids"].as_array().map(Vec::len),"sr_count":baseline["sr_uuids"].as_array().map(Vec::len),"pbd_count":baseline["pbd_uuids"].as_array().map(Vec::len),"network_count":baseline["network_uuids"].as_array().map(Vec::len),"result":result.as_ref().map(|_|"pass").map_err(|e|e.to_string())})
    );
    result
}
