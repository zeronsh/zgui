-- A divider between two panels: dragging it moves their split.
local theme = require("../theme")
local state = require("../state")
local frames = require("../frames")
local layout = require("@lib/layout")

local ratios = state.ratios

export type Props = { id: string }

return ui.component(function(props: Props)
	local id = props.id
	local element: Element? = nil
	local function find(list: { layout.Divider }): layout.Divider?
		for _, d in list do
			if d.id == id then
				return d
			end
		end
		return nil
	end
	local first: layout.Divider = find((frames.frames:peek()).dividers)
		or { id = id, axis = "row", x = 0, y = 0, w = 0, h = 0, length = 0 }
	local row = first.axis == "row"
	ui.effect(function()
		local f = frames.frames:get()
		local d = find(f.dividers)
		if element and d then
			frames.place(element, d, f.smooth)
		end
	end)
	local start: { at: number, ratio: number, length: number }? = nil
	return ui.div {
		absolute = true,
		left = first.x,
		top = first.y,
		w = first.w,
		h = first.h,
		on_mount = function(el)
			element = el
			local d = find((frames.frames:peek()).dividers)
			if d then
				frames.place(el, d, false)
			end
		end,
		rounded = 3,
		cursor = if row then "col_resize" else "row_resize",
		bg = theme.clear,
		hover = { bg = "#ffffff12" },
		on_press = function(e)
			local d = find((frames.frames:peek()).dividers)
			if d then
				start = { at = if row then e.x else e.y, ratio = (ratios:peek())[id] or 0.5, length = d.length }
			end
		end,
		on_pointer_move = function(e)
			local s = start
			if not s or s.length <= 0 then
				return
			end
			local moved = (if row then e.x else e.y) - s.at
			local least = math.min(0.45, 200 / s.length)
			ratios:set_key(id, math.clamp(s.ratio + moved / s.length, least, 1 - least))
		end,
		on_release = function()
			start = nil
		end,
	}
end)
