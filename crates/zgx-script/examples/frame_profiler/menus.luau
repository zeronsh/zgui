-- The profiler's popovers and tooltips (pen kZfcb and XFJTT): Target, More
-- with Export, Runtime settings, Search, Overlays, capture options, Budget,
-- Zoom, the span context menu, and the status bar's Logs and Capture
-- buffer, whose selects open dropdowns.
-- One layer over the window that the pointer passes through: a catcher
-- that closes on a press, a dim for search, the open panel (scaling in
-- from its anchor's corner, fading out), an open dropdown and the hover
-- tooltip.
-- (Both are already loaded: these are the modules app.luau passes in, here
-- for their types.)
local Kit = require("./kit")
local Data = require("./data")

type Box = { x: number, y: number, w: number, h: number }
-- Where a panel goes in the window, and the corner it scales from.
type Where = { left: number?, top: number?, bottom: number?, origin: { number }? }
-- A panel's props: a column's, and whether a field inside takes focus.
type PanelProps = Merge<ContainerProps, { keep_focus: boolean? }>
-- A menu list's entry: a header, a separator, a hint or an item (see `list`).
type Entry = {
	header: Text?,
	sep: boolean?,
	hint: string?,
	icon: string?,
	label: Text?,
	mono: boolean?,
	keys: { string }?,
	right: Text?,
	color: Color?,
	check: (() -> boolean)?,
	selected: (() -> boolean)?,
	run: (() -> ())?,
	hover: (() -> ())?,
	submenu: boolean?,
}
-- The open panel's keyboard: item count, its highlight, how to run item i;
-- More's Export submenu: the item that opens it, its count, how to run one.
type Keys = {
	count: number,
	state: State<number>?,
	run: ((i: number) -> ())?,
	sub: number?,
	sub_count: number?,
	sub_run: ((i: number) -> ())?,
}
type Dropdown = { state: State<number>, options: { { string } }, w: number }
type TipSpec = { label: string, glyph: string?, keys: string?, detail: string?, up: boolean? }

-- What the menus read and call of the profiler's state (app.luau's `S`).
export type Store = {
	menu: State<string>,
	hint: State<string>,
	tab: State<number>,
	range: State<number>,
	frame: State<number>,
	span: State<string>,
	area: State<number>,
	rec: State<string>,
	cap_length: State<number>,
	cap_start: State<number>,
	overlays: State<{ boolean }>,
	budget: State<number>,
	follow_cap: State<boolean>,
	zoom: State<Data.Zoom>,
	keep_zoom: State<boolean>,
	ctx: State<{ id: string, x: number, y: number }>,
	query: State<string>,
	target: State<number>,
	backend: State<number>,
	cap: State<number>,
	buffer_size: State<number>,
	when_full: State<number>,
	on_top: State<boolean>,
	switches: {
		blur: State<boolean>,
		scroll: State<boolean>,
		damage: State<boolean>,
		gpu: State<boolean>,
		trace: State<boolean>,
		reload: State<boolean>,
	},
	viewport: State<{ w: number, h: number }>,
	anchors: { [string]: Element },
	anchor_tick: State<number>,
	current: State<Data.FrameInfo>,
	hint_hover: ((name: string, inside: boolean) -> ())?,
	comma: (n: number) -> string,
	open: (name: string) -> (),
	target_meta: () -> string,
	flip_overlay: (i: number) -> (),
	reset_runtime: () -> (),
	step: (d: number) -> (),
	preview: (r: string) -> (),
	clear: () -> (),
	span_by_id: (id: string) -> Data.Span?,
	zoom_to: (mode: string) -> (),
	zoom_by: (f: number) -> (),
	buffer_fill: () -> number,
	buffer_frames_text: () -> string,
	buffer_bytes: () -> string,
	buffer_eta: () -> string,
	buffer_drop: () -> string,
}

return function(S: Store, K: typeof(Kit), D: typeof(Data))
	local C, icon, text, mono, spacer = K.C, K.icon, K.text, K.mono, K.spacer
	local M = {}
	local hi = ui.state("menu_hi", 0)
	local sub = ui.state("submenu", false)
	local subHi = ui.state("submenu_hi", 0)
	-- The open dropdown, and its highlighted option.
	local drop = ui.state("dropdown", "")
	local dropHi = ui.state("dropdown_hi", 0)
	-- The open panel's keyboard: item count, its highlight, how to run item i.
	local keys: Keys = { count = 0 }
	local function close()
		S.menu:set("")
	end
	ui.effect(function()
		local m = S.menu:get()
		hi:set(m == "span" and 1 or 0)
		sub:set(false)
		drop:set("")
		S.hint:set("")
	end)

	local function viewport(): { w: number, h: number }
		return S.viewport:peek()
	end
	local function anchorBox(name: string): Box
		local a = S.anchors[name]
		if a and a.mounted then
			return { x = a.x or 0, y = a.y or 0, w = a.width or 0, h = a.height or 0 }
		end
		return { x = 0, y = 0, w = 0, h = 0 }
	end
	-- Where a panel `w` wide goes: under its anchor, from its left ("start")
	-- or right ("end") edge, or above it ("up", "up-end").
	local function place(name: string, w: number, how: string): Where
		local a, v = anchorBox(name), viewport()
		if how == "up" then
			return { left = math.clamp(a.x, 8, v.w - w - 8), bottom = v.h - a.y + 6, origin = { 0, 1 } }
		elseif how == "up-end" then
			return { left = math.clamp(a.x + a.w - w, 8, v.w - w - 8), bottom = v.h - a.y + 6, origin = { 1, 1 } }
		elseif how == "end" then
			return { left = math.clamp(a.x + a.w - w, 8, v.w - w - 8), top = a.y + a.h + 4, origin = { 1, 0 } }
		end
		return { left = math.clamp(a.x, 8, v.w - w - 8), top = a.y + a.h + 4, origin = { 0, 0 } }
	end

	local SHADOW: { ShadowSpec } =
		{ { y = 12, blur = 28, color = "#00000066" }, { y = 2, blur = 6, color = "#00000040" } }
	-- A panel: menus are 10-rounded with 4 points of padding, popovers 12.
	-- It scales in from its `origin` (its anchor's corner) as it mounts and
	-- takes keyboard focus, unless `keep_focus` (a field inside takes it).
	local function panel(props: PanelProps, where: Where?): Node
		if where then
			props.left = where.left or props.left
			props.top = where.top or props.top
			props.bottom = where.bottom or props.bottom
			props.origin = where.origin or props.origin
		end
		local keep = props.keep_focus
		props.keep_focus = nil
		props.absolute = true
		props.bg = C.menu
		props.border = 1
		props.border_color = C.strong
		props.rounded = props.rounded or 10
		props.shadow = SHADOW
		props.focusable = true
		props.opacity = 0
		props.on_mount = function(el: Element)
			anime.animate(el, { opacity = 1, scale = { 0.96, 1 }, duration = 200, ease = "outExpo" })
			if not keep then
				el:focus()
			end
		end
		return ui.column(props)
	end
	local function highlight(state: State<number>, offsets: { number }, h: number, inset: number?): Node
		return ui.div {
			absolute = true,
			left = inset or 0,
			right = inset or 0,
			top = 0,
			h = h,
			rounded = h > 30 and 8 or 6,
			bg = C.selected,
			y = ui.spring(
				ui.computed(function()
					return offsets[math.max(1, state:get())] or 0
				end),
				{ stiffness = 800, damping = 44 }
			),
			opacity = ui.tween(
				ui.computed(function()
					return offsets[state:get()] and 1 or 0
				end),
				{ duration = 80 }
			),
		}
	end

	-- A menu list. Entries: { header }, { sep = true }, { hint } or an item
	-- { icon, label, mono, keys, right, color, check, selected, run, hover,
	-- submenu }. `check` swaps the icon for a check; `selected` (a choice
	-- in a group) fills the row and puts a check at its end.
	local function list(id: string, entries: { Entry }, state: State<number>): (Node, number, { Entry }, { number })
		local nodes: { Node }, offsets: { number }, runs: { Entry } = {}, {}, {}
		local y, n = 0, 0
		for _, e in entries do
			if e.header then
				table.insert(
					nodes,
					ui.row { h = 23, px = 8, pt = 6, pb = 4, items = "center", mono(e.header, 10, C.muted) }
				)
				y += 24
			elseif e.sep then
				table.insert(
					nodes,
					ui.column { w_full = true, h = 9, py = 4, ui.div { w_full = true, h = 1, bg = C.line } }
				)
				y += 10
			elseif e.hint then
				table.insert(
					nodes,
					ui.row {
						w_full = true,
						px = 8,
						py = 6,
						text(e.hint, 11, C.muted, { wrap = true, line_height = 15.4, grow = 1, shrink = 1, min_w = 0 }),
					}
				)
			else
				n += 1
				local i = n
				offsets[i], runs[i] = y, e
				local fg = e.color or C.text
				local on = e.selected
				table.insert(
					nodes,
					ui.row {
						id = id .. "_" .. i,
						w_full = true,
						h = 30,
						px = 8,
						gap = 9,
						items = "center",
						rounded = 6,
						cursor = "pointer",
						bg = on and function()
							return on() and C.selected or C.clear
						end or nil,
						on_hover = function(inside)
							if inside then
								state:set(i)
								if e.hover then
									e.hover()
								end
							end
						end,
						on_click = function()
							if e.run then
								e.run()
							end
						end,
						e.check and icon("check", 14, C.tertiary, {
							opacity = ui.tween(
								ui.computed(function()
									return e.check() and 1 or 0
								end),
								{ duration = 120 }
							),
							scale = ui.spring(
								ui.computed(function()
									return e.check() and 1 or 0.6
								end),
								{ stiffness = 700, damping = 30 }
							),
						}) or icon(e.icon or "", 14, on and function()
							return on() and C.text or C.tertiary
						end or (e.color or C.tertiary)),
						e.mono and mono(e.label or "", 11.5, fg) or text(e.label or "", 12.5, fg),
						spacer(),
						e.keys and K.keys(e.keys) or nil,
						e.right and mono(e.right, 10.5, C.muted) or nil,
						e.submenu and icon("chevron-right", 12, C.muted) or nil,
						on and icon("check", 13, C.text, {
							opacity = ui.tween(
								ui.computed(function()
									return on() and 1 or 0
								end),
								{ duration = 120 }
							),
							scale = ui.spring(
								ui.computed(function()
									return on() and 1 or 0.6
								end),
								{ stiffness = 700, damping = 30 }
							),
						}) or nil,
					}
				)
				y += 31
			end
		end
		local body =
			ui.overlay { w_full = true, highlight(state, offsets, 30), ui.column { w_full = true, gap = 1, nodes } }
		return body, n, runs, offsets
	end
	local function bind(n: number, runs: { Entry }, state: State<number>)
		keys.count, keys.state = n, state
		keys.run = function(i: number)
			local entry = runs[i]
			if entry and entry.run then
				entry.run()
			end
		end
	end
	local function menu(id: string, w: number, where: Where, entries: { Entry }): Node
		local body, n, runs = list(id .. "_item", entries, hi)
		bind(n, runs, hi)
		return panel({
			id = id,
			w = w,
			p = 4,
			on_hover = function(inside)
				if not inside then
					hi:set(0)
				end
			end,
			body,
		}, where)
	end
	local function run(f: () -> ()): () -> ()
		return function()
			f()
			close()
		end
	end

	-- Popover pieces: a titled header, a ruled footer, a text button.
	local function head(title: string, meta: string): Node
		return ui.row {
			w_full = true,
			gap = 8,
			items = "center",
			pt = 12,
			px = 14,
			pb = 6,
			text(title, 12.5, C.text, { weight = 500 }),
			mono(meta, 10, C.muted),
		}
	end
	local function foot(children: { Node }): Node
		return ui.row {
			w_full = true,
			h = 38,
			px = 14,
			gap = 8,
			items = "center",
			border = 1,
			border_sides = { 1, 0, 0, 0 },
			border_color = C.subtle,
			children,
		}
	end
	local function link(id: string, label: string, color: Color?, on_click: () -> ()): Node
		return ui.button {
			id = id,
			p = 0,
			bg = C.clear,
			border = 0,
			cursor = "pointer",
			color = color or C.secondary,
			hover = { color = C.text },
			focus = {},
			on_click = on_click,
			ui.text { label, text_size = 12 },
		}
	end

	-- Dropdowns (pen XFJTT · size / when full open): a select that opens a
	-- list of options, each a label over a description, under its right
	-- edge. Options: { { label, description } }; `state` holds the index.
	local DROPS: { [string]: Dropdown } = {}
	local function dropdown(id: string, state: State<number>, options: { { string } }, w: number?): Node
		DROPS[id] = { state = state, options = options, w = w or 240 }
		local open = function()
			return drop:get() == id
		end
		return ui.div {
			on_resize = function(_, el: Element)
				S.anchors[id] = el
			end,
			K.button({
				id = id,
				h = 26,
				pl = 9,
				pr = 8,
				gap = 6,
				rounded = 7,
				bg = function()
					return open() and "#343430" or C.selected
				end,
				hover = { bg = "#343430" },
				on_click = function()
					if drop:peek() == id then
						drop:set("")
					else
						dropHi:set(state:peek())
						drop:set(id)
					end
				end,
				ui.text {
					function()
						return options[state:get()][1]
					end,
					text_size = 12,
					color = C.text,
				},
				icon("chevrons-down", 12, C.muted),
			}, 0.96),
		}
	end
	local function dropPanel(id: string): Node
		local d = DROPS[id]
		local a, v = anchorBox(id), viewport()
		local rows: { Node } = {}
		for i, o in d.options do
			rows[i] = ui.column {
				id = id .. "_option_" .. i,
				w_full = true,
				gap = 2,
				py = 6,
				px = 8,
				rounded = 6,
				cursor = "pointer",
				bg = function()
					if d.state:get() == i then
						return C.selected
					end
					return dropHi:get() == i and "#222220" or C.clear
				end,
				on_hover = function(inside)
					if inside then
						dropHi:set(i)
					end
				end,
				on_click = function()
					d.state:set(i)
					drop:set("")
				end,
				text(o[1], 12.5, C.text),
				text(o[2], 11, C.muted, { wrap = true, w_full = true }),
			}
		end
		local w = d.w
		local below = a.y + a.h + 6
		local where: Where = if below + 220 < v.h
			then { top = below, origin = { 1, 0 } }
			else { bottom = v.h - a.y + 6, origin = { 1, 1 } }
		where.left = math.clamp(a.x + a.w - w, 8, v.w - w - 8)
		return panel({ id = id .. "_list", w = w, p = 4, gap = 1, keep_focus = true, rows }, where)
	end

	-- Target: processes, opening upward from the status bar.
	local function targetPanel(): Node
		local entries: { Entry } = { {
			header = function()
				return S.target_meta()
			end,
		} }
		for i, p in D.processes do
			table.insert(entries, {
				mono = true,
				label = p[1],
				right = p[2],
				check = function()
					return S.target:get() == i
				end,
				run = run(function()
					S.target:set(i)
				end),
			})
		end
		table.insert(entries, { sep = true })
		table.insert(
			entries,
			{ icon = "plus", label = "Attach to process…", keys = { "command", "arrow-up", "T" }, run = close }
		)
		table.insert(entries, { icon = "x", label = "Detach", run = close })
		return menu("target_menu", 248, place("target", 248, "up"), entries)
	end

	-- More, with Export opening to its left.
	local function morePanel(): Node
		local entries: { Entry } = {
			{ header = "Capture" },
			{
				icon = "skip-forward",
				label = "Step one frame",
				keys = { "arrow-right" },
				run = run(function()
					S.step(1)
				end),
			},
			{ icon = "flag", label = "Mark frame", keys = { "M" }, run = close },
			{ sep = true },
			{ icon = "upload", label = "Import capture…", keys = { "command", "O" }, run = close },
			{
				icon = "download",
				label = "Export",
				submenu = true,
				run = function()
					sub:set(true)
					subHi:set(1)
				end,
				hover = function()
					sub:set(true)
				end,
			},
			{ sep = true },
			{
				icon = "pin",
				label = "Keep window on top",
				run = function()
					S.on_top:toggle()
				end,
			},
			{
				icon = "sliders",
				label = "Runtime settings…",
				run = function()
					S.menu:set("runtime")
				end,
			},
			{ sep = true },
			{ icon = "slash", label = "Clear capture", color = C.coral, keys = { "command", "K" }, run = run(S.clear) },
		}
		for _, e in entries do
			if e.label and not e.submenu then
				e.hover = function()
					sub:set(false)
				end
			end
		end
		local body, n, runs, offsets = list("more_item", entries, hi)
		bind(n, runs, hi)
		keys.sub = 4
		local subBody, subN, subRuns = list("export_item", {
			{
				header = function()
					return "Frame " .. S.comma(S.current:get().n)
				end,
			},
			{ icon = "braces", label = "Chrome trace", right = ".json", run = close },
			{ icon = "file-text", label = "Perfetto trace", right = ".pftrace", run = close },
			{ icon = "image", label = "Frame as PNG", keys = { "command", "arrow-up", "S" }, run = close },
			{ sep = true },
			{ icon = "clipboard", label = "Copy summary", keys = { "command", "arrow-up", "C" }, run = close },
		}, subHi)
		keys.sub_count = subN
		keys.sub_run = function(i: number)
			local entry = subRuns[i]
			if entry and entry.run then
				entry.run()
			end
		end
		return panel({
			id = "more_menu",
			w = 248,
			p = 4,
			on_hover = function(inside)
				if not inside and not sub:peek() then
					hi:set(0)
				end
			end,
			ui.overlay {
				w_full = true,
				body,
				-- Keep window on top shows a check while on.
				ui.row {
					absolute = true,
					right = 8,
					top = offsets[5] + 8,
					h = 14,
					opacity = ui.tween(
						ui.computed(function()
							return S.on_top:get() and 1 or 0
						end),
						{ duration = 120 }
					),
					icon("check", 14, C.tertiary),
				},
			},
			ui.presence(function()
				return sub:get() and "export" or nil
			end, function()
				return panel {
					id = "export_menu",
					w = 220,
					p = 4,
					right = 252,
					top = offsets[4],
					origin = { 1, 0 },
					keep_focus = true,
					subBody,
				}
			end, {
				absolute = true,
				top = 0,
				right = 0,
				pointer_events = "none",
				enter = { x = { 8, 0 }, duration = 180, ease = "outExpo" },
				exit = { opacity = 0, x = 4, duration = 90, ease = "inQuad" },
			}),
		}, place("more", 248, "end"))
	end

	-- Runtime settings, from More: renderer and development flags.
	local copied = ui.state("copied", false)
	local function section(label: string, children: { Node }): Node
		return ui.column {
			w_full = true,
			pt = 4,
			px = 12,
			pb = 8,
			border = 1,
			border_sides = { 0, 0, 1, 0 },
			border_color = C.subtle,
			ui.row { h = 28, items = "center", mono(label, 10, C.muted, { letter_spacing = 0.8 }) },
			children,
		}
	end
	local function setting(label: string, control: Node): Node
		return ui.row { w_full = true, h = 34, gap = 10, items = "center", text(label, 12.5, C.text), spacer(), control }
	end
	local function runtimePanel(): Node
		keys.count = 0
		local sw = S.switches
		return panel({
			id = "runtime_menu",
			w = 300,
			rounded = 12,
			section("RENDERER", {
				setting("Backend", dropdown("backend_select", S.backend, D.backends)),
				setting("Frame cap", dropdown("cap_select", S.cap, D.caps)),
				setting("Blur & effects", K.toggle("sw_blur", sw.blur)),
				setting("Scroll resample", K.toggle("sw_scroll", sw.scroll)),
			}),
			section("DEVELOPMENT", { setting("Hot reload", K.toggle("sw_reload", sw.reload)) }),
			ui.row {
				w_full = true,
				px = 12,
				py = 10,
				gap = 8,
				items = "center",
				K.button({
					id = "copy_env",
					h = 28,
					px = 10,
					gap = 7,
					rounded = 8,
					bg = C.raised,
					border = 1,
					border_color = C.line,
					hover = { bg = C.selected },
					on_click = function()
						copied:set(true)
						anime.createTimer {
							duration = 1400,
							onComplete = function()
								copied:set(false)
							end,
						}
					end,
					ui.presence(copied, function(done: boolean)
						return ui.row {
							gap = 7,
							items = "center",
							icon(done and "check" or "copy", 13, done and C.live or C.secondary),
							ui.text { done and "Copied" or "Copy version & env", text_size = 12, color = C.text },
						}
					end, {
						mode = "wait",
						layout = "width",
						enter = { opacity = { 0, 1 }, y = { 4, 0 }, duration = 160, ease = "outQuad" },
						exit = { opacity = 0, y = -4, duration = 90 },
					}),
				}, 0.96),
				spacer(),
				link("runtime_reset", "Reset", C.tertiary, S.reset_runtime),
			},
		}, place("more", 300, "end"))
	end

	-- Capture options, from the stopped record control's chevron.
	local function capturePanel(): Node
		local entries: { Entry } = { { header = "Length" } }
		-- A length is { label, icon, seconds? }.
		for i, l in D.lengths do
			table.insert(entries, {
				icon = l[2] :: string,
				label = l[1],
				selected = function()
					return S.cap_length:get() == i
				end,
				run = function()
					S.cap_length:set(i)
				end,
			})
		end
		table.insert(entries, { sep = true })
		table.insert(entries, { header = "Start" })
		for i, s in D.starts do
			table.insert(entries, {
				icon = s[2],
				label = s[1],
				selected = function()
					return S.cap_start:get() == i
				end,
				run = function()
					S.cap_start:set(i)
				end,
			})
		end
		table.insert(entries, { hint = "Waits for a click, key or scroll in the main window." })
		return menu("capture_menu", 232, place("record", 232, "end"), entries)
	end

	-- Right-click on the record control: jump to any of its six states.
	local function recStatesPanel(): Node
		local entries: { Entry } = { { header = "Preview state" } }
		for _, st in D.rec_states do
			table.insert(entries, {
				label = st[2],
				right = st[3],
				check = function()
					return S.rec:get() == st[1]
				end,
				run = run(function()
					S.preview(st[1])
				end),
			})
		end
		return menu("rec_states_menu", 248, place("record", 248, "end"), entries)
	end

	-- Frame budget, from the Frames legend: sets the red line.
	local function budgetPanel(): Node
		local entries: { Entry } = { { header = "Frame budget" } }
		-- A budget is { ms, label, rate }.
		for _, b in D.budgets do
			table.insert(entries, {
				mono = true,
				label = b[2],
				right = b[3],
				check = function()
					return not S.follow_cap:get() and S.budget:get() == b[1]
				end,
				run = run(function()
					S.follow_cap:set(false)
					S.budget:set(b[1] :: number)
				end),
			})
		end
		table.insert(entries, { sep = true })
		table.insert(
			entries,
			{ icon = "link", label = "Follow frame cap", run = run(function()
				S.follow_cap:toggle()
			end) }
		)
		table.insert(entries, { icon = "edit", label = "Custom…", run = close })
		return menu("budget_menu", 232, place("budget", 232, "start"), entries)
	end

	-- Trace zoom, from the Frame trace header.
	local function zoomPanel(): Node
		local function mode(m: string): () -> boolean
			return function()
				return S.zoom:get().mode == m
			end
		end
		return menu("zoom_menu", 240, place("zoom", 240, "end"), {
			{ header = "Trace zoom" },
			{
				label = "Fit frame",
				check = mode("frame"),
				keys = { "command", "0" },
				run = run(function()
					S.zoom_to("frame")
				end),
			},
			{
				label = "Fit vsync interval",
				check = mode("vsync"),
				run = run(function()
					S.zoom_to("vsync")
				end),
			},
			{
				label = "Fit selection",
				check = mode("selection"),
				keys = { "command", "arrow-up", "0" },
				run = run(function()
					S.zoom_to("selection")
				end),
			},
			{ sep = true },
			{
				icon = "zoom-in",
				label = "Zoom in",
				keys = { "command", "+" },
				run = function()
					S.zoom_by(1.5)
				end,
			},
			{
				icon = "zoom-out",
				label = "Zoom out",
				keys = { "command", "−" },
				run = function()
					S.zoom_by(1 / 1.5)
				end,
			},
			{ sep = true },
			{
				icon = "lock",
				label = "Keep zoom between frames",
				run = function()
					S.keep_zoom:toggle()
				end,
			},
		})
	end

	-- Right-click on a span, at the pointer.
	local function spanPanel(): Node
		local ctx = S.ctx:peek()
		local s = S.span_by_id(ctx.id) or D.spans[1]
		local name = s.title or s.name
		local v = viewport()
		return menu(
			"span_menu",
			252,
			{ left = math.clamp(ctx.x, 8, v.w - 260), top = math.clamp(ctx.y, 8, v.h - 240), origin = { 0, 0 } },
			{
				{ header = string.format("%s · %s · %.2f ms", name, s.kind, s.w / D.px_per_ms) },
				{
					icon = "maximize",
					label = "Zoom to span",
					keys = { "Z" },
					run = run(function()
						S.span:set(s.id)
						S.zoom_to("selection")
					end),
				},
				{
					icon = "search",
					label = "Find all " .. name,
					keys = { "command", "arrow-up", "F" },
					run = function()
						S.query:set(s.name)
						S.menu:set("search")
					end,
				},
				{ icon = "layout", label = "Show Toasts in Areas", run = run(function()
					S.area:set(1)
				end) },
				{ icon = "crosshair", label = "Highlight in app", run = close },
				{ icon = "list", label = "Show in Events", run = run(function()
					S.tab:set(5)
				end) },
				{ sep = true },
				{ icon = "copy", label = "Copy span name", keys = { "command", "C" }, run = close },
			}
		)
	end

	-- Overlays: six toggles, ⌥1–6, and Hide all.
	local function overlaysPanel(): Node
		local rows: { Node }, offsets: { number } = {}, {}
		for i, o in D.overlays do
			offsets[i] = (i - 1) * 46
			local on = function()
				return S.overlays:get()[i]
			end
			rows[i] = ui.row {
				id = "overlay_" .. i,
				w_full = true,
				h = 46,
				px = 12,
				gap = 14,
				items = "center",
				rounded = 8,
				cursor = "pointer",
				on_hover = function(inside)
					if inside then
						hi:set(i)
					end
				end,
				on_click = function()
					S.flip_overlay(i)
				end,
				ui.div {
					w = 14,
					h = 14,
					rounded = 4,
					border = 1,
					border_color = o[3],
					bg = function()
						return on() and o[3] or C.clear
					end,
				},
				ui.column { grow = 1, shrink = 1, min_w = 0, gap = 2, text(o[1], 13, C.text), mono(o[2], 10.5, C.muted) },
				mono("⌥" .. i, 10.5, C.muted),
				ui.div {
					w = 14,
					scale = ui.spring(
						ui.computed(function()
							return on() and 1 or 0.5
						end),
						{ stiffness = 700, damping = 30 }
					),
					opacity = ui.tween(
						ui.computed(function()
							return on() and 1 or 0
						end),
						{ duration = 100 }
					),
					icon("check", 14, C.text),
				},
			}
		end
		keys.count, keys.state, keys.run = #D.overlays, hi, S.flip_overlay
		return panel({
			id = "overlays_menu",
			w = 288,
			ui.row {
				w_full = true,
				px = 16,
				pt = 14,
				pb = 8,
				gap = 8,
				items = "center",
				text("Overlays", 13.5, C.text),
				mono(function()
					return "drawn on " .. D.processes[S.target:get()][1]
				end, 10.5, C.muted),
			},
			ui.overlay {
				w_full = true,
				px = 6,
				pb = 6,
				on_hover = function(inside)
					if not inside then
						hi:set(0)
					end
				end,
				highlight(hi, offsets, 46, 6),
				ui.column { w_full = true, rows },
			},
			foot {
				ui.button {
					id = "overlays_hide_all",
					p = 0,
					bg = C.clear,
					border = 0,
					direction = "row",
					gap = 8,
					items = "center",
					cursor = "pointer",
					color = C.secondary,
					hover = { color = C.text },
					focus = {},
					on_click = function()
						S.overlays:set { false, false, false, false, false, false }
					end,
					ui.text { "Hide all", text_size = 12.5 },
					mono("⌥0", 10.5, C.muted),
				},
				spacer(),
				mono(function()
					local n = 0
					for _, v in S.overlays:get() do
						if v then
							n += 1
						end
					end
					return n .. " on"
				end, 10.5, C.muted),
			},
		}, place("overlays", 288, "start"))
	end

	-- Status bar: Logs & checks.
	local function logRow(id: string, label: string, env: string, value: State<boolean>): Node
		return ui.row {
			w_full = true,
			gap = 10,
			py = 4,
			items = "center",
			ui.column {
				grow = 1,
				gap = 2,
				text(label, 12.5, function()
					return value:get() and C.text or C.secondary
				end),
				mono(env, 10, C.muted),
			},
			K.toggle(id, value),
		}
	end
	local function logsPanel(): Node
		keys.count = 0
		local sw = S.switches
		return panel({
			id = "logs_menu",
			w = 288,
			rounded = 12,
			head("Logs & checks", "stderr"),
			ui.column {
				w_full = true,
				gap = 10,
				pt = 2,
				px = 14,
				pb = 8,
				logRow("log_gpu", "GPU stats log", "ZGX_GPU_STATS", sw.gpu),
				logRow("log_trace", "Action trace", "ZGX_ACTION_TRACE", sw.trace),
				logRow("log_damage", "Damage check", "ZGX_DAMAGE_CHECK", sw.damage),
			},
			foot {
				icon("folder-open", 13, C.tertiary),
				link("open_logs", "Open log folder", C.secondary, close),
				spacer(),
				link("runtime_link", "Runtime settings…", C.tertiary, function()
					S.menu:set("runtime")
				end),
			},
		}, place("logs", 288, "up-end"))
	end

	-- Status bar: Capture buffer.
	local function bufferPanel(): Node
		keys.count = 0
		return panel({
			id = "buffer_menu",
			w = 296,
			rounded = 12,
			head("Capture buffer", "ring buffer"),
			ui.column {
				w_full = true,
				gap = 10,
				pt = 4,
				px = 14,
				pb = 12,
				ui.column {
					w_full = true,
					gap = 6,
					ui.div {
						w_full = true,
						h = 6,
						rounded = 3,
						bg = C.selected,
						ui.div {
							h = 6,
							rounded = 3,
							bg = C.tertiary,
							w = function()
								return 268 * S.buffer_fill()
							end,
						},
					},
					ui.row {
						w_full = true,
						items = "center",
						mono(function()
							return S.buffer_frames_text()
						end, 10, C.text),
						spacer(),
						mono(function()
							return S.buffer_bytes()
						end, 10, C.secondary),
					},
					mono(function()
						return S.buffer_eta()
					end, 10, C.muted),
				},
				ui.row {
					w_full = true,
					h = 32,
					gap = 10,
					items = "center",
					text("Size", 12.5, C.text),
					spacer(),
					dropdown("buffer_size", S.buffer_size, D.buffer_options),
				},
				ui.row {
					w_full = true,
					h = 32,
					gap = 10,
					items = "center",
					text("When full", 12.5, C.text),
					spacer(),
					dropdown("buffer_full", S.when_full, D.full_options),
				},
			},
			foot {
				link("buffer_clear", "Clear buffer", C.coral, function()
					S.clear()
					close()
				end),
				spacer(),
				mono("⌘K", 10, C.muted),
			},
		}, place("buffer", 296, "up"))
	end

	-- Search: filters the frame's spans, areas and nodes.
	local query = S.query
	local kind = ui.state("search_kind", 1)
	local KINDS: { { string? } } = { { "All", nil }, { "Spans", "spans" }, { "Areas", "areas" }, { "Nodes", "nodes" } }
	local DESIGN_COUNTS = { 14, 9, 2, 3 }
	local function matches(k: string?): { Data.SearchResult }
		local q = query:get():lower()
		local out: { Data.SearchResult } = {}
		for _, r in D.search do
			local hay = r.name:lower() .. " " .. r.meta:lower() .. " " .. (r.keywords or "")
			if (k == nil or r.kind == k) and (q == "" or hay:find(q, 1, true)) then
				table.insert(out, r)
			end
		end
		return out
	end
	local results = ui.computed(function()
		return matches(KINDS[kind:get()][2])
	end)
	local function count(i: number): number
		if query:get() == "glyph" then
			return DESIGN_COUNTS[i]
		end
		return #matches(KINDS[i][2])
	end
	local function jump(r: Data.SearchResult?)
		if not r then
			return
		end
		ui.batch(function()
			S.tab:set(1)
			if r.span then
				S.range:set(1)
				S.frame:set(D.selected)
				S.span:set(r.span)
			end
			if r.area then
				S.area:set(r.area)
			end
			close()
		end)
	end
	-- The design's Shortcut · Keycap: outlined, an icon and/or a key.
	local function keycap(glyph: string?, key: string?): Node
		return ui.row {
			h = 20,
			px = 6,
			gap = 3,
			rounded = 4,
			border = 1,
			border_color = C.strong,
			items = "center",
			glyph and icon(glyph, 11.5, C.secondary) or nil,
			key and mono(key, 10.5, C.secondary) or nil,
		}
	end
	local function resultRows(): Node
		local nodes: { Node }, offsets: { number }, y, last: string? = {}, {}, 0, nil
		for i, r in results:peek() do
			if r.kind ~= last then
				last = r.kind
				table.insert(
					nodes,
					ui.row {
						h = 26,
						px = 8,
						items = "center",
						mono(r.kind:upper(), 10, C.muted, { letter_spacing = 0.8 }),
					}
				)
				y += 26
			end
			offsets[i] = y
			local lit = function()
				return hi:get() == i
			end
			table.insert(
				nodes,
				ui.row {
					id = "result_" .. i,
					w_full = true,
					h = 32,
					px = 8,
					gap = 9,
					items = "center",
					rounded = 7,
					cursor = "pointer",
					on_hover = function(inside)
						if inside then
							hi:set(i)
						end
					end,
					on_click = function()
						jump(r)
					end,
					ui.row {
						w = 14,
						h = 14,
						items = "center",
						justify = "center",
						r.swatch and ui.div { w = 8, h = 8, rounded = 2, bg = r.swatch }
							or icon(r.icon or "", 14, C.tertiary),
					},
					r.mono and mono(r.name, 11.5, C.text) or text(r.name, 12.5, C.text, {
						weight = function()
							return lit() and 500 or 400
						end,
					}),
					mono(r.meta, 10, C.muted),
					spacer(),
					mono(r.value, 11, function()
						return lit() and C.text or C.secondary
					end),
					ui.div {
						opacity = ui.tween(
							ui.computed(function()
								return lit() and 1 or 0
							end),
							{ duration = 100 }
						),
						keycap("enter"),
					},
				}
			)
			y += 32
		end
		if #nodes == 0 then
			table.insert(
				nodes,
				ui.row {
					h = 64,
					w_full = true,
					justify = "center",
					items = "center",
					text("Nothing matches “" .. query:peek() .. "”", 12.5, C.muted),
				}
			)
		end
		return ui.overlay {
			w_full = true,
			px = 6,
			pb = 6,
			highlight(hi, offsets, 32, 6),
			ui.column { w_full = true, nodes },
		}
	end
	local function searchPanel(): Node
		keys.count = 0
		local v = viewport()
		local function move(d: number)
			local n = #results:peek()
			if n > 0 then
				hi:set((math.max(hi:peek(), 1) - 1 + d) % n + 1)
			end
		end
		ui.effect(function()
			results:get()
			hi:set(#results:peek() > 0 and 1 or 0)
		end)
		local function hint(glyph: string, label: string): Node
			return ui.row { gap = 6, items = "center", keycap(glyph), text(label, 11.5, C.tertiary) }
		end
		return panel({
			id = "search_menu",
			w = 520,
			rounded = 12,
			keep_focus = true,
			ui.row {
				w_full = true,
				h = 46,
				pl = 14,
				pr = 12,
				gap = 10,
				items = "center",
				border = 1,
				border_sides = { 0, 0, 1, 0 },
				border_color = C.subtle,
				icon("search", 15, C.muted),
				ui.overlay {
					grow = 1,
					shrink = 1,
					min_w = 0,
					h = 22,
					items = "center",
					ui.text {
						"Search spans, areas and nodes",
						absolute = true,
						left = 0,
						top = 3,
						text_size = 14,
						color = C.muted,
						hidden = function()
							return query:get() ~= ""
						end,
					},
					ui.input {
						id = "search_input",
						label = "Search",
						value = query,
						w_full = true,
						h = 22,
						p = 0,
						border = 0,
						bg = C.clear,
						text_size = 14,
						color = C.text,
						focus = { border_color = C.clear },
						on_mount = function(el)
							el:focus()
						end,
						on_key = function(e)
							if e.key == "ArrowDown" then
								move(1)
								return true
							end
							if e.key == "ArrowUp" then
								move(-1)
								return true
							end
							if e.key == "Enter" then
								jump(results:peek()[math.max(hi:peek(), 1)])
								return true
							end
							if e.key == "Escape" then
								close()
								return true
							end
							return false
						end,
					},
				},
				keycap(nil, "Esc"),
			},
			ui.row {
				w_full = true,
				px = 10,
				py = 8,
				K.segmented("search_kind", kind, 4, function(i: number, active: () -> boolean): Child
					return {
						ui.text {
							KINDS[i][1] or "",
							text_size = 12,
							color = function()
								return active() and C.text or C.tertiary
							end,
						},
						mono(function()
							return tostring(count(i))
						end, 10, C.muted),
					}
				end, { h = 24, px = 9, r = 6, p = 0, gap = 6, spacing = 4 }),
			},
			ui.column {
				w_full = true,
				layout = "height",
				layout_clip = true,
				ui.presence(
					function()
						return kind:get() .. ":" .. query:get()
					end,
					resultRows,
					{ enter = { opacity = { 0, 1 }, duration = 120 }, exit = { opacity = 0, duration = 60 } }
				),
			},
			ui.row {
				w_full = true,
				h = 36,
				px = 14,
				gap = 14,
				items = "center",
				border = 1,
				border_sides = { 1, 0, 0, 0 },
				border_color = C.subtle,
				hint("arrow-up-down", "Navigate"),
				hint("enter", "Jump to span"),
				hint("arrow-right-to-line", "Filter timeline"),
				spacer(),
				mono(function()
					local n = query:get() == "glyph" and 14 or #results:get()
					return n .. (n == 1 and " result" or " results")
				end, 10, C.muted),
			},
		}, {
			left = math.floor((v.w - 520) / 2),
			top = math.max(56, math.floor((v.h - 420) * 0.3)),
			origin = { 0.5, 0 },
		})
	end

	-- Tooltips: a dark callout under (or over) its trigger, after 500 ms, or
	-- at once while another has just been showing. It slides out of its
	-- trigger, straight down (or up).
	local TIPS: { [string]: () -> TipSpec } = {
		record = function()
			local r = S.rec:get()
			if r == "recording" then
				return { label = "Stop recording", glyph = "command", keys = "R" }
			end
			if r == "paused" then
				return { label = "Resume recording", glyph = "command", keys = "R" }
			end
			if r == "capturing" or r == "countdown" or r == "armed" then
				return { label = "Cancel", keys = "esc" }
			end
			return { label = "Start recording", glyph = "command", keys = "R" }
		end,
		search = function()
			return { label = "Search", glyph = "command", keys = "F" }
		end,
		more = function()
			return { label = "More actions" }
		end,
		overlays = function()
			return { label = "Overlays", glyph = "option", keys = "1–6" }
		end,
		buffer = function()
			return {
				label = "Capture buffer",
				detail = string.format("%d%% full · %s", math.floor(S.buffer_fill() * 100 + 0.5), S.buffer_drop()),
				up = true,
			}
		end,
	}
	local warm = false
	local timer: Animation?, cool: Animation? = nil, nil
	function S.hint_hover(name: string, inside: boolean)
		if timer then
			timer:cancel()
			timer = nil
		end
		if inside then
			if S.menu:peek() ~= "" then
				return
			end
			if warm or S.hint:peek() ~= "" then
				S.hint:set(name)
			else
				timer = anime.createTimer {
					duration = 500,
					onComplete = function()
						timer = nil
						S.hint:set(name)
					end,
				}
			end
		elseif S.hint:peek() == name then
			S.hint:set("")
			warm = true
			if cool then
				cool:cancel()
			end
			cool = anime.createTimer {
				duration = 300,
				onComplete = function()
					warm = false
				end,
			}
		end
	end
	-- The design's tooltip: a borderless dark body 4 points from its trigger.
	-- Its content follows its spec (the record control's changes as it
	-- changes state), and it stays centred on a trigger that resizes.
	local function tooltip(name: string): Node
		local spec = ui.computed(TIPS[name])
		local first = TIPS[name]()
		local up = first.up
		local w = 300
		local function centre(): number
			S.anchor_tick:get()
			local a, v = anchorBox(name), viewport()
			return math.clamp(a.x + a.w / 2 - w / 2, 4 - w / 2 + 60, v.w - w / 2 - 64)
		end
		local a, v = anchorBox(name), viewport()
		local function glyph(g: string): Node
			return icon(g, 11.5, C.secondary, {
				hidden = function()
					return spec:get().glyph ~= g
				end,
			})
		end
		local body = first.detail
				and ui.column {
					py = 7,
					px = 10,
					gap = 3,
					bg = C.tip,
					rounded = 7,
					shadow = { y = 6, blur = 16, color = "#00000080" },
					text(function()
						return spec:get().label
					end, 11.5, C.text),
					mono(function()
						return spec:get().detail or ""
					end, 10, C.tertiary),
				}
			or ui.row {
				h = 28,
				pl = 9,
				pr = 5,
				gap = 8,
				items = "center",
				bg = C.tip,
				rounded = 7,
				shadow = { y = 6, blur = 16, color = "#00000080" },
				text(function()
					return spec:get().label
				end, 11.5, C.text),
				ui.row {
					h = 20,
					px = 6,
					gap = 3,
					rounded = 4,
					border = 1,
					border_color = C.strong,
					items = "center",
					hidden = function()
						return not spec:get().keys
					end,
					glyph("command"),
					glyph("option"),
					mono(function()
						return spec:get().keys or ""
					end, 10.5, C.secondary),
				},
			}
		return ui.column {
			id = "tooltip_" .. name,
			absolute = true,
			w = w,
			items = "center",
			pointer_events = "none",
			opacity = 0,
			left = centre,
			top = not up and a.y + a.h + 4 or nil,
			bottom = up and v.h - a.y + 4 or nil,
			on_mount = function(el: Element)
				anime.animate(el, { opacity = 1, y = { up and 4 or -4, 0 }, duration = 160, ease = "outQuad" })
			end,
			body,
		}
	end

	local BUILD: { [string]: () -> Node } = {
		target = targetPanel,
		more = morePanel,
		overlays = overlaysPanel,
		runtime = runtimePanel,
		search = searchPanel,
		capture = capturePanel,
		rec_states = recStatesPanel,
		budget = budgetPanel,
		zoom = zoomPanel,
		span = spanPanel,
		logs = logsPanel,
		buffer = bufferPanel,
	}
	-- Fills the window: the layer and its parts, and its presences' boxes.
	local function fill(props: ContainerProps): ContainerProps
		props.absolute, props.top, props.left, props.right, props.bottom = true, 0, 0, 0, 0
		return props
	end
	local function fillOptions(opts: PresenceOptions): PresenceOptions
		opts.absolute, opts.top, opts.left, opts.right, opts.bottom = true, 0, 0, 0, 0
		return opts
	end
	-- Presences fade out what leaves; what enters animates itself as it
	-- mounts, from its own corner (a presence's wrapper fills the window,
	-- so scaling it would drift the panel toward the window's centre).
	M.layer = ui.overlay(fill {
		id = "menu_layer",
		pointer_events = "none",
		ui.div(fill {
			id = "menu_dim",
			bg = "#00000059",
			pointer_events = "none",
			opacity = ui.tween(
				ui.computed(function()
					return S.menu:get() == "search" and 1 or 0
				end),
				{ duration = 160 }
			),
		}),
		ui.div(fill {
			id = "menu_catcher",
			hidden = function()
				return S.menu:get() == ""
			end,
			on_press = close,
		}),
		ui.presence(function()
			local m = S.menu:get()
			return m ~= "" and m or nil
		end, function(m: string)
			return BUILD[m]()
		end, fillOptions { pointer_events = "none", exit = { opacity = 0, duration = 120, ease = "inQuad" } }),
		ui.div(fill {
			id = "dropdown_catcher",
			hidden = function()
				return drop:get() == ""
			end,
			on_press = function()
				drop:set("")
			end,
		}),
		ui.presence(function()
			local d = drop:get()
			return d ~= "" and d or nil
		end, dropPanel, fillOptions { pointer_events = "none", exit = { opacity = 0, duration = 90, ease = "inQuad" } }),
		ui.presence(function()
			local h = S.hint:get()
			return h ~= "" and h or nil
		end, tooltip, fillOptions { pointer_events = "none", exit = { opacity = 0, duration = 80 } }),
	})

	function M.on_key(e: KeyEvent): boolean
		local m = S.menu:peek()
		local d = drop:peek()
		if d ~= "" then
			local n = #DROPS[d].options
			if e.key == "Escape" then
				drop:set("")
				return true
			end
			if e.key == "ArrowDown" or e.key == "ArrowUp" then
				dropHi:set((math.max(dropHi:peek(), 1) - 1 + (e.key == "ArrowDown" and 1 or -1)) % n + 1)
				return true
			end
			if e.key == "Enter" or e.key == " " then
				if dropHi:peek() > 0 then
					DROPS[d].state:set(dropHi:peek())
				end
				drop:set("")
				return true
			end
			return false
		end
		if m == "" or m == "search" then
			return false
		end
		if e.key == "Escape" then
			if sub:peek() then
				sub:set(false)
			else
				close()
			end
			return true
		end
		local state: State<number>?, count: number? = keys.state, keys.count
		if sub:peek() then
			state, count = subHi, keys.sub_count
		end
		if not count or count == 0 then
			return false
		end
		if not state then
			return false
		end
		if e.key == "ArrowDown" or e.key == "ArrowUp" then
			local dir = e.key == "ArrowDown" and 1 or -1
			local cur = state:peek()
			state:set(cur == 0 and (dir == 1 and 1 or count) or (cur - 1 + dir) % count + 1)
			return true
		end
		if e.key == "ArrowRight" and m == "more" and not sub:peek() and hi:peek() == keys.sub then
			sub:set(true)
			subHi:set(1)
			return true
		end
		if e.key == "ArrowLeft" and sub:peek() then
			sub:set(false)
			return true
		end
		if e.key == "Enter" or e.key == " " then
			local i = state:peek()
			if i > 0 then
				if sub:peek() then
					if keys.sub_run then
						keys.sub_run(i)
					end
				elseif keys.run then
					keys.run(i)
				end
			end
			return true
		end
		return false
	end

	return M
end
