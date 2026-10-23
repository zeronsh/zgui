# zgui

zgui is a retained-mode GUI framework in Rust for macOS, Windows and Linux. Components mount once and fine-grained signals update only the nodes that depend on them, so an idle window does no work and a busy one repaints only what changed. Interfaces are written in Rust or in Luau scripts that hot reload without losing state. It is young (0.1, used from git) and its API still changes.

## What it has

- **Layout** as CSS's flexbox and grid do it, defaults included: children stretch across and shrink to fit (`min-width: auto`), grids, wrapping, percentages, absolute positioning, `display: contents`, right-to-left with logical `start`/`end` edges, and layouts that answer the room they have.
- **Text:** shaped rich text with font fallbacks, selectable documents (drag, double and triple press, copy as HTML), editors with undo, clipboard, spellchecking, IME (Japanese, Chinese, Korean) and bidirectional text.
- **Paint:** a wgpu renderer (Metal, Direct3D 12, Vulkan; GL on Linux where no Vulkan driver is) that redraws only damaged regions, with rounded borders, gradients, shadows, backdrop blur, SVG, images, canvas drawing and custom WGSL fills.
- **Controls:** buttons, inputs, checkboxes, sliders, progress, virtual lists, scroll views with native scrollbars and rubber-banding, menus, popovers, modals, tooltips, web views (WebKit on macOS, WebView2 on Windows) and terminals (Ghostty, on macOS).
- **The platform:** native menus and dialogs, multiple windows, the clipboard and primary selection, drag and drop, touch and pen, the system's accent, appearance, text size, reduced motion, contrast and transparency, and the user's language direction.
- **Accessibility** through AccessKit (VoiceOver, Narrator, Orca): roles, headings, live regions, selection and keyboard-only use, with focus groups as ARIA's composite widgets.
- **Scripting:** Luau with types, a linter, hot reload that keeps state, anime.js-style animation, sandboxed plugins, and `zgui-run` to run a script as an app. Release builds carry scripts compiled into the binary.

## Quick start

In Rust, with the dependencies [getting started](docs/getting-started.md#in-rust) lists (zgui's crates from git, and the forks it builds with):

```rust,no_run
use zgui::compose::prelude::*;
use zgui_desktop::{Application, WindowOptions};

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

In Luau: put this in `hello/app.luau` and run `zgui-run hello/` (`LUAU_CXXFLAGS=-DNDEBUG cargo install --git https://github.com/zeronsh/zgui zgui-run`). Saving the file updates the window and keeps the count.

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
cargo run --release -p zgui-desktop --example gallery
cargo run --release -p zgui-run -- crates/zgui-script/examples/mail
cargo run -p zgui-script --bin zgui -- check crates/zgui-script/examples/mail
```

`zgui check` type checks scripts with [luau-lsp](https://github.com/JohnnyMorganz/luau-lsp/releases), which it needs on the `PATH`; `zgui lint` needs nothing. On Linux, install the fontconfig, xkbcommon and D-Bus development packages and `pkg-config`. On Windows, use the MSVC toolchain ([notes](docs/windows.md)).

## Performance

The [macOS dashboard comparison](docs/results/latest-macos-heavy-2026-10-11-5493829/) runs the same heavy scene in zgui and GPUI (with and without its fast path) on an Apple Silicon Mac, three runs each, medians:

| Mode | CPU (% of a core) zgui / GPUI / GPUI fast | GPU (Mcycles/s) zgui / GPUI / GPUI fast | Memory (MB) zgui / GPUI / GPUI fast |
| --- | --- | --- | --- |
| idle | 0.04 / 0.33 / 0.35 | 0 / 0 / 0 | 112 / 148 / 127 |
| scroll | 2.7 / 11.3 / 6.1 | 7.9 / 13.4 / 19.0 | 254 / 273 / 257 |
| stream | 8.0 / 19.8 / 11.4 | 12.1 / 12.9 / 14.7 | 256 / 274 / 258 |
| both | 8.5 / 20.2 / 11.7 | 12.9 / 14.6 / 17.3 | 259 / 275 / 269 |

These are one scene on one machine; Windows and Linux are not measured against GPUI yet. How the runs are made, and the other series, are in [benchmarking](docs/benchmarking.md).

## Docs

[Getting started](docs/getting-started.md), [components](docs/composition.md), [styling](docs/styling.md), [layout](docs/layout.md), [scripting](docs/scripting.md), [Luau reference](docs/reference/index.md), [Windows](docs/windows.md), [Linux](docs/linux.md), [changes](CHANGELOG.md), [roadmap](docs/ROADMAP.md). MIT licensed.
