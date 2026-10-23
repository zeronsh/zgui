-- The settings window's content.
local state = require("../state")

return function(): Node
	return ui.column {
		w_full = true,
		h_full = true,
		p = 24,
		gap = 14,
		bg = "$surface.base",
		ui.text { "Settings", text_size = "$font.size.lg", bold = true },
		ui.checkbox { label = "Dark appearance", value = state.dark },
		ui.checkbox { label = "Check spelling while typing", value = state.spellcheck },
		ui.row {
			gap = 8,
			items = "center",
			ui.text { "Density", color = "$text.secondary" },
			ui.button {
				on_click = ui.set(state.density, "comfortable"),
				bg = function()
					return if state.density:get() == "comfortable" then "$selection.item" else "$button.bg"
				end,
				"Comfortable",
			},
			ui.button {
				on_click = ui.set(state.density, "compact"),
				bg = function()
					return if state.density:get() == "compact" then "$selection.item" else "$button.bg"
				end,
				"Compact",
			},
		},
	}
end
