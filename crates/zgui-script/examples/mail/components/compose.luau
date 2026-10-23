-- Writing a message: a sheet with the recipients, the subject and a
-- spellchecked body.
local state = require("../state")
local actions = require("../actions")

local function field(label: string, value: State<string>): Node
	return ui.row {
		w_full = true,
		gap = 8,
		items = "center",
		ui.text { label, w = 64, shrink = 0, color = "$text.secondary" },
		ui.input { value = value, label = label, grow = 1 },
	}
end

return function(): Node
	return ui.modal {
		open = state.composing,
		label = "New Message",
		w = 560,
		gap = 10,
		ui.text { "New Message", text_size = "$font.size.lg", bold = true },
		field("To", state.draft_to),
		field("Subject", state.draft_subject),
		ui.textarea {
			id = "body",
			value = state.draft_body,
			label = "Message",
			placeholder = "Write your message",
			spellcheck = state.spellcheck,
			w_full = true,
			h = 220,
		},
		ui.row {
			gap = 8,
			justify = "end",
			ui.button { on_click = ui.set(state.composing, false), "Cancel" },
			ui.button { id = "send", on_click = actions.send, "Send" },
		},
	}
end
