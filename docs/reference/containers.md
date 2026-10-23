# Containers: column, row, div, overlay, grid

Boxes that lay out their children. Each takes one table: string keys are [properties](properties.md) and [events](events.md), array entries are children.

```luau
ui.column(props: ContainerProps?) -> Node
ui.row(props: ContainerProps?) -> Node
ui.div(props: ContainerProps?) -> Node
ui.overlay(props: ContainerProps?) -> Node
ui.grid(props: ContainerProps?) -> Node
```

| Function | Lays its children out |
| --- | --- |
| `column` | top to bottom (a flex column) |
| `row` | left to right (a flex row) |
| `div` | as `column` does; `direction = "row"` turns it |
| `overlay` | all in one box, each aligned on its own: `items` across, `justify` down. With neither set they stretch to fill it, except along an axis they size themselves on. The overlay is as large as its largest child |
| `grid` | in equal columns and rows: a `div` with `grid = true`; `cols` and `rows` set the tracks, children take `col_span`, `row_span`, `col_start`, `row_start` |

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
- A percentage size inside a box sized by its content falls back to the content's size (`ZGUI_LAYOUT_WARNINGS=1` reports it).
- `show`, `switch` and `presence` lay out as what they show, as if it stood in their place; `each` and `slot` are a box of their own that lays its rows out as its parent would. Absolute positioning searches for a `relative` ancestor, so mark the one you mean rather than relying on being its direct child ([properties](properties.md#position)).
- A container returned by a function is built once, when the function runs: to make a part of the tree follow state, use [`show`, `switch`, `each`](dynamic.md) or a function property.
