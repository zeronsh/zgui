#!/usr/bin/env python3
"""zgx against GPUI Fast, MyGo and GPUI on Windows: the comparison scenes
(`comparisons/README.md`), each framework's adapter built at its strongest,
run in turn on the same desktop. The Windows counterpart of the macOS
comparison runner.

Scenes and modes (each framework's binary per `comparisons/README.md`):

- `lab`: `idle`, `stream` (text streaming), `scroll`, `both`;
- `animate`: display-rate animation;
- `heavy`: the dashboard, every live value (`both`);
- `vlist`: each framework's own variable-height list: `scroll`, `fling`,
  `jump`;
- `busy` (GPUI's old adapter only): a busy window with one spinner.

Each run lasts `--warmup` plus `--seconds`, sampled every 100 ms after the
warm-up: CPU time (`GetProcessTimes`), CPU cycles (`QueryProcessCycleTime`,
finer than CPU time's scheduler ticks), working set and private bytes. The
frameworks alternate in rotating order over `--repeats`; a scene and mode
whose cycles spread wider than `--spread` of their median gets another
round, up to `--max-repeats`. A run is valid only if it lasted as long as it
was asked and did its work: it must report its ticks (`workload_ticks`,
`"ticks"`) or frames (`animation_frames`), at least nine tenths of 60 a
second outside `idle`; an invalid run is run again once. Cycles are also
given per tick or frame, so a run that did less work cannot look cheaper.

The compositor's share is sampled with it (`windows_pdh`): `dwm.exe`'s CPU
cycles (and its tick-counted CPU time) and its GPU time on the 3D engines,
and the app's own GPU time, each also per tick or frame. `--desktop` adds
a run of nothing to each round, the desktop's own compositor cost, to
subtract.

    cargo build --profile dist --locked -p zgx-desktop --example component_workload  (and heavy_workload, animation_workload, vlist_workload)
    cargo build --profile dist --locked --manifest-path comparisons/gpui-fast/Cargo.toml
    (cd comparisons/mygo && ./build.sh)
    python scripts/windows_compare.py --zgx target/dist/examples --gpui-fast comparisons/gpui-fast/target/dist --mygo comparisons/mygo/bin --output target/windows-compare

Each run's window is brought to the front (a window opened from a process in
the background stays behind the foreground one), and a run whose window
stays covered is invalid. Windows' High performance power plan is held for
the runs (`--power-plan keep` leaves the user's) and each run records the
processors' clock: under Balanced, a busy UI thread's clock moved from run
to run and so did its cycles. Keep the machine otherwise idle, and on mains
power.
"""
import argparse
import ctypes
import ctypes.wintypes
import hashlib
import json
import os
import pathlib
import platform
import re
import statistics
import subprocess
import time

import windows_input as win
import windows_pdh

FRAMEWORKS = ("zgx", "gpui-fast", "mygo", "gpui")
# Scene: (modes, {framework: binary}).
SCENES = {
    "lab": (("idle", "stream", "scroll", "both"), {
        "zgx": "component_workload.exe", "gpui-fast": "zgx-compare-gpui-fast.exe", "mygo": "lab.exe",
        "gpui": "zgx-compare-gpui.exe"}),
    "animate": (("animate",), {
        "zgx": "animation_workload.exe", "gpui-fast": "animate.exe", "mygo": "animate.exe", "gpui": "animate.exe"}),
    "heavy": (("both",), {
        "zgx": "heavy_workload.exe", "gpui-fast": "heavy.exe", "mygo": "heavy.exe", "gpui": "heavy.exe"}),
    "vlist": (("scroll", "fling", "jump"), {
        "zgx": "vlist_workload.exe", "gpui-fast": "vlist.exe", "mygo": "vlist.exe"}),
    "busy": (("busy",), {"zgx": "busy_workload.exe", "gpui": "busy.exe"}),
}
TICKS_PER_SECOND = 60
# The one font file every adapter loads (`comparisons/fonts`), staged as the
# macOS runner stages it.
FONT = pathlib.Path(__file__).resolve().parent.parent / "comparisons" / "fonts" / "DejaVuSans.ttf"
font_file = None


def units(text):
    """The work a run reports it did."""
    for pattern in (r"workload_ticks=(\d+)", r'"ticks":(\d+)', r"animation_frames=(\d+)"):
        match = re.search(pattern, text)
        if match:
            return int(match.group(1))
    return None


# Processes that read other apps' windows through UI Automation without being
# screen readers: present, one may have attached to a run's window.
UIA_CLIENTS = ("TextInputHost", "VoiceAccess", "Magnify", "Narrator", "nvda", "jfw", "Inspect",
               "AccessibilityInsights", "WinAppDriver", "PowerToys", "LiveCaptions")


def accessibility_state():
    """Whether a screen reader runs (SPI_GETSCREENREADER), and which known
    UIA client processes run, as this machine is during a run."""
    flag = ctypes.c_int(0)
    ctypes.windll.user32.SystemParametersInfoW(0x0046, 0, ctypes.byref(flag), 0)
    out = subprocess.run(["tasklist", "/fo", "csv", "/nh"], capture_output=True, text=True).stdout
    running = {line.split('","')[0].strip('"').removesuffix(".exe") for line in out.splitlines() if line}
    return bool(flag.value), sorted(name for name in running if name in UIA_CLIENTS)


class Invalid(Exception):
    pass


def uncovered(hwnd):
    """Whether `hwnd` is the window on top at its client area's centre and
    near each corner: what is drawn there is what shows."""
    user32 = ctypes.windll.user32
    user32.WindowFromPoint.restype = ctypes.c_void_p
    user32.WindowFromPoint.argtypes = [ctypes.wintypes.POINT]
    user32.GetAncestor.restype = ctypes.c_void_p
    user32.GetAncestor.argtypes = [ctypes.c_void_p, ctypes.c_uint]
    width, height = win.client_size(hwnd)
    for fx, fy in ((0.5, 0.5), (0.05, 0.05), (0.95, 0.05), (0.05, 0.95), (0.95, 0.95)):
        x, y = win.to_screen(hwnd, int(width * fx), int(height * fy))
        top = user32.GetAncestor(user32.WindowFromPoint(ctypes.wintypes.POINT(x, y)), 2)  # GA_ROOT
        if top != win._handle(hwnd):
            return False
    return True


def windows_titled(title):
    """Visible top-level windows whose title is `title`."""
    found = []
    user32 = ctypes.windll.user32

    @ctypes.WINFUNCTYPE(ctypes.c_bool, ctypes.c_void_p, ctypes.c_void_p)
    def each(hwnd, _):
        if user32.IsWindowVisible(hwnd) and not user32.IsIconic(hwnd) and win.title(hwnd) == title:
            found.append(hwnd)
        return True

    user32.EnumWindows(each, None)
    return found


HIGH_PERFORMANCE = "8c5e7fda-e8bf-4a96-9a85-a6e23a8c635c"


def active_power_plan():
    out = subprocess.run(["powercfg", "/getactivescheme"], capture_output=True, text=True).stdout
    found = re.search(r"GUID: ([0-9a-f-]+)\s+\((.*)\)", out)
    return (found.group(1), found.group(2)) if found else (None, None)


class PowerPlan:
    """Windows' High performance power plan for the runs, the user's own
    restored after. Under Balanced, Windows clocked a busy UI thread up and
    down from run to run (66 to 73% of nominal across eight runs), and
    cycles a frame, which count time, spread 30% between runs of one binary;
    under High performance, 5%."""

    def __init__(self, guid):
        self.guid, self.previous = guid, None

    def __enter__(self):
        self.previous, _ = active_power_plan()
        if self.guid and self.previous and self.guid != self.previous:
            subprocess.run(["powercfg", "/setactive", self.guid], check=True)
        return self

    def __exit__(self, *_):
        if self.guid and self.previous and self.guid != self.previous:
            subprocess.run(["powercfg", "/setactive", self.previous], check=True)


class Minimized:
    """Windows titled any of `titles` minimized for the measurement and
    restored after, as the macOS suite hides Zeron: a translucent, animated
    window over or beside the runs adds the compositor's work to theirs."""

    def __init__(self, titles):
        self.titles, self.hwnds = titles, []

    def __enter__(self):
        for title in self.titles:
            for hwnd in windows_titled(title):
                ctypes.windll.user32.ShowWindow(ctypes.c_void_p(hwnd), 6)  # SW_MINIMIZE
                self.hwnds.append(hwnd)
        return self

    def __exit__(self, *_):
        for hwnd in self.hwnds:
            ctypes.windll.user32.ShowWindow(ctypes.c_void_p(hwnd), 9)  # SW_RESTORE


def bring_forward(proc):
    """The run's window, brought to the front: a window opened from a
    process in the background stays behind the foreground one, where the
    compositor draws it covered (blurred under a translucent window, or not
    at all), and what it shows can't be read."""
    try:
        hwnd = win.wait_window(proc)
        win.activate(hwnd)
    except (RuntimeError, OSError):
        return None
    time.sleep(0.2)
    return hwnd if uncovered(hwnd) else None


def desktop(seconds, warmup):
    """The compositor with no app running, sampled as `run` samples one."""
    dwm = windows_pdh.pid_of("dwm.exe")
    with windows_pdh.Counters([windows_pdh.dwm_cpu(), windows_pdh.gpu_time(dwm)]) as counters:
        time.sleep(warmup)
        first, start, cycles = counters.sample(), time.perf_counter(), windows_pdh.cycles_of(dwm)
        time.sleep(seconds)
        last, wall = counters.sample(), time.perf_counter() - start
        cycles = windows_pdh.cycles_of(dwm) - cycles
    cpu, gpu = ((b - a) / 1e7 for a, b in zip(first, last))
    return {"dwm_cpu_percent": 100 * cpu / wall, "dwm_mcycles_per_second": cycles / wall / 1e6,
            "dwm_gpu_ms_per_second": 1000 * gpu / wall}


def run(path, mode, seconds, warmup, log_path):
    total = warmup + seconds
    # ZGUI_*: adapters built before the rename read those alone.
    env = dict(os.environ, ZGX_MODE=mode, ZGX_SECONDS=f"{total:g}", ZGX_INITIAL_TICKS="0",
               ZGUI_MODE=mode, ZGUI_SECONDS=f"{total:g}", ZGUI_INITIAL_TICKS="0", ZGX_ACCESSIBILITY_LOG="1",
               **({"ZGX_FONT_FILE": str(font_file), "ZGUI_FONT_FILE": str(font_file)} if font_file else {}))
    started = time.perf_counter()
    samples = []
    with open(log_path, "w") as log:
        proc = subprocess.Popen([str(path)], env=env, stdout=log, stderr=log)
        dwm = windows_pdh.pid_of("dwm.exe")
        counters = windows_pdh.Counters([windows_pdh.dwm_cpu(), windows_pdh.gpu_time(dwm),
                                         windows_pdh.gpu_time(proc.pid)])
        clock, clocks = windows_pdh.Rate(windows_pdh.processor_performance()), []
        compositor = []
        try:
            front = bring_forward(proc)
            still_front = None
            while proc.poll() is None:
                elapsed = time.perf_counter() - started
                if elapsed > total + 30:
                    raise Invalid(f"{path.name} did not end")
                if elapsed >= warmup:
                    try:
                        private, working = win.memory(proc.pid)
                        samples.append((time.perf_counter(), win.cpu_seconds(proc.pid), win.cycles(proc.pid),
                                        working, private))
                        compositor.append(counters.sample() + [windows_pdh.cycles_of(dwm)])
                        clocks.append(clock.sample())
                    except OSError:
                        pass
                    # Still uncovered as sampling ends.
                    if front and elapsed >= total - 0.5 and still_front is None:
                        still_front = uncovered(front)
                time.sleep(0.1)
        finally:
            counters.close()
            clock.close()
            if proc.poll() is None:
                proc.kill()
            code = proc.wait()
    lasted = time.perf_counter() - started
    text = pathlib.Path(log_path).read_text(errors="replace")
    done = units(text)
    # The last sample can be of a process already releasing its memory.
    kept = [i for i, s in enumerate(samples) if s[3] > 0][:-1]
    samples, compositor = [samples[i] for i in kept], [compositor[i] for i in kept]
    if not front:
        raise Invalid(f"{path.name} {mode}: its window stayed covered")
    if still_front is False:
        raise Invalid(f"{path.name} {mode}: its window was covered while sampled")
    if code or len(samples) < 10 or done is None:
        raise Invalid(f"{path.name} {mode}: exit {code}, {len(samples)} samples, work {done}; see {log_path}")
    wall = samples[-1][0] - samples[0][0]
    if lasted < total * 0.95 or wall < seconds * 0.85:
        raise Invalid(f"{path.name} {mode}: ran {lasted:.1f} s, sampled {wall:.1f} s, of {total:g}")
    if mode != "idle" and done < 0.9 * TICKS_PER_SECOND * total:
        raise Invalid(f"{path.name} {mode}: {done} ticks or frames in {total:g} s; see {log_path}")
    cpu = samples[-1][1] - samples[0][1]
    cycles = samples[-1][2] - samples[0][2]
    # The work over the sampled span, of the whole run's.
    sampled_units = done * wall / total if done else 0
    # 100 ns units: the compositor's CPU, its GPU and the app's GPU.
    dwm_cpu, dwm_gpu, app_gpu = ((b - a) / 1e7 for a, b in zip(compositor[0][:3], compositor[-1][:3]))
    dwm_cycles = compositor[-1][3] - compositor[0][3]

    def per_unit(value):
        return 1e6 * value / sampled_units if sampled_units else None

    return {
        "dwm_cpu_percent": 100 * dwm_cpu / wall,
        "dwm_mcycles_per_second": dwm_cycles / wall / 1e6,
        "dwm_gpu_ms_per_second": 1000 * dwm_gpu / wall,
        "gpu_ms_per_second": 1000 * app_gpu / wall,
        "dwm_cpu_us_per_unit": per_unit(dwm_cpu),
        "dwm_gpu_us_per_unit": per_unit(dwm_gpu),
        "gpu_us_per_unit": per_unit(app_gpu),
        "cpu_percent": 100 * cpu / wall,
        "mcycles_per_second": cycles / wall / 1e6,
        "mcycles_per_unit": (cycles / 1e6 / sampled_units) if sampled_units else None,
        "units": done,
        "working_set_mib": statistics.mean(s[3] for s in samples) / 2**20,
        "peak_working_set_mib": max(s[3] for s in samples) / 2**20,
        "private_mib": statistics.mean(s[4] for s in samples) / 2**20,
        # zgx says when a UI Automation client read its window (the others
        # have no provider, so nothing to read).
        "uia_client_attached": "accessibility: a client asked for the tree" in text,
        # Brought to the front, and found uncovered as sampling began and ended.
        "window_uncovered": True,
        # The processors' clock against nominal: cycles count time, so a run
        # clocked down costs more of them for the same work.
        "processor_performance_percent": statistics.mean(c for c in clocks if c is not None),
    }


def key_metric(rows):
    """What a scene's spread is judged by: cycles per tick or frame where
    there is work, else per second."""
    per_unit = [r["mcycles_per_unit"] for r in rows if r["mcycles_per_unit"]]
    return per_unit if len(per_unit) == len(rows) else [r["mcycles_per_second"] for r in rows]


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    for name in FRAMEWORKS:
        parser.add_argument(f"--{name}", type=pathlib.Path, help=f"{name}'s adapter binaries")
    parser.add_argument("--scenes", default="lab,animate,heavy,vlist")
    parser.add_argument("--seconds", type=float, default=10, help="sampled, after the warm-up")
    parser.add_argument("--warmup", type=float, default=3)
    parser.add_argument("--repeats", type=int, default=3)
    parser.add_argument("--max-repeats", type=int, default=6)
    parser.add_argument("--spread", type=float, default=0.05,
                        help="another round while (max - min) / median exceeds this")
    parser.add_argument("--power-plan", default="high", choices=("high", "keep"),
                        help="hold Windows' High performance plan for the runs (the user's restored after)")
    parser.add_argument("--keep", action="append", default=[],
                        help="leave this window title showing (Zeron's are minimized during the runs)")
    parser.add_argument("--desktop", action="store_true",
                        help="add a run of nothing to each round: the compositor's cost at rest")
    parser.add_argument("--font", type=pathlib.Path, default=FONT, help="the one font file every adapter loads")
    parser.add_argument("--output", type=pathlib.Path, default=pathlib.Path("target/windows-compare"))
    args = parser.parse_args()
    global font_file
    font_file = args.font.resolve() if args.font.exists() else None
    if font_file is None:
        print(f"no font file at {args.font}: each adapter draws in its own font")
    dirs = {name: getattr(args, name.replace("-", "_")) for name in FRAMEWORKS}
    dirs = {name: d for name, d in dirs.items() if d}
    if len(dirs) < 2:
        raise SystemExit("give at least two frameworks' binaries (--zgx, --gpui-fast, --mygo, --gpui)")
    args.output.mkdir(parents=True, exist_ok=True)
    screen_reader, uia_clients = accessibility_state()
    print(f"screen reader running: {screen_reader}; UI Automation clients running: {', '.join(uia_clients) or 'none known'}")
    results, binaries, failures = {}, {}, []
    rounds = 0

    def round_of(scene, mode, frameworks, index):
        shift = index % len(frameworks)
        if args.desktop:
            row = desktop(args.seconds, args.warmup)
            results.setdefault((scene, mode, "desktop"), []).append(row)
            print(f"{scene:7} {mode:6} {'desktop':9} dwm CPU {row['dwm_mcycles_per_second']:6.1f} Mcyc/s  "
                  f"dwm GPU {row['dwm_gpu_ms_per_second']:6.2f} ms/s", flush=True)
        for name in frameworks[shift:] + frameworks[:shift]:
            path = binaries[f"{scene}/{name}"]
            log = args.output / f"{scene}.{mode}.{name}.{len(results.get((scene, mode, name), []))}.log"
            for attempt in (1, 2):
                try:
                    row = run(path, mode, args.seconds, args.warmup, log)
                    break
                except Invalid as error:
                    if attempt == 2:
                        failures.append(str(error))
                        print(f"INVALID {error}", flush=True)
                        row = None
            if row is None:
                continue
            results.setdefault((scene, mode, name), []).append(row)
            print(f"{scene:7} {mode:6} {name:9} CPU {row['cpu_percent']:6.2f}%  "
                  f"{row['mcycles_per_second']:8.1f} Mcyc/s  {(row['mcycles_per_unit'] or 0):7.2f} Mcyc/unit  "
                  f"units {row['units']:5}  WS {row['working_set_mib']:6.1f}  private {row['private_mib']:6.1f} MiB  "
                  f"dwm CPU {row['dwm_mcycles_per_second']:6.1f} Mcyc/s  dwm GPU {row['dwm_gpu_ms_per_second']:6.2f} ms/s  "
                  f"GPU {row['gpu_ms_per_second']:6.2f} ms/s"
                  f"{'  UIA client attached' if row['uia_client_attached'] else ''}", flush=True)

    # Zeron, translucent and animated, minimized while the runs go (restored after).
    power = PowerPlan(HIGH_PERFORMANCE if args.power_plan == "high" else None)
    with power, Minimized([t for t in ("Zeron",) if t not in args.keep]) as hidden:
        for scene in args.scenes.split(","):
            modes, names = SCENES[scene]
            frameworks = [n for n in FRAMEWORKS if n in dirs and n in names and (dirs[n] / names[n]).exists()]
            for name in frameworks:
                binaries[f"{scene}/{name}"] = dirs[name] / names[name]
            if len(frameworks) < 2:
                print(f"{scene}: fewer than two frameworks have its binary; skipped")
                continue
            for mode in modes:
                for index in range(args.repeats):
                    round_of(scene, mode, frameworks, index)
                index = args.repeats
                while index < args.max_repeats:
                    spreads = []
                    for name in frameworks:
                        values = key_metric(results.get((scene, mode, name), []))
                        if len(values) >= 2:
                            spreads.append((max(values) - min(values)) / statistics.median(values))
                    if not spreads or max(spreads) <= args.spread:
                        break
                    print(f"{scene} {mode}: spread {max(spreads):.1%}, another round", flush=True)
                    round_of(scene, mode, frameworks, index)
                    index += 1
    summary = []
    for (scene, mode, name), rows in results.items():
        median = {key: statistics.median(r[key] for r in rows if r[key] is not None)
                  for key in rows[0]
                  if not isinstance(rows[0][key], bool) and any(r[key] is not None for r in rows)}
        values = key_metric(rows) if name != "desktop" else []
        summary.append({"scene": scene, "mode": mode, "framework": name, "median": median,
                        "spread": (max(values) - min(values)) / statistics.median(values) if values else None,
                        "uia_client_attached_runs": sum(r.get("uia_client_attached", False) for r in rows),
                        "window_uncovered_runs": sum(r.get("window_uncovered", False) for r in rows),
                        "runs": rows})
    metadata = {
        "platform": platform.platform(),
        "processor": platform.processor(),
        "seconds": args.seconds,
        "warmup": args.warmup,
        "repeats": args.repeats,
        "max_repeats": args.max_repeats,
        "spread": args.spread,
        "screen_reader_running": screen_reader,
        "uia_client_processes_running": uia_clients,
        "invalid_runs": failures,
        # Each run's window was brought forward and found uncovered (else invalid).
        "windows_minimized": [win.title(h) for h in hidden.hwnds],
        "power_plan": "High performance" if power.guid else "the user's",
        "font_file": str(font_file) if font_file else None,
        "binary_bytes": {key: os.path.getsize(path) for key, path in binaries.items()},
        "binary_sha256": {key: hashlib.sha256(pathlib.Path(path).read_bytes()).hexdigest()
                          for key, path in binaries.items()},
    }
    (args.output / "summary.json").write_text(json.dumps({"metadata": metadata, "results": summary}, indent=1))
    shown = [n for n in FRAMEWORKS if n in dirs] + (["desktop"] if args.desktop else [])
    by = {(s["scene"], s["mode"], s["framework"]): s["median"] for s in summary}
    print("\nscene   mode    metric              " + "".join(f"{n:>11}" for n in shown))
    for scene in args.scenes.split(","):
        for mode in SCENES[scene][0]:
            for key in ("mcycles_per_unit", "mcycles_per_second", "cpu_percent", "private_mib", "working_set_mib",
                        "dwm_mcycles_per_second", "dwm_cpu_percent", "dwm_gpu_ms_per_second", "gpu_ms_per_second",
                        "dwm_gpu_us_per_unit",
                        "gpu_us_per_unit"):
                cells = [by.get((scene, mode, n), {}).get(key) for n in shown]
                if sum(c is not None for c in cells) < 2:
                    continue
                print(f"{scene:7} {mode:7} {key:19} " + "".join(f"{c:11.2f}" if c is not None else f"{'-':>11}"
                                                              for c in cells))
    if failures:
        print(f"\n{len(failures)} invalid runs, see summary.json")


if __name__ == "__main__":
    main()
