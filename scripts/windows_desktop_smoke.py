#!/usr/bin/env python3
"""Exercise the gallery's native Win32 input/clipboard bridge (desktop_smoke.py's Windows twin).

Run on an unlocked interactive desktop; it takes the keyboard and pointer for a few seconds:
    cargo build -p zgui-desktop --example gallery
    python scripts/windows_desktop_smoke.py target/debug/examples/gallery.exe
"""
import argparse
import os
import pathlib
import subprocess
import time

import windows_input as win


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("binary", type=pathlib.Path)
    parser.add_argument("--screenshot", type=pathlib.Path)
    args = parser.parse_args()
    env = dict(os.environ, ZGUI_SECONDS="15", ZGUI_SNAPSHOT="1")
    proc = subprocess.Popen([str(args.binary.resolve())], env=env, stdout=subprocess.PIPE,
                            stderr=subprocess.STDOUT, text=True, encoding="utf-8")
    try:
        window = win.wait_window(proc)
        scale = win.dpi(window) / 96
        expected_size = (round(900 * scale), round(760 * scale))
        actual_size = win.client_size(window)
        if actual_size != expected_size:
            raise AssertionError(f"initial size at {scale:g}x: expected {expected_size}, got {actual_size}")
        win.activate(window)
        win.click(window, 60 * scale, 126 * scale)
        win.click(window, 80 * scale, 430 * scale)
        win.key("ctrl+a")
        win.type_text("Native bridge")
        win.key("ctrl+a", "ctrl+c", "tab", "ctrl+a", "ctrl+v")
        # A resize checks the native logical/physical viewport handoff too.
        win.resize_client(window, round(1000 * scale), round(800 * scale))
        time.sleep(.3)
        if args.screenshot:
            win.save_png(win.capture(window), args.screenshot)
        win.close(window)
        output, _ = proc.communicate(timeout=25)
        if proc.returncode:
            raise RuntimeError(output)
        for expected in ['SEMANTIC "Count: 1"', 'SEMANTIC "Name" Some("Native bridge")',
                         'SEMANTIC "Notes" Some("Native bridge")', 'VIEWPORT (1000.0, 800.0)']:
            if expected not in output:
                raise AssertionError(f"missing native interaction result {expected!r}:\n{output}")
        print(f"PASS: Windows scale={scale:g}, initial physical size={actual_size}, native pointer "
              "activation, text entry, selection, clipboard, Tab focus, resize, close")
    finally:
        if proc.poll() is None:
            proc.kill()
            proc.wait()


if __name__ == "__main__":
    main()
