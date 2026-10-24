--!strict
-- What the menus, keys and bars do: open and save files, make a sample,
-- search and replace, go to a line.
local state = require("./state")
local highlight = require("@lib/highlight")
local search = require("@lib/search")

local M = {}

-- The editor, for its selection.
M.editor = ui.ref()

local function editor(): Element?
	return M.editor:get()
end

-- The text shown with the caret at its start: a file just opened.
local function show(text: string, language: string, file: File?)
	state.text:set(text)
	state.saved:set(text)
	state.language:set(language)
	state.file:set(file)
	state.current:set(0)
	local el = editor()
	if el then
		el:set_selection_bytes(0)
	end
end

function M.new()
	show("", "text", nil)
	state.status:set("")
end

-- `file` read and shown (one the user picked, or the system opened).
function M.open_file(file: File)
	ui.task(function()
		local ok, text = pcall(function()
			return ui.read_file(file):await()
		end)
		if ok then
			show(text, highlight.language(file.name), file)
			state.status:set(`Opened {file.name}`)
		else
			state.status:set(`Couldn't open {file.name}`)
		end
	end)
end

function M.open()
	ui.task(function()
		local files = ui.open_files({ title = "Open a File" }):await()
		if files[1] then
			M.open_file(files[1])
		end
	end)
end

local function write(file: File)
	local text = state.text:peek()
	local ok = pcall(function()
		ui.write_file(file, text):await()
	end)
	if ok then
		state.file:set(file)
		state.saved:set(text)
		state.status:set(`Saved {file.name}`)
	else
		state.status:set(`Couldn't save {file.name}`)
	end
end

function M.save_as()
	ui.task(function()
		local current = state.file:peek()
		local file = ui.save_file({ title = "Save", name = current and current.name or "untitled.txt" }):await()
		if file then
			write(file)
			state.language:set(highlight.language(file.name))
		end
	end)
end

function M.save()
	local file = state.file:peek()
	if file then
		ui.task(function()
			write(file)
		end)
	else
		M.save_as()
	end
end

-- A Rust file of `lines` lines: something to try the editor on.
function M.sample(lines: number?)
	local n = lines or 100000
	local out = table.create(n)
	local body = {
		"/// A function, its docs and its body.",
		"pub fn compute_%d(input: &[u8], scale: f32) -> Result<Vec<u32>, Error> {",
		'    let name = "item %d"; // a comment',
		"    let total: u32 = input.iter().map(|b| *b as u32 * %d).sum();",
		"    /* a block comment",
		"       running on */",
		"    if total > 0x%x { return Err(Error::Overflow(total)); }",
		"    Ok(vec![total; scale as usize])",
		"}",
		"",
	}
	for i = 1, n do
		local template = body[(i - 1) % #body + 1]
		out[i] = if string.find(template, "%%") then string.format(template, i) else template
	end
	show(table.concat(out, "\n"), "rust", nil)
	state.status:set(`A sample of {n} lines`)
end

-- The search's matches, found where the text is held when Next, Previous
-- and Replace need them: the editor marks them itself, and the find bar
-- counts them (`M.count`), so typing a query makes no list of them.
local function matches(): { { number } }
	local query = state.query:peek()
	if query == "" or not state.find_open:peek() then
		return {}
	end
	return (state.text:find(query, { case = state.case:peek(), limit = search.LIMIT }))
end

-- How many matches there are (at most the limit), and whether there were
-- more, kept current as the text or query changes.
M.count = ui.computed(function()
	local query = state.query:get()
	if query == "" or not state.find_open:get() then
		return { n = 0, more = false }
	end
	local n, more = state.text:count(query, { case = state.case:get(), limit = search.LIMIT })
	return { n = n, more = more }
end)

-- Select match `index` (from 1) in the editor, showing it.
local function select(index: number)
	local list = matches()
	if #list == 0 then
		state.current:set(0)
		return
	end
	index = (index - 1) % #list + 1
	state.current:set(index)
	local el = editor()
	if el then
		el:set_selection_bytes(list[index][1] - 1, list[index][2])
	end
end

function M.find()
	state.find_open:set(true)
end
function M.close_find()
	state.find_open:set(false)
	state.current:set(0)
	local el = editor()
	if el then
		el:focus()
	end
end

-- The next match after the caret (or the current match), or the one before.
function M.find_next()
	local list = matches()
	local current = state.current:peek()
	if current > 0 and current <= #list then
		select(current + 1)
	else
		select(search.after(list, state.caret:peek().focus + 1))
	end
end
function M.find_previous()
	local list = matches()
	local current = state.current:peek()
	if current > 0 and current <= #list then
		select(current - 1)
	else
		select(search.after(list, state.caret:peek().focus + 1) - 1)
	end
end

-- Replace the current match (choosing the next one first if none is), as
-- typing would: one undo step each.
function M.replace()
	local list = matches()
	local current = state.current:peek()
	if current == 0 or current > #list then
		M.find_next()
		return
	end
	local el = editor()
	if el then
		local match = list[current]
		el:set_selection_bytes(match[1] - 1, match[2])
		el:replace_selection(state.replacement:peek())
		-- The matches after it moved; the one now at its place is next.
		select(search.after(matches(), match[1] + #state.replacement:peek()))
	end
end

-- Every match replaced, in one edit (one undo step).
function M.replace_all()
	local list = matches()
	if #list == 0 then
		return
	end
	local text = state.text:peek()
	local replacement = state.replacement:peek()
	local parts = table.create(#list * 2 + 1)
	local at = 1
	for _, match in list do
		parts[#parts + 1] = string.sub(text, at, match[1] - 1)
		parts[#parts + 1] = replacement
		at = match[2] + 1
	end
	parts[#parts + 1] = string.sub(text, at)
	local el = editor()
	if el then
		el:set_selection_bytes(0, #text)
		el:replace_selection(table.concat(parts))
		el:set_selection_bytes(0)
	end
	state.current:set(0)
	state.status:set(`Replaced {#list} matches`)
end

-- The caret to the start of line `line` (from 1).
function M.go_to(line: number)
	local text = state.text:peek()
	local at = 0
	for _ = 2, line do
		local next = string.find(text, "\n", at + 1, true)
		if not next then
			break
		end
		at = next
	end
	local el = editor()
	if el then
		el:set_selection_bytes(at)
		el:focus()
	end
end

return M
