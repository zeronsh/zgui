#!/usr/bin/env python3
"""What VoiceOver reads of zgx's components and charts, on real macOS windows.

    python3 scripts/macos_accessibility_probe.py [--target-dir DIR]

Builds zgx-run, opens the component catalog and the dashboard example, and
reads each window's accessibility tree through the AX API
(scripts/macos_ax_tree.swift): attributes only, no input sent, so it is safe
on a desktop in use. Checks that grids give VoiceOver their rows (its table
navigation), that charts drawn on canvases are images named by what they
show, that a group of chips is a radio group saying which is chosen, and
that a list's options are named, selectable and pressable. Needs this
terminal to be allowed to control the computer (System Settings, Privacy &
Security, Accessibility); exits 2 if it is not. macOS only.
"""
import argparse
import json
import os
import pathlib
import platform
import subprocess
import sys
import tempfile
import time

ROOT = pathlib.Path(__file__).resolve().parents[1]
EXAMPLES = ROOT / 'crates/zgx-script/examples'


def walk(node):
    yield node
    for child in node.get('children', []):
        yield from walk(child)


def named(tree, role, prefix):
    return [n for n in walk(tree) if n.get('role') == role and n.get('title', '').startswith(prefix)]


def read_tree(reader, app, ready, timeout=30):
    """Launch `app` (zgx-run's argument) and read its tree once `ready(tree)`."""
    # A clean environment: an agent's or terminal's variables stay out of the app.
    env = {'HOME': os.environ['HOME'], 'PATH': os.environ['PATH']}
    process = subprocess.Popen([str(zgx_run), str(app)], env=env, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    try:
        deadline = time.monotonic() + timeout
        tree = {}
        while time.monotonic() < deadline:
            time.sleep(0.5)
            result = subprocess.run([str(reader), str(process.pid)], capture_output=True, text=True)
            if result.returncode == 3:
                sys.exit('not allowed to read accessibility: allow this terminal in System Settings, '
                         'Privacy & Security, Accessibility')
            if result.returncode == 0 and result.stdout:
                tree = json.loads(result.stdout)
                # The tree is made when something first asks for it: read again once whole.
                if ready(tree):
                    return tree
        raise SystemExit(f'{app.name}: its tree did not come up in {timeout} s')
    finally:
        process.terminate()
        process.wait(timeout=10)


def main():
    global zgx_run
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument('--target-dir', type=pathlib.Path, default=ROOT / 'target')
    args = parser.parse_args()
    if platform.system() != 'Darwin':
        sys.exit('macOS only')
    env = dict(os.environ, CARGO_TARGET_DIR=str(args.target_dir))
    subprocess.run(['cargo', 'build', '-p', 'zgx-run', '--locked'], cwd=ROOT, env=env, check=True)
    zgx_run = args.target_dir.resolve() / 'debug/zgx-run'
    work = pathlib.Path(tempfile.mkdtemp(prefix='zgx-ax-'))
    reader = work / 'macos_ax_tree'
    subprocess.run(['swiftc', '-O', str(ROOT / 'scripts/macos_ax_tree.swift'), '-o', str(reader)], check=True)

    failures = []

    def check(ok, what):
        print(('PASS ' if ok else 'FAIL ') + what)
        if not ok:
            failures.append(what)

    catalog = read_tree(reader, EXAMPLES / 'catalog', lambda t: named(t, 'AXTable', 'Orders'))
    orders = named(catalog, 'AXTable', 'Orders')[0]
    check(orders.get('rows', 0) >= 2, f"the data grid gives its rows (AXRows {orders.get('rows', 0)})")
    days = named(catalog, 'AXTable', 'Day')
    check(bool(days) and days[0].get('rows', 0) >= 5, "the date picker's month gives its weeks as rows")
    files = named(catalog, 'AXOutline', 'Files')
    check(bool(files) and files[0].get('rows', 0) >= 1, 'the tree gives its rows')
    for role, name in [('AXRadioGroup', 'Range'), ('AXIncrementor', 'Copies'), ('AXSlider', 'Volume'),
                       ('AXTabGroup', 'Documents'), ('AXPopUpButton', 'Fruit')]:
        check(bool(named(catalog, role, name)), f'{name}: a {role}')

    dashboard = read_tree(reader, EXAMPLES / 'dashboard', lambda t: named(t, 'AXList', 'Instruments'))
    check(bool(named(dashboard, 'AXImage', 'Price, last ')), "the price chart is an image named by what it shows")
    check(bool(named(dashboard, 'AXImage', 'Volume by sector: ')), 'the volume chart is an image named by its bars')
    sector = named(dashboard, 'AXRadioGroup', 'Sector')
    chosen = [n for n in walk(sector[0]) if n.get('role') == 'AXRadioButton' and n.get('value') == '1'] if sector else []
    check(len(chosen) == 1 and chosen[0].get('title') == 'All', 'the sector chips say which is chosen')
    instruments = named(dashboard, 'AXList', 'Instruments')[0]
    options = [n for n in instruments.get('children', []) if 'trend ' in n.get('title', '')]
    check(instruments.get('rows', 0) >= 5 and bool(options), "the instruments are rows named with their sparkline's trend")
    check(bool(options) and 'selected' in options[0] and 'AXPress' in options[0].get('actions', []),
          'an instrument says whether it is selected and can be pressed')

    if failures:
        sys.exit(f'{len(failures)} failed')
    print('macOS accessibility: all passed')


if __name__ == '__main__':
    main()
