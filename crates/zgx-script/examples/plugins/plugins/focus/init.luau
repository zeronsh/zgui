-- Focus: a button beside the app's Add (its declared hook, open to every
-- plugin) and, while it is on, a filter on the list hiding what is done
-- (filtering takes Capability::Override("#tasks")). `focus` and `tasks`
-- are shared state the host exposes to plugins.
local focus = ui.shared("focus", false)

ui.contribute {
	target = "hook:toolbar.add",
	after = ui.button {
		id = "focus",
		on_click = ui.toggle(focus),
		ui.text {
			function()
				return if focus:get() then "Show all" else "Focus"
			end,
		},
	},
}

ui.filter {
	target = "#tasks",
	items = function(items)
		if not focus:get() then
			return items
		end
		local open = {}
		for _, task in items do
			if not task.done then
				table.insert(open, task)
			end
		end
		return open
	end,
}
