-- A small pill of text, from a package several apps share (the plugins
-- example lists it as `kit`): read only, run as the app's own code.
local kit = {}

function kit.badge(text: string | () -> string)
	return ui.text {
		text,
		px = 8,
		py = 2,
		rounded = "$radius.full",
		bg = "$accent",
		color = "$accent.text",
		text_size = "$font.size.sm",
	}
end

return kit
