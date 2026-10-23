# Text: text, rich, span, markdown

`ui.text` shows one string in one style; `ui.rich` shows runs of differently styled text (`ui.span`s) as one paragraph. Both follow state without calling the script when their text is a template, and call it again only for a function.

```luau
ui.text(content: Text | TextProps) -> Node
ui.rich(props: RichProps) -> Node
ui.span(content: Text | SpanProps) -> Span

type Text = string | number | (() -> TextValue)
type TextValue = (string | number | boolean)?
```

`ui.text "Hello"`, `ui.text(function() … end)` and `ui.text { "Hello", text_size = 18 }` are the same element: the table form takes the text first and [properties](properties.md) after it. A string child of a container (`ui.column { "Hello" }`) is a `ui.text` too.

## Text

| Form | Shows |
| --- | --- |
| `"Count: {count}"` | the string, with `{name}` replaced by the state named `name`, followed in Rust (no script call per change) |
| `"{{" "}}"` | literal braces; braces around anything but a name (`{not a name}`) are literal too |
| a number | it, as written (`3`, `2.5`) |
| a function | what it returns, run again when the state it reads changes: strings as they are, numbers and booleans as text, `nil` as nothing |

A template's `{name}` looks for, in order: the calling module's own `ui.state` or `ui.persist` of that key, the script's, then `ui.shared`. A list shows its items joined by ", ".

## Text options

Any [property](properties.md); these are the text ones (they also set the default for text inside a container that has them):

| Option | Type | Default | |
| --- | --- | --- | --- |
| `[1]` | `Text` | `""` | the text |
| `literal` | `boolean` | `false` | show a string as written, braces and all: for data (code, titles, messages) that may hold `{name}` |
| `text_size` | `number` | `16` | points |
| `color` | `Color` | `"#e5edf7"` | |
| `weight` | `number` | `400` | 100 to 900 |
| `bold` | `boolean` | `false` | weight 700 |
| `italic` | `boolean` | `false` | |
| `font` | `string` | the host's | `"mono"` (or `"monospace"`), `"sans"`, `"serif"`, or a family name |
| `align` | `"start" \| "left" \| "center" \| "right"` | `"start"` | within the text's own box |
| `wrap` | `boolean` | `false` | wrap at the box's width |
| `lines` | `number` | none | at most this many lines |
| `truncate` | `boolean` | `false` | end an overflowing line with "…" |
| `line_height` | `number` | the font's | points |
| `letter_spacing` | `number` | `0` | points |
| `tabular` | `boolean` | `false` | tabular figures (OpenType `tnum`): every digit one width, for numbers that change in place |

## Rich text and spans

`ui.rich` takes its runs as array entries, and any [property](properties.md) for the paragraph (`wrap`, `text_size`, `color`… are the runs' defaults):

| Entry | |
| --- | --- |
| `ui.span { "text", … }` | a styled run |
| a string or function | a run in the paragraph's style (templates work) |
| a table, `{ "text", bold = true }` | a span's options in place: the same as `ui.span { … }` |

| Span option | Type | Default | |
| --- | --- | --- | --- |
| `[1]` | `Text` | `""` | the run's text: a template or a function follows state |
| `color` | `Color` | the paragraph's | |
| `bg` | `Color` | none | behind the run (a highlight) |
| `text_size` | `number` | the paragraph's | |
| `weight` | `number` | the paragraph's | `bold = true` is 700 |
| `bold`, `italic`, `underline`, `strike` | `boolean` | `false` | |
| `font` | `"mono"` | the paragraph's | |
| `padding` | `number` or `{ across, down }` | `0` | room around the run: across, before and after it (it takes space in the line); with `bg`, the background hugs the text plus this instead of filling the line, an inline code chip |
| `radius` | `number` | `0` | the background's corner radius |
| `underline_offset` | `number` | a tenth of the text size | how far below the baseline the underline sits (CSS's `text-underline-offset`) |
| `on_click` | `Handler \| Action` | none | a click on the run (a link, underlined) |
| `hover` | `{ underline: boolean?, color: Color? }` | none | a link's look under the pointer (CSS's `a:hover`); with it, the link isn't underlined at rest unless `underline` says so |
| `icon` | `{ src?, code?, size?, gap?, tint? }` | none | an icon before the text, in the line (a file chip's file type): `src` (a file, `https://…`) or `code` (SVG markup), `size` square (12), `gap` from the text (4), centred on the line; `tint` a colour, or `"currentColor"` for the span's |

## Example

```lua
local count = ui.state("count", 0)
local name = ui.state("name", "Ada")
return ui.column {
	p = 24,
	gap = 10,
	w = 360,
	ui.text { "Hello, {name}", text_size = 22, bold = true },
	ui.text {
		function()
			return if count:get() == 1 then "1 message" else `{count:get()} messages`
		end,
		color = "#8b95a5",
	},
	ui.text {
		"A long line wraps at the column's width instead of running out of it.",
		wrap = true,
	},
	ui.rich {
		wrap = true,
		"Press ",
		ui.span { "Add", bold = true },
		" or edit ",
		ui.span { "app.luau", font = "mono", bg = "#ffffff14", padding = { 4, 0.5 }, radius = 3 },
		" ",
		ui.span { "({count} so far)", color = "#5ea5ff", underline = true, on_click = ui.add(count) },
	},
}
```

## Markdown

```luau
ui.markdown(text: string, opts: MarkdownOptions?) -> { MarkdownBlock }
```

`ui.markdown` parses markdown (CommonMark with tables, strikethrough and task lists; raw HTML dropped) into blocks for a script to draw its own way. A block has `t`: `p` (`s`: runs), `h` (`l`: level, `s`), `code` (`lang`, `text`), `ul` and `ol` (`start`, `items`: each item a list of blocks), `quote` (`b`: blocks), `hr`, `table` (`head`: cells, `rows`: rows of cells, a cell being runs) and `img` (`src`, `alt`). A tight list item's text is a `p` with `tight`. A run is `{ x = text, b?, i?, c? (code), s? (strike), a? (link), f? (a file) }`.

| Option | Type | Default | |
|---|---|---|---|
| `key` | `string` | none | names a text that grows (a reply streaming in): each call parses only its new end, and its earlier blocks are the very tables of the last call, so a `ui.each` over them builds only the blocks that changed. A marker left open at the end (`**bold`, a link's address half there) shows closed until the closer arrives, so the text does not restyle and reflow when it does; such a link's `a` is `"zgui:pending-link"` (style it, but it goes nowhere yet) |
| `breaks` | `boolean` | `false` | single newlines break lines, as people's messages show them |
| `paths` | `boolean` | `false` | repository paths (`src/main.rs:12`), code spans that are a path whole and relative links that read as one are files: runs with `f` |

Without a `key`, the same text gives the same blocks. Rust hosts parse with `zgui_script::markdown::parse` and stream with `zgui_script::markdown::Incremental`, which take a code block hook too (coloured lines, say).

```lua
local reply = ui.state("reply", "A **streaming** reply.\n\n- one\n- two")

local function runs(list: { any }): Node
	local props: RichProps = { wrap = true }
	for _, r in list do
		table.insert(props, ui.span { r.x, literal = true, weight = if r.b then 600 else nil })
	end
	return ui.rich(props)
end

local blocks = ui.computed(function()
	return ui.markdown(reply:get(), { key = "reply" })
end)

return ui.column {
	gap = 8,
	selectable = true,
	ui.each(blocks, function(b: MarkdownBlock)
		if b.t == "ul" then
			local items: { Child } = {}
			for _, item in b.items do
				table.insert(items, runs(item[1].s))
			end
			return ui.column { gap = 4, items }
		end
		return runs(b.s or {})
	end),
}
```

## Selectable text

Text is not selectable unless asked, as in native apps. `selectable = true` on any node makes the text in it and everything it holds selectable as a web page's is: one selection runs across its texts, paragraphs, list rows and spans in document order. Drag to select; a double press selects a word and a triple press a paragraph, and dragging after either extends by words or paragraphs; Shift extends; ⌘A / Ctrl+A selects it all and ⌘C / Ctrl+C copies plain text (a tab between texts side by side, a blank line between blocks set apart). A secondary press on selected text offers Copy and Select All. Links and buttons inside still click; a drag that starts on one selects. Holding a drag past a scroll viewport's edge scrolls it, and a selection in `ui.list`'s rows outlives the rows it recycles. `selectable = false` keeps a part out (a toolbar, a timestamp). Markdown has no built-in view, so put `selectable = true` on the node you draw its blocks in, as above. [crates/zgui-script/examples/selectable](../../crates/zgui-script/examples/selectable/app.luau) draws a whole document (`cargo run -p zgui-script --example selectable`).

## Gotchas

- Text from data is a template too: a commit message with `{id}` in it looks for state named `id` (an error if there is none), and `{{` shows as `{`. Pass `literal = true` (on `ui.text`, `ui.rich` or a span) for text the script didn't write.
- A template names state by its key, not by its Luau variable: `"{count}"` shows `ui.state("count", …)` whatever the local is called. A name no state has is an error when the text is built.
- Text does not wrap by default, and a row does not shrink it: `wrap = true` (in a box with a width), or `shrink = 1, min_w = 0, truncate = true` in a row.
- Span options are plain values, read once: a state or function there is not followed (its value at build time is used). Only the run's text follows state. To restyle a run, build the paragraph in a function, or use two texts.
- A span's `font` only knows `"mono"`; any other family is ignored (`ui.text`'s `font` takes names).
- A span is used once: putting the same `ui.span` in two paragraphs is an error. Make a new one each time.
- `ui.text` and `ui.rich` take no children; extra array entries in `ui.text` are an error.
- A function's text is called again for each change it read; prefer a template for plain state, which costs no script call.
