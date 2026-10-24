-- Mail: a sidebar of mailboxes, a virtualized message list, the open
-- message's selectable body, a compose sheet, and a settings window.
-- Narrower windows fold it up as a web mail client does: the mailboxes
-- become a drawer, then one pane shows at a time.
local state = require("./state")
local actions = require("./actions")
local Sidebar = require("@components/sidebar")
local List = require("@components/list")
local Detail = require("@components/detail")
local Compose = require("@components/compose")
local Settings = require("@components/settings")

-- The system's appearance, unless the settings choose one.
ui.effect(function()
	local appearance = state.appearance:get()
	if appearance == "system" then
		ui.theme.follow_system { dark = "zgx-dark", light = "zgx-light" }
	else
		ui.theme.set(if appearance == "dark" then "zgx-dark" else "zgx-light")
	end
end)

ui.command {
	id = "new",
	title = "New Message",
	menu = "File",
	key = "cmd+n",
	run = function()
		actions.compose()
	end,
}
ui.command {
	id = "settings",
	title = "Settings…",
	menu = "File",
	key = "cmd+,",
	run = function()
		state.settings_open:set(true)
	end,
}
ui.command {
	id = "receive",
	title = "Get New Mail",
	menu = "Mailbox",
	key = "cmd+shift+n",
	run = function()
		actions.receive()
	end,
}
ui.command {
	id = "reply",
	title = "Reply",
	menu = "Message",
	key = "cmd+r",
	run = function()
		local id = state.selected:peek()
		if id then
			actions.reply(id)
		end
	end,
}

ui.window { id = "settings", title = "Settings", width = 420, height = 300, open = state.settings_open, Settings() }

local function divider(): Node
	return ui.div { w = 1, h_full = true, shrink = 0, bg = "$border.subtle" }
end

return ui.row {
	ref = state.root,
	w_full = true,
	h_full = true,
	color = "$text.primary",
	ui.show(function()
		return state.layout:get() == "wide"
	end, ui.row { h_full = true, shrink = 0, Sidebar(), divider() }),
	-- One pane at a time when narrow: the list, or the open message.
	ui.show(function()
		return state.layout:get() ~= "narrow" or state.selected:get() == nil
	end, List()),
	ui.show(function()
		return state.layout:get() ~= "narrow"
	end, divider()),
	ui.show(function()
		return state.layout:get() ~= "narrow" or state.selected:get() ~= nil
	end, Detail()),
	-- The strip under the title bar moves the window: over the panes (an
	-- absolute node paints in its place among its siblings), under the
	-- drawer and the composer.
	ui.div { absolute = true, top = 0, left = 0, right = 0, h = 36, window_drag = true },
	-- The mailboxes over the list, while the window is too narrow for them.
	ui.show(
		function()
			return state.drawer:get() and state.layout:get() ~= "wide"
		end,
		ui.row {
			absolute = true,
			top = 0,
			left = 0,
			bottom = 0,
			right = 0,
			ui.row { h_full = true, shrink = 0, shadow = "$shadow.lg", Sidebar() },
			-- A click beside it closes it.
			ui.div { grow = 1, h_full = true, on_click = ui.set(state.drawer, false) },
		}
	),
	Compose(),
}
