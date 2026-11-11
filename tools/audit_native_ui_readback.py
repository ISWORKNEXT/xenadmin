#!/usr/bin/env python3
"""Read-only, allowlisted verification of zz-parity-ui-* resources on dev host."""
import getpass,json,ssl,urllib.request
from pathlib import Path
password=getpass.getpass('Authorized dev-host password (hidden): ')
ctx=ssl._create_unverified_context();counter=0
out=Path(__file__).resolve().parents[1]/'docs/audit-live-ui-evidence-20261111/clicks/backend-readback.json'
results=[]
def rpc(method,params):
 global counter
 counter+=1
 req=urllib.request.Request('https://192.168.8.170/jsonrpc',data=json.dumps({'jsonrpc':'2.0','id':counter,'method':method,'params':params}).encode(),headers={'Content-Type':'application/json'})
 with urllib.request.urlopen(req,context=ctx,timeout=45) as r:v=json.load(r)
 if v.get('error'):raise RuntimeError('Read check failed: '+method)
 return v['result']
while True:
 label=input().strip()
 if label=='quit':break
 s=rpc('session.login_with_password',['root',password,'1.0','GUI mutation verification read-only'])
 try:
  vms=rpc('VM.get_all_records',[s]);vbds=rpc('VBD.get_all_records',[s]);vdis=rpc('VDI.get_all_records',[s]);vifs=rpc('VIF.get_all_records',[s])
  selected={r:v for r,v in vms.items() if v.get('name_label','').startswith('zz-parity-ui-')}
  b={r:v for r,v in vbds.items() if v.get('VM') in selected}
  disks={r:v for r,v in vdis.items() if v.get('name_label','').startswith('zz-parity-ui-') or any(z.get('VDI')==r for z in b.values())}
  row={'stage':label,'vms':[{k:v.get(k) for k in ['uuid','name_label','power_state','is_a_snapshot','VCPUs_max','VCPUs_at_startup','memory_static_max']} for v in selected.values()],
       'vbds':[{k:v.get(k) for k in ['uuid','userdevice','currently_attached','type','mode','bootable']} for v in b.values()],
       'vdis':[{k:v.get(k) for k in ['uuid','name_label','virtual_size','is_a_snapshot','read_only']} for v in disks.values()],
       'vifs':[{k:v.get(k) for k in ['uuid','device','MAC','currently_attached','locking_mode']} for v in vifs.values() if v.get('VM') in selected]}
  results.append(row);out.write_text(json.dumps(results,indent=2)+'\n');print(json.dumps(row),flush=True)
 finally:rpc('session.logout',[s])
