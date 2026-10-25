#!/usr/bin/env python3
"""Replace a model during real text-input-v3 composition and verify recovery."""
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
    parser.add_argument('--read-only-test', action='store_true', help='cancel real preedit by toggling read-only without changing the model')
    parser.add_argument('--output', type=pathlib.Path, required=True)
    args = parser.parse_args()
    for name in ('Xvfb', 'openbox', 'sway', 'swaymsg', 'dbus-daemon', 'fcitx5', 'fcitx5-remote', 'xdotool', 'xwininfo', 'grim'):
        if not shutil.which(name):
            parser.error('Missing executable: ' + name)
    args.output.mkdir(parents=True, exist_ok=True)
    (args.output/'result.json').unlink(missing_ok=True)
    app = ime = compositor = wm = xvfb = bus = None
    with private_runtime('zgx-wayland-external-ime-') as private:
        env = dict(os.environ, XDG_RUNTIME_DIR=private, XDG_CONFIG_HOME=private+'/config',
                   XDG_CACHE_HOME=private+'/cache', XDG_DATA_HOME=private+'/data',
                   WLR_BACKENDS='x11', WLR_X11_OUTPUTS='1', WLR_RENDERER='pixman', LANG='C.UTF-8')
        for name in ('DISPLAY', 'WAYLAND_DISPLAY', 'SWAYSOCK', 'I3SOCK', 'IBUS_ADDRESS', 'XMODIFIERS', 'GTK_IM_MODULE', 'QT_IM_MODULE', 'QT_IM_MODULES', 'SDL_IM_MODULE', 'AT_SPI_BUS_ADDRESS', 'FCITX_DBUS_ADDRESS'):
            env.pop(name, None)
        profile = pathlib.Path(private, 'config/fcitx5/profile')
        profile.parent.mkdir(parents=True)
        profile.write_text('[Groups/0]\nName=Default\nDefault Layout=us\nDefaultIM=pinyin\n\n[Groups/0/Items/0]\nName=keyboard-us\nLayout=\n\n[Groups/0/Items/1]\nName=pinyin\nLayout=\n\n[GroupOrder]\n0=Default\n')
        config = pathlib.Path(private, 'sway.conf')
        config.write_text('xwayland disable\nseat seat0 fallback true\noutput * mode 1200x800\ndefault_border none\nfor_window [title="zgx external IME model"] fullscreen enable\n')
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
            xvfb = launch('xvfb', ['Xvfb','-r','-displayfd','1','-screen','0','1400x1000x24','-ac'], stdout=subprocess.PIPE, text=True)
            if not select.select([xvfb.stdout], [], [], 10)[0]: raise RuntimeError('Xvfb timeout')
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
                app = subprocess.Popen([str(args.binary.resolve())] + (['--read-only-test'] if args.read_only_test else []), env=native, stdout=log, stderr=protocol)
                deadline = time.monotonic()+10
                while 'EXTERNAL ' not in (args.output/'application.log').read_text():
                    if time.monotonic()>deadline or app.poll() is not None: raise RuntimeError('IME fixture failed')
                    time.sleep(.05)
                def click(x,y):
                    xdo('mousemove','--window',window,x,y,'click',1); time.sleep(.35)
                def key(*keys):
                    xdo('key','--clearmodifiers',*keys); time.sleep(.2)
                def type_text(value):
                    xdo('type','--clearmodifiers','--delay',90,value); time.sleep(.25)
                snapshots=[]
                def sample():
                    matches=re.findall(r'EXTERNAL replaced=(true|false) preedit=(true|false) focus=(true|false) focus_changes=(\d+) model=(".*")', (args.output/'application.log').read_text())
                    assert matches, 'Missing native editor sample'
                    replaced, preedit, focus, changes, model=matches[-1]
                    return dict(replaced=replaced=='true', preedit=preedit=='true', focus=focus=='true', focus_changes=int(changes), model=json.loads(model))
                def capture(stage, model, preedit=False, replaced=True):
                    deadline=time.monotonic()+5
                    while True:
                        current=sample()
                        if current['model']==model and current['preedit']==preedit and current['replaced']==replaced:
                            break
                        if time.monotonic()>deadline or app.poll() is not None:
                            raise AssertionError(f'{stage}: expected model={model!r}, preedit={preedit}, replaced={replaced}; observed {current!r}')
                        time.sleep(.05)
                    assert current['focus'] and current['focus_changes']==initial['focus_changes'], (initial,current)
                    snapshots.append(dict(stage=stage,**current))
                    (args.output/'snapshots.json').write_text(json.dumps(snapshots,indent=2)+'\n')
                    subprocess.run(['grim',str((args.output/(stage+'.png')).resolve())],env=env,check=True)
                    return current
                # The fixture logs its initial model before its Wayland window
                # maps. Wait for compositor focus before directing host input.
                deadline=time.monotonic()+8
                while not re.search(r'wl_keyboard[#@]\d+\.enter\(', (args.output/'protocol.log').read_text()):
                    if time.monotonic()>deadline: raise RuntimeError('Native editor window did not receive keyboard focus')
                    time.sleep(.05)
                xdo('windowactivate','--sync',window)
                xdo('windowfocus','--sync',window)
                click(100,76)
                key('End')
                initial=sample()
                assert initial['model']==('replacement' if args.read_only_test else '') and not initial['replaced'], initial
                # Fcitx answers before its Pinyin engine has loaded: switch
                # once it can.
                deadline=time.monotonic()+10
                while subprocess.run(['fcitx5-remote','-s','pinyin'],env=env,capture_output=True).returncode:
                    if time.monotonic()>deadline: raise RuntimeError('Fcitx never loaded Pinyin')
                    time.sleep(.1)
                subprocess.run(['fcitx5-remote','-o'],env=env,check=True)
                time.sleep(.3)
                engine=subprocess.check_output(['fcitx5-remote','-n'],env=env,text=True).strip()
                assert engine=='pinyin',engine
                type_text('nihao')
                capture('read_only_cancellation' if args.read_only_test else 'external_replacement','replacement')
                if args.read_only_test:
                    deadline=time.monotonic()+5
                    while 'READ_ONLY_ENABLED editable=true' not in (args.output/'application.log').read_text():
                        if time.monotonic()>deadline: raise RuntimeError('Read-only re-enable timed out')
                        time.sleep(.05)
                    trace=(args.output/'application.log').read_text()
                    assert 'READ_ONLY_DISABLED had_preedit=true read_only=true' in trace, trace
                    capture('editable_again','replacement')
                key('Return')
                capture('old_candidate_does_not_commit','replacement')
                key('BackSpace')
                capture('ordinary_backspace','replacemen')
                type_text('7')
                capture('ordinary_input','replacemen7')
                protocol_offset=len((args.output/'protocol.log').read_text())
                type_text('n')
                capture('fresh_preedit','replacemen7',preedit=True)
                # Sequence each fresh key through an observed native preedit.
                # This is a recovery smoke, not a rapid-input stress test.
                for character,preedit_text in (('i','ni'),('h','ni h'),('a','ni ha'),('o','ni hao')):
                    type_text(character)
                    deadline=time.monotonic()+5
                    token='preedit_string("'+preedit_text+'"'
                    while token not in (args.output/'protocol.log').read_text()[protocol_offset:]:
                        if time.monotonic()>deadline: raise RuntimeError('Fresh Pinyin preedit timed out: '+preedit_text)
                        time.sleep(.05)
                key('space')
                capture('fresh_commit','replacemen7你好')
                # Closed as a user closes it (sway asks it to), not by the
                # app's own timer, which a quick run would wait out.
                subprocess.run(['swaymsg',f'[pid={app.pid}] kill'],env=env,capture_output=True)
                app.wait(timeout=25)
            trace=(args.output/'application.log').read_text()
            protocol=(args.output/'protocol.log').read_text()
            assert app.returncode==0, trace
            final=sample()
            assert final['model']=='replacemen7你好' and not final['preedit'], final
            assert final['focus'] and final['focus_changes']==initial['focus_changes'], final
            original_model='replacement' if args.read_only_test else ''
            assert re.search(r'EXTERNAL replaced=false preedit=true .*model="'+original_model+r'"',trace), trace
            assert trace.count('COMMIT ')==1 and 'COMMIT "你好"' in trace, trace
            for token in ('zwp_text_input_v3', '.enable()', '.disable()', '.set_cursor_rectangle(', 'preedit_string("ni hao"', 'commit_string("你好"'):
                assert token in protocol, 'Missing protocol evidence: '+token
            result=dict(result='pass',read_only_toggle=args.read_only_test,backend='native Wayland text-input-v3 / Sway / Fcitx5 Pinyin',client_display_unset=True,
                        checks=[('read-only toggle during native preedit without model mutation' if args.read_only_test else 'external model replacement during native preedit'),'preedit cancellation without focus change','Return does not commit cancelled candidate','ordinary Backspace recovery','ordinary digit input recovery without engine switch','fresh composition and Unicode commit'],snapshots=snapshots,
                        limitations=['Does not inject delayed stale done serials or guarantee serial-based cancellation or fast-typing delivery.'])
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
