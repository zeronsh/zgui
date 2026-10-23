-- Shapes and the rectangles around them, in world coordinates (points at
-- 100%); the view maps them to the screen.
export type Kind = "rect" | "ellipse"
export type Shape = {
	id: string,
	kind: Kind,
	name: string,
	x: number,
	y: number,
	w: number,
	h: number,
	fill: string,
	stroke: string,
	stroke_width: number,
	radius: number,
}
export type Rect = { x: number, y: number, w: number, h: number }
export type Handle = "nw" | "n" | "ne" | "e" | "se" | "s" | "sw" | "w"
export type View = { x: number, y: number, zoom: number }

local M = {}

M.HANDLES = { "nw", "n", "ne", "e", "se", "s", "sw", "w" } :: { Handle }

function M.to_world(view: View, sx: number, sy: number): (number, number)
	return (sx - view.x) / view.zoom, (sy - view.y) / view.zoom
end

function M.to_screen(view: View, x: number, y: number): (number, number)
	return x * view.zoom + view.x, y * view.zoom + view.y
end

-- The rectangle two corners span, whichever way they were dragged.
function M.span(x1: number, y1: number, x2: number, y2: number): Rect
	return { x = math.min(x1, x2), y = math.min(y1, y2), w = math.abs(x2 - x1), h = math.abs(y2 - y1) }
end

function M.bounds(shapes: { Shape }): Rect?
	if #shapes == 0 then
		return nil
	end
	local x1, y1 = math.huge, math.huge
	local x2, y2 = -math.huge, -math.huge
	for _, s in shapes do
		x1 = math.min(x1, s.x)
		y1 = math.min(y1, s.y)
		x2 = math.max(x2, s.x + s.w)
		y2 = math.max(y2, s.y + s.h)
	end
	return { x = x1, y = y1, w = x2 - x1, h = y2 - y1 }
end

function M.contains(s: Shape, x: number, y: number): boolean
	if s.kind == "ellipse" then
		local rx, ry = s.w / 2, s.h / 2
		if rx <= 0 or ry <= 0 then
			return false
		end
		local dx, dy = (x - s.x - rx) / rx, (y - s.y - ry) / ry
		return dx * dx + dy * dy <= 1
	end
	return x >= s.x and x <= s.x + s.w and y >= s.y and y <= s.y + s.h
end

function M.overlaps(a: Rect, b: Rect): boolean
	return a.x < b.x + b.w and b.x < a.x + a.w and a.y < b.y + b.h and b.y < a.y + a.h
end

-- Where handle `h` of `r` is.
function M.handle_at(r: Rect, h: Handle): (number, number)
	local x = if h == "nw"
			or h == "w"
			or h == "sw"
		then r.x
		elseif h == "n" or h == "s" then r.x + r.w / 2
		else r.x + r.w
	local y = if h == "nw"
			or h == "n"
			or h == "ne"
		then r.y
		elseif h == "w" or h == "e" then r.y + r.h / 2
		else r.y + r.h
	return x, y
end

-- `r` with handle `h` moved to (`x`, `y`), never inside out: dragged past
-- the opposite edge it stops at a point's width.
function M.resize(r: Rect, h: Handle, x: number, y: number): Rect
	local x1, y1, x2, y2 = r.x, r.y, r.x + r.w, r.y + r.h
	if h == "nw" or h == "w" or h == "sw" then
		x1 = math.min(x, x2 - 1)
	elseif h == "ne" or h == "e" or h == "se" then
		x2 = math.max(x, x1 + 1)
	end
	if h == "nw" or h == "n" or h == "ne" then
		y1 = math.min(y, y2 - 1)
	elseif h == "sw" or h == "s" or h == "se" then
		y2 = math.max(y, y1 + 1)
	end
	return { x = x1, y = y1, w = x2 - x1, h = y2 - y1 }
end

M.CURSORS = {
	nw = "nwse-resize",
	se = "nwse-resize",
	ne = "nesw-resize",
	sw = "nesw-resize",
	n = "ns-resize",
	s = "ns-resize",
	e = "ew-resize",
	w = "ew-resize",
} :: { [Handle]: Cursor }

return M
