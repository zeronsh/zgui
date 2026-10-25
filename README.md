# zgx

zgx is a retained-mode GUI framework in Rust for macOS, Windows and Linux. Components mount once and fine-grained signals update only the nodes that depend on them, so an idle window does no work and a busy one repaints only what changed. Interfaces are written in Rust or in Luau scripts that hot reload without losing state. It is young (0.1, used from git) and its API still changes.

## What it has

- **Layout** as CSS's flexbox and grid do it, defaults included: children stretch across and shrink to fit (`min-width: auto`), grids, wrapping, percentages, absolute positioning, `display: contents`, right-to-left with logical `start`/`end` edges, and layouts that answer the room they have.
- **Text:** shaped rich text with font fallbacks, selectable documents (drag, double and triple press, copy as HTML), editors with undo, clipboard, spellchecking, IME (Japanese, Chinese, Korean) and bidirectional text.
- **Paint:** a wgpu renderer (Metal, Direct3D 12, Vulkan; GL on Linux where no Vulkan driver is) that redraws only damaged regions, with rounded borders, gradients, shadows, backdrop blur, SVG, images, canvas drawing and custom WGSL fills.
- **Controls:** buttons, inputs, checkboxes, sliders, progress, virtual lists, scroll views with native scrollbars and rubber-banding, menus, popovers, modals, tooltips, web views (WebKit on macOS, WebView2 on Windows) and terminals (Ghostty, on macOS).
- **Components** (`zgx-components`): a data grid, a tree view, a calendar, a date picker and a time field, a combobox, a select, tabs, toasts, a command palette, a segmented control, a stepper, a disclosure and an accordion, built on the controls above, keyboard complete and accessible.
- **The platform:** native menus and dialogs, multiple windows, the clipboard and primary selection, drag and drop, touch and pen, the system's accent, appearance, text size, reduced motion, contrast and transparency, and the user's language direction.
- **Accessibility** through AccessKit (VoiceOver, Narrator, Orca): roles, headings, live regions, selection and keyboard-only use, with focus groups as ARIA's composite widgets.
- **Scripting:** Luau with types, a linter, hot reload that keeps state, anime.js-style animation, sandboxed plugins, and `zgx-run` to run a script as an app. Release builds carry scripts compiled into the binary.

## Quick start

In Rust, with the dependencies [getting started](docs/getting-started.md#in-rust) lists (zgx's crates from git, and the forks it builds with):

```rust,no_run
use zgx::compose::prelude::*;
use zgx_desktop::{Application, WindowOptions};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    Application::new()
        .window(WindowOptions { title: "Hello".into(), ..Default::default() })
        .run(|cx| {
            let count = cx.ui.signal(0);
            let shown = count.clone();
            cx.render(
                row()
                    .p(24.)
                    .gap(12.)
                    .child(button().child(text("Add")).on_click(move || {
                        count.set(count.get() + 1);
                    }))
                    .child(text_signal(move || format!("Clicked {} times", shown.get()))),
            );
        })
}
```

In Luau: put this in `hello/app.luau` and run `zgx-run hello/` (`LUAU_CXXFLAGS=-DNDEBUG cargo install --git https://github.com/zeronsh/zgx zgx-run`). Saving the file updates the window and keeps the count.

```lua
local count = ui.state("count", 0)
return ui.row {
	p = 24,
	gap = 12,
	ui.button { on_click = ui.add(count, 1), "Add" },
	ui.text { "Clicked {count} times" },
}
```

The examples run from a checkout:

```sh
cargo run --release -p zgx-desktop --example gallery
cargo run --release -p zgx-run -- crates/zgx-script/examples/mail
cargo run --release -p zgx-run -- crates/zgx-script/examples/code_editor path/to/file.rs
cargo run -p zgx-script --bin zgx -- check crates/zgx-script/examples/mail
```

`zgx check` type checks scripts with [luau-lsp](https://github.com/JohnnyMorganz/luau-lsp/releases), which it needs on the `PATH`; `zgx lint` needs nothing. On Linux, install the fontconfig, xkbcommon and D-Bus development packages and `pkg-config`. On Windows, use the MSVC toolchain ([notes](docs/windows.md)).

## Performance

The [macOS dashboard comparison](docs/results/latest-macos-heavy-2026-10-25-282bfc0/) runs the same heavy scene in zgx and GPUI (with and without its fast path) on an Apple Silicon Mac, three runs each of builds made with each framework's `dist` profile, medians:

| Mode | CPU (% of a core) zgx / GPUI / GPUI fast | GPU (Mcycles/s) zgx / GPUI / GPUI fast | Memory (MB) zgx / GPUI / GPUI fast |
| --- | --- | --- | --- |
| idle | 0.00 / 0.38 / 0.40 | 0 / 0 / 0 | 59 / 88 / 93 |
| scroll | 3.2 / 14.6 / 8.7 | 11.5 / 21.6 / 21.9 | 255 / 272 / 256 |
| stream | 8.5 / 22.0 / 13.9 | 16.8 / 22.1 / 21.7 | 259 / 272 / 275 |
| both | 9.1 / 23.0 / 15.9 | 16.7 / 22.0 / 21.6 | 259 / 272 / 274 |

Since the previous run (`7d559ad`), zgx's GPU work fell 14% in `both` and 5% in `stream` and `scroll` (damage drawn through a stencil of itself), and its CPU and memory held. Idle is sampled 20–35 s after launch, once the Metal driver has given its memory back, the median of five launches ([why](docs/benchmarking.md#why-idle-is-sampled-2035-s-after-launch)). An idle window settles at one of two levels about 25 MB apart, launch to launch (zgx's five launches 53–79 MB, GPUI fast's 89–95, GPUI's 88–105): which one is the driver's timing, not the app's. zgx also paces every mode's frames more evenly (99th-percentile interval 17.6 ms against 24.4–25.4 ms, with no frame late). GPUI fast's memory in `scroll` varies by about 15 MB between sessions (256 MB here, 272 in the previous run), so read its memory cells with that spread. The window server's own CPU is too noisy between separate trials to rank here. Measured in paired seconds, with the two windows taking turns, zgx's costs the same as GPUI fast's in `stream` and `scroll`, and at most about 2 points more in `both` ([how, and what it depends on](docs/performance-feature-audit.md#the-window-servers-cost-of-heavy-content-macos)).

With every framework built with profile-guided optimization, each trained on its own runs (a supplement of 2026-10-24, zgx at `c9975e1`: no pacing captures, idle not run; [`latest-macos-heavy-pgo-2026-10-24-c9975e1`](docs/results/latest-macos-heavy-pgo-2026-10-24-c9975e1/)), zgx still leads CPU and GPU in every mode:

| Mode, PGO | CPU (% of a core) zgx / GPUI / GPUI fast | GPU (Mcycles/s) zgx / GPUI / GPUI fast | Memory (MB) zgx / GPUI / GPUI fast |
| --- | --- | --- | --- |
| scroll | 3.0 / 14.6 / 8.1 | 12.3 / 21.6 / 21.8 | 255 / 273 / 255 |
| stream | 7.7 / 20.6 / 13.9 | 17.6 / 22.2 / 21.7 | 259 / 273 / 275 |
| both | 8.6 / 21.5 / 14.4 | 19.2 / 22.3 / 21.6 | 260 / 274 / 276 |

**Correction:** the comparison published here on 2026-10-23/24 (zgx at `51a669c`) built zgx with mimalloc, unintentionally, while GPUI used the system allocator. The table above is the re-run with the system allocator for zgx too; [old and new numbers side by side](docs/results/README.md#correction-2026-10-24-macos-runs-of-2026-1023-24-built-zgui-with-mimalloc).

These are one scene on one machine; Windows and Linux are not measured against GPUI yet. How the runs are made, and the other series, are in [benchmarking](docs/benchmarking.md).

## Docs

[Getting started](docs/getting-started.md), [components](docs/composition.md), [styling](docs/styling.md), [layout](docs/layout.md), [scripting](docs/scripting.md), [Luau reference](docs/reference/index.md), [Windows](docs/windows.md), [Linux](docs/linux.md), [changes](CHANGELOG.md), [roadmap](docs/ROADMAP.md). MIT licensed.
