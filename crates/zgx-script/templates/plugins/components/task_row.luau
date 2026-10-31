-- A task's row: its title, dimmed once done, and a Done button until then
-- (the app's `complete` action, which plugins granted it call too).
local tokens = require("@app/theme")

export type Task = { id: number, title: string, done: boolean }
export type Props = { task: Task }

return ui.component(function(props: Props)
	local task = props.task
	return ui.row {
		id = `task-{task.id}`,
		px = "$space.3",
		py = "$space.2",
		gap = "$space.2",
		rounded = "$radius.md",
		bg = tokens.card,
		items = "center",
		ui.text {
			task.title,
			literal = true,
			grow = 1,
			color = if task.done then tokens.muted else "$text.primary",
		},
		ui.show(
			not task.done,
			ui.button {
				id = `done-{task.id}`,
				label = `Done: {task.title}`,
				"Done",
				on_click = ui.action("complete", task.id),
			}
		),
	}
end)
