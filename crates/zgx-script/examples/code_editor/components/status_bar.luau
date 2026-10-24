--!strict
-- The caret's line and column (and what is selected), the file's lines,
-- its language, soft wrap, and what last happened.
local state = require("../state")
local highlight = require("@lib/highlight")

return function(): Node
	return ui.row {
		id = "status-bar",
		gap = 16,
		px = 12,
		py = 4,
		items = "center",
		bg = "$surface.raised",
		border_sides = { 1, 0, 0, 0 },
		border_color = "$border",
		ui.text {
			id = "status-caret",
			function(): string
				local caret = state.caret:get()
				local selected = math.abs(caret.focus - caret.anchor)
				local where = `Ln {caret.line}, Col {caret.column}`
				return if selected > 0 then `{where} ({selected} selected)` else where
			end,
			size = 12,
			color = "$text.secondary",
		},
		ui.text {
			id = "status-message",
			function(): string
				return state.status:get()
			end,
			size = 12,
			color = "$text.tertiary",
			grow = 1,
			truncate = true,
		},
		ui.text {
			id = "status-language",
			function(): string
				return (highlight.names :: { [string]: string })[state.language:get()] or "Plain Text"
			end,
			size = 12,
			color = "$text.secondary",
		},
		ui.checkbox { id = "status-wrap", label = "Wrap", value = state.wrap },
	}
end
