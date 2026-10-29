#!/usr/bin/env python3
"""MANUAL REVIEW: run with a person watching (docs/validation.md).

The Microsoft Japanese IME in zgx's editors: romaji typed compose kana
in the editor's preedit, Space converts (日本語, or whatever the IME has
learned to put first), and Enter commits the candidate it selected; while
converted, the clause it marks shows underlined thick, as the target.
Through Windows' Text Services Framework by default (`ZGX_TSF` unset,
removed if set: what an app gets), its display attributes marking the
clause; `--imm32` runs it through winit's IMM32 path (`ZGX_TSF=0`, the
opt-out), and `--force-tsf` with the framework forced (`ZGX_TSF=1`). It
prints which.

The Japanese IME needs its language's typing features installed for the
user (Settings, Time & language, Language & region, Japanese). Without them
this prints PENDING and exits 0, so a suite that runs it stays green. It
asks Windows for Japanese in its own window only and gives the window its
layout back after; the user's languages are not touched. It converts with
Space and commits with Enter, so a person watches it. Before every key, and
after the last, a change to the input methods' preferences stops it (put
back first), and so does a new window that isn't a classic candidate list
(`windows_input.guard_windows`), with a screenshot in `--out`; Windows 11's
input window, where its candidates show, isn't listed by window
enumeration: the guard finds it by hit-testing below the caret and stops on
anything in it but the candidate UI.

    cargo build -p zgx-desktop --example native_ime
    python scripts/windows_tsf_japanese_smoke.py target/debug/examples/native_ime.exe
    python scripts/windows_tsf_japanese_smoke.py target/debug/examples/native_ime.exe --imm32
"""
import argparse
import os
import pathlib
import re
import subprocess
import threading
import time

import windows_input as win

JAPANESE_TIP = "{03B5835F-F03C-411B-9CE2-AA23E1171E36}"
JAPANESE = 0x0411


def installed():
    tips = subprocess.check_output(
        ["powershell", "-NoProfile", "-NonInteractive", "-Command",
         "(Get-WinUserLanguageList | ForEach-Object { $_.InputMethodTips }) -join ','"],
        text=True).strip()
    return JAPANESE_TIP.lower() in tips.lower()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("binary", type=pathlib.Path)
    win.input_path_args(parser)
    parser.add_argument("--out", type=pathlib.Path, default=pathlib.Path("japanese-probe"))
    args = parser.parse_args()
    layout = win.loaded_layout(JAPANESE)
    if not installed() or layout is None:
        print("PENDING: the Microsoft Japanese IME is not installed for this user")
        return
    env = dict(os.environ, ZGX_SECONDS="600")
    mode = win.input_path(env, win.mode_of(args))
    # Opened by the example itself, through IMM32 or the framework's
    # compartments: no on/off key is sent (it could land on a prompt).
    opener = "--open-ime" if args.imm32 else "--open-tsf-ime"
    proc = subprocess.Popen([str(args.binary.resolve()), opener], env=env, stdout=subprocess.PIPE,
                            stderr=subprocess.STDOUT, text=True, encoding="utf-8")
    lines = []
    threading.Thread(target=lambda: lines.extend(proc.stdout), daemon=True).start()
    window = original = None
    # Stopped by the guard: the app and what it shows stay for a person.
    stopped = False
    try:
        window = win.wait_window(proc)
        time.sleep(1)
        win.activate(window)
        win.click(window, 304, 82)
        original = win.layout_of(window)
        # Japanese in this window only: the user's languages stay as they are.
        win.use_layout(window, layout)
        time.sleep(0.8)
        win.require_foreground(window)
        # From here every key sent is checked first, and once more after the last.
        guard = win.guard_windows(args.out, where=win.below_caret(window, lines))
        for key in "nihongo":
            win.key(key)
        win.key("space")
        time.sleep(0.6)
        # Two of the example's reports (every 500 ms) while converted.
        converted = len(lines)
        time.sleep(1.1)
        marked = [line for line in lines[converted:] if line.startswith("UNDERLINE first")]
        # What the field shows converted: the candidate the IME selected.
        shown = [line for line in lines[converted:] if line.startswith("DISPLAY first")]
        win.key("enter")
        time.sleep(0.6)
        guard()
        win.use_layout(window, original)
        win.close(window)
        proc.wait(timeout=15)
    except win.UnexpectedWindow:
        stopped = True
        print(f"STOPPED: left the app (pid {proc.pid}) and what it shows open for a person to look at")
        raise
    finally:
        if not stopped and proc.poll() is None:
            if window and original:
                win.user32.PostMessageW(window, win.WM_INPUTLANGCHANGEREQUEST, 0, original)
                time.sleep(0.3)
            proc.kill()
            proc.wait()
    final = next((line for line in lines if line.startswith("FINAL")), "")
    first = final.split("first=", 1)[1].split(" second=", 1)[0].strip('"') if "first=" in final else ""
    selected = re.search(r'DISPLAY first "(.*?)" Rect', shown[-1]).group(1) if shown else None
    # Converted (no romaji left) and committed as it was selected.
    if not first or re.search(r"[A-Za-z]", first) or first != selected:
        raise AssertionError(f"the selected candidate {selected!r} wasn't committed as converted: {final}\n"
                             f"{''.join(lines)}")
    heights = [float(m.group(1)) for line in marked if (m := re.search(r"height: ([\d.]+)", line))]
    if not any(h >= 2 for h in heights):
        raise AssertionError(f"the converted clause wasn't underlined thick (its display attribute): {marked}")
    path = mode
    print(f"PASS: Japanese IME through {path}: {final.strip()} (as selected), "
          f"the target clause underlined thick")


if __name__ == "__main__":
    main()
