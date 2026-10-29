#!/usr/bin/env python3
"""A popover opened from a row of another's panel, each in a popup window of
its own, on macOS: a click in the outer popup's window closes the nested
popover and leaves the outer open; a click on the outer's field focuses it as
it closes the nested one; a click in the main window closes both.

    python3 scripts/macos_nested_popover_probe.py [--target-dir DIR]

Builds the nested_popover_probe example and runs it (see its header): it
posts AppKit events to its own windows only, each inside one. The user's
pointer is never moved and nothing outside the process is touched.

It is launched as a direct child, from /tmp, with a clean environment. If a
system dialog appears, stop and report it; never click it. macOS only.
"""
import argparse
import os
import pathlib
import platform
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
    subprocess.run(['cargo', 'build', '-q', '-p', 'zgx-desktop', '--example', 'nested_popover_probe', '--locked'],
                   cwd=ROOT, env=env, check=True)
    with tempfile.TemporaryDirectory(prefix='zgx-nested-popover-') as stage:
        binary = pathlib.Path(stage) / 'nested_popover_probe'
        shutil.copy2(pathlib.Path(args.target_dir) / 'debug/examples/nested_popover_probe', binary)
        clean = {key: os.environ[key] for key in ('HOME', 'TMPDIR') if key in os.environ}
        clean['PATH'] = '/usr/bin:/bin'
        out = subprocess.run([str(binary)], cwd='/tmp', env=clean, capture_output=True, text=True,
                             timeout=60).stdout
    print(out, end='')
    lines = out.splitlines()
    line = lambda label: next((l for l in lines if l.startswith(label + ' ')), '')
    checks = [
        ('the outer popover opens in a popup window', line('OPENED') == 'OPENED outer=true inner=false popups=1'),
        ('its row opens the nested one', line('NESTED') == 'NESTED outer=true inner=true popups=2'),
        ('blank space in the outer panel closes the nested one alone',
         line('BLANK') == 'BLANK outer=true inner=false popups=1'),
        ('the outer field focuses as the nested one closes',
         line('FIELD') == 'FIELD outer=true inner=false focused=true'),
        ('a click in the main window closes both', line('OUTSIDE') == 'OUTSIDE outer=false inner=false popups=0'),
    ]
    failed = [name for name, ok in checks if not ok]
    for name, ok in checks:
        print(('PASS ' if ok else 'FAIL ') + name)
    return 1 if failed else 0


if __name__ == '__main__':
    sys.exit(main())
