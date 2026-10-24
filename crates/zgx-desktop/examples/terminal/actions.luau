-- What the menus, the tab strip and the panes do to tabs and panes. Each
-- changes the state in state.luau; the views follow it.
local state = require("./state")
local sessions = require("./sessions")
local layout = require("@lib/layout")

local tabs, active, focus, zoom = state.tabs, state.active, state.focus, state.zoom

local M = {}

function M.fresh(prefix: string): string
	local n = state.next_id:peek()
	state.next_id:set(n + 1)
	return prefix .. n
end

function M.tab_index(id: string): number?
	for i, tab in tabs:peek() do
		if tab.id == id then
			return i
		end
	end
	return nil
end
function M.tab_of(pane: string): string?
	for _, tab in tabs:peek() do
		if layout.contains(tab.root, pane) then
			return tab.id
		end
	end
	return nil
end
function M.root_of(id: string): layout.Tree?
	local i = M.tab_index(id)
	return if i then tabs:peek()[i].root else nil
end
function M.set_root(id: string, root: layout.Tree)
	local i = M.tab_index(id)
	if i then
		tabs:set_key({ i, "root" }, root)
	end
end
-- The active tab's focused pane.
function M.current(): string?
	return (focus:peek())[active:peek()]
end

-- Focus and tabs ------------------------------------------------------------

function M.focus_pane(tab: string, pane: string)
	if (focus:peek())[tab] ~= pane then
		focus:set_key(tab, pane)
	end
	if active:peek() == tab then
		sessions.focus_view(pane)
	end
end
function M.activate(tab: string)
	active:set(tab)
	local pane = (focus:peek())[tab]
	if pane then
		sessions.focus_view(pane)
	end
end
-- What a new pane shows: a terminal starting in `cwd` (kind nil or
-- "terminal"), or a page, the plugins or a plugin's panel (state.kinds).
local function make(pane: string, kind: string?, cwd: string)
	if kind == nil or kind == "terminal" then
		sessions.spawn_dir[pane] = cwd
		sessions.session(pane)
		return
	end
	state.kinds:set_key(pane, kind)
	if kind == "browser" then
		state.address_focus:set(pane)
	end
end

-- A tab with a new pane (a terminal, or `kind`), or `pane` moved there,
-- before the tab `before` or last.
function M.new_tab(cwd: string, pane: string?, before: string?, kind: string?)
	local id = M.fresh("t")
	local shown = pane or M.fresh("p")
	if not pane then
		make(shown, kind, cwd)
	end
	local at = if before then M.tab_index(before) else nil
	tabs:insert(at or #tabs:peek() + 1, { id = id, root = layout.pane(shown) })
	focus:set_key(id, shown)
	M.activate(id)
end

-- Split the focused pane: a terminal beside it, or what `kind` says (a
-- browser, the plugins, a plugin's panel).
function M.split(axis: layout.Axis, kind: string?)
	local tab = active:peek()
	local pane = M.current()
	local root = M.root_of(tab)
	if not pane or not root then
		return
	end
	local new = M.fresh("p")
	make(new, kind, sessions.cwd_of(pane))
	zoom:set_key(tab, nil)
	M.set_root(tab, layout.split(root, pane, axis, new, M.fresh("s")))
	M.focus_pane(tab, new)
end

-- Take `pane` out of its tab without ending its session; the tab goes when
-- it was its last pane. Returns whether it was found.
function M.detach(pane: string): boolean
	local tab = M.tab_of(pane)
	local old = if tab then M.root_of(tab) else nil
	if not tab or not old then
		return false
	end
	local root, near = layout.remove(old, pane)
	if (zoom:peek())[tab] == pane then
		zoom:set_key(tab, nil)
	end
	if root then
		M.set_root(tab, root)
		if (focus:peek())[tab] == pane then
			M.focus_pane(tab, near or layout.leaves(root)[1])
		end
		return true
	end
	-- Its last pane: the tab goes, a neighbour takes over.
	local i = M.tab_index(tab) or 1
	tabs:remove(i)
	focus:set_key(tab, nil)
	local list = tabs:peek()
	if #list == 0 then
		active:set("")
	elseif active:peek() == tab then
		M.activate(list[math.min(i, #list)].id)
	end
	return true
end

-- Close a pane, ending what runs in it: asking first if it runs something,
-- unless `force`. The window closes with its last pane.
function M.close_pane(pane: string, force: boolean?)
	if not force and sessions.needs_confirm(pane) then
		ui.term(pane):blur()
		state.confirm:set("pane:" .. pane)
		return
	end
	if state.confirm:peek() == "pane:" .. pane then
		state.confirm:set("")
	end
	M.detach(pane)
	if sessions.is_terminal(pane) then
		ui.term(pane):close()
		-- Closed, not detached: its shell ends too.
		ui.invoke("end_session", pane)
	else
		if sessions.is_browser(pane) then
			sessions.page(pane):close()
		end
		state.kinds:set_key(pane, nil)
	end
	if #tabs:peek() == 0 then
		ui.invoke("close_window")
	end
end
sessions.close_pane = M.close_pane

function M.close_tab(tab: string, force: boolean?)
	local root = M.root_of(tab)
	if not root then
		return
	end
	local panes = layout.leaves(root)
	if not force then
		for _, pane in panes do
			if sessions.needs_confirm(pane) then
				ui.term(pane):blur()
				state.confirm:set("tab:" .. tab)
				return
			end
		end
	end
	state.confirm:set("")
	for _, pane in panes do
		M.close_pane(pane, true)
	end
end

-- Moving ------------------------------------------------------------------

-- Move `pane` beside `target`.
function M.move_pane(pane: string, target: string, zone: state.Zone)
	if pane == target or not M.tab_of(target) then
		return
	end
	M.detach(pane)
	local tab = M.tab_of(target)
	local root = if tab then M.root_of(tab) else nil
	if not tab or not root then
		return
	end
	local axis: layout.Axis = if zone == "left" or zone == "right" then "row" else "column"
	local before = zone == "left" or zone == "top"
	M.set_root(tab, layout.split(root, target, axis, pane, M.fresh("s"), before))
	zoom:set_key(tab, nil)
	active:set(tab)
	M.focus_pane(tab, pane)
end

-- Preview `pane` opening as a tab before `before` ("" : at the end).
function M.show_slot(pane: string, before: string)
	local s = state.tab_slot:peek()
	if s.pane ~= pane or s.before ~= before then
		state.tab_slot:set { pane = pane, before = before }
	end
	if state.tab_drop:peek() ~= "" then
		state.tab_drop:set("")
	end
end
function M.hide_slot()
	if (state.tab_slot:peek()).pane then
		state.tab_slot:set {}
	end
end
-- Open the previewed tab: the pane leaves its tab for one of its own.
function M.commit_slot(): boolean
	local s = state.tab_slot:peek()
	state.tab_slot:set {}
	local pane, before = s.pane, s.before
	if not pane then
		return false
	end
	-- Its tab may go with it (its last pane): place it by a neighbour.
	local root = if before then M.root_of(before) else nil
	if before == M.tab_of(pane) and root and #layout.leaves(root) == 1 then
		return true
	end
	if M.detach(pane) then
		M.new_tab("", pane, before)
	end
	return true
end
-- Move `pane` into `tab`, beside its focused pane.
function M.move_pane_to_tab(pane: string, tab: string)
	local target = (focus:peek())[tab]
	if not target or M.tab_of(pane) == tab then
		M.activate(tab)
		return
	end
	M.move_pane(pane, target, "right")
end
-- Move `tab` before or after `target` now, the others sliding to make room
-- (the strip animates its reorders).
function M.reorder(tab: string, target: string, side: "before" | "after")
	local list = tabs:peek()
	local next: { state.Tab } = {}
	local moved: state.Tab? = nil
	for _, item in list do
		if item.id == tab then
			moved = item
		else
			table.insert(next, item)
		end
	end
	if not moved then
		return
	end
	for i, item in next do
		if item.id == target then
			table.insert(next, if side == "before" then i else i + 1, moved)
			break
		end
	end
	local same = true
	for i, item in next do
		if list[i].id ~= item.id then
			same = false
		end
	end
	if same then
		return
	end
	tabs:set(next)
end

-- Dragging the active tab down out of the strip lifts it: the tab beside it
-- shows (a drop on its panes joins them), until the drag comes back to the
-- strip or ends.
function M.lift(tab: string)
	local list = tabs:peek()
	local i = M.tab_index(tab)
	if not i or #list < 2 or active:peek() ~= tab or state.lifted:peek() == tab then
		return
	end
	state.lifted:set(tab)
	M.activate(list[if i < #list then i + 1 else i - 1].id)
end
-- The lifted tab back in the strip, shown again (unless it joined another).
function M.land(tab: string)
	if state.lifted:peek() ~= tab then
		return
	end
	state.lifted:set("")
	if M.tab_index(tab) then
		M.activate(tab)
	end
end

-- Moving focus --------------------------------------------------------------

function M.step_pane(by: number)
	local root = M.root_of(active:peek())
	local pane = M.current()
	if not root or not pane then
		return
	end
	local leaves = layout.leaves(root)
	local i = table.find(leaves, pane) or 1
	M.focus_pane(active:peek(), leaves[(i - 1 + by) % #leaves + 1])
end
function M.step_tab(by: number)
	local list = tabs:peek()
	local i = M.tab_index(active:peek()) or 1
	if #list > 0 then
		M.activate(list[(i - 1 + by) % #list + 1].id)
	end
end
function M.go(direction: layout.Direction)
	local root = M.root_of(active:peek())
	local pane = M.current()
	local other = root and pane and layout.neighbor(root, pane, direction)
	if other then
		M.focus_pane(active:peek(), other)
	end
end

return M
