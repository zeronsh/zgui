# State and components

State is Rust-owned and keyed, so a reload keeps it. A script reads it in text templates, function properties, computeds and effects, which follow it; handlers change it with its methods or with actions Rust runs. Components build part of the interface per instance, with their own state.

```luau
ui.state<T>(key: string, initial: T) -> State<T>
ui.shared<T>(key: string, initial: T) -> State<T>
ui.persist<T>(key: string, initial: T) -> State<T>
ui.use_state<T>(initial: T) -> State<T>
ui.computed<T>(f: () -> T) -> State<T>
ui.effect(f: () -> ...unknown) -> ()
ui.batch<R...>(f: () -> R...) -> R...
ui.ref() -> Ref
ui.component<P>(build: (props: P) -> Node) -> (props: P) -> Node
ui.on_mount(f: () -> ...unknown) -> ()
ui.slot(name: string, props: Props?) -> Node
ui.contribute(name: string, node: Node) -> ()
ui.set<T>(state: State<T>, value: T) -> Action
ui.toggle(state: State<boolean>) -> Action
ui.add(state: State<number>, by: number?) -> Action
ui.action(name: string, arg: unknown?) -> Action
ui.invoke(name: string, arg: unknown?) -> ()
ui.call(name: string, arg: unknown?) -> Promise
ui.task<A...>(f: (A...) -> (), A...) -> ()
ui.fetch(url: string, opts: FetchOptions?) -> Promise<Response>
ui.clipboard.read() -> string?
ui.clipboard.write(text: string) -> boolean
ui.open_files(opts: FilePickerOptions?) -> Promise<{ File }>
ui.save_file(opts: FilePickerOptions?) -> Promise<File?>
ui.read_file(file: File | string) -> Promise<string>
ui.write_file(file: File | string, text: string) -> Promise<nil>
```

## Making state

| Function | Kept by | |
| --- | --- | --- |
| `ui.state(key, initial)` | the script (or module) and `key` | private to the script; a reload gets the same state back, `initial` only counts the first time |
| `ui.shared(key, initial)` | `key`, for the whole host | shared with Rust (`ScriptHost::shared`, see below) and every script and plugin |
| `ui.persist(key, initial)` | the script (or module) and `key`, on disk | as `ui.state`, and saved for the next run: each key a JSON file in the host's storage (`ScriptHost::app_id` or `storage`), written within 250 ms of a change, off the main thread. Without storage it lasts until the app quits, with a warning |
| `ui.use_state(initial)` | the component instance | a new state per instance; only inside a component's function (or an `each` row, `presence` content, a `list` row) |
| `ui.computed(f)` | what built it | derived: `f` runs now and again when what it reads changes; its value is the state's |

State holds `nil`, booleans, numbers, strings, lists, string-keyed tables and elements (`state:set(el)`; Rust sees `Value::Element`, `null` as JSON). Not functions, not refs (keep the ref itself, or store `ref:get()`), and not a table that is both a list and has string keys.

### Shared state from Rust

`ScriptHost::shared(key, initial)` gives Rust a `Shared`: read it as the signal it derefs to, and publish with `set`, which reconciles: the new value keeps the current one's parts wherever they are equal (list items that are tables with an `id` are matched by it, others by position), so a model rebuilt and republished whole on every change (a transcript as a reply streams) is the same value everywhere it did not change. Equal throughout, nothing is notified. Scripts get the same read-only tables for the parts that stayed, converted once (large values are converted in parts and kept while the value lives), `ui.each` skips their rows at a pointer's glance, and a table read from state and written back is its value again without a walk. `Shared::signal()` is the plain signal, whose `set` replaces without reconciling.

The type is the initial value's: `ui.state("count", 0)` is a `State<number>`. Give `nil` and empty tables a type:

```luau
local selected = ui.state("selected", nil :: string?)
local tabs = ui.state("tabs", {} :: { Tab })
```

## State methods

| Method | | |
| --- | --- | --- |
| `get(): T` | the value; inside a computed, effect, function property or text function it subscribes to the state | |
| `peek(): T` | the value, without subscribing | for handlers and for reading what an effect should not follow |
| `set(value: T)` | replaces the value | |
| `update(f: (value: T) -> ...T)` | `f` gets a mutable copy: change it and return nothing, or return the new value | copies the whole value into Luau and back |
| `set_key(path, value)` | one key (`"title"`), a list index from 1 (`3`), or a path (`{ "tabs", 3, "title" }`); `nil` removes a key | makes missing tables along the path; an index may be one past the end (appends); a list cannot get a hole (`nil` removes only its last item); the value is not type checked |
| `patch(fields: { ... })` | a shallow merge of string keys | each field as `set_key(name, value)` |
| `toggle()` | `not value` | |
| `add(by: number?)` | adds `by` (1) to a number; anything else counts as 0 (a boolean as 0 or 1) | |
| `push(item)` | appends to a list (`nil` is an empty list) | |
| `insert(index: number, item)` | inserts at 1 to `len + 1` | |
| `remove(index: number): item?` | removes and returns the item; nil, and nothing changes, outside the list | |
| `len(): number` | the list's length (0 for anything else); tracked as `get` is | |

`set_key`, `patch`, `insert`, `push` and `remove` copy only the path they change, sharing the rest with the old value; prefer them to `update` for large state. A write of an equal value notifies nobody.

## Actions

Handlers take a function or an action Rust runs without calling the script:

| Action | Does |
| --- | --- |
| `ui.set(state, value)` | `state:set(value)` |
| `ui.toggle(state)` | `state:toggle()` |
| `ui.add(state, by)` | `state:add(by)` (`by` is 1 by default) |
| `ui.action(name, arg)` | calls the host's `ScriptHost::action(name, …)` with `arg` (plain data) |
| `ui.invoke(name, arg)` | the same, now, from any function; an error if the host has no such action |

## Host calls with results

`ui.call(name, arg)` calls a host action and returns a promise of its result. An async action (`ScriptHost::async_action(name, |arg| async { Ok(value) })`) runs as a future on the window's tasks, woken by what it waits for, so nothing polls while it waits; `Err(message)` rejects the promise with that message. A plain action (`ScriptHost::action`) settles at once with nil. A promise is read three ways:

| | |
| --- | --- |
| `p:after(on_value, on_error)` | calls one of them once it settles (at once if it has), on the script's budget |
| `p.done`, `p.value`, `p.error` | tracked fields: an interface reading them shows loading, the result and errors as they come, with no handler |
| `p:await()` | inside `ui.task(function() ... end)` only: waits for it, returns its value or raises its error (the host's message as it wrote it, so `pcall` catches it) |

```luau
local user = ui.call("load_user", id)            -- the interface follows it
ui.text { function() return if user.done then user.value.name else "Loading" end }

ui.task(function()                               -- steps in order
    local token = ui.call("sign_in"):await()
    local ok, err = pcall(function() return ui.call("sync", token):await() end)
    if not ok then status:set(err) end
end)
```

A rejection that nothing handles (no `on_error`, no read of `error`, no `await`) is reported as a script error with the action's name, so a failure is never silent. A script reloaded since a call hears nothing of it. A call costs about a microsecond on top of the host's work (`ui.call`, settling and an `:after` callback; `tests/calls.rs`), and the same however many are in flight.

### The network

`ui.fetch(url, { method, headers, body })` makes an HTTP request through the host's transport, `ScriptHost::fetcher(|request| async { … })`: the app owns the HTTP stack, redirects, headers and credentials (a token minted per request), and zgui adds no HTTP dependency. The promise settles with `{ status, ok, headers, body, json }`: `ok` for a 2xx status, header names in lower case, `json` the decoded body when the response says it is JSON; a transport error rejects it. A table `body` is sent as JSON with its content type. Without a fetcher, `ui.fetch` is an error.

`ui.image { src = "https://…" }` and `ui.svg { src = "https://…" }` load through the same fetcher: one request per URL however many nodes show it, decoded off the main thread, cached for the host's life; a node shows nothing until its source arrives, and only the nodes showing it redraw.

### The clipboard and files

A desktop window gives scripts its clipboard and file dialogs (`zgui::platform`). `ui.clipboard.read()` returns its text, `ui.clipboard.write(text)` replaces it. `ui.open_files({ title, directory, filters = { { "Images", { "png" } } }, multiple })` settles with the files the user picks (an empty list if they cancel) and `ui.save_file({ name })` with where they choose to save (or nil), each a `{ path, name }`. A script reads and writes only files the user picked in this host's dialogs, as a web page does: `ui.read_file(file)` settles with its text and `ui.write_file(file, text)` writes beside it and renames over it (never half a file), both off the main thread. Headless, without a window, they are errors.

Plugins reach the network, the clipboard and files only if the host grants each: `ScriptHost::grant("plugin.luau", Capability::Fetch)` (or `Clipboard`, `Files`; `Plugin::grants` for the plugins an app sets), and the app's actions and shared state only where it exposes them (see Plugins in `docs/scripting.md`). A plugin that fails to load is left out and its error reported; the app and the other plugins load.

## Computeds, effects, batches

`ui.computed(f)` and `ui.effect(f)` run `f` at once and again whenever a state it read with `get` (or a tracked field: `web.title`, `term.pwd`, a ref) changes; reads with `peek` do not count. `ui.batch(f)` runs `f` and updates what its writes affect once, after it; it returns what `f` returns. Every handler already runs in a batch.

`ui.on_cleanup(f)` runs `f` when what is being built goes away: a component unmounting, an effect running again, the script reloading (stop a subscription, a timer of the host's). `ui.now()` is milliseconds since the epoch, to the microsecond; it is not tracked (`os.time` counts whole seconds). `print` writes a line to the app's stderr.

## Refs

`ui.ref()` makes a ref; give nodes `ref = r` (or `ref = { r1, r2 }`).

| Field or method | |
| --- | --- |
| `r:get(): Element?`, `r.element` | the first mounted element, nil while none is; reading it in an effect or computed follows it, so an effect that needs the element runs again once it mounts |
| `r:peek(): Element?` | the same, without following |
| `r.mounted: boolean`, `r.count: number` | whether any, and how many, nodes with the ref are mounted; tracked |
| `r:elements(): { Element }` | all of them, in document order; tracked |

A ref has no element methods of its own: `r:get():focus()`, not `r:focus()`. [Events](events.md#elements) lists what elements do.

## Components

`ui.component(build)` returns a constructor: `Card { title = "…", child }`. `build(props)` runs as each instance mounts, in a scope of its own: `ui.use_state` is the instance's, the computeds and effects it makes live as long as the instance, and `ui.on_mount(f)` runs `f` once the instance is mounted. Children in the props table's array part also arrive as `props.children`; `id` and `class` in it name the instance for selectors. Annotate `props` with your own type, with `[number]: Child` if it takes children:

```luau
type CardProps = { title: string, [number]: Child, children: { Child }? }
local Card = ui.component(function(props: CardProps) … end)
```

What `each` rows, `presence` content and `list` rows build owns what it makes in the same way, as long as that part is mounted. Made at the top level, computeds and effects live as long as the script.

## Example

```lua
type Task = { title: string, done: boolean }
local tasks = ui.state("tasks", { { title = "Write docs", done = false } } :: { Task })
local draft = ui.state("draft", "")
local remaining = ui.computed(function()
	local n = 0
	for _, task in tasks:get() do
		if not task.done then
			n += 1
		end
	end
	return n
end)

local Row = ui.component(function(props: { index: number, task: Task })
	local hovered = ui.use_state(false)
	return ui.row {
		gap = 8,
		p = 6,
		rounded = 6,
		bg = function()
			return if hovered:get() then "#2f4058" else "#00000000"
		end,
		on_hover = function(inside)
			hovered:set(inside)
		end,
		ui.text { props.task.title, grow = 1, opacity = if props.task.done then 0.5 else 1 },
		ui.button {
			on_click = function()
				tasks:set_key({ props.index, "done" }, not props.task.done)
			end,
			if props.task.done then "Undo" else "Done",
		},
	}
end)

return ui.column {
	p = 16,
	gap = 8,
	ui.text {
		function()
			return `{remaining:get()} left`
		end,
	},
	ui.row {
		gap = 8,
		ui.input { label = "New task", value = draft },
		ui.button {
			on_click = function()
				ui.batch(function()
					tasks:push { title = draft:peek(), done = false }
					draft:set("")
				end)
			end,
			"Add",
		},
	},
	ui.each(tasks, function(task: Task, i: number)
		return Row { index = i, task = task }
	end),
}
```

## Modules and templates

A module's `ui.state` and `ui.persist` keys are its own: `ui.state("open", false)` in `components/card.luau` is the state `components/card.luau::open`, apart from the script's `open`. `ui.shared` is one namespace for everything. To share state between modules, make it in one module and return it ([app structure](app-structure.md)).

A text template's `{name}` (in `ui.text`, a child string, a span) looks for the state with the key `name` where the template is written: the module's own `ui.state` or `ui.persist` of that key first, then the script's, its persisted state, and shared state. It is an error when there is none. Templates find keyed state only: a computed, a `use_state` or a local's name is not a key; show those with a function (`function() return tostring(n:get()) end`). `{{` and `}}` are literal braces; anything else in braces that is not a name is literal text.

## Plugins and slots

A host's plugins (`ScriptHost::plugins(dir)`) are scripts in their own sandboxed VMs. A plugin returns nothing; it adds nodes to the application's slots with `ui.contribute(name, node)`. `ui.slot(name, props)` is a column holding every contribution to `name`, in plugin order; `props` style it. Plugins see `ui.shared` state, never the application's `ui.state`.

```lua
-- file: app.luau
local user = ui.shared("user", "")
return ui.column { p = 16, gap = 8, ui.text { "Hello, {user}" }, ui.slot("sidebar", { gap = 6 }) }
```

```lua
-- file: plugins/stats.luau
local user = ui.shared("user", "")
ui.contribute(
	"sidebar",
	ui.text {
		function()
			return `{#user:get()} letters`
		end,
	}
)
```

## Gotchas

- State values are read-only snapshots: `tasks:get()[1].done = true` fails with "attempt to modify a readonly table" and a hint naming `set_key`, `patch`, `update`, `insert` and `set`.
- Reading state while the script builds the tree (`ui.text { count:get() }`) uses the value once: the text never changes. Pass the state, a template (`"{count}"`) or a function instead.
- `ui.use_state` and `ui.on_mount` outside a component (or a row or presence content) are errors; use `ui.state`, or `on_mount` on a node.
- A plugin's `ui.state` is its own; the application's is not visible to plugins.
- `ui.persist` reads its file only the first time a run asks for the key; a hot reload keeps the live value. A file that is not JSON starts the key from `initial`, with a warning.
- An effect reads with `get` what it should follow and with `peek` what it only uses: one that writes a state it also follows runs again after each write.
