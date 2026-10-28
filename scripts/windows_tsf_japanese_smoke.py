#!/usr/bin/env python3
"""PENDING until the Japanese typing features are installed on the test PC.

The Microsoft Japanese IME through Windows' Text Services Framework in zgx's
editors (`ZGX_TSF=1`): romaji typed compose kana in the editor's preedit,
Space converts (日本語), Enter commits through the text store; while
converted, the clause it marks (its display attribute) shows underlined
thick, as the target. The next stages check more here: the candidate window
placed under the target clause, and reconversion (select committed text,
press Convert).

The Japanese IME needs its language's typing features, an administrator
install (Settings, Time & language, Language & region, Japanese). Without
them this prints PENDING and exits 0, so a suite that runs it stays green.

    cargo build -p zgx-desktop --example native_ime
    python scripts/windows_tsf_japanese_smoke.py target/debug/examples/native_ime.exe
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


def installed():
    tips = subprocess.check_output(
        ["powershell", "-NoProfile", "-NonInteractive", "-Command",
         "(Get-WinUserLanguageList | ForEach-Object { $_.InputMethodTips }) -join ','"],
        text=True).strip()
    return JAPANESE_TIP.lower() in tips.lower()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("binary", type=pathlib.Path)
    args = parser.parse_args()
    if not installed():
        print("PENDING: the Microsoft Japanese IME is not installed for this user")
        return
    env = dict(os.environ, ZGX_SECONDS="30", ZGX_TSF="1")
    proc = subprocess.Popen([str(args.binary.resolve()), "--open-ime"], env=env, stdout=subprocess.PIPE,
                            stderr=subprocess.STDOUT, text=True, encoding="utf-8")
    lines = []
    threading.Thread(target=lambda: lines.extend(proc.stdout), daemon=True).start()
    try:
        window = win.wait_window(proc)
        time.sleep(1)
        win.activate(window)
        win.click(window, 304, 82)
        time.sleep(0.5)
        win.require_foreground(window)
        for key in "nihongo":
            win.key(key)
        win.key("space")
        time.sleep(0.6)
        # Two of the example's reports (every 500 ms) while converted.
        converted = len(lines)
        time.sleep(1.1)
        marked = [line for line in lines[converted:] if line.startswith("UNDERLINE first")]
        win.key("enter")
        time.sleep(0.6)
        win.close(window)
        proc.wait(timeout=15)
    finally:
        if proc.poll() is None:
            proc.kill()
            proc.wait()
    final = next((line for line in lines if line.startswith("FINAL")), "")
    if "日本語" not in final:
        raise AssertionError(f"日本語 wasn't converted and committed through the text store: {final}\n{''.join(lines)}")
    heights = [float(m.group(1)) for line in marked if (m := re.search(r"height: ([\d.]+)", line))]
    if not any(h >= 2 for h in heights):
        raise AssertionError(f"the converted clause wasn't underlined thick (its display attribute): {marked}")
    print(f"PASS: Japanese IME through the Text Services Framework: {final.strip()}, "
          f"the target clause underlined thick")


if __name__ == "__main__":
    main()
