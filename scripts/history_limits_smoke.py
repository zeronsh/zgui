#!/usr/bin/env python3
"""Native undo/redo honors history limits reduced between operations."""
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
    with tempfile.TemporaryDirectory(prefix="zgui-history-") as runtime:
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
            output = args.output / "history-limits.log"
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
                    raise RuntimeError("history-limits window did not open")
                xdo("windowactivate", "--sync", window)
                time.sleep(.3)
                def snapshot():
                    time.sleep(.25)
                    # The window can show before the first sample.
                    first = time.monotonic() + 10
                    while not (matches := re.findall(r'HISTORY limited=(true|false) model=(".*")', output.read_text())):
                        assert time.monotonic() < first, output.read_text()
                        time.sleep(.05)
                    limited, model = matches[-1]
                    return dict(limited=limited == "true", model=json.loads(model))
                def click(x, y):
                    xdo("mousemove", "--sync", "--window", window, x, y, "click", 1)
                reports = {"initial": snapshot()}
                assert reports["initial"] == dict(limited=False, model=""), reports
                # Each character an undo step of its own: moving the caret
                # (End) ends a typing run, which would otherwise undo whole.
                for character in "abc":
                    xdo("type", "--delay", 100, character)
                    xdo("key", "End")
                reports["typed"] = snapshot()
                assert reports["typed"]["model"] == "abc", reports
                xdo("key", "ctrl+z")
                reports["first_undo"] = snapshot()
                assert reports["first_undo"]["model"] == "ab", reports
                click(150, 128)
                reports["reduced"] = snapshot()
                assert reports["reduced"] == dict(limited=True, model="ab"), reports
                click(50, 76)
                xdo("key", "ctrl+shift+z")
                reports["redo"] = snapshot()
                assert reports["redo"]["model"] == "abc", reports
                xdo("key", "ctrl+z")
                reports["bounded_undo"] = snapshot()
                assert reports["bounded_undo"]["model"] == "ab", reports
                xdo("key", "ctrl+z")
                reports["exhausted_undo"] = snapshot()
                assert reports["exhausted_undo"]["model"] == "ab", reports
                subprocess.run(["import", "-window", window, str(args.output / "history-limits.png")], env=env, check=True, timeout=10)
                # Closed as a user closes it (Openbox's Alt+F4), not by the app's
                # own timer, which a quick run would wait out.
                xdo("windowactivate", "--sync", window, "key", "alt+F4")
                app.wait(timeout=14)
            assert app.returncode == 0, output.read_text()
            (args.output / "results.json").write_text(json.dumps(dict(passed=True, backend="owned X11/Xvfb/Openbox", reports=reports), indent=2) + "\n")
            print("Native history limit checks passed")
        finally:
            for process in (app, wm, xvfb):
                stop(process)


if __name__ == "__main__":
    main()
