#!/usr/bin/env python3
"""Popups past the window on Wayland (`popup_window/wayland.rs`): popup_probe's
menu, which hangs past its window's bottom edge, shown as an `xdg_popup`.

Under a headless sway, driven with a virtual pointer (`wayland_pointer.py`):

- a click on File opens the menu in a popup of its own, where the scene put
  it (16 points right of the content's left, 428 below its top) and past the
  window's bottom edge, drawn there;
- a click on its last item, outside the window, chooses it, and the popup
  goes;
- opened again, a click elsewhere in the window closes it;
- moved near the output's bottom, the menu flips above its trigger (the
  compositor's doing: the positioner is anchored on the trigger), and its
  first item takes a click there.

Under a headless Weston, whose windows winit decorates itself (the window
geometry starts at its title bar, above the content), the menu opened by the
app (`POPUP_PROBE_OPEN`) shows at the same place on the content: the title
bar is accounted for."""
import argparse
import json
import os
import pathlib
import re
import subprocess
import sys
import tempfile
import time

from PIL import Image, ImageChops

sys.path.insert(0, str(pathlib.Path(__file__).parent))
from platform_smoke import stop  # noqa: E402
from wayland_pointer import VirtualPointer  # noqa: E402

OUTPUT = (1280, 960)
CONTENT = (16, 20, 28)
MENU = (35, 46, 64)
# popup_probe's menu, in its window's points: the File button's centre, and
# the menu's top-left as the scene places it.
FILE = (16 + 60, 396 + 14)
PLACED = (16, 428)


def box(image, colour):
    """The box of `image`'s pixels of exactly `colour`."""
    solid = Image.new("RGB", image.size, colour)
    return ImageChops.difference(image, solid).convert("L").point(lambda v: 255 if v == 0 else 0).getbbox()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("probe", type=pathlib.Path, help="the popup_probe example")
    parser.add_argument("--output", type=pathlib.Path, required=True)
    args = parser.parse_args()
    args.output.mkdir(parents=True, exist_ok=True)
    processes, checks = [], {}

    def launch(name, command, env, **options):
        log = args.output / (name + ".log")
        processes.append(subprocess.Popen(command, env=env, stdout=log.open("w"), stderr=subprocess.STDOUT,
                                          **options))
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

    def placed(name, image):
        """The menu's place against the window's content."""
        content = box(image, CONTENT)
        assert content, f"{name}: no window drawn"
        # Below the File button, which has the menu's colour.
        below = content[1] + FILE[1] + 15
        menu = box(image.crop((0, below, image.width, image.height)), MENU)
        assert menu, f"{name}: no menu drawn"
        menu = (menu[0], menu[1] + below, menu[2], menu[3] + below)
        checks[name] = {"content": content, "menu": menu}
        return (menu[0] - content[0], menu[1] - content[1]) == PLACED and menu[1] < content[3] < menu[3]

    try:
        with tempfile.TemporaryDirectory(prefix="zgui-popups-") as private:
            os.chmod(private, 0o700)
            base = dict(os.environ, XDG_RUNTIME_DIR=private, ZGUI_PRESENT_LOG="1")
            for key in ("DISPLAY", "WAYLAND_DISPLAY", "DBUS_SESSION_BUS_ADDRESS"):
                base.pop(key, None)

            # sway: clicked.
            env = dict(base, WLR_BACKENDS="headless", WLR_RENDERER="pixman", WLR_LIBINPUT_NO_DEVICES="1")
            config = pathlib.Path(private, "sway.conf")
            config.write_text("output * mode %dx%d\ndefault_border none\n"
                              "for_window [app_id=\".*\"] floating enable, move position 0 0\n" % OUTPUT)
            sway, _ = launch("sway", ["sway", "--unsupported-gpu", "-c", str(config)], env)
            sockets = lambda: [p for p in pathlib.Path(private).glob("wayland-*") if p.suffix != ".lock"]
            display = until("sway's socket", sockets, 10)[0].name
            env["WAYLAND_DISPLAY"] = display
            app, log = launch("sway-app", [str(args.probe)], env)
            until("the probe", lambda: "READY" in log.read_text())

            def shot(name):
                path = args.output / f"{name}.png"
                subprocess.run(["grim", str(path)], env=env, check=True)
                return Image.open(path).convert("RGB")

            content = until("the window", lambda: box(shot("start"), CONTENT))
            pointer = VirtualPointer(private, display)

            def click(x, y):
                pointer.move_to(content[0] + x, content[1] + y, *OUTPUT)
                time.sleep(.2)
                pointer.click()

            click(*FILE)
            until("the menu", lambda: "popup configured" in log.read_text())
            time.sleep(.3)
            checks["sway: past the window, where the scene put it"] = placed("sway", shot("open"))
            menu = checks["sway"]["menu"]
            # Its last item, below the window.
            click(menu[0] - content[0] + 100, menu[3] - content[1] - 17)
            until("the choice", lambda: 'CHOSEN "Item 8"' in log.read_text())
            checks["sway: chosen outside the window"] = True
            until("the popup to go", lambda: log.read_text().count("popup hide") >= 1)
            click(*FILE)
            until("the menu again", lambda: log.read_text().count("popup configured") >= 2)
            time.sleep(.3)
            click(400, 100)
            until("the menu closed", lambda: log.read_text().count("popup hide") >= 2)
            reports = [line for line in log.read_text().splitlines() if line.startswith("OPEN")]
            checks["sway: a click in the window closes it"] = reports[-1].startswith("OPEN false")
            # Near the output's bottom, the menu has no room below its
            # trigger: the compositor flips it above, where its items take
            # clicks as before.
            sway_socket = str(next(pathlib.Path(private).glob("sway-ipc*.sock")))
            subprocess.run(["swaymsg", "-s", sway_socket, "[app_id=popup_probe] move position 0 520"], env=env,
                           check=True, capture_output=True)
            time.sleep(.5)
            content = until("the moved window", lambda: (lambda b: b if b and b[1] == 520 else None)(
                box(shot("moved"), CONTENT)))
            click(*FILE)
            until("the flipped menu", lambda: log.read_text().count("popup configured") >= 3)
            time.sleep(.3)
            image = shot("flipped")
            menu = box(image.crop((0, 0, image.width, content[1] + FILE[1] - 14)), MENU)
            checks["sway: flipped"] = {"content": content, "menu": menu}
            checks["sway: flipped above its trigger"] = bool(menu) and menu[3] == content[1] + FILE[1] - 14 - 4
            # Its first item, at the top of the flipped menu.
            click(menu[0] - content[0] + 100, menu[1] - content[1] + 17)
            until("the choice", lambda: 'CHOSEN "Item 1"' in log.read_text())
            checks["sway: chosen in the flipped menu"] = True
            pointer.close()
            stop(app)
            stop(sway)

            # Weston: winit's title bar above the content.
            env = dict(base, WAYLAND_DISPLAY="zgui-popups")
            weston, _ = launch("weston", ["weston", "--backend=headless", "--renderer=pixman", "--width=%d" % OUTPUT[0],
                                          "--height=%d" % OUTPUT[1], "--idle-time=0", "--no-config", "--debug",
                                          "--socket=zgui-popups"], env)
            until("Weston's socket", lambda: pathlib.Path(private, "zgui-popups").exists(), 10)
            app, log = launch("weston-app", [str(args.probe)], dict(env, POPUP_PROBE_OPEN="1"))
            until("the menu", lambda: "popup configured" in log.read_text())
            time.sleep(.5)
            shots = set(args.output.glob("wayland-screenshot*.png"))
            subprocess.run(["weston-screenshooter"], env=dict(env, XDG_PICTURES_DIR=str(args.output)),
                           cwd=args.output, check=True, capture_output=True, timeout=10)
            screenshot = sorted(set(args.output.glob("wayland-screenshot*.png")) - shots)[0]
            configured = re.search(r"popup configured at (-?\d+),(-?\d+)", log.read_text())
            checks["weston: configured at"] = configured.group(0)
            checks["weston: below winit's title bar, where the scene put it"] = placed(
                "weston", Image.open(screenshot).convert("RGB"))
            stop(app)
            stop(weston)

        (args.output / "result.json").write_text(json.dumps(checks, indent=2) + "\n")
        failed = [key for key, value in checks.items() if value is False]
        assert not failed, f"failed: {failed}\n{json.dumps(checks, indent=2)}"
        print("PASS: Wayland popups: past the window on sway and Weston, chosen from, closed, and flipped at the output's edge")
    finally:
        for process in reversed(processes):
            stop(process)


if __name__ == "__main__":
    main()
