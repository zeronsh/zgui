-- The app's state: every module that requires this one gets these States.
export type File = { name: string, size: number, type: string, url: string? }
export type Message = {
	-- The server's number once stored; a local "l1", "l2"… while sending.
	id: number | string,
	-- What its row is keyed by: the local id, kept once stored, so the row
	-- the person sent stays the same row.
	key: string,
	channel: string,
	from: string,
	text: string,
	at: number,
	files: { File }?,
	-- Sent and not yet answered.
	pending: boolean?,
	-- Why the server refused it; it can be sent again.
	failed: string?,
}
export type Channel = { id: string, name: string }
export type User = { id: string, name: string, online: boolean }
-- A file going up: how far it has got, and its task to cancel.
export type Upload = { id: string, name: string, size: number, sent: number, url: string? }

local M = {}

-- Who the server says we are, and who else there is.
M.me = ui.state("me", "")
M.channels = ui.state("channels", {} :: { Channel })
M.users = ui.state("users", {} :: { [string]: User })
-- Each channel's messages, oldest first, and the cursor of its page
-- before the first loaded (nil once the start is loaded).
M.messages = ui.state("messages", {} :: { [string]: { Message } })
M.before = ui.state("before", {} :: { [string]: number })
M.current = ui.state("current", "general")
-- Messages that arrived where the person was not looking.
M.unread = ui.state("unread", {} :: { [string]: number })
-- Who is typing in each channel, for a few seconds.
M.typing = ui.state("typing", {} :: { [string]: string })
-- What is being typed, in each channel.
M.drafts = ui.state("drafts", {} :: { [string]: string })
-- Files going up for the next message.
M.uploads = ui.state("uploads", {} :: { Upload })

-- The channel shown's messages.
M.thread = ui.computed(function(): { Message }
	return M.messages:get()[M.current:get()] or {}
end)

-- Every channel's unread count, added up: the app icon's badge.
M.total_unread = ui.computed(function(): number
	local total = 0
	for _, n in M.unread:get() do
		total += n
	end
	return total
end)

function M.name_of(id: string): string
	local user = M.users:peek()[id]
	return if user then user.name else id
end

return M
