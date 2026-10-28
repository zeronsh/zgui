#!/usr/bin/env python3
"""Native button activation survives unrelated keyboard input."""
import argparse
import json
import os
import pathlib
import re
import select
import subprocess
import tempfile
import time
from platform_smoke import stop, wait_for_wm
from PIL import Image


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("binary", type=pathlib.Path)
    parser.add_argument("--output", type=pathlib.Path, required=True)
    args = parser.parse_args()
    args.output.mkdir(parents=True, exist_ok=True)
    (args.output / "results.json").unlink(missing_ok=True)
    xvfb = wm = app = None
    with tempfile.TemporaryDirectory(prefix="zgx-keychords-") as runtime:
        try:
            with (args.output / "xvfb.log").open("w") as log:
                xvfb = subprocess.Popen(["Xvfb", "-r", "-noreset", "-displayfd", "1", "-screen", "0", "800x600x24", "-ac"], stdout=subprocess.PIPE, stderr=log, text=True)
                if not select.select([xvfb.stdout], [], [], 30)[0]:
                    raise RuntimeError("owned Xvfb startup timed out")
                display = ":" + xvfb.stdout.readline().strip()
            env = dict(os.environ, DISPLAY=display, WINIT_X11_SCALE_FACTOR="1", XDG_RUNTIME_DIR=runtime, XDG_CONFIG_HOME=runtime + "/config", XDG_CACHE_HOME=runtime + "/cache", GSETTINGS_BACKEND="memory", DBUS_SESSION_BUS_ADDRESS="unix:path=/nonexistent")
            for key in ("WAYLAND_DISPLAY", "SWAYSOCK", "I3SOCK"):
                env.pop(key, None)
            with (args.output / "openbox.log").open("w") as log:
                wm = subprocess.Popen(["openbox"], env=env, stdout=log, stderr=log)
            wait_for_wm(env)
            output = args.output / "keyboard-chords.log"
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
                    raise RuntimeError("keyboard-chords window did not open")
                xdo("windowactivate", "--sync", window)
                time.sleep(.3)
                def snapshot():
                    time.sleep(.25)
                    matches = re.findall(r"KEYCHORD accepted=(\d+) rejected=(\d+)", output.read_text())
                    assert matches, output.read_text()
                    accepted, rejected = matches[-1]
                    return dict(accepted=int(accepted), rejected=int(rejected))
                def click(x, y):
                    xdo("mousemove", "--sync", "--window", window, x, y, "click", 1)
                def active_pixel(stage, active):
                    screenshot = args.output / (stage + ".png")
                    subprocess.run(["import", "-window", window, str(screenshot)], env=env, check=True, timeout=10)
                    pixel = Image.open(screenshot).convert("RGB").getpixel((430, 70))
                    assert pixel == ((64, 80, 96) if active else (32, 48, 64)), (stage, pixel)
                reports = {"initial": snapshot()}
                assert reports["initial"] == dict(accepted=0, rejected=0), reports
                for key in ("space", "Return"):
                    xdo("keydown", key)
                    xdo("keydown", "a", "keyup", "a")
                    reports[key + "_held"] = snapshot()
                    active_pixel(key + "-held", True)
                    xdo("keyup", key)
                    reports[key + "_released"] = snapshot()
                    active_pixel(key + "-released", False)
                assert reports["space_held"]["accepted"] == 0, reports
                assert reports["space_released"]["accepted"] == 1, reports
                # Enter activates as it goes down, as a browser's button.
                assert reports["Return_held"]["accepted"] == 2, reports
                assert reports["Return_released"]["accepted"] == 2, reports
                # Moving focus while Space is held must cancel the armed button.
                xdo("keydown", "space")
                click(200, 180)
                xdo("keyup", "space")
                reports["focus_lost"] = snapshot()
                assert reports["focus_lost"] == dict(accepted=2, rejected=0), reports
                click(200, 128)
                xdo("keydown", "space", "keydown", "b", "keyup", "b", "keyup", "space")
                reports["prevented"] = snapshot()
                assert reports["prevented"] == dict(accepted=2, rejected=0), reports
                subprocess.run(["import", "-window", window, str(args.output / "keyboard-chords.png")], env=env, check=True, timeout=10)
                # Closed as a user closes it (Openbox's Alt+F4), not by the app's
                # own timer, which a quick run would wait out.
                xdo("windowactivate", "--sync", window, "key", "alt+F4")
                app.wait(timeout=14)
            assert app.returncode == 0, output.read_text()
            (args.output / "results.json").write_text(json.dumps(dict(passed=True, backend="owned X11/Xvfb/Openbox", reports=reports), indent=2) + "\n")
            print("Native keyboard chord checks passed")
        finally:
            for process in (app, wm, xvfb):
                stop(process)


if __name__ == "__main__":
    main()
