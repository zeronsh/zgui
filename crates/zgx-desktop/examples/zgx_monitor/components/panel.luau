-- A panel: a card placed where frames.luau puts it, showing one process's
-- readings. Its header moves it: dragged onto another panel's edge it goes
-- beside that one, onto its middle the two swap. A process dragged over its
-- middle offers to show there.
local theme = require("../theme")
local icons = require("../icons")
local state = require("../state")
local data = require("../data")
local actions = require("../actions")
local frames = require("../frames")
local controls = require("./controls")
local readings = require("./readings")

local HEADER = 40

local drop, dragging = state.drop, state.dragging

-- A panel with no process yet, or one a dragged process would open.
local function placeholder(title: () -> string, body: string): Node
	return ui.column {
		grow = 1,
		items = "center",
		justify = "center",
		gap = 6,
		p = 16,
		ui.svg { code = icons.pulse, w = 22, h = 22, tint = theme.quaternary, mb = 4 },
		ui.text { title, text_size = 13.5, weight = 500, color = theme.secondary },
		ui.text { body, text_size = 12, color = theme.tertiary, align = "center", wrap = true, max_w = 260 },
	}
end

export type Props = { pane: string }

return ui.component(function(props: Props)
	local pane = props.pane
	local element: Element? = nil
	local start = (frames.frames:peek()).panes[pane] or { x = 0, y = 0, w = 0, h = 0 }
	-- The content takes the panel's new size at once and stays put in the
	-- card's top left while the card eases around it (clipping it): it lays
	-- out once, not on every frame of the move.
	local size = ui.use_state { w = start.w, h = start.h }
	ui.effect(function()
		local f = frames.frames:get()
		local r = f.panes[pane]
		if not r then
			return
		end
		local s = size:peek()
		if s.w ~= r.w or s.h ~= r.h then
			size:set { w = r.w, h = r.h }
		end
		if element then
			frames.place(element, r, f.smooth)
		end
	end)

	local function pid(): number?
		return (state.pids:get())[pane]
	end
	-- A dragged process previewing a panel of its own: this one, until dropped.
	local function ghost(): number?
		local d = drop:get()
		return if d.kind == "process" and d.id == pane and d.zone ~= "center" then d.pid else nil
	end
	local function shown(): number?
		return pid() or ghost()
	end
	local function focused()
		return state.active:get() == pane
	end
	local function several()
		local tree = state.root:get()
		return tree ~= nil and tree.kind == "split"
	end
	local function moving()
		local d = dragging:get()
		return d.kind == "panel" and d.id == pane
	end
	-- A dragged process over the middle: it would show here.
	local function offered(): number?
		local d = drop:get()
		return if d.kind == "process" and d.zone == "center" and d.target == pane then d.pid else nil
	end
	local function alive(): boolean
		local p = shown()
		return p ~= nil and data.proc(p) ~= nil
	end
	local header_hover = ui.use_state(false)

	local header = ui.row {
		w_full = true,
		h = HEADER,
		shrink = 0,
		pl = 14,
		pr = 6,
		gap = 8,
		items = "center",
		cursor = "grab",
		border_sides = { 0, 0, 1, 0 },
		border_color = theme.line,
		on_hover = function(inside)
			header_hover:set(inside)
		end,
		on_drag = function(): state.Payload
			dragging:set { kind = "panel", id = pane }
			return { kind = "panel", id = pane }
		end,
		drag_preview = function()
			return controls.chip(pid())
		end,
		on_drag_end = function()
			dragging:set {}
			drop:set {}
		end,
		-- Live, exited, or nothing to show.
		ui.div {
			w = 7,
			h = 7,
			rounded = 4,
			shrink = 0,
			bg = function()
				if not shown() then
					return theme.quaternary
				end
				return if alive() then theme.live else theme.exited
			end,
		},
		ui.text {
			function()
				local p = shown()
				return if p then data.name_of(p) else "Empty panel"
			end,
			id = `title-{pane}`,
			shrink = 1,
			min_w = 0,
			truncate = true,
			text_size = 13,
			weight = 600,
			color = theme.text,
		},
		ui.text {
			function()
				local p = shown()
				if not p then
					return ""
				end
				return if alive() then tostring(p) else `{p} · exited`
			end,
			shrink = 0,
			text_size = 11.5,
			tabular = true,
			color = theme.tertiary,
		},
		ui.div { grow = 1, min_w = 4 },
		-- The grip, while the pointer is on the header.
		ui.row {
			shrink = 0,
			px = 4,
			opacity = ui.tween(header_hover, { duration = 140 }),
			controls.dots(),
		},
		ui.row {
			shrink = 0,
			gap = 2,
			items = "center",
			opacity = ui.tween(header_hover, { from = 0.55, to = 1, duration = 140 }),
			ui.show(
				several,
				controls.tool(icons.expand, function()
					actions.focus(pane)
					actions.toggle_zoom()
				end, 24, 13)
			),
			controls.tool(icons.close, function()
				actions.close(pane)
			end, 24, 14),
		},
	}

	-- The readings, crossfading when the panel moves to another process.
	local body = ui.presence(function(): (number | string)?
		return shown() or "empty"
	end, function(value: number | string)
		if value == "empty" then
			return placeholder(function()
				return "Empty panel"
			end, "Pick a process in the sidebar, browse with ↑ and ↓, or drop one here")
		end
		return readings(value :: number, size, HEADER)
	end, {
		grow = 1,
		w_full = true,
		min_h = 0,
		enter = { opacity = { 0, 1 }, duration = 160, ease = "outQuad" },
		exit = { opacity = 0, duration = 120, ease = "outQuad" },
	})

	-- A process over the middle: "show it here", over the panel.
	local offer = ui.presence(function(): number?
		return offered()
	end, function(p: number)
		-- Placed itself: the presence has no box of its own to take the
		-- pointer while nothing is offered.
		return ui.column {
			absolute = true,
			left = 0,
			top = 0,
			w_full = true,
			h_full = true,
			items = "center",
			justify = "center",
			bg = "#a99cff14",
			ui.row {
				h = 30,
				px = 12,
				gap = 7,
				items = "center",
				rounded = 9,
				bg = "#2a2b33b3",
				backdrop_blur = 20,
				border = 1,
				border_color = "#a99cff59",
				shadow = { y = 6, blur = 18, color = "#00000059" },
				ui.text { "Show", text_size = 12.5, color = theme.secondary },
				ui.text { data.name_of(p), text_size = 12.5, weight = 600, color = theme.text },
				ui.text { "here", text_size = 12.5, color = theme.secondary },
			},
		}
	end, {
		enter = { opacity = { 0, 1 }, duration = 140, ease = "outQuad" },
		exit = { opacity = 0, duration = 120, ease = "outQuad" },
	})

	return ui.column {
		id = `panel-{pane}`,
		absolute = true,
		left = start.x,
		top = start.y,
		w = start.w,
		h = start.h,
		on_mount = function(el)
			element = el
			-- Where it goes now (the frames may have moved on since it was
			-- built); arriving with a change, it grows in there.
			local f = frames.frames:peek()
			local r = f.panes[pane]
			if r then
				frames.place(el, r, false)
			end
			if f.smooth then
				anime.animate(el, { opacity = { 0, 1 }, scale = { 0.97, 1 }, duration = 260, ease = "outQuint" })
			end
		end,
		rounded = frames.radius,
		overflow = "hidden",
		bg = function()
			return if ghost() then "#ffffff08" else theme.panel
		end,
		border = 1,
		border_dash = function(): { number }?
			return if ghost() then { 6, 4 } else nil
		end,
		border_color = function()
			if ghost() or offered() then
				return "#a99cff80"
			end
			if moving() then
				return "#ffffff59"
			end
			return if several() and focused() then theme.ring else theme.line
		end,
		on_press = function()
			actions.focus(pane)
		end,
		ui.column {
			absolute = true,
			left = 0,
			top = 0,
			items = "stretch",
			w = function()
				return size:get().w
			end,
			h = function()
				return size:get().h
			end,
			header,
			body,
		},
		offer,
	}
end)
