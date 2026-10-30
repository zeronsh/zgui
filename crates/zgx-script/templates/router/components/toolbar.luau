-- Back, forward and settings: each through the router, so the history
-- the mouse's buttons and the keys walk is the same.
local tokens = require("@app/theme")

export type Props = { router: Router }

return ui.component(function(props: Props)
	local router = props.router
	return ui.row {
		px = "$space.3",
		py = "$space.2",
		gap = "$space.2",
		items = "center",
		bg = tokens.card,
		ui.button {
			id = "back",
			label = "Back",
			"‹",
			disabled = function()
				return not router.can_back:get()
			end,
			on_click = router.back,
		},
		ui.button {
			id = "forward",
			label = "Forward",
			"›",
			disabled = function()
				return not router.can_forward:get()
			end,
			on_click = router.forward,
		},
		ui.text { "Notes", bold = true, grow = 1 },
		ui.button {
			id = "open-settings",
			"Settings",
			on_click = function()
				router.navigate("settings")
			end,
		},
	}
end)
