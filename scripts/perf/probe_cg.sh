#!/bin/bash
# Perf probe (not for merge): where in __memcmp_avx2_movbe script-update's
# extra instructions go, base vs head (#886), per instruction, disassembled.
W=${ZGX_PERF_DIR:-/tmp/zgx-perf}
for side in base head; do
  env -i PATH="$PATH" HOME="$HOME" LD_PRELOAD=$W/same_run.so ZGX_SAME_RUN_REPORT=1 ZGX_GATE=1 ZGX_ASSISTIVE=0 ZGX_EXAMPLES=$W/scripts-$side TICKS=200 \
    setarch -R valgrind --tool=callgrind --dump-instr=yes --compress-pos=no --compress-strings=no \
      --callgrind-out-file=$W/cgi.$side.out $W/gate-bin-$side/update_bench >/dev/null 2>&1
done
python3 "$(dirname "$0")/probe_memcmp.py" $W/cgi.base.out $W/cgi.head.out
