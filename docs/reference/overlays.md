# Overlays and menus: modal, popover, menu, menu_item, submenu, context_menu

Panels over the interface. Each is bound to a boolean state: setting it opens the panel, and closing it (Escape, a click outside, an item chosen) sets it back to `false`. A panel draws over everything else, escaping its parent's clipping and layout, while what it holds stays part of the script's tree: its states, refs and handlers work as anywhere else.

```luau
ui.modal(props: ModalProps) -> Node
ui.popover(props: PopoverProps) -> Node
ui.menu(props: PopoverProps) -> Node
ui.menu_item(props: MenuItemProps) -> Node
ui.submenu(props: SubmenuProps) -> Node
-- On any node:
context_menu: ((e: PointerEvent) -> ...{ Node }?)?
context_menu_style: Styles?
context_menu_below: boolean?
```

| Function | |
| --- | --- |
| `modal` | a dialog centered in the window over a dimmed backdrop |
| `popover` | a dialog anchored below `anchor` (flipping above when there is no room, clamped to the window); `anchor` stays where the popover is written |
| `menu` | a popover with a menu's roles and keyboard: its children are `menu_item`s, `submenu`s and separators |
| `menu_item` | an action in a menu: its label, and `on_click`; with `label` given, children draw it instead (an icon, the label, a shortcut, a check); `keep_open = true` leaves the menu open when it's chosen (a toggle) |
| `submenu` | a menu item that opens a menu beside it |
| `context_menu` | on any node: a secondary click (control-click on macOS) shows the items the function returns as a menu at the pointer |

## Options

| Option | Type | Default | |
| --- | --- | --- | --- |
| `open` | `State<boolean>` | required (not `menu_item`) | true while shown; written back as it closes |
| `anchor` (`popover`, `menu`) | `Node` | required | the trigger, laid out where the popover is |
| `autofocus` (`modal`, `popover`, `menu`) | `boolean` | `true` | focus its first control as it opens; `false` keeps the panel itself focused (keys still reach it) |
| `open_over` (`menu`) | `() -> string?` | none | the `id` of its item to open over the trigger, highlighted, as a pop-up button's (a select's) menu opens with the chosen item over the button |
| `arrow` | `number` | none | A popover only: an arrow at its trigger, reaching this many points from the panel's edge, as AppKit's popovers have. It points at the trigger's centre from the side the popover opened on (flipping with it), and is drawn as the panel is: its background (on macOS, the material) and border. |
| `trigger` (`submenu`) | `Node` | the label and `›` | the submenu's trigger row, drawn by the app: a `ui.menu_item` with content of its own (an icon, the label, a value, a chevron). A submenu opens when the pointer moves onto its trigger, and closes when it moves onto another item of the menu |
| `placement` (`modal`) | `{ x, y, inset }` | centred | where the panel sits in the window: lined up with its `"start"`, `"center"` or `"end"` across (`x`) and down (`y`), `inset` points from the edges (a number, or `{ top, right, bottom, left }`): a side sheet is `{ x = "end", y = "start" }` with `h` the window's height |
| `placement` (`popover`, `menu`) | `{ side, align, offset }` | `{ side = "bottom", align = "start", offset = 4 }` | where it opens: on `side` of the trigger (`"bottom"`, `"top"`, `"left"`, `"right"`, flipping to the other side when that one lacks room), lined up with the trigger's `"start"`, `"center"` or `"end"`, `offset` points away; it stays inside the window |
| `label` | `string` | `""` | the accessible name; a `menu_item`'s and a `submenu`'s visible label (a `menu_item`'s may be its first entry instead: `ui.menu_item { "Copy" }`) |
| children (not `menu_item`) | `Child` | none | the panel's content |
| properties | [Props](properties.md) | | style the panel (not the anchor): `w`, `p`, `gap`, `bg`, `rounded`, `backdrop_blur`, `border`, `shadow`… |
| `on_click` (`menu_item`) | `Handler \| Action` | none | runs once the menu has closed |
| `checked`, `radio` (`menu_item`) | `boolean \| Readable<boolean> \| (() -> boolean)`, `boolean` | none | a check in the item's gutter while true (a toggle item), or a dot with `radio = true` (one of a group); assistive technology hears a checkbox or radio item |
| `shortcut` (`menu_item`) | `string` | none | the keys that do it, shown at its end in the tertiary text colour (`"⌘C"`) |
| `match_anchor_width` (`popover`, `menu`) | `boolean` | `false` | at least as wide as its trigger, as a select's list is |
| `dismiss_on_backdrop` (`modal`, `popover`) | `boolean` | `true` | `false`: a click outside doesn't close it (a dialog that must be answered); Escape still does |
| `disabled` (`menu_item`, `submenu`) | `boolean \| Readable<boolean> \| (() -> boolean)` | `false` | keyboard navigation skips it |
| `context_menu` | `(e: PointerEvent) -> { Node }?` | none | `e` is `{ x, y, local_x, local_y }`; return the items, or nothing for no menu |
| `context_menu_style` | `Styles` | the menu's own | the context menu panel's look, plain values |
| `context_menu_below` | `boolean` | `false` | open under the node, from its left edge (a tab's), wherever the click was |

Panels start with 16 points of padding, the theme's surface color and rounded corners; a modal is 360 wide, a popover or menu 240, and each is as tall as its content, at most the window.

## Behavior

- Escape closes the top panel; a click outside one closes it (and does nothing else); a click on blank panel space does not.
- A modal or popover traps keyboard focus while open, starting on its first enabled control, and gives focus back when it closes.
- A menu takes focus to its first enabled item. Up and Down move (wrapping), Home and End jump, typing a letter finds an item by its label, Tab closes the menu and moves on.
- Choosing an item closes its menu, and the menus it opened from, before its `on_click` runs, so the handler can open another dialog.
- A submenu opens when the pointer moves onto its item, or with a click, Right or Enter (Left right to left, the way it opens); Left (Right) or Escape closes it alone. A long menu scrolls.
- The innermost node with a `context_menu` takes the secondary click; Shift+F10 or the Menu key opens it under what has focus. Terminals hand the right clicks their program does not use to the node, so a terminal's context menu opens over it.

## Example

```lua
local settings = ui.state("settings", false)
local more = ui.state("more", false)
local share = ui.state("share", false)
local last = ui.state("last", "nothing yet")

local function item(name: string)
	return ui.menu_item {
		name,
		on_click = function()
			last:set(name)
		end,
	}
end

return ui.column {
	p = 16,
	gap = 12,
	ui.row {
		gap = 8,
		ui.button { on_click = ui.set(settings, true), "Settings" },
		ui.menu {
			open = more,
			label = "More",
			anchor = ui.button { on_click = ui.toggle(more), "More" },
			item("Duplicate"),
			item("Rename"),
			ui.div { h = 1, my = 4, bg = "#ffffff1a" },
			ui.submenu { open = share, label = "Share", item("Copy link"), item("Email") },
		},
	},
	ui.text { "Chose: {last}" },
	ui.div {
		w = 240,
		h = 80,
		rounded = 8,
		bg = "#232e40",
		items = "center",
		justify = "center",
		context_menu_style = { bg = "#1d1e22", rounded = 10, p = 4 },
		context_menu = function()
			return { item("Cut"), item("Copy"), item("Paste") }
		end,
		ui.text { "Right click me" },
	},
	ui.modal {
		open = settings,
		label = "Settings",
		gap = 12,
		ui.text { "Settings", text_size = 18, bold = true },
		ui.button { on_click = ui.set(settings, false), "Done" },
	},
}
```

## Gotchas

- `open` must be a state; the panel writes `false` back to it when it closes, so a toggle that reads it stays right.
- The anchor is a node, given as `anchor = ui.button { … }`, not a child: the children are the panel's content. Opening on the anchor's click is yours to do (`on_click = ui.toggle(open)`).
- A panel (and a tooltip or hover card) is built the first time it opens, so a list's hundreds of row menus and cards cost nothing until one opens; closed again, it keeps its content mounted (and its state), drawn and focused only while open. Before it first opens, nothing inside it is found by `id` or a ref.
- A separator is any small node: a 1-point `ui.div`. A `menu_item` with children needs `label` for its accessible name and for typing to find it.
- `context_menu` runs at the click, in one update: build the items there (they may read state). It is called with the pointer, so the same function can offer different items in different places. Returning nothing lets the click through to what is under it.
- `context_menu_style` takes plain values, as `hover` does: a state or a function there is an error.
- Disabling an ancestor of an open panel closes it, and enabling it again does not reopen it.

## layer

`ui.layer { open?, placement?, … }` is a non-modal layer over the window, for toasts and other heads-up content: it takes no focus and has no backdrop, the pointer passes through it except over what it holds, and what it holds stays clickable while a dialog holds focus. It is raised over everything (dialogs included) each time it opens, so toasts shown while a dialog is open draw over the dialog's backdrop. `open` is a state (open when not given); `placement` sits it in the window as a modal's does (`{ x = "end", y = "end", inset = { 0, 24, 16, 0 } }` for a bottom-right stack).
