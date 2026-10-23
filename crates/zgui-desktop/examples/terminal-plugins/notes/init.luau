--!nonstrict
-- (Non-strict until promises carry their values' types in zgui.d.luau.)
-- Notes: a notepad in a pane, kept per pane across launches. It asks for
-- nothing: it only shows its panel (`panels` in plugin.json), opened from
-- ⌘K. zterm gives the component its pane in `props.slot`.
local notes = ui.persist("notes", {} :: { [string]: string })

type Props = { slot: { pane: string } }

local Notes = ui.component(function(props: Props)
	local pane = props.slot.pane
	local text = ui.use_state((notes:peek())[pane] or "")
	-- Saved as it is typed (ui.persist writes in batches).
	ui.effect(function()
		local value = text:get()
		if (notes:peek())[pane] ~= value then
			notes:set_key(pane, value)
		end
	end)
	return ui.column {
		grow = 1,
		min_h = 0,
		p = 12,
		items = "stretch",
		ui.textarea {
			label = "Notes",
			value = text,
			placeholder = "Notes for this pane",
			grow = 1,
			text_size = 13,
			bg = "#00000000",
			focus = { border = 0 },
		},
	}
end)

ui.contribute("panel:notes", Notes {})
