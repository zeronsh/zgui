# Comparison adapters

The same scenes in each framework, for `scripts/run_macos_comparison.py`:
zgx (`crates/zgx-desktop/examples`, `crates/zgx-script/examples`), GPUI Fast
(`gpui-fast`), MyGo (`mygo`) and Rune (`rune`: pinned and patched, see its
README and PATCH.md). `workload` is the scenes' shared content and
motion: every adapter takes its text, numbers, geometry and colours from it
(MyGo's Go port checks itself against `workload`'s golden dump), so the
frameworks draw the same thing. Older adapters (`gpui`, `quickgui`) stay for
the recorded runs that used them.

## The contract every adapter keeps

Environment, read at start (`ZGUI_*` names still work for one release):

| Variable | Meaning |
| --- | --- |
| `ZGX_MODE` | lab and heavy: `idle`, `stream`, `scroll`, `both` (default); vlist: `scroll` (default), `fling`, `jump`; animate: ignored |
| `ZGX_SECONDS` | run this long (default 30), then report and exit 0 |
| `ZGX_INITIAL_TICKS` | lab: ticks of `both` before starting (default 0) |

Ticks: the workload advances once per `workload::PERIOD` (60 a second) by
the wall clock, each tick doing what the mode says (`Workload::tick`,
`Heavy::tick`, `vlist::delta` and `vlist::jump`); animate instead draws on
every display refresh, its clock at the frame's time. An adapter draws only
what changed, however its framework does that, and never more often than
its framework would.

Report, once, as the run ends, on stdout or stderr (the runner logs both and checks it):

| Scene | Line |
| --- | --- |
| lab, heavy, vlist | `workload_ticks=N` (or lab's `{"framework":…,"ticks":N}`) |
| animate | `animation_frames=N fps=F` |
| blink | `blink_frames=N` |

`N` is the ticks run (0 in idle); a run that reports none, or exits
otherwise, fails its trial.

Binaries: zgx's are examples (`heavy_workload`, `component_workload`,
`animation_workload`, `vlist_workload`, `blink_workload`; in Luau, zgx-script's `vlist_luau`); GPUI Fast's are `heavy`,
`zgx-compare-gpui-fast` (lab), `animate`, `vlist`, `blink`; MyGo's are
`comparisons/mygo/bin/{heavy,lab,animate,vlist,blink}`.

blink is a caret blinking in a still window (`zgx_workload::blink`): a 2 ×
20 point bar shown and hidden every 530 ms, nothing else changing, for an
idle focused field's memory and wakes.

Blank rows (vlist) are counted from outside, by a probe capturing the
window: no adapter needs a hook for it.

Windows: one, titled for its scene, at the scene's size (lab and animate
960×720, heavy `heavy::WIDTH`×`heavy::HEIGHT`, vlist `vlist::WIDTH`×
`vlist::HEIGHT`), not resized, in the font the scene names.

Builds: each framework's release profile at its strongest (Rust: the `dist`
profile, fat LTO, one codegen unit; Go: `go build -trimpath -ldflags="-s
-w"`, no race detector), the system allocator. The runner records each
binary's hash, the toolchain's version and the build flags.

## vlist: how each widget measures heights

- zgx (Rust): `measured_virtual_list`: a row's height is measured as it is
  laid out; rows not yet seen are a three-line row's estimate.
- zgx (Luau): `ui.list { measured = true }`, the same list from a script
  (`crates/zgx-script/examples/vlist_luau`), each row's text read from the
  host with `ui.call_sync`; a jump is the list element's `scroll_to_row`,
  which measures the rows around the target as it lands, within the jump's
  frame (the next tick's scroll would cancel a landing still in progress).
- GPUI Fast: `list` with a `ListState` (not `uniform_list`), which lays rows
  out to measure them as they come into view.
- Rune: `El::virtual_rows`, measuring rows as they mount and estimating the
  rest; scrolled through its own wheel path (`Tx::wheel`, the patch), and a
  jump reveals the row (nearest) rather than putting it at the top.
- MyGo: `ui.List` with a `ListState`: keeps its place by an anchor row,
  measures rows as it lays them out, and estimates unmeasured ones at the
  average height.

## Microbenchmarks

`zgx_workload::micro` defines them (docs/benchmarking.md, "Microbenchmarks"):
one state change, and one scroll step of 10,000 rows (fixed and variable
heights), each timed as the write alone and through to a frame ready to
present, in process CPU time per operation. zgx's is
`crates/zgx-desktop/examples/micro_bench.rs`; GPUI Fast's is
`gpui-fast-micro`, a crate of its own because it needs GPUI's headless test
platform (`test-support`, which must not reach the windowed adapters): an
entity update draws the window when it ends, with the real macOS text system
and no renderer, so its frame is the scene ready to present without the GPU's
encode, as zgx's `RENDER=0`.
