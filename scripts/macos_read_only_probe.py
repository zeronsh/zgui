#!/usr/bin/env python3
"""A read-only editor on macOS: typing, Backspace, Return and an input
method's marked and committed text change nothing, while a click focuses it,
Command-A selects all of it and an arrow moves the caret.

    python3 scripts/macos_read_only_probe.py [--target-dir DIR]

Builds the read_only_probe example and runs it (see its header): it posts
AppKit events to its own window and calls its own text input view as an
input method would. Nothing reaches the user's pointer, keyboard, input
sources or pasteboard (copying is not checked: it would write the
pasteboard).

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
TEXT = 'hello world'


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument('--target-dir', default=os.environ.get('CARGO_TARGET_DIR', str(ROOT / 'target')))
    args = parser.parse_args()
    if platform.system() != 'Darwin':
        print('macOS only')
        return 2
    env = dict(os.environ, CARGO_TARGET_DIR=args.target_dir)
    subprocess.run(['cargo', 'build', '-q', '-p', 'zgx-desktop', '--example', 'read_only_probe', '--locked'],
                   cwd=ROOT, env=env, check=True)
    with tempfile.TemporaryDirectory(prefix='zgx-read-only-') as stage:
        binary = pathlib.Path(stage) / 'read_only_probe'
        shutil.copy2(pathlib.Path(args.target_dir) / 'debug/examples/read_only_probe', binary)
        clean = {key: os.environ[key] for key in ('HOME', 'TMPDIR') if key in os.environ}
        clean['PATH'] = '/usr/bin:/bin'
        out = subprocess.run([str(binary)], cwd='/tmp', env=clean, capture_output=True, text=True,
                             timeout=90).stdout
    print(out, end='')
    line = lambda label: next((l for l in out.splitlines() if l.startswith(label + ' ')), '')
    unchanged = f'value={TEXT!r}'.replace("'", '"')
    checks = [
        ('focused by a click, read-only', line('FOCUS') == 'FOCUS focused=true read_only=true'),
        ('a letter types nothing', line('TYPED') == f'TYPED {unchanged}'),
        ('Backspace deletes nothing', line('DELETE') == f'DELETE {unchanged}'),
        ('Return inserts nothing', line('RETURN') == f'RETURN {unchanged}'),
        ('Command-A selects all', line('SELECT_ALL') == f'SELECT_ALL selection=Some((0, {len(TEXT)}))'),
        ('an arrow moves the caret', line('ARROW') == f'ARROW selection=Some(({len(TEXT)}, {len(TEXT)}))'),
        # `has_marked` is winit's own buffer, filled because the probe calls
        # past the gate a real input method meets (see the example's header).
        ('marked text composes nothing',
         re.fullmatch(rf'MARKED has_marked=\w+ preedit=Some\(false\) {re.escape(unchanged)}', line('MARKED'))
         is not None),
        ('committed text inserts nothing', line('COMMIT') == f'COMMIT preedit=Some(false) {unchanged}'),
        ('still focused', line('FOCUS_AFTER') == 'FOCUS_AFTER focused=true'),
        ('finished', 'DONE' in out.splitlines()),
    ]
    failed = [name for name, ok in checks if not ok]
    for name, ok in checks:
        print(f"{'ok  ' if ok else 'FAIL'} {name}")
    if failed:
        print(f'read_only_probe: {len(failed)} of {len(checks)} failed', file=sys.stderr)
        return 1
    print(f'read_only_probe: {len(checks)} of {len(checks)} passed')
    return 0


if __name__ == '__main__':
    sys.exit(main())
