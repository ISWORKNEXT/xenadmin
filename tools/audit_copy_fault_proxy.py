#!/usr/bin/env python3
"""One-shot loopback CONNECT relay for real .170 copy interruption testing.
Encrypted traffic is relayed without inspection. A new real VM.copy task triggers
closing only this relay's client sockets. No host networking is changed.
"""
import argparse,json,select,socket,subprocess,threading,time
parser=argparse.ArgumentParser();parser.add_argument("--port",type=int,default=0);parser.add_argument("--no-fault",action="store_true");args=parser.parse_args()
watch_script="""import XenAPI,json,time
s=XenAPI.xapi_local();s.xenapi.login_with_password('','','1.0','owned-copy-fault-observer')
try:
 baseline=set(s.xenapi.task.get_all());print('READY',flush=True)
 for _ in range(1200):
  for ref,task in s.xenapi.task.get_all_records().items():
   if ref not in baseline and 'VM.copy' in task['name_label']:
    print(json.dumps({'task_ref':ref,'name':task['name_label']}),flush=True);raise SystemExit(0)
  time.sleep(.05)
 raise RuntimeError('No new VM.copy task observed')
finally:s.xenapi.session.logout()
"""
watch=None
if not args.no_fault:
 watch=subprocess.Popen(['ssh','-o','BatchMode=yes','root@192.168.8.170','python3','-u','-'],stdin=subprocess.PIPE,stdout=subprocess.PIPE,stderr=subprocess.PIPE,text=True)
 watch.stdin.write(watch_script);watch.stdin.close()
 if watch.stdout.readline().strip()!='READY':raise RuntimeError('Task observer did not become ready')
listener=socket.socket();listener.setsockopt(socket.SOL_SOCKET,socket.SO_REUSEADDR,1);listener.bind(('127.0.0.1',args.port));listener.listen();listener.settimeout(.1)
stopped=threading.Event();sockets=[];lock=threading.Lock()
def relay(client):
 upstream=None
 try:
  client.settimeout(10);request=b''
  while b'\r\n\r\n' not in request:
   chunk=client.recv(1)
   if not chunk or len(request)>8192:raise RuntimeError('Invalid CONNECT request')
   request+=chunk
  if request.split(b'\r\n',1)[0]!=b'CONNECT 192.168.8.170:443 HTTP/1.1':raise RuntimeError('Only authorized dev HTTPS tunnel permitted')
  upstream=socket.create_connection(('192.168.8.170',443),timeout=10)
  with lock:sockets.extend([client,upstream])
  client.sendall(b'HTTP/1.1 200 Connection established\r\n\r\n')
  while not stopped.is_set():
   readable,_,_=select.select([client,upstream],[],[],.1)
   for src in readable:
    chunk=src.recv(65536)
    if not chunk:return
    (upstream if src is client else client).sendall(chunk)
 except OSError:
  if not stopped.is_set():raise
 finally:
  for sock in [client,upstream]:
   if sock:
    try:sock.close()
    except OSError:pass

def observe():
 result=watch.stdout.readline()
 if result:
  event=json.loads(result);time.sleep(.15)
  stopped.set()
  with lock:
   for sock in sockets:
    try:sock.shutdown(socket.SHUT_RDWR);sock.close()
    except OSError:pass
  print(json.dumps({'event':'relay-closed-after-real-copy-task','task':event}),flush=True)
 else:
  stopped.set();print(json.dumps({'event':'observer-failed'}),flush=True)
if watch is not None:threading.Thread(target=observe,daemon=True).start()
print(json.dumps({'port':listener.getsockname()[1]}),flush=True)
try:
 while not stopped.is_set():
  try:client,_=listener.accept()
  except socket.timeout:continue
  threading.Thread(target=relay,args=(client,),daemon=True).start()
finally:
 listener.close()
 if watch is not None:
  try:watch.wait(timeout=5)
  except subprocess.TimeoutExpired:watch.terminate();watch.wait(timeout=5)
