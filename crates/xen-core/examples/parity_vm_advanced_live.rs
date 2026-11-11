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
        source: xen_core::provisioning::VmCreationSource::Template(template.uuid.clone()),
        name: format!("{prefix}-source"),
        description: "D".repeat(2000),
        vcpus_max: 1,
        vcpus_at_startup: 1,
        cores_per_socket: None,
        memory_mib: 1024,
        sr_uuid: sr.uuid.clone(),
        new_disk_gib: None,
        network_uuid: None,
        iso_uuid: None,
        affinity_host_uuid: None,
    };
    let source = c.create_configured_vm(&request).await?;
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
        pv_args: None,
        auto_poweron: None,
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
        cpu_weight: None,
        cores_per_socket: None,
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
    request.source = xen_core::provisioning::VmCreationSource::Template(source.clone());
    request.name = format!("{prefix}-legacy-child");
    request.description = "L".repeat(40000);
    let child = c.create_configured_vm(&request).await?;
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
        .create_configured_vm(&request)
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

fn disk_hash(uuid: &str, prefix: &str, write: bool) -> Result<String, Error> {
    let config = json!({"uuid":uuid,"prefix":prefix,"write":write});
    let script=r#"
import XenAPI,json,os,hashlib,time
C=json.loads(CONFIG)
s=XenAPI.xapi_local();s.xenapi.login_with_password('','','1.0','full-copy-disk-qualification')
vbd=None
try:
 r=s.xenapi.VDI.get_by_uuid(C['uuid']);disk=s.xenapi.VDI.get_record(r)
 assert disk['name_label'].startswith(C['prefix']) and int(disk['virtual_size'])==16777216
 domains=s.xenapi.VM.get_all_records();dom0=next(r for r,v in domains.items() if v['is_control_domain'])
 device=s.xenapi.VM.get_allowed_VBD_devices(dom0)[0]
 vbd=s.xenapi.VBD.create({'VM':dom0,'VDI':r,'userdevice':device,'bootable':False,'mode':'RW' if C['write'] else 'RO','type':'Disk','empty':False,'unpluggable':True,'other_config':{},'qos_algorithm_type':'','qos_algorithm_params':{}})
 s.xenapi.VBD.plug(vbd);path='/dev/'+s.xenapi.VBD.get_device(vbd)
 for i in range(20):
  if os.path.exists(path):break
  time.sleep(.1)
 fd=os.open(path,os.O_RDWR if C['write'] else os.O_RDONLY)
 try:
  if C['write']:
   marker=(b'XEN-DESKTOP-FULL-COPY\n'*4096)[:65536]
   os.lseek(fd,1048576,os.SEEK_SET);assert os.write(fd,marker)==len(marker);os.fsync(fd)
  os.lseek(fd,0,os.SEEK_SET);h=hashlib.sha256();left=int(disk['virtual_size'])
  while left:
   chunk=os.read(fd,min(left,1048576));assert chunk;left-=len(chunk);h.update(chunk)
  print(json.dumps({'sha256':h.hexdigest(),'bytes':int(disk['virtual_size'])}))
 finally:os.close(fd)
finally:
 if vbd:
  if s.xenapi.VBD.get_currently_attached(vbd):s.xenapi.VBD.unplug(vbd)
  s.xenapi.VBD.destroy(vbd)
 s.xenapi.session.logout()
"#.replace("CONFIG",&serde_json::to_string(&config.to_string())?);
    let mut child = Command::new("ssh")
        .args(["-o", "BatchMode=yes", "root@192.168.8.170", "python3", "-"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
    child
        .stdin
        .take()
        .ok_or("No SSH input")?
        .write_all(script.as_bytes())?;
    let output = child.wait_with_output()?;
    if !output.status.success() {
        return Err(format!(
            "Owned test disk probe failed: {}",
            String::from_utf8_lossy(&output.stderr)
        )
        .into());
    }
    let result: serde_json::Value = serde_json::from_slice(&output.stdout)?;
    result["sha256"]
        .as_str()
        .map(str::to_owned)
        .ok_or_else(|| "Missing disk checksum".into())
}

fn attach_foreign_copy_disk(copy_uuid: &str, disk_uuid: &str, attach: bool) -> Result<(), Error> {
    let script=r#"
import XenAPI,json
s=XenAPI.xapi_local();s.xenapi.login_with_password('','','1.0','owned-copy-external-layout-check')
try:
 vm=s.xenapi.VM.get_by_uuid(UUID);v=s.xenapi.VM.get_record(vm)
 disk=s.xenapi.VDI.get_by_uuid(DISK);d=s.xenapi.VDI.get_record(disk)
 assert v['name_label'].startswith('zz-parity-advanced-') and v['power_state']=='Halted' and not v['is_a_template']
 assert d['name_label'].startswith('zz-parity-advanced-') and int(d['virtual_size'])==16777216
 if ATTACH:
  assert all(s.xenapi.VBD.get_VDI(r)!=disk for r in v['VBDs'])
  device=s.xenapi.VM.get_allowed_VBD_devices(vm)[0]
  s.xenapi.VBD.create({'VM':vm,'VDI':disk,'userdevice':device,'bootable':False,'mode':'RO','type':'Disk','empty':False,'unpluggable':True,'other_config':{'parity-foreign-test':'true'},'qos_algorithm_type':'','qos_algorithm_params':{}})
 else:
  found=[r for r in v['VBDs'] if s.xenapi.VBD.get_VDI(r)==disk]
  assert len(found)==1
  r=found[0];b=s.xenapi.VBD.get_record(r);assert not b['currently_attached'] and b['other_config'].get('parity-foreign-test')=='true'
  s.xenapi.VBD.destroy(r)
 assert s.xenapi.VDI.get_uuid(disk)==DISK
 print(json.dumps({'check':'owned-foreign-disk-attachment' if ATTACH else 'owned-foreign-disk-detachment','copy_uuid':v['uuid'],'disk_uuid':d['uuid'],'disk_still_exists':True}))
finally:s.xenapi.session.logout()
"#.replace("UUID",&serde_json::to_string(copy_uuid)?).replace("DISK",&serde_json::to_string(disk_uuid)?).replace("ATTACH",if attach {"True"} else {"False"});
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
            "Owned foreign-disk setup failed: {}",
            String::from_utf8_lossy(&output.stderr)
        )
        .into());
    }
    print!("{}", String::from_utf8(output.stdout)?);
    Ok(())
}

async fn workspace_copy_recovery(
    c: &Connection,
    source: &str,
    request: &xen_core::provisioning::CopyVmRequest,
    prefix: &str,
    input: &Input,
    hash: &str,
) -> Result<(), Error> {
    use xen_core::{operations::TaskStatus, workspace::Workspace};
    let mut proxy = Command::new("python3")
        .arg("tools/audit_copy_fault_proxy.py")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
    let mut output = io::BufReader::new(proxy.stdout.take().ok_or("Missing relay output")?);
    let mut line = String::new();
    output.read_line(&mut line)?;
    let setup: serde_json::Value = serde_json::from_str(&line)?;
    let port = setup["port"].as_u64().ok_or("Relay did not start")?;
    let workspace = Workspace::with_profiles(vec![]);
    let mut healthy = None;
    let result:Result<(),Error>=async {
        let mut settings=input.settings.clone();settings.proxy=Some(format!("http://127.0.0.1:{port}"));
        let connection_id=workspace.connect(settings,&input.password,None).await?;
        let mut request=request.clone();request.name=format!("{prefix}-workspace-reconciled");
        let operation=workspace.start_action(connection_id.clone(),source.to_owned(),VmAction::Copy{request:request.clone()}).await?;
        let mut interrupted=None;
        for _ in 0..120 {
            let view=workspace.view().await;
            let task=view.tasks.iter().find(|t|t.id==operation.id).ok_or("Workspace copy task missing")?;
            if task.status==TaskStatus::Unreachable {interrupted=Some(task.clone());break;}
            if task.status.terminal() {return Err(format!("Workspace copy did not retain interruption: {}",task.message).into());}
            tokio::time::sleep(std::time::Duration::from_millis(500)).await;
        }
        let interrupted=interrupted.ok_or("Workspace did not observe copy interruption")?;
        if interrupted.remote_ref.is_empty() {return Err("Workspace lost remote copy reference".into());}
        line.clear();output.read_line(&mut line)?;
        let event:serde_json::Value=serde_json::from_str(&line)?;
        if event["task"]["task_ref"].as_str()!=Some(interrupted.remote_ref.as_str()) {return Err("Workspace relay interrupted a different task".into());}
        if workspace.start_action(connection_id.clone(),source.to_owned(),VmAction::Copy{request:request.clone()}).await.is_ok() || workspace.disconnect(&connection_id).await.is_ok() {return Err("Interrupted source lost duplicate/disconnect guards".into());}
        println!("{}",json!({"check":"workspace-retains-interrupted-copy","operation_id":operation.id,"remote_ref":interrupted.remote_ref,"duplicate_rejected":true,"disconnect_rejected":true,"status":"VERIFIED"}));
        proxy.wait()?;
        let mut restored=Command::new("python3").args(["tools/audit_copy_fault_proxy.py","--no-fault","--port",&port.to_string()]).stdout(Stdio::piped()).stderr(Stdio::piped()).spawn()?;
        let mut ready=io::BufReader::new(restored.stdout.take().ok_or("Missing restored relay output")?);
        line.clear();ready.read_line(&mut line)?;
        let restored_setup:serde_json::Value=serde_json::from_str(&line)?;
        healthy=Some(restored);
        if restored_setup["port"].as_u64()!=Some(port) {return Err("Relay did not restore the same endpoint".into());}
        workspace.reconnect_connection(&connection_id).await?;
        if workspace.start_action(connection_id.clone(),source.to_owned(),VmAction::Copy{request:request.clone()}).await.is_ok() || workspace.disconnect(&connection_id).await.is_ok() {return Err("Reconnect dropped interrupted-copy reservations".into());}
        println!("{}",json!({"check":"workspace-reconnect-retains-copy-reservation","operation_id":operation.id,"duplicate_rejected":true,"disconnect_rejected":true,"status":"VERIFIED"}));
        workspace.check_task(&operation.id).await?;
        let mut completed=false;
        for _ in 0..120 {
            let view=workspace.view().await;
            let task=view.tasks.iter().find(|t|t.id==operation.id).ok_or("Reconciled task missing")?;
            if task.status==TaskStatus::Succeeded {completed=true;break;}
            if task.status==TaskStatus::Unreachable || task.status==TaskStatus::Failed {return Err(format!("Workspace recovery did not finish: {}",task.message).into());}
            tokio::time::sleep(std::time::Duration::from_millis(500)).await;
        }
        if !completed {return Err("Workspace recovery timed out".into());}
        let inventory=c.inventory().await?;
        let copies:Vec<_>=inventory.vms.iter().filter(|v|v.name==request.name).collect();
        if copies.len()!=1 {return Err("Workspace recovery duplicated the destination".into());}
        let uuid=copies[0].uuid.clone();let details=c.vm_details(&uuid).await?;
        let disk=details.disks.iter().find(|d|!d.is_cd).ok_or("Workspace copy disk missing")?;
        if details.description!=request.description || disk_hash(&disk.vdi_uuid,prefix,false)?!=hash {return Err("Workspace copy readback differs".into());}
        println!("{}",json!({"check":"workspace-check-task-reconciles-existing-copy","operation_id":operation.id,"copy_uuid":uuid,"matching_destination_count":1,"sha256":hash,"status":"VERIFIED"}));
        Ok(())
    }.await;
    workspace.close().await;
    let _ = proxy.kill();
    let _ = proxy.wait();
    if let Some(mut restored) = healthy {
        let _ = restored.kill();
        let _ = restored.wait();
    }
    result
}

async fn full_copy(c: &Connection, prefix: &str, input: &Input) -> Result<(), Error> {
    use xen_core::{
        properties::{ObjectKind, ObjectPropertiesEdit, ObjectTarget},
        provisioning::CopyVmRequest,
    };
    let inventory = c.inventory().await?;
    let template = inventory
        .templates
        .iter()
        .find(|v| v.name == "Other install media")
        .ok_or("Blank template missing")?;
    let sr = inventory
        .storage
        .iter()
        .find(|s| s.name == "Local storage")
        .ok_or("Local storage missing")?;
    let source = c
        .create_configured_vm(&CreateVmRequest {
            source: xen_core::provisioning::VmCreationSource::Template(template.uuid.clone()),
            name: format!("{prefix}-copy-source"),
            description: "Full copy source description".into(),
            vcpus_max: 1,
            vcpus_at_startup: 1,
            cores_per_socket: None,
            memory_mib: 256,
            sr_uuid: sr.uuid.clone(),
            new_disk_gib: None,
            network_uuid: None,
            iso_uuid: None,
            affinity_host_uuid: None,
        })
        .await?;
    c.add_disk(
        &source,
        &sr.uuid,
        16 * 1024 * 1024,
        Some(&format!("{prefix}-marker")),
    )
    .await?;
    let original = c
        .object_properties(&ObjectTarget {
            kind: ObjectKind::Vm,
            uuid: source.clone(),
        })
        .await?;
    let mut edited = original.clone();
    edited.tags.push("  copy exact tag  ".into());
    c.update_object_properties(&ObjectPropertiesEdit { original, edited })
        .await?;
    let source_props = c
        .object_properties(&ObjectTarget {
            kind: ObjectKind::Vm,
            uuid: source.clone(),
        })
        .await?;
    let source_details = c.vm_details(&source).await?;
    let disk = source_details
        .disks
        .iter()
        .find(|d| !d.is_cd)
        .ok_or("Marker disk missing")?;
    let hash = disk_hash(&disk.vdi_uuid, prefix, true)?;
    let mut request = CopyVmRequest {
        name: format!("{prefix}-full-copy-vm"),
        description: "Copied VM description".into(),
        sr_uuid: sr.uuid.clone(),
    };
    if let Some(iso) = inventory.storage.iter().find(|s| s.content_type == "iso") {
        request.sr_uuid = iso.uuid.clone();
        if c.copy_vm(&source, &request).await.is_ok() {
            return Err("ISO SR accepted for copy".into());
        }
        request.sr_uuid = sr.uuid.clone();
        println!(
            "{}",
            json!({"check":"copy-iso-destination-rejected","status":"VERIFIED"})
        );
    }
    let mut normal_copy_uuid = String::new();
    for as_template in [false, true] {
        if as_template {
            c.convert_vm_to_template(&source).await?;
            request.name = format!("{prefix}-full-copy-template");
            request.description = "Copied template description".into();
        }
        let uuid = c.copy_vm(&source, &request).await?;
        if !as_template {
            normal_copy_uuid = uuid.clone();
        }
        let copied = c.vm_details(&uuid).await?;
        let copied_disk = copied
            .disks
            .iter()
            .find(|d| !d.is_cd)
            .ok_or("Copied marker disk missing")?;
        let props = c
            .object_properties(&ObjectTarget {
                kind: ObjectKind::Vm,
                uuid: uuid.clone(),
            })
            .await?;
        if copied.is_template != as_template
            || copied.power_state != "Halted"
            || copied.description != request.description
            || copied_disk.vdi_uuid == disk.vdi_uuid
            || copied_disk.sr_uuid != sr.uuid
            || !copied_disk.owner
            || props.tags != source_props.tags
            || disk_hash(&copied_disk.vdi_uuid, prefix, false)? != hash
        {
            return Err("Copy type, description, tags, disk independence, destination, ownership or bytes differ".into());
        }
        if disk_hash(&disk.vdi_uuid, prefix, false)? != hash {
            return Err("Copy changed the source disk bytes".into());
        }
        let current = c.object_properties(&source_props.target).await?;
        if current.name != source_props.name
            || current.description != source_props.description
            || current.tags != source_props.tags
        {
            return Err("Copy changed source metadata".into());
        }
        println!(
            "{}",
            json!({"check":if as_template {"full-copy-template"} else {"full-copy-vm"},"source_uuid":source,"copy_uuid":uuid,"source_disk_uuid":disk.vdi_uuid,"copy_disk_uuid":copied_disk.vdi_uuid,"sr_uuid":sr.uuid,"raw_bytes":16777216,"sha256":hash,"source_unchanged":true,"exact_tags":true,"independent_disk":true,"status":"VERIFIED"})
        );
    }
    if input.mode == "verify-copy-recovery" {
        let foreign_name = format!("{prefix}-preexisting-unattached");
        c.add_disk(&normal_copy_uuid, &sr.uuid, 16777216, Some(&foreign_name))
            .await?;
        let foreign = c
            .vm_details(&normal_copy_uuid)
            .await?
            .disks
            .into_iter()
            .find(|d| d.vdi_name == foreign_name)
            .ok_or("Foreign test disk missing")?;
        c.remove_disk(&normal_copy_uuid, &foreign.vbd_uuid, false)
            .await?;
        let foreign_hash = disk_hash(&foreign.vdi_uuid, prefix, true)?;

        let mut proxy = Command::new("python3")
            .arg("tools/audit_copy_fault_proxy.py")
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;
        let mut output = io::BufReader::new(proxy.stdout.take().ok_or("Missing relay output")?);
        let mut line = String::new();
        output.read_line(&mut line)?;
        let setup: serde_json::Value = serde_json::from_str(&line)?;
        let port = setup["port"].as_u64().ok_or("Relay did not start")?;
        let result:Result<(),Error>=async {
            let mut settings=input.settings.clone();settings.proxy=Some(format!("http://127.0.0.1:{port}"));
            let isolated=Connection::login(settings,&input.password).await?;
            request.name=format!("{prefix}-reconciled-copy");
            let error=isolated.copy_vm(&normal_copy_uuid,&request).await.err().ok_or("Copy completed before relay interruption")?;
            let xen_core::xapi::ApiError::CopyInterrupted(recovery)=error else {return Err(format!("Expected retained copy context after real connection loss: {error}").into());};
            let reference=recovery.task_reference().ok_or("Interruption occurred before the copy task reference arrived")?.to_owned();
            line.clear();output.read_line(&mut line)?;
            let event:serde_json::Value=serde_json::from_str(&line)?;
            if event["task"]["task_ref"].as_str()!=Some(reference.as_str()) {return Err("Relay interrupted a different host task".into());}
            println!("{}",json!({"check":"real-copy-connection-loss","relay_event":event,"retained_task_ref":reference,"status":"VERIFIED"}));
            let mut recovery=recovery;
            for _ in 0..120 {
                if c.task_record(&reference).await?.status==xen_core::operations::TaskStatus::Succeeded {break;}
                tokio::time::sleep(std::time::Duration::from_millis(500)).await;
            }
            let inventory=c.inventory().await?;
            let pending=inventory.vms.iter().find(|v|v.name==request.name).ok_or("Pending real copy missing")?.uuid.clone();
            let mut readonly_settings=input.settings.clone();readonly_settings.read_only=true;
            let readonly=Connection::login(readonly_settings,&input.password).await?;
            if !matches!(readonly.resume_copy_vm(&recovery).await,Err(xen_core::xapi::ApiError::CopyInterrupted(_))) {return Err("Read-only recovery discarded context".into());}
            readonly.logout().await?;
            println!("{}",json!({"check":"read-only-copy-recovery-retains-context","status":"VERIFIED"}));
            attach_foreign_copy_disk(&pending,&foreign.vdi_uuid,true)?;
            let check=c.resume_copy_vm(&recovery).await;
            let layout_result:Result<(),Error>=async {
                let Err(xen_core::xapi::ApiError::CopyInterrupted(retained))=check else {return Err("Changed copy layout was not retained for inspection".into());};
                recovery=retained;
                if c.vm_details(&pending).await?.disks.iter().filter(|d|!d.is_cd).count()!=2 || disk_hash(&foreign.vdi_uuid,prefix,false)?!=foreign_hash {return Err("Recovery removed the changed copy or its foreign disk".into());}
                println!("{}",json!({"check":"changed-copy-layout-preserves-foreign-disk-and-vm","copy_uuid":pending,"foreign_disk_uuid":foreign.vdi_uuid,"status":"VERIFIED"}));
                Ok(())
            }.await;
            attach_foreign_copy_disk(&pending,&foreign.vdi_uuid,false)?;
            layout_result?;
            let original=c.object_properties(&ObjectTarget{kind:ObjectKind::Vm,uuid:pending.clone()}).await?;
            let mut external=original.clone();external.description="External destination edit during copy interruption".into();
            c.update_object_properties(&ObjectPropertiesEdit{original:original.clone(),edited:external.clone()}).await?;
            let Err(xen_core::xapi::ApiError::CopyInterrupted(retained))=c.resume_copy_vm(&recovery).await else {return Err("Concurrent copy-description edit was overwritten".into());};
            recovery=retained;
            if c.object_properties(&external.target).await?.description!=external.description {return Err("Recovery lost the external destination description".into());}
            c.update_object_properties(&ObjectPropertiesEdit{original:external,edited:original}).await?;
            println!("{}",json!({"check":"copy-recovery-rejects-concurrent-description-edit","copy_uuid":pending,"status":"VERIFIED"}));
            let uuid=c.resume_copy_vm(&recovery).await?;
            if uuid!=pending {return Err("Recovery changed the destination identity".into());}

            let details=c.vm_details(&uuid).await?;
            let copied=details.disks.iter().find(|d|!d.is_cd).ok_or("Reconciled copy disk missing")?;
            let inventory=c.inventory().await?;
            if inventory.vms.iter().filter(|v|v.name==request.name).count()!=1 || details.is_template || details.description!=request.description || copied.vdi_uuid==disk.vdi_uuid || disk_hash(&copied.vdi_uuid,prefix,false)?!=hash {
                return Err("Reconciliation did not finish the existing independent copy exactly once".into());
            }
            println!("{}",json!({"check":"reconcile-existing-copy-without-resubmission","uuid":uuid,"disk_uuid":copied.vdi_uuid,"sha256":hash,"matching_destination_count":1,"status":"VERIFIED"}));
            Ok(())
        }.await;
        let _ = proxy.kill();
        let _ = proxy.wait();
        result?;
        workspace_copy_recovery(c, &normal_copy_uuid, &request, prefix, input, &hash).await?;
    }
    if input.pause_for_ui {
        println!(
            "{}",
            json!({"phase":"ready-for-ui","uuid":source,"prefix":prefix,"sr_uuid":sr.uuid})
        );
        let mut line = String::new();
        io::stdin().lock().read_line(&mut line)?;
    }
    Ok(())
}

async fn exercise(c: &Connection, prefix: &str, input: &Input) -> Result<(), Error> {
    if input.mode == "verify-copy" || input.mode == "verify-copy-recovery" {
        return full_copy(c, prefix, input).await;
    }
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
        .create_configured_vm(&CreateVmRequest {
            source: xen_core::provisioning::VmCreationSource::Template(template.uuid.clone()),
            name: format!("{prefix}-vm"),
            description: "Temporary advanced VM qualification".into(),
            vcpus_max: 1,
            vcpus_at_startup: 1,
            cores_per_socket: None,
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
            pv_args: None,
            auto_poweron: None,
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
            cpu_weight: None,
            cores_per_socket: None,
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
                    pv_args: None,
                    auto_poweron: None,
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
                    cpu_weight: None,
                    cores_per_socket: None,
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
                pv_args: None,
                auto_poweron: None,
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
                cpu_weight: None,
                cores_per_socket: None,
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
                    pv_args: None,
                    auto_poweron: None,
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
                    cpu_weight: None,
                    cores_per_socket: None,
                },
            )
            .await;
        let maximum = c
            .update_vm_resources(
                &uuid,
                &xen_core::operations::VmResourceUpdate {
                    pv_args: None,
                    auto_poweron: None,
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
                    cpu_weight: None,
                    cores_per_socket: None,
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
                .create_configured_vm(&CreateVmRequest {
                    source: xen_core::provisioning::VmCreationSource::Template(uuid.clone()),
                    name: format!("{prefix}-ui"),
                    description: "Temporary UI qualification VM".into(),
                    vcpus_max: 1,
                    vcpus_at_startup: 1,
                    cores_per_socket: None,
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
    for disk in c
        .vdi_catalog()
        .await?
        .iter()
        .filter(|d| d.name.starts_with(&prefix))
    {
        c.delete_vdi(&disk.uuid).await?;
    }
    c.logout().await?;
    println!(
        "{}",
        json!({"cleanup":"completed","result":result.as_ref().map(|_|"pass").map_err(|e|e.to_string())})
    );
    result
}
