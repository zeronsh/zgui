#!/usr/bin/env python3
"""Owned X11/Wayland desktop: window controls and native application D-Bus events."""
import argparse, json, os, pathlib, re, select, shutil, subprocess, time
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
            x=launch('xvfb',['Xvfb','-r','-displayfd','1','-screen','0','1400x1000x24','-ac'],stdout=subprocess.PIPE,text=True)
            assert select.select([x.stdout],[],[],10)[0];env['DISPLAY']=':'+x.stdout.readline().strip()
            config=pathlib.Path(private,'bus.conf');config.write_text('<busconfig><type>session</type><listen>unix:tmpdir='+private+'</listen><auth>EXTERNAL</auth><policy context="default"><allow send_destination="*"/><allow receive_sender="*"/><allow own="*"/></policy></busconfig>')
            bus=launch('bus',['dbus-daemon','--nofork','--config-file='+str(config),'--print-address=1'],stdout=subprocess.PIPE,text=True)
            assert select.select([bus.stdout],[],[],10)[0];env['DBUS_SESSION_BUS_ADDRESS']=bus.stdout.readline().strip()
            openbox=launch('openbox',['openbox'])
            wait_for_openbox(env,a.output,openbox)
            host=None
            if a.wayland:
                env.update(WLR_BACKENDS='x11',WLR_X11_OUTPUTS='1',WLR_RENDERER='pixman')
                config=pathlib.Path(private,'sway.conf');config.write_text('xwayland disable\nseat seat0 fallback true\noutput * mode 1200x800\ndefault_border none\nfor_window [title="zgx native platform"] floating enable\n')
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
            app=launch('application',[str(a.binary.resolve()),'--smoke'],native)
            def wait(token):
                deadline=time.monotonic()+12
                while token not in (a.output/'application.log').read_text():
                    if time.monotonic()>deadline or app.poll() is not None:raise RuntimeError('Missing '+token)
                    time.sleep(.03)
            wait('CONTROLS_READY')
            def xdo(*args):subprocess.run(['xdotool',*map(str,args)],env=env,check=True)
            if a.wayland:
                tree=json.loads(subprocess.check_output(['swaymsg','-t','get_tree','-r'],env=env,text=True));pending=[tree]
                while pending:
                    node=pending.pop();pending.extend(node.get('nodes',[])+node.get('floating_nodes',[]))
                    if node.get('name')=='zgx native platform':rect=node['rect'];break
                xdo('windowactivate','--sync',host);xdo('mousemove','--window',host,rect['x']+90,rect['y']+16)
            else:
                host=subprocess.check_output(['xdotool','search','--name','^zgx native platform$'],env=env,text=True).strip().splitlines()[0]
                xdo('windowactivate','--sync',host);xdo('mousemove','--window',host,90,16)
            xdo('mousedown',1);time.sleep(.15);xdo('mousemove_relative','--sync',80,55);time.sleep(.15);xdo('mouseup',1)
            if a.wayland:
                time.sleep(.15)
                tree=json.loads(subprocess.check_output(['swaymsg','-t','get_tree','-r'],env=env,text=True));pending=[tree]
                while pending:
                    node=pending.pop();pending.extend(node.get('nodes',[])+node.get('floating_nodes',[]))
                    if node.get('name')=='zgx native platform':
                        assert (rect['x'],rect['y']) != (node['rect']['x'],node['rect']['y']), 'Custom titlebar drag did not move Wayland surface'
                        break
                else:raise RuntimeError('Dragged surface disappeared')
            wait('ROOT_CLOSED')
            def call(method,*args):
                return subprocess.check_output(['gdbus','call','--session','--dest','org.zgx.NativePlatform','--object-path','/org/zgx/NativePlatform','--method','org.freedesktop.Application.'+method,*args],env=env,text=True)
            call('Open',"['zgx-test:hello', 'https://example.invalid/path']",'{}');wait('zgx-test:hello')
            call('Activate','{}');wait('REOPENED')
            call('Open',"['zgx-test:quit']",'{}');app.wait(timeout=5);assert app.returncode==0
            log=(a.output/'application.log').read_text()
            assert 'FULLSCREEN true ' in log and 'RESTORED false ' in log,log
            minimum=re.search(r'MINIMUM WindowBounds \{ position: (.*?), size: \((\d+), (\d+)\)',log);assert minimum,log
            assert int(minimum[2])>=300 and int(minimum[3])>=220,minimum[0]
            after_size=re.search(r'AFTER_DRAG WindowBounds .*?size: \((\d+), (\d+)\)',log)
            assert after_size and int(after_size[1])>=300 and int(after_size[2])>=220,log
            initial=re.search(r'INITIAL WindowBounds .*?size: \((\d+), (\d+)\)',log)
            restored=re.search(r'RESTORED false WindowBounds .*?size: \((\d+), (\d+)\)',log)
            assert initial and restored and initial.groups()==restored.groups(),log
            if a.wayland:assert 'placement: false' in log and 'position: None' in log,log
            else:
                before=re.search(r'MINIMUM WindowBounds \{ position: Some\(\((\d+), (\d+)\)\)',log)
                after=re.search(r'AFTER_DRAG WindowBounds \{ position: Some\(\((\d+), (\d+)\)\)',log)
                assert before and after and before.groups()!=after.groups(),log
            (a.output/'result.json').write_text(json.dumps(dict(result='pass',backend='wayland' if a.wayland else 'x11',fullscreen=True,minimum_size=True,custom_titlebar_drag=True,open_urls=True,reopen_after_last_window_closed=True),indent=2)+'\n')
        finally:
            for child in reversed(processes):stop(child)
if __name__=='__main__':main()
