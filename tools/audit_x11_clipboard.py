#!/usr/bin/env python3
"""Serve/read the real clipboard on the isolated :98 QA display only."""
import json,os,select,subprocess,sys,time
from pathlib import Path
from Xlib import X,Xatom,display
from Xlib.protocol import event

def stop_owned_process(process, timeout=3):
 if process.poll() is None:
  try:process.terminate()
  except ProcessLookupError:pass
 try:process.wait(timeout=timeout)
 except subprocess.TimeoutExpired:
  process.kill();process.wait(timeout=timeout)

def start_clipboard(value, env):
 if env.get('DISPLAY')!=':98':raise RuntimeError('Use only private DISPLAY=:98')
 process=subprocess.Popen([sys.executable,str(Path(__file__).resolve()),'serve'],env=env,stdin=subprocess.PIPE,stdout=subprocess.PIPE,stderr=subprocess.PIPE,text=True)
 try:
  process.stdin.write(json.dumps(value)+'\n');process.stdin.close()
  deadline=time.monotonic()+3;line=bytearray()
  while not line.endswith(b'\n'):
   remaining=deadline-time.monotonic()
   if remaining<=0 or not select.select([process.stdout.fileno()],[],[],remaining)[0]:raise TimeoutError('Clipboard provider READY timed out')
   byte=os.read(process.stdout.fileno(),1)
   if not byte or len(line)>128:raise RuntimeError('Clipboard provider failed before READY')
   line.extend(byte)
  if line.strip()!=b'READY':raise RuntimeError('Unexpected clipboard provider response')
  return process
 except BaseException:
  stop_owned_process(process);raise

def main():
 if os.environ.get('DISPLAY')!=':98':raise RuntimeError('Use only private DISPLAY=:98')
 dpy=display.Display();root=dpy.screen().root
 window=root.create_window(0,0,1,1,0,X.CopyFromParent,X.InputOnly,X.CopyFromParent)
 clipboard=dpy.intern_atom('CLIPBOARD');utf8=dpy.intern_atom('UTF8_STRING');targets=dpy.intern_atom('TARGETS');text=dpy.intern_atom('TEXT')
 try:
  if sys.argv[1]=='serve':
   value=json.loads(sys.stdin.readline());assert isinstance(value,str) and len(value.encode('utf-8'))<=131072
   window.set_selection_owner(clipboard,X.CurrentTime);dpy.sync();assert dpy.get_selection_owner(clipboard)==window
   print('READY',flush=True)
   while True:
    request=dpy.next_event()
    if request.type==X.SelectionClear:break
    if request.type!=X.SelectionRequest:continue
    print(json.dumps({'selection_request':dpy.get_atom_name(request.target)}),flush=True)
    prop=request.property or request.target
    if request.target==targets:request.requestor.change_property(prop,Xatom.ATOM,32,[targets,utf8,Xatom.STRING,text])
    elif request.target in (utf8,text):request.requestor.change_property(prop,utf8,8,value.encode('utf-8'))
    elif request.target==Xatom.STRING:request.requestor.change_property(prop,Xatom.STRING,8,value.encode('latin-1',errors='replace'))
    else:prop=X.NONE
    request.requestor.send_event(event.SelectionNotify(time=request.time,requestor=request.requestor,selection=request.selection,target=request.target,property=prop));dpy.flush()
  elif sys.argv[1]=='read':
   prop=dpy.intern_atom('XEN_AUDIT_CLIPBOARD_RESULT');window.convert_selection(clipboard,utf8,prop,X.CurrentTime);dpy.flush();deadline=time.monotonic()+5
   while time.monotonic()<deadline:
    if not dpy.pending_events():
     if not select.select([dpy.fileno()],[],[],max(0,deadline-time.monotonic()))[0]:break
    result=dpy.next_event()
    if result.type!=X.SelectionNotify:continue
    assert result.property!=X.NONE,'Clipboard owner rejected UTF-8'
    value=window.get_full_property(prop,utf8);assert value is not None
    print(json.dumps(bytes(value.value).decode('utf-8')),flush=True);break
   else:raise TimeoutError('Clipboard selection timed out')
   if time.monotonic()>=deadline:raise TimeoutError('Clipboard selection timed out')
  else:raise ValueError('Choose serve or read')
 finally:window.destroy();dpy.close()

if __name__=='__main__':main()
