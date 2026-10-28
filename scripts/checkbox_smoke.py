#!/usr/bin/env python3
"""Validate the checkbox example on an owned Xvfb/Openbox desktop."""
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
            xvfb = subprocess.Popen(["Xvfb", "-r", "-noreset", "-displayfd", "1", "-screen", "0", "1024x768x24", "-ac"], stdout=subprocess.PIPE, stderr=log, text=True)
            if not select.select([xvfb.stdout], [], [], 30)[0]:
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

        def xdo(*args):
            return subprocess.check_output(["xdotool", *map(str, args)], env=env, text=True).strip()

        deadline = time.monotonic() + 20
        window = None
        while time.monotonic() < deadline and app.poll() is None:
            found = subprocess.run(["xdotool", "search", "--onlyvisible", "--pid", str(app.pid)], env=env, capture_output=True, text=True)
            if found.returncode == 0 and found.stdout.strip():
                window = found.stdout.splitlines()[0]
                break
            time.sleep(.05)
        if window is None:
            raise RuntimeError("checkbox window did not open")
        xdo("windowactivate", "--sync", window)
        time.sleep(.3)

        def click(x, y):
            xdo("mousemove", "--window", window, x, y, "click", 1)
            time.sleep(.2)

        click(150, 90)  # Label activation, not the indicator.
        xdo("key", "--clearmodifiers", "space")
        time.sleep(.2)
        click(120, 146)  # Disable checkbox.
        click(150, 90)   # Disabled control must not toggle.
        click(330, 146)  # External write must update disabled checkbox.
        from PIL import ImageGrab
        # Presentation can lag behind input delivery on software Vulkan.
        deadline = time.monotonic() + 2
        while True:
            screenshot = ImageGrab.grab(xdisplay=display)
            nonblack = sum(count for count, color in screenshot.getcolors(screenshot.width * screenshot.height) if color[:3] != (0, 0, 0))
            if nonblack > 50000:
                screenshot.save(args.output / "checkbox.png")
                break
            if time.monotonic() >= deadline:
                raise AssertionError("native screenshot has no visible window")
            time.sleep(.1)
        output, _ = app.communicate(timeout=15)
        (args.output / "checkbox.log").write_text(output)
        if app.returncode:
            raise RuntimeError(output)
        required = ["TOGGLE checked=true activations=1", "TOGGLE checked=false activations=2", "DISABLED true", "MODEL checked=true", "CHECKBOX checked=true disabled=true activations=2", 'role: CheckBox, label: "Enable notifications"', "checked: Some(true), mixed: false, disabled: true"]
        if any(item not in output for item in required) or output.count("TOGGLE ") != 2:
            raise AssertionError(output)
        result = {"platform": "X11/Xvfb", "scale": 1, "result": "pass", "checks": ["pointer label activation", "Space toggle", "disabled activation suppressed", "external model write", "checkbox semantics", "automatic close"], "limitations": "Software rendering; does not validate macOS or hardware GPUs."}
        (args.output / "result.json").write_text(json.dumps(result, indent=2) + "\n")
        print("PASS: " + ", ".join(result["checks"]))
    finally:
        stop(app)
        stop(wm)
        stop(xvfb)


if __name__ == "__main__":
    main()
