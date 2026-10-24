-- The agents sidebar: every coding agent running in a pane, with what it
-- is doing, most urgent first. A row opens its agent's tab and focuses it.
-- It slides open and closed; its toggle (Zeron's sidebar icon) stays beside
-- the traffic lights, over the sidebar's header and the titlebar alike.
local theme = require("../theme")
local state = require("../state")
local agents = require("../agents")
local icons = require("../icons")
local controls = require("./controls")
local kit = require("./kit")

local M = {}

local WIDTH = 240
-- Where the toggle sits: past the traffic lights, centred in the titlebar.
-- The titlebar keeps this much room before its tabs while the sidebar is
-- closed (tabs.luau).
M.TOGGLE_LEFT = 83
M.TOGGLE_SIZE = 28
-- The sidebar's motion, for what moves with it.
M.EASE = "cubicBezier(.22, 1, .36, 1)"
M.DURATION = 280

local sidebar = state.sidebar

-- Relative times ("now", "4m", "2h", "3d") follow this, a tick a minute: no
-- frames between.
local tick = ui.state("tick", 0)
ui.every(60000, function()
	tick:set(tick:peek() + 1)
end)
local function ago(since: number): string
	tick:get()
	local s = math.max(0, os.time() - since)
	if s < 60 then
		return "now"
	elseif s < 3600 then
		return `{s // 60}m`
	elseif s < 86400 then
		return `{s // 3600}h`
	end
	return `{s // 86400}d`
end

-- Zeron's sidebar icon (icons.zeron.sh, "Sidebar"): a rounded frame and a
-- panel inside it that widens as the sidebar opens.
local function sidebar_icon(size: number): Node
	local k = size / 24
	local function panel_width(): number
		return (if sidebar:peek() then 5.5 else 1.75) * k
	end
	return ui.overlay {
		w = size,
		h = size,
		shrink = 0,
		items = "start",
		justify = "start",
		-- The 18 x 16 frame, its 1.75 stroke centred on the edge.
		ui.div {
			absolute = true,
			left = 2.125 * k,
			top = 3.125 * k,
			w = 19.75 * k,
			h = 17.75 * k,
			rounded = 4.875 * k,
			border = 1.75 * k,
			border_color = theme.secondary,
		},
		ui.div {
			absolute = true,
			left = 6.5 * k,
			top = 7.5 * k,
			h = 9 * k,
			w = panel_width(),
			rounded = 0.875 * k,
			bg = theme.secondary,
			on_mount = function(el: Element)
				ui.effect(function()
					local open = sidebar:get()
					anime.animate(el, {
						width = (if open then 5.5 else 1.75) * k,
						duration = M.DURATION,
						ease = M.EASE,
					})
				end)
			end,
		},
	}
end

-- The toggle: one button, placed over the sidebar's header and the titlebar
-- (app.luau), so it never moves while the sidebar opens or closes.
function M.Toggle(): Node
	return ui.button {
		variants = kit.button { tone = "ghost", size = "icon" },
		absolute = true,
		left = M.TOGGLE_LEFT,
		top = (theme.titlebar - M.TOGGLE_SIZE) / 2,
		on_click = function()
			sidebar:set(not sidebar:peek())
		end,
		sidebar_icon(16),
	}
end

-- An agent's status as a mark before its words: an arc turning while it
-- works, a dot when it needs the user (amber) or failed (red), a check when
-- it finished unseen, a hollow ring at rest. A change cross-fades.
local function mark(agent: () -> agents.Agent): Node
	local function arc(): Node
		-- A quarter arc turning on a faint ring, 30 frames a second.
		return ui.div {
			w = 10,
			h = 10,
			-- The arc's box counts from the ring's.
			relative = true,
			rounded = "$radius.full",
			border = 1.5,
			border_color = "$terminal.button.hover",
			ui.div {
				absolute = true,
				left = -1.5,
				top = -1.5,
				w = 10,
				h = 10,
				rounded = "$radius.full",
				border_over = true,
				border = 1.5,
				border_color = theme.accent,
				clip_inset = { 0, 0, 5, 5 },
				on_mount = function(el: Element)
					anime.animate(el, { rotate = 360, duration = 900, ease = "linear", loop = true, frameRate = 30 })
				end,
			},
		}
	end
	local function dot(color: string): Node
		return ui.div { w = 6, h = 6, rounded = "$radius.full", bg = color }
	end
	return ui.presence(function()
		return agent().status
	end, function(status: agents.Status): Node
		return ui.overlay {
			w = 10,
			h = 10,
			items = "center",
			justify = "center",
			if status == "working"
				then arc()
				elseif status == "blocked" then dot(theme.bell)
				elseif status == "error" then dot(theme.danger)
				elseif status == "done" then ui.svg { code = icons.check, w = 11, h = 11, tint = theme.secondary }
				else ui.div { w = 6, h = 6, rounded = "$radius.full", border = 1.25, border_color = theme.tertiary },
		}
	end, {
		w = 10,
		h = 10,
		shrink = 0,
		enter = { opacity = { 0, 1 }, scale = { 0.6, 1 }, duration = 220, ease = "outBack(1.6)" },
		exit = { opacity = 0, scale = 0.6, duration = 120, ease = "inQuad" },
	})
end

-- What each status says, and a blocked agent's wait in words.
local SAYS: { [agents.Status]: string } =
	{ working = "Working", blocked = "Needs you", error = "Failed", done = "Done", idle = "Idle" }
local WAITS = { permission = "Needs permission", question = "Has a question", auth = "Needs a login" }

-- One agent: its program's icon; what it works on, and how long ago its
-- status began; its status and where it runs.
local function row(agent: () -> agents.Agent): Node
	local function current(): boolean
		local a = agent()
		return state.active:get() == a.tab and (state.focus:get())[a.tab] == a.pane
	end
	local element: Element? = nil
	local function to(params: ElementParams)
		if element then
			anime.animate(element, params)
		end
	end
	local hover_layer, hover = controls.hover_glass("$terminal.button.radius")
	return ui.button {
		id = "agent:" .. agent().pane,
		-- The glass layers below are its hover and its selection: none of a
		-- button's own fills (they are solid surfaces).
		variants = kit.row { kind = "agent" },
		on_mount = function(el: Element)
			element = el
		end,
		-- Hover washes in and out; a press gives a little, as glass does.
		on_hover = hover,
		on_press = function()
			to { scale = 0.985, duration = 90, ease = "outQuad" }
		end,
		on_release = function()
			to { scale = 1, duration = 260, ease = "outBack(2)" }
		end,
		on_click = function()
			agents.open(agent())
		end,
		-- The active agent's frosted glass (one piece, sliding between rows)
		-- over a fainter glass while the pointer is on the row.
		hover_layer,
		controls.glass("agent-active", current, "$terminal.button.radius"),
		controls.glyph(
			function()
				local a = agent()
				return { name = a.name, icon = a.icon, tint = a.tint, busy = true }
			end,
			16,
			function()
				return false
			end
		),
		ui.column {
			grow = 1,
			shrink = 1,
			min_w = 0,
			gap = 3,
			items = "stretch",
			ui.row {
				gap = "$space.2",
				items = "center",
				ui.text {
					function()
						local a = agent()
						return if a.name ~= "" then a.name else a.where
					end,
					text_size = "$terminal.type.lg",
					weight = "$font.weight.medium",
					color = theme.text,
					truncate = true,
					grow = 1,
					shrink = 1,
					min_w = 0,
				},
				ui.text {
					function()
						return ago(agent().since)
					end,
					text_size = "$terminal.type.xs",
					color = theme.tertiary,
					tabular = true,
					shrink = 0,
				},
			},
			ui.row {
				gap = 6,
				items = "center",
				mark(agent),
				ui.text {
					function()
						local a = agent()
						local says = if a.status == "blocked"
							then WAITS[a.kind or ""] or SAYS.blocked
							else SAYS[a.status]
						return if a.where ~= "" then `{says} · {a.where}` else says
					end,
					text_size = "$terminal.type.sm",
					color = function()
						return if agent().status == "blocked" then theme.bell else theme.tertiary
					end,
					truncate = true,
					shrink = 1,
					min_w = 0,
				},
			},
		},
	}
end

function M.Sidebar(): Node
	return ui.div {
		-- Opening slides it in: the space eases, the list lays out at once.
		layout = "width",
		layout_clip = true,
		layout_ease = M.EASE,
		layout_duration = M.DURATION,
		w = function()
			return if sidebar:get() then WIDTH else 0
		end,
		h_full = true,
		shrink = 0,
		-- Mounted while open: a closed sidebar keeps no rows or timers. Closing,
		-- it stays (fading) while the width eases shut.
		ui.presence(function()
			return if sidebar:get() then true else nil
		end, function()
			return ui.column {
				w = WIDTH,
				h_full = true,
				items = "stretch",
				-- The header: room for the traffic lights and the toggle
				-- (drawn over it, app.luau); it drags the window.
				ui.row {
					h = theme.titlebar,
					shrink = 0,
					controls.drag_space { grow = 1 },
				},
				ui.row {
					px = "$space.4",
					pt = "$space.1",
					pb = 6,
					gap = 6,
					items = "center",
					shrink = 0,
					ui.text {
						"Agents",
						text_size = "$terminal.type.md",
						weight = "$font.weight.semibold",
						color = theme.secondary,
					},
					ui.text {
						function()
							return tostring(#agents.list:get())
						end,
						text_size = "$terminal.type.md",
						color = theme.tertiary,
					},
				},
				ui.scroll {
					grow = 1,
					min_h = 0,
					-- The list fades out at its top and bottom edges.
					fade = { 14, 18 },
					fade_overflow = true,
					ui.column {
						px = "$space.2",
						pt = 2,
						pb = "$space.3",
						gap = 1,
						items = "stretch",
						ui.show(
							function()
								return #agents.list:get() == 0
							end,
							ui.text {
								function()
									-- A session server from before an update keeps the
									-- shells but cannot report them.
									return if agents.unsupported:get()
										then "Restart zterm's sessions to list agents"
										else "No agents running"
								end,
								px = "$space.2",
								pt = 6,
								text_size = "$terminal.type.body",
								color = theme.tertiary,
								wrap = true,
							}
						),
						ui.each(agents.list, function(agent: agents.Agent)
							-- The row reads its agent's latest status from the list.
							local function latest(): agents.Agent
								for _, a in agents.list:get() do
									if a.pane == agent.pane then
										return a
									end
								end
								return agent
							end
							return row(latest)
						end, {
							key = function(agent: agents.Agent)
								return agent.pane
							end,
							-- Re-sorting by status slides rows to their places.
							animate = { duration = 260, ease = "outQuint" },
							enter = { opacity = { 0, 1 }, y = { 4, 0 }, duration = 220, ease = "outQuad" },
							exit = { opacity = 0, duration = 140 },
						}),
					},
				},
			}
		end, { w = WIDTH, h_full = true, exit = { opacity = 0, duration = M.DURATION, ease = M.EASE } }),
	}
end

return M
