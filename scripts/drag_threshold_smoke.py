#!/usr/bin/env python3
"""The desktop's drag threshold on X11: a stand-in settings portal on a
private session bus gives GNOME's `drag-threshold`, and a press on the
drag_drop example's card starts a drag only once the pointer has moved
farther than it, as GTK's does. At 24 a 12-pixel move is no drag and a
40-pixel one is; with no setting, GTK's own 8 holds: 6 pixels is no drag
(zgx's own default is 4), 12 is."""
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
value = GLib.Variant("i", int(sys.argv[1])) if sys.argv[1] != "none" else None

def call(conn, sender, path, iface, method, params, invocation):
    if value is None or tuple(params.unpack()) != ("org.gnome.desktop.peripherals.mouse", "drag-threshold"):
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
    parser.add_argument("binary", type=pathlib.Path, help="the drag_drop example")
    parser.add_argument("--output", type=pathlib.Path, required=True)
    args = parser.parse_args()
    args.output.mkdir(parents=True, exist_ok=True)

    def session(threshold, distances):
        """Under a `threshold` drag threshold (none: no setting), a press on
        the red card moved each of `distances` pixels and released: whether
        each was a drag."""
        processes = []

        def launch(label, command, env, **options):
            log = (args.output / f"{threshold}-{label}.log").open("w")
            child = subprocess.Popen(command, env=env, stdout=options.pop("stdout", log), stderr=log, **options)
            processes.append(child)
            return child

        with private_runtime("zgx-drag-threshold-") as private:
            env = dict(os.environ, XDG_RUNTIME_DIR=private, WINIT_X11_SCALE_FACTOR="1")
            for key in ("WAYLAND_DISPLAY", "DBUS_SESSION_BUS_ADDRESS"):
                env.pop(key, None)
            try:
                bus = launch("dbus", ["dbus-daemon", "--session", "--nofork", "--print-address=1"], env,
                             stdout=subprocess.PIPE, text=True)
                if not select.select([bus.stdout], [], [], 10)[0]:
                    raise RuntimeError("dbus-daemon startup timed out")
                env["DBUS_SESSION_BUS_ADDRESS"] = bus.stdout.readline().strip()
                portal = launch("portal", ["/usr/bin/python3", "-c", PORTAL, str(threshold)], env,
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
                log = args.output / f"{threshold}-application.log"
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
                dragged = []
                for distance in distances:
                    before = log.read_text().count("DRAG_END Red")
                    # The red card, as drag_drop_smoke presses it; moved in
                    # pixel steps, as a hand does.
                    xdo("mousemove", "--window", window, 60, 110, "mousedown", 1)
                    for step in range(1, distance + 1):
                        xdo("mousemove", "--window", window, 60 + step, 110)
                    time.sleep(.2)
                    xdo("mouseup", 1)
                    time.sleep(.6)
                    dragged.append(log.read_text().count("DRAG_END Red") > before)
                assert app.poll() is None, "the example exited"
                return dict(zip(map(str, distances), dragged))
            finally:
                for process in reversed(processes):
                    stop(process)

    raised = session(24, [12, 40])
    unset = session("none", [6, 12])
    result = {"drag-threshold 24": raised, "no setting (GTK's 8)": unset}
    (args.output / "result.json").write_text(json.dumps(result, indent=2) + "\n")
    assert raised == {"12": False, "40": True}, result
    assert unset == {"6": False, "12": True}, result
    print("PASS: at a drag threshold of 24 a 12-pixel move was no drag and a 40-pixel one was; "
          "with none, 6 was no drag and 12 was")


if __name__ == "__main__":
    main()
