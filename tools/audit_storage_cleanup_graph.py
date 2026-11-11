#!/usr/bin/env python3
"""Read-only admission of the complete deletion graph for an owned .170 SR test.
Input is one JSON object with prefix, location and the original UUID baseline.
Never changes host objects. Rejects suspend images and owner CDs. Snapshots require explicit whole-lineage admission.
"""
import json,subprocess,sys
config=json.load(sys.stdin)
script=r'''
import XenAPI,json,uuid
from pathlib import Path
c=json.loads(CONFIG);prefix=c['prefix'];baseline=c['baseline']
assert prefix.startswith('zz-parity-storage-');uuid.UUID(prefix[len('zz-parity-storage-'):])
parent=Path('/var/tmp')/prefix;location=parent/'sr'
assert c['location']==str(location) and not parent.is_symlink() and not location.is_symlink()
assert json.loads((parent/'owner.json').read_text())=={'prefix':prefix}
s=XenAPI.xapi_local();s.xenapi.login_with_password('','','1.0','owned-storage-cleanup-preflight')
try:
 vms=s.xenapi.VM.get_all_records();vdis=s.xenapi.VDI.get_all_records();vbds=s.xenapi.VBD.get_all_records();srs=s.xenapi.SR.get_all_records();pbds=s.xenapi.PBD.get_all_records();hosts=s.xenapi.host.get_all_records()
 target_hosts=[r for r,h in hosts.items() if h['address']=='192.168.8.170'];assert len(hosts)==len(target_hosts)==1
 admitted={r:v for r,v in vms.items() if v['name_label'].startswith(prefix)}
 vm_plans=[];disk_ids=set()
 for ref,vm in admitted.items():
  assert vm['uuid'] not in baseline['vm_uuids'] and not vm['is_control_domain']
  if c.get('allow_snapshots',False):
   assert all(child in admitted for child in vm['snapshots']),'Unadmitted snapshot descendant'
   if vm['is_a_snapshot']:assert vm['snapshot_of'] in admitted,'Snapshot parent is not admitted'
  else:assert not vm['is_a_snapshot'] and not vm['snapshots'],'Snapshots are outside this storage test cleanup'
  assert vm['suspend_VDI'] in ('','OpaqueRef:NULL'),'Suspend images are outside this storage test cleanup'
  assert not vm['current_operations'],'Owned VM operation is still active'
  owned=[];cds=[]
  for b in vm['VBDs']:
   vbd=vbds[b];disk=vbd['VDI']
   if disk in ('','OpaqueRef:NULL'):continue
   d=vdis[disk]
   if vbd['type']=='CD':
    assert 'owner' not in vbd['other_config'],'Owner-marked nonempty CD is not admitted'
    cds.append(d['uuid']);continue
   assert vbd['type']=='Disk' and 'owner' in vbd['other_config']
   assert d['uuid'] not in baseline['vdi_uuids'] and d['name_label'].startswith(prefix)
   assert (c.get('allow_snapshots',False) or not d['is_a_snapshot']) and not d['current_operations']
   assert all(vbds[r]['VM'] in admitted for r in d['VBDs']),'Disk is referenced by an unadmitted VM'
   owned.append(d['uuid']);disk_ids.add(d['uuid'])
  vm_plans.append({'uuid':vm['uuid'],'name':vm['name_label'],'power_state':vm['power_state'],'disk_uuids':sorted(owned),'nonowner_cd_uuids':sorted(cds),'snapshot_count':len(vm['snapshots']),'is_snapshot':vm['is_a_snapshot'],'suspend_image_empty':True})
 # Delete admitted snapshot leaves first, then their source VMs; reject cycles.
 ordered=[];pending=dict(admitted)
 while pending:
  leaves=[ref for ref,vm in pending.items() if all(child not in pending for child in vm['snapshots'])]
  assert leaves,'Snapshot lineage cycle'
  for ref in leaves:ordered.append(pending.pop(ref)['uuid'])
 vm_plans.sort(key=lambda vm:ordered.index(vm['uuid']))
 sr_plans=[]
 for ref,sr in srs.items():
  if sr['name_label']!=prefix+'-sr':continue
  assert sr['uuid'] not in baseline['sr_uuids'] and sr['type']=='file'
  assert all(p['default_SR']!=ref for p in s.xenapi.pool.get_all_records().values()),'Restore pool default before deleting test SR'
  attachments=[p for p in pbds.values() if p['SR']==ref]
  assert len(attachments)==1
  assert attachments[0]['host']==target_hosts[0] and attachments[0]['device_config'].get('location')==str(location)
  assert attachments[0]['uuid'] not in baseline['pbd_uuids']
  ids=[]
  for d in vdis.values():
   if d['SR']!=ref:continue
   assert d['uuid'] not in baseline['vdi_uuids'] and d['name_label'].startswith(prefix)
   assert (c.get('allow_snapshots',False) or not d['is_a_snapshot']) and not d['current_operations']
   assert all(vbds[r]['VM'] in admitted for r in d['VBDs'])
   ids.append(d['uuid']);disk_ids.add(d['uuid'])
  expected={identifier+'.vhd' for identifier in ids}
  assert location.is_dir()
  for entry in location.iterdir():
   assert entry.is_file() and not entry.is_symlink() and entry.name in expected,'Unexpected file in private SR location'
  sr_plans.append({'uuid':sr['uuid'],'pbd_uuid':attachments[0]['uuid'],'host_uuid':hosts[target_hosts[0]]['uuid'],'location':str(location),'disk_uuids':sorted(ids)})
 assert len(sr_plans)==1,'Expected exactly one owned test SR'
 print(json.dumps({'prefix':prefix,'vms':vm_plans,'srs':sr_plans,'admitted_vdi_uuids':sorted(disk_ids),'snapshots_rejected':not c.get('allow_snapshots',False),'snapshot_descendants_admitted':True,'suspend_images_rejected':True,'owner_cds_rejected':True,'baseline_objects_excluded':True,'private_files_admitted':True}))
finally:s.xenapi.session.logout()
'''.replace('CONFIG',json.dumps(json.dumps(config)))
r=subprocess.run(['ssh','-o','BatchMode=yes','root@192.168.8.170','python3','-'],input=script,text=True,capture_output=True,check=True)
print(json.dumps(json.loads(r.stdout)))
