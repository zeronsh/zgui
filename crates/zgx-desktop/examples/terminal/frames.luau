-- Where the active tab's panes go. The panes are cards placed absolutely,
-- one per pane for as long as it is in the tab, at frames worked out from
-- its split tree. A change of structure (a split, a close, a drop, a drag's
-- preview) eases every card to its new frame; a divider drag moves them at
-- once.
--
-- The frames do not depend on the content area's size: each edge is a
-- percentage of it plus points (layout.Edge), which zgx resolves itself.
-- Resizing the area (the window, the sidebar opening) runs no script: this
-- works out frames only when the structure or a ratio changes.
local theme = require("./theme")
local state = require("./state")
local actions = require("./actions")
local layout = require("@lib/layout")

local active, zoom, drop = state.active, state.zoom, state.drop

local M = {}

-- The frames, with the panes in reading order and whether a change of
-- shape should ease them there.
export type Shown = layout.Frames & { order: { string }, smooth: boolean }

function M.radius(): number
	return ui.theme.get("terminal.pane_radius") :: number
end

local function live_root(id: string): layout.Tree?
	for _, tab in state.tabs:get() do
		if tab.id == id then
			return tab.root
		end
	end
	return nil
end
M.live_root = live_root

local function axis_of(zone: state.Zone?): layout.Axis
	return if zone == "left" or zone == "right" then "row" else "column"
end

-- The tree a drag hovering a pane's edge would make, shown while it hovers.
local function previewed(root: layout.Tree): layout.Tree
	local d = drop:get()
	local target, id = d.target, d.id
	if not target or not id or not layout.contains(root, target) then
		return root
	end
	local axis = axis_of(d.zone)
	local before = d.zone == "left" or d.zone == "top"
	if d.kind == "pane" then
		local base: layout.Tree? = root
		if layout.contains(root, id) then
			base = layout.remove(root, id)
		end
		if not base or not layout.contains(base, target) then
			return root
		end
		return layout.split(base, target, axis, id, "preview", before)
	end
	local other = live_root(id)
	if not other then
		return root
	end
	return layout.attach(root, target, axis, other, "preview", before)
end

local last_shape: string? = nil
M.frames = ui.computed(function(): Shown
	local id = active:get()
	local root = live_root(id)
	if not root then
		return { panes = {}, dividers = {}, order = {}, smooth = false }
	end
	local shown = previewed(root)
	local zoomed = (zoom:get())[id]
	local out: Shown
	if zoomed and layout.contains(shown, zoomed) and not (drop:get()).target then
		out = {
			panes = { [zoomed] = layout.WHOLE },
			dividers = {},
			order = { zoomed },
			smooth = false,
		}
	else
		local placed = layout.frames(shown, state.ratios:get(), theme.gap)
		out = { panes = placed.panes, dividers = placed.dividers, order = layout.leaves(shown), smooth = false }
	end
	-- A new shape eases; the same shape resized follows at once.
	local shape = `{id}#{layout.signature(shown)}#{zoomed or ""}`
	out.smooth = last_shape ~= nil and shape ~= last_shape and last_shape:match("^[^#]+") == id
	last_shape = shape
	return out
end)
M.pane_ids = ui.computed(function()
	return (M.frames:get()).order
end)
-- The panes kept mounted: the active tab's shown ones, and every plugin
-- panel wherever it is (hidden while its tab is not shown, or another pane
-- is zoomed), so a panel is where it was left when its tab comes back, as a
-- terminal's shell is. A hidden one costs no layout or drawing.
M.mounted_ids = ui.computed(function(): { string }
	local out: { string } = {}
	local seen: { [string]: boolean } = {}
	for _, id in (M.frames:get()).order do
		table.insert(out, id)
		seen[id] = true
	end
	local kinds = state.kinds:get()
	for _, tab in state.tabs:get() do
		for _, id in layout.leaves(tab.root) do
			local kind = kinds[id]
			if kind and kind ~= "browser" and not seen[id] then
				table.insert(out, id)
				seen[id] = true
			end
		end
	end
	return out
end)
M.divider_ids = ui.computed(function()
	local ids: { string } = {}
	for _, d in (M.frames:get()).dividers do
		table.insert(ids, d.id)
	end
	return ids
end)

-- The content area's element (content.luau), whose box the frames divide.
local area_element: Element? = nil
function M.set_area(element: Element)
	area_element = element
end
-- Each pane's card (pane.luau), for a picture of it (a dragged tab's).
local pane_elements: { [string]: Element } = {}
function M.set_pane_element(id: string, element: Element)
	pane_elements[id] = element
end
function M.pane_element(id: string): Element?
	local el = pane_elements[id]
	return if el and el.mounted then el else nil
end
-- Where an element shows now, in the window (nothing while unmounted).
local function shown(el: Element?): layout.Rect
	if not el then
		return { x = 0, y = 0, w = 0, h = 0 }
	end
	return { x = el.x or 0, y = el.y or 0, w = el.width or 0, h = el.height or 0 }
end
M.shown = shown
-- The content area's size now (read when needed, not followed).
function M.area(): { w: number, h: number }
	local r = shown(area_element)
	return { w = r.w, h = r.h }
end

local function percent(p: number): string
	return `{p * 100}%`
end
-- A box as a node's insets: a percentage of the content area from each side
-- plus points (margins), for zgx to place at every size.
export type Insets = {
	left: string,
	ml: number,
	right: string,
	mr: number,
	top: string,
	mt: number,
	bottom: string,
	mb: number,
}
function M.insets(box: layout.Box): Insets
	return {
		left = percent(box.left.p),
		ml = box.left.q,
		right = percent(1 - box.right.p),
		mr = -box.right.q,
		top = percent(box.top.p),
		mt = box.top.q,
		bottom = percent(1 - box.bottom.p),
		mb = -box.bottom.q,
	}
end
-- `box()`'s insets, read when the frames change (the whole area until
-- there is a box).
function M.insets_of(box: () -> layout.Box?): () -> Insets
	return function()
		return M.insets(box() or layout.WHOLE)
	end
end

local EASED = { "left", "top", "width", "height", "marginLeft", "marginTop" }
-- On a change of structure, ease `element` from where it shows to its new
-- box, then leave it to its insets again (zgx follows the area from there).
function M.follow(element: () -> Element?, box: () -> layout.Box?)
	ui.effect(function()
		local f = M.frames:get()
		local b = box()
		local el = element()
		if not (f.smooth and b and el and area_element) then
			return
		end
		local area, from = shown(area_element), shown(el)
		-- Not laid out yet (it arrives with this change): it fades in
		-- where it goes instead (pane.luau).
		if from.w <= 0 and from.h <= 0 then
			return
		end
		local to = layout.resolve(b, area.w, area.h)
		anime.animate(el, {
			left = { from.x - area.x, to.x },
			top = { from.y - area.y, to.y },
			width = { from.w, to.w },
			height = { from.h, to.h },
			marginLeft = 0,
			marginTop = 0,
			duration = 280,
			ease = "outQuint",
			onComplete = function()
				anime.utils.cleanInlineStyles(el, EASED)
			end,
		})
	end)
end

-- The pane under a point of the content area, by the committed layout (not
-- a preview's), and the edge of it the point is nearest.
function M.target_at(x: number, y: number): (string?, state.Zone?)
	local id = active:peek()
	local root = actions.root_of(id)
	if not root then
		return nil, nil
	end
	local size = M.area()
	local zoomed = (zoom:peek())[id]
	local placed = if zoomed and layout.contains(root, zoomed)
		then { [zoomed] = layout.WHOLE }
		else layout.frames(root, state.ratios:peek(), theme.gap).panes
	for pane, box in placed do
		local r = layout.resolve(box, size.w, size.h)
		if x >= r.x and x < r.x + r.w and y >= r.y and y < r.y + r.h then
			local fx, fy = (x - r.x) / math.max(r.w, 1), (y - r.y) / math.max(r.h, 1)
			local zone: state.Zone, best = "right", 1 - fx
			if fx < best then
				zone, best = "left", fx
			end
			if fy < best then
				zone, best = "top", fy
			end
			if 1 - fy < best then
				zone = "bottom"
			end
			return pane, zone
		end
	end
	return nil, nil
end

-- Commit what the drop preview showed.
function M.commit_drop(d: state.Drop)
	local tab = active:peek()
	local root = actions.root_of(tab)
	local target, id, zone = d.target, d.id, d.zone
	if not target or not id or not zone or not root or not layout.contains(root, target) then
		return
	end
	if d.kind == "pane" then
		actions.move_pane(id, target, zone)
		return
	end
	-- A whole tab: its panes join this one beside the target.
	local other = actions.root_of(id)
	if not other or id == tab then
		return
	end
	state.tabs:remove(actions.tab_index(id) or 0)
	state.focus:set_key(id, nil)
	actions.set_root(
		tab,
		layout.attach(root, target, axis_of(zone), other, actions.fresh("s"), zone == "left" or zone == "top")
	)
	actions.focus_pane(tab, target)
end

return M
