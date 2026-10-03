# zgui

A retained Rust GUI framework for **Linux, macOS and Windows**, focused on fine-grained updates, bounded rendering resources, and low idle CPU usage. It has a platform-independent core, a wgpu renderer, and a desktop host with native input, clipboard, IME, accessibility, and multiple windows.

```sh
cargo run --release -p zgui-desktop --example components
cargo run --release -p zgui-desktop --example form
cargo run --release -p zgui-desktop --example wrapped_editor
cargo run --release -p zgui-desktop --example line_height
cargo run --release -p zgui-desktop --example letter_spacing
cargo run --release -p zgui-desktop --example percent_sizes
cargo run --release -p zgui-desktop --example overlays
cargo run --release -p zgui-desktop --example menus
cargo run --release -p zgui-desktop --example submenus
ZGUI_MODE=both ZGUI_SECONDS=10 cargo run --release -p zgui-desktop --example component_workload
cargo run --release -p zgui-desktop --example gallery
cargo run --release -p zgui-desktop --example workbench
cargo run --release -p zgui-desktop --example windows
cargo run --release -p zgui-desktop --example effects
cargo run --release -p zgui-desktop --example layout
cargo run --release -p zgui-desktop --example rich_text
cargo run --release -p zgui-desktop --example paint_styles
cargo run --release -p zgui-desktop --example canvas
cargo run --release -p zgui-desktop --example svg_transform
cargo run --release -p zgui-desktop --example animated_image
cargo run --release -p zgui-desktop --example cursors
cargo run --release -p zgui-desktop --example drag_drop
cargo run --release -p zgui-desktop --example dynamic_menus
cargo run --release -p zgui-desktop --example platform_services
```

Linux builds need the native development libraries for fontconfig, xkbcommon and
D-Bus, plus `pkg-config` (for example `libfontconfig1-dev libxkbcommon-dev
libdbus-1-dev pkg-config` on Debian/Ubuntu). File dialogs use the desktop's XDG
portal; prompts use `zenity`. macOS builds use the system frameworks and Xcode
command-line tools. Windows builds use the stable MSVC Rust toolchain and Visual
Studio Build Tools with the C++ workload and Windows SDK; no GTK installation is
needed. See [Windows support](docs/windows.md) for validation and limitations,
and [native services](docs/platform-services.md) for backend
behavior and cancellation guarantees.

Applications describe components and children. They mount once; reactive properties update only their retained targets:

```rust,no_run
use zgui::compose::prelude::*;
use zgui_desktop::Application;

fn counter() -> View {
    component(|cx| {
        let count = cx.state(0_u32);
        let label = count.clone();
        column().p(24.0).gap(12.0)
            .bg(rgb(0x172030)).rounded(12.0)
            .child(text_signal(move || format!("Count: {}", label.get())))
            .child(button()
                .px(16.0).py(10.0).rounded(8.0)
                .bg(rgb(0x345888))
                .hover(|s| s.bg(rgb(0x426da4)))
                .focus(|s| s.border(2.0).border_color(rgb(0x9ac7ff)))
                .child("Increment")
                .on_click(move || { count.update(|n| *n += 1); }))
    })
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    Application::new().run(|cx| { cx.render(counter()); })
}
```

Components resolve typed services with `cx.service::<Model>()`, initialize local state with `cx.state`, and capture caller-owned lexical providers in `cx.slot`. Rows, columns, buttons and slots take child views; `text_input` and `text_area` bind editable values with fluent styles and inherited typography; application components do not manipulate scene parent IDs. `provide`/`provide_with`, keyed children and conditional regions preserve ownership across fine-grained updates. See the [exact model example](crates/zgui/examples/model.rs), [component architecture](docs/composition.md), and [fluent styling](docs/styling.md).

The framework includes:

- Tracked signals, equal-write suppression, batching, scoped providers, stable reactive collection rows, keyed views, explicit ownership, cancellation, bounded background workers and wake-driven async tasks.
- Cached row/column/overlay layout, grid placement/spans, wrapping/reversed flex, basis and alignment, percentage constraints/spacing, auto margins, absolute insets, aspect ratios and independent overflow policies.
- Shaped rich text with inline links/decorations, font features/fallbacks, custom fonts, alignment, ellipsis and line clamping; bounded glyph/image caches and persistent damage rendering.
- Retained vector paths, gradient/pattern fills, independent border edges/corners, multiple shadows, device-scale SVG/tint/transforms, async image caching and visibility-aware GIF playback. Transparency, edge fades, backdrop blur and explicit cached compositor layers compose with these views.
- Typed internal drag/drop with retained previews, grouped external file drops, inherited/reactive cursors, pointer capture, bubbling/capture event routing, focus navigation and trapping, keyboard activation, Unicode selection/editing, IME preedit, clipboard, and bounded undo/redo.
- Buttons, checkboxes, sliders, progress indicators, labels, text inputs, scroll views, keyed fixed/variable-height virtual lists, dialogs, popovers and menus.
- Winit window lifecycle, GPU presentation, display scaling, multiple windows, typed contextual actions/key sequences, native file dialogs/prompts, application menus, URL routing and AccessKit semantics/actions.

Read [components and slots](docs/composition.md), [styling](docs/styling.md), [application development](docs/application.md), [architecture and template lowering](docs/architecture.md), and the [GPUI capability acceptance matrix](docs/gpui-parity.md). The proposed template language remains independent of the idiomatic Rust API; no parser is imposed on Rust applications.

## Performance comparison

The same deterministic streaming-text and 100,000-row virtual-list UI is implemented in zgui, GPUI and QuickGUI. Only viewport rows are mounted. All adapters have locked dependencies and share the workload source.

```sh
ZGUI_MODE=both ZGUI_SECONDS=10 cargo run --release -p zgui-desktop --example component_workload
ZGUI_MODE=idle ZGUI_SECONDS=0 cargo run --release -p zgui-desktop --example component_workload
```

The component workload uses the native GPU host and owned declarative views. `ZGUI_MODE` accepts `idle`, `stream`, `scroll`, or `both`; zero seconds keeps the window open.

A heavier scene, `heavy_workload` (with a GPUI adapter), is a 1280×800 live dashboard where most content changes every tick: 30 cards with scrolling sparklines, stat tiles, a scrolling 100,000-row status table and a streaming log. On an M5 Max, zgui used 45–47% less CPU than GPUI with live data and 70% less while scrolling, and 25–33 MiB more memory ([results](docs/results/latest-macos-heavy-2026-09-27-92fd6a4/README.md); `python3 scripts/run_macos_comparison.py --scene heavy`). The older `zgui-demo` executable remains available for low-level renderer diagnostics, including `ZGUI_RENDERER=software` and display-free `--headless` checks.

See [benchmark commands and methodology](docs/benchmarking.md). The
[parity-expansion comparison](docs/results/gpui-parity-workers4/README.md) records
36 matched trials and 10,808 audited samples on Mesa llvmpipe. Active trials
delivered 59.6–59.95 model updates per requested second. zgui's active CPU medians
were 28.5–53.6% below GPUI and 9.3–41.9% below QuickGUI in this condition. Mean RSS
medians were 103.35 MiB idle and 108.16–108.39 MiB active; scrolling RSS was nearly
tied with GPUI. Idle CPU tied QuickGUI at zero sampled ticks. Full repeat ranges,
raw samples, matched screenshots, source archives, executable hashes and plots
are retained. `LP_NUM_THREADS=4` applies per Mesa pool; QuickGUI initialized two
pools. Unrelated host activity was present, with no concurrent builds observed.
These are **software-GPU measurements, not hardware-GPU rankings, presentation
cadence, or proof of minimum resource use**. Earlier series remain separately
archived in the [result inventory](docs/results/README.md).

## Validation and current boundaries

```sh
cargo test --workspace --all-targets --locked
cargo test --workspace --doc --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo fmt --all -- --check
```

GPU tests require a Vulkan or Metal adapter, including software Vulkan for CI. The [integrated Linux evidence](docs/platform-validation/gpui-parity-integration/README.md) records 712 workspace tests, 31 doctests, two separately run native lifecycle tests, formatting and strict Linux/macOS-target Clippy. The [macOS continuation evidence](docs/platform-validation/macos-continuation/README.md) adds real Apple M5 Max Metal/CoreVideo pixels and lifetimes, native window probes, AppKit dialogs/menus, bidirectional TextEdit clipboard exchange, native dead-key commit/focus-reset checks, accessibility actions and Retina layout/painting. It preserves observed failures and repairs. **macOS qualification remains partial:** Pinyin candidate-based IME, several native interaction details and hardware performance comparison remain unverified; passing compilation or automated tests alone does not close these gates.

On the tested Sway/Fcitx Wayland stack, very rapid input can leave preedit stale; the same failure reproduces in an independent winit program. Unicode commits succeeded in these tests. See the [protocol evidence and open compatibility issue](docs/validation.md#wayland-burst-input-serial-diagnosis).

Development remains active. The [capability matrix](docs/gpui-parity.md) tracks implementation and target-platform evidence separately. See [layout](docs/layout.md), [rich text](docs/rich-text.md), [canvas drawing](docs/canvas.md), [paint styles](docs/paint-styles.md), [SVG transforms](docs/affine-images.md), [media loading/playback](docs/images-and-animation.md), [actions](docs/actions.md), and [native services](docs/platform-services.md), and [macOS CoreVideo surfaces](docs/native-surfaces.md). Backdrop blur reconstructs the affected backdrop when its dependency region changes. Core-only text measurement uses a fallback until a native shaper is installed. See the [GPU renderer's precise resource limits](crates/zgui-gpu/README.md) and [completion plan](docs/ROADMAP.md) for unresolved work; the project does not claim that every GUI feature is finished or that minimum possible CPU/RAM use has been proved.

References: [GPUI](https://gpui.rs/), [zui](https://github.com/zeronsh/zui), [QuickGUI](https://github.com/egoist/quickgui), [React](https://github.com/react/react). Their designs informed this framework. zgui uses direct dependency subscriptions instead of rerunning whole component functions. Font and dependency licenses are retained alongside their assets and manifests.
