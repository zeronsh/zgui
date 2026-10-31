-- A task: a checkbox titled with it, which checks it off.
local tokens = require("@app/theme")

export type Task = { id: number, title: string, done: boolean }
export type Props = { task: Task, on_toggle: (id: number) -> () }

return ui.component(function(props: Props)
	local task = props.task
	return ui.row {
		id = `task-{task.id}`,
		px = "$space.3",
		py = "$space.2",
		rounded = "$radius.md",
		bg = tokens.card,
		items = "center",
		ui.checkbox {
			label = task.title,
			value = function()
				return task.done
			end,
			on_change = function()
				props.on_toggle(task.id)
			end,
		},
	}
end)
