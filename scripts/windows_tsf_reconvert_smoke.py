#!/usr/bin/env python3
"""Reconversion through Windows' Text Services Framework (`ZGX_TSF=1`): the
Microsoft Korean IME converts committed hangul, selected, to hanja. Typed 한
is committed; selected and given the Hanja key, the input method opens its
composition over it (zgx's text store makes that text the composition in its
place), and Enter takes the first candidate, which replaces it. The Japanese
IME's Convert key reconverts the same way (windows_tsf_japanese_smoke.py,
once its language is installed).

The Korean IME composes without a dictionary of its own and converts with
Windows' hanja one, so it works on any install; `--install-korean` adds it
for the run and restores the user's list after, as windows_ime_smoke.py does.

    cargo build -p zgx-desktop --example native_ime
    python scripts/windows_tsf_reconvert_smoke.py target/debug/examples/native_ime.exe --install-korean
"""
import argparse
import os
import pathlib
import subprocess
import threading
import time

import windows_input as win
from windows_ime_smoke import KOREAN_TIP, input_methods, powershell, restore, user32


def hanja(text):
    return any(0x4E00 <= ord(c) <= 0x9FFF or 0xF900 <= ord(c) <= 0xFAFF for c in text)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("binary", type=pathlib.Path)
    parser.add_argument("--install-korean", action="store_true")
    args = parser.parse_args()
    saved = input_methods()
    if KOREAN_TIP.split(":")[1] not in saved:
        if not args.install_korean:
            raise SystemExit("the Microsoft Korean IME is not installed for this user: "
                             "add it in Settings, or pass --install-korean")
        powershell("$l = Get-WinUserLanguageList; $l.Add('ko-KR'); $k = $l[$l.Count - 1]; "
                   f"$k.InputMethodTips.Clear(); $k.InputMethodTips.Add('{KOREAN_TIP}'); "
                   "Set-WinUserLanguageList $l -Force -WarningAction SilentlyContinue")
    env = dict(os.environ, ZGX_SECONDS="30", ZGX_TSF="1")
    proc = subprocess.Popen([str(args.binary.resolve())], env=env, stdout=subprocess.PIPE,
                            stderr=subprocess.STDOUT, text=True, encoding="utf-8")
    lines = []
    threading.Thread(target=lambda: lines.extend(proc.stdout), daemon=True).start()
    try:
        window = win.wait_window(proc)
        time.sleep(1)
        win.activate(window)
        win.click(window, 304, 82)
        thread = user32.GetWindowThreadProcessId(window, None)
        for _ in range(8):
            if (user32.GetKeyboardLayout(thread) or 0) & 0xFFFF == 0x0412:
                break
            win.require_foreground(window)
            win.key("win+space")
            time.sleep(0.8)
            win.key("escape")
            time.sleep(0.3)
        else:
            raise RuntimeError("could not switch the window to the Korean IME")
        win.require_foreground(window)
        win.key("hangul")
        time.sleep(0.3)
        # 한, committed by a space; then 한 alone selected.
        for key in "gks":
            win.key(key)
        win.key("space")
        time.sleep(0.4)
        win.key("left", "shift+left")
        time.sleep(0.3)
        committed = len(lines)
        # Hanja: the input method reconverts the selection; Enter takes its
        # first candidate.
        win.require_foreground(window)
        win.key("hanja")
        time.sleep(1.0)
        reconverting = [line for line in lines[committed:] if line.startswith(("UNDERLINE first", "DISPLAY first"))]
        win.key("enter")
        time.sleep(0.6)
        win.close(window)
        proc.wait(timeout=15)
    finally:
        if proc.poll() is None:
            proc.kill()
            proc.wait()
        if input_methods() != saved:
            restore(saved)
    output = "".join(lines)
    final = next((line for line in lines if line.startswith("FINAL")), "")
    first = final.split("first=", 1)[1].split(" second=", 1)[0].strip('"') if "first=" in final else ""
    if not hanja(first) or "한" in first:
        raise AssertionError(f"한 wasn't reconverted to hanja in place: {final}\n"
                             f"while reconverting: {reconverting}\n{output}")
    if not first.endswith(" ") or len(first.strip()) != 1:
        raise AssertionError(f"more or less than the selection was replaced: {final}")
    print(f"PASS: Korean hanja reconversion through the Text Services Framework: {final.strip()}")


if __name__ == "__main__":
    main()
