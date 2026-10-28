-- The titlebar: the tab strip (each tab an icon per pane and a name),
-- the slot a dragged pane would open in, and the tools.
local theme = require("../theme")
local icons = require("../icons")
local state = require("../state")
local sessions = require("../sessions")
local actions = require("../actions")
local sidebar = require("./sidebar")
local controls = require("./controls")
local menu = require("./menu")
local layout = require("@lib/layout")
local frames = require("../frames")

local tabs, active, focus = state.tabs, state.active, state.focus
local dragging, tab_drop, tab_slot = state.dragging, state.tab_drop, state.tab_slot

-- A tab's shape, shared by the tab, the chip that follows the pointer while
-- one is dragged, and the ghost of the tab a dragged pane would open as.
local tab_style = ui.variants {
	base = {
		h = 28,
		max_w = 170,
		items = "center",
		pl = "$terminal.tab.inset",
		pr = "$terminal.tab.end",
		gap = "$terminal.tab.gap",
		rounded = "$terminal.tab.radius",
		shrink = 1,
		cursor = "default",
	},
	variants = {
		kind = {
			tab = {},
			-- A tab a dragged tab would drop on: it lights.
			target = { bg = "$terminal.drop" },
			-- Blurred behind, as the strip's glass is, so it reads over a terminal.
			chip = { font = theme.font, bg = theme.wash, backdrop_blur = 24, border = 1, border_color = theme.line },
			slot = {
				w = 132,
				shrink = 0,
				overflow = "hidden",
				bg = "$terminal.slot",
				border = 1,
				border_color = "$terminal.slot_line",
			},
		},
	},
	defaults = { kind = "tab" },
}

-- Extracted block glyphs retain Rex's actual vector paths. Custom program
-- symbols and plugin icons continue through the existing description API.
local BLOCK_GLYPHS: { [string]: string } = {
	terminal = "terminal",
	claude = "claude",
	openai = "codex",
	branch = "git",
	browser = "browser",
}
local PROCESSES: { [string]: string } = {
	zsh = "terminal",
	bash = "terminal",
	fish = "terminal",
	node = "node",
	python = "python",
	python3 = "python",
	cargo = "rust",
	rustc = "rust",
	git = "git",
	claude = "claude",
	codex = "codex",
}
local ICON_SIZE = 16
-- glyphScale=1.25 belongs to Rex's block renderer, not a direct SVG size.
-- glyphOffsetAmount=0.3 is retained as source evidence; its block-view use is unknown.
local CARD_SCALE, ROTATION, MAX_ICONS = 0.15, 5, 3
local CARD_WIDTH, CARD_HEIGHT = 20, 18
local GLYPH_SIZE = 11
local CARD_STEP, CARD_RISE = 3, 2

local function badge(pane: string, dim: () -> boolean): Node
	local desc = function()
		return sessions.describe(pane)
	end
	local function block_glyph(): string?
		local d = desc()
		if d.glyph then
			return nil
		end
		local process = if sessions.is_terminal(pane) then ui.term(pane).process else ""
		return PROCESSES[process] or BLOCK_GLYPHS[d.icon]
	end
	return ui.overlay {
		id = `tab_badge:{pane}`,
		shrink = 0,
		border = 0.5,
		rounded = "$terminal.tab.tile_radius",
		w = CARD_WIDTH,
		h = CARD_HEIGHT,
		items = "center",
		justify = "center",
		gradient = function()
			if block_glyph() == "claude" then
				return { angle = 180, stops = { "#dc8666", "#d36e49" } }
			end
			return { angle = 180, stops = { "#515357", "#2b2d31" } }
		end,
		border_color = "#080a0dcc",
		shadow = { y = 1, blur = 2, color = "#00000099" },
		inset_shadow = { y = 1, color = "#ffffff40" },
		ui.svg {
			id = `tab_glyph:{pane}`,
			src = function()
				return "./assets/rex-block-glyphs/block-glyph-" .. (block_glyph() or "placeholder") .. ".svg"
			end,
			hidden = function()
				return block_glyph() == nil
			end,
			w = GLYPH_SIZE,
			h = GLYPH_SIZE,
			tint = function()
				local glyph = block_glyph()
				if glyph == "claude" then
					return "#ffffff"
				end
				if glyph == "codex" then
					return "#ffffff"
				end
				if glyph == "terminal" or glyph == "zsh" or glyph == "bash" or glyph == "fish" then
					return "#78e994"
				end
				return desc().tint or (if dim() then theme.tertiary else theme.text)
			end,
		},
		ui.div {
			hidden = function()
				return block_glyph() ~= nil
			end,
			controls.glyph(desc, ICON_SIZE, dim),
		},
	}
end

-- Keep the focus order separately from the split tree's spatial order.
local icon_history = ui.state("tab_icon_history", {} :: { [string]: { string } })
ui.effect(function()
	local focused = focus:get()
	local previous = icon_history:peek()
	local next_history = table.clone(previous)
	local changed = false
	for tab, pane in focused do
		local history = previous[tab] or {}
		if pane ~= "" and history[1] ~= pane then
			local reordered: { string } = { pane }
			for _, id in history do
				if id ~= pane and #reordered < MAX_ICONS then
					table.insert(reordered, id)
				end
			end
			next_history[tab] = reordered
			changed = true
		end
	end
	if changed then
		icon_history:set(next_history)
	end
end)

type Badge = { key: string, pane: string, index: number, count: number }
local function tab_slots(id: string): State<{ Badge }>
	return ui.computed(function()
		local root: layout.Tree? = nil
		for _, tab in tabs:get() do
			if tab.id == id then
				root = tab.root
			end
		end
		local panes: { string } = if root then layout.leaves(root) else {}
		local ordered: { string } = {}
		for _, recent in (icon_history:get())[id] or {} do
			if table.find(panes, recent) then
				table.insert(ordered, recent)
			end
		end
		for _, pane in panes do
			if not table.find(ordered, pane) then
				table.insert(ordered, pane)
			end
		end
		local count = math.min(#ordered, MAX_ICONS)
		local slots: { Badge } = {}
		for i = count, 1, -1 do
			-- Back-to-front order keeps the unscaled first card legible. Rex's
			-- transform helpers are recovered; this paint-order mapping is inferred.
			table.insert(slots, { key = ordered[i], pane = ordered[i], index = i - 1, count = count })
		end
		return slots
	end)
end

type FanCard = { x: number, y: number, scale: number, angle: number }
type FanGeometry = { width: number, height: number, cards: { FanCard } }
-- Reserve the actual transformed paint bounds, with a shared vertical center.
-- The helper's raw rect is not itself a layout box for rotated card paint.
local function fan_geometry(count: number): FanGeometry
	local cards: { FanCard } = {}
	local left: number = math.huge
	local top: number = math.huge
	local right: number = -math.huge
	local bottom: number = -math.huge
	for index = 0, count - 1 do
		local depth = if count < 2 then 0 else index / (count - 1)
		local scale = 1 - CARD_SCALE * depth
		-- A lone icon is not a fan. The caller-level singleton treatment is
		-- explicit here; the recovered fan transform applies to groups only.
		local angle = if count < 2 then 0 else ROTATION * (count - index) / count
		local radians = math.rad(angle)
		local half_w = (CARD_WIDTH * math.abs(math.cos(radians)) + CARD_HEIGHT * math.abs(math.sin(radians)))
			* scale
			/ 2
		local half_h = (CARD_WIDTH * math.abs(math.sin(radians)) + CARD_HEIGHT * math.abs(math.cos(radians)))
			* scale
			/ 2
		local center_x, center_y = index * CARD_STEP + CARD_WIDTH / 2, CARD_HEIGHT / 2 - index * CARD_RISE
		left, right = math.min(left, center_x - half_w), math.max(right, center_x + half_w)
		top, bottom = math.min(top, center_y - half_h), math.max(bottom, center_y + half_h)
		table.insert(cards, { x = index * CARD_STEP, y = -index * CARD_RISE, scale = scale, angle = angle })
	end
	if count == 0 then
		return { width = 0, height = CARD_HEIGHT, cards = cards }
	end
	for _, card in cards do
		card.x -= left
		card.y -= top
	end
	return { width = (right :: number) - left, height = (bottom :: number) - top, cards = cards }
end

local function badges(id: string, dim: () -> boolean): Node
	local slots = tab_slots(id)
	local geometry = ui.computed(function()
		return fan_geometry(#slots:get())
	end)
	return ui.overlay {
		id = `tab_fan:{id}`,
		relative = true,
		w = function()
			return geometry:get().width
		end,
		h = function()
			return geometry:get().height
		end,
		items = "center",
		justify = "center",
		shrink = 0,
		-- The positioned each box is the cards' containing block, rather than
		-- the tab whose top edge sits above the label's center.
		ui.each(slots, function(slot: Badge)
			local function card(): FanCard
				return geometry:get().cards[slot.index + 1]
			end
			return ui.overlay {
				id = `tab_fan_card:{id}:{slot.key}`,
				absolute = true,
				left = function()
					return card().x
				end,
				top = function()
					return card().y
				end,
				w = CARD_WIDTH,
				h = CARD_HEIGHT,
				items = "center",
				justify = "center",
				origin = { 0.5, 0.5 },
				scale = function()
					return card().scale
				end,
				rotate = function()
					return card().angle
				end,
				badge(slot.pane, dim),
			}
		end, {
			relative = true,
			w_full = true,
			h_full = true,
			key = function(slot: Badge)
				return `{slot.key}:{slot.index}`
			end,
		}),
	}
end

local function never(): boolean
	return false
end

-- A pane's name, "Terminal" until it has one.
local function name_of(pane: string?): string
	local name = sessions.describe(pane).name
	return if name ~= "" then name else "Terminal"
end

local M = {}
M.fan_geometry = fan_geometry

-- What follows the pointer while dragging a tab or a pane: the tab as it
-- shows active in the strip (a pane alone, its own icon).
function M.chip(pane: string?, tab: string?): Node
	local shown = if tab then badges(tab, never) else (if pane then badge(pane, never) else nil)
	return ui.row {
		variants = tab_style { kind = "chip" },
		ui.row { gap = "$terminal.tab.tile_gap", shrink = 0, items = "center", shown },
		ui.text {
			function()
				return name_of(pane)
			end,
			shrink = 1,
			min_w = 0,
			text_size = "$terminal.type.body",
			truncate = true,
			color = theme.text,
		},
	}
end

-- A dragged tab: a small window of what it holds, as a window's preview
-- shows it. Its title row is the tab's (its icons and name); under it, the
-- panes laid out as they are, in proportion, each a picture of what it shows
-- (taken as the drag starts), or its icon and name where there is none.
-- A dragged pane's picture: this much of its size, no narrower than PANE_MIN.
local PANE_SCALE, PANE_MIN = 0.22, 140
local MINI = {
	width = ui.theme.get("terminal.mini.width") :: number,
	max_h = ui.theme.get("terminal.mini.max_h") :: number,
	pad = ui.theme.get("terminal.mini.pad") :: number,
	gap = ui.theme.get("terminal.mini.pane_gap") :: number,
}
-- The box a `w` by `h` area shows in: as wide as the window allows, no
-- taller than it allows.
local function fit(w: number, h: number): (number, number)
	local inner = MINI.width - MINI.pad * 2
	if w <= 0 or h <= 0 then
		return inner, math.floor(inner * 0.6)
	end
	local scale = math.min(inner / w, MINI.max_h / h)
	return math.floor(w * scale), math.floor(h * scale)
end
-- A pane at `r` in the window: a picture of it, or its icon and name.
local function mini_pane(id: string, r: layout.Rect): Node
	-- At twice its size here, for a sharp picture on a Retina screen: a
	-- terminal's last frame, or a panel's card as the window last showed it
	-- (while its tab shows). A page is the platform's to draw, so it shows
	-- as its name.
	local wide = math.ceil(r.w * 2)
	local picture: string? = nil
	-- A panel's picture is its whole card, border and all.
	local carded = false
	local term = sessions.session(id)
	if term then
		picture = term:snapshot(wide)
	elseif not sessions.is_browser(id) then
		local card = frames.pane_element(id)
		picture = if card then card:snapshot(wide) else nil
		carded = picture ~= nil
	end
	return ui.row {
		absolute = true,
		left = r.x,
		top = r.y,
		w = r.w,
		h = r.h,
		items = "center",
		justify = "center",
		gap = "$terminal.tab.gap",
		overflow = "hidden",
		rounded = "$terminal.mini.pane_radius",
		bg = if carded then nil else theme.pane,
		border = if carded then 0 else 1,
		border_color = theme.line,
		if picture
			then ui.image {
				src = picture,
				absolute = true,
				left = 0,
				top = 0,
				w = r.w,
				h = r.h,
				fit = "cover",
			}
			else nil,
		if picture then nil else badge(id, never),
		if picture
			then nil
			else ui.text {
				name_of(id),
				shrink = 1,
				min_w = 0,
				truncate = true,
				text_size = "$terminal.type.body",
				color = theme.secondary,
			},
	}
end
-- The window: a title row of `icons` and `name`, then `panes` in a `w` by
-- `h` box.
local function mini_window(icons: Node, name: string, w: number, h: number, panes: { Node }): Node
	return ui.column {
		p = "$terminal.mini.pad",
		gap = "$terminal.mini.gap",
		rounded = "$terminal.mini.radius",
		font = theme.font,
		bg = "$terminal.mini.bg",
		-- Blurred behind (AppKit's material, in the preview's own window).
		backdrop_blur = 24,
		border = 1,
		border_color = theme.line,
		ui.row {
			w = w,
			h = 22,
			gap = "$terminal.tab.gap",
			items = "center",
			ui.row { gap = "$terminal.tab.tile_gap", shrink = 0, items = "center", icons },
			ui.text {
				name,
				shrink = 1,
				min_w = 0,
				truncate = true,
				text_size = "$terminal.type.body",
				color = theme.text,
			},
		},
		ui.div { relative = true, w = w, h = h, table.unpack(panes) },
	}
end
function M.mini(tab: string): Node
	local area = frames.area()
	local w, h = fit(area.w, area.h)
	local panes: { Node } = {}
	local root = actions.root_of(tab)
	if root then
		local placed = layout.frames(root, state.ratios:peek(), MINI.gap).panes
		for _, id in layout.leaves(root) do
			table.insert(panes, mini_pane(id, layout.resolve(placed[id], w, h)))
		end
	end
	return mini_window(badges(tab, never), name_of((focus:peek())[tab]), w, h, panes)
end
-- A pane dragged by its grip: a picture of it alone, about a fifth of its
-- size (as Ghostty's split drag shows one), on the window's glass.
function M.mini_pane(pane: string): Node
	local shown = frames.shown(frames.pane_element(pane))
	local w = math.clamp(math.floor(shown.w * PANE_SCALE), PANE_MIN, MINI.width)
	local h = if shown.w > 0 then math.floor(w * shown.h / shown.w) else math.floor(w * 0.6)
	if h > MINI.max_h then
		w, h = math.floor(w * MINI.max_h / h), MINI.max_h
	end
	return ui.div {
		relative = true,
		w = w,
		h = h,
		rounded = "$terminal.mini.pane_radius",
		font = theme.font,
		bg = "$terminal.mini.bg",
		backdrop_blur = 24,
		mini_pane(pane, { x = 0, y = 0, w = w, h = h }),
	}
end

-- A tab's padding, for the animations that fold it away and back.
local INSET, END = ui.theme.get("terminal.tab.inset"), ui.theme.get("terminal.tab.end")

local function tab_button(id: string, vertical: boolean?): Node
	local function pane(): string?
		return (focus:get())[id]
	end
	local function is_active()
		return active:get() == id
	end
	local function selection_shown(): boolean
		return is_active() and (state.sidebar:get() == (vertical == true))
	end
	local hover_layer, hover = controls.hover_glass("$terminal.tab.radius")
	-- Lifted out of the strip, it folds away (still mounted: unmounting the
	-- dragged tab would end the drag), and unfolds where it lands.
	local element: Element? = nil
	local width = 0
	local SPACE = if vertical
		then { "height", "minHeight", "paddingTop", "paddingBottom", "marginBottom", "opacity" }
		else { "width", "minWidth", "paddingLeft", "paddingRight", "marginRight", "opacity" }
	ui.effect(function()
		local out = state.lifted:get() == id
		local el = element
		if not el then
			return
		end
		if out then
			width = if vertical then el.height or 0 else el.width or 0
			local collapse: ElementParams = if vertical
				then {
					minHeight = 0,
					height = 0,
					paddingTop = 0,
					paddingBottom = 0,
					marginBottom = -3,
				}
				else {
					minWidth = 0,
					width = 0,
					paddingLeft = 0,
					paddingRight = 0,
					-- Its gap in the strip closes too.
					marginRight = -3,
				}
			collapse.opacity = 0
			collapse.duration = 220
			collapse.ease = "outQuint"
			anime.animate(el, collapse)
		elseif width > 0 then
			local d = dragging:peek()
			local expand: ElementParams = if vertical
				then {
					height = width,
					paddingTop = 0,
					paddingBottom = 0,
					marginBottom = 0,
				}
				else {
					width = width,
					paddingLeft = INSET,
					paddingRight = END,
					marginRight = 0,
				}
			expand.opacity = if d.kind == "tab" and d.id == id then 0.35 else 1
			expand.duration = 240
			expand.ease = "outQuint"
			expand.onComplete = function()
				anime.utils.cleanInlineStyles(el, SPACE)
			end
			anime.animate(el, expand)
			width = 0
		end
	end)
	return ui.row {
		variants = tab_style {
			kind = function()
				return if tab_drop:get() == id then "target" else "tab"
			end,
		},
		id = if vertical then `sidebar_tab:{id}` else `tab:{id}`,
		role = "tab",
		selected = is_active,
		focusable = true,
		label = function()
			return name_of(pane())
		end,
		h = if vertical then 33 else 30,
		w_full = vertical or false,
		max_w = if vertical then 10000 else 170,
		pl = if vertical then 10 else "$terminal.tab.inset",
		shrink = if vertical then 0 else 1,
		relative = true,
		on_mount = function(el: Element)
			element = el
		end,
		-- Where it was while dragged: a dim slot.
		opacity = function()
			local d = dragging:get()
			return if d.kind == "tab" and d.id == id then 0.35 else 1
		end,
		on_click = function()
			actions.activate(id)
		end,
		on_key = function(e)
			if e.key == "Enter" or e.key == " " then
				actions.activate(id)
				return true
			end
			return false
		end,
		context_menu_style = menu.STYLE,
		context_menu_below = true,
		context_menu = function()
			local others = #tabs:peek() > 1
			local items = {
				menu.entry("New Tab", function()
					actions.new_tab(sessions.cwd_of((focus:peek())[id]))
				end),
				menu.separator(),
				menu.entry("Close Tab", function()
					actions.close_tab(id)
				end),
			}
			if others then
				table.insert(
					items,
					menu.entry("Close Other Tabs", function()
						for _, other in tabs:peek() do
							if other.id ~= id then
								actions.close_tab(other.id)
							end
						end
					end)
				)
			end
			return items
		end,
		on_hover = hover,
		on_drag = function(): state.Payload
			dragging:set { kind = "tab", id = id }
			return { kind = "tab", id = id }
		end,
		drag_preview = function()
			return M.mini(id)
		end,
		-- Below the pointer, so the strip and the panes it is over show.
		drag_preview_anchor = { 0.5, 0 },
		drag_preview_offset = { 0, 18 },
		on_drag_end = function()
			dragging:set {}
			state.drop:set {}
			tab_drop:set("")
			-- Let go away from the strip and from another tab's panes: it is
			-- back where it was.
			actions.land(id)
		end,
		drop_when = function(payload: state.Payload)
			return payload.kind == "pane" or (payload.kind == "tab" and payload.id ~= id)
		end,
		-- A tab or a pane over its middle marks it: dropping moves the pane,
		-- or the tab's panes, in, and resting there shows it (spring-loaded,
		-- below). Near an edge, a tab takes this one's place as it passes (a
		-- lifted one lands), and a pane opens a new tab beside it.
		on_drag_over = function(payload: state.Payload, e)
			local at = if vertical then e.local_y else e.local_x
			local length = if vertical then e.h else e.w
			if payload.kind == "tab" then
				actions.land(payload.id)
				-- Its outer quarters reorder, as a link's drop does in Chrome's
				-- strip; its middle half is the tab.
				local quarter = length / 4
				if at < quarter or at > length - quarter then
					if tab_drop:peek() == id then
						tab_drop:set("")
					end
					actions.reorder(payload.id, id, if at < quarter then "before" else "after")
				elseif tab_drop:peek() ~= id then
					tab_drop:set(id)
				end
				return
			end
			-- Its edges (thin, as the gaps between tabs are) open a new tab
			-- beside it; the rest is the tab, which springs open.
			local edge = math.min(10, length * 0.15)
			if at < edge then
				actions.show_slot(payload.id, id)
			elseif at > length - edge then
				local list = tabs:peek()
				local after = ""
				for i, other in list do
					if other.id == id and list[i + 1] then
						after = list[i + 1].id
					end
				end
				actions.show_slot(payload.id, after)
			else
				actions.hide_slot()
				if tab_drop:peek() ~= id then
					tab_drop:set(id)
				end
			end
		end,
		on_drag_leave = function()
			if tab_drop:peek() == id then
				tab_drop:set("")
			end
		end,
		-- Spring-loaded: a pane or a tab resting on another tab's middle (the
		-- system's delay, or Space) shows that tab, so what is dragged can go
		-- among its panes. As Finder's folders do, it flashes twice first.
		on_drag_spring = function(payload: state.Payload)
			local el = element
			if tab_drop:peek() ~= id or active:peek() == id or not el then
				return false
			end
			anime.animate(el, {
				opacity = { 1, 0.35 },
				duration = 70,
				alternate = true,
				loop = 3,
				onComplete = function()
					anime.utils.cleanInlineStyles(el, { "opacity" })
					actions.activate(id)
				end,
			})
			return true
		end,
		on_drop = function(payload: state.Payload)
			local marked = tab_drop:peek() == id
			tab_drop:set("")
			if payload.kind == "pane" and not actions.commit_slot() then
				actions.move_pane_to_tab(payload.id, id)
			elseif payload.kind == "tab" and marked then
				actions.join_tab(payload.id, id)
			end
		end,
		-- The active tab's frosted glass (one piece, sliding between tabs)
		-- over a fainter glass while the pointer is on the tab.
		hover_layer,
		controls.glass(
			if vertical then "sidebar-tab-active" else "titlebar-tab-active",
			selection_shown,
			"$terminal.tab.radius",
			`tab_selection:{if vertical then "sidebar" else "titlebar"}:{id}`,
			false
		),
		ui.row {
			gap = "$terminal.tab.tile_gap",
			shrink = 0,
			items = "center",
			badges(id, function()
				return not is_active()
			end),
		},
		ui.row {
			id = `tab_label_clip:{id}`,
			shrink = 1,
			min_w = 0,
			overflow = "hidden",
			fade_x = { 0, 12 },
			fade_overflow = true,
			ui.text {
				id = `tab_label:{id}`,
				function()
					return name_of(pane())
				end,
				shrink = 0,
				text_size = "$terminal.type.body",
				weight = "$font.weight.semibold",
				color = function()
					return if is_active() then theme.text else theme.secondary
				end,
			},
		},
		-- The bell rang in a pane of a background tab.
		ui.div {
			hidden = function()
				local p = pane()
				return is_active() or p == nil or not sessions.is_terminal(p) or not ui.term(p).bell
			end,
			w = 6,
			h = 6,
			rounded = "$radius.full",
			bg = theme.bell,
			shrink = 0,
		},
	}
end

-- The tab strip's items: the tabs, and the slot a dragged pane would open
-- in (`slot` is that pane).
type Item = { id: string, slot: string? }
local strip = ui.computed(function(): { Item }
	local list = tabs:get()
	local s = tab_slot:get()
	local out: { Item } = {}
	for _, tab in list do
		if s.pane and tab.id == s.before then
			table.insert(out, { id = "slot", slot = s.pane })
		end
		table.insert(out, { id = tab.id })
	end
	if s.pane and (s.before == "" or #out == #list) then
		table.insert(out, { id = "slot", slot = s.pane })
	end
	return out
end)

-- Where a dragged pane would open as a tab: a ghost of that tab, opening
-- up so the tabs after it slide over.
local function slot_tab(pane: string, vertical: boolean?): Node
	return ui.row {
		variants = tab_style { kind = "slot" },
		w = if vertical then "100%" else 132,
		h = if vertical then 33 else 30,
		max_w = if vertical then 10000 else 210,
		on_mount = function(el)
			local enter: ElementParams = { opacity = { 0, 1 }, duration = 200, ease = "outQuint" }
			if vertical then
				enter.height = { 0, 33 }
			else
				enter.width = { 0, 132 }
			end
			anime.animate(el, enter)
		end,
		drop_when = function(payload: state.Payload)
			return payload.kind == "pane"
		end,
		on_drop = function()
			actions.commit_slot()
		end,
		badge(pane, never),
		ui.text {
			function()
				return name_of(pane)
			end,
			shrink = 1,
			min_w = 0,
			text_size = "$terminal.type.body",
			truncate = true,
			color = theme.secondary,
		},
	}
end

-- Sidebar rows reuse every tab action and drop target; only their axis changes.
function M.SidebarTabs(): Node
	return ui.scroll {
		id = "sidebar_tabs",
		h = function()
			return math.min(#strip:get() * 36 + 20, 260)
		end,
		min_h = 0,
		shrink = 1,
		fade = { 8, 8 },
		fade_overflow = true,
		ui.column {
			role = "tablist",
			label = "Terminal Tabs",
			focus_group = "vertical",
			px = 10,
			py = 4,
			gap = 3,
			items = "stretch",
			ui.each(strip, function(item: Item)
				return if item.slot then slot_tab(item.slot, true) else tab_button(item.id, true)
			end, {
				key = function(item: Item)
					return item.id
				end,
				animate = { duration = 240, ease = "outQuint" },
			}),
			ui.div {
				h = 12,
				drop_when = function(payload: state.Payload)
					return payload.kind == "pane"
				end,
				on_drag_over = function(payload: state.Payload)
					actions.show_slot(payload.id, "")
				end,
				on_drop = function()
					actions.commit_slot()
				end,
			},
		},
	}
end

function M.Titlebar(): Node
	return ui.row {
		hidden = state.sidebar,
		w_full = true,
		h = sidebar.titlebar_height,
		id = "titlebar_toolbar",
		shrink = 0,
		items = "center",
		-- A pane anywhere else on it (the new tab button, the tools) opens as
		-- a tab where the slot shows, at the end if none does yet. A lifted
		-- tab back on it lands.
		drop_when = function(payload: state.Payload)
			return payload.kind == "pane" or payload.kind == "tab"
		end,
		on_drag_over = function(payload: state.Payload)
			if payload.kind == "tab" then
				actions.land(payload.id)
			elseif not (tab_slot:peek()).pane then
				actions.show_slot(payload.id, "")
			end
		end,
		on_drop = function(payload: state.Payload)
			if payload.kind == "pane" then
				actions.commit_slot()
			end
		end,
		gap = "$terminal.tab.strip_gap",
		pr = "$terminal.inset",
		-- Room for the traffic lights and the sidebar toggle (drawn over this,
		-- app.luau) while the sidebar is closed; a little air after an open
		-- one. It eases as the sidebar does, so the tabs slide clear of the
		-- toggle rather than under it.
		controls.drag_space {
			w = function()
				return if state.sidebar:get() then 6 else sidebar.toggle_left() + sidebar.TOGGLE_SIZE
			end,
			layout = "width",
			layout_ease = sidebar.EASE,
			layout_duration = sidebar.DURATION,
			shrink = 0,
		},
		ui.row {
			role = "tablist",
			label = "Terminal Tabs",
			focus_group = "horizontal",
			gap = "$terminal.tab.strip_gap",
			items = "center",
			shrink = 1,
			min_w = 0,
			ui.each(strip, function(item: Item)
				if item.slot then
					return slot_tab(item.slot)
				end
				return tab_button(item.id)
			end, {
				key = function(item: Item)
					return item.id
				end,
				-- A reorder slides the tabs from where they were.
				animate = { duration = 240, ease = "outQuint" },
			}),
		},
		controls.tool(icons.plus, function()
			actions.new_tab(sessions.cwd_of(actions.current()))
		end, "New Tab", "titlebar_new_tab"),
		-- Dropping a pane here gives it a tab of its own.
		controls.drag_space {
			grow = 1,
			min_w = 16,
			drop_when = function(payload: state.Payload)
				return payload.kind == "pane"
			end,
			on_drag_over = function(payload: state.Payload)
				actions.show_slot(payload.id, "")
			end,
			on_drop = function()
				actions.commit_slot()
			end,
		},
		controls.tool(icons.split_right, function()
			actions.split("row")
		end, "Split Right", "titlebar_split_right"),
		controls.tool(icons.split_down, function()
			actions.split("column")
		end, "Split Down", "titlebar_split_down"),
	}
end

return M
