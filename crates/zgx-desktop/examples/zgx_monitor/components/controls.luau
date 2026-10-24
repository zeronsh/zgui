-- Small pieces the bars, rows and panels share.
local theme = require("../theme")
local icons = require("../icons")
local data = require("../data")

local M = {}

-- A square icon button.
function M.tool(svg: string, on_click: () -> (), size: number?, icon: number?): Node
	return ui.button {
		w = size or 28,
		h = size or 28,
		p = 0,
		rounded = 8,
		items = "center",
		justify = "center",
		shrink = 0,
		bg = theme.clear,
		hover = { bg = "#ffffff14" },
		active = { bg = "#ffffff1f" },
		focus = { border = 0 },
		on_click = on_click,
		ui.svg { code = svg, w = icon or 15, h = icon or 15, tint = theme.secondary },
	}
end

-- Zeron's sidebar icon: a frame with the sidebar's panel in it, which
-- narrows as the sidebar closes and widens as it opens (280 ms, the icon's
-- own curve). `size` points square; the icon is drawn on a 24-point grid.
function M.sidebar_glyph(open: () -> boolean, size: number, tint: Color): Node
	local k = size / 24
	local panel: Element? = nil
	local function width(): number
		return (if open() then 5.5 else 1.75) * k
	end
	ui.effect(function()
		local w = width()
		local el = panel
		if el then
			anime.animate(el, { width = w, duration = 280, ease = "cubicBezier(.22, 1, .36, 1)" })
		end
	end)
	return ui.div {
		w = size,
		h = size,
		shrink = 0,
		relative = true,
		ui.svg { code = icons.sidebar_frame, w = size, h = size, tint = tint },
		ui.div {
			absolute = true,
			left = 6.5 * k,
			top = 7.5 * k,
			h = 9 * k,
			w = width(),
			rounded = 0.875 * k,
			bg = tint,
			on_mount = function(el)
				panel = el
			end,
		},
	}
end

-- Empty titlebar space moves the window (a double click zooms it).
function M.drag_space(props: ContainerProps): Node
	props.window_drag = true
	return ui.div(props)
end

-- Six dots, two rows of three: a grip.
function M.dots(): Node
	local function row()
		local out: ContainerProps = { gap = 3, items = "center" }
		for _ = 1, 3 do
			table.insert(out, ui.div { w = 3, h = 3, rounded = 2, bg = "#ffffff99" })
		end
		return ui.row(out)
	end
	return ui.column { gap = 3, items = "center", row(), row() }
end

-- What follows the pointer while dragging a process or a panel: its name
-- and pid on a frosted chip.
function M.chip(pid: number?): Node
	return ui.row {
		h = 28,
		pl = 10,
		pr = 11,
		gap = 8,
		items = "center",
		rounded = 8,
		max_w = 240,
		font = theme.font,
		bg = "#2a2b33b3",
		backdrop_blur = 24,
		border = 1,
		border_color = "#ffffff1f",
		shadow = { y = 6, blur = 18, color = "#00000059" },
		ui.div { w = 6, h = 6, rounded = 3, bg = theme.accent, shrink = 0 },
		ui.text {
			function()
				return if pid then data.name_of(pid) else "Empty panel"
			end,
			shrink = 1,
			min_w = 0,
			text_size = 12.5,
			weight = 500,
			truncate = true,
			color = theme.text,
		},
		ui.text {
			if pid then tostring(pid) else "",
			shrink = 0,
			text_size = 11.5,
			tabular = true,
			color = theme.tertiary,
		},
	}
end

return M
