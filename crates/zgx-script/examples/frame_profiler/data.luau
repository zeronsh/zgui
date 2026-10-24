-- Sample data, read from the pen design `zgx · Frame profiler` (MvwFD).
local D = {}

-- A frame's bar: its render, paint, layout and update heights (`r`, `p`,
-- `l`, `u`), in the chart's points. Keyed by those letters.
export type Frame = { [string]: number }
-- A span of the trace: `x` and `w` in the trace's points.
export type Span = {
	id: string,
	lane: number,
	kind: string,
	name: string,
	x: number,
	w: number,
	label: string?,
	title: string?,
}
export type Detail = { dur: string?, rows: { { string } } }
export type Area = {
	id: string,
	name: string,
	state: string,
	nodes: { number | string },
	bar: { number },
	mpx: { string },
	quiet: boolean?,
}
-- A label, this frame's value and the 1 s average.
export type Counter = { [number]: string, warn: boolean? }
export type SearchResult = {
	kind: string,
	swatch: string?,
	icon: string?,
	name: string,
	mono: boolean?,
	meta: string,
	value: string,
	span: string?,
	area: number?,
	keywords: string?,
}
-- A frame as the profiler shows it (app.luau's `info`): its number, bar,
-- phase times in ms (`u`, `l`, `p`, `r`, `present`) and totals.
export type FrameInfo = {
	n: number,
	f: Frame,
	ms: { [string]: number },
	cpu: number,
	gpu: number,
	over: number,
	passes: number,
	design: boolean,
}
-- The trace's zoom: how it fits ("frame", "vsync", "selection", "custom"),
-- `k` times the frame across the track, scrolled `off` points.
export type Zoom = { mode: string, k: number, off: number }
-- Lists of a label and its values (budgets, lengths, speeds) mix strings
-- and numbers: Luau types each item as the union.
export type Row = { number | string }

-- 120 frames, oldest first: render, paint, layout, update heights in the
-- chart's points (120 points = 16.7 ms).
local BARS = [[
6 1 3 3|8 2 3 6|9 1 3 5|9 2 3 4|9 1 2 4|7 2 3 5|7 1 4 4|7 1 4 5|8 1 3 6|6 2 3 6|
7 2 4 5|9 2 4 4|7 2 4 6|9 2 2 5|9 1 4 4|8 2 3 6|10 2 3 6|7 2 4 4|12 3 2 3|12 3 3 5|
15 3 3 4|14 3 4 6|13 3 3 6|15 3 4 4|14 3 4 4|13 3 4 4|14 3 4 4|11 3 4 4|15 3 3 3|14 3 4 5|
13 3 3 5|14 3 3 5|14 3 3 4|12 3 4 4|10 2 2 4|9 2 2 4|7 1 2 5|8 2 4 4|19 4 17 9|6 2 4 3|
6 2 3 5|7 2 4 6|10 2 3 4|8 1 3 5|6 1 4 5|6 2 4 5|8 2 4 5|9 2 2 4|9 1 3 4|10 2 2 6|
7 1 4 6|9 1 4 5|2 1 0 1|3 1 0 1|3 1 0 1|3 1 0 1|2 1 0 1|2 1 0 1|3 1 0 1|2 1 0 1|
3 1 0 1|3 1 0 1|3 1 0 1|2 1 0 1|9 2 4 4|10 1 3 5|8 2 3 5|9 1 3 5|9 2 3 4|8 1 4 5|
8 1 2 6|50 3 6 6|6 2 4 5|7 1 4 4|9 1 4 3|6 2 3 4|7 1 3 6|7 2 2 5|9 2 3 4|9 2 4 4|
6 1 2 4|9 2 2 3|6 2 3 5|9 2 2 6|7 2 4 6|7 1 3 6|10 2 2 6|7 2 4 6|9 2 2 6|7 1 4 5|
9 1 4 4|6 2 2 6|6 1 3 5|8 2 3 5|7 2 4 5|9 1 2 5|26 5 28 12|9 2 3 5|6 2 4 5|7 1 3 4|
10 2 2 4|6 2 3 4|7 2 4 6|7 2 3 3|9 2 4 5|7 2 4 6|7 2 4 5|6 1 3 4|9 2 4 3|7 1 3 6|
10 1 4 4|9 1 3 4|7 2 3 6|7 1 3 4|7 1 3 5|8 2 2 5|9 1 4 4|8 1 3 5|7 1 4 4|8 2 4 5]]
D.frames = {} :: { Frame }
for entry in BARS:gsub("\n", ""):gmatch("[^|]+") do
	local r, p, l, u = entry:match("(%d+) (%d+) (%d+) (%d+)")
	local frame: Frame = { r = tonumber(r) or 0, p = tonumber(p) or 0, l = tonumber(l) or 0, u = tonumber(u) or 0 }
	table.insert(D.frames, frame)
end
D.first = 48117
D.selected = 97 -- frame 48,213
D.ms_per_point = 16.7 / 120

D.colors = {
	update = "#B9A6F2",
	layout = "#4A97B8",
	paint = "#E2C98F",
	render = "#D9826A",
	present = "#5A5850",
	input = "#A3A199",
	actions = "#A3A199",
} :: { [string]: string }
-- A nested span's fill and border, and its fill when selected.
D.dark = {
	update = { "#4D4760", "#B9A6F280", "#6B5F8F" },
	actions = { "#464542", "#A3A19980", "#5E5D58" },
	layout = { "#28424C", "#4A97B880", "#36667A" },
	paint = { "#5B533E", "#E2C98F80", "#7D7051" },
	render = { "#583B32", "#D9826A80", "#8F594A" },
	present = { "#2D2C29", "#5A585080", "#45443F" },
} :: { [string]: { string } }

-- Frame 48,213's spans: x and width in the trace's points (73.15 per ms),
-- the lane (0-2 main thread, 3 GPU queue), and whether the design labels it.
D.px_per_ms = 1024 / 14
D.phases = {
	{ kind = "input", from = 0, to = 14 },
	{ kind = "update", from = 15, to = 117 },
	{ kind = "layout", from = 118, to = 399 },
	{ kind = "paint", from = 401, to = 454 },
	{ kind = "render", from = 455, to = 720 },
	{ kind = "present", from = 721, to = 834 },
}
local S: { Span } = {}
local function span(
	id: string,
	lane: number,
	kind: string,
	name: string,
	x: number,
	w: number,
	label: string?,
	title: string?
)
	table.insert(S, { id = id, lane = lane, kind = kind, name = name, x = x, w = w, label = label, title = title })
end
span("input", 0, "input", "Input", 0, 14)
span("update", 0, "update", "Update", 15, 101.8, "name")
span("layout", 0, "layout", "Layout", 118, 281, "dur")
span("paint", 0, "paint", "Paint", 401, 52.8)
span("render", 0, "render", "Render", 455, 264.9, "dur")
span("present", 0, "present", "Present", 721, 112.7, "name")
span("actions", 1, "actions", "actions", 1, 11.1)
span("luau", 1, "update", "luau", 15, 37.4)
span("reactive", 1, "update", "reactive", 54, 39.6)
span("patch", 1, "update", "patch", 95, 22)
span("pass1", 1, "layout", "pass 1", 118, 92.3, "name", "Layout pass 1")
span("sizes", 1, "layout", "sizes", 212, 35.2)
span("pass2", 1, "layout", "pass 2", 249, 86.4, "name", "Layout pass 2")
span("pass3", 1, "layout", "pass 3", 336, 46.9, nil, "Layout pass 3")
span("hover", 1, "layout", "hover", 385, 14.7)
span("flush", 1, "paint", "flush", 401, 45.4)
span("plan", 1, "paint", "plan", 448, 6)
span("prune", 1, "render", "prune", 455, 6)
span("scroll", 1, "render", "scroll", 462, 7.4)
span("layers", 1, "render", "layers", 471, 61.5)
span("blur", 1, "render", "blur", 534, 38.1)
span("render_flat", 1, "render", "render_flat", 573, 146.4, "dur")
span("latch", 1, "present", "latch wait", 721, 74.7)
span("present_call", 1, "present", "present", 797, 29.4)
span("a11y", 1, "present", "a11y", 828, 6)
span("toasts1", 2, "layout", "Toasts", 124, 65.9, "name", "Toasts · measure")
span("toasts2", 2, "layout", "Toasts", 255, 58.6, nil, "Toasts · measure")
span("toasts3", 2, "render", "Toasts", 471, 39.6, nil, "Toasts · layer")
span("orbs", 2, "render", "Orbs", 512, 20.6, nil, "Status orbs")
span("shape", 2, "render", "shape · 31", 573, 77.6, nil, "Shape text · 31 nodes")
span("glyphs", 2, "render", "glyphs", 652, 33.7, nil, "Glyph upload")
span("encode", 2, "render", "encode", 688, 32.3)
span("gpu_layers", 3, "render", "layers", 799, 32.3, nil, "Layer composite")
span("gpu_blur", 3, "render", "blur × 3", 832, 58.6, nil, "Backdrop blur × 3")
span("gpu_main", 3, "render", "main pass", 892, 55.7, nil, "Main pass")
span("gpu_blit", 3, "render", "blit", 949, 16.9, nil, "Blit to drawable")
D.spans = S
D.details = {
	glyphs = {
		dur = "0.48 ms",
		rows = { { "glyphs uploaded", "188" }, { "atlas", "1024² → 2048²" }, { "source", "Toasts · 42 words" } },
	},
	pass1 = { rows = { { "dirty nodes", "46" }, { "measured", "Toasts × 3" }, { "cause", "wrapped text" } } },
	layout = { rows = { { "passes", "3" }, { "nodes laid out", "212" }, { "longest", "pass 1" } } },
	render = { rows = { { "draw calls", "46" }, { "instances", "1,284" }, { "layers", "6" } } },
} :: { [string]: Detail }

D.areas = {
	{
		id = "toasts",
		name = "Toasts",
		state = "enter · 3 toasts",
		nodes = { 46, 18 },
		bar = { 100, 32, 8 },
		mpx = { "0.31", "0.12" },
	},
	{
		id = "orbs",
		name = "Status orbs",
		state = "animating 120 Hz",
		nodes = { 12, 12 },
		bar = { 2, 21, 21 },
		mpx = { "0.04", "0.04" },
	},
	{
		id = "palette",
		name = "Command palette",
		state = "results list",
		nodes = { 88, 31 },
		bar = { 11, 24, 12 },
		mpx = { "0.09", "0.03" },
	},
	{
		id = "model",
		name = "Model menu",
		state = "open",
		nodes = { 64, 9 },
		bar = { 5, 16, 7 },
		mpx = { "0.05", "0.01" },
	},
	{
		id = "sliders",
		name = "Sliders",
		state = "drag",
		nodes = { 22, 6 },
		bar = { 3, 8, 6 },
		mpx = { "0.02", "0.01" },
	},
	{
		id = "other",
		name = "19 other areas",
		state = "idle",
		nodes = { "1,280", "1,280" },
		bar = { 2, 5, 3 },
		mpx = { "0.03", "0.02" },
		quiet = true,
	},
} :: { Area }

-- Renderer counters: this frame, then the 1 s average.
D.counters = {
	{ "Draw calls", "46", "22" },
	{ "Passes · blur", "7 · 3", "3 · 1" },
	{ "Instances", "1,284", "610" },
	{ "Shaped nodes", "31", "2", warn = true },
	{ "Glyph uploads", "188", "4", warn = true },
	{ "Layer repaints · hits", "2 · 9", "1 · 10" },
	{ "Damaged", "1.42 Mpx", "0.41" },
} :: { Counter }
D.retained = {
	{ "Glyph atlas", "16.0 MB" },
	{ "Layers · 6", "21.4 MB" },
	{ "Shaped text", "3.1 MB" },
	{ "Device total", "142 MB" },
} :: { Counter }

D.ranges = {
	{ label = "1 s", meta = "120 frames · 1 s", ticks = { "−1.0 s", "−0.75 s", "−0.5 s", "−0.25 s", "now" } },
	{ label = "5 s", meta = "600 frames · 5 s", ticks = { "−5 s", "−3.75 s", "−2.5 s", "−1.25 s", "now" } },
	{ label = "30 s", meta = "3,600 frames · 30 s", ticks = { "−30 s", "−22.5 s", "−15 s", "−7.5 s", "now" } },
}

D.processes = { { "controls_lab", "live" }, { "ask_shell", "48190" }, { "scratchpad", "48302" } }
D.overlays = {
	{ "Paint flashing", "tint damaged rects · 1 frame", "#E8A898" },
	{ "Layout bounds", "outline every node box", "#4A97B8" },
	{ "Layer bounds", "isolated layers · cache hits", "#E2C98F" },
	{ "Hit regions", "pointer & drag targets", "#B9A6F2" },
	{ "Scroll copies", "copied vs repainted strips", "#C3CEB2" },
	{ "Native planes", "under / over web views", "#A9BABF" },
}
D.search = {
	{
		kind = "spans",
		swatch = "#D9826A",
		name = "Glyph upload",
		meta = "render · frame 48,213",
		value = "0.48 ms",
		span = "glyphs",
	},
	{
		kind = "spans",
		swatch = "#4A97B8",
		name = "glyph_layout",
		mono = true,
		meta = "layout · pass 2",
		value = "0.21 ms",
		span = "pass2",
	},
	{
		kind = "spans",
		swatch = "#D9826A",
		name = "Glyph atlas grow",
		meta = "gpu queue · 1024² → 2048²",
		value = "0.12 ms",
		span = "gpu_main",
	},
	{
		kind = "areas",
		icon = "layout",
		name = "Toasts",
		meta = "enter · 46 nodes",
		value = "0.31 Mpx",
		area = 1,
		keywords = "glyph upload text",
	},
	{
		kind = "nodes",
		icon = "type",
		name = "GlyphRun#toast-3",
		mono = true,
		meta = "Toasts › Body",
		value = "42 words",
		area = 1,
	},
} :: { SearchResult }

-- Frame budgets (pen XFJTT · Budget ▾), time-scale presets, buffer sizes.
D.budgets = {
	{ 8.33, "8.33 ms", "120 Hz" },
	{ 11.1, "11.1 ms", "90 Hz" },
	{ 16.7, "16.7 ms", "60 Hz" },
	{ 33.3, "33.3 ms", "30 Hz" },
} :: { Row }
D.scale_presets = { 0.1, 0.25, 0.5, 1, 2 }
D.buffer_counts = { 1200, 3000, 6000, 12000 }
D.cap_hz = { 120, 60, 30, 120 }

-- Dropdown options: a label over a description (pen XFJTT · size / when full).
D.buffer_options = {
	{ "1,200 frames", "≈ 10 s at 120 Hz · 4.4 MB" },
	{ "3,000 frames", "≈ 25 s at 120 Hz · 11 MB" },
	{ "6,000 frames", "≈ 50 s at 120 Hz · 21.9 MB" },
	{ "12,000 frames", "≈ 100 s at 120 Hz · 43.8 MB" },
}
D.full_options = {
	{ "Drop oldest", "Keep recording, overwrite the earliest frames" },
	{ "Stop recording", "Keep the buffer as captured" },
	{ "Save and restart", "Export the buffer, then start a new one" },
}
D.backends = {
	{ "wgpu · Metal", "The GPU renderer, on Metal" },
	{ "wgpu · Vulkan", "The GPU renderer, through MoltenVK" },
	{ "Software", "The CPU rasterizer, for checks" },
}
D.caps = {
	{ "120 Hz", "The display's refresh" },
	{ "60 Hz", "Every other refresh" },
	{ "30 Hz", "Every fourth refresh" },
	{ "Uncapped", "As fast as frames finish" },
}
-- Capture options: how long a capture runs, and when it starts.
D.lengths = {
	{ "5 seconds", "timer", 5 },
	{ "15 seconds", "timer", 15 },
	{ "30 seconds", "timer", 30 },
	{ "Until stopped", "infinity" },
} :: { Row }
D.starts = {
	{ "Immediately", "zap" },
	{ "After 3 s countdown", "hourglass" },
	{ "On first interaction", "mouse-pointer-click" },
}
D.rec_states = {
	{ "recording", "Recording", "" },
	{ "paused", "Paused", "buffer kept" },
	{ "stopped", "Stopped", "ready" },
	{ "countdown", "Armed · countdown", "starts in 3 s" },
	{ "armed", "Armed · first input", "waits" },
	{ "capturing", "Capturing", "set length" },
}
D.speeds = { { 0.25, "¼×" }, { 0.5, "½×" }, { 1, "1×" }, { 2, "2×" } } :: { Row }

return D
