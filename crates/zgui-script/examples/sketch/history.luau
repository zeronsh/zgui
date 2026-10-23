-- Undo and redo: snapshots of the shapes. A state's value is a read-only
-- snapshot sharing what didn't change with the one before, so keeping them
-- costs only what each change made new.
local state = require("./state")
local geometry = require("@lib/geometry")

type Snapshot = { geometry.Shape }

local M = {}
local past: { Snapshot } = {}
local future: { Snapshot } = {}
local LIMIT = 200

M.depth = ui.state("history_depth", { undo = 0, redo = 0 })
local function publish()
	M.depth:set { undo = #past, redo = #future }
end

-- Keep what the shapes are now, before a change.
function M.record()
	table.insert(past, state.shapes:peek())
	if #past > LIMIT then
		table.remove(past, 1)
	end
	table.clear(future)
	state.dirty:set(true)
	publish()
end

-- A gesture (a drag, a field typed) records once, at its end, if it changed
-- anything: `begin` notes the shapes, `finish` keeps them if they changed.
local pending: Snapshot? = nil
function M.begin()
	pending = state.shapes:peek()
end
function M.finish()
	local before = pending
	pending = nil
	if before and before ~= state.shapes:peek() then
		table.insert(past, before)
		table.clear(future)
		state.dirty:set(true)
		publish()
	end
end

local function keep_selection()
	local ids = {}
	for _, s in state.shapes:peek() do
		ids[s.id] = true
	end
	local kept = {}
	for _, id in state.selected:peek() do
		if ids[id] then
			table.insert(kept, id)
		end
	end
	state.selected:set(kept)
end

function M.undo()
	local last = table.remove(past)
	if last then
		table.insert(future, state.shapes:peek())
		state.shapes:set(last)
		keep_selection()
		state.dirty:set(true)
		publish()
	end
end

function M.redo()
	local next = table.remove(future)
	if next then
		table.insert(past, state.shapes:peek())
		state.shapes:set(next)
		keep_selection()
		state.dirty:set(true)
		publish()
	end
end

-- A new or opened drawing starts its own history.
function M.reset()
	table.clear(past)
	table.clear(future)
	pending = nil
	publish()
end

return M
