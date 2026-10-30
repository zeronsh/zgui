# Dynamic content: show, switch, each, presence

Parts of the tree that follow state. A script describes its tree once; these functions are how a part of it comes and goes, or repeats, as state changes, without the script running again except to build what appears.

```luau
ui.show(cond: boolean | Readable<unknown> | (() -> unknown), node: Node, otherwise: Node?) -> Node
ui.switch(value: string | Readable<unknown> | (() -> unknown), cases: { [CaseKey]: Node }, default: Node?) -> Node
ui.each(list: State<{ any }> | { any }, build: (item: any, index: number, row: State<any>) -> Node, opts: EachOptions?) -> Node
ui.presence(value: string | Readable<unknown> | (() -> unknown), build: (value: any) -> Node, opts: PresenceOptions?) -> Node
```

| Function | Shows |
| --- | --- |
| `show` | `node` while `cond` is truthy (a state, a function of state, or a boolean), else `otherwise` or nothing |
| `switch` | the case whose key equals `value` (or its text: a case `[1]` matches `"1"`), else `default` or nothing |
| `each` | `build(item, index)` for each item of a list state, or of a plain table that never changes, `index` from 1; a row whose item changes is built again, or, if `build` takes a third parameter, gets its item as state and updates in place; rows that come, go and move can animate |
| `presence` | what `build(value)` makes of the current value; when it changes, the old content leaves (playing `exit`) while the new enters (`enter`). `nil` shows nothing |

`show` and `switch` build the node they show when it appears, from the node you gave: the nodes are descriptions, so state they create inside a component is fresh each time. `each` rows and `presence` content are built by your function, which owns what it creates (`ui.computed`, `ui.effect`, `ui.use_state`): they end when the row or content goes.

## Options

`show` and `switch` take no options. `each` and `presence` take their options and any [property](properties.md) in one table:

| Option | Type | Default | |
| --- | --- | --- | --- |
| `key` (`each`) | `(item: any) -> string \| number` | position | a row's identity: rows with the same key keep their state, element and place when the list reorders |
| `animate` (`each`) | `boolean \| Easing \| { duration: number?, ease: Easing?, bounce: number?, spring: AnimeEase? }` | off | rows the list moves slide from where they were: `true` is a spring (bounce .1, 400 ms); an ease name or `{ duration, ease }` (350 ms unless given); `{ bounce, duration }` a spring of that bounce; `{ spring = anime.spring { … } }` |
| `enter` | `ElementParams` | none | anime.js parameters played on what appears (`{ opacity = { 0, 1 }, duration = 200 }`) |
| `exit` | `ElementParams` | none | played on what leaves; it leaves the flow where it was, as large as it was, and unmounts when the animation completes |
| `initial` | `boolean` | `false` | the first rows (or the first value) play `enter` too |
| `reorder` (`each`) | `(from: number, to: number) -> ()` | none | rows the person drags into a new order, or moves with Alt+↑ and Alt+↓, as dnd-kit's sortable lists: a row dropped on another goes above or below it by where it drops, a line showing where. `reorder(from, to)` moves the script's items (positions from 1, `to` where the row ends up). Each row is a Tab stop a reader names and is told how to move, and a keyboard move is heard ("Moved Inbox to position 2 of 5"). Needs `key`; works with `animate` |
| `item_label` (`each`) | `(item: any) -> string` | the item's key | what a moved row is called as its move is heard |
| `mode` (`presence`) | `"wait"` | both at once | the new content enters once the old has left |
| properties | [Props](properties.md) | | `each`: style the box its rows lay out in. `presence`: sizing (`w`, `h`, `w_full`, `grow`, `p`, `absolute` and insets…) makes it a box its content fills |

## Example

```lua
type Task = { id: string, title: string, done: boolean }
local tasks = ui.state(
	"tasks",
	{
		{ id = "a", title = "Write the docs", done = false },
		{ id = "b", title = "Ship it", done = true },
	} :: { Task }
)
local filter = ui.state("filter", "all")
local selected = ui.state("selected", nil :: string?)
local made = 0

local function row(task: Task, i: number)
	return ui.button {
		w_full = true,
		on_click = ui.set(selected, task.id),
		ui.text { if task.done then "✓" else "·", w = 16 },
		ui.text { task.title, grow = 1 },
		ui.button {
			on_click = function()
				tasks:remove(i)
			end,
			"Remove",
		},
	}
end

return ui.column {
	w_full = true,
	p = 16,
	gap = 10,
	ui.row {
		gap = 6,
		ui.button { on_click = ui.set(filter, "all"), "All" },
		ui.button { on_click = ui.set(filter, "open"), "Open" },
		ui.button {
			on_click = function()
				made += 1
				tasks:insert(1, { id = `new{made}`, title = `Task {made}`, done = false })
			end,
			"Add",
		},
	},
	ui.switch(filter, {
		all = ui.text { "Every task", color = "#8a9ab0" },
		open = ui.text { "Open tasks first", color = "#8a9ab0" },
	}),
	ui.each(tasks, row, {
		gap = 4,
		key = function(task: Task)
			return task.id
		end,
		animate = true,
		enter = { opacity = { 0, 1 }, duration = 200 },
		exit = { opacity = 0, duration = 150 },
	}),
	ui.show(function()
		return #tasks:get() == 0
	end, ui.text { "Nothing to do" }),
	-- The selected task's id, crossfading as it changes.
	ui.presence(selected, function(id: string)
		return ui.text { "Selected: " .. id }
	end, {
		enter = { opacity = { 0, 1 }, duration = 160 },
		exit = { opacity = 0, duration = 120 },
		mode = "wait",
	}),
}
```

## Layout animation

`each`'s `animate` is one of three ways layout moves animate; the others are [properties](properties.md#size-and-position-animation): `layout = "position"` slides a node when layout moves it within its parent, and `layout_id = "selection"` hands a node's box to the next node mounted with the same id (a tab's highlight sliding to the tab that takes it). All hold the node where it showed with a translation that eases to nothing, on the compositor: no layout and no script runs while it moves. A row that leaves (`exit`) leaves the flow at once, so the rows after it close up while it plays.

## Gotchas

- A node at opacity 0 takes no pointer, unlike CSS (where `opacity: 0` is still clicked): a row fading in from 0 (`enter = { opacity = { 0, 1 } }`) takes clicks and drops from its first visible frame, and a row fading out stops taking them once it is gone. To keep something invisible but clickable, give it a small opacity; to make something visible take no pointer, `pointer_events = "none"`.

- `each`'s list must be a state (`ui.state`, `ui.computed`, a component's `ui.use_state`), not a function or a plain table: wrap a derived list in `ui.computed`.
- Without `key`, a row is its item: it stays with its item through reorders, inserts and removals (a list built afresh from state, with equal tables, included), keeping its own state and elements, and an edit rebuilds only the edited row. Items equal in value are told apart by order. Give a `key` when an item's identity is not its value (an edited record should keep its row), and for `animate`, `enter` and `exit`, which need one to tell a moved row from a new one.
- `key` is a function of its item alone, as React's keys are: it runs for an item when the item is a new value, and an item a change left as it was keeps the key it had (a list of a thousand rows that changes in one calls `key` once). A `key` reading other state is not run again when that state changes.
- Rows are built once per key: a row's item is the value when it was built. Read changing parts of the item through state (or key by the parts that change) rather than expecting the build function to run again.
- `switch` picks among nodes you built up front. To build content from the value itself (the open message's view, from its id), use `presence`, whose function gets the value.
- `switch`'s and `presence`'s value must be a string when it is a literal (`ui.switch("a", …)`); states and functions may hold any value. A literal `nil` is an error: use a state or a function that returns `nil` (`presence(function() return if open:get() then "panel" else nil end, …)`).
- `presence` keeps content while its value stays equal: a new value builds new content, the same value does not rebuild it.
- A `presence` box showing nothing is not there for the pointer: a sheet's full-window box (`absolute`, every inset 0) put away leaves presses, drops and the terminals and pages under it alone. Nor is it while what it shows is `hidden` or `invisible`: a page kept mounted once built (`presence(function() built = built or shown(); return if built then "settings" else nil end, …)` with `hidden = function() return not shown() end`) leaves the web page under it its presses while it is hidden. Showing content, the box is the content's.
- `show`, `switch` and `presence` lay out as what they show, as if it stood in their place: its `grow`, `self`, percentages and absolute insets reach the parent. `each` (and `slot`) is a box of its own that lays its rows out as its parent would (a row's `direction`, `gap`, `items`, `justify` and wrapping), stretched across a column. An absolutely positioned row positions against the nearest `relative` ancestor, which `each`'s box is not: mark the one you mean.
- A row stays while its key does. When its item changes, a two-parameter `build` runs again for that row (the others stay as they are); a `build` taking a third parameter, `function(item, i, row)`, runs once and gets `row`, a state that follows the item, so the row's own computeds, functions and nested `each` update in place: a reply streaming into a transcript grows by its last block, not by building the reply again. With the host keeping what did not change ([`Shared`](state.md#shared-state-from-rust)), a list of a thousand rows that changes in one costs one row's work.
- `each`'s row function gets the item as `any`: annotate it (`function(task: Task, i)`) to have it checked ([why](index.md#checking)).
