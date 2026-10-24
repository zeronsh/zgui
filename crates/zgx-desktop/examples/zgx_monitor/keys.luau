-- The window's own keys (the menus' have modifiers): ↑ and ↓ browse the
-- process list in the active panel, ⏎ opens the process in a panel beside
-- it, ← and → move between panels while the filter is empty, Escape
-- clears the filter.
local state = require("./state")
local actions = require("./actions")

local M = {}

function M.on_key(e: KeyEvent): boolean
	if e.meta or e.ctrl or e.alt then
		return false
	end
	local typing = state.query:peek() ~= ""
	if e.key == "ArrowDown" or e.key == "ArrowUp" then
		actions.step(if e.key == "ArrowDown" then 1 else -1)
		return true
	elseif e.key == "Enter" then
		actions.split("row")
		return true
	elseif (e.key == "ArrowLeft" or e.key == "ArrowRight") and not typing then
		actions.cycle(if e.key == "ArrowRight" then 1 else -1)
		return true
	elseif e.key == "Escape" and typing then
		state.query:set("")
		return true
	end
	return false
end

return M
