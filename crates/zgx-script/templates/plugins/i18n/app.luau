-- Tasks, made to be extended: plugins (in plugins/, listed in zgx.json)
-- add to it, filter it and call what it allows, without a slot for each.
-- What it offers them is in zgx.json's `expose`:
-- - `tasks`, shared state they read but don't write, and `focus`, theirs;
-- - `complete`, an action this script defines (`ui.actions`);
-- - `#tasks`, a list they may filter. Every `id` is a target to add
--   beside; `hook = "toolbar.add"` is one it promises to keep, and
--   `hook = false` keeps the Clear button out of every plugin's reach.
-- Its text is messages (`ui.t`) in locales/en.json; the plugin's is its own.
ui.i18n { dir = "locales" }
local tokens = require("./theme")
local TaskRow = require("@components/task_row")

type Task = TaskRow.Task

local tasks = ui.shared(
	"tasks",
	{
		{ id = 1, title = "Read the plugin in plugins/focus", done = false },
		{ id = 2, title = "Make the app", done = true },
	} :: { Task }
)
local draft = ui.state("draft", "")
local next_id = 3

ui.actions {
	complete = function(id: number)
		local list = table.clone(tasks:peek())
		for i, task in list do
			if task.id == id then
				list[i] = { id = task.id, title = task.title, done = true }
			end
		end
		tasks:set(list)
	end,
}

local function add()
	local title = draft:peek()
	if title == "" then
		return
	end
	tasks:push { id = next_id, title = title, done = false }
	next_id += 1
	draft:set("")
end

local function clear_done()
	local open = {}
	for _, task in tasks:peek() do
		if not task.done then
			table.insert(open, task)
		end
	end
	tasks:set(open)
end

return ui.column {
	w_full = true,
	h_full = true,
	ui.row {
		id = "toolbar",
		px = "$space.4",
		py = "$space.3",
		gap = "$space.2",
		items = "center",
		bg = tokens.card,
		ui.text { id = "title", ui.t("title"), bold = true, text_size = "$font.size.lg", grow = 1 },
		ui.button { id = "add", hook = "toolbar.add", ui.t("add"), on_click = add },
		ui.button { id = "clear", hook = false, ui.t("clear"), on_click = clear_done },
	},
	ui.column {
		p = "$space.4",
		gap = "$space.2",
		grow = 1,
		ui.input {
			id = "new",
			label = ui.t("new.label"),
			placeholder = ui.t("new.placeholder"),
			value = draft,
			on_commit = add,
		},
		ui.each(tasks, function(task: Task)
			return TaskRow { task = task }
		end, {
			id = "tasks",
			gap = "$space.1",
			key = function(task: Task)
				return task.id
			end,
		}),
	},
}
