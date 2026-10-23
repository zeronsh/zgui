#!/usr/bin/env python3
"""Drive an owned IBus/libpinyin XIM session with actual native key events."""
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

from platform_smoke import stop, wait_for_wm, use_ibus_engine, wait_for_ibus_xim


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('binary', type=pathlib.Path)
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
    with tempfile.TemporaryDirectory(prefix='zgui-native-ime-') as private:
        try:
            with (args.output / 'xvfb.log').open('w') as log:
                xvfb = subprocess.Popen(['Xvfb', '-displayfd', '1', '-screen', '0', '1024x768x24', '-ac'], stdout=subprocess.PIPE, stderr=log, text=True)
                if not select.select([xvfb.stdout], [], [], 10)[0]:
                    raise RuntimeError('Xvfb startup timed out')
                display = ':' + xvfb.stdout.readline().strip()
            env = dict(os.environ, DISPLAY=display, XDG_RUNTIME_DIR=private,
                       XDG_CONFIG_HOME=private+'/config', XDG_CACHE_HOME=private+'/cache',
                       XDG_DATA_HOME=private+'/data', XMODIFIERS='@im=ibus',
                       GTK_IM_MODULE='ibus', QT_IM_MODULE='ibus', WINIT_X11_SCALE_FACTOR='1', LANG='C.UTF-8',
                       IBUS_ENABLE_SYNC_MODE='0')
            # IBus 1.5.29's synchronous PostProcessKeyEvent path drops show/hide
            # preedit signals (upstream fix 719792d300579c1bfdf43251a83c6ed4e5594c07).
            # Use the supported asynchronous bridge in this owned session only.
            # This is not evidence that the affected default sync mode is fixed.
            # https://github.com/ibus/ibus/commit/719792d300579c1bfdf43251a83c6ed4e5594c07
            env.pop('WAYLAND_DISPLAY', None)
            env.pop('IBUS_ADDRESS', None)
            with (args.output / 'dbus.log').open('w') as log:
                bus = subprocess.Popen(['dbus-daemon', '--session', '--nofork', '--print-address=1'], stdout=subprocess.PIPE, stderr=log, env=env, text=True, start_new_session=True)
                if not select.select([bus.stdout], [], [], 10)[0]:
                    raise RuntimeError('D-Bus startup timed out')
                env['DBUS_SESSION_BUS_ADDRESS'] = bus.stdout.readline().strip()
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
                app = subprocess.Popen([str(args.binary.resolve())], env=env, stdout=output, stderr=output)
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
                from PIL import ImageGrab
                def bounds(window_id):
                    info = subprocess.check_output(['xwininfo', '-id', str(window_id)], env=env, text=True)
                    return {name: int(re.search(pattern, info).group(1)) for name, pattern in {
                        'x': r'Absolute upper-left X:\s+(-?\d+)', 'y': r'Absolute upper-left Y:\s+(-?\d+)',
                        'width': r'Width:\s+(\d+)', 'height': r'Height:\s+(\d+)'}.items()}
                origin = bounds(window)
                def capture(stage):
                    # Each sample must include a fresh model/geometry tick after input.
                    time.sleep(.55)
                    trace = (args.output/'application.log').read_text()
                    tick = trace[trace.rfind('MODEL '):]
                    models = re.search(r'MODEL \d+ first=(".*?") second=(".*?") focus=', tick)
                    if models is None: raise AssertionError('Missing model sample')
                    sample = {'stage': stage, 'first': json.loads(models.group(1)), 'second': json.loads(models.group(2)), 'origin': origin, 'candidates': []}
                    for label in ('first', 'second'):
                        rendered = re.search(r'DISPLAY '+label+r' (".*?") Rect', tick)
                        sample['display_'+label] = json.loads(rendered.group(1))
                        caret = re.search(r'CARET '+label+r' Rect \{ x: ([\d.]+), y: ([\d.]+), width: ([\d.]+), height: ([\d.]+)', tick)
                        sample['caret_'+label] = dict(zip(('x','y','width','height'), map(float,caret.groups())))
                    candidates = subprocess.run(['xdotool','search','--onlyvisible','--class','Ibus-ui-gtk3'],env=env,capture_output=True,text=True)
                    for candidate in candidates.stdout.splitlines():
                        rectangle = bounds(candidate)
                        if rectangle['width'] > 100 and rectangle['height'] > 20: sample['candidates'].append(rectangle)
                    snapshots.append(sample)
                    ImageGrab.grab(xdisplay=display).save(args.output/f'ime-{stage}.png')
                    (args.output/f'windows-{stage}.log').write_text(subprocess.check_output(['xwininfo','-root','-tree'],env=env,text=True))
                click(100,82)
                use_ibus_engine(env,'libpinyin',ime)
                type_text('nihao');capture(1)
                key('space');capture(2)
                type_text('zhong');capture(3)
                key('Escape');capture(4)
                type_text('wo');capture(5)
                click(100,174);capture(6)
                type_text('shijie');capture(7)
                key('space');capture(8)
                app.wait(timeout=25)
            text=(args.output/'application.log').read_text()
            if app.returncode: raise RuntimeError(text)
            if 'COMMIT first "你好"' not in text or 'COMMIT second "世界"' not in text:
                raise AssertionError('Native Unicode commits missing; see application.log')
            if 'FINAL first="你好" second="世界"' not in text:
                raise AssertionError('Unexpected final model; see application.log')
            (args.output/'snapshots.json').write_text(json.dumps(snapshots,indent=2)+'\n')
            expected = [('', '', '你好', ''), ('你好', '', '你好', ''), ('你好', '', '你好中', ''), ('你好', '', '你好', ''), ('你好', '', '你好我', ''), ('你好', '', '你好', ''), ('你好', '', '你好', '世界'), ('你好', '世界', '你好', '世界')]
            for sample, values in zip(snapshots, expected):
                actual = tuple(sample[key] for key in ('first','second','display_first','display_second'))
                if actual != values: raise AssertionError(f'Stage {sample["stage"]}: {actual!r} != {values!r}')
            for stage, label in ((1,'first'),(3,'first'),(5,'first'),(7,'second')):
                sample=snapshots[stage-1]
                caret=sample['caret_'+label]
                desired_x=sample['origin']['x']+caret['x']+caret['width']
                desired_y=sample['origin']['y']+caret['y']+caret['height']
                if not any(abs(r['x']-desired_x)<=3 and abs(r['y']-desired_y)<=3 for r in sample['candidates']):
                    raise AssertionError(f'Candidate does not follow caret below text: {sample!r}')
            if text.count('COMMIT ') != 2: raise AssertionError('Unexpected extra commit: '+text)
            result={'result':'pass' ,'backend':'X11 XIM / IBus libpinyin','ibus_enable_sync_mode':env['IBUS_ENABLE_SYNC_MODE'],'ibus_version':subprocess.check_output(['ibus','version'],env=env,text=True).strip(),'limitations':['IBus 1.5.29 default synchronous bridge omits preedit lifecycle signals; this run explicitly uses the asynchronous bridge.'],'checks':['native preedit','Unicode commits','Escape cancellation','focus switch cancellation','separate editor models','candidate popup follows caret below text'],'snapshots':snapshots}
            (args.output/'result.json').write_text(json.dumps(result,indent=2)+'\n')
            print('PASS: '+', '.join(result['checks']))
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

if __name__=='__main__':main()
