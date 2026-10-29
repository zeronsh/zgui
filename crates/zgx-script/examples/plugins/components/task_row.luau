-- A task's row: its title and, until it is done, a Done button (the
-- host's `complete` action).
return function(task)
	return ui.row {
		id = `task-{task.id}`,
		gap = 8,
		items = "center",
		ui.text {
			task.title,
			literal = true,
			grow = 1,
			color = if task.done then "$text.secondary" else "$text.primary",
		},
		ui.show(not task.done, ui.button { id = `done-{task.id}`, "Done", on_click = ui.action("complete", task.id) }),
	}
end
