--!strict
-- Notes: the same app as NSH's `examples/notes`, in Luau on zgx. zgx draws
-- all of it on the GPU, over the window's frosted glass; the menu bar is
-- AppKit's (notes.rs builds it and sends its commands here). The preview
-- parses Markdown in Luau and lays it out as rich text.

type Note = { id: string, title: string, body: string, pinned: boolean, updated: number }
type Block = { kind: string, text: string, done: boolean?, tone: string? }

-- Saved in the app's data directory, and loaded again on the next run.
-- Nil until the first run seeds it.
local notes = ui.persist("notes", nil :: { Note }?)
-- Menu bar commands from Rust (notes.rs): `{ name, seq }`.
local command = ui.shared("command", nil :: { name: string, seq: number }?)
local preview = ui.shared("preview", true)

local selected = ui.state("selected", "welcome" :: string?)
local query = ui.state("query", "")
local title = ui.state("title", "")
local body = ui.state("body", "")
local confirm = ui.state("confirm", false)
local more = ui.state("more", false)

-- Colours are theme tokens, so any theme styles the notes: the standard
-- ones, and the notes' own below, whose defaults are written in terms of the
-- standard ones. glass.json is their own look (dark glass, as in the web
-- view example); `ui.theme.set("zgx-light")` restyles everything live.
ui.theme.tokens {
	notes = {
		-- The search field and code spans.
		wash = "{hover}",
		-- The dialog's secondary button.
		button = "{button.bg}",
		code = "{warning}",
		-- A note's row under the pointer, and the selected note's pill.
		row = { hover = "alpha({hover}, 0.5)" },
		selected = {
			bg = "alpha({accent}, 0.24)",
			border = "alpha({accent}, 0.25)",
			shadow = "0 4 14 0 alpha({accent}, 0.12)",
		},
		alert = {
			bg = "alpha({success}, 0.08)",
			border = "alpha({success}, 0.35)",
			text = "{success}",
		},
		editor = "{surface.base}",
		preview = "{surface.base}",
		header = "alpha({surface.base}, 0.6)",
	},
}
ui.theme.default(ui.theme.load("glass.json"))
local text = "$text.primary"
local secondary = "$text.secondary"
local tertiary = "$text.tertiary"
local line = "$border.subtle"
local wash = "$notes.wash"
local accent = "$accent"
local clear = "#00000000"

-- The platform's command key, and where its menus are.
local mac = ui.shared("platform", ""):peek() == "macos"
local cmd = if mac then "Cmd" else "Ctrl"

if notes:peek() == nil then
	notes:set {
		{
			id = "welcome",
			title = "Welcome to Notes",
			pinned = true,
			updated = os.time(),
			body = [[This window is zgx, driven by a Luau script: every pixel is drawn by **zgx** on the GPU.

## Try it

- [ ] Edit this note and watch the preview re-parse it in Luau
- [ ] Use the `⋯` menu to pin, duplicate or delete a note
- [ ] Press ]]
				.. cmd
				.. "+N for a new note"
				.. (if mac then ": the menu bar is AppKit's" else "")
				.. "\n"
				.. [[- [ ] Toggle the preview with ]]
				.. cmd
				.. [[+Shift+P and scroll it under the blurred header
- [ ] Save `app.luau` while it runs: the interface reloads and keeps your notes

> [!TIP]
> Drag the window over something colourful: the sidebar is the window's own glass.]],
		},
		{
			id = "groceries",
			title = "Groceries",
			pinned = false,
			updated = os.time() - 86400,
			body = "- Oat milk\n- Coffee beans\n- Lemons\n- Sourdough",
		},
	}
end

local function all(): { Note }
	return notes:get() or {}
end
local function find_in(list: { Note }?, id: string?): Note?
	for _, n in list or {} do
		if n.id == id then
			return n
		end
	end
	return nil
end
local function find(id: string?): Note?
	return find_in(notes:get(), id)
end
local function peek(id: string?): Note?
	return find_in(notes:peek(), id)
end

-- `change` edits a mutable copy of the note.
local function update(id: string, change: (note: Note) -> ())
	notes:update(function(list)
		for _, n in list or {} do
			if n.id == id then
				change(n)
				n.updated = os.time()
			end
		end
		return list
	end)
end

local made = 0
local function add(note: Note)
	ui.batch(function()
		notes:insert(1, note)
		query:set("")
		selected:set(note.id)
	end)
end
local function fresh(): string
	made += 1
	return `n{os.time()}-{made}`
end
local function new_note()
	add { id = fresh(), title = "", body = "", pinned = false, updated = os.time() }
end
local function duplicate()
	local n = peek(selected:peek())
	if n then
		local name = if n.title == "" then "Untitled" else n.title
		add { id = fresh(), title = `{name} copy`, body = n.body, pinned = false, updated = os.time() }
	end
end
local function toggle_pin()
	local n = peek(selected:peek())
	if n then
		update(n.id, function(note)
			note.pinned = not note.pinned
		end)
	end
end

-- Pinned first, then the newest.
local visible = ui.computed(function()
	local q = string.lower(query:get())
	local out: { Note } = {}
	for _, n in all() do
		if
			q == ""
			or string.find(string.lower(n.title), q, 1, true)
			or string.find(string.lower(n.body), q, 1, true)
		then
			table.insert(out, n)
		end
	end
	table.sort(out, function(a: Note, b: Note)
		if a.pinned ~= b.pinned then
			return a.pinned
		end
		if a.updated ~= b.updated then
			return a.updated > b.updated
		end
		return a.id > b.id
	end)
	return out
end)

local function delete_selected()
	local id = selected:peek()
	local list: { Note } = {}
	for _, n in notes:peek() or {} do
		if n.id ~= id then
			table.insert(list, n)
		end
	end
	ui.batch(function()
		notes:set(list)
		confirm:set(false)
		local first = visible:peek()[1]
		selected:set(if first then first.id else nil)
	end)
end

-- The editor edits copies of the selected note's fields: load them when the
-- selection changes, write them back when they change.
ui.effect(function()
	local n = peek(selected:get())
	ui.batch(function()
		title:set(if n then n.title else "")
		body:set(if n then n.body else "")
	end)
end)
ui.effect(function()
	local t, b = title:get(), body:get()
	local n = peek(selected:peek())
	if n and (n.title ~= t or n.body ~= b) then
		update(n.id, function(note)
			note.title, note.body = t, b
		end)
	end
end)

-- Menu bar commands from Rust, each once (not again on reload).
local last = command:peek()
local seen = if last then last.seq else nil
ui.effect(function()
	local c = command:get()
	if not c or c.seq == seen then
		return
	end
	seen = c.seq
	if c.name == "new" then
		new_note()
	elseif c.name == "duplicate" then
		duplicate()
	elseif c.name == "pin" then
		toggle_pin()
	elseif c.name == "delete" and selected:peek() then
		confirm:set(true)
	end
end)

local function stamp(t: number): string
	local time = (string.gsub(os.date("%I:%M %p", t), "^0", ""))
	if os.date("%x", t) == os.date("%x") then
		return time
	end
	return `{os.date("%b %d", t)}  {time}`
end

local function snippet(src: string): string
	for raw in string.gmatch(src, "[^\n]+") do
		local l = string.match(string.gsub(raw, "^[#>%-%*%s%[%]xX!]+", ""), "^%s*(.-)%s*$")
		if l and l ~= "" then
			return l
		end
	end
	return "No additional text"
end

-- Markdown, the parts notes use: headings, task lists, bullets, quotes,
-- GitHub alerts, paragraphs; **bold** and `code` inline.
local function parse(src: string): { Block }
	local out: { Block } = {}
	local para: { string } = {}
	local function flush()
		if #para > 0 then
			table.insert(out, { kind = "p", text = table.concat(para, " ") })
			table.clear(para)
		end
	end
	for l in string.gmatch(src .. "\n", "(.-)\n") do
		local hashes, heading = string.match(l, "^(#+)%s+(.*)")
		local box, task = string.match(l, "^%s*[-*]%s+%[([ xX])%]%s+(.*)")
		local bullet = string.match(l, "^%s*[-*]%s+(.*)")
		local quote = string.match(l, "^>%s?(.*)")
		-- A pattern's captures match together.
		if hashes and heading then
			flush()
			table.insert(out, { kind = `h{math.min(#hashes, 3)}`, text = heading })
		elseif box and task then
			flush()
			table.insert(out, { kind = "task", text = task, done = box ~= " " })
		elseif bullet then
			flush()
			table.insert(out, { kind = "bullet", text = bullet })
		elseif quote then
			flush()
			local last = out[#out]
			local tone = string.match(quote, "^%[!(%u+)%]%s*$")
			if tone then
				table.insert(out, { kind = "alert", tone = tone, text = "" })
			elseif last and (last.kind == "quote" or last.kind == "alert") then
				last.text = if last.text == "" then quote else `{last.text} {quote}`
			else
				table.insert(out, { kind = "quote", text = quote })
			end
		elseif string.match(l, "^%s*$") then
			flush()
		else
			table.insert(para, l)
		end
	end
	flush()
	return out
end

-- A paragraph of rich text: `**bold**` and `code` spans. `fill` lets it take
-- the rest of a row.
local function inline(src: string, color: string?, fill: boolean?): Node
	local size = 15
	local node: RichProps = { text_size = size, line_height = 24, color = color or text, wrap = true }
	if fill then
		node.grow, node.shrink, node.min_w = 1, 1, 0
	end
	color = color or text
	local rest = src
	while #rest > 0 do
		local b = string.find(rest, "**", 1, true)
		local c = string.find(rest, "`", 1, true)
		local at = math.min(b or math.huge, c or math.huge)
		if at == math.huge then
			table.insert(node, ui.span { rest, color = color, text_size = size })
			break
		end
		if at > 1 then
			table.insert(node, ui.span { string.sub(rest, 1, at - 1), color = color, text_size = size })
		end
		local mark = if at == b then "**" else "`"
		local close = string.find(rest, mark, at + #mark, true)
		if not close then
			table.insert(node, ui.span { string.sub(rest, at), color = color, text_size = size })
			break
		end
		local inner = string.sub(rest, at + #mark, close - 1)
		if mark == "**" then
			table.insert(node, ui.span { inner, bold = true, color = color, text_size = size })
		else
			table.insert(node, ui.span { inner, font = "mono", text_size = 13, bg = wash, color = "$notes.code" })
		end
		rest = string.sub(rest, close + #mark)
	end
	return ui.rich(node)
end

local function block(b: Block): Node
	if b.kind == "h1" then
		return ui.text { b.text, text_size = 26, weight = 700, color = text, mt = 4 }
	elseif b.kind == "h2" then
		return ui.text { b.text, text_size = 20, weight = 650, color = text, mt = 12 }
	elseif b.kind == "h3" then
		return ui.text { b.text, text_size = 17, weight = 650, color = text, mt = 8 }
	elseif b.kind == "task" or b.kind == "bullet" then
		local mark = if b.kind == "bullet"
			then ui.text { "•", w = 16, text_size = 15, line_height = 24, color = secondary }
			else ui.div {
				w = 16,
				h = 16,
				mt = 4,
				shrink = 0,
				rounded = 4,
				border = 1.5,
				border_color = if b.done then accent else tertiary,
				bg = if b.done then accent else clear,
			}
		return ui.row {
			gap = 10,
			items = "start",
			mark,
			inline(b.text, if b.done then tertiary else text, true),
		}
	elseif b.kind == "quote" then
		return ui.row {
			gap = 12,
			ui.div { w = 3, rounded = 2, bg = line, shrink = 0 },
			inline(b.text, secondary, true),
		}
	elseif b.kind == "alert" then
		return ui.column {
			p = 14,
			gap = 4,
			rounded = 10,
			bg = "$notes.alert.bg",
			border = 1,
			border_color = "$notes.alert.border",
			ui.text { b.tone or "NOTE", text_size = 12, weight = 700, color = "$notes.alert.text" },
			inline(b.text),
		}
	end
	return inline(b.text)
end
local blocks = ui.computed(function()
	return parse(body:get())
end)

-- Pieces.
local function drag(props: ContainerProps?)
	local spacer: ContainerProps = if props then table.clone(props) else {}
	spacer.h_full = true
	spacer.window_drag = true
	return ui.div(spacer)
end

local function tool(glyph: string, on_click: Handler | Action, on: (() -> boolean)?)
	return ui.button {
		h = 28,
		px = 9,
		shrink = 0,
		rounded = 7,
		items = "center",
		justify = "center",
		bg = function()
			return if on and on() then "$pressed" else clear
		end,
		hover = { bg = "$hover" },
		focus = { border = 0 },
		on_click = on_click,
		ui.text { glyph, text_size = if #glyph > 3 then 12 else 17, color = secondary },
	}
end

local function item(label: string, on_click: () -> ())
	return ui.menu_item {
		label,
		h = 30,
		px = 10,
		rounded = 7,
		text_size = 13,
		color = text,
		bg = clear,
		hover = { bg = "$hover" },
		focus = { bg = "$hover", border = 0 },
		on_click = function()
			more:set(false)
			on_click()
		end,
	}
end

-- A clear input; its placeholder shows while it's empty.
type FieldStyle = {
	h: number?,
	h_full: boolean?,
	wrap: boolean?,
	text_size: number?,
	weight: number?,
	line_height: number?,
	color: string?,
}
local function field(state: State<string>, hint: string, style: FieldStyle, multiline: boolean?)
	local input: InputProps = {
		label = hint,
		value = state,
		placeholder = hint,
		placeholder_color = tertiary,
		w_full = true,
		bg = clear,
		border = 0,
		focus = { border = 0 },
	}
	input.h, input.text_size, input.weight = style.h, style.text_size, style.weight
	input.line_height, input.color = style.line_height, style.color
	input.h_full, input.wrap = style.h_full, style.wrap
	return ui.overlay {
		w_full = true,
		grow = if multiline then 1 else 0,
		if multiline then ui.textarea(input) else ui.input(input),
	}
end

-- Sidebar motion.
local SPRING = anime.spring { bounce = 0.32, duration = 520 }
local SOFT = anime.spring { bounce = 0.12, duration = 420 }

local function index_of(id: string?): number?
	for i, n in visible:peek() do
		if n.id == id then
			return i
		end
	end
	return nil
end

-- The selection is one highlight that springs from row to row (a
-- `layout_id` hands it from the row it leaves to the one it lands in),
-- stretching a little as it travels, rather than a background on each row.
local pill_last: string? = nil
local function pill(id: string)
	return ui.div {
		layout_id = "selection",
		layout_ease = SPRING,
		layout_duration = 520,
		h_full = true,
		rounded = 9,
		bg = "$notes.selected.bg",
		border = 1,
		border_color = "$notes.selected.border",
		shadow = "$notes.selected.shadow",
		on_mount = function(el)
			local from = if pill_last ~= id then index_of(pill_last) else nil
			local to = index_of(id)
			pill_last = id
			if from == nil or to == nil then
				-- Appearing: in place, popping up from slightly small.
				anime.animate(el, { opacity = { 0, 1 }, scale = { 0.92, 1 }, duration = 360, ease = SOFT })
				return
			end
			local far = math.min(math.abs(to - from), 4)
			-- Squash across, stretch along the way it moves, more for longer trips.
			anime.animate(el, {
				scale_y = { 1, 1 + 0.06 * far, 1 },
				scale_x = { 1, 1 - 0.025 * far, 1 },
				duration = 520,
				ease = SPRING,
			})
		end,
	}
end

local function row(n: Note, first_index: number)
	local id = n.id
	local el = ui.ref()
	local dot = ui.ref()

	local content = ui.column {
		ref = el,
		on_click = function()
			selected:set(id)
			-- A small press, released with a spring.
			anime.animate(el, { scale = { 0.97, 1 }, duration = 420, ease = SPRING })
		end,
		on_hover = function(inside)
			anime.animate(el, { x = if inside then 3 else 0, duration = 260, ease = "outCubic" })
		end,
		-- In one after another, by position: on load and as a search refills
		-- the list. A new note, at the top, comes in at once.
		on_mount = function()
			anime.animate(el, {
				opacity = { 0, 1 },
				x = { -14, 0 },
				scale = { 0.96, 1 },
				delay = math.min(first_index - 1, 12) * 45,
				duration = 620,
				ease = SOFT,
			})
		end,
		w_full = true,
		min_h = 52,
		px = 10,
		gap = 2,
		justify = "center",
		rounded = 8,
		cursor = "pointer",
		hover = { bg = "$notes.row.hover" },
		ui.row {
			items = "center",
			-- The dot's own margin, not a gap: hidden, it takes no room.
			ui.show(
				function()
					local m = find(id)
					return m ~= nil and m.pinned
				end,
				ui.text {
					"●",
					ref = dot,
					mr = 6,
					text_size = 9,
					color = accent,
					on_mount = function()
						anime.animate(dot, { scale = { 0, 1.5, 1 }, duration = 520, ease = SPRING })
					end,
				}
			),
			ui.text {
				function()
					local m = find(id)
					return if m and m.title ~= "" then m.title else "Untitled"
				end,
				text_size = 13,
				weight = 600,
				color = text,
				truncate = true,
				grow = 1,
				shrink = 1,
				min_w = 0,
			},
		},
		ui.text {
			function()
				local m = find(id)
				return if m then `{stamp(m.updated)}  {snippet(m.body)}` else ""
			end,
			text_size = 12,
			color = secondary,
			truncate = true,
		},
	}
	-- The highlight sits under the row, not in it: hovering and pressing
	-- move the row, not the highlight.
	return ui.overlay {
		w_full = true,
		ui.presence(function()
			return if selected:get() == id then "selected" else nil
		end, function()
			return pill(id)
		end, { absolute = true, top = 0, left = 0, right = 0, bottom = 0 }),
		content,
	}
end

local sidebar = ui.column {
	w = 260,
	shrink = 0,
	items = "stretch",
	border_sides = { 0, 1, 0, 0 },
	border_color = line,
	ui.row {
		h = 52,
		items = "center",
		pr = 10,
		drag { grow = 1 },
		(function()
			-- Turns a quarter with a spring each time it makes a note.
			local plus = ui.ref()
			local turns = 0
			return ui.button {
				w = 30,
				h = 28,
				rounded = 7,
				items = "center",
				justify = "center",
				bg = clear,
				hover = { bg = "$hover" },
				focus = { border = 0 },
				on_click = function()
					turns += 1
					anime.animate(plus, { rotate = turns * 90, scale = { 0.8, 1 }, duration = 600, ease = SPRING })
					new_note()
				end,
				ui.text { "＋", ref = plus, text_size = 17, color = secondary },
			}
		end)(),
	},
	ui.row {
		mx = 10,
		mb = 8,
		h = 28,
		px = 9,
		items = "center",
		rounded = 7,
		bg = wash,
		field(query, "Search", { h = 28, text_size = 13, color = text }),
	},
	ui.scroll {
		grow = 1,
		ui.column {
			px = 8,
			pb = 12,
			gap = 2,
			items = "stretch",
			-- Reorders (pinning, an edited note rising to the top, a new note
			-- pushing the rest down) play as rows sliding from where they were.
			ui.each(visible, row, {
				key = function(n)
					return n.id
				end,
				animate = { ease = SPRING, duration = 560 },
			}),
		},
	},
	ui.text {
		function()
			local count = #all()
			return `{count} {if count == 1 then "note" else "notes"}`
		end,
		px = 18,
		py = 10,
		text_size = 11,
		color = tertiary,
	},
}

local toolbar = ui.row {
	h = 52,
	items = "center",
	gap = 4,
	px = 12,
	drag {
		grow = 1,
		items = "center",
		justify = "center",
		ui.text {
			function()
				local n = find(selected:get())
				return if n then `Edited {stamp(n.updated)}` else ""
			end,
			text_size = 12,
			color = tertiary,
		},
	},
	ui.menu {
		open = more,
		anchor = tool("⋯", ui.toggle(more)),
		w = 200,
		p = 6,
		gap = 2,
		rounded = 12,
		bg = "$menu.bg",
		backdrop_blur = 24,
		border = 1,
		border_color = "$border",
		shadow = "$shadow.lg",
		items = "stretch",
		item("Pin / Unpin", toggle_pin),
		item("Duplicate", duplicate),
		item("Delete…", function()
			confirm:set(true)
		end),
	},
	tool("Preview", ui.toggle(preview), function()
		return preview:get() == true
	end),
}

local editor = ui.column {
	grow = 1,
	shrink = 1,
	basis = 0,
	min_w = 0,
	items = "stretch",
	bg = "$notes.editor",
	toolbar,
	ui.column {
		grow = 1,
		px = 32,
		pb = 12,
		gap = 6,
		items = "stretch",
		field(title, "Title", { text_size = 26, weight = 700, color = text, h = 40 }),
		field(
			body,
			"Start writing…",
			{ h_full = true, wrap = true, text_size = 15, line_height = 23, color = text },
			true
		),
		ui.text {
			function()
				if find(selected:get()) == nil then
					return "No note selected. Press " .. cmd .. "+N to write one."
				end
				local words = 0
				for _ in string.gmatch(body:get(), "%S+") do
					words += 1
				end
				return `{words} words`
			end,
			text_size = 11,
			color = tertiary,
		},
	},
}

local preview_pane = ui.column {
	w = 400,
	h_full = true,
	shrink = 0,
	relative = true,
	items = "stretch",
	bg = "$notes.preview",
	border_sides = { 0, 0, 0, 1 },
	border_color = line,
	ui.scroll {
		grow = 1,
		w_full = true,
		ui.column {
			w_full = true,
			pt = 68,
			px = 24,
			pb = 28,
			gap = 12,
			items = "stretch",
			selectable = true,
			ui.each(blocks, block),
		},
	},
	-- Over the scroll view: what scrolls under it shows through, blurred.
	ui.row {
		absolute = true,
		top = 0,
		left = 0,
		right = 0,
		h = 52,
		px = 20,
		items = "center",
		bg = "$notes.header",
		backdrop_blur = 20,
		border_sides = { 0, 0, 1, 0 },
		border_color = line,
		window_drag = true,
		ui.text { "Preview", text_size = 13, weight = 600, color = text },
	},
}

local function button(label: string, on_click: () -> (), fill: string)
	return ui.button {
		h = 30,
		px = 16,
		rounded = 8,
		items = "center",
		justify = "center",
		bg = fill,
		hover = { brightness = 1.15 },
		focus = { border = 0 },
		on_click = on_click,
		ui.text { label, text_size = 13, weight = 600, color = text },
	}
end

local dialog = ui.modal {
	open = confirm,
	label = "Delete note",
	w = 340,
	p = 20,
	gap = 12,
	rounded = 14,
	bg = "$surface.overlay",
	backdrop_blur = 24,
	border = 1,
	border_color = "$border",
	shadow = { y = 18, blur = 48, color = "#00000080" },
	items = "stretch",
	ui.text {
		function()
			local n = find(selected:get())
			local name = if n and n.title ~= "" then n.title else "Untitled"
			return `Delete “{name}”?`
		end,
		text_size = 15,
		weight = 700,
		color = text,
	},
	ui.text {
		"This note will be deleted immediately. You can't undo this action.",
		text_size = 13,
		color = secondary,
		wrap = true,
	},
	ui.row {
		justify = "end",
		gap = 8,
		mt = 6,
		button("Cancel", function()
			confirm:set(false)
		end, "$notes.button"),
		button("Delete", delete_selected, "$danger"),
	},
}

return ui.row {
	w_full = true,
	h_full = true,
	items = "stretch",
	bg = "$glass.tint",
	sidebar,
	editor,
	-- The preview, toggled by its button and the View menu. (`layout =
	-- "width"` would ease it open, but leaves the scroll view inside at a
	-- stale size for now.)
	ui.div {
		h_full = true,
		shrink = 0,
		items = "stretch",
		overflow = "hidden",
		w = function()
			return if preview:get() then 400 else 0
		end,
		preview_pane,
	},
	dialog,
}
