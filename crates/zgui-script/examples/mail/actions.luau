-- What the app does to its state: buttons, menus and keys call these.
local state = require("./state")
local visible = require("@lib/filter")

local M = {}

local function index_of(id: number): number?
	for i, m in state.messages:peek() do
		if m.id == id then
			return i
		end
	end
	return nil
end

-- `m` read or unread now, its mailbox's count with it: one key each, not
-- the 4000 messages (whose list and filter would run again).
local function set_read(m: state.Message, read: boolean)
	local was = state.read:peek()[tostring(m.id)]
	local unread = if was == nil then m.unread else not was
	if unread == not read then
		return
	end
	ui.batch(function()
		state.read:set_key({ tostring(m.id) }, read)
		local n = state.unread_counts:peek()[m.mailbox] or 0
		state.unread_counts:set_key({ m.mailbox }, n + (if read then -1 else 1))
	end)
end

function M.open(id: number)
	ui.batch(function()
		state.selected:set(id)
		local i = index_of(id)
		if i then
			set_read(state.messages:peek()[i], true)
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
		set_read(state.messages:peek()[i], false)
	end
end

-- The messages the list shows now.
local function shown(): { state.Message }
	return visible(state.messages:peek(), state.mailbox:peek(), state.query:peek())
end

function M.move(id: number, mailbox: string)
	local i = index_of(id)
	if i then
		-- Where it was in the list: the message after it takes its place,
		-- and is the one open next, as a mail app moves on.
		local at: number? = nil
		for position, m in shown() do
			if m.id == id then
				at = position
				break
			end
		end
		ui.batch(function()
			-- Its unread count moves with it.
			local m = state.messages:peek()[i]
			local read = state.read:peek()[tostring(m.id)]
			if if read == nil then m.unread else not read then
				local counts = state.unread_counts:peek()
				state.unread_counts:set_key({ m.mailbox }, (counts[m.mailbox] or 1) - 1)
				state.unread_counts:set_key({ mailbox }, (counts[mailbox] or 0) + 1)
			end
			state.messages:set_key({ i, "mailbox" }, mailbox)
			if state.selected:peek() == id then
				local list = shown()
				local place = if at then math.min(at, #list) else nil
				local next = if place then list[place] else nil
				if next then
					M.open(next.id)
					-- Focus goes with it, to its row in the list, as a mail
					-- app's does (not to the window): once the list shows it.
					ui.after(0, function()
						for _, row in state.rows:elements() do
							if row.id == `row-{place}` then
								row:focus()
							end
						end
					end)
				else
					state.selected:set(nil)
				end
			end
			state.status:set(`Moved to {mailbox}`)
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
		state.status:set("Message sent")
	end)
end

-- New mail: a message arriving in the inbox, announced.
function M.receive()
	local next_id = #state.messages:peek() + 1
	ui.batch(function()
		state.messages:insert(1, {
			id = next_id,
			mailbox = "Inbox",
			from = "Grace Hopper",
			email = "grace@navy.mil",
			subject = "The nanosecond, again",
			preview = "Here is the length of wire I promised, for the demo on Friday.",
			body = "Here is the length of wire I promised, for the demo on Friday.",
			date = "Now",
			unread = true,
			starred = false,
		})
		local inbox = state.unread_counts:peek().Inbox or 0
		state.unread_counts:set_key({ "Inbox" }, inbox + 1)
		state.status:set("New message from Grace Hopper: The nanosecond, again")
	end)
end

return M
