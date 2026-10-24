#!/usr/bin/env python3
"""Measure what an example costs at rest on Windows: CPU cycles and time while
its window sits untouched, then while the pointer rests over it.

    cargo build --release -p zgx-desktop --example gallery
    python scripts/windows_idle_probe.py target/release/examples/gallery.exe

Cycles (QueryProcessCycleTime) resolve what CPU time (a 15.6 ms scheduler
tick) cannot; a window at rest should spend close to none.
"""
import argparse
import os
import pathlib
import subprocess
import time

import windows_input as win


def window_cost(pid, seconds, threads=False):
    cycles, cpu, before = win.cycles(pid), win.cpu_seconds(pid), win.thread_cycles(pid)
    time.sleep(seconds)
    if threads:
        after = win.thread_cycles(pid)
        busy = sorted(((after[k] - before.get(k, 0), k) for k in after), reverse=True)
        for spent, (tid, name) in busy[:6]:
            print(f"    thread {tid} {name or '(unnamed)'}: {spent / seconds / 1e6:.2f} M cycles/s")
    return win.cycles(pid) - cycles, win.cpu_seconds(pid) - cpu


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("binary", type=pathlib.Path)
    parser.add_argument("--seconds", type=float, default=10)
    parser.add_argument("--settle", type=float, default=3, help="seconds to wait after the window shows")
    parser.add_argument("--env", action="append", default=[], metavar="NAME=VALUE",
                        help="set in the app's environment (repeatable)")
    parser.add_argument("--max-cycles-per-second", type=float,
                        help="fail when the window at rest uses more than this")
    args = parser.parse_args()
    env = dict(os.environ, **dict(pair.split("=", 1) for pair in args.env))
    proc = subprocess.Popen([str(args.binary.resolve())], env=env, stdout=subprocess.DEVNULL,
                            stderr=subprocess.DEVNULL)
    try:
        window = win.wait_window(proc)
        time.sleep(args.settle)
        print("at rest, by thread:")
        rest = window_cost(proc.pid, args.seconds, threads=True)
        width, height = win.client_size(window)
        win.move(window, width // 2, height // 2)
        time.sleep(args.settle)
        print("hovered, by thread:")
        hover = window_cost(proc.pid, args.seconds, threads=True)
        private, working = win.memory(proc.pid)
        print(f"memory: {private / 2**20:.1f} MiB private, {working / 2**20:.1f} MiB working set")
        for name, (cycles, cpu) in [("at rest", rest), ("pointer resting over it", hover)]:
            print(f"{name}: {cycles / args.seconds / 1e6:.2f} M cycles/s, "
                  f"{cpu / args.seconds * 1000:.2f} ms CPU/s")
        if args.max_cycles_per_second is not None:
            for name, (cycles, _) in [("at rest", rest), ("hovered", hover)]:
                if cycles / args.seconds > args.max_cycles_per_second:
                    raise AssertionError(f"{name}: {cycles / args.seconds:.0f} cycles/s over the limit")
        win.close(window)
        proc.wait(timeout=10)
    finally:
        if proc.poll() is None:
            proc.kill()
            proc.wait()


if __name__ == "__main__":
    main()
