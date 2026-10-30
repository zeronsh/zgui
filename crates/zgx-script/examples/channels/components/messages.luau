-- The channel's messages: a list measured as its rows mount, staying at
-- the end as messages arrive (and still where it is when scrolled back),
-- loading earlier pages as it reaches its start without the view jumping,
-- and a "jump to the latest" button while it is away from its end.
local state = require("../state")
local live = require("../live")

type Message = state.Message

local function time_of(at: number): string
	return os.date("%H:%M", at)
end

-- A message's attachments, a chip each (nothing without any).
local function files_of(message: Message)
	local chips: ContainerProps = { gap = 6, wrap = true }
	for _, file in message.files or {} do
		table.insert(
			chips,
			ui.text {
				px = 8,
				py = 4,
				rounded = 6,
				bg = "$surface.raised",
				`📎 {file.name}`,
			}
		)
	end
	return ui.row(chips)
end

-- Built again when its message changes (confirmed, refused): it takes
-- the index too.
local function row(message: Message, _index: State<number>)
	return ui.column {
		id = `message-{message.key}`,
		px = 16,
		py = 6,
		gap = 2,
		opacity = if message.pending then 0.6 else 1,
		ui.row {
			gap = 8,
			items = "baseline",
			ui.text { state.name_of(message.from), weight = 600 },
			ui.text { time_of(message.at), size = 11, color = "$text.tertiary" },
		},
		ui.markdown_view { message.text, breaks = true },
		files_of(message),
		ui.show(
			message.failed ~= nil,
			ui.row {
				gap = 8,
				items = "center",
				ui.text {
					id = `failed-{message.key}`,
					color = "$danger",
					size = 12,
					`Not sent: {message.failed or ""}`,
				},
				ui.button {
					id = `retry-{message.key}`,
					variant = "ghost",
					"Retry",
					on_click = function()
						live.retry(message)
					end,
				},
			}
		),
	}
end

return ui.component(function()
	local list = ui.ref()
	local at_end = ui.use_state(true)
	return ui.overlay {
		grow = 1,
		ui.list {
			id = "messages",
			ref = list,
			w_full = true,
			h_full = true,
			data = state.thread,
			key = function(message: Message)
				return message.key
			end,
			height = 48,
			measured = true,
			follow = true,
			at_end = at_end,
			on_start = function()
				live.older(state.current:peek())
			end,
			row = row,
		},
		ui.show(
			function()
				return not at_end:get()
			end,
			ui.button {
				id = "latest",
				absolute = true,
				bottom = 12,
				right = 16,
				variant = "primary",
				rounded = 16,
				"↓ Latest",
				on_click = function()
					local el = list:peek()
					if el then
						el:scroll_to_row(#state.thread:peek(), { align = "end" })
					end
				end,
			}
		),
	}
end)
