"""Scratch: the bare API floor's GPU time (d3d_frame_floor: a cleared frame
presented each compositor frame, nothing drawn), Direct3D 11 against 12,
PDH 3D-engine running time over 8 settled seconds, rounds alternating.

    python floor_gpu.py <d3d_frame_floor.exe> <rounds>"""
import statistics
import subprocess
import sys
import time

sys.path.insert(0, r"C:\Users\wing_\Documents\GitHub\zgui-d3d11\scripts")
import windows_compare as wc  # noqa: E402
import windows_input as win  # noqa: E402
import windows_pdh as pdh  # noqa: E402

exe, rounds = sys.argv[1], int(sys.argv[2])
modes = ["d3d12", "d3d11"]
found = {m: [] for m in modes}
with wc.PowerPlan(wc.HIGH_PERFORMANCE), wc.Minimized(["Zeron"]):
    for r in range(rounds):
        for mode in modes[r % 2:] + modes[:r % 2]:
            app = subprocess.Popen([exe, mode, "1200"], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
            try:
                win.activate(win.wait_window(app, timeout=15))
                time.sleep(4)
                with pdh.Counters([pdh.gpu_time(app.pid)]) as counters:
                    first, start = counters.sample(), time.perf_counter()
                    time.sleep(8)
                    last, wall = counters.sample(), time.perf_counter() - start
                ms = (last[0] - first[0]) / 1e4 / wall
                found[mode].append(ms)
                print(f"{r} {mode}: app GPU {ms:.3f} ms/s ({ms / 60 * 1000:.1f} us a frame at 60 Hz)", flush=True)
            finally:
                app.kill()
                app.wait()
for mode in modes:
    print(f"{mode}: median {statistics.median(found[mode]):.3f} ms/s")
