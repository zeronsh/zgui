#!/usr/bin/env python3
"""App shortcuts while a native view has the keyboard on macOS, by AppKit key
events posted to the app's own queue.

    python3 scripts/macos_key_routing_probe.py [--ghostty] [--target-dir DIR]

Builds the key_routing_probe example (with `--ghostty`, its terminal pane
too) and runs it. With zgx's own box focused, a menu item's key nothing else
uses (⌘U), a menu item's key the page or terminal takes (⌘K) and a zgx key
binding in no menu (⌘⇧L) each fire. With the page focused, the page hears
all three; ⌘U and ⌘⇧L, which it leaves alone, still reach the app, and ⌘K,
which it takes, does not. With the terminal focused, ⌘U and ⌘⇧L reach the
app and ⌘K stays Ghostty's. AppKit's own menu keys still act with each
part focused: ⌘H hides the app, ⌘M minimizes the window, ⌘Q quits.

The keys go to the app's own queue (`-[NSApplication postEvent:atStart:]`),
not the system: the user's keyboard and other apps hear nothing. It is
launched as a direct child, from /tmp, with a clean environment. If a
system dialog appears, stop and report it; never click it. macOS only.

`--ime ja` instead types a composition into a field of the page with an
enabled Japanese input method (it is selected for the run and the previous
input source selected again after; none is ever enabled here, and with none
enabled the run is skipped), presses ⌘U in the middle of it and Return: the
page composes and commits Japanese text, and the ⌘ key's fate is reported.
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
    parser.add_argument('--ghostty', action='store_true', help='also a Ghostty terminal (the ghostty feature)')
    parser.add_argument('--ime', choices=['ja'], help='a composition in the page, with an enabled input method')
    args = parser.parse_args()
    if platform.system() != 'Darwin':
        print('macOS only')
        return 2
    env = dict(os.environ, CARGO_TARGET_DIR=args.target_dir)
    build = ['cargo', 'build', '-q', '-p', 'zgx-desktop', '--example', 'key_routing_probe', '--locked']
    if args.ghostty:
        build += ['--features', 'ghostty']
    subprocess.run(build, cwd=ROOT, env=env, check=True)
    with tempfile.TemporaryDirectory(prefix='zgx-keys-') as stage:
        binary = pathlib.Path(stage) / 'key_routing_probe'
        shutil.copy2(pathlib.Path(args.target_dir) / 'debug/examples/key_routing_probe', binary)
        clean = {key: os.environ[key] for key in ('HOME', 'TMPDIR', 'USER') if key in os.environ}
        clean['PATH'] = '/usr/bin:/bin'
        if args.ime:
            return ime(binary, clean)
        out = subprocess.run([str(binary)], cwd='/tmp', env=clean, capture_output=True, text=True,
                             timeout=60).stdout
        parts = ['zgx', 'web'] + (['terminal'] if args.ghostty else [])
        standard = {}
        for key in 'hmq':
            for part in parts:
                run = subprocess.run([str(binary), '--standard', key, '--focus', part], cwd='/tmp',
                                     env=clean, capture_output=True, text=True, timeout=60)
                standard[key, part] = (run.returncode, run.stdout)
    actions = set(re.findall(r'^ACTION (\S+) focus=(\S+)$', out, re.M))
    pages = set(re.findall(r'^PAGE (.+) focus=web$', out, re.M))
    checks = {
        'zgx: the unused menu key': ('unused', 'zgx') in actions,
        'zgx: the menu key a view uses': ('used', 'zgx') in actions,
        'zgx: the binding in no menu': ('binding', 'zgx') in actions,
        'page: heard all three': {'u meta', 'k meta', 'L meta shift'} <= pages,
        'page: an unused menu key falls through': ('unused', 'web') in actions,
        'page: a binding in no menu falls through': ('binding', 'web') in actions,
        'page: the key it took stays its': ('used', 'web') not in actions,
        'done': 'KEY_ROUTING_DONE' in out,
    }
    if args.ghostty:
        checks.update({
            'terminal: an unused menu key falls through': ('unused', 'terminal') in actions,
            'terminal: a binding in no menu reaches the app': ('binding', 'terminal') in actions,
            "terminal: Ghostty's binding stays its": ('used', 'terminal') not in actions,
        })
    for (key, part), (code, text) in standard.items():
        name = {'h': 'hides the app', 'm': 'minimizes the window', 'q': 'quits'}[key]
        if key == 'q':
            ok = code == 0 and 'STANDARD_DONE' not in text
        else:
            ok = code == 0 and ('HIDDEN true' if key == 'h' else 'MINIMIZED true') in text
        checks[f'{part}: ⌘{key.upper()} {name}'] = ok
    print('\n'.join(line for line in out.splitlines() if re.match(r'(ACTION|PAGE|FIRST|NO_KEY) ', line)))
    for name, ok in checks.items():
        print(('ok  ' if ok else 'FAIL') + ' ' + name)
    return 0 if all(checks.values()) else 1


def ime(binary, clean):
    out = subprocess.run([str(binary), '--ime', 'ja'], cwd='/tmp', env=clean, capture_output=True,
                         text=True, timeout=60).stdout
    if 'IME_SKIP' in out:
        print('SKIP: no Japanese input method is enabled (none is enabled by this probe)')
        return 0
    pages = re.findall(r'^PAGE (.+) focus=web$', out, re.M)
    kana = re.compile('[\u3040-\u30ff]')
    checks = {
        'composition started': 'composition start' in pages,
        'committed Japanese text': any(p.startswith('composition end ') and kana.search(p) for p in pages),
        "the field's value is Japanese": any(p.startswith('value ') and kana.search(p) for p in pages),
        'done': 'IME_DONE' in out,
    }
    print('\n'.join(line for line in out.splitlines() if re.match(r'(ACTION|PAGE|FIRST) ', line)))
    print('the ⌘ key mid-composition: ' + ('reached the app menu' if 'ACTION unused focus=web' in out
                                          else 'did not reach the app menu'))
    for name, ok in checks.items():
        print(('ok  ' if ok else 'FAIL') + ' ' + name)
    return 0 if all(checks.values()) else 1


if __name__ == '__main__':
    sys.exit(main())
