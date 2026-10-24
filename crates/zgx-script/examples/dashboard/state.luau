-- The dashboard's state: the rows (a list state the feed writes a few
-- hundred rows of a tick), and what the user chose. Each row's recent
-- prices, which only its sparkline draws, are a plain table by id: state
-- holds what drives updates, not every payload shown (the moved row's
-- write already redraws its sparkline).
local data = require("@lib/data")

export type SortKey = "symbol" | "sector" | "price" | "change" | "volume"
export type Appearance = "dark" | "light"

local M = {}
-- `zgx-run dashboard 1000`: fewer rows (a benchmark's).
local rows, histories = data.rows(tonumber(ui.args[1]) or 100_000)
M.history = histories
M.series = data.series(120)
-- Totals the charts show, kept as the feed moves rows rather than summed
-- over all 100,000 each tick.
local volumes: { [string]: number } = {}
local value = 0
for _, row in rows do
	volumes[row.sector] = (volumes[row.sector] or 0) + row.volume
	value += row.price * row.volume
end
M.rows = ui.state("rows", rows)
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
