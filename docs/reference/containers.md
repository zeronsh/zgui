# Containers: column, row, div, overlay, grid, split, tabs, tree, data_grid, disclosure, accordion, separator

Boxes that lay out their children. Each takes one table: string keys are [properties](properties.md) and [events](events.md), array entries are children.

```luau
ui.column(props: ContainerProps?) -> Node
ui.row(props: ContainerProps?) -> Node
ui.div(props: ContainerProps?) -> Node
ui.overlay(props: ContainerProps?) -> Node
ui.grid(props: ContainerProps?) -> Node
ui.split(props: SplitProps) -> Node
ui.tabs(props: TabsProps) -> Node
ui.tree(props: TreeProps) -> Node
ui.data_grid(props: DataGridProps) -> Node
ui.disclosure(props: DisclosureProps) -> Node
ui.accordion(props: AccordionProps) -> Node
ui.section(props: SectionProps) -> Node
ui.separator(props: SeparatorProps?) -> Node
```

| Function | Lays its children out |
| --- | --- |
| `column` | top to bottom (a flex column) |
| `row` | left to right (a flex row) |
| `div` | as `column` does; `direction = "row"` turns it |
| `overlay` | all in one box, each aligned on its own: `items` across, `justify` down. With neither set they stretch to fill it, except along an axis they size themselves on. The overlay is as large as its largest child |
| `grid` | in equal columns and rows: a `div` with `grid = true`; `cols` and `rows` set the tracks, children take `col_span`, `row_span`, `col_start`, `row_start` |
| `split` | its two children side by side (`vertical = true`: one over the other) with a divider between them that drags to resize the first; the second fills the rest |

## Options

Children, and any [property](properties.md) and [event](events.md). What the containers start with:

| Option | Type | Default | |
| --- | --- | --- | --- |
| children (array entries) | `Child` | none | nodes, strings (text), functions (computed text), nested lists (flattened); `nil` and `false` are skipped |
| `items` | `"start" \| "center" \| "end" \| "baseline" \| "stretch"` | `"stretch"` | alignment across the main axis: as CSS's flexbox, children stretch across it unless they have a size of their own there; `"start"` keeps each as wide (or tall) as it is |
| `justify` | `"start" \| "center" \| "end" \| "between" \| "around" \| "evenly" \| "stretch"` | `"start"` (`overlay`: `"stretch"`) | distribution along it |
| `gap` | `number` | `0` | between children; `gap_x`, `gap_y` per axis |
| `grow`, `shrink` | `number` | `0`, `1` | as a child of a row or column: it gives way in one too small for its children, no further than its content needs |
| `cols`, `rows` | `number` | unset | `grid`'s equal tracks: `cols = 3` makes three columns |

## split

```lua
local width = ui.persist("sidebar", 280)
return ui.split {
	size = width,
	min = 200,
	max = 480,
	min_rest = 320,
	h_full = true,
	ui.column { p = 12, ui.text { "Sidebar" } },
	ui.column { p = 12, ui.text { "Content" } },
}
```

| Option | Type | Default | |
| --- | --- | --- | --- |
| `size` | `State<number>` | a state of its own, at 280 | the first pane's width (height, `vertical`) in points, both ways: the divider writes it as it drags, and a `ui.persist` keeps the layout across launches |
| `vertical` | `boolean` | `false` | one pane over the other |
| `second` | `boolean` | `false` | `size` is the second pane's (an inspector at the right), and the first fills the rest |
| `min`, `max` | `number` | `0`, none | the first pane's bounds |
| `label` | `string` | `"Resize"` | what assistive technology calls the divider, which it reads as a splitter with its size and range, and can set |
| `min_rest` | `number` | `0` | the second pane's smallest size: the divider stops short of it, and a window too small for both shrinks the first pane, down to `min` |

The divider drags without running script (only the panes lay out again), shows a resize cursor, takes Tab focus (the arrows move it ten points, a hundred with Shift; Home and End take it to its ends), and goes back to where it started on a double click. Assistive technology sees a splitter.

## tabs

A strip of tabs and the panel of the chosen one, as WAI-ARIA's tabs and an editor's tab strip. One Tab stop: the arrows move among the tabs and the choice follows (mirrored right to left), Home and End go to the ends. A closable tab closes with its ×, Delete, or a middle click; closing the chosen one chooses its neighbour. Reorderable tabs drag along the strip, or move with ⌘ (Ctrl) Shift and an arrow. A strip too long for its width scrolls, and a menu at its end lists every tab.

```lua
local items = ui.state(
	"items",
	{
		{ id = "a", label = "README.md", closable = true },
		{ id = "b", label = "main.rs", closable = true },
	} :: { TabItem }
)
local chosen = ui.state("chosen", "a")
return ui.column {
	w = 400,
	h = 200,
	ui.tabs {
		label = "Files",
		items = items,
		value = chosen,
		reorderable = true,
		panel = function(id: string): Node
			return ui.text { `Editing {id}`, p = 12 }
		end,
	},
}
```

| Option | Type | Default | |
| --- | --- | --- | --- |
| `items` | `State<{ TabItem }>` | required | `{ id, label, closable? }` each, two-way: closing removes one, reordering moves it; other fields are kept as they are |
| `value` | `State<string>` | required | the chosen tab's id |
| `panel` | `(id: string) -> Node` | required | the chosen tab's panel, built as it is chosen |
| `reorderable` | `boolean` | `false` | |
| `keep` | `boolean` | `false` | a panel stays mounted once its tab is chosen, as Radix's `forceMount`, and goes when the tab closes: an editor keeps its undo, caret and scroll. Hidden, it takes no layout, paint or frames; its reactive bindings still follow state, and a script's own timers keep running. A strip then holds a panel per tab it opened, so a `ref` shared by every panel finds the first mounted, not the one shown: give each panel its own (a ref per tab id) |
| `group` | `string` | none | `reorderable` strips of one group, in one window, take each other's tabs: a tab dragged over another's strip or panel goes there and is chosen, as in an editor's split panes. It arrives as `{ id, label, closable }`, without other fields, and its panel is built there, so `keep`'s state doesn't move with it |
| `on_move` | `(id: string, to: string) -> ()` | none | a tab of this strip went to another of its group: its id and that strip's `label`, for an app's undo |
| `on_close` | `(id: string) -> boolean?` | none | asked before a tab closes; `false` keeps it open (an editor asking to save first) |
| `label` | `string` | `""` | the strip's accessible name |

## tree

Items in a hierarchy, as WAI-ARIA's tree and Finder's sidebar. The tree is one Tab stop and keeps focus; its current item is its active descendant, which assistive technology reads with its level and its place among its siblings. Up and Down move; Right opens an item and then enters it, Left closes it and then goes to its parent (mirrored right to left); Home and End go to the ends; typing goes to the next item starting with it; Enter or a double click activates. A click chooses, and a click on the chevron opens and closes. The rows are virtual: a tree of thousands builds only what shows.

```lua
local items = ui.state(
	"items",
	{
		{ id = "src", label = "src", children = { { id = "main", label = "main.luau" } } },
		{ id = "docs", label = "docs", lazy = true },
	} :: { TreeNode }
)
local open = ui.state("open", {} :: { string })
local current = ui.state("current", nil :: string?)
return ui.column {
	h = 240,
	ui.tree {
		label = "Files",
		items = items,
		open = open,
		value = current,
		on_open = function(id: string)
			if id == "docs" then
				items:set_key({ 2, "children" }, { { id = "guide", label = "guide.md" } })
			end
		end,
	},
}
```

| Option | Type | Default | |
| --- | --- | --- | --- |
| `items` | `State<{ TreeNode }>` | required | `{ id, label?, children?, lazy? }` each; `lazy = true` without children shows "Loading…" until the app adds them |
| `open` | `State<{ string }>` | required | the open items' ids, both ways |
| `value` | `State<string?>` | required | the current item's id, both ways |
| `on_open` | `(id: string) -> ()` | none | an item opened: load its children now |
| `on_activate` | `(id: string) -> ()` | none | Enter or a double click |
| `label` | `string` | `""` | its accessible name |

## data_grid

Records in rows under a header of columns, as WAI-ARIA's grid and a spreadsheet's table. The grid is one Tab stop and keeps focus; its current cell is its active descendant, which assistive technology reads with its row and column (the grid's size counts the rows not built). The arrows move (mirrored right to left), Home and End go to the row's ends and, with ⌘ or Ctrl, the grid's; Page Up and Down go a page. Enter or F2 edits an editable cell, as typing over it does; Enter keeps the edit and moves down, Escape drops it, and Delete clears the cell. Enter on a cell that doesn't edit activates its row, as a double click does. Up from the first row reaches the header: Enter or Space sorts its column (ascending, descending, then as given), ⌘⇧ (Ctrl+Shift) with an arrow moves it, Alt with an arrow narrows or widens it. With the pointer, a header's click sorts, a drag moves its column, and its edge's drag resizes it (a double click there puts the width back). The rows are virtual: a grid of a hundred thousand builds only what shows.

```lua
local people = ui.state("people", {
	{ name = "Ada", age = 36 },
	{ name = "Grace", age = 85 },
	{ name = "Linus", age = 21 },
})
local columns = ui.state(
	"columns",
	{
		{ id = "name", title = "Name", width = 160, sortable = true, editable = true },
		{ id = "age", title = "Age", width = 80, sortable = true, editable = true, align = "end" },
	} :: { GridColumn }
)
local sort = ui.state("sort", nil :: GridSort?)
return ui.column {
	h = 240,
	ui.data_grid {
		label = "People",
		columns = columns,
		rows = people,
		sort = sort,
		on_activate = function(row: number)
			ui.toast { title = people:get()[row].name }
		end,
	},
}
```

Each cell shows its row's field named by the column's `id` (or what `cell` makes of it). ⌘C (Ctrl+C) copies as a spreadsheet does: the current cell, or the selected rows, as tab-separated text and an HTML table. The grid sorts `rows` itself (numbers by value, text without case); `row` in `value`, `on_edit` and `on_activate` is the record's index in `rows`, whatever the sort.

| Option | Type | Default | |
| --- | --- | --- | --- |
| `columns` | `{ GridColumn }` or a state of them | required | `{ id, title?, width?, min_width?, sortable?, editable?, align? }` each; `width` 120 unless set, `align = "end"` for numbers. A state is written as columns are resized and moved, the app's other fields kept |
| `rows` | `State<{ any }>` | required | the records |
| `sort` | `State<GridSort?>` | none | `{ column, descending? }`, both ways; nil shows `rows` as they are |
| `value` | `State<{ row: number, column: string }?>` | none | the current cell, both ways |
| `selection` | `State<{ number }>` | none | the selected rows, both ways: a click selects one, Shift (or Shift and an arrow) extends from the last, ⌘ (Ctrl) adds or takes one away, ⌘A (Ctrl+A) selects all |
| `cell` | `(value: any, column: string, record: any) -> string` | the value as text | a cell's text: `function(v, column) return if column == "price" then string.format("$%.2f", v) else tostring(v) end` |
| `on_edit` | `(row: number, column: string, text: string) -> ()` | none | an edit kept; without it the text is written into the record, a number staying a number |
| `on_activate` | `(row: number) -> ()` | none | Enter on a cell that doesn't edit, or a double click |
| `label` | `string` | `""` | its accessible name |

In Rust (`zgx-components`), `data_grid(label, columns, sort, current, GridSource::new(rows, cell).on_edit(…).on_activate(…))`: `cell(row, column)` gives each cell's text, and the app gives its rows in the order `sort` asks.

## disclosure, accordion, separator

`ui.disclosure { open = state, summary = "Advanced", … }` shows its children while `open`, as HTML's `<details>`: its summary is a button (Enter or Space toggles it) that says it is `expanded`, with a chevron that turns as it opens. `summary` is text or a node.

`ui.accordion { open = state, ui.section { id = "ship", title = "Shipping", … }, … }` holds sections of which one at most is open: `open` is its `id`, or nil; opening one closes the one open. A section's children are built when it opens.

`ui.separator {}` is a line between groups across a menu or a list (`vertical = true`: down a toolbar), in the theme's subtle border colour and hidden from assistive technology.

```lua
local open = ui.state("open", false)
local section = ui.state("section", "ship" :: string?)
return ui.column {
	w = 320,
	gap = 8,
	ui.disclosure { open = open, summary = "Advanced", ui.text { "Hidden until opened" } },
	ui.separator {},
	ui.accordion {
		open = section,
		ui.section { id = "ship", title = "Shipping", ui.text { "By sea" } },
		ui.section { id = "pay", title = "Payment", ui.text { "By card" } },
	},
}
```

## Example

```lua
local selected = ui.state("selected", "Inbox")
local function item(name: string)
	return ui.button {
		w_full = true,
		bg = function()
			return if selected:get() == name then "#2f4058" else "#00000000"
		end,
		on_click = ui.set(selected, name),
		name,
	}
end
return ui.row {
	w_full = true,
	h_full = true,
	items = "stretch",
	ui.column { w = 180, p = 8, gap = 2, bg = "#161b22", item("Inbox"), item("Archive") },
	ui.column {
		grow = 1,
		p = 24,
		gap = 12,
		ui.text { "{selected}", text_size = 22, bold = true },
		ui.grid {
			cols = 3,
			gap = 8,
			ui.div { h = 60, rounded = 8, bg = "#232e40", col_span = 2 },
			ui.div { h = 60, rounded = 8, bg = "#232e40" },
		},
		-- A badge over a tile's top right corner.
		ui.overlay {
			items = "end",
			justify = "start",
			ui.div { w = 64, h = 64, rounded = 12, bg = "#5b8def" },
			ui.row {
				w = 18,
				h = 18,
				rounded = 9,
				bg = "#e5534b",
				items = "center",
				justify = "center",
				ui.text { "3", text_size = 11 },
			},
		},
	},
}
```

## Gotchas

- Children stretch across a column or row by default (`items = "stretch"`, as CSS): `items = "start"` keeps each as wide (or tall) as it is.
- Children shrink in a row too narrow for them (`shrink = 1`, as CSS), but a text no narrower than its longest word, and a box no narrower than its content needs. `shrink = 0` holds a child at its size; `min_w = 0` lets one shrink past its content (with `truncate = true` on text to end it with "…").
- Text does not wrap by default: `wrap = true` on the text, inside a box with a width.
- A percentage size inside a box sized by its content falls back to the content's size (`ZGX_LAYOUT_WARNINGS=1` reports it).
- `show`, `switch` and `presence` lay out as what they show, as if it stood in their place; `each` and `slot` are a box of their own that lays its rows out as its parent would. Absolute positioning searches for a `relative` ancestor, so mark the one you mean rather than relying on being its direct child ([properties](properties.md#position)).
- A container returned by a function is built once, when the function runs: to make a part of the tree follow state, use [`show`, `switch`, `each`](dynamic.md) or a function property.
