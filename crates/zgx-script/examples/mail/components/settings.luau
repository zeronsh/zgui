-- The settings window's content.
local state = require("../state")

-- A label and a button per option, the chosen one highlighted.
local function choice<T>(label: string, value: State<T>, options: { { any } }): Node
	local buttons: { Node } = {}
	for _, option in options do
		local key: T, name: string = option[1], option[2]
		table.insert(
			buttons,
			ui.button {
				on_click = ui.set(value, key),
				bg = function()
					return if value:get() == key then "$selection.item" else "$button.bg"
				end,
				name,
			}
		)
	end
	return ui.row {
		gap = 8,
		items = "center",
		ui.text { label, w = 88, shrink = 0, color = "$text.secondary" },
		buttons,
	}
end

return function(): Node
	return ui.column {
		w_full = true,
		h_full = true,
		p = 24,
		gap = 14,
		bg = "$surface.base",
		ui.text { "Settings", text_size = "$font.size.lg", bold = true },
		choice("Appearance", state.appearance, { { "system", "System" }, { "light", "Light" }, { "dark", "Dark" } }),
		ui.checkbox { label = "Check spelling while typing", value = state.spellcheck },
		choice("Density", state.density, { { "comfortable", "Comfortable" }, { "compact", "Compact" } }),
	}
end
