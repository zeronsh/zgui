#!/usr/bin/env python3
"""The caret Core Animation draws on macOS (`Ui::native_caret`), against the
editor's own, in real windows of the caret_idle example.

    python3 scripts/macos_caret_probe.py [--target-dir DIR]

Each case launches the field focused twice, Core Animation's caret and the
editor's (`ZGX_NATIVE_CARET=0`), with the system's blink held shown, and
captures the window at its full pixel size (scripts/window_pixels.swift,
ScreenCaptureKit): the two must match pixel for pixel, at the start,
middle and end of the text, in mixed and right-to-left text, and zoomed
(90%, 125%, 175%). Held hidden (shown 50 ms, then hidden for an hour), the
two must match again and differ from shown at the caret alone: the editor
paints nothing under Core Animation's. Then, blinking as the system does:
captures over two seconds see the caret come and go while the app presents
no frame (`ZGX_PRESENT_LOG`), and the footprint ends below half the
editor's caret's (the Metal driver keeps about 176 MB for a command queue
some 8 s after it last ran, M5, 2026-10-27).

Needs Screen Recording permission for the shell running it, which it only
checks. Launched from /tmp with a clean environment. If a system dialog
appears, stop and report it; never click it. macOS only.
"""
import argparse
import json
import os
import pathlib
import platform
import re
import shutil
import subprocess
import sys
import tempfile
import time

ROOT = pathlib.Path(__file__).resolve().parents[1]
HELD = ['-NSTextInsertionPointBlinkPeriodOn', '3600000']
HIDDEN = ['-NSTextInsertionPointBlinkPeriodOn', '50', '-NSTextInsertionPointBlinkPeriodOff', '3600000']
CASES = {
    'end': {},
    'start': {'ZGX_CARET_AT': '0'},
    'middle': {'ZGX_CARET_AT': '7'},
    'mixed': {'ZGX_TEXT': 'abc אבג def', 'ZGX_CARET_AT': '6'},
    'right-to-left': {'ZGX_TEXT': 'שלום עולם', 'ZGX_CARET_AT': '8'},
    'zoom 90%': {'ZGX_ZOOM': '0.9', 'ZGX_CARET_AT': '3'},
    'zoom 125%': {'ZGX_ZOOM': '1.25'},
    'zoom 175%': {'ZGX_ZOOM': '1.75', 'ZGX_CARET_AT': '11'},
}


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument('--target-dir', default=os.environ.get('CARGO_TARGET_DIR', str(ROOT / 'target')))
    args = parser.parse_args()
    if platform.system() != 'Darwin':
        print('macOS only')
        return 2
    env = dict(os.environ, CARGO_TARGET_DIR=args.target_dir)
    subprocess.run(['cargo', 'build', '-q', '--release', '-p', 'zgx-desktop', '--example', 'caret_idle', '--locked'],
                   cwd=ROOT, env=env, check=True)
    with tempfile.TemporaryDirectory(prefix='zgx-caret-') as stage:
        stage = pathlib.Path(stage)
        pixels, ready = stage / 'window_pixels', stage / 'window_ready'
        subprocess.run(['swiftc', '-O', str(ROOT / 'scripts/window_pixels.swift'), '-o', str(pixels)], check=True)
        subprocess.run(['swiftc', '-O', str(ROOT / 'scripts/window_ready.swift'), '-o', str(ready)], check=True)
        binary = stage / 'caret_idle'
        shutil.copy2(pathlib.Path(args.target_dir) / 'release/examples/caret_idle', binary)
        clean = {key: os.environ[key] for key in ('HOME', 'TMPDIR', 'USER', 'LANG') if key in os.environ}
        clean['PATH'] = '/usr/bin:/bin'

        def launch(extra, blink):
            app = subprocess.Popen([str(binary), *blink], cwd='/tmp', env={**clean, **extra},
                                   stdout=subprocess.DEVNULL, stderr=subprocess.PIPE, text=True)
            subprocess.run([str(ready), str(app.pid), '2.5'], capture_output=True)
            return app

        def shot(name, extra, blink=HELD):
            app = launch(extra, blink)
            try:
                time.sleep(1.5)
                path = stage / f'{name}.png'
                answer = subprocess.run([str(pixels), 'capture', str(app.pid), str(path)], capture_output=True, text=True)
                if 'error' in answer.stdout:
                    sys.exit(f'capture failed: {answer.stdout.strip()} (Screen Recording permission?)')
                return path
            finally:
                app.terminate()
                app.wait()

        def diff(a, b):
            return json.loads(subprocess.run([str(pixels), 'diff', str(a), str(b)], capture_output=True, text=True).stdout)

        checks = {}
        for name, extra in CASES.items():
            ours = diff(shot(f'{name} native', extra), shot(f'{name} editor', {**extra, 'ZGX_NATIVE_CARET': '0'}))
            checks[f'{name}: Core Animation\'s caret is the editor\'s, pixel for pixel'] = (ours['differing'] == 0, ours)
        for name, extra in (('hidden', {}), ('hidden, zoom 125%', {'ZGX_ZOOM': '1.25'})):
            native, editor = shot(f'{name} native', extra, HIDDEN), shot(f'{name} editor', {**extra, 'ZGX_NATIVE_CARET': '0'}, HIDDEN)
            same, caret = diff(native, editor), diff(native, shot(f'{name} shown', extra))
            box = caret['box']
            narrow = box[2] - box[0] < 8 and 0 < caret['differing'] < 1000
            checks[f'{name}: nothing paints the caret, and shown it is all that differs'] = (same['differing'] == 0 and narrow, {'same': same, 'caret': caret})
        # Blinking as the system blinks: it comes and goes with no frame drawn.
        app = launch({'ZGX_PRESENT_LOG': '1'}, [])
        try:
            time.sleep(1.5)
            captures = []
            for i in range(10):
                path = stage / f'blink {i}.png'
                subprocess.run([str(pixels), 'capture', str(app.pid), str(path)], capture_output=True, check=True)
                captures.append(path)
                time.sleep(0.2)
            states = {diff(captures[0], other)['differing'] > 0 for other in captures[1:]}
            footprint = lambda: float(re.search(r'Footprint:\s*([\d.]+)\s*MB', subprocess.run(
                ['footprint', '-p', str(app.pid)], capture_output=True, text=True).stdout).group(1))
            time.sleep(14)
            native_mb = footprint()
        finally:
            app.terminate()
            log = app.communicate()[1]
        presents = [float(line.split()[1]) for line in log.splitlines() if 'present frame=' in line]
        late = [at for at in presents if at > presents[0] + 1.0] if presents else []
        checks['blinking: it shows and hides'] = (states == {True, False}, sorted(states))
        checks['blinking: no frame presented while it blinks'] = (not late, f'{len(late)} late presents')
        editor = launch({'ZGX_NATIVE_CARET': '0'}, [])
        try:
            time.sleep(15.5)
            editor_mb = float(re.search(r'Footprint:\s*([\d.]+)\s*MB', subprocess.run(
                ['footprint', '-p', str(editor.pid)], capture_output=True, text=True).stdout).group(1))
        finally:
            editor.terminate()
            editor.wait()
        checks['footprint: below half the editor caret\'s'] = (native_mb < editor_mb / 2, f'{native_mb} MB against {editor_mb} MB')
    bad = 0
    for name, (ok, detail) in checks.items():
        print(f"{'ok  ' if ok else 'FAIL'} {name}: {detail}")
        bad += not ok
    return 1 if bad else 0


if __name__ == '__main__':
    sys.exit(main())
