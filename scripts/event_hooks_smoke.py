#!/usr/bin/env python3
"""Validate native keyboard interception and clipboard defaults on owned X11."""
import argparse
import json
import os
import pathlib
import select
import subprocess
import time
from platform_smoke import stop, wait_for_wm


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("binary", type=pathlib.Path)
    parser.add_argument("--output", type=pathlib.Path, required=True)
    args = parser.parse_args()
    args.output.mkdir(parents=True, exist_ok=True)
    xvfb = wm = app = None
    try:
        with (args.output / "xvfb.log").open("w") as log:
            xvfb = subprocess.Popen(["Xvfb", "-r", "-noreset", "-displayfd", "1", "-screen", "0", "900x650x24", "-ac"], stdout=subprocess.PIPE, stderr=log, text=True)
            if not select.select([xvfb.stdout], [], [], 10)[0]:
                raise RuntimeError("Xvfb startup timed out")
            display = ":" + xvfb.stdout.readline().strip()
            if display == ":":
                raise RuntimeError("Xvfb startup failed")
        env = dict(os.environ, DISPLAY=display, WINIT_X11_SCALE_FACTOR="1", XDG_RUNTIME_DIR="/tmp", DBUS_SESSION_BUS_ADDRESS="unix:path=/nonexistent")
        env.pop("WAYLAND_DISPLAY", None)
        with (args.output / "openbox.log").open("w") as log:
            wm = subprocess.Popen(["openbox"], env=env, stdout=log, stderr=log)
        wait_for_wm(env)
        app = subprocess.Popen([str(args.binary.resolve()), "--smoke-test"], env=env, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True)
        def xdo(*arguments):
            return subprocess.check_output(["xdotool", *map(str, arguments)], env=env, text=True).strip()
        deadline = time.monotonic() + 20
        window = None
        while time.monotonic() < deadline and app.poll() is None:
            found = subprocess.run(["xdotool", "search", "--onlyvisible", "--pid", str(app.pid)], env=env, capture_output=True, text=True)
            if found.returncode == 0 and found.stdout.strip():
                window = found.stdout.splitlines()[0]
                break
            time.sleep(.05)
        if window is None:
            raise RuntimeError("event hooks window did not open")
        xdo("windowactivate", "--sync", window)
        time.sleep(.2)
        def click(x, y):
            xdo("mousemove", "--window", window, x, y, "click", 1)
            time.sleep(.08)
        def key(*keys):
            xdo("key", "--clearmodifiers", *keys)
            time.sleep(.08)
        click(100, 184)
        key("ctrl+a", "ctrl+c")
        click(100, 104)
        xdo("type", "--clearmodifiers", "--delay", 20, "axb")
        key("space", "ctrl+a", "ctrl+c", "ctrl+x", "ctrl+v")
        click(100, 184)
        key("ctrl+a", "ctrl+v", "ctrl+a", "ctrl+x", "ctrl+v", "End", "space")
        xdo("type", "--clearmodifiers", "--delay", 20, "ok")
        time.sleep(.2)
        from PIL import ImageGrab
        ImageGrab.grab(xdisplay=display).save(args.output / "event-hooks.png")
        output, _ = app.communicate(timeout=12)
        (args.output / "event-hooks.log").write_text(output)
        if app.returncode:
            raise RuntimeError(output)
        expected = 'HOOKS captured="ab " printable=1 copy=1 cut=1 paste=1'
        if expected not in output or 'PLAIN ""' not in output or 'PLAIN "seed ok"' not in output:
            raise AssertionError(output)
        (args.output / "results.json").write_text(json.dumps({"passed": True, "backend": "X11", "checks": ["prevented printable x does not insert", "intercepted clipboard shortcuts do not copy/cut/paste", "Space still inserts", "ordinary clipboard copy/cut/paste works", "ordinary text follows paste"]}, indent=2) + "\n")
        print("Native event hook and clipboard checks passed")
    finally:
        for process in (app, wm, xvfb):
            stop(process)


if __name__ == "__main__":
    main()
