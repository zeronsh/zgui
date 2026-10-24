-- The component catalogue: every built-in control, overlay and container,
-- each in a section with what it does from the keyboard. Switch the theme
-- and the direction to see each follow.
local dark = ui.persist("dark", true)
local rtl = ui.state("rtl", false)

ui.effect(function()
	ui.theme.set(if dark:get() then "zgui-dark" else "zgui-light")
end)

local function section(title: string, hint: string, ...: Child): Node
	return ui.column {
		gap = 10,
		p = 16,
		rounded = "$radius.lg",
		bg = "$surface.base",
		border = 1,
		border_color = "$border.subtle",
		ui.text { title, bold = true },
		ui.text { hint, color = "$text.secondary", text_size = "$font.size.sm", wrap = true },
		ui.row { gap = 12, items = "center", wrap = true, ... },
	}
end

-- Controls ----------------------------------------------------------------

local range = ui.state("range", "week")
local copies = ui.state("copies", 2)
local volume = ui.state("volume", 0.5)
local progress = ui.state("progress", 0.35 :: number?)
local notify = ui.state("notify", true)

local controls = ui.column {
	gap = 12,
	section(
		"Segmented control",
		"One Tab stop; the arrows move the choice.",
		ui.segmented {
			id = "range",
			label = "Range",
			value = range,
			options = {
				{ label = "Day", value = "day" },
				{ label = "Week", value = "week" },
				{ label = "Month", value = "month" },
			},
		},
		ui.text {
			function()
				return `Showing a {range:get()}`
			end,
			color = "$text.secondary",
		}
	),
	section(
		"Stepper",
		"Up and Down step, Page Up and Down ten, Home and End to the ends; typing applies on Enter.",
		ui.stepper { id = "copies", label = "Copies", value = copies, min = 1, max = 99 },
		ui.text { "{copies} copies", color = "$text.secondary" }
	),
	section(
		"Slider and progress",
		"Arrows step by 5%; Shift or Page Up and Down by 50%. Indeterminate progress sweeps.",
		ui.slider { id = "volume", label = "Volume", value = volume, step = 0.05 },
		ui.progress { label = "Download", value = progress, w = 160 },
		ui.button {
			id = "busy",
			on_click = function()
				progress:set(if progress:peek() == nil then 0.35 else nil)
			end,
			function()
				return if progress:get() == nil then "Determinate" else "Indeterminate"
			end,
		}
	),
	section(
		"Checkbox and buttons",
		"Space toggles; disabled controls dim.",
		ui.checkbox { label = "Notify me", value = notify },
		ui.button { "Save", tooltip = "Save the document (⌘S)" },
		ui.button { "Disabled", disabled = true }
	),
}

-- Tabs and select ---------------------------------------------------------

local documents = ui.state(
	"documents",
	{
		{ id = "readme", label = "README.md", closable = true },
		{ id = "main", label = "main.rs", closable = true },
		{ id = "cargo", label = "Cargo.toml", closable = true },
		{ id = "notes", label = "Notes" },
	} :: { TabItem }
)
local document = ui.state("document", "readme")
local next_document = 1
local fruit = ui.state("fruit", "banana")

local choosing = section(
	"Tabs and select",
	"Tabs: the arrows choose; Delete or a middle click closes; drag, or ⌘⇧ and an arrow, to reorder; too many scroll, with a menu of them all. Select: typing chooses while closed.",
	ui.column {
		w = 520,
		h = 160,
		ui.tabs {
			id = "documents",
			label = "Documents",
			items = documents,
			value = document,
			reorderable = true,
			panel = function(id: string): Node
				return ui.column { p = 12, ui.text { `Editing {id}`, color = "$text.secondary" } }
			end,
		},
	},
	ui.button {
		id = "new-tab",
		on_click = function()
			local id = `untitled-{next_document}`
			documents:push { id = id, label = `Untitled {next_document}`, closable = true }
			document:set(id)
			next_document += 1
		end,
		"New tab",
	},
	ui.select {
		id = "fruit",
		label = "Fruit",
		value = fruit,
		placeholder = "Choose a fruit",
		options = {
			{ label = "Apple", value = "apple" },
			{ label = "Banana", value = "banana" },
			{ label = "Blueberry", value = "blueberry", disabled = true },
			{ label = "Cherry", value = "cherry" },
		},
	}
)

-- Combobox and command palette --------------------------------------------

local query = ui.state("query", "")
local city = ui.state("city", nil :: string?)
local searching = ui.state("searching", false)
local found = ui.state("found", {} :: { ComboItem })
local CITIES = {
	"Amsterdam",
	"Athens",
	"Barcelona",
	"Berlin",
	"Lisbon",
	"London",
	"Madrid",
	"Paris",
	"Porto",
	"Prague",
	"Rome",
	"Vienna",
}
-- As an async search would: the answer a moment after the query.
ui.effect(function()
	local q = string.lower(query:get())
	searching:set(true)
	ui.after(150, function()
		local out: { ComboItem } = {}
		for _, c in CITIES do
			if string.find(string.lower(c), q, 1, true) then
				table.insert(out, { label = c, value = string.lower(c), detail = "Europe" })
			end
		end
		found:set(out)
		searching:set(false)
	end)
end)
local palette = ui.state("palette", false)
ui.command {
	id = "palette",
	title = "Command Palette",
	menu = "View",
	key = "cmd+shift+p",
	run = function()
		palette:set(true)
	end,
}
ui.command {
	id = "toggle-rtl",
	title = "Toggle Right to Left",
	menu = "View",
	run = function()
		rtl:set(not rtl:peek())
	end,
}
ui.command {
	id = "toggle-dark",
	title = "Toggle Dark",
	menu = "View",
	run = function()
		dark:set(not dark:peek())
	end,
}

local searching_section = section(
	"Combobox and command palette",
	"The combobox searches as typed (Down and Up move, Enter chooses, focus stays in the field); ⇧⌘P opens the palette of the app's commands.",
	ui.combobox { id = "city", label = "City", text = query, items = found, value = city, loading = searching, w = 260 },
	ui.text {
		function()
			local c = city:get()
			return if c then `Chosen: {c}` else "Nothing chosen"
		end,
		color = "$text.secondary",
	},
	ui.button { id = "open-palette", "Command palette…", on_click = ui.set(palette, true) },
	ui.command_palette { open = palette }
)

-- Overlays ----------------------------------------------------------------

local menu_open = ui.state("menu_open", false)
local wrap = ui.state("wrap", true)
local size = ui.state("size", "medium")
local popover_open = ui.state("popover_open", false)
local dialog_open = ui.state("dialog_open", false)

local menu_button = ui.button { id = "view-menu", "View ▾" }
local popover_button = ui.button { id = "filters", "Filters" }

local function size_item(id: string, label: string): Node
	return ui.menu_item {
		id = `size-{id}`,
		label = label,
		radio = true,
		checked = function()
			return size:get() == id
		end,
		on_click = ui.set(size, id),
	}
end

local overlays = section(
	"Menus, popovers, dialogs, tooltips",
	"Menus: arrows, typeahead, Escape; checkable and radio items with shortcuts. Shift+F10 opens a context menu. Tooltips show on keyboard focus.",
	ui.menu {
		open = menu_open,
		anchor = menu_button,
		match_anchor_width = true,
		ui.menu_item {
			label = "Word Wrap",
			checked = wrap,
			shortcut = "⌥Z",
			keep_open = true,
			on_click = ui.toggle(wrap),
		},
		ui.separator {},
		size_item("small", "Small"),
		size_item("medium", "Medium"),
		size_item("large", "Large"),
	},
	ui.popover {
		open = popover_open,
		anchor = popover_button,
		arrow = 8,
		ui.text { "Filters go here" },
		ui.checkbox { label = "Unread only", value = ui.state("unread", false) },
	},
	ui.button { id = "open-dialog", "Dialog…", on_click = ui.set(dialog_open, true) },
	ui.button {
		id = "toast",
		on_click = function()
			ui.toast {
				title = "Saved",
				body = "Report.pdf",
				kind = "success",
				action = { label = "Undo", run = function() end },
			}
		end,
		"Toast",
	},
	ui.button {
		id = "toast-error",
		on_click = function()
			ui.toast { title = "Upload failed", body = "The server didn't answer.", kind = "error" }
		end,
		"Error toast",
	},
	ui.modal {
		open = dialog_open,
		label = "Discard changes?",
		dismiss_on_backdrop = false,
		ui.text { "Discard changes?", bold = true },
		ui.text { "A click outside doesn't close this one; choose.", color = "$text.secondary" },
		ui.row {
			gap = 8,
			ui.button { "Cancel", on_click = ui.set(dialog_open, false) },
			ui.button { "Discard", on_click = ui.set(dialog_open, false) },
		},
	},
	ui.div {
		id = "file",
		focusable = true,
		p = 8,
		rounded = "$radius.md",
		border = 1,
		border_color = "$border",
		tooltip = "Right-click, Shift+F10 or the Menu key",
		context_menu = function()
			return { ui.menu_item { "Rename", shortcut = "↩" }, ui.menu_item { "Duplicate", shortcut = "⌘D" } }
		end,
		ui.text { "Report.pdf" },
	}
)

-- Containers --------------------------------------------------------------

local details = ui.state("details", false)
local faq = ui.state("faq", "ship" :: string?)

local files = ui.state(
	"files",
	{
		{
			id = "src",
			label = "src",
			children = {
				{ id = "app", label = "app.luau" },
				{ id = "components", label = "components", children = { { id = "button", label = "button.luau" } } },
			},
		},
		{ id = "docs", label = "docs (loads when opened)", lazy = true },
		{ id = "readme", label = "README.md" },
	} :: { TreeNode }
)
local open_files = ui.state("open_files", { "src" } :: { string })
local current_file = ui.state("current_file", nil :: string?)

local tree_section = section(
	"Tree",
	"Up and Down move; Right opens then enters; Left closes then goes up; typing finds; Enter activates.",
	ui.column {
		w = 320,
		h = 180,
		ui.tree {
			id = "files",
			label = "Files",
			items = files,
			open = open_files,
			value = current_file,
			on_open = function(id: string)
				if id == "docs" then
					ui.after(300, function()
						files:set_key(
							{ 2, "children" },
							{ { id = "guide", label = "guide.md" }, { id = "api", label = "api.md" } }
						)
					end)
				end
			end,
			on_activate = function(id: string)
				ui.toast { title = `Opened {id}` }
			end,
		},
	}
)

local function make_orders()
	local out = {}
	for i = 1, 2000 do
		out[i] = {
			id = `#{1000 + i}`,
			customer = `Customer {i % 97}`,
			total = (i * 37) % 1000 / 10,
			status = if i % 3 == 0 then "Shipped" else "Pending",
		}
	end
	return out
end
local orders = ui.state("orders", make_orders())
local order_columns = ui.state(
	"order_columns",
	{
		{ id = "id", title = "Order", width = 90 },
		{ id = "customer", title = "Customer", width = 180, sortable = true, editable = true },
		{ id = "total", title = "Total", width = 90, sortable = true, editable = true, align = "end" },
		{ id = "status", title = "Status", width = 110, sortable = true },
	} :: { GridColumn }
)
local order_sort = ui.state("order_sort", nil :: GridSort?)
local selected_orders = ui.state("selected_orders", {} :: { number })

local grid_section = section(
	"Data grid",
	"Arrows move; Enter, F2 or typing edits, Escape drops; Shift extends the selection, ⌘ adds a row, ⌘A selects all, ⌘C copies. Up from the first row reaches the header, where Enter sorts, ⌘⇧ arrows move a column and Alt arrows resize it. Drag a header to move its column, its edge to resize it.",
	ui.column {
		w = 520,
		h = 260,
		ui.data_grid {
			id = "orders",
			label = "Orders",
			columns = order_columns,
			rows = orders,
			sort = order_sort,
			selection = selected_orders,
			cell = function(value: any, column: string): string
				return if column == "total" then string.format("$%.2f", value) else tostring(value)
			end,
			on_activate = function(row: number)
				ui.toast { title = `Opened order {orders:get()[row].id}` }
			end,
		},
	}
)

local due = ui.state("due", nil :: string?)
local start = ui.state("start", "09:30" :: string?)
local day = ui.state("day", nil :: string?)

local dates_section = section(
	"Dates and times",
	"A picker's button opens its calendar (Down too): the arrows move a day or a week, Page Up and Down a month (with Shift a year), Enter chooses. A time's segments step with Up and Down and take digits.",
	ui.date_picker {
		id = "due",
		label = "Due date",
		value = due,
		-- Weekends can't be chosen.
		unavailable = function(_date: string, weekday: number)
			return weekday == 0 or weekday == 6
		end,
	},
	ui.time_field { id = "start", label = "Start", value = start, minute_step = 15, hour12 = true },
	ui.calendar { id = "day", label = "Day", value = day, first_weekday = 1 }
)

local containers = section(
	"Disclosure and accordion",
	"Enter or Space toggles a section; an accordion keeps one open.",
	ui.column {
		w = 360,
		gap = 4,
		ui.disclosure { open = details, summary = "Advanced settings", ui.text { "Hidden until opened." } },
		ui.separator {},
		ui.accordion {
			open = faq,
			ui.section { id = "ship", title = "Shipping", ui.text { "Ships in two days." } },
			ui.section { id = "pay", title = "Payment", ui.text { "Cards and transfers." } },
			ui.section { id = "return", title = "Returns", ui.text { "Thirty days." } },
		},
	}
)

return ui.column {
	id = "catalog",
	w_full = true,
	h_full = true,
	bg = "$surface.sunken",
	dir = function()
		return if rtl:get() then "rtl" else "ltr"
	end,
	ui.row {
		gap = 12,
		p = 16,
		items = "center",
		ui.text { "Components", text_size = "$font.size.xl", bold = true },
		ui.div { grow = 1 },
		ui.checkbox { id = "rtl", label = "Right to left", value = rtl },
		ui.checkbox { id = "dark", label = "Dark", value = dark },
	},
	ui.scroll {
		grow = 1,
		ui.column {
			gap = 16,
			px = 16,
			pb = 24,
			controls,
			choosing,
			searching_section,
			tree_section,
			grid_section,
			dates_section,
			overlays,
			containers,
		},
	},
}
