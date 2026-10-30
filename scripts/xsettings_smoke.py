#!/usr/bin/env python3
"""The session's XSETTINGS on X11, as GTK takes them there: xsettingsd
publishes `Net/DndDragThreshold` 30 and `Net/DoubleClickTime` 1000 while a
stand-in settings portal answers GNOME's 8 and 300 (XFCE with the GTK portal
installed: the portal's are GNOME's defaults, not the person's). The
drag_drop example's card takes a 20-pixel move as no drag and a 40-pixel one
as a drag; changed to 10 and reloaded, the setting holds once the window is
activated again, and 20 pixels drags. The primary_selection example's
editor takes two presses 700 ms apart as a double click (a word selected,
the PRIMARY selection). With the portal's `color-scheme` no preference
(XFCE's), the GTK theme decides, as it does GTK's apps: under `Net/ThemeName`
"Greybird" that example is light, and changed to "Greybird-dark", dark once
its window is activated again."""
import argparse
import json
import os
import pathlib
import select
import signal
import subprocess
import time

from platform_smoke import private_runtime, stop, wait_for_wm

PORTAL = r'''
from gi.repository import Gio, GLib

XML = """<node><interface name="org.freedesktop.portal.Settings">
  <method name="ReadOne"><arg type="s" direction="in"/><arg type="s" direction="in"/><arg type="v" direction="out"/></method>
  <signal name="SettingChanged"><arg type="s"/><arg type="s"/><arg type="v"/></signal>
  <property name="version" type="u" access="read"/>
</interface></node>"""
settings = {
    ("org.gnome.desktop.peripherals.mouse", "drag-threshold"): GLib.Variant("i", 8),
    ("org.gnome.desktop.peripherals.mouse", "double-click"): GLib.Variant("i", 300),
    ("org.freedesktop.appearance", "color-scheme"): GLib.Variant("u", 0),
}

def call(conn, sender, path, iface, method, params, invocation):
    value = settings.get(tuple(params.unpack()))
    if value is None:
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


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("drag_drop", type=pathlib.Path, help="the drag_drop example")
    parser.add_argument("primary_selection", type=pathlib.Path, help="the primary_selection example")
    parser.add_argument("--output", type=pathlib.Path, required=True)
    args = parser.parse_args()
    args.output.mkdir(parents=True, exist_ok=True)
    processes = []
    result = {}

    def launch(label, command, env, **options):
        log = (args.output / f"{label}.log").open("w")
        child = subprocess.Popen(command, env=env, stdout=options.pop("stdout", log), stderr=log, **options)
        processes.append(child)
        return child

    def until(what, check, seconds=20):
        deadline = time.monotonic() + seconds
        while time.monotonic() < deadline:
            found = check()
            if found:
                return found
            time.sleep(.05)
        raise RuntimeError(f"timed out waiting for {what}")

    with private_runtime("zgx-xsettings-") as private:
        env = dict(os.environ, XDG_RUNTIME_DIR=private, WINIT_X11_SCALE_FACTOR="1")
        for key in ("WAYLAND_DISPLAY", "DBUS_SESSION_BUS_ADDRESS"):
            env.pop(key, None)
        try:
            bus = launch("dbus", ["dbus-daemon", "--session", "--nofork", "--print-address=1"], env,
                         stdout=subprocess.PIPE, text=True)
            if not select.select([bus.stdout], [], [], 10)[0]:
                raise RuntimeError("dbus-daemon startup timed out")
            env["DBUS_SESSION_BUS_ADDRESS"] = bus.stdout.readline().strip()
            portal = launch("portal", ["/usr/bin/python3", "-c", PORTAL], env, stdout=subprocess.PIPE, text=True)
            if not select.select([portal.stdout], [], [], 10)[0] or "ready" not in portal.stdout.readline():
                raise RuntimeError("the stand-in portal did not start")
            xvfb = launch("xvfb", ["Xvfb", "-r", "-noreset", "-displayfd", "1", "-screen", "0", "1280x900x24", "-ac"],
                          env, stdout=subprocess.PIPE, text=True)
            if not select.select([xvfb.stdout], [], [], 30)[0]:
                raise RuntimeError("Xvfb startup timed out")
            env["DISPLAY"] = ":" + xvfb.stdout.readline().strip()
            launch("openbox", ["openbox"], env)
            wait_for_wm(env)
            config = args.output / "xsettingsd.conf"
            config.write_text("Net/DndDragThreshold 30\nNet/DoubleClickTime 1000\nNet/ThemeName \"Greybird\"\n")
            daemon = launch("xsettingsd", ["xsettingsd", "-c", str(config)], env)
            daemon_log = args.output / "xsettingsd.log"
            until("xsettingsd's selection", lambda: "Took ownership" in daemon_log.read_text())
            xdo = lambda *parts: subprocess.check_call(["xdotool", *map(str, parts)], env=env)

            def window_of(app):
                def visible():
                    found = subprocess.run(["xdotool", "search", "--onlyvisible", "--pid", str(app.pid)],
                                           env=env, capture_output=True, text=True)
                    if app.poll() is not None:
                        raise RuntimeError("the example exited")
                    return found.stdout.split()[0] if found.returncode == 0 and found.stdout.strip() else None
                return until("the example's window", visible)

            # The drag threshold.
            log = args.output / "drag_drop.log"
            app = launch("drag_drop", [str(args.drag_drop.resolve())], env, stdout=log.open("w"))
            window = window_of(app)
            xdo("windowactivate", "--sync", window)
            time.sleep(.4)

            def drags(distance):
                before = log.read_text().count("DRAG_END Red")
                xdo("mousemove", "--window", window, 60, 110, "mousedown", 1)
                for step in range(1, distance + 1):
                    xdo("mousemove", "--window", window, 60 + step, 110)
                time.sleep(.2)
                xdo("mouseup", 1)
                time.sleep(.6)
                return log.read_text().count("DRAG_END Red") > before

            result["at 30, 20 pixels drags"] = drags(20)
            result["at 30, 40 pixels drags"] = drags(40)
            # Changed in the settings app: it holds once the window is
            # activated again.
            config.write_text("Net/DndDragThreshold 10\nNet/DoubleClickTime 1000\nNet/ThemeName \"Greybird\"\n")
            daemon.send_signal(signal.SIGHUP)
            until("xsettingsd's reload", lambda: daemon_log.read_text().count("Loaded 3 settings") >= 2)
            other = launch("xmessage", ["xmessage", "-geometry", "+600+400", "elsewhere"], env)
            # (xmessage names no pid on its window: found by its class.)
            elsewhere = until("xmessage's window", lambda: subprocess.run(
                ["xdotool", "search", "--onlyvisible", "--class", "xmessage"], env=env,
                capture_output=True, text=True).stdout.split()[:1])[0]
            xdo("windowactivate", "--sync", elsewhere)
            time.sleep(.3)
            xdo("windowactivate", "--sync", window)
            time.sleep(.4)
            result["changed to 10, 20 pixels drags"] = drags(20)
            stop(other)
            stop(app)

            # The double-click time.
            app = launch("primary_selection", [str(args.primary_selection.resolve()), "--smoke-test"],
                         env)
            window = window_of(app)
            xdo("windowactivate", "--sync", window)
            time.sleep(.4)
            # The window's background below the editors, as
            # portal_appearance_smoke samples it.
            from PIL import Image

            def background(stage):
                shot = args.output / f"{stage}.png"
                subprocess.run(["import", "-window", window, str(shot)], env=env, check=True, timeout=10)
                with Image.open(shot) as picture:
                    return list(picture.convert("RGB").getpixel((8, 248)))
            result["under Greybird, the background"] = background("light")
            # The same spot as primary_selection_smoke's double click.
            # One xdotool for both presses: two take as long to start as a
            # loaded runner likes, past the 1000 ms this needs them within.
            xdo("mousemove", "--window", window, 110, 42, "click", 1, "sleep", .7, "click", 1)

            def primary():
                read = subprocess.run(["xclip", "-o", "-selection", "primary"], env=env,
                                      capture_output=True, text=True, timeout=5)
                return read.stdout if read.returncode == 0 and read.stdout in ("select", "these", "words") else None
            try:
                result["two presses 700 ms apart select"] = until("a selected word", primary, seconds=2)
            except RuntimeError:
                result["two presses 700 ms apart select"] = None
            # A dark theme chosen meanwhile: dark once activated again.
            config.write_text("Net/DndDragThreshold 10\nNet/DoubleClickTime 1000\nNet/ThemeName \"Greybird-dark\"\n")
            daemon.send_signal(signal.SIGHUP)
            until("xsettingsd's second reload", lambda: daemon_log.read_text().count("Loaded 3 settings") >= 3)
            other = launch("xmessage-2", ["xmessage", "-geometry", "+600+400", "elsewhere"], env)
            elsewhere = until("xmessage's window", lambda: subprocess.run(
                ["xdotool", "search", "--onlyvisible", "--class", "xmessage"], env=env,
                capture_output=True, text=True).stdout.split()[:1])[0]
            xdo("windowactivate", "--sync", elsewhere)
            time.sleep(.3)
            xdo("windowactivate", "--sync", window)
            time.sleep(.6)
            result["changed to Greybird-dark, the background"] = background("dark")
        finally:
            for process in reversed(processes):
                stop(process)
    (args.output / "result.json").write_text(json.dumps(result, indent=2) + "\n")
    assert result == {
        "at 30, 20 pixels drags": False,
        "at 30, 40 pixels drags": True,
        "changed to 10, 20 pixels drags": True,
        "two presses 700 ms apart select": result["two presses 700 ms apart select"],
        "under Greybird, the background": result["under Greybird, the background"],
        "changed to Greybird-dark, the background": result["changed to Greybird-dark, the background"],
    } and result["two presses 700 ms apart select"], result
    assert sum(result["under Greybird, the background"]) > 600, result
    assert sum(result["changed to Greybird-dark, the background"]) < 200, result
    print("PASS: XSETTINGS' drag threshold (30, then 10 once reloaded and the window activated again) and "
          "double-click time (1000 ms) held over the portal's 8 and 300, and the GTK theme made the app light, then dark")


if __name__ == "__main__":
    main()
