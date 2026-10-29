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
ui.sort<T>(list: { T }, by: string | (item: T) -> any, descending: boolean?) -> { T }
ui.number(n: number, opts: NumberOptions?) -> string
ui.json.encode(value: any, opts: { pretty: boolean? }?) -> string
ui.json.decode(text: string) -> any
ui.ref() -> Ref
ui.component<P>(build: (props: P) -> Node) -> (props: P) -> Node
ui.on_mount(f: () -> ...unknown) -> ()
ui.slot(name: string, props: Props?) -> Node
ui.contribute(name: string, node: Node) -> ()
ui.contribute({ target: string, before: Node?, after: Node?, replace: Node? }) -> ()
ui.set<T>(state: State<T>, value: T) -> Action
ui.toggle(state: State<boolean>) -> Action
ui.add(state: State<number>, by: number?) -> Action
ui.action(name: string, arg: unknown?) -> Action
ui.invoke(name: string, arg: unknown?) -> ()
ui.call(name: string, arg: unknown?) -> Promise
ui.task<A...>(f: (A...) -> (), A...) -> Task
ui.async<A...>(f: (A...) -> ()) -> (A...) -> ()
ui.optimistic<T>(state: State<T>, next: T, save: () -> ...any) -> UiError?
ui.try<T, A...>(f: (A...) -> ...T, A...) -> (T?, UiError?)
ui.resource<T>(load: () -> T, opts: { initial: T }?) -> Resource<T>
ui.fetch(url: string, opts: FetchOptions?) -> Promise<Response>
ui.clipboard.read() -> string?
ui.clipboard.write(text: string) -> boolean
ui.clipboard.read_image() -> Promise<Bitmap?>
ui.clipboard.write_image(image: Bitmap) -> Promise<boolean>
ui.open_files(opts: FilePickerOptions?) -> Promise<{ File }>
ui.save_file(opts: FilePickerOptions?) -> Promise<File?>
ui.read_file(file: File | string) -> Promise<string>
ui.write_file(file: File | string, text: string) -> Promise<nil>
ui.args: { string }
ui.on_open(handler: (e: { files: { File }, urls: { string } }) -> ())
```

## Making state

| Function | Kept by | |
| --- | --- | --- |
| `ui.state(key, initial)` | the script (or module) and `key` | private to the script; a reload gets the same state back, `initial` only counts the first time |
| `ui.shared(key, initial)` | `key`, for the whole host | shared with Rust (`ScriptHost::shared`, see below) and every script and plugin |
| `ui.persist(key, initial)` | the script (or module) and `key`, on disk | as `ui.state`, and saved for the next run: each key a JSON file in the host's storage (`ScriptHost::app_id` or `storage`), written within 250 ms of a change, off the main thread. Without storage it lasts until the app quits, with a warning |
| `ui.use_state(initial)` | the component instance | a new state per instance; only inside a component's function (or an `each` row, `presence` content, a `list` row) |
| `ui.computed(f)` | what built it | derived: `f` runs now and again when what it reads changes; its value is the state's |
| `ui.sorted(list, spec)` | what built it | a list state's items in order (and filtered), kept in order as the list is written: see [Large data](#large-data) |

State holds `nil`, booleans, numbers, strings, lists, string-keyed tables and elements (`state:set(el)`; Rust sees `Value::Element`, `null` as JSON). Not functions, not refs (keep the ref itself, or store `ref:get()`), not a table that is both a list and has string keys, and nothing nesting more than 128 tables deep (or a table holding itself), which is an error.

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
| `set_key(path, value)` | one key (`"title"`), a list index from 1 (`3`), or a path (`{ "tabs", 3, "title" }`), where a table step picks a list's first item with those fields (`{ "c1", { id = "m5" }, "text" }`); `nil` removes a key | makes missing tables along the path; an index may be one past the end (appends); a list cannot get a hole (`nil` removes only its last item); the value is not type checked |
| `append(path, text)` | adds `text` to the string at `path` (or puts it there) | concatenated in Rust: a token streaming into a reply, without the text so far copied into the script and back |
| `patch(fields: { ... })` | a shallow merge of string keys | each field as `set_key(name, value)` |
| `toggle()` | `not value` | |
| `add(by: number?)` | adds `by` (1) to a number; anything else counts as 0 (a boolean as 0 or 1) | |
| `push(item)` | appends to a list (`nil` is an empty list) | |
| `insert(index: number, item)` | inserts at 1 to `len + 1` | |
| `remove(index: number): item?` | removes and returns the item; nil, and nothing changes, outside the list | |
| `len(): number` | the list's length (0 for anything else); tracked as `get` is | |
| `index_of(field: string, value): number?` | where the first item whose `field` is `value` is, from 1, or nil (`messages:index_of("id", 42)`); tracked as `get` is | found in Rust, without making the list a table |

`set_key`, `patch`, `insert`, `push` and `remove` copy only the path they change, sharing the rest with the old value; prefer them to `update` for large state. `set_key` writes a list in place while nothing else holds it (no script has read it since the last write): a batch writing 300 rows of a 100,000-row list copies the list once, not 300 times. Scripts reading a long list after `set_key` on one row (or a `push`) get its table back with only that row converted, and a computed that returns rows of a state (a filter) hands its readers the table it built: a write to one message of thousands costs the scripts reading them only their own loops. A write of an equal value notifies nobody.

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
| `p:await()` | inside `ui.task(function() ... end)` only: waits for it, returns its value or raises its error (so `pcall` catches it) |

What a promise rejects with (its `error`, `on_error`'s argument, what `:await()` raises) is a `UiError`, the same as `ui.try` gives (below): its `message` is the host's as it wrote it, `kind` is `"action"`, `"network"`, `"cancelled"`, `"http"` or `"io"`.

```luau
local user = ui.call("load_user", id) -- the interface follows it
ui.text {
	function()
		return if user.done then user.value.name else "Loading"
	end,
}

ui.task(function() -- steps in order
	local token = ui.call("sign_in"):await()
	local ok, err = pcall(function()
		return ui.call("sync", token):await()
	end)
	if not ok then
		status:set(err)
	end
end)
```

`ui.task` returns its handle: `task:cancel()` stops it at the await it is at, never to go on (`task.cancelled` says so). A fetch it awaited (and nothing else hears) stops: the host's transport sees its `Body` close and its future is dropped. An action it awaited goes on, its answer unheard. Cancel what a closed view started, as React's effects abort a fetch in their cleanup: `ui.on_cleanup(function() task:cancel() end)`. A resource cancels a load it begins again, and its owner going cancels its load; a form cancels its submit with `form.cancel()` and as it goes.

`ui.async(f)` is a handler that awaits, as React's async handlers do: each call runs `f` as a task with the call's arguments (`on_click = ui.async(save)`, a toast's `action.run`). Its tasks are cancelled as what made the handler goes (a component unmounting, a reload), and what one raises and doesn't catch is reported once as the script's error.

`ui.optimistic(state, next, save)`, inside a task (a `ui.async` handler), writes `next` to `state` now and runs `save`, which may await. If `save` succeeds the write stays and it returns nil; if it raises, the state goes back to what this write replaced and the error comes back (a `UiError`, as `ui.try` gives), as React's `useOptimistic` does. Calls on one state may overlap: the newest write shows, a failure puts back only what its own write replaced (never over a newer write still out or kept), and a write kept settles the older ones still out.

```lua
local liked = ui.state("liked", false)
local like = ui.async(function(on: boolean)
	local err = ui.optimistic(liked, on, function()
		return ui.call("like", on):await()
	end)
	if err then
		ui.toast { title = "Couldn't save that", body = err, kind = "error" }
	end
end)
return ui.checkbox {
	label = "Like",
	value = function()
		return liked:get()
	end,
	on_change = like,
}
```

`ui.try(f, ...)` is `pcall` with its error typed: `f(...)`'s value and nil, or nil and a `UiError` for what it raised, where Luau types `pcall`'s error as `f`'s result. Awaits inside `f` work in a task as through `pcall`. A `UiError` is one shape from every source, to branch on:

| field | |
| --- | --- |
| `message` | what a person reads: the text raised (an error object's, without its traceback; a raised table's `message` field), or the host's |
| `kind` | `"raised"` (`error(...)` in the script), `"action"` (a host action rejected), `"network"` (no answer came), `"cancelled"`, `"http"` (a fetch with `raise = true` answered outside 2xx), `"io"` (a file, the clipboard) |
| `status`, `body` | `"http"`: the response's status, and its body (the JSON decoded, else the text) |
| `value` | `"raised"`: what was raised, as it was (`error({ code = 409 })` gives that table back) |

It is still text where text was expected: `tostring(err)`, `"Couldn't load: " .. err`, `` `{err}` `` and string methods (`err:find("404")`) read its message, a state set to it holds its message, and a toast's `title` or `body`, a prop or a text given it show it. Comparing it with `==` to a string, or `type(err) == "string"`, no longer holds: compare `err.message` or `err.kind`. Making one costs nothing until something fails.

```lua
local status = ui.state("status", "")
ui.task(function()
	local _, err = ui.try(function()
		return ui.call("load_user"):await()
	end)
	status:set(if err then `Couldn't load: {err}` else "Loaded")
end)
return ui.text { "{status}" }
```

A rejection that nothing handles (no `on_error`, no read of `error`, no `await`) is reported as a script error with the action's name, so a failure is never silent. A script reloaded since a call hears nothing of it. A call costs about a microsecond on top of the host's work (`ui.call`, settling and an `:after` callback; `tests/calls.rs`), and the same however many are in flight.

### Loading data

`ui.resource(load, { initial })` loads what an interface shows (a list from the server, the open record) as states, so the interface follows how the load goes with no handler:

| Field | |
| --- | --- |
| `data` | what the last load returned, `initial` (or nil) until one has; kept while it loads again. A plain state: write it (`push`, `set_key`) as a save or an optimistic update changes it, and the next load replaces it |
| `loading` | a load is under way |
| `error` | what the last load raised (the message, as `error(message, 0)` gives it), or nil |
| `status` | `"loading"` until a load has answered (and again while a failed one is tried again), then `"ready"` or `"failed"`: `ui.switch(r.status, { loading = …, failed = …, ready = … })` |
| `reload` | loads again: a handler (`on_click = r.reload`) or a function to call |

`load` runs as a task, so it awaits (`ui.fetch(url):await()`, `ui.call(name):await()`) and raises what went wrong. The states it reads with `get` before its first await are its key: when one changes it loads again (the person a page shows, a search), and only the last load's answer is kept: an earlier one arriving after it is dropped, as is any answer once what made the resource has gone (a component unmounted). After its first await it reads untracked.

```lua
type Person = { name: string }
local id = ui.state("id", 1)
local person = ui.resource(function(): Person?
	local response = ui.fetch(`https://api.example/people/{id:get()}`):await()
	if not response.ok then
		error(`The server answered {response.status}`, 0)
	end
	return response.json :: Person
end)
return ui.switch(person.status, {
	loading = ui.text("Loading…"),
	failed = ui.column {
		ui.text {
			function()
				return person.error:get()
			end,
		},
		ui.button { "Try again", on_click = person.reload },
	},
	ready = ui.text {
		function()
			local found = person.data:get()
			return if found then found.name else ""
		end,
	},
})
```

[The team example](../../crates/zgx-script/examples/team) loads a list this way, and saves and removes people with `ui.task`s that write its `data`.

### The network

`ui.fetch(url, { method, headers, body })` makes an HTTP request through the host's transport, `ScriptHost::fetcher(|request| async { … })`: the app owns the HTTP stack, redirects, headers and credentials (a token minted per request), and zgx and zgx-script add no HTTP dependency. An app `zgx-run` runs has its fetcher, over HTTP and HTTPS ([Running a script](../scripting.md)). The promise settles with `{ status, ok, headers, body, json }`: `ok` for a 2xx status, header names in lower case, `json` the decoded body when the response says it is JSON (typed `any`, as JSON is to JavaScript: cast it to the shape the server sends); a transport error rejects it (a `"network"` error). With `raise = true`, a status outside 2xx rejects too, as an `"http"` error carrying the `status` and `body` (`if err.status == 404 then …`), for a load that should fail on one. A table `body` is sent as JSON with its content type. Without a fetcher, `ui.fetch` is an error.

A response can be read as it arrives (a model's reply, a log): `on_chunk = function(text)` gets the body's text piece by piece, in whole characters, and `on_event = function(e)` each server-sent event (`text/event-stream`) as it ends, `e.event` (`"message"` unless named), `e.data` (its `data:` lines joined by line breaks) and `e.id`. The promise settles once the body has ended, with the status and headers, and with an empty `body` unless the request failed (then the body's start, an error's JSON). `p:cancel()` stops a fetch at once (the promise rejects with a `"cancelled"` error, reported nowhere), as does cancelling the task that awaits it; a reload stops the rest of a stream. The host streams with `ScriptHost::streaming_fetcher(|request, body| async { … })`, writing each piece to `body` as it comes and answering with the status once it ends; a plain `fetcher`'s body arrives as one piece.

```lua
local reply = ui.state("reply", "")
local running = nil
local function ask(question: string)
	reply:set("")
	running = ui.task(function()
		local response = ui.fetch("https://api.example/chat", {
			method = "POST",
			body = { question = question, stream = true },
			on_event = function(e: ServerEvent)
				if e.data ~= "[DONE]" then
					reply:set(reply:peek() .. e.data)
				end
			end,
		}):await()
		if not response.ok then
			reply:set(`Failed: {response.status}`)
		end
	end)
end
return ui.column {
	ui.button {
		"Ask",
		on_click = function()
			ask("Why is the sky blue?")
		end,
	},
	ui.button {
		"Stop",
		on_click = function()
			if running then
				running:cancel()
			end
		end,
	},
	ui.text { "{reply}", wrap = true },
}
```

`ui.image { src = "https://…" }` and `ui.svg { src = "https://…" }` load through the same fetcher: one request per URL however many nodes show it, decoded off the main thread, cached for the host's life; a node shows nothing until its source arrives, and only the nodes showing it redraw.

### The clipboard and files

A desktop window gives scripts its clipboard and file dialogs (`zgx::platform`). `ui.clipboard.read()` returns its text, `ui.clipboard.write(text)` replaces it. `ui.clipboard.read_image()` settles with its image as a full-colour `Bitmap` (`w`, `h`; a screenshot, an image editor's copy, a browser's Copy Image), or nil, and `ui.clipboard.write_image(bitmap)` puts one there (a `ui.bitmap`, or one read before) for other apps to paste, settling with whether it took it: promises, as a large image is decoded and encoded off the main thread (a 4K one took 15 to 70 ms to read and 130 ms to write). State can keep a `Bitmap`, its pixels counted against a plugin's memory limit, and `ui.image { src = state }` shows the one it holds (a pasted picture: `ui.clipboard.read_image():after(function(b) if b then pasted:set(b) end end)`). `ui.open_files({ title, directory, filters = { { "Images", { "png" } } }, multiple })` settles with the files the user picks (an empty list if they cancel) and `ui.save_file({ name })` with where they choose to save (or nil), each a `{ path, name }`. A script reads and writes only files the user picked in this host's dialogs, as a web page does: `ui.read_file(file)` settles with its text and `ui.write_file(file, text)` writes beside it and renames over it (never half a file), both off the main thread. Headless, without a window, they are errors.

`ui.args` is what the app was launched with after its script (`zgx-run app/ notes.txt` gives `{ "notes.txt" }`), and `ui.on_open(handler)` is handed the files and URLs the system asks it to open: those among its launch's arguments, and with an `application_id` (reverse-DNS, in `zgx.json` or `Application::application_id`) a file opened with the app, a link to it, or a second launch's on Windows. Its files are the user's choice, as a dialog's are, so `ui.read_file` takes them; what arrives before the script registers its handler waits for it. It is the app's: a plugin's `ui.on_open` is an error.

Plugins reach the network, the clipboard and files only if the host grants each: `ScriptHost::grant("plugin.luau", Capability::Fetch)` (or `Clipboard`, `Files`; `Plugin::grants` for the plugins an app sets), and the app's actions and shared state only where it exposes them (see Plugins in `docs/scripting.md`). A plugin that fails to load is left out and its error reported; the app and the other plugins load.

## Computeds, effects, batches

`ui.computed(f)` and `ui.effect(f)` run `f` at once and again whenever a state it read with `get` (or a tracked field: `web.title`, `term.pwd`, a ref) changes; reads with `peek` do not count. `ui.batch(f)` runs `f` and updates what its writes affect once, after it; it returns what `f` returns. Every handler already runs in a batch.

`ui.on_cleanup(f)` runs `f` when what is being built goes away: a component unmounting, an effect running again, the script reloading (stop a subscription, a timer of the host's). `ui.now()` is milliseconds since the epoch, to the microsecond; it is not tracked (`os.time` counts whole seconds). `print` writes a line to the app's stderr.

## Large data

State converts what it holds between Luau and Rust, through Luau's C API: 100,000 plain rows go into `ui.state` in some 60 ms and come out the first time in about 110 ms, a list of 100,000 numbers in about a millisecond either way. For rows the app changes a few at a time (a market feed's), keep them in a list state, write the ones that change with `set_key` in a batch (a long list is written in place, not copied a write), and show them through `ui.sorted`, which keeps their order as they are written:

```lua
local initial = {}
for i = 1, 1000 do
	initial[i] = { id = i, price = i }
end
local rows = ui.state("rows", initial)
ui.every(250, function()
	ui.batch(function()
		for _ = 1, 10 do
			local i = math.random(rows:len())
			rows:set_key({ i, "price" }, (rows:at(i) :: any).price + 1)
		end
	end)
end)
-- By price, highest first: a write re-places the rows it wrote.
local order = ui.sorted(rows, { by = "price", descending = true })
return ui.list {
	h = 300,
	count = function()
		return order:len()
	end,
	height = 24,
	row = function(i)
		return ui.text {
			function()
				local row = order:at(i)
				return if row then ui.number(row.price, { decimals = 2 }) else ""
			end,
		}
	end,
}
```

- `ui.sorted(list, spec)` is a list state's items in order, kept in order as the list changes: an item a write leaves alone is the same value, so a write re-places the items it wrote (`by` and `filter` called for those alone), in one pass over the order. `spec` is `{ by, descending, filter }`: `by` a field's name (read in Rust, no script call per item) or a function of an item, `filter` a function keeping the items it is true for, or a table of fields' tests, read in Rust as `by` is (no script call an item): `{ sector = "Tech" }` keeps the items whose `sector` is `"Tech"`, `{ symbol = { contains = "AB" } }` those whose `symbol` holds `"AB"` (`case = false` folds ASCII letters' case, as `state:find`'s), every test passing. A spec giving the same tests again keeps the order it has; a new function always sorts again. Give a function returning the spec when the order follows state (a sort header, a search): it runs when what it reads changes, and a new spec sorts the whole list again. Ties keep the list's order and nil keys go last, as `ui.sort`'s. `state:at(i)` reads one item without making the list a table, `state:len()` its length and `state:index_of(field, value)` where one is: a list shows its rows in view that way, and an action finds a row's place (a mailbox's next message, the one after the archived). A filter that is equality alone (`{ mailbox = "Inbox" }`) is all Rust, and a write judges the item it wrote: archiving a message in a view of 4000 re-places one. The lint (`computed-sort`) points at a `ui.computed` that re-sorts a whole list.
- `ui.sort(list, by, descending?)` sorts `list` in place, stably, by a field of each item (`"price"`) or a function of it, called once an item; it returns `list`, and puts nil keys last. `table.sort` with a comparator calls Luau some 1.7 million times for 100,000 rows (about 95 ms, over a call's 50 ms budget); `ui.sort` reads each key once and sorts in Rust (4 to 8 ms). A state's value is read-only: sort `table.clone(value)`.
- A computed returning row tables it builds converts every row each time it runs; `ui.sorted` hands on the rows the list holds.
- Large payloads that don't drive updates don't belong in state: a row's chart history, a file's bytes, a decoded image. Everything in state is converted between Luau and Rust and kept on both sides, so 100,000 rows with 16 points of history each cost several times what their prices do. Keep such data in a plain table keyed by the row's id, and let state hold what changes and what reading it should follow: the row a feed writes is what redraws its sparkline, which then reads the history from the table.
- Totals over every row (a sum, a count by group) cost a loop over all of them each time they run: keep them as the rows change (the feed adds what it changed) rather than summing in a chart's `draw` each tick.
- `ui.number(n, opts)` formats numbers as `Intl.NumberFormat` does: `ui.number(1234.5)` is "1,234.50"; `decimals`, `style = "percent"` (`n` a fraction: 0.05 is "5%"), `notation = "compact"` ("1.2K", "3.4M", "5.6B", "7.8T"), `sign` (`"auto"`, `"always"`, `"except_zero"`, `"never"`), `grouping` (thousands separators, on) and `currency` (`"$"`, before the number).

- `ui.json.encode(value, { pretty })` writes what state could hold as JSON (lists as arrays, tables as objects, `ui.null` as null), and `ui.json.decode(text)` gives back tables the script may change, failing with where the JSON is wrong: a document saved and opened with `ui.write_file` and `ui.read_file`.

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

`provide = { name = value }` on any node gives its subtree values that the components and rows built in it read with `ui.use_context(name, default)`, as React's context does: the signed-in user, a list's selection, a density, without passing them down every level. A state provided is the state, so what reads it follows it; a nearer `provide` of the same name wins, and the rest of an outer one is kept. `ui.use_context` belongs in a component's, row's or presence's build (where it is mounted); elsewhere it is an error.

```lua
local user = ui.state("user", "Ada")
local Greeting = ui.component(function()
	local who = ui.use_context("user")
	return ui.text {
		function()
			return `Hello, {who:get()}`
		end,
	}
end)
return ui.column { provide = { user = user }, Greeting {} }
```

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

A host's plugins (`ScriptHost::plugins(dir)`) are scripts in their own sandboxed VMs. A plugin returns nothing; it adds nodes to the application's slots with `ui.contribute(name, node)`. `ui.slot(name, props)` is a column holding every contribution to `name`, in plugin order; `props` style it. Plugins see the `ui.shared` state the host exposes (`expose_shared`, or `expose_shared_read` to read and follow only: a plugin's write is an error), never the application's `ui.state`.

```lua
-- file: app.luau
local user = ui.shared("user", "")
return ui.column {
	p = 16,
	gap = 8,
	ui.text { "Hello, {user}" },
	ui.slot("sidebar", { gap = 6 }),
	ui.button { id = "save", label = "Save" },
	ui.input { private = true, value = ui.state("password", "") },
}
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

### Hooks

`ui.contribute { target, before | after | replace = node }` adds at an element of the app's without a slot. `target` is one part of the inspector's syntax: `#id`, `hook:name` (an element declaring `hook = "name"`), `role:button "Save"` or a name, matched against what the app's source writes (its `role` or its kind's, its `label` or literal text). `>>`, `@n`, `row` and `[n]` are refused: they are known only once the interface shows.

- `before` and `after` add beside the element, in plugin order, wherever it is built; the host can make a target ask a capability with `expose_hook(target, Some(capability))`.
- `replace` takes the element's place (an element in a container's children: not the app's root, nor a split's panes) and needs `Capability::Override(target)` or `Capability::Override("*")`; the last plugin's wins.
- `private = true` (or `hook = false`) on an element puts it and everything in it out of reach.
- What a plugin adds has the accessible description "From the plugin `<name>`".

With the app above, a plugin adds a button before its Save; nothing reaches the password field:

```lua
-- file: plugins/preview.luau
ui.contribute { target = "#save", before = ui.button { label = "Preview" } }
```

`ui.filter { target, text | label | hidden | disabled | tooltip | items | order = fn }` passes an element's prop through a function of the plugin's, which takes the value as it is and returns it changed. It runs in the plugin's script, again whenever what it reads changes (the app's state the value follows, the plugin's own), never per frame. It needs `Capability::Override`, as replacing does.

- `text` is the element's text, or a button's or item's (its first child's); `label` its accessible name.
- `hidden` and `disabled` take and return a boolean.
- `tooltip` is a written tooltip's text, passed through as the element is built.
- `items` is what a `ui.each` or a `ui.list` with `data` shows: a list in, a list out.
- `order` is the element's children's order: it gets their ids (a position from 1 for one without) and returns those it puts first; the rest follow as they were. A changed order rebuilds the element's children; an unchanged one keeps them.
- Nothing else is filtered: its handlers, styles and state stay the app's. A filter that fails is reported as the plugin's error, and the value shows as it was.

```luau
-- file: plugins/shout.luau (granted Capability::Override("#title"))
ui.filter { target = "#title", text = function(text) return string.upper(text) end }
ui.filter {
	target = "#tasks",
	items = function(items)
		local open = {}
		for _, task in items do
			if not task.done then table.insert(open, task) end
		end
		return open
	end,
}
```

A plugin targeting an `id` that is not a declared `hook` reaches the app's internals: the app's next version may move or drop it ([design](../design/plugin-hooks.md)).


### Module overrides

`ui.override { module = "@app/<path>", with = "<plugin path>" }`, from a plugin, replaces one of the app's modules: wherever the app requires `@app/<path>`, it gets the plugin's module instead.

- `module` is one of the app's `@app/…` modules (`@app` is the app's directory; a relative require is never replaced). `with` is one of the plugin's own files, a directory as its `init.luau`.
- It needs `Capability::Override(module)` or `Capability::Override("*")`, **a trust grant**: the replacement runs in the app's VM as the app's own code, with all its access. Of several plugins, the last one's wins.
- The replacement may `require("@app-original/<path>")` for the module it replaces (to wrap it); nothing else may. Its own relative requires reach its plugin's directory.
- A plugin requires none of the app's modules; a `.luaurc` defining an `app` or `app-original` alias is an error.

```lua
-- file: plugins/badges/init.luau (granted Capability::Override("@app/components/card"))
ui.override { module = "@app/components/card", with = "./card" }

-- file: plugins/badges/card.luau
local card = require("@app-original/components/card")
return function(props)
    return ui.row { card(props), ui.text { "New" } }
end
```
## Gotchas

- State values are read-only snapshots: `tasks:get()[1].done = true` fails with "attempt to modify a readonly table" and a hint naming `set_key`, `patch`, `update`, `insert` and `set`.
- Reading state while the script builds the tree (`ui.text { count:get() }`) uses the value once: the text never changes. Pass the state, a template (`"{count}"`) or a function instead.
- `ui.use_state` outside a component (or a row or presence content) is an error; use `ui.state`. `ui.on_mount` outside one runs once the app's interface mounts (and again after a reload).
- An error raised with `error("message")` carries where it was raised (`"api.luau:12: message"`); `error("message", 0)` raises the message alone, as a person should read it. Luau types what `pcall` returns on failure as the function's result: show it with `tostring(err)`.
- A plugin's `ui.state` is its own; the application's is not visible to plugins.
- `ui.persist` reads its file only the first time a run asks for the key; a hot reload keeps the live value. A file that is not JSON starts the key from `initial`, with a warning.
- An effect reads with `get` what it should follow and with `peek` what it only uses: one that writes a state it also follows runs again after each write.
