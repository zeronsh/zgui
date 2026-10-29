#!/bin/bash
# Perf probe (not for merge): who calls memcmp in script-update, base vs
# head, for #886's x86 residual (__memcmp_avx2_movbe +445K).
W=${ZGX_PERF_DIR:-/tmp/zgx-perf}
for side in base head; do
  env -i PATH="$PATH" HOME="$HOME" LD_PRELOAD=$W/same_run.so ZGX_SAME_RUN_REPORT=1 ZGX_GATE=1 ZGX_ASSISTIVE=0 ZGX_EXAMPLES=$W/scripts-$side TICKS=200 \
    setarch -R valgrind --tool=callgrind --callgrind-out-file=$W/cg.$side.out $W/gate-bin-$side/update_bench >/dev/null 2>&1
  echo "=== $side: memcmp's callers"
  callgrind_annotate --tree=caller --inclusive=yes --threshold=100 $W/cg.$side.out 2>/dev/null \
    | grep -B40 -E '^ *[0-9,]+ .*\* .*memcmp' | grep -E '<|\*' | head -60
  echo "=== $side: memcmp calls by caller (call counts)"
  callgrind_annotate --tree=caller --threshold=100 $W/cg.$side.out 2>/dev/null | grep -E 'memcmp' | head -20
done
