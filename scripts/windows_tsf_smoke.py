#!/usr/bin/env python3
"""Windows' Text Services Framework in zgx's editors (`ZGX_TSF=1`), with no
input method beyond English: a text service inserts at the caret through the
text store, and keys typed after it reach the editor as before.

The emoji panel (Win+.) is a text service every Windows 10 and 11 install
has. Enter inserts the emoji it has selected; Escape closes it. The panel
inserts through the focused field's text store (`InsertTextAtSelection`),
so with the framework on, the emoji is typed into zgx's editor by zgx's
store. The keys typed after it check that the store hears of typing it
didn't do.

    cargo build -p zgx-desktop --example native_ime
    python scripts/windows_tsf_smoke.py target/debug/examples/native_ime.exe

The window is brought to the front and checked uncovered before each key
(a probe's keys go to the foreground window).
"""
import argparse
import os
import pathlib
import subprocess
import threading
import time

import windows_input as win


def emoji(text):
    """Whether `text` holds an emoji (the panel's are pictographs or symbols)."""
    return any(ord(c) >= 0x1F000 or 0x2600 <= ord(c) <= 0x27BF for c in text)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("binary", type=pathlib.Path)
    args = parser.parse_args()
    env = dict(os.environ, ZGX_SECONDS="20", ZGX_TSF="1")
    proc = subprocess.Popen([str(args.binary.resolve())], env=env, stdout=subprocess.PIPE,
                            stderr=subprocess.STDOUT, text=True, encoding="utf-8")
    lines = []
    threading.Thread(target=lambda: lines.extend(proc.stdout), daemon=True).start()
    try:
        window = win.wait_window(proc)
        time.sleep(1)
        win.activate(window)
        win.click(window, 304, 82)
        time.sleep(0.4)
        win.require_foreground(window)
        win.key("win+.")
        time.sleep(1.5)
        # The panel has the foreground now; Enter picks its selected emoji.
        win.key("enter")
        time.sleep(0.5)
        win.key("escape")
        time.sleep(0.8)
        win.activate(window)
        win.require_foreground(window)
        for key in "abc":
            win.key(key)
        time.sleep(0.6)
        win.close(window)
        proc.wait(timeout=15)
    finally:
        if proc.poll() is None:
            proc.kill()
            proc.wait()
    output = "".join(lines)
    final = next((line for line in lines if line.startswith("FINAL")), None)
    if final is None:
        raise AssertionError(f"no FINAL line:\n{output}")
    first = final.split("first=", 1)[1].split(" second=", 1)[0]
    if not emoji(first):
        raise AssertionError(f"the emoji panel's insertion didn't reach the editor: {final}")
    if not first.rstrip('"').endswith("abc"):
        raise AssertionError(f"typing after the insertion was lost: {final}")
    print(f"PASS: the emoji panel types through zgx's text store, and typing follows: {final.strip()}")


if __name__ == "__main__":
    main()
