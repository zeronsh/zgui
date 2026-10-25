#!/usr/bin/env python3
"""Validate the images example on an owned Xvfb/Openbox desktop."""
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
            xvfb = subprocess.Popen(["Xvfb", "-r", "-displayfd", "1", "-screen", "0", "1024x768x24", "-ac"], stdout=subprocess.PIPE, stderr=log, text=True)
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
            raise RuntimeError("images window did not open")
        xdo("windowactivate", "--sync", window)
        time.sleep(.3)

        def click(x, y):
            xdo("mousemove", "--window", window, x, y, "click", 1)
            time.sleep(.2)

        from PIL import ImageGrab
        window_info = subprocess.check_output(["xwininfo", "-id", window], env=env, text=True)
        origin_x = int(re.search(r"Absolute upper-left X:\s+(-?\d+)", window_info).group(1))
        origin_y = int(re.search(r"Absolute upper-left Y:\s+(-?\d+)", window_info).group(1))
        samples = []
        stages = [((224, 64, 64), 80, 40, 200), ((64, 128, 224), 80, 40, 200), ((64, 192, 96), 120, 60, 200), ((64, 192, 96), 120, 60, 300), ((224, 64, 64), 80, 40, 200)]
        for stage, (color, source_width, source_height, allocated_width) in enumerate(stages):
            if stage:
                click(100, 90)
            deadline = time.monotonic() + 2
            while True:
                screenshot = ImageGrab.grab(xdisplay=display)
                def pixel(x, y):
                    return screenshot.getpixel((origin_x + x, origin_y + y))[:3]
                def is_source(x, y):
                    return all(abs(a-b) <= 2 for a, b in zip(pixel(x, y), color))
                actual = [x for x in range(24, 650) if is_source(x, 144)]
                expected = list(range(28, 28 + source_width)) + list(range(224, 220 + allocated_width - 4))
                intrinsic_vertical = [y for y in range(124, 224) if is_source(48, y)]
                allocated_vertical = [y for y in range(124, 224) if is_source(244, y)]
                padding = [pixel(x, 144) for x in (25, 29 + source_width, 221, 218 + allocated_width)]
                valid = actual == expected and intrinsic_vertical == list(range(128, 128 + source_height)) and allocated_vertical == list(range(128, 200)) and all(p == (32, 48, 64) for p in padding) and pixel(30, 242) == (224, 192, 64)
                if valid:
                    screenshot.save(args.output / f"images-{stage}.png")
                    samples.append({"stage": stage, "source_size": [source_width, source_height], "allocated_width": allocated_width, "intrinsic_fill_width": source_width, "allocated_fill_width": allocated_width - 8})
                    break
                if time.monotonic() >= deadline:
                    screenshot.save(args.output / "failure.png")
                    raise AssertionError(f"stage {stage}: horizontal={actual} intrinsic vertical={intrinsic_vertical} allocated vertical={allocated_vertical} padding={padding} static={pixel(30,242)}")
                time.sleep(.1)
        output, _ = app.communicate(timeout=15)
        (args.output / "images.log").write_text(output)
        if app.returncode:
            raise RuntimeError(output)
        required = ["STATE 1 source=80x40 allocated=200", "STATE 2 source=120x60 allocated=200", "STATE 3 source=120x60 allocated=300", "STATE 4 source=80x40 allocated=200", "IMAGES stage=4 source=80x40 allocated=200", 'role: Image, label: "Intrinsic preview"', 'role: Image, label: "Allocated preview"', 'role: Image, label: "Static swatch"']
        if any(item not in output for item in required):
            raise AssertionError(output)
        result = {"platform": "X11/Xvfb", "scale": 1, "result": "pass", "checks": ["static and reactive sources", "same-size source replacement", "intrinsic source resizing", "allocated image stretching", "padding pixels", "revert cached source", "image semantics", "automatic close"], "pixel_samples": samples, "limitations": "Software rendering; does not validate macOS or hardware GPUs."}
        (args.output / "result.json").write_text(json.dumps(result, indent=2) + "\n")
        print("PASS: " + ", ".join(result["checks"]))
    finally:
        stop(app)
        stop(wm)
        stop(xvfb)


if __name__ == "__main__":
    main()
