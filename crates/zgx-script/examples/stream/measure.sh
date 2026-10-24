#!/bin/zsh
# Compares stream_rust and stream_luau: CPU, physical footprint (as Activity
# Monitor reports memory) and GPU, idle and while streaming 60 tokens/s.
# Each app runs REPEATS times, alternating which goes first; each run samples
# 5 s idle from 15 s after launch, then 20 s of streaming (the stream starts
# at 22 s, sampling at about 25 s). Idle starts late because the GPU driver
# returns startup memory at its own pace: any process settles between 1 and
# 7 s, at about 33 or 66 MB, whichever app it is. Keep both windows visible
# and the desktop quiet: zgx stops drawing covered windows, and the window
# server's work is shared by every app.
# Run from the repository root: crates/zgx-script/examples/stream/measure.sh
set -e
REPEATS=${REPEATS:-5}
SAMPLER=/tmp/zgx-macsample
OUT=${OUT:-/tmp/zgx-stream-compare}
cargo build --release -p zgx-script --example stream_rust --example stream_luau
[[ -x $SAMPLER ]] || swiftc -O scripts/macsample.swift -o $SAMPLER -lIOReport
mkdir -p $OUT && rm -f $OUT/*.json(N)
for repeat in $(seq 1 $REPEATS); do
  apps=(stream_rust stream_luau)
  (( repeat % 2 == 0 )) && apps=(stream_luau stream_rust)
  for app in $apps; do
    echo "repeat $repeat: $app"
    STREAM_START_AFTER=22 ./target/release/examples/$app >/dev/null 2>&1 &
    pid=$!
    $SAMPLER $pid 15 5 0.05 > $OUT/$app.idle.$repeat.json
    $SAMPLER $pid 4 20 0.05 > $OUT/$app.stream.$repeat.json
    kill $pid; wait $pid 2>/dev/null || true
    sleep 2
  done
done
python3 - $OUT <<'EOF'
import json, sys, statistics, pathlib
out = pathlib.Path(sys.argv[1])
rows = {}
for f in sorted(out.glob('*.json')):
    app, phase, _ = f.name.split('.')[:3]
    rows.setdefault((phase, app), []).append(json.loads(f.read_text()))
mib = 1 / 2**20
print(f"{'phase':7} {'app':12} {'CPU %':>7} {'instr/s (M)':>12} {'mean MiB':>9} {'(range)':>9} {'peak MiB':>9} {'GPU %':>6} {'WS CPU %':>8}  runs")
for phase in ('idle', 'stream'):
    for app in ('stream_rust', 'stream_luau'):
        runs = rows.get((phase, app), [])
        if not runs: continue
        med = lambda key, scale=1: statistics.median(r[key] * scale for r in runs)
        print(f"{phase:7} {app:12} {med('cpu_percent_one_core'):7.2f} "
              f"{statistics.median(r['instructions'] / r['wall_seconds'] / 1e6 for r in runs):12.1f} "
              f"{med('mean_footprint_bytes', mib):9.1f} "
              f"{'%.0f-%.0f' % (min(r['mean_footprint_bytes'] for r in runs) * mib, max(r['mean_footprint_bytes'] for r in runs) * mib):>9} "
              f"{med('peak_footprint_bytes', mib):9.1f} "
              f"{med('gpu_percent'):6.2f} {med('windowserver_cpu_percent'):8.2f}  {len(runs)}")
print('medians, with the range of means, over runs; raw JSON in', out)
EOF
