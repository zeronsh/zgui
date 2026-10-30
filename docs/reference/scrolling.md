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
| `row_text` | `(index: number) -> string` | the row's own texts | the plain text of row `index`: in a selectable region, copying a selection that spans rows the list has not mounted (select-all over a long transcript) takes theirs from it. Without it, the list builds the row's description (not mounted) and takes its texts, a `row`'s children joined by tabs and others by line breaks, as a copy of the row on screen does; give it for rows drawn or built in ways that don't say their text |
| `follow` | `boolean` | `false` | stays at the end while the content grows, until scrolled away from it (a transcript, a log) |
| `scrollbar` | `boolean` | `false` | an interactive scrollbar over the trailing edge: drag it, click its track to page; overlay style (hidden until scrolling) where the system has overlay scrollbars |
| `overscroll` | `"auto"` \| `"contain"` | `"auto"` | `"contain"`: a wheel or trackpad gesture that starts over it stays with it at its edges, momentum included, as CSS's `overscroll-behavior: contain`: the scroll views around it neither move nor stretch (a transcript inside a page). `"auto"` hands what it can't scroll on to them |
| `w`, `h` | `Length` | `320` by `240` when nothing else sizes it | a stretching parent, `grow`, `w`/`h` or `w_full` size it instead |
| `gap`, `items`, `p`… | | | lay out the content as a column's (a row's for `scroll_x`) |
| `overflow` | `Overflow` | `"hidden"` | |

The content fills the viewport's width (`scroll`) or height (`scroll_x`) and is as long as its children make it.

A child with `sticky_top` holds at the top as the view scrolls past it, inside its parent: section headers ([sticky](properties.md#sticky)).

## list

| Option | Type | Default | |
| --- | --- | --- | --- |
| `count` | `Reactive<number>` | required without `heights` or `data` | how many rows |
| `data` | `Readable<{ any }>` | none | the list its rows show, as `ui.each`'s (not with `count`; with `heights`, one height an item): `key` is given an item, `row` the item and its index (a state). A two-parameter `row` is built again when its item changes; one taking a third parameter, `function(item, index, row)`, is built once and gets `row`, its item as a state kept current, so the row's functions update in place: a reply streaming into a transcript runs its own row alone |
| `row` | `(index: number) -> Node`, or with `key` `(key, index: State<number>) -> Node`, or with `data` `(item, index: State<number>, row: State<any>?) -> Node` | required | builds row `index`, from 1; with `key`, the row of item `key`, given its index as a state that follows the row; with `data`, the row of an item |
| `key` | `(index: number) -> number \| string \| boolean` | none | each row's item's key (with `data`, a function of the item): a row follows its item as the list changes, moved rather than built or written again. Without it, rows are by position: a removal or an insertion writes every row after it with the item now there (its text, layout and paint done again). Keys are unique; not with `sticky` |
| `height` | `number` | `28` | every row's height, in points (positive); with `measured`, the estimate for a row not yet measured |
| `measured` | `boolean` | `false` | rows take their content's height (wrapped text, images), measured as they mount; not with `heights` |
| `follow` | `boolean` | `false` | with `measured` or `heights`: resting at the end, it stays at the end as rows are added and measured (a chat); scrolled away, it stays where it is |
| `heights` | `{ number }`, a state of one, or a function returning one | none | each row's own height instead, in order; the count is its length |
| `sticky` | `{ number }`, a state of one, or a function returning one | none | with `heights`, and with or without `key`: rows (from 1) that stick to the top while the rows after them scroll past, each pushed up by the next (section headers, a diff's file headers); the one in charge is built again over the list, as any row is |
| `sticky_top` | `number` | `0` | how far below the top the sticky rows stick (CSS's sticky `top`): what scrolls above them still shows, so cover it if it shouldn't |
| `overscan` | `number` | `2` | rows built beyond each edge of the view |
| `offset` | `State<number>` | its own | the scroll position in points, as `scroll`'s |
| `scrollbar` | `boolean` | `false` | an interactive scrollbar, as `scroll`'s |
| `overscroll` | `"auto"` \| `"contain"` | `"auto"` | as `scroll`'s: `"contain"` keeps a gesture over the list with it at its ends |
| `keyboard` | `boolean` | `false` | its rows are one Tab stop, landing on the row last focused, and the arrows, Page Up/Down, Home and End move focus through every row (mounting it), as a list box's do; typing jumps to the next row whose text starts with what was typed (letters within 750 ms build a prefix, the same letter again cycles), rows not mounted read by `row_text`. A row with `role = "option"` (or `"listitem"`) takes the focus itself, so its `on_focus` can open it (selection following focus); other controls in a row keep their own keys |
| `on_end` | `() -> ()` | none | called as its end comes within `end_distance` points of the bottom of the view (an infinite scroll's next page, `ui.paged`'s `more`): once each time, and again once its rows have grown; while its rows are shorter than the view too, until they fill it or a call adds none. Watched in Rust: no script runs while it scrolls short of it |
| `end_distance` | `number` | `200` | how far from the end `on_end` is called |
| `on_start` | `() -> ()` | none | called as its start comes within `start_distance` points of the top of the view (a history's earlier page, react-virtuoso's `startReached`): once each time, and again once its rows have changed. Rows it adds above keep the rows in view where they are: a list with a `key` and `measured` (or `heights`) finds the row it showed by its key, so the view does not jump. Watched in Rust |
| `start_distance` | `number` | `200` | how far from the start `on_start` is called |
| `at_end` | `State<boolean>` | none | kept saying whether the list rests at its end, within a few points of it (react-virtuoso's `atBottomStateChange`): a "jump to the latest" button shows while it is false, and `scroll_to_row(count, { align = "end" })` takes the view there. Written only as it changes |
| `w`, `h` | `Length` | `320`, `240` | |

With `heights`, rows differ in height but are still never measured: the list knows where each starts, so a changed list (a section folded, rows inserted) re-lays out without building anything offscreen. A host can publish the heights as a shared value (`ScriptHost::shared`), and the script never copies them.

With `measured = true`, rows take the height of what they hold, at the list's width: a chat's or a mail's rows of wrapped text, which a script can't measure. A row is measured as it mounts; rows not yet seen are `height` tall until then, so the scrollbar's length is an estimate that settles as rows are seen. The rows in view stay where they are as rows above them are measured (scrolling up, or jumping far with `offset` or End): only the rows measured move. Positional rows keep heights by position; with `key`, heights go with their items, so a removal or an insertion keeps every measured height.

```lua
type Post = { id: number, author: string, text: string }
local posts = ui.state("posts", {} :: { Post })
return ui.list {
	count = function()
		return posts:len()
	end,
	measured = true,
	-- New posts at the end stay in view while it rests there.
	follow = true,
	-- About one line and a name: the estimate until a row is measured.
	height = 48,
	row = function(i: number)
		local post = posts:peek()[i]
		return ui.column {
			w_full = true,
			px = 12,
			py = 6,
			ui.text { post.author, literal = true, bold = true },
			ui.text { post.text, literal = true, wrap = true },
		}
	end,
}
```

To bring a row into view, give the list a `ref` and ask its element: `el:scroll_to_row(i, { align = "center" })`, `i` from 1, `align` `"start"` (its top at the top), `"center"`, `"end"` (its bottom at the bottom) or `"nearest"` (the default: the least scrolling, none if it shows). It works for every kind of list: one of measured rows measures the rows around the target as it lands, so the row ends exactly where asked however far it was.

```lua
local messages = ui.ref()
return ui.column {
	ui.button {
		on_click = function()
			local list = messages:peek()
			if list then
				list:scroll_to_row(5000, { align = "center" })
			end
		end,
		"Jump to 5000",
	},
	ui.list {
		ref = messages,
		count = 10000,
		height = 24,
		w = 300,
		h = 240,
		row = function(i)
			return ui.text { `Message {i}`, h = 24 }
		end,
	},
}
```

A row is built when it scrolls into view and dropped when it leaves; it owns what it makes, as a [component](state.md#components) does: its `ui.use_state`, effects and computed values end with it.

A list whose items are removed or inserted (a mailbox's messages archived, a feed's new posts) keys its rows, and reads each row's item by its key:

```lua
type Message = { id: number, subject: string }
-- Ids are places in the list (messages are only added at the end).
local messages = ui.state("messages", {} :: { Message })
local inbox = ui.state("inbox", {} :: { number })
return ui.list {
	count = function()
		return inbox:len()
	end,
	key = function(i)
		return inbox:at(i)
	end,
	row = function(id: number, index: State<number>)
		return ui.text {
			function()
				local m = messages:at(id)
				return if m then `{index:get()}. {m.subject}` else ""
			end,
		}
	end,
}
```

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
				messages:push { from = "Ada", text = `Message {messages:len() + 1}` }
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

- Every list row is `height` points: a taller row overlaps the next and a shorter one leaves a gap. For rows of different heights give `heights` (each row's own, still never measured), or `measured = true` for rows whose height depends on their text (`zgx lint` flags wrapped text in rows of fixed height).
- A list row is built once for its index, when it comes into view. Reading state in `row` (`items:get()[i]`) takes the value at that moment; to follow a change, read it in a function property or text inside the row (`ui.text(function() return items:get()[i].name end)`), or change `count`. Rows are keyed by index, so a row's own state stays with its position, not its data.
- In a `selectable` region, a selection runs through a list as through any text: dragging past its edge scrolls it, the selection outlives the rows scrolled away (each end keeps its place in its row), and copying takes the rows between, mounted or not (see `row_text`).
- List rows do not stretch across the list: give a row's root `w_full = true`.
- A `scroll` with no size and nothing sizing it is 320 by 240 along its scrolling axis; across it, a column or row stretches it (the default `items`), and inside one with `items = "start"` it keeps 320. Use `w_full = true`, `grow = 1` or a stretching parent.
- All of a `scroll`'s children stay mounted however far away they are; past a few thousand rows, use `list`.
- `offset` counts points from the top (the left for `scroll_x`). Setting it beyond the content clamps it, and a binding to a state that is not a number reads as 0.
- There is no scrollbar unless asked (`scrollbar = true`): a viewport scrolls by wheel, touchpad, keyboard focus and `offset`, and with the keys as a page does (Space, Shift+Space, Page Up/Down, Home/End, arrows) once it has been pressed in or is under the pointer, while nothing has focus (a focused control keeps its keys).

## Keeping a gesture

A scrolling box inside a scrolling page hands a gesture on at its edges unless it says otherwise. With `overscroll = "contain"` a transcript keeps it, momentum included:

```lua
local lines = ui.state("lines", {} :: { string })
for i = 1, 40 do
	lines:push(`Line {i}`)
end
return ui.scroll {
	w = "100%",
	h = 400,
	gap = 8,
	ui.text { "A page" },
	ui.scroll {
		overscroll = "contain",
		h = 200,
		ui.each(lines, function(line: string)
			return ui.text { line }
		end),
	},
	ui.div { h = 600 },
}
```
