--!strict
-- Code: a code editor for files of any length. Lines are shaped and
-- highlighted as they show (Rust, Luau and JSON), with line numbers, the
-- current line, find and replace with every match marked, multi-line
-- selection and copy, undo and redo, Tab indenting, soft wrap and an IME's
-- composition, all the editor's own.
local state = require("./state")
local actions = require("./actions")
local highlight = require("@lib/highlight")
local FindBar = require("@components/find_bar")
local StatusBar = require("@components/status_bar")

local function command(id: string, title: string, menu: string, key: string?, run: () -> ())
	ui.command { id = id, title = title, menu = menu, key = key, run = run }
end
command("new", "New", "File", "cmd+n", actions.new)
command("open", "Open…", "File", "cmd+o", actions.open)
command("save", "Save", "File", "cmd+s", actions.save)
command("save-as", "Save As…", "File", "cmd+shift+s", actions.save_as)
command("sample", "Open a Sample of 100,000 Lines", "File", nil, function()
	actions.sample(100000)
end)
command("find", "Find…", "Edit", "cmd+f", actions.find)
command("find-next", "Find Next", "Edit", "cmd+g", actions.find_next)
command("find-previous", "Find Previous", "Edit", "cmd+shift+g", actions.find_previous)
command("wrap", "Soft Wrap", "View", "alt+z", function()
	state.wrap:update(function(on)
		return not on
	end)
end)

-- A file opened with the app (`zgui-run code_editor/ main.rs`, or the
-- system's Open With): shown.
ui.on_open(function(e)
	if e.files[1] then
		actions.open_file(e.files[1])
	end
end)

-- The search's matches, marked behind the text; the current one stronger.
local function decorations(): { { any } }
	if not state.find_open:get() then
		return {}
	end
	local list = actions.matches:get().list
	local current = state.current:get()
	local out: { { any } } = {}
	for i, match in list do
		out[i] = { match[1], match[2], if i == current then "$warning" else "$selection" }
	end
	return out
end

-- An editor for `language`, coloured by its tokenizer (none for plain
-- text). A new language makes a new editor: its highlighter is its own.
local function editor(language: string): Node
	return ui.textarea {
		id = "editor",
		ref = actions.editor,
		label = "Code",
		value = state.text,
		line_numbers = true,
		wrap = state.wrap,
		font = "mono",
		size = 13,
		grow = 1,
		w_full = true,
		h = 0,
		rounded = 0,
		bg = "$background",
		highlight = (highlight :: any)[language],
		decorations = decorations,
		current_line = "$hover",
		tab = "    ",
		on_caret = function(line, column, anchor, focus)
			state.caret:set { line = line, column = column, anchor = anchor, focus = focus }
		end,
	}
end

return ui.column {
	w_full = true,
	h_full = true,
	bg = "$background",
	ui.show(state.find_open, FindBar()),
	ui.switch(state.language, {
		rust = editor("rust"),
		lua = editor("lua"),
		json = editor("json"),
	}, editor("text")),
	StatusBar(),
}
