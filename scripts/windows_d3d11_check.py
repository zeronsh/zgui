"""Direct3D 11 against Direct3D 12 on this machine, unattended: what decides
whether zgx-gpu's Direct3D 11 path (`d3d11`, `ZGX_D3D11=1`) may become
Windows' default here. Made for a machine with an AMD or Intel GPU
(docs/results/d3d11-proof-windows-2026-10-27 has NVIDIA's). Run from the
repository root, the machine left alone (about 1 h 30); everything lands in
`--out`, `summary.md` first:

    python scripts\\windows_d3d11_check.py --out d3d11-check

1. Builds what it runs (`--no-build`: it was built already): the suite's four
   apps (`dist`, zgx-desktop's `d3d11` feature), `render_parity`,
   `d3d_frame_floor` and `framepace`.
2. Records the machine: GPUs and drivers, displays, Windows.
3. Pixels: `render_parity` draws the suite's four scenes both ways and
   compares them byte for byte (`parity/`).
4. The APIs' own GPU floor: `d3d_frame_floor` clears and presents with
   nothing drawn, Direct3D 12 against 11, both swap effects, the GPU's
   running time read from Windows' counters (`floor.json`).
5. The Windows suite, zgx against zgx-d3d11, every scene and startup
   (`suite/`; `--threaded` adds the driver's threading on).

It fails closed: a parity difference, a suite that exits with an error or a
scene without its summary is listed at the top of `summary.md`, and the run
exits 1.

Needs Rust (the repository's toolchain) and Python 3.10's standard library
only. The windows it opens take the front; nothing else should run."""
import argparse
import json
import os
import shutil
import statistics
import subprocess
import sys
import time
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "scripts"))
import run_windows_suite as suite  # noqa: E402
import windows_input as win  # noqa: E402
import windows_pdh as pdh  # noqa: E402

DIST = ROOT / "target" / "dist" / "examples"
RELEASE = ROOT / "target" / "release" / "examples"
APPS = ["component_workload", "heavy_workload", "animation_workload", "vlist_workload"]


def cargo(*args):
    print("cargo", " ".join(args), flush=True)
    subprocess.run(["cargo", *args, "--locked"], cwd=ROOT, check=True)


def build():
    cargo("build", "--profile", "dist", "-p", "zgx-desktop", "--features", "d3d11",
          *[f"--example={app}" for app in APPS])
    cargo("build", "--release", "-p", "zgx-desktop", "--features", "d3d11", "--example", "render_parity")
    cargo("build", "--release", "-p", "zgx-gpu", "--example", "d3d_frame_floor", "--example", "framepace")


def machine():
    """Every GPU and its driver as Windows lists them, beside the suite's record."""
    query = ("Get-CimInstance Win32_VideoController | "
             "Select-Object Name, DriverVersion, AdapterCompatibility | ConvertTo-Json")
    found = subprocess.run(["powershell", "-NoProfile", "-Command", query], capture_output=True, text=True)
    gpus = json.loads(found.stdout or "[]")
    # Which compiler Direct3D 12's shaders get (zgx pins FXC; this variable
    # overrides it), and whether DXC would be found on PATH.
    shaders = {"WGPU_DX12_COMPILER": os.environ.get("WGPU_DX12_COMPILER"),
               "dxcompiler_on_path": shutil.which("dxcompiler.dll")}
    return {"gpus": gpus if isinstance(gpus, list) else [gpus], "shaders": shaders,
            **suite.environment()}


def parity(out):
    """`render_parity` both ways, and its comparison per scale: (identical,
    the comparison's first lines)."""
    exe = RELEASE / "render_parity.exe"
    results = {}
    for scale in ["1", "1.5", "2"]:
        env = dict(os.environ, SCALE=scale, TICKS="120")
        sides = {}
        for name, value in [("d3d12", "0"), ("d3d11", "1")]:
            sides[name] = out / f"{name}-{scale}"
            subprocess.run([str(exe), "write", str(sides[name])], env=dict(env, ZGX_D3D11=value), check=True)
        compared = subprocess.run([str(exe), "compare", str(sides["d3d12"]), str(sides["d3d11"])],
                                  env=env, capture_output=True, text=True)
        (out / f"compare-{scale}.txt").write_text(compared.stdout, newline="\n")
        results[scale] = (compared.returncode == 0,
                          (compared.stdout + compared.stderr).strip().splitlines()[:3])
        print(f"parity at {scale}x: {'byte-identical' if results[scale][0] else 'DIFFERS'}", flush=True)
    return results


def floor(rounds):
    """App GPU of `d3d_frame_floor`, per API and swap effect: ms/s medians."""
    exe = RELEASE / "d3d_frame_floor.exe"
    found = {}
    for effect in ["sequential", "discard"]:
        for r in range(rounds):
            for mode in (["d3d12", "d3d11"] if r % 2 == 0 else ["d3d11", "d3d12"]):
                app = subprocess.Popen([str(exe), mode, "1200"], env=dict(os.environ, FLOOR_EFFECT=effect),
                                       stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
                try:
                    win.activate(win.wait_window(app, timeout=15))
                    time.sleep(4)
                    with pdh.Counters([pdh.gpu_time(app.pid)]) as counters:
                        first, start = counters.sample(), time.perf_counter()
                        time.sleep(8)
                        last, wall = counters.sample(), time.perf_counter() - start
                    found.setdefault(f"{mode} {effect}", []).append((last[0] - first[0]) / 1e4 / wall)
                finally:
                    app.kill()
                    app.wait()
    return {key: {"median_ms_per_s": statistics.median(v), "runs": v} for key, v in found.items()}


def summary(out, frameworks, floors, identical, info, failures):
    lines = [f"# Direct3D 11 against Direct3D 12 on {', '.join(g.get('Name', '?') for g in info['gpus'])}", ""]
    if failures:
        lines += ["**FAILED:**", ""] + [f"- {failure}" for failure in failures] + [""]
    else:
        lines += ["Passed: pixels identical at every scale, and every scene measured.", ""]
    lines += [f"- {g.get('Name')}: driver {g.get('DriverVersion')}" for g in info["gpus"]]
    lines += [f"- {info['chip']}, Windows {info['windows']}, {info['display_count']} display(s)"]
    lines += [f"- Direct3D 12 shaders: WGPU_DX12_COMPILER={info['shaders']['WGPU_DX12_COMPILER'] or 'unset'}, "
              f"dxcompiler.dll on PATH: {info['shaders']['dxcompiler_on_path'] or 'no'}", ""]
    lines += ["Pixels (`render_parity`, the four scenes): "
              + ", ".join(f"{s}x {'identical' if ok else 'DIFFER'}" for s, (ok, _) in identical.items()), ""]
    lines += ["## The APIs' own GPU floor (`d3d_frame_floor`, nothing drawn)", "",
              "| Swap effect | Direct3D 12 (ms/s) | Direct3D 11 (ms/s) |", "| --- | --- | --- |"]
    for effect in ["sequential", "discard"]:
        d12, d11 = (floors[f"{m} {effect}"]["median_ms_per_s"] for m in ["d3d12", "d3d11"])
        lines.append(f"| {effect} | {d12:.3f} | {d11:.3f} |")
    lines += ["", "## The suite: medians, and Direct3D 11's change", ""]
    metrics = [("mcycles_per_second", "app CPU (M cycles/s)", 1), ("gpu_ms_per_second", "app GPU (ms/s)", 1),
               ("dwm_mcycles_per_second", "DWM CPU (M cycles/s)", 1), ("mean_private_bytes", "private (MiB)", 2**20)]
    lines += ["| Scene, mode | " + " | ".join(label for _, label, _ in metrics) + " |",
              "| --- |" + " --- |" * len(metrics)]
    for scene in ["heavy", "lab", "animate", "vlist"]:
        path = out / "suite" / scene / "summary.json"
        if not path.exists():
            continue
        groups = {(g["framework"], g["mode"]): g for g in json.loads(path.read_text())["groups"]}
        for mode in [m for (f, m) in groups if f == "zgx"]:
            cells = []
            for key, _, unit in metrics:
                a, b = (groups.get((f, mode), {}).get(key, {}).get("median") for f in frameworks[:2])
                if a is None or b is None:
                    cells.append("")
                    continue
                change = f" ({100 * (b - a) / a:+.0f}%)" if a else ""
                cells.append(f"{a / unit:.3g} → {b / unit:.3g}{change}")
            lines.append(f"| {scene} {mode} | " + " | ".join(cells) + " |")
    startup = out / "suite" / "startup" / "startup.json"
    if startup.exists():
        medians = json.loads(startup.read_text())["medians"]
        lines += ["", "Startup (lab, to content): " + ", ".join(
            f"{f} {medians[f]['content_ms']['median']:.0f} ms" for f in frameworks if f in medians)]
    (out / "summary.md").write_text("\n".join(lines) + "\n", encoding="utf-8", newline="\n")


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--out", type=Path, required=True)
    parser.add_argument("--no-build", action="store_true", help="the binaries were built already")
    parser.add_argument("--threaded", action="store_true", help="also zgx-d3d11-threaded")
    parser.add_argument("--rounds", type=int, default=3, help="the floor's rounds per swap effect")
    parser.add_argument("--preflight-threshold", type=float, default=75.)
    args = parser.parse_args()
    out = args.out.resolve()
    out.mkdir(parents=True, exist_ok=True)
    if not args.no_build:
        build()
    info = machine()
    (out / "machine.json").write_text(json.dumps(info, indent=1) + "\n", newline="\n")
    identical = parity(out / "parity")
    with suite.compare.PowerPlan(suite.compare.HIGH_PERFORMANCE), suite.compare.Minimized(["Zeron"]):
        floors = floor(args.rounds)
    (out / "floor.json").write_text(json.dumps(floors, indent=1) + "\n", newline="\n")
    frameworks = ["zgx", "zgx-d3d11"] + (["zgx-d3d11-threaded"] if args.threaded else [])
    ran = subprocess.run([sys.executable, str(ROOT / "scripts" / "run_windows_suite.py"), "--zgx", str(DIST),
                          "--frameworks", ",".join(frameworks), "--framepace", str(RELEASE / "framepace.exe"),
                          "--preflight-threshold", str(args.preflight_threshold), "--out", str(out / "suite")],
                         cwd=ROOT, check=False)
    failures = [f"pixels differ at {scale}x: {' / '.join(lines) or 'no detail'}"
                for scale, (ok, lines) in identical.items() if not ok]
    if ran.returncode != 0:
        failures.append(f"the suite exited with {ran.returncode} (`suite/`)")
    failures += [f"no summary for {scene} (`suite/{scene}/summary.json`)"
                 for scene in ["heavy", "lab", "animate", "vlist"]
                 if not (out / "suite" / scene / "summary.json").exists()]
    summary(out, frameworks, floors, identical, info, failures)
    print((out / "summary.md").read_text(encoding="utf-8"))
    if failures:
        sys.exit(1)


if __name__ == "__main__":
    main()
