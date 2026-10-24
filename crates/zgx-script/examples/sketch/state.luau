-- The drawing and what the user is doing with it.
local geometry = require("@lib/geometry")
local snap = require("@lib/snap")

export type Tool = "select" | "rect" | "ellipse"

local M = {}
-- The shapes, bottom to top.
M.shapes = ui.state("shapes", {} :: { geometry.Shape })
M.selected = ui.state("selected", {} :: { string })
M.tool = ui.state("tool", "select" :: Tool)
-- Where the world's origin is on the board, and the zoom.
M.view = ui.state("view", { x = 80, y = 60, zoom = 1 } :: geometry.View)
M.snapping = ui.persist("snapping", true)
-- What a gesture shows as it goes: guide lines, the selection rectangle.
M.guides = ui.state("guides", {} :: { snap.Guide })
M.marquee = ui.state("marquee", nil :: geometry.Rect?)
-- The file it was opened from or saved to, and whether it changed since.
M.file = ui.state("file", nil :: File?)
M.dirty = ui.state("dirty", false)
M.inspector = ui.persist("inspector", 260)
-- The last message about a file (saved, failed to open).
M.status = ui.state("status", "")
-- The board's size, for zooming about its middle.
M.board = { w = 800, h = 600 }
-- Ids made so far, for the next one.
M.next_id = 1
return M
