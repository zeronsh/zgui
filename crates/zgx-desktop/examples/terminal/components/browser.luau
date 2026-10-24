-- A browser pane's face: its bar (a handle to move it by, history, reload,
-- the address) over the page, or over a quiet start until it has one.
local theme = require("../theme")
local icons = require("../icons")
local state = require("../state")
local sessions = require("../sessions")
local actions = require("../actions")
local menu = require("./menu")
local kit = require("./kit")

local focus = state.focus

local M = {}

-- What the pane (pane.luau) and its browser body share: whether the
-- address is being edited, and how to focus the address (set once the bar
-- is built).
export type Hooks = {
	editing: State<boolean>,
	focus_address: (() -> ())?,
}

-- How an address shows at rest: no scheme, no "www.", no trailing slash.
local function pretty(url: string): string
	if url == "" or url == "about:blank" then
		return ""
	end
	local out = url:gsub("^https?://", "")
	out = out:gsub("^www%.", "")
	return (out:gsub("/$", ""))
end

-- What was typed: a URL, a bare domain, or else a search.
local function address_of(typed: string): string?
	local query = typed:match("^%s*(.-)%s*$") or ""
	if query == "" then
		return nil
	end
	if query:find("^%a[%w+.-]*:") then
		return query
	end
	if not query:find("%s") and (query:find("%.") or query:find("^localhost")) then
		return "https://" .. query
	end
	local escaped = string.gsub(query, "[^%w%-_.~ ]", function(c: string)
		return string.format("%%%02X", string.byte(c))
	end)
	local encoded = escaped:gsub(" ", "+")
	return "https://duckduckgo.com/?q=" .. encoded
end

-- A history or reload button in a page's bar: clear until hovered, dim
-- while it can't act.
local function nav(svg: () -> string, on_click: () -> (), enabled: (() -> boolean)?): Node
	return ui.button {
		variants = kit.button { tone = "ghost", size = "icon_sm" },
		disabled = if enabled
			then function()
				return not enabled()
			end
			else nil,
		on_click = on_click,
		ui.svg {
			src = function()
				return "code:" .. svg()
			end,
			w = 15,
			h = 15,
			tint = function()
				return if enabled == nil or enabled() then theme.secondary else theme.quaternary
			end,
		},
	}
end

-- Built inside the pane's component: its states are the pane's.
function M.Body(pane: string, tab: string, inner: () -> number, hooks: Hooks): Node
	local web = sessions.page(pane)
	local address = ui.use_state("")
	local editing = hooks.editing
	-- The address input: its element while it is mounted.
	local field = ui.ref()
	local function blank(): boolean
		local url = web.url
		return url == "" or url == "about:blank"
	end
	-- The bar shows the page's address, except while it is being edited.
	ui.effect(function()
		local url = web.url
		if not editing:get() then
			address:set(pretty(url))
		end
	end)
	-- Asked for the address bar (a new pane, Open Location): focus it.
	ui.effect(function()
		local el = field:get()
		if el and state.address_focus:get() == pane then
			state.address_focus:set("")
			el:focus()
		end
	end)
	-- The page took focus: this pane is the tab's focused one.
	ui.effect(function()
		if web.focused and (focus:peek())[tab] ~= pane then
			focus:set_key(tab, pane)
		end
	end)
	local function go(typed: string)
		local url = address_of(typed)
		if not url then
			return
		end
		web:load(url)
		editing:set(false)
		web:focus()
	end
	local function focus_address()
		local el = field:peek()
		if el then
			el:focus()
		end
	end
	hooks.focus_address = focus_address
	local bar = ui.row {
		h = 38,
		pl = 6,
		pr = 6,
		gap = 1,
		items = "center",
		shrink = 0,
		context_menu_style = menu.STYLE,
		context_menu = function()
			return {
				menu.entry("Back", function()
					web:back()
				end),
				menu.entry("Forward", function()
					web:forward()
				end),
				menu.entry("Reload", function()
					web:reload()
				end),
				menu.separator(),
				menu.entry("Split Right", function()
					actions.focus_pane(tab, pane)
					actions.split("row")
				end),
				menu.entry("Split Down", function()
					actions.focus_pane(tab, pane)
					actions.split("column")
				end),
				menu.separator(),
				menu.entry("Close Pane", function()
					actions.close_pane(pane)
				end),
			}
		end,
		nav(function()
			return icons.back
		end, function()
			web:back()
		end, function()
			return web.can_go_back
		end),
		nav(function()
			return icons.forward
		end, function()
			web:forward()
		end, function()
			return web.can_go_forward
		end),
		nav(function()
			return if web.loading then icons.stop else icons.refresh
		end, function()
			if web.loading then
				web:stop()
			else
				web:reload()
			end
		end, function()
			return not blank()
		end),
		-- The address: faint at rest, a 2-point ring while edited (over
		-- it, so editing moves nothing).
		ui.row {
			-- Its width is what is left, whatever the address's length.
			w = 0,
			grow = 1,
			min_w = 0,
			h = 26,
			ml = 5,
			pl = 9,
			pr = "$space.1",
			gap = 2,
			items = "center",
			rounded = "$terminal.button.radius_sm",
			overflow = "hidden",
			bg = function()
				return if editing:get() then "$terminal.field.editing" else "$terminal.field.bg"
			end,
			hover = { bg = "$terminal.key.bg" },
			border = function()
				return if editing:get() then 2 else 1
			end,
			border_over = true,
			border_color = function()
				return if editing:get() then "$terminal.field.ring" else "$terminal.field.line"
			end,
			ui.svg {
				src = function()
					return "code:" .. (if editing:get() or blank() then icons.search else icons.browser)
				end,
				w = 13,
				h = 13,
				shrink = 0,
				tint = theme.tertiary,
			},
			ui.overlay {
				w = 0,
				grow = 1,
				min_w = 0,
				h = 24,
				ui.input {
					label = "Address",
					value = address,
					placeholder = "Search or enter address",
					placeholder_color = theme.quaternary,
					-- The first click selects the address, the next places the caret.
					select_on_focus = true,
					w_full = true,
					h = 24,
					text_size = "$terminal.type.body",
					color = function()
						return if editing:get() then theme.text else theme.secondary
					end,
					bg = theme.clear,
					focus = { border = 0 },
					ref = field,
					on_focus = function()
						if not editing:peek() then
							editing:set(true)
							-- The whole address, to edit.
							if not blank() then
								address:set(web.url)
							end
						end
						if (focus:peek())[tab] ~= pane then
							focus:set_key(tab, pane)
						end
					end,
					on_blur = function()
						editing:set(false)
						address:set(pretty(web.url))
					end,
					on_key = function(e)
						if e.key == "Enter" then
							go(address:peek())
							return true
						elseif e.key == "Escape" then
							editing:set(false)
							address:set(pretty(web.url))
							if not blank() then
								web:focus()
							end
							return true
						end
						return false
					end,
				},
			},
		},
	}
	-- Under the bar: a hairline, filling while the page loads.
	local rule = ui.overlay {
		h = 1,
		shrink = 0,
		bg = theme.line,
		ui.div {
			absolute = true,
			left = 0,
			top = 0,
			h = 1,
			bg = "$terminal.progress",
			w = function()
				return string.format("%d%%", math.floor(web.progress * 100))
			end,
			opacity = function()
				return if web.loading then 1 else 0
			end,
		},
	}
	return ui.column {
		grow = 1,
		min_h = 0,
		items = "stretch",
		bar,
		rule,
		ui.overlay {
			grow = 1,
			min_h = 0,
			-- The page fills what the bar leaves.
			ui.webview {
				web = web,
				absolute = true,
				left = 0,
				top = 0,
				right = 0,
				bottom = 0,
				rounded_bl = inner,
				rounded_br = inner,
				hidden = blank,
				on_new_window = function(url)
					web:load(url)
				end,
			},
			-- Before the first page: where to start.
			ui.column {
				w_full = true,
				h_full = true,
				gap = 10,
				items = "center",
				justify = "center",
				hidden = function()
					return not blank()
				end,
				-- (A ref has no `focus`: its element does.)
				on_click = focus_address,
				ui.svg { code = icons.browser, w = 28, h = 28, tint = theme.quaternary },
				ui.text { "Search or enter an address above", text_size = "$terminal.type.body", color = theme.tertiary },
			},
		},
	}
end

return M
