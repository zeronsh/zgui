#!/usr/bin/env python3
"""Native primary drags survive unrelated secondary-button releases."""
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


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("binary", type=pathlib.Path)
    parser.add_argument("--output", type=pathlib.Path, required=True)
    args = parser.parse_args()
    args.output.mkdir(parents=True, exist_ok=True)
    (args.output / "results.json").unlink(missing_ok=True)
    xvfb = wm = app = None
    with tempfile.TemporaryDirectory(prefix="zgui-chords-") as runtime:
        try:
            with (args.output / "xvfb.log").open("w") as log:
                xvfb = subprocess.Popen(["Xvfb", "-displayfd", "1", "-screen", "0", "800x600x24", "-ac"], stdout=subprocess.PIPE, stderr=log, text=True)
                if not select.select([xvfb.stdout], [], [], 8)[0]:
                    raise RuntimeError("owned Xvfb startup timed out")
                display = ":" + xvfb.stdout.readline().strip()
            env = dict(os.environ, DISPLAY=display, WINIT_X11_SCALE_FACTOR="1", XDG_RUNTIME_DIR=runtime, XDG_CONFIG_HOME=runtime + "/config", XDG_CACHE_HOME=runtime + "/cache", GSETTINGS_BACKEND="memory", DBUS_SESSION_BUS_ADDRESS="unix:path=/nonexistent")
            for key in ("WAYLAND_DISPLAY", "SWAYSOCK", "I3SOCK"):
                env.pop(key, None)
            with (args.output / "openbox.log").open("w") as log:
                wm = subprocess.Popen(["openbox"], env=env, stdout=log, stderr=log)
            wait_for_wm(env)
            output = args.output / "pointer-chords.log"
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
                    raise RuntimeError("pointer-chords window did not open")
                xdo("windowactivate", "--sync", window)
                time.sleep(.3)
                def snapshot():
                    time.sleep(.25)
                    matches = re.findall(r"CHORD anchor=(\d+) focus=(\d+) amount=([\d.]+)", output.read_text())
                    assert matches, output.read_text()
                    anchor, focus, amount = matches[-1]
                    return dict(anchor=int(anchor), focus=int(focus), amount=float(amount))
                def move(x, y):
                    xdo("mousemove", "--sync", "--window", window, x, y)
                reports = {}
                move(30, 76)
                xdo("mousedown", 1)
                move(105, 76)
                reports["editor_before_chord"] = snapshot()
                xdo("mousedown", 3, "mouseup", 3)
                move(235, 76)
                reports["editor_after_chord"] = snapshot()
                xdo("mouseup", 1)
                reports["editor_released"] = snapshot()
                move(340, 76)
                reports["editor_after_release_motion"] = snapshot()
                before, after = reports["editor_before_chord"], reports["editor_after_chord"]
                assert after["anchor"] == before["anchor"] and after["focus"] > before["focus"] + 5, reports
                assert reports["editor_released"] == reports["editor_after_release_motion"], reports
                move(100, 128)
                xdo("mousedown", 1)
                reports["slider_before_chord"] = snapshot()
                xdo("mousedown", 3, "mouseup", 3)
                move(365, 128)
                reports["slider_after_chord"] = snapshot()
                xdo("mouseup", 1)
                reports["slider_released"] = snapshot()
                move(480, 128)
                reports["slider_after_release_motion"] = snapshot()
                assert reports["slider_after_chord"]["amount"] > reports["slider_before_chord"]["amount"] + 35, reports
                assert reports["slider_released"] == reports["slider_after_release_motion"], reports
                subprocess.run(["import", "-window", window, str(args.output / "pointer-chords.png")], env=env, check=True, timeout=10)
                app.wait(timeout=14)
            assert app.returncode == 0, output.read_text()
            (args.output / "results.json").write_text(json.dumps(dict(passed=True, backend="owned X11/Xvfb/Openbox", reports=reports), indent=2) + "\n")
            print("Native pointer chord checks passed")
        finally:
            for process in (app, wm, xvfb):
                stop(process)


if __name__ == "__main__":
    main()
