# Working on zgx

zgx is a retained-mode GUI framework: Rust core (`crates/zgx`), the component library built on it (`crates/zgx-components`: data grid, tree, dates, combobox, tabs, toasts…), wgpu renderer (`crates/zgx-gpu`), winit desktop host (`crates/zgx-desktop`), Luau scripting (`crates/zgx-script`, the components' `ui.*` behind its default `components` feature). Read `README.md` once, then the doc for what you touch.

## Build, run, test

```sh
export CARGO_PROFILE_DEV_DEBUG=0              # debug info fills disks: examples link the renderer
cargo run -p zgx-desktop --example gallery    # a Rust app
cargo run -p zgx-script --example hot_reload  # a Luau app, reloading as you save
cargo test -p zgx -p zgx-components -p zgx-desktop -p zgx-script --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo fmt --all -- --check
```

Integration tests of `zgx` and `zgx-script` build as one binary per area (`tests/<area>/main.rs`; linking 140 binaries made a one-line change in the core cost more than its build). **A new test file goes beside the others (`tests/name.rs`) and must be listed in its area's `main.rs` as `#[path = "../name.rs"] mod name;`: a file no area lists never builds and its tests never run** (`every_test_file_is_listed` fails until it is listed). Run an area, a file or a test by filter: `cargo test -p zgx --test layout`, `... --test layout layout_box_model::`. A test that needs a process of its own (a global allocator, environment variables) gets its own `[[test]]` in the crate's `Cargo.toml`.

Linux native probes (real X11/Wayland windows, input, IME, clipboard, accessibility): `cargo build -p zgx-desktop --examples --locked`, then `python3 scripts/linux_smokes.py --only <name>`. CI runs them all; `docs/validation.md` says what each checks.

## Before you commit

- `cargo fmt`, clippy with `-D warnings`, and the tests of every crate you touched.
- Luau scripts: `cargo run -p zgx-script --bin zgx -- lint <dir>` and `... -- check <dir>` (luau-lsp), then `stylua` (2.6, `crates/zgx-script`).
- A commit title says what is now true ("Wheel scrolling eases in whole pixels"), the body why.

## Principles

- **Fix zgx, not around it.** A bug or a slow path found in an app or example is fixed in the engine, with a test there.
- **Measure performance claims** (retired instructions, or repeated timings), never one noisy run: `python3 scripts/perf/suite.py ab origin/main .` compares every headless scene in about two minutes, and `ZGX_PROFILE=1` splits a frame by phase (`scripts/perf/README.md`). Idle windows do no work: keep it so.
- Match the code around you: its naming, comment density and idiom.
- Every behaviour change gets a test; native behaviour gets a probe in `scripts/` registered in `scripts/linux_smokes.py`.

## zgx-script: fast by default

The interface is built once; Rust re-runs only what reads state that changed. `zgx lint` flags the rest, and a debug build lints on every reload (the banner shows findings).

| Do | Not |
| --- | --- |
| `ui.text { "Count: {count}" }` or `function() return n:get() end` | `ui.text { "Count: " .. count:get() }` (read once, never updates) |
| `ui.use_state(false)` inside a component or row | `ui.state("open", false)` there (one state shared by every instance) |
| `ui.tween(state)`, `ui.spring(state)`, `anime.createAnimatable` | `anime.animate` re-fired on every data change |
| `anime.animate(el, { x = … })` or a state as its target | `onUpdate = function() state:set(…) end` (script every frame) |
| `items:push(v)`, `:set_key({ 3, "title" }, v)`, `:patch` | `items:update(function(l) table.insert(l, v) end)` (copies all) |
| `ui.list { count, row = … }` for thousands of rows | `ui.each` over thousands (builds every row) |
| `ui.list { key = function(i) … end, row = function(id, index) … }` where items are removed or inserted | rows reading `list[i]` (a removal writes every row after it) |
| `ui.list { measured = true, … }` for rows of wrapped text | a fixed `height` guessed for wrapped text (cut off, or gaps) |
| `ui.sorted(rows, { by = "price" })` for a list kept in order | a `ui.computed` re-sorting the whole list on each write |
| percentages, `grow`, insets: layout without the script | `on_resize` writing sizes into state for layout |
| `ui.every(1000, …)` at the rate a value changes | `ui.every(1, …)` (more calls than frames) |
| keys of the item alone: `key = function(r) return r.id end` | a `key` reading other state (goes stale) |

`-- zgx-lint: allow(rule)` accepts a deliberate exception.

## Where things are

- `docs/scripting.md` (scripts, state, animation, lint), `docs/reference/` (every Luau element and API), `crates/zgx-script/types/zgx.d.luau` (types luau-lsp checks).
- `docs/composition.md`, `docs/styling.md`, `docs/layout.md` (Rust views).
- `docs/validation.md` (what is verified, on which platform), `docs/ROADMAP.md`, `docs/windows.md`, `docs/linux.md`.
- `.github/workflows/ci.yml`: Linux, macOS and Windows jobs on every PR.
