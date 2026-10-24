-- Markets: a live dashboard. Charts of a price series and each sector's
-- volume beside a sortable, filterable table of 100,000 instruments, with
-- the feed moving prices several times a second.
local state = require("./state")
local actions = require("./actions")
local Charts = require("@components/charts")
local Table = require("@components/table")
local data = require("@lib/data")

ui.effect(function()
	ui.theme.set(if state.appearance:get() == "dark" then "zgx-dark" else "zgx-light")
end)

ui.command {
	id = "refresh",
	title = "Refresh Data",
	menu = "View",
	key = "cmd+r",
	run = actions.feed,
}

-- The feed ticks while live; paused, no timer runs.
local stop_feed: (() -> ())? = nil
ui.effect(function()
	local live = state.live:get()
	if live and not stop_feed then
		stop_feed = ui.every(250, actions.feed)
	elseif not live and stop_feed then
		stop_feed()
		stop_feed = nil
	end
end)

local sectors = { "All" }
for _, s in data.SECTORS do
	table.insert(sectors, s)
end

local function toolbar(): Node
	local chips: { Child } = {}
	for _, s in sectors do
		table.insert(
			chips,
			ui.button {
				id = `sector-{s}`,
				on_click = ui.set(state.sector, s),
				bg = function()
					return if state.sector:get() == s then "$accent" else "$surface.raised"
				end,
				s,
			}
		)
	end
	return ui.row {
		gap = 8,
		p = 12,
		items = "center",
		ui.text { "Markets", text_size = "$font.size.xl", bold = true },
		ui.input { id = "search", label = "Search", placeholder = "Search symbols", value = state.query, w = 220 },
		ui.row { gap = 4, chips },
		ui.div { grow = 1 },
		ui.checkbox { id = "live", label = "Live", value = state.live },
		ui.button {
			id = "appearance",
			on_click = function()
				state.appearance:set(if state.appearance:peek() == "dark" then "light" else "dark")
			end,
			function()
				return if state.appearance:get() == "dark" then "Light" else "Dark"
			end,
		},
	}
end

return ui.column {
	w_full = true,
	h_full = true,
	bg = "$surface.base",
	toolbar(),
	ui.split {
		id = "panes",
		size = state.sidebar,
		min = 240,
		max = 640,
		min_rest = 420,
		grow = 1,
		min_h = 0,
		ui.column {
			id = "sidebar",
			p = 12,
			gap = 12,
			Charts.summary(),
			Charts.line(),
			Charts.bars(),
		},
		Table(),
	},
}
