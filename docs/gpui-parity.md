# GPUI capability acceptance matrix

This is an implementation audit against **GPUI 0.2.2**, the exact dependency in
[the comparison application](../comparisons/gpui/Cargo.toml). It is not a claim
of complete parity. The initial snapshot was inspected on 2026-09-22 before the
current parity expansion. Update a row only with implementation and acceptance
evidence; concurrent work is not counted as completed here.

“Implemented” means a corresponding public zgx capability exists, not identical
API names, rendering pixels, performance or verified behavior on every OS.
“Partial” identifies the working subset. “Missing” means no integrated public
capability was found; application code could sometimes reproduce it manually.
The acceptance cases below make this finite and reviewable. They do not demand
all CSS, all browser behavior, or Zed-specific application widgets.

Reference sources were read from the downloaded `gpui-0.2.2` crate under
`~/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/`. The links below identify
that same version rather than moving `main` sources:

- [Style and text style](https://docs.rs/crate/gpui/0.2.2/source/src/style.rs), [fluent styling](https://docs.rs/crate/gpui/0.2.2/source/src/styled.rs).
- [Interactive elements](https://docs.rs/crate/gpui/0.2.2/source/src/elements/div.rs), [keymap](https://docs.rs/crate/gpui/0.2.2/source/src/keymap.rs).
- [Window painting and input](https://docs.rs/crate/gpui/0.2.2/source/src/window.rs), [application and native integration](https://docs.rs/crate/gpui/0.2.2/source/src/app.rs), [platform options](https://docs.rs/crate/gpui/0.2.2/source/src/platform.rs).
- [Shipped examples](https://docs.rs/crate/gpui/0.2.2/source/examples/), [SVG transformations](https://docs.rs/crate/gpui/0.2.2/source/src/elements/svg.rs), [path builder](https://docs.rs/crate/gpui/0.2.2/source/src/path_builder.rs).

## Components, layout and styles

| Capability / reference | Current zgx status and source | Finite acceptance case |
| --- | --- | --- |
| Components, children, conditional/keyed identity | Implemented: [compose.rs](../crates/zgx/src/compose.rs), providers and lexical slots | Supplied Model/App/Count/Title/Dialog example; unchanged title retained after collection append; child tasks disposed on removal. |
| Row/column flex, grow/shrink, alignment, pixel/percentage sizes | Implemented subset: [style.rs](../crates/zgx/src/style.rs), [scene.rs](../crates/zgx/src/scene.rs) | Nested sidebar/content resize, intrinsic text, constrained grow/shrink, percentage children with padding. |
| Grid with placement and spans; `grid_layout.rs` | Implemented finite grid surface: [layout guide](layout.md), [16 geometry/interaction regressions](../crates/zgx/tests/advanced_layout.rs) and retained Taffy cache disposal test | Port the five-column, five-row header/sidebar/content/ad/footer example, including full-column and multi-row spans; resize and child removal. GPUI's public grid surface is finite, not arbitrary CSS grid syntax. |
| Flex wrapping/reverse, basis, align-self/content; `Style` | Implemented: wrapping/reverse, basis, self/content alignment via retained Taffy; resize and line distribution tests | Wrapping chip row at three widths, reverse row/column, fixed basis with weighted growth, per-child alignment and wrapped-line distribution. |
| Separate axis gaps, aspect ratio, edge insets; `Style` | Implemented: per-axis gaps, aspect ratio and typed edge insets; pin/aspect/gap regressions | Absolute panel pinned right/bottom while parent resizes; square aspect ratio with one definite axis; distinct row/column gaps. |
| Percentage min/max, spacing, auto margins; `Style` lengths | Implemented: percentage min/max and spacing, auto margins; indefinite percentage basis resolves to zero; padding consumers share resolved layout values | Percentage padding and bounded percentage child; centered auto-margin item; intrinsic/indefinite parent has a documented noncyclic resolution. |
| Display/visibility/overflow policies | Implemented finite policies: retained hidden/invisible, focus cleanup, per-axis overflow, shared paint/hit/visibility clipping tests | Hide without removing owned state; distinguish removal from invisible layout participation; hidden content excluded from input/accessibility; independent horizontal/vertical overflow. |
| Border edges/styles, corner radii, multiple shadows; `Style` | Implemented: [detailed paint styles](paint-styles.md), asymmetric corners/edge widths, dashed borders and bounded multiple shadows; GPU/software full-versus-partial repaint tests | Asymmetric rounded card with independent edge widths, dashed separator, two shadows; differential repaint against fresh frame. |
| Hover/active/focus/disabled styling | Implemented: [compose.rs](../crates/zgx/src/compose.rs), [compose_style.rs](../crates/zgx/src/compose_style.rs) | Nested interaction and disabling during held activation preserve event and visual state. |
| User-selected cursor; `Style.mouse_cursor` | Implemented: fluent inherited/reactive `.cursor`, native host mapping, disabled-region and no-layout/no-damage tests; native X11 cursor image verification. Mac cursor shape remains unverified: the automation screenshot's pointer indicator is not the native cursor | Drag handle uses move cursor, disabled region uses appropriate cursor, changes restore on exit. |

## Painting, text and media

| Capability / reference | Current zgx status and source | Finite acceptance case |
| --- | --- | --- |
| Transparency, rounded fills, blur, edge fades, retained layers | Implemented: [GPU renderer](../crates/zgx-gpu/src/lib.rs), [style.rs](../crates/zgx/src/style.rs) | Existing effects example; partial/full pixel equivalence and clean-subtree reuse counters. |
| Linear-gradient and slash-pattern backgrounds; `gradient.rs`, `pattern.rs`, `Fill` | Implemented: retained gradient/slash [background brushes](paint-styles.md), cache reuse, opacity/clipping and native interactive gallery | Resize gradient card and patterned fill; opacity and clipping compose; idle frames do not regenerate unchanged assets. |
| Canvas paths, fills/strokes, curves; `painting.rs`, `PathBuilder`, `Window::paint_path` | Implemented: [component canvas](canvas.md), lines/quadratic/cubic/arcs, fill/stroke/clip and bounded device-scale cached rasterization; CPU/GPU differential tests | Port a line/quadratic/cubic/arc path drawing, filled polygon and stroke; mutation, bounds, clipping and partial repaint. |
| SVG/raster images and fitting | Implemented finite case: bounded PNG/JPEG, image fit and retained `svg`/`svg_signal` components with tint, device-scale resize and cached rasterization; software/GPU repaint regressions | Port image gallery with contain/cover/clipping; component SVG path/tint/resize support without mandatory application raster-management code. SVG text/external resources remain deliberately restricted. |
| Rotation/scale of SVG and paths | Implemented: [affine image/SVG transforms](affine-images.md) with inverse hit-testing and damage bounds; transformed paths and software/GPU clipping/layer regressions; native rotation fixture | Rotate/scale SVG around its center as `examples/svg/svg.rs` does; transformed bounds restore old pixels. **GPUI's SVG transform does not itself establish a requirement for CSS transforms on every div.** |
| Animated GIF; `gif_viewer.rs`, `img.rs` | Implemented: [GIF decoding and retained animation](images-and-animation.md), disposal/timing/loop bounds, visibility/presentation pause and owned cancellation. Decoder regression covers offsets, Keep/Background/Previous disposal, timing and loop metadata. A current-source native three-frame GIF subsequently verified file decode, presentation, pause/resume and normal close; complex disposal, loop limits and invisible submission behavior remain separate test claims | GIF frame disposal, timing, loop count, retained decoding; pause when invisible and cancel deadlines on removal. |
| Async image loading/cache; `image_loading.rs`, `image_cache.rs` | Implemented: [async image component and bounded deduplicating cache](images-and-animation.md), loading/error/success states, retry, final-request cancellation and stale-generation regressions | Image view with loading/error/result states; cancellation on unmount and bounded deduplicated cache. Network transport may remain application supplied. |
| Rich styled text runs/highlights; `elements/text.rs`, `TextStyle`, `HighlightStyle` | Implemented display runs, highlights/decorations and keyboard-accessible shaped inline links: [rich text](rich-text.md), [native evidence](platform-validation/rich-text/README.md), core9/desktop4/GPU3 regressions. No rich-text editor or wavy underline claim | Mixed-weight/color/size runs, Unicode wrap and decorations; retained restyling and shaped measurement match rendered text. This does not require a complete word processor. |
| Text alignment, overflow/ellipsis, line clamp; `styled.rs` | Implemented Start/Left/Center/Right alignment, grapheme-safe ellipsis and visual line clamps; full semantic text retained and hidden link regions removed. Native19 geometry tests plus GPU/software restoration and two-line native capture | Center/right multiline label, one-line ellipsis, three-line clamp, width resize and Unicode boundaries. |
| Font features, explicit fallbacks, custom font loading; `text_system.rs` | Implemented inherited canonical OpenType features, ordered explicit grapheme fallback and validated bundled fonts before window shaping. Real ligature/font-ID/custom-database tests; headless metrics remain approximate; live font replacement is not exposed | Load bundled font; select fallback list and OpenType feature; measured advance and paint agree. |
| Editable Unicode text, clipboard, selection, IME | Implemented: [text_edit.rs](../crates/zgx/src/text_edit.rs), [widgets.rs](../crates/zgx/src/widgets.rs), native host. [Mac observations](platform-validation/macos-continuation/README.md) cover bidirectional Unicode TextEdit clipboard, cut/undo/redo, multiline input, selection and resize. Native Canadian Option-E then E produces marked text and one `é` commit. Focus transfer exposed stale dead-key state; AppKit input-context discard now leaves plain `e` in the second editor and permits fresh `é` composition. **Pinyin candidate commit/cancel/focus/placement remain pending input-source permission**. Dead-key Escape commits a literal accent and is not Pinyin cancellation evidence | Existing input/text-area fixtures including bidi/wrapping/selection and model updates during preedit; Linux XIM evidence uses explicit asynchronous IBus mode. Retain the default synchronous-bridge limitation below and the documented external Wayland serial compatibility issue. |
| Native video/image-buffer surface; `elements/surface.rs` | Implemented macOS-only retained CoreVideo→Metal BGRA/NV12 imports and GPU conversion: [surface contract](native-surfaces.md). [Real Apple M5 Max validation](platform-validation/macos-continuation/README.md) passes imported BGRA/NV12 pixel, reuse, teardown ownership and retained component tests plus a four-frame native window probe | Present changing owned image buffers on Mac, with object fit, lifetime safety and no forced CPU readback. Ordinary RGBA image replacement is useful but does not establish this integration. |

## Input and application integration

| Capability / reference | Current zgx status and source | Finite acceptance case |
| --- | --- | --- |
| Pointer dispatch/capture, focus, keyboard activation | Implemented: [input.rs](../crates/zgx/src/input.rs) | Existing modifier/chord/capture/focus-scope tests and native fixtures. |
| Actions, contextual keymaps, key sequences; `KeyBinding`, `key_context`, `on_action` | Implemented finite acceptance: [typed actions, predicates and timed replay](actions.md), contextual binding/fallback and cancellation tests; independent configuration/precedence contract | Two contexts bind the same chord to different typed actions; two-step chord timeout/cancel; focused handler and application fallback; no accidental text insertion. |
| Function and additional named keys | Implemented F1–F35/Insert mapping and public `Key::Function`; other named keys remain outside the current vocabulary. Mac F1/F2 sequence, F5 action and distinct F12 delivery observed; Insert synthesis is unsupported by the available Mac automation and remains unverified | Native F1/F5/F12 and Insert reach handlers distinctly; unrecognized keys do not silently collide with text. |
| Typed internal drag/drop, preview, acceptance; `drag_drop.rs`, `div.rs` | Implemented finite acceptance: [typed drag/drop guide](drag-drop.md), [8 core regressions](../crates/zgx/tests/drag_drop.rs), [native X11 pointer/preview/Escape evidence](platform-validation/drag-drop/result.json). Mac accepts all three card payloads and rejects outside drops; held preview/Escape cancellation remains unverified | Port three draggable colored cards and drop target; typed payload, drag preview, cancellation, outside-drop and removed target cleanup. |
| External file drag/drop; GPUI `ExternalPaths` | Implemented X11/macOS dispatch-batch grouping and per-path compatibility; [real X11 GTK two-file/escape fixture](platform-validation/file-drop/result.json). Native Wayland URI-list transport has bounded reads and UI-acknowledged completion; [real two-file, leave-cancel, and rejected-region evidence](platform-validation/wayland-file-drop/README.md). Wayland Escape depends on source/compositor policy; macOS runtime remains unverified. X11/macOS dispatch batches are not winit transaction IDs. | Drop two files onto a region, receive complete paths only after accepted drop; cancel and leave clear hover state. |
| Fixed and measured virtual lists | Implemented: [compose_variable.rs](../crates/zgx/src/compose_variable.rs), [compose.rs](../crates/zgx/src/compose.rs) | Existing 100k/million-row fixtures, streaming measured row, keyboard reveal and bounded mount counts. |
| Multiwindow lifecycle/basic controls | Implemented public controls; [covered Wayland FIFO stall fixed with compositor frame pacing](platform-validation/wayland-frame-pacing/README.md), preserving timer/model progress and damage for uncover. Mac component lifecycle, close veto/exactly-once cleanup and independent windows pass; resize/maximize/restore observed. Later direct AppKit window queries verify minimized/restored/hidden/shown visibility, miniaturization and key-window state, with AX recovery after show; these are native observations, not request-state logs | Existing close veto, multiple windows, resize/minimize/maximize/visibility/focus and async wake fixtures. |
| Bounds/display choice, fullscreen, custom titlebars/window movement; `WindowOptions`, `Window` | Implemented public bounds/display/fullscreen/minimum-size/drag controls and reactive WindowInfo; [actual X11/Wayland validation](platform-validation/native-services/README.md). Wayland placement explicitly unavailable. Mac 2× bounds, fullscreen/restoration and minimum size pass after a borderless maximize-query fix. Custom-titlebar movement remains unverified: automation also failed to move a standard AppKit titlebar, with native pressed-button state zero; a true held gesture is required. Actual same-scale physical monitor movement/return now passes with settled native monitor/bounds readback. Mixed-scale transitions and hotplug remain pending | Open at chosen bounds/display, toggle fullscreen, drag custom titlebar, respect minimum size and restored bounds. Host policy may decline placement; report capabilities honestly on Wayland. |
| Native menus, file dialogs, prompts; `set_menus`, `prompt_for_paths`, `prompt_for_new_path`, `Window::prompt` | Implemented owned Linux portals/rendered menus and macOS AppKit APIs. [Linux services](platform-validation/native-services/README.md), [dynamic menus](platform-validation/dynamic-menus/README.md), [Mac observations](platform-validation/macos-continuation/README.md). Mac open/folder/save/cancel, two-file selection, prompt and nested-sheet owner close pass after fixes; menu labels, enable/disable, accelerators and replacement observed. Direct AppKit NSMenuItem reads also verify checked/unchecked and enabled/disabled transitions; checkmark pixels were not captured. Standard Mac application-role menu precedes command menus | Native menu action/checked/enabled state and shortcut, open/save cancellation/multiple selection, native prompt result; platform-specific checks. |
| URL launching, reopen/open-URL application callbacks; `App`/`Application` | Implemented async OS URL launcher, owned Linux org.freedesktop.Application service and macOS AppleEvent handlers preserving winit's delegate. [Linux handler/reopen evidence](platform-validation/native-services/README.md); [Mac reopen and OS URL launch/delivery](platform-validation/macos-continuation/README.md) pass after integration fixes and test-bundle URL registration. Receiver UI displayed the exact delivered URL; earlier registration failures retained. Bundle/desktop URL registration remains application packaging | Launch a user-triggered URL with result reporting; route an external open-URL/reopen event without requiring app-specific event-loop replacement. |
| Accessibility | Implemented: [semantics.rs](../crates/zgx/src/semantics.rs), [accessibility.rs](../crates/zgx-desktop/src/accessibility.rs). [Native Mac AX observations](platform-validation/macos-continuation/README.md) pass label/text tree, button, editor value/selection, slider, checkbox and disabled-edit rejection; rich link action and full unclipped text exposed | Existing native AT-SPI evidence plus real macOS AX tree/actions/selection. This does not establish a complete VoiceOver usability audit. |
| Animation/scheduling | Implemented primitives: [timer.rs](../crates/zgx/src/timer.rs), tasks and reactive styles | Existing effects animation, monotonic progression, cancellation on removal, no animation deadline for idle views. |

## Completion gate

1. Port the finite cases above through idiomatic component/children APIs; public
   paint primitives must not force ordinary applications back to scene append.
2. Test behavior and damage/layout invariants for each new feature. Preserve
   cached-layout and unchanged-subtree behavior, bounded resources and disposal.
3. Run an integrated parity gallery on Linux and macOS, including native menus,
   file dialogs, input and rendering. Cross-compilation is not runtime evidence.
4. Re-run the matched GPUI/QuickGUI streaming/list comparison after implementation
   stabilizes. The existing workload measures those cases, not universal speed.
5. Publish each implemented/partial/missing status with evidence. An application
   workaround, test count or passing example cannot by itself establish parity
   for untested API categories.

The current finite-gate audit is:

| Gate | Evidence and remaining qualification |
| --- | --- |
| 1: Public component/children cases | The implementation audit and retained fixtures cover the finite API surface. No new application-level scene workaround is introduced by this Mac continuation. Unverified platform behavior remains identified in the rows above. |
| 2: Invariants and bounded ownership | Existing Linux evidence and actual Mac Metal/CoreVideo suites pass at recorded sources. The latest full CI outcome remains required; native sleep/wake, driver loss, mixed-scale display transitions and hotplug are separately unverified; same-scale physical monitor movement/return is observed, not substituted by synthetic suspension tests. |
| 3: Integrated native gallery | Linux and Mac public/native fixture suites provide integration coverage across multiple executables, not a claimed all-in-one gallery launch. Mac coverage remains partial while Pinyin, native app deactivation, held input/cursor/file-drop cases are pending. Completing those cases is the remaining integration work; no new monolithic application is required by this audit. |
| 4: Matched comparison after stabilization | The 36-trial Linux comparison passed after the parity expansion and remains valid for its frozen source. A matched rerun on the final stabilized source is still pending: most continuation changes are Mac-only or tests, but shared RGBA loops also changed from `chunks_exact` to equivalent typed chunks. Equivalence is not a new measurement. Mac zgx-only resource probes do not replace the GPUI/QuickGUI matched gate or establish rankings. |
| 5: Status publication | Current statuses, failures and source/binary identities are recorded. Pending native cases and CI outcomes prevent a completion claim; future evidence must retain its actual source identity. |

Highest-priority implementation groups are layout/grid/wrap, path/gradient/media,
text presentation, keyboard actions/drag/drop, and native window integration.
The independent macOS image-buffer surface deserves an explicit platform test;
it must not be silently conflated with PNG support. Zed's docking workspace,
terminal emulator, syntax engine and full editor are application features and
are not added to this framework-parity scope merely because Zed uses GPUI.

## Integrated evidence

The audited public capability implementations have the [integrated Linux test/native evidence](platform-validation/gpui-parity-integration/README.md) and [matched 36-trial Linux performance comparison](results/gpui-parity-workers4/README.md). The default-path and cache-lifetime audit is recorded in [performance-feature-audit.md](performance-feature-audit.md). The [Mac continuation packet](platform-validation/macos-continuation/README.md) adds real Metal/CoreVideo, window lifecycle, AppKit, clipboard, dead-key commit/focus-reset and accessibility observations, plus Retina grid/wrap, rich links, path/gradient/SVG painting and real decoded-GIF playback/pause/resume. The simple native GIF does not prove complex disposal or presentation cadence; request logs do not prove every native window state. Candidate-based Pinyin IME and the explicitly pending rows above prevent a complete macOS qualification claim. Linux software-renderer comparison results remain separate from Mac hardware correctness; no matched Mac performance result is claimed.

Linux X11/XIM has a recorded IBus 1.5.29 default synchronous-bridge
interoperability limit: post-commit preedit and Escape lifecycle signals are
omitted before zgx receives them. The [diagnostic packet](platform-validation/macos-continuation/zgui-linux-native-ime-followup.tar.gz)
preserves the failure, raw boundary traces and rejected private winit experiment.
Original, unpatched zgx/winit passes Pinyin commit/cancel/editor transfer and
candidate placement with task-owned `IBUS_ENABLE_SYNC_MODE=0`, plus external-model/
read-only cancellation and ordinary key recovery. The [official IBus fix](https://github.com/ibus/ibus/commit/719792d300579c1bfdf43251a83c6ed4e5594c07)
addresses missing show/hide preedit handling. The smoke harness now selects and
records asynchronous mode; this does not fix or qualify the affected default
synchronous configuration. No winit production patch was adopted.
