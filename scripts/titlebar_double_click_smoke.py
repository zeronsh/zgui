#!/usr/bin/env python3
"""GNOME's `action-double-click-titlebar` on X11, from a stand-in settings
portal on a private session bus, as GTK's header bars follow it: a double
click on the glass_counter example's drag strip minimizes its window under
"minimize", does nothing under "none" and maximizes it under
"toggle-maximize", each read live."""
import argparse
import json
import os
import pathlib
import re
import select
import subprocess
import time

from platform_smoke import private_runtime, stop, wait_for_wm

# The strip's height, as window_buttons_smoke has it.
STRIP = 40

PORTAL = r'''
import sys
from gi.repository import Gio, GLib

XML = """<node><interface name="org.freedesktop.portal.Settings">
  <method name="ReadOne"><arg type="s" direction="in"/><arg type="s" direction="in"/><arg type="v" direction="out"/></method>
  <signal name="SettingChanged"><arg type="s"/><arg type="s"/><arg type="v"/></signal>
  <property name="version" type="u" access="read"/>
</interface></node>"""
NAMESPACE, KEY = "org.gnome.desktop.wm.preferences", "action-double-click-titlebar"
action = GLib.Variant("s", sys.argv[1])
connection = None

def call(conn, sender, path, iface, method, params, invocation):
    if tuple(params.unpack()) != (NAMESPACE, KEY):
        invocation.return_dbus_error("org.freedesktop.portal.Error.NotFound", "no such setting")
        return
    invocation.return_value(GLib.Variant("(v)", (action,)))

def acquired(conn, name):
    global connection
    connection = conn
    info = Gio.DBusNodeInfo.new_for_xml(XML).interfaces[0]
    conn.register_object("/org/freedesktop/portal/desktop", info, call,
                         lambda *_: GLib.Variant("u", 2), None)
    print("PORTAL ready", flush=True)

def command(source, condition):
    global action
    line = sys.stdin.readline()
    if not line:
        loop.quit()
        return False
    action = GLib.Variant("s", line.strip())
    connection.emit_signal(None, "/org/freedesktop/portal/desktop", "org.freedesktop.portal.Settings",
                           "SettingChanged", GLib.Variant("(ssv)", (NAMESPACE, KEY, action)))
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
    parser.add_argument("binary", type=pathlib.Path, help="the glass_counter example")
    parser.add_argument("--output", type=pathlib.Path, required=True)
    args = parser.parse_args()
    args.output.mkdir(parents=True, exist_ok=True)
    processes = []
    checks = {}

    def launch(label, command, env, **options):
        log = (args.output / f"{label}.log").open("w")
        child = subprocess.Popen(command, env=env, stdout=options.pop("stdout", log), stderr=log, **options)
        processes.append(child)
        return child

    def until(what, check, seconds=10):
        deadline = time.monotonic() + seconds
        while time.monotonic() < deadline:
            found = check()
            if found:
                return found
            time.sleep(.05)
        raise RuntimeError(f"timed out waiting for {what}")

    with private_runtime("zgx-titlebar-") as private:
        env = dict(os.environ, XDG_RUNTIME_DIR=private, WINIT_X11_SCALE_FACTOR="1")
        for key in ("WAYLAND_DISPLAY", "DBUS_SESSION_BUS_ADDRESS"):
            env.pop(key, None)
        try:
            bus = launch("dbus", ["dbus-daemon", "--session", "--nofork", "--print-address=1"], env,
                         stdout=subprocess.PIPE, text=True)
            if not select.select([bus.stdout], [], [], 10)[0]:
                raise RuntimeError("dbus-daemon startup timed out")
            env["DBUS_SESSION_BUS_ADDRESS"] = bus.stdout.readline().strip()
            portal = launch("portal", ["/usr/bin/python3", "-c", PORTAL, "minimize"], env,
                            stdout=subprocess.PIPE, stdin=subprocess.PIPE, text=True)
            if not select.select([portal.stdout], [], [], 10)[0] or "ready" not in portal.stdout.readline():
                raise RuntimeError("the stand-in portal did not start")
            xvfb = launch("xvfb", ["Xvfb", "-r", "-noreset", "-displayfd", "1", "-screen", "0", "1280x900x24", "-ac"],
                          env, stdout=subprocess.PIPE, text=True)
            if not select.select([xvfb.stdout], [], [], 30)[0]:
                raise RuntimeError("Xvfb startup timed out")
            env["DISPLAY"] = ":" + xvfb.stdout.readline().strip()
            launch("openbox", ["openbox"], env)
            wait_for_wm(env)
            run = lambda *argv: subprocess.check_output(list(map(str, argv)), env=env, text=True).strip()
            app = launch("application", [str(args.binary.resolve())], env)

            def visible():
                if app.poll() is not None:
                    raise RuntimeError("the example exited")
                found = subprocess.run(["xdotool", "search", "--onlyvisible", "--pid", str(app.pid)], env=env,
                                       capture_output=True, text=True)
                return found.stdout.split()[0] if found.returncode == 0 and found.stdout.strip() else None
            window = until("the window", visible, 20)
            run("xdotool", "windowactivate", "--sync", window)
            time.sleep(.4)

            def state():
                return run("xprop", "-id", window, "_NET_WM_STATE")

            def double_click():
                info = run("xwininfo", "-id", window)
                x = int(re.search(r"Absolute upper-left X:\s+(-?\d+)", info).group(1))
                y = int(re.search(r"Absolute upper-left Y:\s+(-?\d+)", info).group(1))
                w = int(re.search(r"Width:\s+(\d+)", info).group(1))
                run("xdotool", "mousemove", x + w // 2, y + STRIP // 2, "click", "--repeat", "2", "--delay", "120", "1")

            def portal_sets(value):
                portal.stdin.write(value + "\n")
                portal.stdin.flush()
                until("the portal's change", lambda: "changed" in portal.stdout.readline())
                time.sleep(.3)

            double_click()
            until("minimized", lambda: "HIDDEN" in state())
            checks["minimize"] = True
            run("xdotool", "windowactivate", "--sync", window)
            until("shown again", lambda: "HIDDEN" not in state())
            time.sleep(.4)
            portal_sets("none")
            double_click()
            time.sleep(1)
            checks["none"] = state()
            assert "HIDDEN" not in checks["none"] and "MAXIMIZED" not in checks["none"], checks
            portal_sets("toggle-maximize")
            double_click()
            until("maximized", lambda: "MAXIMIZED_HORZ" in state())
            checks["toggle-maximize"] = True
            (args.output / "result.json").write_text(json.dumps(checks, indent=2) + "\n")
            print("PASS: a title bar double click minimized, did nothing, then maximized, as the portal said live")
        finally:
            for process in reversed(processes):
                stop(process)


if __name__ == "__main__":
    main()
