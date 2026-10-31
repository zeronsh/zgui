#!/usr/bin/env python3
"""Native services on Windows, through other processes and the system's own
dialogs: the clipboard (selected display text copied as text and as HTML,
text from another process pasted in), the file picker, the save dialog and
the message box (each owned by its window), and a file dragged in from another process.

    cargo build -p zgx-desktop --example selectable_text --example gallery --example platform_services --example drag_drop
    python scripts/windows_platform_smoke.py target/debug/examples
"""
import argparse
import ctypes
import os
import pathlib
import re
import subprocess
import tempfile
import threading
import time
from ctypes import wintypes

import windows_input as win

user32 = ctypes.WinDLL("user32")
user32.GetWindow.restype = wintypes.HWND
user32.GetWindow.argtypes = [wintypes.HWND, wintypes.UINT]


def powershell(script):
    return subprocess.run(["powershell", "-NoProfile", "-NonInteractive", "-STA", "-Command", script],
                          capture_output=True, text=True, encoding="utf-8").stdout


def clipboard(kind):
    """The clipboard's text in .NET's `TextDataFormat` `kind` (Text, Html)."""
    return powershell("Add-Type -AssemblyName System.Windows.Forms; [Console]::OutputEncoding = "
                      "[Text.Encoding]::UTF8; [Windows.Forms.Clipboard]::GetText("
                      f"[Windows.Forms.TextDataFormat]::{kind})")


def elements(window):
    """(control type, name, client rect) for each element UI Automation shows."""
    handle = getattr(window, "value", window)
    dump = subprocess.run(["powershell", "-NoProfile", "-ExecutionPolicy", "Bypass", "-File",
                           str(pathlib.Path(__file__).with_name("windows_uia_dump.ps1")),
                           "-Hwnd", str(handle), "-Rects"], capture_output=True, text=True, encoding="utf-8").stdout
    ox, oy = win.to_screen(window, 0, 0)
    found = []
    for line in dump.splitlines():
        match = re.match(r'\s*(\w+) name=("(?:[^"\\]|\\.)*").* rect=(-?\d+),(-?\d+),(\d+),(\d+)', line)
        if match:
            kind, name, x, y, w, h = match.groups()
            found.append((kind, name.strip('"'), (int(x) - ox, int(y) - oy, int(w), int(h))))
    return found


def click_named(window, name):
    for _, element, (x, y, w, h) in elements(window):
        if element == name:
            win.click(window, x + w / 2, y + h / 2)
            return
    raise AssertionError(f"no element named {name!r}")


def launch(binary, *args, env=None):
    proc = subprocess.Popen([str(binary), *args], stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True,
                            encoding="utf-8", env=dict(os.environ, **(env or {})))
    lines = []
    threading.Thread(target=lambda: lines.extend(proc.stdout), daemon=True).start()
    return proc, lines


def wait_for(check, what, timeout=10.0):
    deadline = time.monotonic() + timeout
    while True:
        result = check()
        if result:
            return result
        if time.monotonic() > deadline:
            raise AssertionError(f"timed out waiting for {what}")
        time.sleep(0.1)


def dialog_of(pid, previous):
    """A new top-level dialog (#32770) of process `pid`."""
    for hwnd in win.windows_of(pid):
        name = ctypes.create_unicode_buffer(64)
        user32.GetClassNameW(wintypes.HWND(hwnd), name, 64)
        if name.value == "#32770" and hwnd not in previous:
            return hwnd
    return None


def owned(dialog, window):
    """A dialog must be the window's own: above it, and the window disabled
    (modal) while it shows."""
    owner = user32.GetWindow(wintypes.HWND(dialog), 4)  # GW_OWNER
    if owner != getattr(window, "value", window):
        raise AssertionError(f"the dialog's owner is {owner}, not the window {window}")
    if user32.IsWindowEnabled(wintypes.HWND(getattr(window, "value", window))):
        raise AssertionError("the window takes input while its dialog shows")


def stop(proc):
    if proc.poll() is None:
        proc.kill()
        proc.wait()


def check_clipboard(examples):
    # Display text out: selected, copied, read by another process.
    proc, _ = launch(examples / "selectable_text.exe")
    try:
        window = win.wait_window(proc)
        time.sleep(1)
        win.activate(window)
        win.click(window, 200, 300)
        win.key("ctrl+a", "ctrl+c")
        time.sleep(0.4)
        text, html = clipboard("Text"), clipboard("Html")
        if "Drag across this paragraph" not in text or "line 1)" not in text:
            raise AssertionError(f"copied text is incomplete: {text[:200]!r}")
        if "<" not in html or "Drag across this paragraph" not in html:
            raise AssertionError(f"no HTML beside the text: {html[:300]!r}")
        win.close(window)
    finally:
        stop(proc)
    # Text in: set by another process, pasted into an editor.
    powershell("Set-Clipboard -Value 'pasted from another process'")
    proc, lines = launch(examples / "gallery.exe", env={"ZGX_SNAPSHOT": "1"})
    try:
        window = win.wait_window(proc)
        time.sleep(1)
        win.activate(window)
        win.click(window, 80, 430)
        win.key("ctrl+a", "ctrl+v")
        time.sleep(0.3)
        win.close(window)
        proc.wait(timeout=10)
        time.sleep(0.2)
        if 'SEMANTIC "Name" Some("pasted from another process")' not in "".join(lines):
            raise AssertionError("the paste did not reach the editor:\n" + "".join(lines)[-1500:])
    finally:
        stop(proc)
    return f"copy {len(text)} chars + HTML ({len(html)} bytes), paste from another process"


def check_dialogs(examples):
    with tempfile.TemporaryDirectory(prefix="zgx-dialogs-") as directory:
        fixture = pathlib.Path(directory, "fixture.txt")
        fixture.write_text("zgx")
        proc, lines = launch(examples / "platform_services.exe", "--smoke",
                             env={"ZGX_DIALOG_DIRECTORY": directory})
        try:
            window = win.wait_window(proc)
            # --smoke opens the file picker after a second.
            picker = wait_for(lambda: dialog_of(proc.pid, [window]), "the file picker")
            owned(picker, window)
            # The picker shows before its shell view is ready for typing.
            time.sleep(2)
            win.activate(picker)
            win.type_text(str(fixture))
            picked = lambda: next((l for l in lines if l.startswith("DIALOG open")), None)
            # The first Enter can go to the name box's suggestions instead.
            for _ in range(3):
                if picked() or not win.is_foreground(picker):
                    break
                win.key("enter")
                time.sleep(1.5)
            opened = wait_for(picked, "the picked file")
            if "fixture.txt" not in opened:
                raise AssertionError(f"the picker returned {opened!r}")
            win.activate(window)
            click_named(window, "Confirm action")
            box = wait_for(lambda: dialog_of(proc.pid, [window]), "the message box")
            owned(box, window)
            time.sleep(0.5)
            win.activate(box)
            win.key("enter")
            prompt = wait_for(lambda: next((l for l in lines if l.startswith("PROMPT")), None), "the prompt result")
            win.activate(window)
            win.close(window)
            proc.wait(timeout=10)
        finally:
            stop(proc)
    return f"{opened.strip()}, {prompt.strip()}"


def check_save_dialog(examples):
    """The save dialog, owned by its window: a name typed into it reaches the
    app as the path to save to."""
    with tempfile.TemporaryDirectory(prefix="zgx-save-") as directory:
        target = pathlib.Path(directory, "saved-by-smoke.txt")
        proc, lines = launch(examples / "platform_services.exe", "--smoke",
                             env={"ZGX_DIALOG_DIRECTORY": directory, "ZGX_DIALOG_ACTION": "save"})
        try:
            window = win.wait_window(proc)
            dialog = wait_for(lambda: dialog_of(proc.pid, [window]), "the save dialog")
            owned(dialog, window)
            time.sleep(2)
            win.activate(dialog)
            # Its name box starts with the suggested name, selected.
            win.key("ctrl+a")
            win.type_text(str(target))
            saved = lambda: next((l for l in lines if l.startswith("DIALOG save")), None)
            for _ in range(3):
                if saved() or not win.is_foreground(dialog):
                    break
                win.key("enter")
                time.sleep(1.5)
            result = wait_for(saved, "the save dialog's path")
            if "saved-by-smoke.txt" not in result:
                raise AssertionError(f"the save dialog returned {result!r}")
            win.activate(window)
            win.close(window)
            proc.wait(timeout=10)
        finally:
            stop(proc)
    return result.strip()


DRAG_SOURCE = r"""
Add-Type -AssemblyName System.Windows.Forms
$form = New-Object Windows.Forms.Form
$form.StartPosition = 'Manual'
$form.Location = New-Object Drawing.Point(40, 40)
$form.Size = New-Object Drawing.Size(240, 140)
$form.TopMost = $true
$form.Text = 'zgx drag source'
$form.Add_MouseDown({
    $data = New-Object Windows.Forms.DataObject
    $files = New-Object Collections.Specialized.StringCollection
    [void]$files.Add($env:ZGX_DRAG_FILE)
    $data.SetFileDropList($files)
    $result = $form.DoDragDrop($data, [Windows.Forms.DragDropEffects]::Copy)
    [Console]::WriteLine("DRAG_RESULT $result")
    $form.Close()
})
$form.Add_Shown({ [Console]::WriteLine("SOURCE_READY") })
[Windows.Forms.Application]::Run($form)
"""


def check_file_drop(examples):
    """A file dragged in from another process (OLE drag and drop, as from
    Explorer) onto drag_drop's drop zone. The app last sees the pointer over
    a card, outside the zone: an OLE drag sends it no mouse moves, so the
    file must drop where the drag lets go, not where the pointer was."""
    with tempfile.TemporaryDirectory(prefix="zgx-drop-") as directory:
        dropped = pathlib.Path(directory, "dropped.txt")
        dropped.write_text("zgx")
        proc, lines = launch(examples / "drag_drop.exe")
        source, source_lines = None, []
        try:
            window = win.wait_window(proc)
            time.sleep(1)
            win.activate(window)
            # Last seen over the Red card: a drop at the stale pointer misses.
            scale = win.dpi(window) / 96
            win.move(window, 70 * scale, 110 * scale)
            time.sleep(0.3)
            source = subprocess.Popen(["powershell", "-NoProfile", "-NonInteractive", "-STA", "-Command",
                                       DRAG_SOURCE], stdout=subprocess.PIPE, text=True,
                                      env=dict(os.environ, ZGX_DRAG_FILE=str(dropped)))
            threading.Thread(target=lambda: source_lines.extend(source.stdout), daemon=True).start()
            wait_for(lambda: any(l.strip() == "SOURCE_READY" for l in source_lines), "the drag source", timeout=20)
            form = wait_for(lambda: next((w for w in win.windows_of(source.pid)
                                          if win.title(w) == "zgx drag source"), None), "its window")
            win.activate(form)
            win.drag(form, (60, 40), win.to_client(form, *win.to_screen(window, 190 * scale, 240 * scale)),
                     steps=24, delay=0.03)
            drop = wait_for(lambda: next((l for l in lines if l.startswith("FILE_DROP")), None), "the drop")
            if "dropped.txt" not in drop:
                raise AssertionError(f"dropped {drop!r}")
            wait_for(lambda: any(l.startswith("DRAG_RESULT Copy") for l in source_lines), "the source's result")
            win.activate(window)
            win.close(window)
            proc.wait(timeout=10)
        finally:
            stop(proc)
            if source is not None:
                stop(source)
    return drop.strip()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("examples", type=pathlib.Path, help="the directory holding the built examples")
    args = parser.parse_args()
    examples = args.examples.resolve()
    clip = check_clipboard(examples)
    dialogs = check_dialogs(examples)
    drop = check_file_drop(examples)
    # Last: run before it, the next drag from another process was often
    # not taken.
    save = check_save_dialog(examples)
    print(f"PASS: clipboard ({clip}); dialogs ({dialogs}); save ({save}); "
          f"drag and drop from another process ({drop})")


if __name__ == "__main__":
    main()
