-- The board: the drawing on a dotted grid, panned and zoomed, and what
-- the pointer does to it: draw a shape, select (click, shift-click, or a
-- rectangle), move, resize by a handle, pan (the wheel, or Space and drag)
-- and zoom (⌘ or Ctrl and the wheel, or a pinch) about the pointer.
local state = require("../state")
local actions = require("../actions")
local history = require("../history")
local geometry = require("@lib/geometry")
local snap = require("@lib/snap")

type Shape = geometry.Shape
type Rect = geometry.Rect

-- Handles, and how near a snap pulls, in screen points.
local HANDLE = 8
local PULL = 6

type Gesture = {
	kind: "move" | "resize" | "draw" | "marquee" | "pan",
	-- Where it started, on the board and in the world.
	sx: number,
	sy: number,
	wx: number,
	wy: number,
	view: geometry.View,
	-- The shapes it moves: their index and where they were.
	moving: { { index: number, x: number, y: number } },
	bounds: Rect?,
	handle: geometry.Handle?,
	index: number?,
	-- Selected before a shift-drag's rectangle, kept in it.
	kept: { string },
}
local gesture: Gesture? = nil
local space = false
local cursor = ui.state("board_cursor", "default" :: Cursor)

local function selected_set(): { [string]: boolean }
	local set = {}
	for _, id in state.selected:peek() do
		set[id] = true
	end
	return set
end

-- The topmost shape at world point (`x`, `y`), and its index.
local function shape_at(x: number, y: number): (number?, Shape?)
	local shapes = state.shapes:peek()
	for i = #shapes, 1, -1 do
		if geometry.contains(shapes[i], x, y) then
			return i, shapes[i]
		end
	end
	return nil, nil
end

-- The handle of the single selected shape under board point (`sx`, `sy`).
local function handle_at(sx: number, sy: number): (geometry.Handle?, number?)
	local ids = state.selected:peek()
	if #ids ~= 1 then
		return nil, nil
	end
	local i, s = actions.find(ids[1])
	if not (i and s) then
		return nil, nil
	end
	local view = state.view:peek()
	for _, h in geometry.HANDLES do
		local hx, hy = geometry.handle_at(s, h)
		local x, y = geometry.to_screen(view, hx, hy)
		if math.abs(sx - x) <= HANDLE and math.abs(sy - y) <= HANDLE then
			return h, i
		end
	end
	return nil, nil
end

-- The lines the shapes not in `moving` offer to snap to.
local function lines(skip: { [string]: boolean }): ({ number }, { number })
	local others = {}
	for _, s in state.shapes:peek() do
		if not skip[s.id] then
			table.insert(others, s)
		end
	end
	return snap.lines(others, "x"), snap.lines(others, "y")
end

-- A world point, snapped (when snapping is on) to the others' lines and
-- then the grid; with the guides it lands on.
local function snap_point(x: number, y: number, skip: { [string]: boolean }): (number, number, { snap.Guide })
	if not state.snapping:peek() then
		return x, y, {}
	end
	local zoom = state.view:peek().zoom
	local xs, ys = lines(skip)
	local dx, gx = snap.shift({ x }, xs, PULL / zoom, true)
	local dy, gy = snap.shift({ y }, ys, PULL / zoom, true)
	local guides: { snap.Guide } = {}
	if gx then
		table.insert(guides, snap.guide("x", gx))
	end
	if gy then
		table.insert(guides, snap.guide("y", gy))
	end
	return x + dx, y + dy, guides
end

local function press(e: PointerEvent)
	local view = state.view:peek()
	local sx, sy = e.local_x, e.local_y
	local wx, wy = geometry.to_world(view, sx, sy)
	local g: Gesture = { kind = "pan", sx = sx, sy = sy, wx = wx, wy = wy, view = view, moving = {}, kept = {} }
	gesture = g
	if space then
		return
	end
	local tool = state.tool:peek()
	if tool ~= "select" then
		history.begin()
		local x, y, guides = snap_point(wx, wy, {})
		g.kind, g.wx, g.wy = "draw", x, y
		local id = actions.add(tool :: geometry.Kind, { x = x, y = y, w = 1, h = 1 })
		g.index = #state.shapes:peek()
		state.selected:set { id }
		state.guides:set(guides)
		return
	end
	local handle, index = handle_at(sx, sy)
	if handle and index then
		history.begin()
		g.kind, g.handle, g.index = "resize", handle, index
		local s = state.shapes:peek()[index]
		g.bounds = { x = s.x, y = s.y, w = s.w, h = s.h }
		return
	end
	local _, hit = shape_at(wx, wy)
	if hit then
		local set = selected_set()
		if e.shift then
			-- Shift-click adds or takes away, and moves nothing.
			local ids = {}
			for _, id in state.selected:peek() do
				if id ~= hit.id then
					table.insert(ids, id)
				end
			end
			if not set[hit.id] then
				table.insert(ids, hit.id)
			end
			state.selected:set(ids)
			gesture = nil
			return
		end
		if not set[hit.id] then
			state.selected:set { hit.id }
			set = { [hit.id] = true }
		end
		history.begin()
		g.kind = "move"
		local chosen = {}
		for i, s in state.shapes:peek() do
			if set[s.id] then
				table.insert(g.moving, { index = i, x = s.x, y = s.y })
				table.insert(chosen, s)
			end
		end
		g.bounds = geometry.bounds(chosen)
		return
	end
	g.kind = "marquee"
	if e.shift then
		g.kept = state.selected:peek()
	else
		state.selected:set {}
	end
end

local function move(e: PointerEvent)
	local g = gesture
	if not g then
		-- Hovering: a handle shows how it resizes.
		local handle = handle_at(e.local_x, e.local_y)
		cursor:set(if space then "grab" elseif handle then geometry.CURSORS[handle] else "default")
		return
	end
	local view = state.view:peek()
	local wx, wy = geometry.to_world(view, e.local_x, e.local_y)
	if g.kind == "pan" then
		state.view:set { x = g.view.x + e.local_x - g.sx, y = g.view.y + e.local_y - g.sy, zoom = view.zoom }
	elseif g.kind == "move" and g.bounds then
		local b = g.bounds
		local dx, dy = wx - g.wx, wy - g.wy
		local guides: { snap.Guide } = {}
		if state.snapping:peek() then
			local xs, ys = lines(selected_set())
			local within = PULL / view.zoom
			local ax, gx = snap.shift({ b.x + dx, b.x + dx + b.w / 2, b.x + dx + b.w }, xs, within, true)
			local ay, gy = snap.shift({ b.y + dy, b.y + dy + b.h / 2, b.y + dy + b.h }, ys, within, true)
			dx, dy = dx + ax, dy + ay
			if gx then
				table.insert(guides, snap.guide("x", gx))
			end
			if gy then
				table.insert(guides, snap.guide("y", gy))
			end
		end
		ui.batch(function()
			for _, m in g.moving do
				state.shapes:set_key({ m.index, "x" }, m.x + dx)
				state.shapes:set_key({ m.index, "y" }, m.y + dy)
			end
			state.guides:set(guides)
		end)
	elseif g.kind == "resize" and g.bounds and g.handle and g.index then
		local id = state.shapes:peek()[g.index].id
		local x, y, guides = snap_point(wx, wy, { [id] = true })
		local r = geometry.resize(g.bounds, g.handle, x, y)
		local index = g.index
		ui.batch(function()
			state.shapes:set_key({ index, "x" }, r.x)
			state.shapes:set_key({ index, "y" }, r.y)
			state.shapes:set_key({ index, "w" }, r.w)
			state.shapes:set_key({ index, "h" }, r.h)
			state.guides:set(guides)
		end)
	elseif g.kind == "draw" and g.index then
		local id = state.shapes:peek()[g.index].id
		local x, y, guides = snap_point(wx, wy, { [id] = true })
		if e.shift then
			-- A square, or a circle.
			local side = math.max(math.abs(x - g.wx), math.abs(y - g.wy))
			x = g.wx + (if x < g.wx then -side else side)
			y = g.wy + (if y < g.wy then -side else side)
		end
		local r = geometry.span(g.wx, g.wy, x, y)
		local index = g.index
		ui.batch(function()
			state.shapes:set_key({ index, "x" }, r.x)
			state.shapes:set_key({ index, "y" }, r.y)
			state.shapes:set_key({ index, "w" }, math.max(r.w, 1))
			state.shapes:set_key({ index, "h" }, math.max(r.h, 1))
			state.guides:set(guides)
		end)
	elseif g.kind == "marquee" then
		local r = geometry.span(g.wx, g.wy, wx, wy)
		local ids = table.clone(g.kept)
		local kept = {}
		for _, id in ids do
			kept[id] = true
		end
		for _, s in state.shapes:peek() do
			if not kept[s.id] and geometry.overlaps(r, s) then
				table.insert(ids, s.id)
			end
		end
		ui.batch(function()
			state.marquee:set(r)
			state.selected:set(ids)
		end)
	end
end

local function release(_: PointerEvent)
	local g = gesture
	gesture = nil
	if not g then
		return
	end
	if g.kind == "draw" and g.index then
		-- A click, not a drag: a shape of the usual size.
		local s = state.shapes:peek()[g.index]
		if s.w < 4 and s.h < 4 then
			local index = g.index
			ui.batch(function()
				state.shapes:set_key({ index, "w" }, 120)
				state.shapes:set_key({ index, "h" }, 80)
			end)
		end
		state.tool:set("select")
	end
	if g.kind == "draw" or g.kind == "move" or g.kind == "resize" then
		history.finish()
	end
	ui.batch(function()
		state.guides:set {}
		state.marquee:set(nil)
	end)
end

-- A rounded rectangle and an ellipse, as shapes: drawn as such (no raster
-- to make as the view pans or zooms).
local function rounded(x: number, y: number, w: number, h: number, r: number): Path
	return ui.path():rect(x, y, w, h, r)
end

local function ellipse(x: number, y: number, w: number, h: number): Path
	return ui.path():ellipse(x, y, w, h)
end

local function outline(s: Shape, x: number, y: number, w: number, h: number, zoom: number): Path
	return if s.kind == "ellipse" then ellipse(x, y, w, h) else rounded(x, y, w, h, s.radius * zoom)
end

-- The grid: only the view redraws it, not the shapes moving over it.
local function grid(): Node
	return ui.canvas {
		w_full = true,
		h_full = true,
		draw = function(c, w, h)
			local view = state.view:get()
			c:rect(0, 0, w, h, "$surface.sunken")
			local step = snap.GRID * 2
			while step * view.zoom < 12 do
				step *= 2
			end
			local x0, y0 = geometry.to_world(view, 0, 0)
			local x1, y1 = geometry.to_world(view, w, h)
			for gx = math.floor(x0 / step) * step, x1, step do
				for gy = math.floor(y0 / step) * step, y1, step do
					local sx, sy = geometry.to_screen(view, gx, gy)
					c:rect(sx - 0.5, sy - 0.5, 1, 1, "$border")
				end
			end
		end,
	}
end

local function drawing(): Node
	return ui.canvas {
		id = "drawing",
		w_full = true,
		h_full = true,
		draw = function(c, w, h)
			local view = state.view:get()
			local zoom = view.zoom
			local chosen = {}
			for _, id in state.selected:get() do
				chosen[id] = true
			end
			local one: Shape? = nil
			for _, s in state.shapes:get() do
				local x, y = geometry.to_screen(view, s.x, s.y)
				local sw, sh = s.w * zoom, s.h * zoom
				local path = outline(s, x, y, sw, sh, zoom)
				c:fill(path, s.fill)
				if s.stroke_width > 0 then
					c:stroke(path, s.stroke, s.stroke_width * zoom)
				end
				if chosen[s.id] then
					c:stroke(outline(s, x, y, sw, sh, zoom), "$accent", 1.5)
					one = if one == nil then s else nil
				end
			end
			-- One shape selected: its handles.
			local ids = state.selected:get()
			if #ids == 1 and one then
				for _, handle in geometry.HANDLES do
					local hx, hy = geometry.handle_at(one, handle)
					local x, y = geometry.to_screen(view, hx, hy)
					c:rect(x - 4, y - 4, 8, 8, "#ffffff", 1)
					c:stroke(rounded(x - 4, y - 4, 8, 8, 1), "$accent", 1.5)
				end
			end
			for _, g in state.guides:get() do
				if g.axis == "x" then
					local x = geometry.to_screen(view, g.at, 0)
					c:line(x, 0, x, h, "#ff3b8b", 1)
				else
					local _, y = geometry.to_screen(view, 0, g.at)
					c:line(0, y, w, y, "#ff3b8b", 1)
				end
			end
			local m = state.marquee:get()
			if m then
				local x, y = geometry.to_screen(view, m.x, m.y)
				c:rect(x, y, m.w * zoom, m.h * zoom, "#4c8dff22")
				c:stroke(rounded(x, y, m.w * zoom, m.h * zoom, 0), "$accent", 1)
			end
		end,
	}
end

local M = {}

function M.space(down: boolean)
	space = down
	cursor:set(if down then "grab" else "default")
end

function M.board(): Node
	return ui.overlay {
		id = "board",
		grow = 1,
		min_w = 0,
		h_full = true,
		overflow = "hidden",
		cursor = function()
			return cursor:get()
		end,
		on_resize = function(size: Size)
			state.board = size
		end,
		on_press = press,
		on_pointer_move = move,
		on_release = release,
		on_wheel = function(e: WheelEvent)
			if e.meta or e.ctrl then
				actions.zoom(math.exp(-e.dy * 0.01), e.local_x, e.local_y)
			else
				actions.pan(-e.dx, -e.dy)
			end
			return true
		end,
		on_pinch = function(e: PinchEvent)
			actions.zoom(1 + e.scale, e.local_x, e.local_y)
			return true
		end,
		grid(),
		drawing(),
	}
end

return M
