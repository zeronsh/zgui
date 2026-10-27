"""Scratch: heavy's idle trial alone, repeated, the suite's own way
(run_windows_suite.one_trial and desktop_trial), D3D12 against the D3D11
variants rotated: app cycles, DWM cycles and GPU per trial.

    python idle_dwm.py <dist examples dir> <repeats> <out dir>"""
import statistics
import sys
from pathlib import Path

sys.path.insert(0, r"C:\Users\wing_\Documents\GitHub\zgui-d3d11\scripts")
import run_windows_suite as suite  # noqa: E402

examples, repeats, out = Path(sys.argv[1]), int(sys.argv[2]), Path(sys.argv[3])
out.mkdir(parents=True, exist_ok=True)
binary = examples / "heavy_workload.exe"
names = ["desktop", "zgx", "zgx-d3d11", "zgx-d3d11-threaded"]
found = {name: [] for name in names}
with suite.compare.PowerPlan(suite.compare.HIGH_PERFORMANCE), suite.compare.Minimized(["Zeron"]):
    for r in range(repeats):
        for name in names[r % len(names):] + names[:r % len(names)]:
            if name == "desktop":
                row = suite.desktop_trial(r, out)
            else:
                row = suite.one_trial("heavy", binary, name, "idle", r, out)
            found[name].append(row)
            print(f"{r} {name:20} app {row.get('mcycles_per_second', 0) or 0:5.2f} M cyc/s  "
                  f"dwm {row.get('dwm_mcycles_per_second', 0):6.3f} M cyc/s  "
                  f"dwm gpu {row.get('dwm_gpu_ms_per_second', 0):.4f} ms/s  ticks {row.get('workload_ticks')}",
                  flush=True)
print("\nmedians (min-max):")
for name in names:
    dwm = [row.get("dwm_mcycles_per_second", 0) for row in found[name]]
    print(f"  {name:20} dwm {statistics.median(dwm):6.3f} ({min(dwm):.3f}-{max(dwm):.3f}) M cyc/s over {len(dwm)}")
