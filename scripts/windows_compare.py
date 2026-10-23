#!/usr/bin/env python3
"""zgui against GPUI on Windows: the shared comparison workloads
(`comparisons/workload`), each framework's adapter built in release, run in
turn on the same desktop. The Windows counterpart of `scripts/compare.py`
(whose `/proc` sampler is Linux's).

Scenes (zgui example / GPUI binary):

- `lab` (`component_workload` / `zgui-compare-gpui`), in each mode: `idle`,
  `stream` (text streaming), `scroll`, `both`;
- `animate` (`animation_workload` / `animate`): display-rate animation;
- `heavy` (`heavy_workload` / `heavy`): the dashboard, every live value;
- `busy` (`busy_workload` / `busy`): a busy window with one spinner.

Each run is sampled every 100 ms after `--warmup` seconds: CPU time
(`GetProcessTimes`), CPU cycles (`QueryProcessCycleTime`, finer than CPU
time's scheduler ticks), working set and private bytes. The frameworks
alternate in rotating order over `--repeats`. A run must report the work it
did (`workload_ticks`, `"ticks"` or `animation_frames`), and cycles are also
given per unit of it, so a run that did less work cannot look cheaper.

    cargo build --profile dist --locked -p zgui-desktop --example component_workload  (and the other three, one per command)
    cargo build --profile dist --locked --manifest-path comparisons/gpui/Cargo.toml
    python scripts/windows_compare.py --zgui target/dist/examples --gpui comparisons/gpui/target/dist --output target/windows-compare

Keep the windows uncovered, the machine otherwise idle, and on mains power.
"""
import argparse
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

SCENES = {
    "lab": ("component_workload.exe", "zgui-compare-gpui.exe", ("idle", "stream", "scroll", "both")),
    "animate": ("animation_workload.exe", "animate.exe", ("animate",)),
    "heavy": ("heavy_workload.exe", "heavy.exe", ("both",)),
    "busy": ("busy_workload.exe", "busy.exe", ("busy",)),
}


def units(text):
    """The work a run reports it did."""
    for pattern in (r"workload_ticks=(\d+)", r'"ticks":(\d+)', r"animation_frames=(\d+)"):
        match = re.search(pattern, text)
        if match:
            return int(match.group(1))
    return None


def run(path, mode, seconds, warmup, log_path):
    env = dict(os.environ, ZGUI_MODE=mode, ZGUI_SECONDS=str(seconds), ZGUI_INITIAL_TICKS="0")
    started = time.perf_counter()
    samples = []
    with open(log_path, "w") as log:
        proc = subprocess.Popen([str(path)], env=env, stdout=log, stderr=log)
        try:
            while proc.poll() is None:
                elapsed = time.perf_counter() - started
                if elapsed > seconds + 30:
                    raise RuntimeError(f"{path.name} did not end")
                if elapsed >= warmup:
                    try:
                        private, working = win.memory(proc.pid)
                        samples.append((time.perf_counter(), win.cpu_seconds(proc.pid), win.cycles(proc.pid),
                                        working, private))
                    except OSError:
                        pass
                time.sleep(0.1)
        finally:
            if proc.poll() is None:
                proc.kill()
            code = proc.wait()
    text = pathlib.Path(log_path).read_text(errors="replace")
    done = units(text)
    # The last sample can be of a process already releasing its memory.
    samples = [s for s in samples if s[3] > 0][:-1]
    if code or len(samples) < 10 or done is None:
        raise RuntimeError(f"{path.name} {mode}: exit {code}, {len(samples)} samples, work {done}; see {log_path}")
    wall = samples[-1][0] - samples[0][0]
    cpu = samples[-1][1] - samples[0][1]
    cycles = samples[-1][2] - samples[0][2]
    # The work over the sampled span, of the whole run's (`seconds`).
    sampled_units = done * wall / seconds if done else 0
    return {
        "cpu_percent": 100 * cpu / wall,
        "mcycles_per_second": cycles / wall / 1e6,
        "mcycles_per_unit": (cycles / 1e6 / sampled_units) if sampled_units else None,
        "units": done,
        "working_set_mib": statistics.mean(s[3] for s in samples) / 2**20,
        "peak_working_set_mib": max(s[3] for s in samples) / 2**20,
        "private_mib": statistics.mean(s[4] for s in samples) / 2**20,
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--zgui", type=pathlib.Path, required=True, help="zgui's release examples")
    parser.add_argument("--gpui", type=pathlib.Path, required=True, help="the GPUI adapter's release binaries")
    parser.add_argument("--scenes", default=",".join(SCENES))
    parser.add_argument("--seconds", type=float, default=15)
    parser.add_argument("--warmup", type=float, default=4)
    parser.add_argument("--repeats", type=int, default=3)
    parser.add_argument("--output", type=pathlib.Path, default=pathlib.Path("target/windows-compare"))
    args = parser.parse_args()
    args.output.mkdir(parents=True, exist_ok=True)
    results = {}
    binaries = {}
    for scene in args.scenes.split(","):
        zgui, gpui, modes = SCENES[scene]
        pair = {"zgui": args.zgui / zgui, "gpui": args.gpui / gpui}
        binaries.update({f"{scene}/{name}": str(path) for name, path in pair.items()})
        for mode in modes:
            for repeat in range(args.repeats):
                order = list(pair.items()) if repeat % 2 == 0 else list(reversed(pair.items()))
                for name, path in order:
                    log = args.output / f"{scene}.{mode}.{name}.{repeat}.log"
                    row = run(path, mode, args.seconds, args.warmup, log)
                    results.setdefault((scene, mode, name), []).append(row)
                    print(f"{scene:7} {mode:6} {name:4} CPU {row['cpu_percent']:6.2f}%  "
                          f"{row['mcycles_per_second']:8.1f} Mcyc/s  "
                          f"{(row['mcycles_per_unit'] or 0):7.2f} Mcyc/unit  units {row['units']:5}  "
                          f"WS {row['working_set_mib']:6.1f} MiB  private {row['private_mib']:6.1f} MiB", flush=True)
    summary = []
    for (scene, mode, name), rows in results.items():
        median = {key: statistics.median(r[key] for r in rows if r[key] is not None)
                  for key in rows[0] if any(r[key] is not None for r in rows)}
        spread = {key: [min(r[key] for r in rows), max(r[key] for r in rows)]
                  for key in ("mcycles_per_second", "working_set_mib")}
        summary.append({"scene": scene, "mode": mode, "framework": name, "median": median, "range": spread,
                        "runs": rows})
    sizes = {key: os.path.getsize(path) for key, path in binaries.items()}
    metadata = {
        "platform": platform.platform(),
        "processor": platform.processor(),
        "seconds": args.seconds,
        "warmup": args.warmup,
        "repeats": args.repeats,
        "binary_bytes": sizes,
        "binary_sha256": {key: hashlib.sha256(pathlib.Path(path).read_bytes()).hexdigest()
                          for key, path in binaries.items()},
    }
    (args.output / "summary.json").write_text(json.dumps({"metadata": metadata, "results": summary}, indent=1))
    print("\nscene   mode   metric                 zgui        gpui")
    by = {(s["scene"], s["mode"], s["framework"]): s["median"] for s in summary}
    for scene in args.scenes.split(","):
        for mode in SCENES[scene][2]:
            z, g = by.get((scene, mode, "zgui")), by.get((scene, mode, "gpui"))
            if not z or not g:
                continue
            for key in ("cpu_percent", "mcycles_per_second", "mcycles_per_unit", "working_set_mib", "private_mib"):
                if key in z and key in g:
                    print(f"{scene:7} {mode:6} {key:20} {z[key]:10.2f}  {g[key]:10.2f}")
    for key, size in sizes.items():
        print(f"binary {key:22} {size / 2**20:8.2f} MiB")


if __name__ == "__main__":
    main()
