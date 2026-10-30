# Coming from React

zgx scripts describe an interface the way React does: functions return elements, state drives what shows, lists have keys. The difference that matters is that **a script's tree is built once**. Nothing re-renders. A part of the tree that reads state (a template, a function given as a property, a computed) is followed, and only that part runs again when the state changes. Rust re-runs it; the rest of the script never does.

So where React re-runs a component and diffs, zgx updates the one text, colour or row that read what changed. You get that for free as long as reads happen *inside* the things that follow them: a template (`"{count}"`) or a function (`function() return count:get() end`), never a value read once while building (`"Count: " .. count:get()`).

Every example on this page runs, and type checks in strict mode, as part of zgx-script's tests.

## At a glance

| React | zgx Luau |
| --- | --- |
| `function Card(props) { return <div>…</div> }` | `local Card = ui.component(function(props) return ui.column { … } end)` |
| `<Card title="Hi">child</Card>` | `Card { title = "Hi", child }` (children arrive as `props.children`) |
| `useState(0)` in a component | `ui.use_state(0)`: the instance's own |
| a store (Redux, Zustand, module state) | `ui.state("count", 0)` in a module every file `require`s |
| `setCount(c => c + 1)` | `count:add(1)`, `count:set(v)`, `count:update(f)` |
| `items.map(...)`, `[...items, x]`, a nested `{...m, text}` | `items:push(x)`, `items:remove(i)`, `items:set_key({ i, "text" }, t)`, `items:set_key({ { id = id }, "text" }, t)`, `items:append(path, more)` |
| `{count}` in JSX | `ui.text { count }` (a state), `ui.text { "Count: {count}" }` (a template naming a `ui.state`), or `ui.text { function() return … end }` |
| `useMemo(() => f(a, b), [a, b])` | `ui.computed(function() return f(a:get(), b:get()) end)`: dependencies are what it reads |
| `useEffect(() => { … return cleanup }, [deps])` | `ui.effect(function() … ui.on_cleanup(cleanup) end)`: re-runs when what it read changes |
| `useEffect(() => …, [])` | `ui.on_mount(f)` |
| reading state without subscribing (a store's `getState()`, Solid's `untrack`) | `count:peek()` |
| `useRef(null)` on an element | `local el = ui.ref()`, `ref = el`, `el:peek():focus()` |
| `useRef(value)` for a mutable value | a plain Luau local in the component's build |
| `createContext` / `useContext` | `provide = { user = user }` on a node, `ui.use_context("user")` in a component below |
| `{cond && <X/>}`, ternaries | `ui.show(cond, X)`, `ui.show(cond, A, B)`, `ui.switch(value, { a = A, b = B })` |
| mount and unmount animations (Framer's `AnimatePresence`) | `ui.presence(value, build, { enter = …, exit = … })` |
| `list.map(x => <Row key={x.id} … />)` | `ui.each(list, row, { key = function(x) return x.id end })` |
| a virtualized list (react-window) | `ui.list { data = list, key = …, row = … }`: builds the rows in view |
| Suspense, React Query, SWR | `ui.resource(load)`: `data`, `loading`, `error`, `status` as states |
| `fetch`, `await`, `AbortController` | `ui.fetch(url, opts):await()` in a `ui.task`; `task:cancel()` stops the request |
| streaming a response (`ReadableStream`, SSE) | `ui.fetch(url, { on_event = function(e) … end })` |
| `useWebSocket` (react-use-websocket), `reconnecting-websocket` | `ui.socket(url, { json = true, on_message = fn })`: `status` a state, `send` queued until open, reconnect with backoff built in |
| `await new Promise(r => setTimeout(r, ms))` | `ui.sleep(ms):await()` in a `ui.task` |
| controlled inputs | `ui.input { value = state }`, or `value = function` with `on_change` |
| React Hook Form | `ui.form { fields, validate, submit }` with `field = form.fields.name` on a control |
| portals and modals | `ui.modal { open = state, … }`, `ui.popover`, `ui.menu`: panels over everything, still in your tree |
| error boundaries | a script error shows a banner and the rest keeps running; `ui.try(f)` catches one with its message |
| `className`, CSS | properties on the node: `px = 12, rounded = 8, bg = "$surface.raised", hover = { bg = "$hover" }` |
| react-markdown | `ui.markdown_view { text }` |
| React Testing Library | `zgx_script::testing::TestApp`: find, click, type, read texts, headless |

## Components and props

A component is a function of its props, called once as each instance mounts. Its `ui.use_state` belongs to that instance; what it builds lives as long as it does.

```lua
type CounterProps = { label: string, start: number? }
local Counter = ui.component(function(props: CounterProps)
	local count = ui.use_state(props.start or 0)
	return ui.row {
		gap = 8,
		items = "center",
		ui.text { props.label },
		ui.button { on_click = ui.add(count, 1), "+1" },
		-- The state itself: this text, alone, follows the count.
		ui.text { count },
	}
end)

return ui.column {
	gap = 8,
	Counter { label = "Apples" },
	Counter { label = "Pears", start = 3 },
}
```

There is no re-render, so a component's body is not where values change: it describes, once, what follows the state. Reads go in templates, functions and computeds.

## State shared across the app

React apps reach for a store; here a module that holds `ui.state`s is one. Every file that `require`s it gets the same states.

```lua
-- In a real app this is state.luau, required by every component.
local store = {
	todos = ui.state("todos", {} :: { { id: number, title: string, done: boolean } }),
	filter = ui.state("filter", "all"),
}

local shown = ui.computed(function()
	local all = store.todos:get()
	if store.filter:get() == "all" then
		return all
	end
	local out = {}
	for _, t in all do
		if t.done == (store.filter:get() == "done") then
			table.insert(out, t)
		end
	end
	return out
end)

local next_id = 1
local function add(title: string)
	store.todos:push { id = next_id, title = title, done = false }
	next_id += 1
end
add("Write the docs")
add("Ship it")

return ui.column {
	gap = 6,
	ui.segmented { value = store.filter, options = { "all", "open", "done" } },
	ui.each(shown, function(todo)
		-- Built again when its item changes: plain values read once are fine.
		return ui.button {
			justify = "start",
			on_click = function()
				store.todos:set_key({ { id = todo.id }, "done" }, not todo.done)
			end,
			ui.text { if todo.done then `✓ {todo.title}` else `○ {todo.title}`, literal = true },
		}
	end, {
		key = function(todo)
			return todo.id
		end,
	}),
}
```

Write state with the method that says what changed: `push`, `remove`, `insert`, `set_key` for one field, `append` for text growing at a path. They copy only the path they touch, so a list of thousands stays cheap. `update(f)` copies the whole value: keep it for small state.

## Effects, memos and refs

`ui.computed` is `useMemo` whose dependencies are whatever it reads. `ui.effect` is `useEffect` the same way: it runs once, then again when something it read changes, and `ui.on_cleanup` inside it runs before the next run and when its owner goes.

```lua
local query = ui.state("query", "")
local hits = ui.state("hits", 0)
local box = ui.ref()

-- useMemo
local words = ui.computed(function()
	local n = 0
	for _ in query:get():gmatch("%S+") do
		n += 1
	end
	return n
end)

-- useEffect: runs again whenever the query changes.
ui.effect(function()
	local q = query:get()
	local cancel = ui.after(300, function()
		hits:set(#q)
	end)
	-- Before the next run, and when its owner goes: a debounce.
	ui.on_cleanup(cancel)
end)

return ui.column {
	gap = 6,
	ui.input { ref = box, value = query, label = "Search", placeholder = "Search" },
	ui.text {
		function()
			return `{words:get()} words, {hits:get()} letters searched`
		end,
	},
	ui.button {
		on_click = function()
			local el = box:peek()
			if el then
				el:focus()
			end
		end,
		"Focus the search",
	},
}
```

## Context

`provide` on a node is a context provider for its subtree; `ui.use_context` reads it in a component or row built below. Give it a state, and what reads it follows it.

```lua
local density = ui.state("density", "comfortable")

local Row = ui.component(function(props: { title: string })
	local density = ui.use_context("density")
	return ui.text {
		props.title,
		py = function()
			return if density:get() == "compact" then 2 else 8
		end,
	}
end)

return ui.column {
	provide = { density = density },
	ui.segmented { value = density, options = { "comfortable", "compact" } },
	Row { title = "Inbox" },
	Row { title = "Archive" },
}
```

## Conditional rendering

`ui.show` mounts its node while the condition holds (and unmounts it after, its state gone, as React does); `ui.switch` picks a case; `ui.presence` plays `exit` before removing what leaves.

```lua
local signed_in = ui.state("signed_in", false)
local tab = ui.state("tab", "home")

return ui.column {
	gap = 8,
	ui.show(signed_in, ui.text { "Welcome back" }, ui.button { on_click = ui.set(signed_in, true), "Sign in" }),
	ui.segmented { value = tab, options = { "home", "settings" } },
	ui.switch(tab, {
		home = ui.text { "Home" },
		settings = ui.text { "Settings" },
	}),
	ui.presence(function()
		return if signed_in:get() then "badge" else nil
	end, function()
		return ui.text { "●", color = "$accent" }
	end, { enter = { opacity = { 0, 1 }, duration = 200 } }),
}
```

## Lists, keys and virtualization

`ui.each` builds every item; give it a `key` and rows follow their items as the list reorders, keeping their state. A two-parameter row is built again when its item changes. Taking a third parameter, it gets its item as a state and updates in place: a streaming message grows without rebuilding its row.

For long lists, `ui.list` takes the same `data`, `key` and `row` and builds only what is in view, so it holds a million rows as cheaply as twenty. `measured = true` lets rows take their content's height, and `follow = true` keeps a chat at its end.

```lua
type Message = { id: number, text: string }
local messages = ui.state("messages", {} :: { Message })
for i = 1, 2000 do
	messages:push { id = i, text = `Message {i}` }
end

return ui.list {
	data = messages,
	key = function(m: Message)
		return m.id
	end,
	row = function(m: Message, _index: State<number>, row: State<Message>)
		return ui.text {
			function()
				return row:get().text
			end,
			px = 12,
			py = 6,
			wrap = true,
		}
	end,
	measured = true,
	follow = true,
	height = 32,
	w_full = true,
	h = 400,
}
```

## Data loading

`ui.resource` is Suspense and React Query in one: `load` runs as a task (it may await), and its `data`, `loading`, `error` and `status` are states an interface reads. It loads again when a state it read before its first await changes, and an earlier answer arriving late is dropped.

```lua
local id = ui.state("id", 1)
local user = ui.resource(function()
	local which = id:get()
	-- A host call (or ui.fetch(...):await()): the task waits here.
	ui.call("save", which):await()
	return { name = `User {which}` }
end, { initial = { name = "" } })

return ui.column {
	gap = 6,
	ui.switch(user.status, {
		loading = ui.text { "Loading…" },
		failed = ui.text {
			function()
				return `Couldn't load: {user.error:get()}`
			end,
			color = "$danger",
		},
		ready = ui.text {
			function()
				return user.data:get().name
			end,
		},
	}),
	ui.button { on_click = ui.add(id, 1), "Next user" },
	ui.button { on_click = user.reload, "Reload" },
}
```

Requests go through the host's transport (`ScriptHost::fetcher`), as a browser's go through its network stack. `ui.fetch(url, { on_chunk, on_event })` hears a response as it arrives (a model's reply, server-sent events), and cancelling the task that awaits it stops the request, as `AbortController` does.

A WebSocket is `ui.socket(url, opts)`, through the host's `ScriptHost::socket_connector` (zgx-run brings one): what `useWebSocket` gives, with reconnection as `reconnecting-websocket`'s. `status` is a state for a banner, a message sent before the connection opens waits for it, a dropped connection opens again after a growing wait (`attempt`, `retry_in`), and a URL function reading a state (a room) opens it again when that changes. It closes with the component that made it, as an effect's cleanup would close it.

## Forms

A control bound to a state is a controlled input both ways: typing writes the state, and writing the state (clearing a draft on send) shows in it. For validation and submitting, `ui.form` holds the fields as React Hook Form does.

```lua
local form = ui.form {
	fields = { email = "" },
	validate = {
		email = function(email: string)
			return if email:find("@") then nil else "That is not an email address"
		end,
	},
	submit = function(values)
		return ui.call("save", values):await()
	end,
}

return ui.column {
	gap = 8,
	ui.input { id = "email", label = "Email", field = form.fields.email },
	ui.presence(form.fields.email.error, function(problem: string)
		return ui.text { problem, color = "$danger" }
	end),
	ui.button {
		variant = "primary",
		disabled = form.pending,
		on_click = form.submit,
		function()
			return if form.pending:get() then "Saving…" else "Save"
		end,
	},
}
```

## Modals and portals

A panel opens over everything (no portal to set up) while staying in your tree: its states, refs and handlers work as anywhere else. It is bound to a boolean state, which it sets back to `false` when it closes.

```lua
local open = ui.state("open", false)

return ui.column {
	ui.button { on_click = ui.set(open, true), "Settings" },
	ui.modal {
		open = open,
		label = "Settings",
		gap = 12,
		ui.text { "Settings", bold = true },
		ui.button { variant = "primary", on_click = ui.set(open, false), "Done" },
	},
}
```

## Errors

A script error never takes the app down: a banner shows the file, line and message, and the rest keeps running (an error boundary around everything, built in). To handle one yourself, `ui.try(f, ...)` is `pcall` with its error as text:

```lua
local status = ui.state("status", "")
ui.task(function()
	local _, err = ui.try(function()
		return ui.call("save"):await()
	end)
	status:set(if err then `Couldn't save: {err}` else "Saved")
end)
return ui.text { "{status}" }
```

## Performance: what not to do

The interface is built once, and Rust re-runs only what reads state that changed. That makes most React performance work (memo, useCallback, keys to dodge re-renders) unnecessary, but it moves the traps. `zgx lint` flags these, and a debug build lints on every reload.

| Do | Not |
| --- | --- |
| `ui.text { "Count: {count}" }` or `function() return n:get() end` | `ui.text { "Count: " .. count:get() }`: read once, never updates |
| `ui.use_state(false)` inside a component or row | `ui.state("open", false)` there: one state shared by every instance |
| `items:push(v)`, `:set_key({ 3, "title" }, v)`, `:append(path, text)`, `:patch` | `items:update(function(l) table.insert(l, v) end)`: copies the whole list |
| `ui.list { data = items, … }` for thousands of rows | `ui.each` over thousands: builds every row |
| a three-parameter row (`row(item, index, row)`) for items that change often | a row reading the whole list by index in every function |
| `ui.tween(state)`, `ui.spring(state)`, `anime.createAnimatable` | `anime.animate` fired again on every data change |
| percentages, `grow`, insets: layout without the script | `on_resize` writing sizes into state for layout |
| `ui.every(1000, …)` at the rate a value changes | `ui.every(1, …)`: more calls than frames |
| keys of the item alone: `key = function(r) return r.id end` | a `key` reading other state: it goes stale |
| `ui.markdown_view { text }` for markdown that changes | drawing `ui.markdown`'s blocks again on every change |

`-- zgx-lint: allow(rule)` accepts a deliberate exception.
