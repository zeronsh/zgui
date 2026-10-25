#!/usr/bin/env python3
"""Owned X11/Wayland desktop: covered-window responsiveness and bounded native stack capture."""
import argparse, ctypes, json, os, pathlib, re, select, shutil, subprocess, time
from platform_smoke import private_runtime, stop
from sway_host import wait_for_openbox, wait_for_sway_host

def main():
    p=argparse.ArgumentParser(description=__doc__)
    p.add_argument('binary',type=pathlib.Path);p.add_argument('--output',type=pathlib.Path,required=True)
    p.add_argument('--wayland',action='store_true');a=p.parse_args();a.output.mkdir(parents=True,exist_ok=True)
    processes=[]
    with private_runtime('zgx-platform-') as private:
        env=dict(os.environ,XDG_RUNTIME_DIR=private,XDG_CONFIG_HOME=private+'/config',XDG_DATA_HOME=private+'/data',XDG_CACHE_HOME=private+'/cache')
        for key in ('DISPLAY','WAYLAND_DISPLAY','SWAYSOCK','DBUS_SESSION_BUS_ADDRESS','AT_SPI_BUS_ADDRESS'):env.pop(key,None)
        def launch(name,cmd,environment=None,**kw):
            with (a.output/(name+'.log')).open('w') as log:
                child=subprocess.Popen(cmd,env=environment or env,stderr=log,stdout=kw.pop('stdout',log),**kw)
            processes.append(child);return child
        try:
            x=launch('xvfb',['Xvfb','-displayfd','1','-screen','0','1400x1000x24','-ac'],stdout=subprocess.PIPE,text=True)
            assert select.select([x.stdout],[],[],10)[0];env['DISPLAY']=':'+x.stdout.readline().strip()
            config=pathlib.Path(private,'bus.conf');config.write_text('<busconfig><type>session</type><listen>unix:tmpdir='+private+'</listen><auth>EXTERNAL</auth><policy context="default"><allow send_destination="*"/><allow receive_sender="*"/><allow own="*"/></policy></busconfig>')
            bus=launch('bus',['dbus-daemon','--nofork','--config-file='+str(config),'--print-address=1'],stdout=subprocess.PIPE,text=True)
            assert select.select([bus.stdout],[],[],10)[0];env['DBUS_SESSION_BUS_ADDRESS']=bus.stdout.readline().strip()
            openbox=launch('openbox',['openbox'])
            wait_for_openbox(env,a.output,openbox)
            host=None
            if a.wayland:
                env.update(WLR_BACKENDS='x11',WLR_X11_OUTPUTS='1',WLR_RENDERER='pixman')
                config=pathlib.Path(private,'sway.conf');config.write_text('xwayland disable\nseat seat0 fallback true\noutput * mode 1200x800\ndefault_border none\nfor_window [title="zgx cover.* window"] floating enable\n')
                (a.output/'sway.conf').write_text(config.read_text());sway=launch('sway',['sway','--unsupported-gpu','--config',str(config)])
                host=wait_for_sway_host(env,a.output,sway)
                deadline=time.monotonic()+10
                while True:
                    sockets=list(pathlib.Path(private).glob('sway-ipc*.sock'));displays=[x for x in pathlib.Path(private).glob('wayland-*') if x.suffix!='.lock']
                    if sockets and displays:break
                    if time.monotonic()>deadline:raise RuntimeError('Sway startup timeout')
                    time.sleep(.05)
                env.update(SWAYSOCK=str(sockets[0]),WAYLAND_DISPLAY=displays[0].name)
            native=dict(env,WINIT_UNIX_BACKEND='wayland' if a.wayland else 'x11')
            if a.wayland:native.pop('DISPLAY',None);native['WAYLAND_DEBUG']='1'
            def permit_debugger():
                # Diagnostic child only: permit same-user gdb to capture its stalled stack.
                ctypes.CDLL(None).prctl(0x59616d61,ctypes.c_ulong(-1),0,0,0)
            app=launch('application',[str(a.binary.resolve())],native,preexec_fn=permit_debugger)
            try:app.wait(timeout=14)
            except subprocess.TimeoutExpired:
                with (a.output/'backtrace.log').open('w') as trace:
                    subprocess.run(['gdb','-q','-batch','-ex','set pagination off','-ex','thread apply all bt 15','-p',str(app.pid)],stdout=trace,stderr=subprocess.STDOUT,timeout=15)
                (a.output/'result.json').write_text(json.dumps(dict(result='stalled',application_alive=True,stack='backtrace.log'),indent=2)+'\n')
                return
            log=(a.output/'application.log').read_text()
            assert app.returncode==0 and 'COMPLETE' in log and 'TICK covered 24' in log and 'TICK covering 24' in log,log
            (a.output/'result.json').write_text(json.dumps(dict(result='pass',both_windows_updated=24,event_loop_completed=True),indent=2)+'\n')
        finally:
            for child in reversed(processes):stop(child)
if __name__=='__main__':main()
