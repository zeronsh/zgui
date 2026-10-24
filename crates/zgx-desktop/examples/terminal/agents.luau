-- Coding agents running in panes, and what each is doing: what the sidebar
-- lists. The session server keeps every session's activity from all it
-- prints (terminal/activity.rs), shown or not: an agent in a tab not open,
-- or one that finished while the window was closed, is here. The host
-- publishes it as `agents` as it changes; this matches sessions to panes
-- and names them.
local state = require("./state")
local sessions = require("./sessions")
local actions = require("./actions")
local icons = require("./icons")
local names = require("@lib/names")
local layout = require("@lib/layout")

local M = {}

-- What an agent is doing, in the protocol's words: it needs the user
-- (blocked), failed (error), finished and unseen (done), runs (working), or
-- waits for its next instruction (idle).
export type Status = "blocked" | "error" | "done" | "working" | "idle"

export type Agent = {
	pane: string,
	tab: string,
	status: Status,
	-- With "blocked": what for ("permission", "question", "auth").
	kind: string?,
	-- What it works on (its title), and where (its directory).
	name: string,
	where: string,
	icon: icons.Name,
	tint: string?,
	-- When its status began (Unix seconds).
	since: number,
}

-- A session as the server reports it.
type Session = {
	process: string,
	idle: boolean,
	title: string,
	pwd: string,
	status: {
		state: Status,
		kind: string?,
		since_ms: number,
		msg: string?,
		app: string?,
		-- The program reported it (OSC 7501), not worked out.
		reported: boolean,
	},
}
-- `unsupported`: the session server predates this (it still runs from
-- before an update, keeping the shells): it lists nothing until restarted.
type Reported = { sessions: { [string]: Session }?, unsupported: boolean? }
local reported = ui.shared("agents", nil :: Reported?)

-- Whether the session server is too old to report agents.
M.unsupported = ui.computed(function(): boolean
	local all = reported:get()
	return all ~= nil and all.unsupported == true
end)

-- The sidebar's order: what needs the user first, what rests last.
M.ORDER = { blocked = 1, error = 2, done = 3, working = 4, idle = 5 } :: { [Status]: number }

-- Every terminal pane, with its tab.
local function panes(): { { pane: string, tab: string } }
	local out = {}
	for _, tab in state.tabs:get() do
		for _, pane in layout.leaves(tab.root) do
			if sessions.is_terminal(pane) then
				table.insert(out, { pane = pane, tab = tab.id })
			end
		end
	end
	return out
end

-- Whether a session runs an agent the sidebar lists: one it knows, or any
-- program that reports its own status.
local function is_agent(s: Session): boolean
	return names.agent(s.process) ~= nil or s.status.reported
end

-- Every agent, sorted: by status, then the most recent change first.
M.list = ui.computed(function(): { Agent }
	local out: { Agent } = {}
	local all = reported:get()
	local by_id = (if all then all.sessions else nil) or {}
	for _, p in panes() do
		local s = by_id[p.pane]
		if s and is_agent(s) then
			local desc = names.describe_parts(s.process, s.title, s.pwd, state.home)
			local dir = names.path_of(s.pwd)
			local where = if dir ~= "" then names.short(dir, state.home) else ""
			-- A title that only repeats the directory or the command line
			-- ("claude --resume") says less than the agent's name.
			local name = desc.name
			local command = name:sub(1, #s.process + 1)
			local base = dir:match("([^/]+)/*$")
			if name == where or name == dir or name == base or command == s.process or command == s.process .. " " then
				name = names.label(s.process)
			end
			table.insert(out, {
				pane = p.pane,
				tab = p.tab,
				status = s.status.state,
				kind = s.status.kind,
				name = name,
				where = where,
				icon = desc.icon,
				tint = desc.tint,
				since = s.status.since_ms / 1000,
			})
		end
	end
	table.sort(out, function(a: Agent, b: Agent)
		if a.status ~= b.status then
			return M.ORDER[a.status] < M.ORDER[b.status]
		end
		return a.since > b.since
	end)
	return out
end)

-- Looking at an agent answers it: the pane focused in the window (with its
-- terminal holding the keyboard) tells the server it was seen, so "done"
-- and what it said to get attention clear (what a program reports itself
-- stays until it reports otherwise).
ui.effect(function()
	local all = reported:get()
	local tab = state.active:get()
	local pane = (state.focus:get())[tab]
	local by_id = if all then all.sessions else nil
	if not (by_id and pane) or not sessions.is_terminal(pane) then
		return
	end
	local s = by_id[pane]
	local answers = s and (s.status.state == "done" or (s.status.state == "blocked" and not s.status.reported))
	if answers and ui.term(pane).focused then
		ui.invoke("seen_session", pane)
	end
end)

-- Show `agent`'s tab and give its pane focus (out of a zoom on another).
function M.open(agent: Agent)
	local zoomed = (state.zoom:peek())[agent.tab]
	if zoomed and zoomed ~= agent.pane then
		state.zoom:set_key(agent.tab, nil)
	end
	actions.focus_pane(agent.tab, agent.pane)
	actions.activate(agent.tab)
end

return M
