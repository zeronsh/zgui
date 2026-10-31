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
| `min_w`, `max_w`, `min_h`, `max_h` | `Length` | | also [size math](#size-math) and [content sizes](#content-sizes) |
| `w_full`, `h_full` | `boolean` | `false` | `w = "100%"`, `h = "100%"` |
| `aspect` | `number` | | width over height; supplies the missing side |
| `absolute` | `boolean` | `false` | out of the flow, placed by insets (see [Position](#position)) |
| `relative` | `boolean` | `false` | a containing block for absolute descendants |
| `top`, `left`, `right`, `bottom` | `Length` | | insets of an absolute node |
| `inset_start`, `inset_end` | `Length` | | its inset from the start or end of a line (CSS's `inset-inline-start`/`-end`): `left` and `right` left to right, `right` and `left` with `dir = "rtl"` |
| `dir` | `"ltr" \| "rtl"` | its parent's (`"ltr"` at the root) | which way lines run in it and what it holds (HTML's `dir`): `"rtl"` runs rows from the right, starts a column's children and text at the right, and turns the `ps`/`pe`, `ms`/`me`, `inset_start`/`inset_end` edges round; `pl`, `ml` and `left` stay on the left A function or state follows: [text from people](#which-way-text-from-people-runs), which may run either way |

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
| `grid` | `boolean` | `false` | lay children out in a grid |
| `cols`, `rows` | `number \| string` | | a number of equal tracks, or a track list as CSS's `grid-template-columns` writes it (see [Grid tracks](#grid-tracks)) |
| `display` | `"contents"` | | its children lay out in its place, as its parent's own (CSS `display: contents`): grid cells, row items ([display contents](#display-contents)) |
| `col_start`, `row_start` | `number` | the next free cell | one-based grid lines; negative count from the end |
| `areas` | `{ string }` | | named cells, a string a row ([grid tracks](#grid-tracks)) |
| `flow` | `"row" \| "column" \| "dense" \| "column dense"` | `"row"` | how items without a place fill the grid |
| `area` | `string` | | the area an item fills, by name |
| `grid_column`, `grid_row` | `string \| number` | | an item's lines: `"side / main"`, `"2 / span 3"` |
| `col_span`, `row_span` | `number` | `1` | |
| `col_span_full`, `row_span_full` | `boolean` | `false` | span every track |
| `overflow`, `overflow_x`, `overflow_y` | `"visible" \| "hidden" \| "clip"` | `"visible"` (scroll views: `"hidden"`) | `"hidden"` and `"clip"` clip paint and hit testing |
| `hidden` | `boolean` | `false` | out of layout and input, keeping its state |
| `invisible` | `boolean` | `false` | not drawn, keeping its space |
| `z` | `number` | `0` | paint and hit it over siblings with a lower `z`, under those with a higher; equal stays in document order (see [Stacking](#stacking)) |
| `sticky_top` | `number` | none | inside a scroll view, hold this far below its top as it scrolls past, inside the parent ([sticky](#sticky)) |

### Hit at target

With `hit = "target"`, an element that an animation is moving takes clicks and hover where the animation is taking it, not where it shows. Clicking fast on something still moving lands on what will be there, as Core Animation hits its model layer while its presentation plays. What it holds goes with it.

```lua
local photos = ui.state("photos", { 1, 2, 3, 4 })
local function remove(photo)
	photos:update(function(list)
		local kept = {}
		for _, p in list do
			if p ~= photo then
				table.insert(kept, p)
			end
		end
		return kept
	end)
end
return ui.row {
	gap = 10,
	ui.each(photos, function(photo)
		return ui.div { w = 80, h = 60, bg = "$surface.raised", hit = "target", on_click = function()
			remove(photo)
		end }
	end, {
		key = function(photo)
			return photo
		end,
		animate = { duration = 400 },
		exit = { opacity = 0, duration = 200 },
	}),
}
```

Clicking one spot as fast as you like removes the photos in order, though none has finished sliding.

- Where it's going: `x`, `y`, `scale`, `rotate`, `skew` and `opacity` bound to `ui.tween` or `ui.spring` use the state they follow, and animated by `anime` its last value; a layout animation (`each`'s `animate`, `layout = "position"`) uses the place layout gave it; an absolute element's `left` and `top` in points use their goal. Going to opacity 0, it takes no clicks.
- **`w` and `h` animating aren't laid out at their goal:** its children take clicks where they show inside it. Animate `scale`, or let a layout animation size it, to hit at the end size.
- An `anime` animation of those properties takes it to its last value from the frame it starts. Cancelled or removed mid-way, it takes clicks where it stopped; paused, where it's going. An additive (`composition = "add"`) animation isn't followed.
- Painting, scroll copies and `zgx inspect`'s `box` stay where it shows; `zgx inspect` adds a `hit box` while they differ. `zgx test` clicks by id land where it takes clicks.
- Assistive technology reads it where it's going, so a screen reader's highlight jumps ahead of the motion.
- Elements without `hit = "target"` hit where they show, as before.

### Size math

`w`, `h`, `min_w`, `max_w`, `min_h` and `max_h` take `calc(…)`, `min(…)`, `max(…)` and `clamp(…)`, worked out from the containing block as CSS's are. Layout does the work as sizes change, so a size like these needs no `on_resize` writing state.

```lua
return ui.column {
	w_full = true,
	h = 200,
	ui.div { w = "calc(100% - 24px)", h = 40, bg = "$surface.raised" },
	ui.div { w = "min(320px, 100%)", h = 40, bg = "$accent" },
	ui.div { w = "clamp(120px, 50%, 480px)", h = 40, bg = "$accent" },
}
```

- Percentages are of the parent's content box, as `w = "50%"` is. Under a parent with no size of its own the whole expression lays out as `auto`, as in CSS: the box takes its content's size. In a grid they're of the grid, not of the cell as in CSS: for a cell's width use `w = "100%"`.
- A bare number is points, so `calc(100% - 24)` is `calc(100% - 24px)`. Of two factors one must be a plain number, and a divisor always, as CSS requires; anything else is an error when the script loads.
- A result below zero is zero.
- Only these six properties take them: written on `p`, `m` or an inset, one stops the script with an error. Other units (`em`, `vw`) aren't parsed.
- `state` and function values work, and a value that isn't valid is reported and leaves the last size. A variant's plain `w` doesn't replace a base `calc(…)`: use one in the variant too.
- A node with one makes its parent use the same layout as `wrap` or `gap_x`: its children are measured a little more slowly than a plain column's.

### Content sizes

`w`, `min_w` and `max_w` also take `"min-content"`, `"max-content"`, `"fit-content"` and `"fit-content(…)"`, sized by what the element holds, as CSS's are. `max-content` is as wide as its content laid out on one line, `min-content` as narrow as it can wrap (a text with `wrap = true`: its longest word; text that doesn't wrap is as wide as its line either way), and `fit-content` takes the room it's given between the two. `fit-content(320px)` fits within 320 points instead.

```lua
return ui.column {
	w = 400,
	gap = 8,
	ui.text { w = "fit-content", px = 8, bg = "$surface.raised", "A badge, not the column's width" },
	ui.div { w = 1000, max_w = "max-content", ui.text { "As wide as this line" } },
}
```

- In a column an element stretches to the column's width; with one of these it doesn't.
- `max-content` can be wider than the parent, as in CSS. It overflows; use `fit-content` to stay inside.
- On `h`, `min_h` and `max_h` they lay out as `auto`: an element's height at its width.
- They stand alone, as in CSS: `calc(max-content + 8px)` is an error. Use padding instead.
- The parent measures the element's content once each time it lays out, which costs a little more than a fixed width. Like size math, it makes the parent lay out with taffy.

### Sticky

`sticky_top` holds an element below the top of the scroll view around it once scrolling would take it past, for as long as its parent's box lasts, as CSS's `position: sticky` with `top`. A section's header stays in view until the next section pushes it out, with no script running as it scrolls.

```lua
local function section(name)
	return ui.column {
		ui.text { name, h = 28, bg = "$surface.raised", sticky_top = 0 },
		ui.div { h = 400 },
	}
end
return ui.scroll { w = 240, h = 300, section("Inbox"), section("Sent") }
```

- It keeps its place in layout: what follows it doesn't move up, as with CSS.
- It paints and takes hits over its siblings, as `z = 1` would; a `z` of its own replaces that.
- Its parent's box ends the hold, and a `ui.each` is a box around its rows: give each row a section of its own, with the header inside.
- It sticks to the nearest scroll view, and only to the top of a vertical one. There's no `sticky_bottom` or `sticky_left` yet.
- A sticky element inside another sticky element overshoots: stick one of them.
- `ui.list` has its own `sticky` rows, built over a list that doesn't keep them all ([scrolling](scrolling.md)). On `ui.list` itself, `sticky_top` is their inset.

### Stacking

Siblings paint in document order, later over earlier. `z` changes that for a node among its siblings, for a raised card or a badge over its neighbour:

```lua
return ui.overlay {
	w = 200,
	h = 100,
	ui.div { w = 120, h = 80, bg = "$accent", z = 1 },
	ui.div { w = 200, h = 60, bg = "$surface.raised" },
}
```

- **Among siblings only, unlike CSS's `z-index`.** There are no stacking contexts: a node can't rise past what its parent is under, whatever its `z`. To put something over a later branch, raise the branch.
- Only painting and hit-testing change. Layout, Tab order and what assistive technology reads keep document order, as in CSS.
- A raised node over a scroll view keeps that view from scrolling by copying pixels, as anything drawn over it does.
- For what must rise above everything (a menu, a tooltip), use an overlay ([overlays](overlays.md)), not a large `z`.

### Display contents

`display = "contents"` lays an element's children out in its place, as its parent's own, as CSS `display: contents` does. On `ui.each`, it makes the rows the grid's cells or the row's items, instead of one box holding them.

```lua
local items = ui.state("items", { "One", "Two", "Three", "Four" })
return ui.div {
	grid = true,
	cols = 3,
	gap = 8,
	ui.div { h = 40, bg = "$surface.raised", ui.text { "Pinned" } },
	ui.each(items, function(item: string)
		return ui.div { h = 40, bg = "$surface.raised", ui.text { item, literal = true } }
	end, { display = "contents" }),
}
```

- Its own size, padding and margins don't apply. It spans its children for paint, clip and hits, so a `bg` on it paints the box around them.
- Focus order and assistive technology keep it where it is: a role or label on it stays, with its children under it.
- Its parent lays out with taffy. Use it where the rows must join the parent's own flow (grid cells, items wrapping with the parent's, a `gap` between them and their siblings); where a box holding them lays out the same, keep the box. 500 rows, counted instructions:

  | 500 rows in | box (`ui.each` default) | `display = "contents"` |
  | --- | ---: | ---: |
  | a column, mount | 5.9 M | 9.2 M |
  | a column, each relayout | 0.3 M | 1.2 M |
  | a wrapping row, mount | 25.2 M | 12.9 M |
  | a wrapping row, each relayout | 13.1 M | 3.3 M |

  A column or row of boxes skips taffy; a box that wraps inside a wrapping row is measured several times by its parent.

### Grid tracks

`cols` and `rows` take a number of equal tracks, or a list of sizes. Each size is a number or `"12px"` (points), `"25%"` (of the grid), `"1fr"` (a share of the room left), `"auto"`, `"min-content"`, `"max-content"`, `"minmax(120, 1fr)"` or `"fit-content(300)"`. `"repeat(3, 1fr 2fr)"` repeats a list.

```lua
return ui.div {
	grid = true,
	w_full = true,
	h = 400,
	cols = "220 1fr auto",
	rows = "48 1fr",
	gap = 8,
	ui.div { col_span_full = true, "Toolbar" },
	ui.div { "Sidebar" },
	ui.div { "Content" },
	ui.div { w = 160, "Inspector" },
}
```

`repeat(auto-fill, …)` fits as many tracks as the grid has room for, so cards reflow as the window resizes, with no `on_resize` and no script running:

```lua
local items = ui.state("items", { "One", "Two", "Three", "Four", "Five" })
return ui.div {
	grid = true,
	w_full = true,
	cols = "repeat(auto-fill, minmax(240, 1fr))",
	gap = 12,
	ui.each(items, function(item: string)
		return ui.div { h = 120, bg = "$surface.raised", rounded = "$radius.md", ui.text { item, literal = true } }
	end),
}
```

`auto-fill` keeps the tracks left over, so a short last row keeps its cards the same size. `auto-fit` collapses empty tracks, so a few cards spread across the whole width.

- The rules are CSS's: `repeat(auto-fill, …)` needs a fixed size to count what fits (`minmax(240, 1fr)`, not `1fr` or `auto`), a grid takes one of `auto-fill` and `auto-fit`, beside one every other track has a fixed size, and a minimum can't be `fr`.
- A list that can't be read is an error, not a silent fallback. Written as a literal it stops the script with the reason. From a state or a function it shows in the error banner and the grid keeps its last list.
- A state or function may produce a new list every frame (a dragged sidebar's `` `{w:get()} 1fr` ``): a list is held by the node showing it and goes with it.
- Not supported: subgrid, and line names inside `repeat()`.

Areas name a grid's cells, a string a row, and items place themselves in them by name (CSS `grid-template-areas` and `grid-area`). Each name has to cover a rectangle; `.` is an empty cell.

```lua
return ui.div {
	grid = true,
	w = 600,
	h = 400,
	cols = "200 1fr",
	rows = "48 1fr",
	areas = { "head head", "side main" },
	ui.div { area = "head", bg = "$surface.raised" },
	ui.div { area = "side", bg = "$surface.raised" },
	ui.div { area = "main" },
}
```

- `[name]` in a track list names the line there, and `grid_column` and `grid_row` take lines as CSS's `grid-column` and `grid-row` write them: `"side / main"`, `"2 / span 3"`, `"1 / -1"`, or a number. An area's edges are lines too, named `head-start` and `head-end`.
- `flow = "dense"` (CSS `grid-auto-flow: dense`) fills holes earlier items left with later small ones; `"column"` places down columns.
- Lines and areas go over `col_start` and the like.


## Paint

| Property | Type | Default | |
| --- | --- | --- | --- |
| `bg` | `Color` | none (button, input, slider, progress: `#232e40`) | |
| `gradient` | `Gradient` | | `{ "#f80", "#08f" }` or `{ angle = 135, stops = { … } }`; stops are colors (evenly spaced) or `{ offset, color }`; CSS angles: 0 runs bottom to top, 90 left to right, 180 (the default) top to bottom |
| `radial` | `RadialGradient` | | a radial gradient (CSS `radial-gradient`, its default ellipse): `{ "#fff", "#0000" }` from the centre to the edge, or `{ center = { 0.5, 0 }, radius = 0.8, stops = { … } }` (`center` in fractions of the box; `radius` 1 reaches its farthest corner). Two or three stops, and an error for more: they don't fit its shader's uniforms. Boxes only, not a canvas, and it takes the place of a `bg_shader`. Native gradients with any number of stops, and conic and repeating ones, wait for 0.2 |
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
| `inset_shadow` | `Shadow` | none | shadows inside the box (CSS's `inset`), over its fill and under its border and children: the box less itself moved by `x`, `y` and shrunk by `spread`, cut to its corners; `blur` softens it into the box, half its colour on that edge, as CSS's does. A 1-point top highlight is `{ x = 0, y = 1, color = "#ffffff1f" }` |
| `opacity` | `number` | `1` | 0 to 1 |
| `brightness`, `contrast`, `saturate` | `number` | `1` | CSS filter functions over the node and its subtree |
| `hue_rotate`, `sepia`, `invert` | `number` | `0` | CSS filter functions over the node and its subtree, by the spec's matrices: `hue_rotate` in degrees, `sepia` and `invert` from 0 to 1 (`invert = 1` for a dark-mode icon). They paint it once into a layer that composites through a shader, so they cost what a layer does, and take the place of a `layer_shader`. Not under Direct3D 11 |
| `blur` | `number` | `0` | blurs the node's content (CSS `filter: blur()`) |
| `backdrop_blur` | `number` | `0` | blurs what is behind it: frosted glass |
| `fade` | `Pair` | | a clipping view's top and bottom edges fade over those widths |
| `fade_x` | `Pair` | | its left and right edges |
| `fade_sides` | `Sides` | | each edge: `{ top, right, bottom, left }`. A fade inside another fade is faded by both (the nearest two apply) |
| `layer` | `boolean` | `false` | paint the subtree into its own layer |
| `fit` | `"fill" \| "contain" \| "cover" \| "none" \| "scale_down"` | `"fill"` | how an image fills its box |
| `image_rendering` | `"auto" \| "pixelated"` | `"auto"` | how an image, or a background image, is sampled drawn larger or smaller (CSS's `image-rendering`): `"pixelated"`, the nearest pixel, its edges hard (pixel art, a QR code) |
| `object_position` | `{ x, y }` | `{ 0.5, 0.5 }` | where an image lies in its box where it doesn't fill it (CSS's `object-position`): fractions of the room to spare, so `{ 0.5, 0 }` keeps a portrait's top as `fit = "cover"` crops it. For `ui.image`; a `bg_image` stays centred until 0.2 |
| `tint` | `Color` | | recolors an SVG (a state or function re-tints it) |
| `decode` | `"auto" \| "sync" \| "async"` | `"auto"` | when an image's file decodes (HTML's `decoding`; see [Images and canvas](media.md)) |
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
| `wrap` | `boolean \| "balance"` | `false` | wrap at the box's width. `"balance"` also balances the lines (CSS's `text-wrap: balance`): they break at the narrowest width that keeps as many, so a heading's last line isn't left short, for up to six lines, the box as it was. It's inherited: on a container, the text in it balances |
| `truncate` | `boolean` | `false` | one line ending in "…" |
| `lines` | `number` | all | clamp to that many lines |
| `line_height` | `number` | the font's | points |
| `letter_spacing` | `number` | `0` | points |
| `tabular` | `boolean` | `false` | tabular figures: every digit one width, so a changing number holds still |
| `font_features` | `string` | none | OpenType features, as CSS's `font-feature-settings`: `"ss01 cv02=3 liga=0"`, a tag on, or `tag=value` (`0` off). `hyphens` breaks words at their hyphenation points with a hyphen, as CSS's `hyphens: auto`, in the system's language (zgx's `hyphens` feature; without it, an error where scripts are linted: a debug build, `zgx lint`. `hyphens = "auto"` of its own comes in 0.2). What the font lacks does nothing, and what isn't a four-letter tag with a number is left out. Replaces `tabular`: give `"tnum"` here too |
| `underline` | `boolean` | `false` | underlined text, inherited (CSS's `text-decoration`): `hover = { underline = true }` underlines a link button's label |
| `underline_offset` | `number` | a tenth of the text size | points below the baseline the underline sits; inherited |
| `lang` | `string` | its parent's (the app's locale at the root) | the language its text is in (HTML's `lang`, a BCP 47 tag: `"ja"`, `"zh-TW"`), inherited. Han characters take that language's fonts: a Chinese quote in a Japanese app in a Chinese face, a Japanese name in a Chinese one's in a Japanese face. It wins over the app's locale and over what a line's kana or hangul say. Screen readers read the text in it (AccessKit's language). A function or state follows |

### Which way text from people runs

A message or a field holds what someone wrote, in whichever language. Its letters always run the right way, but `dir` decides which side it starts from, and a function follows the text: HTML's `dir="auto"` looks at the first strong letter, and so can a script.

```lua
local message = ui.state("message", "مرحبا")

local function direction(text: string): "ltr" | "rtl"
	for _, c in utf8.codes(text) do
		-- Hebrew, Arabic and their neighbours, and their presentation forms.
		if (c >= 0x590 and c <= 0x8FF) or (c >= 0xFB1D and c <= 0xFDFF) or (c >= 0xFE70 and c <= 0xFEFF) then
			return "rtl"
		elseif (c >= 65 and c <= 90) or (c >= 97 and c <= 122) or c >= 0xC0 then
			return "ltr"
		end
	end
	return "ltr"
end

return ui.textarea {
	w = 300,
	h = 80,
	value = message,
	dir = function()
		return direction(message:get())
	end,
}
```

## Transforms

`x`, `y` (`number`, points), `scale`, `scale_x`, `scale_y` (`number`, `1`), `rotate`, `skew`, `skew_x`, `skew_y`, `rotate_x`, `rotate_y` (`number`, degrees), `perspective` (`number`, points, `0`: none) and `origin` (`Pair`, fractions of the node, `{ 0.5, 0.5 }`). They apply after layout: the node keeps its place in the flow, and hit testing follows the transform. Rotation, scale, skew and tilt paint the subtree into a cached layer. `rotate_x` and `rotate_y` tilt it about the axes through `origin` (CSS `rotateX`, `rotateY`), seen from `perspective` points away (CSS `perspective()`): the nearer side shows larger, the layer composited as a projected quad; without a perspective a tilt only narrows it.

Known limit: where a tilted node's edge comes past the eye (`perspective` smaller than how far that edge comes forward), it doesn't draw as CSS draws it, cut off at the eye: the quad's depth is clamped at a tiny distance, so it shows as a distorted sliver, wherever the node lies. The frame doesn't fail. The clamp is in the shared quad shader, `draw.wgsl`, whose Direct3D 11 form is compiled ahead with FXC on Windows (`crates/zgx-gpu/src/dxbc/`), so it can only change along with those binaries.

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
| `selection_radius` | `number \| Readable<number> \| (() -> number)` | `$selection.radius` | a selectable node's highlight corners, at most half a line: its lines drawn as one shape. `0` draws them as plain rectangles. It sets `$selection.radius` for the node and what it holds, so editors inside follow it too |
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
| `hit` | `"target"`, `"shown"` | `"shown"` | `"target"`: while an animation moves it, it takes clicks and hover where it's going ([hit at target](#hit-at-target)) |

## Accessibility

What assistive technology (VoiceOver, Narrator, Orca) reads for a node beyond what it is: controls say their role, name and state themselves, and these say what a plain box means. They cost nothing on nodes that don't set them. A `label` given as a state or function costs nothing either while no screen reader is on: it is worked out only while assistive technology reads the window (from the state as it is, when one starts), not each time what it reads changes.

| Property | Type | Default | |
| --- | --- | --- | --- |
| `label` | `string \| state \| function` | its text | the name read for a box (ARIA's `aria-label`): an icon button, a group. A state or function is followed, for a recycled list row's name made from what it shows now (`"Unread, Grace Hopper, Lunch?, Oct 28"`). Widgets take their own `label` |
| `role` | `AriaRole`: `"group"`, `"button"`, `"link"`, `"heading"`, `"list"`, `"listitem"`, `"listbox"`, `"option"`, `"toolbar"`, `"status"`, `"tablist"`, `"tab"`, `"tabpanel"`, `"article"` (a message, a post), `"dialog"`, `"img"`, `"radiogroup"`, `"radio"`, `"switch"`, `"checkbox"`, `"spinbutton"`, `"combobox"`, `"tree"`, `"treeitem"`, `"grid"`, `"row"`, `"gridcell"`, `"columnheader"`, `"alert"`, `"menu"`, `"menuitem"`, `"menuitemcheckbox"`, `"menuitemradio"` (both need `checked`) | what it is | what it is taken for (ARIA's `role`), keeping what it does: a clickable row as an `"option"` in a `"listbox"`, a row of buttons as a `"toolbar"`. Options, tabs, list items and headings are named by their text. A `"listitem"`'s marker ("•", "3.") is its first text with `aria_hidden = true`: the list owns it, so selections and plain text leave it out and copied HTML is a `<ul>` or `<ol>` (`zgx lint`'s `list-marker-hidden`) |
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
