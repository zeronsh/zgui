-- What runs in the panes: terminals (Ghostty surfaces attached to the
-- host's session server), web pages and plugins' panels. The host keeps both across reloads;
-- this run asks for them by pane id.
local state = require("./state")
local names = require("@lib/names")

local M = {}

-- Where a new pane's shell starts, until it has.
local spawn_dir: { [string]: string } = {}
M.spawn_dir = spawn_dir
-- Terminal panes whose sessions this run has asked for.
local made: { [string]: boolean } = {}
M.made = made
-- What a terminal asking to close does. actions.luau, which needs these
-- sessions, sets it: a module cannot require one that requires it.
M.close_pane = function(_pane: string, _force: boolean?) end

function M.is_browser(pane: string?): boolean
	return pane ~= nil and (state.kinds:peek())[pane] == "browser"
end
-- A terminal: what a pane is unless it was made something else.
function M.is_terminal(pane: string?): boolean
	return pane ~= nil and (state.kinds:peek())[pane] == nil
end
-- A plugin panel's plugin and panel ids ("panel:<plugin>/<panel>").
function M.panel_of(pane: string?): (string?, string?)
	local kind = if pane then (state.kinds:peek())[pane] else nil
	if kind then
		return kind:match("^panel:([^/]+)/(.+)$")
	end
	return nil, nil
end
-- The plugin `id` as the host last found it.
function M.plugin(id: string?): state.PluginInfo?
	for _, plugin in state.plugins:get() do
		if plugin.id == id then
			return plugin
		end
	end
	return nil
end

-- A browser pane's page: the host keeps it across reloads, as it does
-- shells.
function M.page(pane: string): Web
	return ui.web("web:" .. pane, { profile = "default" })
end

local function quote(s: string): string
	return "'" .. (s:gsub("'", "'\\''")) .. "'"
end

-- A terminal pane's session, made the first time; every call gives it this
-- run's handlers.
function M.session(pane: string): Term?
	if not M.is_terminal(pane) then
		return nil
	end
	M.made[pane] = true
	local cwd = M.spawn_dir[pane] or state.launch_dir
	local attach = state.attach
	return ui.term(pane, {
		cwd = cwd,
		command = if attach ~= "" then `{attach} --attach {quote(pane)} --cwd {quote(cwd)}` else nil,
		-- The process exited (no question), or a close binding asked.
		on_close = function(alive)
			M.close_pane(pane, not alive)
		end,
	})
end

function M.describe(pane: string?): names.Description
	if not pane then
		return { name = "", icon = "terminal", busy = false }
	end
	local kind = (state.kinds:get())[pane]
	if kind == "browser" then
		return names.describe_page(M.page(pane))
	elseif kind == "plugins" then
		return { name = "Plugins", icon = "plugin", busy = false }
	elseif kind then
		local plugin_id, panel_id = M.panel_of(pane)
		local plugin = M.plugin(plugin_id)
		for _, panel in if plugin then plugin.panels else {} do
			if panel.id == panel_id then
				return { name = panel.title, icon = "plugin", busy = false }
			end
		end
		return { name = panel_id or "Panel", icon = "plugin", busy = false }
	end
	return names.describe(ui.term(pane), state.home)
end

-- Where a shell starts given a directory to start in: there, unless it is
-- none or the root (where an app opened from the Finder starts, not a place
-- anyone works), then where the app was launched (home from the Finder).
function M.start_dir(dir: string?): string
	return if dir and dir ~= "" and dir ~= "/" then dir else state.launch_dir
end

-- The directory a pane's shell is in, for what opens next to it.
function M.cwd_of(pane: string?): string
	local pwd = if M.is_terminal(pane) then names.path_of(ui.term(pane :: string).pwd) else nil
	return M.start_dir(pwd)
end

-- Keyboard focus to a pane's terminal or page.
function M.focus_view(pane: string)
	if M.is_browser(pane) then
		M.page(pane):focus()
	elseif M.is_terminal(pane) then
		ui.term(pane):focus()
	else
		state.panel_focus:set(pane)
	end
end

-- Whether closing the pane would end something running.
function M.needs_confirm(pane: string): boolean
	return M.is_terminal(pane) and ui.term(pane):needs_confirm()
end

return M
