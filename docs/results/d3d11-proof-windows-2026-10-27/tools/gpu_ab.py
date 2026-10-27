"""Scratch: the suite's own trials (run_windows_suite.one_trial) for a few
scenes, zgx's variants rotated: app GPU, app CPU and DWM per variant.

    python gpu_ab.py <examples dir> <repeats> <out dir>"""
import statistics
import sys
from pathlib import Path

sys.path.insert(0, r"C:\Users\wing_\Documents\GitHub\zgui-d3d11\scripts")
import run_windows_suite as suite  # noqa: E402

suite.VARIANTS["zgx-d3d11-lat1"] = ("zgx", {"ZGX_D3D11": "1", "ZGX_EXP_LATENCY": "1"})
suite.VARIANTS["zgx-d3d11-regalways"] = ("zgx", {"ZGX_D3D11": "1", "ZGX_EXP_REGIONS": "always"})
examples, repeats, out = Path(sys.argv[1]), int(sys.argv[2]), Path(sys.argv[3])
CASES = [("lab", "component_workload", "stream"), ("animate", "animation_workload", "animate")]
names = ["zgx", "zgx-d3d11", "zgx-d3d11-lat1", "zgx-d3d11-regalways"]
found = {}
with suite.compare.PowerPlan(suite.compare.HIGH_PERFORMANCE), suite.compare.Minimized(["Zeron"]):
    for scene, exe, mode in CASES:
        (out / scene).mkdir(parents=True, exist_ok=True)
        for r in range(repeats):
            for name in names[r % len(names):] + names[:r % len(names)]:
                row = suite.one_trial(scene, examples / f"{exe}.exe", name, mode, r, out / scene)
                found.setdefault((scene, name), []).append(row)
                print(f"{r} {scene:8} {name:20} gpu {row.get('gpu_ms_per_second', 0):5.2f} ms/s  "
                      f"cpu {row.get('mcycles_per_second', 0):6.1f} M/s  dwm {row.get('dwm_mcycles_per_second', 0):6.1f}",
                      flush=True)
print()
for (scene, name), rows in found.items():
    med = lambda k: statistics.median(r.get(k, 0) or 0 for r in rows)  # noqa: E731
    print(f"{scene:8} {name:20} app GPU {med('gpu_ms_per_second'):5.2f} ms/s  app CPU {med('mcycles_per_second'):6.1f} "
          f"M cycles/s  DWM {med('dwm_mcycles_per_second'):6.1f}  private {med('mean_private_bytes') / 2**20:5.1f} MiB")
