#!/usr/bin/env python3
"""The viewport example's GPU view draws a frame as it shows, one a refresh
while it spins (30 refreshes), and none once it stops: a still view in an
idle window does no work."""
import re
import subprocess
import sys

binary = sys.argv[1]
try:
    run = subprocess.run([binary, "--smoke-test"], capture_output=True, text=True, timeout=60)
except subprocess.TimeoutExpired:
    sys.exit("viewport did not finish")
log = run.stdout + run.stderr
print(log)
if run.returncode != 0:
    sys.exit(f"viewport exited {run.returncode}")
still = re.search(r"VIEWPORT still=(\d+)", log)
spun = re.search(r"VIEWPORT spun=(\d+) idle=(\d+)", log)
if not still or not spun:
    sys.exit("missing its VIEWPORT lines")
still, (spun, idle) = int(still[1]), map(int, spun.groups())
if still != 1:
    sys.exit(f"{still} frames drawn while still, not 1")
# One a refresh: its own timer may start a refresh after the probe's, and
# the refresh it stopped at may draw.
if not 28 <= spun - still <= 32:
    sys.exit(f"{spun - still} frames drawn over 30 refreshes")
if idle != 0:
    sys.exit(f"{idle} frames drawn once stopped")
print(f"PASS: a frame shown, {spun - still} spinning, none idle")
