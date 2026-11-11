#!/usr/bin/env python3
"""Seed/read/clean one exact owned, diskless .170 VM for Boot options QA.
Input JSON: mode=create/read/cleanup, prefix; VM UUID+baseline after create.
Closed fixture mutations external/hvm/pv/legacy-auto require that same admitted VM.
No VDI is ever deleted; seed PV classification without changing HVM parameter maps.
"""
import json,subprocess,sys
config=json.load(sys.stdin)
script=r'''
import XenAPI,json,uuid,hashlib
c=json.loads(CONFIG);prefix=c['prefix'];assert prefix.startswith('zz-parity-boot-');uuid.UUID(prefix[len('zz-parity-boot-'):]);assert c['mode'] in ('create','read','cleanup','external','hvm','pv','legacy-auto')
s=XenAPI.xapi_local();s.xenapi.login_with_password('','','1.0','owned-boot-options-qualification')
def inventory():return {k:sorted(s.xenapi.__getattr__(cls).get_uuid(r) for r in s.xenapi.__getattr__(cls).get_all()) for k,cls in [('vm_uuids','VM'),('vdi_uuids','VDI'),('sr_uuids','SR'),('pbd_uuids','PBD'),('network_uuids','network')]}
def global_state():return {cls:{v['uuid']:hashlib.sha256(json.dumps(v['other_config'],sort_keys=True).encode()).hexdigest() for v in s.xenapi.__getattr__(cls).get_all_records().values()} for cls in ('host','pool')}
try:
 hosts=s.xenapi.host.get_all_records();assert len(hosts)==1 and next(iter(hosts.values()))['address']=='192.168.8.170'
 if c['mode']=='create':
  baseline=inventory();global_before=global_state();sources=[r for r,v in s.xenapi.VM.get_all_records().items() if v['name_label']=='Other install media' and v['is_a_template'] and not v['is_a_snapshot']];assert len(sources)==1
  assert all(s.xenapi.VBD.get_VDI(b) in ('','OpaqueRef:NULL') for b in s.xenapi.VM.get_VBDs(sources[0]))
  ref=s.xenapi.VM.clone(sources[0],prefix+'-source');v=s.xenapi.VM.get_record(ref)
  assert v['uuid'] not in baseline['vm_uuids'];out={'prefix':prefix,'vm_uuid':v['uuid'],'baseline':baseline,'global_before':global_before}
  s.xenapi.VM.set_is_a_template(ref,False);s.xenapi.VM.update_allowed_operations(ref)
  s.xenapi.VM.set_HVM_boot_policy(ref,'');s.xenapi.VM.set_PV_args(ref,'console=hvc0 original=preserve');v=s.xenapi.VM.get_record(ref)
 else:
  baseline=c['baseline'];assert c['vm_uuid'] not in baseline['vm_uuids'];ref=s.xenapi.VM.get_by_uuid(c['vm_uuid']);v=s.xenapi.VM.get_record(ref);out=dict(c);out.pop('mode',None)
 assert v['name_label']==prefix+'-source' and not v['is_control_domain'] and not v['is_a_template'] and not v['is_a_snapshot'] and not v['snapshots'] and v['power_state']=='Halted' and not v['current_operations'] and v['suspend_VDI'] in ('','OpaqueRef:NULL')
 assert all(s.xenapi.VBD.get_VDI(b) in ('','OpaqueRef:NULL') for b in v['VBDs'])
 if c['mode']=='external':
  other=dict(v['other_config']);other['xen-desktop-audit-external']='preserve-after-open';s.xenapi.VM.set_other_config(ref,other);v=s.xenapi.VM.get_record(ref)
 if c['mode'] in ('hvm','pv'):
  s.xenapi.VM.set_HVM_boot_policy(ref,'BIOS order' if c['mode']=='hvm' else '');v=s.xenapi.VM.get_record(ref)
 if c['mode']=='legacy-auto':
  other=dict(v['other_config']);other['auto_poweron']='legacy-disabled';s.xenapi.VM.set_other_config(ref,other);v=s.xenapi.VM.get_record(ref)
 out['record']={k:v[k] for k in ['uuid','name_label','name_description','tags','VCPUs_max','VCPUs_at_startup','VCPUs_params','memory_static_max','memory_static_min','memory_dynamic_min','memory_dynamic_max','HVM_boot_policy','HVM_boot_params','PV_args','platform','other_config','actions_after_crash','power_state']}
 out['global_state_unchanged']=global_state()==out['global_before'];assert out['global_state_unchanged'],'Host/pool other_config changed'
 if c['mode']=='cleanup':
  s.xenapi.VM.destroy(ref);actual=inventory();out={'all_inventory_sets_match':actual==baseline,'counts':{k:len(v) for k,v in actual.items()},'global_state_unchanged':global_state()==out['global_before'],'no_vdi_destroy_called':True};assert actual==baseline and out['global_state_unchanged'],out
 print(json.dumps(out))
finally:s.xenapi.session.logout()
'''.replace('CONFIG',json.dumps(json.dumps(config)))
r=subprocess.run(['ssh','-o','BatchMode=yes','root@192.168.8.170','python3','-'],input=script,text=True,capture_output=True)
if r.returncode:sys.exit(r.stderr)
print(json.dumps(json.loads(r.stdout)))
