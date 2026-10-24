--!strict
-- Find and replace: the query and its matches (the current one of how
-- many), Previous and Next, the replacement, Replace and Replace All.
local state = require("../state")
local actions = require("../actions")

return function(): Node
	local function on_key(e: KeyEvent): boolean?
		if e.key == "Enter" then
			if e.shift then
				actions.find_previous()
			else
				actions.find_next()
			end
			return true
		elseif e.key == "Escape" then
			actions.close_find()
			return true
		end
		return nil
	end
	local count = function(): string
		local matches = actions.count:get()
		local n = matches.n
		if state.query:get() == "" then
			return ""
		elseif n == 0 then
			return "No results"
		end
		local current = state.current:get()
		local more = if matches.more then "+" else ""
		return if current > 0 then `{current} of {n}{more}` else `{n}{more} matches`
	end
	return ui.row {
		id = "find-bar",
		gap = 8,
		px = 12,
		py = 6,
		items = "center",
		bg = "$surface.raised",
		border_sides = { 0, 0, 1, 0 },
		border_color = "$border",
		ui.input {
			id = "find",
			label = "Find",
			placeholder = "Find",
			value = state.query,
			w = 220,
			select_on_focus = true,
			on_key = on_key,
			on_mount = function(el)
				el:focus()
			end,
		},
		ui.text { id = "find-count", count, color = "$text.secondary", min_w = 90 },
		ui.button { id = "find-previous", tooltip = "Previous (Shift+Enter)", on_click = actions.find_previous, "↑" },
		ui.button { id = "find-next", tooltip = "Next (Enter)", on_click = actions.find_next, "↓" },
		ui.checkbox { id = "find-case", label = "Match Case", value = state.case },
		ui.input {
			id = "replace",
			label = "Replace",
			placeholder = "Replace",
			value = state.replacement,
			w = 180,
			on_key = function(e: KeyEvent): boolean?
				if e.key == "Enter" then
					actions.replace()
					return true
				elseif e.key == "Escape" then
					actions.close_find()
					return true
				end
				return nil
			end,
		},
		ui.button { id = "replace-one", on_click = actions.replace, "Replace" },
		ui.button { id = "replace-all", on_click = actions.replace_all, "Replace All" },
		ui.div { grow = 1 },
		ui.button { id = "find-close", tooltip = "Close (Escape)", on_click = actions.close_find, "×" },
	}
end
