-- Tasks: add one, check it off, clear what is done. A starting point that
-- shows the pieces most apps use: state, a list of components, a theme of
-- the app's own and a test (tests/app.test.luau). Its text is messages
-- (`ui.t`) in locales/en.json: add locales/fr.json to translate it.
ui.i18n { dir = "locales" }
local tokens = require("./theme")
local TaskRow = require("@components/task_row")

type Task = TaskRow.Task

local tasks = ui.state("tasks", {} :: { Task })
local draft = ui.state("draft", "")
local next_id = 1

local function add()
	local title = draft:peek()
	if title == "" then
		return
	end
	tasks:push { id = next_id, title = title, done = false }
	next_id += 1
	draft:set("")
end

local function toggle(id: number)
	local list = table.clone(tasks:peek())
	for i, task in list do
		if task.id == id then
			list[i] = { id = task.id, title = task.title, done = not task.done }
		end
	end
	tasks:set(list)
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
	p = "$space.4",
	gap = "$space.3",
	ui.row {
		gap = "$space.2",
		items = "center",
		ui.text { ui.t("title"), bold = true, text_size = "$font.size.lg", grow = 1 },
		ui.text {
			id = "count",
			color = tokens.muted,
			ui.t("count", {
				n = function()
					return #tasks:get()
				end,
			}),
		},
	},
	ui.row {
		gap = "$space.2",
		ui.input {
			id = "new",
			label = ui.t("new.label"),
			placeholder = ui.t("new.placeholder"),
			value = draft,
			on_commit = add,
			grow = 1,
		},
		ui.button { id = "add", ui.t("add"), on_click = add },
	},
	ui.show(function()
		return #tasks:get() == 0
	end, ui.text { id = "empty", ui.t("empty"), color = tokens.muted }),
	ui.each(tasks, function(task: Task)
		return TaskRow { task = task, on_toggle = toggle }
	end, {
		gap = "$space.1",
		key = function(task: Task)
			return task.id
		end,
	}),
	ui.button { id = "clear", ui.t("clear"), on_click = clear_done },
}
