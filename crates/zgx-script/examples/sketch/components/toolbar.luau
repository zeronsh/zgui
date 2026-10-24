-- Tools, history, zoom, snapping, and the file.
local state = require("../state")
local actions = require("../actions")
local history = require("../history")

local TOOLS = {
	{ id = "select", label = "Select", key = "V" },
	{ id = "rect", label = "Rectangle", key = "R" },
	{ id = "ellipse", label = "Ellipse", key = "O" },
}

return function(): Node
	local tools: { Child } = {}
	for _, tool in TOOLS do
		table.insert(
			tools,
			ui.button {
				id = `tool-{tool.id}`,
				tooltip = `{tool.label} ({tool.key})`,
				bg = function()
					return if state.tool:get() == tool.id then "$accent" else "$surface.raised"
				end,
				on_click = function()
					state.tool:set(tool.id :: any)
				end,
				tool.label,
			}
		)
	end
	return ui.row {
		gap = 8,
		px = 12,
		py = 8,
		items = "center",
		bg = "$surface.raised",
		ui.row { gap = 4, tools },
		ui.div { w = 12 },
		ui.button {
			id = "undo",
			tooltip = `Undo ({ui.shortcut_text("cmd+z")})`,
			opacity = function()
				return if history.depth:get().undo > 0 then 1 else 0.4
			end,
			on_click = history.undo,
			"Undo",
		},
		ui.button {
			id = "redo",
			tooltip = `Redo ({ui.shortcut_text("cmd+shift+z")})`,
			opacity = function()
				return if history.depth:get().redo > 0 then 1 else 0.4
			end,
			on_click = history.redo,
			"Redo",
		},
		ui.div { grow = 1 },
		ui.text {
			id = "status",
			function()
				return state.status:get()
			end,
			color = "$text.secondary",
			text_size = "$font.size.sm",
		},
		ui.text {
			id = "title",
			function()
				local file = state.file:get()
				return (if file then file.name else "Untitled") .. (if state.dirty:get() then " •" else "")
			end,
		},
		ui.div { grow = 1 },
		ui.checkbox { id = "snapping", label = "Snap", value = state.snapping },
		ui.button {
			id = "zoom-out",
			"−",
			on_click = function()
				actions.zoom_middle(1 / 1.25)
			end,
		},
		ui.text {
			id = "zoom",
			function()
				return ui.number(state.view:get().zoom, { style = "percent", decimals = 0 })
			end,
			w = 48,
			align = "center",
		},
		ui.button {
			id = "zoom-in",
			"+",
			on_click = function()
				actions.zoom_middle(1.25)
			end,
		},
		ui.button { id = "open", "Open…", on_click = actions.open },
		ui.button { id = "save", "Save", on_click = actions.save },
	}
end
