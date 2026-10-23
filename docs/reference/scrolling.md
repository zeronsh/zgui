# Scrolling: scroll, scroll_x, list

`scroll` and `scroll_x` are viewports over ordinary children, all of them mounted. `list` is virtualized: of its `count` rows, only those in view (and a few beyond) are built, so it holds a million rows as cheaply as twenty.

```luau
ui.scroll(props: ScrollProps?) -> Node
ui.scroll_x(props: ScrollProps?) -> Node
ui.list(props: ListProps) -> Node
```

## scroll and scroll_x

`scroll` stacks its children top to bottom and scrolls vertically; `scroll_x` lays them in a row and scrolls horizontally. The wheel scrolls the innermost viewport that can still move, and passes to an outer one at an edge; focus moving into a viewport (Tab, `el:focus()`) scrolls the focused node into view.

| Option | Type | Default | |
| --- | --- | --- | --- |
| children | `Child` | none | |
| `offset` | `State<number>` | its own | the scroll position in points, both ways: scrolling writes it, setting it scrolls (clamped) |
| `follow` | `boolean` | `false` | stays at the end while the content grows, until scrolled away from it (a transcript, a log) |
| `scrollbar` | `boolean` | `false` | an interactive scrollbar over the trailing edge: drag it, click its track to page; overlay style (hidden until scrolling) where the system has overlay scrollbars |
| `w`, `h` | `Length` | `320` by `240` when nothing else sizes it | a stretching parent, `grow`, `w`/`h` or `w_full` size it instead |
| `gap`, `items`, `p`… | | | lay out the content as a column's (a row's for `scroll_x`) |
| `overflow` | `Overflow` | `"hidden"` | |

The content fills the viewport's width (`scroll`) or height (`scroll_x`) and is as long as its children make it.

## list

| Option | Type | Default | |
| --- | --- | --- | --- |
| `count` | `Reactive<number>` | required without `heights` | how many rows |
| `row` | `(index: number) -> Node` | required | builds row `index`, from 1 |
| `height` | `number` | `28` | every row's height, in points (positive) |
| `heights` | `{ number }`, a state of one, or a function returning one | none | each row's own height instead, in order; the count is its length |
| `sticky` | `{ number }`, a state of one, or a function returning one | none | with `heights`: rows (from 1) that stick to the top while the rows after them scroll past, each pushed up by the next (section headers, a diff's file headers); the one in charge is built again over the list, as any row is |
| `sticky_top` | `number` | `0` | how far below the top the sticky rows stick (CSS's sticky `top`): what scrolls above them still shows, so cover it if it shouldn't |
| `overscan` | `number` | `2` | rows built beyond each edge of the view |
| `offset` | `State<number>` | its own | the scroll position in points, as `scroll`'s |
| `scrollbar` | `boolean` | `false` | an interactive scrollbar, as `scroll`'s |
| `w`, `h` | `Length` | `320`, `240` | |

With `heights`, rows differ in height but are still never measured: the list knows where each starts, so a changed list (a section folded, rows inserted) re-lays out without building anything offscreen. A host can publish the heights as a shared value (`ScriptHost::shared`), and the script never copies them.

A row is built when it scrolls into view and dropped when it leaves; it owns what it makes, as a [component](state.md#components) does: its `ui.use_state`, effects and computed values end with it.

## Example

```lua
type Message = { from: string, text: string }
local messages = ui.state("messages", {} :: { Message })
local offset = ui.state("offset", 0)
return ui.row {
	p = 16,
	gap = 16,
	items = "start",
	-- A transcript: every message mounted, kept at the end as it grows.
	ui.column {
		gap = 8,
		ui.scroll {
			w = 260,
			h = 200,
			gap = 6,
			follow = true,
			ui.each(messages, function(m: Message)
				return ui.text { `{m.from}: {m.text}`, wrap = true }
			end),
		},
		ui.button {
			on_click = function()
				messages:push({ from = "Ada", text = `Message {messages:len() + 1}` })
			end,
			"Send",
		},
	},
	-- A hundred thousand rows; about ten are built at a time.
	ui.column {
		gap = 8,
		ui.list {
			count = 100000,
			height = 24,
			w = 200,
			h = 200,
			offset = offset,
			row = function(i)
				return ui.text { `Row {i}`, w_full = true, h = 24, px = 8 }
			end,
		},
		ui.text { "{offset}" },
		ui.button { on_click = ui.set(offset, 0), "Top" },
	},
}
```

## Gotchas

- Every list row is `height` points: a taller row overlaps the next and a shorter one leaves a gap. For rows of different heights give `heights` (each row's own, still never measured).
- A list row is built once for its index, when it comes into view. Reading state in `row` (`items:get()[i]`) takes the value at that moment; to follow a change, read it in a function property or text inside the row (`ui.text(function() return items:get()[i].name end)`), or change `count`. Rows are keyed by index, so a row's own state stays with its position, not its data.
- List rows do not stretch across the list: give a row's root `w_full = true`.
- A `scroll` with no size and nothing sizing it is 320 by 240 along its scrolling axis; across it, a column or row stretches it (the default `items`), and inside one with `items = "start"` it keeps 320. Use `w_full = true`, `grow = 1` or a stretching parent.
- All of a `scroll`'s children stay mounted however far away they are; past a few thousand rows, use `list`.
- `offset` counts points from the top (the left for `scroll_x`). Setting it beyond the content clamps it, and a binding to a state that is not a number reads as 0.
- There is no scrollbar unless asked (`scrollbar = true`): a viewport scrolls by wheel, touchpad, keyboard focus and `offset`, and with the keys as a page does (Space, Shift+Space, Page Up/Down, Home/End, arrows) once it has been pressed in or is under the pointer, while nothing has focus (a focused control keeps its keys).
