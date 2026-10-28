#!/usr/bin/env python3
"""Images through the clipboard on X11 (Xvfb/Openbox, xclip) and as a
Wayland client of a headless sway (wl-copy, wl-paste, a virtual pointer):
the clipboard_image example's Paste and Copy, clicked.

- a 30 by 20 PNG another client put there as `image/png`, its left columns
  half transparent, pastes at its size with its pixels, alpha straight
  (the example prints their FNV-1a digest);
- Copy offers its 64 by 48 gradient as `image/png`, which the other
  client reads back pixel for pixel, its transparent column included."""
import argparse
import io
import json
import os
import pathlib
import re
import select
import subprocess
import sys
import tempfile
import time

from PIL import Image

sys.path.insert(0, str(pathlib.Path(__file__).parent))
from platform_smoke import release_mounts, stop, wait_for_wm  # noqa: E402
from wayland_pointer import VirtualPointer  # noqa: E402

# The example's buttons, in its window's points.
PASTE, COPY = (80, 51), (210, 51)


def digest(pixels):
    """FNV-1a over RGBA bytes, as the example prints it."""
    value = 0xcbf29ce484222325
    for byte in pixels:
        value = ((value ^ byte) * 0x100000001b3) & 0xffffffffffffffff
    return f"{value:016x}"


def source():
    """The image another client offers: 30 by 20, its 5 left columns half
    transparent."""
    image = Image.new("RGBA", (30, 20))
    for y in range(20):
        for x in range(30):
            image.putpixel((x, y), (x * 8, y * 12, 100, 128 if x < 5 else 255))
    return image


def gradient():
    """The example's Copy: 64 by 48, red across, green down, its left
    quarter fading in from transparent."""
    image = Image.new("RGBA", (64, 48))
    for y in range(48):
        for x in range(64):
            image.putpixel((x, y), (x * 4, y * 5, 160, x * 16 if x < 16 else 255))
    return image


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("example", type=pathlib.Path, help="the clipboard_image example")
    parser.add_argument("--output", type=pathlib.Path, required=True)
    args = parser.parse_args()
    args.output.mkdir(parents=True, exist_ok=True)
    processes, checks = [], {}
    png = io.BytesIO()
    source().save(png, "PNG")
    offered = png.getvalue()
    pasted = f"PASTED 30x20 {digest(source().tobytes())} first=[0, 0, 100, 128]"
    copied = f"COPIED written=true {digest(gradient().tobytes())}"

    def launch(name, command, env, **options):
        log = args.output / (name + ".log")
        processes.append(subprocess.Popen(command, env=env, stdout=options.pop("stdout", log.open("w")),
                                          stderr=subprocess.STDOUT, **options))
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

    def session(name, env, offer, read, click):
        """`offer(bytes)` puts the PNG on the clipboard as another client,
        `read()` reads its `image/png` back (or None), `click(x, y)`
        presses in the example's window."""
        offer(offered)
        app, log = launch(f"{name}-app", [str(args.example.resolve())], env)
        # Paste is clicked until it answers: the window shows when it can.
        paste = lambda: re.search(r"^PASTED .*$", log.read_text(), re.M)
        line = until(f"{name}: the paste", lambda: paste() or click(*PASTE), 20).group(0)
        checks[f"{name} pasted"] = line
        checks[f"{name}: another client's PNG pastes, its pixels and alpha as they were"] = line == pasted
        click(*COPY)
        line = until(f"{name}: the copy", lambda: re.search(r"^COPIED .*$", log.read_text(), re.M)).group(0)
        checks[f"{name} copied"] = line
        back = until(f"{name}: the copy read back", read)
        (args.output / f"{name}-copied.png").write_bytes(back)
        image = Image.open(io.BytesIO(back)).convert("RGBA")
        checks[f"{name} read back"] = [image.size, image.getpixel((0, 0)), image.getpixel((20, 10))]
        checks[f"{name}: the copy reaches another client pixel for pixel"] = (
            line == copied and image.size == (64, 48) and image.tobytes() == gradient().tobytes())
        stop(app)

    try:
        with tempfile.TemporaryDirectory(prefix="zgx-clipboard-image-") as private:
            os.chmod(private, 0o700)
            base = dict(os.environ, XDG_RUNTIME_DIR=private, DBUS_SESSION_BUS_ADDRESS="unix:path=/nonexistent")
            for key in ("DISPLAY", "WAYLAND_DISPLAY"):
                base.pop(key, None)

            # X11.
            xvfb = subprocess.Popen(["Xvfb", "-r", "-noreset", "-displayfd", "1", "-screen", "0", "1280x900x24",
                                     "-ac"], stdout=subprocess.PIPE, stderr=(args.output / "xvfb.log").open("w"),
                                    text=True)
            processes.append(xvfb)
            if not select.select([xvfb.stdout], [], [], 30)[0]:
                raise RuntimeError("Xvfb startup timed out")
            x11 = dict(base, DISPLAY=":" + xvfb.stdout.readline().strip())
            launch("openbox", ["openbox"], x11)
            wait_for_wm(x11)

            def x11_offer(data):
                # xclip serves it until another client takes the selection.
                offer = subprocess.Popen(["xclip", "-selection", "clipboard", "-t", "image/png", "-i"], env=x11,
                                         stdin=subprocess.PIPE, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
                offer.stdin.write(data)
                offer.stdin.close()
                processes.append(offer)
                until("xclip's offer", lambda: "image/png" in subprocess.run(
                    ["xclip", "-selection", "clipboard", "-t", "TARGETS", "-o"], env=x11, capture_output=True,
                    text=True).stdout)

            def x11_read():
                read = subprocess.run(["xclip", "-selection", "clipboard", "-t", "image/png", "-o"], env=x11,
                                      capture_output=True, timeout=10)
                return read.stdout if read.returncode == 0 and read.stdout.startswith(b"\x89PNG") else None

            def x11_click(x, y):
                found = subprocess.run(["xdotool", "search", "--onlyvisible", "--name", "^zgx clipboard image$"],
                                       env=x11, capture_output=True, text=True).stdout.split()
                if found:
                    subprocess.run(["xdotool", "mousemove", "--window", found[0], str(x), str(y), "click", "1"],
                                   env=x11, check=True)
                time.sleep(.3)

            session("x11", x11, x11_offer, x11_read, x11_click)

            # Wayland: the window at the output's top left.
            sway_env = dict(base, WLR_BACKENDS="headless", WLR_RENDERER="pixman", WLR_LIBINPUT_NO_DEVICES="1")
            config = pathlib.Path(private, "sway.conf")
            config.write_text("output * mode 1280x960\ndefault_border none\n"
                              "for_window [app_id=\".*\"] floating enable, move position 0 0\n")
            launch("sway", ["sway", "--unsupported-gpu", "-c", str(config)], sway_env)
            sockets = lambda: [p for p in pathlib.Path(private).glob("wayland-*") if p.suffix != ".lock"]
            socket = until("sway's socket", sockets, 10)[0].name
            wayland = dict(sway_env, WAYLAND_DISPLAY=socket)
            pointer = VirtualPointer(private, socket)

            def wayland_offer(data):
                # wl-copy serves it, forked, until the selection is replaced.
                subprocess.run(["wl-copy", "--type", "image/png"], env=wayland, input=data, check=True)
                until("wl-copy's offer", lambda: "image/png" in subprocess.run(
                    ["wl-paste", "--list-types"], env=wayland, capture_output=True, text=True).stdout)

            def wayland_read():
                read = subprocess.run(["wl-paste", "--no-newline", "--type", "image/png"], env=wayland,
                                      capture_output=True, timeout=10)
                return read.stdout if read.returncode == 0 and read.stdout.startswith(b"\x89PNG") else None

            def wayland_click(x, y):
                pointer.move_to(x, y, 1280, 960)
                time.sleep(.2)
                pointer.click()
                time.sleep(.3)

            session("wayland", wayland, wayland_offer, wayland_read, wayland_click)
            pointer.close()
            for process in reversed(processes):
                stop(process)
            release_mounts(private)

        (args.output / "result.json").write_text(json.dumps(checks, indent=2) + "\n")
        failed = [key for key, value in checks.items() if value is False]
        assert not failed, f"failed: {failed}\n{json.dumps(checks, indent=2)}"
        print("PASS: another client's PNG pastes with its pixels and alpha, and a copy reaches another client "
              "pixel for pixel (X11, Wayland)")
    finally:
        for process in reversed(processes):
            stop(process)


if __name__ == "__main__":
    main()
