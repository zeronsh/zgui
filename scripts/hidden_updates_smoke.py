#!/usr/bin/env python3
"""Owned Xvfb/Openbox lifecycle check: native map state and restored GPU pixels.

Requires Xvfb, Openbox, xdotool, xwininfo, ImageMagick import and Pillow.
Does not establish hardware performance or native macOS behavior.
"""
import argparse
import json
import os
from pathlib import Path
import select
import subprocess
import time

from PIL import Image
from platform_smoke import stop, wait_for_wm


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("binary", type=Path)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    output = args.output.resolve()
    output.mkdir(parents=True, exist_ok=True)
    xvfb = wm = app = None
    try:
        with (output / "xvfb.log").open("w") as xlog, (output / "openbox.log").open("w") as wlog, (output / "application.log").open("w") as log:
            xvfb = subprocess.Popen(["Xvfb", "-r", "-noreset", "-displayfd", "1", "-screen", "0", "1024x768x24", "-ac"], stdout=subprocess.PIPE, stderr=xlog, text=True)
            if not select.select([xvfb.stdout], [], [], 10)[0]:
                raise RuntimeError("Xvfb did not start")
            display = xvfb.stdout.readline().strip()
            if not display:
                raise RuntimeError("Xvfb did not provide a display")
            env = dict(os.environ, DISPLAY=":" + display, XDG_RUNTIME_DIR="/tmp", WINIT_X11_SCALE_FACTOR="1", DBUS_SESSION_BUS_ADDRESS="unix:path=/nonexistent")
            env.pop("WAYLAND_DISPLAY", None)
            wm = subprocess.Popen(["openbox"], env=env, stdout=wlog, stderr=wlog)
            wait_for_wm(env)
            app = subprocess.Popen([str(args.binary.resolve()), "--smoke-test"], env=env, stdout=log, stderr=log)

            def wait_marker(marker):
                deadline = time.monotonic() + 15
                while marker not in (output / "application.log").read_text():
                    if app.poll() is not None or time.monotonic() >= deadline:
                        raise RuntimeError(f"missing marker {marker}:\n" + (output / "application.log").read_text())
                    time.sleep(.03)

            wait_marker("HIDDEN updated 20")
            ids = subprocess.check_output(["xdotool", "search", "--name", "^zgx hidden updates$"], env=env, text=True).splitlines()
            if len(ids) != 1:
                raise RuntimeError(f"expected one native window, got {ids}")
            window = ids[0]

            def map_state(expected):
                info = subprocess.check_output(["xwininfo", "-id", window], env=env, text=True)
                if f"Map State: {expected}" not in info:
                    raise RuntimeError(f"expected {expected}: {info}")
                return expected

            def pixels(name, rgb):
                time.sleep(.5)
                map_state("IsViewable")
                path = output / name
                subprocess.run(["import", "-window", window, str(path)], env=env, check=True, timeout=10)
                image = Image.open(path).convert("RGB")
                matching = sum(count for count, pixel in image.getcolors(image.width * image.height)
                               if all(abs(a-b) <= 2 for a, b in zip(pixel, rgb)))
                if matching < 30000:
                    raise RuntimeError(f"restored color {rgb} missing: only {matching} matching pixels")
                return {"image": name, "expected_rgb": rgb, "matching_pixels": matching}

            result = {"backend": "X11 / owned Xvfb / Openbox", "hidden_updates": 20, "hidden_map_state": map_state("IsUnMapped")}
            wait_marker("SHOWN 20")
            result["shown"] = pixels("shown.png", (40, 180, 80))
            wait_marker("MINIMIZED updated 40")
            result["minimized_map_state"] = map_state("IsUnMapped")
            result["total_updates"] = 40
            wait_marker("RESTORED 40")
            result["restored"] = pixels("restored.png", (40, 100, 220))
            if app.wait(timeout=10) != 0:
                raise RuntimeError((output / "application.log").read_text())
            wait_marker("hidden updates smoke passed")
            result["exit_code"] = 0
            (output / "results.json").write_text(json.dumps(result, indent=2) + "\n")
            print(json.dumps(result, indent=2))
    finally:
        stop(app)
        stop(wm)
        stop(xvfb)


if __name__ == "__main__":
    main()
