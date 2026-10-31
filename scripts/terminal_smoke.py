#!/usr/bin/env python3
"""A terminal zgx draws (zgx-desktop's `terminal` feature), natively on X11
(Xvfb/Openbox, xdotool): crates/zgx-desktop/examples/terminal_pane.rs
running the login shell on a pseudoterminal, read back from screen captures
by colours the shell prints.

- a command the app types into its session (`Terminal::input`) runs: its
  magenta cells show;
- X11 keys typed into the window reach the shell (through the hidden
  editor at the cursor): green cells;
- Ctrl+C interrupts the foreground program (`sleep 30`), and the shell
  takes the next command at once: blue cells;
- resizing the window resizes the grid the shell sees: a bar `tput cols`
  wide shrinks with the window;
- an idle, focused terminal does no work: the process's CPU time stays put
  over two seconds (no caret blink, no timers, no redraws without output).

Build with `cargo build -p zgx-desktop --features terminal --example
terminal_pane` first."""
import argparse
import os
import pathlib
import subprocess
import sys
import time

from PIL import Image, ImageChops, ImageGrab

sys.path.insert(0, str(pathlib.Path(__file__).parent))
from platform_smoke import stop  # noqa: E402

MAGENTA, GREEN, BLUE, ORANGE = (255, 0, 255), (0, 255, 0), (0, 0, 255), (255, 128, 0)


def bar(colour):
    """A shell command printing eight cells of `colour` behind spaces."""
    r, g, b = colour
    return f"printf '\\033[48;2;{r};{g};{b}m%8s\\033[0m\\n' ''"


def box(image, colour):
    """The box of `image`'s pixels of exactly `colour`, or None."""
    solid = Image.new("RGB", image.size, colour)
    return ImageChops.difference(image, solid).convert("L").point(lambda v: 255 if v == 0 else 0).getbbox()


def cpu_seconds(pid):
    fields = pathlib.Path(f"/proc/{pid}/stat").read_text().rsplit(")", 1)[1].split()
    return (int(fields[11]) + int(fields[12])) / os.sysconf("SC_CLK_TCK")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("binary", type=pathlib.Path)
    parser.add_argument("--output", type=pathlib.Path, default=pathlib.Path("/tmp/zgx-terminal"))
    args = parser.parse_args()
    args.output.mkdir(parents=True, exist_ok=True)
    display = os.environ.get("DISPLAY")
    if not display:
        parser.error("DISPLAY must name an X11 display with a window manager")
    env = dict(os.environ, WINIT_X11_SCALE_FACTOR="1", SHELL="/bin/sh", PS1="$ ", ENV="/dev/null")
    env.pop("WAYLAND_DISPLAY", None)
    env.setdefault("DBUS_SESSION_BUS_ADDRESS", "unix:path=/nonexistent")
    log = (args.output / "terminal.log").open("w")
    process = subprocess.Popen([str(args.binary.resolve()), bar(MAGENTA)], env=env, stdout=log, stderr=subprocess.STDOUT)

    def xdo(*arguments):
        return subprocess.check_output(["xdotool", *map(str, arguments)], env=env, text=True).strip()

    def capture(name):
        image = ImageGrab.grab(xdisplay=display).convert("RGB")
        image.save(args.output / f"{name}.png")
        return image

    def until(what, colour, seconds=15):
        deadline = time.monotonic() + seconds
        while time.monotonic() < deadline:
            if process.poll() is not None:
                raise RuntimeError(f"the terminal exited ({process.returncode}) waiting for {what}")
            found = box(capture(what.replace(" ", "-")), colour)
            if found:
                return found
            time.sleep(0.1)
        raise AssertionError(f"{what}: no {colour} cells showed (see {args.output})")

    def type_line(text):
        xdo("type", "--clearmodifiers", "--delay", 8, text)
        xdo("key", "--clearmodifiers", "Return")

    try:
        deadline = time.monotonic() + 20
        window = None
        while time.monotonic() < deadline and process.poll() is None:
            found = subprocess.run(["xdotool", "search", "--onlyvisible", "--pid", str(process.pid)],
                                   env=env, text=True, capture_output=True)
            if found.returncode == 0 and found.stdout.strip():
                window = found.stdout.splitlines()[0]
                break
            time.sleep(0.05)
        if not window:
            raise RuntimeError("terminal_pane did not open a visible window")
        xdo("windowsize", window, 900, 600)
        magenta = until("the typed-in command", MAGENTA)
        width = magenta[2] - magenta[0]
        if width < 8 * 5:
            raise AssertionError(f"eight cells are {width} pixels wide")
        print(f"terminal: the app's input ran (8 cells, {width} px)")

        xdo("mousemove", "--window", window, 450, 300, "click", 1)
        time.sleep(0.3)
        type_line("clear; " + bar(GREEN))
        until("X11 keys", GREEN)
        print("terminal: X11 keys reached the shell")

        type_line("clear; sleep 30")
        time.sleep(0.5)
        xdo("key", "--clearmodifiers", "ctrl+c")
        type_line(bar(BLUE))
        until("Ctrl+C", BLUE, seconds=5)
        print("terminal: Ctrl+C interrupted sleep")

        orange = "clear; printf '\\033[48;2;255;128;0m%*s\\033[0m\\n' $(tput cols) ''"
        type_line(orange)
        wide = until("a full-width bar", ORANGE)
        xdo("windowsize", window, 500, 400)
        time.sleep(0.5)
        type_line(orange)
        deadline = time.monotonic() + 10
        narrow = wide
        while time.monotonic() < deadline:
            narrow = box(capture("a-narrower-bar"), ORANGE) or wide
            if narrow[2] - narrow[0] < (wide[2] - wide[0]) * 0.75:
                break
            time.sleep(0.1)
        else:
            raise AssertionError(f"the grid didn't follow the window: {wide} then {narrow}")
        print(f"terminal: resizing the window resized the grid ({wide[2] - wide[0]} -> {narrow[2] - narrow[0]} px)")

        time.sleep(1)
        before = cpu_seconds(process.pid)
        time.sleep(2)
        spent = cpu_seconds(process.pid) - before
        if spent > 0.05:
            raise AssertionError(f"an idle, focused terminal spent {spent:.2f} s of CPU in 2 s")
        print(f"terminal: idle and focused, {spent:.2f} s of CPU in 2 s")
    finally:
        stop(process)
        log.close()


if __name__ == "__main__":
    main()
