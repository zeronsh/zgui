-- What a panel shows of its process: three tiles (now, and the minute's
-- average or peak) over a chart per series, the charts sharing the height
-- the panel has. A panel too small for them scrolls.
local theme = require("../theme")
local data = require("../data")
local format = require("@lib/format")
local Chart = require("./chart")

-- The tiles' row and the space around and between the parts, for sharing
-- out the rest: a chart's own title and time labels take 43.
local PAD, GAP, TILES, CHROME = 12, 14, 76, 43
local MIN_PLOT, MAX_PLOT = 54, 280

type Stats = { cpu: number?, cpu_avg: number?, gpu: number?, gpu_avg: number?, mem: number?, mem_peak: number? }

local function stats(h: data.History?): Stats
	if not h or #h.cpu == 0 then
		return {}
	end
	local function sum(s: data.Series): number
		local total = 0
		for _, v in s do
			total += v
		end
		return total
	end
	local peak = 0
	for _, v in h.mem do
		peak = math.max(peak, v)
	end
	local n = #h.cpu
	return {
		cpu = h.cpu[n],
		cpu_avg = sum(h.cpu) / n,
		gpu = h.gpu[n],
		gpu_avg = sum(h.gpu) / n,
		mem = h.mem[n],
		mem_peak = peak,
	}
end

local function tile(hue: string, label: string, value: () -> string, detail: () -> string): Node
	return ui.column {
		grow = 1,
		basis = 112,
		shrink = 1,
		min_w = 0,
		h = TILES,
		px = 12,
		py = 10,
		gap = 3,
		rounded = 10,
		bg = theme.wash,
		border = 1,
		border_color = theme.line,
		ui.row {
			gap = 6,
			items = "center",
			ui.div { w = 6, h = 6, rounded = 3, bg = "$" .. hue },
			ui.text { label, text_size = 10, weight = 600, letter_spacing = 1.2, color = theme.tertiary },
		},
		ui.text { value, text_size = 19, weight = 500, tabular = true, truncate = true, color = theme.text },
		ui.text { detail, text_size = 10.5, tabular = true, truncate = true, color = theme.tertiary },
	}
end

return function(pid: number, size: State<{ w: number, h: number }>, header: number): Node
	local history = data.history(pid)
	local hover = ui.use_state(nil :: number?)
	local now = ui.computed(function()
		return stats(history:get())
	end)
	local function shown(key: string, metric: format.Metric): () -> string
		return function()
			local v = (now:get() :: any)[key]
			return if v then format.value(metric, v) else "—"
		end
	end
	local function plot_h(): number
		local room = size:get().h - header - 2 * PAD - TILES - 3 * GAP
		return math.clamp(math.floor(room / 3 - CHROME), MIN_PLOT, MAX_PLOT)
	end
	local function chart(metric: format.Metric, title: string): Node
		return Chart { metric = metric, title = title, history = history, hover = hover, height = plot_h }
	end
	return ui.scroll {
		w_full = true,
		h_full = true,
		p = PAD,
		gap = GAP,
		ui.row {
			w_full = true,
			gap = 8,
			flex_wrap = true,
			tile(theme.hues.cpu, "CPU", shown("cpu", "cpu"), function()
				local avg = (now:get()).cpu_avg
				return if avg then `avg {format.percent(avg)}` else ""
			end),
			tile(theme.hues.gpu, "GPU", shown("gpu", "gpu"), function()
				local avg = (now:get()).gpu_avg
				return if avg then `avg {format.percent(avg)}` else ""
			end),
			tile(theme.hues.mem, "MEMORY", shown("mem", "mem"), function()
				local peak = (now:get()).mem_peak
				return if peak then `peak {format.memory(peak)}` else ""
			end),
		},
		chart("cpu", "CPU"),
		chart("gpu", "GPU"),
		chart("mem", "MEMORY"),
	}
end
