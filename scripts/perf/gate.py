#!/usr/bin/env python3
"""Exact instruction counts of the CPU-only headless scenes, base against head.

    python3 scripts/perf/gate.py BASE [--threshold 1.0] [--markdown out.md]

Builds this checkout and BASE (a revision, checked out in a worktree under
/tmp/zgui-perf/gate-base) into one target directory, runs each scene under
valgrind's cachegrind without cache simulation, and compares the counted
instructions (`I refs`): exact, and the same on any machine of the
architecture, where hardware counters and time are not. Prints a table (and
writes it as Markdown); exits 1 when a scene costs more than THRESHOLD
percent over BASE (a scene the base does not have yet shows as new). A run
that fails (a script's time budget, a missing GPU) is reported as the
gate's failure, never as a result; benches run with ZGUI_GATE=1, which
the script benches take for budgets long enough for cachegrind. Linux
(CI's `perf` job). Nothing renders: under valgrind a software rasterizer's
instructions would swamp zgui's, so heavy, lab and animate run with
RENDER=0 (update, layout and damage, text measured as a renderer would).
"""
import argparse
import os
import pathlib
import re
import shutil
import subprocess
import sys

ROOT = pathlib.Path(__file__).resolve().parents[2]
WORK = pathlib.Path(os.environ.get('ZGUI_PERF_DIR', '/tmp/zgui-perf'))

# name, package, example, environment: sized to a few billion instructions.
# The windowed scenes' headless twins run with RENDER=0: update, layout and
# damage, with text measured as a renderer would, and nothing drawn.
SCENES = [
    ('heavy-both', 'zgui-desktop', 'heavy_headless', {'ZGUI_MODE': 'both', 'TICKS': '300', 'RENDER': '0'}),
    ('heavy-stream', 'zgui-desktop', 'heavy_headless', {'ZGUI_MODE': 'stream', 'TICKS': '300', 'RENDER': '0'}),
    ('heavy-scroll', 'zgui-desktop', 'heavy_headless', {'ZGUI_MODE': 'scroll', 'TICKS': '300', 'RENDER': '0'}),
    ('lab-stream', 'zgui-desktop', 'workload_headless', {'ZGUI_MODE': 'stream', 'RENDER': '0'}),
    ('lab-scroll', 'zgui-desktop', 'workload_headless', {'ZGUI_MODE': 'scroll', 'RENDER': '0'}),
    ('animate', 'zgui-desktop', 'animation_headless', {'FRAMES': '1200', 'RENDER': '0'}),
    ('mount', 'zgui-script', 'mount_bench', {}),
    ('script-update', 'zgui-script', 'update_bench', {'TICKS': '200'}),
    ('anime', 'zgui-script', 'anime_bench', {}),
    ('script-layout', 'zgui-script', 'script_layout_bench', {}),
    ('list-state', 'zgui-script', 'list_state_bench', {}),
    ('style', 'zgui', 'style_bench', {}),
    ('widgets', 'zgui', 'widget_bench', {}),
]


def has_example(checkout, package, example):
    """Whether `checkout` has the example (a scene new since the base has not)."""
    crate = checkout / 'crates' / package
    manifest = (crate / 'Cargo.toml').read_text() if (crate / 'Cargo.toml').exists() else ''
    return (crate / 'examples' / f'{example}.rs').exists() or f'name = "{example}"' in manifest


def build(checkout, target, into):
    scenes = [s for s in SCENES if has_example(checkout, s[1], s[2])]
    command = ['cargo', 'build', '--release', '--locked', '--quiet']
    for package in dict.fromkeys(s[1] for s in scenes):
        command += ['-p', package]
        for example in dict.fromkeys(s[2] for s in scenes if s[1] == package):
            command += ['--example', example]
    env = dict(os.environ, CARGO_TARGET_DIR=str(target), CARGO_INCREMENTAL='0',
               CARGO_PROFILE_RELEASE_DEBUG='0')
    subprocess.run(command, cwd=checkout, env=env, check=True)
    # One target directory serves both sides (their dependencies are
    # shared): keep this side's binaries before the other overwrites them.
    into.mkdir(parents=True, exist_ok=True)
    for _, _, example, _ in scenes:
        shutil.copy2(target / 'release' / 'examples' / example, into / example)


class Infra(Exception):
    """The gate could not measure: not a result about the change."""


def count(binary, env):
    """The run's instructions. A run that fails is the gate's failure, not
    the change's: under cachegrind a script's time budget (`ZGUI_GATE`
    lifts it in the benches) or a missing GPU would otherwise look like a
    result."""
    result = subprocess.run(
        ['valgrind', '--tool=cachegrind', '--cache-sim=no', '--cachegrind-out-file=/dev/null', str(binary)],
        env=dict(os.environ, ZGUI_GATE='1', **env), capture_output=True, text=True)
    found = re.search(r'I\s+refs:\s+([\d,]+)', result.stderr)
    if result.returncode or not found:
        why = 'a script hit its time budget' if 'time budget' in result.stdout + result.stderr else 'it failed'
        raise Infra(f'{binary.name}: {why} under cachegrind\n{result.stdout[-1500:]}{result.stderr[-3000:]}')
    return int(found.group(1).replace(',', ''))


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument('base')
    parser.add_argument('--threshold', type=float, default=1.0, help='percent a scene may grow')
    parser.add_argument('--markdown', type=pathlib.Path)
    args = parser.parse_args()
    # Both sides build into one target directory: CI's (ZGUI_GATE_TARGET)
    # sits in the workspace's, where its cache keeps the dependencies.
    target = pathlib.Path(os.environ.get('ZGUI_GATE_TARGET', WORK / 'gate-target')).resolve()
    base = WORK / 'gate-base'
    if base.exists():
        subprocess.run(['git', 'worktree', 'remove', '--force', str(base)], cwd=ROOT)
    subprocess.run(['git', 'worktree', 'add', '--detach', str(base), args.base], cwd=ROOT, check=True,
                   capture_output=True)
    build(base, target, WORK / 'gate-bin-base')
    # Cargo names a path crate's artifacts the same from either checkout,
    # and judges them fresh by modification time: this checkout's files,
    # older than what the base just built, would be taken for built. Newer
    # now, they are compiled again (the registry's crates stay shared).
    tracked = subprocess.run(['git', 'ls-files', '-z'], cwd=ROOT, capture_output=True, check=True).stdout
    for path in filter(None, tracked.split(b'\0')):
        file = ROOT / path.decode()
        if file.is_file():
            os.utime(file)
    build(ROOT, target, WORK / 'gate-bin-head')
    rows, failed = [], []
    for name, _, example, env in SCENES:
        base_binary = WORK / 'gate-bin-base' / example
        try:
            before = count(base_binary, env) if base_binary.exists() else None
            after = count(WORK / 'gate-bin-head' / example, env)
        except Infra as error:
            # Loudly, and as what it is: no table that reads as a result.
            if args.markdown:
                args.markdown.write_text(f'**Instruction gate: infrastructure failure, no result.**\n\n```\n{error}\n```\n')
            sys.exit(f'GATE INFRA FAILURE (not a performance result): {error}')
        delta = (after - before) / before * 100 if before else None
        if delta is not None and delta > args.threshold:
            failed.append(name)
        rows.append((name, before, after, delta))
        shown = f'{before / 1e6:10.1f} M {after / 1e6:10.1f} M {delta:+7.2f}%' if before else f'{"new":>12} {after / 1e6:10.1f} M'
        print(f'{name:14} {shown}', flush=True)
    sha = subprocess.run(['git', 'rev-parse', '--short', args.base], cwd=ROOT, capture_output=True,
                         text=True).stdout.strip()
    table = [f'**Instruction counts** (cachegrind, exact) against `{sha}`; the gate fails above '
             f'+{args.threshold:g}%.', '', '| scene | base | head | change |', '| --- | ---: | ---: | ---: |']
    for name, before, after, delta in rows:
        flag = ' ⚠️' if name in failed else ''
        if before is None:
            table.append(f'| {name} | (new) | {after / 1e6:,.1f} M | |')
        else:
            table.append(f'| {name} | {before / 1e6:,.1f} M | {after / 1e6:,.1f} M | {delta:+.2f}%{flag} |')
    if args.markdown:
        args.markdown.write_text('\n'.join(table) + '\n')
    if failed:
        sys.exit(f'more than +{args.threshold:g}% instructions: {", ".join(failed)}')


if __name__ == '__main__':
    main()
