--!strict
-- The editor's state: the text, the file it came from and its language,
-- the caret (as the editor tells it), soft wrap, and the search.

local M = {}

M.text = ui.state("text", "")
-- The file the text came from, if any, and whether the text changed since.
M.file = ui.state("file", nil :: File?)
M.saved = ui.state("saved", "")
M.language = ui.state("language", "text")
M.wrap = ui.persist("wrap", false)
-- Where the caret is: line and column from 1, the selection in bytes.
M.caret = ui.state("caret", { line = 1, column = 1, anchor = 0, focus = 0 })
M.status = ui.state("status", "")

-- The search: open, what it looks for and replaces with, whether case
-- counts, and which match is the current one (from 1; 0 for none).
M.find_open = ui.state("find_open", false)
M.query = ui.state("query", "")
M.replacement = ui.state("replacement", "")
M.case = ui.persist("case", false)
M.current = ui.state("current", 0)

return M
