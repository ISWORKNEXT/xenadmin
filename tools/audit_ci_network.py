#!/usr/bin/env python3
"""Read-only bounded DNS/HTTPS diagnostics; never prints environment secrets."""
import argparse,json,os,platform,random,shutil,socket,struct,subprocess,time
from pathlib import Path

parser=argparse.ArgumentParser();parser.add_argument('--output',type=Path,required=True);args=parser.parse_args()
args.output.parent.mkdir(parents=True,exist_ok=True)
report={'platform':platform.platform(),'proxy_variables_present':{key:bool(os.environ.get(key)) for key in ('HTTP_PROXY','HTTPS_PROXY','NO_PROXY')},'system_dns':[],'direct_dns':[],'https':[]}
resolver=Path('/etc/resolv.conf')
if resolver.exists():report['nameservers']=[line.split()[1] for line in resolver.read_text().splitlines() if line.startswith('nameserver ') and len(line.split())>=2]
mirrors=Path('/etc/apt/apt-mirrors.txt')
if mirrors.exists():report['configured_mirrors']=mirrors.read_text().splitlines()
def save():args.output.write_text(json.dumps(report,indent=2)+'\n')
def command(argv):
 started=time.monotonic()
 try:
  result=subprocess.run(argv,text=True,capture_output=True,timeout=8)
  return {'exit_code':result.returncode,'stdout':result.stdout[-4000:],'stderr':result.stderr[-2000:],'elapsed_s':round(time.monotonic()-started,3)}
 except subprocess.TimeoutExpired:return {'timed_out':True,'elapsed_s':round(time.monotonic()-started,3)}
save()
for host in ('github.com','packages.microsoft.com','azure.archive.ubuntu.com','archive.ubuntu.com','security.ubuntu.com','pypi.org'):
 result={'host':host,**command(['getent','ahostsv4',host])};report['system_dns'].append(result);save();print(json.dumps({'system_dns':host,**{key:value for key,value in result.items() if key not in ('stdout','stderr','host')}}),flush=True)
for server in ('1.1.1.1','8.8.8.8'):
 for host in ('github.com','archive.ubuntu.com'):
  started=time.monotonic();result={'server':server,'host':host}
  try:
   transaction=random.randrange(65536);question=b''.join(bytes([len(label)])+label.encode('ascii') for label in host.split('.'))+b'\0'+struct.pack('!HH',1,1)
   packet=struct.pack('!HHHHHH',transaction,0x0100,1,0,0,0)+question
   with socket.socket(socket.AF_INET,socket.SOCK_DGRAM) as query:
    query.settimeout(3);query.connect((server,53));query.send(packet);answer=query.recv(4096)
   actual,flags,questions,answers,authority,additional=struct.unpack('!HHHHHH',answer[:12]);assert actual==transaction and flags&0x8000
   result.update(rcode=flags&15,answers=answers)
  except (OSError,ValueError,AssertionError,struct.error) as error:result['error']=type(error).__name__+': '+str(error)
  result['elapsed_s']=round(time.monotonic()-started,3);report['direct_dns'].append(result);save();print(json.dumps({'direct_dns':result}),flush=True)
if shutil.which('curl'):
 for url in ('https://github.com/','https://packages.microsoft.com/','https://archive.ubuntu.com/ubuntu/dists/noble/InRelease'):
  result={'url':url,**command(['curl','--silent','--show-error','--head','--location','--max-time','6','--connect-timeout','4','--output',os.devnull,'--write-out','%{http_code}',url])};report['https'].append(result);save();print(json.dumps({'https':result}),flush=True)
report['collection_complete']=True;save()
