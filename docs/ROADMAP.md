# Framework completion plan

The active goal is practical GPUI UI capability parity on Linux and macOS. The finite [capability acceptance matrix](gpui-parity.md) supersedes the historical core-framework milestones below. Completion requires working public APIs and integrated examples, behavior tests, documentation, and truthful platform/performance evidence. Merely adding metadata or documenting missing behavior does not complete a feature.

Primary platforms: **Linux and macOS**, per user preference. Windows remains a portability target, not the first runtime-validation gate.

## Core and layout

- [x] Reactive signals, batched updates, stable collection rows, scoped services.
- [x] Owned views and deterministic subscription/subtree disposal.
- [x] Retained generational scene, cached layout and separate dirty phases.
- [x] Constraint-aware layout, flex sizing, alignment, margins, accurate text measurement.
- [x] Keyed child reconciliation, dynamic conditional/list views and reusable components.
- [x] Event dispatch, focus navigation, pointer capture and keyboard activation.
- [x] Unicode text editing, selection, clipboard, IME composition, bounded undo/redo.
- [ ] Known issue, for 0.1.6: a fixed-size row or column item held at its automatic minimum while its parent shrinks it (an 80-wide shrinkable column whose 80-wide child keeps it from narrowing, in a row too small for it and an 80-wide sibling) keeps its width when its content's least size drops (the child made narrower, clipped or absolute). A fresh layout makes it narrower, and the boxes after it move up. The item is a layout boundary (`is_layout_boundary`), so the change is laid out at the item alone, under its old room. Parked: every form found so far fixes it but costs on hot layout paths, against fixes' no-loss rule. The best is on branch `layout-boundary-min` (draft #904, tests in `crates/zgx/tests/layout_history.rs`, which fail on main). Forms measured (x86 CI, worst cells):
  - recording rows that shrink and laying out from them (`layout/boundary-minimum`): 0.1% to 0.5% in app scenes;
  - a bit set where the flex loop floors an item, read by `is_layout_boundary`: app-dashboard-sort +0.15%, micro-state +0.11% (parent relayouts that change nothing);
  - held items in a map, checked when laid out alone: micro-state +0.23% (`automatic_minimum` outlined by its second caller), then +0.14% and +0.12% with the map write out of line and leaves skipped;
  - only a check when laid out alone, from the siblings' sizes: wrong (rows full by `grow` read as shrunk: widgets +228%);
  - a bit on a row or column that shrank its children (one comparison per flex loop) plus that check: script-update +0.16%, vscroll-fixed +0.14%, widgets +0.12% (arm64 gate at most +0.06%; the form on the branch).

  Any instruction per measure or per boundary laid out alone shows as +0.03% to +0.15%, and a test in the invalidation walk changes `set_box`'s inlining (+1.4 to 3.4 M on heavy). Taffy's cache can't hold the answer: zgx lays out rows and columns itself (Taffy only takes `layout_options` containers).

## Rendering and platform

- [x] GPU surface renderer as the primary desktop path, with bounded resources.
- [x] Shaped text and glyph caching shared by measurement and rendering.
- [x] GPU transparency, blur, edge fades, damage updates and retained layer composition.
- [x] Rounded shapes, borders, shadows, images and scalable icons.
- [x] Reusable application/window host, resizing, DPI changes, lifecycle and errors.
- [x] Accessibility tree, roles, states and native accessibility actions.
- [x] Native clipboard, IME positioning, cursor icons, window transparency.
- [x] Cross-platform build coverage and clear runtime-validation matrix (Linux native checks plus actual macOS Metal/CoreVideo, window, clipboard and AX evidence; remaining Mac native checks are explicit below).

## Widgets and application development

- [x] Theme/style API and layout containers.
- [x] Labels, buttons, toggles/checkboxes, sliders and progress indicators.
- [x] Text inputs and multi-line editing.
- [x] Scroll views, keyed virtual lists, menus, popovers/dialogs and focus containment.
- [x] Runnable widget gallery and realistic stateful application example.
- [x] Public API documentation, lifecycle/async guides and future template-lowering examples.

## Luau app ergonomics, against React

[The team example](../crates/zgx-script/examples/team) (a list loaded from an API, a form checked as it is typed in a modal, a removal confirmed) was written to find what building a real app in Luau lacks. Fixed with it: `ui.resource` (data with loading, error and status states, loads keyed by the state they read, a stale answer dropped), a modal's or popover's `build` (its content new each time it opens, as React's conditionally rendered dialog), `ui.on_mount` outside components, `TestApp::shown`, `Value::record` (a map in any order; `Value::Map` must be sorted), option lists typed as plain lists, a fetcher in `zgx-run` (a networked Luau app needs no Rust host for its transport), and `ui.form` with controls' `field` (checks as typed, Enter submitting, the first wrong field focused, errors heard by assistive technology as `aria-invalid`).

[The settings example](../crates/zgx-script/examples/settings) (a profile loaded from an API, a form saved with an Undo toast, a switch saving at once and going back if refused) was written the way a React developer would, to find what still costs more. Fixed with it: `ui.form { values }` (a form filled from what is loaded and following it, an edited field keeping what was typed), `dirty` on a form and its fields, the form handed to `submit` and `on_success` (no forward declaration typed `Form?`), controlled checkboxes, sliders, steppers, segmented controls and selects (`value = function` with `on_change`), `reset` and `response.json` typed so a record fits without a cast, and `TestApp::fill`. Still awkward:

- An optimistic update is written out (a snapshot, the write, a task, `ui.try`, the revert and its toast), where React has `useOptimistic`.
- A handler can't await: a save in an `on_click` or a toast's Undo is wrapped in `ui.task`. `ui.try` gives a task's error typed as text (Luau types `pcall`'s as the function's result). Tasks cancel (`ui.task` returns a handle; resources and forms cancel their own), and a cancelled fetch stops its request (an action's host work goes on, its answer dropped).

## Quality and performance

- [x] Damage differential tests against full repaint.
- [x] Matched streaming/list applications using actual GPUI and QuickGUI.
- [x] Reproducible process CPU/RSS harness and raw smoke measurements.
- [x] GPU correctness/readback tests and bounded cache/resource tests.
- [x] Input/widget integration tests, accessibility/IME behavior tests.
- [x] Resizing/DPI/surface-loss/suspension regression checks (native DPI, fault-injected acquisition, and actual host suspension/recreation).
- [x] Updated comparison measurements on equivalent renderer configurations (36-trial four-worker Mesa series, raw samples/source archive/hashes audited; matched hardware-GPU comparison remains unverified).
- [x] CI configuration for core, desktop and supported platform builds, with preserved failure evidence. Incomplete Mac native interaction checks and the diagnosed external input-stack issue remain qualification limits below.

Platform-specific behavior is not considered validated merely because it compiles. Native Mac process CPU/RSS/physical-footprint smoke measurements now exist; matched hardware-GPU comparisons, presentation cadence and energy measurements remain unverified. The plan is updated as implementation and evidence arrive.

## Breaking changes for 0.2

0.1.x stays additive (cargo-semver-checks clean). Changes that need a break wait
here, to land together:

- [ ] Named keys outside `Key`'s vocabulary. The desktop host drops every
  winit `NamedKey` outside Tab, Enter, Space, Escape, the editing and
  navigation keys, Insert, ContextMenu and F1–F35: media (play/pause, next,
  previous, stop, volume up, down and mute), input-method keys (Kana, Eisu,
  Convert/Henkan, NonConvert/Muhenkan, Hiragana, Katakana, Zenkaku/Hankaku,
  Hangul, Hanja), PrintScreen, ScrollLock, Pause, NumLock, CapsLock, Help,
  Clear, the Select, Find, Undo, Copy and Paste keys, Browser and Launch keys,
  brightness and power. They reach no handler or binding. `Key` and
  `InputEvent` are exhaustive, so a variant for them is a major change.
  Design: `Key::Named(NamedKey)`, with `NamedKey` a `#[non_exhaustive]` enum of
  those keys (and the keypad's identities, `NumpadEnter` and the others).
  `Key` and `InputEvent` become `#[non_exhaustive]`, so later keys and events
  are additive. Every backend maps them: winit's `NamedKey` on macOS, Windows
  and Linux, and the Wayland and X11 paths' own. Keymaps,
  `ui.command { keys = }` and the Luau key names take their names (`MediaPlayPause`,
  `Kana`), and so do the docs and lint. Each backend has a mapping test. Until
  then, a key's physical code binds it where the layout gives it a `Key`
  (`KeyBinding::code`, added in 0.1.6).
- [ ] zgx-3d's `Material` and `Object` become `#[non_exhaustive]`, made by
  builders (`Material::rgb(..).with(..)`, `Object::mesh(..).at(..)` already
  are), so later fields are additive. They're public structs of public fields
  today (`Material` is `Copy`), so a field is a major change. Then a mesh's
  texture moves onto its `Material`: in 0.1.x textures come in as the
  world's, per object, so nothing breaks.

## Remaining validation and refinement

Completed checkboxes describe implemented, tested capabilities, not complete platform coverage or a stable API release. The following work remains explicit:

- Shared GPU/font resources and demand-sized glyph storage are implemented. The latest parity-expansion 36-trial comparison reports zgx mean RSS medians of 103.35–108.39 MiB on llvmpipe, with lower active CPU medians than both pinned references in this condition. Scrolling RSS is nearly tied with GPUI. These shared-host software-renderer results do not establish universal resource minima or hardware-GPU rankings. See [the full report](results/gpui-parity-workers4/README.md).
- Surface acquisition recovery and desktop retry/focus policies have deterministic regression tests. Live X11 and Wayland DPI transitions now pass; real driver surface recreation remains unverified. Winit 0.30.13 emits formal suspension callbacks on Android/iOS/Web, not Linux/macOS; callback teardown/recreation is defensive portability coverage. Machine sleep/wake is a separate desktop scenario and remains unverified.
- Real macOS 26.4 / Apple M5 Max execution now passes the complete Metal suite, including imported CoreVideo BGRA/NV12 pixels, reuse and teardown, plus component/multiwindow probes. Native AX tree/actions/selection, Unicode clipboard exchange with TextEdit, AppKit dialogs and substantial menu/layout/painting behavior were observed. See [the Mac continuation packet](platform-validation/macos-continuation/README.md) for source-specific checks and preserved failures.
- Native Canadian dead-key composition now records marked `´` followed by exactly one `é` commit. A focus-transfer defect initially delivered stale `é` to the second editor; discarding AppKit's input-context state now yields plain `e`, followed by successful fresh `é` composition. Escape under this source commits a literal accent, so it does not validate candidate-based cancellation. Complete Pinyin commit/cancel/focus/candidate placement after explicit input-source permission is resolved.
- Cursor shape, Insert delivery, held drag/Escape, a Finder-style two-file drop and a custom title bar's drag request now pass ([native checks probe](validation.md#macos-cursors-insert-held-drags-file-drops-and-title-bars)); its first run found file drops going to the pointer's last place, fixed. The window server's move after a title-bar press follows the real pointer and is not probed. Native two-file dialog selection/cancel now passes. Direct NSMenuItem reads verify checked/unchecked and enabled/disabled transitions, alongside native menu actions/replacement; checkmark pixels were not captured. Direct AppKit queries verify minimized/restored/hidden/shown visibility and key-window state; AX recovers after show. Native reopen and OS URL launch/delivery pass with a registered test receiver bundle. No unavailable check is counted as passed.
- GPU-default matched measurements, raw logs, renderer configuration and source archive are recorded in `docs/results`. The [Mac packet](platform-validation/macos-continuation/README.md) adds native release process CPU, RSS and physical-footprint smoke measurements. Repeat matched workloads on real target GPUs with longer trials and independent presentation/latency/energy evidence; no matched Mac ranking or measured presentation/energy result is claimed.
- Run the matched GPUI/QuickGUI/zgx comparison on the final stabilized source. The historical post-parity Linux series remains valid, but later shared RGBA iteration changes, although equivalent, have not received a final-source matched measurement. Native decoded-GIF playback/pause integration now has current-source evidence; complex disposal/loop-limit decoder tests remain separate from that simple native GIF observation.
- Historical pre-parity integrated checks passed: 600 workspace tests, 31 doctests, strict Clippy, formatting, macOS ARM64 all-target checking, native effects and host recreation. Source and binary evidence is frozen in `docs/platform-validation/retained-blur-batching`.

The parity expansion now adds rich styled labels and inline links, affine images/SVGs/paths, grid and extended flex layout, canvas/gradient/decorative painting, animated and async images, typed drag/drop, contextual actions, native menus/dialogs and macOS CoreVideo surfaces. The capability matrix records corresponding tests and the remaining Mac runtime checks. A full rich-text document editor and template parser remain separate application/language work; the public component/children API supports later lowering. CSS compatibility and a stable binary ABI are not objectives.

## Component and style architecture (user refinement)

The progress notes in this section retain the validation limits at the time of
each implementation. Their historical references to pending Mac execution are
superseded only by the specific observations in the current capability matrix
and Mac continuation packet; untested categories remain open.

- [x] Declarative view/component children API, with scene mutation kept inside mounting.
- [x] Typed providers, instance-local state, lexical slots and component-owned async work.
- [x] Keyed/conditional mounted children retain identity and dispose replaced resources.
- [x] Fluent GPUI/QuickGUI-inspired sparse styles, interaction variants and inherited typography.
- [x] Re-express the supplied Model/App/Count/Title/Dialog example using this API and exercise real native buttons.
- [x] Test lexical slot resolution, child-state preservation, targeted updates, style rendering and lifetime cleanup.

The fluent typography surface includes inherited color, size, wrapping, font family, weight and italic slant. Font-aware native measurement and GPU rendering, cache invalidation, and software styled-font pixels are covered by tests. Rich spans, decorations, links, font features/fallbacks, custom font setup, alignment and display truncation are now implemented; see [rich text](rich-text.md). Letter spacing, custom line height and percentage width/height are also implemented and validated. Current support is documented precisely in `docs/styling.md`.

Declarative `text_input` and `text_area` now reuse native editing, IME, clipboard and accessibility through retained component ownership. Typography, caret/hit testing, padding and allocated size stay synchronized; geometry checks skip unchanged layout revisions. Native form and GPU differential tests cover this path. Text areas support bounded wheel scrolling and opt-in soft wrapping through `.text_wrap(true)`.

Public component virtualization now preserves keyed row ownership and reactive row indices while mounting only the visible range plus overscan. Allocated viewport dimensions drive range updates without application scene writes; padded clipping, resize observer teardown, extreme arithmetic and failed row construction have regression coverage. The new component workload matches visible comparison geometry and has native correctness smoke evidence. The current normalized release comparison covers the public component path: 36 successful processes, 1,188–1,199 active updates per twenty requested seconds, audited raw samples/source archives/binary hashes. It has lower sampled active CPU and RSS than both references on llvmpipe, including the current backend initialization policy. See [current results](results/measured-framework-workers4/README.md); earlier component and backend-ablation series remain separate. Hardware GPU and native macOS validation remain pending. Historical direct-scene measurements stay separate.

Dynamic window title, logical size, visibility, minimization/maximization and focus requests preserve requested state across pending creation and suspension. Actual X11 window-manager changes are tested; Wayland limitations are explicit. Allocation feedback is bounded and reportable through fallible frame/input APIs, including recovery tests. GPU initialization now avoids unused secondary backend stacks by default; a controlled llvmpipe idle ablation measured roughly 22 MiB and 49 threads saved. Native macOS and hardware GPU evidence remain outstanding.

Hidden/minimized/occluded/zero-size windows now retain pending damage without layout or GPU submission while tasks continue. Native X11 show/restore captures verify latest model state. Stale dirty generations compact amortized against arena capacity, including a 100,000-replacement unflushed stress test. OS suspension and real-driver surface loss remain separate validation gates.

Keyed updates now stage construction before deleting old children in both declarative components and low-level scopes. Late constructor failures preserve the previous mounted set and release partially constructed resources; retry and retained-state regressions cover both APIs.

Declarative sliders now expose the existing control behavior through owned fluent views, including inherited thumb color, allocation-aware geometry, pointer capture, keyboard and accessibility actions. Component, native X11 and GPU differential tests cover resize and updates; legacy slider/progress allocation tracking is also corrected. Native macOS and hardware GPU validation remain open.

Declarative progress indicators support inherited fill color, fluent track/layout styles and read-only numeric accessibility. Progress and slider value-only changes now use clipped fill translation without relayout; resizing remains allocation-driven. Core layout counters, GPU pixel coverage/differential tests and native X11 checks validate this path.

Decoded images now have static/reactive component constructors with fluent decorated roots, accessible descriptions, intrinsic sizing and allocated-content stretching. Source replacements retain nodes; equal-dimension replacements avoid layout. Component ownership tests, native X11 pixel checks and GPU damage comparisons cover this path. Decoding/I/O stays outside component construction; centered aspect-fit/crop modes are now exposed through fluent `object_fit`, with retained geometry and upload-reuse checks.

Ordinary scrollable content now uses declarative children, automatic extent measurement and retained translation. Nested wheel routing, resize/content shrink clamping, native accessibility page actions and padded clipping have component/GPU/native X11 coverage. Automatic reveal of newly focused ordinary-scroll descendants is now implemented; horizontal scrolling is now implemented through `scroll_x`.

Focus transitions now reveal ordinary scroll descendants from inner to outer viewports, including batched reactive updates and redirected focus. Tests cover oversize controls and preserve manual scroll position until another focus transition; native X11 Tab/Shift-Tab and GPU/AccessKit evidence cover the integrated path. This does not materialize offscreen virtual-list rows for keyboard navigation.

Horizontal ordinary scrollers now compose children as rows with intrinsic width, viewport height, axis-specific wheel routing and shared focus reveal. Native accessibility page actions follow explicit scroll-axis metadata. Mixed-axis nested, GPU clipping and native X11 wheel/focus evidence cover the public path; optional overlay scrollbars are now provided.

Opt-in `.scrollbar(true)` adds retained overlay controls to ordinary and virtual scrollers: pointer capture, track paging, keyboard navigation, accessible numeric values/actions and visible focus. Bars disappear from hit testing/focus/accessibility without overflow and leave content clicks intact. Value changes translate the thumb without relayout. Core/GPU/native X11 tests cover the integrated behavior; offscreen virtual-list keyboard navigation is now available through an opt-in mode.

Opt-in `.keyboard_navigation(true)` focuses virtual row wrappers across unmounted ranges with bounded mounting, visible cues and accessible set metadata. Retained-key reorder preserves focus; offscreen key movement/removal falls back to the viewport without scanning the entire collection. Child editors/buttons keep their keys. Native million-row input evidence, GPU pixels and core/accessibility regressions pass.

Targeted independent-device stress reproduced a Vulkan loader crash matching upstream Vulkan-Loader #1863; the same binary passed three 240-lifecycle runs with loader 1.4.345 containing the fix, without disabling debug/validation flags. Evidence is retained in `docs/platform-validation/gpu-lifecycle-stress`. Ordinary fixtures bound resource overlap and preserve four-device concurrency coverage. The earlier stackless failure cannot be proven identical; hardware and native macOS validation remain open.


Declarative modal/popover panels now compose styled children and lexical slots through
owned portals. Absolute/relative fluent styles preserve normal-flow sizing; retained
world-bounds observation follows anchor translation without layout. Nested and sibling
focus scopes, reentrant removal, disabled logical ancestry, replacement and virtual-row
cleanup are covered by tests. Native X11, AccessKit and GPU readback validate the public
path. Popovers trap focus; declarative action menus now provide menu-specific roles
and keyboard navigation through the same owned portal infrastructure.


Declarative action menus now support fluent items, component/keyed ownership,
trigger expanded/popup semantics, disabled-item skipping, wrapped arrow navigation,
Home/End, bounded Unicode prefix search, Escape and Tab exit, and activation that
closes the enclosing menu chain before invoking application code. Long menus size
intrinsically up to the window cap, then scroll and reveal focused items with
retained translation. Reactive spacing and dynamic content shrink have regressions;
GPU readback and native X11/AccessKit checks validate the integrated public API.
Dedicated submenu triggers, side placement and Right/Left navigation are now
implemented through `submenu`, while ordinary action items close their menu chain
before callbacks.


Cascading submenu triggers now fill their menu row and display a decorative chevron.
Keyboard/pointer opening, Left/Escape restoration, sibling replacement, disabled
ownership, parent pointer handoff and outside-chain dismissal are implemented.
Child-menu semantic ownership follows logical parentage through portals, with
cycle/stale-reference protection. GPU readback, native AccessKit consumers and
12 native X11 stages validate the public API. Hovering a submenu's trigger row
opens it and hovering another item of its menu closes it, as AppKit's menus do;
a click or the keyboard opens it too.

Owned component event hooks now compose through views, providers and slots, with
explicit focusability and user listeners before built-in defaults. Native key
cancellation suppresses the associated printable text and clipboard shortcut;
editor cleanup remains independent of cancellation. Native Wayland seat testing
also exposed and fixed the X11-only clipboard backend. Standard Wayland data-device
copy/paste now passes alongside pointer, keyboard, disabled controls and resize;
native IBus/X11 IME now passes. Paced Wayland composition/cancellation is now
validated below; burst-input reliability remains open. On macOS, editing,
Unicode clipboard exchange with TextEdit, Tab focus and resize are observed in the
[Mac continuation packet](platform-validation/macos-continuation/README.md).

Multiline editors now support inherited/reactive `.text_wrap(true)` using the
allocated content width for rendering, hit testing, selections, caret and IME
geometry. Visual Home/End carries wrap-boundary affinity; vertical movement keeps
the preferred column through short lines. Resizing, wheel scrolling, model edits
and font changes preserve ownership and bounded invalidation. Trailing newline
geometry includes its empty final row. Core, shaped Unicode/bidi, GPU differential
and native X11 checks cover the public path. IBus/libpinyin X11 composition is
now validated separately, and Wayland text-input-v3 composition passes a paced
Sway/Fcitx5 smoke (below). macOS dead-key composition passes; macOS candidate
(Pinyin) input remains open.

Native Linux platform validation now includes live Wayland 1×/2×/1× scaling,
X11 XSETTINGS DPI changes with stationary pointer input (normal and maximized
windows), held-key focus transfer, and real IBus/libpinyin preedit/commit/cancel
and candidate placement. These checks exposed and fixed stale X11 scale queries,
synthetic key activation, inactive/stale IME routing and X11 candidate baseline
positioning. Exact-source archives and before/after evidence are retained.
Formal Winit suspension is not emitted on Linux/macOS; machine sleep/wake,
physical monitor transitions, driver loss, hardware GPU and native macOS evidence
remain outstanding.

Custom line height is implemented through inherited sparse styles and normalized
font metrics. Layout, native GPU/software glyph positioning and editor
caret/selection/IME geometry share the same pitch. Exact positive tight values
retain the existing outer text clipping policy; normal reset uses the child's
font size. Core, shaped-text, GPU differential and native X11 tests pass, including
glyph reuse and zero invalidation for equivalent styles. Headless measurement
uses the shared grapheme placement rules with constant auxiliary memory.


Focused-editor interaction now reuses one accounted layout snapshot per UI,
with bounded text/weight admission and unknown custom shapers bypassing retention.
Typography, wrapping width, composition text and shaping-engine replacement
invalidate geometry correctly. Counting and native GPU regressions cover reuse,
selection, scrolling and navigation; see [editor evidence](performance/editor-shaping/README.md).
This removes repeated shaping, while display-string allocations remain a separate
optimization opportunity.


Percentage width/height now compose with fluent sparse styles, padding, flex,
min/max constraints and absolute panels. Parent resize updates retained editors,
images and viewport geometry. Indefinite normal-flow axes use intrinsic sizing
instead of cyclic percentage solving. Component, core-layout and GPU damage tests
cover the behavior. Percentage min/max sizes, margins and padding are exposed too
(`min_w_percent`, `m_percent`, `p_percent` and their per-edge forms), covered in
`tests/advanced_layout.rs`.


Inherited letter spacing now flows through component typography, font-aware
measurement, editors, GPU text and the software renderer. Normalized style keys
preserve equal-write suppression; native logical-pixel tracking converts to
shaper em units. Tests cover Unicode geometry, signed tracking, bounded extreme
values, selections, cache invalidation and bitmap reuse. The core-only fallback
remains approximate. Rich-text spans take it too (`TextSpan::letter_spacing`).


A lifecycle audit added regression coverage for repeated keyed/provider/slot
teardown with retained handles, reentrant listener/task destruction and waker
cleanup outside scheduler locks. Shutdown explicitly releases clipboard resources
even when an inert WindowFactory clone survives. Native X11 verifies that release;
these checks do not substitute for OS sleep/wake or native macOS lifecycle tests.

Live Linux AT-SPI bus-client checks now cover component names/roles, editor text
and selection, control actions/state, and fresh-state accessibility reactivation.
Text metadata is retained across selection-only changes. See the
[validation evidence](platform-validation/atspi/README.md) for bridge limitations;
Orca usability and native macOS accessibility remain platform validation work.

Retained rendering now skips zero-opacity subtrees before image uploads, layer
preparation and backdrop filtering. Fixed-size image-source replacement avoids
intrinsic relayout. IME accessibility cancellation resets keyboard suppression;
the queued reset interval is guarded, but pinned winit's Wayland backend does
not expose protocol serials needed to reject arbitrary delayed server commits.
This remains part of the Wayland IME validation/integration work.

Zero-opacity subtrees are now pruned during layer traversal and bound calculation,
preventing invisible descendants from inflating offscreen allocations. Hidden
blur filters no longer expand unrelated damage. Layout and mounted state are
preserved; public paint traversal still supports resource liveness accounting.

Effects normalize nonfinite opacity, blur and fade at the scene mutation boundary.
Repeated equivalent invalid values remain idle; public styling docs specify the
fallback policy, with component precedence and GPU pixel/counter regressions.

Nonfinite translation coordinates normalize independently to zero before scene
mutation. Regression coverage includes hit testing, damage restoration, reactive
style precedence and compositor layer reuse. Large finite-coordinate accumulation
is not clamped by this policy.

Pointer capture tracks its initiating button. Multi-button pointer regressions
cover editors, sliders, scrollbars, custom capture and activation cleanup, with
native X11 editor/slider chord evidence.

Keyboard activation survives unrelated key releases. Active styling separately
tracks pointer and keyboard presses and follows the latest nonrepeat Enter/Space
owner; core and native X11 chord/cancellation tests cover the behavior.

Disabling a subtree now atomically detaches pending interactions before cleanup
callbacks, with regressions for reentrant removal, reenable and refocus. Native
held-interaction tests verify cancellation and fresh input after reenable;
composed slider pointer presses now explicitly acquire focus.

External model replacement during preedit now advances an explicit local
cancellation revision. The host resets the native context and releases keyboard
suppression while preserving normal empty-preedit/Commit behavior. Real X11
IBus tests cover replacement without focus/engine changes, ordinary editing and
fresh Unicode composition. This is not a native protocol epoch: delayed Wayland
server commits and model writes between native empty preedit and commit remain
outside its guarantee.

Committed text normalization now covers initial/external models and all editor
input paths. Canonical-equivalent writes preserve editing state and stay idle;
normalization borrows canonical text. Editor/tree construction batches effects
to make model-triggered unmount safe, with retained-image initialization compatible
with deferred bindings. Native model/display and IME regressions pass.

Editors now expose static/reactive read-only policies through component builders
and retained handles. Selection, navigation, copying and external model writes
remain available; user mutation paths are blocked. Native IME permission follows
editability without changing logical focus, including reset guards for rapid
toggles during composition. AccessKit projection and real X11 clipboard/IBus
checks cover the integrated behavior, and on macOS a probe of its own
window (`scripts/macos_read_only_probe.py`) checks typing, Backspace, Return
and input-method text change nothing while focus, selection and navigation
work.

Public mount initialization now carries temporary ownership through deferred
first bindings, including initial keyed/conditional children. Failure removes
that exact mount and releases resources while preserving unrelated siblings;
success releases the temporary scope. Binding self-removal no longer resurrects
ownership, and disposed effects cannot acquire new orphan subscriptions. This
is ownership cleanup rather than arbitrary state rollback or native panic
recovery; late render failures cannot restore an already replaced document.

External editor updates now store a single changed UTF-8 span rather than whole
old/new documents in undo history. Canonical signal ingress avoids full-text
clones; selection, preedit cancellation and bounded history behavior are tested.
An archived before/after headless benchmark demonstrates lower elapsed time and
requested allocation bytes while retaining more undo steps under the same
limits. Document comparison remains linear, and full GUI performance claims
continue to use the separate matched-framework evidence.

Editor selection/policy refresh now borrows committed display text, retains the
paint Arc and updates semantic text without cloning the old document. Allocation
regressions and live AT-SPI/native IME checks validate reuse and correctness.
Native accessibility projection now skips unchanged nodes before rebuilding
owned values, using per-node semantic tokens and cached geometry/children.
Changed nodes still require owned values and cached clones; projection still
walks the visible scene metadata.

GPU backdrop blur now reconstructs connected filter dependency regions rather
than unconditionally repainting the whole target. Physical kernel bounds and
bounded source copies are covered by fresh-frame GPU differential tests,
including cached isolated layers and 64 mutation frames. The fluent component
`effects` example passes native blur/fade/opacity/animation controls; scratch
textures remain full size and overlapping dependencies may still require a full
repaint. See [validation](validation.md#bounded-gpu-backdrop-reconstruction-and-declarative-effects).

Multiline Page Up/Down navigation now uses the padded viewport, shares preferred
X with arrow navigation, preserves Shift-selection and undo/redo, and supports
read-only fields. Seven integration tests and native wrapped-Unicode paging,
resize and boundary checks pass. Stationary selection-drag autoscroll is now implemented and validated below. See
[validation](validation.md#editor-visual-page-navigation).

Shift-click, double/triple-click and whole-unit selection dragging are implemented
and verified through the native modifier/click-count path. Unicode segment
selection respects graphemes and clicked glyph geometry; native visual-line
ranges account for mixed-direction text separately from physical Home/End edges.
Legacy synthetic event literals and single-click dispatch stay compatible.
Native selection, pointer-chord capture and IME checks pass; see
[validation](validation.md#editor-pointer-selection-and-native-click-metadata).

Stationary editor selection-drag autoscroll is implemented with one core-owned
interaction deadline, bounded motion, no idle scheduling, and captured-gesture
cleanup. Deterministic tests cover resize rearming and capture transfer; native
held-pointer and hide/restore checks pass without periodic diagnostic wakeups.
See [validation](validation.md#stationary-editor-drag-autoscroll).

Native transparent-window composition now passes an owned X11/picom desktop
pixel test covering alpha overlap, retained updates, movement/resizing and a
changed backdrop. Native Wayland/Sway composition now also passes six stages
and 32 exact source-over pixel samples; native macOS composition remains
unverified. See [Wayland evidence](validation.md#native-wayland-transparent-window-composition). Real
Wayland text-input-v3 composition now passes a paced Sway/Fcitx5 smoke with
Escape/focus cancellation and two editor models. A separate initial input
burst lost client-visible preedit updates around a cursor transaction; its
trace is preserved and rapid-input reliability remains unresolved. Native
macOS IME and delayed stale serial handling remain open. See
[Wayland IME evidence](validation.md#real-wayland-input-method-composition). See [validation](validation.md#native-x11-transparent-window-composition).


Native Wayland external-model and read-only cancellation now pass with actual
Fcitx5 composition, stable focus, resumed normal editing and a fresh Unicode
commit. The consolidated three-session evidence verifies 163 source inputs and
two current binary hashes. Passing tests deliberately pace native preedit;
rapid-input transaction losses remain a separate unresolved finding. A direct
winit/softbuffer diagnostic delivered final preedit and commits in 22/22 tested
bursts, including a cursor-delay sweep; it did not reproduce the full failure.
No product workaround was added. See [diagnostic evidence](platform-validation/wayland-ime-burst/README.md).
See [cancellation evidence](validation.md#wayland-model-and-read-only-composition-cancellation).


Two earlier performance refreshes remain deliberately unaccepted: the first attempt
stopped amid disk exhaustion, and its 36-process retry failed update parity in
one scrolling trial while unrelated host builds were active. Raw evidence and
source/binary hashes are retained. Failure sample preservation now has six
regression tests. A subsequent preregistered four-worker Mesa series passed all 36 trials and
the unchanged workload gate. Its 10,808 samples and source/binary proofs are
audited; shared-host interference remains explicit. See the
[latest comparison](results/measured-framework-workers4/README.md).


Variable-height component lists now use a shared reactive height index, retained
keyed children and the existing fluent viewport styles. Prefix searches and point
updates take logarithmic work; appending extends the prefix tree rather than
rebuilding it. Height changes preserve the first visible index/intra-row anchor;
keyboard paging, scrollbars and focus markers follow actual allocated heights.
Application-supplied allocations remain available alongside naturally measured
rows whose offscreen heights are explicitly approximate. Source-frozen validation and the native streaming
fixture are recorded in `docs/platform-validation/variable-virtual-list`.


Naturally measured virtualization shares the same retained child architecture.
Mounted wrapping/child changes update cached heights in a batch; a minimum row
allocation and geometric discovery bound inaccurate-estimate work. Keyboard
reveal remains active during discovery so measured rows cannot displace the
requested focus target. Offscreen caches remain estimates until remeasurement;
no global layout or exact unseen extent is claimed.

The combined measured-list source archive, native streaming/resize evidence and
full validation are recorded in `docs/platform-validation/measured-virtual-list`.

The macOS CI now includes mandatory Metal pixel/damage tests and bounded native
component-window lifecycle checks with preserved logs. The portable lifecycle
harness passes on Linux; no remote macOS execution is claimed.

Compositor-side burst traces now reproduce stuck final preedit in both zgx and
an independent winit/softbuffer program under zero-delay injection. The tested
Sway/wlroots/Fcitx stack rejects the final transaction with an obsolete serial;
all subsequent Unicode commits still succeed. This establishes an external
input-stack compatibility issue rather than a proven zgx rendering defect.
No speculative workaround was added. The exact reproducer and unsent upstream
report are linked from [validation](validation.md#wayland-burst-input-serial-diagnosis).

A private Fcitx frontend patch now demonstrates recovery of rejected final
preedit at the new input-method serial, with all four rapid-input observations
and eight cancellation/focus stages passing. It is retained as an upstream
candidate with exact source and binary proof; it is not installed, bundled into
zgx or claimed as a general input-method fix. See the
[experiment](platform-validation/wayland-ime-fcitx-private-refresh/README.md).

Clean blur panels no longer force distant streaming updates into separate
per-draw render passes. The batched path is selected after dependency expansion;
halo-touching changes still repaint filters. GPU work counters and fresh-frame
differential tests cover flat and isolated scenes. Full isolated-layer repaint
and target-hardware qualification remain explicit optimization/testing bounds.

## Parity expansion validation

The [integrated Linux packet](platform-validation/gpui-parity-integration/README.md) records 712 workspace tests, 31 doctests, two separately executed native lifecycle tests, strict all-target Linux/macOS cross-checks, and rebuilt Linux native capability fixtures. The [matched Linux comparison](results/gpui-parity-workers4/README.md) passes all 36 trials and an independent raw-data/source/binary audit. Its software Vulkan measurements remain separate from native Mac correctness and do not establish Mac hardware rankings.

The [Mac continuation packet](platform-validation/macos-continuation/README.md)
records actual Metal/CoreVideo tests, native lifecycle probes and GUI observations
on Apple M5 Max. It also preserves failures that cross-compilation missed:
font-dependent assertions, AppKit menu placement, owner-close sheet dismissal,
application delegate integration, a borderless maximize query causing an
event storm, and stale native dead-key state on editor focus transfer. Repairs
preserve the retained component/children and fluent-style
architecture. Native reopen, fullscreen/restoration and minimum-size enforcement
now execute successfully; resource sampling and final quality evidence must be
associated with their actual source/binary identities. The pending native checks
listed above prevent a completion claim. Automated suite counts and model-update
logs do not close those gates.
