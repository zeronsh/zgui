#!/usr/bin/env python3
"""Disabling a composed ancestor cancels native held gestures."""
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
    with tempfile.TemporaryDirectory(prefix="zgui-disabled-") as runtime:
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
            output = args.output / "disabled-interaction.log"
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
                    raise RuntimeError("disabled-interaction window did not open")
                xdo("windowactivate", "--sync", window)
                time.sleep(.3)
                def snapshot():
                    time.sleep(.25)
                    # The window can show before the first sample.
                    first = time.monotonic() + 10
                    while not (matches := re.findall(r"DISABLED disabled=(true|false) accepted=(\d+) amount=([\d.]+) focus=(true|false) capture=(true|false)", output.read_text())):
                        assert time.monotonic() < first, output.read_text()
                        time.sleep(.05)
                    disabled, accepted, amount, focus, capture = matches[-1]
                    return dict(disabled=disabled == "true", accepted=int(accepted), amount=float(amount), focus=focus == "true", capture=capture == "true")
                def move(x, y):
                    xdo("mousemove", "--sync", "--window", window, x, y)
                def disable():
                    xdo("key", "d")
                    state = snapshot()
                    assert state["disabled"] and not state["focus"] and not state["capture"], state
                    return state
                def enabled():
                    deadline = time.monotonic() + 3
                    while time.monotonic() < deadline:
                        state = snapshot()
                        if not state["disabled"]:
                            return state
                    raise AssertionError("controls did not re-enable")
                reports = {"initial": snapshot()}
                xdo("keydown", "space")
                reports["space_disabled"] = disable()
                xdo("keyup", "space")
                reports["space_enabled"] = enabled()
                assert reports["space_enabled"]["accepted"] == 0, reports
                move(150, 76)
                xdo("click", 1)
                reports["fresh_click"] = snapshot()
                assert reports["fresh_click"]["accepted"] == 1, reports
                xdo("mousedown", 1)
                reports["pointer_disabled"] = disable()
                xdo("mouseup", 1)
                reports["pointer_enabled"] = enabled()
                assert reports["pointer_enabled"]["accepted"] == 1, reports
                move(100, 128)
                xdo("mousedown", 1)
                reports["slider_held"] = snapshot()
                assert reports["slider_held"]["capture"] and reports["slider_held"]["focus"], reports
                reports["slider_disabled"] = disable()
                move(350, 128)
                xdo("mouseup", 1)
                reports["slider_enabled"] = enabled()
                assert reports["slider_enabled"]["amount"] == reports["slider_disabled"]["amount"], reports
                assert not reports["slider_enabled"]["capture"], reports
                move(390, 128)
                xdo("click", 1)
                reports["fresh_slider"] = snapshot()
                assert reports["fresh_slider"]["amount"] > reports["slider_enabled"]["amount"] + 40, reports
                subprocess.run(["import", "-window", window, str(args.output / "disabled-interaction.png")], env=env, check=True, timeout=10)
                # Closed as a user closes it (Openbox's Alt+F4), not by the app's
                # own timer, which a quick run would wait out.
                xdo("windowactivate", "--sync", window, "key", "alt+F4")
                app.wait(timeout=20)
            assert app.returncode == 0, output.read_text()
            (args.output / "results.json").write_text(json.dumps(dict(passed=True, backend="owned X11/Xvfb/Openbox", reports=reports), indent=2) + "\n")
            print("Native disabled interaction checks passed")
        finally:
            for process in (app, wm, xvfb):
                stop(process)


if __name__ == "__main__":
    main()
