# Images and canvas: image, svg, canvas

Pixels from files, vector icons, and drawing in script. Paths in `src` are relative to the script's directory. The same section covers what makes paint for these and other nodes: `ui.path` and `ui.linear` for canvases, `ui.bitmap` for masks, and `ui.shader` for shader fills.

```luau
ui.image(props: ImageProps) -> Node
ui.svg(props: SvgProps) -> Node
ui.canvas(props: CanvasProps) -> Node
ui.path() -> Path
ui.linear(x1: number, y1: number, x2: number, y2: number, colors: { read [number]: Color }) -> Brush
ui.bitmap(w: number, h: number, alpha: (x: number, y: number) -> number) -> Bitmap
ui.shader(source: string, opts: { fps: number? }?) -> Shader
```

## image

A PNG or JPEG, decoded once per `src` and kept across reloads.

| Option | Type | Default | |
| --- | --- | --- | --- |
| `src` | `Reactive<string>` | required | a path (from the script's directory) or an absolute one; `"host:<name>"`, an image the app made (`ScriptHost::image`), following each one it replaces it with; a state or function swaps the image |
| `fallback` | `Node` | none | shown in the image's place while a remote one (`https://…`) loads, and if it fails (a failed picture is logged, not a script error): an avatar's initials |
| `fit` | `"fill" \| "contain" \| "cover" \| "none" \| "scale_down"` | `"fill"` | how the pixels fill the box (CSS `object-fit`) |
| `label` | `string` | `""` | its accessible name |
| `w`, `h` | `Length` | the image's size in pixels, as points | |

## svg

An SVG, from a file or from markup, drawn as vectors at every scale; `tint` recolors it.

| Option | Type | Default | |
| --- | --- | --- | --- |
| `src` | `Reactive<string>` | `src` or `code` | a path; a state or function swaps it, and may return `"code:" .. markup` |
| `code` | `string` | | SVG markup |
| `tint` | `Reactive<Color>` | none | every painted pixel in this color, keeping its alpha (a state or function re-tints); `"currentColor"` takes the text color the node inherits, as CSS's does, so an icon follows its button's `color` and `hover` color |
| `label` | `string` | `""` | its accessible name |
| `w`, `h` | `Length` | the SVG's size | |

## canvas

`draw(c, w, h)` paints the canvas's box, `w` by `h` points. It runs when the canvas first lays out, and again when its size or a state it read changes; an equal drawing keeps its pixels.

| Option | Type | Default | |
| --- | --- | --- | --- |
| `draw` | `(c: Canvas, w: number, h: number) -> ()` | required | |

| Method of `c` | |
| --- | --- |
| `c:rect(x, y, w, h, paint, radius?)` | a filled rectangle, its corners rounded by `radius` (0) |
| `c:circle(cx, cy, r, paint)` | a filled circle |
| `c:line(x1, y1, x2, y2, paint, width?, opts?)` | a stroked line, `width` 1; `opts`: `cap` (`"butt"`, `"round"`, `"square"`), `join` (`"miter"`, `"round"`, `"bevel"`), `dash` (`{ on, off, … }`), `dash_offset` |
| `c:fill(path, paint, rule?)` | fills a path; `rule` `"nonzero"` (the default) or `"evenodd"` |
| `c:stroke(path, paint, width?, opts?)` | strokes a path; `opts` as `line`'s, and `draw = t` (or `{ start, end }`) strokes only that fraction of it, as anime.js draws lines |
| `c:clip(path)` | clips what follows to the path, until `c:restore()` |
| `c:restore()` | ends the last clip |

`paint` is a [color](properties.md) or a brush from `ui.linear(x1, y1, x2, y2, { colors })`, its colors spread evenly from (`x1`, `y1`) to (`x2`, `y2`). A color may name a theme token (`"$accent"`, `"$surface.raised"`), resolved in the canvas's theme while `draw` runs: the drawing follows it as it follows state, so a theme change draws it again. `ui.linear` called inside `draw` takes tokens too.

A path is built by chaining: `ui.path():move(x, y):line(x, y):quad(cx, cy, x, y):cubic(c1x, c1y, c2x, c2y, x, y):arc(cx, cy, r, start, sweep):close()`. `arc` angles are degrees, clockwise from +x. Once drawn, a path is fixed: it can be drawn again (make it once, outside `draw`), not extended. `:rect(x, y, w, h, r?)` and `:ellipse(x, y, w, h)` add a rounded rectangle or an ellipse as a closed subpath.

A canvas whose drawing is only solid fills and strokes of rectangles (rounded or not), circles and lines along an axis (`c:rect`, `c:circle`, `c:line`, or a path that is just one `:rect` or `:ellipse`) is drawn by the GPU as those shapes: nothing is rasterized or uploaded, so redrawing it as a view pans or zooms costs its shapes alone. Anything else in it (curves, other ellipses, gradients, patterns, dashes, clips), or the canvas faded as a whole (`opacity` below 1, an edge fade), rasterizes it as before.

## Masks and shaders

Any node can show its content through a bitmap's alpha, or have its box filled by a WGSL function; see [properties](properties.md#paint):

| | |
| --- | --- |
| `ui.bitmap(w, h, fn)` | `w` by `h` pixels (1 to 4096 a side), white, each with the alpha `fn(x, y)` returns (0 to 1) |
| `mask = bitmap` or `mask = { bitmap, w = …, h = …, x = …, y = …, sweep = … }` | the content shows through the bitmap's alpha, drawn `w` by `h` (the bitmap's size) from (`x`, `y`), unsmoothed; `maskX` and `maskY` animate its place, and `sweep = { x, y, duration, frame_rate }` moves it by (`x`, `y`) every `duration` ms, over and over, on the engine's own clock (a text shimmer: no script, style pass or layout per step, only the masked node composites again; 4 steps a second while it doesn't show) |
| `ui.shader(wgsl, { fps })` | a fill: the source defines `fn fill(input: FillInput) -> vec4<f32>` (straight alpha), reading `input.p` (points from the box's top left), `input.size`, `input.time` (seconds), `input.scale` (pixels per point) and `input.params` (four `vec4`s); `fps` caps how often time repaints it |
| `bg_shader = shader`, `shader_params = { … }` | fill the box with it, over `bg` and under the border and children; parameters are colors (0 to 1) or lists of four numbers; `shaderTime` and `shaderParam0` to `shaderParam15` animate |

## Example

```lua
local level = ui.state("level", 0.6)
local wave = ui.path():move(0, 30):cubic(30, 0, 60, 60, 90, 30):cubic(120, 0, 150, 60, 180, 30)
local glow = ui.shader([[
fn fill(input: FillInput) -> vec4<f32> {
	let t = input.p.x / max(input.size.x, 1.0);
	return vec4<f32>(0.2 + 0.6 * t, 0.4, 1.0 - 0.5 * t, 1.0);
}]])
-- A dither over the whole box: every other pixel shows.
local dots = ui.bitmap(180, 24, function(x, y)
	return if (x + y) % 2 == 0 then 1 else 0
end)
return ui.column {
	p = 24,
	gap = 12,
	ui.row {
		gap = 12,
		items = "center",
		ui.image { src = "photo.png", w = 48, h = 48, fit = "cover", rounded = 8, label = "Avatar" },
		ui.svg { src = "icon.svg", w = 20, h = 20, tint = "#5ea5ff" },
		ui.svg { code = "<svg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 2 2'><rect width='2' height='2'/></svg>", w = 12, h = 12, tint = "#e5534b" },
	},
	ui.canvas {
		w = 180,
		h = 60,
		draw = function(c, w, h)
			c:rect(0, 0, w, h, "#161b22", 8)
			c:rect(0, h - 6, w * level:get(), 6, ui.linear(0, 0, w, 0, { "#5ea5ff", "#a99cff" }))
			c:stroke(wave, "#ffffffcc", 2, { cap = "round", draw = level:get() })
		end,
	},
	ui.div { w = 180, h = 24, rounded = 6, bg_shader = glow },
	ui.div { w = 180, h = 24, bg = "#5ea5ff", mask = dots },
	ui.slider { label = "Level", value = level, w = 180 },
}
```

## Gotchas

- `ui.image` needs `src`; `code` is the SVG's (an image with only `code` fails to load). A missing or undecodable file shows the error in the window and an empty image.
- Images and SVGs are decoded once per `src` and shared by every node showing it. A host that watches its scripts (`ScriptHost::watch`) also watches these files: saving an edited image reloads the scripts, state kept, and they show the new one.
- An image is as large as its pixels in points unless sized; give it a `w` and `h` (or `aspect` and one of them).
- `draw` runs again only for its size and the state it read; reading state through `peek` (or outside `draw`) does not redraw it. Keep it cheap: it runs in the script for every change it follows.
- A drawn path cannot grow (`a drawn path cannot be extended`): build paths whole, then draw them.
- `ui.shader` checks the WGSL when it is made: a source that does not compile is a script error with its line. A shader is compiled once per source and `fps`.
- A mask's bitmap is sampled without smoothing, which suits dithers and sprites; scale it with `w` and `h`, not by making it larger.
