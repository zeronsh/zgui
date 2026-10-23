-- The series as a line chart, and each sector's volume as bars.
local state = require("../state")
local data = require("@lib/data")

local M = {}

function M.line(): Node
	return ui.canvas {
		w_full = true,
		h = 180,
		draw = function(c, w, h)
			state.tick:get()
			local values = state.series
			local low, high = math.huge, -math.huge
			for _, v in values do
				low = math.min(low, v)
				high = math.max(high, v)
			end
			local span = math.max(high - low, 1)
			c:rect(0, 0, w, h, "$surface.raised", 8)
			local path = ui.path()
			for i, v in values do
				local x = (i - 1) / (#values - 1) * (w - 16) + 8
				local y = h - 12 - (v - low) / span * (h - 24)
				if i == 1 then
					path = path:move(x, y)
				else
					path = path:line(x, y)
				end
			end
			c:stroke(path, "$accent", 2, { join = "round" })
		end,
	}
end

function M.bars(): Node
	return ui.canvas {
		w_full = true,
		h = 160,
		draw = function(c, w, h)
			local totals = state.volumes:get()
			local order = data.SECTORS
			local most = 0
			for _, v in totals do
				most = math.max(most, v)
			end
			c:rect(0, 0, w, h, "$surface.raised", 8)
			local bar = (w - 16) / #order
			for i, sector in order do
				local bh = totals[sector] / most * (h - 24)
				c:rect(8 + (i - 1) * bar + 2, h - 8 - bh, bar - 4, bh, "$accent", 3)
			end
		end,
	}
end

function M.summary(): Node
	return ui.text {
		function()
			return `Market value {ui.number(state.value:get(), { notation = "compact", currency = "$" })}`
		end,
		text_size = "$font.size.lg",
		bold = true,
	}
end

return M
