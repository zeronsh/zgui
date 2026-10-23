-- The selection's properties: position, size, corners, fill and stroke,
-- each field applying when Enter is pressed or it is left.
local state = require("../state")
local actions = require("../actions")
local geometry = require("@lib/geometry")

type Shape = geometry.Shape

local STROKES = { "#00000000", "#202124", "#e8eaed", "#4c8dff", "#f4511e" }

local function shape(id: string): Shape?
	for _, s in state.shapes:get() do
		if s.id == id then
			return s
		end
	end
	return nil
end

local function shown(n: number): string
	return ui.number(n, { decimals = if n % 1 == 0 then 0 else 1, grouping = false })
end

-- A number field for `key` of shape `id`.
local function number(id: string, key: string, label: string, min: number?): Node
	return ui.row {
		gap = 6,
		items = "center",
		ui.text { label, w = 16, color = "$text.secondary", text_size = "$font.size.sm" },
		ui.input {
			id = `field-{key}`,
			label = label,
			w_full = true,
			shrink = 1,
			value = function()
				local s = shape(id)
				return if s then shown((s :: any)[key]) else ""
			end,
			on_commit = function(text: string)
				local n = tonumber(text)
				if n then
					actions.set_field(id, key, if min then math.max(n, min) else n)
				end
			end,
		},
	}
end

local function swatches(id: string, key: "fill" | "stroke", colors: { string }): Node
	local row: { Child } = {}
	for _, color in colors do
		table.insert(
			row,
			ui.button {
				id = `{key}-{color}`,
				label = color,
				w = 22,
				h = 22,
				p = 0,
				rounded = 11,
				bg = color,
				border = function()
					local s = shape(id)
					return if s and (s :: any)[key] == color then 2 else 1
				end,
				border_color = function()
					local s = shape(id)
					return if s and (s :: any)[key] == color then "$accent" else "$border"
				end,
				on_click = function()
					actions.set_field(id, key, color)
				end,
			}
		)
	end
	return ui.row { gap = 6, wrap = true, row }
end

local function section(title: string, ...: Child): Node
	return ui.column {
		gap = 8,
		ui.text { title, bold = true, text_size = "$font.size.sm", color = "$text.secondary" },
		...,
	}
end

local function one(id: string): Node
	return ui.column {
		gap = 16,
		ui.input {
			id = "field-name",
			label = "Name",
			value = function()
				local s = shape(id)
				return if s then s.name else ""
			end,
			on_commit = function(text: string)
				actions.set_field(id, "name", text)
			end,
		},
		section(
			"Position and size",
			ui.grid {
				cols = 2,
				gap = 8,
				number(id, "x", "X"),
				number(id, "y", "Y"),
				number(id, "w", "W", 1),
				number(id, "h", "H", 1),
			}
		),
		ui.show(function()
			local s = shape(id)
			return s ~= nil and s.kind == "rect"
		end, section("Corners", number(id, "radius", "R", 0))),
		section(
			"Fill",
			swatches(id, "fill", actions.FILLS),
			ui.input {
				id = "field-fill",
				label = "Fill",
				value = function()
					local s = shape(id)
					return if s then s.fill else ""
				end,
				on_commit = function(text: string)
					if string.match(text, "^#%x%x%x%x%x%x$") or string.match(text, "^#%x%x%x%x%x%x%x%x$") then
						actions.set_field(id, "fill", text)
					end
				end,
			}
		),
		section("Stroke", swatches(id, "stroke", STROKES), number(id, "stroke_width", "W", 0)),
		ui.row {
			gap = 8,
			ui.button { id = "duplicate", "Duplicate", on_click = actions.duplicate },
			ui.button { id = "delete", "Delete", on_click = actions.delete_selected },
		},
	}
end

local function many(): Node
	return ui.column {
		gap = 12,
		ui.text {
			function()
				return `{#state.selected:get()} shapes`
			end,
			bold = true,
		},
		ui.row {
			gap = 8,
			ui.button {
				"Bring to Front",
				on_click = function()
					actions.arrange(true)
				end,
			},
			ui.button {
				"Send to Back",
				on_click = function()
					actions.arrange(false)
				end,
			},
		},
		ui.row {
			gap = 8,
			ui.button { "Duplicate", on_click = actions.duplicate },
			ui.button { "Delete", on_click = actions.delete_selected },
		},
	}
end

local function none(): Node
	local tips = {
		{ "V", "Select and move" },
		{ "R", "Rectangle" },
		{ "O", "Ellipse" },
		{ "Space", "Hold and drag to pan" },
		{ "⌘ + wheel", "Zoom" },
		{ "Shift", "Square shapes, add to selection" },
		{ "⌘Z", "Undo" },
	}
	local rows: { Child } = {}
	for _, tip in tips do
		table.insert(
			rows,
			ui.row {
				gap = 8,
				ui.text { tip[1], w = 72, font = "mono", text_size = "$font.size.sm" },
				ui.text { tip[2], color = "$text.secondary", text_size = "$font.size.sm" },
			}
		)
	end
	return ui.column {
		gap = 12,
		ui.text { "Nothing selected", bold = true },
		ui.text {
			function()
				local n = #state.shapes:get()
				return if n == 1 then "1 shape" else `{n} shapes`
			end,
			color = "$text.secondary",
		},
		ui.column { gap = 6, rows },
	}
end

return function(): Node
	return ui.scroll {
		id = "inspector",
		h_full = true,
		bg = "$surface.raised",
		ui.column {
			p = 16,
			gap = 16,
			-- What it shows changes with the selection.
			ui.presence(function()
				local ids = state.selected:get()
				return if #ids == 0 then "none" elseif #ids == 1 then ids[1] else "many"
			end, function(key: string)
				if key == "none" then
					return none()
				elseif key == "many" then
					return many()
				end
				return one(key)
			end),
		},
	}
end
