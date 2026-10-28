#!/usr/bin/env python3
"""Cursor shapes, Insert, Escape in a held drag, a two-file drop and a custom
title bar's drag, on macOS, by AppKit events posted to the app's own window.

    python3 scripts/macos_native_checks_probe.py [--target-dir DIR]

Builds the native_checks_probe example and runs it (see its header): it
answers AppKit's pointer location and pressed buttons itself, posts mouse
events to its window and sends it keys, and hands the window's dragging
destination a drag of two files of its own, as Finder's are. Nothing reaches
the user's pointer, keyboard or pasteboard, and no drag leaves the window: a
press on the title bar's drag strip is recorded, not performed (the window
server would follow the real pointer), so the move itself is not checked.

It is launched as a direct child, from /tmp, with a clean environment. If a
system dialog appears, stop and report it; never click it. macOS only.
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
CURSORS = ['arrow', 'pointer', 'text', 'move', 'not_allowed', 'col_resize', 'arrow']


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument('--target-dir', default=os.environ.get('CARGO_TARGET_DIR', str(ROOT / 'target')))
    args = parser.parse_args()
    if platform.system() != 'Darwin':
        print('macOS only')
        return 2
    env = dict(os.environ, CARGO_TARGET_DIR=args.target_dir)
    subprocess.run(['cargo', 'build', '-q', '-p', 'zgx-desktop', '--example', 'native_checks_probe', '--locked'],
                   cwd=ROOT, env=env, check=True)
    with tempfile.TemporaryDirectory(prefix='zgx-checks-') as stage:
        binary = pathlib.Path(stage) / 'native_checks_probe'
        shutil.copy2(pathlib.Path(args.target_dir) / 'debug/examples/native_checks_probe', binary)
        clean = {key: os.environ[key] for key in ('HOME', 'TMPDIR') if key in os.environ}
        clean['PATH'] = '/usr/bin:/bin'
        out = subprocess.run([str(binary)], cwd='/tmp', env=clean, capture_output=True, text=True,
                             timeout=90).stdout
    cursors = re.findall(r'^CURSOR (\w+) ok=(\w+)$', out, re.M)
    keys = re.findall(r'^KEY (.*)$', out, re.M)
    lines = out.splitlines()
    # What each drag printed, up to its own summary line.
    def before(label):
        for i, line in enumerate(lines):
            if line.startswith(label):
                start = max((j + 1 for j in range(i) if re.match(r'(DRAG_DROP|DRAG_ESCAPE|FILES_DRAG) ', lines[j])),
                            default=0)
                return lines[start:i], line
        return [], ''
    dropped, drop_line = before('DRAG_DROP')
    escaped, escape_line = before('DRAG_ESCAPE')
    files = [before(f'FILES_DRAG end={end}') for end in ('Dropped', 'Out', 'Beside')]
    sent = re.search(r'sent=(\[.*\])', files[0][1])
    title = re.search(r'^TITLE_DRAG strip=(\d+) down=(\w+) at=Some\(\(([\d.]+), ([\d.]+)\)\) button_drags=(\d+)$', out, re.M)
    checks = {
        'each box sets its cursor, the arrow elsewhere': [name for name, _ in cursors] == CURSORS
        and all(ok == 'true' for _, ok in cursors),
        'Insert (the Help key, 0x72) arrives as Insert': any(k.startswith('Insert shift=false control=false') for k in keys),
        'a drag let go over its destination drops': 'DROPPED' in dropped and 'DRAG_END accepted=true' in dropped,
        'Escape ends a held drag': 'held=true after=false' in escape_line,
        'after Escape, letting go there drops nothing': 'DROPPED' not in escaped and 'DRAG_END accepted=false' in escaped,
        'two files dropped on the box arrive as one batch, in order': bool(sent)
        and [l for l in files[0][0] if l.startswith('FILES ')] == [f'FILES {sent.group(1)}'],
        'files dragged over the box and out drop nothing': files[1][1] != '' and not any(l.startswith('FILES ') for l in files[1][0]),
        'files let go beside the box drop nothing there': files[2][1] != '' and not any(l.startswith('FILES ') for l in files[2][0]),
        'a press on the title strip asks for a window drag from it': bool(title) and title.group(1) == '1'
        and title.group(2) == 'true' and (float(title.group(3)), float(title.group(4))) == (300., 20.),
        'a press on the strip\'s button clicks it, no drag': bool(title) and title.group(5) == '0' and 'BUTTON pin' in out,
        'done': 'DONE' in out,
    }
    print(out, end='')
    for name, ok in checks.items():
        print(('ok  ' if ok else 'FAIL') + ' ' + name)
    print('not checked: the window following the real pointer once the window server moves it')
    return 0 if all(checks.values()) else 1


if __name__ == '__main__':
    sys.exit(main())
