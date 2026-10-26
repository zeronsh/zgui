-- macos_accessibility_probe.py's text: a two-paragraph note whose text,
-- selection and caret VoiceOver reads, and whose characters it finds on
-- screen.
local note = ui.state("note", "one two\nthree four")
return ui.column {
	p = 16,
	ui.input { id = "note", label = "Note", value = note, multiline = true, w = 300, h = 120 },
}
