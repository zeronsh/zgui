-- The active tab's panes and dividers. A pane or a tab dragged over them
-- shows, live, where it would go; dropping keeps it.
local theme = require("../theme")
local state = require("../state")
local actions = require("../actions")
local frames = require("../frames")
local Pane = require("./pane")
local Divider = require("./divider")

local active, drop = state.active, state.drop

return function(): Node
	return ui.div {
		grow = 1,
		min_h = 0,
		mx = theme.inset,
		mb = theme.inset,
		mt = function()
			return if state.sidebar:get() then theme.inset else 0
		end,
		-- The box the panes' insets count from (not the lists' own nodes).
		relative = true,
		on_mount = function(el)
			frames.set_area(el)
		end,
		-- The active tab itself comes here lifted out of the strip, when
		-- another can show.
		drop_when = function(payload: state.Payload)
			return payload.kind == "pane"
				or (payload.kind == "tab" and (payload.id ~= active:peek() or #state.tabs:peek() > 1))
		end,
		on_drag_over = function(payload: state.Payload, e)
			actions.hide_slot()
			if payload.kind == "tab" then
				actions.lift(payload.id)
			end
			local target, zone = frames.target_at(e.local_x, e.local_y)
			if target == payload.id then
				target = nil
			end
			local d = drop:peek()
			if d.target ~= target or d.zone ~= zone or d.id ~= payload.id then
				drop:set(if target then { target = target, zone = zone, kind = payload.kind, id = payload.id } else {})
			end
		end,
		on_drag_leave = function()
			if (drop:peek()).target then
				drop:set {}
			end
		end,
		on_drop = function()
			local d = drop:peek()
			drop:set {}
			frames.commit_drop(d)
		end,
		ui.each(frames.mounted_ids, function(id: string)
			return Pane { pane = id }
		end, {
			-- A pane moved to another tab is built again there.
			key = function(id: string)
				return `{id}@{actions.tab_of(id) or ""}`
			end,
		}),
		ui.each(frames.divider_ids, function(id: string)
			return Divider { id = id }
		end, {
			key = function(id: string)
				return id
			end,
		}),
	}
end
