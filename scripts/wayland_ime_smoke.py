#!/usr/bin/env python3
"""Exercise real text-input-v3 composition in an owned Sway/Fcitx5 session."""
from sway_host import wait_for_openbox
import argparse
import json
import os
import pathlib
import re
import select
import shutil
import signal
import subprocess
import time

from platform_smoke import private_runtime, stop


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('binary', type=pathlib.Path)
    parser.add_argument('--output', type=pathlib.Path, required=True)
    parser.add_argument('--input-modes', action='store_true',
                        help="native_ime --input-modes: each field's input mode reaches the input method as its "
                             "content purpose (email, then digits)")
    args = parser.parse_args()
    for name in ('Xvfb', 'openbox', 'sway', 'swaymsg', 'dbus-daemon', 'fcitx5', 'fcitx5-remote', 'xdotool', 'xwininfo', 'grim'):
        if not shutil.which(name):
            parser.error('Missing executable: ' + name)
    args.output.mkdir(parents=True, exist_ok=True)
    (args.output/'result.json').unlink(missing_ok=True)
    app = ime = compositor = wm = xvfb = bus = None
    with private_runtime('zgx-wayland-ime-') as private:
        env = dict(os.environ, XDG_RUNTIME_DIR=private, XDG_CONFIG_HOME=private+'/config',
                   XDG_CACHE_HOME=private+'/cache', XDG_DATA_HOME=private+'/data',
                   WLR_BACKENDS='x11', WLR_X11_OUTPUTS='1', WLR_RENDERER='pixman', LANG='C.UTF-8')
        for name in ('DISPLAY', 'WAYLAND_DISPLAY', 'SWAYSOCK', 'I3SOCK', 'IBUS_ADDRESS', 'XMODIFIERS', 'GTK_IM_MODULE', 'QT_IM_MODULE', 'QT_IM_MODULES', 'SDL_IM_MODULE', 'AT_SPI_BUS_ADDRESS', 'FCITX_DBUS_ADDRESS'):
            env.pop(name, None)
        profile = pathlib.Path(private, 'config/fcitx5/profile')
        profile.parent.mkdir(parents=True)
        profile.write_text('[Groups/0]\nName=Default\nDefault Layout=us\nDefaultIM=pinyin\n\n[Groups/0/Items/0]\nName=keyboard-us\nLayout=\n\n[Groups/0/Items/1]\nName=pinyin\nLayout=\n\n[GroupOrder]\n0=Default\n')
        config = pathlib.Path(private, 'sway.conf')
        config.write_text('xwayland disable\nseat seat0 fallback true\noutput * mode 1200x800\ndefault_border none\nfor_window [title="zgx native IME"] fullscreen enable\n')
        (args.output/'profile').write_text(profile.read_text())
        (args.output/'sway.conf').write_text(config.read_text())
        bus_config = pathlib.Path(private, 'dbus.conf')
        # No activation directories: desktop portals must not mount into this
        # disposable runtime or start unrelated services during the probe.
        bus_config.write_text('<busconfig><type>session</type><listen>unix:tmpdir=' + private + '</listen><auth>EXTERNAL</auth><policy context="default"><allow send_destination="*"/><allow receive_sender="*"/><allow own="*"/></policy></busconfig>')
        def launch(name, command, process_env=env, **kwargs):
            with (args.output/(name+'.log')).open('w') as log:
                return subprocess.Popen(command, env=process_env, stderr=log, stdout=kwargs.pop('stdout', log), **kwargs)
        try:
            xvfb = launch('xvfb', ['Xvfb','-r','-noreset','-displayfd','1','-screen','0','1400x1000x24','-ac'], stdout=subprocess.PIPE, text=True)
            if not select.select([xvfb.stdout], [], [], 30)[0]: raise RuntimeError('Xvfb timeout')
            env['DISPLAY'] = ':'+xvfb.stdout.readline().strip()
            bus = launch('dbus', ['dbus-daemon','--config-file='+str(bus_config),'--nofork','--print-address=1'], stdout=subprocess.PIPE, text=True, start_new_session=True)
            if not select.select([bus.stdout], [], [], 10)[0]: raise RuntimeError('D-Bus timeout')
            env['DBUS_SESSION_BUS_ADDRESS'] = bus.stdout.readline().strip()
            wm = launch('openbox', ['openbox'])
            wait_for_openbox(env, args.output, wm)
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
            with (args.output/'protocol.log').open('w') as protocol, (args.output/'application.log').open('w') as log:
                command = [str(args.binary.resolve())] + (['--input-modes'] if args.input_modes else [])
                app = subprocess.Popen(command, env=native, stdout=log, stderr=protocol)
                deadline = time.monotonic()+10
                while 'MODEL ' not in (args.output/'application.log').read_text():
                    if time.monotonic()>deadline or app.poll() is not None: raise RuntimeError('IME fixture failed')
                    time.sleep(.05)
                def click(x,y):
                    xdo('mousemove','--window',window,x,y,'click',1); time.sleep(.35)
                def key(*keys):
                    xdo('key','--clearmodifiers',*keys); time.sleep(.2)
                def type_text(value):
                    # Pace functional composition through fresh native events.
                    # Rapid-input delivery is a separate, unresolved scenario.
                    for character in value:
                        offset=len((args.output/'protocol.log').read_text())
                        xdo('type','--clearmodifiers','--delay',90,character)
                        deadline=time.monotonic()+8
                        while not re.search(r'zwp_text_input_v3[#@]\d+\.preedit_string\("[^"\n]+"', (args.output/'protocol.log').read_text()[offset:]):
                            if time.monotonic()>deadline or app.poll() is not None:
                                raise RuntimeError('Native Pinyin preedit timed out after '+repr(character))
                            time.sleep(.05)
                        # Let cursor state settle before the next transaction.
                        time.sleep(.15)
                expected=[('', '', 'ni hao', ''), ('你好', '', '你好', ''),
                          ('你好', '', '你好zhong', ''), ('你好', '', '你好', ''),
                          ('你好', '', '你好wo', ''), ('你好', '', '你好', ''),
                          ('你好', '', '你好', 'shi jie'), ('你好', '世界', '你好', '世界')]
                snapshots=[]
                def capture(stage):
                    deadline=time.monotonic()+4
                    while True:
                        trace=(args.output/'application.log').read_text()
                        tick=trace[trace.rfind('MODEL '):]
                        model=re.search(r'MODEL \d+ first=(".*?") second=(".*?") focus=',tick)
                        rendered={label:re.search(r'DISPLAY '+label+r' (".*?") Rect',tick)
                                  for label in ('first','second')}
                        sample=None
                        if model and all(rendered.values()):
                            sample=dict(stage=stage, first=json.loads(model[1]), second=json.loads(model[2]))
                            for label in ('first','second'):
                                sample['display_'+label]=json.loads(rendered[label][1])
                            actual=tuple(sample[k] for k in ('first','second','display_first','display_second'))
                            if actual == expected[stage-1]: break
                        if time.monotonic()>deadline or app.poll() is not None:
                            raise AssertionError(f'Stage {stage}: expected {expected[stage-1]!r}, observed {sample!r}')
                        time.sleep(.05)
                    snapshots.append(sample)
                    (args.output/'snapshots.json').write_text(json.dumps(snapshots,indent=2)+'\n')
                    subprocess.run(['grim',str((args.output/f'ime-{stage}.png').resolve())],env=env,check=True)
                if args.input_modes:
                    # text-input-v3's content types, as the app sends them
                    # with each enable: (hint, purpose), email 6, digits 2.
                    sent = []
                    for y, purpose in ((82, 6), (174, 2)):
                        offset = len((args.output/'protocol.log').read_text())
                        click(100, y)
                        deadline = time.monotonic()+6
                        while True:
                            found = re.findall(r'zwp_text_input_v3[#@]\d+\.set_content_type\((\d+), (\d+)\)',
                                               (args.output/'protocol.log').read_text()[offset:])
                            if found or time.monotonic()>deadline or app.poll() is not None: break
                            time.sleep(.05)
                        sent.append((y, purpose, found))
                        if not found or int(found[-1][1]) != purpose:
                            raise AssertionError(f'the field at y={y} sent content types {found}, not purpose {purpose}')
                    subprocess.run(['swaymsg',f'[pid={app.pid}] kill'],env=env,capture_output=True)
                    app.wait(timeout=25)
                    result=dict(result='pass', backend='native Wayland text-input-v3 / Sway / Fcitx5',
                                checks=['email content purpose','digits content purpose'], sent=sent)
                    (args.output/'result.json').write_text(json.dumps(result,indent=2)+'\n')
                    print('PASS: '+', '.join(result['checks']))
                    return
                click(100,82)
                # Fcitx answers before its Pinyin engine has loaded: switch
                # once it can.
                deadline=time.monotonic()+10
                while subprocess.run(['fcitx5-remote','-s','pinyin'],env=env,capture_output=True).returncode:
                    if time.monotonic()>deadline: raise RuntimeError('Fcitx never loaded Pinyin')
                    time.sleep(.1)
                subprocess.run(['fcitx5-remote','-o'],env=env,check=True)
                time.sleep(.3)
                engine = subprocess.check_output(['fcitx5-remote','-n'],env=env,text=True).strip()
                assert engine == 'pinyin', engine
                type_text('nihao')
                capture(1)
                key('space'); capture(2)
                type_text('zhong'); capture(3)
                key('Escape'); capture(4)
                type_text('wo'); capture(5)
                click(100,174); capture(6)
                type_text('shijie'); capture(7)
                key('space'); capture(8)
                # Closed as a user closes it (sway asks it to), not by the
                # app's own timer, which a quick run would wait out.
                subprocess.run(['swaymsg',f'[pid={app.pid}] kill'],env=env,capture_output=True)
                app.wait(timeout=25)
            trace=(args.output/'application.log').read_text()
            protocol=(args.output/'protocol.log').read_text()
            assert app.returncode == 0, trace
            for sample, values in zip(snapshots, expected):
                actual=tuple(sample[k] for k in ('first','second','display_first','display_second'))
                assert actual == values, (sample['stage'],actual,values)
            assert 'FINAL first="你好" second="世界"' in trace, trace
            assert trace.count('COMMIT ') == 2, trace
            for token in ('zwp_text_input_v3', '.enable()', '.set_cursor_rectangle(', 'preedit_string("ni hao"', 'commit_string("你好"', 'commit_string("世界"'):
                assert token in protocol, 'Missing protocol evidence: '+token
            result=dict(result='pass', backend='native Wayland text-input-v3 / Sway / Fcitx5 Pinyin', client_display_unset=True,
                        checks=['native preedit','Unicode commits','Escape cancellation','focus switch cancellation','separate editor models'], snapshots=snapshots,
                        limitations=['Does not test external model replacement or read-only changes.', 'Does not test rapid-input delivery, delayed stale done serials, or guarantee serial-based cancellation.'])
            (args.output/'result.json').write_text(json.dumps(result,indent=2)+'\n')
            print('PASS: '+', '.join(result['checks']))
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


if __name__ == '__main__':
    main()
