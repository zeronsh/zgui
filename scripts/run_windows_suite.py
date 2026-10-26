#!/usr/bin/env python3
"""The Windows comparison suite in one run: the macOS suite's scenes and
protocol (scripts/run_macos_suite.py, run_macos_comparison.py --suite),
measured with Windows' own counters, into
docs/results/suite-windows-<date>-<commit>/<scene>/ in the macOS suite's
files (current.csv, pacing.csv, summary.json, audit.json; manifest.json at
the top), which `scripts/build_site.py --platform windows` publishes.

    cargo build --profile dist --locked -p zgx-desktop --example component_workload --example heavy_workload --example animation_workload --example vlist_workload
    cargo build --profile dist --locked -p zgx-script --example vlist_luau
    cargo build --profile dist --locked --manifest-path comparisons/gpui-fast/Cargo.toml
    (cd comparisons/mygo && CGO_ENABLED=0 go build -trimpath -ldflags="-s -w" -o bin/<scene>.exe ./cmd/<scene>)   (heavy, lab, animate, vlist)
    (cd comparisons/rune && <build.sh's checkout and patch> && cargo build --profile dist --locked --bins)   (copy dist/<scene>.exe into bin/)
    cargo build --release -p zgx-gpu --example framepace
    python scripts/run_windows_suite.py

Scenes and modes as the macOS suite's: heavy (idle, stream, scroll, both;
idle only here), lab (stream, scroll, both), animate, vlist (scroll, fling,
jump; zgx through Luau too). Each trial is a fresh launch: its window brought
to the front and found uncovered (readiness), the first 5 s after that
discarded and the next 10 s sampled every 100 ms (idle: 20-30 s, the 5-15 s
figures kept as `early_*`); 3 trials of each cell in rotating order, a desktop
baseline (nothing launched) each round, idle twice, and one more trial of any
cell whose CPU or app GPU time spreads past 10% of its median. Then 3 frame
pacing captures of each cell, each in a launch of its own (Desktop
Duplication: `framepace`, scripts/windows_framepace.py).

What a trial reads, over its sample: the app's CPU time (`GetProcessTimes`)
and cycles (`QueryProcessCycleTime`; per frame or tick too), its private
bytes and working set; through performance counters, its GPU time on the 3D
engines (and on every engine), the compositor's (`dwm.exe`) CPU and GPU time;
the NVIDIA board's energy (NVML's total energy counter, read at the start
and end: no polling), when there is one; and the processors' clock against
nominal. A trial counts only if it ran as long as asked at 60 updates a
second (animate: its frames at its reported rate), loaded the suite's font,
and its window was uncovered as its sample began and ended.

Before measuring, it waits until no build runs and the rest of the machine
uses under 75% of a core, holds the High performance power plan (the user's
restored after) and minimizes Zeron's windows (restored after). Startup (the
lab scene's binaries, scripts/windows_startup_probe.py) goes in startup/.
"""
import argparse
import csv
import ctypes
import ctypes.wintypes
import datetime
import hashlib
import json
import os
import platform
import re
import statistics
import subprocess
import sys
import tempfile
import time
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / 'scripts'))
import windows_compare as compare  # noqa: E402
import windows_framepace as framepace  # noqa: E402
import windows_input as win  # noqa: E402
import windows_pdh as pdh  # noqa: E402
import windows_startup_probe as startup  # noqa: E402

# The macOS suite's protocol (run_macos_comparison.SUITE), sampled every
# 100 ms: CPU, cycles and the counters are read at a sample's two ends, so
# the interval only sets memory's mean.
REQUESTED, WARMUP, SAMPLED, INTERVAL = 15, 5, 10, 0.1
LIFETIME_MARGIN = 3
IDLE_SETTLE, IDLE_REPEATS = 20, 2
PACING_SECONDS, PACING_REPEATS = 8, 3
REPEATS = 3
SPREAD, EXTRA = 0.10, 1
PROBE_TOLERANCE = 2.0
STARTUP_LAUNCHES = 10
FRAMEWORKS = ['zgx', 'zgx-luau', 'gpui-fast', 'mygo', 'rune']
# Scene: modes, and each framework's binary.
SCENES = {
    'heavy': (['idle', 'stream', 'scroll', 'both'],
              {'zgx': 'heavy_workload', 'gpui-fast': 'heavy', 'mygo': 'heavy', 'rune': 'heavy'}),
    'lab': (['stream', 'scroll', 'both'],
            {'zgx': 'component_workload', 'gpui-fast': 'zgx-compare-gpui-fast', 'mygo': 'lab', 'rune': 'lab'}),
    'animate': (['animate'],
                {'zgx': 'animation_workload', 'gpui-fast': 'animate', 'mygo': 'animate', 'rune': 'animate'}),
    'vlist': (['scroll', 'fling', 'jump'],
              {'zgx': 'vlist_workload', 'zgx-luau': 'vlist_luau', 'gpui-fast': 'vlist', 'mygo': 'vlist',
               'rune': 'vlist'}),
}
FONT = ROOT / 'comparisons/fonts/DejaVuSans.ttf'
SOURCES = ['crates', 'comparisons', 'scripts', 'vendor', 'Cargo.toml', 'Cargo.lock']
# The checkout the binaries were built from (--source): its commit, and
# whether it had uncommitted changes, go into each scene's audit.
SOURCE = ROOT
# The Go toolchain MyGo was built with (--go).
GO = 'go'
METRICS = ['cpu_percent_one_core', 'mcycles_per_second', 'mcycles_per_frame', 'gpu_ms_per_second',
           'gpu_all_engines_ms_per_second', 'dwm_cpu_percent', 'dwm_gpu_ms_per_second', 'gpu_board_watts',
           'mean_private_bytes', 'peak_private_bytes', 'mean_working_set_bytes', 'peak_working_set_bytes',
           'processor_performance_percent', 'workload_ticks', 'wall_seconds']
FIELDS = ['framework', 'mode', 'repeat', 'ran_seconds', 'requested_seconds', 'ready_seconds', 'font_fnv64',
          'wall_seconds', 'cpu_seconds', 'cpu_percent_one_core', 'cycles', 'mcycles_per_second', 'mcycles_per_frame',
          'gpu_ms_per_second', 'gpu_all_engines_ms_per_second', 'dwm_cpu_percent', 'dwm_gpu_ms_per_second',
          'gpu_board_watts', 'mean_private_bytes', 'peak_private_bytes', 'mean_working_set_bytes',
          'peak_working_set_bytes', 'processor_performance_percent', 'samples', 'workload_ticks', 'reported_fps',
          'dpi', 'uia_client_attached', 'exit_code', 'terminated_after_report', 'early_mean_private_bytes',
          'early_cpu_percent_one_core', 'early_mcycles_per_second', 'graphics_modules']
GRAPHICS_MODULES = {'d3d11.dll', 'd3d12.dll', 'vulkan-1.dll', 'opengl32.dll'}
PACING = ['frames_per_second', 'interval_p50_ms', 'interval_p95_ms', 'interval_p99_ms', 'interval_max_ms',
          'long_interval_fraction']
# Processes whose presence means the machine is not quiet.
HEAVY = ('cargo', 'rustc', 'link', 'cl', 'go', 'msbuild', 'devenv', 'TiWorker')


def fnv64(data):
    h = 0xcbf29ce484222325
    for byte in data:
        h = ((h ^ byte) * 0x100000001b3) & 0xffffffffffffffff
    return f'{h:016x}'


FONT_FNV64 = fnv64(FONT.read_bytes()) if FONT.exists() else None


def run(cmd, **kw):
    return subprocess.run(cmd, check=True, capture_output=True, text=True, **kw).stdout.strip()


class Board:
    """The NVIDIA board's energy since the driver loaded (NVML), read at a
    sample's ends: its mean power without polling. None without NVML."""

    def __init__(self):
        self.device = None
        try:
            self.nvml = ctypes.WinDLL(r'C:\Windows\System32\nvml.dll')
        except OSError:
            return
        device = ctypes.c_void_p()
        if self.nvml.nvmlInit_v2() == 0 and self.nvml.nvmlDeviceGetHandleByIndex_v2(0, ctypes.byref(device)) == 0:
            self.device = device

    def joules(self):
        if self.device is None:
            return None
        energy = ctypes.c_ulonglong()
        if self.nvml.nvmlDeviceGetTotalEnergyConsumption(self.device, ctypes.byref(energy)):
            return None
        return energy.value / 1000


BOARD = Board()


class ReadinessFailure(RuntimeError):
    """A launch that can be tried once more, without counting a trial."""


CRASHES, LAUNCHES, READINESS_FAILURES = [], {}, []


def counted(framework, kind):
    LAUNCHES.setdefault(framework, {}).setdefault(kind, 0)
    LAUNCHES[framework][kind] += 1


def environment_for(binary, mode, seconds):
    env = dict(os.environ, ZGX_MODE=mode, ZGX_SECONDS=str(seconds), ZGX_INITIAL_TICKS='0',
               ZGUI_MODE=mode, ZGUI_SECONDS=str(seconds), ZGUI_INITIAL_TICKS='0',  # rename-to-zgx: keep
               ZGX_ACCESSIBILITY_LOG='1')
    if FONT.exists():
        env['ZGX_FONT_FILE'] = env['ZGUI_FONT_FILE'] = str(FONT)
    return env


def launch(binary, mode, seconds, log):
    """The app, started with its window in front and uncovered (or a
    ReadinessFailure within the lifetime margin's budget)."""
    started = time.perf_counter()
    with log.open('w') as handle:
        process = subprocess.Popen([str(binary)], stdout=handle, stderr=subprocess.STDOUT,
                                   env=environment_for(binary, mode, seconds), cwd=tempfile.gettempdir())
    process.started = started
    if mode == 'probe':
        return process
    budget = LIFETIME_MARGIN - 0.5
    try:
        hwnd = win.wait_window(process, timeout=budget)
        win.activate(hwnd)
        while not compare.uncovered(hwnd):
            if time.perf_counter() - started > budget:
                raise RuntimeError('window covered')
            time.sleep(0.05)
    except RuntimeError as error:
        if process.poll() is None:
            process.kill()
            process.wait()
        raise ReadinessFailure(f'{binary.name} {mode}: {error}') from None
    process.hwnd, process.ready = hwnd, time.perf_counter() - started
    log.with_suffix('.ready.json').write_text(json.dumps({'ready': True, 'spawn_to_ready_seconds': process.ready,
                                                          'dpi': win.dpi(hwnd)}, indent=1) + '\n', newline='\n')
    return process


def finish(process, seconds, log):
    """The app's exit code, and whether it was ended after reporting."""
    try:
        return process.wait(timeout=max(1, seconds + 10 - (time.perf_counter() - process.started))), False
    except subprocess.TimeoutExpired:
        process.kill()
        reported = bool(re.search(r'"ticks":|workload_ticks=|animation_frames=', log.read_text(errors='replace')))
        return process.wait(), reported


def abnormal(exit_code, terminated):
    return exit_code != 0 and not terminated


def sample(process, start, seconds, counters, early=None):
    """CPU, cycles, memory and the counters over [start, start + seconds]
    after readiness: their figures over the span."""
    def at(offset):
        delay = process.started + process.ready + offset - time.perf_counter()
        if delay > 0:
            time.sleep(delay)

    spans = [(start, seconds)] + ([early] if early else [])
    spans.sort()
    found = {}
    for begin, length in spans:
        at(begin)
        clock = pdh.Rate(pdh.processor_performance())
        first = (time.perf_counter(), win.cpu_seconds(process.pid), win.cycles(process.pid), counters.sample(),
                 BOARD.joules())
        uncovered_first = compare.uncovered(process.hwnd)
        memory, clocks = [], []
        while time.perf_counter() < process.started + process.ready + begin + length:
            try:
                memory.append(win.memory(process.pid))
            except OSError:
                pass
            clocks.append(clock.sample())
            time.sleep(INTERVAL)
        last = (time.perf_counter(), win.cpu_seconds(process.pid), win.cycles(process.pid), counters.sample(),
                BOARD.joules())
        clock.close()
        uncovered_last = compare.uncovered(process.hwnd)
        wall = last[0] - first[0]
        dwm_cpu, dwm_gpu, app_gpu, app_all = ((b - a) / 1e7 for a, b in zip(first[3], last[3]))
        memory = [m for m in memory if m[0] > 0]
        clocks = [c for c in clocks if c is not None]
        found[begin] = {
            'wall_seconds': round(wall, 4),
            'cpu_seconds': round(last[1] - first[1], 4),
            'cpu_percent_one_core': 100 * (last[1] - first[1]) / wall,
            'cycles': last[2] - first[2],
            'mcycles_per_second': (last[2] - first[2]) / wall / 1e6,
            'gpu_ms_per_second': 1000 * app_gpu / wall,
            'gpu_all_engines_ms_per_second': 1000 * app_all / wall,
            'dwm_cpu_percent': 100 * dwm_cpu / wall,
            'dwm_gpu_ms_per_second': 1000 * dwm_gpu / wall,
            'gpu_board_watts': (last[4] - first[4]) / wall if first[4] is not None and last[4] is not None else '',
            'mean_private_bytes': round(statistics.mean(m[0] for m in memory)) if memory else 0,
            'peak_private_bytes': max((m[0] for m in memory), default=0),
            'mean_working_set_bytes': round(statistics.mean(m[1] for m in memory)) if memory else 0,
            'peak_working_set_bytes': max((m[1] for m in memory), default=0),
            'processor_performance_percent': statistics.mean(clocks) if clocks else '',
            'samples': len(memory),
            'uncovered': uncovered_first and uncovered_last,
        }
    return found[start], (found[early[0]] if early else None)


def desktop_trial(repeat, out):
    """The desktop with no app launched, sampled as a trial is."""
    dwm = pdh.pid_of('dwm.exe')
    with pdh.Counters([pdh.dwm_cpu(), pdh.gpu_time(dwm)]) as counters:
        time.sleep(WARMUP)
        first, board, start = counters.sample(), BOARD.joules(), time.perf_counter()
        time.sleep(SAMPLED)
        last, board_last, wall = counters.sample(), BOARD.joules(), time.perf_counter() - start
    cpu, gpu = ((b - a) / 1e7 for a, b in zip(first, last))
    row = {'framework': 'desktop', 'mode': 'baseline', 'repeat': repeat, 'wall_seconds': round(wall, 4),
           'cpu_percent_one_core': 0.0, 'dwm_cpu_percent': 100 * cpu / wall, 'dwm_gpu_ms_per_second': 1000 * gpu / wall,
           'gpu_board_watts': (board_last - board) / wall if board is not None and board_last is not None else '',
           'workload_ticks': 0, 'exit_code': 0, 'terminated_after_report': False}
    (out / f'desktop.baseline.{repeat}.json').write_text(json.dumps(row, indent=1) + '\n', newline='\n')
    return row


def one_trial(scene, binary, framework, mode, repeat, out):
    log = out / f'{framework}.{mode}.{repeat}.log'
    idle = mode == 'idle'
    requested = (IDLE_SETTLE + SAMPLED + 1 if idle else REQUESTED) + LIFETIME_MARGIN
    process = launch(binary, mode, requested, log)
    try:
        dwm = pdh.pid_of('dwm.exe')
        with pdh.Counters([pdh.dwm_cpu(), pdh.gpu_time(dwm), pdh.gpu_time(process.pid),
                           pdh.gpu_time(process.pid, '*')]) as counters:
            settled, early = sample(process, IDLE_SETTLE if idle else WARMUP, SAMPLED, counters,
                                    (WARMUP, SAMPLED) if idle else None)
    except OSError:
        settled, early = None, None
    # The graphics API it drew with: the driver's memory floor differs
    # between Direct3D 11 and 12 (docs/benchmarking.md, On Windows).
    graphics = sorted({name.lower() for _, _, name in win.modules(process.pid)} & GRAPHICS_MODULES)
    exit_code, terminated = finish(process, requested, log)
    ran = time.perf_counter() - process.started
    text = log.read_text(errors='replace')
    ticks = re.search(r'"ticks":(\d+)|workload_ticks=(\d+)|animation_frames=(\d+)', text)
    fps = re.search(r'fps=([0-9.]+)', text)
    font = re.search(r'font_file_fnv64=([0-9a-f]{16})', text)
    ticks = int(next(g for g in ticks.groups() if g)) if ticks else -1
    fps = float(fps.group(1)) if fps else ''
    row = {'framework': framework, 'mode': mode, 'repeat': repeat, **(settled or {'samples': 0, 'uncovered': False}),
           'workload_ticks': ticks, 'reported_fps': fps,
           'uia_client_attached': 'accessibility: a client asked for the tree' in text,
           'graphics_modules': ' '.join(graphics),
           'exit_code': exit_code, 'terminated_after_report': terminated,
           'ran_seconds': round(ran, 2), 'requested_seconds': requested, 'ready_seconds': round(process.ready, 4),
           'font_fnv64': font.group(1) if font else ''}
    row['dpi'] = json.loads(log.with_suffix('.ready.json').read_text())['dpi']
    if settled and ticks > 0:
        # The frames or ticks in the sample: animate's at its reported rate,
        # the others' at the rate over the whole run.
        rate = fps if scene == 'animate' and fps else ticks / ran
        row['mcycles_per_frame'] = settled['cycles'] / 1e6 / (rate * settled['wall_seconds'])
    if early:
        row['early_mean_private_bytes'] = early['mean_private_bytes']
        row['early_cpu_percent_one_core'] = early['cpu_percent_one_core']
        row['early_mcycles_per_second'] = early['mcycles_per_second']
    (out / f'{framework}.{mode}.{repeat}.json').write_text(json.dumps(row, indent=1) + '\n', newline='\n')
    return row


def trial(scene, binary, framework, mode, repeat, out):
    """Once more after an abnormal exit or a readiness failure."""
    for attempt in range(2):
        counted(framework, 'trial')
        try:
            row = one_trial(scene, binary, framework, mode, repeat, out)
        except ReadinessFailure as error:
            READINESS_FAILURES.append(dict(scene=scene, framework=framework, mode=mode, kind='trial',
                                           attempt=attempt + 1, exhausted=bool(attempt), error=str(error)))
            print(f'{framework} {mode}: readiness failed ({error})', flush=True)
            if attempt:
                raise
            time.sleep(2)
            continue
        if not abnormal(row['exit_code'], row['terminated_after_report']):
            return row
        CRASHES.append({'framework': framework, 'scene': scene, 'mode': mode, 'kind': 'trial',
                        'exit_code': row['exit_code']})
        print(f"{framework} {mode}: exited abnormally ({row['exit_code']}); once more", flush=True)
        if attempt:
            return row
        time.sleep(2)


def valid(row, scene):
    """Whether a trial ran as asked (run_macos_comparison.valid), and its
    window showed uncovered as its sample began and ended."""
    if row['framework'] == 'desktop':
        return True
    if abnormal(row['exit_code'], row['terminated_after_report']) or not row.get('uncovered'):
        return False
    if abs(row['ran_seconds'] - row['requested_seconds']) > 3 or abs(row.get('wall_seconds', 0) - SAMPLED) > 0.5:
        return False
    if FONT_FNV64 and row['font_fnv64'] != FONT_FNV64:
        return False
    ticks, seconds = row['workload_ticks'], row['requested_seconds']
    if scene == 'animate':
        return ticks > 0 and row['reported_fps'] != '' and abs(ticks / seconds - row['reported_fps']) <= 0.15 * row['reported_fps']
    if row['mode'] == 'idle':
        return ticks == 0
    return 58 <= ticks / seconds <= 61


def pacing(scene, binary, framework, mode, repeat, out, pacer):
    seconds = WARMUP + PACING_SECONDS + 1 + LIFETIME_MARGIN
    log = out / f'{framework}.{mode}.{repeat}.pacing.log'
    for attempt in range(2):
        counted(framework, 'pacing')
        try:
            process = launch(binary, mode, seconds, log)
        except ReadinessFailure as error:
            READINESS_FAILURES.append(dict(scene=scene, framework=framework, mode=mode, kind='pacing',
                                           attempt=attempt + 1, exhausted=bool(attempt), error=str(error)))
            if attempt:
                raise
            time.sleep(2)
            continue
        settle = process.started + process.ready + WARMUP - time.perf_counter()
        result = framepace.capture(pacer, process.hwnd, settle=max(0, settle), seconds=PACING_SECONDS)
        covered = not compare.uncovered(process.hwnd)
        exit_code, terminated = finish(process, seconds, log)
        if not abnormal(exit_code, terminated) and not covered and result['frames'] > 1:
            break
        CRASHES.append({'framework': framework, 'scene': scene, 'mode': mode, 'kind': 'pacing', 'exit_code': exit_code,
                        **({'covered': True} if covered else {}), **({'frames': result['frames']} if result['frames'] <= 1 else {})})
        if attempt:
            raise SystemExit(f'{framework} {mode}: two pacing launches failed')
        time.sleep(2)
    row = {'framework': framework, 'mode': mode, 'repeat': repeat, **result}
    (out / f'{framework}.{mode}.{repeat}.pacing.json').write_text(json.dumps(row, indent=1) + '\n', newline='\n')
    return row


def probe(scene, binaries, out):
    """vlist: every framework's measured heights of the probe rows agree
    (run_macos_comparison.probe), or the scene is not measured."""
    found = {}
    for framework, binary in binaries.items():
        log = out / f'{framework}.probe.log'
        process = launch(binary, 'probe', 30, log)
        finish(process, 30, log)
        text = log.read_text(errors='replace')
        heights = re.search(r'row_heights=(\S+)', text)
        widths = dict(re.findall(r'(list_width|text_width)=([0-9.]+)', text))
        font = re.search(r'font_file_fnv64=([0-9a-f]{16})', text)
        found[framework] = {
            'heights': {int(r): float(h) for r, h in (pair.split(':') for pair in heights.group(1).split(','))} if heights else {},
            **{k: float(v) for k, v in widths.items()}, 'font_fnv64': font.group(1) if font else ''}
    rows = sorted(set().union(*(f['heights'] for f in found.values())))
    disagree = [r for r in rows if None in {f['heights'].get(r) for f in found.values()} or
                max(f['heights'][r] for f in found.values()) - min(f['heights'][r] for f in found.values()) > PROBE_TOLERANCE]
    fonts = [f for f, v in found.items() if FONT_FNV64 and v['font_fnv64'] != FONT_FNV64]
    report = {'tolerance_points': PROBE_TOLERANCE, 'frameworks': found, 'disagreeing_rows': disagree, 'wrong_font': fonts}
    (out / 'probe.json').write_text(json.dumps(report, indent=1) + '\n', newline='\n')
    if disagree or fonts or not rows:
        raise SystemExit(f'probe: frameworks disagree on rows {disagree} or fonts {fonts}: not measuring vlist')


def spread_of(rows, framework, mode, key):
    values = [float(r[key]) for r in rows if (r['framework'], r['mode']) == (framework, mode) and r.get(key) not in ('', None)]
    if len(values) < 2 or statistics.median(values) <= 0:
        return 0.
    return (max(values) - min(values)) / statistics.median(values)


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


def source_diff_sha256():
    digest = hashlib.sha256(run(['git', 'diff', 'HEAD', '--', *SOURCES], cwd=SOURCE).encode())
    for path in sorted(run(['git', 'ls-files', '--others', '--exclude-standard', '--', *SOURCES], cwd=SOURCE).split()):
        digest.update(path.encode() + b'\0' + (SOURCE / path).read_bytes())
    return digest.hexdigest()


class DISPLAY_DEVICEW(ctypes.Structure):
    _fields_ = [('cb', ctypes.wintypes.DWORD), ('DeviceName', ctypes.c_wchar * 32), ('DeviceString', ctypes.c_wchar * 128),
                ('StateFlags', ctypes.wintypes.DWORD), ('DeviceID', ctypes.c_wchar * 128), ('DeviceKey', ctypes.c_wchar * 128)]


class DEVMODEW(ctypes.Structure):
    # The display fields of DEVMODEW, by their offsets.
    _fields_ = [('dmDeviceName', ctypes.c_wchar * 32), ('dmSpecVersion', ctypes.c_ushort), ('dmDriverVersion', ctypes.c_ushort),
                ('dmSize', ctypes.c_ushort), ('dmDriverExtra', ctypes.c_ushort), ('dmFields', ctypes.c_ulong),
                ('dmPositionX', ctypes.c_long), ('dmPositionY', ctypes.c_long), ('dmDisplayOrientation', ctypes.c_ulong),
                ('dmDisplayFixedOutput', ctypes.c_ulong), ('dmColor', ctypes.c_short), ('dmDuplex', ctypes.c_short),
                ('dmYResolution', ctypes.c_short), ('dmTTOption', ctypes.c_short), ('dmCollate', ctypes.c_short),
                ('dmFormName', ctypes.c_wchar * 32), ('dmLogPixels', ctypes.c_ushort), ('dmBitsPerPel', ctypes.c_ulong),
                ('dmPelsWidth', ctypes.c_ulong), ('dmPelsHeight', ctypes.c_ulong), ('dmDisplayFlags', ctypes.c_ulong),
                ('dmDisplayFrequency', ctypes.c_ulong), ('rest', ctypes.c_byte * 32)]


def displays():
    """The desktop's displays: adapter, resolution, refresh rate, primary."""
    found = []
    user32 = ctypes.windll.user32
    for index in range(16):
        device = DISPLAY_DEVICEW(cb=ctypes.sizeof(DISPLAY_DEVICEW))
        if not user32.EnumDisplayDevicesW(None, index, ctypes.byref(device), 0):
            break
        if not device.StateFlags & 1:  # DISPLAY_DEVICE_ATTACHED_TO_DESKTOP
            continue
        mode = DEVMODEW(dmSize=ctypes.sizeof(DEVMODEW))
        user32.EnumDisplaySettingsW(device.DeviceName, -1, ctypes.byref(mode))  # ENUM_CURRENT_SETTINGS
        found.append({'name': device.DeviceString, 'resolution': f'{mode.dmPelsWidth} x {mode.dmPelsHeight} @ {mode.dmDisplayFrequency}Hz',
                      'main': bool(device.StateFlags & 4)})
    return found


def environment():
    class MEMORYSTATUSEX(ctypes.Structure):
        _fields_ = [('dwLength', ctypes.c_ulong), ('dwMemoryLoad', ctypes.c_ulong), ('ullTotalPhys', ctypes.c_ulonglong),
                    ('rest', ctypes.c_ulonglong * 6)]

    class SYSTEM_POWER_STATUS(ctypes.Structure):
        _fields_ = [('ACLineStatus', ctypes.c_byte), ('rest', ctypes.c_byte * 11)]

    status = MEMORYSTATUSEX(dwLength=ctypes.sizeof(MEMORYSTATUSEX))
    ctypes.windll.kernel32.GlobalMemoryStatusEx(ctypes.byref(status))
    power = SYSTEM_POWER_STATUS()
    ctypes.windll.kernel32.GetSystemPowerStatus(ctypes.byref(power))
    import winreg
    with winreg.OpenKey(winreg.HKEY_LOCAL_MACHINE, r'HARDWARE\DESCRIPTION\System\CentralProcessor\0') as key:
        chip = winreg.QueryValueEx(key, 'ProcessorNameString')[0].strip()
    try:
        gpu = run(['nvidia-smi', '--query-gpu=name,driver_version', '--format=csv,noheader']).split(', ')
    except (OSError, subprocess.CalledProcessError):
        gpu = ['', '']
    screen_reader, uia_clients = compare.accessibility_state()
    ctypes.windll.kernel32.GetTickCount64.restype = ctypes.c_ulonglong
    found = displays()
    # A remote desktop host streaming the screen captures and encodes it
    # throughout: work beside every framework's, which the record names.
    names = {line.strip('"').split('","')[0].lower().removesuffix('.exe')
             for line in run(['tasklist', '/fo', 'csv', '/nh']).splitlines() if line}
    streaming = sorted(names & {'sunshine', 'apollo', 'parsecd', 'rustdesk', 'anydesk', 'teamviewer'})
    return {
        'displays': found, 'display_count': len(found),
        'power_plan': compare.active_power_plan()[1],
        'on_battery': power.ACLineStatus == 0,
        # Windows 11 reports itself as 10: its builds start at 22000.
        'windows': f"{'11' if int(platform.version().split('.')[-1]) >= 22000 else platform.release()} ({platform.version()})",
        'uptime_hours': round(ctypes.windll.kernel32.GetTickCount64() / 3.6e6, 1),
        'chip': chip, 'cores': os.cpu_count(), 'memory_bytes': status.ullTotalPhys,
        'gpu': gpu[0], 'gpu_driver': gpu[1] if len(gpu) > 1 else '',
        'power': 'AC Power' if power.ACLineStatus == 1 else 'Battery',
        'rust': run(['rustc', '--version']),
        'screen_reader_running': screen_reader, 'uia_client_processes_running': uia_clients,
        'remote_display_hosts_running': streaming,
    }


def toolchains(frameworks):
    found = {'rust': run(['rustc', '--version']), 'rust_profile': 'dist (release, lto = "fat", codegen-units = 1)'}
    if 'mygo' in frameworks:
        mygo = SOURCE / 'comparisons/mygo'
        version = run([GO, 'list', '-m', '-f', '{{.Version}}', 'github.com/egoist/mygo'], cwd=mygo)
        info = json.loads(run([GO, 'mod', 'download', '-json', f'github.com/egoist/mygo@{version}'], cwd=mygo))
        found['go'] = run([GO, 'version'])
        found['mygo'] = {'mygo_version': version, 'mygo_commit': info.get('Hash') or info.get('Origin', {}).get('Hash', ''), 'mygo_license': 'MIT',
                         'mygo_source': 'https://github.com/egoist/mygo', 'go': found['go'].removeprefix('go version '),
                         'flags': '-trimpath -ldflags=-s -w', 'cgo': False}
    if 'rune' in frameworks:
        rune = SOURCE / 'comparisons/rune'
        patch = (rune / 'rune-wheel.patch').read_bytes().replace(b'\r\n', b'\n')
        source = rune / 'rune-src'
        found['rune'] = {'rune_rev': run(['git', 'rev-parse', 'HEAD'], cwd=source) if source.exists() else '',
                         'rune_branch': 'dev', 'rune_license': 'MIT', 'rune_source': 'https://github.com/anomalyco/rune',
                         'rune_patch': f'PATCH.md: Tx::wheel (sha256 {hashlib.sha256(patch).hexdigest()})',
                         'profile': "dist (Rune's perf with fat LTO and codegen-units 1)"}
    return found


def quiet(threshold=75.0, timeout=900, log=print):
    """Wait until no build runs and the machine's other CPU is under
    `threshold` percent of a core over 5 s; what was found."""
    class FILETIME(ctypes.Structure):
        _fields_ = [('low', ctypes.c_ulong), ('high', ctypes.c_ulong)]

    def times():
        idle, kernel, user = FILETIME(), FILETIME(), FILETIME()
        ctypes.windll.kernel32.GetSystemTimes(ctypes.byref(idle), ctypes.byref(kernel), ctypes.byref(user))
        value = lambda t: ((t.high << 32) | t.low) / 1e7
        return value(idle), value(kernel) + value(user)

    def cpu(pids):
        found = {}
        for pid in pids:
            try:
                found[pid] = win.cpu_seconds(pid)
            except OSError:
                pass  # a protected process: in the machine's total alone
        return found

    deadline = time.monotonic() + timeout
    while True:
        rows = [line.strip('"').split('","') for line in run(['tasklist', '/fo', 'csv', '/nh']).splitlines() if line]
        names = {int(r[1]): r[0].removesuffix('.exe') for r in rows}
        heavy = sorted({n for n in names.values() if n.lower() in {h.lower() for h in HEAVY}})
        idle0, busy0 = times()
        before = cpu(names)
        time.sleep(5)
        after = cpu(names)
        idle1, busy1 = times()
        other = 100 * ((busy1 - busy0) - (idle1 - idle0)) / 5
        busiest = sorted(((after[p] - before[p]) / 5 * 100, names[p]) for p in after if p in before)[::-1][:6]
        busiest = [f'{name} {percent:.1f}%' for percent, name in busiest]
        if not heavy and other < threshold:
            return {'heavy': heavy, 'other_cpu_percent': round(other, 1), 'threshold_percent': threshold,
                    'busiest': busiest}
        log(f'preflight: waiting (builds {heavy}, rest of the machine {other:.0f}% of a core: {", ".join(busiest)})')
        if time.monotonic() > deadline:
            raise SystemExit('the machine did not quiet down')


def measure_scene(scene, binaries, out, pacer, commit, preflight, tools):
    started = datetime.datetime.now(datetime.timezone.utc)
    out.mkdir(parents=True, exist_ok=True)
    modes, _ = SCENES[scene]
    frameworks = list(binaries)
    if scene == 'vlist':
        probe(scene, binaries, out)
    rows, paced = [], []
    for repeat in range(max(REPEATS, IDLE_REPEATS) if 'idle' in modes else REPEATS):
        order = frameworks[repeat % len(frameworks):] + frameworks[:repeat % len(frameworks)]
        chosen = [m for m in modes if repeat < (IDLE_REPEATS if m == 'idle' else REPEATS)]
        rows.append(desktop_trial(repeat, out))
        print(f"desktop  baseline {repeat}: dwm cpu {rows[-1]['dwm_cpu_percent']:5.2f}% gpu "
              f"{rows[-1]['dwm_gpu_ms_per_second']:5.2f} ms/s board {rows[-1]['gpu_board_watts'] or 0:5.1f} W", flush=True)
        for framework, mode in [(f, m) for m in chosen for f in order]:
            try:
                row = trial(scene, binaries[framework], framework, mode, repeat, out)
            except ReadinessFailure:
                continue
            print(f"{framework:9} {mode:7} {repeat}: cpu {row.get('mcycles_per_second', 0):7.1f} Mcyc/s "
                  f"{row.get('mcycles_per_frame', 0) or 0:6.2f} Mcyc/frame gpu {row.get('gpu_ms_per_second', 0):6.2f} ms/s "
                  f"dwm cpu {row.get('dwm_cpu_percent', 0):5.2f}% gpu {row.get('dwm_gpu_ms_per_second', 0):5.2f} ms/s "
                  f"board {row.get('gpu_board_watts') or 0:5.1f} W private {row.get('mean_private_bytes', 0) / 2**20:6.1f} MiB "
                  f"ticks {row['workload_ticks']}{'' if valid(row, scene) else '  INVALID'}", flush=True)
            rows.append(row)
            time.sleep(2)
    # Cells whose CPU cycles or app GPU time disagree get one more trial
    # each (CPU time counts scheduler ticks: too coarse to judge by).
    wide = [(f, m) for m in modes if m != 'idle' for f in frameworks
            if max(spread_of(rows, f, m, 'mcycles_per_second'), spread_of(rows, f, m, 'gpu_ms_per_second')) > SPREAD]
    for extra in range(EXTRA):
        for framework, mode in wide:
            try:
                row = trial(scene, binaries[framework], framework, mode, REPEATS + extra, out)
            except ReadinessFailure:
                continue
            print(f"{framework:9} {mode:7} {REPEATS + extra}: cpu {row.get('mcycles_per_second', 0):7.1f} Mcyc/s "
                  f"gpu {row.get('gpu_ms_per_second', 0):6.2f} ms/s (extra: wide spread)", flush=True)
            rows.append(row)
            time.sleep(2)
    for repeat in range(PACING_REPEATS):
        order = frameworks[repeat % len(frameworks):] + frameworks[:repeat % len(frameworks)]
        for mode in [m for m in modes if m != 'idle']:
            for framework in order:
                try:
                    row = pacing(scene, binaries[framework], framework, mode, repeat, out, pacer)
                except ReadinessFailure:
                    continue
                print(f"{framework:9} {mode:7} {repeat}: pacing {row['frames_per_second']:6.1f} fps p99 "
                      f"{row['interval_p99_ms']:5.1f} ms long {row['long_interval_fraction']:.3f}", flush=True)
                paced.append(row)
                time.sleep(2)
    write_csv(out / 'current.csv', FIELDS, rows)
    write_csv(out / 'pacing.csv', ['framework', 'mode', 'repeat', 'display_hz', 'frames', *PACING, 'long_intervals'], paced)
    groups = [group(rows, METRICS, 'desktop', 'baseline')]
    groups += [group(rows, METRICS + ['reported_fps'], f, m) for f in frameworks for m in modes]
    pacing_groups = [group(paced, PACING, f, m) for f in frameworks for m in modes if m != 'idle']
    (out / 'summary.json').write_text(json.dumps({'groups': groups, 'pacing': pacing_groups}, indent=1) + '\n', newline='\n')
    apps = [r for r in rows if r['framework'] != 'desktop']
    if scene == 'animate':
        active = [r['reported_fps'] for r in apps if r['reported_fps'] != '']
    else:
        active = [r['workload_ticks'] / r['requested_seconds'] for r in apps if r['mode'] != 'idle']
    invalid = [f"{r['framework']}/{r['mode']}/{r['repeat']}" for r in apps if not valid(r, scene)]
    complete = all(len([r for r in apps if (r['framework'], r['mode']) == (f, m)]) >=
                   (IDLE_REPEATS if m == 'idle' else REPEATS) for f in frameworks for m in modes)
    accepted = complete and not invalid and not any(r['exhausted'] for r in READINESS_FAILURES if r['scene'] == scene)
    audit = {'accepted': accepted, 'platform': 'windows', 'trials': len(rows), 'samples': sum(r.get('samples', 0) for r in apps),
             'pacing_trials': len(paced),
             ('reported_fps' if scene == 'animate' else 'active_ticks_per_requested_second'):
                 {'min': min(active), 'max': max(active)} if active else None,
             'scene': scene, 'commit': commit,
             'source_dirty': bool(run(['git', 'status', '--porcelain', '--', *SOURCES], cwd=SOURCE)),
             'source_diff_sha256': source_diff_sha256(),
             'started_utc': started.isoformat(), 'finished_utc': datetime.datetime.now(datetime.timezone.utc).isoformat(),
             'environment': environment(), 'preflight': preflight,
             'binaries': {f: {'sha256': hashlib.sha256(p.read_bytes()).hexdigest(), 'bytes': p.stat().st_size}
                          for f, p in binaries.items()},
             'toolchains': {k: v for k, v in tools.items() if k in ('rust', 'rust_profile', 'go') or k in frameworks},
             'invalid_trials': invalid,
             'abnormal_exits': [c for c in CRASHES if c['scene'] == scene],
             'launches': LAUNCHES, 'readiness_failures': [r for r in READINESS_FAILURES if r['scene'] == scene],
             'protocol': {'requested_seconds': REQUESTED + LIFETIME_MARGIN, 'warmup_seconds': WARMUP,
                          'sampled_seconds': SAMPLED, 'sampling_anchor': 'window frontmost and uncovered',
                          'readiness_timeout_seconds': LIFETIME_MARGIN - 0.5, 'lifetime_margin_seconds': LIFETIME_MARGIN,
                          'interval_seconds': INTERVAL, 'repeats': REPEATS, 'idle_repeats': IDLE_REPEATS,
                          'idle_window_seconds': [IDLE_SETTLE, IDLE_SETTLE + SAMPLED],
                          'idle_requested_seconds': IDLE_SETTLE + SAMPLED + 1 + LIFETIME_MARGIN,
                          'quick': False, 'pgo': False, 'frameworks': frameworks, 'modes': modes,
                          'pacing_seconds': PACING_SECONDS, 'pacing_repeats': PACING_REPEATS,
                          'pacing_during_trials': False, 'pacing_capture': 'Desktop Duplication (framepace)',
                          'acceptance_updates_per_second': [58, 61], 'suite': True,
                          'spread_for_extra_trials': SPREAD, 'extra_trials': EXTRA,
                          'extended_cells': [f'{f}/{m}' for f, m in wide],
                          'spread_metrics': ['mcycles_per_second', 'gpu_ms_per_second'],
                          'power_plan': 'High performance', 'font_fnv64': FONT_FNV64}}
    (out / 'audit.json').write_text(json.dumps(audit, indent=1) + '\n', newline='\n')
    return accepted


def measure_startup(binaries, out, commit):
    """Spawn to window and to first content (the header's text drawn), and
    the process's CPU time and cycles by then, of the lab scene's binaries
    (scripts/windows_startup_probe.py), launches rotated; startup.json in the
    macOS startup run's shape (docs/results/startup-macos-*)."""
    out.mkdir(parents=True, exist_ok=True)
    # The suite's font, as every trial loads it (the probe's launches take
    # this process's environment).
    if FONT.exists():
        os.environ['ZGX_FONT_FILE'] = os.environ['ZGUI_FONT_FILE'] = str(FONT)
    found = {f: [] for f in binaries}
    names = list(binaries)
    for index in range(STARTUP_LAUNCHES):
        shift = index % len(names)
        for framework in names[shift:] + names[:shift]:
            window, content, cpu, mcycles = startup.launch(str(binaries[framework]), 0)
            found[framework].append({'window_ms': round(window, 1), 'content_ms': round(content, 1),
                                     'cycles': round(mcycles * 1e6), 'cpu_ms': round(cpu, 1)})
            time.sleep(0.5)
    medians = {f: {k: {'median': statistics.median(r[k] for r in runs), 'min': min(r[k] for r in runs),
                       'max': max(r[k] for r in runs)} for k in runs[0]} for f, runs in found.items()}
    machine = environment()
    record = {
        'commit': commit,
        'machine': f"{machine['chip']}, {machine['gpu']}",
        'probe': 'scripts/windows_startup_probe.py: spawn, window, the header strip until its text has drawn '
                 '(PrintWindow); CPU time and cycles of the process at content',
        'apps': {f: f'{p.name} (lab), ZGX_MODE=idle' for f, p in binaries.items()},
        'protocol': f"{STARTUP_LAUNCHES} rounds, the frameworks rotating; the suite's font",
        'launches': found,
        'medians': medians,
    }
    (out / 'startup.json').write_text(json.dumps(record, indent=1) + '\n', newline='\n')
    for f, m in medians.items():
        print(f"startup {f:9}: window {m['window_ms']['median']:5.0f} ms, content {m['content_ms']['median']:5.0f} ms, "
              f"cpu {m['cpu_ms']['median']:5.0f} ms, {m['cycles']['median'] / 1e6:6.0f} M cycles", flush=True)


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument('--zgx', type=Path, default=ROOT / 'target/dist/examples', help="zgx's examples (and vlist_luau)")
    parser.add_argument('--gpui-fast', type=Path, default=ROOT / 'comparisons/gpui-fast/target/dist')
    parser.add_argument('--mygo', type=Path, default=ROOT / 'comparisons/mygo/bin')
    parser.add_argument('--rune', type=Path, default=ROOT / 'comparisons/rune/bin')
    parser.add_argument('--framepace', type=Path, default=ROOT / 'target/release/examples/framepace.exe')
    parser.add_argument('--scenes', default='heavy,lab,animate,vlist,startup')
    parser.add_argument('--frameworks', default=','.join(FRAMEWORKS))
    parser.add_argument('--out', type=Path)
    parser.add_argument('--preflight-threshold', type=float, default=75.,
                        help='percent of a core the rest of the machine may use (a desktop streamed by a remote '
                             'display host uses more than a core of its own)')
    parser.add_argument('--go', type=Path, default=Path('go'), help='the Go that built MyGo, for the record')
    parser.add_argument('--source', type=Path, default=ROOT,
                        help='the checkout the binaries were built from (default this one)')
    args = parser.parse_args()
    global SOURCE, GO
    SOURCE, GO = args.source.resolve(), str(args.go)
    dirs = {'zgx': args.zgx, 'zgx-luau': args.zgx, 'gpui-fast': args.gpui_fast, 'mygo': args.mygo, 'rune': args.rune}
    wanted = args.frameworks.split(',')
    binaries = {scene: {f: dirs[f] / f'{name}.exe' for f, name in SCENES[scene][1].items() if f in wanted}
                for scene in SCENES}
    scenes = args.scenes.split(',')
    missing = [str(p) for s in scenes if s in SCENES for p in binaries[s].values() if not p.exists()]
    missing += [] if args.framepace.exists() else [str(args.framepace)]
    assert not missing, 'not built:\n' + '\n'.join(missing)
    commit = run(['git', 'rev-parse', 'HEAD'], cwd=SOURCE)
    # What the audits record, read before measuring: a tool missing from
    # PATH (go) stops the run here, not after a scene's trials.
    tools = toolchains(wanted)
    environment()
    started = datetime.datetime.now(datetime.timezone.utc)
    out = args.out or ROOT / 'docs/results' / f'suite-windows-{started:%Y-%m-%d}-{commit[:7]}'
    out.mkdir(parents=True, exist_ok=True)
    results = {}
    with compare.PowerPlan(compare.HIGH_PERFORMANCE), compare.Minimized(['Zeron']) as hidden:
        # Zeron minimized first: its window's drawing is what the run hides.
        found = quiet(args.preflight_threshold, log=lambda line: print(line, flush=True))
        for scene in scenes:
            began = time.monotonic()
            print(f'== {scene}', flush=True)
            if scene == 'startup':
                measure_startup(binaries['lab'], out / 'startup', commit)
                continue
            accepted = measure_scene(scene, binaries[scene], out / scene, args.framepace, commit, found, tools)
            results[scene] = {'accepted': accepted, 'minutes': round((time.monotonic() - began) / 60, 1)}
            print(f"== {scene}: {'ACCEPTED' if accepted else 'REJECTED'} in {results[scene]['minutes']} min", flush=True)
        minimized = [win.title(h) for h in hidden.hwnds]
    manifest = {'commit': commit, 'platform': 'windows', 'started_utc': started.isoformat(),
                'finished_utc': datetime.datetime.now(datetime.timezone.utc).isoformat(),
                'preflight': found, 'windows_minimized': minimized, 'scenes': results, 'frameworks': wanted,
                'protocol': {'REQUESTED': REQUESTED, 'WARMUP': WARMUP, 'SAMPLED': SAMPLED, 'IDLE_REPEATS': IDLE_REPEATS,
                             'PACING_REPEATS': PACING_REPEATS, 'spread_for_extra_trials': SPREAD, 'extra_trials': EXTRA}}
    (out / 'manifest.json').write_text(json.dumps(manifest, indent=1) + '\n', newline='\n')
    print(('ACCEPTED' if all(r['accepted'] for r in results.values()) else 'REJECTED'), out)


if __name__ == '__main__':
    main()
