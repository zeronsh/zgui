#!/usr/bin/env python3
"""MANUAL REVIEW: run with a person watching (docs/validation.md).

Microsoft Pinyin (Simplified Chinese) composing in zgx's editors: "nihao"
typed composes pinyin in the editor's preedit, the digit 1 takes the first
candidate (你好) and commits it, and pinyin composed then cancelled with
Escape commits nothing. Through Windows' Text Services Framework
by default (`ZGX_TSF` unset, removed if set: what an app gets); `--imm32`
runs it through winit's IMM32 path (`ZGX_TSF=0`, the opt-out), and
`--force-tsf` with the framework forced (`ZGX_TSF=1`). It prints which.

Microsoft Pinyin must be installed for the user (Chinese (Simplified) in
Settings, Time & language, Language & region). Without it this prints
PENDING and exits 0, so a suite that runs it stays green. It never edits the
user's input languages: it asks Windows for Pinyin in its own window only
(WM_INPUTLANGCHANGEREQUEST) and gives the window its layout back after.

It answers no prompt. Pinyin can offer a first-use choice (Bing's cloud
suggestions) that Space or Enter would accept, so the probe never sends
them: a digit takes the candidate. Before every key, and after the last, a
change to the input methods' preferences stops it (put back first), and so
does a new window that isn't a classic candidate list
(`windows_input.guard_windows`), with a screenshot in `--out`. Windows 11
draws Pinyin's candidates and offers in an input window that window
enumeration doesn't list: the guard finds it by hit-testing below the caret
and stops on anything in it but the candidate UI. Look at the screenshots
too.

    cargo build -p zgx-desktop --example native_ime
    python scripts/windows_tsf_chinese_smoke.py target/debug/examples/native_ime.exe
    python scripts/windows_tsf_chinese_smoke.py target/debug/examples/native_ime.exe --imm32
"""
import argparse
import os
import pathlib
import subprocess
import threading
import time

import windows_input as win
from windows_ime_smoke import input_methods

CHINESE = 0x0804
NIHAO = "\u4f60\u597d"


def model(lines, field):
    """The field's text in the example's last report."""
    for line in reversed(lines):
        if line.startswith("MODEL ") and f" {field}=" in line:
            return line.split(f" {field}=", 1)[1].split(" ", 1)[0].strip('"')
    return None


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("binary", type=pathlib.Path)
    win.input_path_args(parser)
    parser.add_argument("--out", type=pathlib.Path, default=pathlib.Path("pinyin-probe"))
    args = parser.parse_args()
    layout = win.loaded_layout(CHINESE)
    if layout is None:
        print("PENDING: Microsoft Pinyin is not installed for this user")
        return
    saved = input_methods()
    env = dict(os.environ, ZGX_SECONDS="600")
    mode = win.input_path(env, win.mode_of(args))
    proc = subprocess.Popen([str(args.binary.resolve())], env=env, stdout=subprocess.PIPE,
                            stderr=subprocess.STDOUT, text=True, encoding="utf-8")
    lines = []
    reader = threading.Thread(target=lambda: lines.extend(proc.stdout), daemon=True)
    reader.start()
    window = original = None
    # Stopped by the guard: the app and what it shows stay for a person.
    stopped = False
    try:
        window = win.wait_window(proc)
        time.sleep(1)
        win.activate(window)
        win.click(window, 304, 82)
        original = win.layout_of(window)
        # Pinyin in this window only: the user's languages stay as they are.
        win.use_layout(window, layout)
        time.sleep(0.8)
        # From here every key sent is checked first, and once more after the last.
        guard = win.guard_windows(args.out, where=win.below_caret(window, lines))
        for key in "nihao":
            win.key(key, delay=0.12)
        time.sleep(1.2)
        win.save_png(win.capture(window), str(args.out / "candidates.png"))
        # The first candidate by its digit: never Space or Enter.
        win.key("1")
        deadline = time.monotonic() + 3
        while model(lines, "first") != NIHAO:
            if time.monotonic() > deadline:
                raise AssertionError(f"你好 wasn't committed: first={model(lines, 'first')!r}\n{''.join(lines)}")
            time.sleep(0.1)
        # Pinyin composed, then cancelled: nothing of it committed.
        for key in "ceshi":
            win.key(key, delay=0.12)
        time.sleep(0.8)
        win.key("escape", "escape")
        time.sleep(0.8)
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
        reader.join(timeout=2)
        args.out.mkdir(parents=True, exist_ok=True)
        (args.out / "child.log").write_text("".join(lines), encoding="utf-8")
        if input_methods() != saved:
            raise AssertionError("the user's input languages changed")
    output = "".join(lines)
    if proc.returncode != 0:
        raise AssertionError(f"the app ended with {proc.returncode}:\n{output}")
    final = next((line.strip() for line in lines if line.startswith("FINAL")), "")
    if final != f'FINAL first="{NIHAO}" second=""':
        raise AssertionError(f"expected 你好 alone: {final}\n{output}")
    path = mode
    print(f"PASS: Microsoft Pinyin through {path}: {final}")


if __name__ == "__main__":
    main()
