#!/usr/bin/env python3
"""Keystroke to commit, frame pacing and idle wakeups of a native Wayland
client, from its own protocol log.

Runs the input_latency example under a Weston (X11 backend, on Xvfb, 60 Hz)
with WAYLAND_DEBUG=1, types into its field through Weston's seat, and reads
the log's timestamps: from each key press the client receives to the commit
of the frame that shows it, then (as Weston repaints on its own clock) the
commit shows at Weston's next repaint, up to one refresh later. With
--animate the example animates at the display's rate while keys arrive, so
typed text competes with frames already in flight. It also reports the
intervals between the animation's commits, and how often the idle client
wakes (protocol traffic and CPU time while nothing changes).

    cargo build --release -p zgx-desktop --example input_latency
    python3 scripts/wayland_latency_probe.py target/release/examples/input_latency
    python3 scripts/wayland_latency_probe.py target/release/examples/input_latency --animate

A development measurement, not a CI probe: software rendering (lavapipe)
on a shared runner sets the render time, so compare runs on one machine.
"""
import argparse
import os
import pathlib
import random
import re
import select
import statistics
import subprocess
import tempfile
import time

from platform_smoke import stop, wait_for_wm

LINE = re.compile(r"\[\s*(\d+\.\d+)\]\s+(->)?\s*(\w+)[#@](\d+)\.(\w+)\((.*)\)")


def events(path):
    for line in pathlib.Path(path).read_text(errors="replace").splitlines():
        match = LINE.search(line)
        if match:
            at, sent, interface, ident, name, args = match.groups()
            yield float(at), bool(sent), interface, int(ident), name, args


def summary(values):
    values = sorted(values)
    if not values:
        return "none"
    pick = lambda q: values[min(len(values) - 1, int(q * len(values)))]
    return (f"n={len(values)} median {statistics.median(values):.1f} ms, "
            f"p90 {pick(.9):.1f}, max {values[-1]:.1f}")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("binary", type=pathlib.Path)
    parser.add_argument("--output", type=pathlib.Path, default=pathlib.Path("/tmp/zgx-wayland-latency"))
    parser.add_argument("--samples", type=int, default=40)
    parser.add_argument("--animate", action="store_true")
    parser.add_argument("--idle-seconds", type=float, default=5)
    args = parser.parse_args()
    args.output.mkdir(parents=True, exist_ok=True)
    processes = []

    def launch(name, command, env, **options):
        log = (args.output / (name + ".log")).open("w")
        child = subprocess.Popen(command, env=env, stdout=log, stderr=options.pop("stderr", log), **options)
        processes.append(child)
        return child

    with tempfile.TemporaryDirectory(prefix="zgx-wayland-latency-") as runtime:
        os.chmod(runtime, 0o700)
        try:
            xvfb = subprocess.Popen(["Xvfb", "-r", "-noreset", "-displayfd", "1", "-screen", "0", "1280x960x24", "-ac"],
                                    stdout=subprocess.PIPE, stderr=subprocess.DEVNULL, text=True)
            processes.append(xvfb)
            if not select.select([xvfb.stdout], [], [], 10)[0]:
                raise RuntimeError("Xvfb startup timed out")
            env = dict(os.environ, DISPLAY=":" + xvfb.stdout.readline().strip(), XDG_RUNTIME_DIR=runtime,
                       DBUS_SESSION_BUS_ADDRESS="unix:path=/nonexistent")
            env.pop("WAYLAND_DISPLAY", None)
            launch("openbox", ["openbox"], env)
            wait_for_wm(env)
            launch("weston", ["weston", "--backend=x11", "--renderer=pixman", "--shell=kiosk-shell.so",
                              "--no-config", "--socket=zgx-latency", "--width=640", "--height=280",
                              "--idle-time=0"], env)
            deadline = time.monotonic() + 10
            window = None
            while window is None:
                found = re.search(r"x11 output .* window id (\d+)", (args.output / "weston.log").read_text())
                window = found and found.group(1)
                if time.monotonic() > deadline:
                    raise RuntimeError("Weston did not start; see weston.log")
                time.sleep(.05)

            def xdo(*parts):
                subprocess.check_call(["xdotool", *map(str, parts)], env=env)

            xdo("windowactivate", "--sync", window)
            client = dict(env, WAYLAND_DISPLAY="zgx-latency", WAYLAND_DEBUG="1", WINIT_UNIX_BACKEND="wayland")
            client.pop("DISPLAY", None)
            if args.animate:
                client["ZGX_ANIMATE"] = "1"
            protocol = args.output / "protocol.log"
            with protocol.open("w") as log:
                app = launch("application", [str(args.binary.resolve())], client, stderr=log)
            deadline = time.monotonic() + 15
            while "attach(wl_buffer" not in protocol.read_text(errors="replace"):
                if time.monotonic() > deadline or app.poll() is not None:
                    raise RuntimeError("the client did not present; see protocol.log")
                time.sleep(.05)
            time.sleep(1)
            xdo("mousemove", "--window", window, 100, 44, "click", 1)
            time.sleep(.5)

            idle = None
            if not args.animate:
                # Nothing changes now: count what the client does meanwhile.
                lines = len(protocol.read_text(errors="replace").splitlines())
                ticks = lambda: sum(map(int, pathlib.Path(f"/proc/{app.pid}/stat").read_text().rsplit(")", 1)[1].split()[11:13]))
                before = ticks()
                time.sleep(args.idle_seconds)
                cpu = (ticks() - before) / os.sysconf("SC_CLK_TCK")
                traffic = len(protocol.read_text(errors="replace").splitlines()) - lines
                idle = (traffic, cpu)

            for i in range(args.samples):
                if i % 15 == 14:
                    xdo("key", "ctrl+a", "BackSpace")
                    time.sleep(.3)
                xdo("key", random.choice("abcdefghijklmnopqrstuvwxyz"))
                time.sleep(random.uniform(.12, .25))
            time.sleep(.5)
        finally:
            for process in reversed(processes):
                stop(process)

    surface = None
    presses, commits, callbacks = [], [], []
    for at, sent, interface, ident, name, rest in events(protocol):
        if sent and interface == "wl_surface" and name == "attach" and "wl_buffer" in rest and surface is None:
            surface = ident
        if not sent and interface == "wl_keyboard" and name == "key" and rest.rstrip().endswith(", 1"):
            presses.append(at)
        if sent and interface == "wl_surface" and name == "commit" and ident == surface:
            commits.append(at)
        if not sent and interface == "wl_callback" and name == "done":
            callbacks.append(at)
    latencies = []
    for press in presses:
        later = [c for c in commits if c > press]
        if later:
            latencies.append(later[0] - press)
    print(f"{'animating' if args.animate else 'at rest'}: key press to commit {summary(latencies)}")
    if args.animate:
        frames = [b - a for a, b in zip(commits, commits[1:]) if b - a < 200]
        print(f"commit intervals while animating: {summary(frames)}"
              f" ({sum(1 for f in frames if f > 25)} longer than 1.5 refreshes)")
    if idle:
        print(f"idle {args.idle_seconds:.0f} s: {idle[0]} protocol messages, {idle[1]:.2f} s CPU")


if __name__ == "__main__":
    main()
