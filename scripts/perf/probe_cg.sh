#!/bin/bash
# Perf probe (not for merge): who calls strpbrk in app-code, base vs head,
# with call counts (#924's x86 __strpbrk_sse42 +61K).
W=${ZGX_PERF_DIR:-/tmp/zgx-perf}
for side in base head; do
  env -i PATH="$PATH" HOME="$HOME" LD_PRELOAD=$W/same_run.so ZGX_SAME_RUN_REPORT=1 ZGX_GATE=1 ZGX_ASSISTIVE=0 ZGX_EXAMPLES=$W/scripts-$side \
    APP=code LINES=5000 REPS=2 SKIP='a new letter' \
    setarch -R valgrind --tool=callgrind --callgrind-out-file=$W/cgs.$side.out $W/gate-bin-$side/list_state_bench >/dev/null 2>&1
  echo "=== $side: strpbrk's callers (inclusive, call counts)"
  callgrind_annotate --tree=caller --threshold=100 $W/cgs.$side.out 2>/dev/null \
    | grep -B30 -E '^ *[0-9,]+ .*\* .*strpbrk' | grep -E '<|\*' | head -40
  echo "=== $side: callers of those callers (str_find_aux, lmemfind)"
  callgrind_annotate --tree=caller --threshold=100 $W/cgs.$side.out 2>/dev/null \
    | grep -B30 -E '^ *[0-9,]+ .*\* .*(str_find_aux|nospecials)' | grep -E '<|\*' | head -40
done
