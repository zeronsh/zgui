#!/usr/bin/env python3
"""Owned X11 smoke: real mixed typography, inline pointer/keyboard activation."""
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
    owned = []
    with tempfile.TemporaryDirectory(prefix="zgx-rich-") as runtime:
        try:
            with (args.output / "xvfb.log").open("w") as log:
                xvfb = subprocess.Popen(["Xvfb", "-r", "-noreset", "-displayfd", "1", "-screen", "0", "900x650x24", "-ac"], stdout=subprocess.PIPE, stderr=log, text=True)
                owned.append(xvfb)
                if not select.select([xvfb.stdout], [], [], 10)[0]:
                    raise RuntimeError("Xvfb startup timed out")
                display = ":" + xvfb.stdout.readline().strip()
                if display == ":":
                    raise RuntimeError("Xvfb startup failed")
            env = dict(os.environ, DISPLAY=display, WINIT_X11_SCALE_FACTOR="1", XDG_RUNTIME_DIR=runtime, DBUS_SESSION_BUS_ADDRESS="unix:path=/nonexistent")
            env.pop("WAYLAND_DISPLAY", None)
            with (args.output / "openbox.log").open("w") as log:
                wm = subprocess.Popen(["openbox"], env=env, stdout=log, stderr=log)
                owned.append(wm)
            wait_for_wm(env)
            log_path = args.output / "application.log"
            with log_path.open("w") as log:
                app = subprocess.Popen([str(args.binary.resolve()), "--smoke-test"], env=env, stdout=log, stderr=subprocess.STDOUT)
                owned.append(app)
            def xdo(*values):
                return subprocess.check_output(["xdotool", *map(str, values)], env=env, text=True).strip()
            deadline = time.monotonic() + 7
            bounds = None
            while time.monotonic() < deadline and app.poll() is None:
                bounds = re.search(r"RICH_BOUNDS x=([\d.]+) y=([\d.]+) width=([\d.]+) height=([\d.]+)", log_path.read_text())
                if bounds:
                    break
                time.sleep(.05)
            if not bounds:
                raise RuntimeError("Native link bounds were not reported")
            clamp = re.search(r"RICH_CLAMP height=([\d.]+)", log_path.read_text())
            assert clamp and 0 < float(clamp.group(1)) <= 56.01, log_path.read_text()
            window = xdo("search", "--onlyvisible", "--pid", app.pid).splitlines()[0]
            xdo("windowactivate", "--sync", window)
            x, y, width, height = map(float, bounds.groups())
            xdo("mousemove", "--window", window, int(x+8), int(y+height/2), "click", 1)
            time.sleep(.2)
            assert "RICH_LINK count=1" in log_path.read_text(), log_path.read_text()
            xdo("key", "--clearmodifiers", "Return")
            time.sleep(.2)
            assert "RICH_LINK count=2" in log_path.read_text(), log_path.read_text()
            from PIL import ImageGrab
            image = ImageGrab.grab(xdisplay=display)
            image.save(args.output / "rich-text.png")
            app.wait(timeout=10)
            if app.returncode:
                raise RuntimeError(f"Native example exited {app.returncode}")
            (args.output / "results.json").write_text(json.dumps({"pointer_activation": 1, "keyboard_activation": 2, "link_bounds": [x,y,width,height], "clamped_height": float(clamp.group(1)), "exit_code": app.returncode}, indent=2)+"\n")
        finally:
            for process in reversed(owned):
                stop(process)


if __name__ == "__main__":
    main()
