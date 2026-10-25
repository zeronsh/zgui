#!/usr/bin/env python3
"""An undecorated window's title bar on X11 (Xvfb/Openbox): a
`Titlebar::Inset` window has no frame of the window manager's; the window
buttons zgx draws maximize and restore it, minimize it and close it, a
press on its corner resizes it, and its drag strip maximizes and restores it
with a double click and moves it with a drag (through the window manager,
as a GTK header bar's window does).

Then the buttons follow the desktop: a stand-in settings portal on a private
session bus gives GNOME's `button-layout` as `close,minimize,maximize:` (all
three on the left, maximize third), then changes it live to `appmenu:close`
(close alone, on the right)."""
import argparse
import json
import os
import pathlib
import re
import select
import subprocess
import time

from platform_smoke import private_runtime, stop, wait_for_wm

# libadwaita's buttons as window_chrome.rs draws them: 24-point circles, 8
# apart and 8 from the window's edge, centred in the 40-point strip.
BUTTON, GAP, MARGIN, STRIP = 24, 8, 8, 40

PORTAL = r'''
import sys
from gi.repository import Gio, GLib

XML = """<node><interface name="org.freedesktop.portal.Settings">
  <method name="ReadOne"><arg type="s" direction="in"/><arg type="s" direction="in"/><arg type="v" direction="out"/></method>
  <signal name="SettingChanged"><arg type="s"/><arg type="s"/><arg type="v"/></signal>
  <property name="version" type="u" access="read"/>
</interface></node>"""
NAMESPACE, KEY = "org.gnome.desktop.wm.preferences", "button-layout"
layout = GLib.Variant("s", sys.argv[1])
connection = None

def call(conn, sender, path, iface, method, params, invocation):
    if tuple(params.unpack()) != (NAMESPACE, KEY):
        invocation.return_dbus_error("org.freedesktop.portal.Error.NotFound", "no such setting")
        return
    invocation.return_value(GLib.Variant("(v)", (layout,)))

def acquired(conn, name):
    global connection
    connection = conn
    info = Gio.DBusNodeInfo.new_for_xml(XML).interfaces[0]
    conn.register_object("/org/freedesktop/portal/desktop", info, call,
                         lambda *_: GLib.Variant("u", 2), None)
    print("PORTAL ready", flush=True)

def command(source, condition):
    global layout
    line = sys.stdin.readline()
    if not line:
        loop.quit()
        return False
    layout = GLib.Variant("s", line.strip())
    connection.emit_signal(None, "/org/freedesktop/portal/desktop", "org.freedesktop.portal.Settings",
                           "SettingChanged", GLib.Variant("(ssv)", (NAMESPACE, KEY, layout)))
    return True

Gio.bus_own_name(Gio.BusType.SESSION, "org.freedesktop.portal.Desktop", Gio.BusNameOwnerFlags.NONE,
                 acquired, None, None)
GLib.io_add_watch(sys.stdin, GLib.IO_IN, command)
loop = GLib.MainLoop()
loop.run()
'''


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("binary", type=pathlib.Path, help="the glass_counter example")
    parser.add_argument("--output", type=pathlib.Path, required=True)
    args = parser.parse_args()
    args.output.mkdir(parents=True, exist_ok=True)
    processes = []
    checks = {}

    def launch(name, command, env, **options):
        log = (args.output / (name + ".log")).open("w")
        child = subprocess.Popen(command, env=env, stdout=options.pop("stdout", log), stderr=log, **options)
        processes.append(child)
        return child

    with private_runtime("zgx-window-buttons-") as private:
        env = dict(os.environ, XDG_RUNTIME_DIR=private, WINIT_X11_SCALE_FACTOR="1",
                   DBUS_SESSION_BUS_ADDRESS="unix:path=/nonexistent")
        env.pop("WAYLAND_DISPLAY", None)
        try:
            xvfb = launch("xvfb", ["Xvfb", "-r", "-noreset", "-displayfd", "1", "-screen", "0", "1280x900x24", "-ac"], env,
                          stdout=subprocess.PIPE, text=True)
            if not select.select([xvfb.stdout], [], [], 10)[0]:
                raise RuntimeError("Xvfb startup timed out")
            env["DISPLAY"] = ":" + xvfb.stdout.readline().strip()
            launch("openbox", ["openbox"], env)
            wait_for_wm(env)

            def run(*argv):
                return subprocess.check_output(list(map(str, argv)), env=env, text=True).strip()

            def open_window(name):
                app = launch(name, [str(args.binary.resolve())], env)

                def until(what, test, seconds=10):
                    deadline = time.monotonic() + seconds
                    while True:
                        value = test()
                        if value:
                            return value
                        if time.monotonic() > deadline or app.poll() is not None:
                            raise AssertionError(f"timed out waiting for {what}: {checks}")
                        time.sleep(.05)

                def find():
                    found = subprocess.run(["xdotool", "search", "--onlyvisible", "--pid", str(app.pid)],
                                           env=env, capture_output=True, text=True)
                    return found.stdout.split()[0] if found.returncode == 0 and found.stdout.strip() else None

                window = until("the window", find)
                run("xdotool", "windowactivate", "--sync", window)
                return app, window, until

            def geometry(window):
                info = run("xwininfo", "-id", window)
                value = lambda name: int(re.search(name + r":\s+(-?\d+)", info).group(1))
                return value("Absolute upper-left X"), value("Absolute upper-left Y"), value("Width"), value("Height")

            def state(window):
                return run("xprop", "-id", window, "_NET_WM_STATE")

            def click_button(window, index, right=True):
                x, y, w, _ = geometry(window)
                offset = MARGIN + BUTTON // 2 + index * (BUTTON + GAP)
                run("xdotool", "mousemove", x + (w - offset if right else offset), y + STRIP // 2, "click", 1)

            # No portal: minimize, maximize and close on the right.
            app, window, until = open_window("application")
            extents = run("xprop", "-id", window, "_NET_FRAME_EXTENTS")
            checks["undecorated"] = extents.endswith("0, 0, 0, 0") or "not found" in extents
            assert checks["undecorated"], extents
            before = geometry(window)
            click_button(window, 1)
            until("maximized", lambda: "MAXIMIZED_HORZ" in state(window))
            checks["maximize"] = until("the maximized size",
                                       lambda: geometry(window)[2:] if geometry(window)[2] > before[2] else None)
            click_button(window, 1)
            until("restored", lambda: "MAXIMIZED" not in state(window))
            checks["restore"] = until("the restored size",
                                      lambda: geometry(window)[2:] if geometry(window)[2] == before[2] else None)

            # A press on the bottom right corner resizes it through the
            # window manager (_NET_WM_MOVERESIZE).
            x, y, w, h = geometry(window)
            run("xdotool", "mousemove", x + w - 2, y + h - 2, "mousedown", 1)
            time.sleep(.2)
            run("xdotool", "mousemove", x + w + 60, y + h + 40)
            time.sleep(.2)
            run("xdotool", "mouseup", 1)
            resized = until("the resized size", lambda: geometry(window) if geometry(window)[2] >= w + 40 else None)
            checks["corner resize"] = [w, h, resized[2], resized[3]]
            assert resized[3] >= h + 20, checks

            # Its drag strip, as a GTK header bar: a double click maximizes
            # and restores it (a click is not a move), and a press moved
            # while held moves it.
            x, y, w, h = geometry(window)
            title = (x + w // 2, y + STRIP // 2)
            run("xdotool", "mousemove", *title, "click", "--repeat", "2", "--delay", "120", "1")
            until("maximized from the title bar", lambda: "MAXIMIZED_HORZ" in state(window))
            time.sleep(.6)
            run("xdotool", "mousemove", *geometry(window)[:2])
            run("xdotool", "mousemove_relative", "--", w // 2, STRIP // 2, "click", "--repeat", "2", "--delay", "120",
                "1")
            until("restored from the title bar", lambda: "MAXIMIZED" not in state(window))
            checks["title bar double click"] = True
            time.sleep(.6)
            x, y, w, h = until("the restored place", lambda: geometry(window) if geometry(window)[2] == w else None)
            run("xdotool", "mousemove", x + w // 2, y + STRIP // 2, "mousedown", "1")
            for _ in range(6):
                time.sleep(.08)
                run("xdotool", "mousemove_relative", "--", 10, 8)
            run("xdotool", "mouseup", "1")
            moved = until("the moved window", lambda: geometry(window) if geometry(window)[0] >= x + 30 else None)
            checks["title bar drag"] = [x, y, moved[0], moved[1]]
            assert moved[1] >= y + 20, checks

            click_button(window, 2)
            until("minimized", lambda: "HIDDEN" in state(window))
            checks["minimize"] = True
            run("xdotool", "windowactivate", "--sync", window)
            until("shown again", lambda: "HIDDEN" not in state(window))

            click_button(window, 0)
            app.wait(timeout=10)
            checks["close"] = app.returncode == 0
            assert checks["close"], app.returncode

            # GNOME's layout, through the settings portal: all three on the
            # left, then close alone on the right, live.
            bus = launch("dbus", ["dbus-daemon", "--session", "--nofork", "--print-address=1"], env,
                         stdout=subprocess.PIPE, text=True)
            if not select.select([bus.stdout], [], [], 10)[0]:
                raise RuntimeError("dbus-daemon startup timed out")
            env["DBUS_SESSION_BUS_ADDRESS"] = bus.stdout.readline().strip()
            portal = launch("portal", ["/usr/bin/python3", "-c", PORTAL, "close,minimize,maximize:"], env,
                            stdout=subprocess.PIPE, stdin=subprocess.PIPE, text=True)
            if not select.select([portal.stdout], [], [], 10)[0] or "ready" not in portal.stdout.readline():
                raise RuntimeError("the stand-in portal did not start; see portal.log")
            app, window, until = open_window("application-gnome")
            click_button(window, 2, right=False)
            until("maximized from the left", lambda: "MAXIMIZED_HORZ" in state(window))
            checks["layout from the portal"] = "close,minimize,maximize:"
            portal.stdin.write("appmenu:close\n")
            portal.stdin.flush()
            # Close's circle shows on the right once the layout moved.
            def pixel(px, py):
                text = run("import", "-window", "root", "-crop", f"1x1+{px}+{py}", "txt:-")
                return re.search(r"#([0-9A-Fa-f]{6})", text).group(1)

            def moved():
                x, y, w, _ = geometry(window)
                close = pixel(x + w - MARGIN - BUTTON // 2 + 8, y + STRIP // 2)
                strip = pixel(x + w // 2, y + 4)
                return close != strip
            until("close on the right", moved)
            click_button(window, 0)
            checks["layout followed live"] = app.wait(timeout=10) == 0
            (args.output / "result.json").write_text(json.dumps({"result": "pass", "checks": checks}, indent=2) + "\n")
            print("PASS: undecorated, maximize, restore, corner resize, minimize, close; "
                  "GNOME's layout read and followed live")
        finally:
            for process in reversed(processes):
                stop(process)


if __name__ == "__main__":
    main()
