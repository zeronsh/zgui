# Events

Handlers any node takes: pointer, keyboard, focus, lifecycle, and drag and drop within the window. A handler is a function, or for the input events an action Rust runs without calling the script (`ui.set(s, v)`, `ui.toggle(s)`, `ui.add(s, n)`, `ui.action(name, arg)`; see [State](state.md#actions)). Every handler runs in one reactive batch: what it writes updates the interface once, after it returns.

```luau
on_click: (Handler | Action)?
on_press: (((e: PointerEvent) -> ()) | Action)?
on_release: (((e: PointerEvent) -> ()) | Action)?
on_aux_click: (((e: AuxClickEvent) -> ()) | Action)?
on_hover: (((inside: boolean) -> ()) | Action)?
on_pointer_move: (((e: PointerEvent) -> ()) | Action)?
on_key: (((e: KeyEvent) -> boolean?) | Action)?
on_key_up: (((key: string) -> ()) | Action)?
on_focus: (Handler | Action)?
on_blur: (Handler | Action)?
on_wheel: (((e: WheelEvent) -> boolean?) | Action)?
on_pinch: (((e: PinchEvent) -> boolean?) | Action)?
on_mount: ((element: Element) -> ())?
on_resize: ((size: Size, element: Element) -> ())?
on_visible: ((visible: boolean, element: Element) -> ())?
on_drag: (() -> any)?
drag_preview: ((payload: any) -> Node)?
drag_preview_anchor: { number }?
drag_preview_offset: { number }?
on_drag_end: ((payload: any, accepted: boolean) -> ())?
drag_export: ((payload: any) -> DragExport?)?
on_drag_outside: ((payload: any, e: { x, y, grab_x, grab_y }) -> ())?
drag_moves_window: ((payload: any) -> boolean)?
drop_when: ((payload: any) -> unknown)?
on_drag_over: ((payload: any, e: DragPoint) -> ())?
on_drag_leave: ((payload: any) -> ())?
on_drop: ((payload: any, e: DragPoint) -> ())?
on_drag_spring: ((payload: any) -> boolean?)?
```

## Options

| Event | Called with | When |
| --- | --- | --- |
| `on_click` | `{ shift, ctrl, alt, meta, cmd }`: the keys held | the node is activated: a primary click on it (press and release), Enter as it goes down or Space as it comes up while it has focus (a link takes Enter alone; a checkbox, radio button or switch Space alone, as a browser's do), or an accessibility action. The innermost node with `on_click` takes it |
| `on_press` | `{ x, y, local_x, local_y, clicks, shift, ctrl, alt, meta, cmd }` | the primary button goes down on the node or anything in it; `clicks` counts presses in a row (2 for a double click's second, 3 for a triple's, and on past it) |
| `on_release` | the same | the primary button comes up |
| `on_aux_click` | `{ button, x, y, local_x, local_y, shift, ctrl, alt, meta, cmd }` (`button`: `"middle"`, or the platform's number for another extra button) | the middle button (or another beyond the primary and secondary) goes down and comes back up on the node or anything in it, the pointer not leaving it in between, as the DOM's `auxclick`: a tab closing on a middle click, a link opening in the background. The innermost node with `on_aux_click` takes it. The secondary button opens `context_menu` |
| `on_hover` | `true`, then `false` | once when the pointer comes inside the node or anything in it, once when it is outside all of them (the DOM's `mouseenter` and `mouseleave`); leaving the window, the window going to the back, and the pointer moving onto a page or a terminal count as leaving |
| `on_pointer_move` | `{ x, y, local_x, local_y }` | the pointer moves over it (or anywhere, while it holds the pointer) |
| `on_key` | `{ key, code, shift, ctrl, alt, meta, cmd, repeat, editing }` (`code`: the physical key, as the DOM's `KeyboardEvent.code`, `"KeyA"`, `"Digit1"`; `editing`: a text field holds focus, so a bare-key shortcut gives way) | a key goes down while it, or a node inside it, has focus. Return `true` to consume the key: the default action and the ancestors' handlers do not see it |
| `on_key_up` | the key's name | the key comes up |
| `on_focus`, `on_blur` | nothing | it takes or loses keyboard focus |
| `on_wheel` | `{ dx, dy, x, y, local_x, local_y, shift, ctrl, alt, meta, cmd }` | a wheel or trackpad scroll over it. Return `true` to take it from the scroll views around it: a canvas pans with it, or zooms about the pointer with ⌘ or Ctrl held |
| `on_pinch` | `{ scale, x, y, local_x, local_y, shift, ctrl, alt, meta, cmd }` (`scale`: the change in the fingers' spread, 0.05 for 5% apart: zoom in; negative, out) | a trackpad pinch over it (macOS); `true` takes it |
| `on_mount` | its [element](#elements) | once, when it is mounted: start animations here |
| `on_resize` | `{ w, h }`, its element | layout gives it a new size, and at its first layout (as a ResizeObserver reports) |
| `on_visible` | whether any of it shows, its element | it comes into view or all of it leaves (inside every clip and scroll viewport above it), and at its first layout (as an IntersectionObserver reports): build what only matters on screen |
| `on_drag` | nothing; returns the payload | the pointer moves past the system's drag threshold from a press on it (4 points until the desktop gives its own, GTK's 8 on Linux): a drag starts, carrying what it returns (any plain value) |
| `drag_preview` | the payload; returns a node | what follows the pointer while it drags |
| `drag_preview_anchor`, `drag_preview_offset` | (values) `{ x, y }` | where the preview goes: its point `anchor` (fractions of its size; `{ 0.5, 0 }` its top middle) `offset` points from the pointer. `{ 0.5, 0 }` with `{ 0, 16 }` hangs it below the pointer, centred, so what the drag is over stays in view; without either, the grab point stays under the pointer |
| `on_drag_end` | the payload, whether it was dropped on a destination | the drag ends |
| `drag_export` | the payload; returns `{ text, url, files }` | the drag leaves the window: other apps take what it returns (macOS) |
| `on_drag_outside` | the payload, `{ x, y, grab_x, grab_y }` | released outside every window of the app (macOS): a tab torn off its window; its preview goes rather than sliding back |
| `drag_moves_window` | the payload; returns whether the drag moves its window | the drag starts: true moves the window with the pointer instead of showing a preview (macOS), as a window's only tab does |
| `drop_when` | the payload; returns whether to accept it | a drag comes over the node (without it, every payload is accepted) |
| `on_drag_over` | the payload, `{ x, y, local_x, local_y, w, h }` | an accepted drag moves over it |
| `on_drag_leave` | the payload | the drag leaves it |
| `on_drop` | the payload, `{ x, y, local_x, local_y, w, h }` | an accepted drag is released over it |
| `on_drag_spring` | the payload; returns `false` to decline | spring-loading: an accepted drag has rested on it for the system's delay (macOS's setting, 0.5 s by default; never where it is turned off), or Space was pressed while it is over it. Open what it stands for, as a tab shows or a folder opens; once per stay (moving within it does not start the delay again, leaving does) |

`cmd` is the platform's command key, as `cmd` in a command's keys: Command (`meta`) on macOS, Ctrl (`ctrl`) elsewhere; test it for a shortcut (`e.cmd and e.key == "z"`) rather than `meta or ctrl`, which takes Super+Z on Linux and Ctrl+Z on macOS. `x` and `y` are in points from the window's top left, `local_x` and `local_y` from the node's, `w` and `h` its size. Key names are the character typed (`"a"`, `"A"`, `"/"`), `" "` for Space, `"F1"` to `"F35"`, or one of `"Tab"`, `"Enter"`, `"Escape"`, `"Backspace"`, `"Delete"`, `"ArrowLeft"`, `"ArrowRight"`, `"ArrowUp"`, `"ArrowDown"`, `"Home"`, `"End"`, `"PageUp"`, `"PageDown"`, `"Insert"`.

## Example

```lua
local width = ui.state("width", 160)
local log = ui.state("log", "")
type Card = { id: string, title: string }
local todo = ui.state("todo", { { id = "a", title = "Draft" }, { id = "b", title = "Review" } } :: { Card })
local done = ui.state("done", {} :: { Card })

local function card(item: Card)
	return ui.div {
		p = 8,
		rounded = 6,
		bg = "#232e40",
		cursor = "grab",
		on_drag = function()
			return item
		end,
		drag_preview = function(payload: Card)
			return ui.text { payload.title, p = 6, rounded = 6, bg = "#2f4058" }
		end,
		ui.text { item.title },
	}
end

local start: { x: number, w: number }? = nil
return ui.row {
	w_full = true,
	h_full = true,
	items = "stretch",
	focusable = true,
	on_key = function(e)
		if e.key == "Escape" then
			log:set("escape")
			return true
		end
		return false
	end,
	ui.column {
		w = width,
		p = 8,
		gap = 6,
		ui.each(todo, card, {
			key = function(item: Card)
				return item.id
			end,
		}),
	},
	-- A divider: dragging it resizes the column. With on_press and
	-- on_pointer_move it keeps the pointer until release.
	ui.div {
		w = 6,
		cursor = "col_resize",
		bg = "#ffffff12",
		on_press = function(e)
			start = { x = e.x, w = width:peek() }
		end,
		on_pointer_move = function(e)
			local s = start
			if s then
				width:set(math.clamp(s.w + e.x - s.x, 80, 300))
			end
		end,
		on_release = function()
			start = nil
		end,
	},
	ui.column {
		grow = 1,
		p = 8,
		gap = 6,
		drop_when = function(payload: Card)
			return payload.id ~= nil
		end,
		on_drop = function(payload: Card)
			for i, item in todo:peek() do
				if item.id == payload.id then
					todo:remove(i)
				end
			end
			done:push(payload)
		end,
		on_resize = function(size)
			log:set(`done column: {size.w} wide`)
		end,
		ui.text { "{log}", color = "#8a9ab0" },
		ui.each(done, card, {
			key = function(item: Card)
				return item.id
			end,
		}),
	},
}
```

## Elements

An element is a mounted node as a script sees it: `on_mount` and `on_resize` receive one, a [ref](state.md#refs) holds the ones mounted with it, and [animation](anime.md) function values get their target as one. A node is reachable this way (and by selectors) when it has an `id`, a `class`, a `ref`, an `on_mount`, an `on_resize` or an `on_visible`.

```luau
declare extern type Element with
	id: string?
	tag: string
	mounted: boolean
	width: number?
	height: number?
	x: number?
	y: number?
	function focus(self): boolean
	function blur(self): ()
	function has_class(self, class: string): boolean
	function select_all(self): boolean
	function set_selection(self, a: number, b: number?): boolean
	function selection(self): (number?, number?)
	function snapshot(self, max_width: number?): string?
end
```

| Member | |
| --- | --- |
| `id`, `tag` | its `id`, and its kind as selectors name it (`"column"`, `"button"`, `"text"`, `"input"`…) |
| `mounted` | whether it is in the interface now |
| `width`, `height`, `x`, `y` | its laid-out box, in points from the window's top left; nil while it is not in a scene. Read as they are, not followed (a ref's `width` and `height` are) |
| `focus()` | keyboard focus to it or its first shown focusable descendant; whether it took |
| `blur()` | clears keyboard focus when it is on the element or inside it |
| `has_class(class)` | whether its `class` names `class` |
| `select_all()`, `set_selection(a, b)` | an input's selection, in characters (Unicode code points from 0, as `utf8.len` counts them); `b` defaults to `a`, a caret. False for other elements |
| `selection()` | an input's anchor and focus (where the caret is); nil for other elements |
| `snapshot(max_width?)` | a picture of it as the window last showed it, at most `max_width` pixels wide (480 if not given): the `src` of an image showing it (`ui.image { src = el:snapshot(240) }`, a pane's thumbnail), replaced on each call and forgotten as its node goes. Nil if it is not laid out or the host takes none (the desktop host on macOS takes them); native views in it (a page, a terminal) show as what is under them, so ask a terminal itself (`term:snapshot`) |

A ref (`ui.ref()`, given to nodes as `ref = r`) answers `r:get()` and `r.element` (its first mounted element, else nil), `r:peek()` (the same, without following), `r.mounted`, `r.count`, `r:elements()` (every mounted node sharing it, in document order), and `r.width` and `r.height` (its element's laid-out size, nil while none is mounted). Reading `get`, `element`, `mounted`, `count`, `elements`, `width` or `height` in an effect or a computed follows the ref (and `width` and `height` follow layout resizing the element), so an effect that needs the element runs again once it mounts. A ref has no `focus` or other element methods: ask its element (`local el = r:get(); if el then el:focus() end`).

### Container queries

A layout that changes with the room it has (CSS's container and media queries) reads its container's width through a ref, `r.width`. The change lands in the frame the size changes: layout, the computed and layout again all run before it shows, as a browser settles its ResizeObserver callbacks before painting.

```lua
local root = ui.ref()
local wide = ui.computed(function()
	return (root.width or 900) >= 900
end)
return ui.row {
	ref = root,
	w_full = true,
	h_full = true,
	ui.show(function()
		return wide:get()
	end, ui.column { w = 220, shrink = 0, "Sidebar" }),
	ui.column { grow = 1, "Content" },
}
```

Query a box whose size does not depend on what the query changes (here the window's row): a box sized by its content that hid content below a width could go back and forth (zgx stops after a few rounds).

## Gotchas

- `on_click` gets the keys held; `on_press` and `on_release` get the pointer. `on_press` is for what must start before release (dragging a window, a divider); `on_click` is a completed click.
- The innermost node with `on_click` takes a click: a button inside a clickable row runs its own handler, not the row's. An empty `on_click = function() end` keeps clicks on a panel from reaching a backdrop under it.
- `on_press`, `on_release`, `on_pointer_move`, `on_key` and `on_wheel` hear their events over (or, for keys, from inside) the node, children included, before its ancestors do. A node with `on_press` and either `on_pointer_move` or `on_release` keeps the pointer from press to release, wherever it goes.
- Keys go to the node with focus and then up its ancestors; with nothing focused (and no modal open) they go to the app's root node, as a page's go to its body, so an `on_key` there hears the app's shortcuts. Check `editing` before taking a bare key, so a text field keeps its typing. A plain node takes focus with `focusable = true`; buttons and inputs take it themselves. Return `true` from `on_key` only for the keys you handle, or typing into an input inside stops working.
- `on_hover` reports entering and leaving the node as a whole: moving between its children tells it nothing.
- `on_resize` reports size changes only, not moves, and `on_visible` changes only; both are called outside any tracking: reading state in them subscribes nothing.
- Drag payloads are the script's own values (`any` to the types): annotate the parameters of `drag_preview`, `drop_when` and `on_drop` with your payload type. The nearest destination that accepts a payload gets the drag; `drop_when` returning nothing (or `false`) refuses it.
- `on_mount` runs once, when the node mounts; a node that is rebuilt (a row with a new key, a `show` toggled back) mounts again.
