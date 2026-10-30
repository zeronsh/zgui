#!/usr/bin/env python3
"""Drive an owned IBus/libpinyin XIM session with actual native key events.

`--password`: the first editor is a password field. Pinyin typed into it
types its letters (no preedit, nothing composed), as a browser's password
field takes no input method.

`--server-gone`: the input method's server (ibus-x11) goes while a focus
for the app is queued behind it (the app stopped meanwhile), as when IBus
went as a window closed (#564); the app takes the focus without waiting on
the server, stays responsive, and closes on Alt+F4.

`--server-stopped`: the input method's server is there but answers nothing
(ibus-x11 stopped) as the app closes its focused window. Closing must not
wait on it: the app exits (#564, a closing window blocked in a synchronous
XIM round trip, its server alive)."""
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

from platform_smoke import release_mounts, stop, wait_for_wm, use_ibus_engine, wait_for_ibus_xim, wait_for_exit, close_window


def password(env, app, window, xdo, key, type_text, output):
    """Pinyin into a password field: the letters themselves, nothing composed."""
    type_text('nihao'); key('space')
    deadline = time.monotonic() + 10
    while True:
        log = (output/'application.log').read_text()
        models = re.findall(r'MODEL \d+ first=(".*?") second=', log)
        if models and json.loads(models[-1]) == 'nihao ':
            break
        if time.monotonic() > deadline:
            raise AssertionError(f'the password field took {models[-1:] or "nothing"}, not the letters typed')
        time.sleep(.1)
    composed = [line for line in log.splitlines() if line.startswith(('PREEDIT first', 'COMMIT first'))]
    if composed:
        raise AssertionError(f'an input method composed into the password field: {composed}')
    xdo('windowactivate', window); time.sleep(.3); xdo('key', 'alt+F4')
    wait_for_exit(app, env, output, 10)
    (output/'result.json').write_text(json.dumps({'result': 'pass'}, indent=2)+'\n')
    print('PASS: pinyin typed into a password field typed its letters; no input method composed there')


def server_gone(env, app, window, xdo, key, type_text, output):
    """ibus-x11 killed with a focus for the app queued ahead of the news:
    the app, stopped, has focus moved to it from another window, then the
    server goes, then the app runs on. It must take the focus without a
    round trip to the server, answer input, and close on Alt+F4."""
    from platform_smoke import display_processes
    type_text('nihao'); key('space')
    deadline = time.monotonic() + 10
    while 'COMMIT first "你好"' not in (output/'application.log').read_text():
        if time.monotonic() > deadline: raise AssertionError('the composition did not commit')
        time.sleep(.05)
    other = subprocess.Popen(['xmessage', '-geometry', '+600+600', 'another window'], env=env,
                             stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    try:
        deadline = time.monotonic() + 10
        while not (found := subprocess.run(['xdotool', 'search', '--onlyvisible', '--class', 'Xmessage'], env=env,
                                           capture_output=True, text=True).stdout.split()):
            if time.monotonic() > deadline: raise AssertionError('the other window did not show')
            time.sleep(.05)
        xdo('windowactivate', '--sync', found[0]); time.sleep(.5)
        servers = display_processes(env, 'ibus-x11')
        if not servers: raise AssertionError("no ibus-x11 on the probe's display")
        os.kill(app.pid, signal.SIGSTOP)
        try:
            subprocess.run(['xdotool', 'windowactivate', window], env=env, capture_output=True)
            time.sleep(.5)
            for pid in servers: os.kill(pid, signal.SIGKILL)
            time.sleep(.5)
        finally:
            os.kill(app.pid, signal.SIGCONT)
        # Still answering: it reports its model each half second.
        seen = (output/'application.log').read_text().count('MODEL ')
        deadline = time.monotonic() + 5
        while (output/'application.log').read_text().count('MODEL ') < seen + 2:
            if time.monotonic() > deadline:
                from platform_smoke import dump_display
                dump_display(env, output, app.pid)
                raise AssertionError('the app stopped answering once the input method went (a round trip to it)')
            time.sleep(.1)
        xdo('windowactivate', window); time.sleep(.3); xdo('key', 'alt+F4')
        wait_for_exit(app, env, output, 10)
    finally:
        other.kill()
    (output/'result.json').write_text(json.dumps({'result': 'pass', 'killed': servers}, indent=2)+'\n')
    print('PASS: the input method gone with a focus queued, the app took it, answered and closed')


def server_stopped(env, app, window, xdo, key, type_text, output):
    """ibus-x11 stopped, alive but answering nothing, as the app's focused
    window closes: the app must exit, its close waiting on no round trip to
    the input method."""
    from platform_smoke import display_processes
    type_text('nihao'); key('space')
    deadline = time.monotonic() + 10
    while 'COMMIT first "你好"' not in (output/'application.log').read_text():
        if time.monotonic() > deadline: raise AssertionError('the composition did not commit')
        time.sleep(.05)
    servers = display_processes(env, 'ibus-x11')
    if not servers: raise AssertionError("no ibus-x11 on the probe's display")
    xdo('windowactivate', '--sync', window); time.sleep(.5)
    for pid in servers: os.kill(pid, signal.SIGSTOP)
    try:
        # The app closes its focused window itself (ZGX_SECONDS): hidden,
        # it loses the focus, and its input context goes, the server
        # answering nothing.
        wait_for_exit(app, env, output, 20)
    finally:
        for pid in servers: os.kill(pid, signal.SIGCONT)
    (output/'result.json').write_text(json.dumps({'result': 'pass', 'stopped': servers}, indent=2)+'\n')
    print('PASS: the input method answering nothing, the app\'s window closed and it exited')


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('binary', type=pathlib.Path)
    parser.add_argument('--output', type=pathlib.Path, required=True)
    parser.add_argument('--server-gone', action='store_true', help="ibus-x11 goes with a focus queued (#564)")
    parser.add_argument('--password', action='store_true', help="a password field takes no input method")
    parser.add_argument('--server-stopped', action='store_true', help="ibus-x11 answers nothing as the window closes (#564)")
    args = parser.parse_args()
    for executable in ('Xvfb', 'openbox', 'dbus-daemon', 'gsettings', 'ibus-daemon', 'ibus', 'xdotool', 'xwininfo'):
        if shutil.which(executable) is None:
            parser.error('Missing required executable: '+executable)
    memconf = next((candidate for candidate in (shutil.which('ibus-memconf'), '/usr/libexec/ibus-memconf', '/usr/lib/ibus/ibus-memconf', '/usr/lib/x86_64-linux-gnu/ibus/ibus-memconf') if candidate and os.access(candidate, os.X_OK)), None)
    if memconf is None:
        parser.error('Missing ibus-memconf; install the IBus package with its memory configuration service')
    args.output.mkdir(parents=True, exist_ok=True)
    (args.output / 'result.json').unlink(missing_ok=True)
    xvfb = wm = bus = ime = app = xim = None
    snapshots = []
    with tempfile.TemporaryDirectory(prefix='zgx-native-ime-') as private:
        try:
            with (args.output / 'xvfb.log').open('w') as log:
                xvfb = subprocess.Popen(['Xvfb', '-r', '-noreset', '-displayfd', '1', '-screen', '0', '1024x768x24', '-ac'], stdout=subprocess.PIPE, stderr=log, text=True)
                if not select.select([xvfb.stdout], [], [], 30)[0]:
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
                ime = subprocess.Popen(['ibus-daemon', '--replace', '--cache=none', '--emoji-extension=disable', '--config='+memconf, '--verbose'], env=env, stdout=log, stderr=log, start_new_session=True)
            deadline = time.monotonic()+10
            while True:
                ready = subprocess.run(['ibus', 'list-engine'], env=env, capture_output=True, text=True)
                if ready.returncode == 0 and 'libpinyin' in ready.stdout:
                    break
                if time.monotonic()>deadline or ime.poll() is not None:
                    raise RuntimeError('IBus failed: '+ready.stdout+ready.stderr)
                time.sleep(.2)
            # The XIM frontend started here, not by the daemon (`--xim`), its
            # output and exit kept: #564's app hung on its way out in a
            # round trip to it, and by then it had gone, how unknown.
            xim_log = (args.output / 'ibus-x11.log').open('w')
            xim = subprocess.Popen(['/usr/libexec/ibus-x11', '--kill-daemon'], env=env, stdout=xim_log, stderr=xim_log,
                                   start_new_session=True)
            wait_for_ibus_xim(env)
            def xdo(*arguments):
                return subprocess.check_output(['xdotool', *map(str,arguments)], env=env, text=True).strip()
            with (args.output / 'application.log').open('w') as output:
                # --server-stopped: the app closes its own window (after 12 s),
                # as it would quitting: no key and no window manager grab in
                # flight to the stopped input method.
                app_env = dict(env, ZGX_SECONDS='12') if args.server_stopped else env
                app = subprocess.Popen([str(args.binary.resolve())] + (['--password'] if args.password else []),
                                       env=app_env, stdout=output, stderr=output)
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
                def near(found, caret):
                    want = (origin['x']+caret['x']+caret['width'], origin['y']+caret['y']+caret['height'])
                    return any(abs(r['x']-want[0])<=3 and abs(r['y']-want[1])<=3 for r in found)
                def candidates_near(caret, deadline):
                    """The candidate popups, once one sits at `caret`'s bottom
                    right: IBus moves its popup after the preedit changes,
                    so a sample taken meanwhile finds it where it was."""
                    while True:
                        found = []
                        listed = subprocess.run(['xdotool','search','--onlyvisible','--class','Ibus-ui-gtk3'],env=env,capture_output=True,text=True)
                        for candidate in listed.stdout.splitlines():
                            try: rectangle = bounds(candidate)
                            except subprocess.CalledProcessError: continue
                            if rectangle['width'] > 100 and rectangle['height'] > 20: found.append(rectangle)
                        if near(found, caret) or time.monotonic()>deadline:
                            return found
                        time.sleep(.1)
                # Each stage's models and what they display: (first, second,
                # first shown, second shown).
                expected = [('', '', '你好', ''), ('你好', '', '你好', ''), ('你好', '', '你好中', ''), ('你好', '', '你好', ''), ('你好', '', '你好我', ''), ('你好', '', '你好', ''), ('你好', '', '你好', '世界'), ('你好', '世界', '你好', '世界')]
                def shown(tick):
                    values = [re.search(r'MODEL \d+ first=(".*?") second=(".*?") focus=', tick)]
                    values += [re.search(r'DISPLAY '+label+r' (".*?") Rect', tick) for label in ('first', 'second')]
                    if not all(values): return None
                    return (*map(json.loads, values[0].groups()), json.loads(values[1].group(1)), json.loads(values[2].group(1)))
                nudges = []
                def capture(stage, follows=None):
                    # A tick that begins after the input settled: one printed
                    # before a late preedit arrived shows the one before it.
                    # IBus converts as it is typed, out of step with the keys:
                    # "shiji" shows 世纪 until the last "e" makes it 世界, which
                    # a loaded machine delivers later. Ticks are read until one
                    # shows the stage's text, or 5 s pass (the last is checked).
                    time.sleep(.55)
                    settled = len((args.output/'application.log').read_text())
                    # The app may be writing it: read until it is whole.
                    deadline = time.monotonic()+5
                    nudge_at = time.monotonic()+1.5
                    while True:
                        trace = (args.output/'application.log').read_text()
                        tick = trace[trace.rfind('MODEL '):]
                        whole = trace.rfind('MODEL ') >= settled and tick.count('DISPLAY ') == 2 and tick.count('CARET ') == 2
                        if (whole and shown(tick) == expected[stage-1]) or time.monotonic()>deadline: break
                        # ibus-x11 1.5.29, asynchronous, can hold a preedit
                        # update until its next XIM event: IBus sent 我, and
                        # no ClientMessage reached the app for 6 s, until a
                        # click (2 of 160 runs, traced on both sides). One key
                        # that means nothing to libpinyin lets it go. What
                        # the app shows after that is still checked.
                        if nudge_at is not None and time.monotonic()>nudge_at:
                            nudge_at = None
                            nudges.append(stage)
                            xdo('key','F24')
                        if whole:
                            # The next tick (the example prints one each half
                            # second).
                            settled = len(trace)
                        time.sleep(.05)
                    models = re.search(r'MODEL \d+ first=(".*?") second=(".*?") focus=', tick)
                    if models is None: raise AssertionError('Missing model sample')
                    sample = {'stage': stage, 'first': json.loads(models.group(1)), 'second': json.loads(models.group(2)), 'origin': origin, 'candidates': []}
                    for label in ('first', 'second'):
                        rendered = re.search(r'DISPLAY '+label+r' (".*?") Rect', tick)
                        sample['display_'+label] = json.loads(rendered.group(1))
                        caret = re.search(r'CARET '+label+r' Rect \{ x: ([\d.]+), y: ([\d.]+), width: ([\d.]+), height: ([\d.]+)', tick)
                        sample['caret_'+label] = dict(zip(('x','y','width','height'), map(float,caret.groups())))
                    if follows:
                        caret = sample['caret_'+follows]
                        found = candidates_near(caret, time.monotonic()+.5)
                        # IBus moves its candidates to a new spot only when it
                        # next lays them out (a lookup table update): a spot
                        # after its last one, as when a frame was rendering as
                        # the preedit came, waits for the next key. Down and Up
                        # lay them out again, the preedit unchanged.
                        sample['followed_at_once'] = near(found, caret)
                        if not sample['followed_at_once']:
                            key('Down'); key('Up')
                            found = candidates_near(caret, time.monotonic()+3)
                        sample['candidates'] = found
                    snapshots.append(sample)
                    ImageGrab.grab(xdisplay=display).save(args.output/f'ime-{stage}.png')
                    # Evidence: a popup closing as xwininfo walks the tree fails it, and that fails nothing here.
                    (args.output/f'windows-{stage}.log').write_text(subprocess.run(['xwininfo','-root','-tree'],env=env,text=True,capture_output=True).stdout)
                click(100,82)
                use_ibus_engine(env,'libpinyin',ime)
                if args.server_gone:
                    server_gone(env, app, window, xdo, key, type_text, args.output)
                    return
                if args.password:
                    password(env, app, window, xdo, key, type_text, args.output)
                    return
                if args.server_stopped:
                    server_stopped(env, app, window, xdo, key, type_text, args.output)
                    return
                type_text('nihao');capture(1,'first')
                key('space');capture(2)
                type_text('zhong');capture(3,'first')
                key('Escape');capture(4)
                type_text('wo');capture(5,'first')
                click(100,174);capture(6)
                type_text('shijie');capture(7,'second')
                key('space');capture(8)
                # Closed as a user closes it (the window manager's close:
                # WM_DELETE_WINDOW), not by the app's own timer, which a quick
                # run would wait out; with no key, which this asynchronous
                # IBus could still be handling as the window went, and crash.
                close_window(env, window)
                wait_for_exit(app, env, args.output, 25)
            text=(args.output/'application.log').read_text()
            if app.returncode: raise RuntimeError(text)
            if 'COMMIT first "你好"' not in text or 'COMMIT second "世界"' not in text:
                raise AssertionError('Native Unicode commits missing; see application.log')
            if 'FINAL first="你好" second="世界"' not in text:
                raise AssertionError('Unexpected final model; see application.log')
            (args.output/'snapshots.json').write_text(json.dumps(snapshots,indent=2)+'\n')
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
            result={'result':'pass' ,'backend':'X11 XIM / IBus libpinyin','ibus_enable_sync_mode':env['IBUS_ENABLE_SYNC_MODE'],'nudged_stages':nudges,'ibus_version':subprocess.check_output(['ibus','version'],env=env,text=True).strip(),'limitations':['IBus 1.5.29 default synchronous bridge omits preedit lifecycle signals; this run explicitly uses the asynchronous bridge.'],'checks':['native preedit','Unicode commits','Escape cancellation','focus switch cancellation','separate editor models','candidate popup follows caret below text'],'snapshots':snapshots}
            (args.output/'result.json').write_text(json.dumps(result,indent=2)+'\n')
            print('PASS: '+', '.join(result['checks']))
        finally:
            stop(app)
            if xim:
                # Still running (None), exited (its status) or killed (minus
                # the signal), before anything here stops it.
                (args.output / 'ibus-x11.log').open('a').write(f'\nibus-x11 status before cleanup: {xim.poll()}\n')
                stop(xim)
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
