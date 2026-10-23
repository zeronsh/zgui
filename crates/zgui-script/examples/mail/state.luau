-- The app's state: every module that requires this one gets these States.
local sample = require("@lib/sample")

export type Message = sample.Message
export type Density = "comfortable" | "compact"
export type Appearance = "system" | "light" | "dark"
-- How the window is laid out, by its width: every pane, the list and the
-- message (the mailboxes in a drawer), or one pane at a time.
export type Layout = "wide" | "medium" | "narrow"

local M = {}
M.messages = ui.state("messages", sample.messages(4000))
-- Read or unread, where it differs from how a message arrived (id -> true
-- when read, false when marked unread): opening a message changes this,
-- not the messages, so the list and its filter do not run again.
M.read = ui.state("read", {} :: { [string]: boolean })
-- Unread messages by mailbox: counted once, then kept by what changes them.
local counts: { [string]: number } = {}
for _, m in M.messages:peek() do
	if m.unread then
		counts[m.mailbox] = (counts[m.mailbox] or 0) + 1
	end
end
M.unread_counts = ui.state("unread_counts", counts)
-- Whether `m` is unread now (followed, in a computed).
function M.is_unread(m: Message): boolean
	local read = M.read:get()[tostring(m.id)]
	return if read == nil then m.unread else not read
end
M.mailbox = ui.persist("mailbox", "Inbox")
M.selected = ui.state("selected", nil :: number?)
M.query = ui.state("query", "")
-- Composing: open, and the draft.
M.composing = ui.state("composing", false)
M.draft_to = ui.state("draft_to", "")
M.draft_subject = ui.state("draft_subject", "")
M.draft_body = ui.state("draft_body", "")
-- Settings, kept across launches; their window.
M.appearance = ui.persist("appearance", "system" :: Appearance)
M.density = ui.persist("density", "comfortable" :: Density)
M.spellcheck = ui.persist("spellcheck", true)
M.settings_open = ui.state("settings_open", false)
-- The window's root, and the layout its width calls for: read in a
-- computed, an element's width follows layout (a container query).
M.root = ui.ref()
-- The message list's mounted rows, to give one focus.
M.rows = ui.ref()
M.layout = ui.computed(function(): Layout
	local width = M.root.width or 1180
	return if width >= 1000 then "wide" elseif width >= 680 then "medium" else "narrow"
end)
-- What the last action did ("Message sent"): the list's status line, which
-- screen readers announce as it changes.
M.status = ui.state("status", "")
-- The mailboxes drawer, while the window is too narrow for the sidebar.
M.drawer = ui.state("drawer", false)
return M
