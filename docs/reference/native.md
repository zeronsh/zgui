# Web views, terminals, commands: webview, web, terminal, term, command

Native content laid out as nodes, and the app's menu commands. A web view is a WebKit page (macOS), a WebView2 page (Windows, the desktop host's `webview2` feature) or a WebKitGTK page drawn as an image (Linux, the `webkitgtk` feature, which `zgx-run` has; see [the Linux notes](../linux.md#web-views)), and a terminal is drawn by zgx (every platform, the desktop host's `terminal` feature, which zgx-run turns on) or is a Ghostty surface (macOS, the `ghostty` feature, which wins there); each is placed, sized, rounded and clipped like any node, and zgx draws over it (a frosted popover over a page works). Without a native host (headless, other platforms) each shows a labelled placeholder box and its controller still answers. Pages and sessions belong to the host, not to a run, so a reload keeps them.

```luau
ui.webview(props: WebViewProps) -> Node
ui.web(opts: WebOptions?) -> Web
ui.web(id: string, opts: WebOptions?) -> Web
ui.terminal(props: TerminalProps) -> Node
ui.term(id: string, opts: TermOptions?) -> Term
ui.command(opts: CommandOptions) -> ()
```

| Function | |
| --- | --- |
| `webview` | shows a page: its own, or a `ui.web` controller's (`web = …`) |
| `web` | a page's controller, usable before and after its view mounts. `ui.web(opts)` makes a page for this run; `ui.web(id, opts)` returns the host's page for `id`, made with `opts` the first time, kept across reloads until `web:close()` |
| `terminal` | shows a session (`term = ui.term(id)`) |
| `term` | the session for `id`, started the first time with `opts`; later calls return the same session, their handlers replacing the previous run's |
| `command` | an app command: the host shows it as a menu item (`ScriptHost::commands`) and runs it when chosen, its key working wherever focus is |

## Web view options

`ui.webview` takes these, `ui.web` the first six, and any [property](properties.md) and [event](events.md):

| Option | Type | Default | |
| --- | --- | --- | --- |
| `transparent` | `boolean` | `false` | no page background of its own: the node's shows where the page is transparent |
| `profile` | `string` | `"default"` | `"default"` shares cookies and storage, `"ephemeral"` keeps nothing, any other name is a persistent store of its own |
| `inspectable` | `boolean` | `true` in debug builds | Web Inspector (`web:devtools()`) |
| `user_agent` | `string` | WebKit's | |
| `messages` | `boolean` | `true` | the page's `window.zgx.postMessage(value)` reaches `on_message` |
| `new_windows` | `boolean` | `true` | `window.open` and `target=_blank` reach `on_new_window` (and open nothing) |
| `web` | `Web` | a page for the node | a controller, so the page outlives the node (a tab switched away and back) |
| `src` | `Reactive<string>` | none | the URL; the page navigates when it changes |
| `html`, `base_url` | `string`, `string?` | none | a page from markup, loaded once |
| `on_navigate` | `(e: WebEvent) -> ()` | none | `e` is `{ kind, url }`, `kind` one of `"started"`, `"committed"`, `"finished"`, `"failed"`, `"changed"` |
| `on_load` | `(url: string) -> ()` | none | a navigation finished |
| `on_title` | `(title: string) -> ()` | none | |
| `on_message` | `(value: any, origin: string, main_frame: boolean) -> ()` | none | what the page posted, as Luau data, and who posted it: the sending frame's origin (`https://example.com`) and whether it is the top frame. Any page the view shows, and any iframe in it, can post: check `origin` before acting on a message |
| `on_error` | `(url: string, err: string) -> ()` | none | a navigation failed, or the page's process ended (`url` empty) |
| `on_new_window` | `(url: string) -> ()` | none | |

A `Web` controller's `url`, `title`, `loading`, `progress`, `can_go_back`, `can_go_forward`, `focused`, `error` and `pointer` (`{ x, y }` in points while the pointer is over the page, else nil) are tracked like state: a text or computed reading them follows the page. Its methods: `load(url)`, `html(html, base_url?)`, `back()`, `forward()`, `reload()`, `stop()`, `focus()`, `blur()`, `devtools()`, `zoom(z)`, `run(js)`, `eval(js, function(value, err) … end)` (the script's value as Luau data, or nil and the error), and `close()`.

## Terminal options

`ui.term` takes these; `ui.terminal` takes `term` and any property and event:

| Option | Type | Default | |
| --- | --- | --- | --- |
| `term` (`terminal`) | `Term` | required | the session to show |
| `command` | `string` | the login shell | a command line |
| `cwd` | `string` | home | where it starts; `"~"` is home |
| `env` | `{ [string]: string }` | none | added to the environment |
| `font_size` | `number` | 13 (Ghostty's own, under Ghostty) | points |
| `on_title` | `(title: string) -> ()` | none | the program set its title (OSC 0/2) |
| `on_pwd` | `(pwd: string) -> ()` | none | the shell reported its directory (OSC 7) |
| `on_process` | `(name: string) -> ()` | none | the foreground program changed |
| `on_exit` | `(code: number) -> ()` | none | its process ended |
| `on_close` | `(alive: boolean) -> ()` | none | it asks to close: its process exited (`alive = false`), or a close binding ran with something still running |
| `on_bell` | `() -> ()` | none | |
| `on_command` | `(exit_code: number?, duration_ms: number) -> ()` | none | a command finished (shell integration) |
| `on_notify` | `(title: string, body: string) -> ()` | none | the program asked for a notification |

A `Term`'s `id`, `title`, `pwd`, `process`, `pid`, `focused`, `bell` (it rang since it last had focus), `progress` (OSC 9;4: 0 to 1, negative while indeterminate, nil when none), `exited` (the exit code, once it ended), `columns`, `rows` and `pointer` are tracked like state. Its methods: `focus()`, `blur()`, `input(text)` (as typed: `"ls\r"` runs ls), `paste(text)`, `binding(action)` (a Ghostty binding action: `"copy_to_clipboard"`, `"clear_screen"`, `"increase_font_size:1"`), `needs_confirm()` (whether closing would end something other than an idle shell), `snapshot(max_width?)` (a picture of what it shows now, at most `max_width` pixels wide, 480 if not given: the `src` of a `ui.image` showing it, as `"host:…"`, replaced by each call and forgotten as it closes; nil before it shows or where the platform takes none, macOS taking it from Ghostty's last frame) and `close()`.

## Command options

| Option | Type | Default | |
| --- | --- | --- | --- |
| `id` | `string` | required | unique among the run's commands |
| `title` | `string` | the id | the menu item's text |
| `menu` | `string` | `"File"` | `"File"`, `"Edit"`, `"View"`, `"Window"` or any other menu |
| `key` | `string` | none | modifiers and a key joined by `+`: `"cmd+shift+d"`, `"cmd+alt+arrowleft"`, `"cmd+["`, `"ctrl+tab"`. `cmd` is Command on macOS and Control elsewhere; `super` names the Windows or Super key itself. `ui.shortcut_text(keys)` shows keys as the platform does (`"cmd+shift+z"`: ⇧⌘Z on macOS, Ctrl+Shift+Z elsewhere), for a tooltip or a list of keys |
| `run` | `() -> ()` | required | called when it is chosen |
| `separator` | `boolean` | `false` | a separator line in `menu` instead of a command (`id` and `run` are not needed) |

Commands are the app's menu bar, and their keys are its menu items' shortcuts: `zgx run` (and `zgx_run::run_hosting`, for a Rust host serving its script a fetcher or actions) installs them. A Rust host that makes its own window gets the same with `zgx_run::serve_commands(&host, cx)` in its `Application::run`, keeping the effect it returns; headless, a test runs a command with `ScriptHost::run_command(id)`.

## Windows

```luau
ui.window(opts: WindowOptions) -> ()
ui.windows(list: State<{ any }>, build: (item: any) -> ItemWindow, opts: { key: ((any) -> string | number)?, on_close: ((any) -> ())? }?) -> ()
```

`ui.window { id, open = state, node }` declares a window of the app's own, a settings window say: it is open while `open` is true, closing it sets `open` false, and its node is its content. Declared at a script's top level, as commands are, each load declares them again; what a window shows follows reloads. The windows share the app's state: a switch in the settings window changes what the main window shows (and `ui.theme.set` restyles every window). `zgx-run` opens them, and a Rust host opens them from `ScriptHost::windows()` with `ScriptHost::window_view(id)` (`zgx_run::follow_windows` does both).

| Option | Type | Default | |
| --- | --- | --- | --- |
| `id` | `string` | required | unique among the run's windows |
| `open` | `State<boolean>` | required | the window shows while true |
| `title` | `string` | the id | |
| `width`, `height` | `number` | `480`, `360` | points |
| `frame_autosave` | `string` | none | its frame saved under this name as it moves and resizes, and reopened where it was left (macOS, Windows, X11; on Wayland its size and maximized state) |
| `tabbing` | `"automatic"`, `"preferred"`, `"disallowed"` | `"automatic"` | AppKit's window tabs (macOS): as the user's setting says, always, or never (a window with tabs of its own) |
| `under_pointer` | `{ number }` | none | where it opens: its top-left this far up and left of the pointer, as `on_drag_outside`'s `{ e.grab_x, e.grab_y }`, so a tab torn off stays under the grab (macOS, Windows, X11) |
| `[1]` | `Node` | required | its content |

```lua
local settings = ui.state("settings", false)
local dark = ui.persist("dark", true)
ui.command {
	id = "settings",
	title = "Settings…",
	key = "cmd+,",
	run = function()
		settings:set(true)
	end,
}
ui.window {
	id = "settings",
	title = "Settings",
	width = 420,
	height = 240,
	open = settings,
	ui.column { p = 24, ui.checkbox { label = "Dark appearance", value = dark } },
}
return ui.button { on_click = ui.set(settings, true), "Settings…" }
```

### A window an item

`ui.windows(list, build, { key, on_close })` is a window an item of a list, as `ui.each` is a row an item: a detail window per photo, a tab torn off into a window of its own. An item's window opens as the item appears in the list and closes as it leaves. `build(item)` gives its options, `ui.window`'s without `id` and `open`, and its content, once, as the item first appears; what the content reads follows state as any node does. `key(item)` names an item (its `id` field by default): an item that keeps its key keeps its window. A person closing a window calls `on_close(item)`, for the app to let the item go; until it does, the window stays closed. Declared at the top level, as `ui.window` is, and a reload keeps the windows whose items are still in the list.

```lua
local details = ui.persist("details", {} :: { { id: string, name: string } })
ui.windows(details, function(photo)
	return {
		title = photo.name,
		width = 480,
		height = 360,
		frame_autosave = `detail:{photo.id}`,
		ui.column { p = 16, ui.text { photo.name, literal = true } },
	}
end, {
	on_close = function(photo)
		details:update(function(list)
			local kept = {}
			for _, d in list do
				if d.id ~= photo.id then
					table.insert(kept, d)
				end
			end
			return kept
		end)
	end,
})
return ui.button { on_click = function() details:push({ id = "p1", name = "Harbour" }) end, "Open Harbour" }
```

A list in `ui.persist` reopens its windows on the next launch, each where `frame_autosave` left it.

### The title bar

```luau
ui.window.titlebar_area: State<WindowRect?>
ui.window.controls: State<WindowRect?>
```

Content under an inset or hidden title bar stays clear of the window's own buttons by reading where it may draw, as the web's Window Controls Overlay gives it. `ui.window.titlebar_area` is the strip the title bar leaves, less the window's buttons. `ui.window.controls` is the buttons. Each is `{ x, y, width, height }` in points from the window's top left, or nil for a standard title bar, in fullscreen, and where the platform does not say (macOS and Linux, for now). Both follow resizes, scale and fullscreen. `zgx run` keeps them; a Rust host passes its window's with `ScriptHost::window_chrome`.

```lua
return ui.row {
	h = 44,
	window_drag = true,
	items = "center",
	-- Clear of Windows' buttons at the right, or macOS's at the left.
	pl = function()
		local area = ui.window.titlebar_area:get()
		return if area then area.x + 12 else 84
	end,
	pr = function()
		local area, buttons = ui.window.titlebar_area:get(), ui.window.controls:get()
		return if area and buttons and buttons.x >= area.x + area.width then buttons.width + 12 else 12
	end,
	ui.text { "Title" },
}
```

### Zoom

```luau
ui.window.zoom: State<number>
ui.window.zoomable: State<boolean>
```

The app's window zooms as a browser's page does: Cmd+= or Cmd++ zooms in, Cmd+- out and Cmd+0 back to 100% (Ctrl elsewhere), through Chrome's levels from 25% to 500%, and its content reflows in the smaller or larger viewport, text and paths drawn crisply. `ui.window.zoom` is the zoom (1 at 100%): it follows the keys and the View menu's items, and setting it zooms the window (`ui.window.zoom:set(1.25)`). `ui.window.zoomable:set(false)` turns the keys off; a `ui.command` with one of the keys keeps it either way. `zgx run` keeps both; a Rust host passes its window with `ScriptHost::window_zoom`.

### Size

```luau
ui.window.size: State<{ width: number, height: number }>
```

The app's window's size in points, read-only, followed as the window resizes, zooms or moves between displays, as the web's `innerWidth` and `innerHeight` are. It is for decisions layout can't make: a sidebar collapsing to icons below a width, a menu taking the place of tabs. Sizes themselves are layout's (percentages, `grow`, `min_w` and `max_w`), which follows a resize without the script, and `zgx lint` reports an element's size computed from it (`window-size-layout`). A ref's `width` gives the room one element has. `zgx run` keeps it; a Rust host passes its window's `WindowContext::viewport` with `ScriptHost::window_size`.

```lua
local narrow = ui.computed(function()
	return ui.window.size:get().width < 700
end)
return ui.row {
	ui.column {
		w = function()
			return if narrow:get() then 48 else 240
		end,
		ui.text { "Sidebar" },
	},
	ui.column { grow = 1, ui.text { "Content" } },
}
```

### Notifications and badges

```luau
ui.window.active: State<boolean>
ui.window.shown: State<boolean>
ui.notify(opts: { title: string, body: string?, tag: string?, on_click: (() -> ())? }) -> Promise<nil>
ui.badge(count: number | string | nil) -> boolean
```

`ui.window.active` says whether the app's window is the active one, its keys going to it; `ui.window.shown`, whether it shows (not hidden, minimized or covered), as the web's `document.hasFocus()` and `document.visibilityState` do. Both are read-only and follow the window. `zgx run` keeps them; a Rust host passes its window with `ScriptHost::window_state`. What arrives while either is false is worth telling the person of:
- `ui.notify { title, body, tag }` shows a system notification, as the web's `Notification` does. The promise settles once the system has it, or rejects with a `UiError` saying why not: `kind` `"unsupported"` where this system shows none for the app, `"denied"` where the person has not allowed them. One with the same `tag` replaces the last (a conversation's newest message, not one for each). `on_click` runs when the person clicks it, the app's window brought forward first: open the conversation it is about. A click is heard while the app runs.
- `ui.badge(n)` puts a count on the app's icon, as `navigator.setAppBadge` does; `0` or `nil` takes it off. It returns whether the system shows one.

Each platform shows them its own way, and needs the app to say who it is:

| | Notifications | Badge |
| --- | --- | --- |
| macOS | The Notification Center, for an app run from its bundle (an `Info.plist` with a `CFBundleIdentifier`). The first one asks the person whether the app may notify; the answer is the system's to keep, changed in System Settings › Notifications. A binary run outside a bundle (`cargo run`) gets `"unsupported"`. | The Dock tile's label. |
| Windows | A toast, for the app's AppUserModelID: its `application_id` (`Application::application_id`, `zgx.json`'s `application_id`). Without one, `"unsupported"`. The first notification sets it as the process's AppUserModelID and registers it for the user, as an unpackaged app's toasts need: the key `HKEY_CURRENT_USER\Software\Classes\AppUserModelId\<application_id>` with a `DisplayName` value, the executable's name, which the toast shows as its sender. Nothing else is written, and nothing needs administrator rights. | An overlay on the taskbar button (`ITaskbarList3::SetOverlayIcon`): a red disc with the count (`9+` past 99), a plain disc for text, described to screen readers as "3 unread" (or the text). |
| Linux | The desktop's notification server (`org.freedesktop.Notifications` on the session bus); none running gets `"unsupported"`. | The count docks read from a launcher entry (Dash to Dock, KDE's task manager, Plank), matched to the app's desktop file (`Application::desktop_id`, else `application_id`). |

A plugin can do neither: they are the app's.

```lua
local unread = ui.state("unread", 0)
local function arrived(from: string, text: string)
	if ui.window.active:peek() then
		return
	end
	unread:set(unread:peek() + 1)
	ui.badge(unread:peek())
	ui.notify({ title = from, body = text, tag = from }):after(nil, function(err: UiError)
		print(`no notification: {err}`)
	end)
end
ui.effect(function()
	if ui.window.active:get() then
		unread:set(0)
		ui.badge(0)
	end
end)
return ui.button {
	"Pretend a message",
	on_click = function()
		arrived("Ada", "Lunch?")
	end,
}
```

## Example

```lua
local url = ui.state("url", "https://example.com")
local web = ui.web("docs", { profile = "ephemeral" })
local term = ui.term("main", {
	on_title = function(title)
		print("terminal:", title)
	end,
})

ui.command {
	id = "reload",
	title = "Reload Page",
	menu = "View",
	key = "cmd+r",
	run = function()
		web:reload()
	end,
}
ui.command { menu = "View", separator = true }
ui.command {
	id = "clear",
	title = "Clear Terminal",
	menu = "View",
	key = "cmd+k",
	run = function()
		term:binding("clear_screen")
	end,
}

return ui.row {
	w_full = true,
	h_full = true,
	p = 8,
	gap = 8,
	items = "stretch",
	ui.column {
		grow = 1,
		gap = 6,
		ui.row {
			gap = 6,
			items = "center",
			ui.button {
				disabled = function()
					return not web.can_go_back
				end,
				on_click = function()
					web:back()
				end,
				"Back",
			},
			ui.text {
				function()
					return if web.loading then "Loading…" else web.title
				end,
				truncate = true,
				shrink = 1,
				min_w = 0,
			},
		},
		ui.webview {
			web = web,
			src = url,
			grow = 1,
			rounded = 10,
			on_navigate = function(e)
				print(e.kind, e.url)
			end,
		},
	},
	ui.terminal { term = term, grow = 1, rounded = 10 },
}
```

## Gotchas

- `ui.term(id)` without options only looks a session up, keeping its handlers; with options it replaces them with this run's. Either starts one, with default options, when `id` has none (or its session closed). Give the options wherever the session may start.
- Sessions outlive reloads and the nodes that show them: a reload keeps every shell running, and moving the `terminal` node (another split, another tab) moves the terminal with its process. End one with `term:close()`, usually from `on_close`.
- `ui.term` rejects an unknown option. The `on_*` options are a session's events, not a node's: give them to `ui.term`, not `ui.terminal`.
- A page from `ui.web(id)` is kept until `web:close()`; `ui.web(opts)` and a `webview` without `web` make a page for the run (a reload makes a new one).
- Over a page or a Ghostty terminal that takes input, zgx's nodes hear no pointer moves (they had their leave): follow `web.pointer` or `term.pointer` instead (an engine-drawn terminal reports `pointer` too).
- The engine-drawn terminal's fidelity, keys and limits (no images, no kitty keyboard protocol yet, clipboard reads unanswered) are in [native surfaces](../native-surfaces.md#terminals). The terminal reads its own keyboard, IME and mouse; app shortcuts reach the app through its commands, whose keys work wherever focus is.
- Terminals hand the right clicks their program does not use to the node, so a `context_menu` on the `terminal` node opens over it.
- Each successful load declares its commands afresh, replacing the last; a failed reload keeps them. Declaring an id twice in one run is an error.
- `on_message` and `eval` get the page's JSON as Luau data, typed `any`: check its shape before using it.
