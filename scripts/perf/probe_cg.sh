#!/bin/bash
# Perf probe (not for merge): callgrind base vs head of the gate's binaries,
# per function, for the Luau cells #940 moves on x86.
W=${ZGX_PERF_DIR:-/tmp/zgx-perf}
probe() { local name=$1 bin=$2; shift 2
  for side in base head; do
    env -i PATH="$PATH" HOME="$HOME" LD_PRELOAD=$W/same_run.so ZGX_SAME_RUN_REPORT=1 ZGX_GATE=1 ZGX_ASSISTIVE=0 ZGX_EXAMPLES=$W/scripts-$side "$@" \
      setarch -R valgrind --tool=callgrind --callgrind-out-file=$W/cg.$side.out $W/gate-bin-$side/$bin >/dev/null 2>&1
    callgrind_annotate --auto=no --threshold=100 $W/cg.$side.out 2>/dev/null | grep -E '^ *[0-9,]+ ' > $W/fn.$side.txt
  done
  echo "=== $name"
  python3 "$(dirname "$0")/probe_cgdiff.py" $W/fn.base.txt $W/fn.head.txt
}
probe style style_bench
probe anime anime_bench
probe script-layout script_layout_bench
