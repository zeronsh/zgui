-- The menu bar's commands, with their keys: the host shows them as menus,
-- and their keys work wherever focus is (a terminal, a page).
local state = require("./state")
local sessions = require("./sessions")
local actions = require("./actions")
local palette = require("@components/palette")
local layout = require("@lib/layout")

local active, zoom = state.active, state.zoom
local current = actions.current

local function command(id: string, title: string, menu: string, key: string?, run: () -> ())
	ui.command { id = id, title = title, menu = menu, key = key, run = run }
end

-- What a terminal binding does on a page.
local PAGE_ACTIONS: { [string]: (web: Web, pane: string) -> () } = {
	copy_to_clipboard = function(web)
		web:run("document.execCommand('copy')")
	end,
	paste_from_clipboard = function(web)
		web:run("document.execCommand('paste')")
	end,
	select_all = function(web)
		web:run("document.execCommand('selectAll')")
	end,
}
-- Page zoom by pane, for Bigger and Smaller.
local page_zoom: { [string]: number } = {}
local function zoom_page(pane: string, by: number?)
	local z = if by then math.clamp((page_zoom[pane] or 1) * by, 0.3, 3) else 1
	page_zoom[pane] = z
	sessions.page(pane):zoom(z)
end
PAGE_ACTIONS["increase_font_size:1"] = function(_, pane)
	zoom_page(pane, 1.1)
end
PAGE_ACTIONS["decrease_font_size:1"] = function(_, pane)
	zoom_page(pane, 1 / 1.1)
end
PAGE_ACTIONS.reset_font_size = function(_, pane)
	zoom_page(pane)
end
-- A Ghostty binding on the focused terminal, or what it means on a page.
local function binding(action: string): () -> ()
	return function()
		local pane = current()
		if not pane then
			return
		end
		if sessions.is_browser(pane) then
			local run = PAGE_ACTIONS[action]
			if run then
				run(sessions.page(pane), pane)
			end
		elseif sessions.is_terminal(pane) then
			ui.term(pane):binding(action)
		end
	end
end

local M = {}

-- Declares them; each load declares its commands afresh.
function M.declare()
	command("new_tab", "New Tab", "Shell", "cmd+t", function()
		actions.new_tab(sessions.cwd_of(current()))
	end)
	command("split_right", "Split Right", "Shell", "cmd+d", function()
		actions.split("row")
	end)
	command("split_down", "Split Down", "Shell", "cmd+shift+d", function()
		actions.split("column")
	end)
	command("new_browser", "New Browser Pane", "Shell", "cmd+shift+b", function()
		actions.split("row", "browser")
	end)
	-- Terminals, browsers, the plugins and plugins' panels.
	command("open_panel", "Open Panel…", "Shell", "cmd+k", palette.open)
	ui.command { menu = "Shell", separator = true }
	command("close_pane", "Close Pane", "Shell", "cmd+w", function()
		local pane = current()
		if pane then
			actions.close_pane(pane)
		end
	end)
	command("close_tab", "Close Tab", "Shell", "cmd+alt+w", function()
		actions.close_tab(active:peek())
	end)

	command("copy", "Copy", "Edit", "cmd+c", binding("copy_to_clipboard"))
	command("paste", "Paste", "Edit", "cmd+v", binding("paste_from_clipboard"))
	command("select_all", "Select All", "Edit", "cmd+a", binding("select_all"))
	ui.command { menu = "Edit", separator = true }
	-- ⌘K opens panels; Clear moves over, by ⇧.
	command("clear", "Clear", "Edit", "cmd+shift+k", binding("clear_screen"))

	command("zoom_pane", "Zoom Pane", "View", "cmd+shift+enter", function()
		local tab, pane = active:peek(), current()
		if pane then
			zoom:set_key(tab, if (zoom:peek())[tab] == pane then nil else pane)
			sessions.focus_view(pane)
		end
	end)
	command("toggle_sidebar", "Toggle Agents Sidebar", "View", "cmd+ctrl+s", function()
		state.sidebar:set(not state.sidebar:peek())
	end)
	command("equalize", "Equalize Panes", "View", "cmd+ctrl+=", function()
		local root = actions.root_of(active:peek())
		if root then
			state.ratios:update(function(map)
				for _, id in layout.splits(root) do
					map[id] = nil
				end
			end)
		end
	end)
	ui.command { menu = "View", separator = true }
	command("bigger", "Bigger", "View", "cmd+=", binding("increase_font_size:1"))
	command("smaller", "Smaller", "View", "cmd+-", binding("decrease_font_size:1"))
	command("actual_size", "Actual Size", "View", "cmd+0", binding("reset_font_size"))
	ui.command { menu = "View", separator = true }
	command("reload_page", "Reload Page", "View", "cmd+r", function()
		local pane = current()
		if pane and sessions.is_browser(pane) then
			sessions.page(pane):reload()
		end
	end)
	command("open_location", "Open Location", "View", "cmd+l", function()
		local pane = current()
		if pane and sessions.is_browser(pane) then
			state.address_focus:set(pane)
		end
	end)

	command("next_pane", "Next Pane", "Window", "cmd+]", function()
		actions.step_pane(1)
	end)
	command("previous_pane", "Previous Pane", "Window", "cmd+[", function()
		actions.step_pane(-1)
	end)
	for _, direction: layout.Direction in { "left", "right", "up", "down" } do
		command(
			"focus_" .. direction,
			"Focus Pane " .. direction:sub(1, 1):upper() .. direction:sub(2),
			"Window",
			"cmd+alt+arrow" .. direction,
			function()
				actions.go(direction)
			end
		)
	end
	ui.command { menu = "Window", separator = true }
	command("next_tab", "Next Tab", "Window", "cmd+shift+]", function()
		actions.step_tab(1)
	end)
	command("previous_tab", "Previous Tab", "Window", "cmd+shift+[", function()
		actions.step_tab(-1)
	end)
	for n = 1, 9 do
		command("tab_" .. n, if n == 9 then "Last Tab" else "Tab " .. n, "Window", "cmd+" .. n, function()
			local list = state.tabs:peek()
			local tab = if n == 9 then list[#list] else list[n]
			if tab then
				actions.activate(tab.id)
			end
		end)
	end
end

return M
