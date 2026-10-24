#!/usr/bin/env python3
"""A WebView2 page in zgx on Windows (the `web_view` example, built with the
`webview2` feature), driven by SendInput: the page loads and its title reaches
`WebViewState`; a click in the page reaches it (its message comes back); zgx
content over the page takes clicks and the keyboard back from it; script
evaluation changes the page; the menu popover draws above the page and takes
its clicks; the wheel scrolls the page; and after a resize the page still
fills its node, edge to edge.

    cargo build -p zgx-desktop --features webview2 --example web_view
    python scripts/windows_webview_smoke.py target/debug/examples/web_view.exe
"""
import argparse
import pathlib
import subprocess
import threading
import time

import windows_input as win

BACKGROUND = (0x14, 0x15, 0x19)


def wait_for(check, what, timeout=10.0):
    deadline = time.monotonic() + timeout
    while not check():
        if time.monotonic() > deadline:
            raise AssertionError(f"timed out waiting for {what}")
        time.sleep(0.05)


def near(a, b, tolerance=12):
    return all(abs(x - y) <= tolerance for x, y in zip(a, b))


def white_pixels(capture, box):
    """How many near-white pixels `capture` has in `box` (left, top, right, bottom)."""
    left, top, right, bottom = box
    return sum(
        1
        for y in range(top, bottom, 2)
        for x in range(left, right, 2)
        if min(win.pixel(capture, x, y)) > 235
    )


def differs(a, b, box):
    """The share of `box`'s pixels that differ between captures `a` and `b`."""
    left, top, right, bottom = box
    points = [(x, y) for y in range(top, bottom, 3) for x in range(left, right, 3)]
    changed = sum(1 for x, y in points if not near(win.pixel(a, x, y), win.pixel(b, x, y)))
    return changed / len(points)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("binary", type=pathlib.Path)
    parser.add_argument("--shots", type=pathlib.Path, help="save captures here")
    args = parser.parse_args()
    lines = []
    proc = subprocess.Popen(
        [str(args.binary.resolve())], stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True
    )
    threading.Thread(target=lambda: lines.extend(proc.stdout), daemon=True).start()
    heard = lambda prefix: any(line.startswith(prefix) for line in lines)

    def shot(name):
        capture = win.capture(window)
        if args.shots:
            args.shots.mkdir(parents=True, exist_ok=True)
            win.save_png(capture, args.shots / f"{name}.png")
        return capture

    try:
        window = win.wait_window(proc)
        win.activate(window)
        scale = win.dpi(window) / 96
        at = lambda x, y: (round(x * scale), round(y * scale))
        box = lambda l, t, r, b: (*at(l, t), *at(r, b))

        wait_for(lambda: heard("FINISHED https://zgx.example/"), "the page to load", 30)
        wait_for(lambda: heard("TITLE zgx web view"), "the page's title")
        time.sleep(0.5)
        print("ok: the page loaded and its title reached the state")

        loaded = shot("loaded")
        assert white_pixels(loaded, box(180, 110, 620, 170)) > 200, "the page's heading shows"
        print("ok: the page shows in its node")

        win.click(window, *at(150, 298))
        wait_for(lambda: heard("MESSAGE"), "the page's message after a click in it")
        print("ok: a click in the page reaches it")

        # zgx's composer over the page: the click and the keyboard are zgx's.
        win.click(window, *at(400, 700))
        time.sleep(0.3)
        win.type_text("Hi")
        time.sleep(0.2)
        win.click(window, *at(780, 700))
        wait_for(
            lambda: white_pixels(shot("evaluated"), box(180, 110, 620, 170)) < 40,
            "the page's heading to change (Send to page)",
        )
        print("ok: zgx content over the page takes clicks and keys; script runs in the page")

        # The menu shows in a window of its own: over the page, and past the
        # window's right edge where the screen has room (else inside it).
        x, y = win.to_screen(window, *at(860, 60))
        strip = lambda: win.capture_rect(x, y, round(420 * scale), round(120 * scale))
        before = strip()
        win.click(window, *at(1060, 24))
        menu = (0, 0, round(420 * scale), round(120 * scale))
        wait_for(lambda: win.popups_of(proc.pid) and differs(before, strip(), menu) > 0.15,
                 "the menu over the page")
        if args.shots:
            win.save_png(strip(), args.shots / "menu.png")
        print("ok: the popover draws above the page")
        # Zoom to 125%, where the menu lies either way: zgx's, over the page.
        win.click(window, *at(1070, 138))
        wait_for(lambda: not win.popups_of(proc.pid), "the menu to close (its click was zgx's)")
        print("ok: a click on the popover over the page is zgx's")

        before = shot("before-scroll")
        win.wheel(window, *at(500, 400), -3)
        wait_for(lambda: differs(before, shot("scrolled"), box(40, 200, 1000, 600)) > 0.2,
                 "the page to scroll")
        print("ok: the wheel over the page scrolls it")

        win.resize_client(window, *at(800, 600))
        time.sleep(1.0)
        resized = shot("resized")
        width, height = win.client_size(window)
        # The page's node is inset 12 points from the window's right and bottom.
        background = lambda point: near(win.pixel(resized, *point), BACKGROUND, 4)
        right = next(x for x in range(width - 1, 0, -1) if not background((x, height // 2)))
        bottom = next(y for y in range(height - 1, 0, -1) if not background((width // 3, y)))
        expected = (width - round(12 * scale) - 1, height - round(12 * scale) - 1)
        assert abs(right - expected[0]) <= 2 and abs(bottom - expected[1]) <= 2, (
            (right, bottom), expected)
        print("ok: after a resize the page fills its node, edge to edge")
        print("webview smoke passed")
    finally:
        if proc.poll() is None:
            try:
                win.close(window)
                proc.wait(5)
            except Exception:
                proc.kill()


if __name__ == "__main__":
    main()
