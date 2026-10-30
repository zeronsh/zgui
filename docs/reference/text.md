# Text: text, rich, span, markdown

`ui.text` shows one string in one style; `ui.rich` shows runs of differently styled text (`ui.span`s) as one paragraph. Both follow state without calling the script when their text is a template or a state given as it is (`ui.text { count }`: a component's `ui.use_state`, which no template can name, as its value), and call it again only for a function.

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
| `"{user.name}"` | a field of a state after a dot (`{rows.1}`: a list's first item); nothing where there is none |
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
| `font_features` | `string` | none | OpenType features: `"ss01 cv02=3 liga=0"` ([properties](properties.md)) |

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
| `underline_style` | `"solid"`, `"double"`, `"dotted"`, `"dashed"`, `"wavy"` | `"solid"` | how the underline is drawn (CSS's `text-decoration-style`): `"wavy"` marks a spelling mistake or a diagnostic. The software raster draws them all solid |
| `underline_color` | `Color` | the text's | its colour, or a token (`"$danger"`) |
| `underline_thickness` | `number` | `1` | its thickness, in points. Any of these three underlines the span |
| `on_click` | `ClickHandler \| Handler \| Action` | none | a click on the run (a link, underlined), told the keys held (`{ shift, ctrl, alt, meta }`); a middle click passes as a ⌘-click (Control elsewhere), as a browser opens a background tab |
| `href` | `string` | none | where the run links (`"https://…"`): a link, underlined, with the pointing hand; a click (after `on_click`'s) opens it with the system's handler if it is a web or mail link (`http:`, `https:`, `mailto:`; other schemes don't open), and its context menu has Open Link (those only) and Copy Link |
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
ui.markdown_view(props: MarkdownViewProps) -> Node
```

`ui.markdown_view { text }` draws markdown: paragraphs and headings of styled runs (bold, italic, struck, `code`, links), lists, quotes, rules, tables, and code blocks with their language and a Copy button, in the theme's colours and sizes (`$surface.sunken`, `$border.subtle`, `$text.tertiary`, `$font.size.*`), selectable as prose is. Its text is a string, a template, a state's (`"{reply}"`) or a function. As the text changes it parses only what changed and builds only the blocks whose value changed, so a reply streaming in grows by its last block, and every block before it keeps its nodes, layout and paint. Options: `breaks` (single newlines break lines), `paths` (repository paths are files), `copy = false` (no Copy buttons); the rest are a column's styles (`gap` between blocks is 10).

```lua
local reply = ui.state("reply", "Some **bold** text, then code:\n\n```lua\nprint(1)\n```")
return ui.markdown_view { "{reply}", w = 520, text_size = 15 }
```

To draw blocks your own way, take them from `ui.markdown`:

```luau
ui.markdown(text: string, opts: MarkdownOptions?) -> { MarkdownBlock }
```

`ui.markdown` parses markdown (CommonMark with tables, strikethrough and task lists; raw HTML dropped) into blocks for a script to draw its own way. A block has `t`: `p` (`s`: runs), `h` (`l`: level, `s`), `code` (`lang`, `text`), `ul` and `ol` (`start`, `items`: each item a list of blocks), `quote` (`b`: blocks), `hr`, `table` (`head`: cells, `rows`: rows of cells, a cell being runs) and `img` (`src`, `alt`). A tight list item's text is a `p` with `tight`. Quotes and lists nest 64 levels at most (a list level counts two); deeper ones show as the deepest level's blocks. A run is `{ x = text, b?, i?, c? (code), s? (strike), a? (link), f? (a file) }`. The types say so: a `MarkdownBlock` is a union by `t`, so `if b.t == "code" then … b.lang … end` is checked, and a run is a `MarkdownRun`.

| Option | Type | Default | |
|---|---|---|---|
| `key` | `string` | none | names a text that grows (a reply streaming in): each call parses only its new end, and its earlier blocks are the very tables of the last call, so a `ui.each` over them builds only the blocks that changed. A marker left open at the end (`**bold`, a link's address half there) shows closed until the closer arrives, so the text does not restyle and reflow when it does; such a link's `a` is `"zgx:pending-link"` (style it, but it goes nowhere yet) |
| `breaks` | `boolean` | `false` | single newlines break lines, as people's messages show them |
| `paths` | `boolean` | `false` | repository paths (`src/main.rs:12`), code spans that are a path whole and relative links that read as one are files: runs with `f` |

Without a `key`, the same text gives the same blocks. Rust hosts parse with `zgx_script::markdown::parse` and stream with `zgx_script::markdown::Incremental`, which take a code block hook too (coloured lines, say).

```lua
local reply = ui.state("reply", "A **streaming** reply.\n\n- one\n- two")

local function runs(list: { MarkdownRun }): Node
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
				local first = item[1]
				if first and first.t == "p" then
					table.insert(items, runs(first.s))
				end
			end
			return ui.column { gap = 4, items }
		elseif b.t == "p" or b.t == "h" then
			return runs(b.s)
		end
		return ui.div {}
	end),
}
```

## Selectable text

Text is not selectable unless asked, as in native apps. `selectable = true` on any node makes the text in it and everything it holds selectable as a web page's is: one selection runs across its texts, paragraphs, list rows and spans in document order. Drag to select; a double press selects a word and a triple press a paragraph, and dragging after either extends by words or paragraphs; Shift extends; ⌘A / Ctrl+A selects it all and ⌘C / Ctrl+C copies plain text (a tab between texts side by side, a blank line between blocks set apart). A secondary press on selected text offers Copy and Select All. Links and buttons inside still click; a drag that starts on one selects. Holding a drag past a scroll viewport's edge scrolls it, and a selection in `ui.list`'s rows outlives the rows it recycles. `selectable = false` keeps a part out (a toolbar, a timestamp). `ui.markdown_view` is selectable already; drawing `ui.markdown`'s blocks yourself, put `selectable = true` on the node you draw them in, as above. [crates/zgx-script/examples/selectable](../../crates/zgx-script/examples/selectable/app.luau) draws a whole document (`cargo run -p zgx-script --example selectable`). The highlight follows CSS's `::selection`. `selection_color` and `selected_text_color` on the selectable node set its background and the selected glyphs' colour (a colour, a `"$token"`, a state or a function). The tokens `$selection`, `$selection.inactive` (window at the back), `$selection.text` and `$selection.text.inactive` (transparent, the default, keeps the glyphs' colours) and `$selection.radius` style it per subtree through `theme`: `ui.column { selectable = true, theme = { ["selection.text"] = "#ffffff", ["selection.radius"] = 3 }, … }`. Prose the examples ship (the agent trace, the streaming transcript, the notes preview) is selectable, as a browser's would be; controls and chrome are not.

On macOS a selection, or an input's, also answers the system's text services: the context menu offers Look Up “…” (⌃⌘D looks up without it), the Services menu takes it as plain text, and a service returning text replaces an editable selection. Selected text drags into inputs, where it moves (Option copies), and into other apps as plain text; a password's never leaves its field ([Rust: dragging text](../drag-drop.md#dragging-text)).

## Localized messages

`ui.i18n { dir = "locales", source = "en" }` loads the script's locale files: `locales/en.json`, `locales/ar.json`, …, each named by its locale (`pt-BR.json` is `pt-BR`). Nested keys are flattened to dotted names, and messages are ICU MessageFormat: `{name}`, `{n, plural, one {# message} other {# messages}}` (`#` and `=0`), `{n, selectordinal, …}`, `{x, select, …}` and `{n, number}`. The source locale (`en` unless `source` says otherwise) has every key.

```json file=locales/en.json
{
  "inbox": { "count": "{n, plural, one {# message} other {# messages}}" },
  "search": "Search",
  "actions": { "save": "Save" }
}
```

```lua
-- file: app.luau
ui.i18n { dir = "locales" }
local count = ui.state("count", 3)
local query = ui.state("query", "")
return ui.column {
    ui.text { ui.t("inbox.count", { n = count }) }, -- "3 messages"
    ui.input { value = query, placeholder = ui.t("search") },
    ui.button { ui.t("actions.save"), on_click = function() count:add(1) end },
}
```

- Call `ui.i18n` before `ui.t`: a message needs its catalog (`ui.t` before it is an error saying so).
- `ui.t(key, args)` is a message: accepted wherever text is (a child, `ui.text`, `placeholder`), it is formatted in Rust when its locale, its file or a state its arguments read changes, never per frame, and only the texts showing a changed message update. Arguments are numbers, strings, states or functions (called, tracked, when it formats). `msg:get()` gives its text now, in a handler or a computed (which follows it); `msg:key()` its key.
- `ui.locale:set("ar")` asks for a locale; `ui.locale:set(nil)` follows the system's preferred languages again. `ui.locale.current` is the one shown (tracked): the one asked for, or preferred, as the app has it, through its parents (`pt-BR`, then `pt`) and then the source.
- A key the locale lacks falls back through its parents to the source locale, then shows the key itself; an argument the message names but wasn't given shows as `{name}`. Each is told once, to stderr and `ScriptHost::warnings()`, as are messages that don't parse (with their file, line and column: the rest of the file loads).
- The script runs the way its locale does: an Arabic or Hebrew locale lays the script's root out right to left, unless the root says which way (`dir`).
- Edited locale files show at once, as theme files do, without running the script again; `zgx bundle` ships them.
- A widget's `label` (a tab strip's, a select's, a modal's, a menu item's) takes a message or a state too, and follows it: the name screen readers give it, and a menu item's drawn text. A plain string costs nothing more.
- Items given as data (tab titles, tree rows) follow the locale through a computed list that reads its messages: `items = ui.computed(function() return { { id = "inbox", label = ui.t("tabs.inbox"):get() } } end)`. A select's `options` follow the same way, given as a computed (a table is read once, as it is built).
- Plural rules and number formatting for every locale come from ICU4X with zgx-script's `i18n` feature (about 90 KB; `zgx run` has it). A host built without it still loads and shows messages, with English's plural rules (`one`, `other`) for every locale and numbers as Rust writes them: turn it on (`zgx-script = { …, features = ["i18n"] }`) to ship translations.
- `zgx lint` checks each locale against the source (`i18n-missing`, `i18n-extra`, `i18n-args`, `i18n-parse`, and `i18n-key` for `ui.t`'s keys) and each plural for the cases its locale needs (`i18n-plural`), and a debug build shows the findings in the error layer as the script loads. With the `i18n` feature (`zgx run` has it) those cases are CLDR's: French needs `many` too (a million: `"1 000 000 de messages"`), Russian `few` and `many`, Arabic `zero`, `two`, `few` and `many`. A missing one shows `other`'s text and is reported. Without the feature, every locale's are English's two.

## Gotchas

- Text from data is a template too: a commit message with `{id}` in it looks for state named `id` (an error if there is none), and `{{` shows as `{`. Pass `literal = true` (on `ui.text`, `ui.rich` or a span) for text the script didn't write.
- A template names state by its key, not by its Luau variable: `"{count}"` shows `ui.state("count", …)` whatever the local is called (a component's `ui.use_state` has no key: give `ui.text` the state itself, `ui.text { count }`), and `"{state.count}"` is state `state`'s field `count`, not a module's state. A name no state has is an error when the text is built.
- Text does not wrap by default, and a row shrinks it no narrower than it is (its min-content, unwrapped): `wrap = true` (in a box with a width) lets it wrap down to its longest word, or `min_w = 0, truncate = true` ends it with "…" in a row.
- Span options are plain values, read once: a state or function there is not followed (its value at build time is used). Only the run's text follows state. To restyle a run, build the paragraph in a function, or use two texts.
- A span's `font` only knows `"mono"`; any other family is ignored (`ui.text`'s `font` takes names).
- A span is used once: putting the same `ui.span` in two paragraphs is an error. Make a new one each time.
- `ui.text` and `ui.rich` take no children; extra array entries in `ui.text` are an error.
- A function's text is called again for each change it read; prefer a template for plain state, which costs no script call.
