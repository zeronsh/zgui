-- What the app does to its state: buttons, menus and keys call these.
local state = require("./state")

local M = {}

local function index_of(id: number): number?
	for i, m in state.messages:peek() do
		if m.id == id then
			return i
		end
	end
	return nil
end

function M.open(id: number)
	ui.batch(function()
		state.selected:set(id)
		local i = index_of(id)
		if i and state.messages:peek()[i].unread then
			state.messages:set_key({ i, "unread" }, false)
		end
	end)
end

function M.toggle_star(id: number)
	local i = index_of(id)
	if i then
		state.messages:set_key({ i, "starred" }, not state.messages:peek()[i].starred)
	end
end

function M.mark_unread(id: number)
	local i = index_of(id)
	if i then
		state.messages:set_key({ i, "unread" }, true)
	end
end

function M.move(id: number, mailbox: string)
	local i = index_of(id)
	if i then
		ui.batch(function()
			state.messages:set_key({ i, "mailbox" }, mailbox)
			if state.selected:peek() == id then
				state.selected:set(nil)
			end
		end)
	end
end

function M.compose(to: string?, subject: string?)
	ui.batch(function()
		state.draft_to:set(to or "")
		state.draft_subject:set(subject or "")
		state.draft_body:set("")
		state.composing:set(true)
	end)
end

function M.reply(id: number)
	local i = index_of(id)
	if i then
		local m = state.messages:peek()[i]
		M.compose(m.email, "Re: " .. m.subject)
	end
end

function M.send()
	local next_id = #state.messages:peek() + 1
	ui.batch(function()
		state.messages:insert(1, {
			id = next_id,
			mailbox = "Sent",
			from = "Me",
			email = state.draft_to:peek(),
			subject = state.draft_subject:peek(),
			preview = string.sub(state.draft_body:peek(), 1, 90),
			body = state.draft_body:peek(),
			date = "Now",
			unread = false,
			starred = false,
		})
		state.composing:set(false)
	end)
end

return M
