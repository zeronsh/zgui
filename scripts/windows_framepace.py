#!/usr/bin/env python3
"""A window's frame pacing as shown, for any framework: the Windows
counterpart of scripts/framepace.swift. Captures the window's client area
through Desktop Duplication (`framepace`, a zgx-gpu example) after a
settling time, and gives the times between frames whose pixels changed:
frames a second, the 50th, 95th and 99th percentile and longest interval,
and the share of intervals over 25 ms (a refresh missed at 60 Hz); and
how many captured frames covered several compositions (`merged_frames`: the
capture fell behind; about 0 when it keeps up).

    cargo build --release -p zgx-gpu --example framepace
    python scripts/windows_framepace.py <framepace.exe> <hwnd> [--settle 5] [--seconds 8]

Used by scripts/run_windows_suite.py; the window must be in front and
uncovered for the whole capture (what covers it is what is captured).
"""
import argparse
import json
import subprocess
import time

import windows_input as win

LONG_MS = 25.0


def capture(framepace, hwnd, settle=5.0, seconds=8.0):
    """The pacing of `hwnd`'s client area, as framepace.swift reports it."""
    time.sleep(settle)
    width, height = win.client_size(hwnd)
    x, y = win.to_screen(hwnd, 0, 0)
    out = subprocess.run([str(framepace), str(x), str(y), str(width), str(height), str(int(seconds))],
                         capture_output=True, text=True, timeout=seconds + 30)
    hz, frames, composed, merged = 0, [], 0, 0
    for line in out.stdout.splitlines():
        kind, _, value = line.partition(" ")
        if kind == "HZ":
            hz = int(value)
        elif kind == "FRAME":
            frames.append(float(value.split()[0]))
        elif kind == "COMPOSED":
            composed = int(value)
        elif kind == "MERGED":
            merged = int(value)
    intervals = sorted(b - a for a, b in zip(frames, frames[1:]))
    pick = lambda q: intervals[round((len(intervals) - 1) * q)] if intervals else 0.0  # noqa: E731
    long = sum(i > LONG_MS for i in intervals)
    return {
        "display_hz": hz,
        "frames": len(frames),
        "composed": composed,
        "merged_frames": merged,
        "frames_per_second": len(frames) / seconds,
        "interval_p50_ms": pick(0.5),
        "interval_p95_ms": pick(0.95),
        "interval_p99_ms": pick(0.99),
        "interval_max_ms": intervals[-1] if intervals else 0.0,
        "long_intervals": long,
        "long_interval_fraction": long / len(intervals) if intervals else 0.0,
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("framepace")
    parser.add_argument("hwnd", type=lambda v: int(v, 0))
    parser.add_argument("--settle", type=float, default=5)
    parser.add_argument("--seconds", type=float, default=8)
    args = parser.parse_args()
    print(json.dumps(capture(args.framepace, args.hwnd, args.settle, args.seconds)))


if __name__ == "__main__":
    main()
