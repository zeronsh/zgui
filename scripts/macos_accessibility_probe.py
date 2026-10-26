#!/usr/bin/env python3
"""What VoiceOver reads of zgx's components and charts, on real macOS windows.

    python3 scripts/macos_accessibility_probe.py [--target-dir DIR]

Builds zgx-run, opens the component catalog and the dashboard example, and
reads each window's accessibility tree through the AX API
(scripts/macos_ax_tree.swift): attributes only, no input sent, so it is safe
on a desktop in use. Checks that the menu bar (made after the first
frame) has its standard menus and lists the open window, that grids give
VoiceOver their rows (its table navigation), that charts drawn on canvases are images named by what they
show, that a group of chips is a radio group saying which is chosen, and
that a list's options are named, selectable and pressable, and that an
invalid form field says so and why (scripts/macos_ax_form), and that a
text area's text, selection and caret read and show where its characters
are (scripts/macos_ax_text; its selection is set through accessibility, as
VoiceOver sets it). Needs this
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
    # The menu bar, made once the first frame shows: the application menu's
    # standard items, and the window (open before it) in the Window menu.
    bar = [n for n in walk(catalog) if n.get('role') == 'AXMenuBar']
    items = [n.get('title') for n in walk(bar[0]) if n.get('role') == 'AXMenuItem'] if bar else []
    check('Services' in items and 'Minimize' in items and any(i and i.startswith('Quit') for i in items),
          f'the menu bar has the application and Window menus ({len(items)} items)')
    windows = [n.get('title') for n in catalog.get('children', []) if n.get('role') == 'AXWindow']
    check(bool(windows) and windows[0] in items, f'the Window menu lists the window ({windows[:1]})')

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

    # A form field wrong: VoiceOver hears it invalid (AXInvalid), and why
    # after its description (AXHelp), from the text showing it or, with none,
    # from the field's own error.
    form = read_tree(reader, ROOT / 'scripts/macos_ax_form', lambda t: named(t, 'AXTextField', 'Email'))
    email = named(form, 'AXTextField', 'Email')[0]
    check(email.get('invalid') is True, 'an invalid field says so (AXInvalid)')
    check('That is not an email address' in email.get('help', ''),
          f"its error message is read with it (AXHelp {email.get('help')!r})")
    name = named(form, 'AXTextField', 'Name')
    check(bool(name) and name[0].get('invalid') is True and 'A name is required' in name[0].get('help', ''),
          'a field whose error shows nowhere says it too')

    # A text area as VoiceOver reads and moves through it: its text, a
    # selection set through AX (as VoiceOver sets one) read back, and where
    # its characters and a caret on the second line show on screen
    # (AXBoundsForRange), inside the field (scripts/macos_ax_text).
    texter = work / 'macos_ax_text'
    subprocess.run(['swiftc', '-O', str(ROOT / 'scripts/macos_ax_text.swift'), '-o', str(texter)], check=True)
    env = {'HOME': os.environ['HOME'], 'PATH': os.environ['PATH']}
    process = subprocess.Popen([str(zgx_run), str(ROOT / 'scripts/macos_ax_text')], env=env,
                               stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    try:
        time.sleep(1.5)
        result = subprocess.run([str(texter), str(process.pid), 'Note'], capture_output=True, text=True, timeout=60)
    finally:
        process.terminate()
        process.wait(timeout=10)
    if result.returncode == 3:
        sys.exit('not allowed to read accessibility: allow this terminal in System Settings, '
                 'Privacy & Security, Accessibility')
    text = json.loads(result.stdout) if result.returncode == 0 and result.stdout else {}
    check(text.get('value') == 'one two\nthree four', f"the note's text reads ({text.get('value')!r})")
    check(text.get('selected_range') == [4, 3] and text.get('selected_text') == 'two',
          f"a selection set as VoiceOver sets one takes ({text.get('selected_range')}, {text.get('selected_text')!r})")
    frame, shown = text.get('frame'), text.get('selection_bounds')

    def inside(box):
        return bool(box and frame) and box[2] > 0 and box[3] > 0 and frame[0] - 1 <= box[0] \
            and box[0] + box[2] <= frame[0] + frame[2] + 1 and frame[1] - 1 <= box[1] \
            and box[1] + box[3] <= frame[1] + frame[3] + 1

    check(inside(shown), f'the selection shows where its characters are (AXBoundsForRange {shown} in {frame})')
    check(text.get('caret_range') == [8, 0], f"the caret moves to the second line ({text.get('caret_range')})")
    first, second = text.get('first_line_bounds'), text.get('second_line_bounds')
    check(inside(first) and inside(second) and second[1] > first[1],
          f'the second line shows below the first ({first}, {second})')

    if failures:
        sys.exit(f'{len(failures)} failed')
    print('macOS accessibility: all passed')


if __name__ == '__main__':
    main()
