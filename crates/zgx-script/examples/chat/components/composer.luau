-- The composer: a text area that grows with what is typed (to a limit),
-- Enter to send and Shift+Enter for a new line, and Send, or Stop while a
-- reply streams in. Each conversation keeps its own draft.
local state = require("../state")
local api = require("../api")

-- The draft of the conversation shown, both ways.
local function draft(): string
	return state.drafts:get()[state.current:get()] or ""
end
local function edit(text: string)
	state.drafts:set_key(state.current:peek(), if text == "" then nil else text)
end
local function send()
	api.send(state.drafts:peek()[state.current:peek()] or "")
end
local function streaming(): boolean
	return state.streaming:get() == state.current:get()
end

return function(): Node
	return ui.column {
		w_full = true,
		px = 24,
		pb = 18,
		pt = 6,
		ui.column {
			w_full = true,
			max_w = 760,
			self = "center",
			gap = 6,
			p = 10,
			rounded = 16,
			bg = "$surface.raised",
			border = 1,
			border_color = "$border.subtle",
			ui.textarea {
				id = "composer",
				ref = state.composer,
				label = "Message",
				placeholder = "Ask anything",
				value = draft,
				on_change = edit,
				wrap = true,
				w_full = true,
				h = "auto",
				max_h = 200,
				bg = "#00000000",
				on_key = function(e: KeyEvent): boolean
					if e.key == "Enter" and not (e.shift or e.alt or e.ctrl or e.meta) then
						send()
						return true
					end
					return false
				end,
			},
			ui.row {
				w_full = true,
				items = "center",
				gap = 8,
				ui.text {
					function()
						return if streaming() then "Esc to stop" else "Enter to send, Shift+Enter for a new line"
					end,
					color = "$text.tertiary",
					text_size = "$font.size.sm",
					grow = 1,
				},
				ui.show(streaming, ui.button { id = "stop", "Stop", on_click = api.stop }),
				ui.show(
					function()
						return not streaming()
					end,
					ui.button {
						id = "send",
						"Send",
						variant = "primary",
						disabled = function()
							return draft():match("%S") == nil or state.streaming:get() ~= nil
						end,
						on_click = send,
					}
				),
			},
		},
	}
end
