#!/usr/bin/env python3
"""Mac-only validation bundle. Automated GPU/host checks plus explicit manual UI checks.

Run on a logged-in macOS desktop. --interactive additionally launches native
menu/dialog and accessibility fixtures for operator verification. Omitting it
leaves those checks pending, never reported as passed.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import platform
import re
import subprocess
import sys
import tarfile
from datetime import datetime, timezone


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--target-dir', type=Path, default=Path('target'))
    parser.add_argument('--interactive', action='store_true')
    args = parser.parse_args()
    if platform.system() != 'Darwin':
        parser.error('This bundle requires an actual macOS host; cross-compilation is not runtime validation.')
    root = Path(__file__).resolve().parents[1]
    args.output = args.output.resolve()
    args.target_dir = args.target_dir.resolve()
    args.output.mkdir(parents=True, exist_ok=False)
    env = dict(os.environ, CARGO_TARGET_DIR=str(args.target_dir))
    results = {'status': 'running', 'platform': platform.platform(), 'automated': [], 'manual': [], 'binaries': {},
               'started_utc': datetime.now(timezone.utc).isoformat()}
    def run(name, command, timeout=600):
        try:
            with (args.output / (name + '.log')).open('w') as log:
                process = subprocess.run(command, cwd=root, env=env, stdout=log, stderr=subprocess.STDOUT, timeout=timeout)
        except subprocess.TimeoutExpired:
            results['automated'].append({'name': name, 'command': list(map(str, command)), 'exit_code': None, 'error': 'timeout', 'timeout_seconds': timeout})
            raise
        results['automated'].append({'name': name, 'command': list(map(str, command)), 'exit_code': process.returncode})
        if process.returncode:
            raise RuntimeError(f'{name} failed; inspect its retained log')
        return (args.output / (name + '.log')).read_text()
    checks = [
        ('platform_services', 'Open file, choose folder, save and cancel; native prompt answer; close an owner while its picker is open. Verify the selected path/result in the log.'),
        ('dynamic_menus', 'Use Cmd+B, Cmd+S, Cmd+R and Cmd+D as shown. Verify native menu checked/disabled state, command routing and menu replacement.'),
        ('atspi', 'Enable VoiceOver or Accessibility Inspector. Read static labels/editor text, change selection/value/checked state through accessibility, and verify disabled actions are rejected. This is the macOS AX check despite the fixture historical name.'),
        ('rich_text', 'Use Tab and Enter on the inline link; verify VoiceOver exposes a named clickable link and full unclipped paragraph text.'),
        ('form', 'Check Unicode typing, selection, multiline Enter, Tab focus, Cmd+Z/Shift+Cmd+Z, and Cmd+C/X/V between Name/Notes and TextEdit in both directions. Toggle editing must block edits; resizing must preserve text. Close the window to retain final model logs.'),
        ('native_ime', 'This fixture closes after 20 seconds. Enable macOS Pinyin before launching. Compose nihao, inspect marked text and candidate placement, select 你好, and verify exactly one COMMIT and the final model. Capture the candidate panel near the caret.'),
        ('native_ime', 'Separate 20-second launch: compose with macOS Pinyin then Escape. Verify cancellation leaves the committed model unchanged and clears marked text. Preserve PREEDIT/COMMIT/FINAL logs.'),
        ('native_ime', 'Separate 20-second launch: switch from first to second editor or another application during composition. Verify no stale commit reaches the wrong editor; move the window and check candidate placement.'),
        ('layout', 'Resize width from 800 to 600 to 1000. Check grid spans, sidebar/main/detail placement, wrapping chips and scroll reachability through Item 29. Capture layout at Retina scale; record any unavailable display-scale checks as SKIP.'),
        ('paint_styles', 'Check asymmetric corners/edges, two colored shadows, patterned fill and dashed border. Click Change gradient twice: colors must change and restore without stale pixels or displaced children.'),
        ('canvas', 'Resize the window, then click Change curve twice. The curve must change and restore, with intact gradient and slash-pattern backgrounds and no old stroke footprints.'),
        ('svg_transform', 'Click Rotate 30 degrees twelve times and Toggle scale twice. Check scaling/rotation, cleared old footprints and return to original geometry, including Retina rendering.'),
        ('animated_image', 'The default fixture uses generated red/green/blue frames, not GIF decoding. Pause for more than one second, resume, minimize/uncover and verify playback and pixels. Record actual observations; decoded GIF correctness is covered separately by renderer tests.'),
        ('window_controls', 'Observe resize, maximize and restore through controls. Separately observe minimize/restore, hide/show and focus during --smoke-test. Its log proves resize acknowledgements but only requests the other states; record actual window behavior.'),
        ('native_platform', 'Check initial bounds/display and minimum size, custom-titlebar dragging, fullscreen and restoration (the --smoke launch requests these). Close the root then use native application reopen and registered open-URL delivery. Preserve APPLICATION logs. URL scheme registration is application packaging; unsupported setup stays pending.'),
        ('drag_drop', 'Drag each colored card onto target; verify preview, accepted payload, outside-drop and Escape cancellation. From Finder drop two files and verify complete grouped paths, then cancel and leave the region; hover must clear.'),
        ('actions', 'Focus the editor: F5 and F1 then F2 must save; x then y must save without insertion, while x followed by timeout must replay x. Check Escape cancellation and F1/F5/F12/Insert with ZGX_ACTION_EVENTS=1; use ZGX_ACTION_TRACE=1 for model logs. Multiple-context precedence and application fallback remain separate behavior tests.'),
        ('cursors', 'Move over cursor regions and disabled regions, then exit. Observe the native pointer shape and restoration.'),
    ]
    for index, (example, instruction) in enumerate(checks):
        results["manual"].append({"fixture": example, "instruction": instruction, "status": "pending",
                                  "log": f"manual-{index:02d}-{example}.log"})
    try:
        # Preserve the actual working tree, including local fixes. Hardware output
        # deliberately excludes serial numbers, UUIDs, network and account details.
        results['git_head'] = run('git-head', ['git', 'rev-parse', 'HEAD']).strip()
        results['git_status'] = run('git-status', ['git', 'status', '--porcelain=v1'])
        run('source-diff', ['git', 'diff', '--binary', 'HEAD'])
        results['source_diff_sha256'] = hashlib.sha256((args.output / 'source-diff.log').read_bytes()).hexdigest()
        paths = subprocess.check_output(['git', 'ls-files', '-z', '--cached', '--others', '--exclude-standard'], cwd=root).split(b'\0')
        source_hashes = {}
        with tarfile.open(args.output / 'source.tar.gz', 'w:gz') as archive:
            for raw in sorted(set(paths)):
                if not raw:
                    continue
                relative = Path(os.fsdecode(raw))
                # Historical evidence is immutable and can dwarf the code archive.
                if (relative.parts[0] not in ('crates', 'scripts', '.github', 'assets', 'comparisons')
                        and not (relative.parts[0] == 'docs' and len(relative.parts) == 2)
                        and len(relative.parts) != 1):
                    continue
                path = root / relative
                if path.is_file() and not path.is_symlink():
                    source_hashes[str(relative)] = hashlib.sha256(path.read_bytes()).hexdigest()
                    archive.add(path, arcname=str(relative), recursive=False)
        (args.output / 'source-files.json').write_text(json.dumps(source_hashes, indent=2) + '\n')
        results['source_archive_sha256'] = hashlib.sha256((args.output / 'source.tar.gz').read_bytes()).hexdigest()
        run('rust-toolchain', ['rustc', '-vV'])
        run('os-version', ['sw_vers'])
        run('hardware', ['sysctl', 'hw.model', 'hw.memsize', 'hw.ncpu', 'machdep.cpu.brand_string'])
        display_data = json.loads(subprocess.check_output(['system_profiler', 'SPDisplaysDataType', '-json'], timeout=60))
        allowed = ('sppci_model', 'spdisplays_vendor', 'spdisplays_metal', 'spdisplays_metal_family',
                   'spdisplays_resolution', 'spdisplays_pixels', 'spdisplays_retina', 'spdisplays_main', 'spdisplays_online')
        def display_summary(value):
            if isinstance(value, list):
                return [display_summary(item) for item in value]
            if isinstance(value, dict):
                return {key: display_summary(item) for key, item in value.items()
                        if key in allowed or isinstance(item, (dict, list))}
            return value
        results['displays'] = display_summary(display_data)
        run('metal-rendering-tests', ['cargo', 'test', '-p', 'zgx-gpu', '--locked', '--', '--test-threads=1'], 1800)
        run('accessibility-projection-tests', ['cargo', 'test', '-p', 'zgx-desktop', '--lib', 'accessibility::', '--locked'])
        # What VoiceOver reads of the components and charts, from live windows.
        run('accessibility-tree', [sys.executable, str(root / 'scripts/macos_accessibility_probe.py'),
            '--target-dir', str(args.target_dir)], 600)
        examples = ['native_surfaces', 'rich_text', 'platform_services', 'dynamic_menus', 'atspi',
                    'form', 'native_ime', 'layout', 'paint_styles', 'canvas', 'svg_transform', 'animated_image',
                    'component_workload', 'windows', 'window_controls', 'native_platform', 'drag_drop', 'actions', 'cursors',
                    'transparency']
        command = ['cargo', 'build', '-p', 'zgx-desktop', '--locked']
        for example in examples:
            command += ['--example', example]
        run('build-fixtures', command)
        directory = args.target_dir / 'debug' / 'examples'
        for name in examples:
            results['binaries'][name] = hashlib.sha256((directory / name).read_bytes()).hexdigest()
        run('component-host-lifecycle', [sys.executable, str(root / 'scripts/component_host_smoke.py'),
            str(directory / 'component_workload'), '--output', str(args.output / 'component-host')], 100)
        # Closing a window ran as it should and caught nothing.
        log = run('multi-window-lifecycle', [str(directory / 'windows'), '--smoke-test'], 30)
        assert 'multi-window smoke passed' in log and 'contained a panic' not in log, log
        log = run('window-controls', [str(directory / 'window_controls'), '--smoke-test'], 30)
        assert 'window controls smoke passed' in log and 'contained a panic' not in log, log
        log = run('native-surfaces-window', [str(directory / 'native_surfaces'), '--smoke-test'], 30)
        assert re.findall(r'NATIVE_SURFACE frame=(\d+) nv12=(true|false)', log) == [('1', 'false'), ('2', 'true'), ('3', 'false'), ('4', 'true')], log
        # What the window server composites of a transparent window, read in
        # the process (no screen capture): flags, layers and the shown frame.
        log = run('transparent-window-composition', [str(directory / 'transparency'), '--composition'], 60)
        assert re.findall(r'^STAGE (\w+)', log, re.M) == ['initial', 'blue', 'hidden', 'restored', 'resized'], log
        assert re.search(r'^PASS$', log, re.M) and 'FAIL' not in log, log
        log = run('rich-text-window', [str(directory / 'rich_text'), '--smoke-test'], 30)
        assert 'RICH_CLAMP height=56' in log, log
        checklist = []
        for index, (example, instruction) in enumerate(checks):
            entry = results['manual'][index]
            checklist.append(f'- `{directory / example}`: {instruction}')
            if args.interactive:
                with (args.output / entry['log']).open('w') as log:
                    process = subprocess.Popen([str(directory / example)], cwd=root, env=env, stdout=log, stderr=subprocess.STDOUT)
                    try:
                        print(instruction)
                        response = input('Record observed PASS, FAIL, or SKIP, followed by notes: ').strip()
                        verdict = response.split(maxsplit=1)[0].upper() if response else ''
                        if verdict not in ('PASS', 'FAIL', 'SKIP'):
                            verdict = 'UNRECOGNIZED'
                        entry.update(status='operator-report', verdict=verdict, report=response)
                    finally:
                        if process.poll() is None:
                            process.terminate()
                        try:
                            process.wait(timeout=10)
                        except subprocess.TimeoutExpired:
                            process.kill()
                            process.wait()
        (args.output / 'manual-validation.md').write_text('# Native Mac operator checks\n\nAutomation/VoiceOver permissions may be required. These are pending unless operator reports are recorded.\n\n' + '\n'.join(checklist) + '\n')
        results['status'] = 'automated-subset-passed'
        verdicts = [entry.get('verdict') for entry in results['manual']]
        results['manual_qualification'] = ('operator-reported-failure' if 'FAIL' in verdicts else
            'operator-reported-pass' if verdicts and all(v == 'PASS' for v in verdicts) else 'pending')
        if 'FAIL' in verdicts:
            raise RuntimeError('An operator reported a failed native check; inspect manual results and logs.')
    except BaseException as error:
        results['status'] = 'failed'
        results['error'] = {'type': type(error).__name__, 'message': str(error)}
        raise
    finally:
        results['finished_utc'] = datetime.now(timezone.utc).isoformat()
        (args.output / 'results.json').write_text(json.dumps(results, indent=2) + '\n')
    print('Automated Mac subset passed. Inspect results.json for manual checks; pending checks are not parity proof.')


if __name__ == '__main__':
    main()
