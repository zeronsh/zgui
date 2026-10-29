#!/usr/bin/env python3
"""A window losing key status and getting it back, on macOS: as it goes to
the back its caret goes, hover and a press held on a button are let go and
Shift held is forgotten; back, the field has its focus and caret again,
typing reaches it, and the press let go then clicks nothing.

    python3 scripts/macos_deactivation_probe.py [--target-dir DIR]

Builds the deactivation_probe example and runs it (see its header): a helper
window of its own takes key status, as another app's window would, and gives
it back. No other app is activated and nothing outside the process is
touched. Run with `ZGX_NATIVE_CARET=0`, so the editor paints its caret where
the probe reads it.

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
    subprocess.run(['cargo', 'build', '-q', '-p', 'zgx-desktop', '--example', 'deactivation_probe', '--locked'],
                   cwd=ROOT, env=env, check=True)
    with tempfile.TemporaryDirectory(prefix='zgx-deactivation-') as stage:
        binary = pathlib.Path(stage) / 'deactivation_probe'
        shutil.copy2(pathlib.Path(args.target_dir) / 'debug/examples/deactivation_probe', binary)
        clean = {key: os.environ[key] for key in ('HOME', 'TMPDIR') if key in os.environ}
        clean['PATH'] = '/usr/bin:/bin'
        clean['ZGX_NATIVE_CARET'] = '0'
        out = subprocess.run([str(binary)], cwd='/tmp', env=clean, capture_output=True, text=True,
                             timeout=90).stdout
    print(out, end='')
    lines = out.splitlines()
    line = lambda label: next((l for l in lines if l.startswith(label + ' ')), '')
    checks = [
        ('before: focused, caret, hovered',
         line('BEFORE') == 'BEFORE focused=true caret=true hovered=button captured=none'),
        ('away: focus kept, no caret, nothing hovered, not key',
         line('AWAY') == 'AWAY focused=true caret=false hovered=none captured=none main_key=false'),
        ('back: focus and caret again, key',
         line('BACK') == 'BACK focused=true caret=true hovered=none captured=none main_key=true'),
        ('a key after coming back has no Shift', line('KEY') == 'KEY Character("x") shift=false'),
        ('typing reaches the field again', line('TYPED') == 'TYPED value="x"'),
        ('a press is captured', line('PRESSED').endswith('hovered=button captured=button')),
        ('going to the back lets the press go', line('PRESS_AWAY').endswith('hovered=none captured=none')),
        ('its release after coming back clicks nothing', 'CLICK' not in lines),
        ('finished', 'DONE' in lines),
    ]
    failed = [name for name, ok in checks if not ok]
    for name, ok in checks:
        print(f"{'ok  ' if ok else 'FAIL'} {name}")
    if failed:
        print(f'deactivation_probe: {len(failed)} of {len(checks)} failed', file=sys.stderr)
        return 1
    print(f'deactivation_probe: {len(checks)} of {len(checks)} passed')
    return 0


if __name__ == '__main__':
    sys.exit(main())
