-- anime.js in Luau, run by zgx's Rust engine. Edit and save: the window
-- reloads. Every value below moves in Rust; this script runs once, and
-- again only for the buttons and callbacks it sets.

local tint = "#0d0d0dcc"
local wash = "#ffffff0a"
local line = "#ffffff1a"
local text = "#e8e8ea"
local muted = "#a9a9ae"
local accent = "#8b7cf6"

local function card(title, ...)
	return ui.column {
		p = 16,
		gap = 12,
		rounded = 14,
		bg = wash,
		border = 1,
		border_color = line,
		overflow = "hidden",
		ui.text { title, text_size = 11, bold = true, color = muted, letter_spacing = 0.6 },
		{ ... },
	}
end
local function button(label, on_click)
	return ui.button {
		px = 16,
		py = 6,
		rounded = 8,
		bg = "#ffffff14",
		hover = { bg = "#ffffff24" },
		cursor = "pointer",
		on_click = on_click,
		label,
	}
end

-- 1. A wave through a grid, staggered from its centre.
local dots = {}
for i = 1, 14 * 6 do
	dots[i] = ui.div { class = "dot", w = 10, h = 10, rounded = 5, bg = accent, opacity = 0.25 }
end
local function wave()
	anime.animate(".dot", {
		y = { 0, -10, 0 },
		opacity = { 0.25, 1, 0.25 },
		scale = { 1, 1.6, 1 },
		delay = anime.stagger(40, { grid = { 14, 6 }, from = "center" }),
		duration = 900,
		ease = "inOutSine",
	})
end

-- 2. A timeline: bars grow in sequence, a label waits for them.
local tl = anime.createTimeline { defaults = { duration = 500, ease = "outExpo" }, autoplay = false }
tl:add(".bar", { w = { 0, 220 }, delay = anime.stagger(80) })
	:add("#done", { opacity = { 0, 1 }, x = { -12, 0 } }, "-=200")

-- 3. A draggable knob that snaps inside its track.
-- (Created at load; it attaches when the knob mounts.)
local snapped = ui.state("snapped", 0)
anime.createDraggable("#knob", {
	y = false,
	container = "#track",
	snap = 60,
	releaseStiffness = 220,
	releaseDamping = 18,
	onSettle = function(d)
		snapped:set(math.floor(d.x / 60 + 0.5))
	end,
})

-- 4. Rows fading in as they scroll through their list.
local rows = {}
for i = 1, 24 do
	rows[i] = ui.div {
		class = "row",
		h = 36,
		rounded = 8,
		bg = "#ffffff10",
		px = 12,
		items = "center",
		opacity = 0.15,
		ui.text { "Row " .. i, color = muted },
	}
end
anime.animate(".row", {
	opacity = { 0.15, 1 },
	x = { -16, 0 },
	ease = "linear",
	duration = 100,
	autoplay = anime.onScroll { container = "#scroller", sync = 0.2, enter = "end start", leave = "center end" },
})

-- 5. A dot travelling an SVG path.
local route = "M10 70 C60 0 120 140 170 70 S280 0 300 70"
local path = anime.svg.createMotionPath(route)
anime.animate("#runner", {
	x = path.translateX,
	y = path.translateY,
	duration = 2400,
	loop = true,
	alternate = true,
	ease = "inOutQuad",
})

-- 6. A headline revealed word by word.
local function reveal()
	anime.animate(".word", {
		opacity = { 0, 1 },
		y = { 14, 0 },
		delay = anime.stagger(70),
		duration = 700,
		ease = "outBack(1.4)",
	})
end

-- 7. Squares turning and breathing: rotation and scale re-composite each
-- square's cached layer, without repainting it.
local squares = {}
for i = 1, 8 do
	squares[i] = ui.div {
		class = "sq",
		w = 26,
		h = 26,
		rounded = 6,
		gradient = { angle = 135, stops = { "#8b7cf6", "#58a6ff" } },
	}
end
anime.animate(".sq", {
	rotate = "1turn",
	scale = { 0.6, 1.1 },
	delay = anime.stagger(90),
	duration = 1600,
	loop = true,
	alternate = true,
	ease = "inOutQuart",
})

-- 8. A line drawing itself on a canvas.
local drawn = ui.state("drawn", 0)
local function draw()
	anime.animate(drawn, { value = { 0, 1 }, duration = 1600, ease = "inOutCubic" })
end

return ui.column {
	w_full = true,
	h_full = true,
	bg = tint,
	color = text,
	ui.row {
		w_full = true,
		h = 40,
		items = "center",
		justify = "center",
		window_drag = true,
		ui.text { "anime.js · Luau · zgx", text_size = 13, bold = true, color = muted },
	},
	ui.scroll {
		grow = 1,
		shrink = 1,
		min_h = 0,
		w_full = true,
		ui.grid {
			w_full = true,
			cols = 2,
			gap = 14,
			px = 20,
			pb = 20,
			card(
				"STAGGER FROM CENTRE",
				ui.grid { cols = 14, gap = 8, py = 6, on_mount = wave, dots },
				button("Wave", wave)
			),
			card(
				"TIMELINE",
				ui.column {
					gap = 8,
					ui.div { class = "bar", h = 8, w = 0, rounded = 4, bg = accent },
					ui.div { class = "bar", h = 8, w = 0, rounded = 4, bg = "#5b8def" },
					ui.div { class = "bar", h = 8, w = 0, rounded = 4, bg = "#3fb950" },
				},
				ui.row {
					gap = 8,
					items = "center",
					button("Play", function()
						tl:restart()
					end),
					ui.text { "Done", id = "done", opacity = 0, color = muted },
				}
			),
			card(
				"DRAGGABLE · SNAP 60",
				ui.overlay {
					id = "track",
					w = 300,
					h = 40,
					rounded = 20,
					bg = "#ffffff0d",
					ui.div {
						id = "knob",
						w = 40,
						h = 40,
						rounded = 20,
						bg = accent,
						shadow = { y = 4, blur = 12, color = "#00000066" },
					},
				},
				ui.text { "Notch {snapped}", color = muted, text_size = 12 }
			),
			card(
				"SCROLL-LINKED",
				ui.scroll {
					id = "scroller",
					h = 150,
					w_full = true,
					rounded = 8,
					fade = { 12, 12 },
					ui.column { gap = 6, p = 6, w_full = true, items = "stretch", rows },
				}
			),
			card(
				"MOTION PATH",
				ui.overlay {
					w = 310,
					h = 140,
					ui.canvas {
						w = 310,
						h = 140,
						draw = function(c)
							c:stroke(
								ui.path():move(10, 70):cubic(60, 0, 120, 140, 170, 70):cubic(220, 0, 280, 0, 300, 70),
								"#ffffff22",
								2
							)
						end,
					},
					ui.div {
						id = "runner",
						absolute = true,
						w = 14,
						h = 14,
						rounded = 7,
						bg = "#f778ba",
						left = -7,
						top = -7,
					},
				}
			),
			card(
				"TEXT SPLIT",
				anime.text.split("Retained, scripted, animated in Rust.", {
					words = { class = "word" },
					style = { text_size = 20, bold = true, opacity = 0 },
					props = { gap_x = 0, on_mount = reveal },
				}),
				button("Again", reveal)
			),
			card(
				"LINE DRAWING",
				ui.canvas {
					w = 310,
					h = 90,
					on_mount = draw,
					draw = function(c, w, h)
						local p = ui.path():move(10, h - 10)
						for i = 1, 12 do
							p:line(10 + i * (w - 20) / 12, (i % 2 == 0) and h - 10 or 14)
						end
						c:stroke(
							p,
							ui.linear(0, 0, w, 0, { accent, "#58a6ff", "#3fb950" }),
							3,
							{ draw = drawn:get(), cap = "round", join = "round" }
						)
					end,
				},
				button("Draw", draw)
			),
			card("ROTATE · SCALE", ui.row { gap = 10, py = 8, squares }),
			card(
				"SPRING",
				ui.row {
					gap = 10,
					items = "center",
					ui.div { id = "ball", w = 28, h = 28, rounded = 14, bg = "#58a6ff" },
					button("Bounce", function()
						anime.animate(
							"#ball",
							{ x = { 0, 220 }, ease = anime.spring { bounce = 0.55, duration = 600 } }
						)
					end),
				}
			),
		},
	},
}
