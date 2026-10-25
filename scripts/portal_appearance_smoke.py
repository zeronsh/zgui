#!/usr/bin/env python3
"""The desktop's appearance through the XDG settings portal, on X11.

A stand-in portal on a private session bus answers
org.freedesktop.portal.Settings for org.freedesktop.appearance, as GNOME and
KDE's portals do. The primary_selection example (which follows the system's
appearance) must start dark with a red accent on its selected text, then
follow the portal live: a blue accent, light, high contrast in light
(white), then in dark (black).
"""
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
import gi
from gi.repository import Gio, GLib

XML = """<node><interface name="org.freedesktop.portal.Settings">
  <method name="ReadOne"><arg type="s" direction="in"/><arg type="s" direction="in"/><arg type="v" direction="out"/></method>
  <method name="Read"><arg type="s" direction="in"/><arg type="s" direction="in"/><arg type="v" direction="out"/></method>
  <signal name="SettingChanged"><arg type="s"/><arg type="s"/><arg type="v"/></signal>
  <property name="version" type="u" access="read"/>
</interface></node>"""
settings = {
    "color-scheme": GLib.Variant("u", 1),
    "accent-color": GLib.Variant("(ddd)", (1.0, 0.0, 0.0)),
    "contrast": GLib.Variant("u", 0),
}
connection = None

def call(conn, sender, path, iface, method, params, invocation):
    namespace, key = params.unpack()
    if namespace != "org.freedesktop.appearance" or key not in settings:
        invocation.return_dbus_error("org.freedesktop.portal.Error.NotFound", "no such setting")
        return
    value = settings[key]
    if method == "Read":
        value = GLib.Variant("v", value)
    invocation.return_value(GLib.Variant("(v)", (value,)))

def prop(conn, sender, path, iface, name):
    return GLib.Variant("u", 2)

def acquired(conn, name):
    global connection
    connection = conn
    info = Gio.DBusNodeInfo.new_for_xml(XML).interfaces[0]
    conn.register_object("/org/freedesktop/portal/desktop", info, call, prop, None)
    print("PORTAL ready", flush=True)

def command(source, condition):
    # "key signature value", as GLib.Variant.parse reads it.
    line = sys.stdin.readline()
    if not line:
        loop.quit()
        return False
    key, signature, text = line.strip().split(" ", 2)
    settings[key] = GLib.Variant.parse(GLib.VariantType(signature), text, None, None)
    connection.emit_signal(None, "/org/freedesktop/portal/desktop", "org.freedesktop.portal.Settings",
                           "SettingChanged", GLib.Variant("(ssv)", ("org.freedesktop.appearance", key, settings[key])))
    print("PORTAL changed", flush=True)
    return True

Gio.bus_own_name(Gio.BusType.SESSION, "org.freedesktop.portal.Desktop", Gio.BusNameOwnerFlags.NONE,
                 acquired, None, None)
GLib.io_add_watch(sys.stdin, GLib.IO_IN, command)
loop = GLib.MainLoop()
loop.run()
'''


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
                                 stderr=log, **options)
        processes.append(child)
        return child

    with private_runtime("zgx-portal-appearance-") as private:
        env = dict(os.environ, XDG_RUNTIME_DIR=private, WINIT_X11_SCALE_FACTOR="1")
        for key in ("WAYLAND_DISPLAY", "DBUS_SESSION_BUS_ADDRESS"):
            env.pop(key, None)
        try:
            bus = launch("dbus", ["dbus-daemon", "--session", "--nofork", "--print-address=1"], env,
                         stdout=subprocess.PIPE, text=True)
            if not select.select([bus.stdout], [], [], 10)[0]:
                raise RuntimeError("dbus-daemon startup timed out")
            env["DBUS_SESSION_BUS_ADDRESS"] = bus.stdout.readline().strip()
            portal = launch("portal", ["/usr/bin/python3", "-c", PORTAL], env,
                            stdout=subprocess.PIPE, stdin=subprocess.PIPE, text=True)
            if not select.select([portal.stdout], [], [], 10)[0] or "ready" not in portal.stdout.readline():
                raise RuntimeError("the stand-in portal did not start; see portal.log")
            xvfb = launch("xvfb", ["Xvfb", "-r", "-noreset", "-displayfd", "1", "-screen", "0", "1280x900x24", "-ac"], env,
                          stdout=subprocess.PIPE, text=True)
            if not select.select([xvfb.stdout], [], [], 10)[0]:
                raise RuntimeError("Xvfb startup timed out")
            env["DISPLAY"] = ":" + xvfb.stdout.readline().strip()
            launch("openbox", ["openbox"], env)
            wait_for_wm(env)
            app = launch("application", [str(args.binary.resolve()), "--smoke-test"], env)
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

            def xdo(*parts):
                subprocess.check_call(["xdotool", *map(str, parts)], env=env)

            xdo("windowactivate", "--sync", window)
            time.sleep(.4)
            xdo("mousemove", "--window", window, 110, 42, "click", 1)
            time.sleep(.2)
            xdo("key", "--clearmodifiers", "ctrl+a")

            from PIL import Image

            def sample(stage):
                # The selected field's average colour (behind its glyphs),
                # and the window's background below the editors.
                path = args.output / f"{stage}.png"
                subprocess.run(["import", "-window", window, str(path)], env=env, check=True, timeout=10)
                with Image.open(path) as picture:
                    picture = picture.convert("RGB")
                    pixels = list(picture.crop((34, 30, 180, 54)).getdata())
                    background = picture.getpixel((8, 248))
                return tuple(sum(p[i] for p in pixels) / len(pixels) for i in range(3)), background

            def wait(stage, accepts):
                deadline = time.monotonic() + 10
                while not accepts(*(seen := sample(stage))):
                    if time.monotonic() > deadline:
                        raise AssertionError(f"{stage}: selection and background are {seen}")
                    time.sleep(.2)
                return seen

            def portal_sets(key, signature, value):
                portal.stdin.write(f"{key} {signature} {value}\n")
                portal.stdin.flush()

            seen = {"dark, red accent": wait("dark-red", lambda tint, bg: tint[0] > tint[2] + 20 and sum(bg) < 200)}
            portal_sets("accent-color", "(ddd)", "(0, 0.3, 1)")
            seen["blue accent"] = wait("blue", lambda tint, bg: tint[2] > tint[0] + 20)
            portal_sets("color-scheme", "u", "2")
            seen["light"] = wait("light", lambda tint, bg: sum(bg) > 600)
            portal_sets("contrast", "u", "1")
            seen["high contrast, light"] = wait("contrast-light", lambda tint, bg: bg == (255, 255, 255))
            portal_sets("color-scheme", "u", "1")
            seen["high contrast, dark"] = wait("contrast-dark", lambda tint, bg: bg == (0, 0, 0))
            result = {"result": "pass", "platform": "X11 / Xvfb / stand-in settings portal",
                      "samples": {stage: {"selection": tint, "background": bg} for stage, (tint, bg) in seen.items()},
                      "checks": ["dark and accent read at start", "accent followed live", "light followed live",
                                 "high contrast in light and dark"]}
            (args.output / "result.json").write_text(json.dumps(result, indent=2) + "\n")
            print("PASS: " + ", ".join(result["checks"]))
        finally:
            for process in reversed(processes):
                stop(process)


if __name__ == "__main__":
    main()
