-- The dashboard's state: the rows (a plain table, changed in place by the
-- feed), and what the user chose.
local data = require("@lib/data")

export type SortKey = "symbol" | "sector" | "price" | "change" | "volume"
export type Appearance = "dark" | "light"

local M = {}
M.rows = data.rows(100_000)
M.series = data.series(120)
-- Totals the charts show, kept as the feed moves rows rather than summed
-- over all 100,000 each tick.
local volumes: { [string]: number } = {}
local value = 0
for _, row in M.rows do
	volumes[row.sector] = (volumes[row.sector] or 0) + row.volume
	value += row.price * row.volume
end
M.volumes = ui.state("volumes", volumes)
M.value = ui.state("value", value)
-- Bumped each time the feed changes rows: what reads it follows the feed.
M.tick = ui.state("tick", 0)
M.live = ui.state("live", true)
M.query = ui.state("query", "")
M.sector = ui.state("sector", "All")
M.sort = ui.state("sort", { key = "symbol" :: SortKey, descending = false })
M.appearance = ui.persist("appearance", "dark" :: Appearance)
M.selected = ui.state("selected", nil :: number?)
-- The sidebar's width, which its divider drags (kept across launches).
M.sidebar = ui.persist("sidebar", 360)
return M
