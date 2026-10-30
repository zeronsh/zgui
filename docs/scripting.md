# Scripting zgx with Luau

Coming from React? [Coming from React](coming-from-react.md) maps its patterns (state, effects, context, lists, data loading, forms, modals) to these, with the performance rules.

`zgx-script` builds interfaces from [Luau](https://luau.org) scripts. A script runs once per load and returns a description of components, state bindings and animations; Rust lowers it to ordinary retained views. Layout, drawing, state and animation all stay in Rust. The script is called again only for event handlers, components as they mount, and properties it explicitly computes with a function. Text templates (`"Count: {count}"`) and every animation run without calling the script.

```lua
local count = ui.state("count", 0)
return ui.column {
	p = 24,
	gap = 12,
	bg = "#172030",
	ui.text { "Count: {count}", text_size = 18 },
	ui.button { on_click = ui.add(count), px = 16, py = 10, "Increment" },
}
```

```rust
let host = ScriptHost::new(&cx.ui.runtime, "app.luau").plugins("plugins").app_id("my-app");
host.action("save", |arg| { /* Rust, called by ui.action("save", arg) */ });
let _ = host.reload();                                // errors show over the window
cx.tasks.spawn(host.watch(Duration::from_millis(250))); // hot reload in development
cx.render(host.view());
```

Or run a script with no host of your own: `cargo run -p zgx-run -- path/to/app/` (`zgx run app/` or `zgx-run app/`, once installed with `cargo install --path crates/zgx-run`: `zgx run` hands the app to the zgx-run beside it or on PATH) opens it in a window and reloads it as its files change. A `zgx.json` beside the script sets the window (`title`, `width`, `height`, `min_width`, `min_height`, `titlebar`: `"standard"`, `"inset"` or `"hidden"`, `glass`) and `app_id` (where `ui.persist` keeps its data, and on Linux its windows' desktop file ID, `app_id.desktop`, which docks match them to), `application_id` (reverse-DNS: the system then hands the app the files and URLs it opens, `ui.on_open`), and `packages` (`{ "kit": "../shared/kit" }`: module folders outside the script's directory it may require, below); what follows the script on its command line is its `ui.args`, and the files among them open with it; `ui.command` entries are its menu bar and `ui.window` declarations its other windows. Its `ui.fetch` (and remote images) reach the network through zgx-run's fetcher: HTTP and HTTPS (rustls, with Mozilla's roots), each request on a thread of its own, its body streamed to `on_chunk` and `on_event` as it arrives. A cancelled fetch closes its connection at once, even while the server is silent, and a read or write waiting five minutes ends the request. Connections are kept for reuse between fetches, and a cancel closes only its own. A host that brings its own transport replaces it (`ScriptHost::fetcher` in `zgx_run::run_hosting`'s setup), and `zgx_run::install_fetcher` gives it to a host made elsewhere; zgx-run's `fetch` feature (on by default, about 1.6 MB of a release binary) leaves it and its dependencies out. Its `ui.socket` connects through zgx-run's connector: WebSockets, `ws://` and `wss://` (the same rustls and roots), each connection on a thread of its own that sleeps until the network or the script has something for it, so an open, quiet connection costs nothing; what the script sends is taken as fast as the connection writes it, and the rest waits in the script's `buffered`. `zgx_run::install_connector` gives it to a host made elsewhere, `ScriptHost::socket_connector` replaces it, and the `socket` feature (on by default, about 170 KB of a build for shipping) leaves it out. The mail example (`examples/mail`) runs this way.

State is Rust-owned and keyed, so a reload keeps it, as React's Fast Refresh keeps a component's state: a save shows new code with the values you had, not the ones your code starts from. So changing a state's initial value (`ui.state("messages", sample(4000))` to `sample(100)`) does not apply to a running app; change its key (`ui.state("messages_v2", …)`), which makes it a new state from its initial value, or restart. `ui.persist` keeps its value across launches the same way. Each script gets its own sandboxed VM (read-only globals, a heap limit, 1 GiB for the app's own scripts, which hold its data, and 32 MiB for a plugin's, its state's values (which Rust holds) counted with its VM, and a time budget per call, 50 ms of the CPU time the script uses, so a busy machine does not fail it, and ten times that in a debug build, which runs scripts several times slower; it counts from a CPU-time reading up to 1 ms old, so a script may be stopped up to 1 ms short of its budget, never past it), which makes plugins safe to load next to the application. A host changes them with `ScriptHost::limits`; a script that reaches one says which. A failed reload keeps the last good interface running and shows the error, with file and line, until a good save.

The [Luau reference](reference/index.md) has a page per element and API group: signatures, every option with its type and default, an example and the gotchas. [App structure](reference/app-structure.md) shows how to split a large app across modules that share its state.

Examples: `hot_reload` (an app and two plugins), `frame_profiler` (a profiler window with sample data, menus, popovers and tooltips), `stream_luau` / `stream_rust` (the same window in Luau and Rust, compared by `examples/stream/measure.sh`), `anime_showcase` (the animation API), `anime_bench` (per-frame engine cost), `state_bench` (what a state update costs), `notes` (an app that saves with `ui.persist`), `mail` (a mail client in modules: a virtualized list, a selectable message, a spellchecked compose sheet, menus, tooltips and a settings window; run it with `zgx-run`), `chat` (a chat client against a model served in memory: replies streamed as server-sent events into a virtualized transcript of markdown with code blocks, a composer that grows, conversations with a search, and shortcuts; `cargo run -p zgx-run --example chat`, `zgx_run::run_hosting` serving it), and in zgx-desktop `terminal` and `zgx_monitor` (multi-module apps whose panes split, resize and move by dragging; the monitor's host samples processes and publishes them as shared state).

## Types

`crates/zgx-script/types/zgx.d.luau` types the `ui` and `anime` globals for [luau-lsp](https://github.com/JohnnyMorganz/luau-lsp): editors complete property names, events and API calls, and report misspelled ones (`bgg`, `durtion`, `on_clik`, `state:sett`) before the script runs. In VS Code, list the file under `luau-lsp.types.definitionFiles` and turn on Luau's new type solver, which the types need:

```json
{
    "luau-lsp.platform.type": "standard",
    "luau-lsp.types.definitionFiles": ["crates/zgx-script/types/zgx.d.luau"],
    "luau-lsp.fflags.override": { "LuauSolverV2": "true" }
}
```

In Zed, with the Luau extension, the repository's `.zed/settings.json` does the same; for a project elsewhere, copy it and point `definitions` at the file.

Put `"languageMode": "strict"` in the project's `.luaurc` (non-strict mode reports little), and the `standard` platform resolves `require` by path, so types flow between modules.

The types say what the engine accepts. Each element function takes its own props type (`InputProps` needs `value`, a `State<string>`; leaves such as `input` take no children; containers do), and every props and options table is closed, so an unknown key is an error: luau-lsp reports it as the key's type not being `number` (`Expected this to be 'number', but got '"bgg"'`). String options are the values the engine knows (`items = "centre"` is an error). Events get their payloads (`on_press` a `PointerEvent`, `on_key` a `KeyEvent`, returning `true` to consume it). `State<T>` takes its initial value's type: `count:get() + 1` checks, `count:set("x")` does not, and a list state's `push` takes its items; give `nil` and empty tables a type (`ui.state("selected", nil :: string?)`, `ui.state("tabs", {} :: { Tab })`). A property bound to state takes any state whose value fits it.

The file's `export type`s are global type names for a script's own annotations: `Node`, `Child`, `State<T>`, `Props`, `ContainerProps`, each element's props type, `Styles`, `Color`, `Element`, `Ref`, and `Merge<A, B>` to make a props type of your own ([reference](reference/index.md#reading-the-signatures)). Where an argument is the script's own data Luau cannot infer well, it is `any` (`each`'s row and `presence`'s value, drag payloads, a page's messages): annotate the callback's parameter, `function(task: Task)`, to check it.

The definitions are written in `types/zgx.d.luau.in`, where element props are `Merge<Props, { … }>`; `zgx.d.luau` is that file with each `Merge` written out (luau-lsp finds that many type function calls too complex). `tests/types.rs` regenerates it (`ZGX_BLESS=1 cargo test -p zgx-script --test tooling types::`) and fails when it is stale, and fails when the engine accepts a style property, element option, event, `ui` or `anime` function, State, Element, Ref, Web, Term, Canvas or Animation method or field, or animatable key the types do not have. At run time an unknown property or animation property is an error too, shown over the window.

### Checking before running: `zgx check`

```sh
cargo install --path crates/zgx-script --bin zgx   # or: cargo run -p zgx-script --bin zgx --
zgx check app.luau          # a file
zgx check examples/notes    # every .luau file under a directory
```

`zgx check` runs `luau-lsp analyze` with the definitions above built into the binary, the `standard` platform, the new solver (`--flag:LuauSolverV2=true`) and strict mode, unless a `.luaurc` (`languageMode`) or a `--!nonstrict` comment picks another mode (`--nonstrict` changes the default). It prints `file:line:col: error[TypeError]: …` and lint warnings, and exits 1 when there are errors (2 for usage errors, 3 when luau-lsp is missing). It catches what otherwise appears only at run time, such as a local function used before its definition, which is `nil` until called:

```lua
local function render()
    return ui.column { helper() } -- app.luau:2:24: error[TypeError]: Unknown global 'helper'
end
local function helper() return ui.text "hi" end
```

An unused local that a text template uses (`local count = ui.state("count", 0)` shown as `"{count}"`) is not reported. luau-lsp comes from its [releases](https://github.com/JohnnyMorganz/luau-lsp/releases) (put it on the `PATH`, or set `LUAU_LSP` to it); without it, `zgx check` says how to install it. A hot-reloading host runs the same check after every reload and shows the errors in the window's banner: in a debug build `host.watch` turns it on when luau-lsp is installed (a release build, or a machine without luau-lsp, does nothing). `host.check_on_reload(true | false)` or `ZGX_CHECK=1` / `0` decides instead. The check runs luau-lsp in its own process from another thread, after a reload rather than per frame, and costs about 0.1 s of that process's CPU for a twenty-module app; results arrive through `host.watch`.

### Best practice: `zgx lint`

```sh
zgx lint app.luau           # a file
zgx lint examples/notes     # every .luau file under a directory
zgx lint --rules            # what it looks for
```

`zgx check` asks whether a script is right; `zgx lint` asks whether it is written the way zgx runs fast. It reads the source only (no luau-lsp, nothing run), takes milliseconds for a large app, prints `file:line:col: warning[rule]: …` with the cheaper way to say the same thing, and exits 1 when it finds anything. CI lints every example with it. Run it, with `zgx check`, before handing a script over:

| Rule | What it finds | Instead |
| --- | --- | --- |
| `untracked-get` | `state:get()` while an element is built (`ui.text { "n: " .. n:get() }`, `ui.show(open:get(), …)`): the interface is built once, untracked, so it shows that moment's value forever | the state itself, a template (`"n: {n}"`) or a function (`function() return n:get() end`) |
| `state-in-builder` | `ui.state("key", …)` inside a component, `each` row or `list` row: one script-wide state that every instance shares | `ui.use_state(initial)`, or a key naming the instance (`` `file:{pane}` ``) when sharing by key is the point |
| `timer-interval` | `ui.every(ms)` under 16 ms: more script calls than display frames | `anime` or `ui.tween` (Rust, paced by the display), or the rate the value really changes |
| `tick-tween` | `anime.animate` in a `ui.every` callback running half the interval or more, uncapped: a 420 ms tween every 500 ms animates 84% of the time, laying out and painting on most frames for as long as the timer runs | `frameRate` (30 or less), or a duration well under the interval |
| `key-reads-state` | an `each` key function reading state: keys are kept per item value, so the key goes stale when the state changes | key on the item alone (`item.id`); read the state in the row |
| `per-frame-state` | `onUpdate`/`onRender`/`onBeforeUpdate` writing state with no `frameRate`: the script and every reader of the state run each display frame | animate the property, or the state as a target (`anime.animate(state, { value = 1 })`); or cap it (`frameRate = 8` for a clock) |
| `update-copy` | `state:update(fn)` that only inserts, removes or sets one path: it copies the whole value through Luau | `push`, `insert`, `remove`, `set_key(path, v)`, `patch` |
| `computed-sort` | `ui.sort` in a `ui.computed`: every change to what it reads (a row written, a tick) sorts the whole list again | `ui.sorted(list, { by, filter })`, which re-places only the rows written |
| `list-key` | a `ui.list` without `key` whose rows index into a list state the same file removes from or inserts into: rows are by position, so each removal or insertion writes every row after it with the item now there (its text, layout and paint again) | `key = function(i) return list[i].id end`, the row reading its item by key: the rows a change shifts are moved, not written |
| `list-measured` | a `ui.list` of fixed heights (`height`, `heights`) whose row wraps text (`wrap = true` with no `lines` cap, `ui.markdown`): how many lines it takes depends on its text and the width, which a script can't measure, so rows are cut off or leave gaps | `measured = true`: each row takes its content's height as it mounts, `height` the estimate until then |
| `reserved-system-key` | a `ui.command` with `reserved = true` on an editing or system key (`cmd+c`, `cmd+v`, `cmd+x`, `cmd+z`, `cmd+shift+z`, `cmd+a`, `cmd+q`, `cmd+h`, `cmd+m`): reserved, the key is the app's even in a web page or a terminal, which then can't copy, paste, undo or quit with it | leave it unreserved: a page or terminal hands the app what it does not use |
| `canvas-view` | a `ui.canvas` without `view` whose `draw` reads a zoom (`view.zoom`): every pan or zoom runs `draw` again, the script for every mark | draw in the canvas's own coordinates and give `view = state` (`{ x, y, zoom }`); `bg_grid` for a grid under it; what keeps its size on screen in a small canvas of its own |
| `draw-item-state` | a `ui.canvas` `draw_item` reading state (`:get()`): each item's drawing is kept until that item is written, so it shows what it read then | keep what an item looks like in the item (its colour, whether it is chosen), or draw what depends on other state in `draw` or a canvas of its own |
| `zoomable-zoom` | an element given both `zoomable` and `zoom`: its zoomable state sets its zoom, and `zoom` sets it again each time its style applies | give the zoom to the state alone (`ui.state("zoom", 1.5)`) and drop `zoom` |
| `window-size-layout` | an element's size or place (`w`, `max_w`, `x`…) a function computes from `ui.window.size`: Luau runs on every frame of a resize for what layout does itself | a percentage (`w = "30%"`), `grow`, `min_w`/`max_w`; read the window's size for decisions (a sidebar collapsing), a ref's `width` for one element's room |
| `list-marker-hidden` | a `role = "listitem"` whose first child is a marker text ("•", "3.") without `aria_hidden`: selection and plain text take it, copied HTML repeats it inside its `<li>`, and screen readers read it | give the marker `aria_hidden = true`: the list then draws it, as a browser draws an `<li>`'s |
| `animated-zoom` | a `zoom` given `ui.tween` or `ui.spring`, or animated by `anime`: a zoom lays its subtree out again, as CSS's does, so every frame of the animation does | animate `scale` (drawn scaled, without layout) and set `zoom` once, where it settles |
| `unknown-token` | a `"$…"` token in a style or a computed style that neither zgx nor the app declares (a misspelled `"$app.crad"` reads as transparent), one of the wrong kind for its field (`p = "$app.accent"`, a colour), or a theme (`ui.theme.load("x.json")`, `ui.theme.register { … }`) setting a name nothing declares. It reads the app's scripts together: one may declare what another uses; a declaration built at run time leaves its group unchecked | declare the token with its default in `ui.theme.tokens { app = { … } }` (a theme overrides it), or use the name the finding suggests |
| `window-option` | an option of `ui.window { … }` that doesn't exist (`titel`): types take its table's extra fields | the names in `WindowOptions` |
| `component-id` | `Card { id = … }` where `Card`, in the same file, sets an `id` on its root: the root's goes unseen | another name for data; `id` names the element |
| `unknown-route` | a literal route name given to a router's `navigate`, `replace` or `is`, or a case of its `view`, that no `ui.router { routes = { … } }` declares (`"thraed"`: did you mean `thread`?). The types catch the calls; this catches a misspelled `view` case (Luau accepts a table's extra fields) and apps past the types' 1,000 routes. Routes built at run time leave every name unchecked | use a declared route's name, or declare it in the router's `routes` |

`-- zgx-lint: allow(rule)` on the finding's line or the line above accepts a deliberate exception (`allow(rule-a, rule-b)` for several).

A debug build lints the scripts itself after every load and reload, so a findings list needs no one to remember to ask: each finding is printed once to stderr, kept in `ScriptHost::warnings()`, and shown over the interface (`ScriptHost::lint_findings()`) while there is no error. It costs milliseconds per reload, a release build or a bundled app does nothing, and `ZGX_LINT=0` turns it off. For tools, `zgx lint --format json` prints `{ "files": n, "findings": [{ "file", "line", "column", "rule", "message" }] }`; like the text form, it exits 1 when it finds anything.

Some costs the engine removes instead of asking scripts to avoid them: `ui.tween`, `ui.spring`, `ui.loop`, text templates and animations of states or elements run without calling the script at all.

### Assistive technology: `zgx audit`

`zgx audit app/` runs the app headless and checks what a screen reader gets of its first screen and of each command, button and route it opens: unnamed controls, focus that skips or traps, alerts not announced. It prints one line of JSON (each finding's rule, `#id`, role, name, source line and the state it was found in), or a table with `--human`, and exits 1 when it finds any ([accessibility audit](accessibility-audit.md)).

## Diagnostics

**Errors in the window.** When a script fails to load or reload, or a handler, computed property or effect raises, a banner over the bottom of the window shows the file and line (`app.luau:12`, in monospace), the message and as much of the traceback as fits. A failed reload keeps the previous interface running under it; the next good reload clears it, and × dismisses it. The banner is mounted only while there is an error (`View::child_when`), so without one it costs no nodes and no per-frame work. `host.error()` is the same message as a signal.

**Layout inspector.** Ctrl+Option+Cmd+I in a desktop window (Ctrl+Alt+Shift+I on Linux and Windows) toggles it, as does `ZGX_INSPECT=1`, `cx.inspector.set_enabled(true)` on a `WindowContext`, or `host.inspect(true)`. It outlines every box and describes the one under the pointer: its kind, `#id` and `.class`es, the script line that created it (`created at app.luau:42`; inner nodes say `created inside`), size and position, explicit sizes and limits, padding, margin, border and gap, flex grow, shrink, alignment and justification, and its clip. It follows the pointer only; it never synthesizes input, and input and cursors pass through it. It is drawn as an overlay of ordinary scene nodes appended last under the window's root, absolutely positioned with a fixed size: it takes no room in the interface's layout and its own updates lay out only itself, and it renders through the same renderer (GPU, software, native-view planes) rather than a special pass. While it is on, scripts record where they create nodes (turning it on reloads them, keeping state); off, nothing is recorded and it holds no nodes.

**Layout warnings.** While the inspector is on, or with `ZGX_LAYOUT_WARNINGS=1`, each layout is checked for: a zero-width or zero-height box with children or text (not one collapsed on purpose with `w = 0` or `max_h = 0`); in-flow content past the edges of a clipping box that does not scroll; a percentage width or height inside a parent sized by its content, which falls back to its content size. Each is reported once per node to stderr (`zgx layout warning: app.luau:12: column #list: zero height (380 × 0) with 3 children; …`) and listed by the inspector, which outlines the box in orange. Debug builds do not check by default: the walk is linear in the scene on every layout, which animations repeat each frame.

## Elements

Elements take one table: string keys are properties, events and options; array entries are children (nodes, strings as text, functions as computed text, nested arrays, which flatten). `nil` and `false` entries are skipped, so `cond and node or nil` works anywhere in the list.

The component library's elements (`ui.data_grid`, `ui.tree`, `ui.calendar`, `ui.date_picker`, `ui.time_field`, `ui.combobox`, `ui.select`, `ui.tabs`, `ui.command_palette`, `ui.segmented`, `ui.stepper`, `ui.disclosure`, `ui.accordion` and `ui.separator`, and `ui.toast`) are `zgx-components`', through zgx-script's `components` feature, on by default: a host built without it (`default-features = false`) has the rest of `ui`.

| Function | |
| --- | --- |
| `column`, `row`, `div`, `overlay`, `button`, `grid` | containers (`grid` sets `grid = true`) |
| `scroll`, `scroll_x` | scroll views; `offset = state` binds the position in points; `follow = true` keeps the view at its end while the content grows, until it is scrolled away (a transcript) |
| `text "…"`, `text { "…", … }`, `text(fn)` | `{name}` interpolates state in Rust; `{{` `}}` are literal braces |
| `rich { "plain", span { "bold", bold = true, on_click = … } }` | inline runs: `color bg text_size weight bold italic underline strike font on_click href` |
| `markdown_view { text, breaks, paths, copy }` | markdown drawn (code blocks with Copy, the theme's colours), parsed as its text changes and only its changed blocks built again: a streaming reply's view ([reference](reference/text.md#markdown)) |
| `markdown(text, { key, breaks, paths })` | markdown parsed into blocks of runs to draw your own way; with `key`, text that grows (a streaming reply) parses only its new end and keeps its earlier blocks the same tables ([reference](reference/text.md#markdown)) |
| `image { src = "a.png", fit = "cover" }` | PNG or JPEG, relative to the script; `src` may be state |
| `svg { src = "icon.svg" }`, `svg { code = "<svg…>" }` | `tint = color` (a state or function re-tints it); `tint = "currentColor"` follows the text color it inherits (a button's, hover included) |
| `canvas { draw = function(c, w, h) … end }` | runs again when its size or the state it reads changes |
| `canvas { each = state, draw_item = function(c, item, w, h) … end }` | draws each item on its own and keeps it: only items written since are drawn again |
| `webview { src = "https://…", … }` | a native web page (WKWebView on macOS) laid out, rounded, clipped and faded like any node, with zgx content (frosted popovers too) drawn over it; see [Web views](#web-views) |
| `terminal { term = ui.term(id) }` | a terminal session (a Ghostty surface, with the desktop host's `ghostty` feature) placed like a web view; see [Terminals](#terminals) |
| `checkbox`, `input`, `textarea`, `progress`, `slider` | `{ label, value = state }`; `slider` takes `min`, `max`; `input` and `textarea` take `placeholder` (text shown while the value is empty: a string, state or function), `placeholder_color` and `select_on_focus = true` (focus selects all the text: an address bar) |
| `show(cond, node, else)` | `cond`: state, function or boolean. What it shows lays out as if it stood in its place (its `grow`, `self`, percentages and `absolute` insets reach the parent), as for `switch` and `presence` |
| `switch(state, { key = node, … }, default)` | |
| `presence(value, function(v) … end, { enter, exit, mode, initial, layout })` | shows what `value` (a state or function) builds; when it changes, the old content leaves out of the flow, where it was and as large, playing `exit` (anime.js parameters), while the new plays `enter`. `mode = "wait"` enters after the exit; `nil` shows nothing; `initial = true` plays `enter` on the first value too. Sizing in its options (`w`, `h_full`, `grow`, `p`, …) makes it a box its content fills. A new value builds its content again, as a new key does in React: when the content can update in place (the open message's pane, as another opens), key it by whether something shows, not which one (`if open:get() then "open" else nil`), its texts reading what shows |
| `each(list, function(item, i) … end, { key = fn, animate, enter, exit, initial })` | rows follow their items (without `key`, by value), keeping their state across reorders, inserts and removals; a row whose item changes is built again, or with `function(item, i, row)` gets its item as state and updates in place; `animate = true` slides rows the list moves from where they were, `enter` and `exit` play on rows that come and go, as for `presence` (see [Layout animation](#layout-animation)) |
| `list { count, height, row = function(i) … end, offset }` | virtualized: only visible rows are built, each owning its `ui.use_state` and effects as a component does; with `key = function(i) … end` a row follows its item (`row = function(key, index)`, `index` a state), moved rather than written again as items are removed or inserted; with `measured = true` rows take their content's height (wrapped text), measured as they mount |
| `modal { open = state }`, `popover { open, anchor }`, `menu { open, anchor }`, `menu_item { "Copy", on_click }`, `submenu { open }` | overlays |
| `slot(name)`, `contribute(name, node)` | plugin extension points |

Canvas drawing: `c:rect(x, y, w, h, paint, radius)`, `c:circle(x, y, r, paint)`, `c:line(x1, y1, x2, y2, paint, width, opts)`, `c:fill(path, paint, "evenodd"?)`, `c:stroke(path, paint, width, { cap, join, dash, dash_offset, draw })`, `c:clip(path)`, `c:restore()`. Paths: `ui.path():move(x, y):line(…):quad(…):cubic(…):arc(cx, cy, r, start°, sweep°):close()`. Paint is a color or `ui.linear(x1, y1, x2, y2, { colors })`. `draw = t` (or `{ start, end }`) strokes a fraction of the path, as anime.js draws lines.

### Web views

`ui.webview` shows `src` (a URL, state or function: the page navigates when it changes) or `html` with `base_url`. Its options make the page: `transparent`, `profile` (`"default"` shares cookies and storage, `"ephemeral"` keeps nothing, any other string is a named, persistent store), `inspectable`, `user_agent`, `messages` and `new_windows`. To keep a page across the node (a tab switched away and back) or drive it from elsewhere, make its controller with `ui.web(options)` and pass `web = controller`. `ui.web(id, options)` keeps the page in the host by `id`, as `ui.term` keeps sessions: a reload asking for `id` again gets the same page, history and all, until `web:close()`.

```lua
local web = ui.web { profile = "ephemeral" }
local url = ui.state("url", "https://example.com")
local function back()
	web:back()
end
return ui.column {
	ui.row {
		ui.button { "Back", on_click = back },
		ui.text {
			function()
				return web.title
			end,
		},
	},
	ui.webview {
		web = web,
		src = url,
		grow = 1,
		rounded = 12,
		on_navigate = function(e)
			print(e.kind, e.url) -- started, committed, finished, failed, changed
		end,
		on_message = function(value)
			print(value.kind) -- window.zgx.postMessage({ kind = … }) from the page
		end,
	},
}
```

The controller's `url`, `title`, `loading`, `progress`, `can_go_back`, `can_go_forward`, `focused`, `error` and `pointer` are tracked like state. `pointer` is `{ x, y }` in points from the page's top left while the pointer is over the page's visible box (zgx content drawn over it included), nil otherwise: over a page that takes input zgx's nodes have had their leave and hear no moves (as over a terminal, whose `term.pointer` is the same), so a page's own edge affordances follow `web.pointer`. Its methods: `load(url)`, `html(html, base_url)`, `back()`, `forward()`, `reload()`, `stop()`, `focus()`, `blur()`, `devtools()`, `zoom(z)`, `run(js)` and `eval(js, function(value, err) … end)`, which gets the script's value as Luau data. Events: `on_navigate({ kind, url })`, `on_load(url)`, `on_title(title)`, `on_message(value)`, `on_error(url, err)` and `on_new_window(url)`. Without a native host (headless, other platforms) the node shows a labelled box.

### Terminals

`ui.term(id, options)` returns the terminal session for `id`, starting it the first time: `command` (a command line; the login shell by default), `cwd` (`"~"` is home), `env` (a table) and `font_size`. Sessions belong to the host, not to a run: a reload keeps every shell running and the run that asks for `id` again gets the same session, its options' handlers replacing the previous run's (`ui.term(id)` without options only looks it up). Its process starts when it is first shown, with the options given by then: a lookup that comes first (a sidebar reading every pane's state) does not decide how it starts. Show one with `ui.terminal { term = t, … }`, sized and rounded like any node; moving the node elsewhere (another split, another tab) moves the terminal with its process.

```lua
local term = ui.term("p1", {
	cwd = "~/src",
	on_title = function(title)
		print(title)
	end,
	on_close = function(alive)
		term:close()
	end, -- the process exited (alive = false)
})
return ui.terminal {
	term = term,
	grow = 1,
	rounded = 10,
	on_focus = function()
		print("typing here")
	end,
}
```

The session's `title` (what the program set), `pwd` (the shell's directory), `process` (the foreground program's name), `pid`, `focused`, `bell`, `progress`, `exited`, `columns` and `rows` are tracked like state. Methods: `focus()`, `blur()`, `input(text)` (as typed), `paste(text)`, `binding(action)` (a Ghostty binding action: `"copy_to_clipboard"`, `"clear_screen"`, `"increase_font_size:1"`), `needs_confirm()` (whether closing ends something other than an idle shell), `snapshot(max_width?)` (a picture of what it shows now, as a `ui.image` `src`) and `close()`. Events, as options: `on_title`, `on_pwd`, `on_process`, `on_exit(code)`, `on_close(alive)`, `on_bell`, `on_command(exit_code, ms)` (shell integration) and `on_notify(title, body)`. `progress_error` says the last progress report (OSC 9;4) was an error. `status` is what the program reports of itself through the [Program Status Protocol](https://www.superlogical.com/rex/docs/build/program-status) (OSC 7501), when it does: `state` (`"idle"`, `"working"`, `"done"`, `"blocked"`, `"error"`), `kind` (what a blocked program waits for: `"permission"`, `"question"`, `"auth"`), `progress`, `app`, `title`, `msg`, `child_blocked` (a sub-task is blocked) and `since` (Unix seconds); `nil` otherwise. A terminal host fills it in; zgx-desktop's terminal example keeps a session's reports in its session server. The terminal reads its own keyboard, IME and mouse; app shortcuts reach the app through its menus (see [Commands](#commands-and-host-actions)). `zgx-desktop/examples/terminal` is a multiplexer written this way.

### Commands and host actions

`ui.command { id, title, menu, key, run }` declares an app command: the host shows it as a menu item (`ScriptHost::commands`, in declaration order) with `key` as its shortcut (`"cmd+shift+d"`, `"cmd+alt+arrowleft"`, `"cmd+["`) and calls `run` when it is chosen (`ScriptHost::run_command`). Menu shortcuts work wherever focus is, a terminal or a web page included: a focused page or terminal has each key first, as in a browser, and the app has those it leaves alone. `reserved = true` makes a command's key the app's whatever has the keyboard, before the page or terminal (as a browser keeps ⌘T and ⌘W from its pages; `ScriptHost::reserved_keys`, `KeyBinding::reserved` in Rust). Only the app's own commands can be reserved, not a plugin's, and `zgx lint` flags an editing or system key reserved (`reserved-system-key`). `ui.command { menu = "View", separator = true }` adds a separator. Each successful load replaces the commands; a failed one keeps the last. `ui.invoke(name, arg)` calls a host action now, from any handler (`ui.action` binds one to a handler instead). Lua tables drop keys set to `nil`; `ui.null` keeps one, as JSON's null: `{ caps = ui.null }` reaches the host as `{"caps": null}`.

### Drag and drop

A node becomes a drag source with `on_drag = function() return payload end` (any plain value), dragged once the pointer moves past the system's drag threshold (GNOME's and KDE's setting, GTK's 8 points on Linux without one, Windows' `SM_CXDRAG`, 4 elsewhere); `drag_preview = function(payload) return node end` builds what follows the pointer, and `on_drag_end(payload, accepted)` reports how it ended. A destination takes `on_drop(payload, e)`, optionally `drop_when(payload)` to accept only some payloads, and `on_drag_over(payload, e)` / `on_drag_leave(payload)` to show where a drop would land; `e` is `{ x, y, local_x, local_y, w, h }`. `on_drag_spring(payload)` spring-loads it: the drag rested on it for the system's delay (or Space was pressed), so it opens what it stands for, as a tab shows. `drag_preview_anchor = { 0.5, 0 }` with `drag_preview_offset = { 0, 16 }` hangs the preview below the pointer. The nearest accepting destination gets the drag. A node with `on_press` and `on_pointer_move` or `on_release` keeps the pointer from press to release, wherever it goes (a divider being dragged).

### Context menus

`context_menu = function(e) return { … } end` on any node shows what it returns (`ui.menu_item`s, `ui.submenu`s, separators such as a 1-point `ui.div`) as a menu at the pointer on a secondary click, or a control-click on macOS; returning nothing shows none and lets the click through. `e` is `{ x, y, local_x, local_y }`. The innermost node with a context menu takes the click. `context_menu_style = { … }` styles the menu's panel with plain values (`bg`, `backdrop_blur`, `rounded`, `p`, `w`, `border`, `shadow`). `context_menu_below = true` opens it under the node, from its left edge, as a tab's does, wherever the click was. Terminals hand right clicks their program does not use to the node, so a terminal's context menu opens over it. Rust views have the same in `View::context_menu`, `View::context_menu_style` and `View::context_menu_below`.

### Properties

Any property can be a value, a state, a function (rerun when what it reads changes) or an animation binding.

- Size and position: `w h min_w max_w min_h max_h` (points, `"50%"`), `size w_full h_full aspect absolute relative top left right bottom`; `snap = true` lays the node's edges on whole points after layout (percentages and ratios leave fractions, which blur a 1-point border across pixels), its size following its rounded edges so boxes sharing an edge still meet.
- Spacing: `p px py pt pr pb pl`, margins `m mx my mt mr mb ml` (also `"auto"`), `gap gap_x gap_y`.
- Flex and grid: `direction grow shrink basis flex_wrap wrap_reverse items justify self content`, `grid cols rows col_start row_start col_span row_span col_span_full row_span_full`.
- Paint: `bg gradient color rounded corners border border_color border_dash shadow opacity blur backdrop_blur fade layer fit tint cursor`, `overflow overflow_x overflow_y hidden invisible`. `blur` blurs the node's own content (CSS `filter: blur()`); `brightness`, `contrast` and `saturate` (CSS filter functions, 1 is unchanged) recolor the node and its subtree without a layer, and animate on the compositor (`hover` brightening: `anime.animate(el, { brightness = 1.12, duration = 150 })`); `backdrop_blur` blurs what is behind it (frosted glass). `fade = { top, bottom }`, `fade_x = { left, right }` or `fade_sides = { top, right, bottom, left }` fade a clipping view's edges over those widths (a horizontal row fading its right 28 points: `fade_x = { 0, 28 }`); `fade_overflow = true` fades an edge only as far as content runs past it, so a list scrolled to its top shows no top band and the band grows in as the content scrolls under that edge.
- Background images: `bg_image = "photo.jpg"` (a path, state or function) with `bg_fit = "cover"` (default), `"contain"`, `"fill"`, `"none"` or `"scale_down"`: drawn over `bg`, under the border and children, cut by `rounded`/`corners`.
- Masks: `mask = { ui.bitmap(w, h, function(x, y) return alpha end), w = …, h = …, x = …, y = … }` shows the content through the bitmap's alpha, drawn at `w`×`h` from (`x`, `y`), sampled without smoothing. Animating `maskX` / `maskY` slides it on the compositor (a dithered fill, a sprite strip stepped with `ease = "steps(n)"`); for a band sweeping on and on (a text shimmer), `sweep = { x = 640, duration = 2000, frame_rate = 30 }` in the mask has the engine move it on its own frame clock: no script, style pass or layout per step, only the masked node composited again.
- Shader fills: `bg_shader = ui.shader(wgsl, { fps = 30 })` fills the box with a WGSL function run per pixel on the GPU, over `bg`, under the border and children, cut by the corners. The source defines `fn fill(input: FillInput) -> vec4<f32>` (straight alpha) and reads `input.p` (points from the box's top left), `input.size`, `input.time` (seconds), `input.scale` (pixels per point) and `input.params` (four `vec4`s). `shader_params = { "#e2c98f", { 1, 0.5, 0, 0 } }` sets the parameters (colors become 0 to 1; a theme token, `"$accent"`, follows the theme); animate `shaderTime` and `shaderParam0` to `shaderParam15` like any property, on the compositor (`anime.animate(el, { shaderTime = { 0, 60 }, duration = 60000, ease = "linear", loop = true, frameRate = 12 })` turns a dither at 12 fps). `fps` caps how often time repaints it. A shader that does not compile fills with `bg`.
- Blend modes: `blend = "soft-light"` (CSS `mix-blend-mode`: `multiply`, `screen`, `overlay`, `darken`, `lighten`, `color-dodge`, `color-burn`, `hard-light`, `soft-light`, `difference`, `exclusion`, `hue`, `saturation`, `color`, `luminosity`) mixes the node with what lies beneath it, inside the nearest layer it sits in (a tilted card's foil blends with the card). A gradient with `blend = "color"` tints a photo; `"multiply"` darkens a texture onto a surface; `"difference"` keeps a cursor visible on any background.
- Layer shaders, for what blend modes and filters can't express: `layer_shader = ui.layer_shader(wgsl)` composites the node's subtree, painted once into a cached layer, through a WGSL function run for each pixel of its box. The source defines `fn layer(input: LayerInput) -> vec4<f32>` and reads `input.color` (the content's straight-alpha pixel), `input.p`, `input.size`, `input.time`, `input.scale` and `input.params` as fills do, and may call `zgx_sample(p)` for the content elsewhere in the box (a ripple, a chromatic split). `layer_params` sets the parameters; animating `layerTime` and `layerParam0` to `layerParam15` composites again without repainting the subtree (a holographic sheen following the pointer). Past the box, and when the shader does not compile, the layer shows as painted.
- Clip insets: `clip_inset = { top, right, bottom, left }` (CSS `clip-path: inset(...)`) paints the node and its children only inside its box shrunk by those amounts (negative reaches past it, rounded with the node's corners when it clips). Animate the sides as `clipTop`, `clipRight`, `clipBottom`, `clipLeft`, in points or as a percentage of the node's size: `{ clipBottom = { 0, "100%" } }` closes a node upward while its space closes, on the compositor.
- Size animation: `layout = "height" | "width" | "size"` lets the node's content size change freely while the space it takes eases to it (a spring of 0.4 s, bounce 0.1, or `layout_ease` / `layout_duration`). The node lays out at its new size at once, as it would without `layout` (its `grow`, percentages and the scroll views inside it too), and is not clipped (`layout_clip = true` clips it to the space); `layout_enter = true` grows it from nothing when it mounts. Combine with `presence` for content that swaps, folds or docks.
- Position animation: `layout = "position"` slides the node when layout moves it within its parent, and `layout_id = "selection"` hands a node's box to the next node mounted with the same id; see [Layout animation](#layout-animation).
- Text: `text_size weight bold italic align font wrap truncate lines line_height letter_spacing`.
- Transforms: `x y scale scale_x scale_y rotate skew skew_x skew_y rotate_x rotate_y perspective origin` (degrees; `perspective` in points; origin as fractions of the node). `rotate_x` / `rotate_y` with a `perspective` tilt the node in 3D (a card leaning toward the pointer), composited without repainting.
- `hover`, `active`, `focus`, `disabled` take tables of plain values; `disabled = state` disables.
- `selectable = true` makes the text in the node and everything it holds selectable across nodes, as on a web page: drag, double/triple press, Shift, ⌘A / Ctrl+A, ⌘C / Ctrl+C and Copy in the context menu on selected text; `false` keeps a part out. Text is not selectable by default (put it on the node that draws a `ui.markdown` document). See [selectable text](reference/text.md#selectable-text).
- `window_drag = true` makes the node a window drag region (CSS's `-webkit-app-region: drag`): a press on it, or on anything inside it that takes no input itself, moves the window, and a double click zooms or minimizes it as the system's title bar setting says. Buttons, inputs and other focusable or pointer-following nodes inside keep their clicks. With `WindowOptions { titlebar: Titlebar::Inset, glass: Some(Glass::default()), .. }` in the host, a `window_drag` strip under the traffic lights is a whole frosted title bar.
- `id`, `class = "a b"`, `ref = ui.ref()` make a node reachable by selectors and animations.
- `pointer_events = "none"` (CSS `pointer-events: none`) lets the pointer through the node, and through descendants without handlers of their own, to what is under it: a full-window layer that holds popovers and tooltips, a hover card over a chart.

Lua tables have no key order, so a property that refines another applies after it whatever order the keys come in: `p = 0, px = 8` pads the sides, `border = 1, border_sides = { 1, 0, 0, 0 }` draws only the top, and `rounded` gives way to `corners` and `rounded_tl`.

Colors are `"#rgb"`, `"#rrggbb"`, `"#rrggbbaa"`, `rgb()`, `rgba()`, `hsl()`, a few names, `0xrrggbb`, or a theme token, `"$text.secondary"` (lengths, sizes, fonts and shadows take tokens too; see [Theming](#theming)). `gradient = { angle = 135, stops = { "#f80", "#08f" } }` (CSS `linear-gradient` angles: 0 runs bottom to top, 90 left to right, the default 180 top to bottom; stops may be `{ offset, color }`); `shadow = { x = 0, y = 8, blur = 24, spread = 0, color = "#0006" }` (keyed) or a list of them (CSS `box-shadow` values: `blur` is the CSS blur radius); `corners = { tl, tr, br, bl }`, or single corners `rounded_tl rounded_tr rounded_br rounded_bl` (animatable as `borderTopLeftRadius` and so on); `border_sides = { top, right, bottom, left }` sets each side's width (a dock open at the bottom: `{ 1, 1, 0, 1 }`); `border_over = true` paints the border over the node's children (an image's frame) instead of under them.

### Boxes and positioning

Sizes are border-box, as in CSS with `box-sizing: border-box`: `w` and `h` include the border, padding and children sit inside it (each side of `border_sides` on its own), and a node sized by its content adds its border. `ui.column { w = 200, p = 8, border = 1 }` lays its children out 182 wide, 9 in from each edge. Only `border_over = true` (an inset stroke over the content) takes no room, so use it for a ring that must not move content; a transparent border still takes its width. The default focus ring of buttons and inputs is such a stroke (or recolours their own border); `focus = { border = 0 }` turns it off.

`ui.overlay` stacks its children in one box, as CSS grid lays out items sharing one area: each is sized against the overlay and aligned in it on its own, `items` across (`"start" "center" "end" "stretch"`) and `justify` down (the same, plus `"between"` as start and `"around"`/`"evenly"` as centre). With neither set, children stretch to fill it, except along an axis they size themselves on; `items = "center", justify = "center"` centres an icon in a tile. The overlay is as large as its largest child, so as a pane (`grow = 1` in a row) it shows children that ask to fill it. `ui.scroll` and `ui.scroll_x` are 320 by 240 only when nothing else sizes them: a stretching parent stretches them across, `grow` grows them, `w`/`h` hold; their content fills their width (`scroll`) or height (`scroll_x`). `ui.input` and `ui.textarea` are 240 wide by default whatever their text (a long value scrolls inside, keeping the caret in view), widen in a stretching parent and shrink in a row too narrow for them, as every box does (`shrink = 1`, CSS's default, each no narrower than its content needs; an input scrolls its text, so it may shrink to nothing); `shrink = 0` holds one.

An `absolute = true` node positions against its containing block: the nearest ancestor with `relative = true`, searched from its parent up but not past an `absolute` ancestor (which contains its own absolute descendants); with none, its parent. `left`, `top`, `right` and `bottom` count from that block's padding box, inside its border; percentages are of its size; both insets on an axis without a size stretch it between them; an axis without insets keeps it at its parent's content origin (plus `ml`/`mt`). `show`, `switch`, `each` and `presence` wrap their content in a node of their own, so mark the intended ancestor `relative` rather than relying on being its direct child. Transforms and scroll offsets never change the containing block: an absolute node moves with them, and still paints in its parent's place, under its clips.

```lua
ui.column {
	relative = true,
	w = 300,
	h = 200,
	ui.column { p = 12, ui.div { absolute = true, right = 8, bottom = 8, w = 16, h = 16 } },
}
```

### Responsive layout

A layout that changes with the room it has, as CSS's media and container queries change one, reads its container's size through a ref: `r.width` and `r.height` are followed, so a computed reading them runs again when layout resizes the element. The change lands in the frame the size changes: layout, the computed and layout again all run before it shows, as a browser settles its ResizeObserver callbacks before painting, so nothing flashes at the old layout.

```lua
local root = ui.ref()
-- "wide", "medium" or "narrow", by the window's width.
local layout = ui.computed(function()
	local w = root.width or 1200
	return if w >= 1000 then "wide" elseif w >= 680 then "medium" else "narrow"
end)
return ui.row {
	ref = root,
	w_full = true,
	h_full = true,
	ui.show(function()
		return layout:get() == "wide"
	end, ui.column { w = 220, shrink = 0, "Mailboxes" }),
	ui.column {
		grow = 1,
		-- Wraps onto a second line rather than holding the pane wide.
		ui.row { flex_wrap = true, gap = 8, ui.button { "Reply" }, ui.button { "Archive" } },
	},
}
```

Query a box whose size does not depend on what the query changes, such as the window's root row: a box sized by its content that hid content below a width would go back and forth (zgx stops after a few rounds). Within a layout, flex does the rest: children shrink to fit (`shrink = 1`) down to their content's narrowest, text wraps with `wrap = true`, rows with `flex_wrap = true`, and `min_w = 0` lets a truncated label go narrower still. The mail example (`examples/mail`) folds a three-pane window this way, into a drawer and then one pane at a time.

### Layout animation

Three things animate what layout does, all the same way: after the layout pass that moves a node, it is held where it showed by a translation, which eases to nothing. The frames that follow only move it on the compositor: no layout, no script, and frames only while something moves. Positions come from layout, so scrolling and other animations are not moves, and a move in flight that is interrupted continues from where the node shows. The default ease is the layout boxes' spring (0.4 s, bounce 0.1).

- `ui.each(list, build, { key = fn, animate = true })`: when the list changes, rows that stay but move slide from where they were, whatever their sizes, in a column, a row, a wrap or a grid (the list's own `grid = true, cols = 2`). `animate` takes `true`, an ease name, `{ duration, ease }` (ms, a name or `anime.spring`), `{ bounce, duration }` (a spring) or `{ spring = anime.spring { … } }`. New rows play `enter` and leaving rows `exit`, with the parameters `presence` takes (`initial = true` plays `enter` on the first rows too); a leaving row leaves the flow where it was, so the rows after it close up while it goes. Rows need a `key` to be the same rows after a change.
- `layout = "position"` on any node: when layout moves it within its parent (a sibling grows, one before it closes), it slides there. `layout_ease` and `layout_duration` time it.
- `layout_id = "selection"`: a node mounting while another with the same id is mounted, or as it unmounts in the same frame, starts at that one's box (translated and scaled) and moves to its own: a tab's selection highlight sliding to the tab that takes it. Ids are scoped to the script. The node also slides when layout moves it, as with `"position"`; `layout_ease` and `layout_duration` time both.

```lua
ui.each(tabs, function(tab)
	return ui.row {
		px = 10,
		h = 28,
		-- Under the tab's content, filling it: the presence is placed.
		ui.presence(function()
			return if active:get() == tab.id then "on" else nil
		end, function()
			return ui.div { layout_id = "selection", h_full = true, rounded = 8, bg = "#ffffff14" }
		end, { absolute = true, top = 0, left = 0, right = 0, bottom = 0 }),
		ui.text { tab.name },
	}
end, {
	key = function(tab)
		return tab.id
	end,
	animate = { duration = 240, ease = "outQuint" },
	exit = { opacity = 0, duration = 150 },
})
```

The moves go on the node's translation over its own: a row's `x` and `y` (literal or animated) compose with them. A shared element's scale is the node's own scale, so animating its `scale` while it hands over fights the handover.

### Events

`on_click`, `on_press` (primary pointer-down), `on_release`, `on_hover(inside)` (`true` once when the pointer comes inside the node or anything in it, `false` once when it is outside it and all its descendants: moving between children tells the parent nothing, as the DOM's `mouseenter`/`mouseleave`; leaving the window, the window going to the back and the pointer moving onto a web page or a terminal leave too), `on_pointer_move({ x, y, local_x, local_y })`, `on_key({ key, shift, ctrl, alt, meta, repeat })` (return `true` to consume it), `on_key_up(key)`, `on_focus`, `on_blur`, `on_wheel({ dx, dy })`, `on_mount(element)`, and `on_resize({ w, h }, element)`, called when layout gives the node a new size (and at its first layout), as a ResizeObserver reports: decide from real sizes (a row that wraps, text that overflows) rather than estimates; and `on_visible(visible, element)`, called when any of the node comes into view or all of it leaves (inside every clip and scroll viewport above it), and at its first layout: build what only matters on screen. Elements read their laid-out `width`, `height`, `x` and `y`. `el:focus()` moves keyboard focus to the element or its first shown focusable descendant, and `el:blur()` clears focus inside it. An input's element selects: `el:select_all()`, `el:set_selection(a, b)` (`b` defaults to `a`, a caret) and `local a, b = el:selection()` (the anchor and the focus, where the caret is), in characters: Unicode code points from 0, as `utf8.len` counts them, so `el:set_selection(0, utf8.len(text))` selects everything. They return false (`selection`: nil) for other elements. Handlers take a function or a Rust-run action: `ui.set(s, v)`, `ui.toggle(s)`, `ui.add(s, n)`, or `ui.action(name, arg)` for a function registered with `ScriptHost::action`.

### State and components

- `require` loads modules as Luau resolves them: `require("./card")` and `require("../theme")` from the requiring file, `require("@components/card")` through the `aliases` in a `.luaurc` beside the script, `require("@self/x")` from the requiring file's own directory, and a directory as its `init.luau` (a bare `require("lib/theme")` resolves from the script's directory). Modules stay inside the script's directory, or a package: a folder of modules outside it the host lists (`ScriptHost::package("kit", dir)`, or zgx.json's `"packages"` under zgx-run), required as `require("@kit/button")` or through a `.luaurc` alias into it, so luau-lsp follows it too. A package is read, never written, its modules run as the app's own and reach nothing outside their folder (a link out of it included), and plugins reach none. Its theme files load the same way (`ui.theme.load("@kit/theme.json")`). Modules run once and return what they return; a change to any of them reloads the scripts, keeping state. A module's `ui.state` and `ui.persist` keys are its own (two modules' `ui.state("open")` are two states), and so are the `{name}`s its text templates show; `ui.shared` stays global, and `ui.use_state` is per component instance. [App structure](reference/app-structure.md) gives a layout for a large app, with its shared state in one module.

```lua
-- ui/components/card.luau
type CardProps = { title: string, tone: ("info" | "error")? }
return function(props: CardProps)
    return ui.column { p = 12, rounded = 10, ui.text { props.title } }
end

-- app.luau, with .luaurc: { "languageMode": "strict", "aliases": { "components": "./ui/components" } }
local Card = require("@components/card")
return Card { title = "Saved" }   -- `Card { titel = … }` is a type error in the editor
```
- `ui.state(key, initial)`: private to the script, kept across reloads. `ui.shared(key, initial)`: shared with Rust and every script. `ui.persist(key, initial)`: like `ui.state`, and saved across runs (see below).
- State values: `nil`, booleans, numbers, strings, lists, string-keyed tables and elements (`ui.use_state(el)`, `state:set(el)`: Rust sees them as `Value::Element`, opaque, equal only to themselves, `null` in JSON). A value is an immutable snapshot: `get` and `peek` return read-only tables (changing one is an error that names the methods below), and every change makes a new value that shares whatever it did not touch, so tracking stays cheap and a write of an equal value notifies nobody.
- Handles: `get` (tracked), `peek`, `set(v)`, `set_key(key, v)` (one key; an index from 1 into a list; or a path, `set_key({ "tabs", 3, "title" }, "x")`; `nil` removes the key), `patch({ a = 1, b = 2 })` (a shallow merge), `update(fn)` (`fn` gets a mutable copy: change it in place and return nothing, or return the new value), `insert(i, v)`, `push(v)`, `remove(i)`, `toggle`, `add(n)`, `len`, `count(query, { case, limit })` and `find(query, { case, limit })` (a string's matches, `{ from, to }` bytes from 1 and whether there were more than `limit`: searched where the string is held, so a long document isn't copied into the script). `set_key`, `patch`, `insert`, `push` and `remove` copy only the path they change; `update` copies the whole value into Luau and back, so prefer the others for large state.
- `ui.computed(fn)`: derived state, current whenever it is read, in a handler or `ui.batch` too, right after a write it depends on. `ui.effect(fn)`: runs again when what it reads changes. `ui.batch(fn)`: one update for many writes. `ui.sorted(list, { by, descending, filter })`: a list state's items in order, kept in order as rows are written (`state:at(i)` reads one).
- `ui.form { fields, validate, submit, on_success }`: a form's fields checked as they are typed (shown once left or a submit tried), its submit in a task with `pending` and `failure`; a control takes `field = form.fields.email` for its value, its error (heard by assistive technology) and Enter submitting ([Forms](reference/controls.md#forms)).
- `ui.resource(load, { initial })`: data loaded in a task (`load` may await `ui.fetch` or `ui.call`), as states `data`, `loading`, `error` and `status`, with `reload`; it loads again when a state `load` read before its first await changes, keeping only the last answer ([Loading data](reference/state.md#loading-data)).
- `ui.every(ms, fn)` calls `fn` every `ms` milliseconds until it returns `false`; `ui.after(ms, fn)` calls it once. Both return a function that stops them, and are owned like effects. They sleep between firings: no frames run while a timer waits (a clock showing "4m ago" ticks once a minute, not sixty times a second).
- `ui.ref()`: give a node `ref = r` and `r:get()` (or `r.element`) is its element while it is mounted and `nil` when it is not; reading it in an effect or computed follows it, so an effect that needs the element runs again once it mounts (`local el = field:get(); if el and wanted:get() then el:focus() end`). `r:peek()` reads without following; `r.mounted`, `r.count` and `r:elements()` (every node sharing the ref) are tracked too.
- Functions that build part of the interface (`each` rows, `presence` content, components) own what they create: `ui.computed` and `ui.effect` made while building live as long as the part they built.
- `ui.component(function(props) … end)` returns a constructor: `Card { title = "…", child }` (children also arrive as `props.children`). Inside, `ui.use_state(initial)` is per instance, effects belong to the instance, and `ui.on_mount(fn)` runs once it is mounted (outside a component, once the app's interface is).
- `ui.variants { base, variants, compound, defaults }` gives a component its styles by variant, as shadcn's `cva`: `variants = button { tone = props.tone, size = "sm" }` on an element. Options chosen by name resolve into the element's literal styles as it is built; chosen by a state or function, they restyle it in Rust when that changes, interaction styles included, without calling the script. The options are typed from the definition (`tone = "loud"` is an error), and `VariantProps<typeof(button)>` gives a component's props them. See [Design systems](reference/design.md).

### Saved state

`ui.persist(key, initial)` returns state that the next run of the app starts from. Each key is a JSON file (`Value::to_json`) in the host's storage: `ScriptHost::app_id("my-app")` puts it in the platform's data directory for the app (`~/Library/Application Support/my-app/script-state` on macOS, `%APPDATA%\my-app\script-state` on Windows, `$XDG_DATA_HOME/my-app/script-state` elsewhere; `zgx_script::app_data_dir`), and `ScriptHost::storage(dir)` puts it anywhere else. Without either, it is kept only until the app quits, with a warning.

```lua
local notes = ui.persist("notes", {})
notes:insert(1, { id = "n1", title = "Groceries" }) -- saved within 250 ms
```

A key is read from disk the first time a run asks for it, while the script loads; after that it lives in the host like `ui.state`, so a hot reload keeps the live value and reads nothing. Keys are the script's own and a required module's are its own, as with `ui.state`. Writes are batched: the first change starts a 250 ms timer, and then every value changed since becomes JSON once, on the main thread, and are written by a background thread to a temporary file renamed over the old one, so a crash leaves the old value or the new, never half a file. `ScriptHost::save()` writes what is pending and waits; dropping the host does too. A file that is not JSON starts the key from `initial`, with a warning.

### Plugins

[Plugins](plugins.md) walks through a worked example: making an app hookable, what plugins may do, the trust model of `Override`, and what they see of the app. In short, a plugin is a script that runs beside the app in a VM of its own and adds to the app's slots: `ui.slot(name, { context, from })` is where contributions go, and `ui.contribute(name, node)` adds one. A contributed component is an instance per slot, and gets the slot's `context` as `props.slot`: one panel component, a panel per pane. `from = "notes"` shows only what the plugin `notes` contributes.

A plugin can also add next to any element of the app, or in its place, without a slot: `ui.contribute { target = "#save", after = node }` (or `before`, or `replace`). A target is an `id`, a declared `hook = "name"` (`hook:name`), or a role and a name as the app writes them (`role:button "Save"`); `private = true` takes an element and what it holds out of reach. `ui.filter { target, text = fn }` passes an element's text, label, hidden, disabled, tooltip, list items or children's order through a plugin's function. Replacing and filtering ask `Capability::Override`, and `expose_hook(target, capability)` makes adding at a target ask one; what a plugin adds is described to assistive technology as the plugin's ([plugin hooks](design/plugin-hooks.md)). Every `id` is reachable, so a plugin targeting one that is not a declared hook may break with the app's next version.

`ScriptHost::plugins(dir)` loads every script in a directory; `set_plugins(vec![Plugin { name, entry, grants }])` sets the plugins an app manages itself (one that asks its user first), from the next `reload` on, read from the disk even in a bundled app and watched for edits. What a plugin may do is what the app grants it, and nothing else of the app's is in reach:

| | the app's scripts | a plugin |
|---|---|---|
| its own `ui.state`, `ui.persist`, components and slots' contributions | yes | yes, its own only (a plugin named as an app script still has its own saved state) |
| host actions (`ui.action`, `ui.invoke`, `ui.call`) | every one | those `expose_action(name, capability)` exposes, and only with the capability when it names one |
| shared state (`ui.shared`, `{key}` in a template) | every key | those `expose_shared(key, capability)` exposes, likewise; those `expose_shared_read(key, capability)` exposes, to read and follow only (a write is an error naming the key) |
| `ui.fetch`, remote images | yes | with `Capability::Fetch` |
| the clipboard, file dialogs | yes | with `Capability::Clipboard`, `Capability::Files` |
| terminals (`ui.term`) | yes | with `Capability::Terminals` |
| web pages (`ui.web`, `ui.webview`) | yes | with `Capability::Web`: its own pages, with their own cookies and storage, not the app's |
| windows of its own (`ui.window`) | yes | with `Capability::Windows` (one could pose as the app's, or the system's asking for a password); never the app's title bar (`ui.window.titlebar_area`, `controls`) |
| files: modules, themes, images | from its directory | from its directory or below, links followed (one pointing outside is outside) |
| commands (`ui.command`) | yes | yes, as `<plugin>/<id>`; a shortcut the app (or an earlier plugin) has stays theirs |
| the app's modules (`@app/…`) | requires them | replaces one with `ui.override` given `Capability::Override(module)` (or `"*"`), **a trust grant**: the replacement runs as the app's own code; never requires them |

A plugin's files are source, always: Luau checks no bytecode it loads, so a plugin's (or a module's) bytecode is read as source and fails to parse, rather than run. Only an app's bundle carries bytecode, its build's.

**Module overrides.** A plugin replaces one of the app's modules, as a WordPress plugin replaces a theme's template: `ui.override { module = "@app/components/card", with = "./card" }` loads the plugin's `card.luau` wherever the app requires `@app/components/card`, and the replacement may `require("@app-original/components/card")` to wrap the original. Only what the app requires as `@app/…` (its own directory, under zgx's `app` alias) can be replaced; a relative require stays the app's. **`Capability::Override` on a module is a trust grant, not a permission over the interface:** the replacement runs in the app's VM with everything the app's scripts may do, so `Override("@app/…")` trusts the plugin with that module and `Override("*")` with the app. Grant it as you would install the plugin's code into the app. `@app-original/…` is reached from the replacement alone; a `.luaurc` cannot define `app` or `app-original`.

Reaching for something it was not given is an error that names what it needs; a plugin that fails to load is left out, and the app and the other plugins load. `Capability::Named(..)` is the app's own vocabulary for what `expose_*` asks. The terminal example's plugins (`examples/terminal/plugins.rs`) are a whole system on top: a folder per plugin with a manifest saying what it needs, a sheet asking the user to allow it, and its panels opened from ⌘K.

### Localized text

`ui.i18n { dir = "locales" }` loads locale files of ICU MessageFormat messages, `ui.t(key, args)` shows one wherever text goes (following its locale, its file and its arguments' states, formatted in Rust), and `ui.locale` asks for a locale or follows the system's. See [Text: localized messages](reference/text.md#localized-messages).

### Warnings

Some mistakes are not errors but are probably wrong; each is reported once per call site, with its file and line, to stderr and to `ScriptHost::warnings()`. Durations are milliseconds everywhere (`ui.tween`, `ui.loop`, `layout_duration`, `anime`): a fractional duration under 10 (`duration = 0.16`, likely meant as seconds) and a non-looping one over a minute both warn. `ui.every` under 16 ms warns too: it calls the script more often than a display refreshes. A `ui.each` over more than 1,000 items warns: it builds every row, where `ui.list` builds only those in view. An `anime.animate` line that replaces its own unfinished animation again and again warns as well (what drives it changes faster than it plays).

## Testing an app

An app's own tests are Luau files named `*.test.luau` (in `tests/`, by custom). `zgx test` runs them, each file in a VM of its own, and exits 1 when any fails. `zgx test . sav` runs only the tests whose names hold `sav`.

```lua
-- tests/app.test.luau
test("adding a task counts it", function()
	local app = mount() -- the directory's app.luau, headless, at zgx.json's size
	app:type("#new", "Water the plants")
	app:key("Enter")
	expect(app:text("#count")).to_be("1 task")
	expect(app:has('role:checkbox "Water the plants"')).to_be(true)
	app:click('role:button "Clear done"')
	expect(app:wait("#empty", { timeout_ms = 1000 })).to_be(true)
end)
```

- **The globals.** A test file gets three:
  - `test(name, body)` adds a test. Tests run in the file's order, and each mounts fresh apps.
  - `mount(path?)` mounts the app headless (`path` from the app's directory).
  - `expect(actual)` checks a value: `to_be` (`==`), `to_equal` (tables field by field), `to_contain` (a substring, or a member of a list), `to_be_truthy` and `to_be_nil`. Each also has a negated form under `never`.
- **Failures** name the test file's line: an `expect` that fails, a target that matches nothing, an unknown key.
- **Targets are devtools' own** (`zgx inspect`): `#id`, `"label or text"`, `~"part of it"`, `role:button`, `role:checkbox "Done"`, `#list row 3`, `"Copy"[2]`, `#dialog >> "OK"`.
- **An app's methods:**
  - acting: `click`, `type` (click a field and type), `fill` (replace its whole text), `hover`, `drag(from, to)`, `key` (`"Enter"`, `"cmd+k"`), `scroll(target, dy)`, `command(id)` (a `ui.command`) and `resize(width, height)`;
  - reading: `text` (a field's value, or the text in the target), `selection` (a field's, in bytes from 0), `has`, `count` and `tree` (all of it, or from a target);
  - waiting: `frames(n)`, and `wait(target, { text?, gone?, timeout_ms? })`, which delivers frames until the target shows (holding `text`) or is gone and answers whether it did;
  - state: `state(name)` and `set_state(name, value)`;
  - `error()`, the script error showing, if any.
- **The desktop is a fake one.** The clipboard and dialogs work, and answer as the test says: `save_to(name)` is where the next `ui.save_file` saves, `pick({ names })` what the next `ui.open_files` picks, `write_file(name, text)` and `read_file(name)` make and read those files (in a directory of the app's own, gone with it), and `clipboard()` and `copy(text)` read and fill the clipboard. A dialog with no answer waiting is cancelled.
- **What a headless app lacks:** a fetcher (`ui.fetch` fails as it does offline) and storage (`ui.persist` keeps state in memory).
- **Types.** `zgx check` gives test files these globals' types (`types/zgx-test.d.luau`); the app's own scripts don't see them.

`zgx_script::testing::TestApp` is the same kit for Rust: it mounts a host headless, with the task executor and frame clock a window has, and drives it as a person would. Set up actions, the fetcher and shared state on the host before returning it; `TestApp::try_new` answers the script's error rather than panicking, and `TestApp::wrapped` provides what a window does, and `TestApp::with_desktop` a clipboard and file dialogs (`testing::FakeDesktop`) that answer as the test queues (`desktop.pick([path])`, `desktop.save_to(path)`), so open and save run as in a window. `TestApp::fill(id, text)` replaces a field's text as a person selecting it all and typing would, so a form's `dirty` and validation follow.

```rust
use zgx_script::{ScriptHost, testing::TestApp};

#[test]
fn signing_in_greets_the_user() {
    let mut app = TestApp::new(|runtime| {
        let host = ScriptHost::new(runtime, "ui/app.luau");
        host.async_action("sign_in", |_| async { Ok("Ada".into()) });
        host
    });
    app.type_text("email", "ada@example.com");
    app.click("sign-in");
    assert!(app.wait_until(|app| app.text("greeting") == "Hello, Ada"));
    let calls = app.calls();
    app.frames(30);
    assert_eq!(app.calls(), calls, "nothing runs while nothing changes");
}
```

It has `click`, `type_text`, `fill` (a field's whole text replaced), `hover` (the pointer moved over a node), `drag` (one node onto another, past the drag threshold), `scroll` (the viewport under a node), `key` and `key_with`, `frame` and `frames`, `settle` (run what is ready), `wait_until` (for host calls, fetches and timers, up to five seconds), `find`, `shown` (found and not transparent: a closed modal's kept content is not shown), `bounds`, `text`, `texts`, `opacity`, `error` and `calls` (calls into scripts: a hot path holds at zero). zgx-script's own tests use it.

## Shipping an app

In development a host reads its scripts from the disk and reloads them as they change (`ScriptHost::new` and `watch`). What ships carries them compiled into the binary instead: a build script bundles the directory, and the host runs from the bundle.

```rust
// build.rs (zgx-script as a build-dependency)
fn main() {
    zgx_script::build::bundle("ui", "app").unwrap();
}
// the app
static UI: zgx_script::Bundle = zgx_script::include_bundle!("app");
let host = ScriptHost::bundled(&cx.ui.runtime, &UI, "app.luau");
```

`bundle` compiles every `.luau` file to Luau bytecode (a script that does not parse fails the build, with its file and line) and keeps `.luaurc`, theme `.json` files and images as they are; Rust sources and other dotfiles stay out. The packages the directory's `zgx.json` lists come along, each under `.packages/<name>/`, its `.luaurc` aliases into one pointing at that copy, and the bundled host requires them as `@name/…`. A bundled host reads nothing from the disk: `require`, `@self`, `.luaurc` aliases, packages, `ui.theme.load`, `ui.image` and `ScriptHost::plugins` all resolve inside the bundle, and a module it does not have is an error, not a read elsewhere. The plugins an app sets (`set_plugins`) are its users', read from the disk; `watch` on a bundled host watches only those.

Errors are for the developer: `error_banner(true)` shows each over the interface with where it happened, and is the default in debug builds. In release builds the banner is off: a failed load or a handler's error leaves the last good interface running, and `host.on_error(|message| ...)` hears it, for the app's log or crash reports. The terminal example builds either way: with `--features terminal-bundle` it runs from its bundle (`ZGX_SCRIPTS=source` reads the source tree instead), without it from the source tree, reloading as it is edited; `scripts/zterm-app.sh` packages the bundled build.

## Theming

A style names a theme token as a string: `bg = "$surface.raised"`, `color = "$text.secondary"`, `p = "$space.3"`, `rounded = "$radius.md"`, `font = "$font.mono"`, `shadow = "$shadow.lg"`. The standard tokens, their file format, app tokens and costs are described in [styling: Theming](styling.md#theming); every theme defines the standard tokens, so a script styled with them takes any theme.

```lua
ui.theme.tokens { app = { card = "{surface.raised}", badge = "alpha({accent}, 0.2)" } }
ui.theme.default(ui.theme.load("brand.json")) -- the app's look, unless the user picks another
return ui.column {
	bg = "$background",
	p = "$space.4",
	gap = "$space.2",
	ui.text { "Settings", text_size = "$font.size.xl" },
	ui.div { bg = "$app.card", rounded = "$radius.lg", h = 80 },
	ui.button {
		"Light",
		on_click = function()
			ui.theme.set("zgx-light")
		end,
	},
	ui.column { theme = "zgx-light", bg = "$surface.raised", ui.text { "A light card" } },
}
```

Tokens are `"$name"` strings rather than values read from a `theme` table (`theme.text.secondary`) because a string is a reference: Rust interns it once, when the script is lowered, and the node styles itself from the token, so a theme switch restyles exactly the nodes that use the tokens that changed without calling the script, and without a function per property to make the value reactive. The strings work wherever a value does: in `hover` tables, in states, and returned from computed properties (`bg = function() return if on:get() then "$accent" else "$surface.base" end`).

**`ui.tokens`, typed.** `ui.tokens` names every token as a path of fields whose value is its string: `bg = ui.tokens.surface.raised` is `bg = "$surface.raised"`. Editors complete the next name (`ui.tokens.notes.` lists the app's notes tokens), and a typo is a type error (`Key 'wahs' not found`) rather than a transparent box. A group that is a token too has it as `value` (`ui.tokens.accent.value`, beside `ui.tokens.accent.hover`); a number in a name is an index (`ui.tokens.space[3]`). It is built from the applied theme the first time it is read and again once the tokens change, so a script that never reads it builds nothing, and a name no token has is an error at run time too. The standard tokens are in `zgx.d.luau`; for an app's own, run `zgx types <dir>`: it writes `<dir>/.zgx/types.d.luau`, zgx's definitions with the names the app's `ui.theme.tokens` tables declare, and an editor lists that file instead of `zgx.d.luau`. `zgx check` writes it too, and checks with it, and a debug build writes it again when a reload changes the names (where the file is). Leave `.zgx/` out of version control.

`"$…"` strings stay valid, and `zgx lint` checks them (`unknown-token`: a name nothing declares, with the nearest one; a token of the wrong kind for its field; a theme file setting an undeclared name). Props themselves are typed `string`, not a union of the token names: such a union makes Luau's new solver report false errors wherever a colour passes through state or a function, and check half as fast.

- `ui.theme.set(name)` applies a theme (and stops following the system); `ui.theme.default(name)` applies it unless the app has chosen one, so a reload does not undo a choice.
- `ui.theme.current` is state holding the applied theme's name: `ui.theme.current:get()` (tracked, so text or a computed property reading it follows) and `ui.theme.current:set(name)`. `ui.theme.kind()` is `"dark"`, `"light"`, `"high-contrast"` or `"high-contrast-light"`; `ui.theme.names()` lists the themes.
- `ui.prefers` is the system's display settings, as CSS's media queries read them. Each is a tracked state that follows the system: `ui.prefers.reduced_motion:get()` (`prefers-reduced-motion`; macOS's Reduce motion, Windows' animation effects off), `ui.prefers.more_contrast:get()` (`prefers-contrast: more`; macOS's Increase contrast, Windows' contrast themes) and `ui.prefers.reduced_transparency:get()` (`prefers-reduced-transparency`; macOS's Reduce transparency). The themes' tokens already take them: motion durations of zero, borders as strong as secondary text, and opaque, unblurred glass. A view with a backdrop blur paints its background solid then. Read them for what tokens do not cover, such as an `anime` timeline to skip.
- `ui.theme.load(path)` reads a theme file beside the script, or one a package ships (`ui.theme.load("@kit/theme.json")`), a zgx theme or a VS Code colour theme, and returns its name. The host watches it: saving it restyles the window in place, keeping state and without reloading the scripts; a file that stops reading shows its error until it reads again.
- `ui.theme.register(table)` registers a theme written in Luau (`{ name, type, extends, tokens }`, as a theme file); a module returning one reloads like any module.
- `ui.theme.tokens(table)` declares the app's tokens, each with a default written in standard tokens (`terminal = { pane = "{surface.sunken}" }`, nested or dotted), so that any theme styles the app; a theme may set them by name.
- `ui.theme.get(name)` returns a token's value (tracked): colours as `"#rrggbbaa"`, numbers, font names, shadows as `{ x, y, blur, spread, color }` lists. Canvas drawing and layout arithmetic use it.
- `ui.theme.follow_system({ dark = "…", light = "…" })` follows the system's appearance; `false` stops.
- `theme = …` on any element scopes its subtree: a theme's name (a light card in a dark app), or a table of tokens to override, with `theme = name` (or `extends`) to start from (`theme = { accent = "#e5484d" }`). It may be a state or a function; the scope follows it. Tokens derived from an overridden one follow, and inherited text takes the scope's colour.

**The design lint.** `ui.theme.strict("warn")` (or `"error"`, or `ZGX_DESIGN_LINT=warn`) reports every style that hard-codes a colour, spacing, corner radius, type size, weight, font or shadow, with the token to use: `` `bg = "#232e40"` hard-codes a colour … "$surface.raised" is exactly this in the current theme ``. `ui.raw(value, "reason")` lets a deliberate exception through. Sizes, `0`, `"auto"`, percentages, state values and theme definitions are not checked. See [Design systems](reference/design.md#the-design-lint).

A token nothing defines resolves to nothing (transparent, 0), and a debug build warns once. The terminal (`zgx-desktop/examples/terminal`: `theme.luau`, `glass.json`) and notes (`examples/notes`: `glass.json`) examples are written this way: their own themes reproduce their palettes, and `ui.theme.set("zgx-light")` restyles them whole.

## Animation: anime.js v4

The `anime` global follows anime.js v4's API. Animations run in a Rust engine: one task ticks every playing animation once per display frame, in one reactive batch, and writes translation, opacity, scale, rotation and skew straight to the scene, once per element and frame. Other properties (colors, sizes, radii, text size) go through the node's reactive style. Nothing runs while nothing plays: the engine asks for no frames at rest. Script code runs per frame only for callbacks a script sets (`onUpdate`, `onRender`), function eases and function modifiers. Methods work as `anim.play()` (as in JavaScript) and `anim:play()`.

```lua
anime.animate(".card", {
	x = { 0, 240 },
	opacity = 0.6,
	rotate = "1turn",
	delay = anime.stagger(60, { from = "center" }),
	duration = 900,
	ease = "outElastic(1, .5)",
	loop = true,
	alternate = true,
	onComplete = function(a)
		print("done")
	end,
})
```

| anime.js | Luau |
| --- | --- |
| `animate(targets, params)` | same; targets are selectors (`"#id"`, `".class"`, `"button"`, `"button.primary"`, comma lists), refs, elements, states, plain tables, or lists of them |
| `createTimeline(params)` | `tl:add(targets, params, position)`, `tl:add(timerParams, position)`, `set`, `call`, `label`, `sync`, `remove`; positions `500`, `"+=100"`, `"-=100"`, `"<"`, `"<<"`, `"<+=50"`, `"label"`, `"label+=50"`; `defaults` |
| `createTimer(params)` | endless unless given a `duration` |
| `createAnimatable(targets, { x = 400, y = { duration = 600, ease = "out(3)" } })` | `a.x(value, duration?, ease?)` animates; `a.x()` reads |
| `createDraggable(target, params)` | `x`/`y` (`true`, `false`, `{ snap }`), `container` (selector, element or `{ top, right, bottom, left }`), `containerPadding`, `containerFriction`, `snap`, `dragSpeed`, `velocityMultiplier`, `minVelocity`, `maxVelocity`, `releaseStiffness`, `releaseDamping`, `releaseMass`, `cursor`, `onGrab`, `onDrag`, `onUpdate`, `onRelease`, `onSnap`, `onSettle`; `enable`, `disable`, `reset`, `revert`, `stop`, `setX`, `setY`; `x`, `y`, `isDragging`, `velocity` |
| `onScroll(params)`, as `autoplay` | `container` (default: the nearest scroll ancestor), `target`, `enter`/`leave` (`"<container edge> <target edge>"`: `start center end`, `%`, points, `+=`/`-=`), `sync` (`true`, a smoothing amount, or an ease), `repeat`, `onEnter`, `onLeave`, `onUpdate`, `onSync` |
| `stagger(value, params)` | ranges `{ -100, 100 }`, units, `start`, `from` (`"first"`, `"center"`, `"last"`, `"random"`, index), `reversed`, `ease`, `grid`, `axis`, `use`, `total`, `modifier` |
| `createSpring({ mass, stiffness, damping, velocity })`, `spring({ bounce, duration })` | an ease that sets the duration it settles in |
| `eases.*`, ease strings | `linear`, `in(p)`, `out(p)`, `inOut(p)`, `outIn(p)`, Quad to Quint, Sine, Circ, Expo, Bounce, `Back(overshoot)`, `Elastic(amplitude, period)`, `steps(n, fromStart)`, `cubicBezier(…)`, `linear(…)`, `irregular(…)`; functions; v3 names (`easeInOutQuad`) |
| `createScope({ defaults })` | `scope:add(fn)`, `scope:revert()` |
| `text.split(text, params)` | returns a node: one text per character (`chars`) or word (`words`), with a `class` to animate; `style` styles each piece, `props` the row |
| `svg.createMotionPath(d)` | SVG path data; returns `translateX`, `translateY`, `rotate` to animate along it; `svg.getTotalLength(d)` |
| `utils` | `set`, `get`, `remove`, `cleanInlineStyles(targets, props?)` (only the named properties when given; each returns to the node's own value), `random`, `randomPick`, `shuffle`, `round`, `clamp`, `snap`, `wrap`, `mapRange`, `lerp`, `damp`, `degToRad`, `radToDeg`, `roundPad`, `padStart`, `padEnd`, `createSeededRandom`; called without the value, `round` returns a modifier that runs in Rust |
| `engine` | `speed`/`playbackRate`, `fps`, `timeUnit` (`"ms"` or `"s"`), `currentTime`, `pause()`, `resume()` |
| `anime({ targets = …, … })` | anime.js v3's form |

Elements also animate `blur` (the content's), `maskX` and `maskY` on the compositor.

Parameters: `duration`, `delay`, `ease`, `composition` (`"replace"`, `"none"`, `"add"`), `modifier`, `loop` (count or `true`), `loopDelay`, `alternate`, `reversed`, `autoplay`, `frameRate`, `playbackRate`, `playbackEase`, `keyframes` (a list of property sets, or `{ ["0%"] = …, ["50%"] = … }`), callbacks `onBegin`, `onBeforeUpdate`, `onUpdate`, `onRender`, `onLoop`, `onPause`, `onComplete`. Property values: numbers, strings with units (`px`, `%`, `rem`, `em`, `deg`, `rad`, `turn`), relative (`"+=10"`, `"-=10"`, `"*=2"`), colors, strings with numbers in them, functions of `(target, index, total)`, staggers, value keyframes (`{ from, …, to }`), and tween objects (`{ to, from, duration, delay, ease, composition, modifier }`), alone or in lists. Bare numbers for angles are degrees, as in anime.js.

Controls: `play`, `pause`, `resume`, `restart`, `reverse`, `alternate`, `seek(time)`, `complete`, `cancel`, `revert`, `reset`, `stretch(duration)`, `refresh`, and `done(fn)` (anime.js `then`, a Luau keyword; `a["then"](fn)` also works). Fields: `id`, `currentTime` (settable), `progress` (settable), `iterationProgress`, `currentIteration`, `duration`, `iterationDuration`, `paused`, `began`, `completed`, `reversed`, `speed`, `labels`.

Scripts create animations before the elements they target mount: an animation resolves its targets the first time it plays, and draggables and scroll observers attach once their targets mount. Draggables and scroll-linked animations live without a script handle, until their script is replaced.

The `ui.tween(state, opts)`, `ui.spring(state, opts)` and `ui.loop(opts)` bindings remain for declarative transitions: a property bound to one eases whenever the state changes, with any anime.js ease. Their `duration` is in milliseconds, as anime's is (`ui.tween(open, { duration = 160 })`; 250 by default, a loop's cycle 1000).

### Cost

**Release builds need `LUAU_CXXFLAGS=-DNDEBUG`.** Luau checks its asserts unless `NDEBUG` is defined, and the crate that compiles it for mlua never defines it: with them, every script loop, call and table access runs about a fifth slower (`list_state_bench`: 6.3 M instructions a tick against 5.0 M). This workspace sets it in `.cargo/config.toml`; an app built elsewhere sets it in its own:

```toml
# .cargo/config.toml
[env]
LUAU_CXXFLAGS = "-DNDEBUG"
```

A release build without it says so once on stderr and in `ScriptHost::warnings()`. Debug builds keep the asserts either way.

`anime_bench` animates 1,000 elements' `y` and `opacity` every frame, headless, and times one frame (Apple M5 Max, release):

| | per frame |
| --- | --- |
| Luau + anime engine | about 90 µs |
| The same motion in Rust, a signal and reactive style per element | about 240 µs |
| The same values written straight to the scene (no engine) | about 120 µs |

Writing to the scene directly is the floor for the work itself; the engine is below it because it skips values that did not change and writes a moving, fading element's translation and opacity together, damaging each look once. What remains is the scene's own invalidation.

`script_layout_bench` (`cargo run --release -p zgx-script --example script_layout_bench`) reverses 1,000 rows of three heights in a scroll view (same machine, release, mean of 20):

| | the change | its layout frame | each moving frame |
| --- | --- | --- | --- |
| `ui.each` | 0.36 ms | 0.25 ms | (none) |
| `ui.each` with `animate` | 0.51 ms | 0.55 ms | 0.13 ms, nothing laid out or painted |

The change reads every row's place once, the frame that lays it out reads them again and starts one animation for all the rows that moved, and the frames after only composite. Once the rows settle nothing asks for a frame.

**Keep hot paths out of the script.** Work a script does on every frame of a resize or an animation is work Rust could do without it. Say what depends on the size, not what it comes to: a box placed by percentages and points (`left = "40%", ml = 2`, both insets of an axis to stretch between them), `grow` and `basis`, or `layout = "size"` is resolved by the layout engine on every frame, while the script runs only when what it describes changes. The terminal example's split panes work this way: `lib/layout.luau` turns a split tree into each pane's edges as a percentage of the content area plus points (ratios scale, gaps add), `frames.luau` gives those to the cards as insets with `snap = true` (whole-point edges, so 1-point borders stay crisp at any ratio), and resizing the window or easing the sidebar open runs no script at all; a change of structure eases cards with `anime` and hands them back with `anime.utils.cleanInlineStyles`. Moving it out of the script cut the frames of a sidebar animation from about 0.55 ms of Luau each to none. `ScriptHost::script_calls()` counts calls into scripts, so a test can hold a hot path to zero (`resizing_the_window_moves_the_panes_without_running_script` in `tests/terminal_example.rs`).

Rotation, scale and skew promote a node to a cached layer: its subtree is painted once into a texture, and each frame only composites that texture under a new matrix (no layout, no repaint). Scales re-rasterize at steps of √2, so a scale animation repaints a handful of times, not every frame. Each transformed node is its own texture and draw: thousands of them at once are bounded by the renderer's layer budget (64 MiB, 1,024 textures). Hit testing follows the transform; positions inside widgets that track the pointer themselves (a slider's thumb, a text caret) do not yet account for a rotated or scaled ancestor.

## Comparison with Rust views

The streaming example (`stream_luau` against `stream_rust`, 60 tokens a second into a transcript) measured on macOS: streaming CPU within noise (2.6% each), about 2% more instructions for Luau, and 0.9 MiB more memory (a Luau VM and the script's tree). Idle CPU is zero for both.
