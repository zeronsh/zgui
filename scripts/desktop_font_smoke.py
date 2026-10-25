#!/usr/bin/env python3
"""The desktop's interface font on X11: a stand-in settings portal on a
private session bus gives GNOME's `font-name`, and the gallery's text (the
theme's generic sans-serif) takes it, as GTK apps do. Run once with
"DejaVu Sans 11" and once with "DejaVu Serif 11": the window's text must
differ, the rest of it not."""
import argparse
import json
import os
import pathlib
import select
import subprocess
import time

from platform_smoke import private_runtime, stop, wait_for_wm

PORTAL = r'''
import sys
from gi.repository import Gio, GLib

XML = """<node><interface name="org.freedesktop.portal.Settings">
  <method name="ReadOne"><arg type="s" direction="in"/><arg type="s" direction="in"/><arg type="v" direction="out"/></method>
  <signal name="SettingChanged"><arg type="s"/><arg type="s"/><arg type="v"/></signal>
  <property name="version" type="u" access="read"/>
</interface></node>"""
font = GLib.Variant("s", sys.argv[1])

def call(conn, sender, path, iface, method, params, invocation):
    if tuple(params.unpack()) != ("org.gnome.desktop.interface", "font-name"):
        invocation.return_dbus_error("org.freedesktop.portal.Error.NotFound", "no such setting")
        return
    invocation.return_value(GLib.Variant("(v)", (font,)))

def acquired(conn, name):
    info = Gio.DBusNodeInfo.new_for_xml(XML).interfaces[0]
    conn.register_object("/org/freedesktop/portal/desktop", info, call,
                         lambda *_: GLib.Variant("u", 2), None)
    print("PORTAL ready", flush=True)

Gio.bus_own_name(Gio.BusType.SESSION, "org.freedesktop.portal.Desktop", Gio.BusNameOwnerFlags.NONE,
                 acquired, None, None)
GLib.MainLoop().run()
'''


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("binary", type=pathlib.Path, help="the gallery example")
    parser.add_argument("--output", type=pathlib.Path, required=True)
    args = parser.parse_args()
    args.output.mkdir(parents=True, exist_ok=True)
    from PIL import Image, ImageChops

    def shot(font):
        processes = []
        name = font.split()[1].lower()

        def launch(label, command, env, **options):
            log = (args.output / f"{name}-{label}.log").open("w")
            child = subprocess.Popen(command, env=env, stdout=options.pop("stdout", log), stderr=log, **options)
            processes.append(child)
            return child

        with private_runtime("zgx-desktop-font-") as private:
            env = dict(os.environ, XDG_RUNTIME_DIR=private, WINIT_X11_SCALE_FACTOR="1")
            for key in ("WAYLAND_DISPLAY", "DBUS_SESSION_BUS_ADDRESS"):
                env.pop(key, None)
            try:
                bus = launch("dbus", ["dbus-daemon", "--session", "--nofork", "--print-address=1"], env,
                             stdout=subprocess.PIPE, text=True)
                if not select.select([bus.stdout], [], [], 10)[0]:
                    raise RuntimeError("dbus-daemon startup timed out")
                env["DBUS_SESSION_BUS_ADDRESS"] = bus.stdout.readline().strip()
                portal = launch("portal", ["/usr/bin/python3", "-c", PORTAL, font], env,
                                stdout=subprocess.PIPE, text=True)
                if not select.select([portal.stdout], [], [], 10)[0] or "ready" not in portal.stdout.readline():
                    raise RuntimeError("the stand-in portal did not start")
                xvfb = launch("xvfb", ["Xvfb", "-displayfd", "1", "-screen", "0", "1280x900x24", "-ac"], env,
                              stdout=subprocess.PIPE, text=True)
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
                        raise RuntimeError("the gallery's window did not appear")
                    time.sleep(.05)
                time.sleep(1.5)
                path = args.output / f"{name}.png"
                subprocess.check_call(["import", "-window", window, str(path)], env=env)
                return Image.open(path).convert("RGB")
            finally:
                for process in reversed(processes):
                    stop(process)

    sans, serif = shot("DejaVu Sans 11"), shot("DejaVu Serif 11")
    assert sans.size == serif.size, (sans.size, serif.size)
    changed = ImageChops.difference(sans, serif).getbbox()
    pixels = sum(1 for p in ImageChops.difference(sans, serif).getdata() if p != (0, 0, 0))
    result = {"result": "pass" if changed else "fail", "changed_box": changed, "changed_pixels": pixels}
    (args.output / "result.json").write_text(json.dumps(result, indent=2) + "\n")
    assert changed, "the portal's font-name changed nothing"
    print(f"PASS: the desktop's font reaches sans-serif text ({pixels} pixels differ)")


if __name__ == "__main__":
    main()
