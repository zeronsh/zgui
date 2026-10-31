#!/usr/bin/env python3
"""A window made hidden shows whole (`WindowOptions::visible`), through the
hidden_window example, on macOS:

- its present log (`ZGX_PRESENT_LOG`) has its first frame presented while
  hidden, nothing presented while hidden after it though its content keeps
  changing, and, as it shows, a frame presented before `window shown`: what
  changed while hidden is drawn before the window appears;
- `hidden_window_probe.swift` sees the window off screen until it asks to
  show, its first capture on screen holding the content (no blank one), and
  the app all but idle while hidden.

    cargo build --release -p zgx-desktop --example hidden_window
    python3 scripts/hidden_window_probe.py target/release/examples/hidden_window

Needs Screen Recording permission for the capture.
"""
import argparse
import json
import os
import pathlib
import subprocess
import tempfile


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("binary", type=pathlib.Path)
    parser.add_argument("--runs", type=int, default=3)
    args = parser.parse_args()
    binary = str(args.binary.resolve())
    env = {"HOME": os.environ["HOME"], "PATH": "/usr/bin:/bin", "ZGX_PRESENT_LOG": "1"}
    # One stream, in the order the app wrote it.
    log = subprocess.run([binary], env=env, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True,
                         timeout=60)
    lines = log.stdout.splitlines()
    order = [line for line in lines if "base present" in line or "window shown" in line or line.startswith("SHOW")]
    show = next(i for i, line in enumerate(order) if line.startswith("SHOW"))
    hidden = [line for line in order[:show] if "base present" in line]
    assert len(hidden) == 1, f"one frame presented while hidden, not {len(hidden)}: {order[:show]}"
    after = order[show + 1:]
    shown = next(i for i, line in enumerate(after) if "window shown" in line)
    assert any("base present" in line for line in after[:shown]), \
        f"no frame presented before it showed: {after[:shown + 1]}"
    print(f"ordering: 1 frame while hidden, {sum('base present' in l for l in after[:shown])} before it showed")
    helper = pathlib.Path(tempfile.gettempdir()) / "zgx-hidden-probe"
    source = pathlib.Path(__file__).with_name("hidden_window_probe.swift")
    subprocess.run(["swiftc", "-O", str(source), "-o", str(helper)], check=True)
    results = []
    for _ in range(args.runs):
        out = subprocess.run([str(helper), binary], env=env, capture_output=True, text=True, timeout=60)
        result = json.loads(out.stdout.strip().splitlines()[-1])
        print(result)
        assert not result["on_screen_before_show"], "on screen while hidden"
        assert result["flat_captures"] == 0, "a blank frame on screen"
        results.append(result)
    window = sorted(r["window_ms"] for r in results)[len(results) // 2]
    content = sorted(r["content_ms"] for r in results)[len(results) // 2]
    print(f"PASS: hidden until shown, whole as it showed (median {window:.0f} ms to on screen, "
          f"{content:.0f} ms to its first capture)")


if __name__ == "__main__":
    main()
