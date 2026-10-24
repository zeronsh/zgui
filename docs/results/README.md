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
