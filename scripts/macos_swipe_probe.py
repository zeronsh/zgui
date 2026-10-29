#!/usr/bin/env python3
"""A trackpad's swipe between pages on macOS, by scroll gestures sent to the
app's own window.

    python3 scripts/macos_swipe_probe.py [--target-dir DIR]

Builds the swipe_probe example and runs it. Over a page that goes back and
forward (`View::on_navigate`), a row scrolling across takes the gestures it
can scroll (toward its end; back from the middle of its content) and
nothing navigates; a three-finger swipe (`swipeWithEvent:`) to the right
goes back, to the left forward. With the system's "Swipe between pages" set
to scroll with two fingers, a two-finger swipe to the right over the plain
box goes back, one over the row at its start too (it does not stretch),
and one to the left forward; set otherwise (never changed here), those
gestures scroll as before and nothing navigates, and the checks say SKIP.

The scrolls go to the probe's own window (`-[NSWindow sendEvent:]`): the
user's trackpad and other apps hear nothing. It is launched as a direct
child, from /tmp, with a clean environment. If a system dialog appears,
stop and report it; never click it. macOS only.

AppKit's tracking of a two-finger swipe follows the gesture's later scrolls
through the application; synthetic ones sent to the window may not drive
it. `swipe_probe --manual` shows the counts for a person's trackpad.
"""
import argparse
import os
import pathlib
import platform
import re
import shutil
import subprocess
import sys
import tempfile

ROOT = pathlib.Path(__file__).resolve().parents[1]


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument('--target-dir', default=os.environ.get('CARGO_TARGET_DIR', str(ROOT / 'target')))
    args = parser.parse_args()
    if platform.system() != 'Darwin':
        print('macOS only')
        return 2
    env = dict(os.environ, CARGO_TARGET_DIR=args.target_dir)
    subprocess.run(['cargo', 'build', '-q', '-p', 'zgx-desktop', '--example', 'swipe_probe', '--locked'],
                   cwd=ROOT, env=env, check=True)
    with tempfile.TemporaryDirectory(prefix='zgx-swipe-') as stage:
        binary = pathlib.Path(stage) / 'swipe_probe'
        shutil.copy2(pathlib.Path(args.target_dir) / 'debug/examples/swipe_probe', binary)
        clean = {key: os.environ[key] for key in ('HOME', 'TMPDIR', 'USER') if key in os.environ}
        clean['PATH'] = '/usr/bin:/bin'
        out = subprocess.run([str(binary)], cwd='/tmp', env=clean, capture_output=True, text=True,
                             timeout=60).stdout
    if 'NO_KEY_WINDOW' in out:
        print("the probe's window never became key (another window in front?): run it again")
        return 2
    # Each step's lines: what navigated, and the row's offset after.
    steps = {}
    for block in re.split(r'^STEP ', out, flags=re.M)[1:]:
        name, _, rest = block.partition('\n')
        row = re.search(r'^ROW (\S+)$', rest, re.M)
        steps[name] = (re.findall(r'^NAVIGATE (\S+)$', rest, re.M), float(row.group(1)) if row else None)
    went = lambda name: steps.get(name, ([], None))[0]
    row = lambda name: steps.get(name, ([], None))[1]
    two_fingers = 'SWIPES_FROM_SCROLLS true' in out
    checks = {
        'the row scrolls toward its end': (row('row-toward-end') or 0) > 100,
        'and back from the middle of its content': 0 < (row('row-mid-back') or 0) < (row('row-toward-end') or 0),
        'nothing navigates on what the row scrolls': not went('row-toward-end') and not went('row-mid-back'),
        'three fingers to the right go back': went('three-back') == ['back'],
        'three fingers to the left go forward': went('three-forward') == ['forward'],
        'done': 'SWIPE_DONE' in out,
    }
    swipes = {
        'two fingers to the right over the plain box go back': went('plain-back') == ['back'],
        'over the row at its start too, and it stays there': went('row-start-back') == ['back']
        and row('row-start-back') == 0,
        'two fingers to the left go forward': went('plain-forward') == ['forward'],
    }
    if two_fingers:
        checks.update(swipes)
    else:
        checks['two-finger gestures navigate nothing with the setting off'] = not (
            went('plain-back') or went('row-start-back') or went('plain-forward'))
    print('\n'.join(line for line in out.splitlines() if re.match(r'(STEP|NAVIGATE|ROW|SWIPES_FROM_SCROLLS) ', line)))
    for name, ok in checks.items():
        print(('ok  ' if ok else 'FAIL') + ' ' + name)
    if not two_fingers:
        for name in swipes:
            print('SKIP ' + name + ' ("Swipe between pages" is not set to two fingers)')
    return 0 if all(checks.values()) else 1


if __name__ == '__main__':
    sys.exit(main())
