zgx · Frame profiler, native zgx
=================================

Run from the zgx checkout:

```sh
cargo run --release -p zgx-script --example frame_profiler
```

Imported from the pen file recorded in `source.json`: the Timeline window
(`MvwFD`), its popovers (`kZfcb`) and its states and menus (`XFJTT`), as
revised with the title bar's new order, the rounded body, the reduced status
bar, the six recording states and the dropdowns. The
other tabs' frames are not imported; their tabs show a placeholder. The
"Export menu open" frame only holds the pressed More button, so the Export
submenu follows the More menu's style. Icons are Lucide (Feather's
successor, which the design names); refresh them with:

```sh
node scripts/import_profiler_icons.mjs /path/to/node_modules/lucide
```

Everything is interactive with sample data, nothing is measured:

- Tabs, range (1 s / 5 s / 30 s) and scope switches slide their pills. A
  range change regrows the chart.
- Hovering a bar shows its frame card; a click opens that frame in the trace,
  restretching its spans. Prev/next buttons and ←/→ step frames.
- Spans: hover for details, click to select, right-click for the span menu.
  Zoom (Fit frame / vsync / selection, in, out) springs the spans and redraws
  the ruler.
- Budget menu moves the red line and rescales the chart past 16.7 ms. Over-
  budget badges and the summary count follow it, or the frame cap if
  "Follow frame cap" is on.
- The record control has six states: recording, paused, stopped, armed
  with a 3 s countdown, armed until the first input, and a timed capture
  with an underline. Stopped, its chevron opens the capture options (length,
  and how it starts). It is one button throughout: its colours ease, the
  label's slot springs to the new text's width so the button resizes, and
  the glyph and action icon change in place. Right-click it to preview any
  of the six states.
- Menus: Target, More with Export and Runtime settings, Search (⌘F, filters
  and jumps), Overlays (⌥1–6, ⌥0), capture options, Budget, Zoom, the span
  context menu, and the status bar's Logs and Capture buffer, which open
  upward. Selects open dropdowns of options with descriptions. Arrow keys,
  Enter and Escape drive them.
- The status bar's speed control (¼× to 2×) slides its pill; the Logs chip
  shows unread lines until its popover opens.
- Tooltips appear after 500 ms, and at once while one has just been showing.
  They slide straight out of their trigger; panels scale in from their
  anchor's corner.

Cost: one canvas draws the 120 bars; hover and selection columns, tooltips
and the frame card move on the compositor. The recording clock and its dot
run at 20 fps, so the engine asks for 20 frames a second (about 0.5% CPU on
an M-series Mac). Paused or stopped, the window requests no frames.

Engine additions this example needed:

- `pointer_events = "none"`: the full-window menu layer and the hover card
  let the pointer through to what is under them.
- Box shadows: the quad shader drew a shadow at full strength up to the box
  and fell off as a Gaussian of the distance, a hard dark band. It now
  covers 0.5·erfc(d / σ√2), half at the edge, as a blurred box does; the
  software rasterizer shares the formula.
- `el:focus()` / `el:blur()`: the search field takes focus as it opens.
- Property order: Lua tables have no key order, so `border` could reset
  `border_sides`, and `p` could override `px`, at random. Longhands now
  apply after their shorthand.

Tests: `cargo test --release -p zgx-script --test examples frame_profiler::` opens
every menu, the span context menu and its zoom, a tooltip's delay, and checks
that paused and stopped windows rest. `FRAME_PROFILER_SNAPSHOT=/tmp/fp`
writes software captures (the headless text measurer is approximate).
