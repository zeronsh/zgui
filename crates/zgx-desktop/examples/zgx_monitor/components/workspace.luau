-- The panels and their dividers. A panel or a process dragged over them
-- shows, live, where it would go; dropping keeps it.
local theme = require("../theme")
local icons = require("../icons")
local state = require("../state")
local actions = require("../actions")
local frames = require("../frames")
local Panel = require("./panel")
local Divider = require("./divider")

local root, drop, area = state.root, state.drop, state.area

-- With no panel open: where to start, and a place to drop a process.
local function empty(): Node
	local over = ui.state("empty_over", false)
	return ui.column {
		absolute = true,
		left = 0,
		top = 0,
		w_full = true,
		h_full = true,
		items = "center",
		justify = "center",
		gap = 6,
		rounded = frames.radius,
		border = 1,
		border_dash = { 6, 5 },
		border_color = function()
			return if over:get() then "#a99cff80" else "#ffffff1f"
		end,
		bg = function()
			return if over:get() then "#a99cff0f" else theme.clear
		end,
		drop_when = function(payload: state.Payload)
			return payload.kind == "process"
		end,
		on_drag_over = function()
			over:set(true)
		end,
		on_drag_leave = function()
			over:set(false)
		end,
		on_drop = function(payload: state.Payload)
			over:set(false)
			actions.open(payload.pid)
		end,
		ui.svg { code = icons.pulse, w = 24, h = 24, tint = theme.quaternary, mb = 6 },
		ui.text { "No panels open", text_size = 14, weight = 500, color = theme.secondary },
		ui.text { "Pick a process in the sidebar, or drag one here", text_size = 12, color = theme.tertiary },
	}
end

local function content(): Node
	return ui.div {
		grow = 1,
		min_w = 0,
		relative = true,
		mr = theme.inset,
		mb = theme.inset,
		on_resize = function(size)
			local a = area:peek()
			if a.w ~= size.w or a.h ~= size.h then
				area:set { w = size.w, h = size.h }
			end
		end,
		drop_when = function(payload: state.Payload)
			return root:peek() ~= nil and (payload.kind == "panel" or payload.kind == "process")
		end,
		on_drag_over = function(payload: state.Payload, e)
			local target, zone = frames.target_at(e.local_x, e.local_y, payload.kind)
			if payload.kind == "panel" and target == payload.id then
				target = nil
			end
			local id = if payload.kind == "panel" then payload.id else actions.next_panel()
			local d = drop:peek()
			if d.target ~= target or d.zone ~= zone or d.id ~= id or d.pid ~= payload.pid then
				drop:set(
					if target then { target = target, zone = zone, kind = payload.kind, id = id, pid = payload.pid } else {}
				)
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
			actions.commit_drop(d)
		end,
		ui.show(function()
			return root:get() == nil
		end, empty()),
		ui.each(frames.pane_ids, function(id: string)
			return Panel { pane = id }
		end, {
			key = function(id: string)
				return id
			end,
			-- A closed panel shrinks away where it was.
			exit = { opacity = 0, scale = 0.96, duration = 160, ease = "outQuad" },
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

return content
