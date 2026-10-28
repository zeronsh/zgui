#!/usr/bin/env python3
"""Native IME composition on Windows (native_ime_smoke.py's twin), through the
Microsoft Korean IME: real keystrokes compose hangul syllables in zgx's
editor, commit, and a click into the other editor mid-composition leaves
nothing of it behind there.

The Korean IME composes without a dictionary, so it works on any Windows
install. It must be in the user's input methods; `--install-korean` adds it
for the run and restores the user's list after. native_ime --open-ime opens it
(input methods ignore a synthesized on/off key); the keystrokes are real.

    cargo build -p zgx-desktop --example native_ime
    python scripts/windows_ime_smoke.py target/debug/examples/native_ime.exe --install-korean

`--tsf` runs it through Windows' Text Services Framework (`ZGX_TSF=1`): the
composition is the editor's preedit through zgx's text store, not IMM32's
events, so the editors' text is what is checked: 한글 committed into the
first, nothing in the second.
"""
import argparse
import ctypes
import os
import pathlib
import subprocess
import threading
import time
from ctypes import wintypes

import windows_input as win

KOREAN_TIP = "0412:{A028AE76-01B1-46C2-99C4-ACD9858AE02F}{B5FE1F02-D5F2-4445-9C03-C568F23C99A1}"
user32 = ctypes.WinDLL("user32")
user32.GetKeyboardLayout.restype = wintypes.HANDLE
user32.GetWindowThreadProcessId.argtypes = [wintypes.HWND, ctypes.c_void_p]


def powershell(script):
    return subprocess.check_output(["powershell", "-NoProfile", "-NonInteractive", "-Command", script],
                                   text=True).strip()


def input_methods():
    # Indexed: piped or enumerated, PowerShell hands over the list as one object.
    return powershell("$l = Get-WinUserLanguageList; $(for ($i = 0; $i -lt $l.Count; $i++) "
                      "{ $l[$i].LanguageTag + '=' + ($l[$i].InputMethodTips -join ',') }) -join ';'")


def restore(saved):
    # Rebuild the list exactly as it was, tags and input methods in order.
    entries = [entry.split("=", 1) for entry in saved.split(";") if entry]
    script = ["$l = New-WinUserLanguageList " + entries[0][0], "$l[0].InputMethodTips.Clear()"]
    for index, (tag, tips) in enumerate(entries):
        if index:
            script += [f"$l.Add('{tag}')", f"$l[{index}].InputMethodTips.Clear()"]
        script += [f"$l[{index}].InputMethodTips.Add('{tip}')" for tip in tips.split(",") if tip]
    script.append("Set-WinUserLanguageList $l -Force -WarningAction SilentlyContinue")
    powershell("; ".join(script))


def finish_replaced(proc, lines):
    """What the app's edit mid-composition left: its text, none of the
    composition, and the app alive to the end."""
    output = "".join(lines)
    if proc.returncode != 0:
        raise AssertionError(f"the app ended with {proc.returncode}:\n{output}")
    final = next((line for line in lines if line.startswith("FINAL")), "")
    first = final.split("first=", 1)[1].split(" second=", 1)[0].strip('"') if "first=" in final else ""
    replaced = next(line for line in lines if line.startswith("REPLACED while composing"))
    if '"안"' not in replaced:
        raise AssertionError(f"the app replaced the text before 안 was composed: {replaced}\n{output}")
    # The space after the cancellation types a space: nothing of 안.
    if first != "REPLACED ":
        raise AssertionError(f"the cancelled composition came back: {final}\n{output}")
    print(f"PASS: the app's edit ended the composition, none of it committed: {final.strip()}")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("binary", type=pathlib.Path)
    parser.add_argument("--install-korean", action="store_true")
    parser.add_argument("--tsf", action="store_true", help="through the Text Services Framework")
    parser.add_argument("--replace-while-composing", action="store_true",
                        help="the app sets the field's text mid-composition: the composition ends, "
                             "none of it committed")
    args = parser.parse_args()
    saved = input_methods()
    if KOREAN_TIP.split(":")[1] not in saved:
        if not args.install_korean:
            raise SystemExit("the Microsoft Korean IME is not installed for this user: "
                             "add it in Settings, or pass --install-korean")
        powershell("$l = Get-WinUserLanguageList; $l.Add('ko-KR'); $k = $l[$l.Count - 1]; "
                   f"$k.InputMethodTips.Clear(); $k.InputMethodTips.Add('{KOREAN_TIP}'); "
                   "Set-WinUserLanguageList $l -Force -WarningAction SilentlyContinue")
    env = dict(os.environ, ZGX_SECONDS="30")
    if args.tsf:
        env["ZGX_TSF"] = "1"
    command = [str(args.binary.resolve()), "--open-ime"]
    if args.replace_while_composing:
        command.append("--replace-while-composing")
    proc = subprocess.Popen(command, env=env, stdout=subprocess.PIPE,
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
        # Through the framework, IMM32's context isn't the window's, so
        # `--open-ime` can't open it: the Korean input method starts in its
        # English mode, and its Hangul key opens it.
        if args.tsf:
            win.require_foreground(window)
            win.key("hangul")
            time.sleep(0.3)
        deadline = time.monotonic() + 5
        while not args.tsf and "IME_OPEN true\n" not in lines:
            if time.monotonic() > deadline:
                raise RuntimeError("the input method did not open")
            time.sleep(0.1)
        if args.replace_while_composing:
            # 안, held composing until the app replaces the text (its 500 ms
            # report), then a key the input method takes again.
            win.require_foreground(window)
            for key in "dks":
                win.key(key)
            deadline = time.monotonic() + 6
            while not any(line.startswith("REPLACED while composing") for line in lines):
                if time.monotonic() > deadline:
                    raise RuntimeError("the app never saw the composition to replace")
                time.sleep(0.1)
            time.sleep(0.6)
            win.key("space")
            time.sleep(0.4)
            win.close(window)
            proc.wait(timeout=15)
            return finish_replaced(proc, lines)
        # 한글: two syllables composed from six keys, committed by a space.
        win.require_foreground(window)
        for key in "gksrmf":
            win.key(key)
        win.key("space")
        time.sleep(0.4)
        # A syllable still composing when the other editor is clicked.
        for key in "dks":
            win.key(key)
        time.sleep(0.3)
        win.require_foreground(window)
        win.click(window, 304, 186)
        time.sleep(0.4)
        win.key("tab")
        win.close(window)
        proc.wait(timeout=15)
    finally:
        if proc.poll() is None:
            proc.kill()
            proc.wait()
        if input_methods() != saved:
            restore(saved)
    output = "".join(lines)
    final = next(line for line in lines if line.startswith("FINAL"))
    if args.tsf:
        first = final.split("first=", 1)[1].split(" second=", 1)[0]
        if "한글" not in first:
            raise AssertionError(f"the composition didn't commit through the text store: {final}\n{output}")
        if "second=\"\"" not in final:
            raise AssertionError(f"the composition followed focus into the other editor: {final}")
        print(f"PASS: Korean IME through the Text Services Framework: {final.strip()}")
        return
    # The caret after the composing syllable, as in Windows' own fields.
    for expected in ['PREEDIT first "ㅎ" Some((3, 3))', 'PREEDIT first "한" Some((3, 3))', 'COMMIT first "한"',
                     'COMMIT first "글"', 'PREEDIT first "안"']:
        if expected not in output:
            raise AssertionError(f"missing {expected!r}:\n{output}")
    if "second=\"\"" not in final or "ㅇ" in final.split("second=")[1]:
        raise AssertionError(f"the composition followed focus into the other editor: {final}")
    print(f"PASS: Korean IME preedit and commit, focus change mid-composition: {final.strip()}")


if __name__ == "__main__":
    main()
