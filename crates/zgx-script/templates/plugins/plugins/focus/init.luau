-- Focus: a button beside the app's Add (its declared hook), and while it
-- is on, a filter on the list hiding what is done. zgx.json allows it the
-- shared `tasks` (read only) and `focus`, the app's `complete` action, and
-- filtering `#tasks`; it reaches nothing else of the app's.
local focus = ui.shared("focus", false)
local tasks = ui.shared("tasks", {} :: { { id: number, done: boolean } })

ui.contribute {
	target = "hook:toolbar.add",
	after = ui.button {
		id = "focus",
		on_click = ui.toggle(focus),
		ui.text {
			function()
				if focus:get() then
					return "Show all"
				end
				local open = 0
				for _, task in tasks:get() do
					if not task.done then
						open += 1
					end
				end
				return `Focus ({open})`
			end,
		},
	},
}

-- "Done next" completes the first open task through the app's action.
ui.contribute {
	target = "hook:toolbar.add",
	after = ui.button {
		id = "done-next",
		"Done next",
		on_click = function()
			for _, task in tasks:peek() do
				if not task.done then
					ui.invoke("complete", task.id)
					return
				end
			end
		end,
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
