#!/usr/bin/env python3
"""Validate held-key focus transfer without synthetic text or activation on owned X11."""
import argparse
import hashlib
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
    binary_sha256 = hashlib.sha256(args.binary.read_bytes()).hexdigest()
    xvfb = wm = app = None
    try:
        with (args.output / "xvfb.log").open("w") as log:
            xvfb = subprocess.Popen(["Xvfb", "-displayfd", "1", "-screen", "0", "900x650x24", "-ac"], stdout=subprocess.PIPE, stderr=log, text=True)
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
        subprocess.check_call(["xset", "r", "off"], env=env)
        deadline = time.monotonic() + 20
        windows = {}
        while time.monotonic() < deadline and app.poll() is None:
            for name in ("source", "target"):
                found = subprocess.run(["xdotool", "search", "--onlyvisible", "--name", "^zgx held keys " + name + "$"], env=env, capture_output=True, text=True)
                if found.returncode == 0 and found.stdout.strip():
                    windows[name] = found.stdout.splitlines()[0]
            if len(windows) == 2:
                break
            time.sleep(.05)
        if len(windows) != 2:
            raise RuntimeError("held-key windows did not open")
        xdo("windowmove", windows["source"], 20, 40)
        xdo("windowmove", windows["target"], 440, 40)
        def focus(name):
            xdo("windowactivate", "--sync", windows[name])
            time.sleep(.15)
        def editor(name):
            focus(name)
            xdo("mousemove", "--window", windows[name], 100, 75, "click", 1)
            time.sleep(.1)
        editor("target")
        editor("source")
        xdo("keydown", "x")
        time.sleep(.08)
        focus("target")
        xdo("keyup", "x")
        xdo("key", "b")
        time.sleep(.1)
        # Tab gives the target action focus without activating it.
        xdo("key", "Tab")
        editor("source")
        xdo("keydown", "space")
        time.sleep(.08)
        focus("target")
        xdo("keyup", "space")
        time.sleep(.1)
        # A genuine subsequent Space must activate exactly once.
        xdo("key", "space")
        time.sleep(.2)
        from PIL import ImageGrab
        ImageGrab.grab(xdisplay=display).save(args.output / "held-keys.png")
        output, _ = app.communicate(timeout=12)
        (args.output / "held-keys.log").write_text(output)
        if app.returncode:
            raise RuntimeError(output)
        expected = ['HELD source value="x " actions=0', 'HELD target value="b" actions=1']
        if not all(line in output for line in expected):
            raise AssertionError(output)
        (args.output / "results.json").write_text(json.dumps({"passed": True, "backend": "X11", "binary_sha256": binary_sha256, "script_sha256": hashlib.sha256(pathlib.Path(__file__).read_bytes()).hexdigest(), "checks": ["held printable focus transfer does not type into target", "held Space focus transfer does not activate target", "normal subsequent text and Space remain functional"]}, indent=2) + "\n")
        print("Native held-key focus-transfer checks passed")
    finally:
        for process in (app, wm, xvfb):
            stop(process)


if __name__ == "__main__":
    main()
