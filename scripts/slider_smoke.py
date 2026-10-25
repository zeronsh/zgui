#!/usr/bin/env python3
"""Validate the slider example on an owned Xvfb/Openbox desktop."""
import argparse
import json
import os
import pathlib
import re
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
            raise RuntimeError("slider window did not open")
        xdo("windowactivate", "--sync", window)
        time.sleep(.3)

        def click(x, y):
            xdo("mousemove", "--window", window, x, y, "click", 1)
            time.sleep(.2)

        def key(*keys):
            xdo("key", "--clearmodifiers", *keys)
            time.sleep(.15)

        def drag(end_x):
            xdo("mousemove", "--window", window, 150, 90, "mousedown", 1)
            xdo("mousemove", "--window", window, end_x, 90)
            time.sleep(.1)
            xdo("mouseup", 1)
            time.sleep(.15)

        def report():
            click(100, 202)

        drag(333)        # Approximately 75% of the original 394px usable track.
        report()
        key("Tab", "Home", "Right")
        report()         # Keyboard Home + Right = 1.
        key("Tab", "End", "Left")
        report()         # Keyboard End + Left = 99.
        click(490, 146)  # Reactive width expands to 580px.
        drag(453)        # Approximately 75% of the expanded 554px usable track.
        report()
        click(100, 146)  # Disable slider.
        click(100, 90)
        key("Home", "Right")
        report()         # Disabled input preserves the previous value.
        click(300, 146)  # External model write still updates a disabled control.
        report()
        from PIL import ImageGrab
        # Presentation can lag behind input delivery on software Vulkan.
        deadline = time.monotonic() + 2
        while True:
            screenshot = ImageGrab.grab(xdisplay=display)
            nonblack = sum(count for count, color in screenshot.getcolors(screenshot.width * screenshot.height) if color[:3] != (0, 0, 0))
            if nonblack > 50000:
                screenshot.save(args.output / "slider.png")
                break
            if time.monotonic() >= deadline:
                raise AssertionError("native screenshot has no visible window")
            time.sleep(.1)
        # Closed as a user closes it (Openbox's Alt+F4), not by the app's
        # own timer, which a quick run would wait out.
        xdo("windowactivate", "--sync", window, "key", "alt+F4")
        output, _ = app.communicate(timeout=15)
        (args.output / "slider.log").write_text(output)
        if app.returncode:
            raise RuntimeError(output)
        snapshots = re.findall(r"SNAPSHOT (\d+) value=([\d.]+) disabled=(true|false) wide=(true|false)", output)
        expected = [(75., "false", "false"), (1., "false", "false"), (99., "false", "false"), (75., "false", "true"), (75., "true", "true"), (25., "true", "true")]
        if len(snapshots) != len(expected):
            raise AssertionError(output)
        for index, ((number, value, disabled, wide), (expected_value, expected_disabled, expected_wide)) in enumerate(zip(snapshots, expected), 1):
            if int(number) != index or abs(float(value) - expected_value) > .2 or (disabled, wide) != (expected_disabled, expected_wide):
                raise AssertionError(output)
        required = ['SLIDER value=25.000 disabled=true wide=true snapshots=6', 'role: Slider, label: "Volume"', 'numeric_value: Some(25.0)', 'width: 580.0']
        if any(item not in output for item in required):
            raise AssertionError(output)
        result = {"platform": "X11/Xvfb", "scale": 1, "result": "pass", "checks": ["pointer drag", "Home/End/arrows", "reactive width pointer mapping", "disabled input suppressed", "external model write", "slider semantics", "automatic close"], "limitations": "Software rendering; does not validate macOS or hardware GPUs."}
        (args.output / "result.json").write_text(json.dumps(result, indent=2) + "\n")
        print("PASS: " + ", ".join(result["checks"]))
    finally:
        stop(app)
        stop(wm)
        stop(xvfb)


if __name__ == "__main__":
    main()
