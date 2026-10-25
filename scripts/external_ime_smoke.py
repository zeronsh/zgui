#!/usr/bin/env python3
"""Replace an editor model during a real private IBus/libpinyin composition."""
import argparse
import json
import os
import pathlib
import re
import select
import signal
import shutil
import subprocess
import tempfile
import time

from platform_smoke import release_mounts, stop, wait_for_wm, use_ibus_engine, wait_for_ibus_xim


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('binary', type=pathlib.Path)
    parser.add_argument('--read-only-test', action='store_true', help='cancel composition by toggling read-only without changing the model')
    parser.add_argument('--output', type=pathlib.Path, required=True)
    args = parser.parse_args()
    for executable in ('Xvfb', 'openbox', 'dbus-daemon', 'gsettings', 'ibus-daemon', 'ibus', 'xdotool', 'xwininfo'):
        if shutil.which(executable) is None:
            parser.error('Missing required executable: '+executable)
    memconf = next((candidate for candidate in (shutil.which('ibus-memconf'), '/usr/libexec/ibus-memconf', '/usr/lib/ibus/ibus-memconf', '/usr/lib/x86_64-linux-gnu/ibus/ibus-memconf') if candidate and os.access(candidate, os.X_OK)), None)
    if memconf is None:
        parser.error('Missing ibus-memconf; install the IBus package with its memory configuration service')
    args.output.mkdir(parents=True, exist_ok=True)
    (args.output / 'result.json').unlink(missing_ok=True)
    xvfb = wm = bus = ime = app = None
    snapshots = []
    with tempfile.TemporaryDirectory(prefix='zgx-external-ime-') as private:
        try:
            with (args.output / 'xvfb.log').open('w') as log:
                xvfb = subprocess.Popen(['Xvfb', '-r', '-displayfd', '1', '-screen', '0', '1024x768x24', '-ac'], stdout=subprocess.PIPE, stderr=log, text=True)
                if not select.select([xvfb.stdout], [], [], 10)[0]:
                    raise RuntimeError('Xvfb startup timed out')
                display = ':' + xvfb.stdout.readline().strip()
            env = dict(os.environ, DISPLAY=display, XDG_RUNTIME_DIR=private,
                       XDG_CONFIG_HOME=private+'/config', XDG_CACHE_HOME=private+'/cache',
                       XDG_DATA_HOME=private+'/data', XMODIFIERS='@im=ibus',
                       GTK_IM_MODULE='ibus', QT_IM_MODULE='ibus', WINIT_X11_SCALE_FACTOR='1', LANG='C.UTF-8', GSETTINGS_BACKEND='keyfile')
            env.pop('WAYLAND_DISPLAY', None)
            env.pop('SWAYSOCK', None)
            env.pop('I3SOCK', None)
            env.pop('IBUS_ADDRESS', None)
            with (args.output / 'dbus.log').open('w') as log:
                bus = subprocess.Popen(['dbus-daemon', '--session', '--nofork', '--print-address=1'], stdout=subprocess.PIPE, stderr=log, env=env, text=True, start_new_session=True)
                if not select.select([bus.stdout], [], [], 10)[0]:
                    raise RuntimeError('D-Bus startup timed out')
                env['DBUS_SESSION_BUS_ADDRESS'] = bus.stdout.readline().strip()
            # Share this setting with the owned daemon via a temporary keyfile;
            # memory backends are process-local, and the user's dconf is untouched.
            subprocess.run(['gsettings', 'set', 'org.freedesktop.ibus.general', 'use-system-keyboard-layout', 'true'], env=env, check=True)
            with (args.output / 'openbox.log').open('w') as log:
                wm = subprocess.Popen(['openbox'], env=env, stdout=log, stderr=log)
            wait_for_wm(env)
            with (args.output / 'ibus.log').open('w') as log:
                ime = subprocess.Popen(['ibus-daemon', '--xim', '--replace', '--cache=none', '--emoji-extension=disable', '--config='+memconf, '--verbose'], env=env, stdout=log, stderr=log, start_new_session=True)
            deadline = time.monotonic()+10
            while True:
                ready = subprocess.run(['ibus', 'list-engine'], env=env, capture_output=True, text=True)
                if ready.returncode == 0 and 'libpinyin' in ready.stdout:
                    break
                if time.monotonic()>deadline or ime.poll() is not None:
                    raise RuntimeError('IBus failed: '+ready.stdout+ready.stderr)
                time.sleep(.2)
            wait_for_ibus_xim(env)
            def xdo(*arguments):
                return subprocess.check_output(['xdotool', *map(str,arguments)], env=env, text=True).strip()
            with (args.output / 'application.log').open('w') as output:
                app = subprocess.Popen([str(args.binary.resolve())] + (['--read-only-test'] if args.read_only_test else []), env=env, stdout=output, stderr=output)
                deadline=time.monotonic()+20
                while True:
                    found=subprocess.run(['xdotool','search','--onlyvisible','--pid',str(app.pid)],env=env,capture_output=True,text=True)
                    if found.returncode==0 and found.stdout.strip():
                        window=found.stdout.splitlines()[0]; break
                    if time.monotonic()>deadline or app.poll() is not None: raise RuntimeError('Native IME probe did not open')
                    time.sleep(.05)
                xdo('windowactivate','--sync',window)
                time.sleep(.5)
                def click(x,y):
                    xdo('mousemove','--window',window,x,y,'click',1);time.sleep(.4)
                def key(*keys):
                    xdo('key','--clearmodifiers',*keys);time.sleep(.4)
                def type_text(text):
                    xdo('type','--clearmodifiers','--delay',90,text);time.sleep(.5)
                def sample():
                    # The window can show before the first sample.
                    first = time.monotonic() + 10
                    while not (matches := re.findall(r'EXTERNAL replaced=(true|false) preedit=(true|false) focus=(true|false) focus_changes=(\d+) model=(".*")', (args.output/'application.log').read_text())):
                        assert time.monotonic() < first, 'Missing native editor sample'
                        time.sleep(.05)
                    replaced, preedit, focus, changes, model = matches[-1]
                    return dict(replaced=replaced == 'true', preedit=preedit == 'true', focus=focus == 'true', focus_changes=int(changes), model=json.loads(model))
                click(100, 76)
                key('End')
                initial = sample()
                use_ibus_engine(env,'libpinyin',ime)
                type_text('nihao')
                deadline = time.monotonic() + 5
                while time.monotonic() < deadline:
                    current = sample()
                    if current['replaced']:
                        break
                    time.sleep(.1)
                assert current['replaced'] and not current['preedit'] and current['model'] == 'replacement', current
                assert current['focus'] and current['focus_changes'] == initial['focus_changes'], (initial, current)
                snapshots.append(dict(stage='external_replacement', **current))
                if args.read_only_test:
                    deadline = time.monotonic() + 3
                    while time.monotonic() < deadline:
                        trace = (args.output/'application.log').read_text()
                        if 'READ_ONLY_ENABLED editable=true' in trace:
                            break
                        time.sleep(.1)
                    assert 'READ_ONLY_DISABLED had_preedit=true read_only=true' in trace, trace
                    assert 'READ_ONLY_ENABLED editable=true' in trace, trace
                    time.sleep(.2)
                key('Return')
                current = sample()
                assert current['model'] == 'replacement' and not current['preedit'], current
                snapshots.append(dict(stage='old_candidate_does_not_commit', **current))
                key('BackSpace')
                current = sample()
                assert current['model'] == 'replacemen' and not current['preedit'], current
                snapshots.append(dict(stage='ordinary_backspace', **current))
                # Digits pass through libpinyin without starting another composition.
                # Do not switch engines or refocus: either could mask host recovery.
                type_text('7')
                current = sample()
                assert current['model'] == 'replacemen7' and not current['preedit'], current
                snapshots.append(dict(stage='ordinary_input', **current))
                type_text('nihao')
                current = sample()
                assert current['preedit'] and current['model'] == 'replacemen7', current
                snapshots.append(dict(stage='new_preedit', **current))
                key('space')
                current = sample()
                assert current['model'] == 'replacemen7你好' and not current['preedit'], current
                assert current['focus_changes'] == initial['focus_changes'] and current['focus'], current
                snapshots.append(dict(stage='new_commit', **current))
                subprocess.run(['import','-window',window,str(args.output/'external-ime.png')],env=env,check=True,timeout=10)
                # Closed as a user closes it (Openbox's Alt+F4), not by the app's
                # own timer, which a quick run would wait out.
                xdo('windowactivate', '--sync', window, 'key', 'alt+F4')
                app.wait(timeout=25)
            trace = (args.output/'application.log').read_text()
            assert app.returncode == 0, trace
            assert re.search(r'PREEDIT "[^"\n]+"', trace), trace
            result = dict(result='pass', read_only_toggle=args.read_only_test, backend='X11 XIM / private IBus libpinyin', checks=[('read-only toggle during genuine preedit without model mutation' if args.read_only_test else 'external model replacement during genuine preedit'), 'preedit cancelled without focus change', 'Return does not commit cancelled candidate', 'ordinary Backspace resumes', 'ordinary digit input resumes without engine switch', 'fresh native composition and Unicode commit'], snapshots=snapshots)
            (args.output/'result.json').write_text(json.dumps(result,indent=2)+'\n')
            print('PASS: external model cancellation and native input recovery')
        finally:
            stop(app)
            for owned in (ime,bus):
                if owned:
                    try:os.killpg(owned.pid,signal.SIGTERM)
                    except ProcessLookupError:pass
                    stop(owned)
                    # Reap any service descendant that outlives its supervisor.
                    try:os.killpg(owned.pid,signal.SIGKILL)
                    except ProcessLookupError:pass
            stop(wm)
            stop(xvfb)
            release_mounts(private)

if __name__=='__main__':main()
