-- The instruments: sortable by any column, filtered by the search and the
-- sector, 100,000 rows in a virtual list, each with a sparkline.
local state = require("../state")
local actions = require("../actions")

-- The rows the search and sector let through: indices into state.rows.
local matching = ui.computed(function()
	local query = string.upper(state.query:get())
	local sector = state.sector:get()
	local shown = {}
	for i, row in state.rows do
		if (sector == "All" or row.sector == sector) and (query == "" or string.find(row.symbol, query, 1, true)) then
			table.insert(shown, i)
		end
	end
	return shown
end)

-- Those, in order. Sorted by a live column, the feed's ticks sort again;
-- the filter above doesn't run again for them.
local view = ui.computed(function()
	local sort = state.sort:get()
	if sort.key == "price" or sort.key == "change" or sort.key == "volume" then
		state.tick:get()
	end
	local rows, key = state.rows, sort.key
	return ui.sort(table.clone(matching:get()), function(i: number)
		return (rows[i] :: any)[key]
	end, sort.descending)
end)

local COLUMNS = {
	{ key = "symbol", label = "Symbol", w = 90 },
	{ key = "sector", label = "Sector", w = 90 },
	{ key = "price", label = "Price", w = 100 },
	{ key = "change", label = "Change", w = 90 },
	{ key = "volume", label = "Volume", w = 90 },
}

local function header(): Node
	local cells: { Child } = {}
	for _, column in COLUMNS do
		table.insert(
			cells,
			ui.button {
				id = `sort-{column.key}`,
				w = column.w,
				on_click = function()
					actions.sort_by(column.key :: any)
				end,
				function()
					local sort = state.sort:get()
					if sort.key == column.key then
						return column.label .. (if sort.descending then " ↓" else " ↑")
					end
					return column.label
				end,
			}
		)
	end
	table.insert(cells, ui.text { "Trend", w = 80, color = "$text.secondary" })
	return ui.row { gap = 4, px = 8, py = 6, items = "center", cells }
end

local function row(i: number): Node
	local function data()
		state.tick:get()
		local index = view:get()[i]
		return if index then state.rows[index] else nil
	end
	return ui.row {
		id = `row-{i}`,
		w_full = true,
		h = 32,
		px = 8,
		gap = 4,
		items = "center",
		bg = function()
			local d = data()
			return if d and state.selected:get() == d.id then "$selection.item" else "#00000000"
		end,
		on_click = function()
			local d = data()
			if d then
				state.selected:set(d.id)
			end
		end,
		ui.text {
			id = `symbol-{i}`,
			function()
				local d = data()
				return if d then d.symbol else ""
			end,
			w = 90,
			bold = true,
		},
		ui.text {
			id = `sector-{i}`,
			function()
				local d = data()
				return if d then d.sector else ""
			end,
			w = 90,
			color = "$text.secondary",
		},
		ui.text {
			id = `price-{i}`,
			function()
				local d = data()
				return if d then ui.number(d.price, { decimals = 2 }) else ""
			end,
			w = 100,
			align = "end",
		},
		ui.text {
			id = `change-{i}`,
			function()
				local d = data()
				return if d
					then ui.number(d.change / 100, { style = "percent", decimals = 2, sign = "except_zero" })
					else ""
			end,
			w = 90,
			align = "end",
			color = function()
				local d = data()
				return if d and d.change < 0 then "#e5534b" else "#3fb950"
			end,
		},
		ui.text {
			id = `volume-{i}`,
			function()
				local d = data()
				return if d then ui.number(d.volume, { notation = "compact" }) else ""
			end,
			w = 90,
			align = "end",
		},
		ui.canvas {
			w = 80,
			h = 20,
			draw = function(c, w, h)
				local d = data()
				if not d then
					return
				end
				local low, high = math.huge, -math.huge
				for _, v in d.history do
					low = math.min(low, v)
					high = math.max(high, v)
				end
				local span = math.max(high - low, 0.0001)
				local path = ui.path()
				for k, v in d.history do
					local x = (k - 1) / (#d.history - 1) * w
					local y = h - (v - low) / span * h
					path = if k == 1 then path:move(x, y) else path:line(x, y)
				end
				c:stroke(path, if d.change < 0 then "#e5534b" else "#3fb950", 1.5)
			end,
		},
	}
end

return function(): Node
	return ui.column {
		grow = 1,
		h_full = true,
		header(),
		ui.list {
			id = "rows",
			count = function()
				return #view:get()
			end,
			height = 32,
			row = row,
			scrollbar = true,
			w_full = true,
			grow = 1,
		},
	}
end
