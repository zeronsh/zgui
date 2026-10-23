-- The app's state: every module that requires this one gets these States.
local sample = require("@lib/sample")

export type Message = sample.Message
export type Density = "comfortable" | "compact"

local M = {}
M.messages = ui.state("messages", sample.messages(4000))
M.mailbox = ui.persist("mailbox", "Inbox")
M.selected = ui.state("selected", nil :: number?)
M.query = ui.state("query", "")
-- Composing: open, and the draft.
M.composing = ui.state("composing", false)
M.draft_to = ui.state("draft_to", "")
M.draft_subject = ui.state("draft_subject", "")
M.draft_body = ui.state("draft_body", "")
-- Settings, kept across launches; their window.
M.dark = ui.persist("dark", true)
M.density = ui.persist("density", "comfortable" :: Density)
M.spellcheck = ui.persist("spellcheck", true)
M.settings_open = ui.state("settings_open", false)
return M
