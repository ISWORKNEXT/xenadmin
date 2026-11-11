#!/usr/bin/env python3
"""Start/read/stop one bounded owned blank .170 VM for hypervisor CPU qualification.
Input JSON: prefix, vm_uuid, mode (start, read, stop), expected_weight, expected_cap.
"""
import json,subprocess,sys
config=json.load(sys.stdin)
script=r'''
import XenAPI,json,uuid,subprocess
c=json.loads(CONFIG);prefix=c['prefix'];assert prefix.startswith('zz-parity-storage-');uuid.UUID(prefix[len('zz-parity-storage-'):]);assert c['mode'] in ('start','read','stop')
s=XenAPI.xapi_local();s.xenapi.login_with_password('','','1.0','owned-runtime-cpu-qualification');started=False;qualified=False
try:
 hosts=s.xenapi.host.get_all_records();assert len(hosts)==1 and next(iter(hosts.values()))['address']=='192.168.8.170'
 ref=s.xenapi.VM.get_by_uuid(c['vm_uuid']);v=s.xenapi.VM.get_record(ref)
 assert v['name_label']==prefix+'-source' and not v['is_control_domain'] and not v['is_a_snapshot'] and not v['is_a_template']
 assert int(v['memory_static_max'])<=1024**3 and int(v['VCPUs_max'])<=4 and not v['current_operations']
 for b in v['VBDs']:
  b=s.xenapi.VBD.get_record(b)
  if b['type']!='Disk':continue
  d=s.xenapi.VDI.get_record(b['VDI']);assert d['name_label'].startswith(prefix) and int(d['virtual_size'])<=1024**3 and 'owner' in b['other_config']
 if c['mode']!='stop':
  assert int(v['VCPUs_params'].get('weight','256'))==c['expected_weight'] and int(v['VCPUs_params'].get('cap','0'))==c['expected_cap']
 if c['mode']=='start':
  assert v['power_state']=='Halted';s.xenapi.VM.start(ref,False,False);started=True
 if c['mode']=='stop':
  assert v['power_state'] in ('Running','Paused');s.xenapi.VM.hard_shutdown(ref)
 v=s.xenapi.VM.get_record(ref);out={'vm_uuid':v['uuid'],'power_state':v['power_state'],'weight':v['VCPUs_params'].get('weight'),'cores_per_socket':v['platform'].get('cores-per-socket')}
 if c['mode']!='stop':
  assert v['power_state']=='Running';domid=int(v['domid']);assert domid>0
  scheduler=subprocess.run(['xl','sched-credit','-d',str(domid)],universal_newlines=True,stdout=subprocess.PIPE,stderr=subprocess.PIPE,check=True).stdout
  rows=[line.split() for line in scheduler.splitlines() if len(line.split())>=4 and line.split()[1]==str(domid)];assert len(rows)==1
  assert int(rows[0][2])==c['expected_weight'] and int(rows[0][3])==c['expected_cap']
  out.update({'domid':domid,'scheduler_stdout':scheduler,'vcpu_stdout':subprocess.run(['xl','vcpu-list',str(domid)],universal_newlines=True,stdout=subprocess.PIPE,stderr=subprocess.PIPE,check=True).stdout,'hypervisor_weight_and_cap_verified':True})
 print(json.dumps(out));qualified=True
finally:
 if started and not qualified and s.xenapi.VM.get_power_state(ref) in ('Running','Paused'):s.xenapi.VM.hard_shutdown(ref)
 s.xenapi.session.logout()
'''.replace('CONFIG',json.dumps(json.dumps(config)))
r=subprocess.run(['ssh','-o','BatchMode=yes','root@192.168.8.170','python3','-'],input=script,universal_newlines=True,capture_output=True)
if r.returncode:sys.exit(r.stderr)
print(json.dumps(json.loads(r.stdout)))
