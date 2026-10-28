# Changes

zgx is on crates.io from 0.1.0 (`zgx = "0.1"`); this lists what changed for apps, newest first.

## Unreleased

### Added

- `View::hollow()`: the pointer passes through a box while nothing it holds is displayed (`hidden` or `invisible`). A Luau `presence` box uses it: a page kept mounted but hidden no longer takes the presses meant for what is under it (a web page, the panes).
- **Windows: input methods through the Text Services Framework**, opt-in with `ZGX_TSF=1` (IMM32 stays the default for now). Editors give Windows' input methods and text services a text store. They read the text around the caret, place candidates at the clause they convert, and insert at the caret: the emoji panel, dictation, the touch keyboard. For it, editors take an input method's clauses (`TextEditor::set_preedit_clauses`, `ClauseStyle`), each underlined, thick under the one being converted. They also take a replacement as one undo step (`TextEditor::replace_range`) and count UTF-16 offsets (`utf16_len`, `byte_to_utf16`, `utf16_to_byte`). `EditorHandle` makes these edits (`insert_text`, `replace_text`, `set_composition`, `commit_composition`, `cancel_composition`, `input_method_area`) ([windows](docs/windows.md)).

### Changed

- macOS: frames presented as `CAMetalLayer` drawables (`ZGX_PRESENT_DRAWABLE`, or `=auto` where its trial picks them) show a refresh or two sooner. They are paced by the view's own display link and kept in phase: when frames start waiting most of a refresh for a drawable (the pool full, as most launches of an M1 Max settled), one tick is skipped and they catch up. The pool is two drawables, not three. A frame's start to its showing: 16 ms against 27–32 on an M5 Max, 24 against 27–40 (median 36) on an M1 Max. The window server's CPU on an M1 Max drops from 34.2% to 31.3%, and each window holds 10 MiB less. Tiles, the default, are unchanged.
- macOS: `ZGX_PRESENT_DRAWABLE=auto` judges by the frames each path skips again, not by energy. A process can't measure the GPU's energy, only its busy time, and what a busy second costs depends on the GPU's clock (0.78 W on an M5 Max, 4.44 on an M1 Max). The energy rule chose drawables where they measure worse. `ZGX_TRIAL_RULE=energy` keeps that rule, uncalibrated. A GPU-heavy, full-rate app on an M1-class Mac may choose drawables itself (`ZGX_PRESENT_DRAWABLE=1`): about 150 mW less GPU and 5.7 points less window-server CPU, for about 4.7 points more app CPU.

## 0.1.4 (2026-10-27)

A patch: Rust and Luau code builds as before (every API change since 0.1.3 is an addition, and zgx-desktop and zgx-gpu 0.1.3 build against this zgx), unless it writes a `zgx_gpu::GpuStats` field by field (below). A few editor keys and clicks change, each below with what to check.

### Changed: new fields

- `zgx_gpu::GpuStats` gains `layer_partial_repaints` (layers repainted only where the window's damage reaches them). One written field by field ends with `..Default::default()`.

### Added

- **Zoom an element**, as CSS's `zoom`: `Styled::zoom(n)` (Luau `zoom = n`) shows a node's subtree `n` times as large, laid out again at that size and drawn crisply at it; zooms multiply with ancestors' and the window's, and `zoom_fixed()` (Luau `zoom_fixed = true`) keeps a node at its own size whatever zooms are around it. `Scene::set_zoom`, `zoom`, `zoom_around`, `Ui::set_zoom_fixed`; `PlaneSplit::native_zoom` for hosts. At 1 nothing changes. A zoomed node paints as a cached layer at its exact scale, repainted only where the window's damage reaches it ([styling](docs/styling.md#zoom)).
- **Zoomable elements**: `View::zoomable(state)` (Luau `zoomable = state`) lets the user zoom a node in place: Control with the wheel over it steps the window's levels (Windows, Linux) and a pinch over it zooms (every platform), 25% to 500%, the innermost zoomable under the pointer taking the gesture and the window zooming elsewhere; in a scroll view what is under the pointer stays there. `zgx lint` reports a `zoom` beside a `zoomable` (`zoomable-zoom`).
- **Double-click selects a word of Chinese, Japanese or Thai**, as Chrome does: those scripts put no spaces between words, and Unicode's boundaries took each character for one. zgx's new `cjk-thai-words` feature, on by default (and forwarded by `zgx-desktop` and `zgx-script`), finds words with ICU4X's dictionaries (北京 in 我们在北京大学, สวัสดี in สวัสดีครับ). The dictionaries are about 2.2 MB of a release binary, paged in when first used, with no startup or per-frame cost. Turn it off with `default-features = false` on each zgx crate an app depends on. zgx's licence is now `MIT AND Unicode-3.0`, for the Thai dictionary it carries ([composition](docs/composition.md)).
- **Lists copy as lists**, as a browser's do: a list item's marker ("•", "3."), drawn as accessible-hidden text leading a `Role::ListItem` (as markdown draws its lists), is now the list's. A selection takes no part of it, plain text leaves it out, and copied HTML is a `<ul>`, or an `<ol start>` from the marker's number, nested as the lists are. Screen readers hear where each item is in its list ("2 of 5") unless the app says so itself. `zgx lint` reports a marker left as text (`list-marker-hidden`) ([composition](docs/composition.md)).
- Windows: a start says whether it read its shaders from FXC's disk cache or compiled them, and which it couldn't keep (`zgx_gpu::shader_cache_counts()`; `ZGX_SHADER_LOG=1` prints it with the first frame, as `ZGX_PROFILE=1`'s startup marks carry it). It is `None` elsewhere, and with crates.io's wgpu-hal.


### Performance

- **CPU, against 0.1.3**, by exact instruction counts (Linux arm64, with a screen reader on for both, as 0.1.3 always ran; with none, as desktops run, 0.1.4 does less again): widgets −93.7% (a mount lays out its text inputs once, below), style −13.5%, animate −11.2%, heavy −9.5 to −9.9%, anime −5.7%, script-update −4.2%, the apps, lists and scrolls −0.2 to −2.7%; lab-stream and list-state level. No scene is slower.
- **macOS: a focused field left alone takes about 180 MB less memory.** Its caret is a Core Animation layer, shown and hidden each blink with no frame drawn, where it was a frame each blink: the Metal driver holds about 176 MB for its command queue for some 8 s after any frame, so a blinking caret kept the window's footprint at 210 MB, where it now falls to an idle window's 30 (M5; GPUI Fast, MyGo and Rune keep 219–404 MB blinking a caret). The window does no GPU work while the caret blinks: two small wakes a second, none while the field is unfocused or the window unseen (its window server cost is what GPUI's blinking frames cost, with 1/13 of the app CPU). It is the editor's caret, drawn in frames as before, wherever Core Animation's could differ (`docs/composition.md`); `ZGX_NATIVE_CARET=0` keeps it everywhere.

### Changed: what to check in an existing app

**A mount's text inputs are laid out once, when it ends, and geometry read during a mount is reactive**: a mount of many inputs is linear, not quadratic (the widgets benchmark −93%), including one whose rows each run a callback of the app's own. An observer in the mount that reads geometry (`Scene::bounds` and the boxes made from it, from a reactive style or a binding) runs again once the mount's layout settles, if what it read has moved: a style derived from an input's width of 160 settles on 160, where it read an earlier layout's (such as the default 240) and kept it. One whose reads keep moving runs again at most a few times, then at the next flush.
- *Migrate:* derive what a callback makes from its geometry read each time it runs. One that keeps only its first read (a `Cell` set once) keeps a value from before layout; an app relying on the earlier width needs nothing else.
- A `TextMeasurer`, `TextShaper` or `FontTextShaper` is a cached computation: it may be called any number of times, in any order. The signals it reads keep their subscriptions; writing a signal from one is unsupported.

**Editor keys go by words and paragraphs as each platform's text fields do.** On Windows, Control-Right and Control-Delete stop at the next word's start (past the spaces), not the end of the word. On Windows and Linux, Control-Up and Control-Down go to the start of a paragraph, not up or down a line. On macOS, Control-A and Control-E go to a wrapped paragraph's edges rather than its row's. Control-K kills into a buffer that Control-Y yanks back, and Control-T swaps characters.

**A fourth click keeps a triple click's paragraph selected**, as Chrome's do: presses in a row count on past three (Rust `EventContext::click_count`, Luau's `clicks`), where they went round to 1 again, and selection takes three and on as a paragraph.
- *Migrate:* a handler that relied on the count going back to 1 after three reads `(clicks - 1) % 3 + 1`.

**Shift-Right extends back through right-to-left text**, as Chrome does: Shift with Left or Right extends a page's selection the way its block runs (`direction`), so in a right-to-left paragraph Shift-Right grows it rightward on screen.

**Copied HTML keeps code blocks and colours**: a selection copied from a page (`View::selectable`) puts monospace text of more than one line in `<pre>`, and runs coloured apart from the text the selection starts in carry their colour (`<span style="color:…">`); body text and links carry none, so a dark theme's white text doesn't paste invisible.

**macOS: Home and End in an editor scroll to the text's start or end and leave the caret where it is**, as NSTextView and Chrome on Mac take them. Shift-Home and Shift-End extend the selection to the text's ends. ⌘← and ⌘→ go to a line's ends, ⌘↑ and ⌘↓ (and ⌘Home, ⌘End) to the text's, as before. Windows and Linux are unchanged.
- *Migrate:* a test or script pressing Home or End in an editor on macOS to move the caret presses ⌘← or ⌘→.

**A password field takes no input method, and on macOS types with secure keyboard entry on**, as browsers' password fields and AppKit's secure text fields do: a candidate window no longer shows what is typed, and other processes can't read the keys while one has the keyboard (`EditorHandle::is_secure`).
- *Migrate:* nothing, unless an app relied on composing (pinyin, kana) in a secure field: its keys now type their characters.

### Fixed

- **A scroll under a rounded clip no longer leaves corners behind**: a panel with rounded corners and `overflow: hidden` around a scrolling view, scrolled down and back, trailed copies of its corners down the view's edge (since before 0.1.2). Copying renderers now repaint each such corner where it is and where the scroll took its pixels; a scroll with no rounded clip around it costs what it did.
- Windows: a frame of the app's own timing ready just before the compositor latches the next refresh waits until just past it, rather than making and missing the refresh in turn: with a timer drifting near the latch, missed refreshes in an 8 s lab stream went from 45–76 to 0–1.
- X11: a wheel notch from a pointer with no scroll valuators (XTEST's, a remote desktop's) scrolls one line and zooms one level, not two.
- A press in a zoomed or scaled editor lands on the character it shows on, and a drag selects the run it covers; extending a selection in a zoomed scroll view scrolls just enough to show its end.
- macOS: each of an app's menus shows once in the menu bar (Edit, Edit, View, View before).

## 0.1.3 (2026-10-27)

A patch: Rust and Luau code builds as before (every API change since 0.1.2 is an addition), unless it writes one of the three structs below field by field. Some defaults change, each below with what to check: zoom and find in page are on, keys and cursors behave as a browser's and AppKit's, and Windows presents and serves assistive technology differently.

### Changed: new fields

New features add fields to these structs; each has a default, and one written field by field ends with `..Default::default()` (or `..LocalTransform::IDENTITY`). Every type and signature that was in 0.1.2 is unchanged: `LocalTransform::matrix` and `PaintItem::matrix` still give an `Affine`, and a tilt's map in perspective is beside them (`LocalTransform::projective`, `PaintItem::projective`, `zgx::affine::Projective`).
- `zgx::scene::LocalTransform` gains `rotate_x`, `rotate_y` and `perspective` (3D tilts, below).
- `zgx::scene::PaintExtras` gains `projective`.
- `zgx::trace::Frame`, a frame of the devtools trace, gains `notes` (why the frame did what it did) and `gpu_micros` (the GPU's own time).

### Changed: what to check in an existing app

**A window zooms, and finds in its text, as a browser's page does** (both below, under Added): Command (Control elsewhere) with plus, minus or 0 zooms a window, and Command-F (Control-F) opens a find bar. On Windows and Linux, Control with the wheel zooms too, and scroll views no longer scroll on it. A keymap or menu item binding those keys keeps them, and a node handling the wheel keeps a Control-wheel, so an app that has its own is unchanged.
- *Opt out:* `Ui::set_zoomable(false)` (Luau `ui.window.zoomable:set(false)`) and `Ui::set_find_in_page(false)` (Rust; a Luau setting follows).

**Enter activates a focused control as it goes down, and toggles no checkbox; Space follows no link.** As a browser's controls and AppKit's take the keys: a button (and anything with `on_click`) runs on Enter's press, not its release, and on Space's release as before; a link takes Enter alone (Space scrolls); a checkbox, radio button or switch takes Space alone, so Enter goes on to the form's default action.
- *Migrate:* a checkbox meant to toggle on Enter handles the key itself (Rust `.on_event` on `KeyDown` of `Key::Enter`, Luau `on_key`), setting its state; a test pressing Enter and expecting the action on its release expects it on the press.

**Buttons, checkboxes, menu items and sliders keep the arrow cursor**, as Chrome's controls and AppKit's do; only links show the pointing hand, and text fields the I-beam. They showed the hand (sliders a resize arrow) when they stated no cursor.
- *Migrate:* a control meant to show the hand states it: `.cursor(Cursor::Pointer)` (Luau `cursor = "pointer"`).

**Option-Tab and Command-Tab (Alt and Super elsewhere) no longer move focus**: they are the system's, or a field's literal tab, and go on unhandled. Tab, Shift-Tab and Control with either still do, and Control-Tab now leaves a code editor that indents on Tab, as AppKit's text views do.
- *Migrate:* an app meant to move focus on Option-Tab binds it (a keymap entry, or `on_key`) and moves focus itself.

**A `display: none` node's children report a 0x0 box** (`Scene::bounds`, `on_resize`, `zgx inspect`), as the node itself did and as CSS gives them: they reported their content's size at no width. They are no longer measured again each time their parent lays out.
- *Migrate:* code reading a hidden child's size to plan its layout reads it once the child shows (its `on_resize` fires then).

**Windows: a window builds its accessibility tree only once a client reads it.** Windows' text services ask every window for its client area as it shows, with no assistive technology running, and zgx answered with its whole tree, built and kept in sync from the first frame (15–22% of a frame in lab and vlist). That request now gets AccessKit's placeholder root. UI Automation's request for the root (Narrator, NVDA) builds the tree at once, as before, and a client reading past the placeholder (its children, focus, a point, a pattern) builds it too, a focus event telling it to read again.
- *Migrate:* a test or tool reading a window through MSAA (`OBJID_CLIENT`) waits for that focus event, or reads through UI Automation. `ZGX_ACCESSIBILITY_LOG=1` logs the requests and when the tree is built.

**Windows: a window presents through a swap chain of its own**, flip-sequential, its buffers the retained frame (as macOS's surfaces are) and each frame's changed regions passed to the compositor as dirty rectangles, rather than copying the whole frame into wgpu's buffer every frame. The app's GPU does 33–45% less in animate, vlist and lab, 12% less in heavy, the compositor less too, and the first text shows 346 → 280 ms after launch.
- *Migrate:* nothing to change. If a window draws wrongly on some GPU or driver, `ZGX_DXGI_PRESENT=0` presents through wgpu's swap chain, as before, for this release (and please report it). A renderer presenting into a caller's visual always does.

### Added

- **Find in page**: Command-F (Control-F elsewhere) opens a find bar at the window's top right, as a browser's: matches across all the window's text, fields and virtual lists' rows (those not mounted too), case and accents ignored, marked behind the text (theme tokens `find.match`, `find.current`), Enter and Command-G (Control-G, F3) stepping, Escape closing with the match selected. A match in a field is scrolled to without moving its caret, until the bar closes on it. On macOS, Command-E finds the selection, and an app's Edit menu gains AppKit's Find submenu. `Ui::find_text`, `find_next`, `find_status`, `find_results`. An app whose keymap or menu binds Command-F, -G or -E keeps its own.
- **Zoom**, as a browser's page zoom: Command (Control elsewhere) with plus or equals zooms a window in, minus out, 0 back to 100%, through Chrome's levels (25% to 500%). The window's content draws at the display's scale times the zoom, so layout reflows in a smaller or larger viewport and text and paths stay crisp; scroll views keep what was at their top. On Windows and Linux, Control with the wheel zooms too, a level a notch, when nothing under the pointer takes the wheel (a canvas's or a script's `on_wheel` that zooms keeps it); on macOS neither the wheel nor a pinch zooms, as in Safari. A window with a frame autosave name keeps its zoom and reopens at it. `Ui::zoom`, `set_zoom`, `zoom_in`, `zoom_out`, `reset_zoom`; Luau's `ui.window.zoom` and `ui.window.zoomable`, states that follow the keys and set the zoom (`ScriptHost::window_zoom` for a Rust host); on macOS an app's View menu gains Zoom In, Zoom Out and Actual Size. An app whose keymap or menu binds the keys keeps them; with Option, or Command and Control both, they are the app's (zterm's font size, ⌘⌥=). Native views follow it: terminals and web pages are placed in the zoom's points, a terminal's text drawn at its scale, a page at its own zoom times the window's (WKWebView's page zoom, WebView2's zoom factor). At 100% nothing changes: the content scale is the display's, exactly.
- `radio(label, selected, value)` and `radio_group(label)`: radio buttons as a browser's and AppKit's, one Tab stop per group landing on the checked one, the arrows (all four) moving and choosing, wrapping at the ends. Any `Role::RadioGroup` (Luau `role = "radiogroup"`, `segmented`) is such a group now; its arrows stopped at the ends and chose nothing.
- `on_aux_click` in scripts: the middle button (or another beyond the primary and secondary) pressed and released on a node, as the DOM's `auxclick` (a tab closed on a middle click, as browsers do). It is told `{ button, x, y, local_x, local_y, shift, ctrl, alt, meta, cmd }`, `button` being `"middle"` or the platform's number; the innermost node with one takes it.
- Typing in a list box (`Role::ListBox`, Luau `role = "listbox"`) or a keyboard virtual list (`keyboard_navigation`, Luau `keyboard = true`) jumps to the next option or row whose text starts with what was typed, as a browser's list box and AppKit's tables do: rows not mounted by their `row_text`, without mounting them. A list box's Page Up and Page Down move a page of its options.

### Drawing

- **Blend modes**, as CSS's `mix-blend-mode`: a view blends with what lies beneath it in any of the W3C's fifteen modes (multiply, screen, overlay, darken, lighten, color-dodge, color-burn, hard-light, soft-light, difference, exclusion, hue, saturation, color, luminosity): `Styled::blend(BlendMode::SoftLight)`, `Scene::set_blend`, Luau `blend = "soft-light"` (a literal or a state). Inside a layer it blends with that layer, not the window. On the wgpu path and in the software renderer.
- **3D tilts in perspective**, as CSS's `perspective() rotateX() rotateY()` about a node's origin: `Styled::rotate_x`, `rotate_y` and `perspective` (Luau props of the same names; anime keys `rotateX`, `rotateY`, `perspective`). The layer composites as a projected quad, sampled perspective-correctly, on the wgpu and direct Metal paths and in the software renderer; its bounds, damage and hit testing follow the projection.
- **Layer shaders**: a node's cached layer composites through WGSL of the app's own (`fn layer(input: LayerInput) -> vec4<f32>`, `zgx_sample(p)` reading the content elsewhere in the box), for a ripple, a chromatic split, scanlines or a dissolve: `Styled::layer_shader`, `Scene::set_layer_shader`, Luau `layer_shader = ui.layer_shader(wgsl)`, with uniforms (`set_layer_uniforms`, `layer_params`) and the animated `layerTime` and `layerParam0..15`, which composite again without repainting.
- Frames with a blend or a layer shader draw through wgpu rather than macOS's direct Metal path, for now.

### Text

- **Selected text drags**, as in AppKit's text views and browsers: a press on a field's selection, or selectable text's, moved past the drag threshold drags it (`DraggedText`). Dropped in a field it goes where the caret shows, selected: within its field it moves as one undo step (`TextEditor::move_text`), into another field it moves too, and Option (Control elsewhere) copies. Out of the window, other apps take it as plain text. A password's text never leaves its field, and read-only fields take none. Views of an app's own take it with `View::on_drop::<DraggedText>`.
- **Text dragged in from other apps** (a browser's selection, an editor's) drops into the field under the pointer, its caret showing where, the text selected there. On Wayland always; on macOS, Windows and X11 with zgx's winit patch, which an app from crates.io has only with its `[patch]` lines ([getting started](docs/getting-started.md), [releasing](docs/releasing.md)).
- **Look Up** (macOS): a field's and selectable text's context menu offers Look Up “…” for the selection, and Control-Command-D looks it up, in the dictionary's panel at the text, as AppKit's text views do (`Platform::looks_up`).
- **Services** (macOS, with zgx's winit patch, as above): the Services menu sends a field's selection, or selected display text, as plain text, and a service returning text replaces a field's selection. The menu bar's Services submenu is the one macOS fills; it was empty.
- **Assistive technology finds where each character shows**: a field's and selectable text's runs carry their visual lines' boxes and each character's place and width, so VoiceOver's cursor, Zoom and Hover Text follow the caret and the selection (macOS answered an empty box before).
- A triple press in a multiline field selects the paragraph, its break included, and a drag goes on by paragraphs; a shift-click after a double or triple press extends by words or paragraphs; Option-Up and Option-Down go to a paragraph's start or end (macOS), as NSTextView does.

### Controls and platforms

- `virtual_list` and `ui.list` stretch at their edges and spring back, as scroll views did (AppKit's tables, Safari's pages); the stretch moves their content alone and places no rows. Springs back and overlay scrollbars' fades step every refresh (they stepped every 16 ms, every other refresh at 120 Hz: `Ui::set_refresh_interval`), and a fling's momentum into an edge bounces once.
- macOS: the menu bar is made once the first frame shows (within a second), not before it; a key pressed sooner makes it at once, so shortcuts hold from the first key.
- `zgx inspect search` and `zgx inspect zoom` (MCP `zgx_search`, `zgx_zoom`) find in page and zoom a running app for agents.
- `zgx inspect frames` says why a frame did what it did (`why:`: a scroll repainted rather than copied and what kept it from copying, a frame kept to the last one's cadence or that missed its refresh, a window that stopped presenting; `zgx::profile::note`) and the GPU's own time (macOS), takes `--runs`, `--since <mark|frame>` and `--keep`, and warns when the window isn't presenting; `mark` sets a point to read from. `replay` takes `--repeat N` (each run and their spread) and `--every MS`, and is refused while the window can't draw; `record stop` warns when a tape was recorded while the window fell behind. `switch` (`zgx::switches`) turns scroll copies, the refresh grid and partial repaints off and on in a running app, to measure what each saves; without the `trace` feature they cost nothing.

### Performance

- CPU cuts in style, layout, damage and text: against 0.1.2, by exact instruction counts (Linux arm64), style −25.1%, widgets −15.6%, animate −14.2%, anime −13.2%, script-update −10.8%, heavy −7.9 to −8.0%, script-layout −6.2%, chat −5.7%, mount −5.3%, the other apps, lists and scrolls −0.1 to −4.2%: every one of the 26 scenes is faster. Two reach the forks, published again with this release: a userdata method's check of its `self` inlined (`zgx-mlua` 0.12.4) and a glyph's place on the device (`zgx-cosmic-text` 0.19.2).
- Windows: the swap chain and accessibility changes above; the GPU device opens on a thread of its own from launch, beside the first window, as macOS's does (first content 299 → 275 ms); Direct3D 12 writes frame vertices into buffers mapped once, and release builds name no driver objects (wgpu's `DISCARD_HAL_LABELS`; `WGPU_DISCARD_HAL_LABELS=0` brings them back for a capture).
- macOS: a launch reaches its first frame with 11% fewer instructions (the menu bar made after it), some 10 M fewer again (the first frame's Metal shaders translated ahead, not through naga), and the first input after a window idles wakes the GPU while its frame is built (render to present 6.7 → 1.7 ms).

### Fixed

- A rich-text link shows its focus ring when focused from the keyboard, as a browser's `:focus-visible` does, not after a click on it (a key pressed on it then shows it, as for any control).
- The arrows walk mixed-direction text in the order it shows (Right in "abc אבג def" went back and forth for ever), and a paste, even of one character, is an undo step of its own.
- Icons drawn from the glyph atlas survive a change of scale and back (a window zoomed, or moved to another display): they drew other glyphs after it.
- macOS: a scroll after a pause loses no refresh (its second frame came ~22 ms after its first).

## 0.1.2 (2026-10-26)

### Fixed

**A Luau app starts in a new project.** Since luau0-src 0.22.1 (Luau 744, 2026-10-24), an app depending on zgx-script 0.1.0 or 0.1.1 that resolved its dependencies afresh panicked as its first script loaded: "Error configuring Luau … attempt to load a text chunk (mode is 'b')". zgx's own lockfile held 0.22.0, so its tests never met it. `zgx-mlua` 0.12.3 holds Luau at 0.22.0 (Luau 740) until mlua works with 744, and a release now checks that a new app, resolving every dependency afresh, runs a script.
- *Migrate:* depend on 0.1.2 (`cargo update -p zgx-script`). On 0.1.0 or 0.1.1, `cargo update -p luau0-src --precise 0.22.0+luau740` fixes an app.

Nothing else changed since 0.1.1.

## 0.1.1 (2026-10-26)

A patch: a Luau app runs as before, and Rust code builds as before unless it matches every variant of one of the enums below or writes one of the structs below field by field (each says what to add). Two fixes change what a box looks like, as CSS draws it; each says what to check.

### Changed: new variants and fields

New features add variants to these enums and fields to these structs. Code naming every variant, or every field, adds the new one. `DragPhase`, `profile::Phase` and `zgx_script::Value` are `#[non_exhaustive]` from now on, so a `match` on one ends with `_` once, and a variant added to them later breaks nothing.
- `zgx::input::DragPhase::Spring` (spring-loading, below): a `match` on a drag's phase ends with `_` (`Spring` before it to handle it).
- `zgx::profile::Phase::Wait` (waiting for the display, apart from a frame's work): a `match` ends with `_`.
- `zgx::semantics::Role::Article` (`role = "article"`): a `match` on a role adds `Article`. It comes last, so the other roles keep their values (`as` casts).
- `zgx_script::Value::Image` (a `Bitmap` in state): a `match` on a script value ends with `_`.
- `zgx_desktop::WindowOptions` gains `level`, `shadow` and `covers_display`; `zgx::decoration::Decoration` `clip_feather`; `zgx::scene::RoundRect` `feather`; `zgx_gpu::GpuStats` `patched_items`. One written field by field ends with `..Default::default()`.
- `GpuRenderer::debug_stencil` and `debug_stamped_frames` do nothing and count nothing (deprecated): frames no longer draw through a stencil.

### Fixed

**A rounded box that clips cuts its children at its padding box**, inside its border with each corner's radius less the border, as CSS's `overflow: hidden` does: clipped at its outer edge with its full radius, a child filling a bordered card painted over the border where it curves.
- *Migrate:* a child meant to reach the box's outer edge, under its border, takes `border_over` on the box: the border then takes no room and paints over the child.

**Rich text shrinks no further than its longest word**, as plain text and CSS's `min-width: auto` do: a `rich_text` (`ui.rich`) squeezed in a row narrowed to a single glyph's width before, one glyph a line.
- *Migrate:* where a paragraph should break inside its words to fit a narrow row, state `.min_w(0.)` (`min_w = 0`).

- An image's `dither` and `characters` follow a state or computed value, as `src` does, and apply to an image with a `fallback` too (`View::dither` panicked there as the script lowered).
- An image drawn in a cached layer (its own opacity, `layer = true`) repaints as the box lending it shader uniforms changes them: its characters' reveal no longer freezes.
- A drag that leaves a destination and comes back waits out the spring delay again (it sprang at once, or never again).
- A focused controlled input (`value = function`) shows a change made elsewhere at once (a chat's composer cleared on send kept the sent text); its answer to an edit still waits until it is left.
- A fixed box that shrank grows back when its content outgrows where it stopped.
- SVGs keep their proportions in a box of others' (SVG's default `preserveAspectRatio`) rather than stretching to it.
- A window's first layout is at the size it opened at, not the size asked for and again on `Resized`.
- X11: an app no longer hangs when its input method (ibus-x11) has gone. Wayland: a window left maximized opens maximized from its first frame.
- `MarkdownBlock` (`ui.markdown`'s blocks) is typed by its `t`: refine on it (`if b.t == "code"`) and its fields are typed. `zgx check` may now point at a field read without refining; nothing changes as it runs.

### Drawing and animation

- Images draw as characters (`characters`: an ASCII-art rendering in a 5 by 7 font in the shader, no glyph texture), revealed from a front measured in the window that every image shares, and dithered (`dither`, a Bayer threshold per dot) ([media](docs/reference/media.md)). The software renderer draws them too.
- Image effects compile in a pipeline of their own when an image first draws with one: an app that draws none pays nothing for them (about 5 M instructions at start and 3 K a frame, had they been in the quad shader).
- `clip_feather`: a rounded clip fades its content out over a distance inside its edge, and a clip inset rounds its inset box, so a round box stays round as it closes in.
- [`agents_intro`](crates/zgx-desktop/examples/agents_intro) is a first-launch intro made of them: a picture materialising out of characters over the desktop, cards flying in. Its agents are placed in percentages of the scene.
- Windows (`WindowOptions`): `level` (always on top), `shadow` and `covers_display`.
- anime: a delayed tween of a property another animation holds takes it over when it starts, from where it was left, rather than cancelling it when made.
- Canvases pan and zoom without drawing again (`View::canvas_view`; `view = state` in scripts), over a grid of dots or lines drawn in Rust (`View::grid_fill`, `bg_grid`). `ui.canvas { each = state, draw_item = … }` draws a list's items one by one, again only those written.
- An image shows a `Bitmap` (`src = bitmap`, a state or function of one), and images go through the clipboard (`Platform::clipboard_image`, `set_clipboard_image`; `ui.clipboard.read_image` and `write_image` in scripts, off the UI thread), on Wayland too.
- **Scrolling moves content by whole device pixels**, as Safari's and AppKit's do: box edges and hairlines no longer shimmer as they move, and text no longer drifts from its background by half a pixel (`Scene::set_pixel_scale`).

### Drag and drop

- **A drag resting on a destination springs it open** (`DragPhase::Spring`; `on_drag_spring` in scripts, `false` to decline) after the system's spring-loading delay (macOS's setting, 0.5 s elsewhere), or at Space, once a visit, as Finder's folders and Safari's tabs do.
- A drag goes on when its source is hidden or removed (a tab switched away from), as in browsers and AppKit; a source disabled or under a modal still cancels it.
- `drag_preview_anchored` hangs a preview by a fraction of its own size (below the pointer, say). `Ui::snapshot` (`el:snapshot`, `term:snapshot`) pictures a node from the window's last frame, for a tab's preview of its panes (macOS).

### Scripting

- `ui.markdown_view` draws markdown (paragraphs, headings, lists, quotes, tables, code blocks with a Copy button), selectable, in the theme's colours: as its text changes it builds only the blocks that did, and a reply streaming in grows its last block in place.
- `ui.resource(load, { initial })` loads data in a task as states (`data`, `loading`, `error`, `status`, `reload`), again when what it read changes; `ui.form { fields, validate, submit }` checks fields as they are typed and controls bind with `field = form.fields.email`; `ui.try` gives a value or its error as text; `ui.task` returns a `Task` that cancels.
- `provide = { name = value }` gives a subtree values that `ui.use_context(name, default)` reads, as React's context does.
- `ui.list` takes `data` (rows built from their items, as `ui.each`'s), `key` (rows follow their items as the list changes), `measured = true` (wrapped rows measured as they mount) with `follow` (a chat resting at its end), and its element scrolls to a row (`el:scroll_to_row(i, { align })`; `NodeHandle::scroll_to_row` in Rust).
- `ui.text { count }` takes a state as its text, followed in Rust. `ui.button { variant = "primary" }` (`"ghost"`, `"danger"`). `role = "article"` names a message or post.
- A state path picks a list's item by its fields (`{ "c1", { id = "m5" }, "text" }`), and `state:append(path, text)` adds text there without copying it into the script. `ui.sorted`'s `filter` takes fields' tests, read in Rust (`{ sector = "Tech", symbol = { contains = "AB" } }`).
- `ui.fetch` hears a response as it arrives (`on_chunk`, and `on_event` for server-sent events), and a cancelled fetch stops; a host streams with `ScriptHost::streaming_fetcher`.
- Copying a selection over a Luau list copies its rows not mounted (their text, or `row_text`), and copied HTML keeps headings and links.
- `zgx lint`: `canvas-view` (a canvas drawing again to pan or zoom), `list-measured` (wrapped text in a list of fixed rows) and `draw-item-state` (a `draw_item` reading state).
- [The chat example](crates/zgx-script/examples/chat) streams replies as markdown; [the team example](crates/zgx-script/examples/team) loads a list, edits it in a modal form and confirms a removal with the above.

### Devtools for agents

- **`zgx inspect`** drives a running app (`zgx-run --inspect`, or `ZGX_INSPECT_SOCKET=1`) over a socket only its user can open, text by default and `--json`: `tree`, `node` (with the script line that made it), `find`, `state` and `set`, `errors`, `lint`, `frames` (each frame's time by phase, the states written, what ran and why), `snapshot` (a node as the window last drew it, as a PNG), and `click`, `type`, `key`, `scroll`, `command`, `wait`. `record` keeps the gestures a window receives (a trackpad's fingers and momentum) as a text tape, and `replay` plays one back through the window's own input path, answering with its frames: how many, the time between them, those shown late. [Read once](docs/agents/inspect.md). It costs nothing unless built with zgx-script's `inspect` feature.
- `zgx inspect mcp` serves them as MCP tools on stdio (`zgx_tree` to `zgx_record` and `zgx_replay`): `claude mcp add zgx -- zgx inspect mcp` adds them.
- `zgx_script::driver` is their vocabulary for tests too: targets (`#id`, `"Send"`, `role:button`, `@12`, `#list row 3`, `a >> b`, `[2]`), actions and a compact tree; `TestApp` takes targets where it took ids (a bare id means what it did) and gains `target`, `tree` and `keys`.
- `zgx_run::serve_commands(&host, cx)` gives a Rust host making its own window its script's commands as the menu bar and their shortcuts.

### Controls and platforms

- A focused editor's caret blinks at the system's rate, stopping after its timeout (`Ui::set_caret_blink`), and is as thick as Windows' text cursor setting.
- Windows: a second press doubles a click only inside the system's double-click box; a drag starts past the system's threshold; a window's only tab moves the window; copied lines end in CR LF; a closed window gives back its icons' GDI objects; UI Automation raises only the events a client listens for.
- Linux: a title bar's double click does what GNOME's setting says; on X11 the session's XSETTINGS interface font comes first, as GTK takes it.
- Assistive technology hears geometry alone change at most every 100 ms.

### Performance

- Text shapes a variable font's advances and GPOS deltas once for its instance (`zgx-harfrust`, a fork published from this release): heavy with the system font −9%, then −5%. Kept frames patch boxes straight into their places, and retained layout skips unchanged children.
- `ui.sorted` and list states: the dashboard's filter keystroke 88 → 48 M instructions, its feed tick 43.9 → 23.5 M; mail's archive 26.3 → 13.1 M; a find-all keystroke 13.0 → 7.9 M; a streamed chat token 926 → 597 K. Sketch's pan step 26.5 → 3.1 M. Select-all over a long document 225 → 2 ms.
- macOS: the window server spends 3–4 points of a core less on a window's frames, and frames at the display's full rate present as its tiles too, with no video layer or CoreMedia thread, one ready early waiting for the refresh the others land on; a start is 7 M instructions sooner. Windows: shaders FXC compiles are kept between launches (first text 330 → 278 ms) and a frame's wake takes one message round.
- Damage is no longer drawn through a stencil of it (0.1.0's line): windows had stopped using it, and headless frames draw their regions whole again.

## 0.1.0 (2026-10-25)

<!-- rename-to-zgx: off (the old names, on purpose) -->
### Renamed: zgui is zgx

**Every crate, binary and name is zgx's:** `zgui` is `zgx`, `zgui-desktop` `zgx-desktop`, and so on for `-gpu`, `-script`, `-components`, `-run` (the crate and its binary) and `-ghostty-sys`. The `zgui` command (`lint`, `check`, …) is `zgx`, Luau's types are `zgx.d.luau`, the built-in themes `zgx-dark` and `zgx-light`, and the environment variables `ZGX_*`. The repository is github.com/zeronsh/zgx (the old address redirects).
- *Migrate:* depend on `zgx`, `zgx-desktop`, … (`zgx = { git = "https://github.com/zeronsh/zgx" }`) and write `zgx::` for `zgui::`; run `zgx lint` and `zgx check`; point luau-lsp at `crates/zgx-script/types/zgx.d.luau`; rename a script's `zgui.json` to `zgx.json` and `ZGUI_*` variables to `ZGX_*`. `scripts/rename-to-zgx.sh` does all of this to a checkout of zgx or a branch begun before the rename.
- *Still read for this release:* each `ZGUI_*` variable where its `ZGX_*` one is unset, and a `zgui.json` where there is no `zgx.json` (each with a warning, once); the theme names `zgui-dark` and `zgui-light`; a page's `window.zgui`.
- *Not carried over:* the window frames zgx keeps for an app (Linux and Windows) are kept under zgx's name now, so windows open at their default size once; the Ghostty build cache (`~/Library/Caches/zgx-ghostty`) builds once more.
<!-- rename-to-zgx: on -->

### An app needs no `[patch]` section

**zgx's forks are crates of their own:** cosmic-text, taffy and mlua, which zgx carries fixes to, are `zgx-cosmic-text`, `zgx-taffy` and `zgx-mlua`, which zgx's crates depend on (under the upstream names: code still reads `cosmic_text::`). An app no longer patches them, and builds against crates.io's winit, wgpu-hal and AccessKit adapters where zgx's repository patches those (Wayland popups then open in their window; [releasing](docs/releasing.md) lists what else differs).
- *Migrate:* delete the `[patch.crates-io]` lines for `cosmic-text`, `taffy` and `mlua` (they no longer apply). Keep those for winit, wgpu-hal and AccessKit to have their fixes ([getting started](docs/getting-started.md)), or drop them.

### Changed defaults: what to check in an existing app

**Children stretch across their container**, as in CSS's flexbox (`align-items: stretch`). A `column`'s children are as wide as it, a `row`'s as tall, unless they size themselves on that axis. `div` and zgx-script's `ui.column`, `ui.row` and `ui.div` do the same.
- *Migrate:* where buttons, chips or labels now span a column, or boxes in a row are taller than their content, state `.items_start()` (`items = "start"`) on the container, or give the child a size.

**Children shrink to fit**, as CSS's `flex-shrink: 1`, each no further than its content's narrowest (`min-width: auto`: a text's longest word, a box's content). A box that clips (a scroll view, `overflow_hidden`) may shrink to nothing. Before, nothing shrank and a too-wide row overflowed.
- *Migrate:* where a fixed-size box (a sidebar, an icon, a 120-point tile) now narrows in a small window, state `.shrink_0()` (`shrink = 0`). Where a truncated label or a pane should shrink past its content, state `.min_w(0.)` (`min_w = 0`), as CSS needs `min-width: 0`.

**`cmd` in a script's shortcuts is Control off macOS** (`ui.command { key = "cmd+n" }`): Command on macOS, Control on Windows and Linux, as each platform's menus use them. `super` and `meta` name the key itself.
- *Migrate:* nothing for menus written for macOS; a shortcut that meant the Windows key writes `super`.

**A window runs the way the user's language does:** right to left under Arabic, Hebrew, Persian or Urdu.
- *Migrate:* an app whose interface is not translated can set `.direction(Direction::Ltr)` (`dir = "ltr"`) on its root to keep it left to right. `ZGX_DIRECTION=rtl` tries a layout either way.

**Focus moves on when what has it leaves** (a toolbar's button unmounting with its message): to the nearest focusable ancestor, else the next Tab stop, else the one before, rather than to the window.

**Escape in a text field** with no composition to cancel goes to the dialog or menu around it, which closes.

**An image, icon, canvas or native surface keeps the size it sets in a flex line too tight for it**, as CSS's `<img>` and `<canvas>` (replaced elements, `Scene::set_replaced`): it overflows its box, centred, where it was squashed (a 15-point icon in a 28-point button with padding drew 4 wide).
- *Migrate:* where artwork was meant to shrink with its line, give it a percentage or `min_w(0.)` (`min_w = 0`).

**`EditorHandle::value` is a method**, `value()`, returning an `EditorValue`: the editor's `Signal<String>`, or a `Signal<TextValue>` for a long document (below); `get` and `set` read and write it as a string either way.
- *Migrate:* `handle.value.get()` is `handle.value().get()`; a match on the signal itself matches `EditorValue::String(signal)`.

**A template's dotted name reads a state's field:** `"{user.name}"`, `"{rows.1}"`; a Luau path written in braces (`"{state.status}"`) now names state `state`'s field `status` rather than showing its braces.
- *Migrate:* text meant literally takes `literal = true`, or keeps its braces out of the template.

**An app id that is not one plain name has no directory:** `ui.persist` saves nothing for an id of `..`, `a/b` or an absolute path (it saved outside the app's own).

**A theme file nests at most 128 deep** (an array or object deeper is an error): one of 100,000 brackets overflowed the stack.

**A Linux title bar's `window_drag` moves its window once the pointer has moved 4 points**, as GTK's and Chromium's do: a press there no longer gives the window manager the pointer, so a double click maximizes.

**`zgx-run` allocates with mimalloc** (`--no-default-features` keeps the system's); a Rust app opts in with `zgx-desktop`'s `mimalloc` feature. **A debug build gives scripts ten times the time budget**, as a debug build runs that much slower.

**The component library is a crate of its own, `zgx-components`.** The data grid, tree, calendar, date picker and time field, combobox, select, tabs, toasts, command palette, segmented control, stepper, disclosure, accordion and separator moved out of `zgx::compose`; the primitives they are made of (text, editors, buttons, checkboxes, sliders, progress, scroll views and virtual lists, menus, popovers, modals and tooltips, selection, drag) stay. zgx-script's `ui.*` for them is unchanged, through its `components` feature (on by default).
- *Migrate:* add `zgx-components` beside `zgx` and import them from it (`use zgx_components::prelude::*;` beside `zgx::compose::prelude::*`). The local-time hook is `zgx::local_time::set_offset` (zgx-desktop sets it). A component's own semantics are set with the new `NodeHandle::update_semantics`.

### Layout

- CSS's automatic minimum (`min-width`/`min-height: auto`) for flex items; text's min-content is its longest word.
- Right to left (`View::rtl`, `direction`; `dir` in scripts): rows run from the right, columns and text start at the right, wrapping rows mirror, scroll views start at their right and scrollbars sit at the start edge. Padding, margins and insets given as left and right stay physical. Logical edges follow the direction: `ps`/`pe`, `ms`/`me`, `start`/`end` (`inset_start`/`inset_end` in scripts).
- Flexed sizes snap to a 1/256-point grid, so shared-out room no longer drifts.
- Container queries in scripts: a ref's `width` and `height` are followed, settling in the frame they change.

### Text and editing

- Display text is selectable on opt-in (`selectable`): across nodes, by word and paragraph, with Shift and the arrows, copied as text and HTML, styled as CSS's `::selection`.
- Editors: spellchecking as you type (`spellcheck`, with the system's checker on each platform), a context menu, undo in runs, the primary selection on Linux, and arrows that move through right-to-left text the way it shows.
- IME: the clause being converted is underlined and takes the candidate window. Known gap: reconversion is not offered (winit's text input client reports no surrounding text).
- Long documents: `zgx::text::Text` holds a text in chunks (a clone is a pointer, an edit remakes the chunks it touches), and `TextValue` (a `String` or a `Text`) binds an editor without copying (`text_input_value`, `text_area_value`). An editor over 64 KiB keeps its text so, and lays out only the lines in view: a file of 100,000 lines opens in about 20 ms, a keystroke costs about a millisecond. In scripts a string over 64 KiB in state is held the same way and reads as a string.
- Luau text areas take a highlighter (`highlight`, a function of a line and what the line before left), `decorations`, `current_line`, `tab`, `on_caret` and `find` (`{ query, case, current, limit, color, current_color }`: a search's matches marked from the text it is bound to); their element selects in bytes (`set_selection_bytes`, `selection_bytes`) and types over the selection in one undo step (`replace_selection`). [The code editor example](crates/zgx-script/examples/code_editor) uses them all.
- A gutter's line numbers align to its end in any font (they were padded with figure spaces, which some fonts lack).
- With no font installed (a minimal system, a container), an app draws everything but its text, and takes typing; it drew nothing.

### Accessibility

- Roles, headings with levels, selection, live regions, descriptions and hidden decoration (`role`, `heading`, `selected`, `live`, `description`, `aria_hidden`). Labels can follow state.
- Focus groups (`focus_group`; listboxes, toolbars and tab lists without asking) are one Tab stop with arrow keys. A keyboard-navigable list is one stop (`ui.list { keyboard = true }`).
- VoiceOver finds headings (accesskit_macos is vendored with `AXHeading`). Narrator moves between them by level. Empty text leaves the tree.
- VoiceOver's table navigation finds a grid's rows (the data grid's, a calendar's), through any containers between them (vendored AccessKit gave a grid none).
- A canvas with a `label` (a string, state or function, followed) is an image named by it: a chart's summary ("Price, now 143.9, high 150.1"). Without one it is decoration, out of the tree.
- A name or value given as a function of state (`label = function`, `accessible_label_when`, `value_when`) is worked out only while assistive technology reads the window (`Ui::set_assistive_technology`; a desktop window turns it on when a reader first asks for its tree): with no screen reader on, it costs nothing as the state it reads changes.
- A long document reads a page at a time: the lines in view and a page each side (or around the caret), a text run a line, as browsers' code editors give one.
- `scripts/macos_accessibility_probe.py` reads the component catalog and the dashboard as VoiceOver gets them, from live windows.

### Drawing

- Paths know their shape when made as one (`Path::rounded_rect`, `Path::ellipse`, `Path::line`, `Path::rectangle`, a builder's `rounded_rect` and `ellipse`; `ui.path():rect(…)` and `:ellipse(…)`, `c:rect`, `c:circle`, `c:line` in scripts): `Path::shape` says which.
- A canvas of solid rectangles, circles, axis lines and strokes of lines running one way across (a sparkline's, a chart's) is drawn by the GPU as those shapes, nothing rasterized or uploaded: Sketch's pan or zoom step went from 29 ms to 1.5 ms, the dashboard's sparklines from 24 M instructions a tick to almost none. Anything else in a canvas rasterizes as before ([media](docs/reference/media.md#canvas)).

### Controls and platforms

- A component library, `zgx-components`: a data grid (virtual rows, sorting, moving and resizing columns, cell editing, row selection, copying as a spreadsheet's), a tree view (lazy children), a calendar, a date picker and a time field, a combobox (async, virtual suggestions), a select with typeahead, tabs (closable, reorderable, overflowing), toasts, a command palette, a segmented control, a stepper, a disclosure, an accordion and a separator, each keyboard complete, accessible and right-to-left aware.

- Checkboxes drawn as the platform draws them, with a mixed state and a disabled look.
- zgx's own tooltips (`View::tooltip`), and scrollbars that hide at rest where the system's do.
- Touch and pen: a finger taps, holds, drags and pans; a long press on text selects its word.
- macOS: rubber-band overscroll; Reduce motion, Increase contrast and Reduce transparency reach the themes; popups reach past their window.
- Windows: Inset and Hidden title bars with snap layouts, Acrylic and Mica glass, WebView2 web views, the system's text size and accent, one instance per application ID, and [shipping notes](docs/windows.md#shipping-a-zgx-app-on-windows).
- Linux: the desktop's dark or light, high contrast, reduced motion, text size, accent and interface font; KDE's wheel speed; app IDs on Wayland and X11; zenity file dialogs without a portal.
- Wayland: a menu taller than the output is cut to it and scrolls; a popover the compositor flipped points its arrow at its trigger.
- A drag preview shown in a window of its own blurs what is behind it, without a menu's shadow round it.

### Scripting

- `zgx-run` runs a Luau app (`app.luau`, and a `zgx.json` for its window), and `ui.window` declares more windows.
- `zgx lint` finds slow or wrong patterns, and runs on every reload in debug builds.
- Long list states are cheap to filter: a filter's rows come back as the state's own.
- A `ui.computed` read in a handler or `ui.batch`, right after a write it depends on, has the new value (it had its old one until the handler returned).
- `ui.sorted(list, { by, descending, filter })` is a list state's items kept in order as rows are written: a write re-places the rows it wrote, not all of them (the dashboard's feed tick 253 → 85 M instructions). `zgx lint`'s `computed-sort` points at a computed sorting a whole list.
- A list state reads one item (`state:at(i)`) and searches the string it holds where it is held (`state:find(query, { case, limit })`, `state:count`), without copying into the script.
- `ui.args` is what the app was launched with after its script, and `ui.on_open` is handed the files and URLs the system opens it with (Open With, links, a second launch; with an `application_id` in `zgx.json`); `ScriptHost::args` and `ScriptHost::open` from Rust.
- Plugins: a plugin's files load as source only (never bytecode), make no request or window they were not granted, and open only web links; what their states hold counts against their memory limit.

### Performance

- Mounting allocates a third as much. A column taller than its window no longer measures every row twice.
- Text that changes every frame shapes and allocates less. Idle Linux apps' D-Bus threads sleep.
- Damage scattered over a window is drawn through a stencil of it (macOS): the GPU shades what changed, not the regions around it.
- A script runs with no interrupt hook until a call's budget or the memory limit needs one (38 instructions an iteration of every loop, before). `ui.each` without a key costs 15% less an update.
- Removing a large subtree leaves the scene at once and frees its nodes over the following frames (the frame removing 90,000 nodes takes 3.3 ms).
- On macOS the GPU device is made beside AppKit's launch: a window shows 15-23 ms sooner.
- An app can be built with profile-guided optimization trained on zgx's scenes (`scripts/perf/pgo.py`, [getting started](docs/getting-started.md)); `ZGX_PROFILE=1` splits frames by phase and a start by milestone.
