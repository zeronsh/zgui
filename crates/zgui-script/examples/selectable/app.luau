-- Selectable text in Luau: one `selectable = true` and a markdown document
-- (drawn here from `ui.markdown`'s blocks) selects as a web page does: drag
-- across paragraphs, list items and code, double/triple press for a word
-- or a paragraph, Shift to extend, ⌘A / Ctrl+A, ⌘C / Ctrl+C, and the
-- context menu on selected text. `selectable = false` keeps a part out.

local source = [[
# Selectable markdown

Everything below is **one document**: drag from this paragraph into the
list, the code and the table, and copy it as plain text with *paragraphs*,
`code` and line breaks where they were.

- Double-press a word, triple-press a paragraph.
- Keep dragging after either to extend by words or paragraphs.
- Links like [this one](https://example.com) still click.

```lua
return ui.column { selectable = true, ui.text "Hello" }
```

| Gesture | Selects |
| --- | --- |
| drag | characters |
| double press | a word |
| triple press | a paragraph |
]]

local clicks = ui.state("clicks", 0)

local function runs(list: { any }, props: RichProps?): Node
	local rich: RichProps = props or { wrap = true }
	for _, r in list do
		table.insert(
			rich,
			ui.span {
				r.x,
				literal = true,
				bold = r.b or nil,
				italic = r.i or nil,
				strike = r.s or nil,
				font = if r.c then "mono" else nil,
				bg = if r.c then "#ffffff14" else nil,
				padding = if r.c then { 3, 0.5 } else nil,
				radius = if r.c then 3 else nil,
				color = if r.a then "$accent" else nil,
				on_click = if r.a then ui.add(clicks) else nil,
			}
		)
	end
	return ui.rich(rich)
end

local function block(b: MarkdownBlock): Node
	if b.t == "h" then
		return runs(b.s, { wrap = true, text_size = if b.l == 1 then 26 else 20, bold = true })
	elseif b.t == "ul" or b.t == "ol" then
		local items: { Child } = {}
		for i, item in b.items do
			table.insert(
				items,
				ui.row {
					gap = 8,
					ui.text { if b.t == "ol" then `{i}.` else "•", color = "$text.tertiary" },
					ui.column {
						grow = 1,
						gap = 4,
						(function()
							local blocks: { Child } = {}
							for _, inner in item do
								table.insert(blocks, block(inner))
							end
							return blocks
						end)(),
					},
				}
			)
		end
		return ui.column { gap = 4, items }
	elseif b.t == "code" then
		return ui.column {
			p = 12,
			bg = "$surface.sunken",
			rounded = 6,
			ui.text { b.text, literal = true, font = "mono", text_size = 13 },
		}
	elseif b.t == "table" then
		local rows: { Child } = {}
		local function line(cells: { any }, bold: boolean)
			local row: { Child } = {}
			for _, cell in cells do
				table.insert(row, ui.column { w = 160, runs(cell, { wrap = true, bold = bold or nil }) })
			end
			table.insert(rows, ui.row { gap = 12, row })
		end
		line(b.head, true)
		for _, cells in b.rows do
			line(cells, false)
		end
		return ui.column { gap = 4, rows }
	end
	return runs(b.s or {})
end

local blocks: { Child } = {}
for _, b in ui.markdown(source) do
	table.insert(blocks, block(b))
end

return ui.column {
	w = "100%",
	h = "100%",
	p = 28,
	gap = 14,
	text_size = 15,
	line_height = 22,
	selectable = true,
	blocks,
	ui.text { "Links clicked: {clicks}", color = "$text.secondary" },
	ui.text {
		"This line is kept out of selections (selectable = false).",
		color = "$text.tertiary",
		selectable = false,
	},
}
