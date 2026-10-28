-- ⌘K: what a new pane shows. A terminal, a browser, every panel a running
-- plugin adds, and the plugins themselves, in sections; typing filters
-- them (the part that matched shows brighter), the arrows choose (the
-- highlight slides there), Return opens one beside the focused pane
-- (⇧Return: in a tab of its own), Escape or a click outside puts it away.
local theme = require("../theme")
local icons = require("../icons")
local state = require("../state")
local sessions = require("../sessions")
local actions = require("../actions")
local kit = require("./kit")
local layout = require("@lib/layout")

local open, query, row = state.palette, state.palette_query, state.palette_row

-- What a row opens, and how it shows: its icon on a tile tinted `tint`.
export type Item = {
	kind: string,
	title: string,
	detail: string,
	icon: icons.Name,
	tint: string,
	key: string?,
}
-- A row of the list: a section's title, or an item (`index`: its place
-- among the items, what the arrows count; `typed`: what it highlights).
export type Row = { section: string?, item: Item?, index: number?, typed: string? }

local M = {}

local TERMINAL: Item = {
	kind = "terminal",
	title = "Terminal",
	detail = "A shell where the focused one is",
	icon = "terminal",
	tint = "#a3acbb",
	key = "⌘D",
}
local BROWSER: Item = {
	kind = "browser",
	title = "Browser",
	detail = "A web page",
	icon = "browser",
	tint = "#6ea8ff",
	key = "⇧⌘B",
}
local PLUGINS: Item = {
	kind = "plugins",
	title = "Plugins",
	detail = "What is installed, and what each may do",
	icon = "plugin",
	tint = "#a3acbb",
}
local PANEL_TINT = "#b69cff"

local function matches(item: Item, typed: string): boolean
	return typed == ""
		or item.title:lower():find(typed, 1, true) ~= nil
		or item.detail:lower():find(typed, 1, true) ~= nil
end

-- The items as typed so far, in order: what the arrows step through.
local items = ui.computed(function(): { Item }
	local typed = query:get():lower()
	local out: { Item } = {}
	local function add(item: Item)
		if matches(item, typed) then
			table.insert(out, item)
		end
	end
	add(TERMINAL)
	add(BROWSER)
	for _, plugin in state.plugins:get() do
		if plugin.status == "enabled" then
			for _, panel in plugin.panels do
				add {
					kind = panel.kind,
					title = panel.title,
					-- Whose it is, unless the panel says so already.
					detail = if panel.description == ""
						then plugin.name
						elseif panel.title == plugin.name then panel.description
						else `{panel.description} · {plugin.name}`,
					icon = "plugin",
					tint = PANEL_TINT,
				}
			end
		end
	end
	add(PLUGINS)
	return out
end)
-- The rows shown: each section's title over its items.
local rows = ui.computed(function(): { Row }
	local typed = query:get()
	local out: { Row } = {}
	local last: string? = nil
	for i, item in items:get() do
		local section = if item.kind == "terminal" or item.kind == "browser"
			then "Open"
			elseif item.kind == "plugins" then "Manage"
			else "Panels"
		if section ~= last then
			table.insert(out, { section = section })
			last = section
		end
		table.insert(out, { item = item, index = i, typed = typed })
	end
	return out
end)
-- Typing starts the choice over at the top.
ui.effect(function()
	query:get()
	row:set(1)
end)

function M.open()
	if open:peek() then
		M.close()
		return
	end
	query:set("")
	row:set(1)
	open:set(true)
	-- What is in the plugins directory now: one just put there shows (and
	-- asks to be allowed).
	ui.invoke("plugins_rescan")
	local pane = actions.current()
	if pane and sessions.is_terminal(pane) then
		ui.term(pane):blur()
	end
end

-- Put away, focus back where it was.
function M.close()
	open:set(false)
	local pane = actions.current()
	if pane then
		sessions.focus_view(pane)
	end
end

-- The pane already showing `kind`, for what there is one of (the plugins).
local function showing(kind: string): (string?, string?)
	for _, tab in state.tabs:peek() do
		for _, pane in layout.leaves(tab.root) do
			if (state.kinds:peek())[pane] == kind then
				return tab.id, pane
			end
		end
	end
	return nil, nil
end

function M.choose(item: Item?, new_tab: boolean?)
	if not item then
		return
	end
	open:set(false)
	if item.kind == "plugins" then
		local tab, pane = showing("plugins")
		if tab and pane then
			actions.activate(tab)
			actions.focus_pane(tab, pane)
			return
		end
	end
	if new_tab then
		actions.new_tab(sessions.cwd_of(actions.current()), nil, nil, item.kind)
	else
		actions.split("row", item.kind)
	end
end

-- The search field's keys.
function M.on_key(e: KeyEvent): boolean
	local list = items:peek()
	if e.key == "Escape" then
		M.close()
		return true
	elseif e.key == "ArrowDown" or e.key == "ArrowUp" or e.key == "Tab" then
		if #list > 0 then
			local by = if e.key == "ArrowUp" or (e.key == "Tab" and e.shift) then -1 else 1
			row:set((row:peek() - 1 + by) % #list + 1)
		end
		return true
	elseif e.key == "Enter" then
		M.choose(list[row:peek()], e.shift)
		return true
	end
	return false
end

-- A key in a hint: a small raised chip.
local function key(label: string): Node
	return ui.row {
		variants = kit.chip { kind = "key" },
		ui.text { label, text_size = "$terminal.type.xxs", weight = "$font.weight.medium", color = theme.secondary },
	}
end

-- `text`, with what was typed brighter where it matched.
local function highlighted(
	text: string,
	typed: string,
	size: string,
	weight: string,
	base: string,
	bright: string
): Node
	typed = typed:lower()
	local at = if typed ~= "" then text:lower():find(typed, 1, true) else nil
	if not at then
		return ui.text { text, text_size = size, weight = weight, color = base, truncate = true }
	end
	local stop = at + #typed
	return ui.rich {
		text_size = size,
		weight = weight,
		color = base,
		truncate = true,
		ui.span { text:sub(1, at - 1) },
		-- (Spans take numbers, not tokens.)
		ui.span { text:sub(at, stop - 1), color = bright, weight = 600 },
		ui.span { text:sub(stop) },
	}
end

local function entry(item: Item, index: number, typed: string): Node
	local function chosen(): boolean
		return row:get() == index
	end
	return ui.row {
		id = "open:" .. item.kind,
		variants = kit.row { kind = "option" },
		w_full = true,
		min_w = 0,
		h = 26,
		px = 10,
		gap = 8,
		label = item.title .. ": " .. item.detail,
		tooltip = item.detail,
		on_hover = function(inside)
			if inside and row:peek() ~= index then
				row:set(index)
			end
		end,
		on_click = function()
			M.choose(item)
		end,
		bg = function()
			return if chosen() then "$terminal.primary.bg" else theme.clear
		end,
		ui.column {
			grow = 1,
			shrink = 1,
			min_w = 0,
			gap = 2,
			highlighted(
				item.title,
				typed,
				"$terminal.type.option",
				"$font.weight.medium",
				if chosen() then "$terminal.on_fill" else theme.text,
				"$terminal.bright"
			),
		},
		-- Its own shortcut at rest; Return, where the choice is.
		ui.show(function()
			return item.key ~= nil and not chosen()
		end, ui.text { item.key or "", shrink = 0, text_size = "$terminal.type.sm", color = theme.quaternary }),
		ui.show(chosen, key("↵")),
	}
end

local function section(title: string): Node
	return ui.text {
		title,
		pt = 10,
		pb = "$space.1",
		px = 10,
		text_size = "$terminal.type.xs",
		weight = "$font.weight.semibold",
		color = theme.quaternary,
	}
end

local function hint(keys: { string }, what: string): Node
	local out: ContainerProps = { gap = "$space.1", items = "center" }
	for _, k in keys do
		table.insert(out, key(k))
	end
	table.insert(out, ui.text { what, ml = 2, text_size = "$terminal.type.sm", color = theme.tertiary })
	return ui.row(out)
end

function M.Palette(): Node
	return ui.presence(function()
		return if open:get() then "open" else nil
	end, function(): Node
		return ui.popover {
			open = open,
			label = "Open a terminal, browser or panel",
			anchor = ui.div { w = 0, h = 0 },
			variants = kit.sheet { kind = "palette" },
			w = 330,
			rounded = "$terminal.palette.radius",
			p = 0,
			max_h = 406,
			placement = { side = "bottom", align = "center", offset = 0 },
			ui.column {
				id = "palette-surface",
				w_full = true,
				items = "stretch",
				-- A short window: the list gives way and scrolls.
				max_h = "100%",
				on_mount = function(el: Element)
					anime.animate(el, {
						opacity = { 0, 1 },
						scale = { 0.97, 1 },
						y = { -8, 0 },
						duration = 220,
						ease = "outQuart",
					})
				end,
				ui.row {
					h = 30,
					mx = 8,
					mt = 8,
					mb = 6,
					px = 10,
					rounded = 10,
					bg = theme.hover,
					gap = 6,
					items = "center",
					ui.svg { code = icons.search, w = 14, h = 14, shrink = 0, tint = theme.tertiary },
					ui.input {
						id = "palette-query",
						label = "Open",
						value = query,
						placeholder = "Search terminals, browsers, and panels…",
						placeholder_color = theme.quaternary,
						grow = 1,
						h = 26,
						px = 0,
						text_size = "$terminal.type.lg",
						color = theme.text,
						bg = theme.clear,
						border = 0,
						outline = 0,
						hover = { bg = theme.clear },
						focus = { bg = theme.clear, border = 0, outline = 0 },
						on_mount = function(el: Element)
							el:focus()
						end,
						on_key = M.on_key,
					},
				},
				ui.div { h = 1, bg = "$terminal.key.bg" },
				ui.scroll {
					id = "palette-options",
					w_full = true,
					px = 6,
					pb = 6,
					items = "stretch",
					max_h = 320,
					shrink = 1,
					min_h = 0,
					ui.column {
						w_full = true,
						min_w = 0,
						items = "stretch",
						-- Unkeyed: a plugin's new title rebuilds its row.
						ui.each(rows, function(r: Row): Node
							if r.section then
								return section(r.section)
							end
							return entry(r.item :: Item, r.index :: number, r.typed or "")
						end),
						ui.column {
							hidden = function()
								return #items:get() > 0
							end,
							py = 28,
							gap = "$space.2",
							items = "center",
							ui.svg { code = icons.search, w = 22, h = 22, tint = theme.quaternary },
							ui.text {
								function()
									return `Nothing matches “{query:get()}”`
								end,
								text_size = "$terminal.type.body",
								color = theme.tertiary,
							},
						},
					},
				},
				ui.div { h = 1, bg = theme.wash },
				ui.row {
					h = 29,
					px = 8,
					gap = 10,
					items = "center",
					bg = "$terminal.footer",
					hint({ "↑↓" }, "Choose"),
					hint({ "↵" }, "Open"),
					hint({ "⇧↵" }, "New tab"),
				},
			},
		}
	end, { absolute = true, left = "50%", top = 56, w = 0, h = 0 })
end

return M
