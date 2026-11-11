#!/usr/bin/env python3
"""Read CPU/metadata state or seed sentinels on an owned .170 storage-test VM.
Input JSON: prefix, vm_uuid, seed (default false). Seed requires Halted state.
"""
import json,subprocess,sys
config=json.load(sys.stdin)
script=r'''
import XenAPI,json,uuid
c=json.loads(CONFIG);prefix=c['prefix'];assert prefix.startswith('zz-parity-storage-');uuid.UUID(prefix[len('zz-parity-storage-'):])
s=XenAPI.xapi_local();s.xenapi.login_with_password('','','1.0','owned-cpu-settings-qualification')
try:
 hosts=s.xenapi.host.get_all_records();assert len(hosts)==1 and next(iter(hosts.values()))['address']=='192.168.8.170'
 ref=s.xenapi.VM.get_by_uuid(c['vm_uuid']);v=s.xenapi.VM.get_record(ref)
 assert v['name_label'].startswith(prefix) and not v['is_control_domain'] and not v['is_a_snapshot'] and not v['is_a_template']
 if c.get('seed',False):
  assert v['power_state']=='Halted'
  params=dict(v['VCPUs_params']);params['cap']='0';params['mask']='0-1';s.xenapi.VM.set_VCPUs_params(ref,params)
  platform=dict(v['platform']);platform['xen-desktop-audit-topology']='preserve';s.xenapi.VM.set_platform(ref,platform)
  v=s.xenapi.VM.get_record(ref)
 if c.get('external',False):
  assert v['power_state']=='Halted'
  params=dict(v['VCPUs_params']);params['cap']='10';s.xenapi.VM.set_VCPUs_params(ref,params)
  platform=dict(v['platform']);platform['xen-desktop-audit-external']='preserve-after-open';s.xenapi.VM.set_platform(ref,platform)
  v=s.xenapi.VM.get_record(ref)
 fields=['uuid','name_label','name_description','tags','power_state','VCPUs_max','VCPUs_at_startup','VCPUs_params','platform','HVM_boot_params','HVM_boot_policy','memory_static_max','memory_dynamic_min','memory_dynamic_max','actions_after_crash']
 print(json.dumps({k:v[k] for k in fields}))
finally:s.xenapi.session.logout()
'''.replace('CONFIG',json.dumps(json.dumps(config)))
r=subprocess.run(['ssh','-o','BatchMode=yes','root@192.168.8.170','python3','-'],input=script,text=True,capture_output=True)
if r.returncode:sys.exit(r.stderr)
print(json.dumps(json.loads(r.stdout)))
