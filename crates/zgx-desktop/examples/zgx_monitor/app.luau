-- A resource monitor: every process in the sidebar, and panels charting a
-- process's CPU, GPU and memory over the last minute. Panels lay out as the
-- terminal example's panes do: split beside or below, resized by their
-- dividers, moved by their headers onto another panel's edge (or its
-- middle, to swap), with processes dragged from the sidebar the same way.
-- The host (zgx_monitor.rs) samples; everything else is here.
--
--   state.luau     the panels, the sidebar's filter and sort, drags
--   data.luau      what the host samples, in the sidebar's order
--   actions.luau   what the sidebar, panels, menus and keys do
--   keys.luau      the window's own keys
--   commands.luau  the menu bar's commands
--   frames.luau    where the panels go
--   components/    the sidebar and its rows, the panels and their charts
--   lib/           plain logic: the split tree, number formats
local theme = require("./theme")
local state = require("./state")
local actions = require("./actions")
local keys = require("./keys")
local commands = require("./commands")
local sidebar = require("@components/sidebar")
local Titlebar = require("@components/titlebar")
local Workspace = require("@components/workspace")

-- The first panel shows the process the monitor launched.
actions.start()
commands.declare()

-- Typing filters the list from the start (the window takes the keys
-- while the sidebar is closed).
ui.effect(function()
	local field = (if state.sidebar_open:peek() then state.search else state.window):get()
	if field then
		field:focus()
	end
end)

return ui.column {
	w_full = true,
	h_full = true,
	items = "stretch",
	font = theme.font,
	color = theme.text,
	bg = theme.tint,
	focusable = true,
	ref = state.window,
	on_key = keys.on_key,
	Titlebar(),
	ui.row {
		w_full = true,
		grow = 1,
		min_h = 0,
		items = "stretch",
		sidebar.Sidebar(),
		sidebar.Resizer(),
		Workspace(),
	},
}
