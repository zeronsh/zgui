-- The layout across launches: the host keeps every shell running in its
-- session server, and the layout the last run saved (`saved_layout`) opens
-- them again in their tabs and splits.
local state = require("./state")
local sessions = require("./sessions")
local actions = require("./actions")
local layout = require("@lib/layout")
local names = require("@lib/names")

local tabs = state.tabs

local M = {}

-- The layout the last run left (tabs, splits, which pane each had in
-- focus, pages), its shells still running in the session server.
local function restore(saved: state.SavedLayout?): boolean
	-- Read back from disk: check its shape.
	if type(saved) ~= "table" or type(saved.tabs) ~= "table" or #saved.tabs == 0 then
		return false
	end
	local saved_tabs = saved.tabs
	local saved_kinds = saved.kinds or {}
	state.next_id:set(saved.next_id or 1)
	state.kinds:set(saved_kinds)
	state.ratios:set(saved.ratios or {})
	state.zoom:set(saved.zoom or {})
	state.focus:set(saved.focus or {})
	tabs:set(saved_tabs)
	local keep: { string } = {}
	for _, tab in saved_tabs do
		for _, pane in layout.leaves(tab.root) do
			if saved_kinds[pane] == "browser" then
				local url = (saved.pages or {})[pane]
				if url then
					sessions.page(pane):load(url)
				end
			elseif saved_kinds[pane] == nil then
				sessions.spawn_dir[pane] = sessions.start_dir((saved.dirs or {})[pane])
				sessions.session(pane)
				table.insert(keep, pane)
			end
		end
	end
	-- Sessions no pane has any more go.
	ui.invoke("keep_sessions", keep)
	local active = saved.active
	actions.activate(if active and actions.tab_index(active) then active else saved_tabs[1].id)
	return true
end

-- Start: the saved layout, or one tab where the app was launched. On a
-- reload the tabs are still there: every session gets this run's handlers.
function M.start()
	if #tabs:peek() == 0 then
		if not restore(state.saved_layout:peek()) then
			-- A fresh start: no session from before is anyone's.
			ui.invoke("keep_sessions", {})
			actions.new_tab(state.launch_dir)
		end
	else
		for _, tab in tabs:peek() do
			for _, pane in layout.leaves(tab.root) do
				sessions.session(pane)
			end
		end
		local active = state.active:peek()
		actions.activate(if actions.tab_index(active) then active else tabs:peek()[1].id)
	end
end

-- Saved as it changes (the host writes it a moment later), for the next
-- launch to open as it was.
function M.save_on_change()
	ui.effect(function()
		local list = tabs:get()
		local kinds = state.kinds:get()
		local pages: { [string]: string } = {}
		local dirs: { [string]: string } = {}
		for _, tab in list do
			for _, pane in layout.leaves(tab.root) do
				if kinds[pane] == "browser" then
					local url = sessions.page(pane).url
					if url ~= "" and url ~= "about:blank" then
						pages[pane] = url
					end
				elseif sessions.made[pane] then
					local pwd = names.path_of(ui.term(pane).pwd)
					if pwd ~= "" then
						dirs[pane] = pwd
					end
				end
			end
		end
		local saved: state.SavedLayout = {
			version = 1,
			tabs = list,
			active = state.active:get(),
			focus = state.focus:get(),
			ratios = state.ratios:get(),
			zoom = state.zoom:get(),
			kinds = kinds,
			next_id = state.next_id:get(),
			pages = pages,
			dirs = dirs,
		}
		ui.invoke("save_layout", saved)
	end)
end

return M
