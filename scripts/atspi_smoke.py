#!/usr/bin/env python3
"""Exercise zgx through a private live AT-SPI bus, not a simulated tree."""
import argparse
import json
import os
import pathlib
import select
import signal
import subprocess
import sys
import tempfile
import time
from platform_smoke import release_mounts, stop, wait_for_wm


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("binary", type=pathlib.Path)
    parser.add_argument("--output", type=pathlib.Path, required=True)
    parser.add_argument("--session", action="store_true", help=argparse.SUPPRESS)
    args = parser.parse_args()
    args.binary = args.binary.resolve()
    if not args.binary.is_file() or not os.access(args.binary, os.X_OK):
        parser.error("build the executable before running the bus harness")
    args.output = args.output.resolve()
    args.output.mkdir(parents=True, exist_ok=True)
    if not args.session:
        (args.output / "results.json").unlink(missing_ok=True)
        env = dict(os.environ, ZGX_PRIVATE_ATSPI="1")
        for name in ("AT_SPI_BUS_ADDRESS", "DBUS_SESSION_BUS_ADDRESS", "DISPLAY", "WAYLAND_DISPLAY", "SWAYSOCK", "I3SOCK"):
            env.pop(name, None)
        result = subprocess.run(["dbus-run-session", "--", sys.executable, str(pathlib.Path(__file__).resolve()), str(args.binary), "--output", str(args.output), "--session"], env=env)
        raise SystemExit(result.returncode)
    if os.environ.get("ZGX_PRIVATE_ATSPI") != "1":
        raise RuntimeError("the private session must be created by this runner")
    xvfb = wm = launcher = app = None
    events = []
    with tempfile.TemporaryDirectory(prefix="zgx-atspi-") as runtime:
        os.chmod(runtime, 0o700)
        try:
            with (args.output / "xvfb.log").open("w") as log:
                xvfb = subprocess.Popen(["Xvfb", "-displayfd", "1", "-screen", "0", "900x650x24", "-ac"], stdout=subprocess.PIPE, stderr=log, text=True)
                if not select.select([xvfb.stdout], [], [], 8)[0]:
                    raise RuntimeError("owned Xvfb startup timed out")
                display = ":" + xvfb.stdout.readline().strip()
            os.environ.update(DISPLAY=display, XDG_RUNTIME_DIR=runtime, WINIT_X11_SCALE_FACTOR="1",
                              XDG_CONFIG_HOME=str(pathlib.Path(runtime, "config")),
                              XDG_CACHE_HOME=str(pathlib.Path(runtime, "cache")),
                              GSETTINGS_BACKEND="memory")
            with (args.output / "openbox.log").open("w") as log:
                wm = subprocess.Popen(["openbox"], stdout=log, stderr=log)
            wait_for_wm(os.environ)
            with (args.output / "a11y-bus.log").open("w") as log:
                launcher = subprocess.Popen(["/usr/libexec/at-spi-bus-launcher", "--launch-immediately", "--a11y=1", "--screen-reader=1"], stdout=log, stderr=log, start_new_session=True)
            import gi
            gi.require_version("Atspi", "2.0")
            from gi.repository import Atspi, Gio, GLib
            session = Gio.bus_get_sync(Gio.BusType.SESSION, None)
            def call(interface, method, parameters):
                return session.call_sync("org.a11y.Bus", "/org/a11y/bus", interface, method, parameters, None, Gio.DBusCallFlags.NO_AUTO_START, 2000, None)
            deadline = time.monotonic() + 8
            while True:
                try:
                    for name in ("IsEnabled", "ScreenReaderEnabled"):
                        call("org.freedesktop.DBus.Properties", "Set", GLib.Variant("(ssv)", ("org.a11y.Status", name, GLib.Variant("b", True))))
                    address = call("org.a11y.Bus", "GetAddress", None).unpack()[0]
                    break
                except GLib.Error:
                    if time.monotonic() > deadline:
                        raise
                    time.sleep(.05)
            status = call("org.freedesktop.DBus.Properties", "GetAll", GLib.Variant("(s)", ("org.a11y.Status",))).unpack()[0]
            assert status["IsEnabled"] and status["ScreenReaderEnabled"], status
            (args.output / "bus.json").write_text(json.dumps(dict(session_address=os.environ["DBUS_SESSION_BUS_ADDRESS"], accessibility_address=address, status=status, owned_launcher_pid=launcher.pid), indent=2) + "\n")
            os.environ["AT_SPI_BUS_ADDRESS"] = address
            Atspi.init()
            context = GLib.MainContext.default()
            def pump():
                while context.pending():
                    context.iteration(False)
            def wait_for(function, description, timeout=8):
                deadline = time.monotonic() + timeout
                while time.monotonic() < deadline:
                    pump()
                    try:
                        value = function()
                        if value:
                            return value
                    except GLib.Error:
                        pass
                    time.sleep(.05)
                raise AssertionError("timed out: " + description)
            def record(event, *_):
                try:
                    events.append(dict(type=event.type, name=event.source.get_name(), detail1=event.detail1, detail2=event.detail2))
                except GLib.Error:
                    pass
            listener = Atspi.EventListener.new(record)
            for event_type in ("object:state-changed", "object:property-change", "object:text-changed", "object:text-caret-moved"):
                assert listener.register(event_type)
            with (args.output / "application.log").open("w") as log:
                app = subprocess.Popen([str(args.binary), "--smoke-test"], stdout=log, stderr=log)
                def tree():
                    desktop = Atspi.get_desktop(0)
                    result = []
                    def visit(node, depth):
                        if node is None or depth > 30:
                            return
                        node.clear_cache()
                        result.append((node, depth))
                        for index in range(node.get_child_count()):
                            visit(node.get_child_at_index(index), depth + 1)
                    visit(desktop, 0)
                    return result
                def find(name):
                    return next((node for node, _ in tree() if node.get_name() == name), None)
                wait_for(lambda: any(node.get_process_id() == app.pid for node, _ in tree()), "zgx application on private AT-SPI desktop")
                entry = wait_for(lambda: find("AT-SPI name"), "named text input")
                count = wait_for(lambda: find("AT-SPI count"), "count button")
                slider = wait_for(lambda: find("AT-SPI amount"), "numeric slider")
                option = wait_for(lambda: find("AT-SPI option"), "checkbox")
                disable = wait_for(lambda: find("Disable input"), "disable button")
                assert entry.get_role_name() in ("entry", "text")
                assert count.get_role_name() in ("button", "push button")
                assert slider.get_role_name() == "slider"
                assert option.get_role_name() == "check box"
                def snapshot_tree():
                    result = []
                    for node, depth in tree():
                        interfaces = list(node.get_interfaces())
                        record = dict(name=node.get_name(), role=node.get_role_name(), depth=depth, pid=node.get_process_id(), interfaces=interfaces)
                        if "Text" in interfaces:
                            record["text"] = Atspi.Text.get_text(node, 0, -1)
                        result.append(record)
                    return result
                (args.output / "tree.json").write_text(json.dumps(snapshot_tree(), indent=2) + "\n")
                def has_text(value):
                    return any(record.get("text") == value or record["name"] == value for record in snapshot_tree())
                def state(node, flag):
                    node.clear_cache()
                    return node.get_state_set().contains(flag)
                def activate(node):
                    action = node.get_action_iface()
                    assert action is not None and Atspi.Action.get_n_actions(action) > 0
                    assert Atspi.Action.do_action(action, 0)
                assert state(entry, Atspi.StateType.ENABLED)
                assert not state(option, Atspi.StateType.CHECKED)
                assert entry.get_component_iface().grab_focus()
                wait_for(lambda: state(entry, Atspi.StateType.FOCUSED), "AT-SPI focus action")
                text = wait_for(entry.get_text_iface, "editor Text interface")
                assert Atspi.Text.get_text(text, 0, -1) == "Ada"
                assert Atspi.Text.set_caret_offset(text, 1)
                wait_for(lambda: Atspi.Text.get_caret_offset(text) == 1, "AT-SPI caret action")
                assert Atspi.Text.get_text(text, 0, -1) == "Ada", "caret update must preserve full text"
                assert Atspi.Text.add_selection(text, 0, 3)
                def selected():
                    if Atspi.Text.get_n_selections(text) != 1:
                        return False
                    selection = Atspi.Text.get_selection(text, 0)
                    return selection.start_offset == 0 and selection.end_offset == 3
                wait_for(selected, "AT-SPI text selection action")
                assert Atspi.Text.get_text(text, 0, -1) == "Ada", "selection update must preserve full text"
                subprocess.run(["xdotool", "type", "--clearmodifiers", "Grace"], check=True)
                wait_for(lambda: Atspi.Text.get_text(text, 0, -1) == "Grace", "native typing updates AT-SPI text")
                activate(count)
                value = slider.get_value_iface()
                assert Atspi.Value.get_minimum_value(value) == 0 and Atspi.Value.get_maximum_value(value) == 100
                assert Atspi.Value.get_current_value(value) == 25
                assert Atspi.Value.set_current_value(value, 65)
                wait_for(lambda: Atspi.Value.get_current_value(value) == 65, "AT-SPI numeric value action")
                activate(option)
                wait_for(lambda: state(option, Atspi.StateType.CHECKED), "checkbox action and checked state")
                activate(disable)
                wait_for(lambda: not state(entry, Atspi.StateType.ENABLED), "reactive disabled state")
                assert not state(entry, Atspi.StateType.SENSITIVE)
                entry.get_component_iface().grab_focus()
                time.sleep(.15)
                pump()
                assert not state(entry, Atspi.StateType.FOCUSED)
                assert Atspi.Text.get_text(text, 0, -1) == "Grace"
                activate(disable)
                wait_for(lambda: state(entry, Atspi.StateType.ENABLED), "reactive enabled state")
                assert Atspi.Text.get_text(text, 0, -1) == "Grace"
                wait_for(lambda: has_text("Count: 1"), "button activation changes accessible status")
                # Disable the platform bridge, change the real application while
                # it is inactive, then require fresh nodes with current state.
                button_bounds = Atspi.Component.get_extents(count, Atspi.CoordType.SCREEN)
                for name in ("ScreenReaderEnabled", "IsEnabled"):
                    call("org.freedesktop.DBus.Properties", "Set", GLib.Variant("(ssv)", ("org.a11y.Status", name, GLib.Variant("b", False))))
                wait_for(lambda: find("AT-SPI name") is None, "accessibility deactivation", timeout=5)
                subprocess.run(["xdotool", "mousemove", str(button_bounds.x + button_bounds.width // 2), str(button_bounds.y + button_bounds.height // 2), "click", "1"], check=True)
                time.sleep(.2)
                for name in ("IsEnabled", "ScreenReaderEnabled"):
                    call("org.freedesktop.DBus.Properties", "Set", GLib.Variant("(ssv)", ("org.a11y.Status", name, GLib.Variant("b", True))))
                entry = wait_for(lambda: find("AT-SPI name"), "accessibility reactivation")
                wait_for(lambda: has_text("Count: 2"), "fresh tree includes update made while inactive")
                text = entry.get_text_iface()
                slider = find("AT-SPI amount")
                value = slider.get_value_iface()
                assert Atspi.Text.get_text(text, 0, -1) == "Grace"
                assert Atspi.Value.get_current_value(value) == 65
                assert state(find("AT-SPI option"), Atspi.StateType.CHECKED)
                assert state(entry, Atspi.StateType.ENABLED)
                activate(find("AT-SPI count"))
                wait_for(lambda: has_text("Count: 3"), "actions work after reactivation")
                pump()
                assert any(event["type"].startswith("object:state-changed:focused") for event in events), events
                assert any(event["type"].startswith("object:text-changed") for event in events), events
                (args.output / "final-tree.json").write_text(json.dumps(snapshot_tree(), indent=2) + "\n")
                subprocess.run(["import", "-display", display, "-window", "root", str(args.output / "atspi.png")], check=True, timeout=10)
                (args.output / "results.json").write_text(json.dumps(dict(passed=True,
                    backend="live AccessKit Unix AT-SPI bridge / private D-Bus / owned X11",
                    checks=["tree roles and names", "focus action and event", "text reading", "caret and selection actions", "native typing reflected in text interface", "button activation", "numeric value mutation", "checkbox state", "disabled and reenabled input", "bridge deactivation and fresh-state reactivation"],
                    final_text=Atspi.Text.get_text(text, 0, -1), final_value=Atspi.Value.get_current_value(value),
                    events_observed=len(events), process_id=app.pid,
                    limitation="AT-SPI bus-client validation; not an Orca end-user usability test."), indent=2) + "\n")
                print("Live AT-SPI bus checks passed")
        finally:
            (args.output / "events.json").write_text(json.dumps(events, indent=2) + "\n")
            stop(app)
            stop(wm)
            if launcher is not None:
                try:
                    os.killpg(launcher.pid, signal.SIGTERM)
                except ProcessLookupError:
                    pass
                launcher.wait(timeout=5)
            stop(xvfb)
            release_mounts(runtime)


if __name__ == "__main__":
    main()
