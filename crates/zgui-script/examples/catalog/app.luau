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
		ui.column { gap = 16, px = 16, pb = 24, controls, overlays, containers },
	},
}
