#!/usr/bin/env python3
"""Validate the progress example on an owned Xvfb/Openbox desktop."""
import argparse
import json
import os
import pathlib
import re
import select
import subprocess
import time

from platform_smoke import stop, wait_for_wm


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("binary", type=pathlib.Path)
    parser.add_argument("--output", type=pathlib.Path, required=True)
    args = parser.parse_args()
    args.output.mkdir(parents=True, exist_ok=True)
    xvfb = wm = app = None
    try:
        with (args.output / "xvfb.log").open("w") as log:
            xvfb = subprocess.Popen(["Xvfb", "-displayfd", "1", "-screen", "0", "1024x768x24", "-ac"], stdout=subprocess.PIPE, stderr=log, text=True)
            if not select.select([xvfb.stdout], [], [], 10)[0]:
                raise RuntimeError("Xvfb startup timed out")
            display = ":" + xvfb.stdout.readline().strip()
            if display == ":":
                raise RuntimeError("Xvfb startup failed")
        env = dict(os.environ, DISPLAY=display, WINIT_X11_SCALE_FACTOR="1", XDG_RUNTIME_DIR="/tmp", DBUS_SESSION_BUS_ADDRESS="unix:path=/nonexistent")
        env.pop("WAYLAND_DISPLAY", None)
        with (args.output / "openbox.log").open("w") as log:
            wm = subprocess.Popen(["openbox"], env=env, stdout=log, stderr=log)
        wait_for_wm(env)
        app = subprocess.Popen([str(args.binary.resolve()), "--smoke-test"], env=env, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True)

        def xdo(*args):
            return subprocess.check_output(["xdotool", *map(str, args)], env=env, text=True).strip()

        deadline = time.monotonic() + 20
        window = None
        while time.monotonic() < deadline and app.poll() is None:
            found = subprocess.run(["xdotool", "search", "--onlyvisible", "--pid", str(app.pid)], env=env, capture_output=True, text=True)
            if found.returncode == 0 and found.stdout.strip():
                window = found.stdout.splitlines()[0]
                break
            time.sleep(.05)
        if window is None:
            raise RuntimeError("progress window did not open")
        xdo("windowactivate", "--sync", window)
        time.sleep(.3)

        def click(x, y):
            xdo("mousemove", "--window", window, x, y, "click", 1)
            time.sleep(.2)

        from PIL import ImageGrab
        window_info = subprocess.check_output(["xwininfo", "-id", window], env=env, text=True)
        origin_x = int(re.search(r"Absolute upper-left X:\s+(-?\d+)", window_info).group(1))
        origin_y = int(re.search(r"Absolute upper-left Y:\s+(-?\d+)", window_info).group(1))
        fill_color = (64, 160, 224)
        samples = []
        # Outer widths include 4px padding on each side; the fill is clipped to
        # the inner allocation. Inspect the native presentation, not model logs.
        for stage, (outer_width, fraction) in enumerate([(400, 0), (400, .25), (400, 1), (600, 1), (600, .25), (200, .25), (200, 0)]):
            if stage:
                click(100, 142)
            expected = round((outer_width - 8) * fraction)
            deadline = time.monotonic() + 2
            while True:
                screenshot = ImageGrab.grab(xdisplay=display)
                pixels = [screenshot.getpixel((origin_x + x, origin_y + 88))[:3] for x in range(24, 650)]
                positions = [i + 24 for i, color in enumerate(pixels) if all(abs(a-b) <= 2 for a, b in zip(color, fill_color))]
                track = screenshot.getpixel((origin_x + 25, origin_y + 88))[:3]
                valid = len(positions) == expected and (not positions or positions == list(range(28, 28 + expected))) and track == (32, 48, 64)
                if valid:
                    screenshot.save(args.output / f"progress-{stage}.png")
                    samples.append({"stage": stage, "width": outer_width, "fraction": fraction, "visible_fill_pixels": len(positions)})
                    break
                if time.monotonic() >= deadline:
                    screenshot.save(args.output / "failure.png")
                    raise AssertionError(f"stage {stage}: fill positions {positions}, expected {expected}; track={track}")
                time.sleep(.1)
        output, _ = app.communicate(timeout=15)
        (args.output / "progress.log").write_text(output)
        if app.returncode:
            raise RuntimeError(output)
        required = ["STATE 1 value=0.250 width=400", "STATE 2 value=1.000 width=400", "STATE 3 value=1.000 width=600", "STATE 4 value=0.250 width=600", "STATE 5 value=0.250 width=200", "STATE 6 value=0.000 width=200", "PROGRESS value=0.000 width=200 stage=6", 'role: Progress, label: "Download"', 'numeric_value: Some(0.0)']
        if any(item not in output for item in required):
            raise AssertionError(output)
        result = {"platform": "X11/Xvfb", "scale": 1, "result": "pass", "checks": ["zero/quarter/full native pixel extent", "grow/shrink allocation", "padding clip", "inherited fill color", "NaN normalization", "progress semantics", "automatic close"], "pixel_samples": samples, "limitations": "Software rendering; does not validate macOS or hardware GPUs."}
        (args.output / "result.json").write_text(json.dumps(result, indent=2) + "\n")
        print("PASS: " + ", ".join(result["checks"]))
    finally:
        stop(app)
        stop(wm)
        stop(xvfb)


if __name__ == "__main__":
    main()
