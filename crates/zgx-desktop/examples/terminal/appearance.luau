-- Appearance preferences and material values traced to Rex Beta's bundled
-- MaterialStyles-BlendTheme / UseTheme / Modern JSON and themes.json.
-- The native host composites window glass: GPU blend layers cannot sample
-- AppKit's behind-window surface. Selection/pane Liquid Glass needs a native
-- node effect; their Luau fallback below retains the resource's alpha values.
local theme = require("./theme")
local recipes: any = require("./material_recipes")
local profiles = require("./material_profiles")
local terminal_colors: any = require("./terminal_colors")

local M = {}
M.mode = ui.persist("appearance_mode", "glass")
M.scheme = ui.persist("appearance_scheme", "auto")
M.tint = ui.persist("appearance_tint", true)
M.pane_glass = ui.persist("appearance_pane_glass", false)
M.window_active = ui.shared("window_active", true)
M.native_liquid = ui.shared("native_liquid", false)
M.native_surface_schema = ui.shared("native_surface_recipe_schema", 0)
M.system_colors = ui.shared("system_colors", {} :: { [string]: string })

local LIGHT: { [string]: string | number | { [string]: any } } = {
	["text.primary"] = "#1f2328",
	["text.secondary"] = "#696d71",
	["text.tertiary"] = "#878a8e",
	["text.disabled"] = "#878a8e",
	["surface.base"] = "#f8f9fb",
	["surface.sunken"] = "#f6f7f9",
	["surface.raised"] = "#fafbfc",
	["surface.overlay"] = "#fafbfc",
	["border.subtle"] = "#e7e9eb",
	["border.strong"] = "#d1d2d5",
	["hover"] = "#1f23280a",
	["accent"] = "#0667d8",
	["warning"] = "#9d5c00",
	["danger"] = "#cf222e",
	["success"] = "#116329",
	["terminal.canvas"] = "#f6f7f9",
	["terminal.surface_raised"] = "#fafbfc",
	["terminal.surface_subtle"] = "#eceef2",
	["terminal.pane"] = "#f8f9fb",
	["terminal.terminal_bg"] = "#ffffff",
	["terminal.wash"] = "#1f232814",
	["terminal.tile_bg"] = "#f6f7f8",
	["terminal.tile_dim"] = "#1f232805",
	["terminal.tile_line"] = "#e7e9eb",
	["terminal.ring"] = "#e7e9eb",
	["terminal.menu.bg"] = "#fafbfc",
	["terminal.menu.line"] = "#d1d2d5",
	["terminal.menu.hover"] = "#1f23280a",
	["terminal.menu.separator"] = "#e7e9eb",
	["terminal.bright"] = "#1f2328",
	["terminal.error_text"] = "#cf222e",
	["terminal.control_hover"] = "#1f23280a",
	["terminal.button.hover"] = "#1f232814",
	["terminal.button.line"] = "#e7e9eb",
	["terminal.primary.bg"] = "#0969da",
	["terminal.primary.hover"] = "#0667d8",
	["terminal.primary.solid"] = "#0969da",
	["terminal.danger.bg"] = "#cf222e",
	["terminal.danger.hover"] = "#cf222e",
	["terminal.sheet.bg"] = "#fafbfc",
	["terminal.sheet.line"] = "#d1d2d5",
	["terminal.palette.bg"] = "#00000000",
	["terminal.palette.line"] = "#0000001a",
	["terminal.key.bg"] = "#f6f7f8",
	["terminal.key.line"] = "#e7e9eb",
	["terminal.chip.bg"] = "#1f232805",
	["terminal.chip.line"] = "#e7e9eb",
	["terminal.card.bg"] = "#eceef2",
	["terminal.card.line"] = "#e7e9eb",
	["terminal.field.bg"] = "#f6f7f8",
	["terminal.field.line"] = "#e7e9eb",
	["terminal.field.editing"] = "#f6f7f9",
	["terminal.field.ring"] = "#0667d8",
	["terminal.progress"] = "#0667d8",
	["glass.tint"] = "#f6f7f980",
	["terminal.glass"] = "#fafbfc40",
	["terminal.glass_hover"] = "#1f232814",
	["terminal.selection_line"] = "#1f23281a",
	["terminal.on_fill"] = "#ffffff",
	["font.sans"] = "System Font",
}

ui.theme.register { name = "terminal-light", type = "light", extends = "terminal-glass", tokens = LIGHT }

local function alpha(color: string, opacity: number): string
	return string.sub(color, 1, 7) .. string.format("%02x", math.floor(opacity * 255 + 0.5))
end
local function token(name: string): string
	return ui.theme.get(name) :: string
end
local function system_color(name: string, fallback: string): string
	return M.system_colors:get()[name] or fallback
end
function M.dark(): boolean
	local kind = ui.theme.kind()
	return kind == "dark" or kind == "high-contrast"
end
function M.effective_mode(): string
	-- Readable's generated type currently gives :get() a never receiver.
	local reduced = ui.prefers.reduced_transparency :: { get: (self: unknown) -> boolean }
	return if reduced:get() then "opaque" else M.mode:get()
end
function M.root_bg(): string
	-- Native underlay and the decoded window recipe own the complete surface.
	-- In particular System.dark supplies only a shape, with no enabled fill.
	return theme.clear
end

-- The untinted styles resolve content through macOS's semantic palette;
-- pane roles prefixed theme: still retain the selected terminal theme.
function M.scope(): ThemeSpec
	local tokens: { [string]: string | number | { [string]: any } } = { theme = ui.theme.current:get() }
	if not M.tint:get() then
		local colors = M.system_colors:get()
		if colors.label then
			tokens["text.primary"] = colors.label
		end
		if colors.secondary_label then
			tokens["text.secondary"] = colors.secondary_label
		end
		if colors.tertiary_label then
			tokens["text.tertiary"] = colors.tertiary_label
		end
	end
	return tokens
end

function M.RootLayers(): Node
	return ui.div { absolute = true, left = 0, top = 0, right = 0, bottom = 0, pointer_events = "none" }
end

-- The native root adapter receives resolved paints plus the original nested
-- recipe. Source alpha and per-layer geometry remain available separately.
function M.material(): { [string]: any }
	local dark = M.dark()
	local source = terminal_colors[if dark then "dark" else "light"]
	local window = M.window_recipe()
	local glass = window.glass
	local shadow = window.innerShadow
	local inner_shadow = if type(shadow) == "table" and shadow.isEnabled == true
		then {
			color = M.layer_color(shadow),
			blur = shadow.blurRadius,
			x = shadow.offsetX,
			y = shadow.offsetY,
			blend = shadow.blendMode,
			knockout = shadow.knockout,
			geometry = shadow.geometry,
		}
		else nil
	return {
		window_recipe = M.resolve_colors(window),
		inner_shadow = inner_shadow,
		interface_style = M.interface_style(),
		mode = M.effective_mode(),
		scheme = M.scheme:get(),
		resolved_scheme = if dark then "dark" else "light",
		tint = M.tint:get(),
		pane_glass = M.effective_mode() == "glass" and M.pane_glass:get(),
		glass_enabled = type(glass) == "table" and glass.isEnabled == true,
		material = if type(glass) == "table" then glass.windowMaterial else "underWindowBackground",
		style = if type(glass) == "table" then glass.style else "regular",
		corner_radius = window.shape.cornerRadius,
		tint_color = if type(glass) == "table" and glass.usesTint
			then M.layer_color { color = glass.tint }
			else theme.clear,
		backdrop_color = M.layer_color(window.backdrop),
		backdrop_blend = if type(window.backdrop) == "table" then window.backdrop.blendMode else "normal",
		fill_color = M.layer_color(window.fill),
		fill_blend = if type(window.fill) == "table" then window.fill.blendMode else "normal",
		terminal_background = source.background,
		terminal_foreground = source.foreground,
		terminal_ansi = source.ansi,
		terminal_cursor = source.cursor,
		terminal_cursor_text = source.cursorText,
		terminal_selection_background = source.selectionBackground,
		terminal_selection_foreground = source.selectionForeground,
		active = M.window_active:get(),
	}
end

function M.interface_style(): string
	local glass = M.effective_mode() == "glass"
	return if glass
		then (if M.tint:get() then "blendTheme" else "modern")
		else (if M.tint:get() then "useTheme" else "system")
end

local function branch(): any
	return recipes[M.interface_style()][if M.dark() then "dark" else "light"][if M.window_active:get()
		then "active"
		else "inactive"]
end
function M.window_recipe(): any
	local scheme = recipes[M.interface_style()][if M.dark() then "dark" else "light"]
	local raw = profiles.merge(branch().window, { innerShadow = scheme.windowInnerShadow })
	return profiles.normalize(raw)
end
local function selected_recipe(focused: boolean): any
	return profiles.pane(branch(), M.effective_mode() ~= "glass" or not M.pane_glass:get(), focused)
end
local pane_selected = ui.computed(function()
	return selected_recipe(true)
end)
local pane_other = ui.computed(function()
	return selected_recipe(false)
end)
function M.pane_recipe(focused: boolean): any
	return (if focused then pane_selected else pane_other):get()
end

local ROLE_TOKENS = {
	canvas = "terminal.canvas",
	surface = "terminal.pane",
	surfaceRaised = "terminal.surface_raised",
	surfaceSubtle = "terminal.surface_subtle",
	content = "text.primary",
	contentSecondary = "text.secondary",
	contentTertiary = "text.tertiary",
	stroke = "border.strong",
	strokeSubtle = "border.subtle",
	control = "hover",
}
local SYSTEM_ROLES = {
	canvas = "window_background",
	surface = "under_page_background",
	surfaceRaised = "under_page_background",
	surfaceSubtle = "window_background",
	content = "label",
	contentSecondary = "secondary_label",
	contentTertiary = "tertiary_label",
}
function M.role_color(role: string?, fallback: string): string
	if not role or role == "" then
		return fallback
	end
	local themed = M.tint:get()
	if string.sub(role, 1, 6) == "theme:" then
		themed, role = true, string.sub(role, 7)
	elseif string.sub(role, 1, 6) == "macOS:" then
		themed, role = false, string.sub(role, 7)
	end
	if themed then
		local key = ROLE_TOKENS[role]
		return if key then token(key) else fallback
	end
	local key = SYSTEM_ROLES[role]
	return if key then system_color(key, fallback) else fallback
end
function M.layer_color(layer: any): string
	if type(layer) ~= "table" or layer.isEnabled == false then
		return theme.clear
	end
	local spec = layer.color or {}
	local adaptive = spec.color or {}
	local fixed = (if M.dark() then adaptive.dark or adaptive.light else adaptive.light) or {}
	local literal = string.format(
		"#%02x%02x%02x",
		math.floor((fixed.red or 0) * 255 + 0.5),
		math.floor((fixed.green or 0) * 255 + 0.5),
		math.floor((fixed.blue or 0) * 255 + 0.5)
	)
	local color = M.role_color(spec.role, literal)
	return alpha(color, (fixed.alpha or 1) * ((layer.geometry or {}).opacity or 1))
end
function M.pane_bg(focused: boolean): string
	return if M.recipe_supported() then theme.clear else M.layer_color(M.pane_recipe(focused).backdrop)
end
-- Ghostty's transparent background exposes this same surface below the
-- title and body. Pane glass is a whole-card choice, never a header band.
function M.terminal_surface_bg(focused: boolean): string
	return if M.effective_mode() == "glass" then M.pane_bg(focused) else "$terminal.terminal_bg"
end
function M.pane_fill(focused: boolean): string
	return if M.recipe_supported() then theme.clear else M.layer_color(M.pane_recipe(focused).fill)
end
function M.pane_fill_blend(focused: boolean): BlendMode
	local blend = (M.pane_recipe(focused).fill or {}).blendMode or "normal"
	-- plusLighter/plusDarker need additive paint support; normal is an
	-- explicit fallback until that operation is exposed by the renderer.
	return if blend == "overlay" or blend == "color" then blend else "normal"
end
function M.pane_tint(focused: boolean): string
	local glass = M.pane_recipe(focused).glass or {}
	return if glass.usesTint and glass.tint then M.layer_color { color = glass.tint } else theme.clear
end
function M.pane_native(focused: boolean): boolean
	return M.native_liquid:get()
		and M.effective_mode() == "glass"
		and M.pane_glass:get()
		and (M.pane_recipe(focused).glass or {}).isEnabled == true
end
local selection = ui.computed(function(): any
	return profiles.selection(branch(), false)
end)
local hover = ui.computed(function(): any
	return profiles.selection(branch(), true)
end)
function M.resolve_colors(value: any): any
	if type(value) ~= "table" then
		return value
	end
	local out = {}
	for key, child in value do
		out[key] = M.resolve_colors(child)
	end
	if type(value.role) == "string" then
		out.resolvedColor = M.role_color(value.role, M.layer_color { color = value })
	end
	return out
end
function M.recipe_supported(): boolean
	return M.native_surface_schema:peek() == 1
end
function M.pane_surface(focused: boolean): SurfaceRecipe
	return M.resolve_colors(M.pane_recipe(focused))
end
function M.selection_surface(): SurfaceRecipe
	return M.resolve_colors(selection:get())
end
function M.hover_surface(): SurfaceRecipe
	return M.resolve_colors(hover:get())
end
function M.hover_bg(): string
	return M.layer_color(hover:get().fill)
end
function M.selection_material(): "liquid" | "blur"
	return if M.effective_mode() == "glass" and (selection:get().glass or {}).isEnabled == true
		then "liquid"
		else "blur"
end
function M.selection_bg(): string
	if M.recipe_supported() then
		return theme.clear
	end
	if M.selection_material() == "liquid" and not M.native_liquid:get() then
		return theme.glass
	end
	return M.layer_color(selection:get().backdrop)
end
function M.selection_tint(): string
	local glass = selection:get().glass or {}
	return if glass.usesTint and glass.tint then M.layer_color { color = glass.tint } else theme.clear
end
function M.selection_fill(): string
	return if M.recipe_supported() then theme.clear else M.layer_color(selection:get().fill)
end
function M.selection_fill_blend(): BlendMode
	local blend = (selection:get().fill or {}).blendMode
	return if blend == "overlay" or blend == "color" then blend else "normal"
end

function M.content_opacity(focused: boolean): number
	return M.pane_recipe(focused).contentOpacity or 1
end

function M.start()
	ui.effect(function()
		local scheme = M.scheme:get()
		if scheme == "auto" then
			ui.theme.follow_system { dark = "terminal-glass", light = "terminal-light" }
		else
			ui.theme.set(if scheme == "light" then "terminal-light" else "terminal-glass")
		end
	end)
	ui.effect(function()
		ui.invoke("set_appearance", M.material())
	end)
end

return M
