-- A terminal multiplexer: tabs in the titlebar, each a tree of split panes
-- running Ghostty terminals, or web pages. Panes resize by their dividers
-- and move by the grip in their top padding, onto another pane's edge or
-- onto a tab; tabs reorder by dragging. Everything is state that survives
-- a reload, and the shells keep running through one.
--
-- The app is split by what each part is about (docs/reference/app-structure.md):
--   state.luau     the app's state, shared by every module that requires it
--   sessions.luau  the terminals and pages that run in panes
--   actions.luau   what the menus, tabs and panes do to tabs and panes
--   saved.luau     the layout across launches
--   commands.luau  the menu bar's commands
--   frames.luau    where the active tab's panes go
--   agents.luau    the coding agents in every pane, as the session server sees them (the sidebar)
--   components/    the views: the titlebar, sidebar, panes, dividers, the sheets,
--                  the ⌘K palette, plugins' panels
--
-- Plugins (terminal/plugins.rs) add panels: each runs in a VM of its own,
-- reaching only what the user allowed it when zterm found it.
--   lib/           plain logic: the split tree, pane names
local theme = require("./theme")
local state = require("./state")
local sessions = require("./sessions")
local saved = require("./saved")
local commands = require("./commands")
local tabs = require("@components/tabs")
local sidebar = require("@components/sidebar")
local content = require("@components/content")
local confirm = require("@components/confirm")
local palette = require("@components/palette")
local consent = require("@components/consent")

-- Open the last layout (or a first tab), keep it saved, declare the menus.
saved.start()
saved.save_on_change()
commands.declare()

-- The window's title follows the active tab (Mission Control, the Dock).
ui.effect(function()
	local name = sessions.describe((state.focus:get())[state.active:get()]).name
	ui.invoke("set_title", if name ~= "" then name else "Terminal")
end)

return ui.row {
	w_full = true,
	h_full = true,
	font = theme.font,
	items = "stretch",
	bg = theme.tint,
	on_key = confirm.on_key,
	-- The agents running in every tab, by status (components/sidebar.luau).
	sidebar.Sidebar(),
	ui.column {
		grow = 1,
		shrink = 1,
		min_w = 0,
		items = "stretch",
		tabs.Titlebar(),
		content(),
	},
	-- Over the sidebar's header and the titlebar, past the traffic lights.
	sidebar.Toggle(),
	confirm.Sheet(),
	-- ⌘K: a new pane (components/palette.luau).
	palette.Palette(),
	-- A plugin found that has not been allowed asks (components/consent.luau).
	consent.Sheet(),
}
