#!/usr/bin/env python3
"""Owned X11/Wayland desktop: actual Wayland URI-list file-drop transport."""
import argparse, json, os, pathlib, re, select, shutil, subprocess, time
from platform_smoke import private_runtime, stop
from sway_host import wait_for_openbox, wait_for_sway_host

def main():
    p=argparse.ArgumentParser(description=__doc__)
    p.add_argument('binary',type=pathlib.Path);p.add_argument('--output',type=pathlib.Path,required=True)
    p.set_defaults(wayland=True);a=p.parse_args();a.output.mkdir(parents=True,exist_ok=True)
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
                config=pathlib.Path(private,'sway.conf');config.write_text('xwayland disable\nseat seat0 fallback true\noutput * mode 1200x800\ndefault_border none\nfor_window [title="zgx typed drag"] floating enable\nfor_window [title="zgx file source"] floating enable\n')
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
            app=launch('application',[str(a.binary.resolve())],native)
            def node(title):
                deadline=time.monotonic()+15
                while True:
                    tree=json.loads(subprocess.check_output(['swaymsg','-t','get_tree','-r'],env=env,text=True));pending=[tree]
                    while pending:
                        item=pending.pop();pending.extend(item.get('nodes',[])+item.get('floating_nodes',[]))
                        if item.get('name')==title:return item
                    if time.monotonic()>deadline:raise RuntimeError('Missing '+title)
                    time.sleep(.05)
            node('zgx typed drag')
            files=[a.output.resolve()/'first.txt',a.output.resolve()/'second file.txt']
            for path in files:path.write_text('owned native Wayland file drag fixture\n')
            source_env=dict(native,GDK_BACKEND='wayland')
            source=launch('source',['/usr/bin/python3',str(pathlib.Path(__file__).with_name('native_file_source.py')),*map(str,files)],source_env)
            node('zgx file source')
            for title,x in [('zgx typed drag',400),('zgx file source',20)]:subprocess.run(['swaymsg','[title="^'+title+'$"]','move','position',str(x),'100'],env=env,check=True,stdout=subprocess.DEVNULL)
            time.sleep(.2)
            target=node('zgx typed drag')['rect'];origin=node('zgx file source')['rect']
            def xdo(*args):subprocess.run(['xdotool',*map(str,args)],env=env,check=True)
            xdo('windowactivate','--sync',host)
            def move(x,y):xdo('mousemove','--window',host,x,y)
            def drag(cancel=False, accepted_region=True):
                move(origin['x']+80,origin['y']+60);xdo('mousedown',1)
                for dx,dy in [(110,60),(160,90),(260,160)]:move(origin['x']+dx,origin['y']+dy);time.sleep(.15)
                move(target['x']+(150 if accepted_region else 450),target['y']+(230 if accepted_region else 350));time.sleep(.5)
                if cancel:move(1100,700);time.sleep(.2)
                xdo('mouseup',1)
            def wait(token):
                deadline=time.monotonic()+8
                while token not in (a.output/'application.log').read_text():
                    if time.monotonic()>deadline:
                        subprocess.run(['grim',str(a.output.resolve()/'timeout.png')],env=env,check=False)
                        raise RuntimeError('Missing '+token)
                    time.sleep(.05)
            drag();wait('FILE_DROP')
            trace=(a.output/'application.log').read_text();assert trace.count('FILE_DROP')==1
            for path in files:assert str(path) in trace
            subprocess.run(['grim',str(a.output.resolve()/'dropped.png')],env=env,check=True)
            count=trace.count('FILE_CANCEL');drag(True)
            deadline=time.monotonic()+5
            while (a.output/'application.log').read_text().count('FILE_CANCEL')<=count:
                if time.monotonic()>deadline:raise RuntimeError('Missing cancellation after leaving target')
                time.sleep(.05)
            trace=(a.output/'application.log').read_text();assert trace.count('FILE_DROP')==1
            finishes=trace.count('.finish()');ended=(a.output/'source.log').read_text().count('SOURCE_END')
            drag(accepted_region=False)
            deadline=time.monotonic()+5
            while (a.output/'source.log').read_text().count('SOURCE_END')<=ended:
                if time.monotonic()>deadline:raise RuntimeError('Rejected drop source did not complete')
                time.sleep(.05)
            trace=(a.output/'application.log').read_text();assert trace.count('FILE_DROP')==1
            assert trace.count('.finish()')==finishes, 'Unaccepted region falsely finished transfer'
            assert '.receive("text/uri-list"' in trace and '.finish()' in trace, 'Missing real data-offer transfer/finish'
            subprocess.run(['swaymsg','[pid='+str(app.pid)+']','kill'],env=env,check=True,stdout=subprocess.DEVNULL);app.wait(timeout=5);assert app.returncode==0
            (a.output/'result.json').write_text(json.dumps(dict(result='pass',backend='native Wayland',files=list(map(str,files)),grouped_drop_count=1,leave_cancellation=True,unaccepted_region_not_finished=True,client_display_unset=True),indent=2)+'\n')
        finally:
            for child in reversed(processes):stop(child)
if __name__=='__main__':main()
