-- zterm's design system: the pieces its components share, as ui.variants
-- recipes over the terminal's tokens (theme.luau, glass.json). A component
-- takes its look from these rather than values of its own, so the same
-- thing looks the same everywhere and a theme restyles all of it.
local theme = require("../theme")

local M = {}

-- A button. `tone`: what it does (`primary` and `danger` filled, `secondary`
-- washed, `outline` washed with an edge, `ghost` clear until hovered,
-- `quiet` text alone); `size`: its scale, `icon` and `icon_sm` square for a
-- glyph alone.
M.button = ui.variants {
	base = {
		items = "center",
		justify = "center",
		shrink = 0,
		gap = "$terminal.button.gap",
		rounded = "$terminal.button.radius",
		focus = { border = 0 },
	},
	variants = {
		tone = {
			primary = {
				bg = "$terminal.primary.bg",
				color = "$terminal.on_fill",
				hover = { bg = "$terminal.primary.hover" },
			},
			danger = {
				bg = "$terminal.danger.bg",
				color = "$terminal.on_fill",
				hover = { bg = "$terminal.danger.hover" },
			},
			secondary = { bg = theme.wash, color = theme.text, hover = { bg = "$terminal.button.hover" } },
			outline = {
				bg = theme.wash,
				color = theme.text,
				border = 1,
				border_color = "$terminal.button.line",
				hover = { bg = "$terminal.button.hover" },
			},
			ghost = { bg = theme.clear, hover = { bg = "$terminal.control_hover" } },
			quiet = { bg = theme.clear, color = theme.secondary, hover = { color = theme.text } },
		},
		size = {
			xs = {
				h = 24,
				px = "$terminal.button.pad_xs",
				rounded = "$terminal.button.radius_sm",
				text_size = "$terminal.type.md",
				weight = "$font.weight.medium",
			},
			sm = {
				h = 24,
				px = "$terminal.button.pad_sm",
				text_size = "$terminal.type.md",
				weight = "$font.weight.medium",
			},
			md = { h = 28, px = "$terminal.button.pad", text_size = "$terminal.type.lg" },
			icon = { w = 28, h = 28 },
			icon_sm = { w = 26, h = 26, rounded = "$terminal.button.radius_sm" },
		},
	},
	defaults = { tone = "secondary", size = "md" },
}

-- On or off, as a switch (its knob is `M.knob`).
M.switch = ui.variants {
	base = {
		w = 36,
		h = 16,
		px = "$terminal.switch.pad",
		rounded = "$radius.lg",
		shrink = 0,
		items = "center",
		focus = { border = 0 },
	},
	variants = {
		on = {
			[true] = { justify = "end", bg = "$terminal.primary.solid", hover = { bg = "$terminal.primary.hover" } },
			[false] = { justify = "start", bg = "$terminal.switch.off", hover = { bg = "$terminal.switch.off_hover" } },
		},
	},
	defaults = { on = false },
}
M.knob = ui.variants {
	base = {
		w = 12,
		h = 12,
		rounded = "$radius.full",
		bg = "$terminal.switch.knob",
		shadow = "$terminal.switch.knob_shadow",
	},
}

-- What dims the window under a sheet (the sheet sits in it): centred for a
-- dialog, near the top for ⌘K.
M.scrim = ui.variants {
	base = { w_full = true, h_full = true, items = "center" },
	variants = {
		kind = {
			dialog = { bg = "$terminal.scrim", justify = "center" },
			palette = { bg = "$terminal.scrim_light", pt = "$terminal.palette.top", pb = "$space.5" },
		},
	},
	defaults = { kind = "dialog" },
}
-- A frosted sheet over the window: a dialog's, or ⌘K's.
M.sheet = ui.variants {
	base = { border = 1 },
	variants = {
		kind = {
			dialog = {
				p = "$terminal.sheet.pad",
				gap = "$terminal.sheet.gap",
				rounded = "$terminal.sheet.radius",
				bg = "$terminal.sheet.bg",
				backdrop_blur = 28,
				border_color = "$terminal.sheet.line",
				shadow = "$terminal.sheet.shadow",
			},
			palette = {
				items = "stretch",
				rounded = "$terminal.sheet.radius",
				overflow = "hidden",
				bg = "$terminal.palette.bg",
				backdrop_blur = 40,
				border_color = "$terminal.palette.line",
				shadow = "$terminal.palette.shadow",
			},
		},
	},
	defaults = { kind = "dialog" },
}

-- A square tile under an icon. Untinted, it is dimmer in what is not active
-- (`dim`); an item with a colour of its own gives the tile `M.tint`s.
M.tile = ui.variants {
	base = {
		shrink = 0,
		items = "center",
		justify = "center",
		border = 1,
		border_color = "$terminal.tile_line",
	},
	variants = {
		size = {
			-- In a tab, where "+3" widens it.
			sm = { h = 20, min_w = 20, rounded = "$terminal.tab.tile_radius" },
			md = { w = 30, h = 30, rounded = "$terminal.button.radius" },
			lg = { w = 36, h = 36, rounded = "$radius.lg" },
		},
		dim = { [true] = { bg = "$terminal.tile_dim" }, [false] = { bg = "$terminal.tile_bg" } },
	},
	defaults = { size = "sm", dim = false },
}
-- An item's own colour at `alpha` (two hex digits): a tinted tile's fill or
-- edge. The colour is the item's identity, not a theme's.
function M.tint(color: string, alpha: string): string
	local value = if color:sub(1, 1) == "$" then ui.theme.get(color:sub(2)) :: string else color
	return ui.raw(if alpha == "" then value else value:sub(1, 7) .. alpha, "an item's own colour")
end

-- A small label: a key in a hint, a permission or a panel, a status.
M.chip = ui.variants {
	base = { items = "center", shrink = 0 },
	variants = {
		kind = {
			key = {
				h = 18,
				min_w = 18,
				px = "$terminal.key.pad",
				justify = "center",
				rounded = "$terminal.key.radius",
				bg = "$terminal.key.bg",
				border = 1,
				border_color = "$terminal.key.line",
			},
			label = {
				h = 22,
				px = "$space.2",
				gap = "$terminal.chip.gap",
				rounded = "$radius.md",
				bg = "$terminal.chip.bg",
				border = 1,
				border_color = "$terminal.chip.line",
			},
			badge = { h = 18, px = "$terminal.badge.pad", rounded = "$terminal.badge.radius" },
		},
	},
	defaults = { kind = "label" },
}

-- A row to choose from, over the glass that marks the chosen one
-- (controls.glass): ⌘K's options, the agents. An agent's is a button whose
-- own fills give way to the glass.
M.row = ui.variants {
	base = { relative = true, items = "center" },
	variants = {
		kind = {
			option = {
				h = 46,
				px = "$space.2",
				gap = "$space.3",
				rounded = "$terminal.row.option_radius",
				cursor = "pointer",
			},
			agent = {
				px = "$terminal.row.agent_pad_x",
				py = "$terminal.row.agent_pad_y",
				gap = "$terminal.row.agent_gap",
				rounded = "$terminal.button.radius",
				shrink = 0,
				bg = theme.clear,
				hover = { bg = theme.clear },
				active = { bg = theme.clear },
				focus = { border = 0 },
			},
		},
	},
}

-- A plugin's card.
M.card = ui.variants {
	base = {
		p = "$terminal.card.pad",
		gap = "$terminal.card.pad",
		items = "start",
		rounded = "$terminal.card.radius",
		bg = "$terminal.card.bg",
		border = 1,
		border_color = "$terminal.card.line",
	},
}

return M
