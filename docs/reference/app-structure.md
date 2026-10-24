# App structure

A small app is one file. A large one is a handful of modules, each about one thing, sharing the app's state through one module that holds it. This page gives a layout that scales, how modules share state, how to type component props, and how hot reload treats modules. The terminal example (`crates/zgx-desktop/examples/terminal`) is built this way.

## Layout

```
my-app/
  .luaurc          strict mode and require aliases
  app.luau         the entry: requires the parts, starts things, returns the root node
  state.luau       the app's shared state, typed
  theme.luau       colors, sizes, the font
  actions.luau     what the app does to its state (buttons, menus and keys call these)
  components/      the views, one module per piece of interface
  lib/             plain logic with no `ui`: parsing, geometry, naming
```

`app.luau` stays short: it requires the modules, runs what has to happen once per load in the order it has to happen (restoring saved data, declaring commands), and returns the root. Views ask `actions` to change state rather than changing it inline, so the same change behind a button, a menu command and a key binding is one function.

```json file=.luaurc
{
	"languageMode": "strict",
	"aliases": {
		"components": "./components",
		"lib": "./lib"
	}
}
```

zgx resolves `require` as Luau does, from this `.luaurc` (beside the script) and the requiring file: `require("./state")` and `require("../theme")` relative to the requiring file, `require("@components/card")` through an alias, `require("@self/x")` from the requiring file's own directory, a directory as its `init.luau`. luau-lsp reads the same file, so editors and `zgx check` follow the same requires and check every module strictly. Modules stay inside the script's directory, run once per load, and return what they return.

## Sharing state

| Where it lives | Use it for |
| --- | --- |
| a state module returning States | the app's state: what several modules read and change |
| `ui.use_state` in a component | one instance's own state: hover, an edit in progress |
| `ui.state` in a module | that module's own state |
| `ui.persist` | state the next launch starts from (in the state module, usually) |
| `ui.shared` | state Rust and plugins see: what the host publishes, what it observes |

A module's `ui.state` and `ui.persist` keys are its own: `ui.state("open", false)` in `components/sidebar.luau` is the state `components/sidebar.luau::open`, and another module's `ui.state("open")` is another state. So state shared between modules is made once, in one module, and every module that needs it requires that module and gets the same States. Text templates follow the same keys: `"{count}"` in a module shows that module's `count` (then the script's, a persisted one, a shared one).

```lua
-- file: state.luau
-- The app's state: every module that requires this one gets these States.
export type Todo = { id: string, title: string, done: boolean }
export type Filter = "all" | "open" | "done"

local M = {}
M.todos = ui.persist("todos", {} :: { Todo })
M.filter = ui.state("filter", "all" :: Filter)
M.draft = ui.state("draft", "")
M.next_id = ui.state("next_id", 1)
return M
```

```lua
-- file: actions.luau
-- What the app does to its state.
local state = require("./state")

local M = {}

function M.add(title: string)
	if title == "" then
		return
	end
	local id = state.next_id:peek()
	ui.batch(function()
		state.next_id:set(id + 1)
		state.todos:push { id = `t{id}`, title = title, done = false }
		state.draft:set("")
	end)
end

function M.toggle(id: string)
	for i, todo in state.todos:peek() do
		if todo.id == id then
			state.todos:set_key({ i, "done" }, not todo.done)
		end
	end
end

function M.clear_done()
	state.todos:update(function(list)
		local open: { state.Todo } = {}
		for _, todo in list do
			if not todo.done then
				table.insert(open, todo)
			end
		end
		return open
	end)
end

return M
```

A state module holds data; logic that only reads it goes in `lib/` as plain functions of their arguments, which are easy to test and to reuse:

```lua
-- file: lib/filter.luau
local state = require("../state")

-- The todos a filter shows, in order.
return function(todos: { state.Todo }, filter: state.Filter): { state.Todo }
	local out: { state.Todo } = {}
	for _, todo in todos do
		if filter == "all" or (filter == "done") == todo.done then
			table.insert(out, todo)
		end
	end
	return out
end
```

(`lib/filter.luau` requires `state.luau` only for its types; a module can require another just for its `export type`s.)

## Typed components

A component module exports its props type and returns its constructor. A plain function suits a piece of interface with no state of its own; `ui.component` gives each instance its own `ui.use_state`, effects that end with it and `ui.on_mount`. Props that take children declare `[number]: Child`; `ui.component` also passes them as `props.children`.

```lua
-- file: components/row.luau
local theme = require("../theme")
local state = require("../state")
local actions = require("../actions")

export type Props = { todo: state.Todo }

return ui.component(function(props: Props)
	local todo = props.todo
	local hovered = ui.use_state(false)
	return ui.row {
		gap = 10,
		px = 10,
		h = 32,
		items = "center",
		rounded = 6,
		bg = function()
			return if hovered:get() then theme.hover else theme.clear
		end,
		on_hover = function(inside)
			hovered:set(inside)
		end,
		on_click = function()
			actions.toggle(todo.id)
		end,
		ui.text { if todo.done then "☑" else "☐", color = theme.accent },
		ui.text { todo.title, color = if todo.done then theme.muted else theme.text },
	}
end)
```

```lua
-- file: components/panel.luau
local theme = require("../theme")

export type Props = { title: string, [number]: Child }

-- A titled box around its children.
return function(props: Props): Node
	return ui.column {
		w_full = true,
		p = 16,
		gap = 8,
		rounded = 10,
		bg = theme.surface,
		ui.text { props.title, bold = true, color = theme.muted, text_size = 12 },
		{ table.unpack(props) },
	}
end
```

`Panel { title = "x", ui.text "y" }` with a misspelled `titel`, a missing `title` or a number for it is an error in the editor. To wrap an element with defaults, take its own props type, copy the table and fill in what is missing:

```lua
-- file: components/field.luau
local theme = require("../theme")

-- An input with the app's look; anything given wins.
return function(props: InputProps): Node
	local p = table.clone(props)
	p.bg = p.bg or theme.surface
	p.rounded = p.rounded or 6
	p.px = p.px or 10
	p.h = p.h or 32
	p.placeholder_color = p.placeholder_color or theme.muted
	return ui.input(p)
end
```

`Merge<A, B>` makes a props type from two: `Merge<ContainerProps, { selected: boolean }>`. Keep a component's own options out of what it passes to an element, which rejects keys it does not know.

```lua
-- file: theme.luau
return {
	text = "#e6edf3",
	muted = "#8b95a5",
	accent = "#5b8def",
	surface = "#161b22",
	hover = "#ffffff0f",
	clear = "#00000000",
	page = "#0d1117",
}
```

```lua
-- file: app.luau
-- Todos: the state, what changes it and the views each in their own
-- modules; this file puts them together.
local theme = require("./theme")
local state = require("./state")
local actions = require("./actions")
local visible = require("@lib/filter")
local Row = require("@components/row")
local Panel = require("@components/panel")
local field = require("@components/field")

local shown = ui.computed(function()
	return visible(state.todos:get(), state.filter:get())
end)

local function tab(filter: state.Filter, label: string): Node
	return ui.button {
		bg = function()
			return if state.filter:get() == filter then theme.hover else theme.clear
		end,
		on_click = ui.set(state.filter, filter),
		label,
	}
end

return ui.column {
	w_full = true,
	h_full = true,
	p = 24,
	gap = 16,
	bg = theme.page,
	color = theme.text,
	Panel {
		title = "NEW",
		ui.row {
			gap = 8,
			field {
				value = state.draft,
				label = "New todo",
				placeholder = "What needs doing?",
				grow = 1,
				on_key = function(e)
					if e.key == "Enter" then
						actions.add(state.draft:peek())
						return true
					end
					return false
				end,
			},
			ui.button {
				on_click = function()
					actions.add(state.draft:peek())
				end,
				"Add",
			},
		},
	},
	Panel {
		title = "TODOS",
		ui.row { gap = 4, tab("all", "All"), tab("open", "Open"), tab("done", "Done") },
		ui.each(shown, function(todo: state.Todo)
			return Row { todo = todo }
		end, {
			key = function(todo: state.Todo)
				return todo.id
			end,
			gap = 2,
		}),
		ui.text {
			function()
				local left = 0
				for _, todo in state.todos:get() do
					left += if todo.done then 0 else 1
				end
				return `{left} left`
			end,
			color = theme.muted,
		},
		ui.button { on_click = actions.clear_done, "Clear done" },
	},
}
```

## Hot reload and modules

- A save to any module reloads everything: the script and every module run again in a new VM, in the order they are required. A module's top-level code runs at its first `require` in each load.
- State survives by key, as React's Fast Refresh keeps state: changing a state's initial value does nothing to a running app, and changing its key starts it over from the new initial value. `ui.state` and `ui.persist` keys include the module's path, so moving a state to another module, or renaming the module, starts it from its initial value once. `ui.shared` keys are global.
- What a module does at its top level happens on every load: effects and computed values are made again (the previous load's end with it), commands are declared again. Put what must happen once per launch, not once per load, behind state: the terminal's `saved.luau` restores the saved layout only while there are no tabs.
- Terminal sessions (`ui.term(id)`) and web pages (`ui.web(id)`) belong to the host and outlive reloads; each load asks for them again by id.
- An error in a module shows with the module's path (`components/row.luau:12`). A failed reload keeps the last good interface.
- A require cycle is an error. When a lower module needs to call into a higher one (the terminal's `sessions.luau` must close a pane, which `actions.luau` does, and `actions` requires `sessions`), the lower module exports a hook and the higher one sets it:

```luau
-- sessions.luau
M.close_pane = function(_pane: string, _force: boolean?) end
-- actions.luau
sessions.close_pane = M.close_pane
```

## The terminal example

`crates/zgx-desktop/examples/terminal` is a 2,000-line app in nineteen modules:

| Module | |
| --- | --- |
| `app.luau` | requires the parts, restores the layout, declares the menus, returns titlebar, panes and sheet |
| `state.luau` | the tabs (each a split tree), the active tab, each tab's focused pane, split ratios, zoom, drag state; typed (`Tab`, `Drop`, `Payload`…) |
| `sessions.luau` | the terminals and web pages in panes: making them, naming them, focusing them |
| `actions.luau` | tabs and panes: new, split, close (asking first), move, reorder, step focus |
| `saved.luau` | restoring the last launch's layout and saving it as it changes |
| `commands.luau` | the menu bar's commands and their keys |
| `frames.luau` | where the active tab's panes and dividers go, and where a drag would drop |
| `components/` | `tabs.luau` (the titlebar), `pane.luau`, `browser.luau`, `divider.luau`, `content.luau`, `confirm.luau`, `menu.luau`, `controls.luau` |
| `lib/` | `layout.luau` (the split tree as plain data), `names.luau` (what a pane is called) |
| `theme.luau`, `icons.luau` | colors and sizes; SVG icons by name |

Its headless test (`crates/zgx-script/tests/terminal_example.rs`) drives it through its commands and pointer, and `zgx check crates/zgx-desktop/examples/terminal` checks every module.
