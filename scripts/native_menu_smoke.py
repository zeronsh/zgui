#!/usr/bin/env python3
"""Owned X11/Wayland desktop: dynamic menus, command state and native-key shortcuts."""
import argparse, ctypes, json, os, pathlib, re, select, shutil, subprocess, tempfile, time
from platform_smoke import stop
from sway_host import wait_for_openbox, wait_for_sway_host

def main():
    p=argparse.ArgumentParser(description=__doc__)
    p.add_argument('binary',type=pathlib.Path);p.add_argument('--output',type=pathlib.Path,required=True)
    p.add_argument('--wayland',action='store_true');p.add_argument('--overlap-debug',action='store_true');a=p.parse_args();a.output.mkdir(parents=True,exist_ok=True)
    processes=[]
    with tempfile.TemporaryDirectory(prefix='zgx-platform-') as private:
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
                config=pathlib.Path(private,'sway.conf');config.write_text('xwayland disable\nseat seat0 fallback true\noutput * mode 1200x800\ndefault_border none\nfor_window [title="zgx dynamic menus.*"] floating enable\n')
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
            def permit_debugger():ctypes.CDLL(None).prctl(0x59616d61,ctypes.c_ulong(-1),0,0,0)
            app=launch('application',[str(a.binary.resolve())],native,preexec_fn=permit_debugger)
            def wait(token):
                deadline=time.monotonic()+10
                while token not in (a.output/'application.log').read_text():
                    if time.monotonic()>deadline or app.poll() is not None:
                        if a.overlap_debug and app.poll() is None:
                            with (a.output/'backtrace.log').open('w') as trace:subprocess.run(['gdb','-q','-batch','-ex','set pagination off','-ex','thread apply all bt 20','-p',str(app.pid)],stdout=trace,stderr=subprocess.STDOUT,timeout=15)
                        raise RuntimeError('Missing '+token)
                    time.sleep(.03)
            def xdo(*args):subprocess.run(['xdotool',*map(str,args)],env=env,check=True)
            def focus(title):
                if a.wayland:
                    deadline=time.monotonic()+10
                    while subprocess.run(['swaymsg','[title="^'+title+'$"]','focus'],env=env,stdout=subprocess.DEVNULL).returncode:
                        if time.monotonic()>deadline:raise RuntimeError('Missing native menu window '+title)
                        time.sleep(.05)
                    xdo('windowactivate','--sync',host)
                else:
                    deadline=time.monotonic()+20
                    while True:
                        found=subprocess.run(['xdotool','search','--name','^'+title+'$'],env=env,text=True,stdout=subprocess.PIPE)
                        if found.returncode==0:target=found.stdout.strip().splitlines()[0];break
                        if time.monotonic()>deadline:raise RuntimeError('Missing native menu window '+title)
                        time.sleep(.05)
                    xdo('windowactivate','--sync',target)
                time.sleep(.12)
            def key(name):xdo('key','--clearmodifiers',name)
            wait('READY');focus('zgx dynamic menus secondary')
            if a.wayland and not a.overlap_debug:
                subprocess.run(['swaymsg','[title="^zgx dynamic menus$"]','move','position','20','40'],env=env,check=True,stdout=subprocess.DEVNULL)
                subprocess.run(['swaymsg','[title="^zgx dynamic menus secondary$"]','move','position','600','40'],env=env,check=True,stdout=subprocess.DEVNULL)
            elif not a.overlap_debug:
                for title,x in [('zgx dynamic menus',20),('zgx dynamic menus secondary',600)]:
                    target=subprocess.check_output(['xdotool','search','--name','^'+title+'$'],env=env,text=True).strip().splitlines()[0];xdo('windowmove',target,x,40)
            focus('zgx dynamic menus')
            key('ctrl+s');time.sleep(.15);assert 'ACTION save' not in (a.output/'application.log').read_text()
            key('ctrl+b');wait('CHECKED true');key('ctrl+s');wait('ACTION save')
            focus('zgx dynamic menus secondary');key('ctrl+s');wait('SECOND save')
            focus('zgx dynamic menus');key('ctrl+b');wait('CHECKED false');key('ctrl+s');time.sleep(.2)
            assert (a.output/'application.log').read_text().count('ACTION save')==1
            key('ctrl+r');wait('REPLACED');key('ctrl+s');time.sleep(.2)
            assert (a.output/'application.log').read_text().count('ACTION save')==1
            key('ctrl+d');wait('ACTION new')
            focus('zgx dynamic menus secondary');key('ctrl+d');wait('SECOND new')
            if a.wayland:
                subprocess.run(['grim',str(a.output.resolve()/'replaced.png')],env=env,check=True)
                subprocess.run(['swaymsg','[pid='+str(app.pid)+']','kill'],env=env,check=True,stdout=subprocess.DEVNULL)
            else:
                for title in ['zgx dynamic menus','zgx dynamic menus secondary']:
                    focus(title);key('alt+F4')
            app.wait(timeout=5);assert app.returncode==0
            (a.output/'result.json').write_text(json.dumps(dict(result='pass',backend='wayland' if a.wayland else 'x11',disabled_shortcut_suppressed=True,checked_state_updated=True,replacement_removed_old_shortcut=True,existing_windows_updated=True),indent=2)+'\n')
        finally:
            for child in reversed(processes):stop(child)
if __name__=='__main__':main()
