#!/usr/bin/env python3
"""Startup to first frame on Windows, the same way for every framework: spawn
the binary, wait for its window, then read its header strip until the
header's text has drawn (light pixels on the dark background). Reports, per
binary, the time from spawn to the window and to the text, and the process's
CPU time and cycles at that point, over `--launches` launches in rotating
order. With `--settle`, each process then runs on, idle, that many seconds,
and its private bytes and working set are taken: the memory it holds at rest.

    python scripts/windows_startup_probe.py target/dist/examples/component_workload.exe comparisons/gpui-fast/target/dist/zgx-compare-gpui-fast.exe comparisons/mygo/bin/lab.exe --settle 5

The comparison adapters (zgx, GPUI Fast, MyGo) all draw "zgx performance
lab" at (20, 20) logical pixels, 20 px. The window is read as it drew
itself (`PrintWindow`), so another window covering it cannot pass for its
text. Medians of cycles say more than wall times, which swing with the
system and the compositor. A zgx launch also says what its kept shaders did
by its first present (its startup marks, `ZGX_PROFILE`): a start that
compiled them is slower by about 45 ms, and says so.
"""
import argparse
import os
import pathlib
import re
import statistics
import subprocess
import tempfile
import time

import windows_input as win


def drawn(window, scale):
    x, y, w, h = (round(v * scale) for v in (20, 20, 240, 28))
    width, height, data = win.capture_window(window)
    light = 0
    for row in range(y, min(y + h, height)):
        for column in range(x, min(x + w, width), 2):
            b, g, r, _ = data[(row * width + column) * 4:(row * width + column) * 4 + 4]
            if min(r, g, b) > 170:
                light += 1
    return light > 30


SHADERS = re.compile(r"first frame presented \(shaders: (\d+) read, (\d+) compiled, (\d+) not kept\)")


def shaders_of(log):
    """What zgx's kept shaders did by its first present, from its startup
    mark (`ZGX_PROFILE`): None for another framework, or a zgx without it."""
    found = SHADERS.search(pathlib.Path(log).read_text(errors="replace"))
    return dict(zip(("read", "compiled", "not_kept"), map(int, found.groups()))) if found else None


def launch(binary, settle):
    """(window ms, first text ms, CPU ms, M cycles, shaders[, private MiB,
    working set MiB]): `shaders` is what zgx's kept shaders did by its first
    present (`shaders_of`), so a start slowed by compiling them says so."""
    # ZGUI_*: adapters built before the rename read those alone. ZGX_PROFILE:
    # zgx's startup marks, the shaders' among them, on stderr.
    env = dict(os.environ, ZGX_MODE="idle", ZGX_SECONDS=str(settle + 8), ZGX_INITIAL_TICKS="0",
               ZGUI_MODE="idle", ZGUI_SECONDS=str(settle + 8), ZGUI_INITIAL_TICKS="0", ZGX_PROFILE="1")
    log = tempfile.NamedTemporaryFile("w", suffix=".log", delete=False)
    started = time.perf_counter()
    proc = subprocess.Popen([binary], env=env, stdout=subprocess.DEVNULL, stderr=log)
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
        startup = (shown * 1000, first * 1000, win.cpu_seconds(proc.pid) * 1000, win.cycles(proc.pid) / 1e6)
        if settle:
            time.sleep(settle)
            private, working = win.memory(proc.pid)
    finally:
        proc.kill()
        proc.wait()
        log.close()
    shaders = shaders_of(log.name)
    os.unlink(log.name)
    return (*startup, shaders, private / 2**20, working / 2**20) if settle else (*startup, shaders)


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("binaries", nargs="+")
    parser.add_argument("--launches", type=int, default=10)
    parser.add_argument("--settle", type=float, default=0,
                        help="seconds at rest after the first frame before taking its memory")
    args = parser.parse_args()
    binaries = [str(pathlib.Path(b).resolve()) for b in args.binaries]
    results = {b: [] for b in binaries}
    for launch_index in range(args.launches):
        # A different one first each round: none always starts after the same.
        shift = launch_index % len(binaries)
        for binary in binaries[shift:] + binaries[:shift]:
            results[binary].append(launch(binary, args.settle))
            time.sleep(0.5)
    for binary, runs in results.items():
        column = lambda i: statistics.median(run[i] for run in runs)
        memory = (f", at rest {column(5):.1f} MiB private, {column(6):.1f} MiB working set"
                  if args.settle else "")
        compiled = [run[4]["compiled"] for run in runs if run[4]]
        shaders = f"; shaders compiled in {sum(c > 0 for c in compiled)} of {len(compiled)}" if compiled else ""
        print(f"{pathlib.Path(binary).name}: window {column(0):.0f} ms, first text {column(1):.0f} ms, "
              f"CPU {column(2):.0f} ms, {column(3):.0f} M cycles by then{memory} (medians of {len(runs)}){shaders}")


if __name__ == "__main__":
    main()
