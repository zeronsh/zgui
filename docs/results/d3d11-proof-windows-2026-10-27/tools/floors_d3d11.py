"""Scratch copy of imescratch\floors.py for the D3D11 proof (its own targets). Floors beside zgx: each target launched in front, idle, sampled alike:
CPU cycles a second over 10 s after a 5 s warm-up (QueryProcessCycleTime:
every thread of the process, the driver's included), then private bytes,
working set, and GPU memory (PDH GPU Process Memory: dedicated and shared),
and the target's own `FIRST <ms>` line if it prints one.

    python floors.py <rounds>
    FLOOR_ONLY=name,name  FLOOR_EXTRA=name=exe (run with WGPU_BACKEND=gl)"""
import ctypes
import os
import statistics
import subprocess
import sys
import tempfile
import time
from ctypes import wintypes
from pathlib import Path

sys.path.insert(0, r"C:\Users\wing_\Documents\GitHub\zgui-d3d11\scripts")
import windows_compare as wc  # noqa: E402
import windows_input as win  # noqa: E402
import windows_pdh as pdh  # noqa: E402

S = Path(r"C:\Users\wing_\imescratch")
M = Path(r"C:\Users\wing_\Documents\GitHub\zgui-memory")
FONT = r"C:\Users\wing_\Documents\GitHub\zgui\comparisons\fonts\DejaVuSans.ttf"
APP = {"ZGX_MODE": "idle", "ZGX_SECONDS": "25", "ZGX_INITIAL_TICKS": "0", "ZGX_FONT_FILE": FONT}
D = Path(r"C:\Users\wing_\Documents\GitHub\zgui-d3d11\target\dist\examples")
D3D11 = {"ZGX_D3D11": "1"}
THREADED = {"ZGX_D3D11": "1", "ZGX_D3D11_THREADED": "1"}
TARGETS = [
    ("bare d3d11", [S / "memsteps/target/release/memsteps.exe", "d3d11"], {}),
    ("bare d3d12", [S / "memsteps/target/release/memsteps.exe", "d3d12"], {}),
    ("zgx lab d3d12", [D / "component_workload.exe"], APP),
    ("zgx lab d3d11", [D / "component_workload.exe"], {**APP, **D3D11}),
    ("zgx lab d3d11 thr", [D / "component_workload.exe"], {**APP, **THREADED}),
    ("zgx heavy d3d12", [D / "heavy_workload.exe"], APP),
    ("zgx heavy d3d11", [D / "heavy_workload.exe"], {**APP, **D3D11}),
    ("zgx heavy d3d11 thr", [D / "heavy_workload.exe"], {**APP, **THREADED}),
]
extra = os.environ.get("FLOOR_EXTRA")
if extra:
    name, exe = extra.split("=", 1)
    TARGETS.append((name, [Path(exe)], {"WGPU_BACKEND": "gl"}))
if os.environ.get("FLOOR_ONLY"):
    TARGETS = [t for t in TARGETS if t[0] in os.environ["FLOOR_ONLY"].split(",")]

kernel32 = ctypes.WinDLL("kernel32", use_last_error=True)
psapi = ctypes.WinDLL("psapi", use_last_error=True)
kernel32.OpenProcess.restype = wintypes.HANDLE
kernel32.OpenProcess.argtypes = [wintypes.DWORD, wintypes.BOOL, wintypes.DWORD]
kernel32.QueryProcessCycleTime.argtypes = [wintypes.HANDLE, ctypes.POINTER(ctypes.c_ulonglong)]
kernel32.CloseHandle.argtypes = [wintypes.HANDLE]


class COUNTERS(ctypes.Structure):
    _fields_ = [("cb", wintypes.DWORD), ("PageFaultCount", wintypes.DWORD)] + [
        (n, ctypes.c_size_t) for n in ("PeakWorkingSetSize", "WorkingSetSize", "QuotaPeakPagedPoolUsage",
                                       "QuotaPagedPoolUsage", "QuotaPeakNonPagedPoolUsage",
                                       "QuotaNonPagedPoolUsage", "PagefileUsage", "PeakPagefileUsage",
                                       "PrivateUsage")]


psapi.GetProcessMemoryInfo.argtypes = [wintypes.HANDLE, ctypes.POINTER(COUNTERS), wintypes.DWORD]


def cycles(handle):
    value = ctypes.c_ulonglong()
    kernel32.QueryProcessCycleTime(handle, ctypes.byref(value))
    return value.value


def memory(handle):
    c = COUNTERS(cb=ctypes.sizeof(COUNTERS))
    psapi.GetProcessMemoryInfo(handle, ctypes.byref(c), c.cb)
    return c.PrivateUsage, c.WorkingSetSize


rounds = int(sys.argv[1])
found = {}
log_path = Path(tempfile.gettempdir()) / "floors.log"
with wc.PowerPlan(wc.HIGH_PERFORMANCE), wc.Minimized(["Zeron"]):
    for r in range(rounds):
        order = TARGETS[r % len(TARGETS):] + TARGETS[:r % len(TARGETS)]
        for name, cmd, env in order:
            with open(log_path, "w") as log:
                app = subprocess.Popen([str(c) for c in cmd], env=dict(os.environ, BARE_SECONDS="25", **env),
                                       stdout=log, stderr=subprocess.STDOUT, cwd=str(Path(cmd[0]).parent))
            handle = kernel32.OpenProcess(0x1000 | 0x0400 | 0x0010, False, app.pid)
            try:
                win.activate(win.wait_window(app, timeout=15))
                time.sleep(5)
                c0, t0 = cycles(handle), time.perf_counter()
                time.sleep(10)
                c1, t1 = cycles(handle), time.perf_counter()
                private, ws = memory(handle)
                with pdh.Counters([rf"\GPU Process Memory(pid_{app.pid}_*)\Dedicated Usage",
                                   rf"\GPU Process Memory(pid_{app.pid}_*)\Shared Usage"]) as counters:
                    dedicated, shared = counters.sample()
            except Exception as error:  # noqa: BLE001
                print(f"{r} {name:16} failed: {error}", flush=True)
                continue
            finally:
                app.kill()
                app.wait()
                kernel32.CloseHandle(handle)
            first = [line.split()[1] for line in open(log_path, errors="replace") if line.startswith("FIRST")]
            row = {"mcyc": (c1 - c0) / (t1 - t0) / 1e6, "private": private / 2**20, "ws": ws / 2**20,
                   "dedicated": dedicated / 2**20, "shared": shared / 2**20,
                   "first": float(first[0]) if first else None}
            found.setdefault(name, []).append(row)
            print(f"{r} {name:16} cyc {row['mcyc']:6.2f} M/s private {row['private']:6.1f} ws {row['ws']:6.1f} "
                  f"gpu ded {row['dedicated']:6.1f} shared {row['shared']:6.1f} MiB first {row['first']}", flush=True)
print("\nmedians: | target | Mcyc/s | private MiB | working set MiB | GPU dedicated MiB | GPU shared MiB | first present ms |")
for name, *_ in TARGETS:
    rows = found.get(name, [])
    med = lambda k: statistics.median([x[k] for x in rows if x[k] is not None] or [float("nan")])  # noqa: E731
    print(f"| {name} | {med('mcyc'):.2f} | {med('private'):.1f} | {med('ws'):.1f} | {med('dedicated'):.1f} | "
          f"{med('shared'):.1f} | {med('first'):.0f} |")
