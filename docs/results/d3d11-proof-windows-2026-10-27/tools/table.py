"""Scratch: the D3D11 proof's suite as one table: each scene and mode, zgx on
Direct3D 12 against the Direct3D 11 variants (medians, and D3D11 against
D3D12 as a change)."""
import json
import sys
from pathlib import Path

run = Path(sys.argv[1])
FWS = ["zgx", "zgx-d3d11", "zgx-d3d11-threaded"]
METRICS = [
    ("mcycles_per_second", "app CPU, M cycles/s", 1),
    ("mcycles_per_frame", "app CPU, M cycles/frame", 1),
    ("gpu_ms_per_second", "app GPU, ms/s", 1),
    ("dwm_mcycles_per_second", "DWM CPU, M cycles/s", 1),
    ("dwm_gpu_ms_per_second", "DWM GPU, ms/s", 1),
    ("mean_private_bytes", "private, MiB", 2**20),
    ("mean_working_set_bytes", "working set, MiB", 2**20),
    ("gpu_board_watts", "board, W", 1),
]
PACING = [("interval_p50_ms", "pacing p50, ms"), ("interval_p99_ms", "pacing p99, ms"),
          ("long_interval_fraction", "long intervals"), ("merged_frames", "merged frames")]


def change(new, old):
    if old in (None, 0) or new is None:
        return ""
    return f"{100 * (new - old) / old:+.0f}%"


for scene in ["heavy", "lab", "animate", "vlist"]:
    summary = json.loads((run / scene / "summary.json").read_text())
    groups = {(g["framework"], g["mode"]): g for g in summary["groups"]}
    pacing = {(g["framework"], g["mode"]): g for g in summary.get("pacing", [])}
    modes = [m for (f, m) in groups if f == "zgx"]
    for mode in modes:
        print(f"\n### {scene} {mode}\n")
        print("| | " + " | ".join(FWS) + " | D3D11 | D3D11 threaded |")
        print("| --- | " + " | ".join("---" for _ in FWS) + " | --- | --- |")
        for key, label, unit in METRICS:
            values = [groups.get((f, mode), {}).get(key, {}).get("median") for f in FWS]
            if all(v is None for v in values):
                continue
            values = [None if v is None else v / unit for v in values]
            cells = ["" if v is None else f"{v:.3g}" for v in values]
            print(f"| {label} | " + " | ".join(cells) + f" | {change(values[1], values[0])} | {change(values[2], values[0])} |")
        for key, label in PACING:
            values = [pacing.get((f, mode), {}).get(key, {}) for f in FWS]
            values = [v.get("median") if isinstance(v, dict) else v for v in values]
            if all(v in (None, {}) for v in values):
                continue
            cells = ["" if v in (None, {}) else f"{v:.3g}" for v in values]
            print(f"| {label} | " + " | ".join(cells) + " | | |")
startup = json.loads((run / "startup" / "startup.json").read_text())["medians"]
print("\n### startup (lab, medians of 10 launches)\n")
print("| | " + " | ".join(FWS) + " |")
print("| --- | " + " | ".join("---" for _ in FWS) + " |")
for key, label in [("window_ms", "to window, ms"), ("content_ms", "to content, ms"), ("cpu_ms", "CPU by then, ms"),
                   ("cycles", "cycles by then, M")]:
    values = [startup[f][key]["median"] / (1e6 if key == "cycles" else 1) for f in FWS]
    print(f"| {label} | " + " | ".join(f"{v:.0f}" for v in values) + " |")
