#!/usr/bin/env python3
"""Diagnose unchanged 90 ms Pinyin bursts with direct winit and private Fcitx5.

Every session owns its compositor, input method, D-Bus, and configuration.
Observed input loss is recorded as an outcome, never converted into a pass.
"""
import argparse
import hashlib
import sys
import json
import os
import pathlib
import re
import select
import shutil
import signal
import subprocess
import tempfile
import time

from platform_smoke import stop


def strings(trace, method):
    return [json.loads(value) for value in re.findall(r'\.'+method+r'\(("(?:[^"\\]|\\.)*")', trace)]


def run_session(args, mode, index, cursor_delay_ms):
    output=args.output/f'{mode}-delay{cursor_delay_ms}-{index}'
    output.mkdir(parents=True, exist_ok=True)
    (output/'result.json').unlink(missing_ok=True)
    app = ime = compositor = wm = xvfb = bus = None
    with tempfile.TemporaryDirectory(prefix='zgx-wayland-ime-burst-') as private:
        env = dict(os.environ, XDG_RUNTIME_DIR=private, XDG_CONFIG_HOME=private+'/config',
                   XDG_CACHE_HOME=private+'/cache', XDG_DATA_HOME=private+'/data',
                   WLR_BACKENDS='x11', WLR_X11_OUTPUTS='1', WLR_RENDERER='pixman', LANG='C.UTF-8')
        for name in ('DISPLAY', 'WAYLAND_DISPLAY', 'SWAYSOCK', 'I3SOCK', 'IBUS_ADDRESS', 'XMODIFIERS', 'GTK_IM_MODULE', 'QT_IM_MODULE', 'QT_IM_MODULES', 'SDL_IM_MODULE', 'AT_SPI_BUS_ADDRESS', 'FCITX_DBUS_ADDRESS'):
            env.pop(name, None)
        profile = pathlib.Path(private, 'config/fcitx5/profile')
        profile.parent.mkdir(parents=True)
        profile.write_text('[Groups/0]\nName=Default\nDefault Layout=us\nDefaultIM=pinyin\n\n[Groups/0/Items/0]\nName=keyboard-us\nLayout=\n\n[Groups/0/Items/1]\nName=pinyin\nLayout=\n\n[GroupOrder]\n0=Default\n')
        config = pathlib.Path(private, 'sway.conf')
        config.write_text('xwayland disable\nseat seat0 fallback true\noutput * mode 1200x800\ndefault_border none\nfor_window [title="zgx direct winit IME probe"] fullscreen enable\n')
        (output/'profile').write_text(profile.read_text())
        (output/'sway.conf').write_text(config.read_text())
        bus_config = pathlib.Path(private, 'dbus.conf')
        # No activation directories: desktop portals must not mount into this
        # disposable runtime or start unrelated services during the probe.
        bus_config.write_text('<busconfig><type>session</type><listen>unix:tmpdir=' + private + '</listen><auth>EXTERNAL</auth><policy context="default"><allow send_destination="*"/><allow receive_sender="*"/><allow own="*"/></policy></busconfig>')
        def launch(name, command, process_env=env, **kwargs):
            with (output/(name+'.log')).open('w') as log:
                return subprocess.Popen(command, env=process_env, stderr=log, stdout=kwargs.pop('stdout', log), **kwargs)
        try:
            xvfb = launch('xvfb', ['Xvfb','-displayfd','1','-screen','0','1400x1000x24','-ac'], stdout=subprocess.PIPE, text=True)
            if not select.select([xvfb.stdout], [], [], 10)[0]: raise RuntimeError('Xvfb timeout')
            env['DISPLAY'] = ':'+xvfb.stdout.readline().strip()
            bus = launch('dbus', ['dbus-daemon','--config-file='+str(bus_config),'--nofork','--print-address=1'], stdout=subprocess.PIPE, text=True, start_new_session=True)
            if not select.select([bus.stdout], [], [], 10)[0]: raise RuntimeError('D-Bus timeout')
            env['DBUS_SESSION_BUS_ADDRESS'] = bus.stdout.readline().strip()
            wm = launch('openbox', ['openbox'])
            time.sleep(.4)
            compositor = launch('sway', ['sway','--unsupported-gpu','--config',str(config)])
            deadline = time.monotonic()+10
            while True:
                sockets = list(pathlib.Path(private).glob('sway-ipc*.sock'))
                displays = [p for p in pathlib.Path(private).glob('wayland-*') if p.suffix != '.lock']
                if sockets and displays: break
                if time.monotonic()>deadline or compositor.poll() is not None: raise RuntimeError('Sway failed')
                time.sleep(.05)
            env.update(WAYLAND_DISPLAY=displays[0].name, SWAYSOCK=str(sockets[0]))
            deadline = time.monotonic()+8
            while True:
                tree = subprocess.check_output(['xwininfo','-root','-tree'], env=env, text=True)
                match = re.search(r'(0x[0-9a-f]+) "wlroots - X11-1".*1200x800', tree)
                if match: break
                if time.monotonic()>deadline: raise RuntimeError('Sway host window missing')
                time.sleep(.05)
            window = match.group(1)
            def xdo(*parts):
                subprocess.run(['xdotool', *map(str, parts)], env=env, check=True)
            xdo('windowfocus','--sync',window)
            native = dict(env, WAYLAND_DEBUG='1', WINIT_UNIX_BACKEND='wayland')
            native.pop('DISPLAY', None)
            ime = launch('fcitx', ['fcitx5','-D','--disable=xcb'], process_env=native, start_new_session=True)
            deadline = time.monotonic()+10
            while subprocess.run(['fcitx5-remote','--check'], env=env, capture_output=True).returncode:
                if time.monotonic()>deadline or ime.poll() is not None: raise RuntimeError('Fcitx startup failed')
                time.sleep(.1)
            with (output/'protocol.log').open('w') as protocol, (output/'application.log').open('w') as log:
                app=subprocess.Popen([str(args.binary.resolve()), '--'+mode, '--seconds','120','--cursor-delay-ms',str(cursor_delay_ms)],env=native,stdout=log,stderr=protocol)
                deadline=time.monotonic()+10
                while not re.search(r'wl_keyboard[#@]\d+\.enter\(', (output/'protocol.log').read_text()):
                    if time.monotonic()>deadline or app.poll() is not None:
                        raise RuntimeError('Direct winit window did not receive keyboard focus')
                    time.sleep(.05)
                xdo('windowactivate','--sync',window)
                xdo('windowfocus','--sync',window)
                xdo('mousemove','--window',window,100,100,'click',1)
                deadline=time.monotonic()+5
                while '.enable()' not in (output/'protocol.log').read_text():
                    if time.monotonic()>deadline: raise RuntimeError('Direct winit client did not enable IME')
                    time.sleep(.05)
                subprocess.run(['fcitx5-remote','-s','pinyin'],env=env,check=True)
                subprocess.run(['fcitx5-remote','-o'],env=env,check=True)
                assert subprocess.check_output(['fcitx5-remote','-n'],env=env,text=True).strip()=='pinyin'
                time.sleep(.3)
                trials=[]
                for trial in range(args.bursts):
                    paths={name:output/(name+'.log') for name in ('application','protocol','fcitx')}
                    offsets={name:len(path.read_text()) for name,path in paths.items()}
                    def slices():
                        return {name:path.read_text()[offsets[name]:] for name,path in paths.items()}
                    began=time.monotonic()
                    # Deliberately preserve the original unsynchronized burst.
                    # No per-character wait, primer, engine switch, or refocus.
                    xdo('type','--clearmodifiers','--delay',90,'nihao')
                    injected=time.monotonic()
                    time.sleep(args.observation_seconds)
                    before=slices()
                    xdo('key','--clearmodifiers','space')
                    time.sleep(.7)
                    after=slices()
                    client_preedits=strings(before['protocol'],'preedit_string')
                    engine_preedits=strings(before['fcitx'],'set_preedit_string')
                    client_commits=strings(after['protocol'],'commit_string')
                    application_preedits=[json.loads(value) for value in re.findall(r'IME Preedit\(("(?:[^"\\]|\\.)*")',before['application'])]
                    application_commits=[json.loads(value) for value in re.findall(r'IME Commit\(("(?:[^"\\]|\\.)*")',after['application'])]
                    engine_commits=strings(after['fcitx'],'commit_string')
                    full_preedit='ni hao' in client_preedits and 'ni hao' in application_preedits
                    unicode_commit=client_commits==['你好'] and application_commits==['你好']
                    result=dict(trial=trial+1,condition='cold' if trial==0 else 'warm',
                                warm_definition='Prior burst completed in the same input-method process; no engine switch or refocus.',
                                burst='nihao',xdotool_delay_ms=90,burst_wall_seconds=injected-began,
                                observation_seconds=args.observation_seconds,client_preedits=client_preedits,
                                engine_preedits=engine_preedits,client_commits=client_commits,engine_commits=engine_commits,
                                application_preedits=application_preedits,application_commits=application_commits,
                                full_preedit=full_preedit,unicode_commit=unicode_commit,
                                outcome='delivered' if full_preedit and unicode_commit else 'delivery_failure',
                                client_cursor_updates=re.findall(r'\.set_cursor_rectangle\(([^\n]+)\)',before['protocol']),
                                engine_transaction_serials=re.findall(r'zwp_input_method_v2[#@]\d+\.commit\((\d+)\)',before['fcitx']),
                                client_done_serials=re.findall(r'zwp_text_input_v3[#@]\d+\.done\((\d+)\)',before['protocol']),
                                client_state_commit_count=len(re.findall(r'zwp_text_input_v3[#@]\d+\.commit\(\)',before['protocol'])),
                                engine_key_events=re.findall(r'zwp_input_method_keyboard_grab_v2[#@]\d+\.key\(([^\n]+)\)',before['fcitx']),
                                input_method_done_count=len(re.findall(r'zwp_input_method_v2[#@]\d+\.done\(',before['fcitx'])))
                    for phase,traces in (('before_commit',before),('after_commit',after)):
                        for name,trace in traces.items():
                            (output/f'trial-{trial+1}-{phase}-{name}.log').write_text(trace)
                    trials.append(result)
                    (output/'trials.json').write_text(json.dumps(trials,indent=2)+'\n')
                    # Reset between trials without changing engine or focus.
                    xdo('key','--clearmodifiers','Escape')
                    time.sleep(.2)
                    xdo('key','--clearmodifiers','F8')
                    time.sleep(.2)
                    if app.poll() is not None: raise RuntimeError('Direct winit probe exited during observation')
                subprocess.run(['grim',str((output/'final.png').resolve())],env=env,check=True)
                subprocess.run(['swaymsg','-s',str(sockets[0]),'[pid='+str(app.pid)+']','kill'],env=env,check=True,stdout=subprocess.DEVNULL)
                app.wait(timeout=5)
            if app.returncode: raise RuntimeError('Direct winit probe exited unsuccessfully')
            result=dict(mode=mode,cursor_delay_ms=cursor_delay_ms,session=index,client_display_unset=True,trials=trials,
                        limitations=['Diagnostic only; no delayed event injection or serial-cancellation guarantee.',
                                     'Cold means a fresh private Fcitx process and empty user data, not a cold OS page cache.'])
            (output/'result.json').write_text(json.dumps(result,indent=2)+'\n')
            return result
        finally:
            stop(app)
            for process in (ime,bus):
                if process:
                    try: os.killpg(process.pid,signal.SIGTERM)
                    except ProcessLookupError: pass
                    stop(process)
                    try: os.killpg(process.pid,signal.SIGKILL)
                    except ProcessLookupError: pass
            stop(compositor); stop(wm); stop(xvfb)


def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('binary',type=pathlib.Path)
    parser.add_argument('--output',type=pathlib.Path,required=True)
    parser.add_argument('--sessions',type=int,default=2,help='independent sessions per candidate-rectangle mode')
    parser.add_argument('--bursts',type=int,default=3,help='identical bursts per session, first cold then warm')
    parser.add_argument('--observation-seconds',type=float,default=2.,help='fixed observation period after each unchanged burst')
    parser.add_argument('--cursor-delays-ms',default='0',help='comma-separated moving-rectangle delays; fixed control always uses zero')
    args=parser.parse_args()
    try:
        delays=list(dict.fromkeys(int(value) for value in args.cursor_delays_ms.split(',')))
    except ValueError:
        parser.error('Cursor delays must be comma-separated integer milliseconds')
    if any(value<0 or value>1000 for value in delays):
        parser.error('Cursor delays must be between zero and 1000 milliseconds')
    if args.sessions<1 or args.bursts<2 or args.observation_seconds<=0:
        parser.error('Require at least one session, two bursts, and a positive observation period')
    for executable in ('Xvfb','openbox','sway','swaymsg','dbus-daemon','fcitx5','fcitx5-remote','xdotool','xwininfo','grim'):
        if not shutil.which(executable): parser.error('Missing executable: '+executable)
    args.output.mkdir(parents=True,exist_ok=True)
    (args.output/'result.json').unlink(missing_ok=True)
    source=pathlib.Path(__file__).read_bytes()
    (args.output/pathlib.Path(__file__).name).write_bytes(source)
    provenance=dict(script_sha256=hashlib.sha256(source).hexdigest(),
                    binary_sha256=hashlib.sha256(args.binary.read_bytes()).hexdigest(),
                    argv=sys.argv,python=sys.version)
    (args.output/'provenance.json').write_text(json.dumps(provenance,indent=2)+'\n')
    sessions=[]
    configurations=[('moving',delay) for delay in delays]+[('fixed',0)]
    # Alternate order across independent sessions to reduce ordering bias.
    for index in range(1,args.sessions+1):
        for mode,delay in (configurations if index%2 else list(reversed(configurations))):
            result=run_session(args,mode,index,delay)
            sessions.append(result)
            (args.output/'sessions.json').write_text(json.dumps(sessions,indent=2)+'\n')
            print(f'{mode} delay={delay}ms session {index}: '+', '.join(f'{t["condition"]}={t["outcome"]}' for t in result['trials']),flush=True)
    findings={}
    for mode,delay in configurations:
        key=f'{mode}-delay{delay}'
        findings[key]={}
        for condition in ('cold','warm'):
            trials=[trial for session in sessions if session['mode']==mode and session['cursor_delay_ms']==delay for trial in session['trials'] if trial['condition']==condition]
            findings[key][condition]=dict(trials=len(trials),full_preedit=sum(t['full_preedit'] for t in trials),unicode_commit=sum(t['unicode_commit'] for t in trials),delivery_failures=sum(t['outcome']=='delivery_failure' for t in trials))
    result=dict(result='diagnostic_completed',findings=findings,sessions=sessions,
                conclusion='Observed outcomes apply only to these runs; failures remain recorded and no framework fix is claimed.')
    (args.output/'result.json').write_text(json.dumps(result,indent=2)+'\n')
    print(json.dumps(findings,indent=2))


if __name__=='__main__':
    main()
