-- One series of a process's last minute: a line over a soft fill, an axis
-- that eases to fit it, and a readout of the newest sample or the one under
-- the pointer (the panel's charts share where the pointer is).
--
-- Each sample slides the plot one step left over the interval between
-- samples, so the line moves steadily instead of jumping: the canvas is a
-- step wider than its box, drawn with the newest sample a step past the
-- right edge, and eased from where the last sample left it. The slide is a
-- translation, on the compositor; the line is drawn once a sample.
local theme = require("../theme")
local state = require("../state")
local data = require("../data")
local format = require("@lib/format")

-- Room for the marker at the plot's edges.
local INSET = 6

-- A metric's series in a sample history, and its hue.
local function series_of(history: data.History, metric: format.Metric): data.Series
	return if metric == "cpu" then history.cpu elseif metric == "gpu" then history.gpu else history.mem
end
local function hue_of(metric: format.Metric): string
	local hues = theme.hues
	return if metric == "cpu" then hues.cpu elseif metric == "gpu" then hues.gpu else hues.mem
end

export type Props = {
	metric: format.Metric,
	title: string,
	history: State<data.History?>,
	-- The sample under the pointer, by index, for every chart in the panel.
	hover: State<number?>,
	height: () -> number,
}

return ui.component(function(props: Props)
	local metric, history, hover = props.metric, props.history, props.hover
	local hue = hue_of(metric)
	local floor = if metric == "mem" then 64 else 10
	local capacity = state.capacity
	local width = ui.use_state(0)
	-- The axis's top, eased to each new ceiling.
	local top = ui.use_state(0)
	local canvas: Element? = nil

	local function series(): data.Series
		local h = history:get()
		return if h then series_of(h, metric) else {}
	end
	local function step(w: number): number
		return math.max(w - 2 * INSET, 1) / (capacity - 1)
	end

	local ceiling = 0
	ui.effect(function()
		local s = series()
		local peak = 0
		for _, v in s do
			peak = math.max(peak, v)
		end
		local target = format.nice_max(peak, floor)
		if ceiling == 0 then
			top:set(target)
		elseif target ~= ceiling then
			anime.animate(top, { value = target, duration = 450, ease = "outQuint" })
		end
		ceiling = target
		local el = canvas
		if el and #s > 1 then
			anime.animate(el, {
				x = { 0, -step(width:peek()) },
				duration = state.interval,
				ease = "linear",
				frameRate = theme.sample_fps,
			})
		end
	end)

	local function color(alpha: string): string
		return string.sub(ui.theme.get(hue) :: string, 1, 7) .. alpha
	end

	local function draw(c: Canvas, w: number, h: number)
		local plot_w = width:get()
		local st = step(plot_w)
		local plot_h = h - 2 * INSET
		-- Recessive grid at 0, ½ and the axis's top.
		for _, f in { 0, 0.5, 1 } do
			c:rect(0, INSET + plot_h * (1 - f) - 0.5, w, 1, "#ffffff10")
		end
		local s = series()
		local n = #s
		if n < 2 then
			return
		end
		local t = math.max(top:get(), 1e-6)
		local right = plot_w - INSET + st
		local function x(k: number): number
			return right - st * (n - k)
		end
		local function y(v: number): number
			return INSET + plot_h * (1 - math.clamp(v / t, 0, 1))
		end
		local bottom = INSET + plot_h
		local line = ui.path():move(x(1), y(s[1]))
		local area = ui.path():move(x(1), bottom):line(x(1), y(s[1]))
		for k = 2, n do
			line = line:line(x(k), y(s[k]))
			area = area:line(x(k), y(s[k]))
		end
		area = area:line(x(n), bottom):close()
		c:fill(area, ui.linear(0, INSET, 0, bottom, { color("47"), color("00") }))
		c:stroke(line, color("ff"), 1.75, { cap = "round", join = "round" })
		-- A crosshair at the hovered sample; a ringed marker there, or at the
		-- newest.
		local hovered = hover:get()
		local k = if hovered then math.clamp(hovered, 1, n) else n
		if hovered then
			c:rect(x(k) - 0.5, 0, 1, h, "#ffffff38")
		end
		c:circle(x(k), y(s[k]), 5.5, "#17181d")
		c:circle(x(k), y(s[k]), 3.5, color("ff"))
	end

	local function readout(): string
		local s = series()
		local n = #s
		if n == 0 then
			return "—"
		end
		local hovered = hover:get()
		local k = if hovered then math.clamp(hovered, 1, n) else n
		local ago = (n - k) * state.interval / 1000
		local when = if ago == 0 then "now" else string.format("%.1f s ago", ago)
		return `{format.value(metric, s[k])}  ·  {when}`
	end

	local function label(fraction: number): Node
		return ui.text {
			function()
				return format.axis(metric, top:get() * fraction)
			end,
			text_size = 10,
			tabular = true,
			color = theme.tertiary,
		}
	end

	return ui.column {
		w_full = true,
		gap = 6,
		ui.row {
			w_full = true,
			items = "center",
			gap = 7,
			ui.div { w = 6, h = 6, rounded = 3, bg = "$" .. hue, shrink = 0 },
			ui.text {
				props.title,
				text_size = 10.5,
				weight = 600,
				letter_spacing = 1.2,
				color = theme.tertiary,
			},
			ui.div { grow = 1 },
			ui.text { readout, text_size = 11.5, tabular = true, color = theme.secondary },
		},
		ui.row {
			w_full = true,
			gap = 8,
			ui.column {
				w = 30,
				shrink = 0,
				h = props.height,
				py = INSET - 6,
				items = "end",
				justify = "between",
				label(1),
				label(0.5),
				label(0),
			},
			-- The plot's box clips the wider canvas sliding inside it.
			ui.div {
				grow = 1,
				min_w = 0,
				h = props.height,
				relative = true,
				overflow = "hidden",
				on_resize = function(size)
					width:set(size.w)
					local el = canvas
					if el then
						anime.utils.set(el, { x = -step(size.w) })
					end
				end,
				on_pointer_move = function(e)
					local n = #series()
					if n == 0 then
						return
					end
					local w = width:peek()
					local k = n - math.floor((w - INSET - e.local_x) / step(w) + 0.5)
					k = math.clamp(k, 1, n)
					if hover:peek() ~= k then
						hover:set(k)
					end
				end,
				on_hover = function(inside)
					if not inside then
						hover:set(nil)
					end
				end,
				ui.canvas {
					absolute = true,
					left = 0,
					top = 0,
					w = function()
						local w = width:get()
						return w + step(w)
					end,
					h = props.height,
					on_mount = function(el)
						canvas = el
						anime.utils.set(el, { x = -step(width:peek()) })
					end,
					draw = draw,
				},
			},
		},
		ui.row {
			w_full = true,
			pl = 38,
			justify = "between",
			ui.text { "60 s", text_size = 10, color = theme.quaternary },
			ui.text { "30 s", text_size = 10, color = theme.quaternary },
			ui.text { "now", text_size = 10, color = theme.quaternary },
		},
	}
end)
