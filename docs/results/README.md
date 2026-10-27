# Windows suite (2026-10-27): every scene on `a065781`

[`suite-windows-2026-10-27-a065781`](suite-windows-2026-10-27-a065781/): the same protocol, machine and display as the run below, on `a065781`, after Windows opened its GPU device beside the first window (#714) and presented timed frames clear of the compositor's latch (#734). Every scene accepted, no app exiting abnormally. GPUI Fast was rebuilt (its comparison app now sets the lab's menu bar, `03a9d895`); MyGo's and Rune's sources are unchanged, and their binaries too. `python scripts/build_site.py docs/results/suite-windows-2026-10-27-a065781 --platform windows --startup docs/results/suite-windows-2026-10-27-a065781/startup/startup.json` publishes it to `site/windows/`.

Medians, zgx / GPUI Fast / MyGo / Rune (vlist: zgx / zgx in Luau / GPUI Fast / MyGo / Rune):

| Scene, mode | CPU (M cycles/s) | App GPU (ms/s) | Private memory (MB) | Frame time p99 (ms) |
| --- | --- | --- | --- | --- |
| heavy idle | 1.3 / 10 / 0.8 / 4.1 | 0 / 0 / 0 / 0 | 153 / 106 / 132 / 378 | - |
| heavy stream | 255 / 548 / 798 / 1051 | 11.1 / 12.6 / 19.0 / 10.3 | 159 / 112 / 153 / 391 | 17.2 / 33.6 / 17.4 / 17.4 |
| heavy scroll | 107 / 256 / 629 / 1161 | 2.3 / 12.7 / 17.0 / 10.2 | 155 / 109 / 143 / 394 | 17.3 / 33.5 / 17.3 / 17.4 |
| heavy both | 299 / 563 / 775 / 1327 | 13.3 / 12.6 / 18.7 / 10.3 | 160 / 112 / 152 / 396 | 17.4 / 33.6 / 17.3 / 17.4 |
| lab stream | 125 / 177 / 414 / 242 | 3.4 / 6.3 / 6.4 / 7.1 | 133 / 94 / 135 / 378 | 17.2 / 33.6 / 17.3 / 17.5 |
| lab scroll | 90 / 127 / 317 / 213 | 1.9 / 4.8 / 8.9 / 6.9 | 132 / 91 / 127 / 375 | 17.4 / 33.5 / 17.6 / 17.4 |
| lab both | 107 / 177 / 404 / 247 | 3.5 / 6.6 / 11.6 / 7.2 | 133 / 95 / 135 / 379 | 33.2 / 33.6 / 17.3 / 17.4 |
| animate | 98 / 102 / 268 / 149 | 2.0 / 3.3 / 3.4 / 5.2 | 131 / 91 / 121 / 370 | 17.3 / 17.3 / 17.4 / 17.5 |
| vlist scroll | 94 / 99 / 140 / 409 / 274 | 2.6 / 2.8 / 7.3 / 12.6 / 6.5 | 146 / 147 / 109 / 142 / 392 | 17.3 / 17.3 / 33.5 / 33.6 / 17.5 |
| vlist fling | 97 / 104 / 151 / 449 / 364 | 2.8 / 2.8 / 7.5 / 12.3 / 6.6 | 146 / 148 / 110 / 144 / 395 | 33.2 / 33.4 / 33.7 / 33.6 / 17.4 |
| vlist jump | 102 / 112 / 154 / 427 / 438 | 2.9 / 2.9 / 7.7 / 13.3 / 6.7 | 141 / 144 / 110 / 150 / 396 | 17.3 / 17.3 / 33.5 / 33.4 / 17.4 |

Startup (lab, medians of 10 launches): first content at 290 / 312 / 261 / 625 ms, 818 / 918 / 768 / 1937 M cycles by then. zgx's went from 323 ms and 947 M cycles: its GPU device now opens beside the first window (#714). It shows its window with its first frame (249 ms); MyGo and Rune show theirs at 45–51 ms and draw into it later.

zgx uses the fewest CPU cycles in all 10 active tests and the least app GPU time in 8. Where it is not best, its worst ratio against the best rival per metric:
- **Idle CPU:** 1.58× MyGo's (1.3 against 0.8 M cycles/s; NVIDIA's Direct3D 12 driver thread, below).
- **Frame time p99:** 1.92× MyGo's in lab both (33.2 against 17.3 ms), and long frames 7.1× there (1.5 against 0.2%). These are the capture's misses, not zgx's: two of the three captures merged compositions (8 and 6 merged frames), and their long intervals (8 and 7) all but match them, so at most one was a refresh zgx missed. Merged frames come up for every framework, 6–23 in each scene's captures.
- **Memory:** 1.44× GPUI Fast's (Direct3D 12 against Direct3D 11, below).
- **App GPU time:** 1.29× Rune's in heavy both.
- **DWM:** CPU 1.29× and GPU 1.04×.
- **Startup to first content:** 1.11× MyGo's.

Against `9562c78`, for zgx:
- The heavy scene takes 7–9% fewer cycles (stream 279 → 255, both 322 → 299).
- Long frames: lab stream 0.6 → 0%, lab scroll 3.5 → 0.2%. The latch (#734) ended the runs of missed refreshes.
- DWM's CPU while zgx runs did not move with #734 (animate 2.79 → 2.80%). Its worst ratio fell from 1.63× because the rivals' tick-counted readings moved.
- Lab stream's median reads 125 M cycles/s against 101, but that is between launches, not builds. Its repeats here were 100, 123, 128 and 132. Alternating `9562c78`'s binary with this one, six rounds each, read 110 and 108 (all twelve within 102–113).

# Windows suite (2026-10-26): every scene on `9562c78`

[`suite-windows-2026-10-26-9562c78`](suite-windows-2026-10-26-9562c78/): the macOS suite's scenes, frameworks and protocol on Windows (`scripts/run_windows_suite.py`, `docs/benchmarking.md`, On Windows), every scene accepted, no app exiting abnormally. An i7-12700KF with an RTX 4090 (driver 617.42), Windows 11 build 26200, a 60 Hz 1920×1080 display at 100%, the High performance plan held; a remote display host (Apollo) streamed the desktop throughout. `python scripts/build_site.py --platform windows` publishes it to `site/windows/`.

CPU is the process's cycles (Windows' CPU time counts 15.6 ms scheduler ticks: too coarse here), in millions a second; the GPU figure is the app's time on the 3D engines, in ms a second (Windows reads no system GPU power; the NVIDIA board's power, recorded in every trial, moved 2–50% between repeats and separated neighbouring frameworks in 3 of 33 pairs, so it is not shown). Medians, zgx / GPUI Fast / MyGo / Rune (vlist: zgx / zgx in Luau / GPUI Fast / MyGo / Rune):

| Scene, mode | CPU (M cycles/s) | App GPU (ms/s) | Private memory (MB) | Frame time p99 (ms) |
| --- | --- | --- | --- | --- |
| heavy idle | 1.7 / 9.5 / 0.9 / 4.1 | 0 / 0 / 0 / 0 | 152 / 107 / 131 / 396 | - |
| heavy stream | 279 / 550 / 730 / 1106 | 11.0 / 12.6 / 19.2 / 10.3 | 155 / 112 / 152 / 389 | 17.3 / 33.6 / 17.3 / 17.4 |
| heavy scroll | 109 / 265 / 643 / 1194 | 2.1 / 12.6 / 17.0 / 10.3 | 152 / 110 / 143 / 396 | 17.3 / 33.8 / 17.3 / 17.4 |
| heavy both | 322 / 572 / 791 / 1384 | 13.4 / 12.7 / 18.6 / 10.2 | 156 / 113 / 153 / 395 | 17.3 / 33.6 / 17.4 / 17.4 |
| lab stream | 101 / 163 / 397 / 254 | 3.2 / 6.3 / 6.4 / 7.3 | 130 / 94 / 136 / 377 | 17.4 / 34.1 / 17.4 / 17.5 |
| lab scroll | 87 / 120 / 311 / 221 | 1.8 / 4.8 / 8.9 / 7.0 | 129 / 92 / 125 / 374 | 33.4 / 33.6 / 17.5 / 17.6 |
| lab both | 106 / 174 / 405 / 251 | 3.4 / 6.4 / 11.6 / 7.2 | 130 / 95 / 134 / 380 | 17.3 / 33.6 / 17.6 / 17.4 |
| animate | 96 / 100 / 260 / 135 | 2.0 / 3.4 / 3.4 / 5.3 | 129 / 91 / 121 / 373 | 17.3 / 17.4 / 17.4 / 17.4 |
| vlist scroll | 87 / 91 / 137 / 434 / 290 | 2.6 / 2.6 / 7.6 / 12.2 / 6.6 | 141 / 143 / 109 / 144 / 391 | 17.4 / 17.3 / 33.5 / 33.4 / 17.4 |
| vlist fling | 95 / 94 / 140 / 446 / 357 | 2.8 / 2.8 / 7.5 / 12.3 / 6.6 | 143 / 145 / 109 / 144 / 394 | 33.4 / 33.4 / 33.6 / 33.6 / 17.4 |
| vlist jump | 93 / 98 / 149 / 430 / 416 | 2.9 / 2.9 / 7.7 / 13.1 / 6.7 | 135 / 139 / 110 / 149 / 394 | 17.4 / 17.4 / 33.5 / 17.5 / 17.4 |

Startup (lab, in the suite's font, medians of 10 launches, measured after the suite): first content at 323 / 299 / 234 / 605 ms, 947 / 891 / 724 / 1902 M cycles by then. zgx shows its window with its first frame (253 ms); MyGo and Rune show theirs at 38–40 ms and draw into it later. Creating the Direct3D device takes most of zgx's: 190 ms for a bare Direct3D 12 device on this machine, 170 ms for Direct3D 11.

zgx uses the fewest CPU cycles in all 10 active tests and the least app GPU time in 8. Where it is not best, its worst ratio against the best rival per metric: idle CPU 1.95× MyGo's (1.7 against 0.9 M cycles/s: zgx's own threads use none at rest; it is NVIDIA's Direct3D 12 driver's background thread, which a bare Direct3D 12 device runs too, and which only developer mode can turn off); frame time p99 1.92× Rune's in the vlist fling (33.4 against 17.4 ms: whole-pixel scrolling in the fling's slow tail, below); memory 1.43× GPUI Fast's (152 against 107 MB; zgx and Rune load Direct3D 12, GPUI Fast and MyGo Direct3D 11, whose drivers' floors differ); app GPU time 1.31× Rune's in heavy both (13.4 against 10.2 ms/s); long frames 8.2× MyGo's in lab scroll (3.5 against 0.4%: some of zgx's captures miss refreshes for seconds at a time, with nothing merged: 84 long frames in one lab stream capture, 35 in one heavy both; under investigation); DWM CPU 1.63× (tick-counted, wide ranges) and DWM GPU 1.06×; startup to first content 1.38× MyGo's (323 against 234 ms). The animation runs at this display's 60 Hz, not the Mac's 120.

The vlist fling's long frames are whole-pixel scrolling, not missed frames. The fling is 160 px a tick decaying ×0.94 a tick over 90 ticks, so its last ticks move under a pixel. zgx scrolls in whole device pixels (as Chrome and Windows' own lists do, for crisp text), so in that tail some refreshes change nothing on screen. Captured alongside zgx's own logs: every refresh had one tick, one render and one present the compositor took; each frame's GPU work was done 0.1–0.2 ms after its present; and the capture saw no composition at all in each long interval. The present mode made no difference (own swap chain at sync interval 0 or 1, wgpu mailbox or FIFO: 1.1–3.3% long frames each). Steady scrolling at a fixed step decides it:

| Step a tick | Long frames (two captures) | Frames a second |
| --- | --- | --- |
| 12 px | 2.1%, 0% | 58–60 |
| 3 px | 0.2%, 0% | 60 |
| 1.3 px | 2.2%, 0.6% | 58–60 |
| 0.6 px | 66.8%, 66.6% | 36 |

Rune draws at fractional positions, so its pixels change every tick. (These captures predate `framepace`'s word hash; the 12 and 1.3 px residue is its old merging, below.)

The pacing captures were taken again (`--pacing-into`, the trials kept, the same binaries): `framepace` first hashed each frame byte by byte and fell behind at the dashboard's 1280×800, merging compositions (`AccumulatedFrames` > 1) that then read as long frames, for every framework (heavy stream: zgx 1.1%, all merged). It now hashes words and reports merged frames (`merged_frames`), about 0 since.

# Republished (2026-10-25): every macOS scene on `282bfc0`

Lab, heavy and animate measured again on zgx `282bfc0` with the same builds of GPUI, GPUI Fast and QuickGUI and the same protocol (dist builds, three trials per active cell, five of idle sampled 20–35 s after launch, pacing captured apart): [`latest-macos-2026-10-25-282bfc0`](latest-macos-2026-10-25-282bfc0/), [`latest-macos-heavy-2026-10-25-282bfc0`](latest-macos-heavy-2026-10-25-282bfc0/), [`latest-macos-animate-2026-10-25-282bfc0`](latest-macos-animate-2026-10-25-282bfc0/). All three accepted; the site and the README show them. Medians; memory in MB, idle with the five launches' range.

Lab, zgx / GPUI / GPUI Fast / QuickGUI:

| Mode | CPU (% of a core) | GPU (Mcycles/s) | Memory (MB) |
| --- | --- | --- | --- |
| idle | 0.00 / 0.38 / 0.40 / 0.00 | 0 / 0 / 0 / 0 | 67 (55–67) / 89 (64–96) / 53 (53–79) / 65 (57–116) |
| scroll | 2.9 / 4.9 / 4.5 / 8.4 | 10.9 / 18.0 / 18.1 / 23.9 | 236 / 246 / 236 / 413 |
| stream | 3.2 / 6.8 / 5.1 / 15.3 | 11.3 / 18.3 / 18.2 / 24.4 | 236 / 247 / 239 / 411 |
| both | 3.6 / 7.0 / 5.9 / 16.0 | 11.6 / 18.1 / 18.2 / 24.2 | 237 / 248 / 239 / 415 |

Heavy, zgx / GPUI / GPUI Fast:

| Mode | CPU (% of a core) | GPU (Mcycles/s) | Memory (MB) |
| --- | --- | --- | --- |
| idle | 0.00 / 0.38 / 0.40 | 0 / 0 / 0 | 59 (53–79) / 88 (88–105) / 93 (89–95) |
| scroll | 3.2 / 14.6 / 8.7 | 11.5 / 21.6 / 21.9 | 255 / 272 / 256 |
| stream | 8.5 / 22.0 / 13.9 | 16.8 / 22.1 / 21.7 | 259 / 272 / 275 |
| both | 9.1 / 23.0 / 15.9 | 16.7 / 22.0 / 21.6 | 259 / 272 / 274 |

Animate, zgx / GPUI / GPUI Fast: CPU 5.1 / 7.8 / 5.8%, GPU 20.5 / 28.0 / 27.5 Mcycles/s, memory 247 / 249 / 248 MB, 119.3 / 119.8 / 119.4 fps.

Against `7d559ad`: zgx's GPU work fell in every active cell of lab (−4 to −10%) and heavy (−14% in `both`, −5% in `stream` and `scroll`): damage all over a window is drawn through a stencil of itself, so the GPU shades what changed and not the regions around it (#382). Animate, which the stencil does not apply to, is level. zgx's CPU is level in all three (within each cell's range). In lab, zgx's memory in the active modes is a tie with GPUI Fast's (236–237 MB against 236–239; 225–226 against 225–228 MiB), and zgx's idle median moved from 42 to 67 MB because three of its five launches settled at the upper of the two levels this time: both runs' ranges include the upper level (about 63 MiB), so the idle difference is the driver's timing, not a change in zgx. GPUI Fast's idle moved the other way in heavy (70 → 93 MB) for the same reason.

# Correction (2026-10-24): macOS runs of 2026-10-23/24 built zgui with mimalloc

The macOS comparisons published on 2026-10-23 and 2026-10-24 at `51a669c` (lab, heavy and animate) built zgui with **mimalloc** as its allocator, unintentionally, while GPUI, GPUI Fast and QuickGUI used the system allocator. zgui-desktop had `zgui-run` as a dev-dependency, and `zgui-run` turns on mimalloc by default; Cargo unifies features, so every zgui-desktop example (the comparison workloads among them) was built with it from `919b719` on. #332 fixed the dependency. All three scenes were re-run on `7d559ad` (#332 included; the zgui binaries were checked to contain no mimalloc) with the same protocol: dist builds, three trials per cell, a quiet desktop. The site and the README now show the corrected runs; the old runs stay in place, each with a `CORRECTION.md`.

What changed, medians, old → new (CPU % of one core; GPU megacycles a second; mean footprint in MB; the window server's CPU less the desktop alone's):

### Lab: [`latest-macos-2026-10-23-51a669c`](latest-macos-2026-10-23-51a669c/) → [`latest-macos-2026-10-24-7d559ad`](latest-macos-2026-10-24-7d559ad/)
| Framework | Mode | CPU % old → new | GPU Mcyc/s old → new | memory MB old → new | WS CPU % (+) old → new |
| --- | --- | --- | --- | --- | --- |
| zgui | both | 4.35 → 3.68 | 13.0 → 12.9 | 238 → 238 | 7.4 → 17.5 |
| zgui | idle | 0.06 → 0.00 | 0.0 → 0.0 | 98 → 104 | -1.0 → -0.0 |
| zgui | scroll | 3.47 → 2.73 | 11.3 → 12.1 | 236 → 236 | 8.3 → 18.2 |
| zgui | stream | 4.10 → 3.24 | 11.2 → 11.8 | 237 → 236 | 3.3 → 18.0 |
| gpui | both | 7.23 → 6.99 | 16.0 → 18.1 | 251 → 249 | 10.1 → 16.6 |
| gpui | idle | 0.43 → 0.38 | 0.0 → 0.0 | 132 → 106 | -0.8 → 0.4 |
| gpui | scroll | 5.56 → 5.10 | 13.4 → 17.9 | 246 → 246 | 5.0 → 16.1 |
| gpui | stream | 7.14 → 6.50 | 18.4 → 18.1 | 250 → 249 | -2.4 → 16.7 |
| gpui-fast | both | 6.18 → 5.83 | 15.5 → 18.1 | 242 → 239 | 6.8 → 16.4 |
| gpui-fast | idle | 0.44 → 0.40 | 0.0 → 0.0 | 121 → 95 | -0.6 → -0.0 |
| gpui-fast | scroll | 4.72 → 4.44 | 13.6 → 18.1 | 242 → 236 | 3.8 → 17.4 |
| gpui-fast | stream | 5.89 → 5.48 | 18.5 → 18.3 | 250 → 238 | 2.8 → 16.1 |
| quickgui | both | 16.97 → 15.71 | 21.2 → 24.2 | 415 → 415 | -0.5 → 17.2 |
| quickgui | idle | 0.07 → 0.01 | 0.0 → 0.0 | 173 → 136 | -1.0 → -0.0 |
| quickgui | scroll | 9.73 → 8.95 | 24.4 → 24.1 | 402 → 413 | 0.8 → 17.3 |
| quickgui | stream | 15.58 → 14.98 | 18.1 → 24.2 | 411 → 414 | 1.2 → 16.7 |

### Heavy: [`latest-macos-heavy-2026-10-23-51a669c`](latest-macos-heavy-2026-10-23-51a669c/) → [`latest-macos-heavy-2026-10-24-7d559ad`](latest-macos-heavy-2026-10-24-7d559ad/)
| Framework | Mode | CPU % old → new | GPU Mcyc/s old → new | memory MB old → new | WS CPU % (+) old → new |
| --- | --- | --- | --- | --- | --- |
| zgui | both | 10.83 → 9.85 | 17.7 → 19.8 | 262 → 260 | 8.1 → 16.2 |
| zgui | idle | 0.07 → 0.00 | 0.0 → 0.0 | 107 → 116 | 7.7 → -0.0 |
| zgui | scroll | 3.43 → 3.24 | 10.7 → 12.3 | 255 → 255 | 11.1 → 17.5 |
| zgui | stream | 9.80 → 8.84 | 11.8 → 16.5 | 261 → 259 | 15.3 → 17.3 |
| gpui | both | 22.05 → 22.92 | 18.9 → 22.4 | 276 → 273 | 5.9 → 15.8 |
| gpui | idle | 0.42 → 0.35 | 0.0 → 0.0 | 157 → 137 | -1.4 → 0.3 |
| gpui | scroll | 14.89 → 15.96 | 21.2 → 21.7 | 273 → 273 | 10.7 → 17.2 |
| gpui | stream | 21.49 → 22.71 | 17.6 → 22.1 | 274 → 273 | 6.9 → 15.7 |
| gpui-fast | both | 14.89 → 15.65 | 21.0 → 21.6 | 276 → 273 | 9.5 → 17.1 |
| gpui-fast | idle | 0.43 → 0.38 | 0.0 → 0.0 | 129 → 111 | 5.1 → 0.1 |
| gpui-fast | scroll | 8.01 → 8.53 | 21.3 → 21.9 | 273 → 261 | 14.6 → 17.4 |
| gpui-fast | stream | 14.27 → 15.27 | 21.3 → 21.6 | 276 → 259 | 11.3 → 16.9 |

### Animate: [`latest-macos-animate-2026-10-24-51a669c`](latest-macos-animate-2026-10-24-51a669c/) → [`latest-macos-animate-2026-10-24-7d559ad`](latest-macos-animate-2026-10-24-7d559ad/)
| Framework | Mode | CPU % old → new | GPU Mcyc/s old → new | memory MB old → new | WS CPU % (+) old → new |
| --- | --- | --- | --- | --- | --- |
| zgui | animate | 5.43 → 5.02 | 17.6 → 20.6 | 247 → 248 | 20.3 → 37.6 |
| gpui | animate | 7.19 → 7.64 | 17.4 → 27.7 | 248 → 249 | 20.6 → 37.3 |
| gpui-fast | animate | 5.45 → 5.84 | 17.8 → 27.8 | 249 → 250 | 20.2 → 37.3 |


The idle cells of the lab and heavy re-runs were then measured again with the protocol that followed (2026-10-24, zgui at `81c73a7`): 20–35 s after launch, five launches, median and range. In MB, zgui / GPUI / GPUI Fast (/ QuickGUI): lab 42 (41–66) / 89 (64–89) / 60 (53–78) / 116; heavy 78 (63–78) / 113 (98–114) / 70 (70–95). An idle window settles at one of two levels about 25 MB apart; in heavy, zgui's levels (63, 78) are below GPUI Fast's (70, 95), but three of zgui's launches settled on its upper one and four of GPUI Fast's on its lower. The first idle trials (5–20 s, once) stay in each run's `idle-5-20s/`, and each new trial keeps its 5–20 s figures as `early_*`.

Heavy's active cells were measured again the same day with zgui at `81c73a7` (after the memory work of #354–#361; the other frameworks' binaries unchanged): zgui 9.3 / 8.8 / 3.2% CPU (both / stream / scroll) and 260 / 259 / 254 MB; GPUI 22.7 / 22.3 / 14.8% and 273 / 272 / 273; GPUI Fast 16.1 / 14.8 / 8.5% and 275 / 274 / 272. zgui's footprint is unchanged by that work (its live heap fell 3 MiB, but fragmentation took most of it back). GPUI Fast's `stream` and `scroll` memory moved by about 15 MB between the two sessions (259 and 261 MB, then 274 and 272): read its memory cells with that spread. The first active trials stay in each mode's `<mode>-before/`.

**PGO supplement** (2026-10-24, zgui at `c9975e1`): every framework built with profile-guided optimization, each app trained on its own scene (`scripts/perf/pgo_compare.py`), measured without pacing captures or idle. Medians of three, zgui / GPUI / GPUI Fast:

| Scene, mode | CPU % of a core | GPU Mcycles/s | Memory MB |
| --- | --- | --- | --- |
| heavy, both | 8.57 / 21.53 / 14.42 | 19.2 / 22.3 / 21.6 | 260 / 274 / 276 |
| heavy, stream | 7.71 / 20.61 / 13.92 | 17.6 / 22.2 / 21.7 | 259 / 273 / 275 |
| heavy, scroll | 2.98 / 14.63 / 8.06 | 12.3 / 21.6 / 21.8 | 255 / 273 / 255 |
| lab, both | 3.36 / 6.29 / 5.41 | 12.9 / 18.2 / 18.3 | 237 / 248 / 247 |
| lab, stream | 2.96 / 6.36 / 5.40 | 11.5 / 18.2 / 18.1 | 236 / 249 / 240 |
| lab, scroll | 2.54 / 4.72 / 4.17 | 12.1 / 17.9 / 18.1 | 236 / 246 / 237 |
| animate | 4.59 / 7.21 / 5.62 | 20.5 / 28.0 / 27.8 | 247 / 249 / 249 |

Against the plain heavy run of the same session, PGO took CPU down 7.6 / 12 / 7% for zgui (both / stream / scroll), 5 / 8 / 1% for GPUI and 10 / 6 / 5% for GPUI Fast; GPU and memory barely moved. Runs: [`latest-macos-pgo-2026-10-24-c9975e1`](latest-macos-pgo-2026-10-24-c9975e1/), [`latest-macos-heavy-pgo-2026-10-24-c9975e1`](latest-macos-heavy-pgo-2026-10-24-c9975e1/), [`latest-macos-animate-pgo-2026-10-24-c9975e1`](latest-macos-animate-pgo-2026-10-24-c9975e1/).

zgui leads on CPU and GPU in every active cell of both runs. In memory, the new heavy run shows GPUI Fast lower than zgui at idle (111 vs 116 MB, where the driver's return of its startup memory decides the 5–20 s mean) and level with it in `stream` (259 vs 259). The window server's added CPU reads 15–18% for every framework on 2026-10-24, against at most 15% on 2026-10-23: the desktop differed between the days (every framework moved), so compare within a run.

# Historical direct-scene GPU comparison on Xvfb / llvmpipe

The latest [parity-expansion component series](gpui-parity-workers4/README.md)
passes all 36 trials and the unchanged logical-update gate. Its report includes
CPU/RSS medians and ranges, 10,808 audited samples, source/binary proofs, plots
and shared-host limitations. The [preceding four-worker series](measured-framework-workers4/README.md)
and historical direct-scene measurements below remain separate.


The [historical retry](refined-framework-retry/README.md) completed all 36 processes but failed delivered-update parity; it is not accepted performance evidence. The [first refresh attempt](refined-framework/README.md) stopped after a failed process amid disk exhaustion. Both artifact sets are preserved separately.

The newer [public component release comparison](component-comparison/README.md) measures the component/slot/style/virtual-list API with normalized reference adapters. This page preserves the earlier direct-scene series; its timings do not measure component overhead.

These are native-window measurements of the matched streaming-text and 100,000-row virtual-list applications. They are **software-GPU environment measurements, not hardware-GPU rankings**. All windows were 960 × 720 logical pixels at scale 1 on Xvfb `:96` (1280 × 900) with Openbox. The available Vulkan device was Mesa llvmpipe (LLVM 21.1.8, Mesa 26.0.8); zgui explicitly logged that CPU adapter and its GPU renderer. GPUI and QuickGUI used their GPU rendering paths. Native frozen-state screenshots were inspected for all three applications before measurement; text baselines/rasterization are close but not pixel-identical.

Each series was configured for all four modes, three frameworks, **two repeats, 10 seconds per process, 3 seconds of warmup excluded**, sampling process CPU time and RSS every 50 ms. Framework order rotates across repeats. Team builds and native tests were paused during measurement, but this is a shared machine without CPU pinning or thermal isolation. Two short repeats show observed spread, not statistical confidence. Idle zeros mean no CPU tick increment at the sampler's resolution.

## Archived direct-scene build: complete 24-run series

[`xvfb-llvmpipe-gpu-current.csv`](xvfb-llvmpipe-gpu-current.csv) records a fresh, complete series after component/style/typography integration and the shared-context/lazy-atlas changes. All 24 processes exited successfully; active runs delivered 596–599 workload ticks, and every zgui run retained a 1 MiB glyph atlas. The benchmark itself still uses the low-level retained-scene adapter, so this measures that renderer/workload path, not component-mounting or interaction-style overhead.

In this environment zgui used less sampled process CPU than either reference for active workloads. Its mean RSS medians were 122.65–125.55 MiB, below QuickGUI but above GPUI in every mode. Compared with the earlier eager-atlas zgui baseline, mean RSS medians fell by 13.51–14.94 MiB across modes. That is an observed cross-build result, not isolation of one optimization: typography and other source changes also occurred between builds. Active CPU medians increased slightly from that baseline (about 1–2%). This does not prove minimum possible CPU/memory use or predict hardware-GPU performance.

| Mode | Framework | CPU median [range], % one core | Mean RSS median [range], MiB | Completed ticks [range] |
| --- | --- | ---: | ---: | ---: |
| idle | zgui | 0.00 [0.00–0.00] | 122.65 [122.21–123.10] | 0–0 |
| idle | gpui | 0.14 [0.14–0.14] | 107.96 [107.64–108.28] | 0–0 |
| idle | quickgui | 0.07 [0.00–0.14] | 131.51 [130.67–132.34] | 0–0 |
| stream | zgui | 125.36 [125.25–125.47] | 125.55 [124.91–126.19] | 599–599 |
| stream | gpui | 314.86 [314.51–315.21] | 109.79 [109.75–109.82] | 598–599 |
| stream | quickgui | 238.75 [238.02–239.47] | 137.05 [136.72–137.39] | 599–599 |
| scroll | zgui | 138.75 [138.03–139.47] | 124.83 [124.47–125.20] | 599–599 |
| scroll | gpui | 288.89 [281.68–296.11] | 108.88 [108.55–109.21] | 596–599 |
| scroll | quickgui | 228.11 [227.31–228.92] | 139.12 [138.82–139.42] | 599–599 |
| both | zgui | 164.90 [164.29–165.51] | 125.55 [124.78–126.32] | 598–599 |
| both | gpui | 311.70 [310.60–312.80] | 109.71 [109.44–109.99] | 598–599 |
| both | quickgui | 240.64 [240.35–240.93] | 139.95 [139.92–139.98] | 599–599 |

The [renderer profile](xvfb-llvmpipe-gpu-current-renderer-profile.json), [summary](xvfb-llvmpipe-gpu-current-summary.json), and [source manifest](xvfb-llvmpipe-gpu-current-source.json) identify this build. A [source archive](xvfb-llvmpipe-gpu-current-source.tar.gz) preserves source, manifests, lockfiles, assets and harness immediately before the release build; every archived file was verified unchanged after building. Executable SHA256 hashes identify all three measured binaries, and the two reference binaries are unchanged from the baseline. The current [native frozen-state screenshot](../images/zgui-gpu-current-comparison.png) was inspected before timing.

This series excludes zero-RSS process-teardown samples; historical metrics below retain their original sampler behavior. An initial startup attempt collected zero samples because its X display had exited; the [startup log](xvfb-llvmpipe-gpu-current-startup-display-failure.log) is preserved. The successful series then owned its display and window manager for the entire run and started all 24 trials from the beginning.

## GPU baseline: eager glyph atlas

The first GPU series, [`xvfb-llvmpipe-gpu.csv`](xvfb-llvmpipe-gpu.csv), is the release build before the shared-context/lazy-atlas changes. The renderer allocated a 2048 × 2048 RGBA atlas (16 MiB) eagerly. Its detailed [resource profile](xvfb-llvmpipe-gpu-renderer-profile.json) identifies this configuration. The application does not request images, backdrop filters, or isolated layers.

All 24 runs exited successfully. Active workloads completed 597–599 ticks out of approximately 600 scheduled ticks; idle completed zero. The zgui GPU baseline consumed less sampled process CPU than either reference in the active workloads here, but had higher mean RSS than GPUI in every mode and generally higher RSS than QuickGUI. It does **not** establish a memory advantage or extrapolate to hardware GPU execution.

| Mode | Framework | CPU median [range], % one core | Mean RSS median [range], MiB | Completed ticks [range] |
| --- | --- | ---: | ---: | ---: |
| idle | zgui | 0.00 [0.00–0.00] | 137.06 [136.80–137.33] | 0–0 |
| idle | gpui | 0.14 [0.14–0.14] | 107.71 [107.27–108.14] | 0–0 |
| idle | quickgui | 0.00 [0.00–0.00] | 132.09 [132.00–132.18] | 0–0 |
| stream | zgui | 124.26 [124.16–124.36] | 139.06 [138.50–139.63] | 599–599 |
| stream | gpui | 314.06 [312.68–315.43] | 110.56 [109.98–111.14] | 599–599 |
| stream | quickgui | 240.01 [239.74–240.28] | 136.55 [136.31–136.80] | 599–599 |
| scroll | zgui | 136.42 [136.19–136.65] | 139.77 [139.41–140.13] | 598–599 |
| scroll | gpui | 294.19 [292.38–296.00] | 107.94 [107.40–108.48] | 599–599 |
| scroll | quickgui | 226.95 [226.88–227.03] | 139.00 [138.96–139.04] | 599–599 |
| both | zgui | 162.51 [162.43–162.58] | 139.23 [138.83–139.62] | 599–599 |
| both | gpui | 310.00 [309.42–310.59] | 110.23 [110.01–110.45] | 597–597 |
| both | quickgui | 239.36 [238.12–240.61] | 139.84 [139.46–140.22] | 599–599 |

## Lazy-atlas follow-up: incomplete series

The follow-up [`xvfb-llvmpipe-gpu-optimized.csv`](xvfb-llvmpipe-gpu-optimized.csv) contains **14 of 24 planned runs**. All first repeats completed, followed by the second GPUI and QuickGUI idle runs. The second zgui idle run and all nine second active runs are missing. This series has no aggregate comparison table because its repeat counts are unequal; it is superseded by the complete fresh current-build series above.

The [follow-up renderer profile](xvfb-llvmpipe-gpu-optimized-renderer-profile.json) records shared device/font resources and a lazy glyph atlas. Each recorded zgui run reports a 1 MiB retained atlas, versus the baseline's eager 16 MiB allocation. This is an allocation-counter observation, not a 15 MiB measured process-memory saving: RSS also includes fonts, driver allocations, and mapped/shared pages. Only one window was measured, so these runs do not measure the benefit of sharing resources between windows. Reference executable hashes are unchanged between series; the zgui executable differs. Neither frozen zgui build includes subsequent component/style/typography development.

## Artifact audit

The [artifact audit](artifact-audit.json) verifies the 24 current-build, 24 baseline and 14 recorded follow-up trials: CSV summaries agree with adjacent JSON, raw samples reproduce CPU and mean RSS, logs report matching workload ticks, and all recorded exit codes are zero. All frozen executable hashes matched their recorded metadata when audited. The current series contains 3,394 verified raw samples, no zero-RSS observations, and no missing trials. Regenerating the baseline summary reproduced the checked-in JSON byte for byte. The audit lists every missing follow-up trial; no missing run has been inferred from a log or filled with another series' results.

Three trials contain one final zero-RSS sample after normal resident samples: baseline zgui `both/0`, and follow-up zgui `scroll/0` and `both/0`. These are consistent with process exit during sampling. Raw records and original means are preserved; the zero lowers each affected trial's mean by approximately 0.7%. It does not change the baseline's lack of a memory advantage over GPUI. The current series excludes these observations explicitly.

## Artifacts and reproduction

- The CSV files preserve every trial; summary JSON preserves medians, minimums, maximums, and completed ticks.
- Each `.csv.<framework>.<mode>.<repeat>.json` contains raw timestamped CPU/RSS samples, its summary, and any application JSON reports. Adjacent `.log` files preserve stdout/stderr and reported workload counts.
- Metadata JSON records executable hashes, commands, compiler, machine, environment, and a supplementary worktree source snapshot. Binary copies were frozen before each series; executable hashes identify the measured builds even while source development continued. Worktree hashes alone are not a claim of an immutable source checkout.
- [Vulkan environment](xvfb-llvmpipe-gpu-vulkan.txt), [zgui screenshot](../images/zgui-gpu-comparison.png), [GPUI screenshot](../images/gpui-gpu-comparison.png), [QuickGUI screenshot](../images/quickgui-gpu-comparison.png).

```sh
env -u WAYLAND_DISPLAY -u ZGUI_SCREENSHOT -u ZGUI_EFFECTS \
  DISPLAY=:96 DBUS_SESSION_BUS_ADDRESS=unix:path=/nonexistent \
  ZGUI_RENDERER=gpu WGPU_BACKEND=vulkan \
  python3 scripts/compare.py \
  --zgui /path/to/frozen/zgui-demo \
  --gpui /path/to/frozen/zgui-compare-gpui \
  --quickgui /path/to/frozen/zgui-compare-quickgui \
  --seconds 10 --warmup 3 --repeats 2 --output comparison.csv
python3 scripts/summarize_comparison.py comparison.csv --output comparison-summary.json
```

The display/DBus settings above describe this development environment, not recommended production settings. Hardware validation should use a real desktop, longer warmup/trials, additional repeats, and independent presentation-cadence, latency, energy, and GPU-memory measurements. Workload ticks show delivered model updates, not independently verified presented frames. Process RSS includes resident mapped/shared pages, not just private allocations; these measurements exclude window-server CPU, child-process memory, and separate GPU memory/time. See [full methodology](../benchmarking.md).

The [earlier software-renderer record](software-baseline.md) remains available separately. Its single 3-second repeat and shorter warmup used zgui's CPU raster backend, so it must not be combined with these GPU series as if they measured the same implementation.
