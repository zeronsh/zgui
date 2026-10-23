# Web views, terminals, commands: webview, web, terminal, term, command

Native content laid out as nodes, and the app's menu commands. A web view is a WebKit page (macOS) or a WebView2 page (Windows, the desktop host's `webview2` feature; Linux has none yet, see [the Linux notes](../linux.md#web-views)), and a terminal a Ghostty surface (macOS, the `ghostty` feature); each is placed, sized, rounded and clipped like any node, and zgui draws over it (a frosted popover over a page works). Without a native host (headless, other platforms) each shows a labelled placeholder box and its controller still answers. Pages and sessions belong to the host, not to a run, so a reload keeps them.

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
| `messages` | `boolean` | `true` | the page's `window.zgui.postMessage(value)` reaches `on_message` |
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
| `font_size` | `number` | Ghostty's | points |
| `on_title` | `(title: string) -> ()` | none | the program set its title (OSC 0/2) |
| `on_pwd` | `(pwd: string) -> ()` | none | the shell reported its directory (OSC 7) |
| `on_process` | `(name: string) -> ()` | none | the foreground program changed |
| `on_exit` | `(code: number) -> ()` | none | its process ended |
| `on_close` | `(alive: boolean) -> ()` | none | it asks to close: its process exited (`alive = false`), or a close binding ran with something still running |
| `on_bell` | `() -> ()` | none | |
| `on_command` | `(exit_code: number?, duration_ms: number) -> ()` | none | a command finished (shell integration) |
| `on_notify` | `(title: string, body: string) -> ()` | none | the program asked for a notification |

A `Term`'s `id`, `title`, `pwd`, `process`, `pid`, `focused`, `bell` (it rang since it last had focus), `progress` (OSC 9;4: 0 to 1, negative while indeterminate, nil when none), `exited` (the exit code, once it ended), `columns`, `rows` and `pointer` are tracked like state. Its methods: `focus()`, `blur()`, `input(text)` (as typed: `"ls\r"` runs ls), `paste(text)`, `binding(action)` (a Ghostty binding action: `"copy_to_clipboard"`, `"clear_screen"`, `"increase_font_size:1"`), `needs_confirm()` (whether closing would end something other than an idle shell) and `close()`.

## Command options

| Option | Type | Default | |
| --- | --- | --- | --- |
| `id` | `string` | required | unique among the run's commands |
| `title` | `string` | the id | the menu item's text |
| `menu` | `string` | `"File"` | `"File"`, `"Edit"`, `"View"`, `"Window"` or any other menu |
| `key` | `string` | none | modifiers and a key joined by `+`: `"cmd+shift+d"`, `"cmd+alt+arrowleft"`, `"cmd+["`, `"ctrl+tab"` |
| `run` | `() -> ()` | required | called when it is chosen |
| `separator` | `boolean` | `false` | a separator line in `menu` instead of a command (`id` and `run` are not needed) |

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
- Over a page or a terminal that takes input, zgui's nodes hear no pointer moves (they had their leave): follow `web.pointer` or `term.pointer` instead. The terminal reads its own keyboard, IME and mouse; app shortcuts reach the app through its commands, whose keys work wherever focus is.
- Terminals hand the right clicks their program does not use to the node, so a `context_menu` on the `terminal` node opens over it.
- Each successful load declares its commands afresh, replacing the last; a failed reload keeps them. Declaring an id twice in one run is an error.
- `on_message` and `eval` get the page's JSON as Luau data, typed `any`: check its shape before using it.
