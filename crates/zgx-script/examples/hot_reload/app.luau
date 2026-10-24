-- Describes the window once per save. Layout, drawing, state and animation
-- run in Rust; this file only runs again for clicks and function properties.

-- Shared state is visible to plugins; ui.state is private to this script.
local count = ui.shared("count", 0)
local open = ui.state("open", true)
local todos = ui.state("todos", { "Edit app.luau while this runs", "Drop a plugin into plugins/" })
local draft = ui.state("draft", "")

local page = "#0d1117"
local surface = "#161b22"
local raised = "#212833"
local accent = "#5b8def"
local muted = "#8b95a5"

-- `extra` overrides the defaults (a primary button's fill, a compact row's padding).
type ButtonStyle = { bg: Color?, hover: Styles?, py: number? }
local function button(label: string | () -> string, on_click: Handler | Action, extra: ButtonStyle?)
	local style: ButtonStyle = extra or {}
	return ui.button {
		px = 14,
		py = style.py or 8,
		rounded = 8,
		bg = style.bg or raised,
		cursor = "pointer",
		hover = style.hover or { bg = "#2b3442" },
		active = { bg = "#344052" },
		on_click = on_click,
		label,
	}
end

local function card(title: string, ...: Child)
	return ui.column {
		-- Each entry as wide as it is, not the card's width.
		items = "start",
		p = 16,
		gap = 12,
		rounded = 12,
		bg = surface,
		ui.text { title, text_size = 12, bold = true, color = muted },
		{ ... },
	}
end

local function add_todo()
	local text = draft:get()
	if text ~= "" then
		todos:push(text)
		draft:set("")
	end
end

return ui.row {
	w_full = true,
	h_full = true,
	items = "stretch",
	bg = page,
	color = "#e6edf3",

	ui.column {
		w = 230,
		p = 16,
		gap = 12,
		bg = surface,
		ui.text { "PLUGINS", text_size = 11, bold = true, color = muted },
		ui.slot("sidebar", { gap = 10 }),
	},

	ui.scroll {
		grow = 1,
		shrink = 1,
		min_w = 0,
		min_h = 0,
		h_full = true,
		ui.column {
			w_full = true,
			p = 24,
			gap = 16,
			items = "stretch",
			ui.row {
				gap = 10,
				items = "center",
				ui.div {
					w = 8,
					h = 8,
					rounded = 4,
					bg = "#3fb950",
					opacity = ui.loop { duration = 1200, from = 0.3, to = 1, fps = 30 },
				},
				ui.text { "Hello from Luau", text_size = 24, bold = true },
			},
			ui.text { "Window {window} · save app.luau to reload", color = muted, text_size = 13 },

			card(
				"COUNTER",
				ui.row {
					gap = 10,
					items = "center",
					button("−", ui.add(count, -1)),
					ui.text { "{count}", text_size = 20, w = 56, align = "center" },
					button("+", ui.add(count)),
					button("Reset", ui.set(count, 0)),
				}
			),

			card(
				"ANIMATION",
				button(function()
					return if open:get() then "Collapse" else "Expand"
				end, ui.toggle(open)),
				ui.div {
					h = 64,
					p = 16,
					rounded = 10,
					bg = ui.tween(open, { from = raised, to = "#24406e", duration = 350 }),
					opacity = ui.tween(open, { from = 0.4, to = 1, duration = 350 }),
					x = ui.spring(open, { from = 0, to = 16, stiffness = 220, damping = 18 }),
					mr = 16,
					ui.text("Colour and opacity tween, offset springs. Rust runs every frame."),
				}
			),

			card(
				"TODOS",
				ui.row {
					gap = 8,
					ui.input { label = "New todo", value = draft, w = 300, px = 10, py = 8, rounded = 8, bg = raised },
					button("Add", add_todo, { bg = accent, hover = { bg = "#6f9cf2" } }),
				},
				ui.each(todos, function(item, index)
					return ui.row {
						gap = 10,
						items = "center",
						px = 12,
						py = 8,
						rounded = 8,
						bg = raised,
						ui.text { item, grow = 1 },
						button("Done", function()
							todos:remove(index)
						end, { py = 4, bg = "#2b3442", hover = { bg = "#344052" } }),
					}
				end, { gap = 6 })
			),
		},
	},
}
