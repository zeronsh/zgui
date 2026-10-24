-- What the sidebar, the panels, the menus and the keys do to the panels.
-- Each changes the state in state.luau; the views follow it.
local theme = require("./theme")
local state = require("./state")
local data = require("./data")
local layout = require("@lib/layout")

local root, pids, active, zoom = state.root, state.pids, state.active, state.zoom

local M = {}

-- Past this many panels, a process dropped on a panel's edge shows in it
-- instead of opening another.
M.MAX = 6

function M.fresh(prefix: string): string
	local n = state.next_id:peek()
	state.next_id:set(n + 1)
	return prefix .. n
end
-- The panel id `fresh("p")` gives next: a drag previews its panel by it.
function M.next_panel(): string
	return "p" .. state.next_id:peek()
end

function M.panels(): { string }
	local tree = root:peek()
	return if tree then layout.leaves(tree) else {}
end
function M.has(pane: string): boolean
	local tree = root:peek()
	return tree ~= nil and layout.contains(tree, pane)
end
function M.full(): boolean
	return #M.panels() >= M.MAX
end
function M.pid_of(pane: string?): number?
	return if pane then (pids:peek())[pane] else nil
end

function M.focus(pane: string)
	if active:peek() ~= pane then
		active:set(pane)
	end
end

-- A first panel, showing `pid` or nothing yet.
function M.open(pid: number?)
	local id = M.fresh("p")
	if pid then
		pids:set_key(id, pid)
	end
	root:set(layout.pane(id))
	M.focus(id)
end

-- The first panel shows the process the monitor launched, once per launch.
function M.start()
	if not state.started:peek() then
		state.started:set(true)
		M.open(state.launched)
	end
end

-- Show `pid` in the active panel (opening one if there is none).
function M.show(pid: number)
	local pane = active:peek()
	if not M.has(pane) then
		M.open(pid)
		return
	end
	pids:set_key(pane, pid)
end

-- A panel beside the active one (right of it or below), showing `pid` or
-- the same process; the active one stays as it is, to compare against.
function M.split(axis: layout.Axis, pid: number?)
	local pane = active:peek()
	local tree = root:peek()
	if not tree or not layout.contains(tree, pane) then
		M.open(pid)
		return
	end
	if M.full() then
		if pid then
			M.show(pid)
		end
		return
	end
	local shown = pid or M.pid_of(pane)
	local id = M.fresh("p")
	if shown then
		pids:set_key(id, shown)
	end
	zoom:set("")
	root:set(layout.split(tree, pane, axis, id, M.fresh("s")))
	M.focus(id)
end

function M.close(pane: string)
	local tree = root:peek()
	if not tree or not layout.contains(tree, pane) then
		return
	end
	local rest, near = layout.remove(tree, pane)
	pids:set_key(pane, nil)
	if zoom:peek() == pane then
		zoom:set("")
	end
	root:set(rest)
	if active:peek() == pane then
		active:set(near or "")
	end
end

-- Close the active panel; with none, the window.
function M.close_active()
	if M.has(active:peek()) then
		M.close(active:peek())
	else
		ui.invoke("close_window")
	end
end

function M.toggle_zoom()
	local pane = active:peek()
	if M.has(pane) then
		zoom:set(if zoom:peek() == pane then "" else pane)
	end
end

function M.equalize()
	local tree = root:peek()
	if tree then
		state.ratios:update(function(map)
			for _, id in layout.splits(tree) do
				map[id] = nil
			end
		end)
	end
end

-- The sidebar -----------------------------------------------------------------

-- Open or close the sidebar, easing it; the filter has focus while it is open.
function M.set_sidebar(open: boolean)
	state.sidebar_open:set(open)
	anime.animate(state.sidebar_shown, {
		value = if open then state.sidebar_w:peek() else 0,
		duration = 300,
		ease = "outQuint",
	})
	local target = if open then state.search:peek() else state.window:peek()
	if target then
		target:focus()
	end
end
function M.toggle_sidebar()
	M.set_sidebar(not state.sidebar_open:peek())
end

-- Scroll the process list just enough to show row `index`.
function M.reveal(index: number)
	local top, bottom = (index - 1) * theme.row, index * theme.row
	local offset, viewport = state.list_offset:peek(), state.list_h:peek()
	local target = offset
	if top < offset then
		target = top
	elseif bottom > offset + viewport then
		target = bottom - viewport
	end
	if target ~= offset then
		anime.animate(state.list_offset, { value = target, duration = 160, ease = "outQuad" })
	end
end

-- The row showing `pid`, if the list has one.
local function row_of(pid: number?): number?
	for i, p in data.rows do
		if p.pid == pid then
			return i
		end
	end
	return nil
end

-- Show the process `by` rows from the active panel's in the list.
function M.step(by: number)
	local rows = data.rows
	if #rows == 0 then
		return
	end
	local index = row_of(M.pid_of(active:peek()))
	local next = if index then math.clamp(index + by, 1, #rows) elseif by > 0 then 1 else #rows
	data.hold()
	M.show(rows[next].pid)
	M.reveal(next)
end

-- Moving focus ------------------------------------------------------------------

local function focus_and_reveal(pane: string)
	M.focus(pane)
	local index = row_of(M.pid_of(pane))
	if index then
		M.reveal(index)
	end
end

-- The next panel in reading order, or `by` back.
function M.cycle(by: number)
	local list = M.panels()
	if #list == 0 then
		return
	end
	local i = table.find(list, active:peek()) or 1
	focus_and_reveal(list[(i - 1 + by) % #list + 1])
end

function M.go(direction: layout.Direction)
	local tree = root:peek()
	local other = if tree then layout.neighbor(tree, active:peek(), direction) else nil
	if other then
		focus_and_reveal(other)
	end
end

-- Dropping ------------------------------------------------------------------------

function M.axis_of(zone: state.Zone?): layout.Axis
	return if zone == "left" or zone == "right" then "row" else "column"
end
function M.before(zone: state.Zone?): boolean
	return zone == "left" or zone == "top"
end

-- Commit what the drop preview showed.
function M.commit_drop(d: state.Drop)
	local tree = root:peek()
	local target, zone = d.target, d.zone
	if not tree or not target or not zone or not layout.contains(tree, target) then
		return
	end
	if d.kind == "panel" then
		local id = d.id
		if not id or id == target or not layout.contains(tree, id) then
			return
		end
		if zone == "center" then
			root:set(layout.swap(tree, id, target))
		else
			local base = layout.remove(tree, id)
			if not base then
				return
			end
			root:set(layout.split(base, target, M.axis_of(zone), id, M.fresh("s"), M.before(zone)))
		end
		zoom:set("")
		M.focus(id)
		return
	end
	local pid = d.pid
	if not pid then
		return
	end
	if zone == "center" or M.full() then
		pids:set_key(target, pid)
		M.focus(target)
		return
	end
	-- The panel the preview showed: the same id, so it stays where it is.
	local id = M.fresh("p")
	pids:set_key(id, pid)
	zoom:set("")
	root:set(layout.split(tree, target, M.axis_of(zone), id, M.fresh("s"), M.before(zone)))
	M.focus(id)
end

return M
