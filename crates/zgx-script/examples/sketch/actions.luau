-- What the menus, keys and the inspector do to the drawing.
local state = require("./state")
local history = require("./history")
local geometry = require("@lib/geometry")

type Shape = geometry.Shape

local M = {}

M.FILLS = { "#4c8dff", "#33b679", "#f6bf26", "#f4511e", "#a142f4", "#e8eaed", "#202124" }

function M.new_id(): string
	local id = `s{state.next_id}`
	state.next_id += 1
	return id
end

-- The index of shape `id`, and the shape.
function M.find(id: string): (number?, Shape?)
	for i, s in state.shapes:peek() do
		if s.id == id then
			return i, s
		end
	end
	return nil, nil
end

function M.selected_shapes(): { Shape }
	local chosen = {}
	for _, id in state.selected:peek() do
		chosen[id] = true
	end
	local out = {}
	for _, s in state.shapes:peek() do
		if chosen[s.id] then
			table.insert(out, s)
		end
	end
	return out
end

function M.add(kind: geometry.Kind, r: geometry.Rect): string
	local count = #state.shapes:peek() + 1
	local shape: Shape = {
		id = M.new_id(),
		kind = kind,
		name = `{if kind == "rect" then "Rectangle" else "Ellipse"} {count}`,
		x = r.x,
		y = r.y,
		w = r.w,
		h = r.h,
		fill = M.FILLS[(count - 1) % #M.FILLS + 1],
		stroke = "#00000000",
		stroke_width = 0,
		radius = if kind == "rect" then 6 else 0,
	}
	state.shapes:push(shape)
	return shape.id
end

-- Field `key` of shape `id`, as one change to undo.
function M.set_field(id: string, key: string, value: any)
	local i = M.find(id)
	if i then
		history.record()
		state.shapes:set_key({ i, key }, value)
	end
end

function M.delete_selected()
	local chosen = {}
	for _, id in state.selected:peek() do
		chosen[id] = true
	end
	if next(chosen) == nil then
		return
	end
	history.record()
	local kept = {}
	for _, s in state.shapes:peek() do
		if not chosen[s.id] then
			table.insert(kept, s)
		end
	end
	state.shapes:set(kept)
	state.selected:set {}
end

function M.duplicate()
	local copies = {}
	local shapes = M.selected_shapes()
	if #shapes == 0 then
		return
	end
	history.record()
	for _, s in shapes do
		local copy = table.clone(s)
		copy.id = M.new_id()
		copy.name = `{s.name} copy`
		copy.x += 16
		copy.y += 16
		state.shapes:push(copy)
		table.insert(copies, copy.id)
	end
	state.selected:set(copies)
end

-- Move the selection by (`dx`, `dy`) world points, as one change.
function M.nudge(dx: number, dy: number)
	local chosen = {}
	for _, id in state.selected:peek() do
		chosen[id] = true
	end
	if next(chosen) == nil then
		return
	end
	history.record()
	ui.batch(function()
		for i, s in state.shapes:peek() do
			if chosen[s.id] then
				state.shapes:set_key({ i, "x" }, s.x + dx)
				state.shapes:set_key({ i, "y" }, s.y + dy)
			end
		end
	end)
end

function M.select_all()
	local ids = {}
	for _, s in state.shapes:peek() do
		table.insert(ids, s.id)
	end
	state.selected:set(ids)
end

-- To the top (`front`) or the bottom of the stack.
function M.arrange(front: boolean)
	local chosen = {}
	for _, id in state.selected:peek() do
		chosen[id] = true
	end
	if next(chosen) == nil then
		return
	end
	history.record()
	local moved, rest = {}, {}
	for _, s in state.shapes:peek() do
		table.insert(if chosen[s.id] then moved else rest, s)
	end
	local out = if front then rest else moved
	for _, s in if front then moved else rest do
		table.insert(out, s)
	end
	state.shapes:set(out)
end

-- Zoom by `factor` about the board point (`sx`, `sy`): what is under it
-- stays under it.
function M.zoom(factor: number, sx: number, sy: number)
	local v = state.view:peek()
	local zoom = math.clamp(v.zoom * factor, 0.1, 8)
	local wx, wy = geometry.to_world(v, sx, sy)
	state.view:set { x = sx - wx * zoom, y = sy - wy * zoom, zoom = zoom }
end

-- Zoom about the board's middle (the menu, the toolbar's buttons).
function M.zoom_middle(factor: number)
	M.zoom(factor, state.board.w / 2, state.board.h / 2)
end

function M.actual_size()
	M.zoom(1 / state.view:peek().zoom, state.board.w / 2, state.board.h / 2)
end

function M.pan(dx: number, dy: number)
	local v = state.view:peek()
	state.view:set { x = v.x + dx, y = v.y + dy, zoom = v.zoom }
end

-- Files: the drawing as JSON.
local function document(): string
	return ui.json.encode({ version = 1, shapes = state.shapes:peek() }, { pretty = true })
end

local function load(text: string): boolean
	local ok, doc = pcall(ui.json.decode, text)
	if not ok or type(doc) ~= "table" or type(doc.shapes) ~= "table" then
		state.status:set("Not a Sketch drawing")
		return false
	end
	local highest = 0
	for _, s in doc.shapes do
		local n = tonumber(string.match(s.id or "", "^s(%d+)$"))
		highest = math.max(highest, n or 0)
	end
	state.next_id = highest + 1
	state.shapes:set(doc.shapes)
	state.selected:set {}
	history.reset()
	state.dirty:set(false)
	return true
end
M.load = load

function M.new()
	state.shapes:set {}
	state.selected:set {}
	state.file:set(nil)
	history.reset()
	state.dirty:set(false)
	state.status:set("")
end

-- Read `file` as the drawing.
local function open_file(file: File)
	local ok, text = pcall(function()
		return ui.read_file(file):await()
	end)
	if not ok then
		state.status:set(`Couldn't open {file.name}`)
	elseif load(text) then
		state.file:set(file)
		state.status:set(`Opened {file.name}`)
	end
end

function M.open()
	ui.task(function()
		local files =
			ui.open_files({ title = "Open a Drawing", filters = { { "Sketch drawing", { "json" } } } }):await()
		if files[1] then
			open_file(files[1])
		end
	end)
end

-- A drawing the system opens with the app (`zgx-run sketch drawing.json`).
ui.on_open(function(event)
	local file = event.files[1]
	if file then
		ui.task(function()
			open_file(file)
		end)
	end
end)

local function write(file: File)
	local ok = pcall(function()
		ui.write_file(file, document()):await()
	end)
	if ok then
		state.file:set(file)
		state.dirty:set(false)
		state.status:set(`Saved {file.name}`)
	else
		state.status:set(`Couldn't save {file.name}`)
	end
end

function M.save_as()
	ui.task(function()
		local file = ui.save_file({ title = "Save the Drawing", name = "Drawing.json" }):await()
		if file then
			write(file)
		end
	end)
end

function M.save()
	local file = state.file:peek()
	if file then
		ui.task(function()
			write(file)
		end)
	else
		M.save_as()
	end
end

return M
