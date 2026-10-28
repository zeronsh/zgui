-- The terminal's colours are theme tokens, so any theme styles it: the
-- standard ones, and the app's own below, whose defaults are written in
-- terms of the standard ones. Its own look is glass.json, a theme that sets
-- both; `ui.theme.set("zgx-light")` (or any other theme) restyles it live.
ui.theme.tokens {
	terminal = {
		canvas = "{background}",
		surface_raised = "{surface.raised}",
		surface_subtle = "{surface.sunken}",
		terminal_bg = "{surface.sunken}",
		-- Each terminal's card.
		pane = "{surface.sunken}",
		-- A selected tab, a dialog's button.
		wash = "{pressed}",
		-- Frosted glass under what is active (the tab, the agent); a fainter
		-- one under the pointer.
		glass = "{pressed}",
		glass_hover = "{hover}",
		selection_line = "{border.subtle}",
		-- The focused pane's border, when there are several.
		ring = "{border.strong}",
		-- A tab's icon tiles (dimmer in a background tab) and their edge.
		tile_bg = "{pressed}",
		tile_dim = "{hover}",
		tile_line = "{border.subtle}",
		-- A tab a dragged tab would drop on, and the ghost of the tab a
		-- dragged pane would open as.
		drop = "{pressed}",
		slot = "{hover}",
		slot_line = "{border.strong}",
		-- From the window's edge to the panes and the tools.
		inset = 8,
		pane_radius = 10,
		pane_header = 34,
		sidebar = { width = 232, radius = 16.5 },
		-- The interface's type scale: denser than the standard one, as a
		-- terminal's chrome is.
		type = {
			xxs = 10.5,
			xs = 11,
			sm = 11.5,
			md = 12,
			-- Tabs' names, menus, most text.
			body = 13,
			lg = 13,
			xl = 14,
			xxl = 15,
			-- An option's name in ⌘K.
			option = 13.5,
			title = 20,
		},
		-- The brightest text: what a search matched.
		bright = "{text.primary}",
		-- Text and marks on a filled button.
		on_fill = "{accent.text}",
		-- A plugin's error.
		error_text = "{danger}",
		-- Buttons (components/kit.luau): a clear one's hover, a filled
		-- one's hover and edge, and the filled tones.
		control_hover = "{hover}",
		button = {
			hover = "{pressed}",
			line = "{border.subtle}",
			radius = 15,
			radius_sm = 13,
			gap = 6,
			pad = 14,
			pad_sm = 10,
			pad_xs = 12,
		},
		primary = { bg = "{accent}", hover = "{accent.hover}", solid = "{accent}" },
		danger = { bg = "{danger}", hover = "{danger}" },
		switch = {
			off = "{pressed}",
			off_hover = "{hover}",
			knob = "{text.primary}",
			knob_shadow = "{shadow.sm}",
			pad = 2,
		},
		-- Sheets: what dims the window under one, and the panel itself.
		scrim = "alpha(#000000, 0.3)",
		scrim_light = "alpha(#000000, 0.18)",
		sheet = {
			bg = "{surface.overlay}",
			line = "{border}",
			shadow = "{shadow.lg}",
			pad = 18,
			gap = 6,
			radius = 16.5,
		},
		-- ⌘K: its sheet, and how far down the window it sits.
		palette = { bg = "{surface.overlay}", line = "{border}", shadow = "{shadow.lg}", top = 56, radius = 16 },
		-- Small labels: a key in a hint, a permission or panel, a status.
		key = { bg = "{hover}", line = "{border.subtle}", pad = 5, radius = 5 },
		chip = { bg = "{hover}", line = "{border.subtle}", gap = 5 },
		badge = { pad = 7, radius = 9 },
		-- A plugin's card.
		card = { bg = "{hover}", line = "{border.subtle}", pad = 14, radius = 12 },
		-- The grip a pane is dragged by: frosted, with two rows of dots.
		grip = {
			bg = "{surface.overlay}",
			line = "{border}",
			shadow = "{shadow.sm}",
			dot = "{text.secondary}",
			gap = 3,
			radius = 7,
		},
		-- A text field in a bar (a page's address): at rest, and edited.
		field = {
			bg = "{hover}",
			line = "{border.subtle}",
			editing = "{surface.sunken}",
			ring = "{focus.ring}",
		},
		-- A page loading: the line filling under its bar.
		progress = "{text.secondary}",
		-- ⌘K's footer of hints: a little lighter than its list.
		footer = "alpha(#ffffff, 0.02)",
		-- A pane being moved: its edge.
		moving = "{border.strong}",
		-- Rows to choose from: ⌘K's, and the agents'.
		row = { option_radius = 9, agent_pad_x = 10, agent_pad_y = 7, agent_gap = 10 },
		-- Context menus: frosted panels.
		menu = {
			bg = "{surface.overlay}",
			line = "{border.subtle}",
			hover = "{hover}",
			separator = "{border.subtle}",
			shadow = "{shadow.md}",
			-- In from the panel's edge to its items.
			pad = 3,
		},
		tab = {
			-- The icons' tiles sit as far in from a tab's edge as from its
			-- top; the name, without a tile, a little more.
			inset = 5,
			["end"] = 10,
			gap = 7,
			radius = 16.5,
			-- Between the tiles in a tab, and between tabs in the strip.
			tile_gap = 3,
			strip_gap = 3,
			tile_radius = 5,
			-- "+3", for panes past three: its tile's sides and text.
			tile_pad = 5,
			tile_text = 10.5,
		},
		-- A dragged tab: a small window of what it holds, its panes laid out
		-- as they are, each a picture of what it shows.
		mini = {
			width = 260,
			-- A tall pane's picture is no taller than this.
			max_h = 200,
			pad = 6,
			gap = 6,
			radius = 12,
			pane_gap = 4,
			pane_radius = 6,
			bg = "{glass.tint}",
		},
		-- Claude's colour, whatever the theme.
		claude = "#d97757",
	},
}
ui.theme.default(ui.theme.load("glass.json"))

return {
	tint = "$glass.tint",
	pane = "$terminal.pane",
	wash = "$terminal.wash",
	glass = "$terminal.glass",
	glass_hover = "$terminal.glass_hover",
	hover = "$hover",
	line = "$border.subtle",
	ring = "$terminal.ring",
	text = "$text.primary",
	secondary = "$text.secondary",
	tertiary = "$text.tertiary",
	quaternary = "$text.disabled",
	accent = "$accent",
	bell = "$warning",
	claude = "$terminal.claude",
	danger = "$danger",
	clear = "#00000000",
	font = "$font.sans",

	-- The comfortable native layout's outer inset and pane gutter.
	inset = 8,
	gap = 8,
	titlebar = 50,
}
