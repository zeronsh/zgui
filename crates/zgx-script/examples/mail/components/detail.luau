-- The open message: its header, actions and selectable body.
local state = require("../state")
local actions = require("../actions")

local function find(id: number?): state.Message?
	if id == nil then
		return nil
	end
	-- A message's id is its place: one item read, not the list.
	local at = state.messages:at(id)
	if at and at.id == id then
		return at
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

local function action(label: string | () -> string, tip: string, run: () -> ()): Node
	return ui.button { tooltip = tip, on_click = run, label }
end

return function(): Node
	return ui.column {
		label = "Message",
		grow = 1,
		h_full = true,
		bg = "$surface.raised",
		-- Keyed by whether a message is open, not which: moving to the next
		-- message changes the texts in place rather than building the pane
		-- again.
		ui.presence(function()
			return if open:get() then "open" else nil
		end, function(): Node
			local function field(read: (state.Message) -> string): () -> string
				return function()
					local m = open:get()
					return if m then read(m) else ""
				end
			end
			-- Another message reads from its top.
			local scrolled = ui.use_state(0)
			ui.effect(function()
				state.selected:get()
				scrolled:set(0)
			end)
			-- What the actions act on: the message open when they run.
			local function on_open(run: (id: number) -> ()): () -> ()
				return function()
					local m = open:peek()
					if m then
						run(m.id)
					end
				end
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
					action("Reply", "Reply to the sender", on_open(actions.reply)),
					action(
						field(function(m)
							return if m.starred then "Unstar" else "Star"
						end),
						"Flag it to find it again",
						on_open(actions.toggle_star)
					),
					action(
						"Archive",
						"Move it out of the inbox",
						on_open(function(id)
							actions.move(id, "Archive")
						end)
					),
					action(
						"Delete",
						"Move it to the trash",
						on_open(function(id)
							actions.move(id, "Trash")
						end)
					),
				},
				ui.text {
					field(function(m)
						return m.subject
					end),
					id = "subject",
					heading = 1,
					text_size = "$font.size.xl",
					bold = true,
					wrap = true,
				},
				ui.rich {
					wrap = true,
					ui.span {
						field(function(m)
							return m.from
						end),
						bold = true,
					},
					ui.span {
						field(function(m)
							return `  <{m.email}>`
						end),
						color = "$text.secondary",
					},
					ui.span {
						field(function(m)
							return `  ·  {m.date}`
						end),
						color = "$text.tertiary",
					},
				},
				ui.div { h = 1, bg = "$border.subtle" },
				ui.scroll {
					grow = 1,
					w_full = true,
					scrollbar = true,
					offset = scrolled,
					-- The body, read and selected as a document.
					ui.column {
						selectable = true,
						label = "Message body",
						gap = 12,
						ui.text {
							field(function(m)
								return m.body
							end),
							wrap = true,
							line_height = 22,
						},
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
