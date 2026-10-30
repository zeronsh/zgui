# Changes

zgx is on crates.io from 0.1.0 (`zgx = "0.1"`); this lists what changed for apps, newest first.

## Unreleased

### Added

- **`zgx run <app>`** (zgx-script) opens a Luau app in a window as `zgx-run` does, by the zgx-run beside `zgx` or on PATH (saying how to install it when neither is there), so one command runs, checks, lints, audits and tests an app. `zgx audit` finds it the same way.
- **Tests in Luau** (zgx-script): `zgx test` runs an app's `*.test.luau` files, each test against the app mounted headless. A file gets three globals: `test(name, body)`, `mount()`, and `expect(v)` with `to_be`, `to_equal`, `to_contain`, `to_be_truthy`, `to_be_nil` and their `never` forms. The app is driven by devtools' targets (`#id`, `"label"`, `role:button`, `#list row 3`): click, type, fill, key, scroll, command, text, has, count, wait, state and set_state. A failure names its line, and `zgx check` knows the globals' types in test files alone. `TestApp::try_new` mounts or answers the script's error ([testing an app](docs/scripting.md#testing-an-app)).
- **Skinned meshes in 3D scenes**: `Geometry::with_skin(joints, weights)` gives a mesh's vertices up to four joints each, and `World::set_skin(id, Some(skin))` binds an object showing it to a `Skin::new(joints, inverse_binds)` (objects as joints, up to `MAX_JOINTS`, 256): the GPU places each vertex by its joints' matrices, so a clip moving the joints bends the mesh. glTF models bring their skins (`JOINTS_0`, `WEIGHTS_0`); its shadow and picking follow the pose. A scene without skins binds nothing new and costs what it did (the scene3d demo level, 23 driver blocks); CesiumMan walking with its texture: 25. Morph targets are not read yet.
- **glTF animations**: `Model::clips` (and `clip(name)`) reads a model's animations: channels moving its nodes' translation, rotation and scale, by STEP, LINEAR (rotations along the shorter arc) or CUBICSPLINE keys. `World::place_model` adds a model as `add_model` does and keeps its nodes' objects; `Placed::pose(world, clip, seconds)` poses a clip at any time. In Luau, `ui.model { animation = "Walk" }` (or its place, from 1) plays one round and round, posed in Rust a frame at a time. The `model` example plays a file's first clip.
- **A trackpad's swipe between pages goes back and forward** (macOS): two fingers across (or three, as the system's Trackpad setting has it), as Safari does, over a view with `on_navigate` (zgx-script) or `View::on_navigate` (Rust), which hears `"back"` or `"forward"`. A two-finger swipe navigates only where it starts where nothing under the fingers scrolls that way, so a carousel mid-content still scrolls; AppKit tracks it from there. With nothing that navigates under it, it scrolls and stretches as before ([events](docs/reference/events.md)).
- **Live connections** (zgx-script): `ui.socket(url, { json, reconnect, on_message, on_open, on_close, max_buffered })`, as React's `useWebSocket`, keeps a connection open through the host's `ScriptHost::socket_connector`. It has `status` (`"connecting"`, `"open"`, `"reconnecting"`, `"closed"`), `attempt`, `retry_in` and `buffered` as states, `error` as a `UiError`, and `send(v)` (text, or a table as JSON), `send_bytes(s)`, `close()` and `reconnect()`. What is sent before it opens waits and goes in order. It opens again after a growing wait when its connection ends, and again at once when a state its URL function reads changes. `ui.sleep(ms)` is a promise to await in a task ([state](docs/reference/state.md#live-connections)).
- **Files dropped from outside the window, and uploads** (zgx-script, zgx-run): `on_file_drop = function(files, e)` and `on_file_hover = function(over)` hear files dragged in from Finder, Explorer or a file manager (react-dropzone's `onDrop` and `isDragActive`). A `File` now has `size` and `type` (its media type) as the web's does. `ui.fetch(url, { file = f })` sends a file as the body, `multipart = { … }` sends a form's text and files as `multipart/form-data`, both read off the window's thread, and `on_progress(sent, total)` hears the body go out. zgx-run's fetcher tells it as it writes, and a host's streaming fetcher does with `Body::sent` ([state](docs/reference/state.md#the-clipboard-and-files), [events](docs/reference/events.md)).
- **Suggestions at the caret** (zgx, zgx-script): `ui.popover { at_caret = true, anchor = ui.textarea { … } }` (`View::at_caret`) places a popover at the caret of the field in its anchor, as a mention's or a slash command's suggestions are. It follows the caret as text is typed, a line wraps, the field scrolls and an input method composes. It takes no focus, so the field's keys move through the suggestions, and `active_descendant = id` (`View::active_descendant`, HTML's `aria-activedescendant`) tells assistive technology the one it is on ([overlays](docs/reference/overlays.md#suggestions-at-the-caret)).

## 0.1.6 (2026-10-29)

A patch: Rust and Luau code builds as before (every API change since 0.1.5 is an addition, and zgx-desktop and zgx-gpu 0.1.5 build against this zgx). zgx-gpu and zgx-desktop now need `zgx-cosmic-text` 0.19.3, published with it. One behaviour changes, below with what to check: Windows input methods go through the Text Services Framework by default.

### Changed: what to check in an existing app

- **Windows: input methods go through the Text Services Framework by default**, not IMM32. Editors give input methods and text services (the emoji panel, dictation, the touch keyboard) a text store: they read the text around the caret, place candidates at the clause they convert, reconvert committed text in place and follow the editor's input mode, and each clause is underlined. To keep IMM32, an app calls `Application::text_services(false)`, and a user sets `ZGX_TSF=0` (`ZGX_TSF=1` forces the framework on); a window where the framework does not start uses IMM32 ([windows](docs/windows.md#the-text-services-framework)).

### Added

- **An inbox example** (zgx-script): `examples/inbox`, a React-style app on `ui.router` (a modal route, deep links), `ui.paged` with a list's `on_end`, `ui.debounced` search, a reorderable `ui.each` and context, in the accessibility audit.
- **Emission and occlusion maps in 3D scenes**: `Map::Emission` colours the light a mesh gives off (multiplying its material's emission), `Map::Occlusion` dims the ambient light reaching each point (glTF's occlusion, not the sun's or a bulb's). glTF models bring theirs, and `KHR_materials_emissive_strength`. A scene without textures still binds none and costs what it did; DamagedHelmet with all five maps: 25 driver blocks, as with three.
- **Routing** (zgx-script): `ui.router { routes, scheme }` keeps where an app is: named routes with `:params` (`navigate` by name, `open` by path), back and forward with the mouse's buttons and Alt+←/→ (⌘[ and ⌘] on macOS), deep links by scheme, and `router.view` showing each route. A modal, popover or menu can be controlled (`open = function`, `on_close`), so a modal route opens with its route and goes back as it closes ([routing](docs/reference/state.md#routing)).
- **Reorderable lists** (zgx-script): `ui.each(list, row, { key, reorder = function(from, to) end })` makes rows the person drags into a new order (above or below a row, a line showing where) or moves with Alt+↑/↓, each a Tab stop told how to move and a keyboard move heard ("Moved Inbox to position 2 of 5"); it works with `animate`. `TestApp::drag` and `TestApp::scroll` drive drags and scrolling in tests ([dynamic](docs/reference/dynamic.md)).
- **Lists that load as they scroll, and debounced states** (zgx, zgx-script): `ui.list { on_end = fn, end_distance = 200 }` (`View::on_end` in Rust) calls `fn` as the list's end comes in reach, once each time and again once its rows grow, filling a short view by itself, with no script running while it scrolls; `ui.paged(load(cursor) -> { items, next })` gives `items`, `loading`, `status`, `done`, `error` (a `UiError`), `more()` and `reload()`, keyed like `ui.resource` and cancelling a load superseded; `ui.debounced(state, ms)` follows `state` once it has settled, on one timer ([state](docs/reference/state.md#pages-and-debouncing)). A script node is also 16 bytes smaller (a list's rarely set options are boxed).
- **Handlers that await, and optimistic writes** (zgx-script): `ui.async(f)` is a handler running `f` as a task each call (a save on a click, a toast's Undo), cancelled as its owner goes, an uncaught error reported once. `ui.optimistic(state, next, save)` writes `next` now, keeps it if `save` succeeds and puts back what it replaced if not, returning the error; overlapping calls never revert over a newer write ([state](docs/reference/state.md#host-calls-with-results)).
- **A 3D texture decoded from a file keeps the file, not its pixels**: `Texture::decode` holds the PNG or JPEG and its decoded pixels only until a renderer has uploaded them, decoding again only if another upload needs them (a lost device, another window's renderer). Uploads share the decoder's pixels and build mipmaps without copying the full-size level. DamagedHelmet's live heap: 64.8 → 17.2 MB.
- **A failure is a typed error, not only text** (zgx-script): `ui.try`'s error, a promise's `error`, `:after`'s `on_error` and what `:await()` raises are a `UiError` with `message`, `kind` (`"raised"`, `"action"`, `"network"`, `"cancelled"`, `"http"`, `"io"`), an HTTP failure's `status` and `body`, and a raised value kept as `value`. `ui.fetch(url, { raise = true })` rejects a status outside 2xx as an `"http"` error. It is still text: `tostring`, `..`, `{err}`, string methods, a state set to it, a toast's `title`/`body` and props read its message. Code comparing it with `==` to a string, or checking `type(err) == "string"`, should read `err.message` or `err.kind`; under strict types, a state of strings is set to `err.message` ([state](docs/reference/state.md)).
- **Forms filled from what is loaded** (zgx-script): `ui.form { values = state }` fills a form from a loaded record and follows it, a field the person has edited keeping what they typed; a form and its fields have `dirty`; `submit` and `on_success` are handed the form. Checkboxes, sliders, steppers, segmented controls and selects can be controlled as React's are (`value = function`, `on_change`). `response.json` is typed `any` and `form.reset` takes any record. `TestApp::fill(id, text)` replaces a field's text. [The settings example](crates/zgx-script/examples/settings) shows them together ([forms](docs/reference/controls.md#forms)).
- **Decoding a PNG or JPEG holds about the image once, not twice**: `zgx_gpu::assets::decode_image` decodes into the image's final buffer, widening RGB and grey to RGBA in place, rather than into a buffer the image was then copied out of. A decode's peak: PNG 2.00 → 1.09 images, JPEG 2.00 → 1.36 (zune-jpeg's own colour planes). For `ui.image` and 3D textures alike; under the system allocator, fewer freed large regions kept (a 3D model's three 2048 textures: 35.6 → 23.6 MB).
- **Normal and metal-roughness maps in 3D scenes**: `World::set_map(id, Map::Normal | Map::MetalRoughness, texture)` (zgx-3d; `Map::BaseColour` is `set_texture`'s). A normal map tilts a surface by its texels (glTF's convention: +x along u, +y up the texture), its frame taken from the screen's derivatives, so meshes need no tangents. A metal-roughness map scales the material's factors (blue metal, green roughness). glTF models bring theirs, in Rust and in `ui.model`. A scene without textures still binds none and costs what it did.
- **Textures in 3D scenes**: `Texture` (zgx-3d), made from RGBA8 pixels or a PNG or JPEG, and `World::set_texture(id, texture)` colour a mesh across its texture coordinates, mipmapped and filtered. glTF models bring their base colour textures, in Rust and in `ui.model`. A scene without textures binds none and costs what it did. GPU-view painters get `zgx_gpu::viewport::texture_with_levels`: a texture written by the CPU in place on a Mac with unified memory, never copied. A copy into a texture would keep some 160 MB of Apple's driver memory while copies come ([3D scenes](docs/reference/3d.md#model)).
- **Localized messages** (`zgx::i18n`): ICU MessageFormat messages from JSON locale files, the locale negotiated from what the app asks or the system's preferred languages, and each message followed, so a locale switch or an edited file updates only the texts that changed. Plurals and numbers come behind the `i18n` feature, dates behind `i18n-dates`, both off by default (on in zgx-run). In scripts, `ui.i18n { dir = "locales", source = "en" }` loads a directory of locale files (a bundle ships them), `ui.t(key, args)` is a message as text or a label, and `ui.locale` reads and sets the locale. Every widget's label takes a message or a state and follows it: tabs, selects, modals, menus and the other components name their parts in the current locale.
- **A window's glass and light or dark change while it shows**: `WindowHandle::set_glass(Some(glass))` replaces its glass (macOS's effect view behind the content, Windows 11's system backdrop) and `set_glass(None)` removes it. `WindowHandle::set_theme(Some(appearance))` shows it light or dark whatever the system shows, title bar included, and `None` follows the system again. Both were fixed when the window was made.
- **Theme tokens as typed fields** in scripts: `ui.tokens.notes.wash` is `"$notes.wash"`, built from the applied theme, so an editor completes an app's tokens and luau-lsp checks them. `zgx lint`'s `unknown-token` rule names a token nothing declares (a misspelled `"$app.crad"` read as transparent before).
- **A key binding can name a physical key**: `KeyBinding::code("Numpad1", modifiers, action)` matches the key with that physical code (the DOM's `KeyboardEvent.code`) whatever the layout makes it, so the keypad's 1 is apart from the digit row's.
- **A select's options follow a state** (`ui.select { options = state }`, Rust `select_live`): relabelled in place in a new locale, added and removed by their value.
- **glTF models in Luau: `ui.model { src, fit }`** shows a `.glb` or `.gltf` in a 3D scene. It's read off the main thread and shows once read, `fit` scales it to that many units across, and it hears clicks and hovers as a group does. A changed file reloads the script, as an image does ([3D scenes](docs/reference/3d.md#model)). It's zgx-script's `gltf` feature, on in zgx-run (48.5 KiB of a build for shipping).
- **Read-only shared state for plugins** (zgx-script): `ScriptHost::expose_shared_read(key, capability)` lets plugins read and follow a shared key without writing it. A plugin's `set`, `update`, `push` and the rest on it, or binding it where it is written, is an error naming the key; the app's scripts write it as before ([plugins](docs/plugins.md#what-plugins-see-of-the-app)).
- **glTF models in zgx-3d** (its `gltf` feature): `Model::from_bytes` reads a `.glb`, or a `.gltf` whose buffers are inline; `Model::from_bytes_with` reads one whose buffers sit beside it. A model brings its default scene's node tree, its meshes' triangles (smooth normals made where a mesh has none) and its materials' metallic-roughness factors, blending and unlit. `World::add_model` adds it as one group, with its base colour, metal-roughness and normal textures. Emission and occlusion textures, animations and skins are not read yet. `cargo run -p zgx-3d --features gltf --example model -- FILE` shows one.
- **A plugins example and guide** (zgx-script): `examples/plugins`, a task list two plugins extend with before/after, a replacement, a filter, an `@app` module override and a shared package, its host deciding what they see; [plugins.md](docs/plugins.md) covers making an app hookable, `hook = false`, the trust model of `Override`, and `ui.shared`/`expose_action`. A `ui.show`'s or `ui.switch`'s element can now be replaced or filtered, and `zgx check` resolves `@app/…` and `@app-original/…` against the app's directory.
- **Shadows in 3D scenes**: `World::set_shadows(true)` (zgx-3d), or `shadows = true` on `ui.scene3d`, makes the sun (the first directional light) cast shadows from opaque meshes, filtered soft at their edges. Off by default, it costs nothing. On, it costs a 2048 by 2048 depth map (16 MB) and a depth pass that's drawn again only when meshes or the sun move, not the camera. In the Luau demo, orbiting: +2% GPU and +0.7% CPU ([3D scenes](docs/reference/3d.md#scene3d)).
- **Clicking and hovering 3D objects**: `World::pick(point, size)` (zgx-3d) says which mesh a point of the view's box shows, where on it and how far from the camera (`Hit`). It's found on the CPU from the meshes' triangles, so a world never picked costs nothing. In Luau, `ui.mesh` and `ui.group` take `on_click(hit)` and `on_hover(over)`; a group hears its meshes', and a drag (orbiting) clicks nothing ([3D scenes](docs/reference/3d.md#mesh)).
- **Input modes, as HTML's `inputmode`**: `View::input_mode(InputMode::Email)` (`Text`, `Decimal`, `Numeric`, `Tel`, `Search`, `Email`, `Url`, `None`), `EditorHandle::set_input_mode`, and Luau's `inputmode = "email"`, say what kind of text a field takes. It is a hint, never a check. On Windows it is the field's input scope (through the Text Services Framework, the default; not with `ZGX_TSF=0` or `text_services(false)`), which the touch keyboard and input methods follow, and `None` shows no touch keyboard. Windows input methods also reconvert committed text in place (select it, press Convert or Hanja).
- **macOS: an opaque GPU view gets a layer of its own** (`GpuView::set_opaque(true)`): where its box shows it whole and alone, the painter draws straight into a `CAMetalLayer`, and zgx no longer copies it into the window's frame. On an M5 Max the system's GPU power about halves, level with wgpu's own, for about 1–2 points more app CPU, about 4 more of the window server's (as wgpu's) and about 20 MB. It never makes the main thread wait for a drawable: a thread of the plane's own waits for its drawables. It shows every refresh at 120 Hz on an M5 Max and an M1 Max, 19 and 27 ms from a frame's start. A box moved, clipped, faded or drawn over goes back into the window's frame in the same transaction, so no frame shows the view nowhere. The extra CPU is macOS's own for presenting a `CAMetalLayer`, so it stays opt-in: views not marked opaque, and zgx-3d's, composite as before ([GPU views](docs/gpu-views.md#a-layer-of-its-own-macos)).
- **Plugin hooks** (zgx-script): a plugin adds before or after any element of the app's, or in its place, without a slot: `ui.contribute { target = "#save", after = node }`. A target is an `id`, a declared `hook = "name"`, or a role and a name as the app writes them; `private = true` takes an element out of reach. Replacing needs `Capability::Override`, `ScriptHost::expose_hook` makes a target ask a capability, and what a plugin adds is described to assistive technology as the plugin's ([plugin hooks](docs/design/plugin-hooks.md)).
- **Plugin filters** (zgx-script): `ui.filter { target = "#title", text = function(text) … end }` passes an element's text, label, hidden, disabled, tooltip, a `ui.each`'s or `ui.list`'s items or its children's order through a plugin's function, again when what it reads changes; it needs `Capability::Override` ([plugin hooks](docs/design/plugin-hooks.md#filtering)).
- **Module overrides** (zgx-script): a plugin trusted with `Capability::Override` replaces one of the modules the app requires as `@app/…`: `ui.override { module = "@app/components/card", with = "./card" }`, the replacement reaching the original as `@app-original/…`. The replacement runs as the app's own code, so the grant is a trust grant.
- **Packages** (zgx-script, zgx-run): `ScriptHost::package("kit", dir)`, or zgx.json's `"packages"`, lets an app require modules from a folder outside its directory, as `@kit/…` or through a `.luaurc` alias into it; read only, and plugins reach none. `build::bundle` brings the packages its zgx.json lists into the bundle.
- **Styled highlight spans**: a text area's highlighter can set a weight, italics, a strike and a wash behind a range, as well as its colour (`View::span_highlighter`, `EditorHandle::set_span_highlighter`, and `SpanStyle`, built as `SpanStyle::default().weight(600).italic()` so later styles stay additive; Luau's `highlight` spans take `weight`, `italic`, `strike` and `bg` by name beside `{ from, to, color }`). A line whose spans set a weight or italics is shaped with them, so its width, wrapping, carets and clicks follow what shows. Other lines are shaped as plain text, as before. Overlapping spans merge, so a code span inside bold text stays bold. This is enough for a Markdown composer's emphasis and inline code ([styled spans](docs/reference/controls.md#styled-spans)).
- **Reserved shortcuts**: a key the app reserves is its own whatever has the keyboard, before a focused web page or terminal sees it, as a browser keeps ⌘T and ⌘W from its pages: `Application::reserved_shortcuts`, `WindowFactory::set_reserved_shortcuts`, `KeyBinding::reserved()`, and Luau's `ui.command { reserved = true }` (`ScriptHost::reserved_keys`). Only the app's own commands can be reserved, not a plugin's; `zgx lint`'s `reserved-system-key` flags an editing or system key reserved ([platform services](docs/platform-services.md)).
- **Replacing a font while the app runs**: `WindowFactory::replace_font(font)` shapes the families `font` holds with it in every window from then on, in place of the font they had: their text measures, lays out and draws again ([styling](docs/styling.md)).
- **An accessibility audit**: `zgx_desktop::accessibility::audit::audit(&mut ui)` checks the tree a screen reader gets of a mounted interface, headless and with no permission, and walks Tab through it: controls, images and dialogs named, controls workable, check boxes, tabs and sliders saying their state, alerts live, field errors given to a reader, every Tab stop reached and focus coming round. `Report::to_json()` is a line for agents and CI. zgx's own CI audits the gallery and the Luau examples with it, their toasts, menus, popovers, dialogs and field errors open ([composition](docs/composition.md#accessibility)).
- **A popover or menu with no name is named by its trigger** (`aria-labelledby`), as the trigger reads now ([overlays](docs/reference/overlays.md)).
- **macOS transparency checked in the process**: `WindowHandle::debug_composition()` reports what the window server composites of a window (its opacity and background, its layers, the pixels it shows), with no screen capture; the transparency example's `--composition` mode checks it on macOS CI ([validation](docs/validation.md)).
- **An editor's own caret colour and width**: `View::caret_color` / `caret_color_when`, `View::caret_width`, `EditorHandle::set_caret_style`, and Luau's `caret_color` (a colour or `"$token"`) and `caret_width` on `ui.input` and `ui.textarea`. By default the caret still takes the text's colour and the app's width (`Ui::set_caret_width`).

### Performance

- **Scripts set up and run faster** (zgx-script): a script's VM makes only the `ui` modules, and `anime`, that it names (#956), so setting one up costs 1.53 M instructions, against 3.83 M in 0.1.5 (−60%). By exact instruction counts against 0.1.5 on Linux arm64 and x86, every script benchmark is faster, by 0.06% to 3.6%: mount −1.7 to −2.0%, the apps' interactions −0.08 to −3.13%, the Luau microbenchmarks −1.79 to −3.56%.
- **The Rust benchmarks are within ±0.04% of 0.1.5 on arm64 and ±0.09% on x86**, the rest being where the C allocator places memory; zgx's own code is flat.
- **A localized message costs about what its template twin does**: updating a text whose plural follows a count costs about 1–2% more than the same text as a Luau template string (`"{count} messages"`): +1.2% by default, +2.2% with zgx-script's `i18n` feature, which zgx-run turns on. #974 chooses the plural form by a lookup made as the message parses.

### Fixed

- **Tab moves past a split's divider**: the divider claimed every key, Tab included, so focus stayed on it in every split view.
- An alert (`Role::Alert`) is assertively live without saying so, as ARIA's is; an unnamed image inside a button, check box, tab, menu item or other control is the control's, not a bare "image" to a screen reader.
- Letter spacing in a scene whose text shaper takes no font (`Scene::set_text_shaper`) spaces that shaper's own advances and wraps them as the GPU shaper does; it measured grapheme cells instead.
- **A faded clip wholly outside a popup no longer panics the app** (zgx-gpu). Drawing a fade's bands as one, its box was clipped to what the window draws and clamped into; a box entirely outside it (seen with a folding list in an out-of-window popover) left that box inverted, `f32::clamp` panicked on every frame, and after a run of contained panics the app quit. Such a box now has no bands there and fades item by item, as it did.
- **A key a web page or terminal leaves alone reaches the app's shortcuts**: with a WKWebView, a WebView2 page or a Ghostty terminal focused, a menu item's key or a key binding the page didn't use did nothing (a terminal typed it), as a browser hands its pages' unused keys to its own menus.
- An object a page posts (`webkit.messageHandlers.zgx.postMessage({…})`) reaches the app in a debug build on macOS; it aborted it.
- A spring or tween between very far values settles (it overflowed and asked for frames for ever).
- Removing a subtree stops its effects before the destructors of what it held write state, so they no longer run once more on removed nodes; removing a large focused tree no longer panics when a listener's destructor removes another node.
- A missing image's error names the script line that asked for it.
- A press outside a popover closes it and goes on to what it lands on (a button acts, a field takes focus, a parent popover's row acts); a nested popover closes on a press in its parent's panel.
- A fade (`fade_edges`) fades what it holds as one, as CSS's `mask-image` does: stacked items no longer each fade on their own. A faded scroll copies its interior and repaints only its bands, instead of its whole view every move.
- With devtools tracing on, remounts no longer grow its log: an effect dropped takes its trace name with it.
- **A panic that repeats every frame no longer ends the app** (zgx-desktop): one in the window's frame, a popup's frame or the popup windows' update is contained in that piece, reported once (log and banner), and the piece skipped, keeping its last frame, until what it draws changes (then tried again at most once a second). Before, a contained panic that came every frame (a popup's fade bands, #950) reached the 20-a-second limit and the app quit with `panicking repeatedly`.
- **Decoded images and SVGs no longer stay for the process's life** (zgx-script). Each `src` was decoded once and kept until its file changed, so a page of forty large images left ~80 MB behind after it unmounted. What a mounted node shows is still kept. Of the rest, the most recently shown stay within a budget (48 MiB by default, `ScriptHost::image_cache(bytes)`), so a reload or a page shown again is still instant. The others go as nodes showing pictures unmount, never per frame.

## 0.1.5 (2026-10-28)

A patch: Rust and Luau code builds as before (every API change since 0.1.4 is an addition, and zgx-desktop and zgx-gpu 0.1.4 build against this zgx), and `zgx-3d` is a new crate. Two behaviours change, each below with what to check: a radio's, check box's, switch's or link's accessible name, and the button Enter or Space activates.

### Changed: what to check in an existing app

**Radios, check boxes, switches and links are named by their content**, as ARIA names them, unless they are given a name of their own (`named`, Luau `label`). Before, a radio or link built as a button with its role changed (`button().role(Role::RadioButton)`) was named by its content only as it mounted, and a later change to that content left the name behind; a screen reader read the old text, or none. Menu items are left out: their menus name them.
- *Migrate:* a test or tool reading these controls' names through assistive technology reads their current text. A control whose name should differ from what it shows sets one with `named`.

**Enter and Space activate the button they were pressed on**, as long as it is still there and focused. Before, a button whose own key handler closed it and moved focus (a dialog's button closing its dialog) activated the button focus moved to as well: at once on Enter, at release on Space.
- *Migrate:* a handler that meant a key on one button to click the next one calls it itself.

### Added

- **GPU views: an app's own wgpu drawing in a box** (`zgx::compose::gpu_view(label, &view)`, `zgx::gpu_view::GpuView`, its painter a `zgx_gpu::viewport::Viewport`): a 3D scene, a game, a decoder's output. The box is an ordinary one, so clipping, rounded corners, opacity, edge fades, masks, perspective tilts and hit testing apply. A frame is drawn when the box shows, resizes or changes scale, when the view is invalidated, and every refresh while it animates and shows; showing it repaints only its box ([GPU views](docs/gpu-views.md)).
- **A new crate, `zgx-3d`**: a retained 3D world (meshes in metallic-roughness materials, a sun, point lights and a sky/ground hemisphere, a perspective or orthographic camera, parents and visibility) shown in any box through a GPU view. A still world draws nothing; changes before a frame draw one. Cuboids, spheres, planes, cylinders, cones and tori are built in, and `Orbit` turns and dollies a camera from drags, the wheel and pinches.
- **3D scenes in Luau: `ui.scene3d`** with `ui.mesh`, `ui.light` and `ui.group`, drawn by zgx-3d in a GPU view: metallic-roughness materials, directional, point and hemisphere lights, and a camera to orbit by dragging and zoom by scrolling ([3D scenes](docs/reference/3d.md)). Each number, vector and colour may be a state, a function or a `ui.tween`/`ui.spring` binding: a still scene runs no script and draws nothing, and a spring steps in Rust. It is zgx-script's `scene3d` feature, off by default, because it links the renderer (wgpu), which a headless host (tests, tools, a server) otherwise leaves out. `zgx-run` turns it on, so apps it runs have it. A host built without it says which feature a script calling `ui.scene3d` needs.
- **Windows: input methods through the Text Services Framework**, opt-in with `ZGX_TSF=1` (IMM32 stays the default for now). Editors give Windows' input methods and text services a text store. They read the text around the caret, place candidates at the clause they convert, and insert at the caret: the emoji panel, dictation, the touch keyboard. For it, editors take an input method's clauses (`TextEditor::set_preedit_clauses`, `ClauseStyle`), each underlined, thick under the one being converted. They also take a replacement as one undo step (`TextEditor::replace_range`) and count UTF-16 offsets (`utf16_len`, `byte_to_utf16`, `utf16_to_byte`). `EditorHandle` makes these edits (`insert_text`, `replace_text`, `set_composition`, `commit_composition`, `cancel_composition`, `input_method_area`) ([windows](docs/windows.md)). The clauses come from the input method's display attributes, and an input method asking where any range shows gets its box (`EditorHandle::text_area`, `Scene::text_range_area`: across its lines, through zoom and the editor's scroll), for its candidates and prediction list.
- **Windows: UI Automation finds text** in zgx fields and documents (`ITextRangeProvider::FindText`, which AccessKit left out): Windows Speech Recognition's "select <word>", Narrator's find and UIA tools find a word forward or backward, with or without case, within the range they ask.
- **Windows: an experimental Direct3D 11 renderer**, off by default: built only with zgx-gpu's `d3d11` feature (forwarded by zgx-desktop) and taken only with `ZGX_D3D11=1`. It opens no Direct3D 12 device, so a window holds less memory and its driver rests; everything above the device draws as on Direct3D 12 ([windows](docs/windows.md)).
- **`zgx-run` reaches the network**: an app it runs has a fetcher for `ui.fetch` and remote images, over HTTP and HTTPS (ureq, TLS by rustls with Mozilla's roots, no OpenSSL). Each request runs on a thread of its own, its body reaches `on_chunk` and `on_event` as it arrives, and a cancelled fetch closes its own connection at once, even while the server is silent, while other fetches keep theirs, pooled for reuse. A read or write waiting five minutes ends a request. A host's own fetcher replaces it, and `zgx_run::install_fetcher` gives it to a host made outside `run`. Its `fetch` feature is on by default, about 1.6 MB of a release binary; `--no-default-features --features mimalloc,inspect` leaves it out.
- `View::hollow()`: the pointer passes through a box while nothing it holds is displayed (`hidden` or `invisible`). A Luau `presence` box uses it: a page kept mounted but hidden no longer takes the presses meant for what is under it (a web page, the panes).
- `Scene::window_point` and `Scene::window_rect` take a point or box from the space a node is laid out in to the window, through its own and its ancestors' transforms and zooms.
- `zgx inspect` targets and trees reach into open modals, popovers and menus, whose panels mount outside the view that owns them: `"Save"` or `role:button "Save"` in an open dialog is found, and the tree shows the dialog under its owner (`Ui::portals`).

### Changed

- macOS: an inset title bar's traffic lights move with a zoomed window's 40-point strip (centred in 40 × the zoom points, as far in as the zoom makes AppKit's inset), so a title bar laid out around them stays level at any zoom.
- macOS: frames presented as `CAMetalLayer` drawables (`ZGX_PRESENT_DRAWABLE`, or `=auto` where its trial picks them) show a refresh or two sooner. They are paced by the view's own display link and kept in phase: when frames start waiting most of a refresh for a drawable (the pool full, as most launches of an M1 Max settled), one tick is skipped and they catch up. The pool is two drawables, not three. A frame's start to its showing: 16 ms against 27–32 on an M5 Max, 24 against 27–40 (median 36) on an M1 Max. The window server's CPU on an M1 Max drops from 34.2% to 31.3%, and each window holds 10 MiB less. Tiles, the default, are unchanged.
- macOS: `ZGX_PRESENT_DRAWABLE=auto` judges by the frames each path skips again, not by energy. A process can't measure the GPU's energy, only its busy time, and what a busy second costs depends on the GPU's clock (0.78 W on an M5 Max, 4.44 on an M1 Max). The energy rule chose drawables where they measure worse. `ZGX_TRIAL_RULE=energy` keeps that rule, uncalibrated. A GPU-heavy, full-rate app on an M1-class Mac may choose drawables itself (`ZGX_PRESENT_DRAWABLE=1`): about 150 mW less GPU and 5.7 points less window-server CPU, for about 4.7 points more app CPU.
- `zgx inspect`'s layout warnings leave overflow past an edge that fades (`fade_edges`, `fade_edges_x`) alone: a strip of tabs fading at its end runs past it on purpose. Overflow past an edge without a fade still warns.

### Performance

- **CPU, against 0.1.4**, by exact instruction counts (Linux arm64, with a screen reader on for both, as 0.1.4's benchmarks ran): anime −2.57%, heavy −0.68 to −0.69% (scrolling −0.36%), micro-vscroll-variable −0.53%, mount −0.51%, animate −0.30%, widgets −0.25%, and the apps and lists level or lower.
  - **style +0.10%:** two approved trades. The `Signal::with` inlining (+0.05%; heavy −0.69%, anime −2.57%), and #835, which takes back 0.1.4's arm64-only #784 to win on x86 (style −0.49%, heavy −0.30%, animate −0.50% there). The other changes since 0.1.4 make style 0.4% faster.
  - **app-code and app-code-search +1.3%:** they now free the 5,000-line document that 0.1.4 leaked when its editor closed. That's a one-time cost of about 1.2 M instructions, the same however long the editor was used; typing and scrolling cost what they did.

### Fixed

- **A script's values can't panic or abort the app from Rust**: durations no `Duration` holds (`ui.after(1e300, f)`, a toast or tooltip delay of `math.huge`), a deadline past the clock, `anime.utils.clamp` with NaN bounds, `roundPad` and `padStart`/`padEnd` asked for absurd sizes, and anime timings that are no number. A NaN or infinite duration is a script error naming the line, a longer one waits a year (`zgx::timer::MAX_SLEEP`), and anime clamps as anime.js does (found by fuzzing).
- Windows (`ZGX_TSF=1`): the app setting an editor's text or selection while an input method composes in it ends the composition there, as the editor's preedit already ended. The text store kept the composition: ending it committed the cancelled text ("REPLACED候補"), and the input method's next write at the selection panicked.
- `ScriptHost::on_error` hears an error raised again with the same message; it heard only the first.
- A script's error banner shows over an open modal or popover and dismisses with its ×. Under a modal, the click went to the modal's backdrop, which closed the modal and left the error.
- **A Luau app starts with Luau 744's bytecode** (luau0-src 0.22.1, which compiles to bytecode version 13): mlua-sys took versions up to 11, read the rest as text and refused it (`attempt to load a text chunk`). zgx-mlua 0.12.5 checks bytecode by the versions Luau 0.22 loads, and the `luau0-src = "=0.22.0"` pin goes.
- A list or a heading that is itself the selectable root (`column().role(Role::List).selectable(true)`, `text(..).heading(2).selectable(true)`) copies as HTML as a list (`<ol start="3">`, `<ul>`) or a heading (`<h2>`), as it does inside a selectable box. It copied as a paragraph.
- A list that follows its end (`VariableHeights::anchor_end`, Luau `ui.list { follow = true }` with `measured` or `heights`) follows its first rows after being empty: a transcript with no messages yet, or one cleared and filled again, stayed at its top.
- `on_event` hears the first event of a stream that starts with a byte order mark, which it dropped as an unknown field. `on_chunk` still gets the mark.
- `on_chunk` text keeps a character split across two pieces of a body when an invalid byte comes before it. It turned the character into U+FFFDs, where decoding the whole body keeps it.
- A grid item's span or start too large for grid lines (a `column_span` of 32768, or a start of 32767 spanning 2) no longer panics in layout. Lines reach ±10,000, as in Firefox.
- A text of no words, such as `"\n"`, no longer panics when it is measured at a new width after the text prepared just before it was dropped. Its measurement pointed past the end of its storage.
- A child given an aspect ratio in a grid, its parent then made flex, lays out as flex sizes it. It kept the height the grid gave it.
- An editor whose line is taller than the box inside its padding (large type in a short field) shows its composition's underline and its misspelling marks. Both were placed under the line, past the field's clip, and left out.
- macOS: files dragged in from Finder hover and drop where they are. The drop went to wherever the pointer was last seen before the drag came in, because a drag holding the pointer sends the window no moves. A box taking files only got them if the pointer had last been over it. The window now hears where the files are as they enter, move and drop, and that they left (`vendor/winit/ZGX_PATCH.md`).
- A zoomed box changed after its first layout lays out as one built that way from the start. Resized, it moves the boxes after it, which kept their old places before. A `snap` turned on or off applies. Made `contents` (or zoomed once it is), it takes its children's size in its own points, which it scaled by its zoom a second time before. Its parent changed (an alignment, say), it keeps its size, which was divided by its zoom again. Zoomed as a `contents` node's last child, it gives that node the room it now takes.
- `contents` nodes nested a thousand deep no longer overflow the stack when a node is added at the bottom. Each level passed the style up by calling itself, and that ended a 2 MiB thread.
- A pointer move whose hover listener removes the node under the pointer (a tooltip or hover card hiding itself) no longer panics with "stale scene node": the move is routed to what is still there, and a capture whose owner went is released.
- A hover listener that moves the pointer, or opens what is under it, leaves no stale enter or leave: a node told Enter is told Leave once the pointer has left it, and none is told either twice, whatever a listener dispatches meanwhile.
- A window's new pixel scale (a move to another display, a DPI change) places scroll content from the offset asked for: it was rounded again from the offset the old scale had rounded (0.4 at 1× showed 0 at 2×, not 0.5).
- Text geometry given to assistive technology, Look Up and input methods is where zoomed text shows: lines, carets, baselines and the input method's caret and clause were unzoomed, so a screen reader's highlight and IMM32's and TSF's candidates sat off the text.
- A scroll copy repaints where it carries an item over or under its view that was repainted where it is (a translucent veil over a list left a second veil up the view as far as it scrolled), and a layer item (a turned card, an opacity group) is measured by its whole subtree's ink as it shows, turned, not by its layout box.
- Glass (a backdrop blur) scaled or turned over a native view (a web view, a terminal) keeps its platform backdrop; it showed its own pixels unblurred until the transform ended.
- macOS: a popup whose panel paints a shadow of its own gets none from the window server, which outlined it a second time (on macOS 26, a dark rim round it). zgx's own menus and popovers, which paint none, keep the window server's.
- X11: a window closing, or an app ending, no longer hangs on an input method server that is running but answers nothing (`vendor/winit/ZGX_PATCH.md`).
- A damage grid for a viewport too large for one (`Scene::new(f32::MAX, f32::MAX)`) no longer panics on its first flush; its damage joins into one rectangle.
- What goes is let go of:
  - a long editor's lines' layout and text shaper, which held each other once the editor went;
  - a renderer's blend backdrops for sizes no target has any more (a window resized again and again kept one full-window texture a size), and, at `trim`, fill and layer shaders no one holds any more (a reloaded script's), with their pipelines.

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
