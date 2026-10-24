--!strict
-- A small browser on frosted glass: a titlebar with history and an address
-- bar, and a frosted menu over the page, which the page can open too.
local PAGE = [[
<!doctype html>
<style>
  body { margin: 0; font: 15px -apple-system, sans-serif; color: #e8e6e1;
         background: linear-gradient(135deg, #3b2f63, #0f5e66 55%, #c46a2b); min-height: 100vh; }
  main { padding: 48px; }
  h1 { font-size: 44px; margin: 0 0 12px; }
  .grid { display: grid; grid-template-columns: repeat(4, 1fr); gap: 14px; margin-top: 28px; }
  .grid div { height: 120px; border-radius: 14px; background: hsl(calc(var(--i) * 40), 70%, 60%); }
  button { font: inherit; padding: 8px 14px; border-radius: 8px; border: 0; }
</style>
<title>Local page</title>
<main>
  <h1>Hello from WebKit</h1>
  <p>The menu in the top right is zgx, drawn over this page with the system blur.</p>
  <button onclick="window.zgx.postMessage({ kind: 'menu' })">Open it from the page</button>
  <div class="grid">
    <div style="--i:0"></div><div style="--i:1"></div><div style="--i:2"></div><div style="--i:3"></div>
    <div style="--i:4"></div><div style="--i:5"></div><div style="--i:6"></div><div style="--i:7"></div>
  </div>
</main>
]]

local web = ui.web { profile = "ephemeral" }
local open = ui.state("open", false)
-- The window's corner radius, from the host; the page's corners sit
-- concentric with it, `INSET` in from the edge.
local window_radius = ui.shared("window_radius", 10)
local INSET = 6

-- Dark glass: the window's blur shows through the tint. Foregrounds are
-- translucent white, so they take on the colour of what's behind the glass
-- the way AppKit's vibrant labels do, instead of sitting on it as flat grey.
local tint = "#1214168c"
local wash = "#ffffff10"
local line = "#ffffff0d"
local text = "#ffffffeb"
local secondary = "#ffffffa6"
local tertiary = "#ffffff66"
local quaternary = "#ffffff38"
local accent = "#a99cff"

local function icon(paths: string, view: string?): string
	return '<svg xmlns="http://www.w3.org/2000/svg" viewBox="'
		.. (view or "0 0 24 24")
		.. '" fill="none" stroke="#fff" stroke-width="1.75" stroke-linecap="round" stroke-linejoin="round">'
		.. paths
		.. "</svg>"
end
local BACK = icon('<path d="M19 12H5m5.5-5.5L5 12l5.5 5.5"/>')
local FORWARD = icon('<path d="M5 12h14m-5.5-5.5L19 12l-5.5 5.5"/>')
local RELOAD = icon('<path d="M19.5 12a7.5 7.5 0 1 1-2.2-5.3"/><path d="M19.5 4.5v4h-4"/>')
local STOP = icon('<path d="M6.5 6.5l11 11m0-11l-11 11"/>')
local MORE = icon(
	'<circle cx="5" cy="12" r="1" fill="#fff"/><circle cx="12" cy="12" r="1" fill="#fff"/><circle cx="19" cy="12" r="1" fill="#fff"/>'
)
local GLOBE = icon(
	'<circle cx="12" cy="12" r="9"/><path d="M3 12h18M12 3c2.5 2.6 3.6 5.6 3.6 9s-1.1 6.4-3.6 9c-2.5-2.6-3.6-5.6-3.6-9S9.5 5.6 12 3z"/>'
)
local SEARCH = icon('<circle cx="11" cy="11" r="7"/><path d="M20 20l-3.5-3.5"/>')

-- The address bar shows the page's URL, except while it's being edited.
local address = ui.state("address", "")
local editing = ui.state("editing", false)
local function shown(url: string): string
	return if url == "about:blank" then "" else url
end
ui.effect(function()
	local url = web.url
	if not editing:get() then
		address:set(shown(url))
	end
end)

-- A URL as typed, a bare domain, or else a search.
local function go(typed: string)
	local query = typed:match("^%s*(.-)%s*$") or ""
	if query == "" then
		return
	end
	if query:find("^%a[%w+.-]*:") then
		web:load(query)
	elseif not query:find("%s") and (query:find("%.") or query:find("^localhost")) then
		web:load("https://" .. query)
	else
		local encoded = query
			:gsub("[^%w%-_.~ ]", function(c)
				return string.format("%%%02X", string.byte(c))
			end)
			:gsub(" ", "+")
		web:load("https://duckduckgo.com/?q=" .. encoded)
	end
	editing:set(false)
	web:focus()
end

-- A square glyph button, clear until hovered; dimmed while it can't act.
local function tool(glyph: string | () -> string, on_click: Handler | Action, enabled: (() -> boolean)?)
	return ui.button {
		w = 30,
		h = 28,
		shrink = 0,
		rounded = 7,
		items = "center",
		justify = "center",
		bg = "#00000000",
		hover = { bg = "#ffffff14" },
		-- Only the address bar shows focus.
		focus = { border = 0 },
		disabled = enabled and function()
			return not enabled()
		end or nil,
		on_click = on_click,
		ui.svg {
			code = if type(glyph) == "string" then glyph else nil,
			src = if type(glyph) == "function"
				then function()
					return "code:" .. glyph()
				end
				else nil,
			w = 16,
			h = 16,
			tint = function()
				return if enabled == nil or enabled() then secondary else quaternary
			end,
		},
	}
end

-- Empty titlebar space moves the window (a double click zooms it).
local function drag(props: ContainerProps?)
	local spacer: ContainerProps = if props then table.clone(props) else {}
	spacer.h_full = true
	spacer.window_drag = true
	return ui.div(spacer)
end

-- Clear on the glass, lit on hover and keyboard focus.
local function item(label: string, on_click: () -> ())
	return ui.menu_item {
		label,
		h = 30,
		px = 10,
		rounded = 7,
		text_size = 13,
		color = text,
		bg = "#00000000",
		hover = { bg = "#ffffff14" },
		focus = { bg = "#ffffff14", border = 0 },
		on_click = function()
			open:set(false)
			on_click()
		end,
	}
end

-- A hairline along the address bar's bottom edge, filling while the page loads.
local progress = ui.div {
	absolute = true,
	left = 0,
	bottom = 0,
	h = 2,
	bg = accent,
	w = function()
		return string.format("%d%%", math.floor(web.progress * 100))
	end,
	opacity = function()
		return if web.loading then 1 else 0
	end,
}

local address_bar = ui.row {
	grow = 3,
	ml = 6,
	max_w = 680,
	h = 30,
	items = "center",
	gap = 8,
	pl = 10,
	pr = 4,
	rounded = 9,
	overflow = "hidden",
	bg = function()
		return if editing:get() then "#00000040" else wash
	end,
	-- A 2pt ring while editing, over the bar (editing moves nothing).
	border = function()
		return if editing:get() then 2 else 1
	end,
	border_over = true,
	border_color = function()
		return if editing:get() then accent .. "b3" else line
	end,
	ui.svg {
		src = function()
			return "code:" .. (if editing:get() then SEARCH else GLOBE)
		end,
		w = 14,
		h = 14,
		tint = tertiary,
	},
	ui.overlay {
		grow = 1,
		shrink = 1,
		min_w = 0,
		h = 28,
		ui.input {
			label = "Address",
			value = address,
			placeholder = "Search or enter address",
			placeholder_color = tertiary,
			-- The first click selects the address, the next places the caret.
			select_on_focus = true,
			w_full = true,
			h = 28,
			text_size = 13,
			-- Faded until edited.
			color = function()
				return if editing:get() then text else secondary
			end,
			bg = "#00000000",
			-- The bar shows focus; no ring of the input's own.
			focus = { border = 0 },
			on_focus = function()
				editing:set(true)
			end,
			on_blur = function()
				editing:set(false)
			end,
			on_key = function(e)
				if e.key == "Enter" then
					go(address:peek())
					return true
				elseif e.key == "Escape" then
					address:set(shown(web.url))
					web:focus()
					return true
				end
				return false
			end,
		},
	},
	progress,
}

local titlebar = ui.row {
	w_full = true,
	h = 40,
	items = "center",
	gap = 2,
	pr = 8,
	-- Room for the traffic lights.
	drag { w = 78 },
	tool(BACK, function()
		web:back()
	end, function()
		return web.can_go_back
	end),
	tool(FORWARD, function()
		web:forward()
	end, function()
		return web.can_go_forward
	end),
	tool(function()
		return if web.loading then STOP else RELOAD
	end, function()
		if web.loading then
			web:stop()
		else
			web:reload()
		end
	end),
	address_bar,
	drag { grow = 1, min_w = 12 },
	ui.popover {
		open = open,
		anchor = tool(MORE, ui.toggle(open)),
		w = 220,
		p = 6,
		gap = 2,
		rounded = 12,
		bg = "#1c1c2099",
		backdrop_blur = 24,
		border = 1,
		border_color = "#ffffff1f",
		shadow = { y = 12, blur = 32, color = "#00000066" },
		items = "stretch",
		item("Zoom 125%", function()
			web:zoom(1.25)
		end),
		item("Actual size", function()
			web:zoom(1)
		end),
		item("Turn the page", function()
			web:run("document.body.style.filter = 'hue-rotate(90deg)'")
		end),
		item("Web Inspector", function()
			web:devtools()
		end),
	},
}

return ui.column {
	w_full = true,
	h_full = true,
	items = "stretch",
	bg = tint,
	titlebar,
	ui.webview {
		web = web,
		html = PAGE,
		grow = 1,
		mx = INSET,
		mb = INSET,
		rounded = function()
			return math.max(window_radius:get() - INSET, 4)
		end,
		on_message = function(value)
			if value.kind == "menu" then
				open:set(true)
			end
		end,
		on_error = function(_, err)
			print("page failed: " .. err)
		end,
	},
}
