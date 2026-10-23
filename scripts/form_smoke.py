#!/usr/bin/env python3
"""Exercise declarative editors on an existing X11 DISPLAY with a window manager.

Build with `cargo build -p zgui-desktop --example form` first. The script owns
only its form process; it does not start or stop the caller's display server.
"""
import argparse
import os
import pathlib
import re
import subprocess
import time


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("binary", type=pathlib.Path)
    parser.add_argument("--display", default=os.environ.get("DISPLAY"))
    parser.add_argument("--screenshot", type=pathlib.Path)
    parser.add_argument("--log", type=pathlib.Path)
    args = parser.parse_args()
    if not args.display:
        parser.error("--display or DISPLAY must name an X11 display with a window manager")
    env = dict(os.environ, DISPLAY=args.display, WINIT_X11_SCALE_FACTOR="1")
    env.pop("WAYLAND_DISPLAY", None)
    env.setdefault("DBUS_SESSION_BUS_ADDRESS", "unix:path=/nonexistent")
    process = subprocess.Popen([str(args.binary.resolve()), "--smoke-test"], env=env,
                               stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True)

    def xdo(*arguments):
        return subprocess.check_output(["xdotool", *map(str, arguments)], env=env, text=True).strip()

    def click(x, y):
        xdo("mousemove", "--window", window, x, y, "click", 1)
        time.sleep(.1)

    def key(*keys):
        xdo("key", "--clearmodifiers", *keys)
        time.sleep(.1)

    def type_text(value):
        xdo("type", "--clearmodifiers", "--delay", 12, value)
        time.sleep(.1)

    try:
        deadline = time.monotonic() + 20
        window = None
        while time.monotonic() < deadline and process.poll() is None:
            found = subprocess.run(["xdotool", "search", "--onlyvisible", "--pid", str(process.pid)],
                                   env=env, text=True, capture_output=True)
            if found.returncode == 0 and found.stdout.strip():
                window = found.stdout.splitlines()[0]
                break
            time.sleep(.05)
        if not window:
            raise RuntimeError("form did not open a visible window")
        # What a dock matches to the app's desktop file: the executable's name.
        wm_class = subprocess.check_output(["xprop", "-id", window, "WM_CLASS"], env=env, text=True)
        if '"form", "form"' not in wm_class:
            raise AssertionError("WM_CLASS is not the executable's name: " + wm_class)
        xdo("windowactivate", "--sync", window)
        time.sleep(.3)
        click(90, 112)
        key("ctrl+a")
        type_text("Native bridge")
        key("Home", *("shift+Right",) * 6)
        type_text("Styled")
        key("ctrl+a", "ctrl+c", "Tab", "ctrl+a", "ctrl+v", "End", "Return")
        type_text("Second line")
        # Turning editing off must keep both models unchanged even when pointer
        # clicks, selection shortcuts and native text events arrive afterwards.
        click(92, 348)
        click(90, 112)
        key("ctrl+a")
        type_text("FORBIDDEN")
        click(90, 210)
        type_text("FORBIDDEN")
        click(264, 348)
        xdo("windowsize", window, 900, 520)
        deadline = time.monotonic() + 3
        while True:
            geometry = dict(line.split("=", 1) for line in xdo("getwindowgeometry", "--shell", window).splitlines())
            if (int(geometry["WIDTH"]), int(geometry["HEIGHT"])) == (900, 520):
                break
            if time.monotonic() > deadline:
                raise AssertionError("native window resize was not applied")
            time.sleep(.05)
        time.sleep(.3)
        if args.screenshot:
            from PIL import ImageGrab
            args.screenshot.parent.mkdir(parents=True, exist_ok=True)
            ImageGrab.grab(xdisplay=args.display).save(args.screenshot)
        output, _ = process.communicate(timeout=20)
        if args.log:
            args.log.parent.mkdir(parents=True, exist_ok=True)
            args.log.write_text(output)
        if process.returncode:
            raise RuntimeError(output)
        expected = 'FORM name="Styled bridge" notes="Styled bridge\\nSecond line" locked=true wide=true'
        if expected not in output or "VIEWPORT (900.0, 520.0)" not in output:
            raise AssertionError(f"missing final model or viewport:\n{output}")
        for editor in ("name", "notes", "editors"):
            if not re.search(rf"BOUNDS {editor} Rect \{{[^\n]*width: 580\.0[, }}]", output):
                raise AssertionError(f"reactive width not applied to {editor}:\n{output}")
        print("PASS: declarative editors, native pointer focus, selection replacement, clipboard, Tab, multiline entry, disabled input, reactive editor width, native resize, automatic close")
    finally:
        if process.poll() is None:
            process.kill()
            process.wait()


if __name__ == "__main__":
    main()
