#!/usr/bin/env python3
"""Shortcuts on a layout of other letters, on X11 (Xvfb/Openbox, a Russian
layout): Ctrl and the A key (which types ф) select an editor's text, Ctrl
and the C key (с) copy it, as browsers and GTK take shortcuts by the US
letter on the key; and the same keys without Ctrl still type Russian."""
import argparse
import json
import os
import pathlib
import select
import subprocess
import time

from platform_smoke import stop, wait_for_wm

# The physical keys, as X11 keycodes (evdev + 8): what the user presses.
KEYS = {"a": 38, "c": 54, "g": 42, "h": 43, "b": 56, "d": 40, "t": 28, "n": 57}
TEXT = "select these words"


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("binary", type=pathlib.Path, help="the primary_selection example")
    parser.add_argument("--output", type=pathlib.Path, required=True)
    args = parser.parse_args()
    args.output.mkdir(parents=True, exist_ok=True)
    processes, checks = [], {}

    def launch(name, command, env, **options):
        log = (args.output / (name + ".log")).open("w")
        child = subprocess.Popen(command, env=env, stdout=options.pop("stdout", log), stderr=log, **options)
        processes.append(child)
        return child

    try:
        env = dict(os.environ, WINIT_X11_SCALE_FACTOR="1", XDG_RUNTIME_DIR="/tmp",
                   DBUS_SESSION_BUS_ADDRESS="unix:path=/nonexistent")
        env.pop("WAYLAND_DISPLAY", None)
        xvfb = launch("xvfb", ["Xvfb", "-r", "-noreset", "-displayfd", "1", "-screen", "0", "1280x900x24", "-ac"], env,
                      stdout=subprocess.PIPE, text=True)
        if not select.select([xvfb.stdout], [], [], 30)[0]:
            raise RuntimeError("Xvfb startup timed out")
        env["DISPLAY"] = ":" + xvfb.stdout.readline().strip()
        launch("openbox", ["openbox"], env)
        wait_for_wm(env)
        subprocess.check_call(["setxkbmap", "-layout", "ru"], env=env)
        app = launch("application", [str(args.binary.resolve()), "--smoke-test"], env)

        def run(*argv):
            return subprocess.run(list(map(str, argv)), env=env, capture_output=True, text=True, timeout=10).stdout

        def until(what, test, seconds=10):
            deadline = time.monotonic() + seconds
            while True:
                value = test()
                if value:
                    return value
                if time.monotonic() > deadline or app.poll() is not None:
                    raise AssertionError(f"timed out waiting for {what}: {checks}")
                time.sleep(.05)

        def selection(name):
            return run("xclip", "-o", "-selection", name)

        def press(*keys, ctrl=False):
            for key in keys:
                run("xdotool", "key", "--clearmodifiers", ("ctrl+" if ctrl else "") + str(KEYS[key]))
                time.sleep(.05)

        window = until("the window", lambda: (run("xdotool", "search", "--onlyvisible", "--pid", app.pid).split()
                                              or [None])[0])
        run("xdotool", "windowactivate", "--sync", window)
        time.sleep(.4)
        run("xdotool", "mousemove", "--window", window, 110, 42, "click", 1)
        time.sleep(.3)
        press("a", ctrl=True)
        checks["Ctrl+ф selects all"] = until("the selection", lambda: selection("primary") == TEXT)
        press("c", ctrl=True)
        checks["Ctrl+с copies"] = until("the copy", lambda: selection("clipboard") == TEXT)
        # The same keys without Ctrl still type the layout's letters.
        press("g", "h", "b", "d", "t", "n")
        press("a", ctrl=True)
        checks["typing stays Russian"] = until("the typed word", lambda: selection("primary") == "привет")
        (args.output / "result.json").write_text(json.dumps(checks, indent=2, ensure_ascii=False) + "\n")
        print("PASS: on a Russian layout Ctrl+ф selects all, Ctrl+с copies, and typing stays Russian")
    finally:
        for process in reversed(processes):
            stop(process)


if __name__ == "__main__":
    main()
