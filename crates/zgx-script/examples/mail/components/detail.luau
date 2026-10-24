-- The open message: its header, actions and selectable body.
local state = require("../state")
local actions = require("../actions")

local function find(id: number?): state.Message?
	if id == nil then
		return nil
	end
	for _, m in state.messages:get() do
		if m.id == id then
			return m
		end
	end
	return nil
end

local open = ui.computed(function()
	return find(state.selected:get())
end)

local function action(label: string, tip: string, run: () -> ()): Node
	return ui.button { tooltip = tip, on_click = run, label }
end

return function(): Node
	return ui.column {
		label = "Message",
		grow = 1,
		h_full = true,
		bg = "$surface.raised",
		ui.presence(function()
			local m = open:get()
			return if m then m.id else nil
		end, function(id: number): Node
			local m = open:peek()
			if m == nil then
				return ui.column {}
			end
			return ui.column {
				grow = 1,
				pt = 40,
				px = 28,
				gap = 12,
				-- The actions wrap onto a second line in a narrow pane; Back
				-- returns to the list when the window shows one pane.
				ui.row {
					role = "toolbar",
					label = "Message",
					gap = 8,
					flex_wrap = true,
					ui.show(
						function()
							return state.layout:get() == "narrow"
						end,
						action("‹ Back", "Back to the messages", function()
							state.selected:set(nil)
						end)
					),
					action("Reply", "Reply to the sender", function()
						actions.reply(id)
					end),
					action(if m.starred then "Unstar" else "Star", "Flag it to find it again", function()
						actions.toggle_star(id)
					end),
					action("Archive", "Move it out of the inbox", function()
						actions.move(id, "Archive")
					end),
					action("Delete", "Move it to the trash", function()
						actions.move(id, "Trash")
					end),
				},
				ui.text {
					m.subject,
					id = "subject",
					heading = 1,
					text_size = "$font.size.xl",
					bold = true,
					wrap = true,
				},
				ui.rich {
					wrap = true,
					ui.span { m.from, bold = true },
					ui.span { `  <{m.email}>`, color = "$text.secondary" },
					ui.span { `  ·  {m.date}`, color = "$text.tertiary" },
				},
				ui.div { h = 1, bg = "$border.subtle" },
				ui.scroll {
					grow = 1,
					w_full = true,
					scrollbar = true,
					-- The body, read and selected as a document.
					ui.column {
						selectable = true,
						label = "Message body",
						gap = 12,
						ui.text { m.body, wrap = true, line_height = 22 },
					},
				},
			}
		end, { grow = 1 }),
		ui.show(
			function()
				return open:get() == nil
			end,
			ui.column {
				grow = 1,
				w_full = true,
				items = "center",
				justify = "center",
				ui.text { "No message selected", color = "$text.tertiary" },
			}
		),
	}
end
