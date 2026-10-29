-- Tasks, made to be extended: plugins (in plugins/) add to it, filter it
-- and replace parts of it without the app preparing a slot for each. What
-- the app offers them:
-- - every `id` is a target; `hook = "toolbar.add"` is one it promises
--   to keep across versions;
-- - `hook = false` keeps the Clear button out of every plugin's reach;
-- - `tasks` is shared state the host exposes to plugins read only, and
--   `complete` and `clear_done` are host actions, so plugins see the list
--   and change it only through what the host lets them;
-- - rows come from `@app/components/task_row`, a module a trusted plugin
--   may replace.
local row = require("@app/components/task_row")
local kit = require("@kit/badge")

local tasks = ui.shared("tasks", {})
local draft = ui.state("draft", "")

local function add()
	local title = draft:peek()
	if title ~= "" then
		ui.invoke("add", title)
		draft:set("")
	end
end

return ui.column {
	id = "app",
	w_full = true,
	h_full = true,
	ui.row {
		id = "toolbar",
		px = 16,
		py = 12,
		gap = 8,
		items = "center",
		bg = "$surface.raised",
		ui.text { id = "title", "Tasks", bold = true, text_size = "$font.size.lg" },
		kit.badge(function()
			return tostring(#tasks:get())
		end),
		ui.div { grow = 1 },
		ui.button { id = "add", hook = "toolbar.add", "Add", on_click = add },
		ui.button { id = "clear", hook = false, "Clear done", on_click = ui.action("clear_done") },
	},
	ui.column {
		id = "body",
		p = 16,
		gap = 8,
		grow = 1,
		ui.input { id = "draft", value = draft, placeholder = "A new task", on_commit = add },
		ui.show(function()
			return #tasks:get() == 0
		end, ui.text { id = "empty", "Nothing to do.", color = "$text.secondary" }),
		ui.each(tasks, row, {
			id = "tasks",
			gap = 4,
			key = function(task)
				return task.id
			end,
		}),
	},
}
