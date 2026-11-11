#!/usr/bin/env python3
"""Read-only .170 inventory comparison against a saved qualification baseline."""
import argparse,json,subprocess
from pathlib import Path
parser=argparse.ArgumentParser()
parser.add_argument('baseline',type=Path)
parser.add_argument('output',type=Path)
args=parser.parse_args()
script="""import XenAPI,json
s=XenAPI.xapi_local();s.xenapi.login_with_password('','','1.0','parity-cleanup-readback')
try:
 print(json.dumps({'vm_uuids':sorted(s.xenapi.VM.get_uuid(r) for r in s.xenapi.VM.get_all()),'vdi_refs':sorted(s.xenapi.VDI.get_all()),'network_refs':sorted(s.xenapi.network.get_all())}))
finally:s.xenapi.session.logout()
"""
r=subprocess.run(['ssh','-o','BatchMode=yes','root@192.168.8.170','python3','-'],input=script,text=True,capture_output=True,check=True)
current=json.loads(r.stdout);baseline=json.loads(args.baseline.read_text())
keys=['vm_uuids','vdi_refs','network_refs']
result={k:{'baseline_count':len(baseline[k]),'current_count':len(current[k]),'added':sorted(set(current[k])-set(baseline[k])),'removed':sorted(set(baseline[k])-set(current[k]))} for k in keys}
result['sets_match']=all(set(baseline[k])==set(current[k]) for k in keys)
args.output.write_text(json.dumps(result,indent=2)+'\n')
print(json.dumps(result))
raise SystemExit(0 if result['sets_match'] else 1)
