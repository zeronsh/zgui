-- Snapping a moving edge to the others' edges and centres, then to the
-- grid: what a guide line shows.
local M = {}

export type Guide = { axis: "x" | "y", at: number }

M.GRID = 8

function M.guide(axis: "x" | "y", at: number): Guide
	return { axis = axis, at = at }
end

-- The lines along one axis that `others` offer: each one's start, middle
-- and end.
function M.lines(others: { { x: number, y: number, w: number, h: number } }, axis: "x" | "y"): { number }
	local out = {}
	for _, r in others do
		local start, size = if axis == "x" then r.x else r.y, if axis == "x" then r.w else r.h
		table.insert(out, start)
		table.insert(out, start + size / 2)
		table.insert(out, start + size)
	end
	return out
end

-- How far to shift `edges` (positions along one axis) so the closest one
-- lands on a line within `within`, and the line; else onto the grid when
-- `grid` (the first edge), and no line.
function M.shift(edges: { number }, lines: { number }, within: number, grid: boolean): (number, number?)
	local best, line = math.huge, nil :: number?
	for _, e in edges do
		for _, l in lines do
			local d = l - e
			if math.abs(d) < math.abs(best) and math.abs(d) <= within then
				best, line = d, l
			end
		end
	end
	if line then
		return best, line
	end
	if grid and #edges > 0 then
		local e = edges[1]
		return math.floor(e / M.GRID + 0.5) * M.GRID - e, nil
	end
	return 0, nil
end

return M
