--!nonstrict
-- (Non-strict until slot-provided props and decoded JSON are typed: the
-- solver cannot see `props.slot` arrive or narrow API data yet.)
-- Editor: a text editor in a pane. Open a file (⌘O), edit it with its
-- lines numbered, save it (⌘S, ⇧⌘S for elsewhere). A changed file shows a
-- dot; opening another asks first.
--
-- It asks for `files`: it reads and writes only the files you pick in its
-- dialogs, nothing else on the disk.

local function icon(paths: string): string
	return '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="#fff" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round">'
		.. paths
		.. "</svg>"
end
local ICONS = {
	file = icon('<path d="M7 3.5h7l4 4v13H7z"/><path d="M14 3.5v4h4"/>'),
	open = icon('<path d="M4 7.5V18a1 1 0 0 0 1 1h14a1 1 0 0 0 1-1V9.5a1 1 0 0 0-1-1h-7l-2-2.5H5a1 1 0 0 0-1 1z"/>'),
	save = icon(
		'<path d="M6 4h10l3 3v12a1 1 0 0 1-1 1H6a1 1 0 0 1-1-1V5a1 1 0 0 1 1-1z"/><path d="M8 4v5h7V4M8 20v-6h8v6"/>'
	),
	new = icon('<path d="M7 3.5h7l4 4v13H7z"/><path d="M12.5 11v6M9.5 14h6"/>'),
}
local ACCENT = "#6ea8ff"

-- What a file's name says it holds, for the status bar.
local LANGUAGES: { [string]: string } = {
	lua = "Lua",
	luau = "Luau",
	rs = "Rust",
	toml = "TOML",
	json = "JSON",
	md = "Markdown",
	txt = "Plain Text",
	js = "JavaScript",
	ts = "TypeScript",
	tsx = "TSX",
	jsx = "JSX",
	py = "Python",
	go = "Go",
	c = "C",
	h = "C",
	cpp = "C++",
	swift = "Swift",
	sh = "Shell",
	zsh = "Shell",
	yml = "YAML",
	yaml = "YAML",
	html = "HTML",
	css = "CSS",
}
local function language(name: string): string
	local ext = name:match("%.([%w]+)$")
	return if ext then LANGUAGES[ext:lower()] or ext:upper() else "Plain Text"
end
local function count_lines(text: string): number
	local _, n = text:gsub("\n", "")
	return n + 1
end

-- The file each pane had, for the next launch to offer again (a file is
-- reachable only once picked in this run's dialog).
local recent = ui.persist("recent", {} :: { [string]: { path: string, name: string } })

type File = { path: string, name: string }
type Props = { slot: { pane: string } }

local Editor = ui.component(function(props: Props)
	local pane = props.slot.pane
	-- The pane's, not the instance's: kept when the pane moves or the
	-- plugin reloads.
	local file = ui.state(`file:{pane}`, nil :: File?)
	local text = ui.state(`text:{pane}`, "")
	-- The text as it is on the disk (or "" for a new file): unlike `text`,
	-- the file has changes to save.
	local saved = ui.state(`saved:{pane}`, "")
	local editing = ui.state(`editing:{pane}`, false)
	local status = ui.use_state("")
	-- An open waiting on unsaved changes: "open" or "new".
	local asking = ui.use_state("")
	local field = ui.ref()
	-- Asked for the keyboard (a file just opened): the text takes it once
	-- it is there.
	local wants_focus = ui.use_state(false)
	ui.effect(function()
		local el = field:get()
		if el and wants_focus:get() then
			wants_focus:set(false)
			el:focus()
		end
	end)

	local function dirty(): boolean
		return text:get() ~= saved:get()
	end
	local function say(message: string)
		status:set(message)
	end
	local function focus_text()
		wants_focus:set(true)
	end

	local function load(picked: File)
		ui.task(function()
			local ok, result = pcall(function()
				return ui.read_file(picked):await()
			end)
			if not ok then
				say(`Could not open {picked.name}: {result}`)
				return
			end
			if result:find("\0", 1, true) then
				say(`{picked.name} is not text`)
				return
			end
			ui.batch(function()
				file:set(picked)
				text:set(result)
				saved:set(result)
				editing:set(true)
				recent:set_key(pane, { path = picked.path, name = picked.name })
				say("")
			end)
			focus_text()
		end)
	end
	local function open(force: boolean?)
		if dirty() and not force then
			asking:set("open")
			return
		end
		asking:set("")
		local last = (recent:peek())[pane]
		ui.task(function()
			local ok, files = pcall(function()
				return ui.open_files({
					title = "Open a file to edit",
					directory = if last then last.path:match("^(.*)/[^/]*$") else nil,
				}):await()
			end)
			if ok and files[1] then
				load(files[1])
			elseif not ok then
				say(tostring(files))
			end
		end)
	end
	local function new(force: boolean?)
		if dirty() and not force then
			asking:set("new")
			return
		end
		asking:set("")
		ui.batch(function()
			file:set(nil)
			text:set("")
			saved:set("")
			editing:set(true)
			say("")
		end)
		focus_text()
	end
	-- Save to the file, or (`elsewhere`, or a new file) where the user says.
	local function save(elsewhere: boolean?)
		local current = file:peek()
		local body = text:peek()
		ui.task(function()
			local target = current
			if elsewhere or not target then
				local ok, chosen = pcall(function()
					return ui.save_file({ name = if current then current.name else "Untitled.txt" }):await()
				end)
				if not ok then
					say(tostring(chosen))
					return
				end
				target = chosen
			end
			if not target then
				return
			end
			local ok, err = pcall(function()
				ui.write_file(target, body):await()
			end)
			if ok then
				ui.batch(function()
					file:set(target)
					saved:set(body)
					recent:set_key(pane, { path = target.path, name = target.name })
					say(`Saved {target.name}`)
				end)
			else
				say(`Could not save: {err}`)
			end
		end)
	end
	local function on_key(e: KeyEvent): boolean
		if not e.meta then
			return false
		end
		local key = e.key:lower()
		if key == "s" then
			save(e.shift)
			return true
		elseif key == "o" then
			open()
			return true
		end
		return false
	end

	local function button(svg: string, label: string, on_click: () -> (), primary: (() -> boolean)?): Node
		return ui.button {
			h = 26,
			px = 9,
			gap = 6,
			rounded = 7,
			items = "center",
			bg = function()
				return if primary and primary() then ACCENT .. "2b" else "#00000000"
			end,
			hover = { bg = "$hover" },
			focus = { border = 0 },
			on_click = on_click,
			ui.svg {
				code = svg,
				w = 14,
				h = 14,
				tint = function()
					return if primary and primary() then ACCENT else "$text.secondary"
				end,
			},
			ui.text {
				label,
				text_size = 12,
				weight = 500,
				color = function()
					return if primary and primary() then "#a9c8ff" else "$text.secondary"
				end,
			},
		}
	end

	-- Before a file is open: what to do.
	local start = ui.column {
		grow = 1,
		items = "center",
		justify = "center",
		gap = 14,
		p = 24,
		ui.row {
			w = 52,
			h = 52,
			rounded = 14,
			items = "center",
			justify = "center",
			bg = ACCENT .. "1a",
			border = 1,
			border_color = ACCENT .. "33",
			ui.svg { code = ICONS.file, w = 26, h = 26, tint = ACCENT },
		},
		ui.column {
			gap = 4,
			items = "center",
			ui.text { "Edit a file", text_size = 15, weight = 600, color = "$text.primary" },
			ui.text {
				"Open one, or start a new one and save it where you like.",
				text_size = 12,
				color = "$text.tertiary",
			},
		},
		ui.row {
			gap = 8,
			ui.button {
				id = "editor-open",
				h = 30,
				px = 14,
				gap = 7,
				rounded = 8,
				items = "center",
				bg = ACCENT .. "2b",
				border = 1,
				border_color = ACCENT .. "40",
				hover = { bg = ACCENT .. "40" },
				focus = { border = 0 },
				on_click = function()
					open()
				end,
				ui.svg { code = ICONS.open, w = 14, h = 14, tint = ACCENT },
				ui.text { "Open File…", text_size = 12.5, weight = 500, color = "#cfe0ff" },
				ui.text { "⌘O", text_size = 11.5, color = "#a9c8ff99" },
			},
			ui.button {
				id = "editor-new",
				h = 30,
				px = 14,
				gap = 7,
				rounded = 8,
				items = "center",
				bg = "$pressed",
				hover = { bg = "$hover" },
				focus = { border = 0 },
				on_click = function()
					new()
				end,
				ui.svg { code = ICONS.new, w = 14, h = 14, tint = "$text.secondary" },
				ui.text { "New File", text_size = 12.5, weight = 500, color = "$text.primary" },
			},
		},
		ui.text {
			function()
				local last = (recent:get())[pane]
				return if last then `Last here: {last.path}` else ""
			end,
			hidden = function()
				return (recent:get())[pane] == nil
			end,
			text_size = 11.5,
			color = "$text.disabled",
			truncate = true,
			max_w = 360,
		},
	}

	-- Unsaved changes and another file asked for.
	local ask = ui.row {
		hidden = function()
			return asking:get() == ""
		end,
		h = 38,
		px = 12,
		gap = 8,
		items = "center",
		shrink = 0,
		bg = "#e8b34b14",
		ui.div { w = 6, h = 6, rounded = 3, bg = "#e8b34b" },
		ui.text {
			function()
				local f = file:get()
				return `{if f then f.name else "This file"} has changes you have not saved.`
			end,
			grow = 1,
			shrink = 1,
			min_w = 0,
			text_size = 12,
			color = "$text.primary",
			truncate = true,
		},
		button(ICONS.save, "Save", function()
			save()
			asking:set("")
		end),
		ui.button {
			"Discard",
			h = 26,
			px = 9,
			rounded = 7,
			text_size = 12,
			color = "#ff8f87",
			bg = "#00000000",
			hover = { bg = "$hover" },
			focus = { border = 0 },
			on_click = function()
				if asking:peek() == "new" then
					new(true)
				else
					open(true)
				end
			end,
		},
		ui.button {
			"Cancel",
			h = 26,
			px = 9,
			rounded = 7,
			text_size = 12,
			color = "$text.secondary",
			bg = "#00000000",
			hover = { bg = "$hover" },
			focus = { border = 0 },
			on_click = function()
				asking:set("")
			end,
		},
	}

	local editor = ui.column {
		grow = 1,
		min_h = 0,
		items = "stretch",
		ask,
		ui.textarea {
			id = "editor-text",
			label = "File contents",
			value = text,
			ref = field,
			line_numbers = true,
			font = "mono",
			text_size = 12.5,
			grow = 1,
			h = 0,
			px = 6,
			py = 10,
			rounded = 0,
			bg = "#00000000",
			color = "$text.primary",
			focus = { border = 0 },
		},
		ui.div { h = 1, shrink = 0, bg = "#ffffff0d" },
		-- The status bar.
		ui.row {
			h = 26,
			px = 12,
			gap = 14,
			items = "center",
			shrink = 0,
			ui.text {
				function()
					local s = status:get()
					if s ~= "" then
						return s
					end
					return if dirty() then "Edited" else "Saved"
				end,
				grow = 1,
				shrink = 1,
				min_w = 0,
				text_size = 11,
				color = "$text.tertiary",
				truncate = true,
			},
			ui.text {
				function()
					return `{count_lines(text:get())} lines`
				end,
				tabular = true,
				text_size = 11,
				color = "$text.tertiary",
			},
			ui.text {
				function()
					local f = file:get()
					return language(if f then f.name else "")
				end,
				text_size = 11,
				color = "$text.tertiary",
			},
		},
	}

	return ui.column {
		grow = 1,
		min_h = 0,
		items = "stretch",
		on_key = on_key,
		-- The bar: the file, then what to do with it.
		ui.row {
			h = 40,
			pl = 12,
			pr = 6,
			gap = 8,
			items = "center",
			shrink = 0,
			ui.svg { code = ICONS.file, w = 15, h = 15, shrink = 0, tint = ACCENT },
			ui.text {
				function()
					local f = file:get()
					return if f then f.name elseif editing:get() then "Untitled" else "Editor"
				end,
				text_size = 13,
				weight = 600,
				color = "$text.primary",
				truncate = true,
				shrink = 1,
				min_w = 0,
			},
			-- Changes not saved.
			ui.div {
				hidden = function()
					return not dirty()
				end,
				w = 7,
				h = 7,
				rounded = 4,
				shrink = 0,
				bg = "$text.secondary",
			},
			ui.text {
				function()
					local f = file:get()
					return if f then (f.path:match("^(.*)/[^/]*$") or ""):gsub("^/Users/[^/]+", "~") else ""
				end,
				grow = 1,
				shrink = 1,
				min_w = 0,
				text_size = 11.5,
				color = "$text.disabled",
				truncate = true,
			},
			button(ICONS.open, "Open", function()
				open()
			end),
			ui.show(
				function()
					return editing:get()
				end,
				button(ICONS.save, "Save", function()
					save()
				end, dirty)
			),
		},
		ui.div { h = 1, shrink = 0, bg = "#ffffff0d" },
		ui.show(function()
			return editing:get()
		end, editor, start),
	}
end)

ui.contribute("panel:editor", Editor {})
