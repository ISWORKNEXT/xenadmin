#!/usr/bin/env python3
"""Interactive real-host native GUI audit. Credentials stdin only. No fixtures.
Mutations go through native UI input. The independent readback only reports owned zz-parity-advanced-* guests.
Requires python-xlib and Pillow, an X11 display and XEN_AUDIT_OUTPUT.
Select a VM, host, SR or network with XEN_AUDIT_*_UUID; omit for the pool.
"""
import getpass,json,os,ssl,subprocess,tempfile,time,urllib.request
from pathlib import Path
from PIL import Image
from Xlib import X,XK,display
from Xlib.ext import xtest
from audit_native_live import windows
repo=Path(__file__).resolve().parents[1]
out=Path(os.environ['XEN_AUDIT_OUTPUT']).resolve();out.mkdir(parents=True,exist_ok=True)
password=getpass.getpass('Authorized dev-host password (hidden): ')
dpy=display.Display();root=dpy.screen().root
ctx=ssl._create_unverified_context();seq=0

def rpc(method,params):
 global seq
 seq+=1
 req=urllib.request.Request('https://192.168.8.170/jsonrpc',data=json.dumps({'jsonrpc':'2.0','id':seq,'method':method,'params':params}).encode(),headers={'Content-Type':'application/json'})
 with urllib.request.urlopen(req,context=ctx,timeout=45) as r:v=json.load(r)
 if v.get('error'):raise RuntimeError('Read verification failed '+method)
 return v['result']

def readback():
 s=rpc('session.login_with_password',['root',password,'1.0','UI audit read verification'])
 try:
  vms=rpc('VM.get_all_records',[s]);res=[]
  for ref,v in vms.items():
   if v.get('name_label','').startswith('zz-parity-advanced-'):
    res.append({k:v.get(k) for k in ['uuid','name_label','power_state','is_a_template','is_a_snapshot','VCPUs_max','VCPUs_at_startup','memory_static_max','memory_dynamic_min','memory_dynamic_max']})
  return res
 finally:rpc('session.logout',[s])

with tempfile.TemporaryDirectory(prefix='xen-live-clicks-') as t:
 env=os.environ.copy();env.pop('WAYLAND_DISPLAY',None);env.update(XDG_CONFIG_HOME=t,WINIT_UNIX_BACKEND='x11')
 log=(out/'app.log').open('w')
 targets=[(flag,os.environ[key]) for key,flag in [('XEN_AUDIT_VM_UUID','--vm'),('XEN_AUDIT_HOST_UUID','--host'),('XEN_AUDIT_SR_UUID','--sr'),('XEN_AUDIT_NETWORK_UUID','--network')] if key in os.environ]
 if len(targets)>1:raise ValueError('Choose one audit object; omit all object UUIDs for the pool view')
 args=[os.environ.get('XEN_AUDIT_BINARY', str(repo/'target/debug/xen-native')),'--connect-stdin']
 for flag,value in targets:args.extend([flag,value])
 app=subprocess.Popen(args,env=env,stdin=subprocess.PIPE,stdout=log,stderr=log,text=True)
 app.stdin.write(json.dumps({'settings':{'host':'192.168.8.170','username':'root','label':'AUTHORIZED DEV UI AUDIT','read_only':False,'tls':{'bypass_certificate_verification':True}},'password':password})+'\n');app.stdin.close();time.sleep(7)
 def window():
  for w in windows(root):
   try:
    p=w.get_full_property(dpy.intern_atom('_NET_WM_PID'),0)
    if p is not None and int(p.value[0])==app.pid and w.get_geometry().width>100:return w
   except Exception:pass
  raise RuntimeError('app window missing')
 def click(x,y,button=1):
  w=window();w.configure(stack_mode=X.Above);w.set_input_focus(X.RevertToParent,X.CurrentTime);dpy.sync()
  pos=w.translate_coords(root,x,y)
  xtest.fake_input(dpy,X.MotionNotify,x=pos.x,y=pos.y);dpy.sync();time.sleep(1)
  xtest.fake_input(dpy,X.ButtonPress,button);dpy.sync();time.sleep(0.1);xtest.fake_input(dpy,X.ButtonRelease,button);dpy.sync()
 def key(name,press=True):
  code=dpy.keysym_to_keycode(XK.string_to_keysym(name))
  xtest.fake_input(dpy,X.KeyPress if press else X.KeyRelease,code);dpy.sync()
 def text(value):
  code=255; original=dpy.get_keyboard_mapping(code,1)
  try:
   for c in value:
    sym=ord(c)
    dpy.change_keyboard_mapping(code,[(sym,)*len(original[0])]);dpy.sync();time.sleep(0.9)
    xtest.fake_input(dpy,X.KeyPress,code);xtest.fake_input(dpy,X.KeyRelease,code);dpy.sync();time.sleep(0.9)
  finally:
   dpy.change_keyboard_mapping(code,original);dpy.sync()
 def snap(name):
  w=window();g=w.get_geometry();raw=w.get_image(0,0,g.width,g.height,2,0xffffffff)
  Image.frombytes('RGB',(g.width,g.height),raw.data,'raw','BGRX').save(out/(name+'.png'))
 history=[]
 print('READY',flush=True)
 try:
  while True:
   line=input()
   c=None
   try:
    c=json.loads(line)
    if not isinstance(c,dict):raise ValueError('Provide a JSON object')
    if 'hover' in c:
     w=window();w.set_input_focus(X.RevertToParent,X.CurrentTime);dpy.sync()
     pos=w.translate_coords(root,*c['hover']);xtest.fake_input(dpy,X.MotionNotify,x=pos.x,y=pos.y);dpy.sync()
    if 'size' in c:
     width,height=map(int,c['size'])
     if not 800<=width<=3840 or not 600<=height<=2160:raise ValueError('Audit window size is out of range')
     window().configure(width=width,height=height);dpy.sync()
    if 'scroll' in c:
     steps=int(c['scroll'])
     if abs(steps)>500:raise ValueError('At most 500 scroll steps per command')
     button=5 if steps>0 else 4
     for _ in range(abs(steps)):
      xtest.fake_input(dpy,X.ButtonPress,button);xtest.fake_input(dpy,X.ButtonRelease,button);dpy.sync();time.sleep(0.03)
    if 'click' in c:
     click(*c['click']);time.sleep(0.5)
    if 'key' in c:
     names=c['key'].split('+')
     for n in names:key(n)
     for n in names[::-1]:key(n,False)
    if 'text' in c:text(c['text'])
    time.sleep(c.get('wait',2))
    if 'snap' in c:snap(c['snap'])
    result={'ok':True,'action':c}
    if c.get('readback'):result['readback']=readback()
    history.append(result);(out/'actions.json').write_text(json.dumps(history,indent=2)+'\n')
    print(json.dumps(result),flush=True)
    if c.get('quit'):break
   except Exception as e:
    result={'ok':False,'action':c,'error':type(e).__name__+': '+str(e)}
    history.append(result);(out/'actions.json').write_text(json.dumps(history,indent=2)+'\n')
    print(json.dumps(result),flush=True)
 finally:
  app.terminate();app.wait(timeout=8);log.close()
