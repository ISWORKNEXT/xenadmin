//! Opt-in XVA round-trip qualification restricted to the authorized development host.
use serde::Deserialize;
use serde_json::json;
use std::{
    io::{self, BufRead, Write},
    process::{Command, Stdio},
};
use xen_core::{
    operations::VmAction,
    provisioning::CreateVmRequest,
    xapi::{Connection, ConnectionSettings},
    xva::{XvaOutcome, XvaRequest},
};
type Error = Box<dyn std::error::Error>;

#[derive(Deserialize)]
struct Input {
    settings: ConnectionSettings,
    password: String,
    #[serde(default)]
    pause_for_ui: bool,
    #[serde(default)]
    cleanup_prefix: Option<String>,
}

fn require(condition: bool, message: &str) -> Result<(), Error> {
    if condition {
        Ok(())
    } else {
        Err(message.to_owned().into())
    }
}

fn disk_hash(uuid: &str, prefix: &str, write: bool) -> Result<String, Error> {
    let config = json!({"uuid":uuid,"prefix":prefix,"write":write});
    let script=r#"
import XenAPI,json,os,hashlib,time
C=json.loads(CONFIG)
s=XenAPI.xapi_local();s.xenapi.login_with_password('','','1.0','xva-disk-qualification')
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
   marker=(b'XEN-DESKTOP-XVA-ROUNDTRIP\n'*4096)[:65536]
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

async fn exercise(
    c: &Connection,
    prefix: &str,
    directory: &std::path::Path,
    pause: bool,
) -> Result<(), Error> {
    let inventory = c.inventory().await?;
    let template = inventory
        .templates
        .iter()
        .find(|v| v.name == "Other install media")
        .ok_or("Template missing")?;
    let sr = inventory
        .storage
        .iter()
        .find(|v| v.name == "Local storage")
        .ok_or("SR missing")?;
    let network = inventory
        .networks
        .iter()
        .find(|v| v.name == "Pool-wide network 0" && v.bridge != "xenapi")
        .ok_or("Network missing")?;
    let source = c
        .create_configured_vm(&CreateVmRequest {
            guest_setup: None,
            start_automatically: false,
            source: xen_core::provisioning::VmCreationSource::Template(template.uuid.clone()),
            name: format!("{prefix}-source"),
            description: "Temporary XVA qualification VM".into(),
            vcpus_max: 1,
            vcpus_at_startup: 1,
            cores_per_socket: None,
            memory_mib: 256,
            sr_uuid: sr.uuid.clone(),
            new_disk_gib: None,
            network_uuid: Some(network.uuid.clone()),
            iso_uuid: None,
            affinity_host_uuid: None,
        })
        .await?;
    c.add_disk(&source, &sr.uuid, 16 * 1024 * 1024, None)
        .await?;
    let source_details = c.vm_details(&source).await?;
    let original = source_details
        .disks
        .iter()
        .find(|d| !d.is_cd)
        .ok_or("Source disk missing")?;
    let original_hash = disk_hash(&original.vdi_uuid, prefix, true)?;
    let archive = directory.join("source.xva");
    let exported = c
        .transfer_xva(&XvaRequest::Export {
            vm_uuid: source.clone(),
            path: archive.clone(),
        })
        .await?;
    println!(
        "{}",
        json!({"check":"export","result":exported,"source_uuid":source,"archive":archive})
    );
    let original_length = std::fs::metadata(&archive)?.len();
    require(
        c.transfer_xva(&XvaRequest::Export {
            vm_uuid: source.clone(),
            path: archive.clone(),
        })
        .await
        .is_err(),
        "Existing export destination was replaced",
    )?;
    require(
        std::fs::metadata(&archive)?.len() == original_length,
        "Existing archive length changed",
    )?;
    let imported = c
        .transfer_xva(&XvaRequest::Import {
            path: archive.clone(),
            sr_uuid: sr.uuid.clone(),
            name: format!("{prefix}-imported"),
            network_uuid: Some(network.uuid.clone()),
        })
        .await?;
    let XvaOutcome::Imported { uuid } = imported else {
        return Err("Wrong transfer result".into());
    };
    let details = c.vm_details(&uuid).await?;
    let disk = details
        .disks
        .iter()
        .find(|d| !d.is_cd)
        .ok_or("Imported disk missing")?;
    require(
        details.power_state == "Halted" && !details.is_snapshot && !details.is_template,
        "Wrong imported state",
    )?;
    require(
        details.vcpus_at_startup == 1 && details.memory_static_max == 256 * 1024 * 1024,
        "Imported resources differ",
    )?;
    require(
        disk.owner && disk.sr_uuid == sr.uuid && disk.vdi_uuid != original.vdi_uuid,
        "Imported disk ownership or storage differs",
    )?;
    require(
        details.interfaces.len() == 1 && details.interfaces[0].network_uuid == network.uuid,
        "Imported network mapping differs",
    )?;
    let imported_hash = disk_hash(&disk.vdi_uuid, prefix, false)?;
    require(
        original_hash == imported_hash,
        "Imported raw disk content differs",
    )?;
    require(
        disk_hash(&original.vdi_uuid, prefix, false)? == original_hash,
        "Source raw disk content changed",
    )?;
    println!(
        "{}",
        json!({"check":"roundtrip","imported_uuid":uuid,"source_disk":original.vdi_uuid,"imported_disk":disk.vdi_uuid,"sha256":original_hash,"raw_bytes":16777216,"source_unchanged":true,"network_mapped":true,"halted":true,"overwrite_rejected":true})
    );
    let broken = directory.join("truncated.xva");
    std::fs::copy(&archive, &broken)?;
    std::fs::OpenOptions::new()
        .write(true)
        .open(&broken)?
        .set_len((original_length / 2).max(1024))?;
    let rejected = c
        .transfer_xva(&XvaRequest::Import {
            path: broken,
            sr_uuid: sr.uuid.clone(),
            name: format!("{prefix}-broken"),
            network_uuid: None,
        })
        .await;
    require(rejected.is_err(), "Truncated archive was accepted")?;
    println!(
        "{}",
        json!({"check":"truncated-import","error":rejected.err().map(|e|e.to_string())})
    );
    let corrupt = directory.join("corrupt.xva");
    std::fs::copy(&archive, &corrupt)?;
    let status=Command::new("python3").arg("-c").arg("import tarfile,sys; p=sys.argv[1]; t=tarfile.open(p); m=next(m for m in t.getmembers() if m.size==1048576); t.close(); f=open(p,'r+b'); f.seek(m.offset_data+4096); b=f.read(1); f.seek(m.offset_data+4096); f.write(bytes([b[0]^255])); f.close() ").arg(&corrupt).status()?;
    require(status.success(), "Could not prepare the damaged export")?;
    let rejected = c
        .transfer_xva(&XvaRequest::Import {
            path: corrupt,
            sr_uuid: sr.uuid.clone(),
            name: format!("{prefix}-corrupt"),
            network_uuid: None,
        })
        .await;
    require(rejected.is_err(), "Damaged archive was accepted")?;
    println!(
        "{}",
        json!({"check":"corrupt-import","error":rejected.err().map(|e|e.to_string())})
    );
    let snapshot_name = format!("{prefix}-snapshot");
    let task = c
        .begin_vm_action(
            &source,
            &VmAction::Snapshot {
                name: snapshot_name.clone(),
            },
        )
        .await?;
    c.wait_task(&task).await?;
    let inventory = c.inventory().await?;
    let snapshot = inventory
        .snapshots
        .iter()
        .find(|s| s.name == snapshot_name)
        .ok_or("Test snapshot missing")?;
    let template = c
        .create_from_snapshot(&snapshot.uuid, &format!("{prefix}-template"), true)
        .await?;
    let template_archive = directory.join("template.xva");
    c.transfer_xva(&XvaRequest::Export {
        vm_uuid: template,
        path: template_archive.clone(),
    })
    .await?;
    let imported = c
        .transfer_xva(&XvaRequest::Import {
            path: template_archive,
            sr_uuid: sr.uuid.clone(),
            name: format!("{prefix}-template-imported"),
            network_uuid: Some(network.uuid.clone()),
        })
        .await?;
    let XvaOutcome::Imported { uuid } = imported else {
        return Err("Wrong template transfer result".into());
    };
    let details = c.vm_details(&uuid).await?;
    require(
        details.is_template && !details.is_snapshot && details.power_state == "Halted",
        "Imported template type changed",
    )?;
    require(
        details.interfaces.len() == 1 && details.interfaces[0].network_uuid == network.uuid,
        "Template network mapping failed",
    )?;
    let disk = details
        .disks
        .iter()
        .find(|d| !d.is_cd)
        .ok_or("Imported template disk missing")?;
    require(
        disk_hash(&disk.vdi_uuid, prefix, false)? == original_hash,
        "Template raw disk content differs",
    )?;
    println!(
        "{}",
        json!({"check":"template-roundtrip","uuid":uuid,"network_mapped":true,"disk_sha256":original_hash})
    );
    if pause {
        println!(
            "{}",
            json!({"phase":"ready-for-ui","source_uuid":source,"prefix":prefix,"directory":directory,"archive":archive})
        );
        let mut resume = String::new();
        io::stdin().lock().read_line(&mut resume)?;
    }
    Ok(())
}

#[tokio::main]
async fn main() -> Result<(), Error> {
    let mut line = String::new();
    io::stdin().lock().read_line(&mut line)?;
    let input: Input = serde_json::from_str(&line)?;
    require(
        input.settings.host == "192.168.8.170" && !input.settings.read_only,
        "Only the authorized writable dev host is permitted",
    )?;
    let c = Connection::login(input.settings, &input.password).await?;
    let cleanup_only = input.cleanup_prefix.is_some();
    let prefix = input
        .cleanup_prefix
        .unwrap_or_else(|| format!("zz-parity-xva-{}", uuid::Uuid::now_v7()));
    require(
        prefix
            .strip_prefix("zz-parity-xva-")
            .is_some_and(|id| uuid::Uuid::parse_str(id).is_ok()),
        "Invalid qualification prefix",
    )?;
    let directory = tempfile::tempdir()?;
    println!("{}", json!({"prefix":prefix,"directory":directory.path()}));
    let result = if cleanup_only {
        Ok(())
    } else {
        exercise(&c, &prefix, directory.path(), input.pause_for_ui).await
    };
    let inventory = c.inventory().await?;
    for vm in inventory
        .snapshots
        .iter()
        .chain(&inventory.vms)
        .chain(&inventory.templates)
        .filter(|v| v.name.starts_with(&prefix))
    {
        c.destroy_vm(&vm.uuid, true, true).await?;
    }
    c.logout().await?;
    println!(
        "{}",
        json!({"cleanup":"completed","result":result.as_ref().map(|_|"pass").map_err(|e|e.to_string())})
    );
    result
}
