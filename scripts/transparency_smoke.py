#!/usr/bin/env python3
"""Verify native transparent surfaces through an owned X11 compositor."""
import argparse
import ctypes as C
import ctypes.util
import json
import os
from pathlib import Path
import re
import select
import subprocess
import tempfile
import time
from PIL import ImageGrab
from platform_smoke import stop


class Backdrop:
    """Publish a live root pixmap that picom can composite behind ARGB clients."""
    def __init__(self, display):
        self.lib = C.CDLL(ctypes.util.find_library("X11"))
        signatures = {
            "XOpenDisplay": (C.c_void_p, [C.c_char_p]),
            "XDefaultRootWindow": (C.c_ulong, [C.c_void_p]),
            "XDefaultScreen": (C.c_int, [C.c_void_p]),
            "XDefaultDepth": (C.c_int, [C.c_void_p, C.c_int]),
            "XCreatePixmap": (C.c_ulong, [C.c_void_p, C.c_ulong, C.c_uint, C.c_uint, C.c_uint]),
            "XCreateGC": (C.c_void_p, [C.c_void_p, C.c_ulong, C.c_ulong, C.c_void_p]),
            "XSetForeground": (C.c_int, [C.c_void_p, C.c_void_p, C.c_ulong]),
            "XFillRectangle": (C.c_int, [C.c_void_p, C.c_ulong, C.c_void_p, C.c_int, C.c_int, C.c_uint, C.c_uint]),
            "XInternAtom": (C.c_ulong, [C.c_void_p, C.c_char_p, C.c_int]),
            "XChangeProperty": (C.c_int, [C.c_void_p, C.c_ulong, C.c_ulong, C.c_ulong, C.c_int, C.c_int, C.c_void_p, C.c_int]),
            "XSetWindowBackgroundPixmap": (C.c_int, [C.c_void_p, C.c_ulong, C.c_ulong]),
            "XClearWindow": (C.c_int, [C.c_void_p, C.c_ulong]),
            "XSync": (C.c_int, [C.c_void_p, C.c_int]),
            "XCloseDisplay": (C.c_int, [C.c_void_p]),
        }
        for name, (result, arguments) in signatures.items():
            function = getattr(self.lib, name)
            function.restype, function.argtypes = result, arguments
        self.display = self.lib.XOpenDisplay(display.encode())
        assert self.display, "background display connection failed"
        self.root = self.lib.XDefaultRootWindow(self.display)
        depth = self.lib.XDefaultDepth(self.display, self.lib.XDefaultScreen(self.display))
        self.pixmap = self.lib.XCreatePixmap(self.display, self.root, 900, 700, depth)
        self.gc = self.lib.XCreateGC(self.display, self.pixmap, 0, None)

    def set(self, rgb):
        lib, display = self.lib, self.display
        lib.XSetForeground(display, self.gc, rgb)
        lib.XFillRectangle(display, self.pixmap, self.gc, 0, 0, 900, 700)
        value = C.c_ulong(self.pixmap)
        for name in [b"_XROOTPMAP_ID", b"ESETROOT_PMAP_ID"]:
            atom = lib.XInternAtom(display, name, 0)
            lib.XChangeProperty(display, self.root, atom, 20, 32, 0, C.byref(value), 1)
        lib.XSetWindowBackgroundPixmap(display, self.root, self.pixmap)
        lib.XClearWindow(display, self.root)
        lib.XSync(display, 0)

    def close(self):
        self.lib.XCloseDisplay(self.display)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("binary", type=Path)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    args.output.mkdir(parents=True, exist_ok=True)
    (args.output / "results.json").unlink(missing_ok=True)
    backdrop = None
    processes = []
    logs = []
    with tempfile.TemporaryDirectory(prefix="zgx-transparency-") as runtime:
        def launch(name, command, **kwargs):
            log = (args.output / (name + ".log")).open("w")
            logs.append(log)
            process = subprocess.Popen(command, stderr=log, stdout=kwargs.pop("stdout", log), **kwargs)
            processes.append(process)
            return process
        try:
            xvfb = launch("xvfb", ["Xvfb", "-r", "-displayfd", "1", "-screen", "0", "900x700x24", "-ac"], stdout=subprocess.PIPE, text=True)
            assert select.select([xvfb.stdout], [], [], 10)[0], "Xvfb startup timed out"
            display = ":" + xvfb.stdout.readline().strip()
            env = dict(os.environ, DISPLAY=display, XDG_RUNTIME_DIR=runtime,
                       WINIT_X11_SCALE_FACTOR="1", DBUS_SESSION_BUS_ADDRESS="unix:path=/nonexistent",
                       GSETTINGS_BACKEND="memory", XDG_CONFIG_HOME=runtime + "/config")
            for key in ["WAYLAND_DISPLAY", "AT_SPI_BUS_ADDRESS", "SWAYSOCK", "I3SOCK"]:
                env.pop(key, None)
            launch("openbox", ["openbox"], env=env)
            time.sleep(.4)
            backdrop = Backdrop(display)
            backdrop.set(0x204060)
            config = Path(runtime) / "picom.conf"
            config.write_text('backend = "xrender"; shadow = false; fading = false; unredir-if-possible = false;\n')
            compositor = launch("picom", ["picom", "--config", str(config)], env=env)
            time.sleep(.5)
            assert compositor.poll() is None, "picom exited"
            app = launch("application", [str(args.binary.resolve()), "--smoke-test"], env=env)
            def xdo(*parts):
                return subprocess.check_output(["xdotool", *map(str, parts)], env=env, text=True).strip()
            deadline = time.monotonic() + 20
            window = None
            while time.monotonic() < deadline and app.poll() is None:
                found = subprocess.run(["xdotool", "search", "--onlyvisible", "--pid", str(app.pid)], env=env, capture_output=True, text=True)
                if found.returncode == 0 and found.stdout.strip():
                    window = found.stdout.splitlines()[0]
                    break
                time.sleep(.05)
            assert window, "transparent window did not open"
            xdo("windowactivate", "--sync", window)
            time.sleep(1)
            stages = []
            def sample(name, checks):
                deadline = time.monotonic() + 8
                while True:
                    info = subprocess.check_output(["xwininfo", "-id", window], env=env, text=True)
                    x = int(re.search(r"Absolute upper-left X:\s+(-?\d+)", info)[1])
                    y = int(re.search(r"Absolute upper-left Y:\s+(-?\d+)", info)[1])
                    picture = ImageGrab.grab(xdisplay=display).convert("RGB")
                    actual = [picture.getpixel((x + px, y + py)) for px, py, _ in checks]
                    if all(all(abs(a-b) <= 2 for a,b in zip(pixel, expected)) for pixel, (_,_,expected) in zip(actual,checks)):
                        break
                    if time.monotonic() >= deadline:
                        picture.save(args.output / (name + "-failed.png"))
                        raise AssertionError((name, actual, checks))
                    time.sleep(.1)
                picture.save(args.output / (name + ".png"))
                stages.append(dict(stage=name, client_origin=[x,y], samples=[dict(position=[px,py], expected=expected, actual=pixel) for (px,py,expected),pixel in zip(checks,actual)]))
            # Geometry and stage keys correspond to examples/transparency.rs.
            background = (32,64,96)
            def blend(color, behind):
                return tuple(round(color[i] * color[3] / 255 + behind[i] * (1 - color[3] / 255)) for i in range(3))
            red = (224,64,32,128)
            blue = (32,96,224,128)
            green = (64,224,128,64)
            def checks(color, backdrop, visible=True):
                a = blend(color, backdrop) if visible else backdrop
                return [(40,40,a), (120,80,blend(green,a)),
                        (180,120,blend(green,backdrop)), (280,40,backdrop), (25,165,(238,238,238))]
            sample("initial", checks(red, background))
            xdo("key", "c")
            sample("color-update", checks(blue, background))
            xdo("key", "v")
            sample("hidden-panel", checks(blue, background, False))
            xdo("windowmove", window, 100, 120)
            xdo("windowsize", window, 400, 300)
            sample("moved-resized", checks(blue, background, False) + [(380,280,background)])
            xdo("key", "v")
            sample("restored-panel", checks(blue, background))
            background = (112,48,16)
            backdrop.set(0x703010)
            sample("backdrop-change", checks(blue, background) + [(380,280,background)])
            xdo("key", "Escape")
            app.wait(timeout=10)
            assert app.returncode == 0
            assert compositor.poll() is None
            (args.output / "results.json").write_text(json.dumps(dict(passed=True, backend="owned Xvfb/Openbox/picom xrender", stages=stages), indent=2) + "\n")
        finally:
            if backdrop is not None:
                backdrop.close()
            for process in reversed(processes):
                stop(process)
            for log in logs:
                log.close()


if __name__ == "__main__":
    main()
