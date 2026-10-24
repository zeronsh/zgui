-- A plugin found that the user has not allowed (new, or a version asking
-- for more) asks first: a sheet saying what it is and what it would be
-- able to do. It runs only once allowed; Don't Allow keeps it off (the
-- plugins panel turns it on later), Not Now asks again next launch.
--
-- Only a click answers: the sheet comes up on its own, and a Return
-- meant for a terminal must not allow anything.
local theme = require("../theme")
local icons = require("../icons")
local state = require("../state")
local kit = require("./kit")

local later = state.consent_later

local M = {}

-- The plugin waiting for an answer, first by name.
function M.waiting(): state.PluginInfo?
	if state.confirm:get() ~= "" then
		return nil
	end
	local put_off = later:get()
	for _, plugin in state.plugins:get() do
		if plugin.status == "pending" and not put_off[plugin.id] then
			return plugin
		end
	end
	return nil
end

function M.decide(id: string, allowed: boolean)
	ui.invoke("plugin_decide", { id = id, allowed = allowed })
end

local function permission(text: string): Node
	return ui.row {
		gap = "$space.2",
		items = "start",
		ui.div { w = 5, h = 5, mt = 6, rounded = "$radius.full", shrink = 0, bg = theme.secondary },
		ui.text { text, wrap = true, text_size = "$terminal.type.body", color = theme.text, grow = 1, min_w = 0 },
	}
end

local function sheet(plugin: state.PluginInfo): Node
	local asks: { Node } = {}
	for _, p in plugin.permissions do
		table.insert(asks, permission(p.text or p.id))
	end
	local byline = plugin.version
	if plugin.author ~= "" then
		byline = if byline ~= "" then `{byline} · {plugin.author}` else plugin.author
	end
	return ui.div {
		variants = kit.scrim,
		ui.column {
			variants = kit.sheet,
			w = 380,
			ui.row {
				gap = 10,
				items = "center",
				ui.svg { code = icons.plugin, w = 20, h = 20, shrink = 0, tint = theme.secondary },
				ui.column {
					grow = 1,
					shrink = 1,
					min_w = 0,
					ui.text {
						`Allow “{plugin.name}” to run?`,
						wrap = true,
						text_size = "$terminal.type.xl",
						bold = true,
						color = theme.text,
					},
					ui.text {
						byline,
						hidden = byline == "",
						text_size = "$terminal.type.sm",
						color = theme.tertiary,
					},
				},
			},
			ui.text {
				plugin.description,
				hidden = plugin.description == "",
				wrap = true,
				mt = "$space.1",
				text_size = "$terminal.type.body",
				color = theme.secondary,
			},
			ui.text {
				if #asks > 0 then "It will be able to:" else "It asks for nothing beyond showing its own panels.",
				mt = "$space.2",
				text_size = "$terminal.type.body",
				color = theme.secondary,
			},
			ui.column { gap = "$space.1", pl = 2, table.unpack(asks) },
			ui.text {
				`From {state.plugins_dir}/{plugin.id}`,
				mt = "$space.2",
				text_size = "$terminal.type.xs",
				color = theme.quaternary,
				truncate = true,
			},
			ui.row {
				mt = "$space.3",
				gap = "$space.2",
				ui.button {
					"Not Now",
					id = "consent-later",
					variants = kit.button { tone = "quiet" },
					px = "$space.1",
					on_click = function()
						later:set_key(plugin.id, true)
					end,
				},
				ui.div { grow = 1 },
				ui.button {
					"Don't Allow",
					id = "consent-deny",
					variants = kit.button,
					on_click = function()
						M.decide(plugin.id, false)
					end,
				},
				ui.button {
					"Allow",
					id = "consent-allow",
					variants = kit.button { tone = "primary" },
					on_click = function()
						M.decide(plugin.id, true)
					end,
				},
			},
		},
	}
end

function M.Sheet(): Node
	return ui.presence(
		function()
			local plugin = M.waiting()
			-- A new sheet for each plugin, and for each version of what it asks.
			return if plugin then plugin else nil
		end,
		sheet,
		{
			absolute = true,
			top = 0,
			left = 0,
			right = 0,
			bottom = 0,
			enter = { opacity = { 0, 1 }, duration = 160 },
		}
	)
end

return M
