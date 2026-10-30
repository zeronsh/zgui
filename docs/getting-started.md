# Getting started

From nothing to a window, then a list, selectable text, a settings window and a shipped app: first in Rust, then in Luau with `zgx-run`. Every snippet here is compiled (Rust) or run headless (Luau) by the repository's tests, so they work as written.

zgx runs on macOS, Windows and Linux. On Linux, install the fontconfig, xkbcommon and D-Bus development packages and `pkg-config`; on Windows, use the MSVC toolchain ([notes](windows.md)).

## In Rust

Add the crates, from crates.io:

```toml
[dependencies]
zgx = "0.1.6"
zgx-desktop = "0.1.6"
# The component library (a data grid, a tree, tabs, toasts…), if you use it.
zgx-components = "0.1.6"
```

Or, for what is not released yet ([the changelog](../CHANGELOG.md) says what), from its repository:

```toml
[dependencies]
zgx = { git = "https://github.com/zeronsh/zgx" }
zgx-desktop = { git = "https://github.com/zeronsh/zgx" }
zgx-components = { git = "https://github.com/zeronsh/zgx" }
```

That builds as it is: zgx's own forks (of cosmic-text, taffy and mlua) come with it as crates of their own. A few more of its dependencies are patched in zgx's repository alone (winit, wgpu-hal, the AccessKit adapters), and an app gets their releases instead, which zgx builds with, short of those fixes: Wayland popups open in their window, text dragged in from other apps drops into fields on Wayland alone, macOS's Services don't act on zgx's text, Metal frames take more command buffers, VoiceOver misses headings and grid rows, a field's error message is said only in its description ([releasing](releasing.md) lists them). To have them too, add the patches your app's build then takes from zgx's repository (a dependency's `[patch]` never reaches an app):

```toml
[patch.crates-io]
wgpu-hal = { git = "https://github.com/zeronsh/zgx" }
winit = { git = "https://github.com/zeronsh/zgx" }
accesskit_macos = { git = "https://github.com/zeronsh/zgx" }
accesskit_windows = { git = "https://github.com/zeronsh/zgx" }
accesskit_unix = { git = "https://github.com/zeronsh/zgx" }
accesskit_atspi_common = { git = "https://github.com/zeronsh/zgx" }
accesskit_consumer = { git = "https://github.com/zeronsh/zgx" }
```

The first build compiles the stack once (about a minute on an Apple M-series Mac, release); later ones take seconds.

### A window

`Application` opens windows; `render` mounts a view in one. Views are built once: `column`, `row` and `div` lay out their children as CSS's flexbox does (children stretch across and shrink to fit), and styles are methods.

```rust,no_run
use zgx::compose::prelude::*;
use zgx_desktop::{Application, WindowOptions};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    Application::new()
        .window(WindowOptions {
            title: "Hello".into(),
            width: 480.,
            height: 320.,
            ..Default::default()
        })
        .run(|cx| {
            cx.render(column().p(24.).gap(12.).child(text("Hello, zgx")));
        })
}
```

### State

A signal holds a value; what reads it updates when it changes, and nothing else does. `text_signal` follows the signals it reads; a button's `on_click` writes one.

```rust,no_run
use zgx::compose::prelude::*;
use zgx_desktop::{Application, WindowOptions};

fn counter() -> View {
    component(|cx| {
        let count = cx.state(0);
        let shown = count.clone();
        row()
            .p(24.)
            .gap(12.)
            .items_center()
            .child(button().child(text("Add")).on_click(move || {
                count.set(count.get() + 1);
            }))
            .child(text_signal(move || format!("Clicked {} times", shown.get())))
    })
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    Application::new()
        .window(WindowOptions::default())
        .run(|cx| {
            cx.render(counter());
        })
}
```

### A list

`virtual_list` mounts only the rows in view, so ten thousand cost what twenty do. `keyboard_navigation(true)` makes it one Tab stop that the arrows, Page Up and Down, Home and End move through.

```rust,no_run
use zgx::compose::prelude::*;
use zgx_desktop::{Application, WindowOptions};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    Application::new()
        .window(WindowOptions::default())
        .run(|cx| {
            let offset = cx.ui.signal(0.);
            cx.render(
                virtual_list(
                    offset,
                    32.,
                    4,
                    || 10_000,
                    |index| index,
                    |_, index, _| row().px(12.).items_center().child(text(format!("Row {index}"))),
                )
                .w_full()
                .h_full()
                .keyboard_navigation(true),
            );
        })
}
```

### Selectable text

Text is not selectable unless asked, as in an app; `selectable(true)` makes a subtree a document you drag across, double-press for words and copy from, as on a web page.

```rust,no_run
use zgx::compose::prelude::*;
use zgx_desktop::{Application, WindowOptions};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    Application::new()
        .window(WindowOptions::default())
        .run(|cx| {
            cx.render(
                column()
                    .selectable(true)
                    .p(24.)
                    .gap(8.)
                    .child(text("Release notes").text_size(22.).heading(1))
                    .child(text("Drag across these lines to select them, then copy.").text_wrap(true)),
            );
        })
}
```

### A settings window

`cx.windows.open` opens another window. An application's windows share one reactive runtime: a signal changed in the settings window updates the main one.

```rust,no_run
use zgx::compose::prelude::*;
use zgx_desktop::{Application, WindowOptions};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    Application::new()
        .window(WindowOptions::default())
        .run(|cx| {
            let dark = cx.ui.signal(true);
            let windows = cx.windows.clone();
            let shared = dark.clone();
            let shown = dark.clone();
            cx.render(
                column()
                    .p(24.)
                    .gap(12.)
                    .child(text_signal(move || {
                        if shown.get() { "Dark".into() } else { "Light".into() }
                    }))
                    .child(button().child(text("Settings…")).on_click(move || {
                        let dark = shared.clone();
                        windows.open(
                            WindowOptions {
                                title: "Settings".into(),
                                width: 360.,
                                height: 200.,
                                ..Default::default()
                            },
                            move |cx| {
                                cx.render(column().p(24.).child(checkbox("Dark appearance", dark)));
                            },
                        );
                    })),
            );
        })
}
```

### Shipping

`cargo build --profile dist` builds the app to ship. Add the profile to your workspace's `Cargo.toml`:

```toml
# release, optimized as one program: about 14% smaller and a little faster,
# for a slower link. Keep `release` for day-to-day optimized builds.
[profile.dist]
inherits = "release"
lto = "fat"
codegen-units = 1
```

Profile-guided optimization is a further, optional step: build the app instrumented, run what it does, and build it again with what the run recorded. On zgx's headless scenes it saves 2–19% of the instructions (the heavy dashboard 12%, Luau animation 19%), in scenes it was not trained on as much as in those it was. The profile fits the source it was recorded from, so record again after changes. Inside this repository, `python3 scripts/perf/pgo.py -p zgx-run --bin zgx-run` does all three builds, training on zgx's headless scenes and on any command you give it (`'{bin} …'`), and writes `target/dist-pgo/zgx-run`. For an app of your own (`rustup component add llvm-tools` first):

```sh
RUSTFLAGS="-Cprofile-generate=/tmp/pgo" cargo build --profile dist
./target/dist/my-app            # use it as people do, then quit
"$(rustc --print sysroot)"/lib/rustlib/*/bin/llvm-profdata merge -o /tmp/app.profdata /tmp/pgo
RUSTFLAGS="-Cprofile-use=/tmp/app.profdata" cargo build --profile dist
```

zgx-desktop's `mimalloc` feature makes mimalloc the program's allocator: 1–10% fewer cycles in zgx's headless scenes (text 7%, layout 6%, animation 9%, the heavy dashboard 2%) for about 2–3 MB more memory. It also smooths the frames after a large removal: a large subtree leaves the scene at once and zgx frees its nodes a millisecond's worth per frame, but the system allocator on macOS can take 6–9 ms over the first chunk it frees (it hands pages back), where with mimalloc it stays within the millisecond. Turn it on with `features = ["mimalloc"]` on the dependency, unless your program declares a `#[global_allocator]` of its own. Its memory shows in vmmap, footprint and Activity Monitor as "Memory Tag 240" (the first tag macOS leaves to an application), where mimalloc's own default, tag 100, reads as "IOAccelerator", the program's heap mistaken for GPU memory. `MIMALLOC_OS_TAG` sets another.

Release binaries keep their symbol table, so backtraces and crash reports name functions. Stripping it saves about another 1.4 MB, and is your choice to make: add `strip = "symbols"` to `[profile.dist]` and keep the symbols apart, in a dSYM, to read crash reports with. On macOS that is `split-debuginfo = "packed"` with `debug = "line-tables-only"` in the profile; Cargo writes `<binary>.dSYM` beside the binary. Keep it with each release you ship, so crash reports from it can be symbolicated (`atos`, or Xcode's Organizer).

To ship it on Windows (an icon, a manifest, one instance per user, an installer), see [shipping on Windows](windows.md#shipping-a-zgx-app-on-windows); on macOS, bundle the binary in an `.app` with an `Info.plist` (the terminal example's `scripts/zterm-app.sh` does).

## In Luau

The same interface as a script, hot reloaded as you save it, without a Rust host of your own. Install the runner and the checker:

```sh
LUAU_CXXFLAGS=-DNDEBUG cargo install zgx-run
LUAU_CXXFLAGS=-DNDEBUG cargo install zgx-script --bin zgx
```

(For what is not released yet, `--git https://github.com/zeronsh/zgx` before the crate's name installs from the repository.)

(`zgx-run` uses mimalloc as its allocator, for 6–10% less CPU on script, text and layout work at about 2 MB more memory; add `--no-default-features` to keep the system's. `LUAU_CXXFLAGS=-DNDEBUG` builds Luau without its asserts, a fifth faster: the repository's own builds set it in `.cargo/config.toml`, which an install doesn't read. Without it, `zgx-run` says so as it starts.)

The quickest start is an app made for you:

```sh
zgx new my-app                     # or --template router, list or form; --i18n for ui.t messages
cd my-app
zgx run .                          # a window, reloading as you save
zgx test                           # its tests, headless
```

It writes `app.luau`, a component, a theme of the app's own tokens (`theme.luau`), a test (`tests/app.test.luau`), `zgx.json`, a README, and a GitHub Actions workflow that runs `zgx check`, `zgx lint`, `zgx audit` and `zgx test`. All four are clean from the start. The sections below build the same pieces by hand.

### A window

An app is a directory with an `app.luau` returning its interface, and optionally a `zgx.json` for its window:

```json
{ "title": "Hello", "width": 480, "height": 320, "app_id": "com.example.hello" }
```

```lua
return ui.column { p = 24, gap = 12, ui.text { "Hello, zgx" } }
```

`zgx-run hello/` opens it; saving `app.luau` updates the window and keeps its state. `zgx check hello/app.luau` type checks it (with luau-lsp installed), and `zgx lint` finds what is slow or wrong though it runs.

### State

`ui.state` holds a value by key: a reload keeps it. Text templates (`"{count}"`) follow it; actions such as `ui.add` change it without calling the script.

```lua
local count = ui.state("count", 0)
return ui.row {
	p = 24,
	gap = 12,
	items = "center",
	ui.button { on_click = ui.add(count, 1), "Add" },
	ui.text { "Clicked {count} times" },
}
```

### A list

`ui.list` mounts only the rows in view; `keyboard = true` makes it one Tab stop with arrow keys.

```lua
return ui.list {
	count = 10000,
	height = 32,
	keyboard = true,
	w_full = true,
	h_full = true,
	row = function(i: number): Node
		return ui.row { px = 12, items = "center", ui.text { `Row {i}` } }
	end,
}
```

### Selectable text

```lua
return ui.column {
	selectable = true,
	p = 24,
	gap = 8,
	ui.text { "Release notes", text_size = 22, heading = 1 },
	ui.text { "Drag across these lines to select them, then copy.", wrap = true },
}
```

### A settings window

`ui.window` declares another window, open while its state is true; `ui.command` puts an item in the menu bar, with its shortcut (`cmd` is Command on macOS and Control elsewhere).

```lua
local dark = ui.persist("dark", true)
local settings = ui.state("settings", false)
ui.command {
	id = "settings",
	title = "Settings…",
	menu = "File",
	key = "cmd+,",
	run = function()
		settings:set(true)
	end,
}
ui.window {
	id = "settings",
	title = "Settings",
	width = 360,
	height = 200,
	open = settings,
	ui.column { p = 24, ui.checkbox { label = "Dark appearance", value = dark } },
}
return ui.text {
	function()
		return if dark:get() then "Dark" else "Light"
	end,
	p = 24,
}
```

### Shipping

A shipped app carries its scripts compiled into the binary: a build script bundles the directory, and a small Rust host runs it from the bundle ([shipping an app](scripting.md#shipping-an-app)). Build it with the `dist` profile, as a Rust app (above), and set `LUAU_CXXFLAGS=-DNDEBUG` for it ([cost](scripting.md#cost)). The mail example (`crates/zgx-script/examples/mail`) is a whole app written this way.

## Next

- [Components](composition.md), [styling](styling.md) and [layout](layout.md) in Rust.
- [Scripting](scripting.md) and the [Luau reference](reference/index.md).
- [Windows](windows.md) and [Linux](linux.md) notes; [benchmarking](benchmarking.md).
