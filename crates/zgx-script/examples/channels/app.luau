-- Channels: a team's chat over a WebSocket that reconnects on its own (a
-- banner while it does), messages sent at once and confirmed or refused
-- later (a refused one can be sent again), unread counts and who is
-- online, a list that stays at the end as messages arrive and loads earlier
-- ones as it reaches its start, a composer with mentions at the caret and
-- emoji, files dropped anywhere or attached and sent with their progress,
-- and a system notification and the app icon's badge for what arrives
-- while the window is in the background.
local state = require("./state")
local live = require("./live")
local Sidebar = require("@components/sidebar")
local Messages = require("@components/messages")
local Composer = require("@components/composer")

-- Files dragged over the window: a hint over everything until dropped.
local dragging = ui.state("dragging", false)

-- Back in front: the channel shown is read, and the badge follows.
ui.effect(function()
	if ui.window.active:get() then
		local channel = state.current:get()
		if (state.unread:peek()[channel] or 0) > 0 then
			state.unread:set_key(channel, nil)
		end
		ui.badge(state.total_unread:peek())
	end
end)

local socket = live.socket

local banner = ui.show(
	function()
		return socket.status:get() ~= "open"
	end,
	ui.row {
		id = "banner",
		px = 16,
		py = 8,
		gap = 12,
		items = "center",
		bg = "$surface.raised",
		ui.text {
			grow = 1,
			function()
				local status = socket.status:get()
				if status == "connecting" then
					return "Connecting…"
				elseif status == "reconnecting" then
					local wait = socket.retry_in:get()
					return if wait > 0
						then `Connection lost. Reconnecting in {math.ceil(wait / 1000)} s…`
						else "Reconnecting…"
				end
				local err = socket.error
				return if err then `Offline: {err}` else "Offline"
			end,
		},
		ui.button {
			id = "reconnect",
			variant = "ghost",
			"Try now",
			on_click = socket.reconnect,
		},
	}
)

return ui.overlay {
	w_full = true,
	h_full = true,
	on_file_hover = function(over: boolean)
		dragging:set(over)
	end,
	on_file_drop = function(files: { File })
		for _, file in files do
			live.upload(file)
		end
	end,
	ui.row {
		w_full = true,
		h_full = true,
		Sidebar {},
		ui.column {
			grow = 1,
			h_full = true,
			ui.row {
				px = 16,
				py = 10,
				ui.text {
					id = "title",
					weight = 600,
					function()
						return `# {state.current:get()}`
					end,
				},
			},
			banner,
			Messages {},
			Composer {},
		},
	},
	ui.show(
		dragging,
		ui.column {
			id = "drop-hint",
			absolute = true,
			top = 12,
			left = 12,
			right = 12,
			bottom = 12,
			rounded = 12,
			border = 2,
			border_color = "$accent",
			bg = "$surface.overlay",
			justify = "center",
			items = "center",
			pointer_events = false,
			ui.text { "Drop files to upload", size = 18, weight = 600 },
		}
	),
}
