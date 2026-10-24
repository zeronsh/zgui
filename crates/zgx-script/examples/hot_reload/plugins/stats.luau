-- A plugin runs in its own sandboxed VM and contributes to named slots.
-- It sees the app's shared state, never its private state.
local count = ui.shared("count", 0)
local likes = ui.state("likes", 0)

ui.contribute(
	"sidebar",
	ui.column {
		items = "start",
		p = 12,
		gap = 8,
		rounded = 10,
		bg = "#212833",
		ui.text { "Stats", bold = true },
		ui.text { "App counter: {count}", color = "#8b95a5", text_size = 13 },
		ui.button {
			px = 10,
			py = 6,
			rounded = 6,
			bg = "#2b3442",
			hover = { bg = "#344052" },
			on_click = ui.add(likes),
			"Likes: {likes}",
		},
	}
)
