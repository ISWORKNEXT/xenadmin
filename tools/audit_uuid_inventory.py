#!/usr/bin/env python3
"""Read-only .170 UUID inventory comparison. Supply the original baseline JSON on stdin."""
import json,subprocess,sys
baseline=json.load(sys.stdin)
script="""import XenAPI,json
s=XenAPI.xapi_local();s.xenapi.login_with_password('','','1.0','qualification-inventory-comparison')
try:
 hosts=s.xenapi.host.get_all_records();assert len(hosts)==1 and next(iter(hosts.values()))['address']=='192.168.8.170'
 print(json.dumps({k:sorted(s.xenapi.__getattr__(c).get_uuid(r) for r in s.xenapi.__getattr__(c).get_all()) for k,c in [('vm_uuids','VM'),('vdi_uuids','VDI'),('sr_uuids','SR'),('pbd_uuids','PBD'),('network_uuids','network')]}))
finally:s.xenapi.session.logout()
"""
r=subprocess.run(['ssh','-o','BatchMode=yes','root@192.168.8.170','python3','-'],input=script,text=True,capture_output=True,check=True)
actual=json.loads(r.stdout)
assert set(baseline)==set(actual),'Expected all five original UUID sets'
delta={k:{'extra':sorted(set(v)-set(baseline[k])),'missing':sorted(set(baseline[k])-set(v))} for k,v in actual.items()}
matched=actual==baseline
print(json.dumps({'all_inventory_sets_match':matched,'counts':{k:len(v) for k,v in actual.items()},'delta':delta,'actual':actual}))
sys.exit(0 if matched else 1)
