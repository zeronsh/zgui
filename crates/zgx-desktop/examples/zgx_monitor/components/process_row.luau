-- A sidebar row: a process's name, pid and memory, its CPU with a meter.
-- A click shows it in the active panel, + opens it in a panel beside, and
-- it drags onto the panels. Rows are by position (the list is virtual): a
-- row shows whichever process is at its index, following each sample.
local theme = require("../theme")
local icons = require("../icons")
local state = require("../state")
local data = require("../data")
local actions = require("../actions")
local controls = require("./controls")
local format = require("@lib/format")

-- The CPU meter's width; a busy core fills it.
local METER = 44

return function(index: number): Node
	local function proc(): data.Proc?
		data.version:get()
		return data.rows[index]
	end
	local function now(): data.Proc?
		return data.rows[index]
	end
	local hovered = ui.use_state(false)
	-- The active panel's process.
	local function selected(): boolean
		local p = proc()
		return p ~= nil and (state.pids:get())[state.active:get()] == p.pid
	end
	-- Shown in some panel.
	local function open(): boolean
		local p = proc()
		if not p then
			return false
		end
		for _, pid in state.pids:get() do
			if pid == p.pid then
				return true
			end
		end
		return false
	end
	-- Where the meter's fill sits for a process's load.
	local function fill(p: data.Proc?): number
		return -METER * (1 - (if p then math.min(p.cpu / 100, 1) else 0))
	end
	-- The meter eases to each sample's load, at the samples' rate.
	local meter: Element? = nil
	local shown = 0
	ui.effect(function()
		local to = fill(proc())
		local el = meter
		if el and math.abs(to - shown) >= 0.5 then
			shown = to
			anime.animate(el, { x = to, duration = 400, ease = "outCubic", frameRate = theme.sample_fps })
		end
	end)
	return ui.div {
		w_full = true,
		h = theme.row,
		px = 8,
		py = 1,
		ui.row {
			relative = true,
			w_full = true,
			h_full = true,
			pl = 10,
			pr = 6,
			gap = 9,
			items = "center",
			rounded = 9,
			hover = { bg = theme.hover },
			on_hover = function(inside)
				hovered:set(inside)
			end,
			on_click = function()
				local p = now()
				if p then
					data.hold()
					actions.show(p.pid)
				end
			end,
			on_drag = function(): state.Payload?
				local p = now()
				if not p then
					return nil
				end
				state.dragging:set { kind = "process", pid = p.pid }
				return { kind = "process", pid = p.pid }
			end,
			drag_preview = function(payload: state.Payload)
				return controls.chip(payload.pid)
			end,
			on_drag_end = function()
				state.dragging:set {}
				state.drop:set {}
			end,
			-- The active panel's process: a highlight that slides from row to
			-- row as the selection moves.
			ui.show(
				selected,
				ui.div {
					absolute = true,
					left = 0,
					top = 0,
					w_full = true,
					h_full = true,
					rounded = 9,
					bg = "#ffffff12",
					border = 1,
					border_color = "#ffffff12",
					layout_id = "selected-process",
					layout_duration = 220,
					layout_ease = "outQuint",
				}
			),
			-- Shown in a panel.
			ui.div {
				w = 5,
				h = 5,
				rounded = 3,
				shrink = 0,
				bg = theme.accent,
				opacity = function()
					return if open() then 1 else 0
				end,
			},
			ui.column {
				grow = 1,
				shrink = 1,
				min_w = 0,
				gap = 3,
				ui.text {
					function()
						local p = proc()
						return if p then p.name else ""
					end,
					truncate = true,
					text_size = 12.5,
					weight = 500,
					color = theme.text,
				},
				ui.text {
					function()
						local p = proc()
						if not p then
							return ""
						end
						local gpu = if p.gpu >= 0.05 then ` · GPU {format.percent(p.gpu)}` else ""
						local tag = if p.pid == state.launched then " · launched" else ""
						return `{p.pid} · {format.memory(p.mem)}{gpu}{tag}`
					end,
					truncate = true,
					text_size = 10.5,
					tabular = true,
					color = theme.tertiary,
				},
			},
			ui.column {
				shrink = 0,
				items = "end",
				gap = 5,
				ui.text {
					function()
						local p = proc()
						return if p then format.percent(p.cpu) else ""
					end,
					text_size = 12,
					tabular = true,
					color = theme.secondary,
				},
				ui.div {
					w = METER,
					h = 3,
					rounded = 2,
					overflow = "hidden",
					bg = "#ffffff14",
					ui.div {
						w = METER,
						h = 3,
						rounded = 2,
						bg = "$monitor.cpu",
						x = -METER,
						on_mount = function(el)
							meter = el
							shown = fill(now())
							anime.utils.set(el, { x = shown })
						end,
					},
				},
			},
			-- Open beside the active panel.
			ui.row {
				shrink = 0,
				opacity = ui.tween(hovered, { duration = 120 }),
				controls.tool(icons.plus, function()
					local p = now()
					if p then
						actions.split("row", p.pid)
					end
				end, 22, 13),
			},
		},
	}
end
