#!/usr/bin/env python3
"""Exact instruction counts of the CPU-only headless scenes, base against head.

    python3 scripts/perf/gate.py BASE [--threshold 1.0] [--markdown out.md]

Builds this checkout and BASE (a revision, checked out in a worktree under
/tmp/zgx-perf at a path as long as this checkout's, `base_checkout`) into
one target directory, runs each scene under
valgrind's cachegrind without cache simulation, and compares the counted
instructions (`I refs`): exact, and the same on any machine of the
architecture, where hardware counters and time are not. Prints a table (and
writes it as Markdown); exits 1 when a scene costs more than THRESHOLD
percent over BASE (a scene the base does not have yet shows as new). A run
that fails (a script's time budget, a missing GPU) is reported as the
gate's failure, never as a result; benches run with ZGX_GATE=1, which
the script benches take for budgets long enough for cachegrind. Linux
(CI's `perf` job). Nothing renders: under valgrind a software rasterizer's
instructions would swamp zgx's, so heavy, lab and animate run with
RENDER=0 (update, layout and damage, text measured as a renderer would). They
run as a desktop with no screen reader does (`ZGX_ASSISTIVE=0`: labels and
names made for assistive technology wait for one), which is what most
users get; `heavy-stream-at` keeps one listening, to guard that work.
The app scenes count their canvases' CPU work too (`HeadlessCanvases`):
found as shapes the GPU draws, or rasterized where they cannot be. Each
run has one environment, addresses not randomized, and a clock and hash
seeds fixed (`same_run.c`, preloaded): run again, a binary counts the
same instructions, where the clock alone moved mount and app-mail by
tenths of a percent.

With `ZGX_GATE_FUNCTIONS=DIR` (CI's perf job sets it), each cell that grew
more than 0.01% (the largest ten, `ZGX_GATE_FUNCTIONS_MAX`) runs once more
on each side writing what it counted by function, and its difference
(`cgdiff.py`) goes to the log and DIR, with totals by whose code it is
(zgx's, Luau's, the C library's, other Rust) under the table: what the
change did, apart from where the heap or the linker put things. The
counts are the first runs' still.

With ZGX_GATE_BIN_CACHE set, a side's binaries found there, stamped with
its commit and compiler, are taken rather than built, and the head's are
left there after: CI keeps main's by commit, so a pull request builds its
own side alone. `--stage head` builds the head's alone, and `--stage run`
the rest: CI keeps main's head binaries as soon as they are built, and a
pull request looks for its base's once its own are, minutes after a merge
to main rather than at once.
"""
import argparse
import hashlib
import os
import pathlib
import re
import shutil
import statistics
import subprocess
import sys

ROOT = pathlib.Path(__file__).resolve().parents[2]
WORK = pathlib.Path(os.environ.get('ZGX_PERF_DIR', '/tmp/zgx-perf'))
# Where the cells that grew are explained function by function (`by_function`),
# if anywhere: CI's perf job sets it and keeps it as an artifact. The most
# cells explained, the largest first, each two more runs.
FUNCTIONS = os.environ.get('ZGX_GATE_FUNCTIONS')
FUNCTIONS_MAX = int(os.environ.get('ZGX_GATE_FUNCTIONS_MAX', '10'))

# name, package, example, environment: sized to a few billion instructions.
# The windowed scenes' headless twins run with RENDER=0: update, layout and
# damage, with text measured as a renderer would, and nothing drawn.
SCENES = [
    ('heavy-both', 'zgx-desktop', 'heavy_headless', {'ZGX_MODE': 'both', 'TICKS': '300', 'RENDER': '0'}),
    ('heavy-stream', 'zgx-desktop', 'heavy_headless', {'ZGX_MODE': 'stream', 'TICKS': '300', 'RENDER': '0'}),
    ('heavy-scroll', 'zgx-desktop', 'heavy_headless', {'ZGX_MODE': 'scroll', 'TICKS': '300', 'RENDER': '0'}),
    # Every scene runs as a desktop with no screen reader does
    # (`ZGX_ASSISTIVE=0`, see `run`): what most users get. This one keeps
    # assistive technology listening, so its work (labels, names and values
    # from state) stays guarded.
    ('heavy-stream-at', 'zgx-desktop', 'heavy_headless', {'ZGX_MODE': 'stream', 'TICKS': '300', 'RENDER': '0',
                                                          'ZGX_ASSISTIVE': '1'}),
    ('lab-stream', 'zgx-desktop', 'workload_headless', {'ZGX_MODE': 'stream', 'RENDER': '0'}),
    ('lab-scroll', 'zgx-desktop', 'workload_headless', {'ZGX_MODE': 'scroll', 'RENDER': '0'}),
    ('animate', 'zgx-desktop', 'animation_headless', {'FRAMES': '1200', 'RENDER': '0'}),
    # The microbenchmarks (micro_bench, zgx_workload::micro): one state
    # change, one scroll step of 10,000 rows, each written and drawn through
    # to a frame (here undrawn: layout, damage and text measured).
    ('micro-state', 'zgx-desktop', 'micro_bench', {'SCENE': 'state', 'OPS': '300', 'WARMUP': '50', 'RENDER': '0'}),
    ('micro-vscroll-fixed', 'zgx-desktop', 'micro_bench', {'SCENE': 'vscroll-fixed', 'OPS': '300', 'WARMUP': '50', 'RENDER': '0'}),
    ('micro-vscroll-variable', 'zgx-desktop', 'micro_bench', {'SCENE': 'vscroll-variable', 'OPS': '300', 'WARMUP': '50',
                                                              'RENDER': '0'}),
    # The same three written in Luau, through ScriptHost (a binary of its
    # own: micro_bench links no script engine).
    ('micro-state-luau', 'zgx-desktop', 'micro_luau_bench', {'SCENE': 'state', 'OPS': '300', 'WARMUP': '50',
                                                             'RENDER': '0'}),
    ('micro-vscroll-fixed-luau', 'zgx-desktop', 'micro_luau_bench', {'SCENE': 'vscroll-fixed', 'OPS': '300',
                                                                     'WARMUP': '50', 'RENDER': '0'}),
    ('micro-vscroll-variable-luau', 'zgx-desktop', 'micro_luau_bench', {'SCENE': 'vscroll-variable', 'OPS': '300',
                                                                        'WARMUP': '50', 'RENDER': '0'}),
    ('mount', 'zgx-script', 'mount_bench', {}),
    ('script-update', 'zgx-script', 'update_bench', {'TICKS': '200'}),
    ('anime', 'zgx-script', 'anime_bench', {}),
    ('script-layout', 'zgx-script', 'script_layout_bench', {}),
    # Spans in Luau: read, lowered and mounted again each pass (plain, then
    # shaped as a markdown paragraph is), and restyled through a state.
    ('spans-plain', 'zgx-script', 'span_bench', {'SCENE': 'spans-plain', 'N': '10', 'REPS': '15'}),
    ('spans-follow', 'zgx-script', 'span_bench', {'SCENE': 'spans-follow', 'N': '10', 'REPS': '80'}),
    ('spans-md', 'zgx-script', 'span_bench', {'SCENE': 'spans-md', 'N': '10', 'REPS': '10'}),
    ('list-state', 'zgx-script', 'list_state_bench', {}),
    ('style', 'zgx', 'style_bench', {}),
    # 3D frames' CPU work, drawn by wgpu's no-op backend (no driver, no JIT):
    # textured meshes alone, what a feature costs those not using it, then
    # those among skinned, morphing, cut-out and placed ones.
    ('3d-textured', 'zgx-3d', 'world_bench', {'SCENE': 'textured', 'MESHES': '2000', 'FRAMES': '30'}),
    ('3d-mixed', 'zgx-3d', 'world_bench', {'SCENE': 'mixed', 'MESHES': '2000', 'FRAMES': '30'}),
    ('widgets', 'zgx', 'widget_bench', {}),
    # The example apps' common interactions (`support/apps.rs`), small and
    # undrawn but for their canvases' CPU work, run by list_state_bench (no
    # binary of their own to link): the
    # same paths at a sixth of the work (`suite.py` runs `apps_bench` full
    # size).
    ('app-mail', 'zgx-script', 'list_state_bench', {'APP': 'mail', 'REPS': '2'}),
    # A reply's token streamed in, a composer keystroke, a switch.
    ('app-chat', 'zgx-script', 'list_state_bench', {'APP': 'chat', 'REPS': '4'}),
    # A sort with no order kept (the honest whole sort) on its own, as the
    # code editor's new-letter search is.
    ('app-dashboard', 'zgx-script', 'list_state_bench', {'APP': 'dashboard', 'ROWS': '2000', 'REPS': '2',
                                                          'SKIP': 'not sorted by last'}),
    ('app-dashboard-sort', 'zgx-script', 'list_state_bench', {'APP': 'dashboard', 'ROWS': '2000', 'REPS': '3',
                                                               'ONLY': 'not sorted by last'}),
    ('app-sketch', 'zgx-script', 'list_state_bench', {'APP': 'sketch', 'SHAPES': '50', 'REPS': '3'}),
    # The code editor's search of a letter never searched (the whole text
    # read) on its own: added to app-code, it would count as app-code's
    # growth against a base without it.
    ('app-code', 'zgx-script', 'list_state_bench', {'APP': 'code', 'LINES': '5000', 'REPS': '2',
                                                     'SKIP': 'a new letter'}),
    ('app-code-search', 'zgx-script', 'list_state_bench', {'APP': 'code', 'LINES': '5000', 'REPS': '2',
                                                            'ONLY': 'a new letter'}),
]

# What a scene still moves by with no code changed (`same_run.c` fixes the
# rest, and building both sides at paths of one length, `base_checkout`,
# app-code's drift: main against itself now counts it the same): app-chat's
# test waits for its fake server's timer thread, a handoff between threads
# that real time decides.
SPREAD = {'app-chat': 0.2, '3d-textured': 0.06, '3d-mixed': 0.08}
# Built alone: see `build`.
ALONE = 'zgx-3d'

# A Luau scene's count follows where the heap places things (the value
# cache and Luau's tables hash by address, its collector marks in their
# order): a change that resizes any allocated type moves it, by a few
# hundredths of a percent either way. Such a scene that moves more than
# `RECOUNT`% is counted again under these other placements of glibc's
# allocator, each side, and gated on the median; where the deltas disagree
# in sign it is flagged as layout. (Scenes that do not move run once, so
# CI's time stays the same.)
RECOUNT = 0.01
PLACEMENTS = ('glibc.malloc.top_pad=1048576', 'glibc.malloc.mmap_threshold=65536')


def scripted(name, package):
    """Whether a scene runs Luau (its heap placed as `RECOUNT` says)."""
    return package == 'zgx-script' or name.endswith('-luau')


def package_in(checkout, package):
    """`package` as `checkout` names it: a base from before the rename to zgx
    has the crates under their old names."""
    old = package.replace('zgx', 'zgui')  # rename-to-zgx: keep
    renamed = not (checkout / 'crates' / package).exists() and (checkout / 'crates' / old).exists()
    return old if renamed else package


def renamed_env(env):
    """`env`, and each `ZGX_` variable under its old name too: a base from
    before the rename reads those."""
    return {**env, **{'ZGUI_' + k[4:]: v for k, v in env.items() if k.startswith('ZGX_')}}  # rename-to-zgx: keep


def has_example(checkout, package, example):
    """Whether `checkout` has the example (a scene new since the base has not)."""
    crate = checkout / 'crates' / package_in(checkout, package)
    manifest = (crate / 'Cargo.toml').read_text() if (crate / 'Cargo.toml').exists() else ''
    return (crate / 'examples' / f'{example}.rs').exists() or f'name = "{example}"' in manifest


def runs_scene(checkout, env):
    """Whether `checkout`'s binary runs the scene `env` asks for: the app
    scenes (`APP`) are `support/apps.rs`'s, which an older base lacks (its
    binary would count its own bench under their name), and an interaction
    measured alone that its `apps.rs` lacks."""
    apps = checkout / 'crates' / package_in(checkout, 'zgx-script') / 'examples/support/apps.rs'
    if 'APP' not in env:
        return True
    if not apps.exists():
        return False
    # An app, or a scene of one interaction (`ONLY`), new to a base that
    # lacks it: its binary would count nothing, or the app's setup alone.
    text = apps.read_text()
    return f'("{env["APP"]}",' in text and ('ONLY' not in env or env['ONLY'] in text)


def stamp(checkout):
    """What `checkout`'s binaries are built from: its commit and the
    compiler. A checkout with changes of its own has none to match."""
    git = lambda *a: subprocess.run(['git', *a], cwd=checkout, capture_output=True, text=True, check=True).stdout
    if git('status', '--porcelain', '--untracked-files=no').strip():
        return None
    rustc = subprocess.run(['rustc', '-vV'], cwd=checkout, capture_output=True, text=True, check=True).stdout
    return git('rev-parse', 'HEAD') + rustc


def examples(checkout):
    return {s[2] for s in SCENES if has_example(checkout, s[1], s[2])}


def built(checkout, into):
    """Whether `into` holds every binary `checkout` builds, built from it."""
    made = into / 'built-from'
    return (made.exists() and made.read_text() == stamp(checkout)
            and all((into / example).exists() for example in examples(checkout)))


def touch(checkout):
    """Cargo names a path crate's artifacts the same from either checkout,
    and judges them fresh by modification time: files older than what the
    other side last built would be taken for built. Newer, they are compiled
    again (the registry's crates stay shared)."""
    tracked = subprocess.run(['git', 'ls-files', '-z'], cwd=checkout, capture_output=True, check=True).stdout
    for path in filter(None, tracked.split(b'\0')):
        file = checkout / path.decode()
        if file.is_file():
            os.utime(file)


def build(checkout, target, into):
    touch(checkout)
    scenes = [s for s in SCENES if has_example(checkout, s[1], s[2])]
    env = dict(os.environ, CARGO_TARGET_DIR=str(target), CARGO_INCREMENTAL='0',
               CARGO_PROFILE_RELEASE_DEBUG='0')
    # Cargo turns a feature on for everything one build selects, and
    # zgx-3d's bench takes wgpu's no-op backend: built apart, the other
    # benches link the wgpu they always did.
    for part in ([s for s in scenes if s[1] != ALONE], [s for s in scenes if s[1] == ALONE]):
        if not part:
            continue
        command = ['cargo', 'build', '--release', '--locked', '--quiet']
        for package in dict.fromkeys(s[1] for s in part):
            command += ['-p', package_in(checkout, package)]
            for example in dict.fromkeys(s[2] for s in part if s[1] == package):
                command += ['--example', example]
        subprocess.run(command, cwd=checkout, env=env, check=True)
    # One target directory serves both sides (their dependencies are
    # shared): keep this side's binaries before the other overwrites them.
    into.mkdir(parents=True, exist_ok=True)
    for _, _, example, _ in scenes:
        shutil.copy2(target / 'release' / 'examples' / example, into / example)
    made = stamp(checkout)
    if made:
        (into / 'built-from').write_text(made)


class Infra(Exception):
    """The gate could not measure: not a result about the change."""


# The sides' app scripts, copied once a run (see `scripts_of`), and the
# preloaded library, built once (`same_run`).
COPIED = set()


def scripts_of(checkout):
    """`checkout`'s app scripts, copied to a path as long as the other
    side's (WORK/scripts-base and WORK/scripts-head): a script's path is
    read as it loads (resolved, compared, its modules found), and from the
    base's worktree and the head's workspace, paths of different lengths,
    the same work counted differently, with the allocator and Luau's
    collector moving with it (#588: app-mail +0.2% with no code changed)."""
    side = 'head' if pathlib.Path(checkout).resolve() == ROOT.resolve() else 'base'
    to = WORK / f'scripts-{side}'
    if to not in COPIED:
        shutil.rmtree(to, ignore_errors=True)
        shutil.copytree(checkout / 'crates' / package_in(checkout, 'zgx-script') / 'examples', to)
        COPIED.add(to)
    return to


def count(binary, env, checkout=None, placement=None, out='/dev/null'):
    """The run's instructions (and cachegrind's file of them by function in
    `out`). A run that fails is the gate's failure, not the change's: under
    cachegrind a script's time budget (`ZGX_GATE` lifts it in the benches)
    or a missing GPU would otherwise look like a result."""
    # A side's app scripts are its own: a binary cached from main was built
    # in the checkout a pull request runs in, where it would find the
    # head's (a script using what the base lacks would fail there).
    scripts = {}
    if checkout is not None:
        scripts['ZGX_EXAMPLES'] = str(scripts_of(checkout))
    # One environment, addresses not randomized, the clock and seeds fixed
    # (`same_run`): run again, the same binary counts the same.
    fixed = {k: os.environ[k] for k in ('PATH', 'HOME', 'ZGX_REAL_CLOCK') if k in os.environ}
    fixed.update(LD_PRELOAD=str(same_run()), ZGX_SAME_RUN_REPORT='1')
    if placement:
        fixed['GLIBC_TUNABLES'] = placement
    command = ['valgrind', '--tool=cachegrind', '--cache-sim=no', f'--cachegrind-out-file={out}', str(binary)]
    result = subprocess.run(
        ['setarch', '-R', *command] if shutil.which('setarch') else command,
        env={**fixed, **renamed_env({'ZGX_GATE': '1', 'ZGX_ASSISTIVE': '0', **scripts, **env})},
        capture_output=True, text=True)
    found = re.search(r'I\s+refs:\s+([\d,]+)', result.stderr)
    if result.returncode or not found:
        why = 'a script hit its time budget' if 'time budget' in result.stdout + result.stderr else 'it failed'
        raise Infra(f'{binary.name}: {why} under cachegrind\n{result.stdout[-1500:]}{result.stderr[-3000:]}')
    if 'same_run: ' not in result.stderr:
        raise Infra(f'{binary.name}: ran without same_run.so preloaded\n{result.stderr[-3000:]}')
    return int(found.group(1).replace(',', ''))


def compared(name, package, env, base, head):
    """A scene's counts on the base (a binary and its checkout, or `None`
    for a scene new in the head) and the head, the change in percent, and
    every change counted: one, or for a Luau scene that moved more than
    `RECOUNT`%, one under each heap placement, the change their median."""
    after = count(head[0], env, head[1])
    if base is None:
        return None, after, None, []
    before = count(base[0], env, base[1])
    deltas = [(after - before) / before * 100]
    if scripted(name, package) and abs(deltas[0]) > RECOUNT:
        for placement in PLACEMENTS:
            placed = count(base[0], env, base[1], placement)
            deltas.append((count(head[0], env, head[1], placement) - placed) / placed * 100)
    return before, after, statistics.median(deltas), deltas


def by_function(rows, scenes, base):
    """For the cells that grew most, each side run again writing what it
    counted by function, and the difference (`cgdiff.py`) in the log and in
    `ZGX_GATE_FUNCTIONS`, with its totals by whose code it is for the
    table. The counts above are the first runs': these only explain them.
    Never the gate's failure."""
    sys.path.insert(0, str(pathlib.Path(__file__).parent))
    import cgdiff
    into = pathlib.Path(FUNCTIONS)
    into.mkdir(parents=True, exist_ok=True)
    grew = sorted((row for row in rows if row[3] is not None and row[3] > 0.01), key=lambda row: -row[3])
    summary = []
    for name, *_ in grew[:FUNCTIONS_MAX]:
        _, package, example, env = scenes[name]
        try:
            for side, binary, checkout in (('base', WORK / 'gate-bin-base' / example, base),
                                           ('head', WORK / 'gate-bin-head' / example, ROOT)):
                count(binary, env, checkout, out=str(into / f'{name}.{side}.cg'))
            report, groups, total = cgdiff.diff(into / f'{name}.base.cg', into / f'{name}.head.cg')
        except (Infra, OSError, ValueError) as error:
            print(f'=== {name}: no functions ({error})', flush=True)
            continue
        (into / f'{name}.txt').write_text(report + '\n')
        print(f'=== {name} by function\n{report}', flush=True)
        summary.append(f'| {name} | {total:+,} | ' + ' | '.join(f'{groups[g]:+,}' for g in cgdiff.GROUPS) + ' |')
    if not summary:
        return []
    return ['', '<details><summary>Cells that grew, by whose code (instructions, head − base; '
            'functions in the job log and its artifact)</summary>', '',
            '| scene | total | ' + ' | '.join(cgdiff.GROUPS) + ' |',
            '| --- | ---: |' + ' ---: |' * len(cgdiff.GROUPS), *summary, '', '</details>']


def layout_of(deltas):
    """Deltas that disagree in sign, where the heap lies: layout, not the
    change's work."""
    return '; layout' if min(deltas) < 0 < max(deltas) else ''


def base_checkout():
    """Where the base is checked out to build: a path exactly as long as
    the head's checkout. A binary keeps the path its sources were built
    at (panics' locations, `file!`), and app-code's count follows that
    path's length, by about 8,000 instructions a character (two builds of
    one commit, 10 characters apart: 81,632; of equal length: 94), so a
    base built elsewhere read as +0.09% on every change."""
    want = len(str(ROOT.resolve()))
    # Named for this checkout: another checkout's gate (of the same
    # length) shares /tmp, and the work directory may be shared too.
    tag = hashlib.sha1(str(ROOT.resolve()).encode()).hexdigest()
    for parent in (WORK, pathlib.Path('/tmp')):
        room = want - len(str(parent.resolve())) - 1
        if room >= 10:
            return parent / (f'gate-base-{tag}' + '_' * room)[:room]
        if room >= 4:
            return parent / (tag + '_' * room)[:room]
    sys.exit(f'no directory for the base as short as the checkout ({ROOT})')


def same_run():
    """`same_run.c` built for preloading, once a run: it fixes what made
    one binary count differently from run to run (Luau's collector and
    script calls pacing themselves by the clock, std's hash seeds)."""
    built = WORK / 'same_run.so'
    if built not in COPIED:
        WORK.mkdir(parents=True, exist_ok=True)
        source = pathlib.Path(__file__).with_name('same_run.c')
        subprocess.run(['cc', '-O2', '-shared', '-fPIC', '-o', str(built), str(source)], check=True)
        COPIED.add(built)
    return built


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument('base')
    parser.add_argument('--threshold', type=float, default=1.0, help='percent a scene may grow')
    parser.add_argument('--markdown', type=pathlib.Path)
    parser.add_argument('--stage', choices=['head', 'run'],
                        help="build the head's binaries alone, or the rest with them built")
    args = parser.parse_args()
    # Both sides build into one target directory: CI's (ZGX_GATE_TARGET)
    # sits in the workspace's, where its cache keeps the dependencies.
    target = pathlib.Path(os.environ.get('ZGX_GATE_TARGET', WORK / 'gate-target')).resolve()
    base = base_checkout()
    # A worktree left from a run before (or registered, its directory gone
    # with a reboot's /tmp): this checkout's, as `base_checkout` names it.
    subprocess.run(['git', 'worktree', 'prune'], cwd=ROOT)
    if base.exists():
        subprocess.run(['git', 'worktree', 'remove', '--force', str(base)], cwd=ROOT)
        shutil.rmtree(base, ignore_errors=True)
    subprocess.run(['git', 'worktree', 'add', '--detach', str(base), args.base], cwd=ROOT, check=True,
                   capture_output=True)
    cache = os.environ.get('ZGX_GATE_BIN_CACHE')
    cache = pathlib.Path(cache) if cache else None
    # The base's from the cache, before the head's take its place there.
    cached = args.stage != 'head' and cache is not None and built(base, cache)
    if cached:
        print('base binaries from the cache', flush=True)
        shutil.rmtree(WORK / 'gate-bin-base', ignore_errors=True)
        shutil.copytree(cache, WORK / 'gate-bin-base')
    if args.stage != 'run':
        build(ROOT, target, WORK / 'gate-bin-head')
        if cache:
            shutil.rmtree(cache, ignore_errors=True)
            shutil.copytree(WORK / 'gate-bin-head', cache)
        if args.stage == 'head':
            return
    elif not (WORK / 'gate-bin-head').is_dir():
        sys.exit("--stage run: the head's binaries are not built (--stage head)")
    if not cached:
        build(base, target, WORK / 'gate-bin-base')
    rows, failed = [], []
    for name, package, example, env in SCENES:
        base_binary = WORK / 'gate-bin-base' / example
        head_binary = WORK / 'gate-bin-head' / example
        try:
            runs = base_binary.exists() and runs_scene(base, env)
            before, after, delta, deltas = compared(name, package, env, (base_binary, base) if runs else None,
                                                    (head_binary, ROOT))
        except Infra as error:
            # Loudly, and as what it is: no table that reads as a result.
            if args.markdown:
                args.markdown.write_text(f'**Instruction gate: infrastructure failure, no result.**\n\n```\n{error}\n```\n')
            sys.exit(f'GATE INFRA FAILURE (not a performance result): {error}')
        if delta is not None and delta > args.threshold:
            failed.append(name)
        rows.append((name, before, after, delta, deltas))
        shown = f'{before / 1e6:10.1f} M {after / 1e6:10.1f} M {delta:+7.2f}%' if before else f'{"new":>12} {after / 1e6:10.1f} M'
        if len(deltas) > 1:
            shown += f'  median of {", ".join(f"{d:+.3f}%" for d in deltas)}{layout_of(deltas)}'
        print(f'{name:14} {shown}', flush=True)
    sha = subprocess.run(['git', 'rev-parse', '--short', args.base], cwd=ROOT, capture_output=True,
                         text=True).stdout.strip()
    table = [f'**Instruction counts** (cachegrind, exact) against `{sha}`; the gate fails above '
             f'+{args.threshold:g}%.', '', '| scene | base | head | change |', '| --- | ---: | ---: | ---: |']
    for name, before, after, delta, deltas in rows:
        flag = ' ⚠️' if name in failed else ''
        if before is None:
            table.append(f'| {name} | (new) | {after / 1e6:,.1f} M | |')
        else:
            spread = f' (±{SPREAD[name]:g}% with no change)' if name in SPREAD else ''
            if len(deltas) > 1:
                spread += (f' (median of {", ".join(f"{d:+.3f}%" for d in deltas)} under '
                           f'{len(deltas)} heap placements{layout_of(deltas)})')
            table.append(f'| {name} | {before / 1e6:,.1f} M | {after / 1e6:,.1f} M | {delta:+.2f}%{flag}{spread} |')
    if FUNCTIONS:
        table += by_function(rows, {scene[0]: scene for scene in SCENES}, base)
    if args.markdown:
        args.markdown.write_text('\n'.join(table) + '\n')
    if failed:
        sys.exit(f'more than +{args.threshold:g}% instructions: {", ".join(failed)}')


if __name__ == '__main__':
    main()
