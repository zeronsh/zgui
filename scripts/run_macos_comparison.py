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

`--powermetrics` also records package, CPU and GPU power with `sudo
powermetrics`, over the same seconds as the other meters. It needs sudo to run
`/usr/bin/powermetrics` without asking (a sudoers rule, or `sudo -v` first in
the same terminal); without it the columns stay empty. The suite turns it on
whenever that holds.

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
sys.path.insert(0, str(ROOT / 'scripts'))
import preflight  # noqa: E402
from staging import CWD, clean_env, stage  # noqa: E402
REQUESTED, WARMUP, SAMPLED, INTERVAL = 20, 5, 15, 0.05
# App lifetimes include readiness and sampler setup; samples start after
# readiness, so a slower framework's startup cannot shorten its sample.
LIFETIME_MARGIN = 3
# Idle is sampled 20-35 s after launch, five times: until the Metal driver
# gives back the memory it holds for any process that used the GPU lately
# (a ~180 MB floor, returned ~6 or ~8 s after launch for every framework, on
# a grid that follows the desktop's state rather than the app), a 5-20 s
# idle mean measures that timing more than the app. The 5-20 s figures stay
# in each idle trial's raw data, prefixed `early_`. See docs/benchmarking.md.
IDLE_SETTLE, IDLE_REPEATS = 20, 5
PACING_SECONDS, PACING_REPEATS = 8, 8
# The suite's protocol (`--suite`, scripts/run_macos_suite.py): shorter
# trials, fewer pacing captures and idle launches, and more repeats only for
# cells whose trials disagree. Checked against the full protocol's ranges on
# the heavy dashboard (docs/benchmarking.md).
# The full protocol's 5 s warm-up stays: from 3 s, startup's own work (first
# shaping, caches, memory still growing) read 5-10% more CPU for every
# framework (2026-10-25).
# Three pacing captures a cell: of two, one bad capture moved a median
# (lab scroll, 2026-10-25: 30.0 and 17.8 ms; ten more read 18.0). Idle six
# times, each round with its desktop alone: idle CPU reads hundredths of a
# percent, and WindowServer one of a few levels launch to launch for every
# app and the desktop alike (0.2 to 1.1% on an M5 Max), so two launches
# could not rank it. On an M1 Max, 8 rounds, no launch read 0.2 points over
# its round's desktop (2026-10-28); the site shows idle less the desktop.
# Six of every active cell too, each round with its desktop alone: WindowServer
# falls into per-launch levels while animating as well (about 17 or 20% for
# several frameworks, 2026-10-30), so three launches could not rank it either;
# six against their rounds' desktops read 0.1.5 and 0.1.6 within 0.06 pt of
# each other (window 40). The site shows WindowServer less the desktop.
SUITE = {'REQUESTED': 15, 'WARMUP': 5, 'SAMPLED': 10, 'REPEATS': 6, 'IDLE_REPEATS': 6, 'PACING_REPEATS': 3}
# Trials of each active cell outside the suite.
REPEATS = 3
# A cell whose CPU or GPU power spreads more than this share of its median
# over its trials gets EXTRA more (the suite).
SPREAD, EXTRA = 0.10, 1
# Blank-row captures (vlist): after the warm-up, this long, this often, in
# these modes (a fling scrolls as a scroll does; every capture so far read 0%).
BLANK_SECONDS, BLANK_RATE, BLANK_MODES = 8, 10, ['scroll', 'jump']
MODES = ['idle', 'stream', 'scroll', 'both']
MYGO = ROOT / 'comparisons/mygo/bin'
# Rune (anomalyco/rune, pinned and patched: comparisons/rune/PATCH.md).
RUNE = ROOT / 'comparisons/rune/bin'
SCENES = {
    'lab': {
        'modes': MODES,
        'binaries': {
            'zgx': ROOT / 'target/dist/examples/component_workload',
            'mygo': MYGO / 'lab',
            'rune': RUNE / 'lab',
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
            'mygo': MYGO / 'heavy',
            'rune': RUNE / 'heavy',
        },
    },
    'animate': {
        'modes': ['animate'],
        'binaries': {
            'zgx': ROOT / 'target/dist/examples/animation_workload',
            'gpui': ROOT / 'comparisons/gpui/target/dist/animate',
            'gpui-fast': ROOT / 'comparisons/gpui-fast/target/dist/animate',
            'mygo': MYGO / 'animate',
            'rune': RUNE / 'animate',
        },
    },
    # Variable-height rows in each framework's own list (zgx_workload::vlist).
    'vlist': {
        'modes': ['scroll', 'fling', 'jump'],
        'binaries': {
            'zgx': ROOT / 'target/dist/examples/vlist_workload',
            # The same list described in Luau: ui.list { measured = true }.
            'zgx-luau': ROOT / 'target/dist/examples/vlist_luau',
            'gpui-fast': ROOT / 'comparisons/gpui-fast/target/dist/vlist',
            'mygo': MYGO / 'vlist',
            'rune': RUNE / 'vlist',
        },
    },
    # A field of lit cubes drawn with wgpu (crates/zgx-desktop/examples/
    # support/cubes.rs): in a GPU view filling a zgx window, against the same
    # painter drawing straight into a window's surface with winit and wgpu
    # alone. What showing it in zgx costs over a minimal wgpu app; idle, the
    # view's first frame and nothing after.
    'viewport': {
        'modes': ['animate', 'idle'],
        'binaries': {
            'zgx': ROOT / 'target/dist/examples/viewport_workload',
            'wgpu': ROOT / 'target/dist/examples/wgpu_viewport',
            # The same app, its view composited into the window's frame (a
            # view not opaque or not shown whole), for the plane's gain.
            'zgx-composite': ROOT / 'target/dist/examples/viewport_composite',
        },
    },
    # A focused text field left alone, idle (zgx_workload::blink): each
    # framework's own field, its caret as it draws it (MyGo's blinks every
    # 530 ms, Rune's does not blink, zgx's blinks as the system's); GPUI has
    # no field, so its caret is a bar blinking as one would.
    'field': {
        'modes': ['idle'],
        'binaries': {
            'zgx': ROOT / 'target/dist/examples/field_workload',
            'gpui-fast': ROOT / 'comparisons/gpui-fast/target/dist/blink',
            'mygo': MYGO / 'field',
            'rune': RUNE / 'field',
        },
    },
}
# The suite's frameworks: zgx (in Rust, and in Luau where a scene has it),
# the stronger GPUI, MyGo and Rune.
SUITE_FRAMEWORKS = ['zgx', 'zgx-luau', 'gpui-fast', 'mygo', 'rune', 'wgpu', 'zgx-composite']
# Scenes whose active trials report the frames they drew (`animation_frames=`,
# `fps=`) rather than 60 updates a second.
FRAMES = {'animate', 'viewport'}
# Frameworks built as zgx-desktop examples beside zgx's (no checkout of their own).
EXAMPLES = {'wgpu', 'zgx-composite'}
SAMPLER, PACER, BLANK = Path('/tmp/zgx-macsample'), Path('/tmp/zgx-framepace'), Path('/tmp/zgx-blankprobe')
READY = Path('/tmp/zgx-window-ready')
# The one font file every adapter loads (comparisons/fonts), and how each
# reports the bytes it loaded (zgx_workload::fnv64).
FONT = ROOT / 'comparisons/fonts/DejaVuSans.ttf'


def fnv64(data):
    h = 0xcbf29ce484222325
    for byte in data:
        h = ((h ^ byte) * 0x100000001b3) & 0xffffffffffffffff
    return f'{h:016x}'


FONT_FNV64 = fnv64(FONT.read_bytes()) if FONT.exists() else None
# vlist: every framework's measured heights of the probe rows agree to this.
PROBE_TOLERANCE = 2.0
# What the binaries are built from, including vendored patches.
SOURCES = ['crates', 'comparisons', 'scripts', 'vendor', 'Cargo.toml', 'Cargo.lock']
METRICS = ['cpu_percent_one_core', 'cpu_watts', 'instructions', 'cycles', 'gpu_percent',
           'gpu_mcycles_per_second', 'gpu_active_mhz', 'gpu_system_watts',
           'windowserver_cpu_percent', 'windowserver_gpu_percent', 'gpu_utilization_percent',
           'mean_footprint_bytes', 'peak_footprint_bytes', 'mean_rss_bytes', 'peak_rss_bytes',
           'workload_ticks', 'wall_seconds']
POWER = ['package_mw', 'cpu_mw', 'gpu_mw']
FIELDS = ['framework', 'mode', 'repeat', 'ran_seconds', 'requested_seconds', 'ready_seconds', 'font_fnv64', 'wall_seconds', 'cpu_seconds', 'cpu_percent_one_core', 'cpu_energy_joules',
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
    if 'zgx-luau' in frameworks:
        luau = SCENES[scene]['binaries']['zgx-luau'].name
        run(['cargo', 'build', '--profile', 'dist', '--locked', '-p', 'zgx-script', '--example', luau], cwd=ROOT)
    for name in [f for f in frameworks if f in EXAMPLES]:
        run(['cargo', 'build', '--profile', 'dist', '--locked', '-p', 'zgx-desktop', '--example',
             SCENES[scene]['binaries'][name].name], cwd=ROOT)
    for name in [f for f in frameworks if f not in ('zgx', 'zgx-luau', 'mygo', 'rune', *EXAMPLES)]:
        run(['cargo', 'build', '--profile', 'dist', '--locked'], cwd=ROOT / 'comparisons' / name)
    if 'rune' in frameworks:
        # Rune at its pinned rev with the wheel patch (comparisons/rune/build.sh).
        run([str(ROOT / 'comparisons/rune/build.sh')], cwd=ROOT / 'comparisons/rune')
    if 'mygo' in frameworks:
        # Its release build (comparisons/mygo/build.sh: go build -trimpath, stripped).
        run([str(ROOT / 'comparisons/mygo/build.sh')], cwd=ROOT / 'comparisons/mygo')
    tools()


def tools():
    run(['swiftc', '-O', str(ROOT / 'scripts/macsample.swift'), '-o', str(SAMPLER), '-lIOReport'])
    run(['swiftc', '-O', str(ROOT / 'scripts/framepace.swift'), '-o', str(PACER)])
    run(['swiftc', '-O', str(ROOT / 'scripts/blankprobe.swift'), '-o', str(BLANK)])
    run(['swiftc', '-O', str(ROOT / 'scripts/window_ready.swift'), '-o', str(READY)])


def toolchains(frameworks):
    """How each framework's binaries were built, for the record."""
    found = {'rust': run(['rustc', '--version']), 'rust_profile': 'dist (release, lto = "fat", codegen-units = 1)'}
    if 'mygo' in frameworks:
        manifest = ROOT / 'comparisons/mygo/build.json'
        found['go'] = run(['go', 'version'])
        if manifest.exists():
            found['mygo'] = json.loads(manifest.read_text())
    if 'rune' in frameworks:
        manifest = ROOT / 'comparisons/rune/build.json'
        if manifest.exists():
            found['rune'] = json.loads(manifest.read_text())
    return found


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


class ReadinessFailure(RuntimeError):
    """A launch that can be tried once more, without counting a sample."""


def readiness_error(process, log, detail):
    attempt = len(list(log.parent.glob(log.stem + '.ready-error.*.json'))) + 1
    artifact = log.with_suffix(f'.ready-error.{attempt}.json')
    artifact.write_text(json.dumps(detail, indent=1) + '\n')
    if process.poll() is None:
        process.terminate()
        process.wait(timeout=10)
    if detail.get('dialogs'):
        raise SystemExit(f'system dialog on screen; not touching it: {detail["dialogs"]}')
    raise ReadinessFailure(f'{detail.get("error", "window not ready")}; inspect {artifact}')


def launch(binary, mode, seconds, log):
    # A dialog that appeared since the preflight would sit over this window:
    # stop rather than measure under it (never clicked).
    found = preflight.dialogs()
    if found:
        raise SystemExit('a system dialog is on screen; stopping (not clicking it): ' + ' | '.join(found))
    # The variables under their names from before the rename too: a build
    # from before it reads only those, and would otherwise run its default
    # length against the sampler's window (each trial checks the length).
    env = clean_env(ZGX_MODE=mode, ZGX_SECONDS=str(seconds),
                    ZGUI_MODE=mode, ZGUI_SECONDS=str(seconds))  # rename-to-zgx: keep
    if FONT.exists():
        # Staged beside the apps: nothing they read is under ~/Documents.
        env['ZGX_FONT_FILE'] = str(stage(FONT))
    with log.open('w') as handle:
        # A copy outside ~/Documents, run in /tmp (see scripts/staging.py).
        started = time.monotonic()
        process = subprocess.Popen([str(stage(binary))], stdout=handle, stderr=subprocess.STDOUT, env=env, cwd=CWD)
    process.benchmark_started = started
    # vlist's height probe may be headless (Rune); it is not a timed trial.
    if mode != 'probe':
        try:
            ready = json.loads(run([str(READY), str(process.pid), str(LIFETIME_MARGIN - 0.5)]))
        except (subprocess.CalledProcessError, json.JSONDecodeError) as error:
            failure = {'error': str(error), 'stdout': getattr(error, 'stdout', None),
                       'stderr': getattr(error, 'stderr', None)}
            try:
                decoded = json.loads(getattr(error, 'stdout', '') or '{}')
                if isinstance(decoded, dict):
                    failure.update(decoded)
            except json.JSONDecodeError:
                pass
            readiness_error(process, log, failure)
        if not ready.get('ready'):
            readiness_error(process, log, ready)
        process.benchmark_ready_seconds = time.monotonic() - started
        log.with_suffix('.ready.json').write_text(json.dumps({**ready, 'spawn_to_ready_seconds': process.benchmark_ready_seconds}, indent=1) + '\n')
        if process.benchmark_ready_seconds > LIFETIME_MARGIN - 0.5:
            readiness_error(process, log, {'error': 'readiness exceeded the sample lifetime budget',
                                          'ready_seconds': process.benchmark_ready_seconds})
    return process


def finish(process, seconds):
    try:
        return process.wait(timeout=seconds + 10), False
    except subprocess.TimeoutExpired:
        # macOS apps outlive their last window (QuickGUI closes it at the
        # deadline and reports): end it once it has reported.
        process.send_signal(signal.SIGTERM)
        return process.wait(timeout=10), True


def powermetrics(start):
    """Package, CPU and GPU power over the sampled window, if sudo allows:
    from `start` seconds after the window is ready, as the sampler's."""
    command = f'sleep {start}; exec sudo -n /usr/bin/powermetrics --samplers cpu_power,gpu_power -f plist -i 1000 -n {SAMPLED}'
    return subprocess.Popen(['/bin/sh', '-c', command], stdout=subprocess.PIPE, stderr=subprocess.DEVNULL)


def powermetrics_allowed():
    """Whether `sudo powermetrics` runs without asking (a sudoers rule for
    /usr/bin/powermetrics, or cached credentials)."""
    return subprocess.run(['sudo', '-n', '/usr/bin/powermetrics', '--samplers', 'cpu_power', '-i', '100', '-n', '1'],
                          stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL).returncode == 0


def power_of(process):
    output, _ = process.communicate()
    # One plist a sample, each ended by a NUL.
    samples = [plistlib.loads(doc) for doc in output.split(b'\0') if doc.strip()]
    processor = [s.get('processor', {}) for s in samples]
    mean = lambda key: statistics.mean(p[key] for p in processor) if processor and all(key in p for p in processor) else ''
    return {'package_mw': mean('combined_power'), 'cpu_mw': mean('cpu_power'), 'gpu_mw': mean('gpu_power')}


# Apps that exited abnormally (a signal, or no report and not ended by the
# runner): each launch's framework, scene, mode, what it was for and its exit
# code. GPUI Fast aborts now and then in AppKit while ScreenCaptureKit captures
# its window (-[NSWindow _updateButtonsForWindowSharingSession], 2026-10-25):
# such a launch is tried once more, and counted here, against the launches of
# each kind (`LAUNCHES`).
CRASHES = []
LAUNCHES = {}
READINESS_FAILURES = []


def abnormal(exit_code, terminated):
    return exit_code != 0 and not terminated


def counted(framework, kind):
    LAUNCHES.setdefault(framework, {}).setdefault(kind, 0)
    LAUNCHES[framework][kind] += 1


def crash(scene, framework, mode, kind, exit_code):
    CRASHES.append({'framework': framework, 'scene': scene, 'mode': mode, 'kind': kind, 'exit_code': exit_code})
    print(f"{framework} {mode}: exited abnormally ({exit_code}) in a {kind} launch; once more", flush=True)


def failed_readiness(scene, framework, mode, kind, attempt, error):
    READINESS_FAILURES.append(dict(scene=scene, framework=framework, mode=mode, kind=kind,
                                   attempt=attempt + 1, exhausted=bool(attempt), error=str(error)))
    print(f'{framework} {mode}: readiness failed ({error}); ' + ('skipping this sample' if attempt else 'once more'), flush=True)


def trial(scene, framework, mode, repeat, out, power, paced=None):
    """Try once more after an abnormal exit or a retryable readiness failure."""
    for attempt in range(2):
        if framework != 'desktop':
            counted(framework, 'trial')
        try:
            row = one_trial(scene, framework, mode, repeat, out, power, paced)
        except ReadinessFailure as error:
            failed_readiness(scene, framework, mode, 'trial', attempt, error)
            if attempt:
                raise
            time.sleep(2)
            continue
        if framework == 'desktop' or not abnormal(row['exit_code'], row['terminated_after_report']):
            return row
        crash(scene, framework, mode, 'trial', row['exit_code'])
        if attempt:
            return row
        time.sleep(2)


def one_trial(scene, framework, mode, repeat, out, power, paced=None):
    """One timed trial; with `paced` (a list), its frames' pacing too, appended there."""
    log = out / f'{framework}.{mode}.{repeat}.log'
    idle = mode == 'idle'
    requested = (IDLE_SETTLE + SAMPLED + 1 if idle else REQUESTED) + LIFETIME_MARGIN
    if framework == 'desktop':
        process = subprocess.Popen(['/bin/sleep', str(requested)])
    else:
        process = launch(SCENES[scene]['binaries'][framework], mode, requested, log)
    launched = getattr(process, 'benchmark_started', time.monotonic())
    # Idle: the 5-20 s window too, alongside the settled one, for the record.
    early = subprocess.Popen([str(SAMPLER), str(process.pid), str(WARMUP), str(SAMPLED), str(INTERVAL)],
                             stdout=subprocess.PIPE, text=True) if idle else None
    start = IDLE_SETTLE if idle else WARMUP
    meter = powermetrics(start) if power else None
    pacer = None
    if paced is not None and framework != 'desktop' and mode != 'idle':
        # As long as a pacing trial's capture: one running to the app's own
        # end loses its stream as the window closes.
        pacer = subprocess.Popen([str(PACER), str(process.pid), str(WARMUP), str(PACING_SECONDS)],
                                 stdout=subprocess.PIPE, stderr=subprocess.DEVNULL, text=True)
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
    # How long the app ran: it should end as asked (an app that read
    # another length would be sampled over the wrong part of its run).
    ran = time.monotonic() - launched
    text = log.read_text() if log.exists() else ''
    ticks = re.search(r'"ticks":(\d+)|workload_ticks=(\d+)|animation_frames=(\d+)', text)
    fps = re.search(r'fps=([0-9.]+)', text)
    font = re.search(r'font_file_fnv64=([0-9a-f]{16})', text)
    row = {'framework': framework, 'mode': mode, 'repeat': repeat, **sample,
           'workload_ticks': int(next(g for g in ticks.groups() if g)) if ticks else (0 if framework == 'desktop' else -1),
           'reported_fps': float(fps.group(1)) if fps else '',
           **(power_of(meter) if meter else dict.fromkeys(POWER, '')),
           'exit_code': exit_code, 'terminated_after_report': terminated,
           'ran_seconds': round(ran, 2), 'requested_seconds': requested,
           'ready_seconds': round(getattr(process, 'benchmark_ready_seconds', 0), 4),
           'font_fnv64': font.group(1) if font else ''}
    (out / f'{framework}.{mode}.{repeat}.json').write_text(json.dumps(row, indent=1) + '\n')
    return row


def valid(row, scene):
    """Whether a trial ran as asked: as long as requested (within the launch's
    and exit's slack), and its ticks what that length makes (60 a second
    while active, none while idle; animate: frames at a display's rate)."""
    if row['framework'] == 'desktop':
        return True
    if not row['exit_code'] == 0 and not row['terminated_after_report']:
        return False
    if abs(row['ran_seconds'] - row['requested_seconds']) > 3:
        return False
    if 'wall_seconds' in row and abs(row['wall_seconds'] - SAMPLED) > 0.5:
        return False
    # The suite's font: the bundled file, as the app reported loading it.
    if FONT_FNV64 and 'font_fnv64' in row and row['font_fnv64'] != FONT_FNV64:
        return False
    ticks, seconds = row['workload_ticks'], row['requested_seconds']
    if scene in FRAMES and row['mode'] != 'idle':
        return ticks > 0 and row['reported_fps'] not in ('', None) and abs(ticks / seconds - row['reported_fps']) <= 0.15 * row['reported_fps']
    if row['mode'] == 'idle':
        return ticks == 0
    return 58 <= ticks / seconds <= 61


def pacing(scene, framework, mode, repeat, out):
    seconds = WARMUP + PACING_SECONDS + 1 + LIFETIME_MARGIN
    # Screen capture occasionally fails to find a window that just opened:
    # one more try before giving up on the trial.
    # An app that exits abnormally while captured gets one more launch too.
    for attempt in range(2):
        counted(framework, 'pacing')
        try:
            process = launch(SCENES[scene]['binaries'][framework], mode, seconds, out / f'{framework}.{mode}.{repeat}.pacing.log')
        except ReadinessFailure as error:
            failed_readiness(scene, framework, mode, 'pacing', attempt, error)
            if attempt:
                raise
            time.sleep(2)
            continue
        try:
            result = json.loads(run([str(PACER), str(process.pid), str(WARMUP), str(PACING_SECONDS)]))
        except subprocess.CalledProcessError as error:
            # Keep the probe's JSON error for each attempt, including the
            # first one: the exception's string only names the command.
            failure = {'framework': framework, 'mode': mode, 'repeat': repeat,
                       'attempt': attempt + 1, 'command': error.cmd,
                       'returncode': error.returncode, 'stdout': error.stdout,
                       'stderr': error.stderr}
            (out / f'{framework}.{mode}.{repeat}.pacing-error.{attempt + 1}.json').write_text(
                json.dumps(failure, indent=1) + '\n')
            print(f'{framework} {mode}: frame pacing failed: '
                  f'{error.stdout or error.stderr or error}', flush=True)
            exit_code, terminated = finish(process, seconds)
            if abnormal(exit_code, terminated):
                crash(scene, framework, mode, 'pacing', exit_code)
            if attempt:
                raise
            time.sleep(2)
            continue
        exit_code, terminated = finish(process, seconds)
        if not abnormal(exit_code, terminated):
            break
        crash(scene, framework, mode, 'pacing', exit_code)
        if attempt:
            raise SystemExit(f'{framework} {mode}: exited abnormally in two pacing launches')
        time.sleep(2)
    row = {'framework': framework, 'mode': mode, 'repeat': repeat, **result}
    (out / f'{framework}.{mode}.{repeat}.pacing.json').write_text(json.dumps(row, indent=1) + '\n')
    return row


def blank(scene, framework, mode, repeat, out):
    """How often the window shows unfilled rows (vlist), from captures of it
    (scripts/blankprobe.swift), in a launch of its own."""
    seconds = WARMUP + BLANK_SECONDS + 1 + LIFETIME_MARGIN
    for attempt in range(2):
        counted(framework, 'blank')
        try:
            process = launch(SCENES[scene]['binaries'][framework], mode, seconds, out / f'{framework}.{mode}.{repeat}.blank.log')
        except ReadinessFailure as error:
            failed_readiness(scene, framework, mode, 'blank', attempt, error)
            if attempt:
                raise
            time.sleep(2)
            continue
        try:
            result = json.loads(run([str(BLANK), str(process.pid), str(WARMUP), str(BLANK_SECONDS), str(BLANK_RATE)]))
        except (subprocess.CalledProcessError, json.JSONDecodeError) as error:
            result = {'error': str(error)[-300:]}
        exit_code, terminated = finish(process, seconds)
        if not abnormal(exit_code, terminated):
            break
        crash(scene, framework, mode, 'blank', exit_code)
        result = {'error': f'exited abnormally ({exit_code})'}
        time.sleep(2)
    row = {'framework': framework, 'mode': mode, 'repeat': repeat, **result}
    (out / f'{framework}.{mode}.{repeat}.blank.json').write_text(json.dumps(row, indent=1) + '\n')
    return row


def probe(scene, frameworks, out):
    """vlist, before any trial counts: every framework's measured heights of
    the probe rows (ZGX_MODE=probe), which must agree within PROBE_TOLERANCE
    points: the same rows wrapped the same way, in the same font. Stops the
    run with the table when they do not."""
    found = {}
    for framework in frameworks:
        log = out / f'{framework}.probe.log'
        process = launch(SCENES[scene]['binaries'][framework], 'probe', 30, log)
        finish(process, 30)
        text = log.read_text() if log.exists() else ''
        heights = re.search(r'row_heights=(\S+)', text)
        widths = dict(re.findall(r'(list_width|text_width)=([0-9.]+)', text))
        font = re.search(r'font_file_fnv64=([0-9a-f]{16})', text)
        found[framework] = {
            'heights': {int(r): float(h) for r, h in (pair.split(':') for pair in heights.group(1).split(','))} if heights else {},
            **{k: float(v) for k, v in widths.items()}, 'font_fnv64': font.group(1) if font else ''}
    rows = sorted(set().union(*(f['heights'] for f in found.values())))
    disagree = [r for r in rows if len({f['heights'].get(r) for f in found.values()}) and
                (None in {f['heights'].get(r) for f in found.values()} or
                 max(f['heights'][r] for f in found.values()) - min(f['heights'][r] for f in found.values()) > PROBE_TOLERANCE)]
    fonts = [f for f, v in found.items() if FONT_FNV64 and v['font_fnv64'] != FONT_FNV64]
    report = {'tolerance_points': PROBE_TOLERANCE, 'frameworks': found, 'disagreeing_rows': disagree, 'wrong_font': fonts}
    (out / 'probe.json').write_text(json.dumps(report, indent=1) + '\n')
    print('probe: row ' + ' '.join(f'{f:>9}' for f in found), flush=True)
    for r in rows:
        print(f'probe: {r:5d} ' + ' '.join(f"{found[f]['heights'].get(r, float('nan')):9.1f}" for f in found)
              + ('  DISAGREE' if r in disagree else ''), flush=True)
    if disagree or fonts or not rows:
        raise SystemExit(f'probe: frameworks disagree on rows {disagree} or fonts {fonts}: not measuring vlist')


def spread_of(rows, framework, mode, key):
    values = [float(r[key]) for r in rows if (r['framework'], r['mode']) == (framework, mode) and r.get(key) not in ('', None)]
    if len(values) < 2 or statistics.median(values) <= 0:
        return 0.
    return (max(values) - min(values)) / statistics.median(values)


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
        writer = csv.DictWriter(handle, fieldnames=fields, extrasaction='ignore', lineterminator='\n')
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
    parser.add_argument('--suite', action='store_true',
                        help="the suite's protocol: zgx, GPUI Fast and MyGo, short trials, extra repeats where they disagree")
    parser.add_argument('--out', type=Path, help='where to write the run (default docs/results/latest-macos…)')
    parser.add_argument('--into', type=Path,
                        help="a published run of the scene: replace its cells of --modes with this run's")
    args = parser.parse_args()
    frameworks, modes = list(SCENES[args.scene]['binaries']), SCENES[args.scene]['modes']
    if args.suite:
        use_suite()
        # The field is nothing but idle, whose memory is two-level launch to
        # launch (the driver returns its pool or keeps part of it): five.
        if args.scene == 'field':
            globals()['IDLE_REPEATS'] = 5
        frameworks = [f for f in SUITE_FRAMEWORKS if f in SCENES[args.scene]['binaries']]
        # Idle once, in heavy: it is the verdict's idle CPU (and in the
        # field, which is nothing else).
        if args.scene not in ('heavy', 'field', 'viewport'):
            modes = [m for m in modes if m != 'idle']
    if not args.quickgui:
        frameworks = [f for f in frameworks if f != 'quickgui']
    if not args.suite and not args.frameworks:
        # MyGo and Rune are the suite's; the full protocol's runs keep their frameworks.
        frameworks = [f for f in frameworks if f not in ('mygo', 'rune')]
    if args.quick:
        frameworks = [f for f in frameworks if f in ('zgx', 'gpui-fast')]
    if args.frameworks:
        frameworks = [f for f in args.frameworks.split(',') if f in SCENES[args.scene]['binaries']]
    if args.modes:
        modes = [m for m in modes if m in args.modes.split(',')]
    assert not args.into or args.modes, '--into replaces the cells of --modes'
    repeats = args.repeats or (2 if args.quick else REPEATS)
    if args.pgo:
        # The supplement: each framework's apps built with a profile of their own runs.
        for framework, path in SCENES[args.scene]['binaries'].items():
            SCENES[args.scene]['binaries'][framework] = ROOT / 'target/pgo-compare' / framework / path.name
        missing = [f for f in frameworks if not SCENES[args.scene]['binaries'][f].exists()]
        assert not missing, f'no PGO build of {missing}: run scripts/perf/pgo_compare.py'
        args.skip_build = True
        tools()
    if not args.skip_build:
        build(args.scene, frameworks)
    elif not BLANK.exists() or not READY.exists():
        tools()
    commit = run(['git', 'rev-parse', 'HEAD'], cwd=ROOT)
    found = None if args.no_preflight else preflight.wait_until_quiet(
        log=lambda line: print(line, flush=True), threshold=args.preflight_threshold)
    hidden = hide(args.hide) if args.hide else False
    try:
        if not args.no_preflight:
            preflight.desktop_still()
        measure(args, frameworks, modes, repeats, commit, found)
    finally:
        if hidden:
            show(args.hide)


def use_suite():
    """The suite's protocol, for this run."""
    globals().update(SUITE)


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
    out = args.into or args.out or ROOT / 'docs/results' / f'latest-macos{scene}{tag}-{started:%Y-%m-%d}-{commit[:7]}'
    out.mkdir(parents=True, exist_ok=True)
    rows, paced = [], []
    idle_repeats = min(IDLE_REPEATS, repeats) if args.quick else IDLE_REPEATS
    extended = []
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
    if args.scene == 'vlist' and not args.into:
        probe(args.scene, frameworks, out)
    for repeat in range(max(repeats, idle_repeats) if 'idle' in modes else repeats):
        order = frameworks[repeat % len(frameworks):] + frameworks[:repeat % len(frameworks)]
        chosen = [m for m in modes if repeat < (idle_repeats if m == 'idle' else repeats)]
        for framework, mode in [('desktop', 'baseline')] + [(f, m) for m in chosen for f in order]:
            # Into a published run: its own desktop baselines stay, this run's apart.
            place = out / 'idle-rerun-baselines' if args.into and framework == 'desktop' else out
            place.mkdir(exist_ok=True)
            try:
                row = trial(args.scene, framework, mode, repeat, place, args.powermetrics, paced if args.quick else None)
            except ReadinessFailure:
                continue
            print(f"{framework:8} {mode:8} {repeat}: cpu {row['cpu_percent_one_core']:6.2f}% {row['cpu_watts'] * 1000:6.0f} mW "
                  f"gpu {row['gpu_system_watts'] * 1000:5.1f} mW {row['gpu_mcycles_per_second']:5.1f} Mcyc/s server cpu {row['windowserver_cpu_percent']:5.1f}% gpu {row['windowserver_gpu_percent']:5.1f}% "
                  f"footprint {row['mean_footprint_bytes'] / 2**20:6.1f} MiB ticks {row['workload_ticks']}", flush=True)
            rows.append(row)
            time.sleep(2)
    if args.suite:
        # Cells whose headline metrics (CPU, GPU power) disagree get more
        # trials, alternating with the cell's own (so a slow drift does not
        # pick one side).
        wide = [(f, m) for m in modes if m != 'idle' for f in frameworks
                if max(spread_of(rows, f, m, 'cpu_percent_one_core'), spread_of(rows, f, m, 'gpu_system_watts')) > SPREAD]
        for extra in range(EXTRA):
            for framework, mode in wide:
                try:
                    row = trial(args.scene, framework, mode, repeats + extra, out, args.powermetrics)
                except ReadinessFailure:
                    continue
                print(f"{framework:8} {mode:8} {repeats + extra}: cpu {row['cpu_percent_one_core']:6.2f}% "
                      f"gpu {row['gpu_system_watts'] * 1000:5.1f} mW (extra: wide spread)", flush=True)
                rows.append(row)
                time.sleep(2)
        extended = [f'{f}/{m}' for f, m in wide]
    blanks = []
    if args.scene == 'vlist' and not args.no_pacing:
        for framework in frameworks:
            for mode in [m for m in modes if m in BLANK_MODES]:
                try:
                    row = blank(args.scene, framework, mode, 0, out)
                except ReadinessFailure:
                    continue
                print(f"{framework:8} {mode:8}: blank {row.get('blank_captures', '?')}/{row.get('captures', '?')} captures, "
                      f"longest {row.get('longest_blank_points', '?')} pt", flush=True)
                blanks.append(row)
                time.sleep(2)
    for repeat in range(0 if args.quick or args.no_pacing else PACING_REPEATS):
        order = frameworks[repeat % len(frameworks):] + frameworks[:repeat % len(frameworks)]
        for mode in [m for m in modes if m != 'idle']:
            for framework in order:
                try:
                    row = pacing(args.scene, framework, mode, repeat, out)
                except ReadinessFailure:
                    continue
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
    if blanks:
        write_csv(out / 'blank.csv', ['framework', 'mode', 'repeat', 'captures', 'blank_captures', 'blank_fraction',
                                      'longest_blank_points', 'error'], blanks)
    if not args.into:
        write_csv(out / 'pacing.csv', ['framework', 'mode', 'repeat', 'display_hz', 'frames', *PACING, 'long_intervals'], paced)
    groups = [group(rows, METRICS + POWER, 'desktop', 'baseline')]
    groups += [group(rows, METRICS + POWER + ['reported_fps'], f, m) for f in frameworks for m in modes]
    pacing_groups = [group(paced, PACING, f, m) for f in frameworks for m in modes if m != 'idle']
    if 'idle' in rerun:
        # What the idle cells read over 5-20 s, as runs before October 24 sampled them.
        early = [group(rows, [f'early_{k}' for k in METRICS], f, 'idle') for f in frameworks]
        (out / 'idle-5-20s.json').write_text(json.dumps({'groups': early}, indent=1) + '\n')
    blank_groups = [group(blanks, ['blank_fraction', 'longest_blank_points'], f, m) for f in frameworks for m in modes] if blanks else []
    (out / 'summary.json').write_text(json.dumps({'groups': groups, 'pacing': pacing_groups,
                                                  **({'blank': blank_groups} if blanks else {})}, indent=1) + '\n')
    apps = [r for r in rows if r['framework'] != 'desktop']
    if args.scene in FRAMES:
        active = [r['reported_fps'] for r in apps if r['reported_fps'] != '' and r['mode'] != 'idle']
        gated = (all(r['workload_ticks'] > 0 for r in apps if r['mode'] != 'idle')
                 and all(r['workload_ticks'] == 0 for r in apps if r['mode'] == 'idle'))
    else:
        active = [r['workload_ticks'] / float(r.get('requested_seconds') or REQUESTED) for r in apps if r['mode'] != 'idle']
        gated = (all(58 <= t <= 61 for t in active)
                 and all(r['workload_ticks'] == 0 for r in apps if r['mode'] == 'idle'))
    accepted = gated and all(r['exit_code'] == 0 or r['terminated_after_report'] for r in apps)
    # Every trial as long as asked, its ticks what that length makes.
    invalid = [f"{r['framework']}/{r['mode']}/{r['repeat']}" for r in apps if 'ran_seconds' in r and not valid(r, args.scene)]
    accepted = accepted and not invalid and not any(r['exhausted'] for r in READINESS_FAILURES)
    binaries = {name: {'sha256': hashlib.sha256(path.read_bytes()).hexdigest(), 'bytes': path.stat().st_size}
                for name, path in SCENES[args.scene]['binaries'].items() if name in frameworks}
    audit = {'accepted': accepted, 'trials': len(rows), 'samples': sum(r['samples'] for r in rows),
             'pacing_trials': len(paced),
             ('reported_fps' if args.scene in FRAMES else 'active_ticks_per_requested_second'):
                 {'min': min(active), 'max': max(active)} if active else None,
             'powermetrics': any(r.get('package_mw') not in ('', None) for r in rows),
             'scene': args.scene, 'commit': commit,
             # Uncommitted source changes the binaries were built with, if any.
             'source_dirty': bool(run(['git', 'status', '--porcelain', '--', *SOURCES], cwd=ROOT)),
             'source_diff_sha256': source_diff_sha256(),
             'started_utc': started.isoformat(), 'finished_utc': datetime.datetime.now(datetime.timezone.utc).isoformat(),
             'environment': environment(), 'preflight': found, 'binaries': binaries,
             'toolchains': toolchains(frameworks), 'invalid_trials': invalid,
             'abnormal_exits': CRASHES, 'launches': LAUNCHES,
             'readiness_failures': READINESS_FAILURES,
             'protocol': {'requested_seconds': REQUESTED + LIFETIME_MARGIN, 'warmup_seconds': WARMUP, 'sampled_seconds': SAMPLED,
                          'sampling_anchor': 'window frontmost and uncovered', 'readiness_timeout_seconds': LIFETIME_MARGIN - 0.5,
                          'lifetime_margin_seconds': LIFETIME_MARGIN,
                          'interval_seconds': INTERVAL, 'repeats': repeats, 'idle_repeats': idle_repeats,
                          'idle_window_seconds': [IDLE_SETTLE, IDLE_SETTLE + SAMPLED],
                          'idle_requested_seconds': IDLE_SETTLE + SAMPLED + 1 + LIFETIME_MARGIN,
                          'quick': args.quick, 'pgo': args.pgo, 'frameworks': frameworks, 'modes': modes,
                          'pacing_seconds': PACING_SECONDS,
                          'pacing_repeats': 0 if args.no_pacing else repeats if args.quick else PACING_REPEATS,
                          'pacing_during_trials': args.quick,
                          'acceptance_updates_per_second': [58, 61],
                          'suite': args.suite, 'spread_for_extra_trials': SPREAD if args.suite else None,
                          'extra_trials': EXTRA if args.suite else 0, 'extended_cells': extended,
                          **({'blank_seconds': BLANK_SECONDS, 'blank_rate': BLANK_RATE, 'blank_modes': BLANK_MODES} if blanks else {})}}
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
