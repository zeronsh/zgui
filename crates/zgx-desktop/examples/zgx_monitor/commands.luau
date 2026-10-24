-- The menu bar's commands, with their keys: the host shows them as menus,
-- and their keys work wherever focus is.
local state = require("./state")
local actions = require("./actions")
local layout = require("@lib/layout")

local function command(id: string, title: string, menu: string, key: string?, run: () -> ())
	ui.command { id = id, title = title, menu = menu, key = key, run = run }
end

local M = {}

-- Declares them; each load declares its commands afresh.
function M.declare()
	command("split_right", "Split Right", "Panel", "cmd+d", function()
		actions.split("row")
	end)
	command("split_down", "Split Down", "Panel", "cmd+shift+d", function()
		actions.split("column")
	end)
	ui.command { menu = "Panel", separator = true }
	command("zoom_panel", "Zoom Panel", "Panel", "cmd+shift+enter", actions.toggle_zoom)
	command("equalize", "Equalize Panels", "Panel", "cmd+ctrl+=", actions.equalize)
	ui.command { menu = "Panel", separator = true }
	command("close_panel", "Close Panel", "Panel", "cmd+w", actions.close_active)

	command("toggle_sidebar", "Toggle Sidebar", "View", "cmd+ctrl+s", actions.toggle_sidebar)
	command("find", "Find Process", "View", "cmd+f", function()
		if not state.sidebar_open:peek() then
			actions.set_sidebar(true)
		end
		local field = state.search:peek()
		if field then
			field:focus()
			field:select_all()
		end
	end)
	ui.command { menu = "View", separator = true }
	for n, sort in { "cpu", "gpu", "memory", "name" } :: { state.Sort } do
		local title = ({ cpu = "Sort by CPU", gpu = "Sort by GPU", memory = "Sort by Memory", name = "Sort by Name" })[sort]
		command("sort_" .. sort, title, "View", "cmd+" .. n, function()
			state.sort:set(sort)
			state.list_offset:set(0)
		end)
	end

	command("next_panel", "Next Panel", "Window", "cmd+]", function()
		actions.cycle(1)
	end)
	command("previous_panel", "Previous Panel", "Window", "cmd+[", function()
		actions.cycle(-1)
	end)
	for _, direction: layout.Direction in { "left", "right", "up", "down" } do
		command(
			"focus_" .. direction,
			"Focus Panel " .. direction:sub(1, 1):upper() .. direction:sub(2),
			"Window",
			"cmd+alt+arrow" .. direction,
			function()
				actions.go(direction)
			end
		)
	end
end

return M
