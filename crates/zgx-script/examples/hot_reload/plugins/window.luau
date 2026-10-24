-- Reads data Rust publishes (`window`) and calls a Rust action (`quit`).
ui.contribute(
	"sidebar",
	ui.column {
		items = "start",
		p = 12,
		gap = 8,
		rounded = 10,
		bg = "#212833",
		ui.text { "Window", bold = true },
		ui.text { "{window} logical px", color = "#8b95a5", text_size = 13 },
		ui.button {
			px = 10,
			py = 6,
			rounded = 6,
			bg = "#4a1d24",
			hover = { bg = "#5c242d" },
			on_click = ui.action("quit"),
			"Quit",
		},
	}
)
