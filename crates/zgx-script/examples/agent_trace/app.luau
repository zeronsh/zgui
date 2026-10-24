--!nonstrict
-- (Non-strict until the lab's untyped motion data is annotated: `zgx check`
-- reports what strict mode would.)
-- An agent's trace, live, matched to the Ask motion lab: the run summary
-- connects, steps arrive (the row grows, its stem draws down, then the row
-- fades up), labels swap from doing to done through a blur crossfade,
-- narration streams in word by word, finished groups fold away, the trace
-- springs to its new height, and an approval docks onto the composer.
-- Edit and save: the window reloads and plays the run again.

-- Sizes are the lab's CSS pixels at its default 1.5× zoom.
local Z = 1.5
local function z(n)
	return n * Z
end
local SANS, MONO = "Geist", "Geist Mono"
local C = {
	ink = "#F4F2EC",
	text = "#EDEBE5",
	body = "#D4D2CB",
	sub = "#A3A199",
	muted = "#6E6C65",
	place = "#57554F",
	key = "#858379",
	line = "#3A3934",
	issue = "#C98A6B",
	skill = "#B9A6F2",
	memory = "#A8BFC4",
	gold = "#E2C98F",
	domain = "#AEABA3",
	orb = "#1F1E1B",
}

-- The lab's motion: sizes spring (0.4 s, bounce 0.1), swaps crossfade in
-- 140 ms, a new row grows in 320 ms before its content fades up, words blur
-- in over 180 ms.
local FADE, GROW, WORD = 140, 320, 180
local GROW_EASE = "cubicBezier(.25,.8,.25,1)"
local PACE, CONNECT, CLOCK = 1.2, 1.8, 8
-- The lab's run.autoApprove: seconds before the approval accepts itself (0: never).
local AUTO_APPROVE = 0

local SCRIPT = {
	{
		say = "I'll compare the lowest-priced personal blenders first, then check that the cheapest ones can actually handle shakes.",
	},
	{
		label = "Opened amazon.co.uk",
		doing = "Opening amazon.co.uk",
		icon = "globe",
		meta = "amazon.co.uk",
		run = { "Browsing amazon.co.uk", "Browsed amazon.co.uk", "globe" },
	},
	{
		label = "Searched “personal blender”",
		doing = "Searching “personal blender”",
		icon = "search",
		meta = "48 results",
	},
	{
		label = "Searched “smoothie maker”",
		doing = "Searching “smoothie maker”",
		icon = "search",
		meta = "31 results",
	},
	{ label = "Sorted by price: low to high", doing = "Sorting by price", icon = "sort", meta = "low → high" },
	{
		label = "Read the Amazon results",
		doing = "Reading the results",
		icon = "scan",
		meta = "amazon.co.uk",
		retry = true,
	},
	{ label = "Noted 5 candidates", doing = "Writing a note", icon = "note", meta = "5 items" },
	{
		say = "The price sort is full of low-rated cordless models, so I'm filtering to plug-in blenders with at least 4 stars.",
	},
	{
		label = "Filtered to 4★ and up, plug-in only",
		doing = "Filtering the results",
		icon = "filter",
		meta = "12 left",
	},
	{ say = "I found three under £50. Checking they can handle frozen fruit." },
	{
		label = "Opened Salter Nutri Pro",
		doing = "Opening Salter Nutri Pro",
		icon = "pointer",
		meta = "£24.99 · 4.4★",
		run = { "Checking product pages", "Checked 3 product pages", "layers" },
	},
	{
		label = "Read price and reviews",
		doing = "Reading price and reviews",
		icon = "scan",
		meta = "£24.99 · 4.4★",
	},
	{ aside = "Good value. Checking the NutriBullet next." },
	{
		label = "Opened NutriBullet 600",
		doing = "Opening NutriBullet 600",
		icon = "pointer",
		meta = "£49.00 · 4.7★",
	},
	{ label = "Captured the spec table", doing = "Capturing the spec table", icon = "camera", meta = "3:2" },
	{
		label = "Opened Ninja Blast BC151",
		doing = "Opening Ninja Blast BC151",
		icon = "pointer",
		meta = "£34.99 · 4.5★",
		retry = true,
	},
	{
		label = "Read price and reviews",
		doing = "Reading price and reviews",
		icon = "scan",
		meta = "£34.99 · 4.5★",
	},
	{
		skill = "Price check",
		label = "Read 3 seller listings",
		doing = "Reading 3 seller listings",
		icon = "scan",
		meta = "3 tabs",
	},
	{
		skill = "Price check",
		label = "Worked out price per 100 W",
		doing = "Working out price per 100 W",
		icon = "calc",
		meta = "£7.13 best",
	},
	{
		skill = "Price check",
		label = "Added delivery costs",
		doing = "Adding delivery costs",
		icon = "truck",
		meta = "+£4.49 on 1",
	},
	{ skill = "Price check", label = "Checked coupons", doing = "Checking coupons", icon = "ticket", meta = "none" },
	{
		skill = "Recipe macros",
		label = "Checked macros for a 600 ml shake",
		doing = "Checking shake macros",
		icon = "scan",
		meta = "32 g protein",
	},
	{ say = "The Salter is the best value. Adding it to your basket needs your OK." },
	{ approval = true, label = "Add Salter Nutri Pro to basket", doing = "Waiting for you", icon = "hand", meta = "" },
	{
		label = "Added Salter Nutri Pro to basket",
		doing = "Adding to basket",
		icon = "cart",
		meta = "£24.99",
		run = { "Adding to basket", "Added to basket", "cart" },
	},
	{ answer = true },
}
local ANSWER = #SCRIPT
local APPROVAL = ANSWER - 2
local SKILLS = { ["Price check"] = { "preloaded" }, ["Recipe macros"] = { "loaded on demand · 1.2s", C.issue } }

-- Lucide icons, as the lab draws them.
local PATHS = {
	chevron = [[<path d="m6 9 6 6 6-6"/>]],
	plus = [[<path d="M5 12h14"/><path d="M12 5v14"/>]],
	mic = [[<path d="M12 19v3"/><path d="M19 10v2a7 7 0 0 1-14 0v-2"/><rect x="9" y="2" width="6" height="13" rx="3"/>]],
	up = [[<path d="m5 12 7-7 7 7"/><path d="M12 19V5"/>]],
	briefcase = [[<path d="M12 12h.01"/><path d="M16 6V4a2 2 0 0 0-2-2h-4a2 2 0 0 0-2 2v2"/><path d="M22 13a18.15 18.15 0 0 1-20 0"/><rect width="20" height="14" x="2" y="6" rx="2"/>]],
	context = [[<path d="m16 16 2 2 4-4"/><path d="M21 10V8a2 2 0 0 0-1-1.73l-7-4a2 2 0 0 0-2 0l-7 4A2 2 0 0 0 3 8v8a2 2 0 0 0 1 1.73l7 4a2 2 0 0 0 2 0l2-1.14"/><path d="m7.5 4.27 9 5.15"/><polyline points="3.29 7 12 12 20.71 7"/><line x1="12" x2="12" y1="22" y2="12"/>]],
	memory = [[<path d="M13.4 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2v-7.4"/><path d="M2 6h4"/><path d="M2 10h4"/><path d="M2 14h4"/><path d="M2 18h4"/><path d="M21.378 5.626a1 1 0 1 0-3.004-3.004l-5.01 5.012a2 2 0 0 0-.506.854l-.837 2.87a.5.5 0 0 0 .62.62l2.87-.837a2 2 0 0 0 .854-.506z"/>]],
	skill = [[<rect width="18" height="18" x="3" y="3" rx="2"/><line x1="9" x2="15" y1="15" y2="9"/>]],
	globe = [[<circle cx="12" cy="12" r="10"/><path d="M12 2a14.5 14.5 0 0 0 0 20 14.5 14.5 0 0 0 0-20"/><path d="M2 12h20"/>]],
	search = [[<circle cx="11" cy="11" r="8"/><path d="m21 21-4.3-4.3"/>]],
	sort = [[<path d="m21 16-4 4-4-4"/><path d="M17 20V4"/><path d="m3 8 4-4 4 4"/><path d="M7 4v16"/>]],
	scan = [[<path d="M3 7V5a2 2 0 0 1 2-2h2"/><path d="M17 3h2a2 2 0 0 1 2 2v2"/><path d="M21 17v2a2 2 0 0 1-2 2h-2"/><path d="M7 21H5a2 2 0 0 1-2-2v-2"/><circle cx="12" cy="12" r="1"/><path d="M18.944 12.33a1 1 0 0 0 0-.66 7.5 7.5 0 0 0-13.888 0 1 1 0 0 0 0 .66 7.5 7.5 0 0 0 13.888 0"/>]],
	note = [[<path d="M13.4 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2v-7.4"/><path d="M2 6h4"/><path d="M2 10h4"/><path d="M2 14h4"/><path d="M2 18h4"/><path d="M21.378 5.626a1 1 0 1 0-3.004-3.004l-5.01 5.012a2 2 0 0 0-.506.854l-.837 2.87a.5.5 0 0 0 .62.62l2.87-.837a2 2 0 0 0 .854-.506z"/>]],
	filter = [[<path d="M2 5h20"/><path d="M6 12h12"/><path d="M9 19h6"/>]],
	layers = [[<path d="M12.83 2.18a2 2 0 0 0-1.66 0L2.6 6.08a1 1 0 0 0 0 1.83l8.58 3.91a2 2 0 0 0 1.66 0l8.58-3.9a1 1 0 0 0 0-1.83z"/><path d="M2 12a1 1 0 0 0 .58.91l8.6 3.91a2 2 0 0 0 1.65 0l8.58-3.9A1 1 0 0 0 22 12"/><path d="M2 17a1 1 0 0 0 .58.91l8.6 3.91a2 2 0 0 0 1.65 0l8.58-3.9A1 1 0 0 0 22 17"/>]],
	pointer = [[<path d="M14 4.1 12 6"/><path d="m5.1 8-2.9-.8"/><path d="m6 12-1.9 2"/><path d="M7.2 2.2 8 5.1"/><path d="M9.037 9.69a.498.498 0 0 1 .653-.653l11 4.5a.5.5 0 0 1-.074.949l-4.349 1.041a1 1 0 0 0-.74.739l-1.04 4.35a.5.5 0 0 1-.95.074z"/>]],
	camera = [[<path d="M14.5 4h-5L7 7H4a2 2 0 0 0-2 2v9a2 2 0 0 0 2 2h16a2 2 0 0 0 2-2V9a2 2 0 0 0-2-2h-3l-2.5-3z"/><circle cx="12" cy="13" r="3"/>]],
	calc = [[<rect width="16" height="20" x="4" y="2" rx="2"/><line x1="8" x2="16" y1="6" y2="6"/><line x1="16" x2="16" y1="14" y2="18"/><path d="M16 10h.01"/><path d="M12 10h.01"/><path d="M8 10h.01"/><path d="M12 14h.01"/><path d="M8 14h.01"/><path d="M12 18h.01"/><path d="M8 18h.01"/>]],
	truck = [[<path d="M14 18V6a2 2 0 0 0-2-2H4a2 2 0 0 0-2 2v11a1 1 0 0 0 1 1h2"/><path d="M15 18H9"/><path d="M19 18h2a1 1 0 0 0 1-1v-3.65a1 1 0 0 0-.22-.624l-3.48-4.35A1 1 0 0 0 17.52 8H14"/><circle cx="17" cy="18" r="2"/><circle cx="7" cy="18" r="2"/>]],
	ticket = [[<path d="M2 9a3 3 0 0 1 0 6v2a2 2 0 0 0 2 2h16a2 2 0 0 0 2-2v-2a3 3 0 0 1 0-6V7a2 2 0 0 0-2-2H4a2 2 0 0 0-2 2Z"/><path d="M9 9h.01"/><path d="m15 9-6 6"/><path d="M15 15h.01"/>]],
	hand = [[<path d="M18 11V6a2 2 0 0 0-2-2a2 2 0 0 0-2 2"/><path d="M14 10V4a2 2 0 0 0-2-2a2 2 0 0 0-2 2v2"/><path d="M10 10.5V6a2 2 0 0 0-2-2a2 2 0 0 0-2 2v8"/><path d="M18 8a2 2 0 1 1 4 0v6a8 8 0 0 1-8 8h-2c-2.8 0-4.5-.86-5.99-2.34l-3.6-3.6a2 2 0 0 1 2.83-2.82L7 15"/>]],
	cart = [[<circle cx="8" cy="21" r="1"/><circle cx="19" cy="21" r="1"/><path d="M2.05 2.05h2l2.66 12.42a2 2 0 0 0 2 1.58h9.78a2 2 0 0 0 1.95-1.57l1.65-7.43H5.12"/>]],
	check = [[<path d="M20 6 9 17l-5-5"/>]],
}
local function icon(name, size, color, props)
	local node = {
		code = '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="#fff" '
			.. 'stroke-width="2" stroke-linecap="round" stroke-linejoin="round">'
			.. PATHS[name]
			.. "</svg>",
		w = z(size),
		h = z(size),
		tint = color,
		shrink = 0,
	}
	for k, v in pairs(props or {}) do
		node[k] = v
	end
	return ui.svg(node)
end
-- Text in the lab's faces; sizes and line heights in its CSS pixels.
local function text(content, size, color, props, font)
	local node: { [any]: any } = { content, font = font or SANS, text_size = z(size), color = color }
	for k, v in pairs(props or {}) do
		node[k] = v
	end
	if node.line_height then
		node.line_height = z(node.line_height)
	end
	return ui.text(node)
end
local function mono(content, size, color, props)
	return text(content, size, color, props, MONO)
end
-- Columns stretch their children across, as CSS flex columns do.
local function col(props)
	if props.items == nil then
		props.items = "stretch"
	end
	return ui.column(props)
end

-- The run. Rust keeps this state across reloads; a reload starts it over.
local phase = ui.state("phase", "connecting") -- connecting · working · done
local cur = ui.state("cur", 0) -- the live step
local decision = ui.state("decision", "")
local expanded = ui.state("expanded", true)
local opened = ui.state("opened", {}) -- groups folded or unfolded by hand
local clock = ui.state("clock", "0s")
local step_clock = ui.state("step_clock", "0s")
local group_clock = ui.state("group_clock", "0s")
local actions = ui.state("actions", 0)
local paused = ui.state("paused", false)
local slow = ui.state("slow", false)

local function duration(at)
	local s = SCRIPT[at]
	return (s and s.say) and PACE * 1.5 or PACE
end
local function fmt(seconds)
	seconds = math.max(0, math.floor(seconds + 0.5))
	local m = math.floor(seconds / 60)
	return m > 0 and string.format("%dm %02ds", m, seconds % 60) or (seconds .. "s")
end
local function live(at)
	return phase:get() == "working" and cur:get() == at
end

-- Where each step goes: narration on its own, tools gathered into runs.
local function blocks(upto)
	local out, group = { { id = "ctx", kind = "ctx" } }, nil
	for at = 1, math.min(upto, ANSWER - 1) do
		local s = SCRIPT[at]
		if s.say then
			group = nil
			table.insert(out, { id = "s" .. at, kind = "say", at = at })
		elseif s.aside and group then
			table.insert(group.items, at)
		elseif s.aside then
			table.insert(out, { id = "s" .. at, kind = "say", at = at })
		else
			if not group or s.run or s.skill ~= group.skill then
				group = { id = "g" .. at, kind = "run", at = at, skill = s.skill, items = {} }
				table.insert(out, group)
			end
			table.insert(group.items, at)
		end
	end
	return out
end
local trace = ui.computed(function()
	if phase:get() == "connecting" then
		return {}
	end
	return blocks(phase:get() == "done" and ANSWER or cur:get())
end)

-- Clocks tick eight times a second, a lab second each.
local step_started, group_started = 0, 0
local ticker = anime.createTimer {
	frameRate = CLOCK,
	autoplay = false,
	onUpdate = function(t)
		local now = t.currentTime / 1000
		clock:set(fmt(now * CLOCK))
		step_clock:set(fmt((now - step_started) * CLOCK))
		group_clock:set(fmt((now - group_started) * CLOCK))
	end,
}

local step_timer
local advance, decide
local function schedule()
	if step_timer then
		step_timer:cancel()
	end
	local s = SCRIPT[cur:peek()]
	if phase:peek() ~= "working" or not s then
		return
	end
	if s.approval then
		if AUTO_APPROVE > 0 then
			step_timer = anime.createTimer {
				duration = AUTO_APPROVE * 1000,
				onComplete = function()
					decide(true)
				end,
			}
		end
		return
	end
	step_timer = anime.createTimer {
		duration = duration(cur:peek()) * 1000,
		onComplete = function()
			advance()
		end,
	}
end
function advance()
	local at = cur:peek()
	if at >= ANSWER then
		return
	end
	local now = ticker.currentTime / 1000
	step_started = now
	local next = SCRIPT[at + 1]
	if next and (next.run or next.say or next.skill ~= SCRIPT[at].skill) then
		group_started = now
	end
	ui.batch(function()
		if SCRIPT[at].icon then
			actions:set(actions:peek() + 1)
		end
		cur:set(at + 1)
		if at + 1 == ANSWER then
			phase:set("done")
			expanded:set(false)
			ticker:pause()
		end
	end)
	schedule()
end
function decide(yes)
	if not live(APPROVAL) then
		return
	end
	decision:set(yes and "approved" or "denied")
	if not yes then
		cur:set(ANSWER - 1)
	end
	advance()
end

-- Every animation by class gets its own class.
local serial = 0
local function class(prefix)
	serial += 1
	return prefix .. serial
end

-- The lab's 8×8 bayer thresholds, for the shimmer's dithered masks.
local BAYER = {}
do
	local function b2(x, y)
		x, y = math.floor(x), math.floor(y)
		return (x * 0.5 + y * y * 0.75) % 1
	end
	for y = 0, 7 do
		BAYER[y] = {}
		for x = 0, 7 do
			local b4h = b2(x * 0.25, y * 0.25) * 0.25 + b2(x * 0.5, y * 0.5)
			BAYER[y][x] = b4h * 0.25 + b2(x, y)
		end
	end
end
local function smooth(e0, e1, x)
	local t = math.clamp((x - e0) / (e1 - e0), 0, 1)
	return t * t * (3 - 2 * t)
end
-- The orb: the lab's bayer-dither shader in sphere mode, run per pixel.
-- One frame is the light turned 9° further and the noise moved on; the
-- orb steps through frames at the lab's 12 fps (`fps` caps its repaints).
-- params[0] = (cell size, -, -, still frame or -1), params[1] = ink.
local ORB = ui.shader(
	[[
fn bayer2(a: vec2<f32>) -> f32 {
    let b = floor(a);
    return fract(dot(b, vec2<f32>(0.5, b.y * 0.75)));
}
fn bayer4(a: vec2<f32>) -> f32 { return bayer2(0.5 * a) * 0.25 + bayer2(a); }
fn bayer8(a: vec2<f32>) -> f32 { return bayer4(0.5 * a) * 0.25 + bayer2(a); }
fn hash(q: vec2<f32>) -> f32 {
    var p = fract(q * vec2<f32>(123.34, 456.21));
    p += dot(p, p + 45.32);
    return fract(p.x * p.y);
}
fn vnoise(p: vec2<f32>) -> f32 {
    let i = floor(p);
    let f = fract(p);
    let u = f * f * (3.0 - 2.0 * f);
    return mix(mix(hash(i), hash(i + vec2<f32>(1.0, 0.0)), u.x),
               mix(hash(i + vec2<f32>(0.0, 1.0)), hash(i + vec2<f32>(1.0, 1.0)), u.x), u.y);
}
fn fbm(q: vec2<f32>) -> f32 {
    var p = q;
    var v = 0.0;
    var a = 0.5;
    for (var i = 0; i < 5; i++) {
        v += a * vnoise(p);
        p = p * 2.03 + vec2<f32>(1.7, 9.2);
        a *= 0.5;
    }
    return v;
}
fn fill(input: FillInput) -> vec4<f32> {
    let cell = input.params[0].x;
    let id = floor(input.p / cell);
    let uv = (id + 0.5) * cell / input.size;
    let still = input.params[0].w;
    let frame = select(floor(input.time * 12.0) % 40.0, still, still >= 0.0);
    let s = (uv - 0.5) / 0.5;
    let r2 = dot(s, s);
    if (r2 >= 1.0) { return vec4<f32>(0.0); }
    let a = radians(241.2 + frame * 9.0);
    let l = normalize(vec3<f32>(cos(a), sin(a), 0.9));
    let lam = max(dot(vec3<f32>(s, sqrt(1.0 - r2)), l), 0.0);
    var v = mix(0.03, 1.0, pow(lam, 1.1));
    v += (fbm(uv * 3.0 + frame * 0.37) - 0.5) * 0.9 * smoothstep(0.0, 0.25, v);
    let on = step(bayer8(id) + 0.5 / 64.0, clamp(v, 0.0, 1.0));
    let ink = input.params[1];
    return vec4<f32>(ink.rgb, ink.a * on);
}
]],
	{ fps = 12 }
)
local function orb(size, still, ink, base)
	return ui.div {
		w = z(size),
		h = z(size),
		rounded = z(size / 2),
		bg = base or C.orb,
		shrink = 0,
		-- The lab draws a pixel per point at its size; ours is scaled by Z.
		bg_shader = ORB,
		shader_params = { { z(1), 0, 0, still and 3 or -1 }, ink or C.text },
		on_mount = not still and function(el)
			anime.animate(
				el,
				{ shaderTime = { 0, 3600 }, duration = 3600000, ease = "linear", loop = true, frameRate = 12 }
			)
		end or nil,
	}
end

-- The lab's shimmer: a dithered band (bayer threshold of a soft, slightly
-- skewed peak) lighting a bright copy of the label as it sweeps across at
-- 90 px/s, then resting 0.6 s.
local BAND = 28
local band = ui.bitmap(BAND * 2, 24, function(x, y)
	local m = math.abs(x + 0.5 - BAND - 4 * (y / 24 - 0.5)) / BAND
	local f = 0.9 * (1 - smooth(0, 1, m))
	return (BAYER[y % 8][x % 8] + 0.5 / 64 < f) and 1 or 0
end)
local function shimmer(content, size, props)
	local width = utf8.len(content) * size * 0.52
	local travel = (width + BAND * 2) / 90 * 1000
	local light = {}
	for k, v in pairs(props or {}) do
		light[k] = v
	end
	light.mask = { band, w = z(BAND * 2), h = z(24), x = -z(BAND * 2) }
	light.on_mount = function(el)
		anime.animate(
			el,
			{ maskX = { -z(BAND * 2), "100%" }, duration = travel, ease = "linear", loop = true, loopDelay = 600 }
		)
	end
	return ui.overlay {
		shrink = 1,
		min_w = 0,
		text(content, size, C.sub, props),
		text(content, size, C.ink, light),
	}
end

-- "Connecting" is its own progress: lit from the left through a dithered
-- ramp, creeping as p(t) = 0.95 − 0.75·e^(−0.64 t), filling at the first token.
local function connecting(size)
	local width = math.floor(utf8.len("Connecting") * size * 0.52)
	local ramp = ui.bitmap(width * 2, 24, function(x, y)
		local v = 0.5 + 3.125 * (width - x) / width
		return (BAYER[y % 8][x % 8] + 0.5 / 64 < math.clamp(v, 0, 1)) and 1 or 0
	end)
	local function p(t)
		return 0.95 - 0.75 * math.exp(-0.64 * t)
	end
	local stop = p(CONNECT)
	return ui.overlay {
		text("Connecting", size, C.sub),
		text("Connecting", size, C.ink, {
			mask = { ramp, w = z(width * 2), h = z(24), x = (p(0) - 1) * z(width) },
			on_mount = function(el)
				anime
					.createTimeline()
					:add(el, {
						maskX = (stop - 1) * z(width),
						duration = CONNECT * 1000,
						ease = function(t)
							return (p(t * CONNECT) - p(0)) / (stop - p(0))
						end,
					})
					:add(el, { maskX = 0, duration = 180, ease = "linear" })
			end,
		}),
	}
end

-- Words blurring in, one after another, once the row has grown.
local streamed = {}
local function words(key, content, size, lh, color, total, after)
	if streamed[key] then
		return text(content, size, color, { line_height = lh, wrap = true })
	end
	streamed[key] = true
	local name = class("w")
	local count = select(2, string.gsub(content, "%S+", ""))
	local gap = math.min(70, total * 0.7 / math.max(1, count))
	return anime.text.split(content, {
		words = { class = name },
		style = { font = SANS, text_size = z(size), color = color, opacity = 0, line_height = z(lh) },
		props = {
			on_mount = function()
				anime.animate("." .. name, {
					opacity = { 0, 1 },
					blur = { 2, 0 },
					duration = WORD,
					ease = "out(2)",
					delay = anime.stagger(gap, { start = after or 0 }),
				})
			end,
		},
	})
end

-- The lab's transitions.
local swap = {
	enter = { opacity = { 0, 1 }, blur = { 4, 0 }, duration = FADE * 1.5, ease = "out(2)" },
	exit = { opacity = 0, blur = 4, duration = FADE, ease = "out(2)" },
	layout = "height",
}
local fold = {
	enter = { opacity = { 0, 1 }, blur = { 4, 0 }, duration = 480, ease = "out(2)" },
	exit = { opacity = { 1, 0, 0 }, blur = 4, duration = 400, ease = "out(2)" },
	layout = "height",
}
-- A block arrives once: folding and unfolding the trace does not replay it.
local seen = {}
-- Rows grow, then fade up.
local function arrival(id, row)
	if seen[id] then
		return row
	end
	seen[id] = true
	return col {
		layout = "height",
		layout_enter = true,
		layout_ease = GROW_EASE,
		layout_duration = GROW,
		col {
			opacity = 0,
			on_mount = function(el)
				anime.animate(
					el,
					{ opacity = { 0, 1 }, y = { z(3), 0 }, duration = 220, delay = GROW, ease = "out(2)" }
				)
			end,
			row,
		},
	}
end

-- A step's marker column: its mark, and the stem in and out of it. `prev`
-- and `next` say whether a step sits above or below; the stem out draws
-- down when the next one arrives.
local function marker(width, top, mark, mark_size, prev, next)
	local line_out = ui.ref()
	local had = next()
	ui.effect(function()
		local now = next()
		if now and not had then
			anime.animate(line_out, { scale_y = { 0, 1 }, duration = GROW, ease = GROW_EASE })
		end
		had = now
	end)
	return col {
		w = z(width),
		shrink = 0,
		items = "center",
		relative = true,
		ui.div {
			absolute = true,
			top = 0,
			h = z(math.max(0, top - 3)),
			w = 1,
			left = z(width / 2) - 0.5,
			bg = C.line,
			opacity = function()
				return prev() and 1 or 0
			end,
		},
		ui.div { h = z(top), shrink = 0 },
		mark,
		ui.div {
			ref = line_out,
			absolute = true,
			top = z(top + mark_size + 3),
			bottom = 0,
			w = 1,
			left = z(width / 2) - 0.5,
			bg = C.line,
			origin = { 0.5, 0 },
			opacity = function()
				return next() and 1 or 0
			end,
		},
	}
end
-- The stem beside a block's body, down to the next block.
local function stem(width, next)
	return ui.div {
		w = z(width),
		shrink = 0,
		relative = true,
		ui.div {
			absolute = true,
			top = 0,
			bottom = 0,
			w = 1,
			left = z(width / 2) - 0.5,
			bg = C.line,
			opacity = function()
				return next() and 1 or 0
			end,
		},
	}
end

-- A step's row: its icon or live orb, its label crossfading from doing to
-- done, its meta on the right.
local function step_head(at, nested, prev, next)
	local s = SCRIPT[at]
	local size, label_size = nested and 12 or 13.33, nested and 11.5 or 12
	return ui.presence(function()
		if live(at) then
			return "live"
		end
		if s.approval and decision:get() ~= "" then
			return decision:get()
		end
		return "done"
	end, function(state)
		local doing = state == "live"
		local waiting = doing and s.approval
		local mark, mark_size, label
		if doing then
			mark, mark_size = orb(12), 12
		elseif s.approval then
			mark, mark_size = icon(state == "denied" and "hand" or "check", size, C.sub), size
		else
			mark, mark_size = icon(s.icon, size, C.muted), size
		end
		if waiting then
			label = text(s.doing .. " · " .. s.label, label_size, C.text, { truncate = true })
		elseif doing then
			label = shimmer(s.doing, label_size)
		elseif s.approval then
			label = text(
				(state == "denied" and "Denied by you · " or "Approved by you · ") .. s.label,
				label_size,
				C.sub,
				{ truncate = true }
			)
		else
			label = text(s.label, label_size, C.sub, { truncate = true })
		end
		return ui.row {
			h = z(22),
			gap = z(nested and 9 or 10),
			items = "stretch",
			marker(nested and 12 or 13.33, (22 - mark_size) / 2, mark, mark_size, prev, next),
			ui.row {
				grow = 1,
				shrink = 1,
				min_w = 0,
				items = "center",
				gap = z(8),
				ui.row { shrink = 1, min_w = 0, label },
				ui.div { grow = 1 },
				(not doing and s.retry) and mono("1 retried", 10, C.issue) or nil,
				mono(doing and "{step_clock}" or s.meta, 10, C.muted, { shrink = 0 }),
			},
		}
	end, swap)
end

local function say_block(at, prev, next, grew)
	local s = SCRIPT[at]
	return ui.row {
		gap = z(10),
		items = "stretch",
		marker(13.33, 6.67, ui.div { w = z(5.33), h = z(5.33), rounded = z(2.67), bg = C.muted }, 5.33, prev, next),
		col {
			grow = 1,
			shrink = 1,
			min_w = 0,
			pb = z(12),
			words("s" .. at, s.say or s.aside, 12, 18.6, C.body, duration(at) * 1000, grew and GROW or 0),
		},
	}
end

local function chip(label, skill)
	local color = skill and C.skill or C.memory
	return ui.row {
		gap = z(5),
		items = "center",
		px = z(7),
		h = z(20),
		rounded = z(5),
		bg = skill and "#B9A6F214" or "#A8BFC414",
		border = z(1),
		border_color = skill and "#B9A6F233" or "#A8BFC433",
		icon(skill and "skill" or "memory", 11.33, color),
		mono(label, 9.5, color),
	}
end
local function context_block(prev, next)
	return col {
		ui.row {
			h = z(22),
			gap = z(10),
			items = "stretch",
			marker(13.33, 4.33, icon("context", 13.33, C.muted), 13.33, prev, next),
			ui.row {
				grow = 1,
				items = "center",
				gap = z(8),
				text("Started with context", 12, C.sub),
				ui.div { grow = 1 },
				mono("3 memories · 2 skills", 10, C.muted),
			},
		},
		ui.row {
			items = "stretch",
			stem(13.33, next),
			ui.row {
				grow = 1,
				shrink = 1,
				min_w = 0,
				flex_wrap = true,
				gap = z(4),
				pl = z(10),
				pt = z(5),
				pb = z(12),
				chip("Amazon UK · Prime"),
				chip("Under £50"),
				chip("Oat-milk shakes"),
				chip("Price check", true),
				chip("Amazon UK", true),
			},
		},
	}
end

-- A run of tools: a plain row while it holds one step, then a head with a
-- count whose steps fold away once they finish.
local function run_block(block, prev, next)
	local items = ui.computed(function()
		for _, b in trace:get() do
			if b.id == block.id then
				return b.items
			end
		end
		return {}
	end)
	local function is_live()
		for _, at in items:get() do
			if live(at) then
				return true
			end
		end
		return false
	end
	local function grouped()
		return #items:get() > 1 or block.skill ~= nil
	end
	local function is_open()
		local hand = opened:get()[block.id]
		if hand ~= nil then
			return hand
		end
		return is_live()
	end
	local head = SCRIPT[block.at]
	local function group_head(doing)
		local run = head.run
		local skill = block.skill and SKILLS[block.skill]
		local title = block.skill or (run and run[doing and 1 or 2]) or head.label
		local retried = false
		for _, at in items:peek() do
			if SCRIPT[at].retry and not live(at) then
				retried = true
			end
		end
		local mark = block.skill and icon("skill", 13.33, C.skill)
			or icon((run and run[3]) or head.icon, 13.33, doing and C.sub or C.muted)
		local angle = ui.computed(function()
			return is_open() and 0 or -90
		end)
		return ui.row {
			h = z(22),
			gap = z(10),
			cursor = "pointer",
			items = "stretch",
			on_click = function()
				local map = opened:peek()
				map[block.id] = not is_open()
				opened:set(map)
			end,
			marker(13.33, 4.33, mark, 13.33, prev, next),
			ui.row {
				grow = 1,
				shrink = 1,
				min_w = 0,
				items = "center",
				gap = z(8),
				(doing and not block.skill) and shimmer(title, 12) or text(title, 12, C.sub),
				block.skill and ui.div {
					px = z(4),
					h = z(14),
					rounded = z(3),
					bg = "#B9A6F21a",
					items = "center",
					mono("skill", 9, C.skill),
				} or nil,
				mono(function()
					return tostring(#items:get())
				end, 10, C.muted),
				icon("chevron", 11.33, C.muted, {
					rotate = ui.tween(angle, { duration = 200, ease = "out(3)" }),
					opacity = function()
						return is_open() and 1 or 0
					end,
				}),
				ui.div { grow = 1 },
				retried and mono("1 retried", 10, C.issue) or nil,
				skill and mono(skill[1], 10, skill[2] or C.muted) or nil,
				(doing and not block.skill) and mono("{group_clock}", 10, C.muted) or nil,
			},
		}
	end
	return col {
		ui.presence(function()
			if not grouped() then
				return "single"
			end
			return is_live() and "live" or "done"
		end, function(mode)
			if mode == "single" then
				return col {
					step_head(block.at, false, prev, next),
					ui.row { h = z(12), items = "stretch", stem(13.33, next) },
				}
			end
			return group_head(mode == "live")
		end, swap),
		-- The steps, folding with a spring while they fade.
		ui.presence(function()
			if not grouped() then
				return nil
			end
			return is_open() and "open" or "closed"
		end, function(state)
			if state == "closed" then
				return ui.row { h = z(12), items = "stretch", stem(13.33, next) }
			end
			return ui.row {
				items = "stretch",
				stem(13.33, next),
				col {
					grow = 1,
					shrink = 1,
					min_w = 0,
					pl = z(9),
					pt = z(5),
					ui.each(items, function(at, i)
						local s = SCRIPT[at]
						local function has_prev()
							return i > 1
						end
						local function has_next()
							return i < #items:get()
						end
						local row
						if s.aside then
							row = say_block(at, has_prev, has_next, not seen[block.id .. ":" .. at])
						else
							row = col {
								step_head(at, true, has_prev, has_next),
								ui.row {
									h = function()
										return has_next() and z(7.33) or 0
									end,
									items = "stretch",
									stem(12, has_next),
								},
							}
						end
						return arrival(block.id .. ":" .. at, row)
					end, {
						key = function(at)
							return tostring(at)
						end,
					}),
				},
			}
		end, fold),
	}
end

local function block_view(block, i)
	local function prev()
		return i > 1
	end
	local function next()
		return i < #trace:get()
	end
	local grew = not seen[block.id]
	local view
	if block.kind == "ctx" then
		view = context_block(prev, next)
	elseif block.kind == "say" then
		view = say_block(block.at, prev, next, grew)
	else
		view = run_block(block, prev, next)
	end
	return arrival(block.id, view)
end

-- The summary: connecting, then working with a clock, then done. Clicking
-- it folds the trace.
local function chevron(size, open)
	local angle = ui.computed(function()
		return open() and 0 or -90
	end)
	return icon("chevron", size, C.muted, { rotate = ui.tween(angle, { duration = 200, ease = "out(3)" }) })
end
local function summary()
	return ui.row {
		h = z(20),
		items = "center",
		gap = z(8),
		cursor = "pointer",
		on_click = function()
			if phase:peek() ~= "connecting" then
				expanded:set(not expanded:peek())
			end
		end,
		-- The orb stills once the run is done.
		ui.presence(function()
			return phase:get() == "done" and "still" or "live"
		end, function(v)
			return orb(16, v == "still")
		end, swap),
		ui.presence(phase, function(p)
			if p == "connecting" then
				return ui.row { items = "center", connecting(12.5) }
			elseif p == "working" then
				return ui.row {
					items = "center",
					gap = z(8),
					text("Working · {clock}", 12.5, C.sub),
					chevron(12, function()
						return expanded:get()
					end),
				}
			end
			return ui.row {
				items = "center",
				gap = z(8),
				text("Worked for {clock}", 12.5, C.sub),
				chevron(12, function()
					return expanded:get()
				end),
			}
		end, swap),
		ui.div { grow = 1 },
		ui.show(
			function()
				return phase:get() ~= "connecting"
			end,
			mono(function()
				return actions:get() .. (actions:get() == 1 and " action" or " actions")
			end, 10, C.muted)
		),
	}
end

-- The answer, once the run is done: the lead streams in, then sources and
-- picks rise in turn.
local function favicon(site, size)
	return ui.div {
		w = z(size or 12),
		h = z(size or 12),
		rounded = z(3),
		overflow = "hidden",
		shrink = 0,
		ui.image {
			src = "favicons/" .. site:match("^[^.]+") .. ".png",
			w = z(size or 12),
			h = z(size or 12),
			fit = "cover",
		},
	}
end
-- A source chip: favicon, domain, and how many more sources say the same.
local function citation(site, more)
	return ui.row {
		h = z(18),
		pl = z(3),
		pr = z(7),
		gap = z(5),
		rounded = z(9),
		bg = "#FFFFFF10",
		items = "center",
		shrink = 0,
		favicon(site),
		text(site, 11, C.domain, { weight = 500 }),
		more and text(more, 11, C.muted, { weight = 500 }) or nil,
	}
end
local function pick(price, name, sites, why, more)
	local tags = {}
	for i, site in sites do
		table.insert(tags, citation(site, i == #sites and more or nil))
	end
	return ui.row {
		gap = z(12),
		items = "start",
		mono(price, 11, C.text, { w = z(44), shrink = 0, line_height = 18 }),
		col {
			grow = 1,
			shrink = 1,
			min_w = 0,
			gap = z(2),
			ui.row {
				gap = z(6),
				items = "center",
				flex_wrap = true,
				text(name, 12.5, C.text, { weight = 500 }),
				tags,
			},
			text(why, 12, C.sub, { wrap = true, line_height = 18.6 }),
		},
	}
end
local function rise(i, node)
	return col {
		opacity = 0,
		on_mount = function(el)
			anime.animate(
				el,
				{ opacity = { 0, 1 }, y = { z(4), 0 }, duration = 260, delay = 900 + i * 220, ease = "out(2)" }
			)
		end,
		node,
	}
end
local function answer()
	return col {
		gap = z(10),
		words(
			"answer",
			"For protein shakes, get a plug-in personal blender rather than a cordless one. The cheap cordless ones stall on frozen fruit and ice.",
			12.5,
			19.375,
			C.text,
			1400
		),
		rise(0, ui.row { gap = z(4), citation("rtings.com"), citation("which.co.uk") }),
		rise(
			1,
			col {
				gap = z(8),
				pick(
					"£24.99",
					"Salter Nutri Pro",
					{ "amazon.co.uk" },
					"350 W, 2 bottles. The best value that still crushes ice."
				),
				pick(
					"£34.99",
					"Ninja Blast BC151",
					{ "amazon.co.uk", "rtings.com" },
					"Cordless, but it's the only portable one that handles frozen fruit."
				),
				pick("£49.00", "NutriBullet 600", { "which.co.uk" }, "The safe pick if you blend every day.", "+2"),
			}
		),
	}
end

local function transcript()
	return col {
		gap = z(18),
		ui.row {
			justify = "end",
			ui.div {
				max_w = z(300),
				px = z(13),
				py = z(9),
				rounded = z(12),
				bg = "#1F1E1Be6",
				text(
					"Find the cheapest blender you'd recommend for smoothies, like protein shakes",
					12.5,
					C.text,
					{ wrap = true, line_height = 18.125 }
				),
			},
		},
		-- 14 px between the summary, the trace and the answer: each carries
		-- its own space, so a folded trace leaves none.
		col {
			summary(),
			ui.presence(function()
				return (phase:get() ~= "connecting" and expanded:get()) and "open" or nil
			end, function()
				return col {
					pt = z(14),
					ui.each(trace, block_view, {
						key = function(b)
							return b.id
						end,
					}),
				}
			end, fold),
			ui.presence(function()
				return phase:get() == "done" and "answer" or nil
			end, function()
				return col { pt = z(14), answer() }
			end, { layout = "height" }),
		},
	}
end

-- The composer: frosted glass over the glass window. While the run waits
-- for an OK, the approval docks on top of it. Sizes from the lab's CSS.
local function detail(key, value)
	return ui.row {
		h = z(14.67),
		gap = z(8),
		items = "center",
		text(key, 11.5, C.key, { w = z(88), shrink = 0 }),
		text(value, 11.5, C.text),
	}
end
local function button(id, label, primary, on_click, extra)
	return ui.button {
		id = id,
		h = z(28),
		px = z(12),
		gap = z(7),
		rounded = z(8),
		items = "center",
		cursor = "pointer",
		bg = primary and C.text or "#1F1F1D",
		hover = { bg = primary and "#FFFFFF" or "#2a2a27" },
		border = primary and 0 or 1,
		border_color = "#33322D",
		active = { scale = 0.97 },
		on_click = on_click,
		text(label, 12.5, primary and "#121211" or C.text, { weight = 500 }),
		extra,
	}
end
local function dock()
	return col {
		pt = z(10),
		px = z(8),
		pb = z(8),
		gap = z(10),
		bg = "#171716e6",
		relative = true,
		-- The hairline between the dock and the input below it.
		ui.div { absolute = true, left = 0, right = 0, bottom = 0, h = 1, bg = "#33322D" },
		ui.row {
			h = z(18),
			px = z(4),
			gap = z(7),
			items = "center",
			orb(12, true, C.gold, "#25211A"),
			text("Needs your OK", 11.5, C.gold, { weight = 500 }),
			ui.div { grow = 1 },
			ui.row {
				gap = z(5),
				items = "center",
				ui.div { w = z(9.33), h = z(9.33), rounded = z(2), bg = "#DA3743" },
				mono("amazon.co.uk", 10, C.muted),
			},
			icon("chevron", 12, C.key),
		},
		ui.row { px = z(4), text("Add Salter Nutri Pro to your basket?", 13, C.text, { line_height = 18.2 }) },
		col {
			px = z(4),
			gap = z(5),
			detail("Item", "Salter Nutri Pro · 350 W"),
			detail("Price", "£24.99"),
			detail("Delivery", "Tomorrow · free"),
			detail("Booked as", "Sen W. · sen@…"),
			detail("Returns", "Free within 30 days"),
		},
		ui.row {
			pt = z(2),
			pl = z(4),
			gap = z(8),
			items = "center",
			ui.row {
				gap = z(7),
				items = "center",
				ui.div { w = z(13.33), h = z(13.33), rounded = z(4), border = 1, border_color = "#3A3934" },
				text("Always allow on this site", 11.5, C.key),
			},
			ui.div { grow = 1 },
			button("deny", "Deny", false, function()
				decide(false)
			end),
			button(
				"approve",
				"Approve",
				true,
				function()
					decide(true)
				end,
				ui.div {
					h = z(13.33),
					px = z(4),
					rounded = z(4),
					border = 1,
					border_color = "#12121133",
					items = "center",
					justify = "center",
					mono("⏎", 10, "#12121199"),
				}
			),
		},
	}
end
local restart
local function composer()
	local function waiting()
		return live(APPROVAL)
	end
	return col {
		gap = z(6),
		-- One card: the dock and the input share its outline and corners.
		col {
			layout = "height",
			overflow = "hidden",
			rounded = function()
				return waiting() and z(16) or z(22)
			end,
			border = 1,
			border_color = "#33322D",
			bg = "#1C1C1Acc",
			backdrop_blur = 16,
			ui.presence(
				function()
					return waiting() and "dock" or nil
				end,
				dock,
				{
					enter = { opacity = { 0, 1 }, blur = { 4, 0 }, y = { z(8), 0 }, duration = 320, ease = "out(3)" },
					exit = { opacity = 0, blur = 4, duration = FADE, ease = "out(2)" },
				}
			),
			ui.row {
				p = z(6),
				gap = z(8),
				items = "center",
				ui.div {
					w = z(32),
					h = z(32),
					rounded = z(16),
					bg = "#2A2926",
					items = "center",
					justify = "center",
					shrink = 0,
					hover = { bg = "#34332f" },
					icon("plus", 16, "#C9C6BE"),
				},
				ui.row {
					grow = 1,
					shrink = 1,
					min_w = 0,
					ui.presence(function()
						return waiting() and "change" or "ask"
					end, function(v)
						return text(
							v == "change" and "Or tell Ask what to change…"
								or "Ask anything, @ for tabs, / for commands",
							13.5,
							C.place,
							{ truncate = true, line_height = 18.9 }
						)
					end, swap),
				},
				ui.div {
					w = z(32),
					h = z(32),
					rounded = z(16),
					items = "center",
					justify = "center",
					shrink = 0,
					icon("mic", 15.33, C.sub),
				},
				ui.button {
					w = z(32),
					h = z(32),
					rounded = z(16),
					bg = C.text,
					items = "center",
					justify = "center",
					shrink = 0,
					cursor = "pointer",
					hover = { bg = "#FFFFFF" },
					active = { scale = 0.94 },
					on_click = function()
						restart()
					end,
					ui.presence(function()
						return waiting() and "stop" or "send"
					end, function(v)
						if v == "stop" then
							return ui.div { w = z(10), h = z(10), rounded = z(2), bg = "#121211" }
						end
						return icon("up", 16, "#121211")
					end, swap),
				},
			},
		},
		ui.row {
			h = z(26),
			px = z(4),
			items = "center",
			ui.row {
				h = z(26),
				px = z(8),
				gap = z(4),
				items = "center",
				rounded = z(13),
				icon("briefcase", 14, C.sub),
				icon("chevron", 11.33, C.muted),
			},
			ui.div { grow = 1 },
			ui.row {
				h = z(26),
				px = z(8),
				gap = z(6),
				items = "center",
				rounded = z(13),
				orb(13.33, true),
				text("Grok 4.7", 12, C.sub, { weight = 500 }),
				text("High", 12, C.muted),
				icon("chevron", 11.33, C.muted),
			},
		},
	}
end

function restart()
	if step_timer then
		step_timer:cancel()
	end
	streamed, seen = {}, {}
	ui.batch(function()
		phase:set("connecting")
		cur:set(0)
		decision:set("")
		expanded:set(true)
		opened:set {}
		actions:set(0)
		clock:set("0s")
		step_clock:set("0s")
		group_clock:set("0s")
	end)
	step_started, group_started = 0, 0
	ticker:pause()
	ticker:seek(0)
	step_timer = anime.createTimer {
		duration = CONNECT * 1000 + 180,
		onComplete = function()
			ui.batch(function()
				phase:set("working")
				cur:set(1)
			end)
			ticker:play()
			schedule()
		end,
	}
end

-- The lab's controls: replay, pause, step, and slow motion for everything.
local function control(label, on_click, on)
	return ui.button {
		px = 10,
		h = 26,
		rounded = 7,
		items = "center",
		cursor = "pointer",
		bg = function()
			return (on and on:get()) and "#ffffff1f" or "#ffffff0a"
		end,
		border = 1,
		border_color = "#ffffff0d",
		hover = { bg = "#ffffff1a" },
		on_click = on_click,
		ui.text { label, font = SANS, text_size = 12, color = C.sub },
	}
end

restart()

return col {
	w_full = true,
	h_full = true,
	bg = "#0d0d0dcc",
	color = C.text,
	ui.row {
		w_full = true,
		h = 44,
		pl = 84,
		pr = 12,
		gap = 6,
		items = "center",
		window_drag = true,
		ui.text { "Agent trace", font = SANS, text_size = 15, weight = 500, color = C.text },
		ui.text {
			function()
				return paused:get() and "Live run · paused" or "Live run · playing"
			end,
			font = SANS,
			text_size = 12,
			color = C.key,
			ml = 4,
		},
		ui.div { grow = 1 },
		control("Restart", function()
			restart()
		end),
		control(function()
			return paused:get() and "Play" or "Pause"
		end, function()
			paused:set(not paused:peek())
			if paused:peek() then
				anime.engine.pause()
			else
				anime.engine.resume()
			end
		end, paused),
		control("Next step", function()
			if phase:peek() == "connecting" then
				step_timer:complete()
			elseif live(APPROVAL) then
				decide(true)
			else
				advance()
			end
		end),
		control("Slow ¼×", function()
			slow:set(not slow:peek())
			anime.engine.speed = slow:peek() and 0.25 or 1
		end, slow),
	},
	-- The transcript, straight on the window's glass, scrolls under the
	-- frosted composer and follows its end while the run writes.
	ui.overlay {
		grow = 1,
		shrink = 1,
		min_h = 0,
		w_full = true,
		ui.scroll {
			w_full = true,
			h_full = true,
			follow = true,
			col {
				w_full = true,
				items = "center",
				px = 24,
				pt = z(14),
				col { w = z(384), selectable = true, transcript() },
				-- Room for the composer, which docks the approval on top.
				ui.div {
					layout = "height",
					h = function()
						return live(APPROVAL) and z(300) or z(110)
					end,
				},
			},
		},
		col {
			absolute = true,
			left = 0,
			right = 0,
			bottom = 0,
			items = "center",
			pb = z(10),
			col { w = z(396), composer() },
		},
	},
}
