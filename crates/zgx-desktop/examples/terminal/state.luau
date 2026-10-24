-- The app's state: every module that requires this one gets the same
-- States. All of it survives a reload (ui.state keeps it by key, and these
-- keys are this module's own), and the host saves the layout part for the
-- next launch (see saved.luau).
local layout = require("@lib/layout")

export type Tab = { id: string, root: layout.Tree }
export type Kind = "pane" | "tab"
-- A pane's edge a drag would put something against.
export type Zone = "left" | "right" | "top" | "bottom"
-- Where a drag would land over a pane: `{ target, zone, kind, id }`, or {}.
export type Drop = { target: string?, zone: Zone?, kind: Kind?, id: string? }
-- What is being dragged: `{ kind, id }`, or {}.
export type Dragging = { kind: Kind?, id: string? }
-- A drag's payload.
export type Payload = { kind: Kind, id: string }
-- What a run saves for the next launch (saved.luau), as read back.
export type SavedLayout = {
	version: number?,
	tabs: { Tab }?,
	active: string?,
	focus: { [string]: string }?,
	ratios: { [string]: number }?,
	zoom: { [string]: string }?,
	kinds: { [string]: string }?,
	next_id: number?,
	-- Browser panes' addresses and terminal panes' directories, by pane.
	pages: { [string]: string }?,
	dirs: { [string]: string }?,
}
-- A plugin as the host found it (terminal/plugins.rs): whether it runs
-- ("enabled"), waits for the user to allow it ("pending"), was turned off
-- ("off") or does not read ("broken", with its `error`), what it asks for
-- and the panels it adds (`kind`: what a pane showing one is).
export type Permission = { id: string, text: string? }
export type PanelInfo = { id: string, kind: string, title: string, description: string }
export type PluginInfo = {
	id: string,
	name: string,
	version: string,
	author: string,
	description: string,
	status: "enabled" | "pending" | "off" | "broken",
	error: string?,
	permissions: { Permission },
	panels: { PanelInfo },
}
-- Where a dragged pane would open as a tab of its own: `{ pane, before }`
-- (the tab it goes before, "" for the end), or {}.
export type Slot = { pane: string?, before: string? }

local M = {}

-- What the host (terminal.rs) publishes.
M.home = ui.shared("home", ""):peek()
M.launch_dir = ui.shared("launch_dir", ""):peek()
M.window_radius = ui.shared("window_radius", 10)
-- The session server's client command: a pane's terminal runs it attached
-- to its own session.
M.attach = ui.shared("attach", ""):peek()
-- The layout the last run saved, or nil.
M.saved_layout = ui.shared("saved_layout", nil :: SavedLayout?)
-- The plugins in the plugins directory, as of the last look, and where
-- that is.
M.plugins = ui.shared("plugins", {} :: { PluginInfo })
M.plugins_dir = ui.shared("plugins_dir", ""):peek()

M.next_id = ui.state("next_id", 1)
-- The tabs, in strip order.
M.tabs = ui.state("tabs", {} :: { Tab })
M.active = ui.state("active", "")
-- Tab id -> its focused pane.
M.focus = ui.state("focus", {} :: { [string]: string })
-- Split id -> the first side's share (0 to 1).
M.ratios = ui.state("ratios", {} :: { [string]: number })
-- Tab id -> the pane shown alone.
M.zoom = ui.state("zoom", {} :: { [string]: string })
-- Pane id -> what it shows when not a terminal: "browser" (a web page),
-- "plugins" (the plugins), or "panel:<plugin>/<panel>" (a plugin's panel).
-- A pane keeps its kind for life.
M.kinds = ui.state("kinds", {} :: { [string]: string })
-- "pane:<id>" or "tab:<id>" awaiting confirmation to close.
M.confirm = ui.state("confirm", "")
M.drop = ui.state("drop", {} :: Drop)
M.dragging = ui.state("dragging", {} :: Dragging)
-- The tab a dragged pane hovers (dropping moves it there).
M.tab_drop = ui.state("tab_drop", "")
M.tab_slot = ui.state("tab_slot", {} :: Slot)
-- The tab dragged down out of the strip (while it is out, the tab beside it
-- shows), or "".
M.lifted = ui.state("lifted", "")
-- Browser panes waiting to have their address bar focused.
M.address_focus = ui.state("address_focus", "")
-- The agents sidebar is open.
M.sidebar = ui.state("sidebar", true)
-- The panel palette (⌘K) is open; what is typed in it, and the row chosen.
M.palette = ui.state("palette", false)
M.palette_query = ui.state("palette_query", "")
M.palette_row = ui.state("palette_row", 1)
-- Plugins the user put off deciding about this run (Not Now).
M.consent_later = ui.state("consent_later", {} :: { [string]: boolean })
-- A panel pane waiting to take keyboard focus.
M.panel_focus = ui.state("panel_focus", "")

return M
