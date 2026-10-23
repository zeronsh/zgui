-- Popups past the window, in Luau: nothing here asks for it. On macOS a
-- menu, a submenu, a select, a popover, a context menu and a tooltip near
-- the window's edges reach past them, each in a window of its own with
-- AppKit's menu and popover material (the theme's defaults there), opening
-- and closing as AppKit's do; elsewhere they stay inside the window. The
-- chips at the bottom drag out to other apps: text into Mail or Notes, a
-- link into Safari.

local file = ui.state("file", false)
local recent = ui.state("recent", false)
local size = ui.state("size", false)
local chosen = ui.state("chosen", "Medium")
local details = ui.state("details", false)
local name = ui.state("name", "Untitled")
local said = ui.state("said", "Right-click (or control-click) here")

local function item(label: string, on_click: (() -> ())?): Node
	return ui.menu_item { label, h = 24, px = 10, rounded = 5, text_size = 13, on_click = on_click }
end

local function button(label: string, on_click: (() -> ())?): Node
	return ui.button { h = 28, px = 12, text_size = 13, on_click = on_click, ui.text(label) }
end

local menu_look: Styles = { p = 5, gap = 0, rounded = 12, items = "stretch" }

local function file_menu(): Node
	local recents: { Child } = {}
	for _, path in { "Quarterly report.numbers", "Launch plan.pages", "notes.md", "zgui" } do
		table.insert(recents, item(path))
	end
	return ui.menu {
		open = file,
		anchor = button("File"),
		w = 220,
		p = menu_look.p,
		gap = menu_look.gap,
		rounded = menu_look.rounded,
		items = "stretch",
		item("New Window"),
		item("Open…"),
		ui.submenu {
			open = recent,
			label = "Open Recent",
			w = 240,
			p = menu_look.p,
			rounded = menu_look.rounded,
			items = "stretch",
			recents,
		},
		item("Save"),
		item("Print…"),
	}
end

-- A pop-up button: its menu opens with the chosen size over the button.
local function select(): Node
	local sizes: { Child } = {}
	for _, label in { "Extra small", "Small", "Medium", "Large", "Extra large", "Huge", "Enormous" } do
		table.insert(
			sizes,
			ui.menu_item {
				label = label,
				id = label,
				h = 24,
				px = 6,
				rounded = 5,
				text_size = 13,
				on_click = function()
					chosen:set(label)
				end,
				ui.row {
					gap = 4,
					ui.text {
						function()
							return if chosen:get() == label then "✓" else ""
						end,
						w = 14,
					},
					ui.text(label),
				},
			}
		)
	end
	return ui.menu {
		open = size,
		anchor = ui.button {
			h = 28,
			w = 150,
			px = 10,
			text_size = 13,
			ui.text(function()
				return chosen:get()
			end),
		},
		open_over = function()
			return chosen:get()
		end,
		w = 150,
		p = menu_look.p,
		rounded = menu_look.rounded,
		items = "stretch",
		sizes,
	}
end

local function popover(): Node
	return ui.popover {
		open = details,
		anchor = button("Details…", function()
			details:set(true)
		end),
		placement = { side = "right", align = "center", offset = 8 },
		w = 240,
		p = 14,
		gap = 10,
		rounded = 14,
		ui.text { "Rename", text_size = 15 },
		ui.input { value = name, h = 28 },
		ui.text {
			"Keys reach this field while the window keeps its focus.",
			wrap = true,
			text_size = 12,
			opacity = 0.7,
		},
	}
end

-- A chip that drags out to other apps, carrying `export`.
local function chip(label: string, export: DragExport): Node
	return ui.column {
		h = 28,
		px = 12,
		justify = "center",
		rounded = 8,
		bg = "#3d7ef733",
		text_size = 12,
		on_drag = function()
			return label
		end,
		drag_preview = function(payload: string)
			return ui.column {
				h = 28,
				px = 12,
				justify = "center",
				rounded = 8,
				bg = "#3d7ef7e0",
				color = "#ffffff",
				text_size = 12,
				ui.text(payload),
			}
		end,
		drag_export = function()
			return export
		end,
		ui.text(label),
	}
end

return ui.column {
	w = "100%",
	h = "100%",
	p = 14,
	gap = 12,
	ui.row {
		w = "100%",
		gap = 8,
		items = "center",
		file_menu(),
		ui.column { grow = 1 },
		ui.button {
			h = 28,
			w = 36,
			tooltip = "Tooltips show above the title bar",
			ui.text("ⓘ"),
		},
		popover(),
	},
	ui.column {
		grow = 1,
		w = "100%",
		rounded = 10,
		bg = "#80808018",
		items = "center",
		justify = "center",
		context_menu = function()
			local items: { Node } = {}
			for _, label in { "Copy", "Paste", "Duplicate", "Move to Trash" } do
				table.insert(
					items,
					item(label, function()
						said:set(`Chose “{label}”`)
					end)
				)
			end
			return items
		end,
		context_menu_style = menu_look,
		ui.text {
			function()
				return said:get()
			end,
			text_size = 13,
			opacity = 0.7,
		},
	},
	ui.row {
		w = "100%",
		items = "center",
		justify = "between",
		ui.row {
			gap = 8,
			chip("A line of text", { text = "Dragged out of a zgui window." }),
			chip("A link", { url = "https://github.com/zeronsh/zgui" }),
		},
		select(),
	},
}
