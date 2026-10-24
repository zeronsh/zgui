#!/usr/bin/env python3
"""Run the comparison workloads on this Mac.

The Linux protocol (docs/results/*/protocol.md), on macOS with the Metal GPU:
three repeats of each active mode for each framework, five of idle, in
rotated order. Each active trial requests 20 seconds; the first 5 are
excluded, the next 15 sampled every 50 ms by scripts/macsample.swift (idle:
20-35 s after launch, once the Metal driver has given back the memory it
holds for a process that drew lately; the 5-20 s figures are kept too, as
`early_*`; see docs/benchmarking.md): the app's CPU time, CPU energy and
instructions, memory (resident size, and physical footprint: what Activity
Monitor reports, including GPU memory the process owns on unified memory),
its GPU time, and the window server's CPU and GPU time over the same window.
GPU time stretches as the GPU clocks down under light load, so the GPU's
mean active clock is read too (IOReport, as are the whole GPU's power):
GPU time by clock is the megacycles a second the app used.
Active trials must deliver 58-61 logical updates per requested second.

Scenes (`--scene`):
- `lab` (default): streaming text and a 100,000-row virtual list, for zgx,
  GPUI, GPUI Fast (longbridge/gpui-fast, a GPUI fork that retains unchanged
  views, layout and text) and QuickGUI.
- `heavy`: a 1280x800 dashboard where most content changes every tick
  (zgx_workload::heavy), for zgx, GPUI and GPUI Fast (QuickGUI's text
  renderer panics past 32 text batches on it).
- `animate`: dots and text animated at the display's rate
  (zgx_workload::animation), for zgx, GPUI and GPUI Fast; no update gate,
  each reports the frames it drew.

Each repeat also measures the desktop alone (`desktop`/`baseline`: a sleeping
process as the target): the window server composites every window, so its
cost with an app, less the baseline, is what the app adds. After the timed
trials, pacing trials capture each app's window (scripts/framepace.swift) to
time the frames it shows; capturing costs the window server work, so they are
kept apart from the timed trials. There are eight of those per framework and
mode: an app's 60 Hz timer lands at a phase of the display's refresh fixed for
a whole capture, near the window server's latch in some and not others, so
one capture's 99th-percentile interval swings by 5 ms or more between
captures, for every framework.

`--powermetrics` also records package and GPU power with `sudo powermetrics`
(run `sudo -v` first in the same terminal; without cached credentials the
columns stay empty).

`--quick` is for iterating, not publishing: zgx and GPUI Fast only, two
repeats, and pacing captured during the sampling trials instead
of apart (capturing costs the window server work: its numbers then read
high for every framework). About 10 minutes for heavy. The full run measures
QuickGUI only with `--quickgui`. `--modes idle --into docs/results/<run>`
re-measures a published run's idle cells and replaces them.
`--pgo` runs every framework's profile-guided build, each trained on its own
scene (scripts/perf/pgo_compare.py), into `latest-macos-<scene>-pgo-…`;
with `--no-pacing --modes stream,scroll,both` it is the CPU, GPU and memory
supplement beside a main run.

Before measuring, a preflight waits until nothing heavy runs (compilers of
any agent, VMs, Instruments, indexing, backups, updates) and the rest of the
machine uses under 75% of a core (scripts/perf/preflight.py;
`--preflight-threshold`, `--no-preflight`), and Zeron's windows are hidden for the run (`--hide`). The machine's
state (displays and refresh rates, a live wallpaper, power mode, macOS
build, uptime) and what the preflight found go into audit.json.

Windows open on screen: keep the Mac awake and the windows uncovered (a
covered window may stop presenting), and keep the rest of the desktop still.
Writes docs/results/latest-macos[-<scene>]-<date>-<commit>.
"""
import argparse, csv, datetime, hashlib, json, os, plistlib, re, signal, statistics, subprocess, sys, time
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / 'scripts/perf'))
import preflight  # noqa: E402
REQUESTED, WARMUP, SAMPLED, INTERVAL = 20, 5, 15, 0.05
# Idle is sampled 20-35 s after launch, five times: until the Metal driver
# gives back the memory it holds for any process that used the GPU lately
# (a ~180 MB floor, returned ~6 or ~8 s after launch for every framework, on
# a grid that follows the desktop's state rather than the app), a 5-20 s
# idle mean measures that timing more than the app. The 5-20 s figures stay
# in each idle trial's raw data, prefixed `early_`. See docs/benchmarking.md.
IDLE_SETTLE, IDLE_REPEATS = 20, 5
PACING_SECONDS, PACING_REPEATS = 8, 8
MODES = ['idle', 'stream', 'scroll', 'both']
SCENES = {
    'lab': {
        'modes': MODES,
        'binaries': {
            'zgx': ROOT / 'target/dist/examples/component_workload',
            'gpui': ROOT / 'comparisons/gpui/target/dist/zgx-compare-gpui',
            'gpui-fast': ROOT / 'comparisons/gpui-fast/target/dist/zgx-compare-gpui-fast',
            'quickgui': ROOT / 'comparisons/quickgui/target/dist/zgx-compare-quickgui',
        },
    },
    'heavy': {
        'modes': MODES,
        'binaries': {
            'zgx': ROOT / 'target/dist/examples/heavy_workload',
            'gpui': ROOT / 'comparisons/gpui/target/dist/heavy',
            'gpui-fast': ROOT / 'comparisons/gpui-fast/target/dist/heavy',
        },
    },
    'animate': {
        'modes': ['animate'],
        'binaries': {
            'zgx': ROOT / 'target/dist/examples/animation_workload',
            'gpui': ROOT / 'comparisons/gpui/target/dist/animate',
            'gpui-fast': ROOT / 'comparisons/gpui-fast/target/dist/animate',
        },
    },
}
SAMPLER, PACER = Path('/tmp/zgx-macsample'), Path('/tmp/zgx-framepace')
# What the binaries are built from, including vendored patches.
SOURCES = ['crates', 'comparisons', 'scripts', 'vendor', 'Cargo.toml', 'Cargo.lock']
METRICS = ['cpu_percent_one_core', 'cpu_watts', 'instructions', 'cycles', 'gpu_percent',
           'gpu_mcycles_per_second', 'gpu_active_mhz', 'gpu_system_watts',
           'windowserver_cpu_percent', 'windowserver_gpu_percent', 'gpu_utilization_percent',
           'mean_footprint_bytes', 'peak_footprint_bytes', 'mean_rss_bytes', 'peak_rss_bytes',
           'workload_ticks', 'wall_seconds']
POWER = ['package_mw', 'cpu_mw', 'gpu_mw']
FIELDS = ['framework', 'mode', 'repeat', 'wall_seconds', 'cpu_seconds', 'cpu_percent_one_core', 'cpu_energy_joules',
          'cpu_watts', 'instructions', 'cycles', 'gpu_seconds', 'gpu_percent', 'gpu_mcycles_per_second',
          'gpu_active_mhz', 'gpu_system_watts', 'windowserver_cpu_percent',
          'windowserver_gpu_percent', 'gpu_utilization_percent', 'mean_footprint_bytes', 'peak_footprint_bytes',
          'mean_rss_bytes', 'peak_rss_bytes', 'samples', 'workload_ticks', 'reported_fps', *POWER, 'exit_code',
          'terminated_after_report', 'early_mean_footprint_bytes', 'early_cpu_percent_one_core']
PACING = ['frames_per_second', 'interval_p50_ms', 'interval_p95_ms', 'interval_p99_ms', 'interval_max_ms',
          'long_interval_fraction']


def run(cmd, **kw):
    return subprocess.run(cmd, check=True, capture_output=True, text=True, **kw).stdout.strip()


def build(scene, frameworks):
    example = SCENES[scene]['binaries']['zgx'].name
    run(['cargo', 'build', '--profile', 'dist', '--locked', '-p', 'zgx-desktop', '--example', example], cwd=ROOT)
    for name in [f for f in frameworks if f != 'zgx']:
        run(['cargo', 'build', '--profile', 'dist', '--locked'], cwd=ROOT / 'comparisons' / name)
    run(['swiftc', '-O', str(ROOT / 'scripts/macsample.swift'), '-o', str(SAMPLER), '-lIOReport'])
    run(['swiftc', '-O', str(ROOT / 'scripts/framepace.swift'), '-o', str(PACER)])


def environment():
    sysctl = lambda key: run(['sysctl', '-n', key])
    power = run(['pmset', '-g', 'batt']).splitlines()[0]
    return {
        **preflight.state(),
        'machine': sysctl('hw.model'),
        'chip': sysctl('machdep.cpu.brand_string'),
        'cores': int(sysctl('hw.ncpu')),
        'memory_bytes': int(sysctl('hw.memsize')),
        'macos': f"{run(['sw_vers', '-productVersion'])} ({run(['sw_vers', '-buildVersion'])})",
        'power': power.split("'")[1] if "'" in power else power,
        'rust': run(['rustc', '--version']),
    }


def launch(binary, mode, seconds, log):
    env = dict(os.environ, ZGX_MODE=mode, ZGX_SECONDS=str(seconds))
    with log.open('w') as handle:
        process = subprocess.Popen([str(binary)], stdout=handle, stderr=subprocess.STDOUT, env=env)
    time.sleep(0.5)
    # Bring the window forward: covered windows may stop presenting.
    subprocess.run(['osascript', '-e', f'tell application "System Events" to set frontmost of (first process whose unix id is {process.pid}) to true'], capture_output=True)
    return process


def finish(process, seconds):
    try:
        return process.wait(timeout=seconds + 10), False
    except subprocess.TimeoutExpired:
        # macOS apps outlive their last window (QuickGUI closes it at the
        # deadline and reports): end it once it has reported.
        process.send_signal(signal.SIGTERM)
        return process.wait(timeout=10), True


def powermetrics():
    """Package, CPU and GPU power over the sampled window, if sudo allows."""
    command = f'sleep {WARMUP - 0.5}; exec sudo -n /usr/bin/powermetrics --samplers cpu_power,gpu_power -f plist -i 1000 -n {SAMPLED}'
    return subprocess.Popen(['/bin/sh', '-c', command], stdout=subprocess.PIPE, stderr=subprocess.DEVNULL)


def power_of(process):
    output, _ = process.communicate()
    samples = [plistlib.loads(doc.strip() + b'</plist>') for doc in output.split(b'</plist>') if doc.strip()]
    processor = [s.get('processor', {}) for s in samples]
    mean = lambda key: statistics.mean(p[key] for p in processor) if processor and all(key in p for p in processor) else ''
    return {'package_mw': mean('combined_power'), 'cpu_mw': mean('cpu_power'), 'gpu_mw': mean('gpu_power')}


def trial(scene, framework, mode, repeat, out, power, paced=None):
    """One timed trial; with `paced` (a list), its frames' pacing too, appended there."""
    log = out / f'{framework}.{mode}.{repeat}.log'
    idle = mode == 'idle'
    requested = IDLE_SETTLE + SAMPLED + 1 if idle else REQUESTED
    if framework == 'desktop':
        process = subprocess.Popen(['/bin/sleep', str(REQUESTED)])
    else:
        process = launch(SCENES[scene]['binaries'][framework], mode, requested, log)
    # Idle: the 5-20 s window too, alongside the settled one, for the record.
    early = subprocess.Popen([str(SAMPLER), str(process.pid), str(WARMUP - 0.5), str(SAMPLED), str(INTERVAL)],
                             stdout=subprocess.PIPE, text=True) if idle else None
    meter = powermetrics() if power else None
    pacer = None
    if paced is not None and framework != 'desktop' and mode != 'idle':
        # As long as a pacing trial's capture: one running to the app's own
        # end loses its stream as the window closes.
        pacer = subprocess.Popen([str(PACER), str(process.pid), str(WARMUP - 0.5), str(PACING_SECONDS)],
                                 stdout=subprocess.PIPE, stderr=subprocess.DEVNULL, text=True)
    start = IDLE_SETTLE - 0.5 if idle else WARMUP - 0.5
    sample = json.loads(run([str(SAMPLER), str(process.pid), str(start), str(SAMPLED), str(INTERVAL)]))
    if early is not None:
        sample.update({f'early_{key}': value for key, value in json.loads(early.communicate()[0]).items()})
    if pacer is not None:
        output, _ = pacer.communicate()
        try:
            pace = {'framework': framework, 'mode': mode, 'repeat': repeat, **json.loads(output)}
        except json.JSONDecodeError:
            pace = {'framework': framework, 'mode': mode, 'repeat': repeat, 'error': output[-500:]}
        (out / f'{framework}.{mode}.{repeat}.pacing.json').write_text(json.dumps(pace, indent=1) + '\n')
        if 'error' not in pace:
            paced.append(pace)
    exit_code, terminated = finish(process, requested)
    text = log.read_text() if log.exists() else ''
    ticks = re.search(r'"ticks":(\d+)|workload_ticks=(\d+)|animation_frames=(\d+)', text)
    fps = re.search(r'fps=([0-9.]+)', text)
    row = {'framework': framework, 'mode': mode, 'repeat': repeat, **sample,
           'workload_ticks': int(next(g for g in ticks.groups() if g)) if ticks else (0 if framework == 'desktop' else -1),
           'reported_fps': float(fps.group(1)) if fps else '',
           **(power_of(meter) if meter else dict.fromkeys(POWER, '')),
           'exit_code': exit_code, 'terminated_after_report': terminated}
    (out / f'{framework}.{mode}.{repeat}.json').write_text(json.dumps(row, indent=1) + '\n')
    return row


def pacing(scene, framework, mode, repeat, out):
    seconds = WARMUP + PACING_SECONDS + 1
    # Screen capture occasionally fails to find a window that just opened:
    # one more try before giving up on the trial.
    for attempt in range(2):
        process = launch(SCENES[scene]['binaries'][framework], mode, seconds, out / f'{framework}.{mode}.{repeat}.pacing.log')
        try:
            result = json.loads(run([str(PACER), str(process.pid), str(WARMUP - 0.5), str(PACING_SECONDS)]))
            break
        except subprocess.CalledProcessError:
            finish(process, seconds)
            if attempt:
                raise
            time.sleep(2)
    finish(process, seconds)
    row = {'framework': framework, 'mode': mode, 'repeat': repeat, **result}
    (out / f'{framework}.{mode}.{repeat}.pacing.json').write_text(json.dumps(row, indent=1) + '\n')
    return row


def source_diff_sha256():
    """The uncommitted source: tracked changes and untracked files."""
    digest = hashlib.sha256(run(['git', 'diff', 'HEAD', '--', *SOURCES], cwd=ROOT).encode())
    for path in sorted(run(['git', 'ls-files', '--others', '--exclude-standard', '--', *SOURCES], cwd=ROOT).split()):
        digest.update(path.encode() + b'\0' + (ROOT / path).read_bytes())
    return digest.hexdigest()


def group(rows, keys, framework, mode):
    chosen = [r for r in rows if (r['framework'], r['mode']) == (framework, mode)]
    stats = {}
    for key in keys:
        values = [float(r[key]) for r in chosen if r.get(key) not in ('', None)]
        if values:
            stats[key] = {'median': statistics.median(values), 'min': min(values), 'max': max(values)}
    return {'framework': framework, 'mode': mode, 'repeats': len(chosen), **stats}


def write_csv(path, fields, rows):
    with path.open('w', newline='') as handle:
        writer = csv.DictWriter(handle, fieldnames=fields, extrasaction='ignore')
        writer.writeheader()
        writer.writerows(rows)


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument('--skip-build', action='store_true')
    parser.add_argument('--scene', choices=sorted(SCENES), default='lab')
    parser.add_argument('--powermetrics', action='store_true')
    parser.add_argument('--quick', action='store_true', help='zgx and GPUI Fast, 2 repeats, pacing in the trials')
    parser.add_argument('--frameworks', help='comma-separated, from the scene\'s')
    parser.add_argument('--repeats', type=int, help='repeats of each active mode (3; --quick 2)')
    parser.add_argument('--quickgui', action='store_true', help='QuickGUI too (lab)')
    parser.add_argument('--no-preflight', action='store_true')
    parser.add_argument('--preflight-threshold', type=float, default=75.,
                        help='percent of a core the rest of the machine may use')
    parser.add_argument('--hide', default='Zeron', help='an app to hide during the run (empty: none)')
    parser.add_argument('--modes', help="comma-separated, from the scene's (default: all)")
    parser.add_argument('--no-pacing', action='store_true',
                        help='skip the frame pacing captures (a CPU, GPU and memory supplement)')
    parser.add_argument('--pgo', action='store_true',
                        help='every framework profile-guided, from target/pgo-compare (scripts/perf/pgo_compare.py)')
    parser.add_argument('--into', type=Path,
                        help="a published run of the scene: replace its cells of --modes with this run's")
    args = parser.parse_args()
    frameworks, modes = list(SCENES[args.scene]['binaries']), SCENES[args.scene]['modes']
    if not args.quickgui:
        frameworks = [f for f in frameworks if f != 'quickgui']
    if args.quick:
        frameworks = [f for f in frameworks if f in ('zgx', 'gpui-fast')]
    if args.frameworks:
        frameworks = [f for f in args.frameworks.split(',') if f in SCENES[args.scene]['binaries']]
    if args.modes:
        modes = [m for m in modes if m in args.modes.split(',')]
    assert not args.into or args.modes, '--into replaces the cells of --modes'
    repeats = args.repeats or (2 if args.quick else 3)
    if args.pgo:
        # The supplement: each framework's apps built with a profile of their own runs.
        for framework, path in SCENES[args.scene]['binaries'].items():
            SCENES[args.scene]['binaries'][framework] = ROOT / 'target/pgo-compare' / framework / path.name
        missing = [f for f in frameworks if not SCENES[args.scene]['binaries'][f].exists()]
        assert not missing, f'no PGO build of {missing}: run scripts/perf/pgo_compare.py'
        args.skip_build = True
        run(['swiftc', '-O', str(ROOT / 'scripts/macsample.swift'), '-o', str(SAMPLER), '-lIOReport'])
        run(['swiftc', '-O', str(ROOT / 'scripts/framepace.swift'), '-o', str(PACER)])
    if not args.skip_build:
        build(args.scene, frameworks)
    commit = run(['git', 'rev-parse', 'HEAD'], cwd=ROOT)
    found = None if args.no_preflight else preflight.wait_until_quiet(
        log=lambda line: print(line, flush=True), threshold=args.preflight_threshold)
    hidden = hide(args.hide) if args.hide else False
    try:
        measure(args, frameworks, modes, repeats, commit, found)
    finally:
        if hidden:
            show(args.hide)


def hide(app):
    """Hide `app`'s windows if it runs; whether it was hidden."""
    script = f'tell application "System Events" to if exists process "{app}" then set visible of process "{app}" to false'
    return subprocess.run(['osascript', '-e', script], capture_output=True).returncode == 0


def show(app):
    script = f'tell application "System Events" to if exists process "{app}" then set visible of process "{app}" to true'
    subprocess.run(['osascript', '-e', script], capture_output=True)


def measure(args, frameworks, modes, repeats, commit, found):
    started = datetime.datetime.now(datetime.timezone.utc)
    scene = '' if args.scene == 'lab' else f'-{args.scene}'
    tag = ('-quick' if args.quick else '') + ('-pgo' if args.pgo else '')
    out = args.into or ROOT / 'docs/results' / f'latest-macos{scene}{tag}-{started:%Y-%m-%d}-{commit[:7]}'
    out.mkdir(parents=True, exist_ok=True)
    rows, paced = [], []
    idle_repeats = min(IDLE_REPEATS, repeats) if args.quick else IDLE_REPEATS
    rerun = list(modes)
    if args.into:
        # The run's earlier trials of these modes stay, apart: they are what
        # it first published. Its pacing captures stay where they are.
        for mode in rerun:
            legacy = out / ('idle-5-20s' if mode == 'idle' else f'{mode}-before')
            legacy.mkdir(exist_ok=True)
            for path in out.glob(f'*.{mode}.*'):
                if '.pacing.' not in path.name:
                    path.rename(legacy / path.name)
    for repeat in range(max(repeats, idle_repeats) if 'idle' in modes else repeats):
        order = frameworks[repeat % len(frameworks):] + frameworks[:repeat % len(frameworks)]
        chosen = [m for m in modes if repeat < (idle_repeats if m == 'idle' else repeats)]
        for framework, mode in [('desktop', 'baseline')] + [(f, m) for m in chosen for f in order]:
            # Into a published run: its own desktop baselines stay, this run's apart.
            place = out / 'idle-rerun-baselines' if args.into and framework == 'desktop' else out
            place.mkdir(exist_ok=True)
            row = trial(args.scene, framework, mode, repeat, place, args.powermetrics, paced if args.quick else None)
            print(f"{framework:8} {mode:8} {repeat}: cpu {row['cpu_percent_one_core']:6.2f}% {row['cpu_watts'] * 1000:6.0f} mW "
                  f"gpu {row['gpu_percent']:5.2f}% {row['gpu_mcycles_per_second']:5.1f} Mcyc/s server cpu {row['windowserver_cpu_percent']:5.1f}% gpu {row['windowserver_gpu_percent']:5.1f}% "
                  f"footprint {row['mean_footprint_bytes'] / 2**20:6.1f} MiB ticks {row['workload_ticks']}", flush=True)
            rows.append(row)
            time.sleep(2)
    for repeat in range(0 if args.quick or args.no_pacing else PACING_REPEATS):
        order = frameworks[repeat % len(frameworks):] + frameworks[:repeat % len(frameworks)]
        for mode in [m for m in modes if m != 'idle']:
            for framework in order:
                row = pacing(args.scene, framework, mode, repeat, out)
                print(f"{framework:8} {mode:8} {repeat}: pacing {row.get('frames_per_second', 0):6.1f} fps p99 "
                      f"{row.get('interval_p99_ms', 0):5.1f} ms long {row.get('long_interval_fraction', 0):.3f}", flush=True)
                paced.append(row)
                time.sleep(2)
    if args.into:
        # The published run's other cells, and its pacing, with this run's idle.
        kept = [r for r in csv.DictReader((out / 'current.csv').open()) if r['mode'] not in rerun]
        for row in kept:
            for key in ('repeat', 'workload_ticks', 'exit_code', 'samples'):
                row[key] = int(row[key])
            row['terminated_after_report'] = row['terminated_after_report'] == 'True'
        rows = kept + [r for r in rows if r['framework'] != 'desktop']
        paced = list(csv.DictReader((out / 'pacing.csv').open()))
        modes = SCENES[args.scene]['modes']
    write_csv(out / 'current.csv', FIELDS, rows)
    if not args.into:
        write_csv(out / 'pacing.csv', ['framework', 'mode', 'repeat', 'display_hz', 'frames', *PACING, 'long_intervals'], paced)
    groups = [group(rows, METRICS + POWER, 'desktop', 'baseline')]
    groups += [group(rows, METRICS + POWER + ['reported_fps'], f, m) for f in frameworks for m in modes]
    pacing_groups = [group(paced, PACING, f, m) for f in frameworks for m in modes if m != 'idle']
    if 'idle' in rerun:
        # What the idle cells read over 5-20 s, as runs before October 24 sampled them.
        early = [group(rows, [f'early_{k}' for k in METRICS], f, 'idle') for f in frameworks]
        (out / 'idle-5-20s.json').write_text(json.dumps({'groups': early}, indent=1) + '\n')
    (out / 'summary.json').write_text(json.dumps({'groups': groups, 'pacing': pacing_groups}, indent=1) + '\n')
    apps = [r for r in rows if r['framework'] != 'desktop']
    if args.scene == 'animate':
        active = [r['reported_fps'] for r in apps if r['reported_fps'] != '']
        gated = all(r['workload_ticks'] > 0 for r in apps)
    else:
        active = [r['workload_ticks'] / REQUESTED for r in apps if r['mode'] != 'idle']
        gated = (all(58 <= t <= 61 for t in active)
                 and all(r['workload_ticks'] == 0 for r in apps if r['mode'] == 'idle'))
    accepted = gated and all(r['exit_code'] == 0 or r['terminated_after_report'] for r in apps)
    binaries = {name: {'sha256': hashlib.sha256(path.read_bytes()).hexdigest(), 'bytes': path.stat().st_size}
                for name, path in SCENES[args.scene]['binaries'].items() if name in frameworks}
    audit = {'accepted': accepted, 'trials': len(rows), 'samples': sum(r['samples'] for r in rows),
             'pacing_trials': len(paced),
             ('reported_fps' if args.scene == 'animate' else 'active_ticks_per_requested_second'):
                 {'min': min(active), 'max': max(active)} if active else None,
             'powermetrics': any(r.get('package_mw') not in ('', None) for r in rows),
             'scene': args.scene, 'commit': commit,
             # Uncommitted source changes the binaries were built with, if any.
             'source_dirty': bool(run(['git', 'status', '--porcelain', '--', *SOURCES], cwd=ROOT)),
             'source_diff_sha256': source_diff_sha256(),
             'started_utc': started.isoformat(), 'finished_utc': datetime.datetime.now(datetime.timezone.utc).isoformat(),
             'environment': environment(), 'preflight': found, 'binaries': binaries,
             'protocol': {'requested_seconds': REQUESTED, 'warmup_seconds': WARMUP, 'sampled_seconds': SAMPLED,
                          'interval_seconds': INTERVAL, 'repeats': repeats, 'idle_repeats': idle_repeats,
                          'idle_window_seconds': [IDLE_SETTLE, IDLE_SETTLE + SAMPLED],
                          'idle_requested_seconds': IDLE_SETTLE + SAMPLED + 1,
                          'quick': args.quick, 'pgo': args.pgo, 'frameworks': frameworks, 'modes': modes,
                          'pacing_seconds': PACING_SECONDS,
                          'pacing_repeats': 0 if args.no_pacing else repeats if args.quick else PACING_REPEATS,
                          'pacing_during_trials': args.quick,
                          'acceptance_updates_per_second': [58, 61]}}
    if args.into:
        # The published run's own record, with this one's as the idle cells'.
        published = json.loads((out / 'audit.json').read_text())
        if 'idle' in rerun:
            published.setdefault('protocol', {}).update(
                {k: audit['protocol'][k] for k in ('idle_repeats', 'idle_window_seconds', 'idle_requested_seconds')})
        published.update({'accepted': published['accepted'] and accepted, 'trials': len(rows),
                          f"{'_'.join(rerun)}_rerun": {k: audit[k] for k in ('commit', 'source_dirty', 'source_diff_sha256',
                                                                              'started_utc', 'finished_utc', 'environment',
                                                                              'preflight', 'binaries')}})
        audit = published
    (out / 'audit.json').write_text(json.dumps(audit, indent=1) + '\n')
    print(('ACCEPTED' if accepted else 'REJECTED'), out)


if __name__ == '__main__':
    main()
