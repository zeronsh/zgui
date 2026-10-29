-- The app's row with a badge on what is done. It runs as the app's code,
-- so it reaches the original (`@app-original/…`) and the app's package.
local original = require("@app-original/components/task_row")
local kit = require("@kit/badge")

return function(task)
	if not task.done then
		return original(task)
	end
	return ui.row { gap = 8, items = "center", original(task), kit.badge("done") }
end
