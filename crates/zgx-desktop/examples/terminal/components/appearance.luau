-- Appearance preferences apply live, without replacing panes or their sessions.
local theme = require("../theme")
local appearance = require("../appearance")
local actions = require("../actions")
local sessions = require("../sessions")
local kit = require("./kit")

local open = ui.state("appearance_open", false)
local M = {}

function M.close()
	open:set(false)
	local pane = actions.current()
	if pane then
		sessions.focus_view(pane)
	end
end

function M.open()
	open:set(true)
	local pane = actions.current()
	if pane and sessions.is_terminal(pane) then
		ui.term(pane):blur()
	end
end

local function choices(id: string, value: State<string>, options: { { string } }): Node
	local row: ContainerProps = { gap = 2, items = "center", rounded = "$radius.full", bg = theme.wash, p = 2 }
	for _, option in options do
		local key, title = option[1], option[2]
		table.insert(
			row,
			ui.button {
				id = `appearance-{id}-{key}`,
				label = title,
				role = "radio",
				checked = function()
					return value:get() == key
				end,
				variants = kit.button { tone = "ghost", size = "xs" },
				bg = function()
					return if value:get() == key then "$terminal.button.hover" else theme.clear
				end,
				on_click = function()
					value:set(key)
				end,
				ui.text { title, color = theme.text },
			}
		)
	end
	return ui.row(row)
end

local function toggle(id: string, label: string, value: State<boolean>): Node
	return ui.button {
		id = `appearance-{id}`,
		label = label,
		role = "switch",
		checked = value,
		variants = kit.switch { on = value },
		on_click = function()
			value:set(not value:peek())
		end,
		ui.div { variants = kit.knob },
	}
end

local function row(title: string, control: Node): Node
	return ui.row {
		min_h = 39,
		gap = 16,
		items = "center",
		ui.text { title, grow = 1, text_size = "$terminal.type.lg", color = theme.text },
		control,
	}
end

function M.Sheet(): Node
	return ui.presence(function()
		return if open:get() then true else nil
	end, function()
		return ui.modal {
			open = open,
			label = "Appearance",
			variants = kit.sheet,
			w = 440,
			max_w = "95%",
			max_h = "95%",
			ui.column {
				id = "appearance-sheet",
				items = "stretch",
				focusable = true,
				on_click = function() end,
				on_mount = function(el: Element)
					el:focus()
				end,
				on_key = function(e: KeyEvent): boolean
					if e.key == "Escape" then
						M.close()
						return true
					end
					return false
				end,
				ui.text {
					"Appearance",
					text_size = "$terminal.type.xxl",
					weight = "$font.weight.semibold",
					color = theme.text,
				},
				row("Interface", choices("mode", appearance.mode, { { "glass", "Glass" }, { "opaque", "Opaque" } })),
				row(
					"Appearance",
					choices(
						"scheme",
						appearance.scheme,
						{ { "auto", "Auto" }, { "light", "Light" }, { "dark", "Dark" } }
					)
				),
				row("Tint with theme", toggle("tint", "Tint with theme", appearance.tint)),
				row("Use glass for panes", toggle("pane-glass", "Use glass for panes", appearance.pane_glass)),
				ui.row {
					mt = "$space.2",
					justify = "end",
					ui.button { "Done", id = "appearance-done", variants = kit.button, on_click = M.close },
				},
			},
		}
	end, { absolute = true, left = 0, right = 0, top = 0, bottom = 0 })
end

return M
