#!/usr/bin/env python3
"""Exercise a native Wayland client across live Sway output scale 1 -> 2 -> 1."""
from sway_host import wait_for_openbox
import argparse
import json
import os
import pathlib
import re
import select
import subprocess
import tempfile
import time
from platform_smoke import stop
from PIL import Image


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("binary", type=pathlib.Path)
    parser.add_argument("--output", type=pathlib.Path, required=True)
    args = parser.parse_args()
    args.output.mkdir(parents=True, exist_ok=True)
    (args.output / "results.json").unlink(missing_ok=True)
    compositor = app = xvfb = wm = None
    with tempfile.TemporaryDirectory(prefix="zgui-live-dpi-") as runtime:
        os.chmod(runtime, 0o700)
        config = pathlib.Path(runtime, "sway.conf")
        config.write_text('xwayland disable\nseat seat0 fallback true\noutput * mode 1200x800\ndefault_border none\nfor_window [title="zgui live DPI probe"] fullscreen enable\n')
        (args.output / "sway.conf").write_text(config.read_text())
        env = dict(os.environ, XDG_RUNTIME_DIR=runtime, WLR_BACKENDS="x11", WLR_X11_OUTPUTS="1", WLR_RENDERER="pixman")
        for name in ("DISPLAY", "WAYLAND_DISPLAY", "SWAYSOCK", "I3SOCK"):
            env.pop(name, None)
        try:
            with (args.output / "xvfb.log").open("w") as log:
                xvfb = subprocess.Popen(["Xvfb", "-displayfd", "1", "-screen", "0", "1400x1000x24", "-ac"], stdout=subprocess.PIPE, stderr=log, text=True)
                if not select.select([xvfb.stdout], [], [], 8)[0]:
                    raise RuntimeError("owned Xvfb startup timed out")
                env["DISPLAY"] = ":" + xvfb.stdout.readline().strip()
            with (args.output / "openbox.log").open("w") as log:
                wm = subprocess.Popen(["openbox"], env=env, stdout=log, stderr=log)
            wait_for_openbox(env, args.output, wm)
            with (args.output / "sway.log").open("w") as log:
                compositor = subprocess.Popen(["sway", "--unsupported-gpu", "--config", str(config)], env=env, stdout=log, stderr=log)
            deadline = time.monotonic() + 8
            while True:
                sockets = list(pathlib.Path(runtime).glob("sway-ipc*.sock"))
                displays = [p for p in pathlib.Path(runtime).glob("wayland-*") if p.suffix != ".lock"]
                if sockets and displays:
                    break
                if time.monotonic() > deadline or compositor.poll() is not None:
                    raise RuntimeError("owned compositor startup failed")
                time.sleep(.05)
            env.update(WAYLAND_DISPLAY=displays[0].name, SWAYSOCK=str(sockets[0]))
            def ipc(*parts):
                return json.loads(subprocess.check_output(["swaymsg", "-s", str(sockets[0]), *map(str, parts)], env=env, text=True))
            output_name = ipc("-t", "get_outputs")[0]["name"]
            def xdo(*parts):
                return subprocess.check_output(["xdotool", *map(str, parts)], env=env, text=True).strip()
            # The X11 backend's window name is populated asynchronously. Find
            # its exact physical size on our private display, never a user window.
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
            window = match.group(1)
            xdo("windowfocus", "--sync", window)
            client_env = dict(env, WAYLAND_DEBUG="1", WINIT_UNIX_BACKEND="wayland")
            client_env.pop("DISPLAY", None)
            with (args.output / "protocol.log").open("w") as protocol, (args.output / "probe.log").open("w") as log:
                app = subprocess.Popen([str(args.binary.resolve()), "--smoke-test"], env=client_env, stdout=log, stderr=protocol)
                deadline = time.monotonic() + 8
                while "VIEWPORT (1200.0, 800.0)" not in (args.output / "probe.log").read_text():
                    if time.monotonic() > deadline or app.poll() is not None:
                        raise RuntimeError("native client did not receive fullscreen viewport")
                    time.sleep(.05)
                def click(x, y):
                    xdo("mousemove", "--window", window, x * scale, y * scale, "click", 1)
                    time.sleep(.1)
                stages = []
                for stage, scale, character in [(1, 1, "A"), (2, 2, "B"), (3, 1, "C")]:
                    response = ipc("output", output_name, "scale", scale)
                    assert all(item.get("success") for item in response), response
                    time.sleep(.4)
                    state = ipc("-t", "get_outputs")[0]
                    assert state["scale"] == scale
                    click(29, 70)
                    xdo("type", "--clearmodifiers", character)
                    time.sleep(.15)
                    subprocess.run(["grim", "-o", output_name, str((args.output / f"dpi-{stage}.png").resolve())], env=env, check=True)
                    picture = Image.open(args.output / f"dpi-{stage}.png").convert("RGB")
                    assert picture.size == (1200, 800), picture.size
                    # Solid editor fill below glyphs and above rounded corners:
                    # one logical border pixel surrounds a 398-pixel interior.
                    fill = [x for x in range(picture.width)
                            if picture.getpixel((x, 90 * scale)) == (32, 48, 80)]
                    assert fill == list(range(21 * scale, 419 * scale)), (stage, fill[:3], fill[-3:])
                    click(60, 125)
                    stages.append({"stage": stage, "scale": scale, "output": state,
                                   "screenshot_size": picture.size, "editor_fill_pixel_width": len(fill)})
                app.wait(timeout=18)
            if app.returncode:
                raise RuntimeError((args.output / "protocol.log").read_text()[-4000:])
            output = (args.output / "probe.log").read_text()
            reports = [line for line in output.splitlines() if line.startswith("DPI ")]
            assert len(reports) == 3, output
            for report, expected_viewport, model in zip(reports, ["(1200.0, 800.0)", "(600.0, 400.0)", "(1200.0, 800.0)"], ["A0123456789", "BA0123456789", "CBA0123456789"]):
                assert "viewport=" + expected_viewport in report, report
                assert 'model="' + model + '"' in report and "selection=1 " in report, report
                assert "editor=Rect { x: 20.0, y: 54.0, width: 400.0, height: 44.0 }" in report, report
                # The single line is centred in the 44-pixel box: (44 - 23) / 2 below its top.
                assert re.search(r"caret=Rect \{ x: [\d.]+, y: 64\.5, width: 1\.0, height: 23\.0 \}", report), report
            # One 16px monospace advance (0.55-0.65 em, whichever system face
            # answers) past the 28px text origin, unmoved by scale changes.
            carets = {float(re.search(r"caret=Rect \{ x: ([\d.]+)", report)[1]) for report in reports}
            assert len(carets) == 1 and 28 + 8.8 <= min(carets) <= 28 + 10.4, reports
            protocol = (args.output / "protocol.log").read_text()
            scales = re.findall(r"preferred_scale\((\d+)\)", protocol)
            assert scales == ["120", "240", "120"], scales
            destinations = re.findall(r"set_destination\((\d+), (\d+)\)", protocol)
            assert destinations[-3:] == [("1200", "800"), ("600", "400"), ("1200", "800")], destinations
            (args.output / "results.json").write_text(json.dumps({"passed": True, "compositor": subprocess.check_output(["sway", "--version"], text=True).strip(), "backend": "native Wayland / owned Sway X11 / Xvfb / pixman", "transitions": stages, "reports": reports, "client_pid": app.pid, "native_scale_notifications": scales}, indent=2) + "\n")
            print("Native live DPI transition checks passed")
        finally:
            stop(app)
            stop(compositor)
            stop(wm)
            stop(xvfb)


if __name__ == "__main__":
    main()
