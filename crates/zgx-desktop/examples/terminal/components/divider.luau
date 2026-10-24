-- A divider between two panes: dragging it moves their split.
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
	local function box(): layout.Box?
		return find((frames.frames:get()).dividers)
	end
	local insets = frames.insets_of(box)
	local first = find((frames.frames:peek()).dividers)
	local row = first == nil or first.axis == "row"
	-- A change of structure eases it to its new place.
	frames.follow(function()
		return element
	end, box)
	local start: { at: number, ratio: number, length: number }? = nil
	return ui.div {
		-- Placed by zgx at its frame's insets, at every size of the area.
		absolute = true,
		-- Its edges on whole points: crisp borders at any ratio.
		snap = true,
		left = function()
			return insets().left
		end,
		ml = function()
			return insets().ml
		end,
		right = function()
			return insets().right
		end,
		mr = function()
			return insets().mr
		end,
		top = function()
			return insets().top
		end,
		mt = function()
			return insets().mt
		end,
		bottom = function()
			return insets().bottom
		end,
		mb = function()
			return insets().mb
		end,
		on_mount = function(el)
			element = el
		end,
		rounded = "$radius.full",
		cursor = if row then "col_resize" else "row_resize",
		bg = theme.clear,
		hover = { bg = "$terminal.key.bg" },
		on_press = function(e)
			local d = find((frames.frames:peek()).dividers)
			if d then
				-- The length the split shares out, in points now.
				local area = frames.area()
				local length = layout.at(d.length, if row then area.w else area.h)
				start = { at = if row then e.x else e.y, ratio = (ratios:peek())[id] or 0.5, length = length }
			end
		end,
		on_pointer_move = function(e)
			local s = start
			if not s or s.length <= 0 then
				return
			end
			local moved = (if row then e.x else e.y) - s.at
			local least = math.min(0.45, 160 / s.length)
			ratios:set_key(id, math.clamp(s.ratio + moved / s.length, least, 1 - least))
		end,
		on_release = function()
			start = nil
		end,
	}
end)
