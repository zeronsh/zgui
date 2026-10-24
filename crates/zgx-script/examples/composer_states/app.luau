--!nonstrict
-- (Non-strict until the lab's untyped design nodes are annotated: `zgx check`
-- reports what strict mode would.)
-- The Ask motion lab's composer page: every composer state, stepped through
-- in the lab's order, with the lab's transitions between them.
--
-- The lab draws each state from its design file's nodes (frames, text,
-- icons) and moves between states by diffing the old tree against the new:
-- matched nodes ease their sizes, paddings, radii and positions with the
-- lab's spring and their colours over 200 ms; new nodes open their space
-- with the spring while they fade and scale in; removed nodes close theirs
-- while they fade and scale out; changed text crossfades through a ghost.
-- This file is that renderer and that diff, in zgx. The nodes themselves
-- are the lab's (states.luau, generated from it), so sizes, radii, paddings,
-- colours, fonts and copy are the lab's own numbers.
--
-- Sizes are the lab's CSS pixels at its default 1.5× zoom.

-- For screenshots: a state index to jump to when this file reloads, or
-- { from, to, slow } to play one transition in slow motion, or
-- { state, "upload", slow, presses, spacing ms } to run the Upload test.
local GOTO = nil
local PROFILE = nil -- function(what, seconds) print(what, seconds) end

local Z = 1.5
local function z(n)
	return n * Z
end

-- The lab's data (states.luau, generated): { icons = { name = svg body },
-- states = { … } }.
local DATA = require("states")
local STATES = DATA.states

----------------------------------------------------------------------------
-- Motion: the lab's numbers.
----------------------------------------------------------------------------

-- The lab's `kt()`: a spring from { visualDuration, bounce }, integrated in
-- 1 ms steps until it settles and sampled into a CSS `linear()` easing.
-- { .4, .1 } settles in 651 ms.
local function lab_spring(visual, bounce)
	local w = 2 * math.pi / (visual * 1.2)
	local k = w * w
	local c = 2 * math.clamp(1 - bounce, 0.05, 1) * math.sqrt(k)
	local x, v, t, still, samples = 0, 0, 0, 0, {}
	while t < 2 do
		v += (-k * (x - 1) - c * v) * 0.001
		x += v * 0.001
		t += 0.001
		table.insert(samples, x)
		if math.abs(x - 1) < 1e-3 and math.abs(v) < 0.01 then
			still += 1
			if still > 30 then
				break
			end
		else
			still = 0
		end
	end
	local points = {}
	for i = 0, 48 do
		points[i + 1] = string.format("%.4f", samples[math.floor(i / 48 * (#samples - 1) + 0.5) + 1])
	end
	points[49] = "1"
	return "linear(" .. table.concat(points, ",") .. ")", #samples
end
local SPRING, DUR = lab_spring(0.4, 0.1)
local FADE = 140 -- the lab's crossfade
local EASE_OUT = "cubicBezier(0,0,.58,1)" -- CSS `ease-out`
local EASE_ENTER = "cubicBezier(.16,1,.3,1)"
local EASE_EXIT = "cubicBezier(.4,0,1,1)"

-- Removal waits for the exit; anime timers follow slow motion.
local function after(ms, f)
	anime.createTimer { duration = ms, onComplete = f }
end

----------------------------------------------------------------------------
-- The design nodes, drawn as zgx views (the lab's `ei`).
----------------------------------------------------------------------------

local ALIGN =
	{ start = "start", center = "center", ["end"] = "end", space_between = "between", space_around = "around" }
local SUFFIXES = { "expanded", "collapsed", "hovered", "hover", "selected", "minimised", "minimized", "open", "closed" }

local function fills(v)
	return type(v) == "string" and v:sub(1, 14) == "fill_container"
end
local function layout_of(n)
	if n.type ~= "frame" then
		return "none"
	end
	return n.layout or "horizontal"
end
local function pad(n)
	local p = n.padding
	if p == nil then
		return 0, 0, 0, 0
	end
	if type(p) == "number" then
		return p, p, p, p
	end
	if #p == 2 then
		return p[1], p[2], p[1], p[2]
	end
	return p[1], p[2], p[3], p[4]
end
-- A vertical frame with a fixed height but hugging children takes its
-- content's height (the lab's `Ji`).
local function hugs(n)
	if not n.children or type(n.height) ~= "number" or n.clip then
		return false
	end
	for _, c in n.children do
		if fills(c.height) then
			return false
		end
	end
	return true
end
-- What a node matches across states: its type and name, without a state
-- suffix, numbered among its siblings (the lab's `jn`).
local function match_keys(nodes)
	local seen, out = {}, {}
	for i, n in nodes do
		local name = n.name or n.type
		local lower = name:lower()
		for _, s in SUFFIXES do
			if lower:sub(-(#s + 4)) == " · " .. s then
				name = name:sub(1, #name - #s - 4)
				break
			end
		end
		local k = n.type .. ":" .. name
		seen[k] = (seen[k] or 0) + 1
		out[i] = k .. "#" .. seen[k]
	end
	return out
end

local function svg(name)
	local body = (DATA.icons[name] or ""):gsub("currentColor", "#fff")
	return '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="#fff" '
		.. 'stroke-width="2" stroke-linecap="round" stroke-linejoin="round">'
		.. body
		.. "</svg>"
end

-- Stroked boxes stacked in a column without a gap share their seam: the
-- lower one moves up a pixel (the lab's `_d`).
local function stacked(spec, kids, i)
	local prev, this = kids[i - 1], kids[i]
	return (layout_of(spec) == "vertical" and not spec.gap and prev and prev.stroke and this.stroke) and -1 or nil
end

-- A mounted node: its current design node, its children (live and
-- leaving), and the element once mounted.
local serial = 0
local function instance(spec, po, parent)
	serial += 1
	local inst = { id = "n" .. serial, spec = spec, po = po, parent = parent, kids = {}, by = {} }
	local kids = spec.children or {}
	for i, c in kids do
		local child = instance(c, layout_of(spec), inst)
		child.mt = stacked(spec, kids, i)
		table.insert(inst.kids, child)
		inst.by[child.id] = child
	end
	return inst
end
local function ids(inst)
	local out = {}
	for i, c in inst.kids do
		out[i] = c.id
	end
	return out
end
local function bump(inst)
	if inst.rev then
		inst.rev:set(inst.rev:peek() + 1)
	end
end

local view -- (inst) -> node

-- Where a node sits and how big it is, from its design node and its
-- parent's layout (the lab's `ji`), plus what the transitions set.
local function placement(inst, props)
	local function s()
		inst.rev:get()
		return inst.spec
	end
	local function absolute()
		local n = s()
		return inst.abs ~= nil or n.layoutPosition == "absolute" or inst.po == "none"
	end
	props.absolute = function()
		return absolute() or nil
	end
	-- Absolute children sit against the frame's padding box; ours sit in its
	-- content box, so take the padding back off.
	props.left = function()
		if inst.abs then
			return inst.abs.x
		end
		if not absolute() then
			return nil
		end
		local _, _, _, pl = pad(inst.parent and inst.parent.spec or {})
		return z((s().x or 0) - pl)
	end
	props.top = function()
		if inst.abs then
			return inst.abs.y
		end
		if not absolute() then
			return nil
		end
		local pt = pad(inst.parent and inst.parent.spec or {})
		return z((s().y or 0) - pt)
	end
	local function size(axis)
		local n = s()
		if inst.frozen then
			return inst.frozen[axis]
		end
		local v
		if axis == "w" then
			v = n.width
		else
			v = n.height
		end
		if n.type == "icon" then
			local side = n.width or 16
			return z(axis == "w" and side or (n.height or side))
		end
		if axis == "h" and n.type == "frame" and n.layout == "vertical" and hugs(n) then
			return nil
		end
		if type(v) == "number" then
			return z(v)
		end
		if fills(v) and (inst.po == nil or inst.po == "none") then
			return z(axis == "w" and n.bw or n.bh)
		end
		return nil
	end
	props.w = function()
		return size("w")
	end
	props.h = function()
		return size("h")
	end
	local function grows()
		if inst.frozen then
			return false
		end
		local n = s()
		return (inst.po == "horizontal" and fills(n.width)) or (inst.po == "vertical" and fills(n.height))
	end
	props.grow = function()
		return grows() and 1 or 0
	end
	props.shrink = function()
		return grows() and 1 or 0
	end
	props.basis = function()
		return grows() and 0 or nil
	end
	props.min_w = function()
		if grows() and inst.po == "horizontal" then
			return 0
		end
		local m = s().minWidth
		return m and z(m) or nil
	end
	props.self = function()
		local n = s()
		if inst.frozen then
			return nil
		end
		if (inst.po == "horizontal" and fills(n.height)) or (inst.po == "vertical" and fills(n.width)) then
			return "stretch"
		end
		return nil
	end
	props.mt = function()
		s()
		return inst.mt and z(inst.mt) or nil
	end
	props.opacity = function()
		return s().opacity
	end
	props.hidden = function()
		return s().hidden or false
	end
	-- A literal, so animations of the other transform parts keep it (later
	-- changes of tilt animate).
	props.rotate = inst.spec.rotation and -inst.spec.rotation or nil
	return props
end

-- What a node does once mounted: remember its element, and play its
-- entrance if it arrived with a transition.
local enter
local function mounted(inst)
	return function(el)
		inst.el = el
		if inst.entering then
			inst.entering = false
			enter(inst, el)
		end
	end
end

-- The node's own top and bottom margin (the attachment row's clip reaches
-- 10 px above and below it), which opening and closing work around.
local function own_mb(inst)
	return inst.spec.name == "Attachments" and z(-10) or 0
end

-- The lab's clip as a node opens or closes (`kn`): open, it reaches 20 px
-- past the box; closed, it is cut to nothing from the bottom (a column),
-- from the right (a row), or to the box itself (out of the flow).
local function clip_open()
	local o = z(-20)
	return { o, o, o, o }
end
local function clip_closed(mode, w, h)
	if mode == "vertical" then
		return { 0, 0, h, 0 }
	end
	if mode == "horizontal" then
		return { 0, w, 0, 0 }
	end
	return { 0, 0, 0, 0 }
end
local CLIP_KEYS = { "clipTop", "clipRight", "clipBottom", "clipLeft" }
local function clip_params(params, from, to)
	for i, key in CLIP_KEYS do
		params[key] = { from[i], to[i] }
	end
end

-- A node entering starts closed and transparent; `enter` opens it.
local function entering_props(inst, props)
	if not inst.entering then
		return props
	end
	props.opacity = 0
	inst.clip_from = clip_closed(inst.mode, z(inst.spec.bw), z(inst.spec.bh))
	props.clip_inset = function()
		inst.rev:get()
		return inst.clip_from
	end
	local mode = inst.mode
	if mode == "vertical" then
		props.mb = inst.closed + own_mb(inst)
		props.y = z(6)
		props.scale = 0.98
	elseif mode == "horizontal" then
		props.mr = inst.closed
		props.scale = 0.88
	else
		props.y = z(6)
		props.scale = 0.97
	end
	return props
end

local function text_view(inst)
	inst.rev = ui.use_state(0)
	inst.ghost = ui.use_state("")
	local function s()
		inst.rev:get()
		return inst.spec
	end
	local function font(props)
		props.font = function()
			return s().fontFamily or "Geist"
		end
		props.text_size = function()
			return z(s().fontSize)
		end
		props.weight = function()
			local w = s().fontWeight
			return (w == nil or w == "normal") and 400 or tonumber(w)
		end
		props.line_height = function()
			local n = s()
			return z(n.fontSize * (n.lineHeight or 1.3))
		end
		props.letter_spacing = function()
			return z(s().letterSpacing or 0)
		end
		props.wrap = function()
			return s().textGrowth == "fixed-width"
		end
		return props
	end
	local main = font {
		function()
			return s().content or ""
		end,
		color = function()
			return s().fill or "#EDEBE5"
		end,
	}
	main.on_mount = function(el)
		inst.text_el = el
	end
	local ghost = font {
		function()
			return inst.ghost:get()
		end,
		absolute = true,
		left = 0,
		top = 0,
		opacity = 0,
		color = function()
			return inst.ghost_color or "#EDEBE5"
		end,
		w = function()
			return inst.ghost:get() ~= "" and inst.ghost_w or nil
		end,
	}
	ghost.on_mount = function(el)
		inst.ghost_el = el
	end
	local props = placement(inst, { items = "stretch", ui.text(main), ui.text(ghost) })
	props.on_mount = mounted(inst)
	if s().name == "Reply placeholder" then
		props.cursor = "text"
		props.min_w = z(40)
	end
	return ui.column(entering_props(inst, props))
end

local function icon_view(inst)
	inst.rev = ui.use_state(0)
	local function s()
		inst.rev:get()
		return inst.spec
	end
	-- A new glyph fades in (the lab swaps the SVG and fades it over 140 ms);
	-- a new colour eases in `ease_style`.
	local props = placement(inst, {
		ui.presence(function()
			return s().icon
		end, function(name)
			local n = inst.spec
			local side = n.width or 16
			return ui.svg {
				code = svg(name),
				w = z(side),
				h = z(n.height or side),
				tint = function()
					return s().fill or "#EDEBE5"
				end,
				on_mount = function(el)
					inst.svg_el = el
				end,
			}
		end, { enter = { opacity = { 0, 1 }, duration = FADE, ease = EASE_OUT } }),
	})
	props.on_mount = mounted(inst)
	return ui.column(entering_props(inst, props))
end

-- Corner radii from the design node (`out` grows them, for an outer
-- stroke); changes between states ease per corner in `ease_style`.
local corner_list
local function corners_prop(inst, out)
	return function()
		inst.rev:get()
		local r = corner_list(inst.spec)
		local o = out or 0
		return { z(r[1]) + o, z(r[2]) + o, z(r[3]) + o, z(r[4]) + o }
	end
end

-- A stroke drawn inside the box, over its content, as the lab's inset
-- outline.
local function stroke_layer(inst)
	local n = inst.spec
	local function s()
		inst.rev:get()
		return inst.spec
	end
	-- The lab's inset outline: the node's own border, painted over its
	-- children, on every side or some (a dock's open bottom).
	if n.strokeAlignment ~= "outer" and n.strokeAlignment ~= "center" then
		return nil
	end
	-- An outline outside the box (CSS outline-offset 0 or half the width).
	local sw = n.strokeWidth or 0
	local inset = n.strokeAlignment == "outer" and -z(sw) or -z(sw) / 2
	local layer = {
		absolute = true,
		top = inset,
		left = inset,
		right = inset,
		bottom = inset,
		corners = corners_prop(inst, -inset),
		border = function()
			local m = s()
			return (m.stroke and type(m.strokeWidth) == "number") and z(m.strokeWidth) or 0
		end,
		border_color = function()
			return s().stroke
		end,
	}
	layer.on_mount = function(el)
		inst.stroke_el = el
	end
	return ui.div(layer)
end
local function inset_stroke(inst, props)
	local n = inst.spec
	if n.strokeAlignment == "outer" or n.strokeAlignment == "center" then
		return
	end
	local function s()
		inst.rev:get()
		return inst.spec
	end
	props.border_color = function()
		return s().stroke
	end
	if type(n.strokeWidth) == "table" then
		-- Per-side strokes are CSS borders in the lab: inside the box, they
		-- push the content in, as zgx's borders do.
		local sw = n.strokeWidth
		props.border_sides = { z(sw.top or 0), z(sw.right or 0), z(sw.bottom or 0), z(sw.left or 0) }
	else
		-- An inside stroke drawn over the content (a photo's frame), taking
		-- no room.
		props.border_over = true
		props.border = function()
			local m = s()
			return (m.stroke and type(m.strokeWidth) == "number") and z(m.strokeWidth) or 0
		end
	end
end

-- Whether an attachment row's chips run past its right edge.
local function overflows(n)
	if n.name ~= "Attachments" or layout_of(n) ~= "horizontal" or not n.children then
		return false
	end
	local _, pr, _, pl = pad(n)
	local total = pl + pr + (n.gap or 0) * (#n.children - 1)
	for _, c in n.children do
		total += c.bw
	end
	return total > n.bw + 0.5
end

function corner_list(n)
	if n.type == "ellipse" then
		local r = math.min(n.bw, n.bh) / 2
		return { r, r, r, r }
	end
	local r = n.cornerRadius
	if type(r) == "table" then
		return r
	end
	r = r or 0
	return { r, r, r, r }
end

-- What a click does, as the lab's composer page wires it: Send clears to
-- Empty, the photo stack expands and folds, the attachment list opens and
-- closes, a choice gets picked, a dock's buttons resolve it, and attachment
-- chips and queued messages can be removed.
local click -- (inst), defined with the page
local press_feedback
local function clickable(inst)
	local name, parent = inst.spec.name or "", inst.parent and inst.parent.spec.name or ""
	return name == "Send"
		or name:match("^Photos") ~= nil
		or name == "List header"
		or name:match("^Choice · ") ~= nil
		or name == "Dock actions"
		or name:match("^Remove queued message") ~= nil
		or parent == "Attachments"
		or parent:match("^Chip row") ~= nil
end

-- The lab's dither (bayer-dither.glsl, as its canvas port `Z` draws it):
-- the box cut into cells of `pixel` CSS px, each lit when an ordered 8×8
-- Bayer threshold falls under the pattern's value: a linear ramp (0), a
-- radial one (1), a lit sphere (2) or fbm noise (3), with optional noise on
-- top and a soft edge. Drifting fills move their noise seed with time.
local DITHER_WGSL = [[
fn d_fract(x: f32) -> f32 { return x - floor(x); }
fn d_b2(x: f32, y: f32) -> f32 { let fx = floor(x); let fy = floor(y); return d_fract(fx * 0.5 + fy * fy * 0.75); }
fn d_b4(x: f32, y: f32) -> f32 { return d_b2(0.5 * x, 0.5 * y) * 0.25 + d_b2(x, y); }
fn d_bayer(x: f32, y: f32) -> f32 { return d_b4(0.5 * x, 0.5 * y) * 0.25 + d_b2(x, y); }
fn d_hash(px: f32, py: f32) -> f32 {
	var x = d_fract(px * 123.34);
	var y = d_fract(py * 456.21);
	let d = x * (x + 45.32) + y * (y + 45.32);
	x = x + d; y = y + d;
	return d_fract(x * y);
}
fn d_noise(px: f32, py: f32) -> f32 {
	let ix = floor(px); let iy = floor(py);
	let fx = px - ix; let fy = py - iy;
	let ux = fx * fx * (3.0 - 2.0 * fx); let uy = fy * fy * (3.0 - 2.0 * fy);
	let a = d_hash(ix, iy); let b = d_hash(ix + 1.0, iy);
	let c = d_hash(ix, iy + 1.0); let e = d_hash(ix + 1.0, iy + 1.0);
	return a + (b - a) * ux + (c + (e - c) * ux - (a + (b - a) * ux)) * uy;
}
fn d_fbm(px: f32, py: f32) -> f32 {
	var v = 0.0; var amp = 0.5; var x = px; var y = py;
	for (var i = 0; i < 5; i = i + 1) {
		v = v + amp * d_noise(x, y);
		x = x * 2.03 + 1.7; y = y * 2.03 + 9.2; amp = amp * 0.5;
	}
	return v;
}
fn d_smooth(a: f32, b: f32, x: f32) -> f32 { let t = clamp((x - a) / (b - a), 0.0, 1.0); return t * t * (3.0 - 2.0 * t); }
fn fill(input: FillInput) -> vec4<f32> {
	let ink = input.params[0];
	let mode = input.params[1].x; let pixel = input.params[1].y;
	let angle = radians(input.params[1].z); let radius = input.params[1].w;
	let cx = input.params[2].x; let cy = input.params[2].y;
	let v_from = input.params[2].z; let v_to = input.params[2].w;
	let curve = input.params[3].x; let noise = input.params[3].y;
	let scale = input.params[3].z; let soft = input.params[3].w;
	// The box in the lab's CSS px (drawn at 1.5x), cut into whole cells.
	let d = input.size.x / ZOOM; let h = input.size.y / ZOOM;
	let cols = max(1.0, ceil(d / pixel)); let rows = max(1.0, ceil(h / pixel));
	let gx = floor(input.p.x / input.size.x * cols); let gy = floor(input.p.y / input.size.y * rows);
	let t = (gx + 0.5) * pixel / d; let dd = (gy + 0.5) * pixel / h; let m = d / h;
	// Modes 0 and 3 carry their seed and its speed where 1 and 2 keep a centre.
	let seed = select(0.0, cx + input.time * cy, mode < 0.5 || mode > 2.5);
	var v = 0.0;
	if (mode < 0.5) {
		let s = clamp((t - 0.5) * sin(angle) + (dd - 0.5) * cos(angle) + 0.5, 0.0, 1.0);
		v = v_from + (v_to - v_from) * pow(s, curve);
	} else if (mode < 1.5) {
		let s = length(vec2<f32>((t - cx) * m, dd - cy)) / radius;
		v = v_from + (v_to - v_from) * pow(min(1.0, s), curve);
	} else if (mode < 2.5) {
		let sx = (t - cx) * m / radius; let sy = (dd - cy) / radius; let q = sx * sx + sy * sy;
		if (q < 1.0) {
			let l = vec3<f32>(cos(angle), sin(angle), 0.9);
			let lam = max(0.0, dot(vec3<f32>(sx, sy, sqrt(1.0 - q)), l) / length(l));
			v = 0.03 + (v_to - 0.03) * pow(lam, curve);
		} else {
			v = v_from;
		}
	} else {
		let n = d_fbm(t * m * scale + seed, dd * scale + seed);
		v = v_from + (v_to - v_from) * pow(d_smooth(0.25, 0.75, n), curve);
	}
	if (noise > 0.0 && mode < 2.5) {
		v = v + (d_fbm(t * m * scale + seed, dd * scale + seed) - 0.5) * noise * d_smooth(0.0, 0.25, v);
	}
	let threshold = d_bayer(gx, gy) + 0.5 / 64.0;
	let n = clamp(v, 0.0, 1.0);
	var on = select(0.0, 1.0, threshold <= n);
	if (soft > 0.0) { on = clamp((n - threshold) / soft + 0.5, 0.0, 1.0); }
	return vec4<f32>(ink.rgb, ink.a * on);
}
]]
local DITHER_SOURCE = "const ZOOM: f32 = " .. string.format("%.4f", Z) .. ";\n" .. DITHER_WGSL
local DITHER = ui.shader(DITHER_SOURCE)
local DITHER_DRIFT = ui.shader(DITHER_SOURCE, { fps = 30 })
-- The 16 numbers the shader reads (see its first lines).
local function dither_params(u)
	local slot_a, slot_b = u.cx, u.cy
	if u.mode < 0.5 or u.mode > 2.5 then
		slot_a, slot_b = u.seed, u.speed or 0
	end
	return {
		u.ink,
		{ u.mode, u.pixel, u.angle, u.radius },
		{ slot_a, slot_b, u.from, u.to },
		{ u.curve, u.noise, u.scale, u.soft },
	}
end

-- A drawn dither's numbers changed (a new state, the upload's progress):
-- the shader reads them at once, as the lab redraws its canvas.
local function set_dither(el, u)
	if not el or not el.mounted then
		return
	end
	local values, i = {}, 0
	for _, v in dither_params(u) do
		local list = v
		if type(v) == "string" then
			local hex = v:gsub("#", "")
			list = {}
			for k = 0, 3 do
				list[k + 1] = (tonumber(hex:sub(k * 2 + 1, k * 2 + 2), 16) or 255) / 255
			end
		end
		for k = 1, 4 do
			values["shaderParam" .. i] = list[k]
			i += 1
		end
	end
	anime.utils.set(el, values)
end

-- The lab's pressable parts (its `.cmp-hit`: frames of these names, at
-- most 60 tall and 380 wide). Hovered they brighten to 1.18; pressed they
-- squish to .94 and dim to .92 in 80 ms; released they spring back (the
-- lab's press spring, stiffness 520, damping 30) and their brightness eases
-- back over 120 ms.
local HIT = {
	"Add",
	"Send",
	"Voice input",
	"Tools",
	"Model picker",
	"Approve",
	"Deny",
	"Retry",
	"Take over",
	"Show tab",
	"Try ",
	"Always allow",
	"Choice ·",
	"Remove",
	"Edit",
	"Collapse",
	"Expand",
	"More",
	"Photos",
	"Link",
	"PDF",
	"Tab",
	"Image",
	"Document",
	"Spreadsheet",
	"Text & code",
	"Queued message",
	"Option",
	"Item",
	"Row",
	"Skip",
	"Submit",
	"Stop",
	"Open",
	"View",
	"Dismiss",
	"Cancel",
	"Undo",
	"Minimi",
	"Restore",
	"Mention",
	"Command",
	"Result",
}
local function hit_target(n)
	if n.type ~= "frame" or n.bh <= 0 or n.bh > 60 or n.bw > 380 then
		return false
	end
	local name = (n.name or ""):lower()
	for _, prefix in HIT do
		if name:sub(1, #prefix) == prefix:lower() then
			return true
		end
	end
	return false
end
local function spring_k(k, c)
	local x, v, t, still, samples = 0, 0, 0, 0, {}
	while t < 2 do
		v += (-k * (x - 1) - c * v) * 0.001
		x += v * 0.001
		t += 0.001
		table.insert(samples, x)
		if math.abs(x - 1) < 1e-3 and math.abs(v) < 0.01 then
			still += 1
			if still > 30 then
				break
			end
		else
			still = 0
		end
	end
	local points = {}
	for i = 0, 48 do
		points[i + 1] = string.format("%.4f", samples[math.floor(i / 48 * (#samples - 1) + 0.5) + 1])
	end
	points[49] = "1"
	return "linear(" .. table.concat(points, ",") .. ")", #samples
end
local PRESS_SPRING, PRESS_DUR = spring_k(520, 30)
local EASE_OUT_UI = "cubicBezier(.2,0,0,1)" -- the lab's --ease-out
function press_feedback(inst, props)
	-- (The expanded photo row brightens nothing as a whole, as in the lab.)
	if (inst.spec.name or ""):match("^Photos · expanded") then
		return
	end
	local hovered, pressed = false, false
	local function el()
		local mounted = inst.el
		if mounted and mounted.mounted then
			return mounted
		end
		return nil
	end
	props.on_hover = function(inside)
		hovered = inside
		local e = el()
		if e and not pressed then
			anime.animate(e, { brightness = inside and 1.18 or 1, duration = 120, ease = EASE_OUT })
		end
	end
	props.on_press = function()
		pressed = true
		local e = el()
		if not e then
			return
		end
		anime.animate(e, { scale = 0.94, duration = 80, ease = EASE_OUT_UI })
		anime.animate(e, { brightness = 0.92, duration = 80, ease = EASE_OUT })
	end
	props.on_release = function()
		if not pressed then
			return
		end
		pressed = false
		local e = el()
		if not e then
			return
		end
		anime.animate(e, { scale = 1, duration = PRESS_DUR, ease = PRESS_SPRING })
		anime.animate(e, { brightness = hovered and 1.18 or 1, duration = 120, ease = EASE_OUT })
	end
end

local function frame_view(inst)
	inst.rev = ui.use_state(0)
	inst.list = ui.use_state(ids(inst))
	local function s()
		inst.rev:get()
		return inst.spec
	end
	local n = inst.spec
	local props = placement(inst, {})
	props.on_mount = mounted(inst)
	props.id = inst.dom_id or (n.name == "Input row" and "input-row") or (n.name == "Send" and "send") or nil
	if n.on_resize then
		props.on_resize = function(size)
			n.on_resize(size)
		end
	end
	if clickable(inst) then
		props.cursor = "pointer"
		props.on_click = function()
			click(inst)
		end
	end
	if hit_target(n) then
		press_feedback(inst, props)
	end
	-- Paint.
	props.bg = function()
		return s().fill
	end
	props.gradient = function()
		local g = s().gradient
		return g and { angle = g.angle, stops = g.stops } or nil
	end
	props.corners = corners_prop(inst)
	props.shadow = function()
		local list = s().shadows
		if not list then
			return nil
		end
		local out = {}
		for _, sh in list do
			if not sh.inner then
				table.insert(
					out,
					{ x = z(sh.x), y = z(sh.y), blur = z(sh.blur), spread = z(sh.spread), color = sh.color }
				)
			end
		end
		return #out > 0 and out or nil
	end
	props.overflow = function()
		return s().clip and "hidden" or nil
	end
	-- Padding. The attachment row scrolls in the lab, with room around it
	-- for chips to lift: its clip reaches 10 px above and below and 8 px
	-- left of the row.
	local att = n.name == "Attachments"
	local function p(i)
		local m = s()
		local t, r, b, l = pad(m)
		local v = ({ t, r, b, l })[i]
		if att then
			v += ({ 10, 0, 10, 8 })[i]
		end
		return z(v)
	end
	props.pt = function()
		return p(1)
	end
	props.pr = function()
		return p(2)
	end
	props.pb = function()
		return p(3)
	end
	props.pl = function()
		return p(4)
	end
	if att then
		props.overflow = "hidden"
		props.my = z(-10)
		props.ml = z(-8)
		local w = props.w
		props.w = function()
			local v = w()
			if type(v) == "number" then
				return v + z(8)
			end
			return v
		end
	end
	if n.dither then
		-- The lab's dither fill, drawn by its shader (drifting ones at 30 fps).
		props.bg_shader = n.dither.speed and DITHER_DRIFT or DITHER
		props.shader_params = dither_params(n.dither)
		if n.dither.speed then
			local mount = props.on_mount
			props.on_mount = function(el)
				mount(el)
				anime.animate(
					el,
					{ shaderTime = { 0, 3600 }, duration = 3600000, ease = "linear", loop = true, frameRate = 30 }
				)
			end
		end
	end
	if n.img then
		-- An image fill (CSS background-image: cover), cut by the corners.
		props.bg_image = function()
			return s().img
		end
		props.bg_fit = n.fit == "cover" and "cover" or "fill"
	end
	-- The children, laid out as the frame says.
	local layout = layout_of(n)
	table.insert(
		props,
		ui.each(inst.list, function(id)
			return view(inst.by[id])
		end, {
			key = function(id)
				return id
			end,
			direction = function()
				local l = layout_of(s())
				return l == "vertical" and "column" or "row"
			end,
			gap_x = function()
				return z(s().gap or 0)
			end,
			gap_y = function()
				local m = s()
				return z((m.wrap and m.rowGap) or m.gap or 0)
			end,
			flex_wrap = function()
				return s().wrap or false
			end,
			items = function()
				return ALIGN[s().alignItems] or "start"
			end,
			justify = function()
				return ALIGN[s().justifyContent] or "start"
			end,
			grow = 1,
			relative = layout == "none" or nil,
		})
	)
	inset_stroke(inst, props)
	local stroke = stroke_layer(inst)
	if stroke then
		table.insert(props, stroke)
	end
	if att then
		-- The lab masks its scroller's right edge over 28 px while more
		-- chips lie beyond it.
		props.fade_x = function()
			return overflows(s()) and { 0, z(28) } or { 0, 0 }
		end
	end
	return ui.column(entering_props(inst, props))
end

function view(inst)
	local t = inst.spec.type
	if t == "text" then
		return text_view(inst)
	end
	if t == "icon" then
		return icon_view(inst)
	end
	return frame_view(inst)
end

----------------------------------------------------------------------------
-- Moving between states (the lab's `Ft`, `_d` and `kn`).
----------------------------------------------------------------------------

-- Opacity keyframes over a whole transition, eased as one (WAAPI applies
-- the easing to the iteration, not to each keyframe).
local function fade(el, from, mid, at, to, duration)
	anime.animate(el, {
		opacity = { { from = from, to = mid, duration = duration * at }, { to = to, duration = duration * (1 - at) } },
		ease = "linear",
		playbackEase = EASE_OUT,
		duration = duration,
	})
end

-- The space a node closes to: its size and the gap beside it, as a
-- negative margin (the lab's `kn`).
local function closed_margin(inst, size)
	local parent = inst.parent and inst.parent.spec or {}
	local siblings = 0
	if inst.parent then
		for _, c in inst.parent.kids do
			if c ~= inst then
				siblings += 1
			end
		end
	end
	return -(size + (inst.mt or 0)) - (siblings > 0 and (parent.gap or 0) or 0)
end

function enter(inst, el)
	local n, mode = inst.spec, inst.mode
	local params = { duration = DUR, ease = SPRING }
	if mode == "vertical" then
		params.mb = { inst.closed + own_mb(inst), own_mb(inst) }
		params.y = { z(6), 0 }
		params.scale = { 0.98, 1 }
	elseif mode == "horizontal" then
		params.mr = { inst.closed, 0 }
		params.scale = { 0.88, 1 }
		-- Its height opens too, then is its own again.
		params.h = { 0, z(n.bh) }
		params.onComplete = function()
			anime.utils.cleanInlineStyles(el, "h")
		end
	else
		params.y = { z(6), 0 }
		params.scale = { 0.97, 1 }
	end
	clip_params(params, inst.clip_from or clip_closed(mode, z(n.bw), z(n.bh)), clip_open())
	local done = params.onComplete
	params.onComplete = function(a)
		if done then
			done(a)
		end
		-- Open, it no longer clips (the lab's `fill: none`).
		inst.clip_from = nil
		bump(inst)
		anime.utils.cleanInlineStyles(el, CLIP_KEYS)
	end
	anime.animate(el, params)
	fade(el, 0, 0, 0.15, n.opacity or 1, DUR)
end

local function remove_kid(parent, inst)
	for i, c in parent.kids do
		if c == inst then
			table.remove(parent.kids, i)
			break
		end
	end
	parent.by[inst.id] = nil
	if parent.list then
		parent.list:set(ids(parent))
	end
end

local function exit(inst, mode)
	inst.exiting = true
	local el = inst.el
	if not el or not el.mounted then
		remove_kid(inst.parent, inst)
		return
	end
	local w, h = el.width, el.height
	if mode == "none" and inst.parent.el then
		-- Out of the flow, where it was.
		local p = inst.parent
		local _, _, _, pl = pad(p.spec)
		local pt = pad(p.spec)
		inst.abs = { x = el.x - p.el.x - z(pl), y = el.y - p.el.y - z(pt) }
		inst.frozen = { w = w, h = h }
	elseif (mode == "vertical" and fills(inst.spec.height)) or (mode == "horizontal" and fills(inst.spec.width)) then
		inst.frozen = { w = w, h = h }
	end
	bump(inst)
	local params = { duration = DUR, ease = SPRING }
	if mode == "vertical" then
		-- Close the space it takes (its box, less its own top margin) and the
		-- gap after it.
		params.mb = z(closed_margin(inst, (h + own_mb(inst)) / Z))
		params.y = z(6)
		params.scale = 0.98
	elseif mode == "horizontal" then
		params.mr = z(closed_margin(inst, w / Z))
		params.scale = 0.88
		params.h = { h, 0 }
	else
		params.y = z(6)
		params.scale = 0.97
	end
	clip_params(params, clip_open(), clip_closed(mode, w, h))
	anime.animate(el, params)
	fade(el, 1, 0, 0.6, 0, DUR)
	after(DUR, function()
		remove_kid(inst.parent, inst)
	end)
end

-- A matched node's numbers ease with the spring and its colours over 200 ms,
-- as the lab's CSS transitions do.
local function ease_style(inst, old, n)
	local el = inst.el
	if not el or not el.mounted then
		return
	end
	local spring = { duration = DUR, ease = SPRING }
	local any = false
	local function num(key, a, b)
		if type(a) == "number" and type(b) == "number" and math.abs(a - b) > 0.01 then
			spring[key] = { a, b }
			any = true
		end
	end
	if n.type ~= "icon" then
		num("w", type(old.width) == "number" and z(old.width), type(n.width) == "number" and z(n.width))
		if not (n.layout == "vertical" and hugs(n)) and not (old.layout == "vertical" and hugs(old)) then
			num("h", type(old.height) == "number" and z(old.height), type(n.height) == "number" and z(n.height))
		end
	end
	if old.layoutPosition == "absolute" or inst.po == "none" then
		num("left", old.x and z(old.x), n.x and z(n.x))
		num("top", old.y and z(old.y), n.y and z(n.y))
	end
	if n.type == "frame" or n.type == "rectangle" then
		local a, b = { pad(old) }, { pad(n) }
		for i, key in { "pt", "pr", "pb", "pl" } do
			num(key, z(a[i]), z(b[i]))
		end
		local ca, cb = corner_list(old), corner_list(n)
		if ca[1] ~= cb[1] or ca[2] ~= cb[2] or ca[3] ~= cb[3] or ca[4] ~= cb[4] then
			local radii = { duration = DUR, ease = SPRING }
			for i, key in { "rounded_tl", "rounded_tr", "rounded_br", "rounded_bl" } do
				radii[key] = { z(ca[i]), z(cb[i]) }
			end
			anime.animate(el, table.clone(radii))
			if inst.stroke_el then
				anime.animate(inst.stroke_el, table.clone(radii))
			end
		end
	end
	num("rotate", old.rotation and -old.rotation or 0, n.rotation and -n.rotation or 0)
	-- Opacity eases over 200 ms (CSS `opacity .2s ease-out`).
	if (old.opacity or 1) ~= (n.opacity or 1) and not inst.entering then
		anime.animate(el, { opacity = { old.opacity or 1, n.opacity or 1 }, duration = 200, ease = EASE_OUT })
	end
	-- (The eased values stay as the element's own: they end on the new ones.)
	if any then
		anime.animate(el, spring)
	end
	if n.dither and old.dither and n.dither ~= old.dither then
		set_dither(el, n.dither)
	end
	-- Colours.
	if n.type == "icon" then
		if old.fill and n.fill and old.fill ~= n.fill and inst.svg_el then
			anime.animate(inst.svg_el, { tint = { old.fill, n.fill }, duration = 200, ease = EASE_OUT })
		end
	elseif n.type == "text" then
		if old.fill and n.fill and old.fill ~= n.fill and inst.text_el then
			anime.animate(inst.text_el, { color = { old.fill, n.fill }, duration = 200, ease = EASE_OUT })
		end
	elseif n.type ~= "icon" then
		if old.fill and n.fill and old.fill ~= n.fill then
			anime.animate(el, { bg = { old.fill, n.fill }, duration = 200, ease = EASE_OUT })
		end
		if old.stroke and n.stroke and old.stroke ~= n.stroke then
			anime.animate(
				inst.stroke_el or el,
				{ border_color = { old.stroke, n.stroke }, duration = 200, ease = EASE_OUT }
			)
		end
	end
end

-- Changed text crossfades: the old copy fades out on top while the new
-- fades in, and a single line eases to its new width.
local function crossfade(inst, old, n)
	local el = inst.el
	if not el or not el.mounted then
		return
	end
	inst.ghost_w = el.width
	inst.ghost_color = old.fill
	inst.ghost:set(old.content or "")
	if inst.ghost_el then
		anime.animate(inst.ghost_el, {
			opacity = { 1, 0 },
			duration = FADE * 0.6,
			ease = EASE_OUT,
			onComplete = function()
				inst.ghost:set("")
			end,
		})
	end
	if inst.text_el then
		fade(inst.text_el, 0, 0, 0.2, 1, FADE * 1.3)
	end
end

local update
-- The children: match by type and name, update the matched, bring in the
-- new, and let the rest close where they stand (the lab's `_d`).
local function diff(inst, n, snap)
	local layout = layout_of(n)
	local live = {}
	for _, c in inst.kids do
		if not c.exiting then
			table.insert(live, c)
		end
	end
	local old_specs = {}
	for i, c in live do
		old_specs[i] = c.spec
	end
	local old_keys, news = match_keys(old_specs), n.children or {}
	local new_keys = match_keys(news)
	local used, order, entered = {}, {}, {}
	for i, child in news do
		local found
		for j, k in old_keys do
			if k == new_keys[i] and not used[j] then
				found = j
				break
			end
		end
		local c
		if found then
			used[found] = true
			c = live[found]
			c.po = layout
			update(c, child)
		else
			c = instance(child, layout, inst)
			table.insert(entered, c)
		end
		c.mt = stacked(n, news, i)
		order[i] = c
	end
	-- Order as the lab's DOM ends up: the new order is laid over the old
	-- children with leaving ones skipped (each node goes after the one before
	-- it), then each leaving child moves before the next sibling that stays.
	local stays = {}
	for _, o in order do
		stays[o] = true
	end
	local leaving = {}
	for _, c in inst.kids do
		if not stays[c] and not c.exiting then
			table.insert(leaving, c)
		end
	end
	local gone = {}
	for _, c in inst.kids do
		if not stays[c] then
			gone[c] = true
		end
	end
	local final = table.clone(inst.kids)
	local function index(x)
		return table.find(final, x)
	end
	local before = nil
	for _, f in order do
		local at = before and index(before) + 1 or 1
		while final[at] and gone[final[at]] do
			at += 1
		end
		if final[at] ~= f then
			local was = index(f)
			if was then
				table.remove(final, was)
				if was < at then
					at -= 1
				end
			end
			table.insert(final, at, f)
		end
		before = f
	end
	for p, c in live do
		if not stays[c] then
			local anchor
			for q = p + 1, #live do
				if stays[live[q]] then
					anchor = live[q]
					break
				end
			end
			if anchor then
				table.remove(final, index(c))
				table.insert(final, index(anchor), c)
			end
		end
	end
	inst.kids = final
	inst.by = {}
	for _, c in final do
		inst.by[c.id] = c
	end
	for _, c in entered do
		c.entering = not c.spec.instant
		c.mode = snap and "none" or layout
		local size = c.mode == "vertical" and c.spec.bh or c.spec.bw
		c.closed = (c.mode == "none") and 0 or z(closed_margin(c, size))
	end
	for _, c in leaving do
		exit(c, snap and "none" or layout)
	end
	if inst.list then
		inst.list:set(ids(inst))
	end
end

function update(inst, n)
	local old = inst.spec
	inst.spec = n
	if n.type == "text" then
		if old.content ~= n.content and n.name ~= "Reply placeholder" then
			crossfade(inst, old, n)
		end
		ease_style(inst, old, n)
		bump(inst)
		return
	end
	if n.type ~= "icon" then
		diff(inst, n, layout_of(old) ~= layout_of(n))
	end
	ease_style(inst, old, n)
	bump(inst)
end

----------------------------------------------------------------------------
-- The base composer, as the lab's design nodes: the input box (add, the
-- reply field, voice, send or stop) over its footer (tools, model picker).
-- `Composer.base { … }` builds it; `view(instance(node))` draws it, and
-- `update(inst, node)` moves a drawn one to a new node with the lab's
-- transitions. Docks (`docks`) attach above the input box, attachment rows
-- (`attachments`) above the reply field. The numbers are the lab's; the
-- check below holds them to the lab's own Empty and Typing states.
----------------------------------------------------------------------------

local Composer = {}
local function icon(name, glyph, side, fill)
	return { type = "icon", name = name, width = side, height = side, icon = glyph, bw = side, bh = side, fill = fill }
end
local function round_button(name, fill, child)
	return {
		type = "frame",
		name = name,
		width = 32,
		height = 32,
		cornerRadius = 16,
		justifyContent = "center",
		alignItems = "center",
		bw = 32,
		bh = 32,
		fill = fill,
		children = { child },
	}
end
function Composer.add()
	return round_button("Add", "#2A2926", icon("Add icon", "plus", 16, "#C9C6BE"))
end
function Composer.voice()
	return round_button("Voice input", nil, icon("Microphone", "mic", 15, "#A3A199"))
end
-- Send, or Stop while a run works.
function Composer.send(stop)
	return round_button("Send", "#EDEBE5", stop and {
		type = "rectangle",
		name = "Stop glyph",
		width = 10,
		height = 10,
		cornerRadius = 2,
		bw = 10,
		bh = 10,
		fill = "#121211",
	} or icon("Send icon", "arrow-up", 16, "#121211"))
end
-- The reply field: a placeholder, or what was typed.
function Composer.reply(content, typed)
	return {
		type = "text",
		name = "Reply placeholder",
		width = "fill_container",
		content = content or "Ask anything, @ for tabs, / for commands",
		fontFamily = "Geist",
		fontSize = 13.5,
		fontWeight = "normal",
		lineHeight = 1.4,
		textGrowth = "fixed-width",
		bw = 264,
		bh = 19,
		fill = typed and "#EDEBE5" or "#57554F",
	}
end
function Composer.input_row(o)
	return {
		type = "frame",
		name = "Input row",
		width = "fill_container",
		gap = 8,
		alignItems = "center",
		bw = 384,
		bh = 32,
		children = { Composer.add(), Composer.reply(o.reply, o.typed), Composer.voice(), Composer.send(o.stop) },
	}
end
function Composer.input_box(o)
	local children = {}
	if o.attachments then
		table.insert(children, o.attachments)
	end
	table.insert(children, Composer.input_row(o))
	return {
		type = "frame",
		name = "Input box",
		width = "fill_container",
		layout = "vertical",
		gap = 8,
		padding = 6,
		cornerRadius = o.docks and { 0, 0, 16, 16 } or 22,
		bw = 396,
		bh = 44,
		stroke = "#2E2D29",
		strokeWidth = 1,
		fill = "#1C1C1A",
		children = children,
	}
end
function Composer.footer(o)
	return {
		type = "frame",
		name = "Composer footer",
		width = "fill_container",
		padding = { 0, 4 },
		alignItems = "center",
		bw = 396,
		bh = 26,
		children = {
			{
				type = "frame",
				name = "Tools",
				height = 26,
				gap = 4,
				padding = { 0, 8 },
				cornerRadius = 13,
				alignItems = "center",
				bw = 45,
				bh = 26,
				children = {
					icon("Tools icon", "briefcase-business", 14, "#A3A199"),
					icon("Tools menu", "chevron-down", 11, "#6E6C65"),
				},
			},
			{ type = "frame", name = "Spacer", width = "fill_container", height = 1, bw = 213, bh = 1 },
			{
				type = "frame",
				name = "Model picker",
				height = 26,
				gap = 6,
				padding = { 0, 8 },
				cornerRadius = 13,
				alignItems = "center",
				bw = 130,
				bh = 26,
				children = {
					-- The lab's dithered orb (its bayer-dither shader, sphere mode).
					{
						type = "ellipse",
						name = "Model orb",
						width = 13,
						height = 13,
						bw = 13,
						bh = 13,
						fill = "#1F1E1B",
						dither = {
							mode = 2,
							pixel = 1,
							angle = 225,
							cx = 0.5,
							cy = 0.5,
							radius = 0.5,
							from = 0,
							to = 1,
							curve = 1.1,
							noise = 0,
							scale = 4,
							seed = 0,
							soft = 0,
							ink = "#EDEBE5FF",
						},
					},
					{
						type = "text",
						name = "Selected model",
						content = o.model or "Grok 4.7",
						fontFamily = "Geist",
						fontSize = 12,
						fontWeight = "500",
						bw = 46,
						bh = 16,
						fill = "#A3A199",
					},
					{
						type = "text",
						name = "Effort",
						content = o.effort or "High",
						fontFamily = "Geist",
						fontSize = 12,
						fontWeight = "normal",
						bw = 26,
						bh = 16,
						fill = "#6E6C65",
					},
					icon("Choose model", "chevron-down", 11, "#6E6C65"),
				},
			},
		},
	}
end
-- { reply, typed, stop, attachments, docks, model, effort }
function Composer.base(o)
	o = o or {}
	local body = {}
	for _, dock in o.docks or {} do
		table.insert(body, dock)
	end
	table.insert(body, Composer.input_box(o))
	return {
		type = "frame",
		name = "Composer",
		width = 396,
		layout = "vertical",
		gap = 6,
		bw = 396,
		bh = 76,
		children = {
			{
				type = "frame",
				name = "Composer body · Attached docks",
				width = "fill_container",
				layout = "vertical",
				bw = 396,
				bh = 44,
				children = body,
			},
			Composer.footer(o),
		},
	}
end
do
	local function same(a, b, path)
		if type(a) ~= "table" or type(b) ~= "table" then
			assert(
				a == b,
				"Composer.base differs from the lab at " .. path .. ": " .. tostring(a) .. " ≠ " .. tostring(b)
			)
			return
		end
		for k, v in a do
			same(v, b[k], path .. "." .. tostring(k))
		end
		for k, v in b do
			same(a[k], v, path .. "." .. tostring(k))
		end
	end
	local function state(name)
		for _, st in STATES do
			if st.name == name then
				return st.composer
			end
		end
		return nil
	end
	same(Composer.base(), state("Composer · Empty"), "Empty")
	same(Composer.base { reply = "Summarize this page", typed = true }, state("Composer · Typing"), "Typing")
end

----------------------------------------------------------------------------
-- The page: the composer on the stage, the popovers above it, and the
-- lab's controls.
----------------------------------------------------------------------------

local current = ui.state("current", 2) -- "Composer · Empty", as the lab opens
local autoplay = ui.state("autoplay", false)
local slow = ui.state("slow", 1)
anime.engine.speed = 1 / slow:peek()
if current:peek() > #STATES then
	current:set(2)
end
do
	local start = ui.shared("composer_state", 0):peek()
	if start > 0 then
		current:set(start)
	end
	if type(GOTO) == "number" then
		current:set(GOTO)
	end
	if type(GOTO) == "table" then
		current:set(GOTO[1])
	end
end

local root = instance(STATES[current:peek()].composer, nil, nil)
root.dom_id = "composer"

-- Popovers drawn above the composer (the lab's `cmp-over`): a new set
-- rises in over 200 ms, the old fades out over 100 ms.
local over_serial = 0
local over_groups = {}
local over_list = ui.state("over_list", {})
over_list:set {} -- this load's groups only (a reload starts the list over)
local function overlay_key(st)
	local names = {}
	for _, o in st.overlays do
		table.insert(names, o.node.name)
	end
	return table.concat(names, "|")
end
local function show_overlays(st, instant)
	local key = overlay_key(st)
	local live
	for _, g in over_groups do
		if not g.gone then
			live = g
		end
	end
	if live and live.key == key then
		return
	end
	if live then
		live.gone = true
		if live.el then
			anime.animate(live.el, { opacity = 0, duration = 100, ease = EASE_EXIT })
		end
		after(110, function()
			for i, g in over_groups do
				if g == live then
					table.remove(over_groups, i)
					break
				end
			end
			local list = {}
			for _, g in over_groups do
				table.insert(list, g.id)
			end
			over_list:set(list)
		end)
	end
	if #st.overlays > 0 then
		over_serial += 1
		table.insert(over_groups, { id = "o" .. over_serial, key = key, items = st.overlays, instant = instant })
	end
	local list = {}
	for _, g in over_groups do
		table.insert(list, g.id)
	end
	over_list:set(list)
end
local function overlay_view(id)
	local group
	for _, g in over_groups do
		if g.id == id then
			group = g
		end
	end
	if not group then
		return ui.div {}
	end
	local props = {
		absolute = true,
		left = 0,
		bottom = "100%",
		pb = z(10),
		gap = z(8),
		items = "start",
		origin = { 0, 1 },
		on_mount = function(el)
			group.el = el
			if not group.instant then
				anime.animate(
					el,
					{ opacity = { 0, 1 }, y = { z(4), 0 }, scale = { 0.97, 1 }, duration = 200, ease = EASE_ENTER }
				)
			end
		end,
	}
	if not group.instant then
		props.opacity = 0
	end
	for _, o in group.items do
		local inst = instance(o.node, nil, nil)
		local node = ui.component(function()
			return view(inst)
		end)()
		if o.ml then
			node = ui.row { ml = z(o.ml), node }
		end
		table.insert(props, node)
	end
	return ui.column(props)
end

local uploads -- (forward) the live upload test's chips
local function go(i)
	i = (i - 1) % #STATES + 1
	if i == current:peek() then
		return
	end
	uploads.clear()
	ui.batch(function()
		current:set(i)
		update(root, STATES[i].composer)
		show_overlays(STATES[i], false)
	end)
end

-- The lab's "Upload test" (its live attachment row). Each press adds the
-- next file of its list as an uploading chip, entering like any new node,
-- counting "Uploading · N%" up while a dither wash fills it, then settling on
-- its type and size. As the row fills it behaves as the lab's does: two or
-- more photos gather into a photo stack and three or more files into a file
-- stack (the loose chips close as the stack takes them, with a pulse), a row
-- that would wrap goes compact (26 px chips, stacks at 65%) and wraps, and
-- one that would need a third row collapses to the one-line summary.
-- Layouts measured out of sight (`MEASURE(spec, …)`): each spec is drawn,
-- invisible, at its own width, and reports its size through its
-- `on_resize`; a new call replaces the last.
local measure_list = ui.state("measure_list", {})
measure_list:set {}
local measure_specs, measure_serial = {}, 0
local function MEASURE(...)
	measure_specs = {}
	local ids = {}
	for _, spec in { ... } do
		measure_serial += 1
		local id = "m" .. measure_serial
		measure_specs[id] = spec
		table.insert(ids, id)
	end
	measure_list:set(ids)
end

uploads = (function()
	local live, row, design, items, mode, n, timer = nil, nil, {}, {}, "normal", 0, nil
	local entries = {} -- every chip, grouped or not
	local function copy(t)
		local out = {}
		for k, v in t do
			out[k] = type(v) == "table" and copy(v) or v
		end
		return out
	end
	local function child(spec, name)
		if not spec then
			return nil
		end
		for _, c in spec.children or {} do
			if c.name == name then
				return c
			end
			local found = child(c, name)
			if found then
				return found
			end
		end
		return nil
	end
	local function find(inst, spec)
		if not spec then
			return nil
		end
		if inst.spec == spec then
			return inst
		end
		for _, c in inst.kids do
			local found = not c.exiting and find(c, spec)
			if found then
				return found
			end
		end
		return nil
	end
	-- A design node's value changed in place: no transition.
	local function poke(inst, fields)
		if not inst then
			return
		end
		for k, v in fields do
			inst.spec[k] = v
		end
		bump(inst)
	end
	local function size(bytes)
		return bytes >= 1e6 and string.format("%.1f MB", bytes / 1e6) or (math.max(1, math.round(bytes / 1e3)) .. " KB")
	end
	-- The lab's type colours (`oo`).
	local TYPE =
		{ PDF = "#E8A898", XLSX = "#C3CEB2", XLS = "#C3CEB2", CSV = "#C3CEB2", DOCX = "#A9BABF", DOC = "#A9BABF" }
	-- CSS zoom: every length of a node, scaled.
	local LENGTHS =
		{ "width", "height", "bw", "bh", "x", "y", "gap", "fontSize", "letterSpacing", "strokeWidth", "minWidth" }
	local function zoom(node, k)
		for _, key in LENGTHS do
			if type(node[key]) == "number" then
				node[key] *= k
			end
		end
		for _, key in { "padding", "cornerRadius" } do
			local v = node[key]
			if type(v) == "number" then
				node[key] = v * k
			elseif type(v) == "table" then
				local out = {}
				for i, x in v do
					out[i] = x * k
				end
				node[key] = out
			end
		end
		for _, c in node.children or {} do
			zoom(c, k)
		end
		return node
	end

	-- A chip's design node, normal or compact (the lab's `.live-row.compact`
	-- rules: 26 px tall, an 18 px thumb, the name alone at 11.5 px).
	local function chip_spec(c, compact)
		local key = compact and "compact" or "normal"
		if c.specs[key] and c.specs[key].minWidth == c.minWidth then
			return c.specs[key]
		end
		local spec = copy(c.base)
		spec.minWidth = c.minWidth
		if c.finish then
			child(spec, "Attachment detail").content = c.ext .. " · " .. size(c.file.size)
		end
		if c.done then
			child(spec, "Upload progress wash").opacity = 0
		end
		if compact then
			spec.height, spec.bh, spec.gap, spec.padding, spec.cornerRadius = 26, 26, 6, { 0, 9, 0, 4 }, 8
			local thumb, wash = child(spec, "Attachment thumb"), child(spec, "Upload progress wash")
			thumb.width, thumb.height, thumb.bw, thumb.bh, thumb.cornerRadius = 18, 18, 18, 18, 5
			for _, g in thumb.children or {} do
				g.width, g.height = 11, 11
			end
			wash.height, wash.bh, wash.cornerRadius = 26, 26, { 8, 0, 0, 8 }
			child(spec, "Attachment meta").hidden = true
			child(spec, "Attachment name").fontSize = 11.5
		end
		c.specs[key] = spec
		return spec
	end
	local function uploading(g)
		local count, sum = 0, 0
		for _, c in g.items do
			if not c.done then
				count += 1
				sum += c.p or 0
			end
		end
		return count, count > 0 and sum / count or 1
	end
	-- A stack's design node (the lab's `Be`): the photo stack shows the last
	-- three photos and the count; the file stack "N files" and its types.
	-- While items upload, a 2 px bar under it shows their average progress.
	local function group_spec(g, compact, measuring)
		local spec
		local count, progress = uploading(g)
		if g.type == "photos" then
			spec = copy(DATA.photo_stack)
			local shown = {}
			for i = #g.items, math.max(1, #g.items - 2), -1 do
				table.insert(shown, g.items[i])
			end
			local kept = {}
			for _, c in spec.children do
				local at = tonumber((c.name or ""):match("^Photo (%d)$"))
				if at then
					if shown[at] then
						c.img, c.fit = shown[at].file.photo, "cover"
						table.insert(kept, c)
					end
				else
					if c.name == "Count" then
						child(c, "Count label").content = tostring(#g.items)
					end
					table.insert(kept, c)
				end
			end
			spec.children = kept
			spec.name = "Photos"
		else
			spec = copy(DATA.file_stack)
			spec.name = "Files"
			child(spec, "Attachment name").content = #g.items .. " files"
			local meta = child(spec, "Attachment meta")
			local badge = meta.children[1]
			if count > 0 then
				meta.children = {
					{
						type = "text",
						name = "Upload status",
						content = "Uploading " .. count .. " · " .. math.round(progress * 100) .. "%",
						fontFamily = "Geist Mono",
						fontSize = 10,
						fontWeight = "normal",
						fill = "#A3A199",
						bw = 90,
						bh = 13,
					},
				}
			else
				local kinds, seen = {}, {}
				for _, c in g.items do
					if not seen[c.ext] then
						seen[c.ext] = true
						table.insert(kinds, c.ext)
					end
				end
				local out = {}
				for i = 1, math.min(2, #kinds) do
					local b = copy(badge)
					b.name, b.content, b.fill = "Type badge " .. kinds[i], kinds[i], TYPE[kinds[i]] or "#A3A199"
					table.insert(out, b)
				end
				local rest = 0
				for _, c in g.items do
					if c.ext ~= kinds[1] and c.ext ~= kinds[2] then
						rest += 1
					end
				end
				if rest > 0 then
					local b = copy(badge)
					b.name, b.content, b.fill = "Type badge +", "+" .. rest, "#6E6C65"
					table.insert(out, b)
				end
				meta.children = out
			end
		end
		-- The progress bar (the lab's `.live-prog`): fades once all are up.
		local track = {
			type = "frame",
			name = "Group progress",
			layoutPosition = "absolute",
			x = 6,
			y = spec.height + 3,
			width = spec.width - 12,
			height = 2,
			bw = spec.width - 12,
			bh = 2,
			cornerRadius = 1,
			fill = "#33322D",
			clip = true,
			opacity = count > 0 and 1 or 0,
			children = {
				{
					type = "frame",
					name = "Group progress fill",
					layoutPosition = "absolute",
					x = 0,
					y = 0,
					width = math.max(0, (spec.width - 12) * progress),
					height = 2,
					bw = 1,
					bh = 2,
					fill = "#EDEBE5",
				},
			},
		}
		if g.seen_progress or count > 0 then
			if not measuring then
				g.seen_progress = true
			end
			table.insert(spec.children, track)
		end
		if compact then
			zoom(spec, 0.65)
		end
		if not measuring then
			g.spec, g.fill = spec, track.children[1]
		end
		return spec
	end

	-- When the row wraps, measured as the lab does it: the row's chips laid
	-- out at the row's width, once as they are and once compact, out of sight
	-- (the lab's `.cmp-measure`); the real row follows from those heights,
	-- so it never decides from its own changing layout.
	local generation, measured, pending = 0, {}, nil
	local function measure_row(compact)
		local kids = {}
		for _, d in design do
			table.insert(kids, copy(d))
		end
		for _, e in items do
			table.insert(kids, copy(e.kind == "group" and group_spec(e, compact, true) or chip_spec(e.c, compact)))
		end
		local gen = generation
		return {
			type = "frame",
			name = compact and "Measure compact" or "Measure",
			width = 384,
			gap = 8,
			rowGap = 6,
			wrap = true,
			padding = { 0, 2, 0, 2 },
			alignItems = "center",
			bw = 384,
			bh = 40,
			children = kids,
			on_resize = function(size)
				-- (Before its first layout a node reports nothing.)
				if gen ~= generation or size.w <= 0 then
					return
				end
				measured[compact and "compact" or "normal"] = size.h / Z
				if measured.normal and measured.compact and pending then
					local go_on = pending
					pending = nil
					go_on()
				end
			end,
		}
	end

	local apply
	local function render(pulse)
		-- Measure first; the row changes once both layouts are known.
		generation += 1
		measured = {}
		local waiting = pending and pending.pulse or {}
		for _, g in pulse or {} do
			table.insert(waiting, g)
		end
		local go_on = function()
			apply(waiting)
		end
		pending = setmetatable({ pulse = waiting }, { __call = go_on })
		MEASURE(measure_row(false), measure_row(true))
	end
	-- How often the measured decision changed (a test holds it to one per
	-- change of the row).
	local decisions = ui.shared("composer_upload_decisions", 0)
	function apply(pulse)
		local T = os.clock()
		local before = mode
		mode = "normal"
		-- More than one row of full chips goes compact; more than two
		-- compact rows collapses to the summary.
		if measured.normal > 40 + 1 then
			mode = measured.compact > 26 * 2 + 6 + 1 and "summary" or "compact"
		end
		if mode ~= before then
			decisions:set(decisions:peek() + 1)
		end
		local compact = mode ~= "normal"
		local kids = {}
		if mode == "summary" then
			-- The lab's `.live-sum`: the count and size, and nothing else shown.
			local count, bytes = 0, 0
			for _, c in entries do
				count += 1
				bytes += c.file.size
			end
			table.insert(kids, {
				type = "frame",
				name = "Overflow summary",
				instant = true,
				width = "fill_container",
				height = 22,
				gap = 6,
				padding = { 0, 6 },
				cornerRadius = 7,
				alignItems = "center",
				bw = 376,
				bh = 22,
				children = {
					{
						type = "icon",
						name = "Summary icon",
						icon = "paperclip",
						width = 12,
						height = 12,
						bw = 12,
						bh = 12,
						fill = "#A3A199",
					},
					{
						type = "text",
						name = "Summary count",
						content = count .. " attachments",
						fontFamily = "Geist",
						fontSize = 11.5,
						fontWeight = "500",
						fill = "#EDEBE5",
						bw = 80,
						bh = 15,
					},
					{
						type = "text",
						name = "Summary size",
						content = size(bytes),
						fontFamily = "Geist Mono",
						fontSize = 10,
						fontWeight = "normal",
						fill = "#6E6C65",
						bw = 40,
						bh = 13,
					},
					{ type = "frame", name = "Spacer", width = "fill_container", height = 1, bw = 1, bh = 1 },
					{
						type = "icon",
						name = "Summary toggle",
						icon = "chevron-up",
						width = 12,
						height = 12,
						bw = 12,
						bh = 12,
						fill = "#A3A199",
					},
				},
			})
		end
		for _, d in design do
			if mode == "summary" then
				local hidden = table.clone(d)
				hidden.hidden = true
				table.insert(kids, hidden)
			else
				table.insert(kids, d)
			end
		end
		for _, e in items do
			local spec = e.kind == "group" and group_spec(e, compact) or chip_spec(e.c, compact)
			if mode == "summary" then
				spec = table.clone(spec)
				spec.hidden = true
			end
			if e.kind == "chip" then
				e.c.cur = spec
			end
			table.insert(kids, spec)
		end
		row.children = kids
		row.wrap, row.rowGap = compact, 6
		update(root, live)
		local row_inst = find(root, row)
		if before ~= mode and row_inst and row_inst.el then
			anime.animate(row_inst.el, { opacity = { 0.6, 1 }, duration = DUR * 0.6, ease = SPRING })
		end
		for _, g in pulse or {} do
			local inst = find(root, g.spec)
			if inst and inst.el and inst.el.mounted then
				anime.animate(inst.el, { scale = { 1, 1.06, 1 }, duration = 260, ease = EASE_ENTER })
			end
		end
		if PROFILE then
			PROFILE("render", os.clock() - T)
		end
	end

	-- Two or more photos, three or more files: gather the loose chips of a
	-- kind into its stack (the lab's `Hi`).
	local function gather(kind, threshold)
		local loose, group = {}, nil
		for i, e in items do
			if e.kind == "chip" and e.c.kind == kind then
				table.insert(loose, i)
			end
			if e.kind == "group" and e.type == kind then
				group = e
			end
		end
		local total = #loose + (group and #group.items or 0)
		if total < threshold or #loose == 0 then
			return nil
		end
		if not group then
			group = { kind = "group", type = kind, items = {} }
			table.insert(items, loose[1], group)
			for k = 1, #loose do
				loose[k] += 1
			end
		end
		for k = #loose, 1, -1 do
			table.insert(group.items, 1, items[loose[k]].c)
			table.remove(items, loose[k])
		end
		return group
	end

	-- The lab's upload curve: an eased approach to 99.5% over the upload,
	-- then a quick finish while the wash fades.
	local function eased(h)
		h = math.clamp(h, 0, 1)
		local o = 1 - (1 - h) ^ 5
		return 0.995 * (1 - math.exp(-3 * o)) / (1 - math.exp(-3))
	end
	local function tick(t)
		local T = os.clock()
		local busy, rerender = false, false
		for _, c in entries do
			if not c.done then
				local e = t - c.start
				local fade = 1
				if not c.finish then
					c.p = eased(e / c.duration)
					if e >= c.duration then
						c.finish, c.reached = t, c.p
					end
				end
				if c.finish then
					local f = t - c.finish
					local m = math.min(1, f / 140)
					local q = math.clamp((f - 60) / 420, 0, 1)
					c.p = c.reached + (1 - c.reached) * (1 - (1 - m) ^ 5)
					fade = 1 - q * q * (3 - 2 * q)
				end
				local inst = c.group == nil and find(root, c.cur)
				if c.finish and not c.labelled then
					-- The upload is in: the detail rises in as its type and size.
					c.labelled = true
					for _, spec in c.specs do
						child(spec, "Attachment detail").content = c.ext .. " · " .. size(c.file.size)
					end
					local detail = inst and find(inst, child(c.cur, "Attachment detail"))
					bump(detail or root)
					if detail and detail.text_el then
						anime.animate(
							detail.text_el,
							{ opacity = { 0, 1 }, y = { z(3), 0 }, duration = 220, ease = EASE_ENTER }
						)
					end
				end
				if c.finish and fade <= 0 then
					c.done = true
					rerender = rerender or c.group ~= nil
					if inst then
						-- A finished chip keeps its width (the lab's min-width).
						if inst.el and inst.el.mounted and mode == "normal" then
							c.minWidth = inst.el.width / Z
						end
						poke(find(inst, child(c.cur, "Upload progress wash")), { opacity = 0 })
					end
				elseif inst and inst.el and inst.el.mounted then
					local width = inst.el.width / Z
					-- The wash fills to the progress, its dither fading as it lands.
					local wash = find(inst, child(c.cur, "Upload progress wash"))
					if wash then
						local u = table.clone(wash.spec.dither)
						u.from, u.to = fade, 0.2 * fade
						poke(wash, { width = math.max(1, math.round(width * c.p)), dither = u })
						set_dither(wash.el, u)
					end
					if not c.finish then
						poke(
							find(inst, child(c.cur, "Attachment detail")),
							{ content = "Uploading · " .. math.round(c.p * 100) .. "%" }
						)
					end
				end
				busy = busy or not c.done
			end
		end
		-- Stacks: their bar and, for files, their status line.
		for _, e in items do
			if e.kind == "group" then
				local count, progress = uploading(e)
				if count > 0 then
					local k = mode ~= "normal" and 0.65 or 1
					local fill = find(root, e.fill)
					poke(fill, { width = math.max(0, ((e.type == "photos" and 72 or 150) - 12) * k * progress) })
					if e.type == "files" then
						local status = find(root, child(e.spec, "Upload status"))
						poke(status, { content = "Uploading " .. count .. " · " .. math.round(progress * 100) .. "%" })
					end
				end
			end
		end
		if rerender then
			render()
		end
		if PROFILE then
			PROFILE("tick", os.clock() - T)
		end

		if not busy and timer then
			timer:cancel()
			timer = nil
		end
	end

	local self = {}
	function self.clear()
		live, row, design, items, entries, mode = nil, nil, {}, {}, {}, "normal"
		if timer then
			timer:cancel()
			timer = nil
		end
	end
	-- Change the shown design in place (a click in the composer), then move
	-- to it with the usual transitions.
	function self.edit(f)
		live = live or copy(STATES[current:peek()].composer)
		if f(live, child) ~= false then
			update(root, live)
		end
	end
	-- A live chip clicked away (the lab's `Gt`): out of the row or its stack;
	-- with nothing left the composer empties.
	function self.remove(name)
		for i, e in items do
			if e.kind == "chip" and e.c.base.name == name then
				table.remove(items, i)
				break
			end
		end
		for i, c in entries do
			if c.base.name == name then
				table.remove(entries, i)
				break
			end
		end
		if #items == 0 and #design == 0 then
			self.clear()
			update(root, STATES[current:peek()].composer)
			return
		end
		render()
	end
	function self.add()
		n = n % #DATA.uploads + 1
		local file = DATA.uploads[n]
		live = live or copy(STATES[current:peek()].composer)
		local box = child(live, "Input box")
		if not box then
			return
		end
		if not row then
			for _, c in box.children do
				if c.name == "Attachments" then
					row = c
				end
			end
			if row then
				design = row.children or {}
			else
				row = {
					type = "frame",
					name = "Attachments",
					width = "fill_container",
					gap = 8,
					padding = { 2, 2, 0, 2 },
					alignItems = "center",
					bw = box.bw - 12,
					bh = 42,
					children = {},
				}
				local at = #box.children + 1
				for i, c in box.children do
					if c.name == "Input row" then
						at = i
					end
				end
				table.insert(box.children, at, row)
			end
		end
		local base = copy(DATA.uploading)
		local ext = (file.name:match("%.(%w+)$") or "file"):upper()
		local thumb = child(base, "Attachment thumb")
		base.name = "Upload · " .. file.name .. " · " .. (#entries + 1)
		child(base, "Attachment name").content = file.name
		child(base, "Attachment detail").content = "Uploading · 0%"
		local wash = child(base, "Upload progress wash")
		wash.width = 1
		if file.photo then
			thumb.img, thumb.fit, thumb.dither = file.photo, "cover", nil
		else
			local glyph = (ext:match("^XLSX?$") or ext == "CSV") and "file-spreadsheet" or "file-text"
			thumb.children = {
				{
					type = "icon",
					name = "Attachment icon",
					icon = glyph,
					width = 14,
					height = 14,
					bw = 14,
					bh = 14,
					fill = "#A3A199",
				},
			}
		end
		local c = {
			base = base,
			specs = {},
			file = file,
			ext = ext,
			kind = file.photo and "photos" or "files",
			start = (timer and timer.currentTime) or 0,
			duration = math.min(4200, 1400 + file.size / 1e6 * 450),
			p = 0,
		}
		table.insert(entries, c)
		-- Into its kind's stack if there is one, else onto the row; then
		-- gather what reaches a stack's threshold.
		local pulse = {}
		local into
		for _, e in items do
			if e.kind == "group" and e.type == c.kind then
				into = e
			end
		end
		if into then
			table.insert(into.items, c)
			table.insert(pulse, into)
		else
			table.insert(items, { kind = "chip", c = c })
		end
		for _, rule in { { "photos", 2 }, { "files", 3 } } do
			local g = gather(rule[1], rule[2])
			if g then
				table.insert(pulse, g)
			end
		end
		for _, e in items do
			if e.kind == "group" then
				for _, item in e.items do
					item.group = e
				end
			end
		end
		render(pulse)
		if not timer then
			timer = anime.createTimer {
				onUpdate = function(t)
					tick(t.currentTime)
				end,
			}
		end
	end
	return self
end)()
show_overlays(STATES[current:peek()], true)
local function named(name)
	for i, st in STATES do
		if st.name == name then
			return i
		end
	end
	return nil
end
local function toggle(a, b)
	go(named(STATES[current:peek()].name == a and b or a))
end
function click(inst)
	local name, parent = inst.spec.name or "", inst.parent and inst.parent.spec.name or ""
	local state = STATES[current:peek()].name
	if name == "Send" or name == "Dock actions" then
		go(named("Composer · Empty"))
	elseif
		name:match("^Photos") and (state == "Composer · Photo stack" or state == "Composer · Photo stack · Expanded")
	then
		toggle("Composer · Photo stack", "Composer · Photo stack · Expanded")
	elseif name == "List header" then
		toggle("Composer · Overflow summary", "Composer · Overflow summary · Open")
	elseif name:match("^Choice · ") then
		-- Picking the chosen one again answers; another moves the pick.
		if inst.spec.fill == "#1D1F2B" then
			return go(named("Composer · Empty"))
		end
		uploads.edit(function(live, find)
			local options = inst.parent.spec.name
			local function walk(n)
				if n.name == options then
					for _, c in n.children do
						if (c.name or ""):match("^Choice · ") then
							local on = c.name == name
							c.fill = on and "#1D1F2B" or nil
							c.stroke = on and "#5F6AA3" or "#33322D"
						end
					end
				end
				for _, c in n.children or {} do
					walk(c)
				end
			end
			walk(live)
		end)
	elseif name:match("^Remove queued message") then
		-- The queued message closes where it stands.
		local message = inst.parent
		while message and not (message.spec.name or ""):match("^Queued message") do
			message = message.parent
		end
		if message then
			uploads.edit(function(live)
				local function walk(n)
					for i, c in n.children or {} do
						if c.name == message.spec.name then
							table.remove(n.children, i)
							return true
						end
						if walk(c) then
							return true
						end
					end
					return false
				end
				walk(live)
			end)
		end
	elseif name:match("^Upload · ") then
		uploads.remove(name)
	elseif parent == "Attachments" or parent:match("^Chip row") then
		-- A chip closes sideways; with none left the composer empties.
		local left, index = 0, 0
		for _, k in inst.parent.kids do
			if not k.exiting then
				index += 1
			end
			if k == inst then
				break
			end
		end
		uploads.edit(function(live)
			local function walk(n)
				for i, c in n.children or {} do
					if c.name == parent and c.children then
						if c.children[index] and c.children[index].name == name then
							table.remove(c.children, index)
						end
						left += #c.children
						return
					end
					walk(c)
				end
			end
			walk(live)
		end)
		if left == 0 then
			go(named("Composer · Empty"))
		end
	end
	return nil
end

-- For recording a transition: GOTO = { from, to, slow }.
if type(GOTO) == "table" then
	slow:set(GOTO[3] or 1)
	anime.engine.speed = 1 / slow:peek()
	if GOTO[2] == "upload" then
		for k = 1, GOTO[4] or 3 do
			after(300 + (k - 1) * (GOTO[5] or 1000), function()
				uploads.add()
			end)
		end
	else
		after(300 / slow:peek(), function()
			go(GOTO[2])
		end)
	end
end

-- Autoplay steps on every 1.8 s, as the lab's does.
local timer
local function schedule()
	if timer then
		timer:cancel()
	end
	timer = nil
	if not autoplay:peek() then
		return
	end
	timer = anime.createTimer {
		duration = 1800,
		onComplete = function()
			timer = nil
			go(current:peek() + 1)
			schedule()
		end,
	}
end
schedule()

-- Controls in the lab's top bar style.
local SANS, MONO = "Geist", "Geist Mono"
local C = { text = "#EDEBE5", sub = "#A3A199", muted = "#6E6C65", key = "#858379", line = "#262521" }
local function control(label, on_click, on, id)
	return ui.button {
		id = id,
		px = 10,
		h = 26,
		rounded = 7,
		items = "center",
		cursor = "pointer",
		bg = function()
			return (on and on()) and "#ffffff1f" or "#ffffff0a"
		end,
		border = 1,
		border_color = "#ffffff0d",
		hover = { bg = "#ffffff1a" },
		on_click = on_click,
		ui.text { label, font = SANS, text_size = 12, color = C.sub },
	}
end
local function speed(f)
	return control(f .. "×", function()
		slow:set(f)
		anime.engine.speed = 1 / f
	end, function()
		return slow:get() == f
	end)
end

local function sidebar()
	local rows = {}
	local group
	for i, st in STATES do
		local g = st.group:gsub(" · row %d", "")
		if g ~= group then
			group = g
			table.insert(
				rows,
				ui.text { g, font = MONO, text_size = 10, color = C.muted, pt = #rows > 0 and 12 or 0, pb = 4, px = 8 }
			)
		end
		table.insert(
			rows,
			ui.button {
				id = "state-" .. i,
				h = 24,
				px = 8,
				rounded = 6,
				items = "center",
				cursor = "pointer",
				bg = function()
					return current:get() == i and "#ffffff14" or "#ffffff00"
				end,
				hover = { bg = "#ffffff0d" },
				on_click = function()
					go(i)
				end,
				ui.text {
					function()
						return st.name
					end,
					font = SANS,
					text_size = 12,
					truncate = true,
					color = function()
						return current:get() == i and C.text or C.sub
					end,
				},
			}
		)
	end
	return ui.scroll { w = 250, h_full = true, ui.column { items = "stretch", p = 10, gap = 1, rows } }
end

local Stage = ui.component(function()
	return view(root)
end)

return ui.column {
	w_full = true,
	h_full = true,
	bg = "#0d0d0dcc",
	color = C.text,
	items = "stretch",
	on_key = function(e)
		if e.key == "ArrowRight" then
			go(current:peek() + 1)
			return true
		end
		if e.key == "ArrowLeft" then
			go(current:peek() - 1)
			return true
		end
		return false
	end,
	ui.row {
		h = 44,
		-- Clear of the window's own buttons: the free part of the title bar
		-- (macOS's traffic lights at the left, Windows' buttons at the right);
		-- the traffic lights' 84 points where the platform does not say.
		pl = function()
			local area = ui.window.titlebar_area:get()
			return if area then area.x + 12 else 84
		end,
		pr = function()
			local area, buttons = ui.window.titlebar_area:get(), ui.window.controls:get()
			if area and buttons and buttons.x >= area.x + area.width then
				return buttons.width + 12
			end
			return 12
		end,
		gap = 6,
		items = "center",
		window_drag = true,
		ui.text { "Composer", font = SANS, text_size = 15, weight = 500, color = C.text },
		ui.text {
			function()
				local st = STATES[current:get()]
				return current:get() .. " / " .. #STATES .. " · " .. st.group:gsub(" · row %d", "")
			end,
			font = SANS,
			text_size = 12,
			color = C.key,
			ml = 4,
		},
		ui.div { grow = 1 },
		control("Previous", function()
			go(current:peek() - 1)
		end),
		control("Next", function()
			go(current:peek() + 1)
		end),
		control("Upload test", function()
			uploads.add()
		end, nil, "upload"),
		control("Autoplay", function()
			autoplay:set(not autoplay:peek())
			schedule()
		end, function()
			return autoplay:get()
		end),
		ui.div { w = 8 },
		speed(1),
		speed(2),
		speed(5),
		speed(10),
	},
	ui.row {
		grow = 1,
		shrink = 1,
		min_h = 0,
		items = "stretch",
		sidebar(),
		ui.column {
			grow = 1,
			shrink = 1,
			min_w = 0,
			items = "center",
			justify = "end",
			pb = 48,
			bg = ui.shared("composer_compare", false):peek() and "#0A0A09" or nil,
			ui.text {
				function()
					return STATES[current:get()].name
				end,
				font = SANS,
				text_size = 15,
				weight = 500,
				color = C.text,
				absolute = true,
				top = 12,
				left = 16,
			},
			ui.column {
				relative = true,
				items = "start",
				Stage(),
				-- The upload test's measuring, laid out but never drawn.
				ui.column {
					absolute = true,
					left = 0,
					top = 0,
					invisible = true,
					gap = 20,
					ui.each(measure_list, function(id)
						local spec = measure_specs[id]
						return spec and view(instance(spec, nil, nil)) or ui.div {}
					end, {
						key = function(id)
							return id
						end,
					}),
				},
				-- The popovers, after the composer so they draw over it, hung
				-- from its top edge.
				ui.column {
					absolute = true,
					top = 0,
					left = 0,
					right = 0,
					ui.each(over_list, overlay_view, {
						key = function(id)
							return id
						end,
					}),
				},
			},
		},
	},
}
