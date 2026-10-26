-- The conversation: a virtualized list of messages, each as tall as its
-- text, keyed by id. It follows the end while a reply streams in, until
-- scrolled away. A row is built once; a reply's row follows its message
-- (`row`) as it grows, and no other row runs.
local state = require("../state")

type Message = state.Message

local function message(m: Message, _: State<number>, row: State<Message>): Node
	local mine = m.role == "user"
	return ui.row {
		id = `message-{m.id}`,
		w_full = true,
		px = 24,
		py = 10,
		justify = if mine then "end" else "start",
		ui.column {
			role = "article",
			label = if mine then "You" else "Assistant",
			max_w = 720,
			min_w = 0,
			grow = if mine then 0 else 1,
			gap = 8,
			px = if mine then 14 else 0,
			py = if mine then 10 else 0,
			rounded = 14,
			bg = if mine then "$surface.raised" else nil,
			selectable = true,
			-- What you wrote, as you wrote it; a reply, as markdown.
			if mine
				then ui.text { m.text, literal = true, wrap = true }
				else ui.markdown_view {
					function()
						return row:get().text
					end,
					w_full = true,
				},
			ui.show(function()
				local now = row:get()
				return now.streaming == true and now.text == ""
			end, ui.text { "Thinking…", color = "$text.tertiary", id = "thinking" }),
			ui.show(
				function()
					return row:get().error ~= nil
				end,
				ui.text {
					function()
						return row:get().error or ""
					end,
					id = `error-{m.id}`,
					color = "$danger",
					text_size = "$font.size.sm",
				}
			),
		},
	}
end

return function(): Node
	-- Where each conversation was scrolled: kept as another shows, and
	-- given back when it shows again (a new one shows its end).
	local offset = ui.state("transcript_offset", 0)
	local offsets: { [string]: number } = {}
	local shown = state.current:peek()
	ui.effect(function()
		local id = state.current:get()
		if id ~= shown then
			offsets[shown] = offset:peek()
			shown = id
			offset:set(offsets[id] or math.huge)
		end
	end)
	return ui.list {
		id = "transcript",
		ref = state.transcript,
		label = "Messages",
		data = state.thread,
		key = function(m: Message): string
			return m.id
		end,
		row = message,
		offset = offset,
		measured = true,
		follow = true,
		height = 64,
		scrollbar = true,
		w_full = true,
		grow = 1,
		py = 12,
	}
end
