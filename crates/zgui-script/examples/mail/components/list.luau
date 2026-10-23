-- The message list: virtualized, a row per message of the mailbox shown.
local state = require("../state")
local actions = require("../actions")
local visible = require("@lib/filter")

local shown = ui.computed(function()
	return visible(state.messages:get(), state.mailbox:get(), state.query:get())
end)

local offset = ui.state("offset", 0)
local list_ref = ui.ref()

-- Where message `id` is in the list shown, from 1.
local function position(id: number?): number?
	for i, m in shown:peek() do
		if m.id == id then
			return i
		end
	end
	return nil
end

local function row_height(): number
	return if state.density:peek() == "compact" then 52 else 72
end

local function row(i: number): Node
	local message = function(): state.Message?
		return shown:get()[i]
	end
	return ui.column {
		id = `row-{i}`,
		w_full = true,
		h = row_height(),
		px = 14,
		py = 8,
		gap = 2,
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
		ui.row {
			gap = 6,
			items = "center",
			ui.text {
				function()
					local m = message()
					return if m and m.unread then "●" else ""
				end,
				color = "$accent",
				text_size = 8,
				w = 8,
			},
			ui.text {
				function()
					local m = message()
					return if m then m.from else ""
				end,
				bold = true,
				grow = 1,
				truncate = true,
			},
			ui.text {
				function()
					local m = message()
					return if m then m.date else ""
				end,
				color = "$text.tertiary",
				text_size = "$font.size.sm",
			},
		},
		ui.text {
			function()
				local m = message()
				return if m then m.subject else ""
			end,
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
				truncate = true,
			}
		),
	}
end

return function(): Node
	return ui.column {
		w = 360,
		h_full = true,
		bg = "$surface.base",
		ui.row {
			w_full = true,
			pt = 40,
			px = 12,
			pb = 8,
			ui.input {
				id = "search",
				value = state.query,
				label = "Search",
				placeholder = "Search",
				w_full = true,
			},
		},
		ui.list {
			id = "messages",
			count = function()
				return #shown:get()
			end,
			height = row_height(),
			row = row,
			offset = offset,
			ref = list_ref,
			scrollbar = true,
			w_full = true,
			grow = 1,
			-- Up and Down move through the messages, as a mail app's do,
			-- keeping the open one in view.
			focusable = true,
			on_key = function(e)
				if e.key ~= "ArrowDown" and e.key ~= "ArrowUp" then
					return false
				end
				local list = shown:peek()
				local at = position(state.selected:peek()) or 0
				local next = math.clamp(at + (if e.key == "ArrowDown" then 1 else -1), 1, #list)
				if list[next] then
					actions.open(list[next].id)
					local h = row_height()
					local top = (next - 1) * h
					local element = list_ref:peek()
					local view = if element then element.height or 0 else 0
					if top < offset:peek() then
						offset:set(top)
					elseif top + h > offset:peek() + view then
						offset:set(top + h - view)
					end
				end
				return true
			end,
		},
	}
end
