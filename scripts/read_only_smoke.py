#!/usr/bin/env python3
"""Native read-only editor selection, clipboard, model updates and toggling."""
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
    with tempfile.TemporaryDirectory(prefix="zgx-read-only-") as runtime:
        try:
            with (args.output / "xvfb.log").open("w") as log:
                xvfb = subprocess.Popen(["Xvfb", "-r", "-displayfd", "1", "-screen", "0", "800x600x24", "-ac"], stdout=subprocess.PIPE, stderr=log, text=True)
                if not select.select([xvfb.stdout], [], [], 8)[0]:
                    raise RuntimeError("owned Xvfb startup timed out")
                display = ":" + xvfb.stdout.readline().strip()
            env = dict(os.environ, DISPLAY=display, WINIT_X11_SCALE_FACTOR="1", XDG_RUNTIME_DIR=runtime, XDG_CONFIG_HOME=runtime + "/config", XDG_CACHE_HOME=runtime + "/cache", GSETTINGS_BACKEND="memory", DBUS_SESSION_BUS_ADDRESS="unix:path=/nonexistent")
            for key in ("WAYLAND_DISPLAY", "SWAYSOCK", "I3SOCK"):
                env.pop(key, None)
            with (args.output / "openbox.log").open("w") as log:
                wm = subprocess.Popen(["openbox"], env=env, stdout=log, stderr=log)
            wait_for_wm(env)
            output = args.output / "read-only.log"
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
                    raise RuntimeError("read-only window did not open")
                xdo("windowactivate", "--sync", window)
                time.sleep(.3)
                def snapshot():
                    time.sleep(.25)
                    matches = re.findall(r'READONLY enabled=(true|false) source=(".*?") destination=(".*?") anchor=(\d+) focus=(\d+)', output.read_text())
                    assert matches, output.read_text()
                    enabled, source, destination, anchor, focus = matches[-1]
                    return dict(enabled=enabled == "true", source=json.loads(source), destination=json.loads(destination), anchor=int(anchor), focus=int(focus))
                def click(x, y):
                    xdo("mousemove", "--sync", "--window", window, x, y, "click", 1)
                reports = {"initial": snapshot()}
                assert reports["initial"]["enabled"] and reports["initial"]["source"] == "seed", reports
                xdo("type", "--clearmodifiers", "bad")
                xdo("key", "BackSpace", "Delete", "ctrl+z")
                reports["typing_blocked"] = snapshot()
                assert reports["typing_blocked"]["source"] == "seed", reports
                xdo("key", "Home", "Right", "shift+Right")
                reports["selection"] = snapshot()
                assert reports["selection"]["anchor"] == 1 and reports["selection"]["focus"] == 2, reports
                xdo("key", "ctrl+a", "ctrl+c", "ctrl+x")
                reports["copy_cut"] = snapshot()
                assert reports["copy_cut"]["source"] == "seed", reports
                click(50, 172)
                xdo("key", "ctrl+v")
                reports["copied"] = snapshot()
                assert reports["copied"]["destination"] == "seed", reports
                click(50, 120)
                xdo("key", "End", "ctrl+v")
                reports["paste_blocked"] = snapshot()
                assert reports["paste_blocked"]["source"] == "seed", reports
                click(150, 72)
                click(50, 120)
                xdo("key", "End")
                xdo("type", "--clearmodifiers", "!")
                reports["editable"] = snapshot()
                assert not reports["editable"]["enabled"] and reports["editable"]["source"] == "seed!", reports
                click(150, 72)
                click(50, 120)
                xdo("key", "ctrl+z")
                reports["undo_blocked"] = snapshot()
                assert reports["undo_blocked"]["enabled"] and reports["undo_blocked"]["source"] == "seed!", reports
                click(150, 220)
                reports["external"] = snapshot()
                assert reports["external"]["source"] == "external" and reports["external"]["enabled"], reports
                subprocess.run(["import", "-window", window, str(args.output / "read-only.png")], env=env, check=True, timeout=10)
                # Closed as a user closes it (Openbox's Alt+F4), not by the app's
                # own timer, which a quick run would wait out.
                xdo("windowactivate", "--sync", window, "key", "alt+F4")
                app.wait(timeout=20)
            assert app.returncode == 0, output.read_text()
            (args.output / "results.json").write_text(json.dumps(dict(passed=True, backend="owned X11/Xvfb/Openbox", reports=reports), indent=2) + "\n")
            print("Native read-only checks passed")
        finally:
            for process in (app, wm, xvfb):
                stop(process)


if __name__ == "__main__":
    main()
