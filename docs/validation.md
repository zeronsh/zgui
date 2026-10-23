# Validation record

Development checks run on Linux x86_64. They establish behavior of the tested builds; they do not establish macOS runtime correctness, hardware GPU performance, production readiness, or minimum possible resource usage.

## Automated checks

The current integrated workspace run passes **600 tests**, with three tests ignored by the ordinary suite: opt-in independent-device GPU stress, native clipboard shutdown, and native host suspension/recreation (the two native tests run separately on X11). Coverage includes reactive collections, retained component ownership, constrained and absolute layout, fluent styles, editors and controls, ordinary/virtual scrolling, owned modal/popover/menu portals, reentrant focus and disposal, lexical providers/slots, tasks, native accessibility-tree consumers, software differential rendering, surface recovery and GPU pixel/resource tests. All **31 documentation examples**, formatting and strict workspace Clippy pass. The workspace also cross-checks for macOS ARM64; that is not native runtime validation. Rerun the complete commands after integration:

```sh
cargo test --workspace --all-targets --locked
cargo test --workspace --doc --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo fmt --all -- --check
```

GPU tests use an actual Vulkan adapter here, implemented by Mesa llvmpipe on the CPU. They check premultiplied color, damage, DPI, shaped text, rounded geometry, images, blur, bounded caches, isolated group opacity, nested invalidation, fractional movement, texture reuse and teardown. They fail if an adapter is unavailable. The 1,000-frame streaming stress test checks bounded caches and reused vertex storage.

The software differential tests compare incremental output against a cleared full redraw across 1,550 deterministic mutation frames, including fractional bounds, alpha, nested clipping, text, layout, moves, node reuse and filter toggles/removal. These caught a fractional damage boundary bug. Unit tests also guard the separate layout preparation path used by input, which must preserve damage for the next renderer frame.

Surface acquisition uses the same bounded recovery policy exercised by injected Lost, Outdated, Timeout, Occluded, recreation-error and validation-error tests. Desktop presentation retains skipped frames, retries timeouts after 16/32/64 ms without rerendering, then sleeps until new damage or exposure. Occlusion waits for visibility. Focus lifecycle tests cover repeated deactivation, removed/disabled targets, consumed restoration state and canceled pointer presses. These deterministic tests do not substitute for native OS suspension or real driver surface-loss testing.

## Soak and failure checks

`crates/zgui-script/tests/soak.rs` uses an app for a while, headless: each cycle mounts a large tree (rows with tooltips and context menus), opens a menu, a popover and a modal, streams markdown, switches theme and direction, draws on the GPU where there is one, and every few cycles hot-reloads its script, then closes everything. After a warm-up, scene nodes, effects and semantic nodes must be what they were a reload period earlier, and the Luau heap, the GPU's caches and allocations, file descriptors, Mach ports and the physical footprint must stay within what the warm-up reached. CI runs 12 cycles (two seconds) on Linux and macOS. A long run:

```sh
ZGUI_SOAK_CYCLES=600 cargo test --release -p zgui-script --test soak -- --nocapture
```

Measured on macOS (M-series, Metal), 600 cycles: everything zgui holds is flat from the first cycles; the footprint grows 8 MB over the first 300 cycles (the allocator and the driver warming up) and then not at all.

`examples/desktop_soak.rs` does the same with real windows: each cycle opens three (a large tree each, a menu and a popover open, which macOS shows in popup windows, a tooltip, a prompt sheet), closes them, and prints the process's footprint, descriptors, ports and threads; `scripts/desktop_soak.py` judges that they level off:

```sh
cargo build --release -p zgui-desktop --example desktop_soak
python3 scripts/desktop_soak.py target/release/examples/desktop_soak --cycles 300
```

It opens windows for the whole run (a cycle a second); leave the machine to it. `desktop_soak 400 --pause 100` pauses with its windows closed every 100 cycles, a still moment to compare the heap (`heap -s`, or with `MallocStackLogging=1` and a binary signed with `get-task-allow`, `malloc_history -allBySize`). That found two leaks per closed window, both fixed: its popup host stayed registered with the popup windows' watcher when it closed with a menu open, and its Metal layer, freed after the view it observes, left AppKit's records of the observation behind. Measured after the fixes on macOS, 600 cycles: descriptors, ports and threads flat, the footprint's quarter means 290.5, 303.7, 303.9 and 305.6 MB (warm-up, then within 2 MB); the live heap, paused, 17.4 MB at cycles 100, 200 and 300 within 0.3 MB, what grows being libdispatch's own cached blocks.

`examples/failures.rs` fails twice while it animates, for `scripts/failures_smoke.py` (a Linux native smoke): a task panics, which the host contains and shows in a banner, then its GPU device is lost (`WindowHandle::debug_lose_gpu`), and it makes a new renderer and draws on. `crates/zgui-gpu/tests/device_loss.rs` checks the renderer's side; `crates/zgui-script/tests/failures.rs` a script's runtime error, a missing image and a missing font.

## Linux native checks

Native examples run under Xvfb with Openbox, using llvmpipe Vulkan:

- The new declarative `components` example passes actual native +, =, and done clicks, yielding count `3`, title `stable` and done `999`; [screenshot](images/components.png). No application-side scene parent IDs are needed.
- The widget gallery passes pointer activation, Unicode model rendering, typed text, Ctrl+A selection, clipboard copy/paste between fields, Tab focus, native resizing and timed close. `scripts/desktop_smoke.py` verifies final model/semantic values rather than only checking that a window opened.
- The multi-window smoke verifies independent child/grandchild lifetime, a rejected first close request, an accepted second request, exactly-once close cleanup, and cancellation of a pending window before its builder executes.
- The workbench passes virtual-row selection, note editing/autosave across selection changes, menu opening/dismissal, modal confirmation and viewport resizing. See [workbench](images/workbench.png) and [dialog](images/workbench-dialog.png). Its draft storage is session-only memory.
- GPUI, QuickGUI and zgui render the same frozen deterministic workload using their actual pinned frameworks. Geometry/content/colors match; native text rasterization/baseline differences remain. See the [comparison protocol](benchmarking.md) and [raw results](results/README.md).

Further native Linux probes, each on a private Xvfb/Openbox desktop or a nested Sway or Weston compositor:

- Wayland clipboard: copy and paste go through the compositor (`set_selection`, `receive`, `send` in the protocol trace) via one data device per seat, which Weston requires (`wayland_input_smoke.py`).
- The primary selection: an editor's double-clicked word and Ctrl+A reach other clients, and another client's selection lands where a middle click puts the caret, on X11 (`xclip`) and as a native Wayland client of Sway (`wl-paste`/`wl-copy --primary`) (`primary_selection_smoke.py`). Selected display text is the primary selection too (`selectable_text_smoke.py`).
- Cursor styles as a native Wayland client: `wp_cursor_shape_v1` shapes for move, not-allowed, crosshair, a reactive wait and the default (`wayland_cursors_smoke.py`), beside the X11 cursor-image check.
- An undecorated window's title bar (`Titlebar::Inset`): no window manager frame, its drawn buttons maximize, restore, minimize and close it, and a press on its corner resizes it through the window manager (`window_buttons_smoke.py`).
- The desktop's interface font: GNOME's `font-name` from a stand-in portal reaches the gallery's sans-serif text (`desktop_font_smoke.py`).
- New windows fit the work area on X11: on a 1024 by 700 screen whose top 40 pixels a panel reserves, the gallery (900 by 760, decorated) and the kitchen sink (1120 by 740, undecorated) open below the panel and within the screen, frame and all (`work_area_smoke.py`).
- The double-click time follows the desktop's (GNOME's `double-click` from a stand-in portal): at 1000 ms two presses 700 ms apart select a word, at 300 ms two 450 ms apart do not (`double_click_time_smoke.py`).
- A touchpad flick coasts on as a native Wayland client of a headless sway: a virtual pointer (`wayland_pointer.py`, `zwlr_virtual_pointer_v1` in plain Python) scrolls with two fingers at 120 Hz and lifts them; the rows still move between 80 and 500 ms after the lift and have stopped by 2 s (`touchpad_fling_smoke.py`).
- A launcher's token reaches the first window: on X11 its startup notification ends (`_NET_STARTUP_INFO` on the root window), as a Wayland client of sway it activates its surface with the token (`launch_token_smoke.py`).
- A file dialog with no portal on the session bus: zenity's chooser shows, the path typed into it reaches the app, and closing the app's window while it shows ends it (`zenity_dialog_smoke.py`).
- Popups past the window on X11: popup_probe's menu shows in its own override-redirect window, typed a popup menu and transient for its parent, past the window's bottom edge, drawn and chosen there; a press on the bare desktop closes it; no popup window exists before one is near, none shows after, and the app is idle (`popup_windows_smoke.py`). The menus, submenus and overlays probes run through popup windows too: a submenu flips at the screen's edge, and a panel hangs past a short window's bottom edge.
- Drag previews on X11 (under picom): the drag_drop example's card preview shows in its own override-redirect window, drawn, the pointer passing through it (an empty input shape); it follows 30 pointer moves past the window's edge pixel for pixel with no frame drawn in it or the window; released on nothing, it slides back to its card and hides; dropped on the destination, it goes at once; and the app is idle after (`drag_preview_smoke.py`).
- Drags between windows on X11 (under picom): drag_probe's tab dropped on its other window's strip lands there; over an xmessage window covering that strip it does not, and released there it is torn off; tab_windows' tab torn off below its window opens with the tab's grabbed point under the pointer, to the pixel; that window's only tab moves the window, which follows the pointer to the pixel and stays where it is let go, and dropped on the other window's strip merges into it (`drag_windows_smoke.py`).
- Where windows open on Linux: on X11 (Openbox) a second window opens a title bar's height down and right of the first; a window with `frame_autosave` moved and resized is saved once the moves settle (the app then killed), and reopens there at its size; maximized and closed at once with Alt+F4 it reopens maximized, and restores to the frame before; at a saved frame no monitor shows it opens on the screen at its saved size. Under Weston it reopens at its saved size, or maximized (`window_frames_smoke.py`).
- Drags out to other apps on X11: drag_probe's chip, exporting its text, dragged with real input onto a GTK window of the probe's own (`xdnd_target.py`) is taken over XDND as `UTF8_STRING`, and the source hears it accepted; out of the window the preview follows the pointer pixel for pixel, drawing nothing and the window nothing; back over its own window, with Escape, or over a GTK target that refuses it, nothing is dropped and the source hears it not taken (`xdnd_source_smoke.py`).
- Drags between windows on X11 (under picom): drag_probe's tab dropped on its other window's strip lands there; over an xmessage window covering that strip it does not, and released there it is torn off; tab_windows' tab torn off below its window opens with the tab's grabbed point under the pointer, to the pixel (`drag_windows_smoke.py`).
- GL where no Vulkan driver is: the gallery on Xvfb with no Vulkan driver to find starts and draws the same pixels as through lavapipe, with Mesa's GL loaded; with Vulkan, no GL driver is mapped (`gl_fallback_smoke.py`).
- Popups past the window on Wayland: under sway, popup_probe's menu clicked open with a virtual pointer shows as an `xdg_popup` past the window's bottom edge, exactly where the scene placed it, drawn; its last item, below the window, clicked, is chosen and the popup goes; opened again, a press in the window closes it. Under Weston, the menu opened by the app sits the same on the content, below the title bar winit draws (`wayland_popups_smoke.py`).
- Shortcuts on a Russian layout on X11: Ctrl and the A key (ф) select an editor's text, Ctrl and the C key (с) copy it, and the same keys without Ctrl type Russian (`keyboard_layout_smoke.py`).
- An application ID's D-Bus service (`org.freedesktop.Application`): a later launch's `Open` URLs and `Activate` reach the running app, which reopens a window after its last one closed, besides fullscreen, minimum size and a custom title bar's drag (`native_platform_smoke.py`). Its thread and the portal settings' sleep until a message arrives (`linux_dbus.rs`).
- Live Wayland scaling through 1×, 2×, 1.5× (fractional, `preferred_scale(180)`) and back: viewport, viewporter destination, editor geometry, typed input and pixel extents (`dpi_transition_smoke.py`).

Every probe is listed in `scripts/linux_smokes.py`, which runs them side by side (each owns its display or compositor) and reports each one:

```sh
cargo build -p zgui-desktop --examples --locked
cargo test -p zgui-desktop --lib --no-run --locked
python3 scripts/linux_smokes.py --jobs 4              # all of them
python3 scripts/linux_smokes.py --only wayland --only primary
```

CI runs on every push to main and every pull request (`.github/workflows/ci.yml`): a Linux job (fmt, stylua, clippy, `zgui lint` and `zgui check` over every example script, unit and doc tests (zgui-script's included), Vulkan rendering tests on llvmpipe, then every native probe above, four at a time), a macOS job (clippy, tests, Metal rendering and readback, component window lifecycle) and a Windows job (check, core and bundle tests, Direct3D 12 border and transform pixel tests on WARP). Probes wait for what they drive (the window manager, IBus's XIM server and engine, a mapped window) rather than sleeping, so a loaded runner slows them without failing them.

## Platform coverage

Recorded native runs use `scripts/platform_smoke.py`, which creates and cleans up its own Xvfb/Openbox and Weston instances. The [result inventory](platform-validation/results.json) and its adjacent screenshots/native Wayland logs are retained in the repository:

| Platform/backend | Evidence | Remaining limits |
| --- | --- | --- |
| Linux X11, forced scale 1.5 | 1350×1140 initial physical surface; pointer activation, text entry, selection, cross-field clipboard, Tab focus, resize to 1500×1200 with logical viewport 1000×800, timed close | Forced initial scale does not test moving between monitors |
| Linux X11, forced scale 2 | 1800×1520 initial physical surface; same input checks; resize to 2000×1600 with logical viewport 1000×800 | Same monitor-transition limit |
| Linux Wayland, headless Weston/pixman | Captured actual gallery surface, semantic snapshot, timed close, multi-window survival/close-policy/exactly-once cleanup | This historical headless run had no input seat; see the newer nested-seat input run below |
| macOS ARM64 target | `cargo check --workspace --all-targets --target aarch64-apple-darwin --locked` passes from Linux | Type checking only: no native linking, execution, Metal or AppKit interaction evidence |

All native rendering above uses llvmpipe, not a hardware GPU. Reproduce the isolated runs with:

```sh
cargo build -p zgui-desktop --examples --locked
python3 scripts/platform_smoke.py target/debug/examples/gallery \
  --windows target/debug/examples/windows --output /tmp/zgui-platform-validation
```

The script requires Xvfb, Openbox, xdotool, Weston, weston-screenshooter and Python Pillow. It writes screenshots, compositor logs and `results.json`; the Wayland screenshot check rejects an absent gallery surface. X11 checks assert actual native window dimensions and the model's final logical viewport, so merely issuing a resize command cannot pass validation.

## Unverified platform behavior

Real macOS windows, Metal presentation, native macOS clipboard/IME/accessibility interaction, Wayland/macOS transparent-window composition and hardware-GPU throughput/energy have not been validated here. Native X11 alpha composition now passes under the owned picom setup recorded below. A private live Linux AT-SPI client now validates text, actions, state changes and accessibility reactivation. Orca end-user usability remains unvalidated; bus-client checks do not establish the complete screen-reader experience. Native IBus/libpinyin composition, cancellation, focus changes and candidate placement now pass on X11; Wayland and macOS IME services remain unvalidated.

Use a real display and equivalent hardware renderer configurations for performance conclusions. In llvmpipe measurements, process CPU includes CPU execution of GPU shaders and driver allocations can dominate RSS. Keep these results distinct from hardware-backed measurements and from core microbenchmarks.

The surface/focus lifecycle changes also pass a fresh full native smoke run: X11 at 1.5× and 2× and Wayland presentation/multiwindow lifecycle. Its separate [result inventory](platform-validation/lifecycle/results.json) and screenshots preserve the earlier platform evidence. That historical Wayland run had no input seat; the newer nested-seat run below covers input separately.

The declarative [form example](../crates/zgui-desktop/examples/form.rs) passes native X11 pointer focus, selection replacement, cross-editor clipboard, Tab focus, multiline entry, disabled editing and reactive/native resize via `scripts/form_smoke.py`. Its [model report](platform-validation/form/form.log) and [screenshot](platform-validation/form/form.png) are retained. GPU editor tests compare incremental and full frames after font changes, selection, text replacement and shrink/grow cycles; they also check glyph/caret colors and scroll restoration.

The public component virtual list has bounded-work tests on 100,000 rows, keyed state/index retention, resize and count clamping, constructor failure cleanup, wheel chaining and zero-layout scrolling within the same range. Numeric tests cover extreme counts and finite output geometry; observer tests cover reentrant mounting and removal during size notification. GPU tests verify padded viewport clipping and incremental/full equality across large scroll offsets and allocation changes.

The component workload native debug smoke runs all four modes successfully and visually verifies +, = and done controls. [Reports](platform-validation/component-workload/results.json) and [screenshot](platform-validation/component-workload/idle-done.png) are preserved. Debug llvmpipe delivered 179 scroll ticks and 57–59 streaming ticks in three seconds; this is correctness evidence, not release performance evidence. Existing benchmark numbers still describe the earlier direct-scene adapter.

A fresh [normalized public-component release comparison](results/component-comparison/README.md) completed all 24 runs. Its 3,395 raw samples, 70-file source archive and frozen binary hashes were audited. Active runs delivered 594–599 updates per ten seconds; all zgui active runs delivered 599. The series shows lower sampled active CPU for zgui than both references on llvmpipe, while GPUI has lower RSS. Native screenshots of the same seeded workload were inspected. These are software-GPU measurements, not hardware or macOS performance evidence.

Dynamic native window controls pass title, client resize, minimization/restoration, maximization and visibility checks on X11/Openbox; [external state observations](platform-validation/window-controls-x11.json) supplement the example's viewport assertions. Pending/suspended request-state unit tests do not replace native OS suspension validation. Five allocation-feedback regressions cover oscillation, growth, reentrant preparation, panic cleanup and recovery; fallible native frame/input preparation reports errors rather than hanging.

A [same-binary backend-initialization ablation](results/component-memory/README.md) found approximately 22 MiB less idle RSS and 49 fewer threads with Vulkan-only than Vulkan-plus-GL initialization on llvmpipe. The default now enables primary backends and honors wgpu environment overrides. On Linux, GL is made only where Vulkan finds no driver, so that cost is never paid beside Vulkan: `gl_fallback_smoke.py` checks that a Vulkan process maps no GL driver, and that with none the gallery draws the same pixels through Mesa's GL. GL adds 643 KB to a stripped release gallery on Linux (19,058,776 against 18,415,184 bytes) and is not built elsewhere. This is separate from the earlier frozen comparison and does not update its active-workload CPU/RSS numbers.

The primary-backend build passes a fresh native X11 1.5×/2× input/resize smoke and Wayland presentation/multiwindow smoke; [artifacts](platform-validation/primary-backend/results.json) preserve those runs. That historical Wayland run used a headless compositor without an input seat.

Rendering now stops before layout preparation/scene flushing for native-confirmed hidden, minimized, occluded or zero-sized surfaces. Unit tests preserve queued damage, cancel retry deadlines and prevent redraw requests from overriding native occlusion. The [hidden-update native regression](platform-validation/hidden-updates/results.json) verifies unmapped X11 states, 40 model updates and 42,000 expected latest-state pixels after both show and restore. A 100,000-node-replacement core regression bounds stale dirty generations without any frame flush, preventing hidden virtual-list churn from growing the queue indefinitely. X11 minimized-property roundtrips are skipped for focused and already-hidden windows.

Keyed constructor recovery regressions cover both public components and low-level scopes: a late failure after an earlier new child has mounted releases all staged subscriptions/resources, preserves the original child order/identity/local state and permits successful retry. The complete workspace passes 214 tests, 15 doctests, strict Clippy, formatting and macOS ARM64 all-target cross-check after this change.

Declarative checkboxes share fluent styles and inherited typography with other components, including indicators that expand for large fonts. Four component regressions cover input, model/semantics synchronization, disabled state styles, cleanup and sizing. An AccessKit consumer regression verifies checked-state updates, disabled activation rejection and unmount cleanup. The [native checkbox example](../crates/zgui-desktop/examples/checkbox.rs) passes label clicks, Space activation, disabled behavior and external writes under an owned Xvfb window; [results and screenshot](platform-validation/checkbox/result.json) accompany `scripts/checkbox_smoke.py`. Linux CI runs this smoke. Reusable `Styled::apply`, `when` and `when_some` transformations preserve sparse patches; dynamic signal styles continue to use `reactive_style`.

Declarative sliders use allocated content dimensions and padding for thumb/rail drawing and pointer mapping. Four component tests cover resize, capture, keyboard, extreme finite ranges, external normalization, initial/reactive disabled semantics, inherited colors and cleanup. An AccessKit consumer verifies range/value/action updates; GPU readback compares partial/full frames after value, width and color changes. The [native slider run](platform-validation/slider/result.json) validates drag, Home/End/arrows, reactive width, disabled suppression and external values via `scripts/slider_smoke.py`; Linux CI runs this check. Legacy slider and progress geometry also responds to allocation changes, with two regressions. The integrated run passes 228 tests, 17 doctests, formatting, strict Clippy and macOS ARM64 all-target cross-check.

Declarative `progress` uses a clipped translated fill: value-only updates and slider value changes perform zero layout work. Core tests cover normalization, idle equality, resize, disposal and semantics. GPU readback verifies exact fill area, padding exclusion, and partial/full repaint equality across endpoints and resizing. An AccessKit consumer checks the read-only numeric role and live values. Native [progress evidence](platform-validation/progress/result.json) covers seven states with exact pixel assertions; slider smoke is rerun after its fill optimization. The workspace passes 233 tests and 18 doctests, strict Clippy, formatting and macOS ARM64 cross-check.

Declarative `image` and `image_signal` retain decoded image sources under fluent decorated roots. Tests cover intrinsic growth/shrink, allocated sizing, constraints, padding, source identity, cleanup and invalid children. Same-dimension image replacement invalidates paint without layout; GPU readback verifies exact content pixels and damage reconstruction across source and size changes. An AccessKit consumer checks image descriptions/bounds/removal. The [native image run](platform-validation/images/result.json) verifies five stages with exact content/padding pixels and is wired into Linux CI. The full workspace passes 240 tests and 19 doctests, strict Clippy, formatting and macOS ARM64 cross-check.

Ordinary `scroll(offset)` components measure retained child extents, clamp on content/viewport changes, and translate without layout for offset-only changes. Four component regressions cover dynamic extents, nested wheel routing, state retention, disabled state and cleanup. Initial disabled virtual-list semantics now survive styling. Native accessibility exposes page-up/down actions routed through scrolling input, with page/clamp/disabled tests and an AccessKit consumer. GPU differential checks and [native scroll pixel evidence](platform-validation/scroll/result.json) validate padded clipping and resize/shrink updates. Linux CI includes the scroll smoke. The integrated suite passes 248 tests and 20 doctests, formatting, strict Clippy and macOS ARM64 cross-check.

Ordinary scroll focus reveal handles Tab/direct/native focus transitions, nested viewports, batched model transactions, oversized targets, redirected focus and cleanup. Manual scrolling does not continuously reveal the current focus. GPU damage comparison verifies actual newly visible control pixels without relayout; AccessKit consumer checks updated descendant bounds/focus. The [native focus-scroll smoke](platform-validation/focus-scroll/result.json) checks Tab, Shift-Tab and unchanged focus after manual wheel scrolling. Linux CI includes this smoke. Integrated validation passes 255 tests, 20 doctests, strict Clippy, formatting and macOS ARM64 cross-check.

Horizontal `scroll_x` shares retained scrolling and focus reveal with the vertical component, but uses row layout and horizontal wheel deltas. Mixed-axis nested tests cover wheel routing, batched focus, extents and resizing. `ScrollAxis` metadata exposes only applicable native page actions; tests reject wrong-axis requests and respect disabled input. GPU tests compare partial/full repaint and padded clipping. Native [horizontal-scroll evidence](platform-validation/horizontal-scroll/result.json) covers wheel buttons, Tab/Shift-Tab, manual-scroll persistence and resize clamping; Linux CI runs this smoke. Integrated validation passes 262 tests, 21 doctests, strict Clippy, formatting and macOS ARM64 cross-check.

Opt-in overlay scrollbars cover ordinary vertical/horizontal views and virtual lists. Four integration tests verify paging, captured drag/resize/cancel, keyboard/numeric actions, content click-through, hidden lifecycle and virtual extents. GPU readback checks exact thumb pixels, zero-layout offset changes and clearing on shrink. An AccessKit consumer verifies orientation, numeric actions, focus, hide/restore and disabled ancestors. Native [scrollbar evidence](platform-validation/scrollbars/result.json) exercises track clicks, captured drags, Home/End/PageUp/PageDown, horizontal drag and overflow removal; it exposed the now-fixed explicit pointer-focus requirement. Linux CI includes this smoke. Integrated validation passes 268 tests, 21 doctests, strict Clippy, formatting and macOS ARM64 cross-check.

Opt-in virtual-list keyboard navigation mounts and focuses endpoint/page/neighbor rows without scanning intervening keys. Four component regressions cover million-row bounds, batching, keyed focus, removal fallback, child control keys and cleanup. GPU tests verify focused opaque-row pixels and damage; an AccessKit consumer verifies one-based set position, total size and shrink fallback. Native [million-row evidence](platform-validation/virtual-keyboard/result.json) reports 6–7 live rows and 19 total builds over four navigation actions. Linux CI includes the smoke. Final integrated validation passes 275 tests, 22 doctests, formatting, strict Clippy and macOS ARM64 cross-check.

An earlier parallel GPU test process exited with SIGSEGV without a stack. Targeted 24-worker device-lifecycle stress subsequently reproduced a loader fault matching an upstream Vulkan-Loader race. The same binary passed three 240-lifecycle runs with patched loader 1.4.345, retaining debug and validation defaults. [Stacks, matched runs, source and build provenance](platform-validation/gpu-lifecycle-stress/README.md) are retained. This strongly supports the loader diagnosis for the reproduced crash; it cannot prove the earlier stackless failure identical. Ordinary fixtures serialize resource lifetimes and retain an explicit four-device concurrency test; the larger stress test is opt-in. Application and benchmark settings are unchanged.


Declarative modal and popover portals now use ordinary children, fluent panel styles,
lexical providers/slots and logical-owner disposal. Absolute layout excludes portals
from surrounding flow; world-bounds observation tracks anchor translations without
relayout. Tests cover nested/sibling focus scopes, non-LIFO removal, reentrant blur
unmounting, open-tree replacement, disabled logical ancestors, virtual-row disposal,
and constructor failures. GPU partial/full readback agrees through open/close and
clipped-anchor movement; native AccessKit consumers verify modal metadata and focus.
[Eleven native X11 stages](platform-validation/overlays/README.md) cover placement,
resize flipping, Tab containment, nested popups and dismissal. The final integrated
check logs are retained with that evidence. Closed panels keep their subscriptions;
this is not a claim that all hidden-subtree computation is suspended.


Declarative menus now expose accessible menu/item roles, trigger popup/expanded
state, retained component/keyed items, disabled-item skipping, bounded Unicode
prefix search, arrow/Home/End navigation, Escape and Tab exit. Actions close the
menu chain before invoking application callbacks. Automatic overflow preserves
intrinsic sizing and shrinking; focus reveal and wheel scrolling translate retained
content without layout work. Reactive content spacing, partial/full GPU pixels,
AccessKit consumers and [ten native X11 stages](platform-validation/menus/README.md)
pass. Dedicated submenu triggers, side placement and Right/Left navigation are now
implemented and validated below. Native macOS/hardware GPU validation is still separate.


Cascading submenus now own full-width triggers with decorative chevrons, child
panels and focus scopes. Right/Left, Enter, Escape, sibling switching, disabled
ancestry, keyed removal and reentrant disposal have regression coverage. Pointer
handoff routes a single gesture only to an eligible ancestor menu; outside clicks
close the chain without activating background controls. Logical accessibility
parentage keeps portal menus in their parent menu's native tree while preserving
physical bounds/clipping and rejecting cyclic/stale parent overrides. A GPU
partial/full repaint regression and [twelve native X11 stages](platform-validation/submenus/README.md)
pass; compositor-only anchor movement flips the child without layout work.

Native Wayland input now passes through an owned nested Weston seat. The
[protocol/model evidence](platform-validation/wayland-input/README.md) verifies
pointer focus, keyboard selection, Tab, multiline input, disabled controls,
reactive width, actual output resize and clipboard transfer between editors.
The client has no `DISPLAY`, and clipboard assertions require standard Wayland
selection/send/receive traffic. Clipboard backend selection follows the actual
native display; the Wayland worker retains the connection for its lifetime.
This is software-compositor evidence, not physical-seat or real IME validation.

Public component `.on_event` and `.focusable` hooks pass ordering, disposal,
inherited-disabled and default-cancellation regressions across editing, sliders,
scrollbars, scrolling, menus and virtual navigation. The
[native keyboard regression](platform-validation/event-hooks/README.md) confirms
that prevented printable keys emit no editor text, prevented clipboard shortcuts
do not execute, and ordinary spaces/copy/cut/paste still work. The integrated
workspace passes 349 tests (one opt-in stress test ignored), 25 doctests,
formatting, strict Clippy and the macOS ARM64 cross-check. Native macOS runtime
validation remains outstanding.

Opt-in editor soft wrapping now passes core and actual shaped-text tests for
inherited/reactive styles, resize preservation, pointer placement, visual
Home/End, preferred-column navigation, scrolling, Unicode/bidi and trailing empty
lines. Equal wrapping values and idle preparation produce no layout work or
damage. GPU readback verifies multiple painted lines, selection, visible carets
through resize, IME preedit/commit, wrap toggling and exact incremental/full-frame
agreement. The [native wrapped-editor run](platform-validation/wrapped-editor/README.md)
covers five X11 input/resize stages with preserved model text and selection.
The integrated suite passes 362 tests, 25 doctests, formatting, strict workspace
Clippy and the macOS ARM64 cross-check. These remain software-GPU and X11 runtime
checks; native macOS and live input-method-service validation are separate.

Winit 0.30.13 documents formal application suspension for Android, iOS and Web;
it does not emit equivalent callbacks on Linux/macOS. See the
[upstream lifecycle contract](https://docs.rs/winit/0.30.13/winit/application/trait.ApplicationHandler.html#method.suspended),
also verified in the installed dependency source. zgui's callback teardown and
recreation path is defensive portability support. It does not constitute a native
desktop machine-sleep/wake or real driver-loss test; those remain separate gaps.

Real [IBus/libpinyin XIM input](platform-validation/native-ime/README.md) passes
eight native stages: Chinese preedit, committed text, Escape cancellation,
editor focus-switch cancellation and candidate placement beneath each caret.
The test drives native keys in an owned X11/D-Bus/IBus session and independently
checks model text, displayed preedit and candidate-window geometry. It exposed
and fixed X11's baseline-spot convention; other backends keep rectangle geometry.
Native IME routing now rejects late events while inactive or after the owning
editor changes, with two model-level regressions supplementing native evidence.

[Held-key focus transfer](platform-validation/held-keys/README.md) verifies that
synthetic X11 key presses/releases cannot type or activate controls, while actual
subsequent input works. [Live Wayland scaling](platform-validation/dpi-transition/README.md)
checks 1×→2×→1× with native protocol events, fixed physical surface size, correct
logical viewports and caret coordinates, actual pointer edits, and exactly doubled
editor pixels at 2×. The application cannot fall back to X11 in that test.

The [live X11 DPI regression](platform-validation/live-dpi-pointer/README.md)
reproduced stale scale and stationary-pointer routing before the fix. The host
now uses the scale-change event as authoritative for viewport, pointer, wheel,
accessibility, window-size requests and IME conversions. It waits for the native
resize rather than applying a new scale to the old physical size. Only X11's
cached pointer is rescaled; Wayland/macOS surface-local logical coordinates stay
unchanged. Native normal and maximized-window runs pass, including when the
window manager keeps the physical size fixed. Machine sleep, physical monitor
hotplug, real driver loss and native macOS remain separate validation gaps.

This integrated platform revision passes 364 tests (one opt-in stress test
ignored), 25 doctests, formatting, strict workspace Clippy and the macOS ARM64
all-target cross-check. The native scripts are wired into Linux CI; remote CI
execution has not been claimed from these local runs.

Inherited/reactive `.line_height(px)` and `.line_height_normal()` now share a
normalized, hashable font metric across layout, GPU/software rendering and editor
geometry. Tests cover sparse style precedence, component ownership, normal reset,
Unicode wrapping, selection/hit testing, trailing empty lines and idle equality.
The measurement-only fallback uses constant auxiliary memory and the same
placement rules as geometry shaping; its normal pitch now matches the existing
editor convention rather than the former independent approximation.

GPU readback confirms line-height changes reuse rasterized glyphs, preserve model
and selection, and reconstruct the exact full frame after loose/tight/normal
changes. Existing text-box clipping applies to tight outer glyph ink. The
[native four-stage run](platform-validation/line-height/README.md) validates actual
pointer controls and keyboard selection through 24px→42px→12px→normal spacing.
The integrated workspace passes 378 tests (one opt-in stress test ignored),
26 doctests, formatting, strict Clippy and the macOS ARM64 all-target cross-check.


## Bounded editor geometry reuse

The focused editor now shares one bounded layout snapshot across refresh,
selection, scrolling and visual navigation. Six public regressions and ownership,
cache-weight and native GPU tests cover invalidation and disposal. Replacement
shapers refresh caret metrics even when editor bounds remain unchanged. The
[call-count evidence](performance/editor-shaping/README.md) records redundant
shaping removed from unchanged interaction; it is not a CPU-time benchmark.
[Native X11 evidence](platform-validation/editor-shaping/README.md) covers wrapped
resize/editing and inherited line-height changes on an archived 125-input source
snapshot, with both binary hashes retained.


## Percentage sizing and flex allocation

Fluent percentage dimensions now pass core and component regressions for padding,
min/max constraints, sparse pixel/percentage precedence, image sizing, wrapped
editors, indefinite axes and absolute children. Tests cover the definite flex
allocation case even when no size redistribution is needed. A GPU differential
test checks child resize and old-pixel removal. The [native percentage-sizing
record](platform-validation/percent-sizes/README.md) verifies real window resize,
retained text/selection, input placement and panel pixels from frozen sources.


## Inherited letter spacing

Letter spacing now shares normalized font attributes across component inheritance,
measurement, editor geometry, GPU drawing and software rasterization. Tests cover
pixel-to-em conversion, signed tracking, Unicode clusters, selection edges,
wrapping and cache invalidation. A GPU differential test verifies exact pixel
restoration and no new glyph bitmap uploads for integer tracking changes. The
[native X11 record](platform-validation/letter-spacing/README.md) verifies expected
caret/label changes while preserving model and selection on frozen sources.


## Reentrant lifecycle cleanup

A [lifecycle audit](platform-validation/lifecycle-cleanup/README.md) covers repeated
keyed/provider/slot teardown, task completion and destructor reentry, listener
capture disposal, timer/background waker cleanup and retained window factories.
It fixed stale-node and RefCell panics, lock-held waker destruction and clipboard
retention after manager shutdown. The native X11 clipboard regression uses the
archived source and binary, while core probes verify resource release and retained
live-task ownership without requiring a desktop.

A [live Linux AT-SPI run](platform-validation/atspi/README.md) validates ordinary
component controls over a private accessibility bus: readable text and status,
focus, caret and selection, buttons, numeric values, checkbox state, disabled
input and fresh-state reactivation. Static labels now use the AccessKit text
value contract; editors publish cached Unicode TextRuns and route selection
actions through guarded core input. The evidence records the pinned bridge
limitations, source hashes, native trees/events and integrated checks.

[Image fitting](platform-validation/image-fit/README.md) now supports local fluent
Fill/Contain/Cover/None/ScaleDown styles with centered padded clipping. Core
checks cover ownership, reactive precedence, intrinsic sizing and idle updates;
GPU differential tests verify crop pixels and zero texture uploads for fit-only
changes. Eight native resize/mode stages pass 6,608 pixel checks, and the
existing image example retains its original behavior.

A [retained rendering/IME audit](platform-validation/retained-audit/README.md)
removes layout work for fully allocated image-source replacements and prevents
transparent subtrees from uploading textures, rendering layers or applying
backdrop blur. Regressions cover hidden oversized resources and reveal/hide
damage. Accessibility cancellation of editor preedit now releases native
keyboard suppression and guards the queued IME restart interval. Delayed
Wayland compositor commits after local reset notifications remain an explicit
pinned-backend limitation.

[Hidden subtree traversal](platform-validation/hidden-traversal/README.md) now
prunes zero-opacity branches before retained-layer bounds and child traversal.
A hidden 8192×8192 descendant no longer makes a visible 24×24 parent fail the
GPU memory budget; its regression uses 2,704 cached layer bytes. Hidden backdrop
filters also stop expanding unrelated visible damage, while hide/reveal repair
and explicit layer-root opacity remain covered by core/GPU tests.

[Effect normalization](platform-validation/effect-normalization/README.md) keeps
NaN opacity and infinite filter lengths out of paint metadata. Normalization
precedes equality and blur tracking, so repeated invalid reactive values add no
scene damage or layout/compositor invalidation. Core/component/GPU checks cover
sparse override removal, fallback pixels and idle render counters.

[Translation normalization](platform-validation/translation-normalization/README.md)
resolves nonfinite coordinates to zero independently before equality, hit testing
and damage calculations. Direct and composed tests establish repeated-write
idleness and sparse override behavior; GPU tests verify restored pixels and
retained isolated-layer reuse without layout or repaint.

[Pointer chord validation](platform-validation/pointer-chords/README.md) fixes
unrelated mouse-button releases interrupting primary editor/slider/scrollbar
drags. Capture now follows its initiating button. Core tests cover cleanup and
activation; real X11 pointer sequences verify continued drag after secondary
release and stable selection/value after primary release.

[Keyboard chord validation](platform-validation/keyboard-chords/README.md)
preserves pending Space/Enter activation through unrelated releases, consumes
matching presses before reentrant callbacks, and aligns active styling with
key ownership. Native X11 checks cover counts, held/released colors, focus-loss
cancellation and prevented activation.

[Disabled interaction lifecycle](platform-validation/disabled-interaction/README.md)
now publishes disability and detaches interactions before callbacks, tolerates
reentrant removal/reenabling/refocus, and prevents stale releases from activating
uncaptured controls. Composed sliders explicitly focus on pointer presses. Core
regressions and native X11 held-key/button/slider checks cover cleanup and recovery.

[Text history limits](platform-validation/history-limits/README.md) remain enforced
when undo/redo transfers follow a limit reduction. Cached payload totals avoid
whole-history scans, and lowering limits trims/shrinks both stacks. Core tests
cover accounting and eviction; a native editor shortcut sequence verifies the
reduced one-entry limit across redo and undo. Budgets apply independently per
stack and exclude entry/container/allocator metadata.

[External-model IME cancellation](platform-validation/external-ime/README.md)
now synchronizes local preedit cancellation with the host, including model
replacement and selection/navigation edits. A real X11/IBus probe replaces the
model without refocusing or switching engines, rejects the old candidate, edits
the replacement normally and commits a fresh Unicode composition. Native empty
preedit followed by Commit remains valid; Wayland protocol-serial limitations
and the distinction from a complete native session epoch remain explicit.

## Committed editor text normalization

Initial values and external model updates now follow the same CRLF/CR policy as
paste and accessibility replacements; typing and IME commits agree as well.
Canonical-equivalent updates preserve selection, preedit and history without
layout or damage. Construction batching prevents reentrant model observers from
unmounting partially registered editors or declarative trees. Native tests check
model/display agreement, retained nodes, continued input and multiline caret
placement; both IME suites also pass. [Sources, hashes and validation logs](platform-validation/model-text/README.md)
record 489 workspace tests, 29 doctests and the platform checks.

## Read-only editors

Static and reactive read-only policies keep editors focusable/selectable while
blocking user mutations. Core tests cover both editor forms, clipboard methods,
accessibility actions, history preservation, repeated-write idleness and owned
subscription cleanup. AccessKit projection retains selection/focus actions while
removing SetValue. Native X11 clipboard/input tests and isolated real IBus
composition cancellation pass, along with the existing two IME suites.
[Archived source, commands and results](platform-validation/read-only/README.md)
include 497 tests, 29 doctests, Clippy, formatting and the macOS cross-check.
Native macOS behavior and arbitrary delayed Wayland server commits remain
outside this evidence.

## Initial binding ownership and disposal

Failed initial bindings now clean up their public mount even after an outer
batch or active effect deferred execution beyond the mount call. Regressions
cover exact sibling preservation, conditional/keyed initial children, retained
resources, effect counts and retry. Binding registration precedes callback
execution, and self-disposed effects cannot acquire orphan subscriptions.
Later update failures retain their mounted view; an already replaced document
is not restored after a deferred render failure. [Archived validation](platform-validation/initialization/README.md)
records 509 tests, 29 doctests, strict Clippy, formatting, the macOS cross-check
and successful native editor/model/virtual-list regressions.

## Streaming editor replacement deltas

External text replacements now retain only one changed UTF-8 span in history;
canonical model ingress borrows signal text. Unicode/history/selection/IME tests
pass, including small-byte-budget streaming and full undo/redo roundtrips.
[Integrated validation](platform-validation/editor-deltas/README.md) records
516 tests, 29 doctests, strict Clippy, formatting, the macOS cross-check and three
native editor suites. The [isolated editor benchmark](results/editor-streaming/README.md)
measures median 1,000-update time of 42.65→10.20 ms, live requested allocation
bytes of 4,775,648→1,060,788 and retained undo steps of 7→100 across five runs per
version. These are headless editor measurements, not whole-framework or GPU
performance results.

## Editor refresh text storage

Selection/read-only refreshes now retain paint text and semantic string storage.
Measured allocation regressions prove zero semantic-update allocations for
unchanged or same-sized text, and no document-sized allocation for warmed
selection/policy refreshes of a cache-eligible 32 KiB Unicode document.
AccessKit incremental consumer and live AT-SPI checks preserve full text and
selection; model normalization and native IME suites pass. [Archived validation](platform-validation/editor-refresh/README.md)
records 520 tests, 29 doctests, strict Clippy, formatting and the macOS cross-check.
Cold/over-budget shaping and native AccessKit owner-value copies remain separate
costs; these checks do not establish whole-framework timing or RSS improvements.

## Unchanged accessibility projection reuse

Per-node semantic revisions and cached geometry/children allow unchanged native
nodes to skip value construction. Allocation tests verify that unrelated updates
and idle projection do not copy a 128 KiB editor document. Invalidation coverage
includes ancestry, scaling, logical children, visibility, reconnects, store
replacement and partial semantic mutations on unwind. [Archived validation](platform-validation/accessibility-reuse/README.md)
records 526 tests, 29 doctests, strict Clippy, formatting, the macOS cross-check
and successful live AT-SPI, editor model and native IME suites. Projection still
traverses scene metadata; changed nodes still allocate owned AccessKit values.


## Bounded GPU backdrop reconstruction and declarative effects

GPU blur no longer forces every changed frame to reconstruct the whole target.
Damage expands through connected filter outputs and their integer physical
sampling halos, then replays those regions in paint order. Backdrop copies cover
only the required source region, and horizontal filtering covers only the output
width and vertical sampling halo. Two full-size scratch textures remain retained;
connected filters and conservative scene damage can still cover the full target.
The software reference backend retains its full-frame blur policy.

Six new GPU differential tests cover distant updates, sampled backdrop changes
outside the output clip, overlapping filters, offscreen clipping, fractional DPI,
tiny and capped sigma, and 64 deterministic mutation frames. A cached isolated
layer with root blur matches a fresh redraw after both backdrop mutation and
fractional translation, with one layer cache hit and no layer repaint; translation
also allocates no layer texture. A distant 4×4 update damages **16 pixels** in a
240×160 scene containing blur. This is a work-count assertion, not a GPU timing
or CPU/RSS measurement. Four helper tests include 4,445 sampling-support checks
across seven scales.

The native effects example now uses component children, fluent styles, reactive
effect properties and component-owned animation tasks without application scene
mutation. The owned X11 smoke drives its actual controls, checks visible changes
from blur/fade/opacity, animates with blur enabled, verifies pause stability and
waits for timed close. Model-text editing and image-fit/resize smokes also pass.
The consolidated run passes 536 ordinary tests, 29 documentation examples,
strict Clippy, formatting, and the macOS ARM64 cross-check. Native macOS and
hardware GPU execution remain unverified.

[Source manifest](platform-validation/blur-damage/source-manifest.json),
[run metadata](platform-validation/blur-damage/metadata.json), and
[native effects results](platform-validation/blur-damage/effects/results.json)
record this build. Archived inputs and their hashes were independently checked
against the workspace. The Linux renderer is Mesa llvmpipe using the private
patched Vulkan loader; historical framework CPU/RSS comparisons remain unchanged.


## Editor visual-page navigation

Multiline editors now handle Page Up/Down and Shift+Page Up/Down using the
padded viewport height with one visual line of overlap. Paging retains the
preferred horizontal position shared with Up/Down, moves scrolling by the
caret displacement, and clamps at document boundaries. Read-only fields retain
this navigation; single-line fields and Control/Alt/Meta-modified page keys
leave defaults unhandled. Paging cancels active preedit without changing the
committed model or undo/redo history.

Seven integration tests cover viewport resizing, reversed Shift selection,
short-line preferred-column recovery, empty/trailing lines, Unicode graphemes
and soft-wrap affinity, fractional line heights, read-only behavior, preedit
cancellation, event bubbling/default prevention, and existing undo/redo history.
Repeated paging through unchanged wrapped text performs no additional shaping
in the instrumented layout-cache test.

The native component example and owned X11 smoke exercise actual page keys,
Shift-selection, read-only input rejection, resized padded viewports, visible
carets, and document endpoints. Fresh wrapped-editor and native IBus IME
regressions also pass. The consolidated run passes 543 ordinary tests and 29
documentation examples, strict Clippy, formatting, and the macOS ARM64
cross-check. These Linux runs use llvmpipe and do not validate native macOS or
hardware GPU execution.

[Source manifest](platform-validation/editor-paging/source-manifest.json),
[run metadata](platform-validation/editor-paging/metadata.json), and
[native paging results](platform-validation/editor-paging/editor_paging/results.json)
record the validated build. All 163 archived input hashes were independently
checked against the workspace; full command logs are retained beside them.


## Editor pointer selection and native click metadata

Editors now support Shift-click anchor extension, Unicode word-boundary segment
selection on double-click, and visual-line selection on triple-click. Dragging
extends whole selected units and reverses around the original opposite edge.
Single-line triple-click selects all text; line selections exclude hard newline
terminators. Selection remains available read-only, cancels preedit, and leaves
committed text and undo/redo history intact. Ordinary synthetic dispatch remains
unmodified single-click input; native/custom-host modifier dispatch supplies
per-event metadata and a target/button-specific 500 ms, four-logical-pixel
click chain. Counts cycle 1/2/3/1 and reset with the documented interaction
lifecycle. OS-specific multi-click preferences are not queried.

Seven new editor integration tests exercise Shift and unit dragging, grapheme-safe
combining/emoji selection, punctuation/whitespace, right-half glyph hits,
wrapped-line boundaries, cancellation, read-only mode, composition and history.
Dispatcher regressions cover deterministic timing/distance, target/button changes,
legacy dispatch, lifecycle resets and nested-event metadata isolation. Native
shaping now exposes logical visual-row ranges separately from physical Home/End
edges: mixed-direction row edges can omit logical clusters. Shaping regressions
and a separate editor-level test cover this distinction, wrapped affinities,
line endings, empty rows and trailing whitespace.

The fresh owned X11 selection smoke passes nine stages, including real native
Shift-click, double/triple clicks, reversed word dragging, wrapped line selection,
fourth-click reset and read-only input rejection. Pointer-button chord/capture
and native IBus IME regressions also pass. The final integrated build passes
555 ordinary tests, 29 documentation examples, strict Clippy, formatting, and
macOS ARM64 cross-checking. Native macOS and hardware GPU execution remain
unverified; Linux rendering uses llvmpipe.

[Source manifest](platform-validation/editor-selection/source-manifest.json),
[run metadata](platform-validation/editor-selection/metadata.json), and
[native selection results](platform-validation/editor-selection/editor_selection/results.json)
record this build. All 166 archived inputs and binary hashes were independently
verified; full command logs accompany the record.


## Stationary editor drag autoscroll

Captured selection now scrolls while the pointer rests outside an editor.
Character, word and visual-line gestures retain their selection rules; single-line
fields scroll horizontally and multiline fields vertically and, when unwrapped,
horizontally. Edge-clamped hit testing prevents distant pointers from jumping to
the document end. Velocity is bounded at 1,200 logical pixels/second and stalled
ticks contribute at most 50 ms. One core interaction deadline runs at 16 ms
intervals while scrolling can advance; idle calls avoid layout preparation.

Ten deterministic tests cover stationary and reversed unit selection, bounded
motion, read-only behavior, unchanged text/history and shaping reuse, capture and
focus lifecycle, model replacement, resize rearming, prevented movement and
idle work. A dormant boundary gesture cannot overwrite another editor's active
deadline after capture transfer. The native host merges interaction and
presentation deadlines and cancels captured gestures when inactive or hidden.

The owned X11 smoke holds the pointer still without diagnostic polling timers.
Vertical and horizontal movement exceed 100 pixels between reports, beyond the
maximum single delayed tick, proving native deadline-driven progress. Returning
inside or releasing stops motion. Hiding while the button remains held cancels
capture; restoring leaves selection and scroll stable. Fresh selection and
hidden-window update regressions also pass.

The consolidated build passes 565 ordinary tests, 29 documentation examples,
strict Clippy, formatting and the macOS ARM64 cross-check. All 168 source inputs
and binary hashes were independently verified. See the
[source manifest](platform-validation/editor-autoscroll/source-manifest.json),
[run metadata](platform-validation/editor-autoscroll/metadata.json), and
[native results](platform-validation/editor-autoscroll/editor_autoscroll/results.json).
Linux native rendering uses llvmpipe; native macOS and hardware GPU evidence
remain outstanding.


## Native X11 transparent-window composition

The transparent-window fixture uses the public component/fluent-style API and
`WindowOptions::transparent`. An owned Xvfb/Openbox/picom XRender session captures
the final composited desktop, rather than reading the application's framebuffer.
The harness publishes a live root pixmap with a known color; a plain root clear
was insufficient for this compositor and initially yielded a black backdrop.
No renderer correction was needed for the verified setup.

Six stages check 32 sample positions: transparent empty pixels, half-alpha and
quarter-alpha panels, their overlap, an opaque control, retained color updates,
hiding/restoring a panel, native window movement/resizing and a changed backdrop.
Expected colors are computed from source alpha and the known backdrop with a
two-channel-value rounding tolerance. Newly exposed resized areas retain alpha
zero. Desktop screenshots and exact sample values accompany the results.

The consolidated run passes 565 ordinary tests, 29 documentation examples,
strict Clippy, formatting and the macOS ARM64 cross-check. Fresh native
transparency, editor autoscroll and hidden-window-update suites pass. All 169
archived source inputs and binary hashes were independently verified. See the
[source manifest](platform-validation/transparency/source-manifest.json),
[run metadata](platform-validation/transparency/metadata.json), and
[compositor results](platform-validation/transparency/transparency/results.json).
This establishes alpha presentation on this X11/picom setup using llvmpipe;
it does not establish Wayland/macOS transparency, compositor backdrop blur,
hardware GPU performance, or OS-native blur materials.

## Native Wayland transparent-window composition

An owned Sway session now verifies the transparent-window fixture as a native
Wayland client, with Xwayland disabled and the client DISPLAY unset. A private
swaybg surface supplies the known background; grim captures the composited
output. Six stages cover alpha overlap, retained color changes, hide/restore,
movement/resize and a changed background. All 32 client sample colors matched
the source-over calculation exactly, within the allowed tolerance of two.
The background is also sampled independently at every stage.

[Native results](platform-validation/wayland-transparency/native/results.json),
protocol logs and screenshots preserve the evidence. The
[metadata](platform-validation/wayland-transparency/metadata.json) links the
unchanged executable to the earlier transparency build: all 159 compiled inputs
match that build, and 164 inputs are preserved in the new source archive.
No Rust source changed in this step; the previous 565-test/29-doctest validation
remains the Rust baseline. Six new Python sampler regression tests also pass
and preserve failed-trial evidence without emitting successful CSV rows.

This covers Sway nested over X11 with pixman and a llvmpipe-rendered client.
Native macOS, hardware GPU composition, compositor backdrop blur and OS-native
blur materials remain unverified.

## Real Wayland input-method composition

An isolated Sway/Fcitx5/Pinyin session exercises actual text-input-v3 preedit and
Unicode commits. The basic two-editor test verifies Escape cancellation,
focus-switch cancellation and independent models. The client runs with DISPLAY
unset; protocol logs show native enable, cursor rectangle requests, preedit and
commits. Candidate popup placement is not asserted. Private D-Bus disables
service activation to avoid unrelated portal processes and mounts. CI installs
the explicit input-method dependencies.

The [initial passing run](platform-validation/wayland-ime/native/result.json)
and [metadata](platform-validation/wayland-ime/metadata.json) preserve 162 inputs
and a freshly rebuilt native fixture. No Rust code changed. Later functional
coverage also checks external model replacement and read-only toggles during
preedit; the consolidated evidence below records the final harness.

A [failed initial burst](platform-validation/wayland-ime/failed-initial/README.md)
is retained: Fcitx sent the full preedit sequence, while the client only received
the first update around a cursor-state transaction. A second
[fresh-composition burst failure](platform-validation/wayland-cancellation/failed-burst/README.md)
is also preserved. Functional smoke tests now await client-visible preedit
between characters. Passing them does not resolve these burst failures or prove
rapid-input reliability. Transaction backlogs and delayed stale serials remain
open compatibility work.

## Wayland model and read-only composition cancellation

The consolidated [native results](platform-validation/wayland-cancellation/metadata.json)
pass three fresh sessions: two-editor composition (eight stages), external model
replacement (six stages), and read-only cancellation/re-enabling (seven stages).
Both cancellation tests observe genuine preedit, keep logical/native focus
stable, reject the old candidate, resume ordinary Backspace/digit input without
switching engines, and accept exactly one fresh Unicode commit. The read-only
case changes editability without replacing the model.

All 163 archived inputs, source archive contents and two rebuilt binary hashes
were independently checked; see [audit](platform-validation/wayland-cancellation/audit.json).
The scripts await actual window mapping/focus and pace composition through
observed preedit. CI includes both cancellation modes. No Rust source change was
needed for these functional cases; the previous 565 ordinary tests and 29
doctests remain the Rust validation baseline, not a newly rerun test count.

The preserved burst failures above remain unresolved. These tests neither inject
delayed server commits nor establish serial-based cancellation guarantees.
Native macOS IME and candidate popup placement also remain unverified.

The [direct winit diagnostic](platform-validation/wayland-ime-burst/README.md)
delivered final preedit and Unicode commit for all 22 tested bursts across fixed,
moving and delayed candidate geometry. It narrows the investigation but does
not reproduce or resolve the original zgui burst failures.

## Refreshed comparison acceptance checks

The [first refresh](results/refined-framework/README.md) stopped after 17 trials
amid disk exhaustion; its failed process had no usable log and the old sampler
lost its raw samples. The sampler now preserves failure samples/status, covered
by six Python regression tests and CI.

The [retry](results/refined-framework-retry/README.md) completed all 36 GUI
processes and preserved 10,793 samples. Independent recomputation verified all
220 archived inputs and measured binary hashes, but the runner correctly rejected
update parity: zgui scrolling repeat 1 delivered 1,099 updates in 20 seconds,
below the 1,160 minimum. Unrelated builds restarted on the shared host during
sampling. The cause is not isolated, and no new performance ranking is accepted.
Fresh seeded captures were reviewed for all three applications. Both attempts
remain separate from the earlier accepted historical comparison.


## Variable-height component virtualization

The [source-frozen variable-list validation](platform-validation/variable-virtual-list/README.md)
adds heterogeneous allocated heights, logarithmic prefix lookup/updates,
incremental append, first-visible anchoring, pixel-based keyboard paging and
resizing focus markers. Eleven component tests cover retained keyed ownership,
duplicate/constructor rollback, reactive notification suppression, empty/shrunk
lists, disposal and explicit scroll requests made by callbacks. Fixed-height
lists receive the same callback request preservation fix. GPU damage/full-redraw
comparison and 13 native stages pass; at most 14 rows are mounted for 100,000
items. Heights are application-supplied, not automatically measured from text.
The recorded core microbenchmark is separate from GUI CPU/RSS comparisons.


Naturally measured virtual rows now share the retained keyed component path.
The [combined measured-list validation](platform-validation/measured-virtual-list/README.md)
passes 598 tests, 31 documentation examples, GPU wrapped-text damage comparisons,
seven measured-list native stages, and the existing explicit/fixed-list native
fixtures. Estimates can be severely inaccurate without unbounded discovery;
newer focus/scroll requests supersede measurement-time keyboard reveal. Unseen
rows retain approximate heights until remounted, and independently sized lists
need separate measurement caches. This does not add hardware-GPU or native macOS
evidence or convert rejected process benchmarks into valid comparisons.

## Wayland burst-input serial diagnosis

A standalone winit/softbuffer test with synchronous 80 ms blocking delivered all
12 final preedit/Unicode commit pairs. Four additional unsynchronized bursts in
the actual zgui editor also delivered final state. Compositor-side traces in
both tests show some intermediate preedits rejected with stale input-method
serials; these successes do not invalidate the earlier persistent failure.

A separate zero-delay condition reproduced the persistent final-preedit failure
in both zgui and the independent program: two warm failures, two cold successes.
In each failure the compositor advanced its input-method serial after the first
preedit, then rejected the final preedit carrying the previous serial. Space
subsequently committed the correct Unicode text with the current serial in all
four observations. This demonstrates the symptom outside zgui and its GPU
renderer on the tested Sway/wlroots/Fcitx stack; it does not establish failure
rates on other input stacks or lost committed text. No timing workaround or
text replay was added to the framework.

Exact sources, binary hashes, client/server/input-method traces and an **unsent**
upstream report are retained in the [zero-delay evidence](platform-validation/wayland-ime-zero-delay/README.md).
The [blocking matrix](platform-validation/wayland-ime-blocking/README.md) and
[original-rate native matrix](platform-validation/wayland-ime-native-burst/README.md)
remain separate evidence. This platform compatibility issue remains open.

## Bounded surface changes and native host recreation

Repeated recoverable Lost/Outdated surface changes now yield to the host retry
schedule instead of terminating the application after the second status.
Acquisition work remains bounded per call; the rendered target stays retained.
Fatal recreation and validation errors still propagate. Fault-injection tests
cover every Lost/Outdated pair, persistent changes and later successful frames.

An isolated native X11 test now invokes actual Host suspension/recreation with
real windows and GPU renderers. It verifies repeated suspension cleanup, native
window release, cleared preedit/capture/retry state, preserved editor node/model,
updates during suspension, restored focus and rendering with a replacement
renderer. This is integrated lifecycle coverage, not physical machine sleep or
real driver fault injection. CI runs it independently and preserves its log.
See the [source-frozen checks and native log](platform-validation/surface-recovery/README.md).

## Private upstream preedit recovery experiment

A private Fcitx5 5.1.19 frontend patch republishes only the current nonempty
preedit after an ordinary input-method serial change, excluding activation and
deactivation transitions. All four zero-delay observations delivered final
preedit and exactly one Unicode commit. In the zgui warm run, the trace still
shows the original final transaction rejected, followed by an accepted refresh
at the new serial: this directly demonstrates recovery rather than mere absence
of the race. The eight-stage Escape/focus-switch composition smoke also passes.

No installed plugin, global service or zgui production code was changed. This
is an upstream patch candidate, not a shipped fix: empty-preedit cancellation,
stale commit/deletion transactions and broader surrounding-text/lifecycle cases
need upstream testing. Exact patch/source/build provenance, private plugin
mapping, raw results and limits are in the [private experiment](platform-validation/wayland-ime-fcitx-private-refresh/README.md).

## Clean blur batching and final integrated checks

Distant streaming damage now uses one batched render pass even when a clean blur
panel exists elsewhere. Dependency-expanded damage touching the filter still
executes both blur passes. Full-repaint differential tests validate repeated
streaming, sampling halos and recursive isolated-layer accounting. The counters
measure encoded work, not elapsed GPU time; repainted isolated layers still
replay their complete targets.

The [integrated snapshot](platform-validation/retained-blur-batching/README.md)
passes all 600 workspace tests, 31 doctests, strict Clippy, formatting and macOS
ARM64 all-target checking. Native effects and isolated host recreation also pass.
All 175 archived source inputs and the tested native binary hashes are verified.
