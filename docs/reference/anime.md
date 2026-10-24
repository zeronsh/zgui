# Animation: anime and bindings

The `anime` global follows [anime.js v4](https://animejs.com/documentation/)'s API; animations run in a Rust engine that ticks every playing animation once per display frame and asks for no frames at rest. `ui.tween`, `ui.spring` and `ui.loop` are declarative bindings: a property bound to one eases on its own whenever its state changes. Durations are milliseconds everywhere.

```luau
anime.animate(targets: ElementTargets, params: ElementParams) -> Animation
anime.animate(targets: State<any> | { [string]: any }, params: TableParams) -> Animation
anime.createTimeline(params: TimelineParams?) -> Animation   -- also anime.timeline
anime.createTimer(params: Timing?) -> Animation
anime.createAnimatable(targets, params: { [string]: number | Ease | Modifier | Composition | Tween }) -> Animatable
anime.createDraggable(target: ElementTargets, params: DraggableParams?) -> Draggable
anime.onScroll(params: ScrollParams?) -> ScrollObserver
anime.stagger(value: number | string | { number | string }, params: StaggerParams?) -> Stagger
anime.createSpring(params: { mass: number?, stiffness: number?, damping: number?, velocity: number? }?) -> AnimeEase
anime.spring(params: { bounce: number?, duration: number? }?) -> AnimeEase
anime.createScope(params: { defaults: Timing? }?) -> Scope
ui.tween(state: State<any>, opts: TweenOptions?) -> Binding
ui.spring(state: State<any>, opts: SpringOptions?) -> Binding
ui.loop(opts: LoopOptions?) -> Binding
```

## Targets

`ElementTargets` are selectors (`"#id"`, `".class"`, a kind such as `"button"`, compounds such as `"button.primary"`, comma lists, `"*"`), elements (from `on_mount`, a ref's `get`), refs, or lists of them. A selector matches nodes with that `id`, `class` or kind; a node only becomes a target with an `id`, `class`, `ref`, `on_mount` or `on_resize`. Matches are in document order, the order staggers count in. An animation resolves its targets the first time it plays, so a script may create animations before the elements they target mount. A state (its number, or a table's number fields) or a plain Luau table animates too: `anime.animate(scroll_state, { … })`.

## Parameters

`ElementParams` are an element's [animatable properties](#element-properties) and these `Timing` parameters (anime.js's names; the engine also takes snake_case, `on_complete`, `loop_delay`, but the types and `zgx check` know only these):

| Parameter | Type | Default | |
| --- | --- | --- | --- |
| `duration` | `AnimNumber` | `1000` | ms; a number, a stagger, or a function of `(target, index, total)` |
| `delay` | `AnimNumber` | `0` | ms |
| `ease` | `Ease` | `"out(2)"` | a name, an anime ease, or a function of progress |
| `composition` | `"replace" \| "none" \| "add"` | `"replace"` | what happens to another animation of the same property: it stops writing it (and an animation left with nothing to write is cancelled, calling nothing more, as in anime.js), both write, or values add |
| `modifier` | `Modifier` | none | maps each value: a function or `anime.utils.round(n)` |
| `loop` | `number \| boolean` | `0` | repeats; `true` forever |
| `loopDelay` | `number` | `0` | ms between loops |
| `alternate` | `boolean` | `false` | every other loop runs backwards |
| `reversed` | `boolean` | `false` | |
| `autoplay` | `boolean \| ScrollObserver` | `true` | `false` waits for `play()`; an `anime.onScroll` plays it by scrolling |
| `frameRate` | `number` | every frame | caps the rate it renders at |
| `playbackRate` | `number` | `1` | |
| `playbackEase` | `Ease` | none | eases the whole animation's progress |
| `keyframes` | `{ ElementKeys } \| { [string]: ElementKeys }` | none | property sets in turn, or by percentage: `{ ["0%"] = …, ["50%"] = … }` |
| `id` | `string` | a number | names the animation (`anim.id`) |
| `onBegin`, `onBeforeUpdate`, `onUpdate`, `onRender`, `onLoop`, `onPause`, `onComplete` | `(anim: Animation) -> ()` | none | |

Property values (`AnimValue`): numbers (bare angles are degrees), strings with units (`"10px"`, `"50%"`, `"1rem"`, `"90deg"`, `"1turn"`), relative values (`"+=10"`, `"-=10"`, `"*=2"`), colors, strings with numbers in them, functions of `(target, index, total)`, staggers, keyframe lists (`{ 0, 240 }`: from, …, to), and tween tables `{ to, from, duration, delay, ease, composition, modifier }`, alone or in lists.

Eases: `linear` (or `linear(0, .25, 1)` piecewise), `in(p)`, `out(p)`, `inOut(p)`, `outIn(p)` (power `p`, 1.68 by default), the same modes of `Quad`, `Cubic`, `Quart`, `Quint`, `Sine`, `Circ`, `Expo`, `Bounce`, `Back(overshoot)` and `Elastic(amplitude, period)` (`"outElastic(1, .5)"`), `steps(n, fromStart)`, `cubicBezier(x1, y1, x2, y2)`, `irregular(n, randomness)` and v3's names (`easeInOutQuad`). `anime.eases.outQuad()` and the other names make the same as values; `anime.spring { bounce, duration }` (a spring that looks that bouncy and settles in `duration` ms) and `anime.createSpring { mass, stiffness, damping, velocity }` set the animation's duration to the time they settle in.

## Element properties

Compositor properties change no layout and are written straight to the scene: `x`, `y` (also `translateX`, `translateY`), `opacity`, `scale`, `scaleX`, `scaleY`, `rotate` (`rotateZ`, `rotation`), `skew`, `skewX`, `skewY`, `blur` (the content's), `brightness`, `contrast`, `saturate`, `maskX`, `maskY`, `clipTop`, `clipRight`, `clipBottom`, `clipLeft`, `shaderTime` and `shaderParam0` to `shaderParam15`. Others go through the node's style: colors (`bg`, `backgroundColor`, `color`, `border_color`, `tint`), sizes (`w`/`width`, `h`/`height`, `min_w`, `max_h`…), insets (`top`, `left`…), padding and margins, radii (`rounded`, `borderRadius`, single corners), `border`/`borderWidth`, `gap`, `text_size`/`fontSize`, `weight`/`fontWeight`, `line_height`, `letter_spacing`, grid tracks and spans, `fade`, `fade_x`, `border_dash`, `origin`, `aspect`, `grow`, `shrink`, `basis`. The full list is `ElementKeys` in the types. An animated value wins over the node's own until `anime.utils.cleanInlineStyles` (or `revert`) gives it back.

## Animations

`Animation` (from `animate`, `createTimeline` and `createTimer`):

| Field | Type | |
| --- | --- | --- |
| `currentTime` | `number` | settable: seeks |
| `progress` | `number` | 0 to 1; settable: seeks |
| `iterationProgress`, `iterationCurrentTime`, `currentIteration` | `number` | within the current loop |
| `duration`, `iterationDuration` | `number` | ms |
| `paused` | `boolean` | settable: pauses or resumes |
| `began`, `completed`, `reversed` | `boolean` | |
| `speed`, `playbackRate` | `number` | settable |
| `labels` | `{ [string]: number }` | a timeline's labels' times |
| `id` | `number \| string` | |

Methods, each returning the animation, working as `a:play()` and as `a.play()`: `play`, `pause`, `resume`, `restart`, `reverse`, `alternate`, `seek(time)`, `complete`, `cancel`, `revert` (cancels and gives the targets their own values back), `reset`, `stretch(duration)`, `refresh` (recomputes function values), `init` (builds it now) and `done(f)` (anime.js `then`, a Luau keyword; `a["then"](f)` also works; runs `f` at once if already complete).

Timelines also take `add(targets, params, position)` (or `add(timerParams, position)`), `set(targets, params, position)`, `call(f, position)`, `label(name, position)`, `sync(animation, position)` and `remove(targets)`. Positions: `500`, `"+=100"`, `"-=100"`, `"<"` (where the previous ends), `"<<"` (where it starts), `"<+=50"`, `"label"`, `"label+=50"`. `createTimeline { defaults = { … } }` gives every child those parameters.

## The rest

| Function | |
| --- | --- |
| `createTimer(params)` | an animation of nothing: its callbacks and time, endless unless given a `duration` |
| `createAnimatable(targets, { x = 400, y = { duration = 600, ease = "out(3)" } })` | each named property becomes a function: `a.x(value, duration?, ease?)` animates to `value`, `a.x()` reads it (a bare number is that property's duration); `a.revert()`, `a.cancel()` |
| `createDraggable(target, params)` | drag a node by its translation. Params: `x`, `y` (`true`, `false` or `{ snap = step or points }`), `snap`, `container` (targets, or `{ top, right, bottom, left }`), `containerPadding`, `containerFriction` (0.8), `dragSpeed` (1), `velocityMultiplier` (1), `minVelocity` (0), `maxVelocity` (50), `releaseMass` (1), `releaseStiffness` (80), `releaseDamping` (20), `cursor` (`false` keeps it), `onGrab`, `onDrag`, `onUpdate`, `onRelease`, `onSnap`, `onSettle`. A `Draggable` has `x`, `y`, `isDragging`, `enabled`, `velocity` and `enable`, `disable`, `reset`, `revert`, `stop`, `setX(x)`, `setY(y)`; it attaches once its target mounts |
| `onScroll(params)` | pass as `autoplay`: plays the animation as its `target` scrolls through its `container` (the nearest scroll ancestor). `enter` (`"end start"`) and `leave` (`"start end"`) are `"<container edge> <target edge>"` with `start`, `center`, `end`, percentages, points and `+=`/`-=`; `sync` (`true`, a smoothing amount from 0 to 1, or an ease name) scrubs instead of playing; `repeat` (`true`) plays again on each entry; `onEnter`, `onLeave`, `onUpdate`, `onSync` |
| `stagger(value, params)` | a value per target: `stagger(60)` adds 60 ms per index, `stagger({ -100, 100 })` spreads a range; params `start`, `from` (`"first"`, `"center"`, `"last"`, `"random"`, an index), `reversed`, `ease`, `grid` (`{ columns, rows }`), `axis` (`"x"`, `"y"`), `use` (a function giving each target's index), `total`, `modifier` |
| `createScope({ defaults })` | `scope:add(function(scope) … end)` runs the function; what it creates takes the defaults and `scope:revert()` reverts it all |
| `eases.<name>(…)` | an ease as a value |
| `engine` | `speed` (= `playbackRate`), `fps` (nil: every display frame), `timeUnit` (`"ms"` or `"s"`), and read-only `currentTime` and `paused`; `engine.pause()`, `engine.resume()` |
| `utils` | `set(targets, props)`, `get(target, property)`, `remove(targets)` (stops their animations), `cleanInlineStyles(targets or animation, props?)`, `random(min, max, decimals)`, `randomPick`, `shuffle`, `round(value, decimals)`, `clamp`, `snap(value, step)`, `wrap`, `mapRange`, `lerp`, `damp(a, b, dt, factor)`, `degToRad`, `radToDeg`, `roundPad`, `padStart`, `padEnd`, `createSeededRandom(seed)` |
| `svg.createMotionPath(d)` | from SVG path data: `translateX`, `translateY` (also `x`, `y`) and `rotate` values that follow the path, and its `length`; `svg.getTotalLength(d)` |
| `text.split(text, params)` | a row of texts, one per character (`chars`, the default) or word (`words`), each with a class to animate (`"char"`, `"word"`, or `class`); `style` styles each piece, `props` the row |
| `anime { targets = …, … }` | anime.js v3's call form (not in the types) |

## Bindings

A property bound to `ui.tween(state, opts)` or `ui.spring(state, opts)` eases to its new value whenever the state changes, in Rust, with no script call per frame. Without `from` and `to` the scalar is the state's number (a boolean is 0 or 1); with both, it maps 0 to `from` and 1 to `to` (numbers, colors or pairs). Colors may be theme tokens (`from = "$text.secondary", to = "$text.primary"`): the node blends them from its theme as it eases, and keeps following theme changes. `ui.loop(opts)` runs from 0 to 1 and back for as long as the node is mounted.

| Option | Type | Default | |
| --- | --- | --- | --- |
| `duration` | `number` | tween `250`, loop `1000` (a cycle) | ms |
| `ease` | `Easing` | tween `"out(3)"`, loop `"inOut(2)"` | a name or an anime ease (not a function) |
| `stiffness`, `damping` | `number` | `260`, `24` | `ui.spring` only |
| `yoyo` | `boolean` | `true` | `ui.loop`: back and forth, or restart each cycle |
| `fps` | `number` | every frame | `ui.loop`: caps the frames it asks for |
| `from`, `to` | `number \| string \| { number }` | none | both or neither |

## Example

```lua
local open = ui.state("open", false)
local items = { "One", "Two", "Three" }
local rows: { Node } = {}
for _, name in items do
	table.insert(rows, ui.text { name, class = "row", opacity = 0 })
end
anime.animate(".row", {
	opacity = { 0, 1 },
	x = { -12, 0 },
	delay = anime.stagger(60),
	duration = 400,
	ease = "outQuint",
})
return ui.column {
	p = 24,
	gap = 8,
	ui.button {
		on_click = ui.toggle(open),
		on_mount = function(el)
			local pulse = anime.createTimeline { defaults = { duration = 300, ease = "inOut(2)" } }
			pulse:add(el, { scale = 1.05 }):add(el, { scale = 1 })
		end,
		"Toggle",
	},
	ui.div {
		h = 40,
		rounded = 8,
		bg = ui.tween(open, { from = "#232e40", to = "#5b8def", duration = 300 }),
		x = ui.spring(open, { from = 0, to = 24 }),
	},
	ui.div { w = 8, h = 8, rounded = 4, bg = "#3fb950", opacity = ui.loop { from = 0.3, to = 1 } },
	rows,
}
```

## Gotchas

- Durations are milliseconds, unless a script sets `anime.engine.timeUnit = "s"`. A fractional duration under 10 (`duration = 0.3`, meant as seconds) and a non-looping one over a minute warn, once per call site, on stderr and in `ScriptHost::warnings()`.
- A misspelled property (`bgColor`) is a type error, and an error at run time when the animation first plays.
- Only nodes with an `id`, `class`, `ref`, `on_mount` or `on_resize` are targets: a selector matches nothing else.
- `hover`, `active`, `focus` and `disabled` take plain values; animate the base style from an event (`on_hover`) instead.
- Script code runs per frame only for callbacks (`onUpdate`, `onRender`), function eases and function modifiers. `anime.utils.round(2)`, called without the value, is a modifier that runs in Rust; `round(3.14159, 2)` rounds now.
- Draggables and scroll observers live without a script handle until their script is replaced; animations of a replaced script end with it.
- Rotation, scale and skew promote a node to a cached layer: thousands at once are bounded by the renderer's layer budget.
