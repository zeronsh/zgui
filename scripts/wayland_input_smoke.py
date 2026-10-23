#!/usr/bin/env python3
"""Exercise native Wayland form input through an owned Weston X11 seat.

Build the zgui-desktop form example first. Requires Weston 13+ with kiosk-shell,
Xvfb, Openbox, xdotool, and Pillow. The application has no DISPLAY and therefore
cannot silently fall back to X11; protocol evidence is retained separately.
"""
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
    (args.output / "result.json").unlink(missing_ok=True)
    xvfb = wm = compositor = app = None
    with tempfile.TemporaryDirectory(prefix="zgui-wayland-seat-") as runtime:
        os.chmod(runtime, 0o700)
        try:
            with (args.output / "xvfb.log").open("w") as log:
                xvfb = subprocess.Popen(["Xvfb", "-displayfd", "1", "-screen", "0", "1280x960x24", "-ac"], stdout=subprocess.PIPE, stderr=log, text=True)
                if not select.select([xvfb.stdout], [], [], 10)[0]:
                    raise RuntimeError("Xvfb startup timed out")
                display = ":" + xvfb.stdout.readline().strip()
                if display == ":":
                    raise RuntimeError("Xvfb startup failed")
            env = dict(os.environ, DISPLAY=display, XDG_RUNTIME_DIR=runtime, DBUS_SESSION_BUS_ADDRESS="unix:path=/nonexistent")
            env.pop("WAYLAND_DISPLAY", None)
            with (args.output / "openbox.log").open("w") as log:
                wm = subprocess.Popen(["openbox"], env=env, stdout=log, stderr=log)
            wait_for_wm(env)
            with (args.output / "weston.log").open("w") as log:
                compositor = subprocess.Popen(["weston", "--backend=x11", "--renderer=pixman", "--shell=kiosk-shell.so", "--no-config", "--socket=zgui-input", "--width=900", "--height=650", "--idle-time=0"], env=env, stdout=log, stderr=log)
            deadline = time.monotonic() + 10
            while not pathlib.Path(runtime, "zgui-input").exists():
                if time.monotonic() > deadline or compositor.poll() is not None:
                    raise RuntimeError("Weston startup failed; see weston.log")
                time.sleep(.05)

            def xdo(*arguments):
                return subprocess.check_output(["xdotool", *map(str, arguments)], env=env, text=True).strip()

            deadline = time.monotonic() + 5
            while True:
                match = re.search(r"x11 output .* window id (\d+)", (args.output / "weston.log").read_text())
                if match:
                    window = match.group(1)
                    break
                if time.monotonic() > deadline:
                    raise RuntimeError("Weston X11 output window missing")
                time.sleep(.05)
            time.sleep(.3)
            xdo("windowactivate", "--sync", window)
            client_env = dict(env, WAYLAND_DISPLAY="zgui-input", WAYLAND_DEBUG="1", WINIT_UNIX_BACKEND="wayland")
            client_env.pop("DISPLAY", None)
            with (args.output / "protocol.log").open("w") as protocol, (args.output / "form.log").open("w") as output:
                app = subprocess.Popen([str(args.binary.resolve()), "--smoke-test"], env=client_env, stdout=output, stderr=protocol)
                deadline = time.monotonic() + 10
                while True:
                    trace = (args.output / "protocol.log").read_text()
                    if re.search(r"xdg_toplevel[#@]\d+\.configure\(900, 650", trace) and re.search(r"wl_surface[#@]\d+\.attach\(wl_buffer", trace):
                        break
                    if time.monotonic() > deadline or app.poll() is not None:
                        raise RuntimeError("Wayland client did not present; see protocol.log")
                    time.sleep(.05)
                time.sleep(.3)

                def click(x, y):
                    xdo("mousemove", "--window", window, x, y, "click", 1)
                    time.sleep(.12)

                def key(*keys):
                    xdo("key", "--clearmodifiers", *keys)
                    time.sleep(.12)

                def type_text(value):
                    xdo("type", "--clearmodifiers", "--delay", 12, value)
                    time.sleep(.12)

                from PIL import ImageGrab
                ImageGrab.grab(xdisplay=display).save(args.output / "initial.png")
                click(90, 112)
                key("ctrl+a")
                type_text("Native bridge")
                key("Home", *("shift+Right",) * 6)
                type_text("Styled")
                key("ctrl+a", "ctrl+c", "Tab", "ctrl+a", "ctrl+v", "End", "Return")
                type_text("Second line")
                click(92, 348)
                click(90, 112)
                key("ctrl+a")
                type_text("FORBIDDEN")
                click(90, 210)
                type_text("FORBIDDEN")
                click(264, 348)
                xdo("windowsize", window, 1000, 700)
                time.sleep(.5)
                ImageGrab.grab(xdisplay=display).save(args.output / "final.png")
                app.wait(timeout=20)
            output = (args.output / "form.log").read_text()
            protocol = (args.output / "protocol.log").read_text()
            if app.returncode:
                raise RuntimeError("form failed; see protocol.log")
            expected = 'FORM name="Styled bridge" notes="Styled bridge\\nSecond line" locked=true wide=true'
            if expected not in output:
                raise AssertionError(output)
            for editor in ("name", "notes", "editors"):
                if not re.search(rf"BOUNDS {editor} Rect \{{[^\n]*width: 580\.0[, }}]", output):
                    raise AssertionError(output)
            if "VIEWPORT (1000.0, 700.0)" not in output:
                raise AssertionError("Wayland output resize not applied: " + output)
            for event in (r"wl_pointer[#@]\d+\.button\(", r"wl_keyboard[#@]\d+\.key\(", r"xdg_toplevel[#@]\d+\.configure\(1000, 700", r"set_selection\(", r"wl_data_offer[#@]\d+\.receive\(", r"wl_data_source[#@]\d+\.send\("):
                if not re.search(event, protocol):
                    raise AssertionError("Missing Wayland protocol evidence: " + event)
            result = {"result": "pass", "platform": "native Wayland client / Weston X11 backend / Xvfb", "client_display_unset": True, "xwayland_enabled": False, "weston": subprocess.check_output(["weston", "--version"], text=True).strip(), "checks": ["pointer editor focus", "keyboard selection replacement", "Tab focus traversal", "Wayland clipboard copy/paste", "multiline text", "disabled input suppression", "reactive width", "native Wayland resize", "automatic close"], "limitations": "Software rendering and nested compositor; does not validate physical seats, IME, hardware GPUs, or macOS."}
            (args.output / "result.json").write_text(json.dumps(result, indent=2) + "\n")
            print("PASS: " + ", ".join(result["checks"]))
        finally:
            stop(app)
            stop(compositor)
            stop(wm)
            stop(xvfb)


if __name__ == "__main__":
    main()
