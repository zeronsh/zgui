-- The streaming transcript, described in Luau. Rust owns the stream and
-- writes the shared state; this script runs once and again only when the
-- Start/Stop label changes. Same window as examples/stream_rust.rs.
local running = ui.shared("running", false)
local tokens = ui.shared("tokens", 0)
local live = ui.shared("live", "")
local paragraphs = ui.shared("paragraphs", {})
local offset = ui.shared("offset", 0)

-- Glass: a translucent tint over the blurred desktop, surfaces as light
-- washes over it.
local tint = "#0d0d0dcc"
local wash = "#ffffff0a"
local line = "#ffffff1a"
local hover = "#ffffff1f"
local accent = "#8b7cf6"
local accent_hover = "#9d90f8"
local text = "#e8e8ea"
local muted = "#a9a9ae"

local function button(label, on_click, bg, over)
	return ui.button { px = 14, py = 7, rounded = 8, bg = bg, hover = { bg = over }, on_click = on_click, label }
end

return ui.column {
	w_full = true,
	h_full = true,
	bg = tint,
	color = text,

	-- The titlebar: drags the window, under the native traffic lights.
	ui.row {
		w_full = true,
		h = 40,
		items = "center",
		justify = "center",
		window_drag = true,
		ui.text { "Streaming", text_size = 13, bold = true, color = muted },
	},

	ui.column {
		grow = 1,
		shrink = 1,
		min_h = 0,
		w_full = true,
		px = 20,
		pb = 20,
		gap = 14,
		ui.row {
			w_full = true,
			gap = 8,
			items = "center",
			button(function()
				return if running:get() then "Stop" else "Start stream"
			end, ui.action("toggle"), accent, accent_hover),
			button("Clear", ui.action("clear"), wash, hover),
			ui.div { grow = 1 },
			ui.text { "{tokens} tokens", color = muted, text_size = 13 },
		},
		ui.scroll {
			offset = offset,
			grow = 1,
			shrink = 1,
			min_h = 0,
			w_full = true,
			rounded = 12,
			bg = wash,
			border = 1,
			border_color = line,
			fade = { 18, 18 },
			ui.column {
				w_full = true,
				p = 18,
				gap = 12,
				selectable = true,
				ui.each(paragraphs, function(paragraph)
					return ui.text { paragraph, wrap = true, w_full = true }
				end, { w_full = true, gap = 12 }),
				ui.text { "{live}", wrap = true, w_full = true },
			},
		},
	},
}
