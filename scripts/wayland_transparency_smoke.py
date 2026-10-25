#!/usr/bin/env python3
"""Check native Wayland source-over alpha on an owned nested Sway compositor."""
from sway_host import wait_for_openbox
import argparse
import json
import os
from pathlib import Path
import re
import select
import subprocess
import tempfile
import time

from PIL import Image
from platform_smoke import stop


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("binary", type=Path)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    args.output.mkdir(parents=True, exist_ok=True)
    (args.output / "results.json").unlink(missing_ok=True)
    processes, logs, stages = [], [], []
    with tempfile.TemporaryDirectory(prefix="zgx-wayland-alpha-") as runtime:
        os.chmod(runtime, 0o700)

        def launch(name, command, **kwargs):
            log = (args.output / (name + ".log")).open("w")
            logs.append(log)
            process = subprocess.Popen(command, stderr=log,
                                       stdout=kwargs.pop("stdout", log), **kwargs)
            processes.append(process)
            return process

        try:
            config = Path(runtime, "sway.conf")
            config.write_text('xwayland disable\nseat seat0 fallback true\n'
                              'output * mode 1200x800\ndefault_border none\n'
                              'for_window [title="zgx transparency"] floating enable, '
                              'border none, resize set width 320 px height 240 px, '
                              'move position 160 px 140 px\n')
            (args.output / "sway.conf").write_text(config.read_text())
            env = dict(os.environ, XDG_RUNTIME_DIR=runtime, WLR_BACKENDS="x11",
                       WLR_X11_OUTPUTS="1", WLR_RENDERER="pixman",
                       XDG_CONFIG_HOME=runtime + "/config",
                       XDG_CACHE_HOME=runtime + "/cache",
                       XDG_DATA_HOME=runtime + "/data",
                       DBUS_SESSION_BUS_ADDRESS="unix:path=/nonexistent",
                       GSETTINGS_BACKEND="memory")
            for name in ("DISPLAY", "WAYLAND_DISPLAY", "SWAYSOCK", "I3SOCK",
                         "AT_SPI_BUS_ADDRESS", "GTK_IM_MODULE", "QT_IM_MODULE",
                         "XMODIFIERS", "SDL_IM_MODULE", "GLFW_IM_MODULE",
                         "IBUS_ADDRESS", "FCITX_DBUS_ADDRESS"):
                env.pop(name, None)
            xvfb = launch("xvfb", ["Xvfb", "-r", "-displayfd", "1", "-screen", "0",
                                   "1400x1000x24", "-ac"], stdout=subprocess.PIPE, text=True)
            assert select.select([xvfb.stdout], [], [], 8)[0], "owned Xvfb startup timed out"
            env["DISPLAY"] = ":" + xvfb.stdout.readline().strip()
            wm = launch("openbox", ["openbox"], env=env)
            wait_for_openbox(env, args.output, wm)
            compositor = launch("sway", ["sway", "--unsupported-gpu", "--config", str(config)], env=env)
            deadline = time.monotonic() + 8
            while True:
                sockets = list(Path(runtime).glob("sway-ipc*.sock"))
                displays = [p for p in Path(runtime).glob("wayland-*") if p.suffix != ".lock"]
                if sockets and displays:
                    break
                if time.monotonic() > deadline or compositor.poll() is not None:
                    raise RuntimeError("owned compositor startup failed")
                time.sleep(.05)
            env.update(WAYLAND_DISPLAY=displays[0].name, SWAYSOCK=str(sockets[0]))

            def ipc(*parts):
                return json.loads(subprocess.check_output(
                    ["swaymsg", "-s", str(sockets[0]), *map(str, parts)], env=env, text=True))

            def command(*parts):
                response = ipc(*parts)
                assert all(item.get("success") for item in response), response

            def xdo(*parts):
                return subprocess.check_output(["xdotool", *map(str, parts)], env=env, text=True).strip()

            output_name = ipc("-t", "get_outputs")[0]["name"]
            deadline = time.monotonic() + 8
            while True:
                tree = subprocess.check_output(["xwininfo", "-root", "-tree"], env=env, text=True)
                match = re.search(r'(0x[0-9a-f]+) "wlroots - X11-1".*1200x800', tree)
                if match:
                    break
                if time.monotonic() > deadline:
                    raise RuntimeError("owned compositor X11 window did not map")
                time.sleep(.05)
            (args.output / "x11-tree.log").write_text(tree)
            outer_window = match.group(1)
            xdo("windowfocus", "--sync", outer_window)
            native_env = dict(env, WINIT_UNIX_BACKEND="wayland")
            native_env.pop("DISPLAY", None)
            backdrop = launch("background-initial", ["swaybg", "-o", output_name, "-c", "#204060"], env=native_env)
            protocol_file = (args.output / "protocol.log").open("w")
            logs.append(protocol_file)
            application_file = (args.output / "application.log").open("w")
            logs.append(application_file)
            app = subprocess.Popen([str(args.binary.resolve()), "--smoke-test"],
                                   env=dict(native_env, WAYLAND_DEBUG="1"),
                                   stdout=application_file, stderr=protocol_file)
            processes.append(app)

            def client_node():
                def walk(node):
                    if node.get("pid") == app.pid and node.get("name") == "zgx transparency":
                        return node
                    for child in node.get("nodes", []) + node.get("floating_nodes", []):
                        found = walk(child)
                        if found:
                            return found
                return walk(ipc("-t", "get_tree"))

            deadline = time.monotonic() + 10
            while not (node := client_node()):
                if time.monotonic() > deadline or app.poll() is not None:
                    raise RuntimeError("native transparent client did not map")
                time.sleep(.05)
            assert node["shell"] == "xdg_shell", node
            criterion = "[con_id=" + str(node["id"]) + "]"
            command(criterion, "focus")
            # Pointer stays outside the client, avoiding button hover and cursor samples.
            xdo("mousemove", "--window", outer_window, 1100, 700)

            def blend(color, behind):
                return tuple(round(color[i] * color[3] / 255 + behind[i] * (1 - color[3] / 255)) for i in range(3))

            red, blue, green = (224, 64, 32, 128), (32, 96, 224, 128), (64, 224, 128, 64)

            def checks(color, background, visible=True):
                under = blend(color, background) if visible else background
                return [(40, 40, under), (120, 80, blend(green, under)),
                        (180, 120, blend(green, background)), (280, 40, background),
                        (25, 165, (238, 238, 238))]

            def sample(name, expected_rect, checks):
                deadline = time.monotonic() + 8
                path = args.output / (name + ".png")
                while True:
                    node = client_node()
                    assert node, "client closed before sampling"
                    rect = node["rect"]
                    subprocess.run(["grim", "-o", output_name, str(path.resolve())], env=native_env, check=True)
                    with Image.open(path) as source:
                        picture = source.convert("RGB")
                    assert picture.size == (1200, 800), picture.size
                    actual = [picture.getpixel((rect["x"] + x, rect["y"] + y)) for x, y, _ in checks]
                    background_actual = picture.getpixel((1000, 600))
                    good = all(all(abs(a - b) <= 2 for a, b in zip(pixel, expected))
                               for pixel, (_, _, expected) in zip(actual, checks))
                    good = good and background_actual == checks[3][2]
                    if good and rect == expected_rect:
                        break
                    if time.monotonic() >= deadline or app.poll() is not None:
                        path.rename(args.output / (name + "-failed.png"))
                        raise AssertionError(dict(stage=name, rect=rect, expected_rect=expected_rect,
                                                  actual=actual, checks=checks, background=background_actual))
                    time.sleep(.1)
                stages.append(dict(stage=name, client_rect=rect, background_actual=background_actual,
                                   samples=[dict(position=[x, y], expected=expected, actual=pixel)
                                            for (x, y, expected), pixel in zip(checks, actual)]))

            background = (32, 64, 96)
            rect = dict(x=160, y=140, width=320, height=240)
            sample("initial", rect, checks(red, background))
            xdo("key", "c")
            sample("color-update", rect, checks(blue, background))
            xdo("key", "v")
            sample("hidden-panel", rect, checks(blue, background, False))
            command(criterion, "resize set width 400 px height 300 px, move position 100 px 120 px")
            rect = dict(x=100, y=120, width=400, height=300)
            sample("moved-resized", rect, checks(blue, background, False) + [(380, 280, background)])
            xdo("key", "v")
            sample("restored-panel", rect, checks(blue, background))
            stop(backdrop)
            background = (112, 48, 16)
            launch("background-changed", ["swaybg", "-o", output_name, "-c", "#703010"], env=native_env)
            sample("backdrop-change", rect, checks(blue, background) + [(380, 280, background)])
            xdo("key", "Escape")
            app.wait(timeout=10)
            assert app.returncode == 0, app.returncode
            assert compositor.poll() is None, "compositor exited"
            protocol = (args.output / "protocol.log").read_text()
            for proof in ("wl_compositor", "xdg_wm_base", "get_xdg_surface", "set_title(\"zgx transparency\")", "attach("):
                assert proof in protocol, ("missing native protocol evidence", proof)
            application = (args.output / "application.log").read_text()
            assert "TRANSPARENCY blue=true visible=true" in application, application
            (args.output / "results.json").write_text(json.dumps(dict(
                passed=True, backend="native Wayland / owned Sway X11 / Xvfb / pixman",
                compositor=subprocess.check_output(["sway", "--version"], text=True).strip(),
                client_display_unset="DISPLAY" not in native_env, client_shell="xdg_shell",
                client_pid=app.pid, protocol_log="protocol.log", tolerance=2, stages=stages), indent=2) + "\n")
            print("Native Wayland transparency checks passed")
        finally:
            for process in reversed(processes):
                stop(process)
            for log in logs:
                log.close()


if __name__ == "__main__":
    main()
