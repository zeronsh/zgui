-- The app's state: every module that requires this one gets the same
-- States, and all of it survives a reload.
local layout = require("@lib/layout")

export type Sort = "cpu" | "gpu" | "memory" | "name"
-- A panel's edge a drag would put something against, or its middle (a
-- process shows there; a panel swaps with it).
export type Zone = "left" | "right" | "top" | "bottom" | "center"
-- What is dragged: a panel by its header, or a process from the sidebar.
export type Kind = "panel" | "process"
-- Where a drag would land: `{ target, zone, kind, id, pid }`, or {}. `id`
-- is the dragged panel, or the panel a dropped process would open as.
export type Drop = { target: string?, zone: Zone?, kind: Kind?, id: string?, pid: number? }
export type Dragging = { kind: Kind?, id: string?, pid: number? }
-- A drag's payload.
export type Payload = { kind: Kind, id: string?, pid: number? }

local M = {}

-- What the host (zgx_monitor.rs) publishes.
M.launched = ui.shared("launched", nil :: number?):peek()
M.error = ui.shared("error", "")
-- Milliseconds between samples, and how many a history keeps.
M.interval = ui.shared("interval", 500):peek()
M.capacity = ui.shared("capacity", 120):peek()
M.window_radius = ui.shared("window_radius", 10)

M.next_id = ui.state("next_id", 1)
-- Whether the first panel has opened (once per launch, not per reload).
M.started = ui.state("started", false)
-- The panels, as a split tree; nil while there are none.
M.root = ui.state("root", nil :: layout.Tree?)
-- Panel id -> the process it shows (none: an empty panel).
M.pids = ui.state("pids", {} :: { [string]: number })
-- The panel the sidebar and the keys act on.
M.active = ui.state("active", "")
-- Split id -> the first side's share (0 to 1).
M.ratios = ui.state("ratios", {} :: { [string]: number })
-- The panel shown alone, or "".
M.zoom = ui.state("zoom", "")
M.drop = ui.state("drop", {} :: Drop)
M.dragging = ui.state("dragging", {} :: Dragging)
-- The panels' area's size.
M.area = ui.state("area", { w = 0, h = 0 })

M.query = ui.state("query", "")
M.sort = ui.persist("sort", "cpu" :: Sort)
-- The sidebar's width when open, whether it is, and the width it shows
-- now (easing between the two as it opens and closes).
M.sidebar_w = ui.persist("sidebar_w", 280)
M.sidebar_open = ui.persist("sidebar_open", true)
M.sidebar_shown = ui.state("sidebar_shown", if M.sidebar_open:peek() then M.sidebar_w:peek() else 0)
-- The filter field, and the window (which takes the keys while the
-- sidebar is closed).
M.search = ui.ref()
M.window = ui.ref()
-- The process list's scroll position and height, and whether the pointer
-- is on it (its order holds still under the pointer).
M.list_offset = ui.state("list_offset", 0)
M.list_h = ui.state("list_h", 0)
M.list_hover = ui.state("list_hover", false)

return M
