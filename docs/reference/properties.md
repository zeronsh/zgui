# Properties

What every element takes besides its children and its own options. A property is a value, a state holding one, a function returning one (run again when the state it reads changes), or a [binding](anime.md#bindings): `Reactive<T>`. Literal values apply once; anything that can change shares one reactive style per node.

```luau
-- Each style property, as elements take it:
w: Reactive<Length>?
bg: Reactive<Color>?
-- …and as `hover`, `active`, `focus`, `disabled` and `context_menu_style` take it:
hover: Styles?
```

`focus` is CSS's `:focus-visible`: it applies when focus came from the keyboard (or a script, before any click), on a text field however it got focus, and once a key goes to the focused node; a clicked button takes focus without it.

The types: `Color` is `"#rgb"`, `"#rrggbb"`, `"#rrggbbaa"`, `"rgb(…)"`, `"rgba(…)"`, `"hsl(…)"`, a few names, or a number `0xrrggbb`. `Length` is points or a string: `"50%"` (of the containing block), `"12px"`, and `"auto"` for margins. `Pair` is `{ a, b }` or one number for both. Unless a row says otherwise a property has no default: the node has none of it.

## Size and position

| Property | Type | Default | |
| --- | --- | --- | --- |
| `w`, `h` | `Length` | its content's size | border-box: the border and padding are inside |
| `size` | `Pair` | | `w` and `h` at once |
| `min_w`, `max_w`, `min_h`, `max_h` | `Length` | | |
| `w_full`, `h_full` | `boolean` | `false` | `w = "100%"`, `h = "100%"` |
| `aspect` | `number` | | width over height; supplies the missing side |
| `absolute` | `boolean` | `false` | out of the flow, placed by insets (see [Position](#position)) |
| `relative` | `boolean` | `false` | a containing block for absolute descendants |
| `top`, `left`, `right`, `bottom` | `Length` | | insets of an absolute node |
| `inset_start`, `inset_end` | `Length` | | its inset from the start or end of a line (CSS's `inset-inline-start`/`-end`): `left` and `right` left to right, `right` and `left` with `dir = "rtl"` |
| `dir` | `"ltr" \| "rtl"` | its parent's (`"ltr"` at the root) | which way lines run in it and what it holds (HTML's `dir`): `"rtl"` runs rows from the right, starts a column's children and text at the right, and turns the `ps`/`pe`, `ms`/`me`, `inset_start`/`inset_end` edges round; `pl`, `ml` and `left` stay on the left |

## Spacing

| Property | Type | Default | |
| --- | --- | --- | --- |
| `p`, `px`, `py`, `pt`, `pr`, `pb`, `pl` | `Length` | `0` (a button: `px = 12, py = 8`) | padding; later keys refine earlier ones, a percentage counts from the containing block's width |
| `ps`, `pe` | `Length` | | padding at the start and end of a line (CSS's `padding-inline-start`/`-end`), which follow `dir` |
| `m`, `mx`, `my`, `mt`, `mr`, `mb`, `ml` | `Length` | `0` | margins; `"auto"` takes the remaining space |
| `ms`, `me` | `Length` | | margins at the start and end of a line (`margin-inline-start`/`-end`), which follow `dir` |
| `gap` | `number` | `0` (a button: `6`) | between children |
| `gap_x`, `gap_y` | `Length` | | per axis (a grid's or a wrapping row's) |

## Flex and grid

| Property | Type | Default | |
| --- | --- | --- | --- |
| `direction` | `"row" \| "row_reverse" \| "column" \| "column_reverse"` | `column` (`row` in a row) | |
| `grow` | `number` | `0` | share of the free space along the parent's axis |
| `shrink` | `number` | `1` | share of an overflow given up; `0` means it never shrinks. It shrinks no further than its content needs unless `min_w`/`min_h` say otherwise |
| `basis` | `Length` | its size | |
| `flex_wrap`, `wrap_reverse` | `boolean` | `false` | wrap children onto lines |
| `items` | `"start" \| "center" \| "end" \| "baseline" \| "stretch"` | `"stretch"` | children across the axis: stretched to the container's width (a column's) or height (a row's) unless they size themselves there, as in CSS |
| `justify` | `"start" \| "center" \| "end" \| "between" \| "around" \| "evenly" \| "stretch"` | `"start"` | children along it |
| `self` | `"start" \| "center" \| "end" \| "baseline" \| "stretch"` | the parent's `items` | this child across the parent's axis |
| `content` | `"start" \| "center" \| "end" \| "stretch" \| "between" \| "around" \| "evenly"` | `"start"` | wrapped lines |
| `grid` | `boolean` | `false` | lay children out in a grid of equal tracks |
| `cols`, `rows` | `number` | | the grid's equal tracks |
| `col_start`, `row_start` | `number` | the next free cell | one-based grid lines; negative count from the end |
| `col_span`, `row_span` | `number` | `1` | |
| `col_span_full`, `row_span_full` | `boolean` | `false` | span every track |
| `overflow`, `overflow_x`, `overflow_y` | `"visible" \| "hidden" \| "clip"` | `"visible"` (scroll views: `"hidden"`) | `"hidden"` and `"clip"` clip paint and hit testing |
| `hidden` | `boolean` | `false` | out of layout and input, keeping its state |
| `invisible` | `boolean` | `false` | not drawn, keeping its space |

## Paint

| Property | Type | Default | |
| --- | --- | --- | --- |
| `bg` | `Color` | none (button, input, slider, progress: `#232e40`) | |
| `gradient` | `Gradient` | | `{ "#f80", "#08f" }` or `{ angle = 135, stops = { … } }`; stops are colors (evenly spaced) or `{ offset, color }`; CSS angles: 0 runs bottom to top, 90 left to right, 180 (the default) top to bottom |
| `color` | `Color` | `#e5edf7` | text, and a progress bar's fill and a slider's thumb; inherited |
| `rounded` | `number` | `0` (controls: `6`) | corner radius |
| `corners` | `Corners` | | `{ tl, tr, br, bl }`, as a list or keyed |
| `rounded_tl`, `rounded_tr`, `rounded_br`, `rounded_bl` | `number` | | one corner, refining `rounded` or `corners` |
| `border` | `number` | `0` | width, inside the box |
| `border_sides` | `Sides` | | `{ top, right, bottom, left }` widths: a dock open at the bottom is `{ 1, 1, 0, 1 }` |
| `border_color` | `Color` | | |
| `border_over` | `boolean` | `false` | paint the border over the children, taking no room (a ring) |
| `border_dash` | `Pair` | solid | `{ length, gap }` |
| `shadow` | `Shadow` | none | `{ x, y, blur, spread, color }` (CSS `box-shadow`; `color` `#0000005a` by default), a list of them, or `false` |
| `outline`, `outline_color`, `outline_offset` | `number`, `Color`, `number` | none, black, `0` | an outline around the box (CSS's): its width, colour, and where it starts outside the box; a negative offset puts it over the edge (an inset focus ring is `outline = 1, outline_offset = -1`) |
| `inset_shadow` | `Shadow` | none | shadows inside the box (CSS's `inset`), over its fill and under its border and children: the box less itself moved by `x`, `y` and shrunk by `spread`, cut to its corners; drawn sharp (`blur` is not applied). A 1-point top highlight is `{ x = 0, y = 1, color = "#ffffff1f" }` |
| `opacity` | `number` | `1` | 0 to 1 |
| `brightness`, `contrast`, `saturate` | `number` | `1` | CSS filter functions over the node and its subtree |
| `blur` | `number` | `0` | blurs the node's content (CSS `filter: blur()`) |
| `backdrop_blur` | `number` | `0` | blurs what is behind it: frosted glass |
| `fade` | `Pair` | | a clipping view's top and bottom edges fade over those widths |
| `fade_x` | `Pair` | | its left and right edges |
| `fade_sides` | `Sides` | | each edge: `{ top, right, bottom, left }` |
| `layer` | `boolean` | `false` | paint the subtree into its own layer |
| `fit` | `"fill" \| "contain" \| "cover" \| "none" \| "scale_down"` | `"fill"` | how an image fills its box |
| `tint` | `Color` | | recolors an SVG (a state or function re-tints it) |
| `dither` | `number` | | draws an image dithered in dots this many points square (see [Images and canvas](media.md)) |
| `cursor` | `Cursor` | `"default"` | `"pointer"`, `"text"`, `"vertical_text"`, `"crosshair"`, `"move"`, `"grab"`, `"grabbing"`, `"not_allowed"`, `"wait"`, `"progress"`, `"help"`, `"context_menu"`, `"copy"`, `"alias"`, `"no_drop"`, `"all_scroll"`, `"col_resize"`, `"row_resize"`, `"ew_resize"`, `"ns_resize"`, `"nesw_resize"`, `"nwse_resize"`, `"zoom_in"`, `"zoom_out"`; CSS's spellings (`"nwse-resize"`) too. Unstated, a link shows `"pointer"` and a text field `"text"`; buttons, checkboxes, menu items and sliders keep the arrow, as Chrome's and AppKit's do |

## Background images, masks, shader fills, clip insets

| Property | Type | Default | |
| --- | --- | --- | --- |
| `bg_image` | `string` | | a PNG or JPEG path (relative to the script), state or function: drawn over `bg`, under the border and children, cut by the corners |
| `bg_fit` | `"cover" \| "contain" \| "fill" \| "none" \| "scale_down"` | `"cover"` | a literal only |
| `mask` | `Bitmap \| { bitmap, w, h, x, y, sweep }` | | the content shows through `ui.bitmap`'s alpha, drawn at `w`×`h` (the bitmap's size) from (`x`, `y`); animate `maskX`, `maskY`, or `sweep = { x, y, duration, frame_rate }` to have the engine move it by (`x`, `y`) every `duration` ms, over and over, with no script per step |
| `bg_shader` | `Shader` | | `ui.shader(wgsl, { fps })` fills the box per pixel (see [Images and canvas](media.md)) |
| `shader_params` | `{ Color \| { number } }` | zeros | up to four vec4s: colors (0 to 1) or lists of numbers; animate `shaderTime`, `shaderParam0` to `shaderParam15` |
| `blend` | `"multiply" \| "screen" \| "overlay" \| "darken" \| "lighten" \| "color-dodge" \| "color-burn" \| "hard-light" \| "soft-light" \| "difference" \| "exclusion" \| "hue" \| "saturation" \| "color" \| "luminosity"` | `"normal"` | CSS `mix-blend-mode`: the node and its children paint into a cached layer that mixes with what lies beneath it (the window, or the layer it sits in) by the mode's formula, as it composites |
| `layer_shader` | `LayerShader` | | `ui.layer_shader(wgsl, { fps })`: the subtree, cached as a layer, composites through it, pixel by pixel over the box (see [Images and canvas](media.md)) |
| `layer_params` | `{ Color \| { number } }` | zeros | its four vec4s, as `shader_params`; animate `layerTime`, `layerParam0` to `layerParam15`, which composite again without repainting |
| `clip_inset` | `Sides` | | paints only inside the box shrunk by `{ top, right, bottom, left }` (CSS `clip-path: inset()`); animate `clipTop`, `clipRight`, `clipBottom`, `clipLeft` |
| `clip_feather` | `number` | `0` | where it clips (`overflow = "hidden"`), its content fades out over this many points inside its rounded edge rather than being cut there, in the fragment shader quads already run (no layer, no mask). Its corners follow its clip inset, held to half the inset box: with `rounded = 9999` and the clip inset animated from the middle out, a soft circle opens, compositing alone |

## Size and position animation

| Property | Type | Default | |
| --- | --- | --- | --- |
| `layout` | `"height" \| "width" \| "size" \| "position" \| boolean` | none | `"height"` (or `true`), `"width"`, `"size"`: the space the node takes eases to its new size, while it lays out at that size at once; `"position"`: it slides when layout moves it |
| `layout_id` | `string` | | a node mounting while (or as) another with the same id is mounted starts at that one's box and moves to its own; ids are per script |
| `layout_ease` | `Easing` | a spring, 0.4 s, bounce 0.1 | |
| `layout_duration` | `number` | the ease's | ms |
| `layout_enter` | `boolean` | `false` | grow from nothing when it mounts |
| `layout_clip` | `boolean` | `false` | clip the content to the animated space |

[Dynamic content](dynamic.md) covers `each`'s `animate`, which slides rows the same way.

## Text

| Property | Type | Default | |
| --- | --- | --- | --- |
| `text_size` | `number` | `16` | inherited, as all of these |
| `weight` | `number` | `400` | `bold = true` is 700 |
| `bold`, `italic` | `boolean` | `false` | |
| `align` | `"start" \| "left" \| "center" \| "right"` | `"start"` | |
| `font` | `string` | the system's | `"mono"` (`"monospace"`), `"sans"` (`"sans-serif"`), `"serif"`, or a family name the host registered |
| `wrap` | `boolean` | `false` | wrap at the box's width |
| `truncate` | `boolean` | `false` | one line ending in "…" |
| `lines` | `number` | all | clamp to that many lines |
| `line_height` | `number` | the font's | points |
| `letter_spacing` | `number` | `0` | points |
| `tabular` | `boolean` | `false` | tabular figures: every digit one width, so a changing number holds still |
| `underline` | `boolean` | `false` | underlined text, inherited (CSS's `text-decoration`): `hover = { underline = true }` underlines a link button's label |
| `underline_offset` | `number` | a tenth of the text size | points below the baseline the underline sits; inherited |

## Transforms

`x`, `y` (`number`, points), `scale`, `scale_x`, `scale_y` (`number`, `1`), `rotate`, `skew`, `skew_x`, `skew_y`, `rotate_x`, `rotate_y` (`number`, degrees), `perspective` (`number`, points, `0`: none) and `origin` (`Pair`, fractions of the node, `{ 0.5, 0.5 }`). They apply after layout: the node keeps its place in the flow, and hit testing follows the transform. Rotation, scale, skew and tilt paint the subtree into a cached layer. `rotate_x` and `rotate_y` tilt it about the axes through `origin` (CSS `rotateX`, `rotateY`), seen from `perspective` points away (CSS `perspective()`): the nearer side shows larger, the layer composited as a projected quad; without a perspective a tilt only narrows it.

## Zoom

| Property | Type | Default | What it does |
| --- | --- | --- | --- |
| `zoom` | `number` | `1` | shows the subtree this many times as large, as CSS's `zoom`: it lays out again in its own points (its parent's room divided by the zoom) and takes the zoom times the room it needs there, so its siblings move; text and borders draw crisply at the size they show at. Zooms multiply with its ancestors' and the window's (`ui.window.zoom`); hit testing follows. Nonfinite or nonpositive zooms are `1` |
| `zoom_fixed` | `boolean` | `false` | keeps it at its own size whatever zooms are around it, the window's included (a toolbar that stays put while the page zooms) |
| `zoomable` | `State<number>` | | lets the user zoom it: its zoom is bound to this state, which Control with the wheel over it steps through the window's levels (Windows, Linux) and a trackpad pinch over it changes (every platform), from 0.25 to 5. The innermost zoomable under the pointer takes the gesture, at its limit too; elsewhere the window zooms. In a scroll view what is under the pointer stays there. Give the zoom to the state alone, not `zoom` beside it (`zgx lint`'s `zoomable-zoom`) |

Unlike `scale`, which draws the laid-out subtree larger and leaves its place in the flow alone, a zoom changes layout: give `zoom` a value or a function of state, and animate `scale` instead. An animated zoom lays the subtree out on every frame (`zgx lint`'s `animated-zoom`). The window's `ui.window.zoomable` (whether the user may zoom the window) and an element's `zoomable` (the user zooms it, through this state) are the same idea at two levels. More in [styling](../styling.md#zoom).

```lua
local big = ui.state("big", false)
return ui.column {
	zoom = function()
		return big:get() and 1.5 or 1
	end,
	ui.button { on_click = function() big:set(not big:peek()) end, "Zoom" },
	ui.text { "Zoomed with what it holds" },
	ui.row { zoom_fixed = true, ui.text { "Kept at its own size" } },
}
```

```lua
-- A document the user zooms where they point, the window zooming elsewhere.
local zoom = ui.state("doc_zoom", 1)
return ui.scroll {
	h_full = true,
	ui.column { zoomable = zoom, p = 16, ui.text { "Control and the wheel, or a pinch, zoom this" } },
}
```

## Pseudo-states, window dragging, identity

| Property | Type | Default | |
| --- | --- | --- | --- |
| `hover`, `active`, `focus` | `Styles` | (controls: a hover color and an accent focus ring) | styles while the pointer is over it, while it is pressed, while it has keyboard focus; `focus = { border = 0 }` turns a control's ring off |
| `disabled` | `boolean \| state \| function \| Styles` | `false` | a condition disables the node (and what is inside); a table is the styles it shows while disabled |
| `selectable` | `boolean` | `false` | the pointer selects the text in it and in everything it holds, across nodes, as on a web page (CSS `user-select: text`); `false` keeps a part of a selectable node out. See [selectable text](text.md#selectable-text) |
| `selection_color` | `Color \| Readable<Color> \| (() -> Color)` | `$selection` | a selectable node's highlight (CSS `::selection { background }`), in either window state |
| `selected_text_color` | `Color \| Readable<Color> \| (() -> Color)` | `$selection.text` | the selected glyphs' colour (CSS `::selection { color }`); transparent keeps their own |
| `window_drag` | `boolean` | `false` | a press on it, or on what inside takes no input, moves the window; a double click zooms it (CSS `-webkit-app-region: drag`) |
| `tooltip` | `string` or a node | none | shown over it while the pointer rests on it: after 300 ms, at once within a second of another tooltip; gone when the pointer leaves or presses, or a key is pressed anywhere (as a browser's and the system's do). Text takes the theme's look (an overlay card, small text); a node is the app's own content. It fades in over 140 ms and out over 100 ms, at once when it takes over from another; one shows at a time. It takes no focus and lets the pointer through |
| `provide` | `{ [string]: any }` | none | values the components and rows built in its subtree read with `ui.use_context(name)`, as React's context gives them ([components](state.md#components)) |
| `tooltip_placement` | `Placement` | above, centred, 6 points off | where the tooltip sits (as a popover's `placement`) |
| `tooltip_delay` | `number` | `300` | milliseconds the pointer rests before it shows. Keyboard focus shows it too (not a click's), and a text tip describes the node to assistive technology, which doesn't see the tip itself |
| `tooltip_interactive` | `boolean` | `false` | a hover card the pointer can move into and click in: it stays while the pointer is over the node or the card (a moment's grace to cross between them); typing goes to its fields, and Escape closes it |
| `id` | `string` | | for selectors (`"#id"`) and the inspector |
| `class` | `string` | | space-separated, for selectors (`".name"`) |
| `ref` | `Ref \| { Ref }` | | see [State](state.md#refs) |
| `focusable` | `boolean` | controls: `true` | takes keyboard focus |
| `pointer_events` | `"none"`, `"auto"`, `boolean`, or a state or function of one | `"auto"` | `"none"` or `false` lets the pointer pass through to what is under the node (presses, hover, the wheel, drops, and a web view or terminal, even where the node paints over it); descendants with their own handlers still receive input |

## Accessibility

What assistive technology (VoiceOver, Narrator, Orca) reads for a node beyond what it is: controls say their role, name and state themselves, and these say what a plain box means. They cost nothing on nodes that don't set them. A `label` given as a state or function costs nothing either while no screen reader is on: it is worked out only while assistive technology reads the window (from the state as it is, when one starts), not each time what it reads changes.

| Property | Type | Default | |
| --- | --- | --- | --- |
| `label` | `string \| state \| function` | its text | the name read for a box (ARIA's `aria-label`): an icon button, a group. A state or function is followed, for a recycled list row's name made from what it shows now (`"Unread, Grace Hopper, Lunch?, Oct 28"`). Widgets take their own `label` |
| `role` | `AriaRole`: `"group"`, `"button"`, `"link"`, `"heading"`, `"list"`, `"listitem"`, `"listbox"`, `"option"`, `"toolbar"`, `"status"`, `"tablist"`, `"tab"`, `"tabpanel"`, `"article"` (a message, a post), `"dialog"`, `"img"`, `"radiogroup"`, `"radio"`, `"switch"`, `"checkbox"`, `"spinbutton"`, `"combobox"`, `"tree"`, `"treeitem"`, `"grid"`, `"row"`, `"gridcell"`, `"columnheader"`, `"alert"`, `"menu"`, `"menuitem"` | what it is | what it is taken for (ARIA's `role`), keeping what it does: a clickable row as an `"option"` in a `"listbox"`, a row of buttons as a `"toolbar"`. Options, tabs, list items and headings are named by their text. A `"listitem"`'s marker ("•", "3.") is its first text with `aria_hidden = true`: the list owns it, so selections and plain text leave it out and copied HTML is a `<ul>` or `<ol>` (`zgx lint`'s `list-marker-hidden`) |
| `checked`, `expanded` | `boolean \| Readable<boolean> \| (() -> boolean)` | none | what a control of the app's own says of its state: a switch's or a radio's `checked` (`aria-checked`, a checked radio is where Tab lands in its group), what it opens `expanded` (`aria-expanded`) |
| `heading` | `number` | | a section title of that level, 1 to 6 (`h1`…`h6`): screen readers list headings and move between them (VoiceOver's rotor) |
| `selected` | `boolean \| state \| function` | | whether an option, tab or item is the chosen one (`aria-selected`), followed |
| `live` | `"polite" \| "assertive" \| "off"` | `"off"` (`role = "status"`: `"polite"`) | announce changes to the text in it (`aria-live`): a count, a "Sent" note, a message arriving; `"assertive"` interrupts |
| `description` | `string` | | read after the name, as a hint (`aria-description`) |
| `error` | `string \| state \| function` | | what is wrong with its value, followed; nil while fine: assistive technology hears it invalid (`aria-invalid`), and the message after its description. `field` gives a form's |
| `aria_hidden` | `boolean` | `false` | leaves it and what it holds out of the accessibility tree (`aria-hidden`): decoration such as an unread dot, or what is said elsewhere |
| `focus_group` | `"vertical" \| "horizontal" \| "both"` | `role = "listbox"`: `"vertical"`; `"toolbar"`, `"tablist"`: `"horizontal"` | one Tab stop for the focusable things inside, as ARIA's composite widgets are: Tab lands on the one last focused there (else the `selected` one, else the first), and the arrows along the axis, Home and End move among them. A field inside keeps its own arrows |

```lua
local mailbox = ui.state("mailbox", "Inbox")
return ui.column {
	role = "listbox",
	label = "Mailboxes",
	ui.row {
		role = "option",
		selected = function()
			return mailbox:get() == "Inbox"
		end,
		on_click = ui.set(mailbox, "Inbox"),
		ui.text { "●", aria_hidden = true },
		ui.text { "Inbox" },
	},
}
```

## Position

An `absolute = true` node leaves the flow and positions against its containing block: the nearest ancestor with `relative = true`, searched from its parent up but not past an `absolute` ancestor (which contains its own absolute descendants); with none, its parent. `left`, `top`, `right` and `bottom` count from that block's padding box, inside its border; percentages are of its size; both insets on an axis without a size stretch the node between them; an axis without insets keeps it at its parent's content origin (plus `ml`/`mt`). `show`, `switch`, `each` and `presence` wrap their content in a node of their own, so mark the ancestor you mean `relative` rather than relying on being its direct child. Transforms and scroll offsets never change the containing block: an absolute node moves with them, and still paints in its parent's place, under its clips.

## Example

```lua
local liked = ui.state("liked", false)
return ui.column {
	p = 24,
	ui.column {
		relative = true,
		w = 280,
		p = 16,
		gap = 8,
		rounded = 14,
		gradient = { angle = 135, stops = { "#232e40", "#1a2230" } },
		border = 1,
		border_color = "#ffffff1a",
		shadow = { y = 8, blur = 24, color = "#00000066" },
		ui.text { "Ada Lovelace", text_size = 18, weight = 600 },
		ui.text {
			"Wrote the first program for a machine that did not yet exist.",
			wrap = true,
			lines = 2,
			color = "#8b95a5",
			line_height = 20,
		},
		ui.button {
			self = "end",
			rounded = 999,
			bg = function()
				return if liked:get() then "#e5534b" else "#2f4058"
			end,
			hover = { brightness = 1.15 },
			active = { scale = 0.96 },
			on_click = ui.toggle(liked),
			"Like",
		},
		-- Pinned to the card's corner, whatever comes before it.
		ui.div { absolute = true, top = 10, right = 10, w = 8, h = 8, rounded = 4, bg = "#3fb950" },
	},
}
```

## Gotchas

- A state or function whose value does not fit the property (a string for `w` that is not a length, a boolean for `bg`) is ignored at run time; a literal that does not fit is an error. In the editor both are type errors.
- `hover`, `active`, `focus`, `disabled` and `context_menu_style` take plain values: a state, function or binding in them is an error. To animate on hover, animate the base style from `on_hover`.
- Children stretch across their container (`items = "stretch"`) and shrink in one too small for them (`shrink = 1`), as CSS's flexbox; text does not wrap (`wrap = false`) unless asked. A child that shrinks stops at its content's narrowest (CSS's `min-width: auto`: a text's longest word), unless it clips (`overflow`, a scroll) or sets `min_w = 0`; `shrink = 0` holds it.
- `gap` is points only; `gap_x` and `gap_y` take percentages.
- A transparent border still takes its width; a ring that must not move content is `border_over = true`.
- `bg_fit` and the shape of `mask`, `bg_shader`, `shader_params`, `layer_shader` and `layer_params` are read once, as literals.

### Native backdrop materials

`backdrop_material = "liquid"` requests a retained native material behind the
node; `"blur"` restores the existing `backdrop_blur` path. On macOS 26 and
later this uses `NSGlassEffectContainerView` and Regular `NSGlassEffectView`.
Older macOS uses a Popover `NSVisualEffectView`, WithinWindow blending and
Active state. Other platforms retain ordinary paint; the Liquid material is
currently macOS-only.

`backdrop_tint` sets the native glass tint independently from `bg`.
`backdrop_fill` paints above the material and below native children such as
terminals; `backdrop_fill_blend` supports `normal`, `overlay`, `color`,
`multiply` and `screen` on macOS (other blend modes currently use normal).
Colors, material and fill blend may follow state. Color tokens are supported.
Native material placements retain the node's ancestor clips, corner radii,
opacity and paint order. A pane material stays below its native child, while
its title and controls remain on the overlay. A standalone selection can use
native glass without overlapping a web view or terminal. Stable placements
reuse their views and do not request animation frames.

The terminal example gates these properties with its host's `native_liquid`
capability so older running hosts can reload its scripts without recognizing
new property names.

### Native surface recipes

`surface_recipe` supplies typed native decoration independently of Gaussian
blur. It accepts `glass`, `backdrop`, `fill`, `fill2`, `strokes`, `shadows`,
`innerShadow`, `shape`, and content geometry. Layers retain their original
color, blend mode, enabled flag, and geometry. Color specifications can retain
semantic `role`, fixed/adaptive source RGBA, color space and a separately
`resolvedColor`. Source alpha and geometry opacity are multiplied at paint
time; the source data is not flattened into an opaque color.

Omitted fields, explicit `ui.null` clears, and values remain distinct in the
typed transport. Stroke and shadow array order and IDs are retained. Profile
inheritance and source-specific defaults belong to the app's profile resolver;
the node property itself does not merge arbitrary recipes by ID.

On macOS the native compositor retains separate shadow, material, backdrop,
fill and stroke planes. `plusLighter` and `plusDarker` use the native layer
compositing operations. Strokes retain inside/center/outside alignment and
above/below-content levels. Disabled glass retains decoration without adding
Gaussian blur. External edges and shadows use conservative visual bounds and
ancestor clips; a layer's own `clipsToSurface` controls its surface clipping.
Below-content paint precedes descendant native views, above-content strokes
follow them, and scripted labels remain on the overlay. Stable placements
reuse the native views without adding idle frame work.

These native recipe planes are currently macOS-only. The terminal example
constructs the property only when the host publishes
`native_surface_recipe_schema = 1`; older hosts retain their existing styling
fallback. The capability differs from the earlier `native_liquid` flag.
