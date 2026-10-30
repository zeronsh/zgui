#!/usr/bin/env python3
"""Exercise grouped native file drop from a real GTK source on X11.

The app last sees the pointer over a card, outside the drop target: an XDND
drag holds the pointer, so the window hears no motion while it is over it,
and the files must drop where the drag is let go of (its XdndPosition), not
where the pointer was last seen. `--scale 1.5` runs it at that scale
(WINIT_X11_SCALE_FACTOR), the drop point scaled with it."""
import argparse, json, os, select, subprocess, time
from pathlib import Path
from platform_smoke import stop, wait_for_wm

def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('binary',type=Path)
    parser.add_argument('--output',type=Path,default=Path('/tmp/zgx-files-smoke'))
    parser.add_argument('--scale',type=float,default=1.0)
    args=parser.parse_args();args.output.mkdir(parents=True,exist_ok=True)
    xvfb=wm=app=source=None
    logs=[]
    try:
        logs=[(args.output/(name+'.log')).open('w') for name in ('xvfb','wm','app')]
        xvfb=subprocess.Popen(['Xvfb','-r','-displayfd','1','-screen','0','1200x900x24','-ac','-noreset'],stdout=subprocess.PIPE,stderr=logs[0],text=True)
        assert select.select([xvfb.stdout],[],[],10)[0], 'Xvfb startup timed out'
        env=dict(os.environ,DISPLAY=':'+xvfb.stdout.readline().strip(),DBUS_SESSION_BUS_ADDRESS='unix:path=/nonexistent')
        env.pop('WAYLAND_DISPLAY',None)
        wm=subprocess.Popen(['openbox'],env=env,stdout=logs[1],stderr=logs[1]);wait_for_wm(env)
        app_env=dict(env,WINIT_X11_SCALE_FACTOR=str(args.scale))
        app=subprocess.Popen([str(args.binary.resolve())],env=app_env,stdout=logs[2],stderr=logs[2])
        def xdo(*args): return subprocess.run(['xdotool',*map(str,args)],env=env,check=True,capture_output=True,text=True).stdout.strip()
        deadline=time.monotonic()+20;window=None
        while time.monotonic()<deadline:
            result=subprocess.run(['xdotool','search','--onlyvisible','--name','^zgx typed drag$'],env=env,capture_output=True,text=True)
            if result.returncode==0: window=result.stdout.splitlines()[0];break
            assert app.poll() is None, 'Gallery exited during startup'
            time.sleep(.1)
        assert window, 'Gallery window missing'
        files=[args.output/'first.txt',args.output/'second file.txt']
        for path in files:path.write_text('owned file drag fixture\n')
        source_log=(args.output/'source.log').open('w');logs.append(source_log)
        source=subprocess.Popen(['python3',str(Path(__file__).with_name('native_file_source.py')),*map(str,files)],env=env,stdout=source_log,stderr=source_log)
        deadline=time.monotonic()+10;source_window=None
        while time.monotonic()<deadline:
            result=subprocess.run(['xdotool','search','--onlyvisible','--name','^zgx file source$'],env=env,capture_output=True,text=True)
            if result.returncode==0:source_window=result.stdout.splitlines()[0];break
            time.sleep(.1)
        assert source_window,'GTK source missing'
        xdo('windowmove',window,400,100);xdo('windowmove',source_window,20,100);time.sleep(.5)
        # The drop target's middle (logical 190, 240) and the Red card's (70,
        # 110), in the window's pixels at the scale.
        target=(round(190*args.scale),round(240*args.scale))
        card=(round(70*args.scale),round(110*args.scale))
        # Last seen over the card: a drop at the stale pointer misses the target.
        xdo('mousemove','--window',window,*card);time.sleep(.3)
        xdo('mousemove','--window',source_window,80,60);xdo('mousedown',1)
        for x,y in [(110,60),(160,90),(260,160),(400,220)]:
            xdo('mousemove','--window',source_window,x,y);time.sleep(.15)
        xdo('mousemove','--window',window,*target);time.sleep(.6);xdo('mouseup',1)
        # What the drop did, once the app has said so: a loaded runner takes a while.
        deadline=time.monotonic()+15
        while 'FILE_DROP' not in (text:=(args.output/'app.log').read_text()) and time.monotonic()<deadline: time.sleep(.1)
        assert text.count('FILE_DROP')==1,text
        assert 'first.txt' in text and 'second file.txt' in text,text
        subprocess.run(['import','-window',window,str(args.output/'dropped.png')],env=env,check=True,capture_output=True)
        cancellations=text.count('FILE_CANCEL')
        xdo('mousemove','--window',source_window,80,60);xdo('mousedown',1)
        for x,y in [(110,60),(160,90),(260,160),(400,220)]:
            xdo('mousemove','--window',source_window,x,y);time.sleep(.15)
        xdo('mousemove','--window',window,*target);time.sleep(.5);xdo('key','Escape');xdo('mouseup',1);time.sleep(.6)
        text=(args.output/'app.log').read_text();assert text.count('FILE_DROP')==1,text
        assert text.count('FILE_CANCEL')>cancellations,text

        (args.output/'result.json').write_text(json.dumps({'status':'passed','platform':'X11/Xvfb + GTK3 Xdnd source','files':[str(path) for path in files],'scale':args.scale,'scope':'real two-file drop at the drag point with the pointer last over a card, Escape cancellation into component target; percent-encoded filename space'},indent=2)+'\n')

    finally:
        for process in (source,app,wm,xvfb):
            if process: stop(process)
        for log in logs: log.close()
if __name__=='__main__': main()
