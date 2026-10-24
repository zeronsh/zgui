-- Where the panels go. They are cards placed absolutely, one per panel for
-- as long as it is open, at frames worked out from the split tree, as the
-- terminal example places its panes. A change of structure (a split, a
-- close, a drop, a drag's preview) eases every card to its new frame; a
-- divider drag or a window resize moves them at once.
local theme = require("./theme")
local state = require("./state")
local actions = require("./actions")
local layout = require("@lib/layout")

local root, zoom, drop, area = state.root, state.zoom, state.drop, state.area

local M = {}

-- The frames, with the panels in reading order and whether a change of
-- shape should ease them there.
export type Shown = layout.Frames & { order: { string }, smooth: boolean }

function M.radius(): number
	return math.max(state.window_radius:get() - theme.inset, 6)
end

-- The tree a drag over a panel would make, shown while it hovers: a panel
-- moved beside the target or swapped with it, or a process in a panel of
-- its own beside the target.
local function previewed(tree: layout.Tree): layout.Tree
	local d = drop:get()
	local target, id = d.target, d.id
	if not target or not id or not layout.contains(tree, target) then
		return tree
	end
	if d.zone == "center" then
		if d.kind == "panel" and id ~= target and layout.contains(tree, id) then
			return layout.swap(tree, id, target)
		end
		return tree
	end
	local axis, before = actions.axis_of(d.zone), actions.before(d.zone)
	if d.kind == "panel" then
		local base: layout.Tree? = tree
		if layout.contains(tree, id) then
			base = layout.remove(tree, id)
		end
		if not base or not layout.contains(base, target) then
			return tree
		end
		return layout.split(base, target, axis, id, "preview", before)
	end
	return layout.split(tree, target, axis, id, "preview", before)
end

local last_shape: string? = nil
M.frames = ui.computed(function(): Shown
	local tree = root:get()
	local size = area:get()
	if not tree then
		last_shape = nil
		return { panes = {}, dividers = {}, order = {}, smooth = false }
	end
	local shown = previewed(tree)
	local zoomed = zoom:get()
	local out: Shown
	if zoomed ~= "" and layout.contains(shown, zoomed) and not (drop:get()).target then
		local pane: string = zoomed
		out = {
			panes = { [pane] = { x = 0, y = 0, w = size.w, h = size.h } },
			dividers = {},
			order = { pane },
			smooth = false,
		}
	else
		zoomed = ""
		local placed = layout.frames(shown, state.ratios:get(), size.w, size.h, theme.gap)
		out = { panes = placed.panes, dividers = placed.dividers, order = layout.leaves(shown), smooth = false }
	end
	-- A new shape eases; the same shape resized follows at once.
	local shape = `{layout.signature(shown)}#{zoomed}`
	out.smooth = last_shape ~= nil and shape ~= last_shape
	last_shape = shape
	return out
end)
M.pane_ids = ui.computed(function()
	return (M.frames:get()).order
end)
M.divider_ids = ui.computed(function()
	local ids: { string } = {}
	for _, d in (M.frames:get()).dividers do
		table.insert(ids, d.id)
	end
	return ids
end)

-- Ease (or set) an element's box.
function M.place(element: Element, r: layout.Rect, smooth: boolean)
	anime.animate(element, {
		left = r.x,
		top = r.y,
		width = r.w,
		height = r.h,
		duration = if smooth then 300 else 0,
		ease = "outQuint",
	})
end

-- The panel under a point of the area, by the open layout (not a
-- preview's), and where on it the point is: near an edge, that edge; well
-- inside, its middle. A process can only take a full layout's middle.
function M.target_at(x: number, y: number, kind: state.Kind): (string?, state.Zone?)
	local tree = root:peek()
	if not tree then
		return nil, nil
	end
	local size = area:peek()
	local zoomed = zoom:peek()
	local placed = if zoomed ~= "" and layout.contains(tree, zoomed)
		then { [zoomed] = { x = 0, y = 0, w = size.w, h = size.h } }
		else layout.frames(tree, state.ratios:peek(), size.w, size.h, theme.gap).panes
	for pane, r in placed do
		if x >= r.x and x < r.x + r.w and y >= r.y and y < r.y + r.h then
			if kind == "process" and actions.full() then
				return pane, "center"
			end
			local fx, fy = (x - r.x) / math.max(r.w, 1), (y - r.y) / math.max(r.h, 1)
			local zone: state.Zone, best = "right", 1 - fx
			if fx < best then
				zone, best = "left", fx
			end
			if fy < best then
				zone, best = "top", fy
			end
			if 1 - fy < best then
				zone, best = "bottom", 1 - fy
			end
			if best > 0.25 then
				zone = "center"
			end
			return pane, zone
		end
	end
	return nil, nil
end

return M
