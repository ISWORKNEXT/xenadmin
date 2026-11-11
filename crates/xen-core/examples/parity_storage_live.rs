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
        .create_vm_from_template(&CreateVmRequest {
            template_uuid: template.uuid.clone(),
            name: format!("{prefix}-source"),
            description: "Owned storage qualification VM".into(),
            vcpus: 1,
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
    if input.pause_for_ui {
        println!(
            "{}",
            json!({"phase":"ready-for-ui","prefix":prefix,"vm_uuid":source,"target_sr_uuid":target.uuid,"target_sr_name":target.name,"location":location,"source_sr_uuid":original_sr.uuid})
        );
        let mut line = String::new();
        io::stdin().lock().read_line(&mut line)?;
    }
    Ok(())
}
fn cleanup_graph(prefix: &str, location: &str, baseline: &Value) -> Result<Value, Error> {
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
            json!({"prefix":prefix,"location":location,"baseline":baseline})
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
) -> Result<(), Error> {
    let graph = cleanup_graph(prefix, location, baseline)?;
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
    let result = exercise(&c, &prefix, &location, &input).await;
    let clean = cleanup(&c, &prefix, &location, &baseline).await;
    c.logout().await?;
    clean?;
    let final_inventory = snapshot()?;
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
