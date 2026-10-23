#!/usr/bin/env python3
"""Startup to first frame on Windows, the same way for every framework: spawn
the binary, wait for its window, then capture its header strip until the
header's text has drawn (light pixels on the dark background). Reports, per
binary, the time from spawn to the window and to the text, and the process's
CPU time and cycles at that point, over `--launches` alternating launches.

    python scripts/windows_startup_probe.py target/dist/examples/component_workload.exe comparisons/gpui/target/dist/zgui-compare-gpui.exe

Both comparison adapters draw "zgui performance lab" at (20, 20) logical
pixels, 20 px. Medians of cycles say more than wall times, which swing with
the system and the compositor.
"""
import argparse
import os
import pathlib
import statistics
import subprocess
import time

import windows_input as win


def drawn(window, scale):
    x, y, w, h = (round(v * scale) for v in (20, 20, 240, 28))
    width, height, data = win.capture(window)
    light = 0
    for row in range(y, min(y + h, height)):
        for column in range(x, min(x + w, width), 2):
            b, g, r, _ = data[(row * width + column) * 4:(row * width + column) * 4 + 4]
            if min(r, g, b) > 170:
                light += 1
    return light > 30


def launch(binary):
    env = dict(os.environ, ZGUI_MODE="idle", ZGUI_SECONDS="8", ZGUI_INITIAL_TICKS="0")
    started = time.perf_counter()
    proc = subprocess.Popen([binary], env=env, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    try:
        window = None
        while window is None:
            if time.perf_counter() - started > 20:
                raise RuntimeError(f"{binary}: no window")
            found = win.windows_of(proc.pid)
            window = found[0] if found else None
        shown = time.perf_counter() - started
        scale = win.dpi(window) / 96
        while not drawn(window, scale):
            if time.perf_counter() - started > 20:
                raise RuntimeError(f"{binary}: the header never drew")
        first = time.perf_counter() - started
        return shown * 1000, first * 1000, win.cpu_seconds(proc.pid) * 1000, win.cycles(proc.pid) / 1e6
    finally:
        proc.kill()
        proc.wait()


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("binaries", nargs="+")
    parser.add_argument("--launches", type=int, default=10)
    args = parser.parse_args()
    binaries = [str(pathlib.Path(b).resolve()) for b in args.binaries]
    results = {b: [] for b in binaries}
    for _ in range(args.launches):
        for binary in binaries:
            results[binary].append(launch(binary))
            time.sleep(0.5)
    for binary, runs in results.items():
        column = lambda i: statistics.median(run[i] for run in runs)
        print(f"{pathlib.Path(binary).name}: window {column(0):.0f} ms, first text {column(1):.0f} ms, "
              f"CPU {column(2):.0f} ms, {column(3):.0f} M cycles by then (medians of {len(runs)})")


if __name__ == "__main__":
    main()
