#!/usr/bin/env python3
"""Credential-free real native Copy/Paste regression via the actual Connect dialog.
The X11 provider/consumer is independently validated; only private :98 is used.
"""
import argparse,hashlib,json,os,subprocess,sys,tempfile,time
from pathlib import Path
from PIL import Image
from Xlib import X,XK,display
from Xlib.ext import xtest
from audit_native_live import windows
from audit_x11_clipboard import start_clipboard,stop_owned_process
p=argparse.ArgumentParser();p.add_argument('--binary',type=Path,required=True);p.add_argument('--output',type=Path,required=True);p.add_argument('--inspect',action='store_true');args=p.parse_args()
assert os.environ.get('DISPLAY')==':98'
args.binary=args.binary.resolve();out=args.output.resolve();out.mkdir(parents=True,exist_ok=True);helper=Path(__file__).with_name('audit_x11_clipboard.py');clips=[]
expected='clipboard-ทดสอบ-café-Δ';sentinel='COPY_HAS_NOT_OCCURRED'
dpy=display.Display();root=dpy.screen().root
with tempfile.TemporaryDirectory(prefix='xen-clipboard-qa-') as config:
 env=dict(os.environ,XDG_CONFIG_HOME=config,LIBGL_ALWAYS_SOFTWARE='1',WINIT_UNIX_BACKEND='x11');env.pop('WAYLAND_DISPLAY',None)
 with (out/'app.log').open('w') as log:
  app=subprocess.Popen([str(args.binary),'--theme','light','--connect'],env=env,stdout=log,stderr=log)
  try:
   deadline=time.monotonic()+15;window=None
   while time.monotonic()<deadline:
    if app.poll() is not None:raise RuntimeError(f'Native app exited before mapping a window (code {app.returncode}); see app.log')
    for w in windows(root):
     try:
      prop=w.get_full_property(dpy.intern_atom('_NET_WM_PID'),0)
      if prop is not None and int(prop.value[0])==app.pid and w.get_geometry().width>100 and w.get_attributes().map_state==X.IsViewable:window=w;break
     except Exception:pass
    if window:break
    time.sleep(.1)
   assert window is not None,'Native window missing';window.configure(stack_mode=X.Above);window.set_input_focus(X.RevertToParent,X.CurrentTime);dpy.sync();time.sleep(1)
   def snap(name):
    g=window.get_geometry();raw=window.get_image(0,0,g.width,g.height,2,0xffffffff);Image.frombytes('RGB',(g.width,g.height),raw.data,'raw','BGRX').save(out/(name+'.png'))
   def key(name,down=True):xtest.fake_input(dpy,X.KeyPress if down else X.KeyRelease,dpy.keysym_to_keycode(XK.string_to_keysym(name)));dpy.sync()
   def chord(letter):
    key('Control_L');key(letter);key(letter,False);key('Control_L',False);time.sleep(.3)
   def serve(value):
    child=start_clipboard(value,env);clips.append(child);return child
   def read():return json.loads(subprocess.run([sys.executable,str(helper),'read'],env=env,text=True,capture_output=True,check=True,timeout=8).stdout)
   # --connect opens the real dialog; its Host field receives initial focus.
   snap('connect-initial')
   provenance={'binary':str(args.binary),'binary_sha256':hashlib.sha256(args.binary.read_bytes()).hexdigest(),'pid':app.pid,'display':':98','credentials_used':False,'host_connection_submitted':False}
   (out/'startup.json').write_text(json.dumps(provenance,indent=2)+'\n')
   if not args.inspect:
    source=serve(expected);assert read()==expected,'Independent clipboard provider failed'
    chord('a');chord('v');time.sleep(.3);snap('after-paste')
    serve(sentinel);assert read()==sentinel,'Clipboard sentinel failed'
    chord('a');chord('c');time.sleep(.3);actual=read();snap('after-copy')
    verdict={'passed':actual==expected,'expected':expected,'actual':actual,'provider_consumer_verified':True,'dialog_opened_via':'--connect','host_focused_on_open':True,**provenance}
    (out/'verdict.json').write_text(json.dumps(verdict,ensure_ascii=False,indent=2)+'\n');print(json.dumps(verdict,ensure_ascii=False),flush=True)
    if not verdict['passed']:raise SystemExit(1)
  finally:
   try:stop_owned_process(app,timeout=8)
   finally:
    for child in clips:stop_owned_process(child)
    (out/'process-cleanup.json').write_text(json.dumps({'app_pid':app.pid,'app_returncode':app.returncode,'clipboard_children':[{'pid':child.pid,'returncode':child.returncode} for child in clips],'all_children_stopped':all(child.poll() is not None for child in clips)},indent=2)+'\n')
    dpy.close()
