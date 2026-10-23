-- The mailboxes, with how many unread messages each holds.
local state = require("../state")
local actions = require("../actions")

local boxes = { "Inbox", "Starred", "Sent", "Drafts", "Archive", "Trash" }

local function mailbox(name: string): Node
	return ui.row {
		-- A choice among the mailboxes, as assistive technology reads it.
		role = "option",
		selected = function()
			return state.mailbox:get() == name
		end,
		h = 30,
		px = 10,
		rounded = 6,
		items = "center",
		gap = 8,
		bg = function()
			return if state.mailbox:get() == name then "$selection.item" else "#00000000"
		end,
		hover = { bg = "$hover" },
		on_click = function()
			ui.batch(function()
				state.mailbox:set(name)
				state.selected:set(nil)
				state.drawer:set(false)
			end)
		end,
		-- The arrows move through the mailboxes (a list box is one Tab
		-- stop); the one focused shows, as the click would.
		on_focus = function()
			if state.mailbox:peek() ~= name then
				ui.batch(function()
					state.mailbox:set(name)
					state.selected:set(nil)
				end)
			end
		end,
		ui.text { name, grow = 1 },
		ui.text {
			function()
				local n = (state.unread_counts:get() or {})[name] or 0
				return if n > 0 then tostring(n) else ""
			end,
			color = "$text.secondary",
			text_size = "$font.size.sm",
		},
	}
end

return function(): Node
	local rows: { Node } = {}
	for _, name in boxes do
		table.insert(rows, mailbox(name))
	end
	return ui.column {
		w = 210,
		h_full = true,
		pt = 44,
		px = 10,
		gap = 2,
		bg = "$surface.sunken",
		ui.row {
			pb = 10,
			ui.button {
				grow = 1,
				on_click = function()
					actions.compose()
				end,
				tooltip = "Write a new message (⌘N)",
				"New Message",
			},
		},
		ui.column { role = "listbox", label = "Mailboxes", gap = 2, rows },
	}
end
