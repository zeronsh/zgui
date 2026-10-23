-- The live feed and what the header's controls do.
local state = require("./state")
local data = require("@lib/data")

local M = {}

-- Prices move and shares trade: a few hundred rows a tick, the totals by
-- what changed, and the series gets a point.
function M.feed()
	local rows = state.rows
	local volumes = table.clone(state.volumes:peek())
	local value = state.value:peek()
	for _ = 1, 300 do
		local row = rows[math.floor(data.random() * #rows) + 1]
		local moved = row.price * (1 + (data.random() - 0.5) * 0.01)
		local traded = math.floor(data.random() * 20_000)
		value += moved * (row.volume + traded) - row.price * row.volume
		volumes[row.sector] += traded
		row.change += (moved - row.price) / row.price * 100
		row.price = moved
		row.volume += traded
		table.remove(row.history, 1)
		table.insert(row.history, moved)
	end
	state.volumes:set(volumes)
	state.value:set(value)
	local series = state.series
	table.remove(series, 1)
	table.insert(series, math.max(10, series[#series] + (data.random() - 0.5) * 6))
	state.tick:set(state.tick:peek() + 1)
end

function M.sort_by(key: state.SortKey)
	local sort = state.sort:peek()
	state.sort:set { key = key, descending = if sort.key == key then not sort.descending else false }
end

return M
