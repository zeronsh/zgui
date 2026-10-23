#!/usr/bin/env python3
"""Cursor styles as a native Wayland client: the cursors example under an
owned Sway asks for the right shape through wp_cursor_shape_v1 over each
region (move, not-allowed, crosshair, a reactive wait) and the default again
when the pointer returns to plain content.
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

TITLE = "zgui cursor styles"
# wp_cursor_shape_device_v1.shape
SHAPES = {1: "default", 5: "progress", 6: "wait", 8: "crosshair", 13: "move", 15: "not-allowed"}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("binary", type=pathlib.Path)
    parser.add_argument("--output", type=pathlib.Path, required=True)
    args = parser.parse_args()
    args.output.mkdir(parents=True, exist_ok=True)
    processes = []

    def launch(name, command, env, **options):
        log = (args.output / (name + ".log")).open("w")
        child = subprocess.Popen(command, env=env, stdout=options.pop("stdout", log),
                                 stderr=options.pop("stderr", log), **options)
        processes.append(child)
        return child

    with tempfile.TemporaryDirectory(prefix="zgui-wayland-cursors-") as private:
        env = dict(os.environ, XDG_RUNTIME_DIR=private, XDG_CONFIG_HOME=private + "/config",
                   DBUS_SESSION_BUS_ADDRESS="unix:path=/nonexistent")
        for key in ("DISPLAY", "WAYLAND_DISPLAY", "SWAYSOCK", "I3SOCK"):
            env.pop(key, None)
        try:
            xvfb = launch("xvfb", ["Xvfb", "-displayfd", "1", "-screen", "0", "1280x900x24", "-ac"], env,
                          stdout=subprocess.PIPE, text=True)
            if not select.select([xvfb.stdout], [], [], 10)[0]:
                raise RuntimeError("Xvfb startup timed out")
            env["DISPLAY"] = ":" + xvfb.stdout.readline().strip()
            launch("openbox", ["openbox"], env)
            wait_for_wm(env)
            env.update(WLR_BACKENDS="x11", WLR_X11_OUTPUTS="1", WLR_RENDERER="pixman")
            config = pathlib.Path(private, "sway.conf")
            config.write_text(f"xwayland disable\nseat seat0 fallback true\noutput * mode 1200x800\n"
                              f"default_border none\nfor_window [title=\"{TITLE}\"] floating enable\n")
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
            client = dict(env, WINIT_UNIX_BACKEND="wayland", WAYLAND_DEBUG="1")
            client.pop("DISPLAY")
            protocol = args.output / "protocol.log"
            app = launch("application", [str(args.binary.resolve())], client,
                         stderr=protocol.open("w"))

            deadline = time.monotonic() + 20
            while True:
                tree = json.loads(subprocess.check_output(["swaymsg", "-t", "get_tree", "-r"], env=env, text=True))
                pending, rect = [tree], None
                while pending:
                    node = pending.pop()
                    pending.extend(node.get("nodes", []) + node.get("floating_nodes", []))
                    if node.get("name") == TITLE:
                        rect = node["rect"]
                if rect:
                    break
                if time.monotonic() > deadline or app.poll() is not None:
                    raise RuntimeError("the cursors window did not appear")
                time.sleep(.05)
            if "wp_cursor_shape_manager_v1" not in protocol.read_text():
                raise RuntimeError("the compositor offers no wp_cursor_shape_manager_v1")
            subprocess.check_call(["xdotool", "windowactivate", "--sync", host], env=env)
            time.sleep(.4)

            def shape_at(x, y, click=False):
                mark = len(protocol.read_text())
                command = ["xdotool", "mousemove", "--window", host, rect["x"] + x, rect["y"] + y]
                subprocess.check_call([*map(str, command), *(["click", "1"] if click else [])], env=env)
                # The last shape asked for once the pointer settles (entering
                # the surface asks for the default first).
                time.sleep(.3)
                deadline = time.monotonic() + 5
                while True:
                    shapes = re.findall(r"wp_cursor_shape_device_v1[#@]\d+\.set_shape\(\d+, (\d+)\)",
                                        protocol.read_text()[mark:])
                    if shapes or time.monotonic() > deadline:
                        return SHAPES.get(int(shapes[-1]), shapes[-1]) if shapes else None
                    time.sleep(.05)

            seen = {name: shape_at(*point) for name, point in [
                ("move", (90, 125)), ("disabled", (265, 125)), ("crosshair", (450, 125)),
                ("default", (590, 310))]}
            assert seen == {"move": "move", "disabled": "not-allowed", "crosshair": "crosshair",
                            "default": "default"}, seen
            # A press switches the crosshair region to waiting (reactive).
            shape_at(80, 220, click=True)
            seen["wait"] = shape_at(450, 125)
            assert seen["wait"] in ("wait", "progress"), seen
            seen["restored"] = shape_at(590, 310)
            assert seen["restored"] == "default", seen
            result = {"result": "pass", "platform": "native Wayland / Sway (wp_cursor_shape_v1)", "shapes": seen}
            (args.output / "result.json").write_text(json.dumps(result, indent=2) + "\n")
            print("PASS: move, not-allowed, crosshair, reactive wait and default through wp_cursor_shape_v1")
        finally:
            for process in reversed(processes):
                stop(process)


if __name__ == "__main__":
    main()
