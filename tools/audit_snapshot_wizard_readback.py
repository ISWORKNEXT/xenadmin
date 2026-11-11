#!/usr/bin/env python3
"""Verify an owned, halted snapshot-wizard output on .170; temporary RO dom0 VBD only.
Input JSON: prefix, vm_uuid, sr_uuid, network_uuid, host_uuid, vcpus (equal counts)
or vcpus_max/vcpus_at_startup, optional cores_per_socket, memory_mib,
expected_sha256 and description. Credentials are not accepted or persisted.
"""
import json,subprocess,sys
config=json.load(sys.stdin)
script=r'''
import XenAPI,json,hashlib,os,time,uuid
c=json.loads(CONFIG);uuid.UUID(c['prefix'][len('zz-parity-storage-'):]);assert c['prefix'].startswith('zz-parity-storage-')
s=XenAPI.xapi_local();s.xenapi.login_with_password('','','1.0','snapshot-wizard-independent-readback');temporary=None
try:
 hosts=s.xenapi.host.get_all_records();assert len(hosts)==1 and next(iter(hosts.values()))['address']=='192.168.8.170'
 ref=s.xenapi.VM.get_by_uuid(c['vm_uuid']);v=s.xenapi.VM.get_record(ref)
 assert v['name_label'].startswith(c['prefix']) and v['power_state']=='Halted' and not v['is_control_domain'] and not v['is_a_template'] and not v['is_a_snapshot'] and not v['current_operations']
 maximum=c.get('vcpus_max',c.get('vcpus'));initial=c.get('vcpus_at_startup',maximum)
 assert int(v['VCPUs_max'])==maximum and int(v['VCPUs_at_startup'])==initial
 if 'cores_per_socket' in c:assert int(v['platform'].get('cores-per-socket','1'))==c['cores_per_socket']
 assert all(int(v[k])==c['memory_mib']*1024*1024 for k in ['memory_static_max','memory_dynamic_min','memory_dynamic_max'])
 networks=[s.xenapi.network.get_uuid(s.xenapi.VIF.get_network(i)) for i in v['VIFs']]
 assert networks==[c['network_uuid']] and s.xenapi.host.get_uuid(v['affinity'])==c['host_uuid'] and v['name_description']==c['description']
 vbds=[s.xenapi.VBD.get_record(b) for b in v['VBDs'] if s.xenapi.VBD.get_type(b)=='Disk'];assert len(vbds)==1
 b=vbds[0];d=s.xenapi.VDI.get_record(b['VDI']);assert not b['currently_attached'] and 'owner' in b['other_config']
 assert d['name_label'].startswith(c['prefix']) and 0<int(d['virtual_size'])<=1024**3 and s.xenapi.SR.get_uuid(d['SR'])==c['sr_uuid']
 dom0=next(r for r,v in s.xenapi.VM.get_all_records().items() if v['is_control_domain']);device=s.xenapi.VM.get_allowed_VBD_devices(dom0)[0]
 temporary=s.xenapi.VBD.create({'VM':dom0,'VDI':b['VDI'],'userdevice':device,'bootable':False,'mode':'RO','type':'Disk','empty':False,'unpluggable':True,'other_config':{},'qos_algorithm_type':'','qos_algorithm_params':{}})
 s.xenapi.VBD.plug(temporary);path='/dev/'+s.xenapi.VBD.get_device(temporary)
 for _ in range(20):
  if os.path.exists(path):break
  time.sleep(.1)
 digest=hashlib.sha256();remaining=int(d['virtual_size'])
 with open(path,'rb',buffering=0) as f:
  while remaining:
   chunk=f.read(min(remaining,1024**2));assert chunk;digest.update(chunk);remaining-=len(chunk)
 assert digest.hexdigest()==c['expected_sha256']
 print(json.dumps({'status':'VERIFIED','vm_uuid':v['uuid'],'name':v['name_label'],'description':v['name_description'],'vcpus_max':int(v['VCPUs_max']),'vcpus_at_startup':int(v['VCPUs_at_startup']),'cores_per_socket':v['platform'].get('cores-per-socket','1'),'memory_mib':c['memory_mib'],'network_uuids':networks,'affinity_host_uuid':c['host_uuid'],'disk_uuid':d['uuid'],'sr_uuid':c['sr_uuid'],'bytes':int(d['virtual_size']),'sha256':digest.hexdigest()}))
finally:
 if temporary:
  if s.xenapi.VBD.get_currently_attached(temporary):s.xenapi.VBD.unplug(temporary)
  s.xenapi.VBD.destroy(temporary)
 s.xenapi.session.logout()
'''.replace('CONFIG',json.dumps(json.dumps(config)))
r=subprocess.run(['ssh','-o','BatchMode=yes','root@192.168.8.170','python3','-'],input=script,text=True,capture_output=True)
if r.returncode:sys.exit(r.stderr)
print(json.dumps(json.loads(r.stdout)))
