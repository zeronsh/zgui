#!/usr/bin/env python3
"""Where windows open on Windows (`window_place/win32.rs`), through
`window_probe`, which moves only its own windows:

- a second window with no position opens a step on from the first, as
  Windows cascades (`CW_USEDEFAULT`);
- a window with `frame_autosave`, moved and closed, reopens there at its
  size; left maximized, it reopens maximized; at a saved frame no monitor
  shows, it opens on a monitor instead;
- a window opened `under_pointer` has that point of its content under the
  pointer;
- nothing is left in the registry (the probe removes the frame it saved).

    cargo build -p zgx-desktop --example window_probe
    python scripts/windows_window_smoke.py target/debug/examples
"""
import argparse
import pathlib
import re
import subprocess
import winreg


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("examples", type=pathlib.Path)
    args = parser.parse_args()
    result = subprocess.run([str(args.examples / "window_probe.exe")], capture_output=True, text=True,
                            timeout=120)
    out = result.stdout
    print(out, end="")

    def need(pattern, what):
        match = re.search(pattern, out, re.M)
        if not match:
            raise AssertionError(f"{what}: missing {pattern!r} in:\n{out}")
        return match

    cascade = need(r"^CASCADE dx=(-?\d+) dy=(-?\d+)$", "the cascade")
    dx, dy = int(cascade.group(1)), int(cascade.group(2))
    # A step on, or (wrapped round, as Windows' cascade does) elsewhere, but
    # never on top of the first.
    assert (dx, dy) != (0, 0), "the second window opened over the first"
    moved = need(r"^AUTOSAVE moved_to=\((-?\d+), (-?\d+)\) reopened_at=\((-?\d+), (-?\d+)\) size_kept=(\w+)$",
                 "the saved frame")
    assert moved.group(1, 2) == moved.group(3, 4), f"reopened elsewhere: {moved.group(0)}"
    assert moved.group(5) == "true", "the saved size was not kept"
    need(r"^AUTOSAVE_MAXIMIZED true$", "reopening maximized")
    need(r"^AUTOSAVE_OFFSCREEN on_a_monitor=true$", "a saved frame no monitor shows")
    need(r"^UNDER_POINTER off_by=\(0, 0\)$", "the point under the pointer")
    try:
        winreg.OpenKey(winreg.HKEY_CURRENT_USER, r"Software\zgx\window_probe")
        raise AssertionError("the probe left its frames key in the registry")
    except FileNotFoundError:
        pass
    print("PASS: cascade, saved frames (moved, maximized, off every monitor), under the pointer")


if __name__ == "__main__":
    main()
