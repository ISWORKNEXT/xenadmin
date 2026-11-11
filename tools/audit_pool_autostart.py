#!/usr/bin/env python3
"""Journal/read/restore .170 pool autostart; never reboots or changes VM/host settings.
Input JSON: mode=create/read/external/restore, prefix; restore requires expected_auto
from the latest independent readback. Restoration admits exact original test keys.
"""
import json,subprocess,sys
config=json.load(sys.stdin)
script=r'''
import XenAPI,json,uuid,hashlib,os
from pathlib import Path
c=json.loads(CONFIG);prefix=c['prefix'];assert prefix.startswith('zz-parity-pool-autostart-');uuid.UUID(prefix[len('zz-parity-pool-autostart-'):]);assert c['mode'] in ('create','read','external','restore')
path=Path('/var/tmp')/(prefix+'.json');external_key=prefix
s=XenAPI.xapi_local();s.xenapi.login_with_password('','','1.0','pool-autostart-independent-qualification')
def digest(value):return hashlib.sha256(json.dumps(value,sort_keys=True).encode()).hexdigest()
def entry(record,key):return {'present':key in record,'value':record.get(key)}
def state():
 hosts=s.xenapi.host.get_all_records();assert len(hosts)==1 and next(iter(hosts.values()))['address']=='192.168.8.170'
 pools=s.xenapi.pool.get_all_records();assert len(pools)==1;ref,pool=next(iter(pools.items()));vms=s.xenapi.VM.get_all_records()
 other=dict(pool['other_config']);auto=entry(other,'auto_poweron');external=entry(other,external_key);other.pop('auto_poweron',None);other.pop(external_key,None)
 fields=['name_label','name_description','tags','is_a_template','is_a_snapshot','power_state','VCPUs_max','VCPUs_at_startup','VCPUs_params','memory_static_max','memory_static_min','memory_dynamic_min','memory_dynamic_max','HVM_boot_policy','HVM_boot_params','PV_args','platform','other_config','actions_after_crash']
 inventory={k:sorted(s.xenapi.__getattr__(cls).get_uuid(r) for r in s.xenapi.__getattr__(cls).get_all()) for k,cls in [('vm_uuids','VM'),('vdi_uuids','VDI'),('sr_uuids','SR'),('pbd_uuids','PBD'),('network_uuids','network')]}
 return ref,{'pool_uuid':pool['uuid'],'auto_poweron':auto,'external':external,'unrelated_pool_map_sha256':digest(other),'host_config_sha256':digest({v['uuid']:v['other_config'] for v in hosts.values()}),'vm_config_sha256':digest({v['uuid']:{k:v[k] for k in fields} for v in vms.values()}),'scalar_options':{k:pool.get(k) for k in ['migration_compression','igmp_snooping_enabled','live_patching_disabled']},'inventory':inventory}
try:
 ref,current=state()
 if c['mode']=='create':
  assert current['auto_poweron']['value'] in (None,'true','false','0','1') and not current['external']['present']
  journal={'prefix':prefix,'baseline':current}
  with os.fdopen(os.open(str(path),os.O_WRONLY|os.O_CREAT|os.O_EXCL,0o600),'w') as f:json.dump(journal,f)
 else:
  assert not path.is_symlink() and path.stat().st_mode&0o077==0;journal=json.loads(path.read_text());assert journal['prefix']==prefix and journal['baseline']['pool_uuid']==current['pool_uuid']
 if c['mode']=='external':
  other=s.xenapi.pool.get_other_config(ref);assert other.get(external_key) in (None,'owned-external-value');other[external_key]='owned-external-value';s.xenapi.pool.set_other_config(ref,other);ref,current=state()
 if c['mode']=='restore':
  assert current['auto_poweron']==c['expected_auto'],'Autostart changed since the last read; journal retained'
  assert current['external']==journal['baseline']['external'] or current['external']=={'present':True,'value':'owned-external-value'},'External test key changed; journal retained'
  other=s.xenapi.pool.get_other_config(ref)
  assert entry(other,'auto_poweron')==c['expected_auto'] and entry(other,external_key)==current['external'],'Test keys changed during restore admission; journal retained'
  for key,saved in [('auto_poweron',journal['baseline']['auto_poweron']),(external_key,journal['baseline']['external'])]:
   if saved['present']:other[key]=saved['value']
   else:other.pop(key,None)
  s.xenapi.pool.set_other_config(ref,other);ref,current=state()
  assert current==journal['baseline'],'Test keys restored but unrelated state differs; journal retained'
  path.unlink();print(json.dumps({'exact_baseline_restored':True,'journal_removed':True,'state':current}))
 else:print(json.dumps({'prefix':prefix,'journal_path':str(path),'baseline':journal['baseline'],'state':current}))
finally:s.xenapi.session.logout()
'''.replace('CONFIG',json.dumps(json.dumps(config)))
r=subprocess.run(['ssh','-o','BatchMode=yes','-o','ConnectTimeout=10','root@192.168.8.170','python3','-'],input=script,text=True,capture_output=True)
if r.returncode:sys.exit(r.stderr)
print(json.dumps(json.loads(r.stdout)))
