-- A pane that is neither a terminal nor a page: a plugin's panel, or the
-- plugins themselves. A plugin's panel is the whole pane: what the plugin
-- contributes to `panel:<panel>`, an instance per pane, told its pane in
-- `props.slot` ({ pane, tab, radius }: `radius` is the pane's inner corner,
-- for a page or a picture at its bottom corners). Its top 32 points show
-- the pane's grip when the pointer is there, as a browser's bar does.
local theme = require("../theme")
local icons = require("../icons")
local state = require("../state")
local sessions = require("../sessions")
local consent = require("./consent")
local kit = require("./kit")

local M = {}

-- A quiet button with an icon (the page's own actions).
local function action(svg: string, label: string, on_click: () -> ()): Node
	return ui.button {
		variants = kit.button { tone = "outline", size = "sm" },
		on_click = on_click,
		ui.svg { code = svg, w = 13, h = 13, tint = theme.secondary },
		ui.text { label },
	}
end

-- How each status shows: its tile's tint, and a badge for what needs you.
local STATUS: { [string]: { tint: string, badge: string? } } = {
	enabled = { tint = "#b69cff" },
	pending = { tint = "#e8b34b", badge = "Waiting for you" },
	off = { tint = "#8a919e", badge = "Off" },
	broken = { tint = "#e5534b", badge = "Does not load" },
}
-- What each permission shows as, beside its words.
local PERMISSION_ICONS: { [string]: icons.Name } = {
	network = "globe",
	clipboard = "clipboard",
	files = "doc",
	terminals = "terminal",
	web = "browser",
}

-- A permission or a panel: a small rounded label.
local function chip(svg: string?, label: string, tint: string?): Node
	local tinted = tint ~= nil
	local own = tint or ""
	return ui.row {
		variants = kit.chip,
		bg = if tinted then kit.tint(own, "1a") else nil,
		border_color = if tinted then kit.tint(own, "26") else nil,
		ui.show(
			svg ~= nil,
			ui.svg { code = svg or "", w = 11, h = 11, tint = if tinted then kit.tint(own, "") else theme.tertiary }
		),
		ui.text { label, text_size = "$terminal.type.sm", color = if tinted then kit.tint(own, "") else theme.secondary },
	}
end

-- On or off, as a switch: a click asks the host (which writes the answer
-- and loads or drops the plugin).
local function switch(on: boolean, on_click: () -> ()): Node
	return ui.button {
		variants = kit.switch { on = on },
		label = if on then "Turn off" else "Turn on",
		on_click = on_click,
		ui.div { variants = kit.knob },
	}
end

-- A plugin in the list: what it is, what it may do, and its switch.
local function card(plugin: state.PluginInfo): Node
	local status = STATUS[plugin.status] or STATUS.off
	local byline = plugin.version
	if plugin.author ~= "" then
		byline = if byline ~= "" then `{byline} · {plugin.author}` else plugin.author
	end
	local chips: ContainerProps = { gap = 6, flex_wrap = true, items = "center" }
	for _, p in plugin.permissions do
		local icon: icons.Name = PERMISSION_ICONS[p.id] or "check"
		table.insert(chips, chip(icons[icon], p.text or p.id))
	end
	if #plugin.permissions == 0 and plugin.status ~= "broken" then
		table.insert(chips, chip(icons.check, "Asks for nothing beyond its panels"))
	end
	for _, panel in plugin.panels do
		table.insert(chips, chip(icons.plugin, panel.title, "#b69cff"))
	end
	local control: Node? = nil
	if plugin.status == "enabled" or plugin.status == "off" then
		local on = plugin.status == "enabled"
		control = switch(on, function()
			consent.decide(plugin.id, not on)
		end)
	elseif plugin.status == "pending" then
		control = ui.button {
			"Review",
			variants = kit.button { tone = "primary", size = "xs" },
			on_click = function()
				state.consent_later:set_key(plugin.id, nil)
			end,
		}
	end
	return ui.row {
		variants = kit.card,
		ui.row {
			variants = kit.tile { size = "lg" },
			bg = kit.tint(status.tint, "1f"),
			border_color = kit.tint(status.tint, "33"),
			ui.svg { code = icons.plugin, w = 18, h = 18, tint = kit.tint(status.tint, "") },
		},
		ui.column {
			grow = 1,
			shrink = 1,
			min_w = 0,
			gap = "$space.2",
			ui.column {
				gap = 3,
				ui.row {
					gap = "$space.2",
					items = "center",
					ui.text {
						plugin.name,
						text_size = "$terminal.type.xl",
						weight = "$font.weight.semibold",
						color = theme.text,
					},
					ui.show(
						status.badge ~= nil,
						ui.row {
							variants = kit.chip { kind = "badge" },
							bg = kit.tint(status.tint, "24"),
							ui.text {
								status.badge or "",
								text_size = "$terminal.type.xxs",
								weight = "$font.weight.semibold",
								color = kit.tint(status.tint, ""),
							},
						}
					),
				},
				ui.text { byline, hidden = byline == "", text_size = "$terminal.type.sm", color = theme.tertiary },
			},
			ui.text {
				plugin.description,
				hidden = plugin.description == "",
				wrap = true,
				text_size = "$terminal.type.body",
				line_height = 18,
				color = theme.secondary,
			},
			ui.text {
				plugin.error or "",
				hidden = plugin.error == nil,
				wrap = true,
				text_size = "$terminal.type.md",
				color = "$terminal.error_text",
			},
			ui.row(chips),
		},
		control,
	}
end

-- The plugins in the plugins folder: a page of cards, at a readable width.
local function plugins_view(): Node
	return ui.scroll {
		grow = 1,
		w_full = true,
		items = "center",
		ui.column {
			w_full = true,
			max_w = 720,
			px = 24,
			pt = 26,
			pb = 28,
			gap = 18,
			items = "stretch",
			ui.row {
				gap = 12,
				items = "end",
				ui.column {
					grow = 1,
					shrink = 1,
					min_w = 0,
					gap = 4,
					ui.text {
						"Plugins",
						text_size = "$terminal.type.title",
						weight = "$font.weight.semibold",
						color = theme.text,
					},
					ui.text {
						function()
							local n = #state.plugins:get()
							local count = if n == 1 then "1 plugin" else `{n} plugins`
							return `{count} in {state.plugins_dir}`
						end,
						text_size = "$terminal.type.md",
						color = theme.tertiary,
						truncate = true,
					},
				},
				action(icons.refresh, "Look Again", function()
					ui.invoke("plugins_rescan")
				end),
				action(icons.folder, "Open Folder", function()
					ui.invoke("open_plugins_folder")
				end),
			},
			-- Unkeyed: a plugin whose status changes is built again.
			ui.each(state.plugins, card, { direction = "column", gap = 10 }),
			ui.column {
				hidden = function()
					return #state.plugins:get() > 0
				end,
				py = 36,
				gap = 10,
				items = "center",
				ui.svg { code = icons.plugin, w = 28, h = 28, tint = theme.quaternary },
				ui.text {
					"No plugins yet",
					text_size = "$terminal.type.xl",
					weight = "$font.weight.semibold",
					color = theme.secondary,
				},
				ui.text {
					"A plugin is a folder holding plugin.json (its name, what it asks for, its panels) and init.luau. Put one in the plugins folder and look again.",
					wrap = true,
					align = "center",
					max_w = 420,
					text_size = "$terminal.type.md",
					color = theme.tertiary,
				},
			},
		},
	}
end

-- What shows while a plugin's panel cannot: why, and what to do.
local function waiting(plugin_id: string, panel_id: string): Node
	local function why(): (string, string?)
		local plugin = sessions.plugin(plugin_id)
		if not plugin then
			return `The plugin “{plugin_id}” is not installed.`, nil
		elseif plugin.status == "pending" then
			return `“{plugin.name}” is waiting for you to allow it.`, "Review"
		elseif plugin.status == "off" then
			return `“{plugin.name}” is turned off.`, "Turn On"
		elseif plugin.status == "broken" then
			return plugin.error or `“{plugin.name}” does not load.`, nil
		end
		return `“{plugin.name}” has no panel “{panel_id}”.`, nil
	end
	return ui.column {
		grow = 1,
		items = "center",
		justify = "center",
		gap = 10,
		p = 20,
		ui.svg { code = icons.plugin, w = 26, h = 26, tint = theme.quaternary },
		ui.text {
			function()
				return (why())
			end,
			wrap = true,
			align = "center",
			text_size = "$terminal.type.body",
			color = theme.secondary,
		},
		ui.button {
			function()
				local _, action = why()
				return action or ""
			end,
			hidden = function()
				local _, action = why()
				return action == nil
			end,
			variants = kit.button { size = "xs" },
			on_click = function()
				local plugin = sessions.plugin(plugin_id)
				if plugin and plugin.status == "off" then
					consent.decide(plugin_id, true)
				else
					state.consent_later:set_key(plugin_id, nil)
				end
			end,
		},
	}
end

function M.Body(pane: string, tab: string, inner: () -> number): Node
	local kind = (state.kinds:peek())[pane] or ""
	local plugin_id, panel_id = sessions.panel_of(pane)
	local field = ui.ref()
	-- Asked for keyboard focus (the pane was focused): the panel takes it.
	ui.effect(function()
		local el = field:get()
		if el and state.panel_focus:get() == pane then
			state.panel_focus:set("")
			el:focus()
		end
	end)
	local function running(): boolean
		local plugin = sessions.plugin(plugin_id)
		if not plugin or plugin.status ~= "enabled" then
			return false
		end
		for _, panel in plugin.panels do
			if panel.id == panel_id then
				return true
			end
		end
		return false
	end
	local body: Node
	if kind == "plugins" then
		body = plugins_view()
	elseif plugin_id and panel_id then
		body = ui.overlay {
			grow = 1,
			min_h = 0,
			items = "stretch",
			ui.column {
				grow = 1,
				min_h = 0,
				items = "stretch",
				hidden = function()
					return not running()
				end,
				ui.slot(`panel:{panel_id}`, {
					from = plugin_id,
					context = { pane = pane, tab = tab, radius = inner() },
					grow = 1,
					min_h = 0,
				}),
			},
			ui.column {
				grow = 1,
				items = "stretch",
				hidden = running,
				waiting(plugin_id, panel_id),
			},
		}
	else
		body = ui.text { `Nothing shows “{kind}”.`, p = 20, color = theme.tertiary }
	end
	return ui.column {
		grow = 1,
		min_h = 0,
		items = "stretch",
		focusable = true,
		ref = field,
		-- A press anywhere in it makes it the tab's focused pane (what it
		-- pressed keeps keyboard focus).
		on_press = function()
			if (state.focus:peek())[tab] ~= pane then
				state.focus:set_key(tab, pane)
			end
		end,
		body,
	}
end

return M
