#!/usr/bin/env python3
"""Render and resize the retained layout gallery on an isolated X11 desktop."""
import argparse, json, os, select, subprocess, time
from pathlib import Path
from platform_smoke import stop, wait_for_wm

def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('binary',type=Path)
    parser.add_argument('--output',type=Path,default=Path('/tmp/zgx-layout-smoke'))
    args=parser.parse_args();args.output.mkdir(parents=True,exist_ok=True)
    xvfb=wm=app=None
    logs=[]
    try:
        logs=[(args.output/(name+'.log')).open('w') for name in ('xvfb','wm','app')]
        xvfb=subprocess.Popen(['Xvfb','-r','-displayfd','1','-screen','0','1200x900x24','-ac','-noreset'],stdout=subprocess.PIPE,stderr=logs[0],text=True)
        assert select.select([xvfb.stdout],[],[],10)[0], 'Xvfb startup timed out'
        env=dict(os.environ,DISPLAY=':'+xvfb.stdout.readline().strip(),DBUS_SESSION_BUS_ADDRESS='unix:path=/nonexistent')
        env.pop('WAYLAND_DISPLAY',None)
        wm=subprocess.Popen(['openbox'],env=env,stdout=logs[1],stderr=logs[1]);wait_for_wm(env)
        app=subprocess.Popen([str(args.binary.resolve())],env=env,stdout=logs[2],stderr=logs[2])
        def xdo(*args): return subprocess.run(['xdotool',*map(str,args)],env=env,check=True,capture_output=True,text=True).stdout.strip()
        deadline=time.monotonic()+20;window=None
        while time.monotonic()<deadline:
            result=subprocess.run(['xdotool','search','--onlyvisible','--name','^zgx retained layout$'],env=env,capture_output=True,text=True)
            if result.returncode==0: window=result.stdout.splitlines()[0];break
            assert app.poll() is None, 'Gallery exited during startup'
            time.sleep(.1)
        assert window, 'Gallery window missing'
        captures=[]
        for width,height in [(800,600),(560,520),(1000,720)]:
            xdo('windowsize',window,width,height);time.sleep(.7)
            assert app.poll() is None, 'Gallery exited on resize'
            path=args.output/f'{width}x{height}.png'
            # A window shows before its first frame does: a blank capture is one taken too soon.
            deadline=time.monotonic()+15
            while True:
                done=subprocess.run(['import','-window',window,str(path)],env=env,capture_output=True,text=True)
                if done.returncode==0 and path.stat().st_size>1000: break
                assert time.monotonic()<deadline, f'Empty screenshot: {done.stderr}'
                time.sleep(.2)
            captures.append({'size':[width,height],'file':path.name,'bytes':path.stat().st_size})
        (args.output/'result.json').write_text(json.dumps({'status':'passed','platform':'X11/Xvfb','captures':captures,'scope':'startup and three resizes; geometry assertions live in advanced_layout tests'},indent=2)+'\n')
    finally:
        for process in (app,wm,xvfb):
            if process: stop(process)
        for log in logs: log.close()
if __name__=='__main__': main()
