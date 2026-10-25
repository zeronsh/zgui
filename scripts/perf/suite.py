#!/usr/bin/env python3
"""Every headless scene's retired instructions, and A/B comparisons of revisions.

    python3 scripts/perf/suite.py                      # build this checkout, run every scene
    python3 scripts/perf/suite.py --only heavy,mount   # scenes whose names contain these
    python3 scripts/perf/suite.py --all                # the slow scenes too
    python3 scripts/perf/suite.py ab origin/main HEAD  # two revisions, interleaved
    python3 scripts/perf/suite.py ab origin/main .     # main against this checkout's files
    python3 scripts/perf/suite.py dirs /tmp/a /tmp/b   # two sets of built examples (build flags)
    python3 scripts/perf/suite.py clean                # remove the A/B worktrees and builds

A scene is a headless example run with a fixed workload: its whole run's
retired instructions (the kernel's count, every thread) vary by well under
0.5% between runs, where its time varies by several percent. Disturbances
only add instructions, so the least of a scene's runs is the one reported. A/B mode builds
each revision in a worktree of its own under /tmp/zgx-perf (kept, so the
next comparison builds incrementally), runs the scenes alternately, and marks
a difference larger than both sides' spread. Each side builds into a target
of its own, and one last built from another checkout is claimed first
(`claim`): cargo would otherwise hand back the other checkout's crates as
built. A scene whose binary is the same on both sides is noted, and a local
branch behind its remote (a checkout's own `main`) is flagged in the table:
compare against `origin/main`. With ZGX_PROFILE=1 set, a
scene's own phase report (style, layout, text, damage, paint, encode,
submit) is shown too. macOS (`/usr/bin/time -l`) and Linux (`perf stat`).
"""
import argparse
import filecmp
import hashlib
import os
import pathlib
import platform
import re
import shutil
import statistics
import subprocess
import sys
import time

ROOT = pathlib.Path(__file__).resolve().parents[2]
WORK = pathlib.Path(os.environ.get('ZGX_PERF_DIR', '/tmp/zgx-perf'))

# name, package, example, environment (and 'slow': only with --all).
SCENES = [
    ('heavy-both', 'zgx-desktop', 'heavy_headless', {'ZGX_MODE': 'both', 'TICKS': '600'}),
    ('heavy-stream', 'zgx-desktop', 'heavy_headless', {'ZGX_MODE': 'stream', 'TICKS': '600'}),
    ('heavy-scroll', 'zgx-desktop', 'heavy_headless', {'ZGX_MODE': 'scroll', 'TICKS': '600'}),
    ('lab-stream', 'zgx-desktop', 'workload_headless', {'ZGX_MODE': 'stream'}),
    ('lab-scroll', 'zgx-desktop', 'workload_headless', {'ZGX_MODE': 'scroll'}),
    ('animate', 'zgx-desktop', 'animation_headless', {'FRAMES': '1200'}),
    ('mount', 'zgx-script', 'mount_bench', {}),
    ('script-update', 'zgx-script', 'update_bench', {}),
    ('anime', 'zgx-script', 'anime_bench', {}),
    ('script-layout', 'zgx-script', 'script_layout_bench', {}),
    ('list-state', 'zgx-script', 'list_state_bench', {}),
    ('state', 'zgx-script', 'state_bench', {}, 'slow'),
    ('layout', 'zgx-gpu', 'layout_bench', {}),
    ('frame', 'zgx-gpu', 'frame_bench', {}),
    ('text', 'zgx-gpu', 'text_bench', {}),
    ('core', 'zgx', 'core_bench', {}, 'slow'),
    ('style', 'zgx', 'style_bench', {}),
    ('widgets', 'zgx', 'widget_bench', {}),
    ('app-mail', 'zgx-script', 'apps_bench', {'APP': 'mail'}),
    ('app-dashboard', 'zgx-script', 'apps_bench', {'APP': 'dashboard', 'REPS': '5'}),
    ('app-sketch', 'zgx-script', 'apps_bench', {'APP': 'sketch'}),
    ('app-code', 'zgx-script', 'apps_bench', {'APP': 'code'}),
]


def chosen(args):
    if args.only:
        words = args.only.split(',')
        scenes = [scene for scene in SCENES if any(word in scene[0] for word in words)]
    else:
        scenes = [scene for scene in SCENES if args.all or 'slow' not in scene[4:]]
    return [scene[:4] for scene in scenes]


def claim(checkout, target):
    """Make `target` build `checkout`'s workspace crates. Cargo names them the
    same from any checkout and judges them fresh by modification time, so a
    target directory last built from another checkout (another worktree, a
    side of an earlier A/B) would hand back that checkout's crates, older
    files here taken for built. Touched, they are compiled again (the
    registry's crates stay shared); the target remembers whose it is."""
    marker = target / '.zgx-checkout'
    if marker.exists() and marker.read_text() == str(checkout.resolve()):
        return
    tracked = subprocess.run(['git', 'ls-files', '-z'], cwd=checkout, capture_output=True, check=True).stdout
    for path in filter(None, tracked.split(b'\0')):
        file = checkout / path.decode()
        if file.is_file():
            os.utime(file)
    target.mkdir(parents=True, exist_ok=True)
    marker.write_text(str(checkout.resolve()))


def build(checkout, target, scenes):
    claim(checkout, target)
    command = ['cargo', 'build', '--release', '--locked', '--quiet']
    for package in dict.fromkeys(scene[1] for scene in scenes):
        command += ['-p', package]
        for example in dict.fromkeys(s[2] for s in scenes if s[1] == package):
            command += ['--example', example]
    env = dict(os.environ, CARGO_TARGET_DIR=str(target), CARGO_INCREMENTAL='0')
    started = time.monotonic()
    result = subprocess.run(command, cwd=checkout, env=env)
    if result.returncode:
        sys.exit(f'build failed in {checkout}')
    print(f'built {checkout} in {time.monotonic() - started:.0f} s', file=sys.stderr)
    return target / 'release' / 'examples'


def measure(binary, env):
    """(instructions, seconds, the scene's last lines) for one run."""
    env = dict(os.environ, **env)
    if platform.system() == 'Darwin':
        command = ['/usr/bin/time', '-l', str(binary)]
    else:
        command = ['perf', 'stat', '-x,', '-e', 'instructions:u', str(binary)]
    started = time.monotonic()
    result = subprocess.run(command, env=env, capture_output=True, text=True)
    seconds = time.monotonic() - started
    if result.returncode:
        sys.exit(f'{binary} failed:\n{result.stdout}{result.stderr}')
    if platform.system() == 'Darwin':
        found = re.search(r'(\d+)\s+instructions retired', result.stderr)
    else:
        found = re.search(r'^(\d+),[^,]*,instructions', result.stderr, re.M)
    if not found:
        sys.exit(f'no instruction count from {binary}:\n{result.stderr}')
    report = [line for line in (result.stdout + result.stderr).splitlines()
              if line.startswith('profile') or line.startswith('PROFILE')]
    return int(found.group(1)), seconds, report


def spread(values):
    """How far the runs reach above the least disturbed (the reported) one, in percent."""
    return (max(values) - min(values)) / min(values) * 100 if len(values) > 1 else 0.


def run(args):
    scenes = chosen(args)
    examples = build(ROOT, pathlib.Path(os.environ.get('CARGO_TARGET_DIR', ROOT / 'target')), scenes)
    print(f'{"scene":14} {"instructions":>14} {"spread":>7} {"time":>8}')
    for name, _, example, env in scenes:
        runs = [measure(examples / example, env) for _ in range(args.repeat)]
        counts = [r[0] for r in runs]
        print(f'{name:14} {min(counts) / 1e6:12.1f} M {spread(counts):6.2f}% '
              f'{statistics.median(r[1] for r in runs):7.2f}s')
        # The last full report: frames past the warm-up.
        for line in runs[-1][2][-1:]:
            print(f'    {line}')


def behind(revision):
    """How far the local branch `revision` is behind its remote's (`main`
    against `origin/main`): a checkout's own `main` is often the day it was
    made, and an A/B against it measures a stale baseline. 0 when it is not
    a branch with one, or is not behind."""
    git = lambda *a: subprocess.run(['git', *a], cwd=ROOT, capture_output=True, text=True).stdout.strip()
    if not git('rev-parse', '--verify', '--quiet', f'refs/heads/{revision}'):
        return 0, ''
    remote = git('for-each-ref', '--format=%(upstream:short)', f'refs/heads/{revision}') or (
        f'origin/{revision}' if git('rev-parse', '--verify', '--quiet', f'refs/remotes/origin/{revision}') else '')
    if not remote:
        return 0, ''
    count = git('rev-list', '--count', f'{revision}..{remote}')
    return int(count or 0), remote


def worktree(slot, revision):
    """A checkout of `revision` in `slot`, made once and moved after."""
    lag, remote = behind(revision)
    if lag:
        print(f'warning: {revision} is {lag} commits behind {remote} (as last fetched): '
              f'pass {remote} to compare against it', file=sys.stderr)
    path = WORK / slot
    sha = subprocess.run(['git', 'rev-parse', '--verify', revision + '^{commit}'], cwd=ROOT,
                         capture_output=True, text=True).stdout.strip()
    if not sha:
        sys.exit(f'no revision {revision}')
    if not path.exists():
        WORK.mkdir(parents=True, exist_ok=True)
        subprocess.run(['git', 'worktree', 'add', '--detach', str(path), sha], cwd=ROOT, check=True,
                       capture_output=True)
    else:
        subprocess.run(['git', 'checkout', '--quiet', '--detach', sha], cwd=path, check=True)
    return path, sha[:10]


def ab(args):
    scenes = chosen(args)
    sides = []
    for slot, revision in (('a', args.a), ('b', args.b)):
        if revision == '.':
            # A target of this checkout's own: the slots' are their worktrees'.
            checkout, label = ROOT, 'this checkout'
            target = WORK / f'target-{hashlib.sha1(str(ROOT).encode()).hexdigest()[:8]}'
        else:
            checkout, label = worktree(slot, revision)
            lag, remote = behind(revision)
            label = f'{revision} ({label}' + (f', {lag} behind {remote})' if lag else ')')
            target = WORK / f'target-{slot}'
        sides.append((label, build(checkout, target, scenes)))
    compare(scenes, sides, args.repeat)


def dirs(args):
    """Two directories of examples built otherwise (flags, profiles, allocators)."""
    sides = [(str(path), pathlib.Path(path)) for path in (args.a, args.b)]
    scenes = [s for s in chosen(args) if all((side / s[2]).exists() for _, side in sides)]
    compare(scenes, sides, args.repeat)


def compare(scenes, sides, repeat):
    print(f'A = {sides[0][0]}\nB = {sides[1][0]}\n')
    # The same binary on both sides measures nothing: the change does not
    # reach the scene, or a build was taken for the other's.
    same = [name for name, _, example, _ in scenes
            if filecmp.cmp(sides[0][1] / example, sides[1][1] / example, shallow=False)]
    if same:
        print(f'note: A and B are the same binary for {", ".join(same)}\n')
    print(f'{"scene":14} {"A":>11} {"B":>11} {"B - A":>8}  spread A/B')
    for name, _, example, env in scenes:
        counts = ([], [])
        for _ in range(repeat):
            for side, (_, examples) in enumerate(sides):
                counts[side].append(measure(examples / example, env)[0])
        a, b = (min(c) for c in counts)
        delta = (b - a) / a * 100
        noise = max(spread(counts[0]), spread(counts[1]))
        mark = '*' if abs(delta) > max(noise, 0.1) else ' '
        print(f'{name:14} {a / 1e6:9.1f} M {b / 1e6:9.1f} M {delta:+7.2f}%{mark} '
              f'{spread(counts[0]):.2f}/{spread(counts[1]):.2f}%')
    print('\n* larger than the spread of either side')


def clean(_):
    for slot in ('a', 'b'):
        if (WORK / slot).exists():
            subprocess.run(['git', 'worktree', 'remove', '--force', str(WORK / slot)], cwd=ROOT)
    shutil.rmtree(WORK, ignore_errors=True)


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument('--only', help='comma-separated parts of scene names')
    parser.add_argument('--all', action='store_true', help='the slow scenes too (state, core: 10 s a run)')
    parser.add_argument('--repeat', type=int, default=3)
    commands = parser.add_subparsers(dest='command')
    compare = commands.add_parser('ab', help='compare two revisions (. for this checkout)')
    compare.add_argument('a')
    compare.add_argument('b')
    built = commands.add_parser('dirs', help='compare two directories of built examples')
    built.add_argument('a')
    built.add_argument('b')
    commands.add_parser('clean', help='remove the A/B worktrees and builds')
    args = parser.parse_args()
    {'ab': ab, 'dirs': dirs, 'clean': clean}.get(args.command, run)(args)


if __name__ == '__main__':
    main()
