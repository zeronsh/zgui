#!/usr/bin/env python3
"""Native editor models normalize line endings and keep retained geometry."""
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
    with tempfile.TemporaryDirectory(prefix="zgx-model-text-") as runtime:
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
            output = args.output / "model-text.log"
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
                    raise RuntimeError("model-text window did not open")
                xdo("windowactivate", "--sync", window)
                time.sleep(.3)
                def snapshot():
                    time.sleep(.25)
                    rows = {}
                    for label, model, display_text, retained, caret_y, caret_h in re.findall(r'MODEL_TEXT (single|multi) model=("(?:[^"\\]|\\.)*") display=("(?:[^"\\]|\\.)*") retained=(true|false) caret_y=([\d.-]+) caret_h=([\d.]+)', output.read_text()):
                        rows[label] = dict(model=json.loads(model), display=json.loads(display_text), retained=retained == "true", caret_y=float(caret_y), caret_h=float(caret_h))
                    assert set(rows) == {"single", "multi"}, output.read_text()
                    for row in rows.values():
                        assert row["retained"] and row["model"] == row["display"], row
                    return rows
                def click(x, y):
                    xdo("mousemove", "--sync", "--window", window, x, y, "click", 1)
                reports = {"initial": snapshot()}
                assert reports["initial"]["single"]["model"] == "ABCD\tE", reports
                assert reports["initial"]["multi"]["model"] == "A\nB\nC\tD", reports
                click(150, 72)
                reports["replacement"] = snapshot()
                assert reports["replacement"]["single"]["model"] == "XYZ\tQ", reports
                assert reports["replacement"]["multi"]["model"] == "X\nY\nZ\tQ", reports
                click(50, 120)
                xdo("key", "End")
                xdo("type", "--clearmodifiers", "!")
                reports["single_input"] = snapshot()
                assert reports["single_input"]["single"]["model"] == "XYZ\tQ!", reports
                click(50, 165)
                xdo("key", "ctrl+End")
                xdo("type", "--clearmodifiers", "?")
                reports["multiline_input"] = snapshot()
                assert reports["multiline_input"]["multi"]["model"] == "X\nY\nZ\tQ?", reports
                caret = reports["multiline_input"]["multi"]
                assert 195 < caret["caret_y"] < 250 and caret["caret_h"] > 0, caret
                subprocess.run(["import", "-window", window, str(args.output / "model-text.png")], env=env, check=True, timeout=10)
                # Closed as a user closes it (Openbox's Alt+F4), not by the app's
                # own timer, which a quick run would wait out.
                xdo("windowactivate", "--sync", window, "key", "alt+F4")
                app.wait(timeout=14)
            assert app.returncode == 0, output.read_text()
            (args.output / "results.json").write_text(json.dumps(dict(passed=True, backend="owned X11/Xvfb/Openbox", reports=reports), indent=2) + "\n")
            print("Native model text checks passed")
        finally:
            for process in (app, wm, xvfb):
                stop(process)


if __name__ == "__main__":
    main()
