#!/usr/bin/env python3
"""The desktop's double-click time on X11: a stand-in settings portal on a
private session bus gives GNOME's `double-click`, and an editor's double
click (a word selected, readable as the PRIMARY selection) follows it, as
GTK's does. At 1000 ms two presses 700 ms apart select a word; at 300 ms
two presses 450 ms apart do not (zgx's own default is 500 ms)."""
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
value = GLib.Variant("i", int(sys.argv[1]))

def call(conn, sender, path, iface, method, params, invocation):
    if tuple(params.unpack()) != ("org.gnome.desktop.peripherals.mouse", "double-click"):
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
    parser.add_argument("binary", type=pathlib.Path, help="the primary_selection example")
    parser.add_argument("--output", type=pathlib.Path, required=True)
    args = parser.parse_args()
    args.output.mkdir(parents=True, exist_ok=True)

    def session(milliseconds, gap):
        """Two presses `gap` seconds apart under a `milliseconds` double-click
        time: the PRIMARY selection after them, if any."""
        processes = []

        def launch(label, command, env, **options):
            log = (args.output / f"{milliseconds}-{label}.log").open("w")
            child = subprocess.Popen(command, env=env, stdout=options.pop("stdout", log), stderr=log, **options)
            processes.append(child)
            return child

        with private_runtime("zgx-double-click-") as private:
            env = dict(os.environ, XDG_RUNTIME_DIR=private, WINIT_X11_SCALE_FACTOR="1")
            for key in ("WAYLAND_DISPLAY", "DBUS_SESSION_BUS_ADDRESS"):
                env.pop(key, None)
            try:
                bus = launch("dbus", ["dbus-daemon", "--session", "--nofork", "--print-address=1"], env,
                             stdout=subprocess.PIPE, text=True)
                if not select.select([bus.stdout], [], [], 10)[0]:
                    raise RuntimeError("dbus-daemon startup timed out")
                env["DBUS_SESSION_BUS_ADDRESS"] = bus.stdout.readline().strip()
                portal = launch("portal", ["/usr/bin/python3", "-c", PORTAL, str(milliseconds)], env,
                                stdout=subprocess.PIPE, text=True)
                if not select.select([portal.stdout], [], [], 10)[0] or "ready" not in portal.stdout.readline():
                    raise RuntimeError("the stand-in portal did not start")
                xvfb = launch("xvfb", ["Xvfb", "-r", "-noreset", "-displayfd", "1", "-screen", "0", "1280x900x24", "-ac"], env,
                              stdout=subprocess.PIPE, text=True)
                if not select.select([xvfb.stdout], [], [], 30)[0]:
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
                # The window can show before the portal's answer arrives (zgx
                # gives it 300 ms, then reads it again behind the window), so
                # the presses wait for the example to have the time asked for.
                shown = time.monotonic()
                deadline = shown + 20
                log = args.output / f"{milliseconds}-application.log"
                while f"DOUBLE_CLICK {milliseconds}\n" not in log.read_text():
                    if time.monotonic() > deadline or app.poll() is not None:
                        raise RuntimeError(f"the example never had a {milliseconds} ms double-click time: "
                                           f"{log.read_text()!r}")
                    time.sleep(.05)
                print(f"{milliseconds} ms double-click time {(time.monotonic() - shown) * 1000:.0f} ms after the window",
                      flush=True)
                xdo = lambda *parts: subprocess.check_call(["xdotool", *map(str, parts)], env=env)
                xdo("windowactivate", "--sync", window)
                time.sleep(.4)
                # The same spot as primary_selection_smoke's double click.
                # One xdotool for both presses: two take as long to start as a
                # loaded runner likes, past the time this needs them within.
                xdo("mousemove", "--window", window, 110, 42, "click", 1, "sleep", gap, "click", 1)

                def primary():
                    read = subprocess.run(["xclip", "-o", "-selection", "primary"], env=env,
                                          capture_output=True, text=True, timeout=5)
                    return read.stdout if read.returncode == 0 else ""
                deadline = time.monotonic() + 2
                while time.monotonic() < deadline:
                    if primary() in ("select", "these", "words"):
                        break
                    time.sleep(.05)
                return primary()
            finally:
                for process in reversed(processes):
                    stop(process)

    slow = session(1000, .7)
    fast = session(300, .45)
    result = {"1000 ms, presses 700 ms apart": slow, "300 ms, presses 450 ms apart": fast}
    (args.output / "result.json").write_text(json.dumps(result, indent=2) + "\n")
    assert slow in ("select", "these", "words"), result
    assert fast not in ("select", "these", "words"), result
    print(f"PASS: a 1000 ms double-click time selected {slow!r}; at 300 ms nothing was")


if __name__ == "__main__":
    main()
