-- Sketch: draw rectangles and ellipses on a board you pan and zoom;
-- select, move and resize them with snapping; edit them in the inspector;
-- undo and redo; open and save drawings as JSON through the system's
-- dialogs.
local state = require("./state")
local actions = require("./actions")
local history = require("./history")
local Board = require("@components/board")
local Inspector = require("@components/inspector")
local Toolbar = require("@components/toolbar")

local function command(id: string, title: string, menu: string, key: string?, run: () -> ())
	ui.command { id = id, title = title, menu = menu, key = key, run = run }
end
command("new", "New", "File", "cmd+n", actions.new)
command("open", "Open…", "File", "cmd+o", actions.open)
command("save", "Save", "File", "cmd+s", actions.save)
command("save_as", "Save As…", "File", "cmd+shift+s", actions.save_as)
command("duplicate", "Duplicate", "Edit", "cmd+d", actions.duplicate)
command("front", "Bring to Front", "Arrange", "cmd+shift+]", function()
	actions.arrange(true)
end)
command("back", "Send to Back", "Arrange", "cmd+shift+[", function()
	actions.arrange(false)
end)
command("zoom_in", "Zoom In", "View", "cmd+=", function()
	actions.zoom_middle(1.25)
end)
command("zoom_out", "Zoom Out", "View", "cmd+-", function()
	actions.zoom_middle(1 / 1.25)
end)
command("actual_size", "Actual Size", "View", "cmd+0", actions.actual_size)
command("snapping", "Snap to Grid and Shapes", "View", "cmd+'", function()
	state.snapping:set(not state.snapping:peek())
end)

-- Keys the board takes while no field is being edited (the Edit menu's
-- Undo and Select All are the text fields' own).
local TOOL_KEYS = { v = "select", r = "rect", o = "ellipse" }
local function key(e: KeyEvent): boolean?
	if e.editing then
		return nil
	end
	local command = e.meta or e.ctrl
	local k = string.lower(e.key)
	if command and k == "z" then
		if e.shift then
			history.redo()
		else
			history.undo()
		end
		return true
	elseif command and k == "a" then
		actions.select_all()
		return true
	elseif command then
		return nil
	elseif e.key == "Backspace" or e.key == "Delete" then
		actions.delete_selected()
		return true
	elseif e.key == "Escape" then
		state.selected:set {}
		state.tool:set("select")
		return true
	elseif e.key == " " then
		Board.space(true)
		return true
	elseif TOOL_KEYS[k] then
		state.tool:set(TOOL_KEYS[k] :: any)
		return true
	end
	local step = if e.shift then 10 else 1
	local moves =
		{ ArrowLeft = { -step, 0 }, ArrowRight = { step, 0 }, ArrowUp = { 0, -step }, ArrowDown = { 0, step } }
	local by = moves[e.key]
	if by then
		actions.nudge(by[1], by[2])
		return true
	end
	return nil
end

return ui.column {
	id = "sketch",
	w_full = true,
	h_full = true,
	bg = "$surface.base",
	on_key = key,
	on_key_up = function(k: string)
		if k == " " then
			Board.space(false)
		end
	end,
	Toolbar(),
	ui.split {
		size = state.inspector,
		second = true,
		min = 220,
		max = 420,
		min_rest = 320,
		grow = 1,
		min_h = 0,
		Board.board(),
		Inspector(),
	},
}
