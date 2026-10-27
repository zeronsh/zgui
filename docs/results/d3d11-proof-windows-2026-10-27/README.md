# Direct3D 11 against Direct3D 12 on Windows (2026-10-27)

zgx-gpu's experimental Direct3D 11 path (the `d3d11` feature, `ZGX_D3D11=1`;
`crates/zgx-gpu/src/d3d11_frame.rs`) measured against the default, wgpu on
Direct3D 12, on WING-PC: an RTX 4090 (driver 617.42, NVIDIA only), an
i7-12700KF, a 60 Hz display, Windows 11. Every column of every run is the same
binary, the API chosen by the environment.

**Verdict.** Direct3D 11 uses about half the memory, 15–36% less CPU and
starts 19% sooner. Pacing, the compositor's work and board power are level.
The GPU spends about 10 µs more a frame, and that is Direct3D 11's own cost on
this driver: an app that only clears and presents pays it too. The frames are
byte for byte the same.

## The suite (`suite/`)

`scripts/run_windows_suite.py --frameworks zgx,zgx-d3d11,zgx-d3d11-threaded`,
the suite's protocol on every scene and startup, all accepted. `zgx-d3d11`
leaves the driver's own threading off (as GPUI's renderer runs), and
`zgx-d3d11-threaded` turns it on (`ZGX_D3D11_THREADED=1`). The binaries were
built from `d3da4c4d` plus `suite/source.diff`, the `dist` profile with
zgx-desktop's `d3d11` feature. Medians; the change is Direct3D 11's, threading
off, against Direct3D 12:

| Scene, mode | App CPU (M cycles/s) | App GPU (ms/s) | DWM CPU (M cycles/s) | Private (MiB) | Frame p99 (ms) |
| --- | --- | --- | --- | --- | --- |
| heavy idle | 2.03 → 1.08 (−47%) | 0 → 0 | 0.04 → 0.29 (noise, below) | 146 → 67 | - |
| heavy stream | 279 → 237 (−15%) | 11.0 → 9.8 (−11%) | 122 → 125 | 152 → 69 | 17.4 → 17.3 |
| heavy scroll | 123 → 86 (−30%) | 2.12 → 2.93 (+38%) | 122 → 119 | 148 → 67 | 17.2 → 17.3 |
| heavy both | 329 → 267 (−19%) | 13.5 → 11.9 (−12%) | 134 → 118 | 153 → 71 | 17.3 → 17.3 |
| lab stream | 94 → 63 (−33%) | 3.14 → 3.96 (+26%) | 90 → 94 | 126 → 62 | 17.4 → 17.3 |
| lab scroll | 89 → 57 (−36%) | 2.02 → 2.55 (+26%) | 92 → 93 | 125 → 60 | 17.3 → 17.3 |
| lab both | 111 → 75 (−32%) | 3.58 → 4.21 (+18%) | 97 → 94 | 126 → 62 | 17.3 → 17.3 |
| animate | 95 → 67 (−30%) | 2.05 → 2.69 (+31%) | 99 → 98 | 125 → 60 | 17.2 → 17.4 |
| vlist scroll | 92 → 61 (−34%) | 2.77 → 3.16 (+14%) | 110 → 112 | 139 → 67 | 17.3 → 17.3 |
| vlist fling | 92 → 69 (−25%) | 2.85 → 3.47 (+22%) | 96 → 101 | 139 → 67 | 33.1 → 33.0 |
| vlist jump | 92 → 66 (−28%) | 2.94 → 3.55 (+21%) | 99 → 105 | 134 → 67 | 17.3 → 17.2 |

Startup (lab, medians of 10 launches), to window / to content / cycles by
then: Direct3D 12 237 / 280 ms / 814 M; Direct3D 11 193 / 227 ms / 683 M;
threaded 214 / 245 ms / 703 M. Board power: −4% to +3%. The full table,
threading on included, is `suite/table.md` (`tools/table.py`).

With the driver's threading on, the memory and startup wins hold (65 MiB,
245 ms) but the CPU win goes (−4% to +15% against Direct3D 12): off is the
mode.

## At rest, against the bare floors (`floors/`)

Each launched in front and left idle, 10 s sampled after 5 s, medians of 3
rounds (`tools/floors_d3d11.py`, from the Windows lane's `floors.py`; the bare
windows are its `memsteps`: one cleared frame, then a blocking message pump):

| | Cycles (M/s) | Private (MiB) | Working set (MiB) | GPU dedicated (MiB) | GPU shared (MiB) |
| --- | --- | --- | --- | --- | --- |
| bare Direct3D 11 | 1.45 | 55.2 | 38.6 | 16.3 | 0.8 |
| bare Direct3D 12 | 1.88 | 93.2 | 71.3 | 28.1 | 25.2 |
| zgx lab, Direct3D 12 | 1.67 | 124.4 | 93.7 | 38.5 | 31.2 |
| zgx lab, Direct3D 11 | 1.43 | 61.1 | 50.9 | 20.2 | 2.2 |
| zgx heavy, Direct3D 12 | 1.85 | 145.1 | 114.2 | 41.7 | 47.2 |
| zgx heavy, Direct3D 11 | 1.38 | 67.0 | 56.0 | 23.4 | 2.6 |

zgx on Direct3D 11 sits 6 MiB over a bare Direct3D 11 window.

The idle DWM reading above (0.29) was one of the suite's two trials. Heavy's
idle trial alone, 4 times each (`idle-dwm/`, `tools/idle_dwm.py`, the suite's
own trial code), medians and ranges in M cycles/s: the desktop alone 0.072
(0.057–0.585), Direct3D 12 0.137 (0.035–0.547), Direct3D 11 0.048
(0.036–0.111), threaded 0.038 (0.037–0.043). DWM wakes now and then whatever
runs; nothing presents at rest.

## The GPU's extra 10 µs a frame is Direct3D 11's own

`crates/zgx-gpu/examples/d3d_frame_floor.rs` clears a 1280×800 flip-model
swap chain (DirectComposition, 3 buffers) and presents it each compositor
frame, drawing nothing. Its app GPU time (PDH 3D-engine running time, 8 s
settled, rounds alternating; `tools/floor_gpu.py`):

| Swap effect | Direct3D 12 | Direct3D 11 |
| --- | --- | --- |
| flip-sequential | 1.82 ms/s (30 µs a frame) | 2.42 ms/s (40 µs a frame) |
| flip-discard | 1.81 ms/s | 2.42 ms/s |

Direct3D 11 costs +0.60 ms/s, about 10 µs a frame, with nothing drawn; zgx's
gap is the same size whatever a frame draws (92 quads or 600): +0.43 ms/s in
lab stream, +0.63 in animate (`gpu-ab/`, the suite's trial code, 3 each).
Both ran at the GPU's lowest clock (SM 210 MHz, read through NVML): these
scenes never raise it, and at any higher clock the 10 µs is less.

Nothing on zgx's side changed it (`gpu-ab/`, app GPU ms/s, lab stream /
animate):
- as measured: 3.75 / 2.68;
- the swap chain's frame latency at 1, the frame waiting on the CPU for its
  buffer: 3.92 / 2.67;
- clip regions written every frame rather than when they change (kept: the
  CPU saves the copy): 3.67 / 2.68;
- under GPU timestamps, vertices in GPU-local buffers rather than the
  dynamic ring, and catch-up copied rather than drawn or read from a copy
  of zgx's own rather than the shown buffer: no change to the frame's draws.

Direct3D 11 timestamps spanning a frame are not to be trusted here: the
driver may submit a frame in pieces, and the idle gap between them reads as
the frame's time (milliseconds, in some launches). PDH counts busy time only.

## Pixels

Every frame Direct3D 11 draws is byte for byte Direct3D 12's: lab, heavy,
animate and vlist (fling, jump) drawn offscreen as their windows draw them,
read back every tick, 240 frames each at 1× and 120 at 1.5× and 2×
(`crates/zgx-desktop/examples/render_parity.rs`), and
`crates/zgx-gpu/tests/d3d11_parity.rs` in CI's terms. The shaders are the same
WGSL through naga and FXC (`draw.hlsl`, `dxbc/`).
`scripts/windows_present_probe.py` passes on Direct3D 11's swap chain, with
dirty rectangles and whole.

## What it costs to have wgpu without a driver

wgpu runs on its no-op backend, so `GpuRenderer`'s CPU paths are Direct3D 12's
own: about 0.6 MiB and 1 ms at startup, and 0.0% of a 60 s sampled profile of
heavy's steady frames on Direct3D 11 (16,470 samples) in wgpu, wgpu-core,
wgpu-hal or naga. Every number above includes it.

## Not measured

AMD and Intel: their drivers price Direct3D 11's present, and Direct3D 12's
device, differently. Whatever is drawn beyond the suite's scenes (images,
layers, blur, polylines, shader fills): the path refuses those frames.
