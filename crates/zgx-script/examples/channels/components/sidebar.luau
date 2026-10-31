-- The channels, each with what is unread in it, and who is online.
local state = require("../state")

type Channel = state.Channel
type User = state.User

return ui.component(function()
	local function channel_row(channel: Channel)
		return ui.button {
			id = `channel-{channel.id}`,
			variant = "ghost",
			justify = "start",
			gap = 8,
			bg = function()
				return if state.current:get() == channel.id then "$selection.item" else "transparent"
			end,
			on_click = function()
				ui.batch(function()
					state.current:set(channel.id)
					state.unread:set_key(channel.id, nil)
				end)
				ui.badge(state.total_unread:peek())
			end,
			ui.text { grow = 1, `# {channel.name}` },
			ui.show(
				function()
					return (state.unread:get()[channel.id] or 0) > 0
				end,
				ui.text {
					id = `unread-{channel.id}`,
					px = 6,
					rounded = 8,
					bg = "$accent",
					color = "$accent.text",
					size = 11,
					function()
						return tostring(state.unread:get()[channel.id] or 0)
					end,
				}
			),
		}
	end
	local function user_row(user: User)
		return ui.row {
			gap = 8,
			items = "center",
			ui.div {
				id = `presence-{user.id}`,
				w = 8,
				h = 8,
				rounded = 4,
				bg = function()
					local now = state.users:get()[user.id]
					return if now and now.online then "$success" else "$text.disabled"
				end,
				label = function()
					local now = state.users:get()[user.id]
					return if now and now.online then "online" else "away"
				end,
			},
			ui.text { user.name, color = "$text.secondary" },
		}
	end
	local people = ui.computed(function(): { User }
		local list: { User } = {}
		for _, user in state.users:get() do
			if user.id ~= state.me:get() then
				table.insert(list, user)
			end
		end
		table.sort(list, function(a: User, b: User)
			return a.name < b.name
		end)
		return list
	end)
	return ui.column {
		w = 220,
		p = 12,
		gap = 4,
		bg = "$surface.sunken",
		ui.text { "Channels", weight = 600, pb = 4 },
		ui.each(state.channels, channel_row, {
			key = function(c: Channel)
				return c.id
			end,
		}),
		ui.text { "People", weight = 600, pt = 16, pb = 4 },
		ui.each(people, user_row, {
			key = function(u: User)
				return u.id
			end,
		}),
	}
end)
