-- Tokens and small controls shared by the profiler and its menus.
local icons = require("./icons")
local K = {}

-- What `segmented` selects through: a number state, or an object with the
-- same `get` and `set` (the status bar's speed picker). `self` is `any`:
-- `State<number>`'s methods take a `State<number>`, the picker's take
-- nothing, and only `any` is a subtype of both.
export type Choice = {
	get: (self: any) -> number,
	set: (self: any, value: number) -> (),
}
-- `segmented`'s look: option height, padding and gaps, radii, fills.
export type SegmentedStyle = {
	h: number,
	w: Reactive<Length>?,
	px: Length?,
	gap: Num?,
	r: number?,
	p: number?,
	outer: number?,
	spacing: Num?,
	bg: Color?,
	pill: Color?,
	plain: boolean?,
}

K.SANS, K.MONO = "Geist", "Geist Mono"
K.C = {
	window = "#121211",
	body = "#161615",
	inset = "#0F0F0E",
	menu = "#1A1A18",
	tip = "#1F1F1D",
	raised = "#1F1F1D",
	selected = "#262624",
	line = "#262521",
	subtle = "#222220",
	strong = "#33322D",
	text = "#EDEBE5",
	body_text = "#D4D2CB",
	secondary = "#A3A199",
	tertiary = "#858379",
	muted = "#6E6C65",
	faint = "#4A4943",
	coral = "#E8A898",
	gold = "#E2C98F",
	warn = "#D9A23B",
	info = "#A9BABF",
	live = "#C3CEB2",
	clear = "#00000000",
}
local C = K.C

-- Motion: the design system's springs and fades, in ms.
K.FAST, K.FADE, K.MOVE = 120, 160, 220
function K.spring(stiffness: number?, damping: number?): AnimeEase
	return anime.createSpring { stiffness = stiffness or 520, damping = damping or 34 }
end

local svgs: { [string]: string } = {}
function K.icon(name: string, size: number, color: Reactive<Color>?, props: SvgProps?): Node
	local code = svgs[name]
	if not code then
		assert(icons[name], "missing icon " .. name)
		code = '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="#fff" '
			.. 'stroke-width="2" stroke-linecap="round" stroke-linejoin="round">'
			.. icons[name]
			.. "</svg>"
		svgs[name] = code
	end
	-- `props` win over the defaults.
	local node: SvgProps = if props then table.clone(props) else {}
	if node.code == nil then
		node.code = code
	end
	if node.w == nil then
		node.w = size
	end
	if node.h == nil then
		node.h = size
	end
	if node.tint == nil then
		node.tint = color
	end
	if node.shrink == nil then
		node.shrink = 0
	end
	return ui.svg(node)
end

local function textNode(font: Font, content: Text, size: number, color: Reactive<Color>, props: TextProps?): Node
	-- `props` win over the defaults.
	local node: TextProps = if props then table.clone(props) else {}
	if node[1] == nil then
		node[1] = content
	end
	if node.font == nil then
		node.font = font
	end
	if node.text_size == nil then
		node.text_size = size
	end
	if node.color == nil then
		node.color = color
	end
	return ui.text(node)
end
function K.text(content: Text, size: number, color: Reactive<Color>?, props: TextProps?): Node
	return textNode(K.SANS, content, size, color or C.text, props)
end
function K.mono(content: Text, size: number, color: Reactive<Color>?, props: TextProps?): Node
	return textNode(K.MONO, content, size, color or C.muted, props)
end
function K.spacer(): Node
	return ui.div { grow = 1, shrink = 1, min_w = 0 }
end

-- Press feedback: a quick dip, then a spring back.
function K.press(props: ContainerProps, scale: number?): ContainerProps
	local ref = ui.ref()
	props.ref = ref
	props.cursor = props.cursor or "pointer"
	local press, release = props.on_press, props.on_release
	-- The handlers wrapped are the script's functions (not actions).
	props.on_press = function(e: PointerEvent)
		anime.animate(ref, { scale = scale or 0.95, duration = 70, ease = "outQuad" })
		if type(press) == "function" then
			press(e)
		end
	end
	props.on_release = function(e: PointerEvent)
		anime.animate(ref, { scale = 1, duration = 380, ease = K.spring(600, 28) })
		if type(release) == "function" then
			release(e)
		end
	end
	return props
end

-- A plain button: no default padding or fill, a hover wash.
function K.button(props: ContainerProps, scale: number?): Node
	props.p = props.p or 0
	props.bg = props.bg or C.clear
	props.border = props.border or 0
	props.hover = props.hover or { bg = C.raised }
	-- The design has no focus ring: a pressed control keeps its own look.
	props.focus = props.focus or {}
	props.direction = props.direction or "row"
	props.items = props.items or "center"
	return ui.button(K.press(props, scale))
end

function K.icon_button(id: string, name: string, size: number?, on_click: Handler?, active: (() -> boolean)?): Node
	return K.button {
		id = id,
		w = 28,
		h = 28,
		rounded = 7,
		justify = "center",
		on_click = on_click,
		bg = active and function()
			return active() and C.selected or C.clear
		end or nil,
		K.icon(name, size or 14, C.secondary),
	}
end

-- A row of options over a pill that slides (and resizes) to the selected one.
-- `build(i, active)` returns option i's contents; `active()` is reactive.
function K.segmented(
	id: string,
	selected: Choice,
	count: number,
	build: (i: number, active: () -> boolean) -> Child,
	style: SegmentedStyle
): Node
	local geom = ui.state(id .. "_geom", {} :: { { [string]: number } })
	local row: Element? = nil
	local items: { Element } = {}
	local function measure()
		if not row then
			return
		end
		local g: { { [string]: number } } = {}
		for i, el in items do
			g[i] = { x = (el.x or 0) - (row.x or 0), w = el.width or 0 }
		end
		geom:set(g)
	end
	local function at(field: string): Binding
		return ui.spring(
			ui.computed(function()
				local g = geom:get()[selected:get()]
				return g and g[field] or 0
			end),
			{ stiffness = 420, damping = 34 }
		)
	end
	local options: { Node } = {}
	for i = 1, count do
		local active = function()
			return selected:get() == i
		end
		options[i] = K.button({
			id = id .. "_" .. i,
			h = style.h,
			w = style.w,
			px = style.px or 8,
			gap = style.gap or 6,
			rounded = style.r or 5,
			bg = C.clear,
			hover = { bg = C.clear },
			justify = "center",
			on_click = function()
				selected:set(i)
			end,
			on_resize = function(_, el: Element)
				items[i] = el
				measure()
			end,
			build(i, active),
		}, 0.96)
	end
	return ui.overlay {
		id = id,
		h = style.h + 2 * (style.p or 2),
		p = style.p or 2,
		bg = style.bg,
		rounded = style.outer or 7,
		on_resize = function(_, el: Element)
			row = el
			measure()
		end,
		ui.div {
			absolute = true,
			top = style.p or 2,
			left = 0,
			h = style.h,
			rounded = style.r or 5,
			bg = style.pill or C.selected,
			x = at("x"),
			w = at("w"),
			opacity = function()
				return #geom:get() > 0 and 1 or 0
			end,
		},
		ui.row { gap = style.spacing or 2, items = "center", options },
	}
end

-- The design's switch: 32×18, a dark knob on a light track when on.
function K.toggle(id: string, value: State<boolean>): Node
	local knob = ui.spring(
		ui.computed(function()
			return value:get() and 14 or 0
		end),
		{ stiffness = 700, damping = 36 }
	)
	local wash = ui.tween(
		ui.computed(function()
			return value:get() and 1 or 0
		end),
		{ duration = 140 }
	)
	return K.button({
		id = id,
		w = 32,
		h = 18,
		rounded = 9,
		bg = "#343430",
		hover = { bg = "#3A3935" },
		on_click = function()
			value:toggle()
		end,
		ui.overlay {
			w = 32,
			h = 18,
			ui.div { absolute = true, top = 0, left = 0, w = 32, h = 18, rounded = 9, bg = C.text, opacity = wash },
			ui.div {
				absolute = true,
				top = 2,
				left = 2,
				w = 14,
				h = 14,
				rounded = 7,
				x = knob,
				bg = function()
					return value:get() and C.window or C.tertiary
				end,
			},
		},
	}, 0.94)
end

-- A bordered key, as tooltips and hints show shortcuts.
function K.kbd(label: string): Node
	return ui.row {
		h = 18,
		px = 5,
		rounded = 4,
		border = 1,
		border_color = C.strong,
		items = "center",
		K.mono(label, 10, C.secondary, { letter_spacing = 4 }),
	}
end

-- A small key cap: icons and/or text.
function K.keys(parts: { string }, color: Color?): Node
	local nodes: { Node } = {}
	for _, part in parts do
		if icons[part] then
			table.insert(nodes, K.icon(part, 11, color or C.muted))
		else
			table.insert(nodes, K.mono(part, 10.5, color or C.muted))
		end
	end
	return ui.row { gap = 3, items = "center", h = 14, nodes }
end

return K
