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
  first item takes a click there;
- the submenus example's submenu, a popup of its menu, opens beside that
  menu where the compositor flipped it;
- a menu of 60 items, taller than the output, is cut to it by the
  compositor, laid out at that height, and scrolled to its last item, which
  takes a click;
- a popover with an arrow, flipped above its trigger by the compositor near
  the output's bottom, is placed there by the scene: its arrow on its
  bottom edge, pointing down at the trigger's centre;
- on outputs scaled 2× and 1.5× (fractional, through the viewporter), the
  menu's buffer is its size in the output's pixels, it shows where the
  scene put it in those pixels, and its last item takes a click.

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
# The submenus example's menu and its submenu.
SUBMENU_PARENT, SUBMENU = (0x36, 0x51, 0x6e), (0x4a, 0x37, 0x68)
# popup_probe's popover (`POPUP_PROBE_POPOVER`).
POPOVER = (0x4a, 0x37, 0x68)
# popup_probe's menu, in its window's points: the File button's centre, the
# menu's top-left as the scene places it, and its size.
FILE = (16 + 60, 396 + 14)
PLACED = (16, 428)
MENU_SIZE = (200, 202)


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
    processes, checks, base = [], {}, {}

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

    def scaled(scale):
        """popup_probe's menu on a headless sway output scaled `scale`."""
        name = f"sway {scale}x"
        output = (round(OUTPUT[0] * scale), round(OUTPUT[1] * scale))
        px = lambda points: round(points * scale)
        env = dict(base, WLR_BACKENDS="headless", WLR_RENDERER="pixman", WLR_LIBINPUT_NO_DEVICES="1")
        runtime = pathlib.Path(base["XDG_RUNTIME_DIR"], f"scaled-{scale}")
        runtime.mkdir(mode=0o700)
        env["XDG_RUNTIME_DIR"] = str(runtime)
        config = runtime / "sway.conf"
        config.write_text("output * mode %dx%d scale %g\ndefault_border none\n"
                          "for_window [app_id=\".*\"] floating enable, move position 0 0\n" % (*output, scale))
        sway, _ = launch(f"sway-{scale}x", ["sway", "--unsupported-gpu", "-c", str(config)], env)
        sockets = lambda: [p for p in runtime.glob("wayland-*") if p.suffix != ".lock"]
        display = until(f"{name}'s socket", sockets, 10)[0].name
        env["WAYLAND_DISPLAY"] = display
        app, log = launch(f"sway-{scale}x-app", [str(args.probe)], env)
        until(f"the probe on {name}", lambda: "READY" in log.read_text())

        def shot(stage):
            path = args.output / f"{scale}x-{stage}.png"
            subprocess.run(["grim", str(path)], env=env, check=True)
            return Image.open(path).convert("RGB")

        content = until(f"the window on {name}", lambda: box(shot("start"), CONTENT))
        pointer = VirtualPointer(str(runtime), display)

        def click(x, y):
            pointer.move_to(x, y, *output)
            time.sleep(.2)
            pointer.click()

        click(content[0] + px(FILE[0]), content[1] + px(FILE[1]))
        until(f"the menu on {name}", lambda: "popup configured" in log.read_text())
        time.sleep(.3)
        image = shot("open")
        shown = re.search(r"popup show .* buffer (\d+)x(\d+) scale ([\d.]+)", log.read_text())
        buffer = (int(shown.group(1)), int(shown.group(2))) if shown else None
        below = content[1] + px(FILE[1] + 15)
        menu = box(image.crop((0, below, image.width, image.height)), MENU)
        assert menu, f"{name}: no menu drawn"
        menu = (menu[0], menu[1] + below, menu[2], menu[3] + below)
        expected = (px(PLACED[0]), px(PLACED[1]), px(MENU_SIZE[0]), px(MENU_SIZE[1]))
        measured = (menu[0] - content[0], menu[1] - content[1], menu[2] - menu[0], menu[3] - menu[1])
        checks[name] = {"content": content, "menu": menu, "expected": expected, "buffer": buffer}
        # Drawn in the output's pixels: one buffer pixel to each.
        checks[f"{name}: buffer in the output's pixels"] = (
            buffer == (px(MENU_SIZE[0]), px(MENU_SIZE[1])) and float(shown.group(3)) == scale)
        # Within a pixel: a fractional scale rounds the box's edges.
        checks[f"{name}: where the scene put it, its size"] = all(
            abs(a - b) <= 1 for a, b in zip(measured, expected))
        # Its last item, below the window.
        click(menu[0] + px(100), menu[3] - px(17))
        until(f"the choice on {name}", lambda: 'CHOSEN "Item 8"' in log.read_text())
        checks[f"{name}: chosen outside the window"] = True
        pointer.close()
        stop(app)
        stop(sway)

    try:
        with tempfile.TemporaryDirectory(prefix="zgui-popups-") as private:
            os.chmod(private, 0o700)
            base.update(os.environ, XDG_RUNTIME_DIR=private, ZGUI_PRESENT_LOG="1")
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
            stop(app)

            # Taller than the output: cut to it by the compositor, the menu
            # is laid out at that height and scrolls to its last item.
            app, log = launch("sway-tall", [str(args.probe)], dict(env, POPUP_PROBE_ITEMS="60"))
            until("the tall probe", lambda: "READY" in log.read_text())
            content = until("its window", lambda: (lambda b: b if b and b[1] == 0 else None)(box(shot("tall"), CONTENT)))
            click(*FILE)
            until("the tall menu cut", lambda: "popup cut" in log.read_text())
            until("it shown anew", lambda: log.read_text().count("popup configured") >= 2)
            time.sleep(.3)
            menu = box(shot("tall-open"), MENU)
            cut = re.search(r"popup cut .* to (\d+)x(\d+)", log.read_text())
            shown = re.findall(r"popup show .* height: ([\d.]+) ", log.read_text())
            checks["sway: tall"] = {"menu": menu, "cut": cut.group(0), "shown heights": shown}
            checks["sway: a menu taller than the output is cut to it"] = (
                cut.groups() == ("200", str(OUTPUT[1])) and float(shown[-1]) == OUTPUT[1]
                and menu == (content[0] + 16, 0, content[0] + 216, OUTPUT[1]))
            pointer.move_to(menu[0] + 100, OUTPUT[1] // 2, *OUTPUT)
            time.sleep(.2)
            pointer.swipe(dy=60., frames=12)
            time.sleep(.8)
            shot("tall-scrolled")
            # The last item, 5 points of padding above the menu's bottom.
            click(menu[0] - content[0] + 100, menu[3] - content[1] - 5 - 12)
            until("a choice", lambda: 'CHOSEN "Item' in log.read_text())
            chosen = [line for line in log.read_text().splitlines() if line.startswith("OPEN")][-1]
            checks["sway: tall menu's choice"] = chosen
            checks["sway: its last item, scrolled to, takes a click"] = 'CHOSEN "Item 60"' in chosen
            stop(app)

            # A popover with an arrow, with no room below its trigger: the
            # compositor flips it, and the scene places it above, its arrow
            # on its bottom edge pointing down at the trigger.
            app, log = launch("sway-popover", [str(args.probe)], dict(env, POPUP_PROBE_POPOVER="1"))
            until("the popover probe", lambda: "READY" in log.read_text())
            until("its window", lambda: box(shot("popover"), CONTENT))
            subprocess.run(["swaymsg", "-s", sway_socket, "[app_id=popup_probe] move position 0 520"], env=env,
                           check=True, capture_output=True)
            time.sleep(.5)
            content = until("the moved window", lambda: (lambda b: b if b and b[1] == 520 else None)(
                box(shot("popover-moved"), CONTENT)))
            click(*FILE)
            until("the popover flipped", lambda: "popup flipped" in log.read_text())
            until("it shown anew", lambda: log.read_text().count("popup configured") >= 2)
            time.sleep(.3)
            image = shot("popover-flipped")
            popover = box(image, POPOVER)
            trigger_top, centre = content[1] + FILE[1] - 14, content[0] + FILE[0]
            # The arrow's tip: its last rows, a few pixels wide.
            tip = box(image.crop((0, popover[3] - 3, image.width, popover[3])), POPOVER) if popover else None
            checks["sway: popover"] = {"popover": popover, "tip": tip, "trigger top": trigger_top, "centre": centre,
                                        "flipped": re.search(r"popup flipped .*", log.read_text()).group(0)}
            checks["sway: a flipped popover shows above its trigger"] = bool(popover) and popover[3] <= trigger_top
            checks["sway: its arrow points down at the trigger"] = (
                bool(tip) and tip[2] - tip[0] < 12 and abs((tip[0] + tip[2]) / 2 - centre) <= 2
                and trigger_top - 6 <= popover[3] <= trigger_top)
            stop(app)

            # A submenu is a popup of its menu: near the output's bottom the
            # menu flips, and the submenu opens beside it there.
            sub_app, sub_log = launch("sway-submenus", [str(args.probe.with_name("submenus"))], env)
            until("the submenus window", lambda: box(shot("submenus"), CONTENT))
            subprocess.run(["swaymsg", "-s", sway_socket, "[app_id=submenus] move position 0 820"], env=env,
                           check=True, capture_output=True)
            time.sleep(.6)
            content = until("the moved submenus window", lambda: (lambda b: b if b and b[1] == 820 else None)(
                box(shot("submenus-moved"), CONTENT)))
            click(260 + 90, 68 + 16)  # Open actions.
            until("its menu", lambda: "popup configured" in sub_log.read_text())
            time.sleep(.3)
            menu = until("the flipped menu drawn", lambda: box(shot("submenus-open"), SUBMENU_PARENT))
            # Over More, its second item, where the menu shows.
            pointer.move_to(menu[0] + 100, menu[1] + 8 + 36 + 4 + 18, *OUTPUT)
            until("the submenu", lambda: sub_log.read_text().count("popup configured") >= 2)
            time.sleep(.3)
            image = shot("submenu-open")
            child = box(image, SUBMENU)
            checks["sway: submenu"] = {"menu": menu, "submenu": child}
            checks["sway: submenu beside its flipped menu"] = (
                bool(child) and menu[3] <= content[1] + 68 and child[1] == menu[1] + 48
                and abs(child[0] - menu[2]) <= 8)
            stop(sub_app)
            pointer.close()
            stop(app)
            stop(sway)

            # Scaled outputs: the same layout in points, in more pixels.
            for scale in (2, 1.5):
                scaled(scale)

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
        print("PASS: Wayland popups: past the window on sway and Weston, chosen from, closed, flipped at the output's "
              "edge, a submenu beside its flipped menu, a menu taller than the output cut to it and scrolled, "
              "a flipped popover's arrow facing its trigger, "
              "and on outputs scaled 2x and 1.5x")
    finally:
        for process in reversed(processes):
            stop(process)


if __name__ == "__main__":
    main()
