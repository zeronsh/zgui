#!/usr/bin/env python3
"""Keystroke to screen on Windows: how long after SendInput types a character
into a focused text field its pixels change in what the compositor shows.

    cargo build --release -p zgui-desktop --example gallery
    python scripts/windows_latency_probe.py target/release/examples/gallery.exe
    python scripts/windows_latency_probe.py notepad.exe --click 200,200 --region 0,0,400,60

The screen is read back through GDI, which sees the compositor's last frame,
so this is input to composition (scan-out adds up to a refresh more). The
same probe on Notepad gives the native baseline on the same machine.
"""
import argparse
import os
import random
import statistics
import subprocess
import time

import windows_input as win


def region_pixels(window, region):
    x, y, w, h = region
    width, height, data = win.capture(window)
    rows = []
    for row in range(y, min(y + h, height)):
        start = (row * width + x) * 4
        rows.append(data[start:start + min(w, width - x) * 4])
    return b"".join(rows)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("binary")
    parser.add_argument("--samples", type=int, default=40)
    parser.add_argument("--click", default="80,430", help="logical x,y of the text field to type into")
    parser.add_argument("--region", default="24,412,520,36",
                        help="logical x,y,w,h of the field's pixels to watch")
    parser.add_argument("--env", action="append", default=[], metavar="NAME=VALUE")
    parser.add_argument("--press", action="store_true",
                        help="press the key (its virtual key) rather than type the character: for "
                             "apps that count key presses, as the comparison adapters' `keys` do")
    args = parser.parse_args()
    env = dict(os.environ, **dict(pair.split("=", 1) for pair in args.env))
    binary = os.path.abspath(args.binary) if os.path.exists(args.binary) else args.binary
    proc = subprocess.Popen([binary], env=env, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    try:
        window = win.wait_window(proc)
        scale = win.dpi(window) / 96
        time.sleep(1.5)
        win.activate(window)
        cx, cy = (float(v) * scale for v in args.click.split(","))
        win.click(window, cx, cy)
        win.key("ctrl+a", "delete")
        time.sleep(0.5)
        region = tuple(round(float(v) * scale) for v in args.region.split(","))
        latencies = []
        for i in range(args.samples):
            if i % 20 == 19:
                win.key("ctrl+a", "delete")
                time.sleep(0.3)
            before = region_pixels(window, region)
            # A random phase against the display's refresh.
            time.sleep(0.05 + random.random() * 0.05)
            start = time.perf_counter()
            if args.press:
                win.key("x", delay=0)
            else:
                win.type_text("x", delay=0)
            while region_pixels(window, region) == before:
                if time.perf_counter() - start > 1:
                    raise SystemExit("the typed character never showed")
            latencies.append((time.perf_counter() - start) * 1000)
            time.sleep(0.1)
        latencies.sort()
        print(f"{len(latencies)} keystrokes, input to composed screen (ms): "
              f"median {statistics.median(latencies):.1f}, "
              f"p90 {latencies[int(len(latencies) * 0.9)]:.1f}, "
              f"min {latencies[0]:.1f}, max {latencies[-1]:.1f}")
        win.close(window)
    finally:
        try:
            proc.wait(timeout=5)
        except subprocess.TimeoutExpired:
            proc.kill()


if __name__ == "__main__":
    main()
