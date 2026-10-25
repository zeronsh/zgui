-- The instruments: sortable by any column, filtered by the search and the
-- sector, 100,000 rows in a virtual list, each with a sparkline.
local state = require("../state")
local actions = require("../actions")
local data = require("@lib/data")

-- The rows the search and sector let through, in the order the header
-- chose: kept in order as the feed writes rows (those re-placed alone),
-- filtered and sorted again only when the search, sector or order change.
local view: State<{ data.Row }> = ui.sorted(state.rows, function()
	local sort = state.sort:get()
	local query = string.upper(state.query:get())
	local sector = state.sector:get()
	local filter: ((row: data.Row) -> boolean)? = nil
	if query ~= "" or sector ~= "All" then
		filter = function(row: data.Row): boolean
			return (sector == "All" or row.sector == sector)
				and (query == "" or string.find(row.symbol, query, 1, true) ~= nil)
		end
	end
	return { by = sort.key, descending = sort.descending, filter = filter }
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

-- Where a row's recent prices went, as its sparkline shows them.
local function trend(id: number): string
	local history = state.history[id]
	local first, last = history[1], history[#history]
	local low, high = math.huge, -math.huge
	for _, v in history do
		low = math.min(low, v)
		high = math.max(high, v)
	end
	local way = if last > first then "rising" elseif last < first then "falling" else "flat"
	return string.format("%s, low %.2f, high %.2f", way, low, high)
end

local function price(d: data.Row): string
	return ui.number(d.price, { decimals = 2 })
end
local function change(d: data.Row): string
	return ui.number(d.change / 100, { style = "percent", decimals = 2, sign = "except_zero" })
end
local function volume(d: data.Row): string
	return ui.number(d.volume, { notation = "compact" })
end

local function row(i: number): Node
	local function data()
		return view:at(i)
	end
	return ui.row {
		id = `row-{i}`,
		-- An instrument to choose: read as what its cells and sparkline show.
		role = "option",
		selected = function()
			local d = data()
			return d ~= nil and state.selected:get() == d.id
		end,
		label = function()
			local d = data()
			if not d then
				return ""
			end
			-- Worked out only while a screen reader reads the window.
			return `{d.symbol}, {d.sector}, {price(d)}, {change(d)}, volume {volume(d)}, trend {trend(d.id)}`
		end,
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
				return if d then price(d) else ""
			end,
			w = 100,
			align = "end",
		},
		ui.text {
			id = `change-{i}`,
			function()
				local d = data()
				return if d then change(d) else ""
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
				return if d then volume(d) else ""
			end,
			w = 90,
			align = "end",
		},
		ui.canvas {
			w = 80,
			h = 20,
			draw = function(c, w, h)
				-- The row (read, so its write redraws this) and its prices.
				local d = data()
				if not d then
					return
				end
				local history = state.history[d.id]
				local low, high = math.huge, -math.huge
				for _, v in history do
					low = math.min(low, v)
					high = math.max(high, v)
				end
				local span = math.max(high - low, 0.0001)
				local path = ui.path()
				for k, v in history do
					local x = (k - 1) / (#history - 1) * w
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
			role = "listbox",
			label = "Instruments",
			count = function()
				return view:len()
			end,
			height = 32,
			row = row,
			scrollbar = true,
			w_full = true,
			grow = 1,
		},
	}
end
