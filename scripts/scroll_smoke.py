#!/usr/bin/env python3
"""Validate the scroll example on an owned Xvfb/Openbox desktop."""
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
            xvfb = subprocess.Popen(["Xvfb", "-r", "-noreset", "-displayfd", "1", "-screen", "0", "1024x768x24", "-ac"], stdout=subprocess.PIPE, stderr=log, text=True)
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
            raise RuntimeError("scroll window did not open")
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
        def report(stage, height, count):
            click(80, 90)
            screenshot = ImageGrab.grab(xdisplay=display)
            screenshot.save(args.output / f"scroll-{stage}.png")
            sample = {"stage": stage, "height": height, "count": count,
                      "content_pixels": [screenshot.getpixel((origin_x + 36, origin_y + y))[:3] for y in range(132, 124 + height - 8)],
                      "padding_pixels": [screenshot.getpixel((origin_x + x, origin_y + y))[:3] for x,y in [(25,140),(322,140),(36,125),(36,124+height-2)]]}
            samples.append(sample)

        xdo("mousemove", "--window", window, 100, 180, "click", "--repeat", 2, "--delay", 100, 5)
        time.sleep(.25)
        report(1,160,8)
        click(220,90)  # Excessive external offset clamps to bottom.
        report(2,160,8)
        click(370,90)  # Shrink to three rows, shorter than viewport.
        report(3,160,3)
        click(220,90)
        report(4,160,3)
        click(370,90)  # Restore eight rows.
        click(220,90)
        click(500,90)  # Increase viewport height, reducing maximum offset.
        report(5,240,8)
        # Closed as a user closes it (Openbox's Alt+F4), not by the app's
        # own timer, which a quick run would wait out.
        xdo("windowactivate", "--sync", window, "key", "alt+F4")
        output, _ = app.communicate(timeout=15)
        (args.output / "scroll.log").write_text(output)
        if app.returncode:
            raise RuntimeError(output)
        reports = re.findall(r"SNAPSHOT (\d+) offset=([\d.]+) count=(\d+) tall=(true|false)",output)
        if len(reports) != 5:
            raise AssertionError(output)
        expected_offsets = [None,176.,0.,0.,96.]
        for sample, report_values, expected_offset in zip(samples,reports,expected_offsets):
            number, offset_text, count_text, tall = report_values
            offset = float(offset_text)
            if int(number) != sample["stage"] or int(count_text) != sample["count"] or (tall == "true") != (sample["height"] == 240) or (expected_offset is None and not 0 < offset <=176) or (expected_offset is not None and offset != expected_offset):
                raise AssertionError(output)
            if any(tuple(p) != (32,48,64) for p in sample["padding_pixels"]):
                raise AssertionError(f"padding was painted over: {sample}")
            for y, pixel in enumerate(sample.pop("content_pixels")):
                row = int((y + offset) // 40)
                expected = (224,64+row*16,64) if row < sample["count"] else (32,48,64)
                if tuple(pixel) != expected:
                    raise AssertionError(f"stage {number} content y={y}: {pixel}, expected {expected}")
            sample["offset"] = offset
            sample["validated_content_pixels"] = sample["height"] - 16
        required = ["SCROLL offset=96.000 count=8 tall=true snapshots=5", 'role: ScrollView', 'height: 240.0']
        if any(item not in output for item in required):
            raise AssertionError(output)
        result = {"platform": "X11/Xvfb", "scale": 1, "result": "pass", "checks": ["native wheel scroll", "content clip and padding", "external offset clamp", "shrink child count", "viewport resize clamp", "scroll semantics", "automatic close"], "pixel_samples": samples, "limitations": "Software rendering; does not validate macOS or hardware GPUs."}
        (args.output / "result.json").write_text(json.dumps(result, indent=2) + "\n")
        print("PASS: " + ", ".join(result["checks"]))
    finally:
        stop(app)
        stop(wm)
        stop(xvfb)


if __name__ == "__main__":
    main()
