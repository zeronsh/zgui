-- Small pieces the bars and tabs share.
local theme = require("../theme")
local appearance = require("../appearance")
local icons = require("../icons")
local names = require("@lib/names")
local kit = require("./kit")

local M = {}

-- The motion glass moves with (icons.zeron.sh's ease).
local EASE = "cubicBezier(.22, 1, .36, 1)"

-- A row's selected surface (the row must be `relative`). Hosts supporting
-- native materials keep its glass behind the row's labels; earlier hosts
-- use the material's translucent color layers. One piece per `id`:
-- when `shown()` moves to another row, the glass slides there rather than
-- one fading out while another fades in.
function M.glass(id: string, shown: () -> boolean, radius: number | string, node_id: string?, fade_in: boolean?): Node
	return ui.presence(function()
		return if shown() then "on" else nil
	end, function(): Node
		return ui.div {
			id = node_id,
			layout_id = id,
			layout_duration = 320,
			layout_ease = EASE,
			w_full = true,
			h_full = true,
			rounded = radius,
			bg = appearance.selection_bg,
			surface_recipe = if appearance.recipe_supported() then appearance.selection_surface else nil,
			backdrop_material = if appearance.native_liquid:peek() and not appearance.recipe_supported()
				then appearance.selection_material
				else nil,
			backdrop_tint = if appearance.native_liquid:peek() and not appearance.recipe_supported()
				then appearance.selection_tint
				else nil,
			backdrop_fill = if appearance.native_liquid:peek() and not appearance.recipe_supported()
				then appearance.selection_fill
				else nil,
			backdrop_fill_blend = if appearance.native_liquid:peek() and not appearance.recipe_supported()
				then appearance.selection_fill_blend
				else nil,
			border = if appearance.recipe_supported() then 0 else 1,
			border_color = "$terminal.selection_line",
			ui.div {
				absolute = true,
				left = 0,
				top = 0,
				right = 0,
				bottom = 0,
				rounded = radius,
				pointer_events = "none",
				bg = function()
					return if appearance.native_liquid:get() and appearance.selection_material() == "liquid"
						then theme.clear
						else appearance.selection_fill()
				end,
				blend = appearance.selection_fill_blend,
			},
		}
	end, {
		absolute = true,
		top = 0,
		left = 0,
		right = 0,
		bottom = 0,
		enter = if fade_in == false then nil else { opacity = { 0, 1 }, duration = 160 },
	})
end

-- A fainter glass that fades in under a row while the pointer is over it
-- (under `glass`, so the active row shows only that). Returns the layer and
-- what the row's `on_hover` calls.
function M.hover_glass(radius: number | string): (Node, (inside: boolean) -> ())
	local layer: Element? = nil
	local node = ui.div {
		absolute = true,
		top = 0,
		left = 0,
		right = 0,
		bottom = 0,
		rounded = radius,
		bg = function()
			return if appearance.recipe_supported() then theme.clear else appearance.hover_bg()
		end,
		surface_recipe = if appearance.recipe_supported() then appearance.hover_surface else nil,
		border = 0,
		opacity = 0,
		on_mount = function(el: Element)
			layer = el
		end,
	}
	return node,
		function(inside: boolean)
			if layer then
				anime.animate(layer, {
					opacity = if inside then 1 else 0,
					duration = if inside then 140 else 220,
					ease = "outQuad",
				})
			end
		end
end

-- A pane's icon: its program's, or the glyph its title starts with.
function M.glyph(desc: () -> names.Description, size: number, dim: () -> boolean): Node
	return ui.overlay {
		w = size + 2,
		h = size + 2,
		shrink = 0,
		items = "center",
		justify = "center",
		ui.svg {
			src = function()
				return "code:" .. icons[desc().icon]
			end,
			hidden = function()
				return desc().glyph ~= nil
			end,
			w = size,
			h = size,
			tint = function()
				local d = desc()
				return d.tint or (if dim() then theme.tertiary else theme.secondary)
			end,
		},
		ui.text {
			function()
				return desc().glyph or ""
			end,
			hidden = function()
				return desc().glyph == nil
			end,
			text_size = size,
			color = function()
				return desc().tint or theme.secondary
			end,
		},
	}
end

-- A square icon button in the titlebar.
function M.tool(svg: string, on_click: () -> (), label: string?, id: string?): Node
	return ui.button {
		id = id,
		label = label,
		variants = kit.button { tone = "ghost", size = "icon" },
		on_click = on_click,
		ui.svg { code = svg, w = 15, h = 15, tint = theme.secondary },
	}
end

-- Empty titlebar space moves the window (a double click zooms it).
function M.drag_space(props: ContainerProps): Node
	props.h_full = true
	props.window_drag = true
	return ui.div(props)
end

-- Six dots, two rows of three: a grip.
function M.dots(): Node
	local function row()
		local out: ContainerProps = { gap = "$terminal.grip.gap", items = "center" }
		for _ = 1, 3 do
			table.insert(out, ui.div { w = 3, h = 3, rounded = "$radius.full", bg = "$terminal.grip.dot" })
		end
		return ui.row(out)
	end
	return ui.column { gap = "$terminal.grip.gap", items = "center", row(), row() }
end

return M
