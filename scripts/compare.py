#!/usr/bin/env python3
"""Measure built GUI processes on Linux. Never substitutes a headless microbenchmark."""
import argparse
import csv
import json
import hashlib
import os
import pathlib
import platform
import re
import subprocess
import time


def sample(pid):
    try:
        raw = pathlib.Path(f"/proc/{pid}/stat").read_text()
        fields = raw[raw.rfind(")") + 2:].split()
        if fields[0] == "Z":
            return None
        ticks = int(fields[11]) + int(fields[12])
        rss = int(fields[21]) * os.sysconf("SC_PAGE_SIZE")
        # A process can release its address space before /proc reports Z.
        # Do not fold that terminal observation into the live-process RSS mean.
        if rss <= 0:
            return None
        return ticks / os.sysconf("SC_CLK_TCK"), rss
    except (FileNotFoundError, ProcessLookupError):
        return None


def run_trial(path, name, mode, repeat, seconds, warmup, output):
    """Return validated summary; preserve raw evidence even on a failed trial."""
    env = dict(os.environ, ZGX_MODE=mode, ZGX_SECONDS=str(seconds), ZGX_INITIAL_TICKS="0")
    start = time.monotonic()
    samples = []
    code = None
    row = None
    application_reports = []
    failure = None
    try:
        with open(f"{output}.{name}.{mode}.{repeat}.log", "w") as log:
            proc = subprocess.Popen([str(path)], env=env, stdout=log, stderr=log)
            try:
                while proc.poll() is None:
                    elapsed = time.monotonic() - start
                    if elapsed > seconds + 20:
                        proc.kill()
                        raise RuntimeError(f"{name} did not terminate")
                    value = sample(proc.pid)
                    if value and elapsed >= warmup:
                        samples.append((time.monotonic(), *value))
                    time.sleep(.05)
            finally:
                if proc.poll() is None:
                    proc.kill()
                code = proc.wait()
        if code or len(samples) < 2:
            raise RuntimeError(f"{name}/{mode} failed (exit={code}, samples={len(samples)}); inspect log")
        log_text = pathlib.Path(f"{output}.{name}.{mode}.{repeat}.log").read_text()
        ticks_match = re.search(r'workload_ticks=(\d+)|"ticks":(\d+)', log_text)
        if not ticks_match:
            raise RuntimeError(f"{name}/{mode} did not report completed workload ticks")
        ticks = int(next(g for g in ticks_match.groups() if g is not None))
        if (mode == "idle" and ticks != 0) or (mode != "idle" and ticks == 0):
            raise RuntimeError(f"{name}/{mode} reported incompatible workload ticks: {ticks}")
        wall = samples[-1][0] - samples[0][0]
        cpu = samples[-1][1] - samples[0][1]
        row = dict(framework=name, mode=mode, repeat=repeat, wall_seconds=wall, cpu_seconds=cpu,
                   cpu_percent_one_core=100 * cpu / wall, mean_rss_bytes=sum(v[2] for v in samples) / len(samples),
                   peak_rss_bytes=max(v[2] for v in samples), samples=len(samples), workload_ticks=ticks, exit_code=code)
        application_reports = []
        for line in log_text.splitlines():
            try:
                report = json.loads(line)
            except json.JSONDecodeError:
                continue
            if isinstance(report, dict):
                application_reports.append(report)
        return row
    except BaseException as error:
        failure = f"{type(error).__name__}: {error}"
        raise
    finally:
        raw = {
            "summary": row if failure is None else None,
            "requested_seconds": seconds,
            "warmup_seconds": warmup,
            "process_wall_seconds": time.monotonic() - start,
            "application_reports": application_reports,
            "samples": [{"elapsed_seconds": timestamp - start, "cpu_seconds": cpu_seconds, "rss_bytes": rss_bytes}
                        for timestamp, cpu_seconds, rss_bytes in samples],
        }
        if failure is not None:
            raw.update(framework=name, mode=mode, repeat=repeat, exit_code=code, failure_reason=failure)
        pathlib.Path(f"{output}.{name}.{mode}.{repeat}.json").write_text(json.dumps(raw, indent=2))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--zgx", required=True, type=pathlib.Path)
    parser.add_argument("--gpui", required=True, type=pathlib.Path)
    parser.add_argument("--quickgui", required=True, type=pathlib.Path)
    parser.add_argument("--seconds", default=30., type=float)
    parser.add_argument("--warmup", default=5., type=float)
    parser.add_argument("--repeats", default=3, type=int)
    parser.add_argument("--output", default="comparison.csv")
    args = parser.parse_args()
    if platform.system() != "Linux":
        parser.error("this sampler uses Linux /proc; run GUI adapters on Linux")
    if not os.environ.get("DISPLAY") and not os.environ.get("WAYLAND_DISPLAY"):
        parser.error("a real desktop display is required; no GUI performance claims from headless runs")
    if args.seconds <= args.warmup or args.warmup < 0 or args.repeats < 1:
        parser.error("require seconds > warmup >= 0 and repeats >= 1")
    binaries = [(name, getattr(args, name).resolve()) for name in ("zgx", "gpui", "quickgui")]
    for name, path in binaries:
        if not path.is_file() or not os.access(path, os.X_OK):
            parser.error(f"missing executable: {name}: {path}")
    metadata = {"platform": platform.platform(), "machine": platform.machine(), "python": platform.python_version(), "arguments": {k: str(v) for k, v in vars(args).items()}, "display": {k: os.environ.get(k) for k in ("DISPLAY", "WAYLAND_DISPLAY", "WGPU_BACKEND", "WINIT_UNIX_BACKEND", "ZGX_RENDERER", "ZGX_EFFECTS", "DBUS_SESSION_BUS_ADDRESS", "VK_ICD_FILENAMES", "LIBGL_ALWAYS_SOFTWARE")}, "gpui_version": "0.2.2", "quickgui_revision": "811d6e2816d5229711f59683c4c9dfbb6fc74133"}
    for key, command in (("rustc", ["rustc", "--version"]), ("commit", ["git", "rev-parse", "HEAD"]), ("cpu", ["lscpu"])):
        result = subprocess.run(command, text=True, capture_output=True, check=False)
        metadata[key] = result.stdout.strip() if result.returncode == 0 else "unavailable (command failed)"
    metadata["binary_sha256"] = {name: hashlib.file_digest(open(path, "rb"), "sha256").hexdigest() for name, path in binaries}
    metadata["source_sha256"] = {
        str(path): hashlib.file_digest(open(path, "rb"), "sha256").hexdigest()
        for base in (pathlib.Path("crates"), pathlib.Path("comparisons"))
        for path in base.rglob("*")
        if path.is_file() and path.suffix in (".rs", ".toml", ".lock") and "target" not in path.parts
    }
    metadata["harness_sha256"] = hashlib.file_digest(open(__file__, "rb"), "sha256").hexdigest()
    metadata["note"] = "Record physical GPU/driver and display conditions separately. Xvfb/llvmpipe measurements are smoke data, not GPU performance evidence."
    pathlib.Path(args.output + ".metadata.json").write_text(json.dumps(metadata, indent=2))
    fields = ["framework", "mode", "repeat", "wall_seconds", "cpu_seconds", "cpu_percent_one_core", "mean_rss_bytes", "peak_rss_bytes", "samples", "workload_ticks", "exit_code"]
    with open(args.output, "w", newline="") as out:
        writer = csv.DictWriter(out, fieldnames=fields)
        writer.writeheader()
        for repeat in range(args.repeats):
            # Rotate ordering to reduce systematic thermal/order bias.
            ordered = binaries[repeat % 3:] + binaries[:repeat % 3]
            for mode in ("idle", "stream", "scroll", "both"):
                for name, path in ordered:
                    row = run_trial(path, name, mode, repeat, args.seconds, args.warmup, args.output)
                    writer.writerow(row)
                    out.flush()
                    print(f"{name:9} {mode:6} CPU {row['cpu_percent_one_core']:7.2f}% RSS {row['mean_rss_bytes']/1048576:8.2f} MiB", flush=True)


if __name__ == "__main__":
    main()
