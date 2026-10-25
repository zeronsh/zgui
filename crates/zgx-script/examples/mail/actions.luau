-- What the app does to its state: buttons, menus and keys call these.
local state = require("./state")

local M = {}

-- The messages the list shows now, newest first as stored, for the list
-- and for the actions that move on to the next of them: kept in Rust as
-- the messages change (an archive judges the one it wrote, not 4000). A
-- search reads the subject and the sender, a test of its own.
M.shown = ui.sorted(state.messages, function()
	local mailbox, needle = state.mailbox:get(), string.lower(state.query:get())
	local filter: any = if mailbox == "Starred" then { starred = true } else { mailbox = mailbox }
	if needle ~= "" then
		filter = function(m: state.Message): boolean
			local here = if mailbox == "Starred" then m.starred else m.mailbox == mailbox
			return here and string.find(string.lower(m.subject .. " " .. m.from), needle, 1, true) ~= nil
		end
	end
	return { filter = filter }
end)

local function index_of(id: number): number?
	-- A message's id is its place (messages are only ever added at the end):
	-- one item read, not the list.
	local at = state.messages:at(id)
	if at and at.id == id then
		return id
	end
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
			set_read(state.messages:at(i) :: state.Message, true)
		end
	end)
end

function M.toggle_star(id: number)
	local i = index_of(id)
	if i then
		state.messages:set_key({ i, "starred" }, not (state.messages:at(i) :: state.Message).starred)
	end
end

function M.mark_unread(id: number)
	local i = index_of(id)
	if i then
		set_read(state.messages:at(i) :: state.Message, false)
	end
end

function M.move(id: number, mailbox: string)
	local i = index_of(id)
	if i then
		-- Where it was in the list: the message after it takes its place,
		-- and is the one open next, as a mail app moves on.
		local at = M.shown:index_of("id", id)
		local focus: number? = nil
		ui.batch(function()
			-- Its unread count moves with it.
			local m = state.messages:at(i) :: state.Message
			local read = state.read:peek()[tostring(m.id)]
			if if read == nil then m.unread else not read then
				local counts = state.unread_counts:peek()
				state.unread_counts:set_key({ m.mailbox }, (counts[m.mailbox] or 1) - 1)
				state.unread_counts:set_key({ mailbox }, (counts[mailbox] or 0) + 1)
			end
			state.messages:set_key({ i, "mailbox" }, mailbox)
			if state.selected:peek() == id then
				local place = if at then math.min(at, M.shown:len()) else nil
				local next = if place then M.shown:at(place) :: state.Message? else nil
				if next then
					M.open(next.id)
					focus = next.id
				else
					state.selected:set(nil)
				end
			end
			state.status:set(`Moved to {mailbox}`)
		end)
		-- Focus goes with it, to its row in the list, as a mail app's does
		-- (not to the window). The batch over, its row has moved up into
		-- the place; focusing it now, before the toolbar that had focus
		-- leaves, keeps focus from moving on into the next message's toolbar.
		if focus then
			for _, row in state.rows:elements() do
				if row.id == `message-{focus}` then
					row:focus()
				end
			end
		end
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
		local m = state.messages:at(i) :: state.Message
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
