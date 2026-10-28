#!/usr/bin/env python3
"""X11's and Wayland's primary selection through the primary_selection example.

Selecting in an editor (a double click, then Ctrl+A) must make the selection
readable by another client (xclip, or wl-paste through Sway), and a selection
another client owns must land where a middle click puts the caret. X11 runs
on an owned Xvfb/Openbox; --wayland runs the example as a native Wayland
client of an owned Sway (Weston has no primary selection protocol).
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
from sway_host import wait_for_sway_host


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("binary", type=pathlib.Path)
    parser.add_argument("--output", type=pathlib.Path, required=True)
    parser.add_argument("--wayland", action="store_true")
    args = parser.parse_args()
    args.output.mkdir(parents=True, exist_ok=True)
    processes = []

    def launch(name, command, env, **options):
        log = (args.output / (name + ".log")).open("w")
        child = subprocess.Popen(command, env=env, stdout=options.pop("stdout", log), stderr=log, **options)
        processes.append(child)
        return child

    with tempfile.TemporaryDirectory(prefix="zgx-primary-") as private:
        env = dict(os.environ, XDG_RUNTIME_DIR=private, XDG_CONFIG_HOME=private + "/config",
                   DBUS_SESSION_BUS_ADDRESS="unix:path=/nonexistent")
        for key in ("DISPLAY", "WAYLAND_DISPLAY", "SWAYSOCK", "I3SOCK"):
            env.pop(key, None)
        try:
            xvfb = launch("xvfb", ["Xvfb", "-r", "-noreset", "-displayfd", "1", "-screen", "0", "1280x900x24", "-ac"], env,
                          stdout=subprocess.PIPE, text=True)
            if not select.select([xvfb.stdout], [], [], 30)[0]:
                raise RuntimeError("Xvfb startup timed out")
            env["DISPLAY"] = ":" + xvfb.stdout.readline().strip()
            launch("openbox", ["openbox"], env)
            wait_for_wm(env)

            def xdo(*parts):
                return subprocess.check_output(["xdotool", *map(str, parts)], env=env, text=True).strip()

            if args.wayland:
                env.update(WLR_BACKENDS="x11", WLR_X11_OUTPUTS="1", WLR_RENDERER="pixman")
                config = pathlib.Path(private, "sway.conf")
                config.write_text("xwayland disable\nseat seat0 fallback true\noutput * mode 1200x800\n"
                                  "default_border none\nfor_window [title=\"zgx primary selection\"] floating enable\n")
                sway = launch("sway", ["sway", "--unsupported-gpu", "--config", str(config)], env)
                host = wait_for_sway_host(env, args.output, sway)
                deadline = time.monotonic() + 10
                while True:
                    sockets = list(pathlib.Path(private).glob("sway-ipc*.sock"))
                    displays = [p for p in pathlib.Path(private).glob("wayland-*") if p.suffix != ".lock"]
                    if sockets and displays:
                        break
                    if time.monotonic() > deadline:
                        raise RuntimeError("Sway startup timed out")
                    time.sleep(.05)
                env.update(SWAYSOCK=str(sockets[0]), WAYLAND_DISPLAY=displays[0].name)
                client = dict(env, WINIT_UNIX_BACKEND="wayland")
                client.pop("DISPLAY")
            else:
                client = dict(env, WINIT_X11_SCALE_FACTOR="1")
            app = launch("application", [str(args.binary.resolve()), "--smoke-test"], client)
            deadline = time.monotonic() + 20
            while "READY" not in (args.output / "application.log").read_text():
                if time.monotonic() > deadline or app.poll() is not None:
                    raise RuntimeError("the example did not start; see application.log")
                time.sleep(.05)

            # Where the window is, to point at its editors: an X11 window, or
            # the client's box on Sway's output, inside Sway's own X11 window.
            deadline = time.monotonic() + 20
            while True:
                if args.wayland:
                    tree = json.loads(subprocess.check_output(["swaymsg", "-t", "get_tree", "-r"], env=env, text=True))
                    pending, rect = [tree], None
                    while pending:
                        node = pending.pop()
                        pending.extend(node.get("nodes", []) + node.get("floating_nodes", []))
                        if node.get("name") == "zgx primary selection":
                            rect = node["rect"]
                    if rect:
                        window, origin = host, (rect["x"], rect["y"])
                        break
                else:
                    found = subprocess.run(["xdotool", "search", "--onlyvisible", "--pid", str(app.pid)],
                                           env=env, capture_output=True, text=True)
                    if found.returncode == 0 and found.stdout.strip():
                        window, origin = found.stdout.split()[0], (0, 0)
                        break
                if time.monotonic() > deadline or app.poll() is not None:
                    raise RuntimeError("the example's window did not appear")
                time.sleep(.05)
            xdo("windowactivate", "--sync", window)
            time.sleep(.5)

            def at(x, y):
                return ("mousemove", "--window", window, origin[0] + x, origin[1] + y)

            def read_primary():
                command = ["wl-paste", "--primary", "--no-newline"] if args.wayland \
                    else ["xclip", "-o", "-selection", "primary"]
                result = subprocess.run(command, env=env, capture_output=True, text=True, timeout=5)
                return result.stdout if result.returncode == 0 else None

            def wait_primary(expect, what):
                deadline = time.monotonic() + 5
                while (seen := read_primary()) is None or not expect(seen):
                    if time.monotonic() > deadline:
                        raise AssertionError(f"{what}: primary selection is {seen!r}")
                    time.sleep(.1)
                return seen

            # A double click selects a word: other clients read it.
            xdo(*at(110, 42), "click", "--repeat", 2, "--delay", 80, 1)
            word = wait_primary(lambda s: s in ("select", "these", "words"), "double click")
            # Ctrl+A selects the field.
            xdo("key", "--clearmodifiers", "ctrl+a")
            wait_primary(lambda s: s == "select these words", "Ctrl+A")
            # Another client's selection lands where a middle click puts the
            # caret, in the other editor.
            outside = "from another client"
            command = ["wl-copy", "--primary", outside] if args.wayland \
                else ["xclip", "-i", "-selection", "primary"]
            subprocess.run(command, env=env, input=None if args.wayland else outside, text=True, check=True, timeout=5)
            wait_primary(lambda s: s == outside, "the other client's selection")
            xdo(*at(120, 130), "click", 2)
            app.wait(timeout=30)
            report = (args.output / "application.log").read_text()
            match = re.search(r'PRIMARY source="([^"]*)" target="([^"]*)"', report)
            assert match, report
            assert match[1] == "select these words", report
            assert match[2] == outside, f"the middle click pasted {match[2]!r}, not {outside!r}"
            result = {"result": "pass", "platform": "native Wayland / Sway" if args.wayland else "X11 / Xvfb / Openbox",
                      "word": word, "checks": ["double-click selection published", "Ctrl+A selection published",
                                               "another client's selection middle-click pasted at the pointer"]}
            (args.output / "result.json").write_text(json.dumps(result, indent=2) + "\n")
            print("PASS: " + ", ".join(result["checks"]))
        finally:
            for process in reversed(processes):
                stop(process)


if __name__ == "__main__":
    main()
