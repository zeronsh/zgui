-- What the host samples, as the views read it: every live process in the
-- sidebar's order, and the last minute of each process a panel shows.
local state = require("./state")

export type Proc = { pid: number, name: string, cpu: number, gpu: number, mem: number, smooth: number }
export type Series = { number }
-- A process's last minute, oldest sample first.
export type History = { name: string, cpu: Series, gpu: Series, mem: Series }
export type Summary = { count: number?, cpu: number?, gpu: number? }

-- Every live process, each sample (the host writes it).
local procs = ui.shared("procs", {} :: { Proc })
-- The processes the panels show: the host publishes their histories.
local watch = ui.shared("watch", {} :: { number })

local M = {}

-- The whole machine: how many processes, their CPU, the GPU's own load.
M.summary = ui.shared("summary", {} :: Summary)

-- The sidebar's rows in order, and every live process by pid. Plain tables
-- rather than state: the rows read them many times a sample, and follow
-- `version`, one number, instead of each copying the whole list.
M.rows = {} :: { Proc }
M.by_pid = {} :: { [number]: Proc }
M.version = ui.state("version", 0)
M.count = ui.state("count", 0)

-- A process's last minute, published by the host while `watch` names it.
function M.history(pid: number): State<History?>
	return ui.shared(`history:{pid}`, nil :: History?)
end

-- The live process `pid`, following each sample.
function M.proc(pid: number?): Proc?
	M.version:get()
	return if pid then M.by_pid[pid] else nil
end

-- Keyboard browsing holds the order for a few samples, so ↑ and ↓ walk a
-- list that stays put.
local held = 0
function M.hold()
	held = 6
end

local function order(sort: state.Sort): (Proc, Proc) -> boolean
	if sort == "name" then
		return function(a, b)
			local x, y = string.lower(a.name), string.lower(b.name)
			if x ~= y then
				return x < y
			end
			return a.pid < b.pid
		end
	end
	local key = if sort == "gpu" then "gpu" elseif sort == "memory" then "mem" else "smooth"
	return function(a, b)
		local x, y = (a :: any)[key], (b :: any)[key]
		if x ~= y then
			return x > y
		end
		if a.mem ~= b.mem then
			return a.mem > b.mem
		end
		return a.pid < b.pid
	end
end

-- The rows, each sample and as the filter or sort changes. While the
-- pointer is on the list (or something is dragged from it, or the keys
-- browse it) a sample keeps the rows where they are: exited ones go, new
-- ones join the end. A change of filter or sort always sorts.
local last_filter: string? = nil
ui.effect(function()
	local list = procs:get()
	local needle = string.lower(state.query:get()):match("^%s*(.-)%s*$") or ""
	local sort = state.sort:get()
	local filter = `{sort}\n{needle}`
	local steady = filter == last_filter
	last_filter = filter
	local by_pid: { [number]: Proc } = {}
	local matches: { Proc } = {}
	for _, p in list do
		by_pid[p.pid] = p
		if
			needle == ""
			or string.find(string.lower(p.name), needle, 1, true) ~= nil
			or string.sub(tostring(p.pid), 1, #needle) == needle
		then
			table.insert(matches, p)
		end
	end
	if held > 0 then
		held -= 1
	end
	local hold = steady and (state.list_hover:peek() or (state.dragging:peek()).kind ~= nil or held > 0)
	local rows: { Proc } = {}
	if hold then
		local present: { [number]: Proc } = {}
		for _, p in matches do
			present[p.pid] = p
		end
		for _, old in M.rows do
			local p = present[old.pid]
			if p then
				table.insert(rows, p)
				present[old.pid] = nil
			end
		end
		for _, p in matches do
			if present[p.pid] then
				table.insert(rows, p)
			end
		end
	else
		table.sort(matches, order(sort))
		-- The process the monitor launched leads.
		for i, p in matches do
			if p.pid == state.launched and i > 1 then
				table.remove(matches, i)
				table.insert(matches, 1, p)
				break
			end
		end
		rows = matches
	end
	M.rows, M.by_pid = rows, by_pid
	M.count:set(#rows)
	M.version:set(M.version:peek() + 1)
end)

-- The host keeps publishing the histories of the processes the panels show.
ui.effect(function()
	local seen: { [number]: boolean } = {}
	local list: { number } = {}
	for _, pid in state.pids:get() do
		if not seen[pid] then
			seen[pid] = true
			table.insert(list, pid)
		end
	end
	-- A process dragged over a panel's edge previews in a panel of its own.
	local d = state.drop:get()
	if d.pid and not seen[d.pid] then
		table.insert(list, d.pid)
	end
	table.sort(list)
	watch:set(list)
end)

-- A process's name, live or (from its history) exited.
function M.name_of(pid: number?): string
	if not pid then
		return ""
	end
	local p = M.proc(pid)
	if p then
		return p.name
	end
	local h = M.history(pid):get()
	return if h then h.name else `pid {pid}`
end

return M
