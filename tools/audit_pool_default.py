#!/usr/bin/env python3
"""Read-only .170 pool-default SR readback, optionally requiring an expected UUID."""
import json,subprocess,sys
script="""import XenAPI,json
s=XenAPI.xapi_local();s.xenapi.login_with_password('','','1.0','pool-default-independent-readback')
try:
 hosts=s.xenapi.host.get_all_records();assert len(hosts)==1 and next(iter(hosts.values()))['address']=='192.168.8.170'
 pools=s.xenapi.pool.get_all();assert len(pools)==1;p=pools[0];sr=s.xenapi.pool.get_default_SR(p)
 print(json.dumps({'pool_uuid':s.xenapi.pool.get_uuid(p),'sr_ref':sr,'sr_uuid':s.xenapi.SR.get_uuid(sr),'name':s.xenapi.SR.get_name_label(sr)}))
finally:s.xenapi.session.logout()
"""
r=subprocess.run(['ssh','-o','BatchMode=yes','root@192.168.8.170','python3','-'],input=script,text=True,capture_output=True,check=True);state=json.loads(r.stdout)
if len(sys.argv)>1:assert state['sr_uuid']==sys.argv[1],state
print(json.dumps(state))
