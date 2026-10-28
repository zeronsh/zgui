#!/usr/bin/env python3
"""GNOME's `gtk-enable-primary-paste` on X11, from a stand-in settings portal
on a private session bus, as GTK follows it: off, a middle click in the
primary_selection example's empty editor pastes nothing, though another
client owns the primary selection; on, it pastes it there."""
import argparse
import json
import os
import pathlib
import re
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
value = GLib.Variant("b", sys.argv[1] == "on")

def call(conn, sender, path, iface, method, params, invocation):
    if tuple(params.unpack()) != ("org.gnome.desktop.interface", "gtk-enable-primary-paste"):
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
OUTSIDE = "from another client"


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("binary", type=pathlib.Path, help="the primary_selection example")
    parser.add_argument("--output", type=pathlib.Path, required=True)
    args = parser.parse_args()
    args.output.mkdir(parents=True, exist_ok=True)

    def session(setting):
        """The target editor's text after a middle click in it, under
        `gtk-enable-primary-paste` `setting`."""
        processes = []

        def launch(label, command, env, **options):
            log = (args.output / f"{setting}-{label}.log").open("w")
            child = subprocess.Popen(command, env=env, stdout=options.pop("stdout", log), stderr=log, **options)
            processes.append(child)
            return child

        with private_runtime("zgx-primary-paste-") as private:
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
                xvfb = launch("xvfb", ["Xvfb", "-r", "-noreset", "-displayfd", "1", "-screen", "0", "1280x900x24",
                                       "-ac"], env, stdout=subprocess.PIPE, text=True)
                if not select.select([xvfb.stdout], [], [], 30)[0]:
                    raise RuntimeError("Xvfb startup timed out")
                env["DISPLAY"] = ":" + xvfb.stdout.readline().strip()
                launch("openbox", ["openbox"], env)
                wait_for_wm(env)
                log = args.output / f"{setting}-application.log"
                app = launch("application", [str(args.binary.resolve())], env, stdout=log.open("w"))
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
                time.sleep(.4)
                # Another client's selection (xclip serves it until another
                # takes it).
                subprocess.run(["xclip", "-i", "-selection", "primary"], env=env, input=OUTSIDE, text=True,
                               check=True, timeout=5)
                # In the empty editor, as primary_selection_smoke clicks.
                xdo("mousemove", "--window", window, 120, 130, "click", 2)
                time.sleep(.6)
                # Closed as a window manager closes it: the example reports
                # its editors' text.
                xdo("windowactivate", "--sync", window, "key", "alt+F4")
                app.wait(timeout=15)
                match = re.search(r'PRIMARY source="([^"]*)" target="([^"]*)"', log.read_text())
                if not match:
                    raise RuntimeError(f"no report: {log.read_text()}")
                return match[2]
            finally:
                for process in reversed(processes):
                    stop(process)

    off = session("off")
    on = session("on")
    result = {"gtk-enable-primary-paste off, the editor": off, "on": on}
    (args.output / "result.json").write_text(json.dumps(result, indent=2) + "\n")
    assert off == "" and on == OUTSIDE, result
    print(f"PASS: with gtk-enable-primary-paste off a middle click pasted nothing; on, it pasted {on!r}")


if __name__ == "__main__":
    main()
