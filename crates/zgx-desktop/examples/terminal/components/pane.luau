-- A pane: a card placed where frames.luau puts it, holding a terminal, a
-- browser or a panel (panel.luau), with a grip at the top to drag it by.
local theme = require("../theme")
local appearance = require("../appearance")
local icons = require("../icons")
local state = require("../state")
local sessions = require("../sessions")
local actions = require("../actions")
local frames = require("../frames")
local controls = require("./controls")
local kit = require("./kit")
local menu = require("./menu")
local browser = require("./browser")
local panel = require("./panel")
local palette = require("./palette")
local tabs = require("./tabs")
local layout = require("@lib/layout")

local focus, zoom, drop, dragging = state.focus, state.zoom, state.drop, state.dragging

-- The terminal's title and tools live inside its card, as native Rex panes do.
-- Each action focuses this pane before using the shared split/zoom commands.
local function terminal_header(pane: string, tab: string): Node
	local function desc()
		return sessions.describe(pane)
	end
	local function tool(svg: string, label: string, id: string, run: () -> ()): Node
		return ui.button {
			id = `{id}:{pane}`,
			label = label,
			variants = kit.button { tone = "ghost", size = "icon_sm" },
			on_click = run,
			ui.svg { code = svg, w = 14, h = 14, tint = theme.tertiary },
		}
	end
	return ui.row {
		id = `pane_header:{pane}`,
		h = "$terminal.pane_header",
		px = 10,
		gap = 4,
		shrink = 0,
		min_w = 0,
		items = "center",
		on_click = function()
			actions.focus_pane(tab, pane)
		end,
		controls.glyph(desc, 14, function()
			return (focus:get())[tab] ~= pane
		end),
		ui.text {
			function()
				local name = desc().name
				return if name ~= "" then name else "Terminal"
			end,
			grow = 1,
			shrink = 1,
			min_w = 0,
			truncate = true,
			text_size = "$terminal.type.lg",
			weight = "$font.weight.semibold",
			color = function()
				return if (focus:get())[tab] == pane then theme.text else theme.secondary
			end,
		},
		tool(icons.split_right, "Split Right", "pane_split_right", function()
			actions.focus_pane(tab, pane)
			actions.split("row")
		end),
		tool(icons.split_down, "Split Down", "pane_split_down", function()
			actions.focus_pane(tab, pane)
			actions.split("column")
		end),
		ui.button {
			id = `pane_zoom:{pane}`,
			label = function()
				return if (zoom:get())[tab] == pane then "Unzoom Pane" else "Zoom Pane"
			end,
			variants = kit.button { tone = "ghost", size = "icon_sm" },
			on_click = function()
				actions.focus_pane(tab, pane)
				zoom:set_key(tab, if (zoom:peek())[tab] == pane then nil else pane)
			end,
			ui.svg {
				src = function()
					return "code:" .. (if (zoom:get())[tab] == pane then icons.unzoom else icons.zoom)
				end,
				w = 14,
				h = 14,
				tint = theme.tertiary,
			},
		},
		tool(icons.close, "Close Pane", "pane_close", function()
			actions.close_pane(pane)
		end),
	}
end

-- The terminal, filling the card (zgx gives it the card's size; the
-- desktop host hands Ghostty a size that keeps changing a few times a second).
local function terminal_view(term: Term, pane: string, tab: string, inner: () -> number, several: () -> boolean): Node
	return ui.overlay {
		grow = 1,
		min_h = 0,
		-- The card supplies one continuous surface under its title and terminal.
		bg = theme.clear,
		ui.terminal {
			term = term,
			absolute = true,
			left = 0,
			top = 0,
			right = 0,
			bottom = 0,
			rounded = inner,
			on_focus = function()
				actions.focus_pane(tab, pane)
			end,
			context_menu_style = menu.STYLE,
			context_menu = function()
				actions.focus_pane(tab, pane)
				local zoomed = (zoom:peek())[tab] == pane
				local items = {
					menu.entry("Copy", function()
						term:binding("copy_to_clipboard")
					end),
					menu.entry("Paste", function()
						term:binding("paste_from_clipboard")
					end),
					menu.entry("Clear", function()
						term:binding("clear_screen")
					end),
					menu.separator(),
					menu.entry("Split Right", function()
						actions.split("row")
					end),
					menu.entry("Split Down", function()
						actions.split("column")
					end),
					menu.entry("Open Browser Beside", function()
						actions.split("row", "browser")
					end),
					menu.entry("Open Panel…", palette.open),
				}
				if several() or zoomed then
					table.insert(
						items,
						menu.entry(if zoomed then "Unzoom Pane" else "Zoom Pane", function()
							zoom:set_key(tab, if zoomed then nil else pane)
							term:focus()
						end)
					)
				end
				table.insert(items, menu.separator())
				table.insert(
					items,
					menu.entry("Close Pane", function()
						actions.close_pane(pane)
					end)
				)
				return items
			end,
		},
	}
end

export type Props = { pane: string }

return ui.component(function(props: Props)
	local pane = props.pane
	-- Its own tab: a panel stays mounted while another tab shows.
	local tab = actions.tab_of(pane) or state.active:peek()
	local term = sessions.session(pane)
	local element: Element? = nil
	local function box(): layout.Box?
		return (frames.frames:get()).panes[pane]
	end
	local insets = frames.insets_of(box)
	local function focused()
		return (focus:get())[tab] == pane
	end
	local function several()
		local root = frames.live_root(tab)
		return root ~= nil and root.kind == "split"
	end
	local is_browser = sessions.is_browser(pane)
	-- Not a terminal: zgx draws its bar (a page's, a panel's) and hears
	-- the pointer there.
	local by_zgx = not sessions.is_terminal(pane)
	-- A browser's or panel's bar: whether the pointer is near the top.
	local bar_near = ui.use_state(false)
	local function moving()
		local d = drop:get()
		return d.target ~= nil and d.kind == "pane" and d.id == pane
	end
	local hooks: browser.Hooks = { editing = ui.use_state(false) }
	-- A change of structure eases it to its new box.
	frames.follow(function()
		return element
	end, box)
	-- The grip shows while the pointer is near the top edge, the grip
	-- included (the terminal hears the pointer over what zgx draws on it),
	-- when the pane has somewhere to go: beside another pane, or into
	-- another tab.
	-- The focused terminal hears the pointer anywhere in the window, so
	-- only points over it count.
	local grip = ui.use_state(0)
	ui.effect(function()
		local near = false
		if by_zgx then
			near = bar_near:get() and not hooks.editing:get()
		elseif term then
			local p = term.pointer
			near = bar_near:get()
				or (p ~= nil and p.y >= 0 and p.y < 32 and p.x >= 0 and p.x <= frames.shown(element).w)
		end
		local d = dragging:get()
		-- While dragging, only the dragged pane's grip (hiding the source
		-- would end the drag).
		local show = if d.kind ~= nil
			then d.kind == "pane" and d.id == pane
			else near and (several() or #state.tabs:get() > 1)
		grip:set(if show then 1 else 0)
	end)
	local inner = function()
		return frames.radius() - 1
	end
	return ui.column {
		id = `pane:{pane}`,
		-- Placed by zgx at its frame's insets, at every size of the area;
		-- hidden while it has no frame (a panel in a tab not shown).
		absolute = true,
		hidden = function()
			return box() == nil
		end,
		-- Its edges on whole points: crisp borders at any ratio.
		snap = true,
		left = function()
			return insets().left
		end,
		ml = function()
			return insets().ml
		end,
		right = function()
			return insets().right
		end,
		mr = function()
			return insets().mr
		end,
		top = function()
			return insets().top
		end,
		mt = function()
			return insets().mt
		end,
		bottom = function()
			return insets().bottom
		end,
		mb = function()
			return insets().mb
		end,
		on_mount = function(el)
			element = el
			frames.set_pane_element(pane, el)
			-- Arriving with a change, it fades in where it goes.
			if (frames.frames:peek()).smooth then
				anime.animate(el, { opacity = { 0, 1 }, duration = 220, ease = "outQuad" })
			end
		end,
		items = "stretch",
		rounded = frames.radius,
		overflow = "hidden",
		bg = function()
			return if term then appearance.terminal_surface_bg(focused()) else appearance.pane_bg(focused())
		end,
		-- Capability is fixed by the host before loading. Nil keys keep the
		-- previous running host compatible during passive script reloads.
		surface_recipe = if appearance.recipe_supported()
			then function()
				return appearance.pane_surface(focused())
			end
			else nil,
		backdrop_material = if appearance.native_liquid:peek() and not appearance.recipe_supported()
			then function(): "liquid" | "blur"
				return if appearance.pane_native(focused()) then "liquid" else "blur"
			end
			else nil,
		backdrop_tint = if appearance.native_liquid:peek() and not appearance.recipe_supported()
			then function()
				return appearance.pane_tint(focused())
			end
			else nil,
		backdrop_fill = if appearance.native_liquid:peek() and not appearance.recipe_supported()
			then function()
				return appearance.pane_fill(focused())
			end
			else nil,
		backdrop_fill_blend = if appearance.native_liquid:peek() and not appearance.recipe_supported()
			then function()
				return appearance.pane_fill_blend(focused())
			end
			else nil,
		border = if appearance.recipe_supported() then 0 else 1,
		border_color = function()
			if moving() then
				return "$terminal.moving"
			end
			return if several() and focused() then theme.ring else theme.line
		end,
		-- Over a browser's or a panel's bar, zgx hears the pointer: the top
		-- 32 points show the grip, as over a terminal.
		on_pointer_move = function(e)
			local near = e.local_y < 32
			if bar_near:peek() ~= near then
				bar_near:set(near)
			end
		end,
		on_hover = function(inside)
			if not inside and bar_near:peek() then
				bar_near:set(false)
			end
		end,
		ui.div {
			absolute = true,
			left = 0,
			top = 0,
			right = 0,
			bottom = 0,
			pointer_events = "none",
			bg = function()
				return if appearance.pane_native(focused()) then theme.clear else appearance.pane_fill(focused())
			end,
			blend = function()
				return appearance.pane_fill_blend(focused())
			end,
		},
		ui.column {
			grow = 1,
			min_w = 0,
			min_h = 0,
			items = "stretch",
			opacity = function()
				return appearance.content_opacity(focused())
			end,
			if term then terminal_header(pane, tab) else nil,
			if is_browser
				then browser.Body(pane, tab, inner, hooks)
				elseif term then terminal_view(term, pane, tab, inner, several)
				else panel.Body(pane, tab, inner),
		},
		-- The grip, centred at the top.
		ui.row {
			absolute = true,
			top = 6,
			left = 0,
			w_full = true,
			justify = "center",
			-- Only the grip takes the pointer: the bar under the rest of the
			-- row keeps its clicks.
			pointer_events = "none",
			hidden = function()
				return grip:get() == 0
			end,
			ui.column {
				id = `pane_grip:{pane}`,
				w = 36,
				h = 22,
				rounded = "$terminal.grip.radius",
				items = "center",
				justify = "center",
				-- Frosted, as the window: the terminal shows through, blurred.
				bg = "$terminal.grip.bg",
				backdrop_blur = 14,
				border = 1,
				border_color = "$terminal.grip.line",
				shadow = "$terminal.grip.shadow",
				cursor = "grab",
				opacity = ui.tween(grip, { duration = 160 }),
				-- A click (no drag): on a browser, it was meant for the
				-- address under it.
				on_click = function()
					local focus_address = hooks.focus_address
					if is_browser and focus_address then
						focus_address()
					else
						actions.focus_pane(tab, pane)
					end
				end,
				on_drag = function(): state.Payload
					dragging:set { kind = "pane", id = pane }
					return { kind = "pane", id = pane }
				end,
				drag_preview = function()
					return tabs.mini_pane(pane)
				end,
				-- Below the pointer, so where it would go shows.
				drag_preview_anchor = { 0.5, 0 },
				drag_preview_offset = { 0, 18 },
				on_drag_end = function()
					dragging:set {}
					actions.hide_slot()
					state.tab_drop:set("")
				end,
				controls.dots(),
			},
		},
	}
end)
