#!/usr/bin/env python3
"""GNOME's `overlay-scrolling` on X11, from a stand-in settings portal on a
private session bus, as GTK follows it: on, the scrollbars example's
vertical scrollbar is hidden at rest, shows as its view is scrolled with the
wheel, and hides again once scrolling stops; off, it shows at rest."""
import argparse
import json
import os
import pathlib
import re
import select
import subprocess
import time

from PIL import ImageGrab

from platform_smoke import private_runtime, stop, wait_for_wm

PORTAL = r'''
import sys
from gi.repository import Gio, GLib

XML = """<node><interface name="org.freedesktop.portal.Settings">
  <method name="ReadOne"><arg type="s" direction="in"/><arg type="s" direction="in"/><arg type="v" direction="out"/></method>
  <signal name="SettingChanged"><arg type="s"/><arg type="s"/><arg type="v"/></signal>
  <property name="version" type="u" access="read"/>
</interface></node>"""
value = GLib.Variant("b", sys.argv[1] == "on")

def call(conn, sender, path, iface, method, params, invocation):
    if tuple(params.unpack()) != ("org.gnome.desktop.interface", "overlay-scrolling"):
        invocation.return_dbus_error("org.freedesktop.portal.Error.NotFound", "no such setting")
        return
    invocation.return_value(GLib.Variant("(v)", (value,)))

def acquired(conn, name):
    info = Gio.DBusNodeInfo.new_for_xml(XML).interfaces[0]
    conn.register_object("/org/freedesktop/portal/desktop", info, call,
                         lambda *_: GLib.Variant("u", 2), None)
    print("PORTAL ready", flush=True)

Gio.bus_own_name(Gio.BusType.SESSION, "org.freedesktop.portal.Desktop", Gio.BusNameOwnerFlags.NONE,
                 acquired, None, None)
GLib.MainLoop().run()
'''
# The vertical scrollbar's thumb, in scrollbars_smoke's colours and place.
THUMB = ((94, 165, 255), (229, 237, 247))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("binary", type=pathlib.Path, help="the scrollbars example")
    parser.add_argument("--output", type=pathlib.Path, required=True)
    args = parser.parse_args()
    args.output.mkdir(parents=True, exist_ok=True)

    def session(setting):
        """The vertical thumb's pixels at rest, as the view scrolls, and
        after, under `overlay-scrolling` `setting`."""
        processes = []

        def launch(label, command, env, **options):
            log = (args.output / f"{setting}-{label}.log").open("w")
            child = subprocess.Popen(command, env=env, stdout=options.pop("stdout", log), stderr=log, **options)
            processes.append(child)
            return child

        with private_runtime("zgx-overlay-scrolling-") as private:
            env = dict(os.environ, XDG_RUNTIME_DIR=private, WINIT_X11_SCALE_FACTOR="1")
            for key in ("WAYLAND_DISPLAY", "DBUS_SESSION_BUS_ADDRESS"):
                env.pop(key, None)
            try:
                bus = launch("dbus", ["dbus-daemon", "--session", "--nofork", "--print-address=1"], env,
                             stdout=subprocess.PIPE, text=True)
                if not select.select([bus.stdout], [], [], 10)[0]:
                    raise RuntimeError("dbus-daemon startup timed out")
                env["DBUS_SESSION_BUS_ADDRESS"] = bus.stdout.readline().strip()
                portal = launch("portal", ["/usr/bin/python3", "-c", PORTAL, setting], env,
                                stdout=subprocess.PIPE, text=True)
                if not select.select([portal.stdout], [], [], 10)[0] or "ready" not in portal.stdout.readline():
                    raise RuntimeError("the stand-in portal did not start")
                xvfb = launch("xvfb", ["Xvfb", "-r", "-noreset", "-displayfd", "1", "-screen", "0", "1024x768x24",
                                       "-ac"], env, stdout=subprocess.PIPE, text=True)
                if not select.select([xvfb.stdout], [], [], 10)[0]:
                    raise RuntimeError("Xvfb startup timed out")
                env["DISPLAY"] = ":" + xvfb.stdout.readline().strip()
                launch("openbox", ["openbox"], env)
                wait_for_wm(env)
                app = launch("application", [str(args.binary.resolve())], env)
                deadline = time.monotonic() + 20
                while True:
                    found = subprocess.run(["xdotool", "search", "--onlyvisible", "--pid", str(app.pid)],
                                           env=env, capture_output=True, text=True)
                    if found.returncode == 0 and found.stdout.strip():
                        window = found.stdout.split()[0]
                        break
                    if time.monotonic() > deadline or app.poll() is not None:
                        raise RuntimeError("the example's window did not appear")
                    time.sleep(.05)
                xdo = lambda *parts: subprocess.check_call(["xdotool", *map(str, parts)], env=env)
                xdo("windowactivate", "--sync", window)
                info = subprocess.check_output(["xwininfo", "-id", window], env=env, text=True)
                x = int(re.search(r"Absolute upper-left X:\s+(-?\d+)", info).group(1))
                y = int(re.search(r"Absolute upper-left Y:\s+(-?\d+)", info).group(1))

                def thumb(stage):
                    shot = ImageGrab.grab(xdisplay=env["DISPLAY"])
                    shot.save(args.output / f"{setting}-{stage}.png")
                    return sum(shot.getpixel((x + 212, y + row))[:3] in THUMB for row in range(76, 220))

                # The pointer off the scrollbars, which a hovered one keeps.
                xdo("mousemove", "--window", window, 400, 300)
                time.sleep(1)
                rest = thumb("rest")
                xdo("mousemove", "--window", window, 100, 150)
                for _ in range(3):
                    xdo("click", 5)
                time.sleep(.2)
                scrolled = thumb("scrolled")
                xdo("mousemove", "--window", window, 400, 300)
                # A second after it stops, then a fade.
                time.sleep(2.5)
                after = thumb("after")
                assert app.poll() is None, "the example exited"
                return {"rest": rest, "scrolled": scrolled, "after": after}
            finally:
                for process in reversed(processes):
                    stop(process)

    on = session("on")
    off = session("off")
    result = {"overlay-scrolling on": on, "off": off}
    (args.output / "result.json").write_text(json.dumps(result, indent=2) + "\n")
    assert on["rest"] == 0 and on["scrolled"] > 20 and on["after"] == 0, result
    assert off["rest"] > 20, result
    print(f"PASS: with overlay-scrolling on the scrollbar hid at rest, showed scrolled ({on['scrolled']} thumb "
          f"pixels) and hid after; off, it showed at rest ({off['rest']})")


if __name__ == "__main__":
    main()
