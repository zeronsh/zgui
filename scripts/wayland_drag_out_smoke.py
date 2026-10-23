#!/usr/bin/env python3
"""Drags out of zgui on Wayland (`native_drag.rs`, `wayland_data.rs`): under a
headless sway, drag_probe's chip, exporting its text, dragged with a virtual
pointer (`wayland_pointer.py`) out of its window onto test-owned GTK drop
targets (`xdnd_target.py`, GTK's Wayland drag and drop) below it. The
display is the probe's own, so nothing else can take a drop.

Checks that:

- the target takes the chip's text through the compositor's drag, and the
  source hears it accepted (`DRAG_END accepted=true`); the drag's icon, the
  chip's preview, shows under the pointer over the target;
- dragged out over the target and back into its own window and let go
  there, the drag stays zgui's: the target gets nothing, and the source
  hears it not accepted;
- over a target that refuses it (and hears it move), let go there, nothing
  is dropped, not accepted;
- a window's only tab (`drag_moves_window`) moves its window, the
  compositor moving it as the pointer goes."""
import argparse
import json
import os
import pathlib
import re
import subprocess
import sys
import tempfile
import time

from PIL import Image

sys.path.insert(0, str(pathlib.Path(__file__).parent))
from platform_smoke import stop  # noqa: E402
from wayland_pointer import VirtualPointer  # noqa: E402

OUTPUT = (1600, 1000)
# drag_probe's chip, in its Drag window (at the output's top-left).
CHIP = (10 + 60, 10 + 14)
TAKES, REFUSES = (200, 700), (700, 700)
# The chip preview's fill.
PREVIEW = (61, 126, 247)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("binaries", type=pathlib.Path, help="the zgui-desktop examples' directory")
    parser.add_argument("--output", type=pathlib.Path, required=True)
    args = parser.parse_args()
    args.output.mkdir(parents=True, exist_ok=True)
    processes, checks = [], {}

    def launch(name, command, env):
        log = args.output / (name + ".log")
        processes.append(subprocess.Popen(list(map(str, command)), env=env, stdout=log.open("w"),
                                          stderr=subprocess.STDOUT))
        return processes[-1], log

    def until(what, test, seconds=15):
        deadline = time.monotonic() + seconds
        while True:
            value = test()
            if value:
                return value
            if time.monotonic() > deadline:
                raise AssertionError(f"timed out waiting for {what}: {checks}")
            time.sleep(.05)

    try:
        with tempfile.TemporaryDirectory(prefix="zgui-drag-out-") as private:
            os.chmod(private, 0o700)
            env = dict(os.environ, XDG_RUNTIME_DIR=private, WLR_BACKENDS="headless", WLR_RENDERER="pixman",
                       WLR_LIBINPUT_NO_DEVICES="1", GDK_BACKEND="wayland", NO_AT_BRIDGE="1",
                       ZGUI_PRESENT_LOG="1")
            for key in ("DISPLAY", "WAYLAND_DISPLAY", "DBUS_SESSION_BUS_ADDRESS"):
                env.pop(key, None)
            config = pathlib.Path(private, "sway.conf")
            config.write_text(
                "output * mode %dx%d\ndefault_border none\nfor_window [app_id=\".*\"] floating enable\n"
                "for_window [title=\"Drag\"] move position 0 0\n"
                "for_window [title=\"Other\"] move position 900 0\n"
                "for_window [title=\"Takes drops\"] move position 0 600\n"
                "for_window [title=\"Refuses drops\"] move position 500 600\n"
                "for_window [title=\"Solo\"] move position 900 600\n" % OUTPUT)
            sway, _ = launch("sway", ["sway", "--unsupported-gpu", "-c", config], env)
            sockets = lambda: [p for p in pathlib.Path(private).glob("wayland-*") if p.suffix != ".lock"]
            display = until("sway's socket", sockets, 10)[0].name
            env["WAYLAND_DISPLAY"] = display
            app, app_log = launch("drag_probe", [args.binaries / "drag_probe"], env)
            _, takes_log = launch("takes", ["python3", "scripts/xdnd_target.py", "Takes drops", 0, 0], env)
            _, refuses_log = launch("refuses", ["python3", "scripts/xdnd_target.py", "Refuses drops", 0, 0,
                                                "--refuse"], env)
            until("the targets", lambda: "READY" in takes_log.read_text() and "READY" in refuses_log.read_text())
            until("the windows", lambda: "render" in app_log.read_text())
            time.sleep(1)
            pointer = VirtualPointer(private, display)

            def glide(start, end, steps=15):
                for step in range(1, steps + 1):
                    pointer.move_to(start[0] + (end[0] - start[0]) * step // steps,
                                    start[1] + (end[1] - start[1]) * step // steps, *OUTPUT)
                    time.sleep(.03)

            def grab():
                pointer.move_to(*CHIP, *OUTPUT)
                time.sleep(.2)
                pointer.button(True)
                time.sleep(.05)

            def ends():
                return len(re.findall(r"^DRAG_END", app_log.read_text(), re.M))

            # Onto the taking target.
            grab()
            glide(CHIP, TAKES)
            time.sleep(.4)
            shot = args.output / "over-target.png"
            subprocess.run(["grim", str(shot)], env=env, check=True)
            pixel = Image.open(shot).convert("RGB").getpixel((TAKES[0] - 40, TAKES[1] - 8))
            checks["icon under the pointer"] = pixel == PREVIEW
            pointer.button(False)
            until("the drag's end", lambda: ends() == 1)
            until("the drop's text", lambda: "RECEIVED" in takes_log.read_text(), 5)
            received = re.search(r"^RECEIVED (\S+) '(.*)'$", takes_log.read_text(), re.M)
            checks["received"] = received.group(1, 2) if received else None
            checks["the target takes the text"] = bool(received) and received.group(2) == "Some text"
            checks["accepted"] = "DRAG_END accepted=true" in app_log.read_text()

            # Out over the target and back home: zgui's.
            time.sleep(.5)
            grab()
            glide(CHIP, TAKES)
            time.sleep(.3)
            glide(TAKES, (420, 200))
            time.sleep(.3)
            pointer.button(False)
            until("the drag's end", lambda: ends() == 2)
            time.sleep(.5)
            checks["back home: nothing dropped"] = takes_log.read_text().count("RECEIVED") == 1
            checks["back home: not accepted"] = app_log.read_text().count("DRAG_END accepted=false") == 1

            # Over a target that refuses it.
            grab()
            glide(CHIP, REFUSES)
            time.sleep(.3)
            pointer.button(False)
            until("the drag's end", lambda: ends() == 3)
            time.sleep(.3)
            checks["refused: heard it move"] = "MOTION" in refuses_log.read_text()
            checks["refused: nothing dropped"] = "RECEIVED" not in refuses_log.read_text()
            checks["refused: not accepted"] = app_log.read_text().count("DRAG_END accepted=false") == 2
            # A window's only tab moves the window: the compositor's move,
            # following the pointer from where the drag began.
            sway_socket = str(next(pathlib.Path(private).glob("sway-ipc*.sock")))

            def solo():
                tree = json.loads(subprocess.run(["swaymsg", "-s", sway_socket, "-t", "get_tree"], env=env,
                                                 capture_output=True, text=True).stdout)
                nodes = [tree]
                while nodes:
                    node = nodes.pop()
                    if node.get("name") == "Solo":
                        return node["rect"]["x"], node["rect"]["y"]
                    nodes += node.get("nodes", []) + node.get("floating_nodes", [])
                return None

            before = until("the Solo window", solo)
            start = (before[0] + CHIP[0], before[1] + CHIP[1])
            end = (start[0] - 300, start[1] + 150)
            pointer.move_to(*start, *OUTPUT)
            time.sleep(.2)
            pointer.button(True)
            time.sleep(.05)
            glide(start, end)
            time.sleep(.3)
            pointer.button(False)
            until("the tab's end", lambda: "TAB_END solo" in app_log.read_text())
            time.sleep(.3)
            after = solo()
            moved = (after[0] - before[0], after[1] - before[1])
            checks["only tab moved its window by"] = moved
            # Less what the pointer moved before the drag began.
            checks["only tab moves its window"] = (abs(moved[0] + 300) <= 30 and abs(moved[1] - 150) <= 30)
            checks["app alive"] = app.poll() is None
            pointer.close()

        (args.output / "result.json").write_text(json.dumps(checks, indent=2) + "\n")
        failed = [key for key, value in checks.items() if value is False]
        assert not failed, f"failed: {failed}\n{json.dumps(checks, indent=2)}"
        print("PASS: Wayland drags out of zgui: a GTK target takes the text under its icon; back home and over "
              "a refusing target nothing drops; a window's only tab moves its window")
    finally:
        for process in reversed(processes):
            stop(process)


if __name__ == "__main__":
    main()
