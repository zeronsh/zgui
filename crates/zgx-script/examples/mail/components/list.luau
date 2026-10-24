-- The message list: virtualized, a row per message of the mailbox shown.
local state = require("../state")
local actions = require("../actions")
local visible = require("@lib/filter")

local shown = ui.computed(function()
	return visible(state.messages:get(), state.mailbox:get(), state.query:get())
end)

local offset = ui.state("offset", 0)

local function row_height(): number
	return if state.density:peek() == "compact" then 52 else 72
end

local function row(i: number): Node
	local message = function(): state.Message?
		return shown:get()[i]
	end
	return ui.row {
		id = `row-{i}`,
		ref = state.rows,
		role = "option",
		-- Read as one name, as a mail app's rows are: whether it is unread,
		-- who, what, when.
		label = function()
			local m = message()
			if not m then
				return ""
			end
			local parts = { m.from, m.subject, m.date }
			if state.is_unread(m) then
				table.insert(parts, 1, "Unread")
			end
			if m.starred then
				table.insert(parts, "Starred")
			end
			return table.concat(parts, ", ")
		end,
		selected = function()
			local m = message()
			return m ~= nil and state.selected:get() == m.id
		end,
		w_full = true,
		h = row_height(),
		pl = 6,
		pr = 14,
		py = 8,
		gap = 0,
		bg = function()
			local m = message()
			return if m and state.selected:get() == m.id then "$selection.item" else "#00000000"
		end,
		hover = { bg = "$hover" },
		on_click = function()
			local m = message()
			if m then
				actions.open(m.id)
			end
		end,
		-- The arrows move focus through the rows; the one focused opens, as
		-- a mail app's selection follows it.
		on_focus = function()
			local m = message()
			if m then
				actions.open(m.id)
			end
		end,
		context_menu = function()
			local m = message()
			if not m then
				return nil
			end
			local id = m.id
			return {
				ui.menu_item {
					"Reply",
					on_click = function()
						actions.reply(id)
					end,
				},
				ui.menu_item {
					if m.starred then "Unstar" else "Star",
					on_click = function()
						actions.toggle_star(id)
					end,
				},
				ui.menu_item {
					"Mark as Unread",
					on_click = function()
						actions.mark_unread(id)
					end,
				},
				ui.div { h = 1, my = 4, bg = "$border.subtle" },
				ui.menu_item {
					"Archive",
					on_click = function()
						actions.move(id, "Archive")
					end,
				},
				ui.menu_item {
					"Move to Trash",
					on_click = function()
						actions.move(id, "Trash")
					end,
				},
			}
		end,
		-- The unread dot, in a gutter of its own: sender, subject and preview
		-- line up whether it shows or not.
		ui.text {
			function()
				local m = message()
				return if m and state.is_unread(m) then "●" else ""
			end,
			color = "$accent",
			text_size = 8,
			aria_hidden = true,
			w = 14,
			shrink = 0,
			pt = 5,
			align = "center",
		},
		ui.column {
			grow = 1,
			min_w = 0,
			gap = 2,
			ui.row {
				gap = 6,
				items = "center",
				ui.text {
					function()
						local m = message()
						return if m then m.from else ""
					end,
					bold = true,
					grow = 1,
					min_w = 0,
					truncate = true,
				},
				ui.text {
					function()
						local m = message()
						return if m then m.date else ""
					end,
					color = "$text.tertiary",
					text_size = "$font.size.sm",
					shrink = 0,
				},
			},
			ui.text {
				function()
					local m = message()
					return if m then m.subject else ""
				end,
				min_w = 0,
				truncate = true,
			},
			ui.show(
				function()
					return state.density:get() == "comfortable"
				end,
				ui.text {
					function()
						local m = message()
						return if m then m.preview else ""
					end,
					color = "$text.secondary",
					text_size = "$font.size.sm",
					min_w = 0,
					truncate = true,
				}
			),
		},
	}
end

return function(): Node
	return ui.column {
		-- The whole window when it shows one pane at a time.
		w = function()
			return if state.layout:get() == "narrow" then "100%" else 360
		end,
		h_full = true,
		bg = "$surface.base",
		ui.row {
			w_full = true,
			pt = 40,
			px = 12,
			pb = 8,
			gap = 8,
			items = "center",
			-- The mailboxes, while the window is too narrow for the sidebar.
			ui.show(
				function()
					return state.layout:get() ~= "wide"
				end,
				ui.button {
					tooltip = "Mailboxes",
					on_click = ui.set(state.drawer, true),
					shrink = 0,
					"☰",
				}
			),
			ui.input {
				id = "search",
				value = state.query,
				label = "Search",
				placeholder = "Search",
				grow = 1,
			},
		},
		ui.list {
			id = "messages",
			role = "listbox",
			label = "Messages",
			count = function()
				return #shown:get()
			end,
			height = row_height(),
			row = row,
			offset = offset,
			scrollbar = true,
			w_full = true,
			grow = 1,
			-- One Tab stop; the arrows, Page Up and Down, Home and End move
			-- through the messages, as a mail app's list does.
			keyboard = true,
		},
		-- What the last action did; a screen reader says it as it changes
		-- (a status is politely live).
		ui.text {
			function()
				return state.status:get()
			end,
			id = "status",
			role = "status",
			px = 14,
			py = 6,
			color = "$text.secondary",
			text_size = "$font.size.sm",
			truncate = true,
			min_w = 0,
		},
	}
end
