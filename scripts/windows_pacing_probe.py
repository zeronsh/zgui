#!/usr/bin/env python3
"""How evenly an animating example's frames reach the display on Windows.

Runs the example with ZGX_PRESENT_LOG=1 and reads DXGI's account of each shown
present (crates/zgx-gpu/src/dxgi.rs): the refresh it went up on. Animating at
the display's rate, each shown present should follow the last by exactly one
refresh; a gap of two is a refresh that repeated the previous frame (a hitch).

    cargo build --release -p zgx-desktop --example animation_workload
    python scripts/windows_pacing_probe.py target/release/examples/animation_workload.exe
"""
import argparse
import collections
import os
import pathlib
import re
import subprocess

import windows_input as win

SHOWN = re.compile(r"dxgi shown present=\d+ refresh=\d+ \+(\d+) presents \+(\d+) refreshes queued=(\d+)")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("binary", type=pathlib.Path)
    parser.add_argument("--seconds", type=float, default=10)
    parser.add_argument("--skip", type=int, default=30, help="shown presents to ignore while starting")
    parser.add_argument("--env", action="append", default=[], metavar="NAME=VALUE")
    parser.add_argument("--max-hitch-percent", type=float)
    args = parser.parse_args()
    env = dict(os.environ, ZGX_PRESENT_LOG="1", ZGX_SECONDS=str(args.seconds),
               **dict(pair.split("=", 1) for pair in args.env))
    proc = subprocess.Popen([str(args.binary.resolve())], env=env, stdout=subprocess.DEVNULL,
                            stderr=subprocess.PIPE, text=True)
    try:
        window = win.wait_window(proc)
        win.activate(window)
        _, err = proc.communicate(timeout=args.seconds + 30)
    finally:
        if proc.poll() is None:
            proc.kill()
            proc.wait()
    steps = [tuple(map(int, m.groups())) for m in SHOWN.finditer(err)][args.skip:]
    if not steps:
        raise SystemExit("no DXGI statistics in the log:\n" + err[-2000:])
    gaps = collections.Counter()
    queued = collections.Counter()
    for presents, refreshes, depth in steps:
        # Each present beyond the first in a step went up on a refresh of its own.
        gaps[refreshes - (presents - 1)] += 1
        queued[depth] += 1
    total = sum(gaps.values())
    hitches = sum(count for gap, count in gaps.items() if gap > 1)
    print(f"{total} shown presents; refreshes since the previous one:")
    for gap, count in sorted(gaps.items()):
        print(f"  {gap}: {count} ({count / total:.1%})")
    print("presents queued behind the shown one:", dict(sorted(queued.items())))
    print(f"hitches: {hitches / total:.2%}")
    if args.max_hitch_percent is not None and hitches / total * 100 > args.max_hitch_percent:
        raise SystemExit(f"FAIL: more than {args.max_hitch_percent}% hitches")


if __name__ == "__main__":
    main()
