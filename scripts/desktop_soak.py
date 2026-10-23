#!/usr/bin/env python3
"""Run the desktop_soak example for a number of cycles and judge whether what
the process holds levels off: after a warm-up (the first third), its
footprint, file descriptors, Mach ports and threads must stay within the
warm-up's maximum (plus a margin for the allocator and the window server).

    cargo build --release -p zgui-desktop --example desktop_soak
    python3 scripts/desktop_soak.py target/release/examples/desktop_soak --cycles 300

It opens and closes real windows (with menus, popovers, tooltips and prompt
sheets) for the whole run, a cycle a second: leave the machine to it.
"""
import argparse
import re
import subprocess
import sys

parser = argparse.ArgumentParser()
parser.add_argument("binary")
parser.add_argument("--cycles", type=int, default=50)
args = parser.parse_args()

try:
    run = subprocess.run(
        [args.binary, str(args.cycles)],
        capture_output=True,
        text=True,
        timeout=args.cycles * 10 + 60,
    )
except subprocess.TimeoutExpired:
    sys.exit("desktop_soak did not finish")
if run.returncode != 0:
    print(run.stdout + run.stderr)
    sys.exit(f"desktop_soak exited {run.returncode}")
samples = [
    {key: int(value) for key, value in re.findall(r"(\w+)=(\d+)", line)}
    for line in run.stdout.splitlines()
    if line.startswith("soak cycle")
]
if len(samples) != args.cycles:
    print(run.stdout + run.stderr)
    sys.exit(f"{len(samples)} of {args.cycles} cycles reported")
warm = max(len(samples) // 3, 2)
# Margins: the allocator and the window server keep some of what the
# warm-up used; a leak grows past them over a long run.
margins = {"footprint": 48 << 20, "fds": 2, "ports": 40, "threads": 2}
failed = False
for key, margin in margins.items():
    warm_max = max(s[key] for s in samples[:warm])
    late_max = max(s[key] for s in samples[warm:]) if samples[warm:] else warm_max
    first, last = samples[0][key], samples[-1][key]
    status = "ok" if late_max <= warm_max + margin else "GROWS"
    failed |= status != "ok"
    quarter = max(len(samples) // 4, 1)
    means = [
        sum(s[key] for s in samples[i : i + quarter]) // len(samples[i : i + quarter])
        for i in range(0, quarter * 4, quarter)
    ]
    print(
        f"{key:9}: first {first}, warm-up max {warm_max}, later max {late_max}, last {last}, "
        f"quarters' means {means}: {status}"
    )
if failed:
    sys.exit("something the process holds keeps growing")
print(f"PASS: {len(samples)} cycles level off")
