#!/usr/bin/env python3
"""Exercise typed drag acceptance, preview rendering and Escape cancellation on X11."""
import argparse, json, os, select, subprocess, time
from pathlib import Path
from platform_smoke import stop, wait_for_wm

def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('binary',type=Path)
    parser.add_argument('--output',type=Path,default=Path('/tmp/zgx-drag-smoke'))
    args=parser.parse_args();args.output.mkdir(parents=True,exist_ok=True)
    xvfb=wm=app=None
    logs=[]
    try:
        logs=[(args.output/(name+'.log')).open('w') for name in ('xvfb','wm','app')]
        xvfb=subprocess.Popen(['Xvfb','-displayfd','1','-screen','0','1200x900x24','-ac','-noreset'],stdout=subprocess.PIPE,stderr=logs[0],text=True)
        assert select.select([xvfb.stdout],[],[],10)[0], 'Xvfb startup timed out'
        env=dict(os.environ,DISPLAY=':'+xvfb.stdout.readline().strip(),DBUS_SESSION_BUS_ADDRESS='unix:path=/nonexistent')
        env.pop('WAYLAND_DISPLAY',None)
        wm=subprocess.Popen(['openbox'],env=env,stdout=logs[1],stderr=logs[1]);wait_for_wm(env)
        app=subprocess.Popen([str(args.binary.resolve())],env=env,stdout=logs[2],stderr=logs[2])
        def xdo(*args): return subprocess.run(['xdotool',*map(str,args)],env=env,check=True,capture_output=True,text=True).stdout.strip()
        deadline=time.monotonic()+20;window=None
        while time.monotonic()<deadline:
            result=subprocess.run(['xdotool','search','--name','^zgx typed drag$'],env=env,capture_output=True,text=True)
            if result.returncode==0: window=result.stdout.splitlines()[0];break
            assert app.poll() is None, 'Gallery exited during startup'
            time.sleep(.1)
        assert window, 'Gallery window missing'
        xdo('windowactivate','--sync',window);time.sleep(.6)
        xdo('mousemove','--window',window,60,110);xdo('mousedown',1);xdo('mousemove','--window',window,90,110);time.sleep(.3)
        subprocess.run(['import','-window',window,str(args.output/'preview.png')],env=env,check=True,capture_output=True)
        xdo('mousemove','--window',window,150,230);xdo('mouseup',1);time.sleep(.3)
        text=(args.output/'app.log').read_text();assert 'DRAG_DROP Red' in text and 'DRAG_END Red true' in text,text
        xdo('mousemove','--window',window,180,110);xdo('mousedown',1);xdo('mousemove','--window',window,195,110);xdo('key','Escape');xdo('mouseup',1);time.sleep(.3)
        text=(args.output/'app.log').read_text();assert 'DRAG_END Green false' in text,text
        assert text.count('DRAG_DROP')==1,text
        subprocess.run(['import','-window',window,str(args.output/'finished.png')],env=env,check=True,capture_output=True)
        assert app.poll() is None,'Gallery exited during drag'
        (args.output/'result.json').write_text(json.dumps({'status':'passed','platform':'X11/Xvfb','stages':['Red typed drop accepted','retained preview screenshot','Green Escape cancellation'],'scope':'Actual native pointer and keyboard events; external file DND is separate'},indent=2)+'\n')

    finally:
        for process in (app,wm,xvfb):
            if process: stop(process)
        for log in logs: log.close()
if __name__=='__main__': main()
