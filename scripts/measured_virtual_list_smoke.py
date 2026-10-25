#!/usr/bin/env python3
"""Check naturally measured streaming rows on an owned Xvfb/Openbox desktop."""
import argparse
import bisect
import json
import math
import os
from pathlib import Path
import re
import select
import subprocess
import tempfile
import time

from PIL import ImageGrab
from platform_smoke import stop


def wait_for_scanline(capture, expected, timeout=2., interval=.1):
    """Wait for presentation of exact model pixels, without sending more input."""
    started = time.monotonic()
    deadline = started + timeout
    attempts = []
    while True:
        picture, pixels = capture()
        transitions = [[i, color] for i, color in enumerate(pixels)
                       if i == 0 or color != pixels[i - 1]]
        matched = pixels == expected
        attempts.append(dict(elapsed_seconds=time.monotonic() - started,
                             matched=matched, transitions=transitions))
        if matched or time.monotonic() >= deadline:
            return picture, pixels, attempts
        time.sleep(interval)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("binary", type=Path)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    args.output.mkdir(parents=True, exist_ok=True)
    (args.output / "results.json").unlink(missing_ok=True)
    processes, logs, samples = [], [], []
    with tempfile.TemporaryDirectory(prefix="zgx-measured-list-") as runtime:
        def launch(name, command, **kwargs):
            log = (args.output / (name + ".log")).open("w")
            logs.append(log)
            process = subprocess.Popen(command, stdout=kwargs.pop("stdout", log), stderr=log, **kwargs)
            processes.append(process)
            return process

        try:
            xvfb = launch("xvfb", ["Xvfb", "-r", "-noreset", "-displayfd", "1", "-screen", "0", "1200x900x24", "-ac"], stdout=subprocess.PIPE, text=True)
            assert select.select([xvfb.stdout], [], [], 10)[0], "owned Xvfb startup timed out"
            display = ":" + xvfb.stdout.readline().strip()
            assert display != ":", "owned Xvfb failed"
            env = dict(os.environ, DISPLAY=display, XDG_RUNTIME_DIR=runtime,
                       XDG_CONFIG_HOME=runtime + "/config", XDG_CACHE_HOME=runtime + "/cache",
                       XDG_DATA_HOME=runtime + "/data", WINIT_UNIX_BACKEND="x11",
                       WINIT_X11_SCALE_FACTOR="1", DBUS_SESSION_BUS_ADDRESS="unix:path=/nonexistent",
                       GSETTINGS_BACKEND="memory")
            for name in ("WAYLAND_DISPLAY", "SWAYSOCK", "I3SOCK", "AT_SPI_BUS_ADDRESS",
                         "GTK_IM_MODULE", "QT_IM_MODULE", "XMODIFIERS", "IBUS_ADDRESS",
                         "SDL_IM_MODULE", "GLFW_IM_MODULE", "FCITX_DBUS_ADDRESS"):
                env.pop(name, None)
            launch("openbox", ["openbox"], env=env)
            time.sleep(.4)
            app = launch("application", [str(args.binary.resolve()), "--smoke-test"], env=env)

            def xdo(*parts):
                return subprocess.check_output(["xdotool", *map(str, parts)], env=env, text=True).strip()

            deadline = time.monotonic() + 20
            window = None
            while time.monotonic() < deadline and app.poll() is None:
                found = subprocess.run(["xdotool", "search", "--onlyvisible", "--pid", str(app.pid)], env=env, capture_output=True, text=True)
                if found.returncode == 0 and found.stdout.strip():
                    window = found.stdout.splitlines()[0]
                    break
                time.sleep(.05)
            assert window, "variable list client did not map"
            xdo("windowactivate", "--sync", window)
            xdo("mousemove", "--window", window, 620, 10)
            time.sleep(.3)

            def key(*keys):
                xdo("key", "--clearmodifiers", *keys)
                time.sleep(.12)

            def snapshot(stage, focused=None):
                before = len([line for line in (args.output / "application.log").read_text().splitlines() if line.startswith("MEASURED ")])
                key("r")
                deadline = time.monotonic() + 5
                while True:
                    reports = [json.loads(line[len("MEASURED "):]) for line in (args.output / "application.log").read_text().splitlines() if line.startswith("MEASURED ")]
                    if len(reports) > before:
                        report = reports[-1]
                        break
                    if time.monotonic() >= deadline or app.poll() is not None:
                        raise AssertionError("missing native snapshot: " + stage)
                    time.sleep(.05)
                lx, ly, width, height = report["list"]
                cx, cy, cw, ch = lx + 8, ly + 8, width - 16, height - 16
                assert 0 <= report["offset"] <= max(0, report["extent"] - ch), (stage, report)
                assert report["live"] == len(report["rows"]), (stage, report)
                assert 1 <= report["live"] <= math.ceil(ch / 54) + 6, (stage, report)
                assert report["built"] <= 180, (stage, report)
                if focused is not None:
                    assert report["focused"] == focused, (stage, report)
                for index, x, y, w, h, cached, top, natural in report["rows"]:
                    lines = index % 3 + 1 + (report["chunks"] if index == 1000 else 0)
                    expected = 32 + 22 * lines
                    assert h == cached == natural == expected, (stage, "natural height", index, expected, report)
                    assert abs(x - cx) < .1 and abs(y - (cy + top - report["offset"])) < .1, (stage, "position", report)
                    assert abs(w - cw) < .1, (stage, "width", report)
                for left, right in zip(report["rows"], report["rows"][1:]):
                    assert right[0] == left[0] + 1 and abs(right[2] - left[2] - left[4]) < .1, (stage, "row gap", report)
                assert report["rows"][0][2] <= cy and report["rows"][-1][2] + report["rows"][-1][4] >= cy + ch, (stage, "viewport coverage", report)
                info = subprocess.check_output(["xwininfo", "-id", window], env=env, text=True)
                ox = int(re.search(r"Absolute upper-left X:\s+(-?\d+)", info)[1])
                oy = int(re.search(r"Absolute upper-left Y:\s+(-?\d+)", info)[1])
                # Sample every solid row pixel before inset text, outside the
                # 2px focus marker. Model reports precede GPU presentation.
                scan_y = list(range(math.ceil(cy + 1), math.floor(cy + ch - 1)))
                rows = [next(row for row in report["rows"] if row[2] <= y + .5 < row[2] + row[4])
                        for y in scan_y]
                expected = [(32, 48, 64) if row[0] % 2 == 0 else (48, 64, 80) for row in rows]
                first = True
                def capture_scanline():
                    nonlocal first
                    picture = ImageGrab.grab(xdisplay=display).convert("RGB")
                    if first:
                        picture.save(args.output / (stage + "-first.png"))
                        first = False
                    pixels = [picture.getpixel((ox + round(cx + 4), oy + y)) for y in scan_y]
                    return picture, pixels
                picture, pixels, attempts = wait_for_scanline(capture_scanline, expected)
                picture.save(args.output / (stage + ".png"))
                evidence = dict(stage=stage, report=report, scanline_start=scan_y[0], attempts=attempts)
                (args.output / (stage + "-presentation.json")).write_text(json.dumps(evidence, indent=2) + "\n")
                print("MEASURED_PRESENTATION " + json.dumps(evidence), flush=True)
                for y, row, actual, wanted in zip(scan_y, rows, pixels, expected):
                    assert actual == wanted, (stage, "row pixel", row[0], y, actual, wanted)
                report.update(stage=stage, checked_scanline_pixels=len(pixels), content=[cx, cy, cw, ch])
                samples.append(report)
                return report

            initial = snapshot("initial")
            assert initial["offset"] == 0
            key("g")
            anchor = snapshot("anchor")
            row = next(row for row in anchor["rows"] if row[0] == 1000)
            assert abs(anchor["offset"] - row[6] - 8) < .1
            anchor_y = row[2]
            key("s")
            deadline = time.monotonic() + 5
            while "chunks=4" not in (args.output / "application.log").read_text():
                assert time.monotonic() < deadline and app.poll() is None, "stream did not finish"
                time.sleep(.05)
            streamed = snapshot("streamed-growth")
            assert streamed["chunks"] == 4
            assert next(row[2] for row in streamed["rows"] if row[0] == 1000) == anchor_y
            assert streamed["built"] == anchor["built"], ("stream rebuilt row constructors", anchor, streamed)
            xdo("windowsize", "--sync", window, 760, 560)
            time.sleep(.25)
            resized = snapshot("resized")
            assert resized["viewport"] == [760, 560]
            assert resized["content"][3] > streamed["content"][3]
            assert next(row[2] for row in resized["rows"] if row[0] == 1000) == anchor_y
            xdo("mousemove", "--window", window, 150, 150, "click", "--repeat", 5, "--delay", 25, 5)
            xdo("mousemove", "--window", window, 740, 10)
            time.sleep(.2)
            wheeled = snapshot("wheel")
            assert wheeled["offset"] > resized["offset"]
            key("Home")
            snapshot("home", 0)
            key("End")
            snapshot("end", 99_999)
            key("Escape")
            app.wait(timeout=5)
            assert app.returncode == 0
            assert "MEASURED_CLOSED" in (args.output / "application.log").read_text()
            (args.output / "results.json").write_text(json.dumps(dict(
                passed=True, backend="owned Xvfb / Openbox / native X11", row_count=100_000,
                height_source="mounted natural component heights; unseen rows remain estimates",
                checks=["natural child geometry", "bounded mounting", "anchor preservation",
                        "four streaming chunks", "retained constructors", "native resize",
                        "wheel and keyboard navigation", "row background pixels"],
                stages=samples), indent=2) + "\n")
            print("Native measured virtual list checks passed")
        finally:
            for process in reversed(processes):
                stop(process)
            for log in logs:
                log.close()


if __name__ == "__main__":
    main()
