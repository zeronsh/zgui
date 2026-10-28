#!/usr/bin/env python3
"""A Luau board canvas and a measured list, natively, on X11 (Xvfb/Openbox,
xdotool) and as a Wayland client of a headless sway (virtual pointer):
scripts/luau_views/app.luau, run by zgx-desktop's luau_probe example (as
zgx-run runs it), its pixels read from screen captures and its calls from
what it prints.

- the canvas's `bg_grid` shows a 2-pixel magenta dot every 20 points, and
  `each`'s two shapes where they are drawn, each drawn once (`draw_item`);
- the wheel pans its `view` (X11 wheel buttons; on Wayland two fingers):
  the shapes and the grid move by the view's offset exactly, nothing drawn
  again;
- Zoom doubles the view: the shapes show twice the size at twice their
  place, the grid's marks 40 pixels apart and still 2 across, nothing
  drawn again;
- a press on a shape recolours it (`set_key`): that one item is drawn
  again, alone;
- Jump (`el:scroll_to_row(2000, { align = "center" })`) on a measured list
  of 3,000 rows one to three lines tall, guessed 30 points: row 2000's five
  lines (108 pixels) centred in the list to the pixel, its neighbours at
  their measured heights (48 and 28);
- a chip dragged onto the board (`on_drag`) springs it open once it has
  rested there past the delay (`on_drag_spring`, 0.5 s), not before, once
  however it then moves within it, and drops there (`on_drop`)."""
import argparse
import json
import os
import pathlib
import re
import select
import subprocess
import sys
import tempfile
import time

from PIL import Image, ImageChops, ImageGrab

sys.path.insert(0, str(pathlib.Path(__file__).parent))
from platform_smoke import release_mounts, stop, wait_for_wm  # noqa: E402
from wayland_pointer import VirtualPointer  # noqa: E402

APP = pathlib.Path(__file__).parent / "luau_views"
BOARD_BG, GRID, RED, GREEN = (16, 16, 16), (255, 0, 255), (255, 0, 0), (0, 255, 0)
# The app's places, in its window's points: the board, the buttons' centres,
# and the list's box.
BOARD = (0, 0, 400, 400)
# The drag chip's centre, and where on the board (empty) its drag rests.
DRAG, REST = (650, 20), (60, 340)
ZOOM, JUMP = (450, 20), (550, 20)
LIST = (400, 40, 800, 400)
# Rows (`lines` in the app): a line is 20 points, a row's padding 8.
ROW = lambda lines: lines * 20 + 8


def box(image, colour):
    """The box of `image`'s pixels of exactly `colour`."""
    solid = Image.new("RGB", image.size, colour)
    return ImageChops.difference(image, solid).convert("L").point(lambda v: 255 if v == 0 else 0).getbbox()


def runs(values):
    """Consecutive integers in sorted `values`, as (start, length)."""
    out = []
    for value in sorted(set(values)):
        if out and value == out[-1][0] + out[-1][1]:
            out[-1] = (out[-1][0], out[-1][1] + 1)
        else:
            out.append((value, 1))
    return out


def grid(board):
    """The board's dots across and down: (starts, sizes) of each axis's runs
    of grid pixels, whole runs only (not cut by the board's edges)."""
    pixels = board.load()
    xs, ys = [], []
    for y in range(board.height):
        for x in range(board.width):
            if pixels[x, y] == GRID:
                xs.append(x)
                ys.append(y)
    axes = []
    for values, size in ((xs, board.width), (ys, board.height)):
        found = [r for r in runs(values) if r[0] > 0 and r[0] + r[1] < size]
        axes.append(([r[0] for r in found], sorted({r[1] for r in found})))
    return axes


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("runner", type=pathlib.Path, help="the luau_probe example")
    parser.add_argument("--output", type=pathlib.Path, required=True)
    args = parser.parse_args()
    args.output.mkdir(parents=True, exist_ok=True)
    processes, checks = [], {}

    def launch(name, command, env):
        log = args.output / (name + ".log")
        processes.append(subprocess.Popen(command, env=env, stdout=log.open("w"), stderr=subprocess.STDOUT))
        return processes[-1], log

    def until(what, test, seconds=15):
        deadline = time.monotonic() + seconds
        while True:
            value = test()
            if value:
                return value
            if time.monotonic() > deadline:
                raise AssertionError(f"timed out waiting for {what}: {json.dumps(checks, indent=2)}")
            time.sleep(.05)

    def session(name, env, shot, origin, wheel, click, pointer):
        """The checks on one display: `shot()` captures it, `origin()` finds
        the window's content, `wheel()` pans the board, `click(x, y)` presses
        at the window's points, `pointer(x, y, held)` moves there (at the
        window's points) with the button held or not."""
        app, log = launch(f"{name}-app", [str(args.runner), str(APP)], env)
        text = lambda: log.read_text(errors="replace")
        draws = lambda: re.findall(r"^DRAW_ITEM (\d+)$", text(), re.M)
        views = lambda: [tuple(map(float, v)) for v in re.findall(r"^VIEW (\S+) (\S+) (\S+)$", text(), re.M)]
        until(f"{name}: both shapes drawn", lambda: len(draws()) >= 2)
        at = until(f"{name}: the window", origin)

        def board(label):
            image = shot(f"{name}-{label}")
            return image.crop((at[0] + BOARD[0], at[1] + BOARD[1], at[0] + BOARD[2], at[1] + BOARD[3]))

        def settled():
            """The view, once no new one came for half a second."""
            last = None
            while True:
                seen = len(views())
                time.sleep(.5)
                if len(views()) == seen and seen == last:
                    return views()[-1] if seen else (0., 0., 1.)
                last = seen

        def shapes(image, view, label):
            """Where each shape shows, and where the view puts it on the
            board (cut to it): a pan by fingers leaves fractions, whose edge
            pixels are blended, so places are compared to a pixel."""
            x, y, zoom = view
            expect = {"red": (40, 40, 60, 40), "blue": (200, 120, 40, 40)}
            got = {}
            for colour, rgb in (("red", RED), ("blue", (0, 0, 255)), ("green", GREEN)):
                found = box(image, rgb)
                got[colour] = list(found) if found else None
            checks[f"{name} {label} shapes"] = got
            cut = lambda v, size: min(max(v, 0), size)
            want = lambda s: [cut(s[0] * zoom + x, BOARD[2]), cut(s[1] * zoom + y, BOARD[3]),
                              cut((s[0] + s[2]) * zoom + x, BOARD[2]), cut((s[1] + s[3]) * zoom + y, BOARD[3])]
            return {colour: want(expect[colour]) for colour in expect}, got

        near = lambda got, want: got is not None and all(abs(g - w) <= 1 for g, w in zip(got, want))

        def on_grid(starts, offset, step):
            """Whether every dot starts where the view puts a grid point,
            `offset` from it, `step` apart (to a pixel)."""
            return bool(starts) and all(min((g - offset) % step, -(g - offset) % step) <= 1 for g in starts)

        # At rest: the shapes where drawn, the grid's dots 2 pixels across,
        # 20 apart.
        start = board("start")
        wanted, got = shapes(start, (0., 0., 1.), "start")
        checks[f"{name}: shapes drawn where they are"] = got["red"] == wanted["red"] and got["blue"] == wanted["blue"]
        (gx, sizes_x), (gy, sizes_y) = grid(start)
        # A dot's start from its grid point (a dot is centred on it).
        dot = ((gx[0] + 10) % 20 - 10, (gy[0] + 10) % 20 - 10) if gx and gy else (0, 0)
        checks[f"{name} grid at rest"] = {"dot from its point": dot, "sizes": [sizes_x, sizes_y],
                                          "dots": [len(gx), len(gy)]}
        checks[f"{name}: grid dots 2 across, every 20"] = (
            sizes_x == [2] and sizes_y == [2] and on_grid(gx, dot[0], 20) and on_grid(gy, dot[1], 20)
            and len(gx) >= 15)
        checks[f"{name}: each shape drawn once"] = sorted(draws()) == ["1", "2"]

        # Panned by the wheel: the view's offset exactly, nothing drawn again.
        wheel()
        view = settled()
        checks[f"{name} panned view"] = view
        panned = board("panned")
        wanted, got = shapes(panned, view, "panned")
        checks[f"{name}: panned (the view moved)"] = view[0] != 0 and view[1] != 0 and view[2] == 1
        checks[f"{name}: shapes moved by the view"] = near(got["red"], wanted["red"])
        (gx, _), (gy, _) = grid(panned)
        checks[f"{name}: grid moved by the view"] = (
            on_grid(gx, dot[0] + view[0], 20) and on_grid(gy, dot[1] + view[1], 20))
        checks[f"{name}: panning draws nothing again"] = len(draws()) == 2

        # Zoomed: twice the size at twice the place, the marks 40 apart and
        # still 2 across.
        click(*ZOOM)
        until(f"{name}: zoomed", lambda: views() and views()[-1][2] == 2)
        view = settled()
        zoomed = board("zoomed")
        wanted, got = shapes(zoomed, view, "zoomed")
        checks[f"{name} zoomed view"] = view
        checks[f"{name}: zoomed shape twice the size, at twice its place"] = near(got["red"], wanted["red"])
        (gx, sizes_x), (gy, sizes_y) = grid(zoomed)
        checks[f"{name} zoomed grid"] = {"sizes": [sizes_x, sizes_y], "first": [gx[:1], gy[:1]]}
        checks[f"{name}: zoomed grid's marks keep their size, 40 apart"] = (
            sizes_x == [2] and sizes_y == [2] and on_grid(gx, dot[0] + view[0], 40)
            and on_grid(gy, dot[1] + view[1], 40))
        checks[f"{name}: zooming draws nothing again"] = len(draws()) == 2

        # A press on the red shape: it alone is drawn again, green.
        red = wanted["red"]
        click(round((red[0] + red[2]) / 2), round((red[1] + red[3]) / 2))
        until(f"{name}: recoloured", lambda: "RECOLOR 1" in text())
        until(f"{name}: drawn again", lambda: len(draws()) >= 3)
        time.sleep(.3)
        recoloured = board("recoloured")
        _, got = shapes(recoloured, view, "recoloured")
        checks[f"{name}: a written item is drawn again, alone"] = draws()[2:] == ["1"]
        checks[f"{name}: it shows recoloured"] = near(got["green"], red) and got["red"] is None

        # Jump: row 2000 centred, at its measured height, as its neighbours.
        click(*JUMP)
        until(f"{name}: jumped", lambda: "JUMP 2000" in text())
        time.sleep(.5)
        image = shot(f"{name}-jumped")
        column = [image.getpixel((at[0] + LIST[2] - 4, at[1] + y)) for y in range(LIST[1], LIST[3])]
        bands = []
        for offset, rgb in enumerate(column):
            if bands and bands[-1][2] == rgb:
                bands[-1][1] += 1
            else:
                bands.append([LIST[1] + offset, 1, rgb])
        green = [i for i, b in enumerate(bands) if tuple(b[2]) == GREEN]
        checks[f"{name} list bands"] = [b[:2] for b in bands]
        middle = (LIST[1] + LIST[3]) / 2
        if len(green) == 1 and 0 < green[0] < len(bands) - 1:
            top, height, _ = bands[green[0]]
            checks[f"{name}: row 2000 centred, to the pixel"] = abs(top + height / 2 - middle) <= .5
            checks[f"{name}: row 2000 at its measured height"] = height == ROW(5)
            checks[f"{name}: its neighbours at theirs"] = (
                bands[green[0] - 1][1] == ROW(1999 % 3 + 1) and bands[green[0] + 1][1] == ROW(2001 % 3 + 1))
        else:
            checks[f"{name}: row 2000 shows, whole"] = False
        checks[f"{name}: the jump draws nothing on the board"] = len(draws()) == 3

        # The chip dragged to an empty spot on the board, resting: it
        # springs open past the delay, once, and takes the drop.
        sprung = lambda: text().count("SPRUNG chip")
        pointer(*DRAG, False)
        pointer(*DRAG, True)
        entered = None
        for step in range(1, 11):
            x = DRAG[0] + (REST[0] - DRAG[0]) * step // 10
            pointer(x, DRAG[1] + (REST[1] - DRAG[1]) * step // 10, True)
            # The delay runs from the step that comes onto the board
            # (moving on within it does not start it again).
            if entered is None and x < BOARD[2]:
                entered = time.monotonic()
            time.sleep(.03)
        until(f"{name}: the drag", lambda: "DRAGGING chip" in text())
        time.sleep(max(0., entered + .3 - time.monotonic()))
        early, read_at = sprung(), time.monotonic() - entered
        # Read later than the delay's start allows (a slow machine), it says
        # nothing about before.
        checks[f"{name}: not sprung before the delay"] = early == 0 if read_at < .45 else f"read late, {read_at:.2f} s"
        until(f"{name}: sprung", sprung, 5)
        checks[f"{name} sprung after"] = after = round(time.monotonic() - entered, 2)
        checks[f"{name}: sprung past the delay"] = after >= .45
        pointer(REST[0] + 30, REST[1], True)
        time.sleep(1)
        checks[f"{name}: sprung once, however it moves within"] = sprung() == 1
        pointer(REST[0] + 30, REST[1], False)
        until(f"{name}: the drop", lambda: "DROPPED chip" in text())
        checks[f"{name}: the drag drops on the board"] = True
        stop(app)

    try:
        with tempfile.TemporaryDirectory(prefix="zgx-luau-views-") as private:
            os.chmod(private, 0o700)
            base = dict(os.environ, XDG_RUNTIME_DIR=private, XDG_CONFIG_HOME=f"{private}/config",
                        XDG_DATA_HOME=f"{private}/data", ZGX_LINT="0",
                        DBUS_SESSION_BUS_ADDRESS="unix:path=/nonexistent")
            for key in ("DISPLAY", "WAYLAND_DISPLAY"):
                base.pop(key, None)

            # X11: Xvfb and Openbox, the wheel's buttons and clicks by xdotool.
            xvfb = subprocess.Popen(["Xvfb", "-r", "-noreset", "-displayfd", "1", "-screen", "0", "1280x900x24",
                                     "-ac"], stdout=subprocess.PIPE, stderr=(args.output / "xvfb.log").open("w"),
                                    text=True)
            processes.append(xvfb)
            if not select.select([xvfb.stdout], [], [], 30)[0]:
                raise RuntimeError("Xvfb startup timed out")
            display = ":" + xvfb.stdout.readline().strip()
            x11 = dict(base, DISPLAY=display)
            launch("openbox", ["openbox"], x11)
            wait_for_wm(x11)
            xdo = lambda *a: subprocess.run(["xdotool", *map(str, a)], env=x11, check=True,
                                            capture_output=True, text=True).stdout.strip()

            def x11_shot(label):
                image = ImageGrab.grab(xdisplay=display).convert("RGB")
                image.save(args.output / f"{label}.png")
                return image

            def x11_window():
                found = subprocess.run(["xdotool", "search", "--onlyvisible", "--name", "^Luau views$"], env=x11,
                                       capture_output=True, text=True).stdout.split()
                return found[0] if found else None

            def x11_origin():
                window = x11_window()
                if not window:
                    return None
                info = subprocess.run(["xwininfo", "-id", window], env=x11, capture_output=True, text=True).stdout
                found = [re.search(rf"Absolute upper-left {a}:\s+(-?\d+)", info) for a in "XY"]
                if not all(found):
                    return None
                at = (int(found[0].group(1)), int(found[1].group(1)))
                # Shown: the board's background at its corner.
                return at if x11_shot("x11-start").getpixel((at[0] + 1, at[1] + 399)) == BOARD_BG else None

            def x11_wheel():
                # A notch up, a notch left: the view moves right and down.
                xdo("mousemove", "--window", x11_window(), 300, 300, "click", 4, "click", 6)

            def x11_click(x, y):
                xdo("mousemove", "--window", x11_window(), x, y, "click", 1)

            held = [False]

            def x11_pointer(x, y, down):
                xdo("mousemove", "--window", x11_window(), x, y)
                if down != held[0]:
                    xdo("mousedown" if down else "mouseup", 1)
                    held[0] = down

            session("x11", x11, x11_shot, x11_origin, x11_wheel, x11_click, x11_pointer)

            # Wayland: a headless sway, its window at the output's top left,
            # driven by a virtual pointer and captured by grim.
            sway_env = dict(base, WLR_BACKENDS="headless", WLR_RENDERER="pixman", WLR_LIBINPUT_NO_DEVICES="1")
            config = pathlib.Path(private, "sway.conf")
            config.write_text("output * mode 1280x960\ndefault_border none\n"
                              "for_window [app_id=\".*\"] floating enable, move position 0 0\n")
            sway, _ = launch("sway", ["sway", "--unsupported-gpu", "-c", str(config)], sway_env)
            sockets = lambda: [p for p in pathlib.Path(private).glob("wayland-*") if p.suffix != ".lock"]
            socket = until("sway's socket", sockets, 10)[0].name
            wayland = dict(sway_env, WAYLAND_DISPLAY=socket)
            pointer = VirtualPointer(private, socket)

            def sway_shot(label):
                path = args.output / f"{label}.png"
                subprocess.run(["grim", str(path)], env=wayland, check=True)
                return Image.open(path).convert("RGB")

            def sway_origin():
                return (0, 0) if sway_shot("wayland-start").getpixel((1, 399)) == BOARD_BG else None

            def park():
                # Off the window: grim captures the cursor.
                pointer.move_to(1200, 900, 1280, 960)

            def sway_wheel():
                pointer.move_to(300, 300, 1280, 960)
                time.sleep(.2)
                # Two fingers up, then left: the view moves down and right.
                # (One axis a swipe: sway asserts on a frame scrolling both
                # from a virtual pointer, the second axis left sourceless.)
                pointer.swipe(dy=-3., frames=4)
                pointer.swipe(dx=-3., frames=4)
                park()

            def sway_click(x, y):
                pointer.move_to(x, y, 1280, 960)
                time.sleep(.2)
                pointer.click()
                park()

            sway_held = [False]

            def sway_pointer(x, y, down):
                pointer.move_to(x, y, 1280, 960)
                if down != sway_held[0]:
                    pointer.button(down)
                    sway_held[0] = down
                    if not down:
                        park()

            park()
            session("wayland", wayland, sway_shot, sway_origin, sway_wheel, sway_click, sway_pointer)
            pointer.close()
            stop(sway)
            for process in reversed(processes):
                stop(process)
            release_mounts(private)

        (args.output / "result.json").write_text(json.dumps(checks, indent=2) + "\n")
        failed = [key for key, value in checks.items() if value is False]
        assert not failed, f"failed: {failed}\n{json.dumps(checks, indent=2)}"
        print("PASS: a Luau canvas pans and zooms its view over its grid, drawing nothing again, and draws a "
              "written item alone; a measured list jumps to a row, centred at its measured height (X11, Wayland)")
    finally:
        for process in reversed(processes):
            stop(process)


if __name__ == "__main__":
    main()
