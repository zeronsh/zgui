-- The signed-in user's initial, read from context.
return ui.component(function()
	local user = ui.use_context("user")
	return ui.text {
		function()
			return string.sub(user:get().name, 1, 1)
		end,
		w = 28,
		h = 28,
		rounded = "$radius.full",
		bg = "$accent",
		color = "$accent.text",
		align = "center",
	}
end)
