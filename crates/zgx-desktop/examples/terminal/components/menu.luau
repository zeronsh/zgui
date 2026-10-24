-- Context menus: frosted like the rest of the window.
local theme = require("../theme")

local M = {}

local STYLE: Styles = {
	font = theme.font,
	w = 180,
	p = "$terminal.menu.pad",
	gap = 0,
	rounded = "$radius.lg",
	bg = "$terminal.menu.bg",
	backdrop_blur = 30,
	border = 1,
	border_color = "$terminal.menu.line",
	shadow = "$terminal.menu.shadow",
}
M.STYLE = STYLE

function M.entry(label: string, run: () -> ()): Node
	return ui.menu_item {
		label,
		h = 24,
		px = "$space.2",
		rounded = "$radius.md",
		text_size = "$terminal.type.body",
		color = theme.text,
		bg = theme.clear,
		hover = { bg = "$terminal.menu.hover" },
		focus = { bg = "$terminal.menu.hover", border = 0 },
		on_click = run,
	}
end

function M.separator(): Node
	return ui.div { h = 1, mx = "$space.2", my = "$space.1", bg = "$terminal.menu.separator" }
end

return M
