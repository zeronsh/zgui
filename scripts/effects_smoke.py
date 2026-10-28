#!/usr/bin/env python3
"""Exercise declarative effect controls and owned animation on a private X11 display."""
import argparse
import json
import os
import pathlib
import re
import select
import subprocess
import tempfile
import time
from PIL import Image, ImageChops
from platform_smoke import dump_display, stop, wait_for_wm


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("binary", type=pathlib.Path)
    parser.add_argument("--output", type=pathlib.Path, required=True)
    args = parser.parse_args()
    args.output.mkdir(parents=True, exist_ok=True)
    (args.output / "results.json").unlink(missing_ok=True)
    xvfb = wm = app = None
    with tempfile.TemporaryDirectory(prefix="zgx-effects-") as runtime:
        try:
            with (args.output / "xvfb.log").open("w") as log:
                xvfb = subprocess.Popen(["Xvfb", "-r", "-noreset", "-displayfd", "1", "-screen", "0", "1200x900x24", "-ac"], stdout=subprocess.PIPE, stderr=log, text=True)
                if not select.select([xvfb.stdout], [], [], 30)[0]:
                    raise RuntimeError("owned Xvfb startup timed out")
                display = ":" + xvfb.stdout.readline().strip()
            env = dict(os.environ, DISPLAY=display, WINIT_X11_SCALE_FACTOR="1", ZGX_SECONDS="30", XDG_RUNTIME_DIR=runtime, XDG_CONFIG_HOME=runtime + "/config", XDG_CACHE_HOME=runtime + "/cache", GSETTINGS_BACKEND="memory", DBUS_SESSION_BUS_ADDRESS="unix:path=/nonexistent")
            for key in ("WAYLAND_DISPLAY", "SWAYSOCK", "I3SOCK", "AT_SPI_BUS_ADDRESS"):
                env.pop(key, None)
            with (args.output / "openbox.log").open("w") as log:
                wm = subprocess.Popen(["openbox"], env=env, stdout=log, stderr=log)
            wait_for_wm(env)
            output = args.output / "effects.log"
            with output.open("w") as log:
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
                    dump_display(env, args.output, app.pid)
                    raise RuntimeError("effects window did not open: " + output.read_text())
                xdo("windowactivate", "--sync", window)
                time.sleep(.4)
                def report(until=lambda state: True):
                    # The latest report, once it shows what the input asked
                    # for (or after 5 s): a loaded runner may take a while.
                    deadline = time.monotonic() + 5
                    time.sleep(.25)
                    while True:
                        matches = re.findall(r'EFFECTS opacity=([\d.]+) blur=([\d.]+) fade=([\d.]+) animate=(true|false) x=([\d.]+)', output.read_text())
                        if not matches:
                            # The window can show before the first report.
                            assert time.monotonic() < deadline, output.read_text()
                            time.sleep(.05)
                            continue
                        opacity, blur, fade, animate, x = matches[-1]
                        state = dict(opacity=float(opacity), blur=float(blur), fade=float(fade), animate=animate == "true", x=float(x))
                        if until(state) or time.monotonic() > deadline:
                            return state
                        time.sleep(.05)
                def click(x, y):
                    xdo("mousemove", "--sync", "--window", window, x, y, "click", 1)
                def capture(name):
                    path = args.output / (name + ".png")
                    subprocess.run(["import", "-window", window, str(path)], env=env, check=True, timeout=10)
                    with Image.open(path) as image:
                        assert image.size == (960, 680), image.size
                        return image.convert("RGB").crop((70, 185, 560, 535))
                def difference(a, b):
                    pixels = ImageChops.difference(a, b).tobytes()
                    return sum(max(pixel) > 3 for pixel in zip(pixels[0::3], pixels[1::3], pixels[2::3]))
                reports = {"initial": report(lambda r: r["blur"] == 10)}
                initial = capture("initial")
                assert reports["initial"]["blur"] == 10 and reports["initial"]["opacity"] == .88, reports
                click(760, 286)
                xdo("key", "Home")
                reports["no_blur"] = report(lambda r: r["blur"] == 0)
                no_blur = capture("no-blur")
                assert reports["no_blur"]["blur"] == 0, reports
                changes = {"blur": difference(initial, no_blur)}
                assert changes["blur"] > 100, changes
                click(760, 368)
                xdo("key", "End")
                reports["fade"] = report(lambda r: r["fade"] == 80)
                faded = capture("fade")
                assert reports["fade"]["fade"] == 80, reports
                changes["fade"] = difference(no_blur, faded)
                assert changes["fade"] > 100, changes
                click(760, 204)
                xdo("key", "Home")
                reports["hidden"] = report(lambda r: r["opacity"] == 0)
                hidden = capture("hidden")
                assert reports["hidden"]["opacity"] == 0, reports
                changes["opacity"] = difference(faded, hidden)
                assert changes["opacity"] > 100, changes
                xdo("key", "End")
                reports["restored"] = report(lambda r: r["opacity"] == 1)
                restored = capture("restored")
                assert reports["restored"]["opacity"] == 1, reports
                assert difference(hidden, restored) > 100
                click(760, 286)
                xdo("key", "End")
                reports["blur_restored"] = report(lambda r: r["blur"] == 24)
                assert reports["blur_restored"]["blur"] == 24, reports
                restored = capture("blur-restored")
                click(680, 412)
                reports["moving"] = report(lambda r: r["animate"] and abs(r["x"] - 120) > 1)
                time.sleep(.25)
                moving = capture("moving")
                assert reports["moving"]["animate"] and abs(reports["moving"]["x"] - 120) > 1, reports
                changes["animation"] = difference(restored, moving)
                assert changes["animation"] > 100, changes
                click(680, 412)
                reports["paused"] = report(lambda r: not r["animate"])
                time.sleep(.25)
                reports["still_paused"] = report()
                assert not reports["paused"]["animate"], reports
                assert reports["still_paused"]["x"] == reports["paused"]["x"], reports
                capture("paused")
                # Closed as a user closes it (Openbox's Alt+F4), not by the app's
                # own timer, which a quick run would wait out.
                xdo("windowactivate", "--sync", window, "key", "alt+F4")
                app.wait(timeout=35)
            assert app.returncode == 0, output.read_text()
            (args.output / "results.json").write_text(json.dumps(dict(passed=True, backend="owned X11/Xvfb/Openbox", timed_close=True, reports=reports, changed_pixels=changes), indent=2) + "\n")
            print("Native declarative effects checks passed")
        finally:
            for process in (app, wm, xvfb):
                stop(process)


if __name__ == "__main__":
    main()
