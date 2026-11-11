#!/usr/bin/env python3
"""Create/read/remove one unprovisioned owned template for real clipboard UI QA.
No VDI is ever deleted. Cleanup requires no data VBDs, snapshots or suspend image.
Input JSON: mode; prefix for create; complete create result for read/cleanup.
"""
import json,subprocess,sys
config=json.load(sys.stdin)
script=r'''
import XenAPI,json,uuid
c=json.loads(CONFIG);prefix=c['prefix'];assert prefix.startswith('zz-parity-clipboard-');uuid.UUID(prefix[len('zz-parity-clipboard-'):]);assert c['mode'] in ('create','read','cleanup')
s=XenAPI.xapi_local();s.xenapi.login_with_password('','','1.0','owned-clipboard-ui-qualification')
def inventory():return {k:sorted(s.xenapi.__getattr__(cls).get_uuid(r) for r in s.xenapi.__getattr__(cls).get_all()) for k,cls in [('vm_uuids','VM'),('vdi_uuids','VDI'),('sr_uuids','SR'),('pbd_uuids','PBD'),('network_uuids','network')]}
try:
 hosts=s.xenapi.host.get_all_records();assert len(hosts)==1 and next(iter(hosts.values()))['address']=='192.168.8.170'
 if c['mode']=='create':
  baseline=inventory();sources=[r for r,v in s.xenapi.VM.get_all_records().items() if v['name_label'].lower()=='other install media' and v['is_a_template'] and not v['is_a_snapshot']]
  assert len(sources)==1
  for b in s.xenapi.VM.get_VBDs(sources[0]):assert s.xenapi.VBD.get_VDI(b) in ('','OpaqueRef:NULL')
  ref=s.xenapi.VM.clone(sources[0],prefix+'-initial');v=s.xenapi.VM.get_record(ref)
  assert v['uuid'] not in baseline['vm_uuids'];out={'prefix':prefix,'vm_uuid':v['uuid'],'source_uuid':s.xenapi.VM.get_uuid(sources[0]),'baseline':baseline}
 else:
  baseline=c['baseline'];assert c['vm_uuid'] not in baseline['vm_uuids'];ref=s.xenapi.VM.get_by_uuid(c['vm_uuid']);v=s.xenapi.VM.get_record(ref);out=dict(c);out.pop('mode',None)
 assert v['name_label'].startswith(prefix) and not v['is_control_domain'] and not v['is_a_snapshot'] and not v['snapshots'] and v['power_state']=='Halted' and not v['current_operations'] and v['suspend_VDI'] in ('','OpaqueRef:NULL')
 for b in v['VBDs']:assert s.xenapi.VBD.get_VDI(b) in ('','OpaqueRef:NULL'),'Unexpected data VDI; object retained'
 out['record']={k:v[k] for k in ['uuid','name_label','name_description','tags','is_a_template','is_a_snapshot','power_state']}
 if c['mode']=='cleanup':
  s.xenapi.VM.destroy(ref);actual=inventory();out={'all_inventory_sets_match':actual==baseline,'counts':{k:len(v) for k,v in actual.items()},'no_vdi_destroy_called':True}
  assert actual==baseline,out
 print(json.dumps(out))
finally:s.xenapi.session.logout()
'''.replace('CONFIG',json.dumps(json.dumps(config)))
r=subprocess.run(['ssh','-o','BatchMode=yes','root@192.168.8.170','python3','-'],input=script,text=True,capture_output=True)
if r.returncode:sys.exit(r.stderr)
print(json.dumps(json.loads(r.stdout)))
