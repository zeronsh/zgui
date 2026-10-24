-- The monitor's colours are theme tokens, so any theme styles it: the
-- standard ones, and the app's own below, whose defaults are written in
-- terms of the standard ones. Its own look is glass.json, a theme that sets
-- both (the terminal example's glass, with a hue per series).
ui.theme.tokens {
	monitor = {
		-- Each panel's card.
		panel = "{surface.sunken}",
		-- A stat tile, the selected sort, a drag's chip.
		wash = "{pressed}",
		-- The series' hues.
		cpu = "#3987e5",
		gpu = "#e5793a",
		memory = "#199e70",
	},
}
ui.theme.default(ui.theme.load("glass.json"))

return {
	tint = "$glass.tint",
	panel = "$monitor.panel",
	wash = "$monitor.wash",
	hover = "$hover",
	line = "$border.subtle",
	ring = "$border.strong",
	text = "$text.primary",
	secondary = "$text.secondary",
	tertiary = "$text.tertiary",
	quaternary = "$text.disabled",
	accent = "$accent",
	live = "$success",
	exited = "$danger",
	clear = "#00000000",
	font = "$font.sans",
	-- The series' hues by metric, as token names: a canvas paints with
	-- what `ui.theme.get` makes of them.
	hues = { cpu = "monitor.cpu", gpu = "monitor.gpu", mem = "monitor.memory" },

	-- Space between the window's edge and the panels, and between panels.
	inset = 6,
	gap = 6,
	titlebar = 40,
	-- A sidebar row.
	row = 46,
	-- Frames a second for what moves with every sample (the charts'
	-- slide, the CPU meters): a few points a second, as smooth at this rate
	-- as at every display frame, for less of the machine being measured.
	sample_fps = 15,
}
