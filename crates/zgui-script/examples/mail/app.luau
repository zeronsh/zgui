-- Mail: a sidebar of mailboxes, a virtualized message list, the open
-- message's selectable body, a compose sheet, and a settings window.
local state = require("./state")
local actions = require("./actions")
local Sidebar = require("@components/sidebar")
local List = require("@components/list")
local Detail = require("@components/detail")
local Compose = require("@components/compose")
local Settings = require("@components/settings")

-- Dark or light, as the settings say.
ui.effect(function()
	ui.theme.set(if state.dark:get() then "zgui-dark" else "zgui-light")
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

ui.window { id = "settings", title = "Settings", width = 420, height = 260, open = state.settings_open, Settings() }

return ui.row {
	w_full = true,
	h_full = true,
	color = "$text.primary",
	ui.div { absolute = true, top = 0, left = 0, right = 0, h = 36, window_drag = true },
	Sidebar(),
	ui.div { w = 1, h_full = true, bg = "$border.subtle" },
	List(),
	ui.div { w = 1, h_full = true, bg = "$border.subtle" },
	Detail(),
	Compose(),
}
