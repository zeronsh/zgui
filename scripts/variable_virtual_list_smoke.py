#!/usr/bin/env python3
"""Check explicit variable-height streaming rows on an owned Xvfb/Openbox desktop."""
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


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("binary", type=Path)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    args.output.mkdir(parents=True, exist_ok=True)
    (args.output / "results.json").unlink(missing_ok=True)
    processes, logs, samples = [], [], []
    heights = [24, 40, 64, 88] * 25_000

    def prefixes():
        result = [0]
        for height in heights:
            result.append(result[-1] + height)
        return result

    with tempfile.TemporaryDirectory(prefix="zgui-variable-list-") as runtime:
        def launch(name, command, **kwargs):
            log = (args.output / (name + ".log")).open("w")
            logs.append(log)
            process = subprocess.Popen(command, stdout=kwargs.pop("stdout", log), stderr=log, **kwargs)
            processes.append(process)
            return process

        try:
            xvfb = launch("xvfb", ["Xvfb", "-displayfd", "1", "-screen", "0", "1200x900x24", "-ac"], stdout=subprocess.PIPE, text=True)
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
                before = len([line for line in (args.output / "application.log").read_text().splitlines() if line.startswith("VARIABLE ")])
                key("r")
                deadline = time.monotonic() + 5
                while True:
                    reports = [json.loads(line[len("VARIABLE "):]) for line in (args.output / "application.log").read_text().splitlines() if line.startswith("VARIABLE ")]
                    if len(reports) > before:
                        report = reports[-1]
                        break
                    if time.monotonic() >= deadline or app.poll() is not None:
                        raise AssertionError("missing native snapshot: " + stage)
                    time.sleep(.05)
                prefix = prefixes()
                lx, ly, width, height = report["list"]
                cx, cy, cw, ch = lx + 8, ly + 8, width - 16, height - 16
                offset = report["offset"]
                assert report["extent"] == prefix[-1], (stage, report)
                assert 0 <= offset <= max(0, prefix[-1] - ch), (stage, report)
                assert report["live"] == len(report["rows"]), (stage, report)
                assert 1 <= report["live"] <= math.ceil(ch / 24) + 6, (stage, report)
                assert report["built"] <= 350, (stage, report)
                if focused is not None:
                    assert report["focused"] == focused, (stage, "focus", focused, report)
                for index, x, y, w, h in report["rows"]:
                    assert abs(x - cx) < .1 and abs(y - (cy + prefix[index] - offset)) < .1, (stage, index, "origin", report)
                    assert abs(w - cw) < .1 and abs(h - heights[index]) < .1, (stage, index, "size", report)
                first = bisect.bisect_right(prefix, offset) - 1
                last = bisect.bisect_left(prefix, offset + ch)
                actual_indices = [row[0] for row in report["rows"]]
                assert actual_indices == list(range(max(0, first - 2), min(len(heights), last + 2))), (stage, "mounted range", first, last, actual_indices)
                info = subprocess.check_output(["xwininfo", "-id", window], env=env, text=True)
                ox = int(re.search(r"Absolute upper-left X:\s+(-?\d+)", info)[1])
                oy = int(re.search(r"Absolute upper-left Y:\s+(-?\d+)", info)[1])
                picture = ImageGrab.grab(xdisplay=display).convert("RGB")
                picture.save(args.output / (stage + ".png"))
                # Sample the solid row area before the inset text and away from the scrollbar.
                pixels = []
                for screen_y in range(math.ceil(cy), math.floor(cy + ch)):
                    index = min(len(heights) - 1, bisect.bisect_right(prefix, offset + screen_y - cy + .5) - 1)
                    expected = (32, 48, 64) if index % 2 == 0 else (48, 64, 80)
                    actual = picture.getpixel((ox + round(cx + 8), oy + screen_y))
                    assert actual == expected, (stage, "row pixel", index, screen_y, actual, expected)
                    pixels.append(actual)
                focused_row = next((row for row in report["rows"] if row[0] == report["focused"]), None)
                if focused_row:
                    _, x, y, _, h = focused_row
                    for screen_y in range(math.ceil(max(cy, y)), math.floor(min(cy + ch, y + h))):
                        actual = picture.getpixel((ox + round(x + 1), oy + screen_y))
                        assert actual == (94, 165, 255), (stage, "focus marker", screen_y, actual)
                report.update(stage=stage, checked_scanline_pixels=len(pixels), content=[cx, cy, cw, ch])
                samples.append(report)
                return report

            initial = snapshot("initial")
            assert initial["offset"] == 0
            key("End")
            end = snapshot("end", 99_999)
            assert end["offset"] == end["extent"] - end["content"][3]
            page_target = bisect.bisect_right(prefixes(), prefixes()[99_999] - end["content"][3]) - 1
            key("Prior")
            snapshot("page-up", page_target)
            key("Home")
            snapshot("home", 0)
            key("Next")
            page_down_target = bisect.bisect_right(prefixes(), initial["content"][3]) - 1
            snapshot("page-down", page_down_target)
            key("g")
            anchor = snapshot("anchor")
            assert anchor["offset"] == prefixes()[1000] + 8
            anchor_y = next(row[2] for row in anchor["rows"] if row[0] == 1000)
            key("v")
            heights[1000] += 40
            grown = snapshot("visible-growth")
            assert grown["offset"] == anchor["offset"]
            assert grown["built"] == anchor["built"], ("growth rebuilt retained rows", grown, anchor)
            key("a")
            heights[10] += 32
            above = snapshot("above-anchor-growth")
            assert above["offset"] == grown["offset"] + 32
            assert next(row[2] for row in above["rows"] if row[0] == 1000) == anchor_y
            assert above["built"] == grown["built"]
            key("s")
            deadline = time.monotonic() + 5
            while "chunks=4" not in (args.output / "application.log").read_text():
                assert time.monotonic() < deadline and app.poll() is None, "stream did not finish"
                time.sleep(.05)
            heights[1001] += 48
            streamed = snapshot("streamed-growth")
            assert streamed["chunks"] == 4 and streamed["offset"] == above["offset"]
            assert streamed["built"] == above["built"]
            xdo("windowsize", "--sync", window, 760, 560)
            time.sleep(.2)
            resized = snapshot("resized")
            assert resized["viewport"] == [760, 560]
            assert resized["content"][3] > streamed["content"][3]
            assert resized["offset"] == streamed["offset"]
            xdo("mousemove", "--window", window, 150, 150, "click", "--repeat", 5, "--delay", 25, 5)
            xdo("mousemove", "--window", window, 740, 10)
            time.sleep(.2)
            wheeled = snapshot("wheel-burst")
            assert wheeled["offset"] > resized["offset"]
            key("Home")
            snapshot("home-after-stream", 0)
            key("End")
            final = snapshot("end-after-stream", 99_999)
            assert final["offset"] == prefixes()[-1] - final["content"][3]
            key("Escape")
            app.wait(timeout=5)
            assert app.returncode == 0
            assert "VARIABLE_CLOSED" in (args.output / "application.log").read_text()
            (args.output / "results.json").write_text(json.dumps(dict(
                passed=True, backend="owned Xvfb / Openbox / native X11", row_count=100_000,
                height_source="explicit reactive heights; no automatic text measurement",
                checks=["prefix geometry", "bounded mounted rows", "Home/End/Page navigation",
                        "visible growth", "above-anchor preservation", "four streaming chunks",
                        "retained rows", "native resize", "wheel burst", "focus pixels"],
                stages=samples), indent=2) + "\n")
            print("Native variable-height virtual list checks passed")
        finally:
            for process in reversed(processes):
                stop(process)
            for log in logs:
                log.close()


if __name__ == "__main__":
    main()
