#!/usr/bin/env python3
"""Check real native image-fit pixels, padded clipping and window resizing."""
import argparse
import json
import os
import pathlib
import select
import subprocess
import tempfile
import time
from PIL import Image
from platform_smoke import stop, wait_for_wm

COLORS = [(224, 48, 48), (48, 208, 80), (48, 96, 224), (48, 208, 208), (208, 48, 208), (224, 208, 48)]
BACKGROUND = (32, 48, 64)
MODES = ["Fill", "Contain", "Cover", "None", "ScaleDown"]


def check_pixels(picture, width, mode):
    assert picture.size == (width, 320), picture.size
    # Root padding20; controls100 + gap20; image padding20 on all edges.
    x, y, content_width, content_height = 160, 76, width - 200, 140
    if mode == "Fill":
        fitted_width, fitted_height = content_width, content_height
    else:
        scale = {"Contain": min(content_width / 120, content_height / 60),
                 "Cover": max(content_width / 120, content_height / 60),
                 "None": 1., "ScaleDown": min(1., content_width / 120, content_height / 60)}[mode]
        fitted_width, fitted_height = 120 * scale, 60 * scale
    left = x + (content_width - fitted_width) / 2
    top = y + (content_height - fitted_height) / 2
    tested = 0
    seen = set()
    for px in range(145, width - 25, 7):
        for py in range(61, 231, 7):
            inside_clip = x <= px + .5 < x + content_width and y <= py + .5 < y + content_height
            sx = (px + .5 - left) * 120 / fitted_width
            sy = (py + .5 - top) * 60 / fitted_height
            inside_image = 0 <= sx < 120 and 0 <= sy < 60
            if inside_clip and inside_image:
                # Avoid bilinear transitions around source color boundaries.
                if min(abs(sx - edge) for edge in [0, 40, 80, 120]) < 2 or min(abs(sy - edge) for edge in [0, 30, 60]) < 2:
                    continue
                expected = COLORS[int(sy // 30) * 3 + int(sx // 40)]
                seen.add(expected)
            else:
                expected = BACKGROUND
            actual = picture.getpixel((px, py))
            assert actual == expected, (mode, width, (px, py), actual, expected)
            tested += 1
    assert tested > 150 and len(seen) >= 2, (tested, seen)
    return dict(mode=mode, window_width=width, fitted_size=[fitted_width, fitted_height], checked_pixels=tested, colors_visible=len(seen))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("binary", type=pathlib.Path)
    parser.add_argument("--output", type=pathlib.Path, required=True)
    args = parser.parse_args()
    args.output.mkdir(parents=True, exist_ok=True)
    (args.output / "results.json").unlink(missing_ok=True)
    xvfb = wm = app = None
    with tempfile.TemporaryDirectory(prefix="zgui-image-fit-") as runtime:
        try:
            with (args.output / "xvfb.log").open("w") as log:
                xvfb = subprocess.Popen(["Xvfb", "-displayfd", "1", "-screen", "0", "800x600x24", "-ac"], stdout=subprocess.PIPE, stderr=log, text=True)
                if not select.select([xvfb.stdout], [], [], 8)[0]:
                    raise RuntimeError("owned Xvfb startup timed out")
                display = ":" + xvfb.stdout.readline().strip()
            env = dict(os.environ, DISPLAY=display, XDG_RUNTIME_DIR=runtime, WINIT_X11_SCALE_FACTOR="1", DBUS_SESSION_BUS_ADDRESS="unix:path=/nonexistent", GSETTINGS_BACKEND="memory", XDG_CONFIG_HOME=str(pathlib.Path(runtime, "config")), XDG_CACHE_HOME=str(pathlib.Path(runtime, "cache")))
            for name in ("WAYLAND_DISPLAY", "SWAYSOCK", "I3SOCK", "AT_SPI_BUS_ADDRESS"):
                env.pop(name, None)
            with (args.output / "openbox.log").open("w") as log:
                wm = subprocess.Popen(["openbox"], env=env, stdout=log, stderr=log)
            wait_for_wm(env)
            with (args.output / "image-fit.log").open("w") as log:
                app = subprocess.Popen([str(args.binary.resolve()), "--smoke-test"], env=env, stdout=log, stderr=log)
                def xdo(*parts):
                    return subprocess.check_output(["xdotool", *map(str, parts)], env=env, text=True).strip()
                deadline = time.monotonic() + 20
                window = None
                while time.monotonic() < deadline and app.poll() is None:
                    found = subprocess.run(["xdotool", "search", "--onlyvisible", "--pid", str(app.pid)], env=env, text=True, capture_output=True)
                    if found.returncode == 0 and found.stdout.strip():
                        window = found.stdout.splitlines()[0]
                        break
                    time.sleep(.05)
                if window is None:
                    raise RuntimeError("image-fit window did not open")
                xdo("windowactivate", "--sync", window)
                time.sleep(.2)
                checks = []
                stages = [(500, mode) for mode in MODES] + [(280, mode) for mode in ["ScaleDown", "None", "Cover"]]
                current_width = 500
                for index, (width, mode) in enumerate(stages, 1):
                    if width != current_width:
                        xdo("windowsize", "--sync", window, width, 320)
                        current_width = width
                    xdo("mousemove", "--window", window, 70, 72 + MODES.index(mode) * 38, "click", 1)
                    time.sleep(.25)
                    path = args.output / f"image-fit-{index}.png"
                    subprocess.run(["import", "-window", window, str(path)], env=env, check=True, timeout=10)
                    checks.append(check_pixels(Image.open(path).convert("RGB"), width, mode))
                # Closed as a user closes it (Openbox's Alt+F4), not by the app's
                # own timer, which a quick run would wait out.
                xdo("windowactivate", "--sync", window, "key", "alt+F4")
                app.wait(timeout=16)
            output = (args.output / "image-fit.log").read_text()
            assert app.returncode == 0, output
            for mode in MODES:
                assert "FIT " + mode in output, output
            (args.output / "results.json").write_text(json.dumps(dict(passed=True, backend="owned X11/Xvfb/Openbox", checks=checks), indent=2) + "\n")
            print("Native image-fit checks passed")
        finally:
            for process in (app, wm, xvfb):
                stop(process)


if __name__ == "__main__":
    main()
