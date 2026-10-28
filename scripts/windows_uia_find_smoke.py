#!/usr/bin/env python3
"""UI Automation finds text in a zgx window (ITextRangeProvider::FindText,
vendored AccessKit's zgx patch): selectable_text's document finds
"paragraph" forward, "DRAG" without case, an "a" backward, and nothing
for text it doesn't have. The range found is the match's text and has a box
on the window, where the word shows.

    cargo build -p zgx-desktop --example selectable_text
    python scripts/windows_uia_find_smoke.py target/debug/examples/selectable_text.exe
"""
import argparse
import json
import pathlib
import subprocess
import time

import windows_input as win

SCRIPT = r"""
$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName UIAutomationClient,UIAutomationTypes
$root = [System.Windows.Automation.AutomationElement]::FromHandle([IntPtr]{hwnd})
$all = $root.FindAll([System.Windows.Automation.TreeScope]::Subtree, [System.Windows.Automation.Condition]::TrueCondition)
foreach ($element in $all) {{
  $pattern = $null
  if (-not $element.TryGetCurrentPattern([System.Windows.Automation.TextPattern]::Pattern, [ref]$pattern)) {{ continue }}
  $document = $pattern.DocumentRange
  if (-not $document.GetText(-1).Contains('paragraph')) {{ continue }}
  $out = @{{}}
  foreach ($case in @(@('paragraph', $false, $false), @('DRAG', $false, $true), @('a', $true, $false), @('nowhere', $false, $false))) {{
    $range = $document.FindText($case[0], $case[1], $case[2])
    if ($null -eq $range) {{ $out[$case[0]] = $null; continue }}
    $rects = $range.GetBoundingRectangles()
    $out[$case[0]] = @{{ text = $range.GetText(-1); rect = if ($rects.Count) {{ @($rects[0].Left, $rects[0].Top, $rects[0].Width, $rects[0].Height) }} else {{ $null }} }}
  }}
  $out | ConvertTo-Json -Compress -Depth 4
  break
}}
"""


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("binary", type=pathlib.Path)
    args = parser.parse_args()
    proc = subprocess.Popen([str(args.binary.resolve())], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    try:
        window = win.wait_window(proc)
        time.sleep(1)
        win.activate(window)
        deadline = time.monotonic() + 15
        while True:
            done = subprocess.run(["powershell", "-NoProfile", "-NonInteractive", "-Command",
                                   SCRIPT.format(hwnd=int(window))], capture_output=True, text=True)
            if done.stdout.strip():
                found = json.loads(done.stdout)
                break
            if time.monotonic() > deadline:
                raise AssertionError(f"no document with text: {done.stderr.strip()[-800:]}")
            time.sleep(0.5)
        win.close(window)
        proc.wait(timeout=10)
    finally:
        if proc.poll() is None:
            proc.kill()
            proc.wait()
    paragraph, drag, last, nowhere = found["paragraph"], found["DRAG"], found["a"], found["nowhere"]
    assert paragraph and paragraph["text"] == "paragraph", found
    assert paragraph["rect"] and paragraph["rect"][2] > 0 and paragraph["rect"][3] > 0, f"the match has no box: {found}"
    assert drag and drag["text"] == "Drag", f"without case: {found}"
    assert last and last["text"] == "a" and last["rect"], f"backward: {found}"
    assert nowhere is None, f"text it doesn't have: {found}"
    print(f"PASS: UI Automation finds text in zgx's document: {json.dumps(found)}")


if __name__ == "__main__":
    main()
