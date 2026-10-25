--!strict
-- Type definitions for zgx-script's globals, `ui` and `anime`, for
-- luau-lsp (editor completion and checking) and `zgx check`, which runs
--   luau-lsp analyze --definitions=crates/zgx-script/types/zgx.d.luau app.luau
-- In VS Code, set `luau-lsp.platform.type` to "standard" (requires resolve
-- by path), put `"languageMode": "strict"` in the project's .luaurc, add
-- this file to `luau-lsp.types.definitionFiles`, and turn on Luau's new
-- type solver (`"luau-lsp.fflags.override": { "LuauSolverV2": "true" }`,
-- or `--flag:LuauSolverV2=true`): the types below need it.
--
-- Edit zgx.d.luau.in: zgx.d.luau, what editors and `zgx check` read, is
-- that file with each `export type X = Merge<…>` written out as its table
-- (luau-lsp finds that many type function calls too complex to check);
-- `ZGX_BLESS=1 cargo test -p zgx-script --test tooling types::` writes it.
--
-- The `export type`s are global type names scripts can use in their own
-- annotations: `State<T>`, `Props`, `ContainerProps`, `Child`, `Color`…
-- `Merge<A, B>` makes a props type of your own from these.
--
-- The style properties and animation keys follow the engine's tables;
-- `tests/types.rs` fails if they drift, or if a `ui` function, an element's
-- option, an event, or a method of State, Element, Ref, Web, Term, Canvas
-- or Animation goes untyped.

---------------------------------------------------------------------------
-- Type functions.

-- `A` with `B`'s properties added (B's win), and B's indexer if it has one
-- (else A's): `Merge<Props, { tone: string? }>`.
export type function Merge(a, b)
	local out = types.newtable()
	for _, t in { a, b } do
		for key, property in t:properties() do
			if property.read then
				out:setreadproperty(key, property.read)
			end
			if property.write then
				out:setwriteproperty(key, property.write)
			end
		end
	end
	local indexer = b:indexer() or a:indexer()
	if indexer then
		out:setindexer(indexer.index, indexer.readresult)
	end
	return out
end

-- A list's item type: `Item<{ Tab }>` is `Tab` (`Item<{ Tab }?>` too).
-- Anything else has no items. (`index<T, number>` would fail for State of a
-- number, and generic methods lose inference.)
type function Item(t)
	if t:is("any") then
		return types.any
	end
	local parts = if t:is("union") then t:components() else { t }
	for _, part in parts do
		if part:is("table") then
			local indexer = part:indexer()
			if indexer then
				return indexer.readresult
			end
		end
	end
	return types.never
end

-- What a `ui.variants` definition's axes accept, the choosing table its
-- recipe is called with: each axis its options' names (a boolean for an
-- on-off axis, `[true]` / `[false]`), or `extra` (a state or a function
-- choosing it). Closed, as props tables are: an unknown axis is an error.
type function VariantChoice(definition, extra)
	local out = types.newtable()
	if not definition:is("table") then
		return out
	end
	local variants = definition:readproperty(types.singleton("variants"))
	if not variants or not variants:is("table") then
		return out
	end
	for axis, property in variants:properties() do
		local options = property.read
		local names: { type } = {}
		local boolean = false
		if options and options:is("table") then
			for option in options:properties() do
				local name = option:value()
				if name == "true" or name == "false" then
					boolean = true
				else
					names[#names + 1] = option
				end
			end
			local indexer = options:indexer()
			if indexer and indexer.index:is("boolean") then
				boolean = true
			end
		end
		if boolean then
			names[#names + 1] = types.boolean
		end
		names[#names + 1] = extra
		local choice = if #names == 1 then names[1] else types.unionof(table.unpack(names))
		out:setproperty(axis, types.optional(choice))
	end
	out:setindexer(types.number, types.never)
	return out
end

-- A recipe's variant props, for a component's own props type:
-- `Merge<ContainerProps, VariantProps<typeof(button)>>` (the axes alone,
-- so the props keep their children).
export type function VariantProps(recipe)
	local out = types.newtable()
	if not recipe:is("function") then
		return out
	end
	local head: { type } = recipe:parameters().head or {}
	local choice: type? = head[1]
	if choice and choice:is("table") then
		for key, property in choice:properties() do
			if property.read then
				out:setproperty(key, property.read)
			end
		end
	end
	return out
end

-- A table's fields, each optional: what `state:patch` takes.
type function Partial(t)
	if t:is("any") then
		return types.any
	end
	if not t:is("table") then
		return types.never
	end
	local out = types.newtable()
	for key, property in t:properties() do
		if property.read then
			out:setproperty(key, types.optional(property.read))
		end
	end
	return out
end

---------------------------------------------------------------------------
-- Values.

-- Theme tokens: a style names one as `"$surface.raised"` and follows the
-- theme (see docs/scripting.md#theming). These are the standard tokens,
-- which every theme defines; an app's own (`"$terminal.pane"`) are strings
-- too. The lists follow the engine's schema; `tests/types.rs` checks them.
export type ColorToken =
	"$background"
	| "$surface.sunken"
	| "$surface.base"
	| "$surface.raised"
	| "$surface.overlay"
	| "$text.primary"
	| "$text.secondary"
	| "$text.tertiary"
	| "$text.disabled"
	| "$border"
	| "$border.subtle"
	| "$border.strong"
	| "$accent"
	| "$accent.hover"
	| "$accent.text"
	| "$danger"
	| "$warning"
	| "$success"
	| "$info"
	| "$selection"
	| "$selection.item"
	| "$focus.ring"
	| "$hover"
	| "$pressed"
	| "$scrollbar.thumb"
	| "$scrollbar.thumb.active"
	| "$scrollbar.track"
	| "$glass.tint"
	| "$button.bg"
	| "$button.hover"
	| "$button.pressed"
	| "$input.bg"
	| "$menu.bg"
	| "$menu.hover"
	| "$popover.bg"
	| "$backdrop"
	| "$selection.inactive"
	| "$selection.text"
	| "$selection.text.inactive"
export type NumberToken =
	"$glass.blur"
	| "$font.size.xs"
	| "$font.size.sm"
	| "$font.size.md"
	| "$font.size.lg"
	| "$font.size.xl"
	| "$font.size.xxl"
	| "$font.line.xs"
	| "$font.line.sm"
	| "$font.line.md"
	| "$font.line.lg"
	| "$font.line.xl"
	| "$font.line.xxl"
	| "$font.weight.regular"
	| "$font.weight.medium"
	| "$font.weight.semibold"
	| "$font.weight.bold"
	| "$radius.sm"
	| "$radius.md"
	| "$radius.lg"
	| "$radius.xl"
	| "$radius.full"
	| "$space.1"
	| "$space.2"
	| "$space.3"
	| "$space.4"
	| "$space.5"
	| "$space.6"
	| "$space.7"
	| "$space.8"
	| "$control.height"
	| "$motion.duration.fast"
	| "$motion.duration.normal"
	| "$motion.duration.slow"
	| "$selection.radius"
export type FontToken = "$font.sans"
	| "$font.mono"
export type ShadowToken = "$shadow.sm"
	| "$shadow.md"
	| "$shadow.lg"
export type EaseToken = "$motion.ease.standard"
	| "$motion.ease.enter"
	| "$motion.ease.exit"
-- A token's name without the `$`, for `ui.theme.get`.
export type TokenName =
	"background"
	| "surface.sunken"
	| "surface.base"
	| "surface.raised"
	| "surface.overlay"
	| "text.primary"
	| "text.secondary"
	| "text.tertiary"
	| "text.disabled"
	| "border"
	| "border.subtle"
	| "border.strong"
	| "accent"
	| "accent.hover"
	| "accent.text"
	| "danger"
	| "warning"
	| "success"
	| "info"
	| "selection"
	| "selection.item"
	| "focus.ring"
	| "hover"
	| "pressed"
	| "scrollbar.thumb"
	| "scrollbar.thumb.active"
	| "scrollbar.track"
	| "glass.tint"
	| "glass.blur"
	| "font.sans"
	| "font.mono"
	| "font.size.xs"
	| "font.size.sm"
	| "font.size.md"
	| "font.size.lg"
	| "font.size.xl"
	| "font.size.xxl"
	| "font.line.xs"
	| "font.line.sm"
	| "font.line.md"
	| "font.line.lg"
	| "font.line.xl"
	| "font.line.xxl"
	| "font.weight.regular"
	| "font.weight.medium"
	| "font.weight.semibold"
	| "font.weight.bold"
	| "radius.sm"
	| "radius.md"
	| "radius.lg"
	| "radius.xl"
	| "radius.full"
	| "space.1"
	| "space.2"
	| "space.3"
	| "space.4"
	| "space.5"
	| "space.6"
	| "space.7"
	| "space.8"
	| "control.height"
	| "shadow.sm"
	| "shadow.md"
	| "shadow.lg"
	| "motion.duration.fast"
	| "motion.duration.normal"
	| "motion.duration.slow"
	| "motion.ease.standard"
	| "motion.ease.enter"
	| "motion.ease.exit"
	| "button.bg"
	| "button.hover"
	| "button.pressed"
	| "input.bg"
	| "menu.bg"
	| "menu.hover"
	| "popover.bg"
	| "backdrop"
	| "selection.inactive"
	| "selection.text"
	| "selection.text.inactive"
	| "selection.radius"

-- "#rgb", "#rrggbb", "#rrggbbaa", "rgb(…)", "rgba(…)", "hsl(…)", a few
-- names, 0xrrggbb, or a colour token ("$text.secondary"; `ColorToken`
-- lists the standard ones). Plain `string`: the new solver would carry a
-- union of token names through a script's own string code.
export type Color = string | number
-- A number, or a number token: a standard one (`"$radius.md"`, see
-- `NumberToken`) or an app's (`"$terminal.inset"`), hence any string.
export type Num = number | string
-- Points, or a string: "50%", "auto" (margins), "12px", "$space.3".
export type Length = number | string
-- Two numbers, `{ a, b }`, or one for both.
export type Pair = number | { number }
-- `{ tl, tr, br, bl }` (a list in that order, or keyed).
export type Corners = { number } | { tl: number?, tr: number?, br: number?, bl: number? }
-- `{ top, right, bottom, left }`, in CSS's order.
export type Sides = { number }
export type ShadowSpec = { x: number?, y: number?, blur: number?, spread: number?, color: Color? }
-- Or a shadow token (`"$shadow.md"`, an app's own too).
export type Shadow = string | ShadowSpec | { ShadowSpec } | false
-- Stops are colors (evenly spaced) or `{ offset, color }` pairs.
export type Gradient = { read [number]: Color | { Color } } | { angle: number?, stops: { read [number]: Color | { Color } } }

export type Direction = "row" | "row_reverse" | "column" | "column_reverse"
-- `items` and `self`.
export type Align = "start" | "center" | "end" | "baseline" | "stretch"
export type Justify = "start" | "center" | "end" | "between" | "around" | "evenly" | "stretch"
export type AlignContent = "start" | "center" | "end" | "stretch" | "between" | "around" | "evenly"
export type Overflow = "visible" | "hidden" | "clip"
export type Fit = "fill" | "contain" | "cover" | "none" | "scale_down"
export type TextAlign = "start" | "end" | "left" | "center" | "right"
-- "mono" (or "monospace"), "sans" ("sans-serif"), "serif", or a family name.
-- A family name, or a font token (`FontToken`).
export type Font = string
-- Cursor names, or CSS's spellings of them ("nwse-resize").
export type Cursor =
	"default"
	| "pointer"
	| "text"
	| "vertical_text"
	| "crosshair"
	| "move"
	| "grab"
	| "grabbing"
	| "not_allowed"
	| "wait"
	| "progress"
	| "help"
	| "context_menu"
	| "copy"
	| "alias"
	| "no_drop"
	| "all_scroll"
	| "col_resize"
	| "row_resize"
	| "ew_resize"
	| "ns_resize"
	| "nesw_resize"
	| "nwse_resize"
	| "zoom_in"
	| "zoom_out"
	| "vertical-text"
	| "not-allowed"
	| "context-menu"
	| "no-drop"
	| "all-scroll"
	| "col-resize"
	| "row-resize"
	| "ew-resize"
	| "ns-resize"
	| "nesw-resize"
	| "nwse-resize"
	| "zoom-in"
	| "zoom-out"

-- A key into a state's table, an index into its list (from 1), or a path
-- of them: `{ "tabs", 3, "title" }`.
export type StatePath = string | number | { string | number }

---------------------------------------------------------------------------
-- State.

-- Rust-owned state (`ui.state`, `ui.shared`, `ui.persist`, `ui.use_state`,
-- `ui.computed`). Values are read-only snapshots: `get` and `peek` return
-- tables that cannot be changed; change state with its methods. Each
-- shares what it does not touch, and a write of an equal value notifies
-- nobody. The type is the initial value's: give `nil` or `{}` a type,
-- `ui.state("selected", nil :: string?)`, `ui.state("tabs", {} :: { Tab })`.
export type State<T> = {
	-- The value; reading it in a computed, effect or function property
	-- subscribes that to it.
	get: (self: State<T>) -> T,
	-- The value, without subscribing.
	peek: (self: State<T>) -> T,
	set: (self: State<T>, value: T) -> (),
	-- `f` gets a mutable copy: change it and return nothing, or return the
	-- new value. It copies the whole value; prefer the others for large state.
	update: (self: State<T>, f: (value: T) -> ...T) -> (),
	-- One key or path: `set_key("k", v)`, `set_key({ "tabs", 3, "title" }, v)`;
	-- nil removes the key. The value's type depends on the path, which Luau
	-- cannot follow: it is not checked.
	set_key: (self: State<T>, path: StatePath, value: any) -> (),
	-- A shallow merge into the table.
	patch: (self: State<T>, fields: Partial<T>) -> (),
	-- Sets `not value` (a boolean).
	toggle: (self: State<T>) -> (),
	-- Adds to a number (1 by default).
	add: (self: State<T>, by: number?) -> (),
	-- Lists: append, insert at an index from 1, remove (returning the item,
	-- nil outside the list), and the length (tracked, as `get` is).
	push: (self: State<T>, item: Item<T>) -> (),
	insert: (self: State<T>, index: number, item: Item<T>) -> (),
	remove: (self: State<T>, index: number) -> Item<T>?,
	len: (self: State<T>) -> number,
	-- One item of a list (from 1), tracked as `get` is, without the list
	-- made into a table (a long list's rows, as a list shows them).
	at: (self: State<T>, index: number) -> Item<T>?,
	-- Where the first item whose `field` is `value` is (from 1), or nil:
	-- `messages:index_of("id", 42)`, found without making the list a table.
	index_of: (self: State<T>, field: string, value: any) -> number?,
	-- Strings: where `query` is in the string held (tracked, as `get` is),
	-- each match's first and last byte from 1 (as `string.find` gives them),
	-- at most `limit`, and whether there were more. Read where the string is
	-- held: a long document is not copied into the script. `case = false`
	-- folds ASCII letters' case, as `string.lower` does.
	find: (self: State<T>, query: string, options: { case: boolean?, limit: number? }?) -> ({ { number } }, boolean),
	-- How many times `query` is in the string held (`find`'s matches, at
	-- most `limit`, and whether there were more), with no list made.
	count: (self: State<T>, query: string, options: { case: boolean?, limit: number? }?) -> (number, boolean),
}
-- How `ui.sorted` orders a list: by a field's name or a function of an
-- item, nil keys last; equal keys keep the list's order.
-- A field's test in a table `filter`: equal to a value, or (a string)
-- holding a text, `case = false` folding ASCII letters' case.
export type FieldTest = string | number | boolean | { contains: string, case: boolean? }
export type SortedSpec<T> = {
	by: (string | ((item: T) -> any))?,
	descending: boolean?,
	-- A function keeping the items it is true for, or fields' tests read in
	-- Rust: `{ sector = "Tech", symbol = { contains = "AB" } }`.
	filter: ((item: T) -> boolean) | { [string]: FieldTest } | nil,
}
-- What a property bound to state reads: any state whose value is a `T`.
export type Readable<T> = { read get: (self: never) -> T }

---------------------------------------------------------------------------
-- Engine objects.

-- What `ui` element functions return; children and arguments of others.
declare extern type Node with
end
-- `ui.span { … }`: a run of rich text, used once.
declare extern type Span with
end
-- `ui.set`, `ui.toggle`, `ui.add`, `ui.action`: a handler Rust runs.
declare extern type Action with
end
-- `ui.tween`, `ui.spring`, `ui.loop`: a property eased in Rust.
declare extern type Binding with
end
-- Variants chosen from a `ui.variants` recipe: give it to an element as
-- `variants = …`.
declare extern type Variants with
end
declare extern type Bitmap with
	w: number
	h: number
end
declare extern type Shader with
end
declare extern type Brush with
end

export type FetchOptions = {
	-- "GET" unless set.
	method: string?,
	headers: { [string]: string }?,
	-- Text, or data sent as JSON (with its content type).
	body: unknown?,
}
-- What `ui.fetch` settles with (a promise's `value`).
export type Response = {
	status: number,
	-- 2xx.
	ok: boolean,
	-- Names in lower case.
	headers: { [string]: string },
	body: string,
	-- The body decoded, when it is JSON.
	json: unknown?,
}

-- A file the user picked (`ui.open_files`, `ui.save_file`): the only
-- files a script reads or writes.
export type File = { path: string, name: string }
export type FilePickerOptions = {
	title: string?,
	-- Where the dialog opens.
	directory: string?,
	-- The name a save dialog suggests.
	name: string?,
	-- `{ { "Images", { "png", "jpg" } } }`: extensions without the dot.
	filters: { { any } }?,
	multiple: boolean?,
}

-- A value to come (`ui.call`, `ui.fetch`, the file functions), of type
-- `T`. Reads of its fields are tracked: what reads them follows it as it
-- settles.
export type Promise<T = unknown> = {
	-- It settled, with a value or an error.
	done: boolean,
	value: T?,
	error: string?,
	-- `on_value(value)` or `on_error(message)` once it settles (at once if
	-- it has). Without `on_error` (or a read of `error`, or an `await` in a
	-- pcall), a rejection is reported as a script error.
	after: (self: Promise<T>, on_value: ((value: T) -> ())?, on_error: ((message: string) -> ())?) -> (),
	-- Inside `ui.task`: wait for it, and return its value (or raise its
	-- error, the host's message as it wrote it).
	await: (self: Promise<T>) -> T,
}

-- `ui.form`'s spec: each field's first value, its checks (a message, or
-- nil while it is fine; `values.other` reads another field), and what
-- submitting does (in a task: it may await, and raises what went wrong).
export type FormSpec = {
	fields: { [string]: any },
	validate: { [string]: (value: any, values: { [string]: any }) -> string? }?,
	submit: ((values: { [string]: any }) -> any)?,
	-- `on_success(result, values)`: what `submit` returned, and the values.
	on_success: ((result: any, values: { [string]: any }) -> ())?,
}
-- A form's field: give it to a control as `field`.
export type FormField = {
	name: string,
	value: State<any>,
	-- It has been left.
	touched: State<boolean>,
	-- What its check says now, as it is typed.
	problem: State<string?>,
	-- `problem` once it has been left or a submit tried: what to show.
	error: State<string?>,
	ref: Ref,
}
export type Form = {
	fields: { [string]: FormField },
	values: { [string]: State<any> },
	-- Every check passes.
	valid: State<boolean>,
	-- A submit has been tried.
	attempted: State<boolean>,
	-- A submit is under way.
	pending: State<boolean>,
	-- What the last submit raised, or nil.
	failure: State<string?>,
	-- Check every field (showing what is wrong, the first wrong focused);
	-- all fine, submit. An action, or call it.
	submit: () -> (),
	-- Back to the first values (or `values`), nothing shown wrong.
	reset: (values: { [string]: any }?) -> (),
	-- The values now, as a table.
	snapshot: () -> { [string]: any },
}

-- `ui.resource`'s data loaded in a task, and how its loading goes.
export type Resource<T> = {
	-- What the last load returned (`initial` until one has; nil without
	-- it), kept while it loads again. Write it as any state (an added row,
	-- an optimistic update); the next load replaces it.
	data: State<T>,
	-- A load is under way.
	loading: State<boolean>,
	-- What the last load raised, or nil.
	error: State<string?>,
	-- "loading" until a load has answered (and again while a failed one is
	-- tried again), then "ready" or "failed".
	status: State<"loading" | "ready" | "failed">,
	-- Load again (a Reload button, a retry): an action, or call it.
	reload: () -> (),
}

-- A mounted node a script reaches: through `on_mount`, `on_resize`, a ref
-- or an animation's function values.
declare extern type Element with
	id: string?
	-- Its kind, as selectors name it: "column", "button", "text"…
	tag: string
	mounted: boolean
	-- Its laid-out box, in points from the window's top left; nil while it
	-- is not in a scene.
	width: number?
	height: number?
	x: number?
	y: number?
	-- Keyboard focus to the element or its first focusable descendant
	-- (shown ones only); whether it took.
	function focus(self): boolean
	-- A list's row `i` (from 1) in view, placed by `align`: "start",
	-- "center", "end" or "nearest" (the default, the least scrolling). A
	-- list of measured rows measures those around it as it lands, so it
	-- ends where asked. False for elements that are not lists.
	function scroll_to_row(self, i: number, options: { align: ("start" | "center" | "end" | "nearest")? }?): boolean
	function has_class(self, class: string): boolean
	-- An input's selection, in characters (Unicode code points from 0, as
	-- `utf8.len` counts them). Each returns false (or nil) for elements
	-- that are not inputs. `b` defaults to `a`: a caret.
	function select_all(self): boolean
	function set_selection(self, a: number, b: number?): boolean
	-- The anchor and the focus (where the caret is).
	function selection(self): (number?, number?)
	-- The same in bytes from 0, as `string.find` counts them (its match `s,
	-- e` is `s - 1, e`): a code editor's search. A selection set this way
	-- scrolls into view, focused or not.
	function set_selection_bytes(self, a: number, b: number?): boolean
	function selection_bytes(self): (number?, number?)
	-- The selection replaced by `text`, as typing it would (one step of
	-- undo): a search's Replace.
	function replace_selection(self, text: string): boolean
	-- Clear keyboard focus if it is on the element or inside it.
	function blur(self): ()
end

-- `ref = r` on a node. Reading it in an effect or computed follows it: the
-- element while one is mounted, nil when none is.
declare extern type Ref with
	element: Element?
	mounted: boolean
	-- How many mounted nodes share it.
	count: number
	-- Its element's laid-out size, followed: a computed reading it runs
	-- again when layout resizes the element (a container query).
	width: number?
	height: number?
	function get(self): Element?
	-- Without following.
	function peek(self): Element?
	-- Every mounted node sharing it, in document order.
	function elements(self): { Element }
end

export type ToastOptions = {
	title: string,
	body: string?,
	kind: ("info" | "success" | "warning" | "error")?,
	duration: number?,
	-- A button on it: "Undo"; the toast goes after `run`.
	action: { label: string, run: () -> () }?,
}

export type NumberOptions = {
	-- Digits after the point (default: none for whole numbers, else 2).
	decimals: number?,
	style: ("decimal" | "percent")?,
	notation: ("standard" | "compact")?,
	sign: ("auto" | "always" | "except_zero" | "never")?,
	-- Thousands separators (default true).
	grouping: boolean?,
	-- A symbol before the number, after its sign: `"$"`.
	currency: string?,
}

-- `ui.path()`: a path for a canvas, built by chaining.
declare extern type Path with
	function move(self, x: number, y: number): Path
	function line(self, x: number, y: number): Path
	function quad(self, cx: number, cy: number, x: number, y: number): Path
	function cubic(self, c1x: number, c1y: number, c2x: number, c2y: number, x: number, y: number): Path
	-- Angles in degrees, clockwise from +x.
	function arc(self, cx: number, cy: number, r: number, start: number, sweep: number): Path
	function close(self): Path
	-- A rectangle (corners rounded by `r`) or the ellipse in a box, as a
	-- closed subpath. A path that is just one of them (or a circle, or a
	-- line from `canvas:line`) is drawn as that shape, not rasterized.
	function rect(self, x: number, y: number, w: number, h: number, r: number?): Path
	function ellipse(self, x: number, y: number, w: number, h: number): Path
end
export type Paint = Color | Brush
export type LineCap = "butt" | "round" | "square"
export type LineJoin = "miter" | "round" | "bevel"
export type LineOptions = { cap: LineCap?, join: LineJoin?, dash: { number }?, dash_offset: number?, [number]: never }
-- `draw` strokes a fraction of the path (`0.5`) or a span of it
-- (`{ start, end }`), as anime.js draws SVG lines.
export type MarkdownOptions = {
	-- Names a text that grows (a streaming reply): parsed incrementally.
	key: string?,
	-- Single newlines break lines, as people's messages show them.
	breaks: boolean?,
	-- Repository paths (`src/main.rs:12`) are files: runs with `f`.
	paths: boolean?,
}
export type MarkdownBlock = { t: string, [string]: any }

export type StrokeOptions = {
	cap: LineCap?,
	join: LineJoin?,
	dash: { number }?,
	dash_offset: number?,
	[number]: never,
	draw: Pair?,
}
-- What a canvas's `draw(c, w, h)` paints with.
declare extern type Canvas with
	function rect(self, x: number, y: number, w: number, h: number, paint: Paint, radius: number?): ()
	function circle(self, cx: number, cy: number, r: number, paint: Paint): ()
	function line(self, x1: number, y1: number, x2: number, y2: number, paint: Paint, width: number?, opts: LineOptions?): ()
	function fill(self, path: Path, paint: Paint, rule: ("nonzero" | "evenodd")?): ()
	function stroke(self, path: Path, paint: Paint, width: number?, opts: StrokeOptions?): ()
	-- Clips what follows to the path, until `restore`.
	function clip(self, path: Path): ()
	function restore(self): ()
end

-- `ui.web(opts)`: a web page's controller. Its fields are tracked, so a
-- text or computed reading `web.title` follows the page.
declare extern type Web with
	url: string
	title: string
	loading: boolean
	progress: number
	can_go_back: boolean
	can_go_forward: boolean
	focused: boolean
	error: string?
	-- The pointer over the page, in points from its top left; nil outside.
	-- (Over a page zgx hears no pointer moves: its nodes had their leave.)
	pointer: { x: number, y: number }?
	function load(self, url: string): ()
	function html(self, html: string, base_url: string?): ()
	function back(self): ()
	function forward(self): ()
	function reload(self): ()
	function stop(self): ()
	function focus(self): ()
	function blur(self): ()
	function devtools(self): ()
	function zoom(self, zoom: number): ()
	function run(self, js: string): ()
	-- The script's value as Luau data (JSON's shape, whatever the page
	-- returns: `any`), or nil and the error.
	function eval(self, js: string, reply: ((value: any, err: string?) -> ...unknown)?): ()
	-- A page from `ui.web(id, opts)` leaves the host's registry: the next
	-- `ui.web(id)` makes a new one.
	function close(self): ()
end
export type WebOptions = {
	transparent: boolean?,
	-- "default" (shared cookies and storage), "ephemeral", or a name.
	profile: string?,
	inspectable: boolean?,
	user_agent: string?,
	-- `window.zgx.postMessage(value)` from the page reaches on_message.
	messages: boolean?,
	new_windows: boolean?,
	[number]: never,
}
export type WebEvent = { kind: "started" | "committed" | "finished" | "failed" | "changed", url: string }

-- `ui.term(id, opts)`: a terminal session (a Ghostty surface with the
-- desktop host's `ghostty` feature). Sessions outlive reloads: the run that
-- asks for `id` again gets the same one. Its fields are tracked.
export type TermStatus = {
	state: "idle" | "working" | "done" | "blocked" | "error",
	-- With "blocked": what it waits for.
	kind: ("permission" | "question" | "auth")?,
	-- 0 to 100 while working or blocked; nil: indeterminate.
	progress: number?,
	app: string?,
	title: string?,
	msg: string?,
	-- A worker under the program is blocked, whatever its own state.
	child_blocked: boolean,
	since: number,
}

declare extern type Term with
	id: string
	-- What the running program set (OSC 0/2); "" until it does.
	title: string
	-- The shell's working directory (OSC 7); "" until reported.
	pwd: string
	-- The foreground process ("zsh", "nvim", "claude").
	process: string
	pid: number
	focused: boolean
	-- The bell rang since the terminal last had focus.
	bell: boolean
	-- A program's progress (OSC 9;4): 0 to 1, negative while indeterminate.
	progress: number?
	-- The progress report is in its error state (OSC 9;4;2).
	progress_error: boolean
	-- What the program reports it is doing (the Program Status Protocol,
	-- OSC 7501), nil until it reports. `since`: Unix seconds the state began.
	status: TermStatus?
	-- The exit code, once its process ended.
	exited: number?
	columns: number
	rows: number
	-- The pointer over it, in points from its top left; nil outside.
	pointer: { x: number, y: number }?
	function focus(self): ()
	function blur(self): ()
	-- Text as if typed ("ls\r" runs ls).
	function input(self, text: string): ()
	function paste(self, text: string): ()
	-- A Ghostty binding action: "copy_to_clipboard", "clear_screen", ...
	function binding(self, action: string): ()
	-- Whether closing would end something other than an idle shell.
	function needs_confirm(self): boolean
	-- End the session (its process is hung up).
	function close(self): ()
end
export type TermOptions = {
	-- A command line; nil runs the login shell.
	command: string?,
	-- Where it starts ("~" is home); nil is home.
	cwd: string?,
	env: { [string]: string }?,
	font_size: number?,
	-- Handlers (the latest run's are called).
	on_title: ((title: string) -> ...unknown)?,
	on_pwd: ((pwd: string) -> ...unknown)?,
	on_process: ((name: string) -> ...unknown)?,
	on_exit: ((code: number) -> ...unknown)?,
	-- The terminal asks to close: its process exited (alive = false), or
	-- a close binding ran with something still running.
	on_close: ((alive: boolean) -> ...unknown)?,
	on_bell: (() -> ...unknown)?,
	-- A shell-integrated command finished.
	on_command: ((exit_code: number?, duration_ms: number) -> ...unknown)?,
	on_notify: ((title: string, body: string) -> ...unknown)?,
	[number]: never,
}

-- `ui.command(opts)`: an app command, shown as a menu item; its key
-- reaches it wherever focus is (terminals and web pages included). `id`
-- and `run` are required unless `separator` is set (Luau checks table
-- unions poorly, so this is one table).
export type CommandOptions = {
	id: string?,
	-- Defaults to the id.
	title: string?,
	-- "File" (the default), "Edit", "View", "Window" or any other menu.
	menu: string?,
	-- Modifiers and a key joined by "+": "cmd+shift+d", "cmd+alt+arrowleft".
	key: string?,
	run: (() -> ...unknown)?,
	-- A separator line in `menu` instead of a command.
	separator: boolean?,
	[number]: never,
}

-- `ui.window { id, open = state, node }`: a window of the app's own (a
-- settings window), open while `open` is true; closing it sets `open` false.
-- Its node is its content. It shares the app's state.
export type WindowOptions = {
	id: string,
	-- Defaults to the id.
	title: string?,
	-- Points; 480 by 360 by default.
	width: number?,
	height: number?,
	open: State<boolean>,
	-- Its frame saved under this name as it moves and resizes, and reopened
	-- where it was left (macOS).
	frame_autosave: string?,
	-- AppKit's window tabs (macOS): as the user's setting says, always, or
	-- never (a window with tabs of its own).
	tabbing: ("automatic" | "preferred" | "disallowed")?,
	[number]: Node,
}

-- A rect in points, from the window's top left.
export type WindowRect = { x: number, y: number, width: number, height: number }

-- `ui.window`: called, it declares a window (`WindowOptions`). Its fields
-- are the app's own window's title bar, as the web's Window Controls
-- Overlay gives it: `titlebar_area`, where the app may draw in the strip an
-- inset or hidden title bar leaves (nil for a standard title bar, in
-- fullscreen, and where the platform does not say), and `controls`, the
-- window's own buttons in it (nil where there are none). Both follow
-- resizes, scale and fullscreen.
export type WindowApi = ((opts: WindowOptions) -> ()) & {
	titlebar_area: State<WindowRect?>,
	controls: State<WindowRect?>,
}

---------------------------------------------------------------------------
-- Properties.

-- A property's value: a literal, a state, a function of state (rerun when
-- what it reads changes) or an animation binding.
export type Reactive<T> = T | Readable<T> | (() -> T) | Binding
-- What a computed text function may return.
export type TextValue = (string | number | boolean)?
-- Text: a string (`"{name}"` shows the state named `name`), a number, or
-- a function of state.
export type Text = string | number | (() -> TextValue)
-- Children: nodes, strings (text), functions (computed text) and lists of
-- them, which flatten; `false` and `nil` are skipped.
export type Child = Node | string | (() -> TextValue) | { read [number]: Child } | false | nil

-- Event handlers: a function, or an action Rust runs.
export type Handler = () -> ...unknown
-- `clicks` (on_press): the press's count, 2 for a double click's second.
-- `shift`, `ctrl`, `alt`, `meta`: the keys held; `cmd`: the platform's
-- command key (Command on macOS, Ctrl elsewhere), as `cmd` in keys.
export type PointerEvent = {
	x: number,
	y: number,
	local_x: number,
	local_y: number,
	clicks: number?,
	shift: boolean,
	ctrl: boolean,
	alt: boolean,
	meta: boolean,
	cmd: boolean,
}
-- What `on_click` is told: the keys held as it was clicked (a ⌘-click).
export type ClickEvent = { shift: boolean, ctrl: boolean, alt: boolean, meta: boolean, cmd: boolean }
-- A node's click: told the keys held, which it may ignore.
export type ClickHandler = (e: ClickEvent) -> ...unknown
-- `code`: the physical key, as the DOM's `KeyboardEvent.code` (`"KeyA"`,
-- `"Digit1"`, `"BracketLeft"`), whatever the layout and modifiers typed;
-- nil when the platform doesn't say.
export type KeyEvent = {
	key: string,
	code: string?,
	-- A text field holds focus: a bare-key shortcut gives way to typing.
	editing: boolean,
	shift: boolean,
	ctrl: boolean,
	alt: boolean,
	meta: boolean,
	-- Command on macOS, Ctrl elsewhere: the key of shortcuts ("cmd+z").
	cmd: boolean,
	["repeat"]: boolean,
}
-- Points scrolled (`dy` positive: content moves up), where the pointer is,
-- and the keys held: ⌘ or Ctrl with the wheel zooms, by convention.
export type WheelEvent = {
	x: number,
	y: number,
	local_x: number,
	local_y: number,
	clicks: number?,
	shift: boolean,
	ctrl: boolean,
	alt: boolean,
	meta: boolean,
	cmd: boolean,
	dx: number,
	dy: number,
}
-- A trackpad pinch: the fingers' spread changed by `scale` (0.05: 5%
-- larger, zoom in; negative, out), at the pointer.
export type PinchEvent = {
	x: number,
	y: number,
	local_x: number,
	local_y: number,
	clicks: number?,
	shift: boolean,
	ctrl: boolean,
	alt: boolean,
	meta: boolean,
	cmd: boolean,
	scale: number,
}
-- What a drag carries to other apps (`drag_export`): text, a link, files
-- (absolute paths).
export type DragExport = { text: string?, url: string?, files: { string }? }
export type Size = { w: number, h: number }
export type DragPoint = { x: number, y: number, local_x: number, local_y: number, w: number, h: number }

-- Plain styles, for `hover`, `active`, `focus`, `disabled` and
-- `context_menu_style`.
export type Styles = {
	-- Size and position.
	w: Length?,
	h: Length?,
	size: Pair?,
	min_w: Length?,
	max_w: Length?,
	min_h: Length?,
	max_h: Length?,
	w_full: boolean?,
	h_full: boolean?,
	aspect: number?,
	absolute: boolean?,
	relative: boolean?,
	top: Length?,
	left: Length?,
	-- An absolute node's inset from the start or end of a line: `left` and
	-- `right` left to right, `right` and `left` right to left.
	inset_start: Length?,
	inset_end: Length?,
	right: Length?,
	bottom: Length?,
	-- Spacing.
	p: Length?,
	px: Length?,
	py: Length?,
	pt: Length?,
	pr: Length?,
	pb: Length?,
	pl: Length?,
	-- Padding at the start or end of a line (CSS's `padding-inline-start`,
	-- `-end`), following `dir`; `pl` and `pr` stay left and right.
	ps: Length?,
	pe: Length?,
	m: Length?,
	mx: Length?,
	my: Length?,
	mt: Length?,
	mr: Length?,
	mb: Length?,
	ml: Length?,
	-- Margins at the start or end of a line, following `dir`.
	ms: Length?,
	me: Length?,
	gap: Num?,
	gap_x: Length?,
	gap_y: Length?,
	-- Flex and grid.
	direction: Direction?,
	-- Which way lines run in it and what it holds (HTML's `dir`): "rtl"
	-- for Arabic, Hebrew, Persian or Urdu runs rows from the right, starts
	-- columns and text at the right, and turns `ps`, `ms`, `inset_start` round.
	dir: ("ltr" | "rtl")?,
	grow: number?,
	shrink: number?,
	basis: Length?,
	flex_wrap: boolean?,
	wrap_reverse: boolean?,
	items: Align?,
	justify: Justify?,
	self: Align?,
	content: AlignContent?,
	grid: boolean?,
	cols: number?,
	rows: number?,
	col_start: number?,
	row_start: number?,
	col_span: number?,
	row_span: number?,
	col_span_full: boolean?,
	row_span_full: boolean?,
	overflow: Overflow?,
	overflow_x: Overflow?,
	overflow_y: Overflow?,
	hidden: boolean?,
	invisible: boolean?,
	-- Paint.
	bg: Color?,
	gradient: Gradient?,
	color: Color?,
	bg_image: string?,
	bg_fit: Fit?,
	rounded: Num?,
	corners: Corners?,
	rounded_tl: number?,
	rounded_tr: number?,
	rounded_br: number?,
	rounded_bl: number?,
	border: Num?,
	border_sides: Sides?,
	border_color: Color?,
	border_over: boolean?,
	border_dash: Pair?,
	-- An outline around the box (CSS's): its width, colour, and how far
	-- outside it starts (negative: over the box's edge, an inset ring).
	outline: Num?,
	outline_color: Color?,
	outline_offset: Num?,
	shadow: Shadow?,
	-- Shadows inside the box (CSS's inset): a ring as wide on each side as
	-- the offset pushes it, plus the spread; drawn sharp (no blur).
	inset_shadow: Shadow?,
	opacity: Num?,
	brightness: number?,
	contrast: number?,
	saturate: number?,
	blur: number?,
	backdrop_blur: Num?,
	fade: Pair?,
	fade_x: Pair?,
	fade_sides: Sides?,
	-- Fade an edge only as far as content runs past it (a list at its top shows no top band).
	fade_overflow: boolean?,
	-- Lay its edges on whole points (crisp borders where percentages leave fractions).
	snap: boolean?,
	clip_inset: Sides?,
	layer: boolean?,
	fit: Fit?,
	tint: Color?,
	cursor: Cursor?,
	-- Text.
	text_size: Num?,
	weight: Num?,
	bold: boolean?,
	italic: boolean?,
	align: TextAlign?,
	font: Font?,
	wrap: boolean?,
	truncate: boolean?,
	lines: number?,
	line_height: Num?,
	letter_spacing: Num?,
	-- Tabular figures: every digit the same width.
	tabular: boolean?,
	-- Underlined text (inherited, as CSS's `text-decoration`), and how far
	-- below the baseline the line sits.
	underline: boolean?,
	underline_offset: Num?,
	-- Transforms: after layout, about `origin` (fractions of the node).
	x: number?,
	y: number?,
	scale: number?,
	scale_x: number?,
	scale_y: number?,
	rotate: number?,
	skew: number?,
	skew_x: number?,
	skew_y: number?,
	origin: Pair?,
	[number]: never,
}

-- Style properties as elements take them: each may also be a state, a
-- function or a binding.
export type StyleProps = {
	-- Size and position.
	w: Reactive<Length>?,
	h: Reactive<Length>?,
	size: Reactive<Pair>?,
	min_w: Reactive<Length>?,
	max_w: Reactive<Length>?,
	min_h: Reactive<Length>?,
	max_h: Reactive<Length>?,
	w_full: Reactive<boolean>?,
	h_full: Reactive<boolean>?,
	aspect: Reactive<number>?,
	absolute: Reactive<boolean>?,
	relative: Reactive<boolean>?,
	top: Reactive<Length>?,
	left: Reactive<Length>?,
	inset_start: Reactive<Length>?,
	inset_end: Reactive<Length>?,
	right: Reactive<Length>?,
	bottom: Reactive<Length>?,
	-- Spacing.
	p: Reactive<Length>?,
	px: Reactive<Length>?,
	py: Reactive<Length>?,
	pt: Reactive<Length>?,
	pr: Reactive<Length>?,
	pb: Reactive<Length>?,
	pl: Reactive<Length>?,
	ps: Reactive<Length>?,
	pe: Reactive<Length>?,
	m: Reactive<Length>?,
	mx: Reactive<Length>?,
	my: Reactive<Length>?,
	mt: Reactive<Length>?,
	mr: Reactive<Length>?,
	mb: Reactive<Length>?,
	ml: Reactive<Length>?,
	ms: Reactive<Length>?,
	me: Reactive<Length>?,
	gap: Reactive<Num>?,
	gap_x: Reactive<Length>?,
	gap_y: Reactive<Length>?,
	-- Flex and grid.
	direction: Reactive<Direction>?,
	dir: Reactive<"ltr" | "rtl">?,
	grow: Reactive<number>?,
	shrink: Reactive<number>?,
	basis: Reactive<Length>?,
	flex_wrap: Reactive<boolean>?,
	wrap_reverse: Reactive<boolean>?,
	items: Reactive<Align>?,
	justify: Reactive<Justify>?,
	self: Reactive<Align>?,
	content: Reactive<AlignContent>?,
	grid: Reactive<boolean>?,
	cols: Reactive<number>?,
	rows: Reactive<number>?,
	col_start: Reactive<number>?,
	row_start: Reactive<number>?,
	col_span: Reactive<number>?,
	row_span: Reactive<number>?,
	col_span_full: Reactive<boolean>?,
	row_span_full: Reactive<boolean>?,
	overflow: Reactive<Overflow>?,
	overflow_x: Reactive<Overflow>?,
	overflow_y: Reactive<Overflow>?,
	hidden: Reactive<boolean>?,
	invisible: Reactive<boolean>?,
	-- Paint.
	bg: Reactive<Color>?,
	gradient: Reactive<Gradient>?,
	color: Reactive<Color>?,
	bg_image: Reactive<string>?,
	bg_fit: Reactive<Fit>?,
	rounded: Reactive<Num>?,
	corners: Reactive<Corners>?,
	rounded_tl: Reactive<number>?,
	rounded_tr: Reactive<number>?,
	rounded_br: Reactive<number>?,
	rounded_bl: Reactive<number>?,
	border: Reactive<Num>?,
	border_sides: Reactive<Sides>?,
	border_color: Reactive<Color>?,
	border_over: Reactive<boolean>?,
	-- A function may return nil: a solid border.
	border_dash: Reactive<Pair?>?,
	outline: Reactive<Num>?,
	outline_color: Reactive<Color>?,
	outline_offset: Reactive<Num>?,
	shadow: Reactive<Shadow>?,
	inset_shadow: Reactive<Shadow>?,
	opacity: Reactive<Num>?,
	brightness: Reactive<number>?,
	contrast: Reactive<number>?,
	saturate: Reactive<number>?,
	blur: Reactive<number>?,
	backdrop_blur: Reactive<Num>?,
	fade: Reactive<Pair>?,
	fade_x: Reactive<Pair>?,
	fade_sides: Reactive<Sides>?,
	fade_overflow: Reactive<boolean>?,
	snap: Reactive<boolean>?,
	clip_inset: Reactive<Sides>?,
	layer: Reactive<boolean>?,
	fit: Reactive<Fit>?,
	tint: Reactive<Color>?,
	cursor: Reactive<Cursor>?,
	-- Text.
	text_size: Reactive<Num>?,
	weight: Reactive<Num>?,
	bold: Reactive<boolean>?,
	italic: Reactive<boolean>?,
	align: Reactive<TextAlign>?,
	font: Reactive<Font>?,
	wrap: Reactive<boolean>?,
	truncate: Reactive<boolean>?,
	lines: Reactive<number>?,
	line_height: Reactive<Num>?,
	letter_spacing: Reactive<Num>?,
	-- Tabular figures: every digit the same width.
	tabular: Reactive<boolean>?,
	-- Underlined text (inherited, as CSS's `text-decoration`), and how far
	-- below the baseline the line sits.
	underline: Reactive<boolean>?,
	underline_offset: Reactive<Num>?,
	-- Transforms: after layout, about `origin` (fractions of the node).
	x: Reactive<number>?,
	y: Reactive<number>?,
	scale: Reactive<number>?,
	scale_x: Reactive<number>?,
	scale_y: Reactive<number>?,
	rotate: Reactive<number>?,
	skew: Reactive<number>?,
	skew_x: Reactive<number>?,
	skew_y: Reactive<number>?,
	origin: Reactive<Pair>?,
}

-- What every element takes besides its styles.
export type PointerEvents = "none" | "auto" | boolean
export type AriaRole =
	"group"
	| "button"
	| "link"
	| "heading"
	| "list"
	| "listitem"
	| "listbox"
	| "option"
	| "toolbar"
	| "status"
	| "tablist"
	| "tab"
	| "dialog"
	| "img"
	| "tabpanel"
	| "radiogroup"
	| "radio"
	| "switch"
	| "checkbox"
	| "spinbutton"
	| "combobox"
	| "tree"
	| "treeitem"
	| "grid"
	| "row"
	| "gridcell"
	| "columnheader"
	| "alert"
	| "menu"
	| "menuitem"
export type NodeProps = {
	-- The subtree's theme: a theme's name (`"zgx-light"`), or tokens to
	-- override (`{ accent = "#e5484d" }`, with `theme = name` to start from).
	theme: Reactive<ThemeSpec>?,
	-- Identity, for selectors, refs and animations.
	id: string?,
	-- Space-separated class names.
	class: string?,
	-- Styles chosen from a `ui.variants` definition: `button { tone =
	-- "ghost" }`, or the recipe itself for its defaults. The element's own
	-- properties win over them.
	variants: (Variants | ((choice: any) -> Variants))?,
	ref: (Ref | { Ref })?,
	focusable: boolean?,
	-- What assistive technology takes it for (ARIA's `role`), instead of
	-- what it is: a clickable row as an "option", a row of buttons as a
	-- "toolbar". It keeps what it does.
	role: AriaRole?,
	-- A section title of this level, 1 to 6 (`h1`…`h6`): screen readers
	-- list headings and move between them.
	heading: number?,
	-- Whether this option, tab or item is the chosen one (`aria-selected`).
	selected: (boolean | Readable<boolean> | (() -> boolean))?,
	-- Whether a switch, checkbox or radio of the app's own is on
	-- (`aria-checked`), and whether what a control opens shows
	-- (`aria-expanded`): a custom disclosure's.
	checked: (boolean | Readable<boolean> | (() -> boolean))?,
	expanded: (boolean | Readable<boolean> | (() -> boolean))?,
	-- Announce changes to the text in it (`aria-live`): a count, a "Sent"
	-- note, a message arriving.
	live: ("polite" | "assertive" | "off")?,
	-- Read after its name, as a hint (`aria-description`).
	description: string?,
	-- What is wrong with its value (`aria-invalid`), followed: nil while it
	-- is fine. Assistive technology hears it with the field.
	error: (string | Readable<string?> | (() -> string?))?,
	-- Left out of the accessibility tree with what it holds (`aria-hidden`):
	-- decoration, or what is said elsewhere.
	aria_hidden: boolean?,
	-- One Tab stop for the focusable things inside; the arrows along this
	-- axis (and Home, End) move among them, as ARIA's composite widgets
	-- behave. `role = "listbox"` is vertical, "toolbar" and "tablist"
	-- horizontal, without asking.
	focus_group: ("vertical" | "horizontal" | "both")?,
	-- A press on it (or on what inside takes no input) moves the window; a
	-- double click zooms it. Buttons and inputs inside keep their clicks.
	-- CSS pointer-events: "none" (or false) lets the pointer through to what
	-- is under the node (presses, hover, the wheel, drops, a native view);
	-- descendants with handlers still take it. A state or function follows.
	pointer_events: (PointerEvents | Readable<PointerEvents> | (() -> PointerEvents))?,
	window_drag: boolean?,
	-- The pointer selects the text in it and everything it holds, as on a
	-- web page (CSS `user-select: text`): drag, double/triple press, Shift,
	-- ⌘A / Ctrl+A, ⌘C / Ctrl+C. `false` keeps a part out. Off unless set.
	selectable: boolean?,
	-- A selectable node's highlight and selected-text colours (CSS
	-- `::selection`), over the theme's `$selection` and `$selection.text`
	-- tokens; a state or function follows.
	selection_color: Reactive<Color>?,
	selected_text_color: Reactive<Color>?,
	-- Shown over it while the pointer rests on it (after 300 ms; at once
	-- within a second of another): text in the theme's look, or a node of
	-- the app's own. Above it, centred, unless `tooltip_placement` says.
	tooltip: (string | Node)?,
	tooltip_placement: Placement?,
	-- A hover card the pointer can move into and click in: it stays while
	-- the pointer is over the node or the card.
	tooltip_interactive: boolean?,
	-- Milliseconds the pointer rests before it shows (300; once one
	-- shows, the next shows at once). Keyboard focus shows it too.
	tooltip_delay: number?,
	-- A condition (true disables), or the styles it shows while disabled.
	disabled: (boolean | Readable<boolean> | (() -> boolean) | Styles)?,
	hover: Styles?,
	active: Styles?,
	focus: Styles?,
	-- Size animation ("height", "width", "size"; `true` is "height") or
	-- position animation ("position"). A `layout_id` node mounting as
	-- another with the same id unmounts (or while it is mounted) moves and
	-- scales from that one's box.
	layout: ("height" | "width" | "size" | "position" | boolean)?,
	layout_id: string?,
	layout_ease: Easing?,
	-- Milliseconds.
	layout_duration: number?,
	layout_enter: boolean?,
	layout_clip: boolean?,
	-- The content shows through the bitmap's alpha: `mask = bitmap`, or
	-- `{ bitmap, w = …, h = …, x = …, y = …, sweep = … }`.
	mask: (Bitmap | MaskOptions)?,
	bg_shader: Shader?,
	-- Up to four vec4s: lists of numbers, or colors (0 to 1).
	shader_params: { read [number]: Color | { number } }?,
	-- Events.
	on_click: (ClickHandler | Handler | Action)?,
	-- Primary pointer down and up.
	on_press: (((e: PointerEvent) -> ...unknown) | Action)?,
	on_release: (((e: PointerEvent) -> ...unknown) | Action)?,
	on_hover: (((inside: boolean) -> ...unknown) | Action)?,
	on_pointer_move: (((e: PointerEvent) -> ...unknown) | Action)?,
	-- Return true to consume the key.
	on_key: (((e: KeyEvent) -> ...boolean?) | Action)?,
	on_key_up: (((key: string) -> ...unknown) | Action)?,
	on_focus: (Handler | Action)?,
	on_blur: (Handler | Action)?,
	-- Return true to take the gesture from the scroll views around it.
	on_wheel: (((e: WheelEvent) -> ...boolean?) | Action)?,
	on_pinch: (((e: PinchEvent) -> ...boolean?) | Action)?,
	on_mount: ((element: Element) -> ...unknown)?,
	-- When layout gives it a new size, and at its first layout.
	on_resize: ((size: Size, element: Element) -> ...unknown)?,
	-- When any of it comes into view or all of it leaves (inside every
	-- clip and scroll viewport above it), and at its first layout: for
	-- building what only matters on screen.
	on_visible: ((visible: boolean, element: Element) -> ...unknown)?,
	-- Drag and drop within the window. Payloads are what `on_drag`
	-- returns: the script's own plain values, so they are `any` here.
	-- A source: `on_drag` returns the payload once the pointer moves 4
	-- points; `drag_preview` builds what follows the pointer.
	on_drag: (() -> any)?,
	drag_preview: ((payload: any) -> Node)?,
	on_drag_end: ((payload: any, accepted: boolean) -> ...unknown)?,
	-- What the drag carries to other apps as it leaves the window (macOS):
	-- Finder takes files, Mail text, a browser links.
	drag_export: ((payload: any) -> DragExport?)?,
	-- Released outside every window of the app (macOS): a tab torn off its
	-- window. `grab_x`/`grab_y` are where the source was held.
	on_drag_outside: ((payload: any, e: { x: number, y: number, grab_x: number, grab_y: number }) -> ...unknown)?,
	-- The drag moves its window instead, with no preview (macOS): a
	-- window's only tab. Dropped on another window's destination it drops
	-- there; elsewhere the window stays where it was let go.
	drag_moves_window: ((payload: any) -> boolean)?,
	-- A destination: `drop_when` filters payloads (default: all).
	drop_when: ((payload: any) -> unknown)?,
	on_drag_over: ((payload: any, e: DragPoint) -> ...unknown)?,
	on_drag_leave: ((payload: any) -> ...unknown)?,
	on_drop: ((payload: any, e: DragPoint) -> ...unknown)?,
	-- A secondary click (control-click on macOS) shows the returned
	-- `menu_item`s, `submenu`s and separators as a menu at the pointer;
	-- returning nil (or nothing) shows none.
	context_menu: ((e: PointerEvent) -> ...{ Node }?)?,
	-- The context menu panel's look: bg, rounded, p, gap, backdrop_blur,
	-- border, shadow, w, …
	context_menu_style: Styles?,
	-- Open the context menu under the node, from its left edge (a tab's),
	-- rather than at the pointer.
	context_menu_below: boolean?,
}
export type MaskOptions = { [number]: Bitmap, w: number?, h: number?, x: number?, y: number?, sweep: MaskSweep? }
-- The mask moving `x`, `y` points from where it starts, over and over every
-- `duration` ms, at `frame_rate` steps a second at most (60): the engine
-- moves it on its own frame clock, with no script, style pass or layout
-- per step, compositing only the masked node again (a text shimmer).
export type MaskSweep = { x: number?, y: number?, duration: number?, frame_rate: number? }

-- Every element's properties, without children: an unknown key is an
-- error (`[number]: never` makes Luau check them).
export type Props = {
	-- Size and position.
	w: Props_w,
	h: Props_h,
	size: Props_size,
	min_w: Props_min_w,
	max_w: Props_max_w,
	min_h: Props_min_h,
	max_h: Props_max_h,
	w_full: Props_w_full,
	h_full: Props_h_full,
	aspect: Props_aspect,
	absolute: Props_absolute,
	relative: Props_relative,
	top: Props_top,
	left: Props_left,
	inset_start: Props_inset_start,
	inset_end: Props_inset_end,
	right: Props_right,
	bottom: Props_bottom,
	-- Spacing.
	p: Props_p,
	px: Props_px,
	py: Props_py,
	pt: Props_pt,
	pr: Props_pr,
	pb: Props_pb,
	pl: Props_pl,
	ps: Props_ps,
	pe: Props_pe,
	m: Props_m,
	mx: Props_mx,
	my: Props_my,
	mt: Props_mt,
	mr: Props_mr,
	mb: Props_mb,
	ml: Props_ml,
	ms: Props_ms,
	me: Props_me,
	gap: Props_gap,
	gap_x: Props_gap_x,
	gap_y: Props_gap_y,
	-- Flex and grid.
	direction: Props_direction,
	dir: Props_dir,
	grow: Props_grow,
	shrink: Props_shrink,
	basis: Props_basis,
	flex_wrap: Props_flex_wrap,
	wrap_reverse: Props_wrap_reverse,
	items: Props_items,
	justify: Props_justify,
	self: Props_self,
	content: Props_content,
	grid: Props_grid,
	cols: Props_cols,
	rows: Props_rows,
	col_start: Props_col_start,
	row_start: Props_row_start,
	col_span: Props_col_span,
	row_span: Props_row_span,
	col_span_full: Props_col_span_full,
	row_span_full: Props_row_span_full,
	overflow: Props_overflow,
	overflow_x: Props_overflow_x,
	overflow_y: Props_overflow_y,
	hidden: Props_hidden,
	invisible: Props_invisible,
	-- Paint.
	bg: Props_bg,
	gradient: Props_gradient,
	color: Props_color,
	bg_image: Props_bg_image,
	bg_fit: Props_bg_fit,
	rounded: Props_rounded,
	corners: Props_corners,
	rounded_tl: Props_rounded_tl,
	rounded_tr: Props_rounded_tr,
	rounded_br: Props_rounded_br,
	rounded_bl: Props_rounded_bl,
	border: Props_border,
	border_sides: Props_border_sides,
	border_color: Props_border_color,
	border_over: Props_border_over,
	-- A function may return nil: a solid border.
	border_dash: Props_border_dash,
	outline: Props_outline,
	outline_color: Props_outline_color,
	outline_offset: Props_outline_offset,
	shadow: Props_shadow,
	inset_shadow: Props_inset_shadow,
	opacity: Props_opacity,
	brightness: Props_brightness,
	contrast: Props_contrast,
	saturate: Props_saturate,
	blur: Props_blur,
	backdrop_blur: Props_backdrop_blur,
	fade: Props_fade,
	fade_x: Props_fade_x,
	fade_sides: Props_fade_sides,
	fade_overflow: Props_fade_overflow,
	snap: Props_snap,
	clip_inset: Props_clip_inset,
	layer: Props_layer,
	fit: Props_fit,
	tint: Props_tint,
	cursor: Props_cursor,
	-- Text.
	text_size: Props_text_size,
	weight: Props_weight,
	bold: Props_bold,
	italic: Props_italic,
	align: Props_align,
	font: Props_font,
	wrap: Props_wrap,
	truncate: Props_truncate,
	lines: Props_lines,
	line_height: Props_line_height,
	letter_spacing: Props_letter_spacing,
	-- Tabular figures: every digit the same width.
	tabular: Props_tabular,
	-- Underlined text (inherited, as CSS's `text-decoration`), and how far
	-- below the baseline the line sits.
	underline: Props_underline,
	underline_offset: Props_underline_offset,
	-- Transforms: after layout, about `origin` (fractions of the node).
	x: Props_x,
	y: Props_y,
	scale: Props_scale,
	scale_x: Props_scale_x,
	scale_y: Props_scale_y,
	rotate: Props_rotate,
	skew: Props_skew,
	skew_x: Props_skew_x,
	skew_y: Props_skew_y,
	origin: Props_origin,
	-- The subtree's theme: a theme's name (`"zgx-light"`), or tokens to
	-- override (`{ accent = "#e5484d" }`, with `theme = name` to start from).
	theme: Props_theme,
	-- Identity, for selectors, refs and animations.
	id: Props_id,
	-- Space-separated class names.
	class: Props_class,
	-- Styles chosen from a `ui.variants` definition: `button { tone =
	-- "ghost" }`, or the recipe itself for its defaults. The element's own
	-- properties win over them.
	variants: Props_variants,
	ref: Props_ref,
	focusable: Props_focusable,
	-- What assistive technology takes it for (ARIA's `role`), instead of
	-- what it is: a clickable row as an "option", a row of buttons as a
	-- "toolbar". It keeps what it does.
	role: Props_role,
	-- A section title of this level, 1 to 6 (`h1`…`h6`): screen readers
	-- list headings and move between them.
	heading: Props_heading,
	-- Whether this option, tab or item is the chosen one (`aria-selected`).
	selected: Props_selected,
	-- Whether a switch, checkbox or radio of the app's own is on
	-- (`aria-checked`), and whether what a control opens shows
	-- (`aria-expanded`): a custom disclosure's.
	checked: Props_checked,
	expanded: Props_expanded,
	-- Announce changes to the text in it (`aria-live`): a count, a "Sent"
	-- note, a message arriving.
	live: Props_live,
	-- Read after its name, as a hint (`aria-description`).
	description: Props_description,
	-- What is wrong with its value (`aria-invalid`), followed: nil while it
	-- is fine. Assistive technology hears it with the field.
	error: Props_error,
	-- Left out of the accessibility tree with what it holds (`aria-hidden`):
	-- decoration, or what is said elsewhere.
	aria_hidden: Props_aria_hidden,
	-- One Tab stop for the focusable things inside; the arrows along this
	-- axis (and Home, End) move among them, as ARIA's composite widgets
	-- behave. `role = "listbox"` is vertical, "toolbar" and "tablist"
	-- horizontal, without asking.
	focus_group: Props_focus_group,
	-- A press on it (or on what inside takes no input) moves the window; a
	-- double click zooms it. Buttons and inputs inside keep their clicks.
	-- CSS pointer-events: "none" (or false) lets the pointer through to what
	-- is under the node (presses, hover, the wheel, drops, a native view);
	-- descendants with handlers still take it. A state or function follows.
	pointer_events: Props_pointer_events,
	window_drag: Props_window_drag,
	-- The pointer selects the text in it and everything it holds, as on a
	-- web page (CSS `user-select: text`): drag, double/triple press, Shift,
	-- ⌘A / Ctrl+A, ⌘C / Ctrl+C. `false` keeps a part out. Off unless set.
	selectable: Props_selectable,
	-- A selectable node's highlight and selected-text colours (CSS
	-- `::selection`), over the theme's `$selection` and `$selection.text`
	-- tokens; a state or function follows.
	selection_color: Props_selection_color,
	selected_text_color: Props_selected_text_color,
	-- Shown over it while the pointer rests on it (after 300 ms; at once
	-- within a second of another): text in the theme's look, or a node of
	-- the app's own. Above it, centred, unless `tooltip_placement` says.
	tooltip: Props_tooltip,
	tooltip_placement: Props_tooltip_placement,
	-- A hover card the pointer can move into and click in: it stays while
	-- the pointer is over the node or the card.
	tooltip_interactive: Props_tooltip_interactive,
	-- Milliseconds the pointer rests before it shows (300; once one
	-- shows, the next shows at once). Keyboard focus shows it too.
	tooltip_delay: Props_tooltip_delay,
	-- A condition (true disables), or the styles it shows while disabled.
	disabled: Props_disabled,
	hover: Props_hover,
	active: Props_active,
	focus: Props_focus,
	-- Size animation ("height", "width", "size"; `true` is "height") or
	-- position animation ("position"). A `layout_id` node mounting as
	-- another with the same id unmounts (or while it is mounted) moves and
	-- scales from that one's box.
	layout: Props_layout,
	layout_id: Props_layout_id,
	layout_ease: Props_layout_ease,
	-- Milliseconds.
	layout_duration: Props_layout_duration,
	layout_enter: Props_layout_enter,
	layout_clip: Props_layout_clip,
	-- The content shows through the bitmap's alpha: `mask = bitmap`, or
	-- `{ bitmap, w = …, h = …, x = …, y = …, sweep = … }`.
	mask: Props_mask,
	bg_shader: Props_bg_shader,
	-- Up to four vec4s: lists of numbers, or colors (0 to 1).
	shader_params: Props_shader_params,
	-- Events.
	on_click: Props_on_click,
	-- Primary pointer down and up.
	on_press: Props_on_press,
	on_release: Props_on_release,
	on_hover: Props_on_hover,
	on_pointer_move: Props_on_pointer_move,
	-- Return true to consume the key.
	on_key: Props_on_key,
	on_key_up: Props_on_key_up,
	on_focus: Props_on_focus,
	on_blur: Props_on_blur,
	-- Return true to take the gesture from the scroll views around it.
	on_wheel: Props_on_wheel,
	on_pinch: Props_on_pinch,
	on_mount: Props_on_mount,
	-- When layout gives it a new size, and at its first layout.
	on_resize: Props_on_resize,
	-- When any of it comes into view or all of it leaves (inside every
	-- clip and scroll viewport above it), and at its first layout: for
	-- building what only matters on screen.
	on_visible: Props_on_visible,
	-- Drag and drop within the window. Payloads are what `on_drag`
	-- returns: the script's own plain values, so they are `any` here.
	-- A source: `on_drag` returns the payload once the pointer moves 4
	-- points; `drag_preview` builds what follows the pointer.
	on_drag: Props_on_drag,
	drag_preview: Props_drag_preview,
	on_drag_end: Props_on_drag_end,
	-- What the drag carries to other apps as it leaves the window (macOS):
	-- Finder takes files, Mail text, a browser links.
	drag_export: Props_drag_export,
	-- Released outside every window of the app (macOS): a tab torn off its
	-- window. `grab_x`/`grab_y` are where the source was held.
	on_drag_outside: Props_on_drag_outside,
	-- The drag moves its window instead, with no preview (macOS): a
	-- window's only tab. Dropped on another window's destination it drops
	-- there; elsewhere the window stays where it was let go.
	drag_moves_window: Props_drag_moves_window,
	-- A destination: `drop_when` filters payloads (default: all).
	drop_when: Props_drop_when,
	on_drag_over: Props_on_drag_over,
	on_drag_leave: Props_on_drag_leave,
	on_drop: Props_on_drop,
	-- A secondary click (control-click on macOS) shows the returned
	-- `menu_item`s, `submenu`s and separators as a menu at the pointer;
	-- returning nil (or nothing) shows none.
	context_menu: Props_context_menu,
	-- The context menu panel's look: bg, rounded, p, gap, backdrop_blur,
	-- border, shadow, w, …
	context_menu_style: Props_context_menu_style,
	-- Open the context menu under the node, from its left edge (a tab's),
	-- rather than at the pointer.
	context_menu_below: Props_context_menu_below,
	[number]: never,
}
export type Props_w = Reactive<Length>?
export type Props_h = Reactive<Length>?
export type Props_size = Reactive<Pair>?
export type Props_min_w = Reactive<Length>?
export type Props_max_w = Reactive<Length>?
export type Props_min_h = Reactive<Length>?
export type Props_max_h = Reactive<Length>?
export type Props_w_full = Reactive<boolean>?
export type Props_h_full = Reactive<boolean>?
export type Props_aspect = Reactive<number>?
export type Props_absolute = Reactive<boolean>?
export type Props_relative = Reactive<boolean>?
export type Props_top = Reactive<Length>?
export type Props_left = Reactive<Length>?
export type Props_inset_start = Reactive<Length>?
export type Props_inset_end = Reactive<Length>?
export type Props_right = Reactive<Length>?
export type Props_bottom = Reactive<Length>?
export type Props_p = Reactive<Length>?
export type Props_px = Reactive<Length>?
export type Props_py = Reactive<Length>?
export type Props_pt = Reactive<Length>?
export type Props_pr = Reactive<Length>?
export type Props_pb = Reactive<Length>?
export type Props_pl = Reactive<Length>?
export type Props_ps = Reactive<Length>?
export type Props_pe = Reactive<Length>?
export type Props_m = Reactive<Length>?
export type Props_mx = Reactive<Length>?
export type Props_my = Reactive<Length>?
export type Props_mt = Reactive<Length>?
export type Props_mr = Reactive<Length>?
export type Props_mb = Reactive<Length>?
export type Props_ml = Reactive<Length>?
export type Props_ms = Reactive<Length>?
export type Props_me = Reactive<Length>?
export type Props_gap = Reactive<Num>?
export type Props_gap_x = Reactive<Length>?
export type Props_gap_y = Reactive<Length>?
export type Props_direction = Reactive<Direction>?
export type Props_dir = Reactive<"ltr" | "rtl">?
export type Props_grow = Reactive<number>?
export type Props_shrink = Reactive<number>?
export type Props_basis = Reactive<Length>?
export type Props_flex_wrap = Reactive<boolean>?
export type Props_wrap_reverse = Reactive<boolean>?
export type Props_items = Reactive<Align>?
export type Props_justify = Reactive<Justify>?
export type Props_self = Reactive<Align>?
export type Props_content = Reactive<AlignContent>?
export type Props_grid = Reactive<boolean>?
export type Props_cols = Reactive<number>?
export type Props_rows = Reactive<number>?
export type Props_col_start = Reactive<number>?
export type Props_row_start = Reactive<number>?
export type Props_col_span = Reactive<number>?
export type Props_row_span = Reactive<number>?
export type Props_col_span_full = Reactive<boolean>?
export type Props_row_span_full = Reactive<boolean>?
export type Props_overflow = Reactive<Overflow>?
export type Props_overflow_x = Reactive<Overflow>?
export type Props_overflow_y = Reactive<Overflow>?
export type Props_hidden = Reactive<boolean>?
export type Props_invisible = Reactive<boolean>?
export type Props_bg = Reactive<Color>?
export type Props_gradient = Reactive<Gradient>?
export type Props_color = Reactive<Color>?
export type Props_bg_image = Reactive<string>?
export type Props_bg_fit = Reactive<Fit>?
export type Props_rounded = Reactive<Num>?
export type Props_corners = Reactive<Corners>?
export type Props_rounded_tl = Reactive<number>?
export type Props_rounded_tr = Reactive<number>?
export type Props_rounded_br = Reactive<number>?
export type Props_rounded_bl = Reactive<number>?
export type Props_border = Reactive<Num>?
export type Props_border_sides = Reactive<Sides>?
export type Props_border_color = Reactive<Color>?
export type Props_border_over = Reactive<boolean>?
export type Props_border_dash = Reactive<Pair?>?
export type Props_outline = Reactive<Num>?
export type Props_outline_color = Reactive<Color>?
export type Props_outline_offset = Reactive<Num>?
export type Props_shadow = Reactive<Shadow>?
export type Props_inset_shadow = Reactive<Shadow>?
export type Props_opacity = Reactive<Num>?
export type Props_brightness = Reactive<number>?
export type Props_contrast = Reactive<number>?
export type Props_saturate = Reactive<number>?
export type Props_blur = Reactive<number>?
export type Props_backdrop_blur = Reactive<Num>?
export type Props_fade = Reactive<Pair>?
export type Props_fade_x = Reactive<Pair>?
export type Props_fade_sides = Reactive<Sides>?
export type Props_fade_overflow = Reactive<boolean>?
export type Props_snap = Reactive<boolean>?
export type Props_clip_inset = Reactive<Sides>?
export type Props_layer = Reactive<boolean>?
export type Props_fit = Reactive<Fit>?
export type Props_tint = Reactive<Color>?
export type Props_cursor = Reactive<Cursor>?
export type Props_text_size = Reactive<Num>?
export type Props_weight = Reactive<Num>?
export type Props_bold = Reactive<boolean>?
export type Props_italic = Reactive<boolean>?
export type Props_align = Reactive<TextAlign>?
export type Props_font = Reactive<Font>?
export type Props_wrap = Reactive<boolean>?
export type Props_truncate = Reactive<boolean>?
export type Props_lines = Reactive<number>?
export type Props_line_height = Reactive<Num>?
export type Props_letter_spacing = Reactive<Num>?
export type Props_tabular = Reactive<boolean>?
export type Props_underline = Reactive<boolean>?
export type Props_underline_offset = Reactive<Num>?
export type Props_x = Reactive<number>?
export type Props_y = Reactive<number>?
export type Props_scale = Reactive<number>?
export type Props_scale_x = Reactive<number>?
export type Props_scale_y = Reactive<number>?
export type Props_rotate = Reactive<number>?
export type Props_skew = Reactive<number>?
export type Props_skew_x = Reactive<number>?
export type Props_skew_y = Reactive<number>?
export type Props_origin = Reactive<Pair>?
export type Props_theme = Reactive<ThemeSpec>?
export type Props_id = string?
export type Props_class = string?
export type Props_variants = (Variants | ((choice: any) -> Variants))?
export type Props_ref = (Ref | { Ref })?
export type Props_focusable = boolean?
export type Props_role = AriaRole?
export type Props_heading = number?
export type Props_selected = (boolean | Readable<boolean> | (() -> boolean))?
export type Props_checked = (boolean | Readable<boolean> | (() -> boolean))?
export type Props_expanded = (boolean | Readable<boolean> | (() -> boolean))?
export type Props_live = ("polite" | "assertive" | "off")?
export type Props_description = string?
export type Props_error = (string | Readable<string?> | (() -> string?))?
export type Props_aria_hidden = boolean?
export type Props_focus_group = ("vertical" | "horizontal" | "both")?
export type Props_pointer_events = (PointerEvents | Readable<PointerEvents> | (() -> PointerEvents))?
export type Props_window_drag = boolean?
export type Props_selectable = boolean?
export type Props_selection_color = Reactive<Color>?
export type Props_selected_text_color = Reactive<Color>?
export type Props_tooltip = (string | Node)?
export type Props_tooltip_placement = Placement?
export type Props_tooltip_interactive = boolean?
export type Props_tooltip_delay = number?
export type Props_disabled = (boolean | Readable<boolean> | (() -> boolean) | Styles)?
export type Props_hover = Styles?
export type Props_active = Styles?
export type Props_focus = Styles?
export type Props_layout = ("height" | "width" | "size" | "position" | boolean)?
export type Props_layout_id = string?
export type Props_layout_ease = Easing?
export type Props_layout_duration = number?
export type Props_layout_enter = boolean?
export type Props_layout_clip = boolean?
export type Props_mask = (Bitmap | MaskOptions)?
export type Props_bg_shader = Shader?
export type Props_shader_params = { read [number]: Color | { number } }?
export type Props_on_click = (ClickHandler | Handler | Action)?
export type Props_on_press = (((e: PointerEvent) -> ...unknown) | Action)?
export type Props_on_release = (((e: PointerEvent) -> ...unknown) | Action)?
export type Props_on_hover = (((inside: boolean) -> ...unknown) | Action)?
export type Props_on_pointer_move = (((e: PointerEvent) -> ...unknown) | Action)?
export type Props_on_key = (((e: KeyEvent) -> ...boolean?) | Action)?
export type Props_on_key_up = (((key: string) -> ...unknown) | Action)?
export type Props_on_focus = (Handler | Action)?
export type Props_on_blur = (Handler | Action)?
export type Props_on_wheel = (((e: WheelEvent) -> ...boolean?) | Action)?
export type Props_on_pinch = (((e: PinchEvent) -> ...boolean?) | Action)?
export type Props_on_mount = ((element: Element) -> ...unknown)?
export type Props_on_resize = ((size: Size, element: Element) -> ...unknown)?
export type Props_on_visible = ((visible: boolean, element: Element) -> ...unknown)?
export type Props_on_drag = (() -> any)?
export type Props_drag_preview = ((payload: any) -> Node)?
export type Props_on_drag_end = ((payload: any, accepted: boolean) -> ...unknown)?
export type Props_drag_export = ((payload: any) -> DragExport?)?
export type Props_on_drag_outside = ((payload: any, e: { x: number, y: number, grab_x: number, grab_y: number }) -> ...unknown)?
export type Props_drag_moves_window = ((payload: any) -> boolean)?
export type Props_drop_when = ((payload: any) -> unknown)?
export type Props_on_drag_over = ((payload: any, e: DragPoint) -> ...unknown)?
export type Props_on_drag_leave = ((payload: any) -> ...unknown)?
export type Props_on_drop = ((payload: any, e: DragPoint) -> ...unknown)?
export type Props_context_menu = ((e: PointerEvent) -> ...{ Node }?)?
export type Props_context_menu_style = Styles?
export type Props_context_menu_below = boolean?
-- column, row, div, overlay, grid, button: and children.
export type ContainerProps = {
	-- Size and position.
	w: Props_w,
	h: Props_h,
	size: Props_size,
	min_w: Props_min_w,
	max_w: Props_max_w,
	min_h: Props_min_h,
	max_h: Props_max_h,
	w_full: Props_w_full,
	h_full: Props_h_full,
	aspect: Props_aspect,
	absolute: Props_absolute,
	relative: Props_relative,
	top: Props_top,
	left: Props_left,
	inset_start: Props_inset_start,
	inset_end: Props_inset_end,
	right: Props_right,
	bottom: Props_bottom,
	-- Spacing.
	p: Props_p,
	px: Props_px,
	py: Props_py,
	pt: Props_pt,
	pr: Props_pr,
	pb: Props_pb,
	pl: Props_pl,
	ps: Props_ps,
	pe: Props_pe,
	m: Props_m,
	mx: Props_mx,
	my: Props_my,
	mt: Props_mt,
	mr: Props_mr,
	mb: Props_mb,
	ml: Props_ml,
	ms: Props_ms,
	me: Props_me,
	gap: Props_gap,
	gap_x: Props_gap_x,
	gap_y: Props_gap_y,
	-- Flex and grid.
	direction: Props_direction,
	dir: Props_dir,
	grow: Props_grow,
	shrink: Props_shrink,
	basis: Props_basis,
	flex_wrap: Props_flex_wrap,
	wrap_reverse: Props_wrap_reverse,
	items: Props_items,
	justify: Props_justify,
	self: Props_self,
	content: Props_content,
	grid: Props_grid,
	cols: Props_cols,
	rows: Props_rows,
	col_start: Props_col_start,
	row_start: Props_row_start,
	col_span: Props_col_span,
	row_span: Props_row_span,
	col_span_full: Props_col_span_full,
	row_span_full: Props_row_span_full,
	overflow: Props_overflow,
	overflow_x: Props_overflow_x,
	overflow_y: Props_overflow_y,
	hidden: Props_hidden,
	invisible: Props_invisible,
	-- Paint.
	bg: Props_bg,
	gradient: Props_gradient,
	color: Props_color,
	bg_image: Props_bg_image,
	bg_fit: Props_bg_fit,
	rounded: Props_rounded,
	corners: Props_corners,
	rounded_tl: Props_rounded_tl,
	rounded_tr: Props_rounded_tr,
	rounded_br: Props_rounded_br,
	rounded_bl: Props_rounded_bl,
	border: Props_border,
	border_sides: Props_border_sides,
	border_color: Props_border_color,
	border_over: Props_border_over,
	-- A function may return nil: a solid border.
	border_dash: Props_border_dash,
	outline: Props_outline,
	outline_color: Props_outline_color,
	outline_offset: Props_outline_offset,
	shadow: Props_shadow,
	inset_shadow: Props_inset_shadow,
	opacity: Props_opacity,
	brightness: Props_brightness,
	contrast: Props_contrast,
	saturate: Props_saturate,
	blur: Props_blur,
	backdrop_blur: Props_backdrop_blur,
	fade: Props_fade,
	fade_x: Props_fade_x,
	fade_sides: Props_fade_sides,
	fade_overflow: Props_fade_overflow,
	snap: Props_snap,
	clip_inset: Props_clip_inset,
	layer: Props_layer,
	fit: Props_fit,
	tint: Props_tint,
	cursor: Props_cursor,
	-- Text.
	text_size: Props_text_size,
	weight: Props_weight,
	bold: Props_bold,
	italic: Props_italic,
	align: Props_align,
	font: Props_font,
	wrap: Props_wrap,
	truncate: Props_truncate,
	lines: Props_lines,
	line_height: Props_line_height,
	letter_spacing: Props_letter_spacing,
	-- Tabular figures: every digit the same width.
	tabular: Props_tabular,
	-- Underlined text (inherited, as CSS's `text-decoration`), and how far
	-- below the baseline the line sits.
	underline: Props_underline,
	underline_offset: Props_underline_offset,
	-- Transforms: after layout, about `origin` (fractions of the node).
	x: Props_x,
	y: Props_y,
	scale: Props_scale,
	scale_x: Props_scale_x,
	scale_y: Props_scale_y,
	rotate: Props_rotate,
	skew: Props_skew,
	skew_x: Props_skew_x,
	skew_y: Props_skew_y,
	origin: Props_origin,
	-- The subtree's theme: a theme's name (`"zgx-light"`), or tokens to
	-- override (`{ accent = "#e5484d" }`, with `theme = name` to start from).
	theme: Props_theme,
	-- Identity, for selectors, refs and animations.
	id: Props_id,
	-- Space-separated class names.
	class: Props_class,
	-- Styles chosen from a `ui.variants` definition: `button { tone =
	-- "ghost" }`, or the recipe itself for its defaults. The element's own
	-- properties win over them.
	variants: Props_variants,
	ref: Props_ref,
	focusable: Props_focusable,
	-- What assistive technology takes it for (ARIA's `role`), instead of
	-- what it is: a clickable row as an "option", a row of buttons as a
	-- "toolbar". It keeps what it does.
	role: Props_role,
	-- A section title of this level, 1 to 6 (`h1`…`h6`): screen readers
	-- list headings and move between them.
	heading: Props_heading,
	-- Whether this option, tab or item is the chosen one (`aria-selected`).
	selected: Props_selected,
	-- Whether a switch, checkbox or radio of the app's own is on
	-- (`aria-checked`), and whether what a control opens shows
	-- (`aria-expanded`): a custom disclosure's.
	checked: Props_checked,
	expanded: Props_expanded,
	-- Announce changes to the text in it (`aria-live`): a count, a "Sent"
	-- note, a message arriving.
	live: Props_live,
	-- Read after its name, as a hint (`aria-description`).
	description: Props_description,
	-- What is wrong with its value (`aria-invalid`), followed: nil while it
	-- is fine. Assistive technology hears it with the field.
	error: Props_error,
	-- Left out of the accessibility tree with what it holds (`aria-hidden`):
	-- decoration, or what is said elsewhere.
	aria_hidden: Props_aria_hidden,
	-- One Tab stop for the focusable things inside; the arrows along this
	-- axis (and Home, End) move among them, as ARIA's composite widgets
	-- behave. `role = "listbox"` is vertical, "toolbar" and "tablist"
	-- horizontal, without asking.
	focus_group: Props_focus_group,
	-- A press on it (or on what inside takes no input) moves the window; a
	-- double click zooms it. Buttons and inputs inside keep their clicks.
	-- CSS pointer-events: "none" (or false) lets the pointer through to what
	-- is under the node (presses, hover, the wheel, drops, a native view);
	-- descendants with handlers still take it. A state or function follows.
	pointer_events: Props_pointer_events,
	window_drag: Props_window_drag,
	-- The pointer selects the text in it and everything it holds, as on a
	-- web page (CSS `user-select: text`): drag, double/triple press, Shift,
	-- ⌘A / Ctrl+A, ⌘C / Ctrl+C. `false` keeps a part out. Off unless set.
	selectable: Props_selectable,
	-- A selectable node's highlight and selected-text colours (CSS
	-- `::selection`), over the theme's `$selection` and `$selection.text`
	-- tokens; a state or function follows.
	selection_color: Props_selection_color,
	selected_text_color: Props_selected_text_color,
	-- Shown over it while the pointer rests on it (after 300 ms; at once
	-- within a second of another): text in the theme's look, or a node of
	-- the app's own. Above it, centred, unless `tooltip_placement` says.
	tooltip: Props_tooltip,
	tooltip_placement: Props_tooltip_placement,
	-- A hover card the pointer can move into and click in: it stays while
	-- the pointer is over the node or the card.
	tooltip_interactive: Props_tooltip_interactive,
	-- Milliseconds the pointer rests before it shows (300; once one
	-- shows, the next shows at once). Keyboard focus shows it too.
	tooltip_delay: Props_tooltip_delay,
	-- A condition (true disables), or the styles it shows while disabled.
	disabled: Props_disabled,
	hover: Props_hover,
	active: Props_active,
	focus: Props_focus,
	-- Size animation ("height", "width", "size"; `true` is "height") or
	-- position animation ("position"). A `layout_id` node mounting as
	-- another with the same id unmounts (or while it is mounted) moves and
	-- scales from that one's box.
	layout: Props_layout,
	layout_id: Props_layout_id,
	layout_ease: Props_layout_ease,
	-- Milliseconds.
	layout_duration: Props_layout_duration,
	layout_enter: Props_layout_enter,
	layout_clip: Props_layout_clip,
	-- The content shows through the bitmap's alpha: `mask = bitmap`, or
	-- `{ bitmap, w = …, h = …, x = …, y = …, sweep = … }`.
	mask: Props_mask,
	bg_shader: Props_bg_shader,
	-- Up to four vec4s: lists of numbers, or colors (0 to 1).
	shader_params: Props_shader_params,
	-- Events.
	on_click: Props_on_click,
	-- Primary pointer down and up.
	on_press: Props_on_press,
	on_release: Props_on_release,
	on_hover: Props_on_hover,
	on_pointer_move: Props_on_pointer_move,
	-- Return true to consume the key.
	on_key: Props_on_key,
	on_key_up: Props_on_key_up,
	on_focus: Props_on_focus,
	on_blur: Props_on_blur,
	-- Return true to take the gesture from the scroll views around it.
	on_wheel: Props_on_wheel,
	on_pinch: Props_on_pinch,
	on_mount: Props_on_mount,
	-- When layout gives it a new size, and at its first layout.
	on_resize: Props_on_resize,
	-- When any of it comes into view or all of it leaves (inside every
	-- clip and scroll viewport above it), and at its first layout: for
	-- building what only matters on screen.
	on_visible: Props_on_visible,
	-- Drag and drop within the window. Payloads are what `on_drag`
	-- returns: the script's own plain values, so they are `any` here.
	-- A source: `on_drag` returns the payload once the pointer moves 4
	-- points; `drag_preview` builds what follows the pointer.
	on_drag: Props_on_drag,
	drag_preview: Props_drag_preview,
	on_drag_end: Props_on_drag_end,
	-- What the drag carries to other apps as it leaves the window (macOS):
	-- Finder takes files, Mail text, a browser links.
	drag_export: Props_drag_export,
	-- Released outside every window of the app (macOS): a tab torn off its
	-- window. `grab_x`/`grab_y` are where the source was held.
	on_drag_outside: Props_on_drag_outside,
	-- The drag moves its window instead, with no preview (macOS): a
	-- window's only tab. Dropped on another window's destination it drops
	-- there; elsewhere the window stays where it was let go.
	drag_moves_window: Props_drag_moves_window,
	-- A destination: `drop_when` filters payloads (default: all).
	drop_when: Props_drop_when,
	on_drag_over: Props_on_drag_over,
	on_drag_leave: Props_on_drag_leave,
	on_drop: Props_on_drop,
	-- A secondary click (control-click on macOS) shows the returned
	-- `menu_item`s, `submenu`s and separators as a menu at the pointer;
	-- returning nil (or nothing) shows none.
	context_menu: Props_context_menu,
	-- The context menu panel's look: bg, rounded, p, gap, backdrop_blur,
	-- border, shadow, w, …
	context_menu_style: Props_context_menu_style,
	-- Open the context menu under the node, from its left edge (a tab's),
	-- rather than at the pointer.
	context_menu_below: Props_context_menu_below,
	[number]: Child,
	-- The name assistive technology reads for it, instead of its text: a
	-- switch, a button showing only an icon. A state or function is
	-- followed: a list row's name made from what it shows now.
	label: (string | Readable<string> | (() -> string))?,
}
export type ScrollProps = {
	-- Size and position.
	w: Props_w,
	h: Props_h,
	size: Props_size,
	min_w: Props_min_w,
	max_w: Props_max_w,
	min_h: Props_min_h,
	max_h: Props_max_h,
	w_full: Props_w_full,
	h_full: Props_h_full,
	aspect: Props_aspect,
	absolute: Props_absolute,
	relative: Props_relative,
	top: Props_top,
	left: Props_left,
	inset_start: Props_inset_start,
	inset_end: Props_inset_end,
	right: Props_right,
	bottom: Props_bottom,
	-- Spacing.
	p: Props_p,
	px: Props_px,
	py: Props_py,
	pt: Props_pt,
	pr: Props_pr,
	pb: Props_pb,
	pl: Props_pl,
	ps: Props_ps,
	pe: Props_pe,
	m: Props_m,
	mx: Props_mx,
	my: Props_my,
	mt: Props_mt,
	mr: Props_mr,
	mb: Props_mb,
	ml: Props_ml,
	ms: Props_ms,
	me: Props_me,
	gap: Props_gap,
	gap_x: Props_gap_x,
	gap_y: Props_gap_y,
	-- Flex and grid.
	direction: Props_direction,
	dir: Props_dir,
	grow: Props_grow,
	shrink: Props_shrink,
	basis: Props_basis,
	flex_wrap: Props_flex_wrap,
	wrap_reverse: Props_wrap_reverse,
	items: Props_items,
	justify: Props_justify,
	self: Props_self,
	content: Props_content,
	grid: Props_grid,
	cols: Props_cols,
	rows: Props_rows,
	col_start: Props_col_start,
	row_start: Props_row_start,
	col_span: Props_col_span,
	row_span: Props_row_span,
	col_span_full: Props_col_span_full,
	row_span_full: Props_row_span_full,
	overflow: Props_overflow,
	overflow_x: Props_overflow_x,
	overflow_y: Props_overflow_y,
	hidden: Props_hidden,
	invisible: Props_invisible,
	-- Paint.
	bg: Props_bg,
	gradient: Props_gradient,
	color: Props_color,
	bg_image: Props_bg_image,
	bg_fit: Props_bg_fit,
	rounded: Props_rounded,
	corners: Props_corners,
	rounded_tl: Props_rounded_tl,
	rounded_tr: Props_rounded_tr,
	rounded_br: Props_rounded_br,
	rounded_bl: Props_rounded_bl,
	border: Props_border,
	border_sides: Props_border_sides,
	border_color: Props_border_color,
	border_over: Props_border_over,
	-- A function may return nil: a solid border.
	border_dash: Props_border_dash,
	outline: Props_outline,
	outline_color: Props_outline_color,
	outline_offset: Props_outline_offset,
	shadow: Props_shadow,
	inset_shadow: Props_inset_shadow,
	opacity: Props_opacity,
	brightness: Props_brightness,
	contrast: Props_contrast,
	saturate: Props_saturate,
	blur: Props_blur,
	backdrop_blur: Props_backdrop_blur,
	fade: Props_fade,
	fade_x: Props_fade_x,
	fade_sides: Props_fade_sides,
	fade_overflow: Props_fade_overflow,
	snap: Props_snap,
	clip_inset: Props_clip_inset,
	layer: Props_layer,
	fit: Props_fit,
	tint: Props_tint,
	cursor: Props_cursor,
	-- Text.
	text_size: Props_text_size,
	weight: Props_weight,
	bold: Props_bold,
	italic: Props_italic,
	align: Props_align,
	font: Props_font,
	wrap: Props_wrap,
	truncate: Props_truncate,
	lines: Props_lines,
	line_height: Props_line_height,
	letter_spacing: Props_letter_spacing,
	-- Tabular figures: every digit the same width.
	tabular: Props_tabular,
	-- Underlined text (inherited, as CSS's `text-decoration`), and how far
	-- below the baseline the line sits.
	underline: Props_underline,
	underline_offset: Props_underline_offset,
	-- Transforms: after layout, about `origin` (fractions of the node).
	x: Props_x,
	y: Props_y,
	scale: Props_scale,
	scale_x: Props_scale_x,
	scale_y: Props_scale_y,
	rotate: Props_rotate,
	skew: Props_skew,
	skew_x: Props_skew_x,
	skew_y: Props_skew_y,
	origin: Props_origin,
	-- The subtree's theme: a theme's name (`"zgx-light"`), or tokens to
	-- override (`{ accent = "#e5484d" }`, with `theme = name` to start from).
	theme: Props_theme,
	-- Identity, for selectors, refs and animations.
	id: Props_id,
	-- Space-separated class names.
	class: Props_class,
	-- Styles chosen from a `ui.variants` definition: `button { tone =
	-- "ghost" }`, or the recipe itself for its defaults. The element's own
	-- properties win over them.
	variants: Props_variants,
	ref: Props_ref,
	focusable: Props_focusable,
	-- What assistive technology takes it for (ARIA's `role`), instead of
	-- what it is: a clickable row as an "option", a row of buttons as a
	-- "toolbar". It keeps what it does.
	role: Props_role,
	-- A section title of this level, 1 to 6 (`h1`…`h6`): screen readers
	-- list headings and move between them.
	heading: Props_heading,
	-- Whether this option, tab or item is the chosen one (`aria-selected`).
	selected: Props_selected,
	-- Whether a switch, checkbox or radio of the app's own is on
	-- (`aria-checked`), and whether what a control opens shows
	-- (`aria-expanded`): a custom disclosure's.
	checked: Props_checked,
	expanded: Props_expanded,
	-- Announce changes to the text in it (`aria-live`): a count, a "Sent"
	-- note, a message arriving.
	live: Props_live,
	-- Read after its name, as a hint (`aria-description`).
	description: Props_description,
	-- What is wrong with its value (`aria-invalid`), followed: nil while it
	-- is fine. Assistive technology hears it with the field.
	error: Props_error,
	-- Left out of the accessibility tree with what it holds (`aria-hidden`):
	-- decoration, or what is said elsewhere.
	aria_hidden: Props_aria_hidden,
	-- One Tab stop for the focusable things inside; the arrows along this
	-- axis (and Home, End) move among them, as ARIA's composite widgets
	-- behave. `role = "listbox"` is vertical, "toolbar" and "tablist"
	-- horizontal, without asking.
	focus_group: Props_focus_group,
	-- A press on it (or on what inside takes no input) moves the window; a
	-- double click zooms it. Buttons and inputs inside keep their clicks.
	-- CSS pointer-events: "none" (or false) lets the pointer through to what
	-- is under the node (presses, hover, the wheel, drops, a native view);
	-- descendants with handlers still take it. A state or function follows.
	pointer_events: Props_pointer_events,
	window_drag: Props_window_drag,
	-- The pointer selects the text in it and everything it holds, as on a
	-- web page (CSS `user-select: text`): drag, double/triple press, Shift,
	-- ⌘A / Ctrl+A, ⌘C / Ctrl+C. `false` keeps a part out. Off unless set.
	selectable: Props_selectable,
	-- A selectable node's highlight and selected-text colours (CSS
	-- `::selection`), over the theme's `$selection` and `$selection.text`
	-- tokens; a state or function follows.
	selection_color: Props_selection_color,
	selected_text_color: Props_selected_text_color,
	-- Shown over it while the pointer rests on it (after 300 ms; at once
	-- within a second of another): text in the theme's look, or a node of
	-- the app's own. Above it, centred, unless `tooltip_placement` says.
	tooltip: Props_tooltip,
	tooltip_placement: Props_tooltip_placement,
	-- A hover card the pointer can move into and click in: it stays while
	-- the pointer is over the node or the card.
	tooltip_interactive: Props_tooltip_interactive,
	-- Milliseconds the pointer rests before it shows (300; once one
	-- shows, the next shows at once). Keyboard focus shows it too.
	tooltip_delay: Props_tooltip_delay,
	-- A condition (true disables), or the styles it shows while disabled.
	disabled: Props_disabled,
	hover: Props_hover,
	active: Props_active,
	focus: Props_focus,
	-- Size animation ("height", "width", "size"; `true` is "height") or
	-- position animation ("position"). A `layout_id` node mounting as
	-- another with the same id unmounts (or while it is mounted) moves and
	-- scales from that one's box.
	layout: Props_layout,
	layout_id: Props_layout_id,
	layout_ease: Props_layout_ease,
	-- Milliseconds.
	layout_duration: Props_layout_duration,
	layout_enter: Props_layout_enter,
	layout_clip: Props_layout_clip,
	-- The content shows through the bitmap's alpha: `mask = bitmap`, or
	-- `{ bitmap, w = …, h = …, x = …, y = …, sweep = … }`.
	mask: Props_mask,
	bg_shader: Props_bg_shader,
	-- Up to four vec4s: lists of numbers, or colors (0 to 1).
	shader_params: Props_shader_params,
	-- Events.
	on_click: Props_on_click,
	-- Primary pointer down and up.
	on_press: Props_on_press,
	on_release: Props_on_release,
	on_hover: Props_on_hover,
	on_pointer_move: Props_on_pointer_move,
	-- Return true to consume the key.
	on_key: Props_on_key,
	on_key_up: Props_on_key_up,
	on_focus: Props_on_focus,
	on_blur: Props_on_blur,
	-- Return true to take the gesture from the scroll views around it.
	on_wheel: Props_on_wheel,
	on_pinch: Props_on_pinch,
	on_mount: Props_on_mount,
	-- When layout gives it a new size, and at its first layout.
	on_resize: Props_on_resize,
	-- When any of it comes into view or all of it leaves (inside every
	-- clip and scroll viewport above it), and at its first layout: for
	-- building what only matters on screen.
	on_visible: Props_on_visible,
	-- Drag and drop within the window. Payloads are what `on_drag`
	-- returns: the script's own plain values, so they are `any` here.
	-- A source: `on_drag` returns the payload once the pointer moves 4
	-- points; `drag_preview` builds what follows the pointer.
	on_drag: Props_on_drag,
	drag_preview: Props_drag_preview,
	on_drag_end: Props_on_drag_end,
	-- What the drag carries to other apps as it leaves the window (macOS):
	-- Finder takes files, Mail text, a browser links.
	drag_export: Props_drag_export,
	-- Released outside every window of the app (macOS): a tab torn off its
	-- window. `grab_x`/`grab_y` are where the source was held.
	on_drag_outside: Props_on_drag_outside,
	-- The drag moves its window instead, with no preview (macOS): a
	-- window's only tab. Dropped on another window's destination it drops
	-- there; elsewhere the window stays where it was let go.
	drag_moves_window: Props_drag_moves_window,
	-- A destination: `drop_when` filters payloads (default: all).
	drop_when: Props_drop_when,
	on_drag_over: Props_on_drag_over,
	on_drag_leave: Props_on_drag_leave,
	on_drop: Props_on_drop,
	-- A secondary click (control-click on macOS) shows the returned
	-- `menu_item`s, `submenu`s and separators as a menu at the pointer;
	-- returning nil (or nothing) shows none.
	context_menu: Props_context_menu,
	-- The context menu panel's look: bg, rounded, p, gap, backdrop_blur,
	-- border, shadow, w, …
	context_menu_style: Props_context_menu_style,
	-- Open the context menu under the node, from its left edge (a tab's),
	-- rather than at the pointer.
	context_menu_below: Props_context_menu_below,
	[number]: Child,
	-- The name assistive technology reads for it, instead of its text: a
	-- switch, a button showing only an icon. A state or function is
	-- followed: a list row's name made from what it shows now.
	label: (string | Readable<string> | (() -> string))?,
	-- The scroll position in points, both ways.
	offset: State<number>?,
	-- Stays at the end while the content grows, until scrolled away.
	-- An interactive scrollbar over the trailing edge (none by default).
	scrollbar: boolean?,
	follow: boolean?,
}
-- `ui.text { "Count: {count}", text_size = 18 }`: the text first.
export type TextProps = {
	-- Size and position.
	w: Props_w,
	h: Props_h,
	size: Props_size,
	min_w: Props_min_w,
	max_w: Props_max_w,
	min_h: Props_min_h,
	max_h: Props_max_h,
	w_full: Props_w_full,
	h_full: Props_h_full,
	aspect: Props_aspect,
	absolute: Props_absolute,
	relative: Props_relative,
	top: Props_top,
	left: Props_left,
	inset_start: Props_inset_start,
	inset_end: Props_inset_end,
	right: Props_right,
	bottom: Props_bottom,
	-- Spacing.
	p: Props_p,
	px: Props_px,
	py: Props_py,
	pt: Props_pt,
	pr: Props_pr,
	pb: Props_pb,
	pl: Props_pl,
	ps: Props_ps,
	pe: Props_pe,
	m: Props_m,
	mx: Props_mx,
	my: Props_my,
	mt: Props_mt,
	mr: Props_mr,
	mb: Props_mb,
	ml: Props_ml,
	ms: Props_ms,
	me: Props_me,
	gap: Props_gap,
	gap_x: Props_gap_x,
	gap_y: Props_gap_y,
	-- Flex and grid.
	direction: Props_direction,
	dir: Props_dir,
	grow: Props_grow,
	shrink: Props_shrink,
	basis: Props_basis,
	flex_wrap: Props_flex_wrap,
	wrap_reverse: Props_wrap_reverse,
	items: Props_items,
	justify: Props_justify,
	self: Props_self,
	content: Props_content,
	grid: Props_grid,
	cols: Props_cols,
	rows: Props_rows,
	col_start: Props_col_start,
	row_start: Props_row_start,
	col_span: Props_col_span,
	row_span: Props_row_span,
	col_span_full: Props_col_span_full,
	row_span_full: Props_row_span_full,
	overflow: Props_overflow,
	overflow_x: Props_overflow_x,
	overflow_y: Props_overflow_y,
	hidden: Props_hidden,
	invisible: Props_invisible,
	-- Paint.
	bg: Props_bg,
	gradient: Props_gradient,
	color: Props_color,
	bg_image: Props_bg_image,
	bg_fit: Props_bg_fit,
	rounded: Props_rounded,
	corners: Props_corners,
	rounded_tl: Props_rounded_tl,
	rounded_tr: Props_rounded_tr,
	rounded_br: Props_rounded_br,
	rounded_bl: Props_rounded_bl,
	border: Props_border,
	border_sides: Props_border_sides,
	border_color: Props_border_color,
	border_over: Props_border_over,
	-- A function may return nil: a solid border.
	border_dash: Props_border_dash,
	outline: Props_outline,
	outline_color: Props_outline_color,
	outline_offset: Props_outline_offset,
	shadow: Props_shadow,
	inset_shadow: Props_inset_shadow,
	opacity: Props_opacity,
	brightness: Props_brightness,
	contrast: Props_contrast,
	saturate: Props_saturate,
	blur: Props_blur,
	backdrop_blur: Props_backdrop_blur,
	fade: Props_fade,
	fade_x: Props_fade_x,
	fade_sides: Props_fade_sides,
	fade_overflow: Props_fade_overflow,
	snap: Props_snap,
	clip_inset: Props_clip_inset,
	layer: Props_layer,
	fit: Props_fit,
	tint: Props_tint,
	cursor: Props_cursor,
	-- Text.
	text_size: Props_text_size,
	weight: Props_weight,
	bold: Props_bold,
	italic: Props_italic,
	align: Props_align,
	font: Props_font,
	wrap: Props_wrap,
	truncate: Props_truncate,
	lines: Props_lines,
	line_height: Props_line_height,
	letter_spacing: Props_letter_spacing,
	-- Tabular figures: every digit the same width.
	tabular: Props_tabular,
	-- Underlined text (inherited, as CSS's `text-decoration`), and how far
	-- below the baseline the line sits.
	underline: Props_underline,
	underline_offset: Props_underline_offset,
	-- Transforms: after layout, about `origin` (fractions of the node).
	x: Props_x,
	y: Props_y,
	scale: Props_scale,
	scale_x: Props_scale_x,
	scale_y: Props_scale_y,
	rotate: Props_rotate,
	skew: Props_skew,
	skew_x: Props_skew_x,
	skew_y: Props_skew_y,
	origin: Props_origin,
	-- The subtree's theme: a theme's name (`"zgx-light"`), or tokens to
	-- override (`{ accent = "#e5484d" }`, with `theme = name` to start from).
	theme: Props_theme,
	-- Identity, for selectors, refs and animations.
	id: Props_id,
	-- Space-separated class names.
	class: Props_class,
	-- Styles chosen from a `ui.variants` definition: `button { tone =
	-- "ghost" }`, or the recipe itself for its defaults. The element's own
	-- properties win over them.
	variants: Props_variants,
	ref: Props_ref,
	focusable: Props_focusable,
	-- What assistive technology takes it for (ARIA's `role`), instead of
	-- what it is: a clickable row as an "option", a row of buttons as a
	-- "toolbar". It keeps what it does.
	role: Props_role,
	-- A section title of this level, 1 to 6 (`h1`…`h6`): screen readers
	-- list headings and move between them.
	heading: Props_heading,
	-- Whether this option, tab or item is the chosen one (`aria-selected`).
	selected: Props_selected,
	-- Whether a switch, checkbox or radio of the app's own is on
	-- (`aria-checked`), and whether what a control opens shows
	-- (`aria-expanded`): a custom disclosure's.
	checked: Props_checked,
	expanded: Props_expanded,
	-- Announce changes to the text in it (`aria-live`): a count, a "Sent"
	-- note, a message arriving.
	live: Props_live,
	-- Read after its name, as a hint (`aria-description`).
	description: Props_description,
	-- What is wrong with its value (`aria-invalid`), followed: nil while it
	-- is fine. Assistive technology hears it with the field.
	error: Props_error,
	-- Left out of the accessibility tree with what it holds (`aria-hidden`):
	-- decoration, or what is said elsewhere.
	aria_hidden: Props_aria_hidden,
	-- One Tab stop for the focusable things inside; the arrows along this
	-- axis (and Home, End) move among them, as ARIA's composite widgets
	-- behave. `role = "listbox"` is vertical, "toolbar" and "tablist"
	-- horizontal, without asking.
	focus_group: Props_focus_group,
	-- A press on it (or on what inside takes no input) moves the window; a
	-- double click zooms it. Buttons and inputs inside keep their clicks.
	-- CSS pointer-events: "none" (or false) lets the pointer through to what
	-- is under the node (presses, hover, the wheel, drops, a native view);
	-- descendants with handlers still take it. A state or function follows.
	pointer_events: Props_pointer_events,
	window_drag: Props_window_drag,
	-- The pointer selects the text in it and everything it holds, as on a
	-- web page (CSS `user-select: text`): drag, double/triple press, Shift,
	-- ⌘A / Ctrl+A, ⌘C / Ctrl+C. `false` keeps a part out. Off unless set.
	selectable: Props_selectable,
	-- A selectable node's highlight and selected-text colours (CSS
	-- `::selection`), over the theme's `$selection` and `$selection.text`
	-- tokens; a state or function follows.
	selection_color: Props_selection_color,
	selected_text_color: Props_selected_text_color,
	-- Shown over it while the pointer rests on it (after 300 ms; at once
	-- within a second of another): text in the theme's look, or a node of
	-- the app's own. Above it, centred, unless `tooltip_placement` says.
	tooltip: Props_tooltip,
	tooltip_placement: Props_tooltip_placement,
	-- A hover card the pointer can move into and click in: it stays while
	-- the pointer is over the node or the card.
	tooltip_interactive: Props_tooltip_interactive,
	-- Milliseconds the pointer rests before it shows (300; once one
	-- shows, the next shows at once). Keyboard focus shows it too.
	tooltip_delay: Props_tooltip_delay,
	-- A condition (true disables), or the styles it shows while disabled.
	disabled: Props_disabled,
	hover: Props_hover,
	active: Props_active,
	focus: Props_focus,
	-- Size animation ("height", "width", "size"; `true` is "height") or
	-- position animation ("position"). A `layout_id` node mounting as
	-- another with the same id unmounts (or while it is mounted) moves and
	-- scales from that one's box.
	layout: Props_layout,
	layout_id: Props_layout_id,
	layout_ease: Props_layout_ease,
	-- Milliseconds.
	layout_duration: Props_layout_duration,
	layout_enter: Props_layout_enter,
	layout_clip: Props_layout_clip,
	-- The content shows through the bitmap's alpha: `mask = bitmap`, or
	-- `{ bitmap, w = …, h = …, x = …, y = …, sweep = … }`.
	mask: Props_mask,
	bg_shader: Props_bg_shader,
	-- Up to four vec4s: lists of numbers, or colors (0 to 1).
	shader_params: Props_shader_params,
	-- Events.
	on_click: Props_on_click,
	-- Primary pointer down and up.
	on_press: Props_on_press,
	on_release: Props_on_release,
	on_hover: Props_on_hover,
	on_pointer_move: Props_on_pointer_move,
	-- Return true to consume the key.
	on_key: Props_on_key,
	on_key_up: Props_on_key_up,
	on_focus: Props_on_focus,
	on_blur: Props_on_blur,
	-- Return true to take the gesture from the scroll views around it.
	on_wheel: Props_on_wheel,
	on_pinch: Props_on_pinch,
	on_mount: Props_on_mount,
	-- When layout gives it a new size, and at its first layout.
	on_resize: Props_on_resize,
	-- When any of it comes into view or all of it leaves (inside every
	-- clip and scroll viewport above it), and at its first layout: for
	-- building what only matters on screen.
	on_visible: Props_on_visible,
	-- Drag and drop within the window. Payloads are what `on_drag`
	-- returns: the script's own plain values, so they are `any` here.
	-- A source: `on_drag` returns the payload once the pointer moves 4
	-- points; `drag_preview` builds what follows the pointer.
	on_drag: Props_on_drag,
	drag_preview: Props_drag_preview,
	on_drag_end: Props_on_drag_end,
	-- What the drag carries to other apps as it leaves the window (macOS):
	-- Finder takes files, Mail text, a browser links.
	drag_export: Props_drag_export,
	-- Released outside every window of the app (macOS): a tab torn off its
	-- window. `grab_x`/`grab_y` are where the source was held.
	on_drag_outside: Props_on_drag_outside,
	-- The drag moves its window instead, with no preview (macOS): a
	-- window's only tab. Dropped on another window's destination it drops
	-- there; elsewhere the window stays where it was let go.
	drag_moves_window: Props_drag_moves_window,
	-- A destination: `drop_when` filters payloads (default: all).
	drop_when: Props_drop_when,
	on_drag_over: Props_on_drag_over,
	on_drag_leave: Props_on_drag_leave,
	on_drop: Props_on_drop,
	-- A secondary click (control-click on macOS) shows the returned
	-- `menu_item`s, `submenu`s and separators as a menu at the pointer;
	-- returning nil (or nothing) shows none.
	context_menu: Props_context_menu,
	-- The context menu panel's look: bg, rounded, p, gap, backdrop_blur,
	-- border, shadow, w, …
	context_menu_style: Props_context_menu_style,
	-- Open the context menu under the node, from its left edge (a tab's),
	-- rather than at the pointer.
	context_menu_below: Props_context_menu_below,
	[number]: Text,
	-- Show a string as it is, braces and all, not as a template: for data
	-- (code, titles, messages) that might hold `{name}`.
	literal: boolean?,
}
-- `ui.span`'s options are plain values.
export type SpanProps = {
	-- The text first.
	[number]: Text,
	color: Color?,
	bg: Color?,
	text_size: number?,
	weight: number?,
	bold: boolean?,
	italic: boolean?,
	underline: boolean?,
	-- How far below the baseline the underline sits (CSS's
	-- `text-underline-offset`).
	underline_offset: number?,
	strike: boolean?,
	font: "mono"?,
	-- A link (`on_click`) under the pointer: underlined, recoloured. With
	-- it, the link isn't underlined at rest unless `underline` says so.
	hover: { underline: boolean?, color: Color? }?,
	-- An icon before the text, in the line (a file chip's file type):
	-- `src` (a file, `https://…`) or `code` (SVG markup), `size` square
	-- (12), `gap` from the text (4), centred on the line; `tint` a colour,
	-- or `"currentColor"` for the span's.
	icon: {
		src: string?,
		code: string?,
		size: number?,
		gap: number?,
		tint: Color?,
	}?,
	-- A click (and a middle click, which passes as a ⌘-click) on the run,
	-- a link: told the keys held.
	on_click: (ClickHandler | Handler | Action)?,
	-- Where it links (`"https://…"`): a link, underlined, whose click opens
	-- it with the system's handler (after `on_click`'s), and whose context
	-- menu opens or copies it.
	href: string?,
	-- Its text as it is, not a template (see TextProps).
	literal: boolean?,
	-- Room around it: across (before and after it, taking space in the
	-- line), or `{ across, down }`; with `bg`, the background hugs the text
	-- plus this (an inline code chip) instead of filling the line.
	padding: (number | { number })?,
	-- The background's corner radius.
	radius: number?,
}
-- Runs: spans, strings, functions, or span options. `literal`: the runs'
-- strings as they are (a span's own `literal` wins).
export type RichProps = {
	-- Size and position.
	w: Props_w,
	h: Props_h,
	size: Props_size,
	min_w: Props_min_w,
	max_w: Props_max_w,
	min_h: Props_min_h,
	max_h: Props_max_h,
	w_full: Props_w_full,
	h_full: Props_h_full,
	aspect: Props_aspect,
	absolute: Props_absolute,
	relative: Props_relative,
	top: Props_top,
	left: Props_left,
	inset_start: Props_inset_start,
	inset_end: Props_inset_end,
	right: Props_right,
	bottom: Props_bottom,
	-- Spacing.
	p: Props_p,
	px: Props_px,
	py: Props_py,
	pt: Props_pt,
	pr: Props_pr,
	pb: Props_pb,
	pl: Props_pl,
	ps: Props_ps,
	pe: Props_pe,
	m: Props_m,
	mx: Props_mx,
	my: Props_my,
	mt: Props_mt,
	mr: Props_mr,
	mb: Props_mb,
	ml: Props_ml,
	ms: Props_ms,
	me: Props_me,
	gap: Props_gap,
	gap_x: Props_gap_x,
	gap_y: Props_gap_y,
	-- Flex and grid.
	direction: Props_direction,
	dir: Props_dir,
	grow: Props_grow,
	shrink: Props_shrink,
	basis: Props_basis,
	flex_wrap: Props_flex_wrap,
	wrap_reverse: Props_wrap_reverse,
	items: Props_items,
	justify: Props_justify,
	self: Props_self,
	content: Props_content,
	grid: Props_grid,
	cols: Props_cols,
	rows: Props_rows,
	col_start: Props_col_start,
	row_start: Props_row_start,
	col_span: Props_col_span,
	row_span: Props_row_span,
	col_span_full: Props_col_span_full,
	row_span_full: Props_row_span_full,
	overflow: Props_overflow,
	overflow_x: Props_overflow_x,
	overflow_y: Props_overflow_y,
	hidden: Props_hidden,
	invisible: Props_invisible,
	-- Paint.
	bg: Props_bg,
	gradient: Props_gradient,
	color: Props_color,
	bg_image: Props_bg_image,
	bg_fit: Props_bg_fit,
	rounded: Props_rounded,
	corners: Props_corners,
	rounded_tl: Props_rounded_tl,
	rounded_tr: Props_rounded_tr,
	rounded_br: Props_rounded_br,
	rounded_bl: Props_rounded_bl,
	border: Props_border,
	border_sides: Props_border_sides,
	border_color: Props_border_color,
	border_over: Props_border_over,
	-- A function may return nil: a solid border.
	border_dash: Props_border_dash,
	outline: Props_outline,
	outline_color: Props_outline_color,
	outline_offset: Props_outline_offset,
	shadow: Props_shadow,
	inset_shadow: Props_inset_shadow,
	opacity: Props_opacity,
	brightness: Props_brightness,
	contrast: Props_contrast,
	saturate: Props_saturate,
	blur: Props_blur,
	backdrop_blur: Props_backdrop_blur,
	fade: Props_fade,
	fade_x: Props_fade_x,
	fade_sides: Props_fade_sides,
	fade_overflow: Props_fade_overflow,
	snap: Props_snap,
	clip_inset: Props_clip_inset,
	layer: Props_layer,
	fit: Props_fit,
	tint: Props_tint,
	cursor: Props_cursor,
	-- Text.
	text_size: Props_text_size,
	weight: Props_weight,
	bold: Props_bold,
	italic: Props_italic,
	align: Props_align,
	font: Props_font,
	wrap: Props_wrap,
	truncate: Props_truncate,
	lines: Props_lines,
	line_height: Props_line_height,
	letter_spacing: Props_letter_spacing,
	-- Tabular figures: every digit the same width.
	tabular: Props_tabular,
	-- Underlined text (inherited, as CSS's `text-decoration`), and how far
	-- below the baseline the line sits.
	underline: Props_underline,
	underline_offset: Props_underline_offset,
	-- Transforms: after layout, about `origin` (fractions of the node).
	x: Props_x,
	y: Props_y,
	scale: Props_scale,
	scale_x: Props_scale_x,
	scale_y: Props_scale_y,
	rotate: Props_rotate,
	skew: Props_skew,
	skew_x: Props_skew_x,
	skew_y: Props_skew_y,
	origin: Props_origin,
	-- The subtree's theme: a theme's name (`"zgx-light"`), or tokens to
	-- override (`{ accent = "#e5484d" }`, with `theme = name` to start from).
	theme: Props_theme,
	-- Identity, for selectors, refs and animations.
	id: Props_id,
	-- Space-separated class names.
	class: Props_class,
	-- Styles chosen from a `ui.variants` definition: `button { tone =
	-- "ghost" }`, or the recipe itself for its defaults. The element's own
	-- properties win over them.
	variants: Props_variants,
	ref: Props_ref,
	focusable: Props_focusable,
	-- What assistive technology takes it for (ARIA's `role`), instead of
	-- what it is: a clickable row as an "option", a row of buttons as a
	-- "toolbar". It keeps what it does.
	role: Props_role,
	-- A section title of this level, 1 to 6 (`h1`…`h6`): screen readers
	-- list headings and move between them.
	heading: Props_heading,
	-- Whether this option, tab or item is the chosen one (`aria-selected`).
	selected: Props_selected,
	-- Whether a switch, checkbox or radio of the app's own is on
	-- (`aria-checked`), and whether what a control opens shows
	-- (`aria-expanded`): a custom disclosure's.
	checked: Props_checked,
	expanded: Props_expanded,
	-- Announce changes to the text in it (`aria-live`): a count, a "Sent"
	-- note, a message arriving.
	live: Props_live,
	-- Read after its name, as a hint (`aria-description`).
	description: Props_description,
	-- What is wrong with its value (`aria-invalid`), followed: nil while it
	-- is fine. Assistive technology hears it with the field.
	error: Props_error,
	-- Left out of the accessibility tree with what it holds (`aria-hidden`):
	-- decoration, or what is said elsewhere.
	aria_hidden: Props_aria_hidden,
	-- One Tab stop for the focusable things inside; the arrows along this
	-- axis (and Home, End) move among them, as ARIA's composite widgets
	-- behave. `role = "listbox"` is vertical, "toolbar" and "tablist"
	-- horizontal, without asking.
	focus_group: Props_focus_group,
	-- A press on it (or on what inside takes no input) moves the window; a
	-- double click zooms it. Buttons and inputs inside keep their clicks.
	-- CSS pointer-events: "none" (or false) lets the pointer through to what
	-- is under the node (presses, hover, the wheel, drops, a native view);
	-- descendants with handlers still take it. A state or function follows.
	pointer_events: Props_pointer_events,
	window_drag: Props_window_drag,
	-- The pointer selects the text in it and everything it holds, as on a
	-- web page (CSS `user-select: text`): drag, double/triple press, Shift,
	-- ⌘A / Ctrl+A, ⌘C / Ctrl+C. `false` keeps a part out. Off unless set.
	selectable: Props_selectable,
	-- A selectable node's highlight and selected-text colours (CSS
	-- `::selection`), over the theme's `$selection` and `$selection.text`
	-- tokens; a state or function follows.
	selection_color: Props_selection_color,
	selected_text_color: Props_selected_text_color,
	-- Shown over it while the pointer rests on it (after 300 ms; at once
	-- within a second of another): text in the theme's look, or a node of
	-- the app's own. Above it, centred, unless `tooltip_placement` says.
	tooltip: Props_tooltip,
	tooltip_placement: Props_tooltip_placement,
	-- A hover card the pointer can move into and click in: it stays while
	-- the pointer is over the node or the card.
	tooltip_interactive: Props_tooltip_interactive,
	-- Milliseconds the pointer rests before it shows (300; once one
	-- shows, the next shows at once). Keyboard focus shows it too.
	tooltip_delay: Props_tooltip_delay,
	-- A condition (true disables), or the styles it shows while disabled.
	disabled: Props_disabled,
	hover: Props_hover,
	active: Props_active,
	focus: Props_focus,
	-- Size animation ("height", "width", "size"; `true` is "height") or
	-- position animation ("position"). A `layout_id` node mounting as
	-- another with the same id unmounts (or while it is mounted) moves and
	-- scales from that one's box.
	layout: Props_layout,
	layout_id: Props_layout_id,
	layout_ease: Props_layout_ease,
	-- Milliseconds.
	layout_duration: Props_layout_duration,
	layout_enter: Props_layout_enter,
	layout_clip: Props_layout_clip,
	-- The content shows through the bitmap's alpha: `mask = bitmap`, or
	-- `{ bitmap, w = …, h = …, x = …, y = …, sweep = … }`.
	mask: Props_mask,
	bg_shader: Props_bg_shader,
	-- Up to four vec4s: lists of numbers, or colors (0 to 1).
	shader_params: Props_shader_params,
	-- Events.
	on_click: Props_on_click,
	-- Primary pointer down and up.
	on_press: Props_on_press,
	on_release: Props_on_release,
	on_hover: Props_on_hover,
	on_pointer_move: Props_on_pointer_move,
	-- Return true to consume the key.
	on_key: Props_on_key,
	on_key_up: Props_on_key_up,
	on_focus: Props_on_focus,
	on_blur: Props_on_blur,
	-- Return true to take the gesture from the scroll views around it.
	on_wheel: Props_on_wheel,
	on_pinch: Props_on_pinch,
	on_mount: Props_on_mount,
	-- When layout gives it a new size, and at its first layout.
	on_resize: Props_on_resize,
	-- When any of it comes into view or all of it leaves (inside every
	-- clip and scroll viewport above it), and at its first layout: for
	-- building what only matters on screen.
	on_visible: Props_on_visible,
	-- Drag and drop within the window. Payloads are what `on_drag`
	-- returns: the script's own plain values, so they are `any` here.
	-- A source: `on_drag` returns the payload once the pointer moves 4
	-- points; `drag_preview` builds what follows the pointer.
	on_drag: Props_on_drag,
	drag_preview: Props_drag_preview,
	on_drag_end: Props_on_drag_end,
	-- What the drag carries to other apps as it leaves the window (macOS):
	-- Finder takes files, Mail text, a browser links.
	drag_export: Props_drag_export,
	-- Released outside every window of the app (macOS): a tab torn off its
	-- window. `grab_x`/`grab_y` are where the source was held.
	on_drag_outside: Props_on_drag_outside,
	-- The drag moves its window instead, with no preview (macOS): a
	-- window's only tab. Dropped on another window's destination it drops
	-- there; elsewhere the window stays where it was let go.
	drag_moves_window: Props_drag_moves_window,
	-- A destination: `drop_when` filters payloads (default: all).
	drop_when: Props_drop_when,
	on_drag_over: Props_on_drag_over,
	on_drag_leave: Props_on_drag_leave,
	on_drop: Props_on_drop,
	-- A secondary click (control-click on macOS) shows the returned
	-- `menu_item`s, `submenu`s and separators as a menu at the pointer;
	-- returning nil (or nothing) shows none.
	context_menu: Props_context_menu,
	-- The context menu panel's look: bg, rounded, p, gap, backdrop_blur,
	-- border, shadow, w, …
	context_menu_style: Props_context_menu_style,
	-- Open the context menu under the node, from its left edge (a tab's),
	-- rather than at the pointer.
	context_menu_below: Props_context_menu_below,
	[number]: Span | Text | SpanProps,
	literal: boolean?,
}
export type ImageProps = {
	-- Size and position.
	w: Props_w,
	h: Props_h,
	size: Props_size,
	min_w: Props_min_w,
	max_w: Props_max_w,
	min_h: Props_min_h,
	max_h: Props_max_h,
	w_full: Props_w_full,
	h_full: Props_h_full,
	aspect: Props_aspect,
	absolute: Props_absolute,
	relative: Props_relative,
	top: Props_top,
	left: Props_left,
	inset_start: Props_inset_start,
	inset_end: Props_inset_end,
	right: Props_right,
	bottom: Props_bottom,
	-- Spacing.
	p: Props_p,
	px: Props_px,
	py: Props_py,
	pt: Props_pt,
	pr: Props_pr,
	pb: Props_pb,
	pl: Props_pl,
	ps: Props_ps,
	pe: Props_pe,
	m: Props_m,
	mx: Props_mx,
	my: Props_my,
	mt: Props_mt,
	mr: Props_mr,
	mb: Props_mb,
	ml: Props_ml,
	ms: Props_ms,
	me: Props_me,
	gap: Props_gap,
	gap_x: Props_gap_x,
	gap_y: Props_gap_y,
	-- Flex and grid.
	direction: Props_direction,
	dir: Props_dir,
	grow: Props_grow,
	shrink: Props_shrink,
	basis: Props_basis,
	flex_wrap: Props_flex_wrap,
	wrap_reverse: Props_wrap_reverse,
	items: Props_items,
	justify: Props_justify,
	self: Props_self,
	content: Props_content,
	grid: Props_grid,
	cols: Props_cols,
	rows: Props_rows,
	col_start: Props_col_start,
	row_start: Props_row_start,
	col_span: Props_col_span,
	row_span: Props_row_span,
	col_span_full: Props_col_span_full,
	row_span_full: Props_row_span_full,
	overflow: Props_overflow,
	overflow_x: Props_overflow_x,
	overflow_y: Props_overflow_y,
	hidden: Props_hidden,
	invisible: Props_invisible,
	-- Paint.
	bg: Props_bg,
	gradient: Props_gradient,
	color: Props_color,
	bg_image: Props_bg_image,
	bg_fit: Props_bg_fit,
	rounded: Props_rounded,
	corners: Props_corners,
	rounded_tl: Props_rounded_tl,
	rounded_tr: Props_rounded_tr,
	rounded_br: Props_rounded_br,
	rounded_bl: Props_rounded_bl,
	border: Props_border,
	border_sides: Props_border_sides,
	border_color: Props_border_color,
	border_over: Props_border_over,
	-- A function may return nil: a solid border.
	border_dash: Props_border_dash,
	outline: Props_outline,
	outline_color: Props_outline_color,
	outline_offset: Props_outline_offset,
	shadow: Props_shadow,
	inset_shadow: Props_inset_shadow,
	opacity: Props_opacity,
	brightness: Props_brightness,
	contrast: Props_contrast,
	saturate: Props_saturate,
	blur: Props_blur,
	backdrop_blur: Props_backdrop_blur,
	fade: Props_fade,
	fade_x: Props_fade_x,
	fade_sides: Props_fade_sides,
	fade_overflow: Props_fade_overflow,
	snap: Props_snap,
	clip_inset: Props_clip_inset,
	layer: Props_layer,
	fit: Props_fit,
	tint: Props_tint,
	cursor: Props_cursor,
	-- Text.
	text_size: Props_text_size,
	weight: Props_weight,
	bold: Props_bold,
	italic: Props_italic,
	align: Props_align,
	font: Props_font,
	wrap: Props_wrap,
	truncate: Props_truncate,
	lines: Props_lines,
	line_height: Props_line_height,
	letter_spacing: Props_letter_spacing,
	-- Tabular figures: every digit the same width.
	tabular: Props_tabular,
	-- Underlined text (inherited, as CSS's `text-decoration`), and how far
	-- below the baseline the line sits.
	underline: Props_underline,
	underline_offset: Props_underline_offset,
	-- Transforms: after layout, about `origin` (fractions of the node).
	x: Props_x,
	y: Props_y,
	scale: Props_scale,
	scale_x: Props_scale_x,
	scale_y: Props_scale_y,
	rotate: Props_rotate,
	skew: Props_skew,
	skew_x: Props_skew_x,
	skew_y: Props_skew_y,
	origin: Props_origin,
	-- The subtree's theme: a theme's name (`"zgx-light"`), or tokens to
	-- override (`{ accent = "#e5484d" }`, with `theme = name` to start from).
	theme: Props_theme,
	-- Identity, for selectors, refs and animations.
	id: Props_id,
	-- Space-separated class names.
	class: Props_class,
	-- Styles chosen from a `ui.variants` definition: `button { tone =
	-- "ghost" }`, or the recipe itself for its defaults. The element's own
	-- properties win over them.
	variants: Props_variants,
	ref: Props_ref,
	focusable: Props_focusable,
	-- What assistive technology takes it for (ARIA's `role`), instead of
	-- what it is: a clickable row as an "option", a row of buttons as a
	-- "toolbar". It keeps what it does.
	role: Props_role,
	-- A section title of this level, 1 to 6 (`h1`…`h6`): screen readers
	-- list headings and move between them.
	heading: Props_heading,
	-- Whether this option, tab or item is the chosen one (`aria-selected`).
	selected: Props_selected,
	-- Whether a switch, checkbox or radio of the app's own is on
	-- (`aria-checked`), and whether what a control opens shows
	-- (`aria-expanded`): a custom disclosure's.
	checked: Props_checked,
	expanded: Props_expanded,
	-- Announce changes to the text in it (`aria-live`): a count, a "Sent"
	-- note, a message arriving.
	live: Props_live,
	-- Read after its name, as a hint (`aria-description`).
	description: Props_description,
	-- What is wrong with its value (`aria-invalid`), followed: nil while it
	-- is fine. Assistive technology hears it with the field.
	error: Props_error,
	-- Left out of the accessibility tree with what it holds (`aria-hidden`):
	-- decoration, or what is said elsewhere.
	aria_hidden: Props_aria_hidden,
	-- One Tab stop for the focusable things inside; the arrows along this
	-- axis (and Home, End) move among them, as ARIA's composite widgets
	-- behave. `role = "listbox"` is vertical, "toolbar" and "tablist"
	-- horizontal, without asking.
	focus_group: Props_focus_group,
	-- A press on it (or on what inside takes no input) moves the window; a
	-- double click zooms it. Buttons and inputs inside keep their clicks.
	-- CSS pointer-events: "none" (or false) lets the pointer through to what
	-- is under the node (presses, hover, the wheel, drops, a native view);
	-- descendants with handlers still take it. A state or function follows.
	pointer_events: Props_pointer_events,
	window_drag: Props_window_drag,
	-- The pointer selects the text in it and everything it holds, as on a
	-- web page (CSS `user-select: text`): drag, double/triple press, Shift,
	-- ⌘A / Ctrl+A, ⌘C / Ctrl+C. `false` keeps a part out. Off unless set.
	selectable: Props_selectable,
	-- A selectable node's highlight and selected-text colours (CSS
	-- `::selection`), over the theme's `$selection` and `$selection.text`
	-- tokens; a state or function follows.
	selection_color: Props_selection_color,
	selected_text_color: Props_selected_text_color,
	-- Shown over it while the pointer rests on it (after 300 ms; at once
	-- within a second of another): text in the theme's look, or a node of
	-- the app's own. Above it, centred, unless `tooltip_placement` says.
	tooltip: Props_tooltip,
	tooltip_placement: Props_tooltip_placement,
	-- A hover card the pointer can move into and click in: it stays while
	-- the pointer is over the node or the card.
	tooltip_interactive: Props_tooltip_interactive,
	-- Milliseconds the pointer rests before it shows (300; once one
	-- shows, the next shows at once). Keyboard focus shows it too.
	tooltip_delay: Props_tooltip_delay,
	-- A condition (true disables), or the styles it shows while disabled.
	disabled: Props_disabled,
	hover: Props_hover,
	active: Props_active,
	focus: Props_focus,
	-- Size animation ("height", "width", "size"; `true` is "height") or
	-- position animation ("position"). A `layout_id` node mounting as
	-- another with the same id unmounts (or while it is mounted) moves and
	-- scales from that one's box.
	layout: Props_layout,
	layout_id: Props_layout_id,
	layout_ease: Props_layout_ease,
	-- Milliseconds.
	layout_duration: Props_layout_duration,
	layout_enter: Props_layout_enter,
	layout_clip: Props_layout_clip,
	-- The content shows through the bitmap's alpha: `mask = bitmap`, or
	-- `{ bitmap, w = …, h = …, x = …, y = …, sweep = … }`.
	mask: Props_mask,
	bg_shader: Props_bg_shader,
	-- Up to four vec4s: lists of numbers, or colors (0 to 1).
	shader_params: Props_shader_params,
	-- Events.
	on_click: Props_on_click,
	-- Primary pointer down and up.
	on_press: Props_on_press,
	on_release: Props_on_release,
	on_hover: Props_on_hover,
	on_pointer_move: Props_on_pointer_move,
	-- Return true to consume the key.
	on_key: Props_on_key,
	on_key_up: Props_on_key_up,
	on_focus: Props_on_focus,
	on_blur: Props_on_blur,
	-- Return true to take the gesture from the scroll views around it.
	on_wheel: Props_on_wheel,
	on_pinch: Props_on_pinch,
	on_mount: Props_on_mount,
	-- When layout gives it a new size, and at its first layout.
	on_resize: Props_on_resize,
	-- When any of it comes into view or all of it leaves (inside every
	-- clip and scroll viewport above it), and at its first layout: for
	-- building what only matters on screen.
	on_visible: Props_on_visible,
	-- Drag and drop within the window. Payloads are what `on_drag`
	-- returns: the script's own plain values, so they are `any` here.
	-- A source: `on_drag` returns the payload once the pointer moves 4
	-- points; `drag_preview` builds what follows the pointer.
	on_drag: Props_on_drag,
	drag_preview: Props_drag_preview,
	on_drag_end: Props_on_drag_end,
	-- What the drag carries to other apps as it leaves the window (macOS):
	-- Finder takes files, Mail text, a browser links.
	drag_export: Props_drag_export,
	-- Released outside every window of the app (macOS): a tab torn off its
	-- window. `grab_x`/`grab_y` are where the source was held.
	on_drag_outside: Props_on_drag_outside,
	-- The drag moves its window instead, with no preview (macOS): a
	-- window's only tab. Dropped on another window's destination it drops
	-- there; elsewhere the window stays where it was let go.
	drag_moves_window: Props_drag_moves_window,
	-- A destination: `drop_when` filters payloads (default: all).
	drop_when: Props_drop_when,
	on_drag_over: Props_on_drag_over,
	on_drag_leave: Props_on_drag_leave,
	on_drop: Props_on_drop,
	-- A secondary click (control-click on macOS) shows the returned
	-- `menu_item`s, `submenu`s and separators as a menu at the pointer;
	-- returning nil (or nothing) shows none.
	context_menu: Props_context_menu,
	-- The context menu panel's look: bg, rounded, p, gap, backdrop_blur,
	-- border, shadow, w, …
	context_menu_style: Props_context_menu_style,
	-- Open the context menu under the node, from its left edge (a tab's),
	-- rather than at the pointer.
	context_menu_below: Props_context_menu_below,
	[number]: never,
	-- PNG or JPEG, relative to the script; `https://…` (through the
	-- host's fetcher); `host:<name>` (an image the app made).
	src: Reactive<string>,
	-- For accessibility.
	label: string?,
	-- Shown in its place while a remote picture loads and if it fails (an
	-- avatar's initials).
	fallback: Node?,
}
-- One of `src` (a file) and `code` (SVG markup) is required.
export type SvgProps = {
	-- Size and position.
	w: Props_w,
	h: Props_h,
	size: Props_size,
	min_w: Props_min_w,
	max_w: Props_max_w,
	min_h: Props_min_h,
	max_h: Props_max_h,
	w_full: Props_w_full,
	h_full: Props_h_full,
	aspect: Props_aspect,
	absolute: Props_absolute,
	relative: Props_relative,
	top: Props_top,
	left: Props_left,
	inset_start: Props_inset_start,
	inset_end: Props_inset_end,
	right: Props_right,
	bottom: Props_bottom,
	-- Spacing.
	p: Props_p,
	px: Props_px,
	py: Props_py,
	pt: Props_pt,
	pr: Props_pr,
	pb: Props_pb,
	pl: Props_pl,
	ps: Props_ps,
	pe: Props_pe,
	m: Props_m,
	mx: Props_mx,
	my: Props_my,
	mt: Props_mt,
	mr: Props_mr,
	mb: Props_mb,
	ml: Props_ml,
	ms: Props_ms,
	me: Props_me,
	gap: Props_gap,
	gap_x: Props_gap_x,
	gap_y: Props_gap_y,
	-- Flex and grid.
	direction: Props_direction,
	dir: Props_dir,
	grow: Props_grow,
	shrink: Props_shrink,
	basis: Props_basis,
	flex_wrap: Props_flex_wrap,
	wrap_reverse: Props_wrap_reverse,
	items: Props_items,
	justify: Props_justify,
	self: Props_self,
	content: Props_content,
	grid: Props_grid,
	cols: Props_cols,
	rows: Props_rows,
	col_start: Props_col_start,
	row_start: Props_row_start,
	col_span: Props_col_span,
	row_span: Props_row_span,
	col_span_full: Props_col_span_full,
	row_span_full: Props_row_span_full,
	overflow: Props_overflow,
	overflow_x: Props_overflow_x,
	overflow_y: Props_overflow_y,
	hidden: Props_hidden,
	invisible: Props_invisible,
	-- Paint.
	bg: Props_bg,
	gradient: Props_gradient,
	color: Props_color,
	bg_image: Props_bg_image,
	bg_fit: Props_bg_fit,
	rounded: Props_rounded,
	corners: Props_corners,
	rounded_tl: Props_rounded_tl,
	rounded_tr: Props_rounded_tr,
	rounded_br: Props_rounded_br,
	rounded_bl: Props_rounded_bl,
	border: Props_border,
	border_sides: Props_border_sides,
	border_color: Props_border_color,
	border_over: Props_border_over,
	-- A function may return nil: a solid border.
	border_dash: Props_border_dash,
	outline: Props_outline,
	outline_color: Props_outline_color,
	outline_offset: Props_outline_offset,
	shadow: Props_shadow,
	inset_shadow: Props_inset_shadow,
	opacity: Props_opacity,
	brightness: Props_brightness,
	contrast: Props_contrast,
	saturate: Props_saturate,
	blur: Props_blur,
	backdrop_blur: Props_backdrop_blur,
	fade: Props_fade,
	fade_x: Props_fade_x,
	fade_sides: Props_fade_sides,
	fade_overflow: Props_fade_overflow,
	snap: Props_snap,
	clip_inset: Props_clip_inset,
	layer: Props_layer,
	fit: Props_fit,
	tint: Props_tint,
	cursor: Props_cursor,
	-- Text.
	text_size: Props_text_size,
	weight: Props_weight,
	bold: Props_bold,
	italic: Props_italic,
	align: Props_align,
	font: Props_font,
	wrap: Props_wrap,
	truncate: Props_truncate,
	lines: Props_lines,
	line_height: Props_line_height,
	letter_spacing: Props_letter_spacing,
	-- Tabular figures: every digit the same width.
	tabular: Props_tabular,
	-- Underlined text (inherited, as CSS's `text-decoration`), and how far
	-- below the baseline the line sits.
	underline: Props_underline,
	underline_offset: Props_underline_offset,
	-- Transforms: after layout, about `origin` (fractions of the node).
	x: Props_x,
	y: Props_y,
	scale: Props_scale,
	scale_x: Props_scale_x,
	scale_y: Props_scale_y,
	rotate: Props_rotate,
	skew: Props_skew,
	skew_x: Props_skew_x,
	skew_y: Props_skew_y,
	origin: Props_origin,
	-- The subtree's theme: a theme's name (`"zgx-light"`), or tokens to
	-- override (`{ accent = "#e5484d" }`, with `theme = name` to start from).
	theme: Props_theme,
	-- Identity, for selectors, refs and animations.
	id: Props_id,
	-- Space-separated class names.
	class: Props_class,
	-- Styles chosen from a `ui.variants` definition: `button { tone =
	-- "ghost" }`, or the recipe itself for its defaults. The element's own
	-- properties win over them.
	variants: Props_variants,
	ref: Props_ref,
	focusable: Props_focusable,
	-- What assistive technology takes it for (ARIA's `role`), instead of
	-- what it is: a clickable row as an "option", a row of buttons as a
	-- "toolbar". It keeps what it does.
	role: Props_role,
	-- A section title of this level, 1 to 6 (`h1`…`h6`): screen readers
	-- list headings and move between them.
	heading: Props_heading,
	-- Whether this option, tab or item is the chosen one (`aria-selected`).
	selected: Props_selected,
	-- Whether a switch, checkbox or radio of the app's own is on
	-- (`aria-checked`), and whether what a control opens shows
	-- (`aria-expanded`): a custom disclosure's.
	checked: Props_checked,
	expanded: Props_expanded,
	-- Announce changes to the text in it (`aria-live`): a count, a "Sent"
	-- note, a message arriving.
	live: Props_live,
	-- Read after its name, as a hint (`aria-description`).
	description: Props_description,
	-- What is wrong with its value (`aria-invalid`), followed: nil while it
	-- is fine. Assistive technology hears it with the field.
	error: Props_error,
	-- Left out of the accessibility tree with what it holds (`aria-hidden`):
	-- decoration, or what is said elsewhere.
	aria_hidden: Props_aria_hidden,
	-- One Tab stop for the focusable things inside; the arrows along this
	-- axis (and Home, End) move among them, as ARIA's composite widgets
	-- behave. `role = "listbox"` is vertical, "toolbar" and "tablist"
	-- horizontal, without asking.
	focus_group: Props_focus_group,
	-- A press on it (or on what inside takes no input) moves the window; a
	-- double click zooms it. Buttons and inputs inside keep their clicks.
	-- CSS pointer-events: "none" (or false) lets the pointer through to what
	-- is under the node (presses, hover, the wheel, drops, a native view);
	-- descendants with handlers still take it. A state or function follows.
	pointer_events: Props_pointer_events,
	window_drag: Props_window_drag,
	-- The pointer selects the text in it and everything it holds, as on a
	-- web page (CSS `user-select: text`): drag, double/triple press, Shift,
	-- ⌘A / Ctrl+A, ⌘C / Ctrl+C. `false` keeps a part out. Off unless set.
	selectable: Props_selectable,
	-- A selectable node's highlight and selected-text colours (CSS
	-- `::selection`), over the theme's `$selection` and `$selection.text`
	-- tokens; a state or function follows.
	selection_color: Props_selection_color,
	selected_text_color: Props_selected_text_color,
	-- Shown over it while the pointer rests on it (after 300 ms; at once
	-- within a second of another): text in the theme's look, or a node of
	-- the app's own. Above it, centred, unless `tooltip_placement` says.
	tooltip: Props_tooltip,
	tooltip_placement: Props_tooltip_placement,
	-- A hover card the pointer can move into and click in: it stays while
	-- the pointer is over the node or the card.
	tooltip_interactive: Props_tooltip_interactive,
	-- Milliseconds the pointer rests before it shows (300; once one
	-- shows, the next shows at once). Keyboard focus shows it too.
	tooltip_delay: Props_tooltip_delay,
	-- A condition (true disables), or the styles it shows while disabled.
	disabled: Props_disabled,
	hover: Props_hover,
	active: Props_active,
	focus: Props_focus,
	-- Size animation ("height", "width", "size"; `true` is "height") or
	-- position animation ("position"). A `layout_id` node mounting as
	-- another with the same id unmounts (or while it is mounted) moves and
	-- scales from that one's box.
	layout: Props_layout,
	layout_id: Props_layout_id,
	layout_ease: Props_layout_ease,
	-- Milliseconds.
	layout_duration: Props_layout_duration,
	layout_enter: Props_layout_enter,
	layout_clip: Props_layout_clip,
	-- The content shows through the bitmap's alpha: `mask = bitmap`, or
	-- `{ bitmap, w = …, h = …, x = …, y = …, sweep = … }`.
	mask: Props_mask,
	bg_shader: Props_bg_shader,
	-- Up to four vec4s: lists of numbers, or colors (0 to 1).
	shader_params: Props_shader_params,
	-- Events.
	on_click: Props_on_click,
	-- Primary pointer down and up.
	on_press: Props_on_press,
	on_release: Props_on_release,
	on_hover: Props_on_hover,
	on_pointer_move: Props_on_pointer_move,
	-- Return true to consume the key.
	on_key: Props_on_key,
	on_key_up: Props_on_key_up,
	on_focus: Props_on_focus,
	on_blur: Props_on_blur,
	-- Return true to take the gesture from the scroll views around it.
	on_wheel: Props_on_wheel,
	on_pinch: Props_on_pinch,
	on_mount: Props_on_mount,
	-- When layout gives it a new size, and at its first layout.
	on_resize: Props_on_resize,
	-- When any of it comes into view or all of it leaves (inside every
	-- clip and scroll viewport above it), and at its first layout: for
	-- building what only matters on screen.
	on_visible: Props_on_visible,
	-- Drag and drop within the window. Payloads are what `on_drag`
	-- returns: the script's own plain values, so they are `any` here.
	-- A source: `on_drag` returns the payload once the pointer moves 4
	-- points; `drag_preview` builds what follows the pointer.
	on_drag: Props_on_drag,
	drag_preview: Props_drag_preview,
	on_drag_end: Props_on_drag_end,
	-- What the drag carries to other apps as it leaves the window (macOS):
	-- Finder takes files, Mail text, a browser links.
	drag_export: Props_drag_export,
	-- Released outside every window of the app (macOS): a tab torn off its
	-- window. `grab_x`/`grab_y` are where the source was held.
	on_drag_outside: Props_on_drag_outside,
	-- The drag moves its window instead, with no preview (macOS): a
	-- window's only tab. Dropped on another window's destination it drops
	-- there; elsewhere the window stays where it was let go.
	drag_moves_window: Props_drag_moves_window,
	-- A destination: `drop_when` filters payloads (default: all).
	drop_when: Props_drop_when,
	on_drag_over: Props_on_drag_over,
	on_drag_leave: Props_on_drag_leave,
	on_drop: Props_on_drop,
	-- A secondary click (control-click on macOS) shows the returned
	-- `menu_item`s, `submenu`s and separators as a menu at the pointer;
	-- returning nil (or nothing) shows none.
	context_menu: Props_context_menu,
	-- The context menu panel's look: bg, rounded, p, gap, backdrop_blur,
	-- border, shadow, w, …
	context_menu_style: Props_context_menu_style,
	-- Open the context menu under the node, from its left edge (a tab's),
	-- rather than at the pointer.
	context_menu_below: Props_context_menu_below,
	[number]: never,
	src: Reactive<string>?,
	code: string?,
	label: string?,
}
export type CanvasProps = {
	-- Size and position.
	w: Props_w,
	h: Props_h,
	size: Props_size,
	min_w: Props_min_w,
	max_w: Props_max_w,
	min_h: Props_min_h,
	max_h: Props_max_h,
	w_full: Props_w_full,
	h_full: Props_h_full,
	aspect: Props_aspect,
	absolute: Props_absolute,
	relative: Props_relative,
	top: Props_top,
	left: Props_left,
	inset_start: Props_inset_start,
	inset_end: Props_inset_end,
	right: Props_right,
	bottom: Props_bottom,
	-- Spacing.
	p: Props_p,
	px: Props_px,
	py: Props_py,
	pt: Props_pt,
	pr: Props_pr,
	pb: Props_pb,
	pl: Props_pl,
	ps: Props_ps,
	pe: Props_pe,
	m: Props_m,
	mx: Props_mx,
	my: Props_my,
	mt: Props_mt,
	mr: Props_mr,
	mb: Props_mb,
	ml: Props_ml,
	ms: Props_ms,
	me: Props_me,
	gap: Props_gap,
	gap_x: Props_gap_x,
	gap_y: Props_gap_y,
	-- Flex and grid.
	direction: Props_direction,
	dir: Props_dir,
	grow: Props_grow,
	shrink: Props_shrink,
	basis: Props_basis,
	flex_wrap: Props_flex_wrap,
	wrap_reverse: Props_wrap_reverse,
	items: Props_items,
	justify: Props_justify,
	self: Props_self,
	content: Props_content,
	grid: Props_grid,
	cols: Props_cols,
	rows: Props_rows,
	col_start: Props_col_start,
	row_start: Props_row_start,
	col_span: Props_col_span,
	row_span: Props_row_span,
	col_span_full: Props_col_span_full,
	row_span_full: Props_row_span_full,
	overflow: Props_overflow,
	overflow_x: Props_overflow_x,
	overflow_y: Props_overflow_y,
	hidden: Props_hidden,
	invisible: Props_invisible,
	-- Paint.
	bg: Props_bg,
	gradient: Props_gradient,
	color: Props_color,
	bg_image: Props_bg_image,
	bg_fit: Props_bg_fit,
	rounded: Props_rounded,
	corners: Props_corners,
	rounded_tl: Props_rounded_tl,
	rounded_tr: Props_rounded_tr,
	rounded_br: Props_rounded_br,
	rounded_bl: Props_rounded_bl,
	border: Props_border,
	border_sides: Props_border_sides,
	border_color: Props_border_color,
	border_over: Props_border_over,
	-- A function may return nil: a solid border.
	border_dash: Props_border_dash,
	outline: Props_outline,
	outline_color: Props_outline_color,
	outline_offset: Props_outline_offset,
	shadow: Props_shadow,
	inset_shadow: Props_inset_shadow,
	opacity: Props_opacity,
	brightness: Props_brightness,
	contrast: Props_contrast,
	saturate: Props_saturate,
	blur: Props_blur,
	backdrop_blur: Props_backdrop_blur,
	fade: Props_fade,
	fade_x: Props_fade_x,
	fade_sides: Props_fade_sides,
	fade_overflow: Props_fade_overflow,
	snap: Props_snap,
	clip_inset: Props_clip_inset,
	layer: Props_layer,
	fit: Props_fit,
	tint: Props_tint,
	cursor: Props_cursor,
	-- Text.
	text_size: Props_text_size,
	weight: Props_weight,
	bold: Props_bold,
	italic: Props_italic,
	align: Props_align,
	font: Props_font,
	wrap: Props_wrap,
	truncate: Props_truncate,
	lines: Props_lines,
	line_height: Props_line_height,
	letter_spacing: Props_letter_spacing,
	-- Tabular figures: every digit the same width.
	tabular: Props_tabular,
	-- Underlined text (inherited, as CSS's `text-decoration`), and how far
	-- below the baseline the line sits.
	underline: Props_underline,
	underline_offset: Props_underline_offset,
	-- Transforms: after layout, about `origin` (fractions of the node).
	x: Props_x,
	y: Props_y,
	scale: Props_scale,
	scale_x: Props_scale_x,
	scale_y: Props_scale_y,
	rotate: Props_rotate,
	skew: Props_skew,
	skew_x: Props_skew_x,
	skew_y: Props_skew_y,
	origin: Props_origin,
	-- The subtree's theme: a theme's name (`"zgx-light"`), or tokens to
	-- override (`{ accent = "#e5484d" }`, with `theme = name` to start from).
	theme: Props_theme,
	-- Identity, for selectors, refs and animations.
	id: Props_id,
	-- Space-separated class names.
	class: Props_class,
	-- Styles chosen from a `ui.variants` definition: `button { tone =
	-- "ghost" }`, or the recipe itself for its defaults. The element's own
	-- properties win over them.
	variants: Props_variants,
	ref: Props_ref,
	focusable: Props_focusable,
	-- What assistive technology takes it for (ARIA's `role`), instead of
	-- what it is: a clickable row as an "option", a row of buttons as a
	-- "toolbar". It keeps what it does.
	role: Props_role,
	-- A section title of this level, 1 to 6 (`h1`…`h6`): screen readers
	-- list headings and move between them.
	heading: Props_heading,
	-- Whether this option, tab or item is the chosen one (`aria-selected`).
	selected: Props_selected,
	-- Whether a switch, checkbox or radio of the app's own is on
	-- (`aria-checked`), and whether what a control opens shows
	-- (`aria-expanded`): a custom disclosure's.
	checked: Props_checked,
	expanded: Props_expanded,
	-- Announce changes to the text in it (`aria-live`): a count, a "Sent"
	-- note, a message arriving.
	live: Props_live,
	-- Read after its name, as a hint (`aria-description`).
	description: Props_description,
	-- What is wrong with its value (`aria-invalid`), followed: nil while it
	-- is fine. Assistive technology hears it with the field.
	error: Props_error,
	-- Left out of the accessibility tree with what it holds (`aria-hidden`):
	-- decoration, or what is said elsewhere.
	aria_hidden: Props_aria_hidden,
	-- One Tab stop for the focusable things inside; the arrows along this
	-- axis (and Home, End) move among them, as ARIA's composite widgets
	-- behave. `role = "listbox"` is vertical, "toolbar" and "tablist"
	-- horizontal, without asking.
	focus_group: Props_focus_group,
	-- A press on it (or on what inside takes no input) moves the window; a
	-- double click zooms it. Buttons and inputs inside keep their clicks.
	-- CSS pointer-events: "none" (or false) lets the pointer through to what
	-- is under the node (presses, hover, the wheel, drops, a native view);
	-- descendants with handlers still take it. A state or function follows.
	pointer_events: Props_pointer_events,
	window_drag: Props_window_drag,
	-- The pointer selects the text in it and everything it holds, as on a
	-- web page (CSS `user-select: text`): drag, double/triple press, Shift,
	-- ⌘A / Ctrl+A, ⌘C / Ctrl+C. `false` keeps a part out. Off unless set.
	selectable: Props_selectable,
	-- A selectable node's highlight and selected-text colours (CSS
	-- `::selection`), over the theme's `$selection` and `$selection.text`
	-- tokens; a state or function follows.
	selection_color: Props_selection_color,
	selected_text_color: Props_selected_text_color,
	-- Shown over it while the pointer rests on it (after 300 ms; at once
	-- within a second of another): text in the theme's look, or a node of
	-- the app's own. Above it, centred, unless `tooltip_placement` says.
	tooltip: Props_tooltip,
	tooltip_placement: Props_tooltip_placement,
	-- A hover card the pointer can move into and click in: it stays while
	-- the pointer is over the node or the card.
	tooltip_interactive: Props_tooltip_interactive,
	-- Milliseconds the pointer rests before it shows (300; once one
	-- shows, the next shows at once). Keyboard focus shows it too.
	tooltip_delay: Props_tooltip_delay,
	-- A condition (true disables), or the styles it shows while disabled.
	disabled: Props_disabled,
	hover: Props_hover,
	active: Props_active,
	focus: Props_focus,
	-- Size animation ("height", "width", "size"; `true` is "height") or
	-- position animation ("position"). A `layout_id` node mounting as
	-- another with the same id unmounts (or while it is mounted) moves and
	-- scales from that one's box.
	layout: Props_layout,
	layout_id: Props_layout_id,
	layout_ease: Props_layout_ease,
	-- Milliseconds.
	layout_duration: Props_layout_duration,
	layout_enter: Props_layout_enter,
	layout_clip: Props_layout_clip,
	-- The content shows through the bitmap's alpha: `mask = bitmap`, or
	-- `{ bitmap, w = …, h = …, x = …, y = …, sweep = … }`.
	mask: Props_mask,
	bg_shader: Props_bg_shader,
	-- Up to four vec4s: lists of numbers, or colors (0 to 1).
	shader_params: Props_shader_params,
	-- Events.
	on_click: Props_on_click,
	-- Primary pointer down and up.
	on_press: Props_on_press,
	on_release: Props_on_release,
	on_hover: Props_on_hover,
	on_pointer_move: Props_on_pointer_move,
	-- Return true to consume the key.
	on_key: Props_on_key,
	on_key_up: Props_on_key_up,
	on_focus: Props_on_focus,
	on_blur: Props_on_blur,
	-- Return true to take the gesture from the scroll views around it.
	on_wheel: Props_on_wheel,
	on_pinch: Props_on_pinch,
	on_mount: Props_on_mount,
	-- When layout gives it a new size, and at its first layout.
	on_resize: Props_on_resize,
	-- When any of it comes into view or all of it leaves (inside every
	-- clip and scroll viewport above it), and at its first layout: for
	-- building what only matters on screen.
	on_visible: Props_on_visible,
	-- Drag and drop within the window. Payloads are what `on_drag`
	-- returns: the script's own plain values, so they are `any` here.
	-- A source: `on_drag` returns the payload once the pointer moves 4
	-- points; `drag_preview` builds what follows the pointer.
	on_drag: Props_on_drag,
	drag_preview: Props_drag_preview,
	on_drag_end: Props_on_drag_end,
	-- What the drag carries to other apps as it leaves the window (macOS):
	-- Finder takes files, Mail text, a browser links.
	drag_export: Props_drag_export,
	-- Released outside every window of the app (macOS): a tab torn off its
	-- window. `grab_x`/`grab_y` are where the source was held.
	on_drag_outside: Props_on_drag_outside,
	-- The drag moves its window instead, with no preview (macOS): a
	-- window's only tab. Dropped on another window's destination it drops
	-- there; elsewhere the window stays where it was let go.
	drag_moves_window: Props_drag_moves_window,
	-- A destination: `drop_when` filters payloads (default: all).
	drop_when: Props_drop_when,
	on_drag_over: Props_on_drag_over,
	on_drag_leave: Props_on_drag_leave,
	on_drop: Props_on_drop,
	-- A secondary click (control-click on macOS) shows the returned
	-- `menu_item`s, `submenu`s and separators as a menu at the pointer;
	-- returning nil (or nothing) shows none.
	context_menu: Props_context_menu,
	-- The context menu panel's look: bg, rounded, p, gap, backdrop_blur,
	-- border, shadow, w, …
	context_menu_style: Props_context_menu_style,
	-- Open the context menu under the node, from its left edge (a tab's),
	-- rather than at the pointer.
	context_menu_below: Props_context_menu_below,
	[number]: never,
	-- Runs again when the size or the state it reads changes. Required
	-- unless `each` and `draw_item` draw it.
	draw: ((c: Canvas, w: number, h: number) -> ...unknown)?,
	-- A list state whose items are each drawn by `draw_item`, after `draw`,
	-- in order. Each item's drawing is kept: an item not written since
	-- (`set_key`, `patch` and `push` leave the rest as they were) is not
	-- drawn again, so a drag draws the shape it moves.
	each: State<{ any }>?,
	-- One item's drawing, from the item alone: it reads no state (it is
	-- kept until its item changes).
	draw_item: ((c: Canvas, item: any, w: number, h: number) -> ...unknown)?,
	-- What it shows, read by assistive technology as an image's name (a
	-- chart's summary: "Price, now 143.9, high 150.1"). A state or
	-- function is followed. Without one, it is decoration.
	label: (string | Readable<string> | (() -> string))?,
	-- Its drawing's pan and zoom, `{ x, y, zoom }`: drawn at `zoom` times
	-- its size, then moved by `x`, `y` (a board's view). Followed without
	-- calling `draw` again: a pan or zoom places what it drew.
	view: (Readable<CanvasView> | (() -> CanvasView))?,
	-- A grid under its drawing, following `view`: marks every `step`
	-- points (16), the step doubled while they would show closer than
	-- `min_step` (0), each a `dot` this wide (1) or `lines` this wide.
	bg_grid: GridFill?,
}
-- A canvas's pan and zoom (`CanvasProps.view`).
export type CanvasView = { x: number, y: number, zoom: number }
-- `CanvasProps.bg_grid`.
export type GridFill = {
	step: number?,
	min_step: number?,
	dot: number?,
	lines: number?,
	color: Color?,
}
export type CheckboxProps = {
	-- Size and position.
	w: Props_w,
	h: Props_h,
	size: Props_size,
	min_w: Props_min_w,
	max_w: Props_max_w,
	min_h: Props_min_h,
	max_h: Props_max_h,
	w_full: Props_w_full,
	h_full: Props_h_full,
	aspect: Props_aspect,
	absolute: Props_absolute,
	relative: Props_relative,
	top: Props_top,
	left: Props_left,
	inset_start: Props_inset_start,
	inset_end: Props_inset_end,
	right: Props_right,
	bottom: Props_bottom,
	-- Spacing.
	p: Props_p,
	px: Props_px,
	py: Props_py,
	pt: Props_pt,
	pr: Props_pr,
	pb: Props_pb,
	pl: Props_pl,
	ps: Props_ps,
	pe: Props_pe,
	m: Props_m,
	mx: Props_mx,
	my: Props_my,
	mt: Props_mt,
	mr: Props_mr,
	mb: Props_mb,
	ml: Props_ml,
	ms: Props_ms,
	me: Props_me,
	gap: Props_gap,
	gap_x: Props_gap_x,
	gap_y: Props_gap_y,
	-- Flex and grid.
	direction: Props_direction,
	dir: Props_dir,
	grow: Props_grow,
	shrink: Props_shrink,
	basis: Props_basis,
	flex_wrap: Props_flex_wrap,
	wrap_reverse: Props_wrap_reverse,
	items: Props_items,
	justify: Props_justify,
	self: Props_self,
	content: Props_content,
	grid: Props_grid,
	cols: Props_cols,
	rows: Props_rows,
	col_start: Props_col_start,
	row_start: Props_row_start,
	col_span: Props_col_span,
	row_span: Props_row_span,
	col_span_full: Props_col_span_full,
	row_span_full: Props_row_span_full,
	overflow: Props_overflow,
	overflow_x: Props_overflow_x,
	overflow_y: Props_overflow_y,
	hidden: Props_hidden,
	invisible: Props_invisible,
	-- Paint.
	bg: Props_bg,
	gradient: Props_gradient,
	color: Props_color,
	bg_image: Props_bg_image,
	bg_fit: Props_bg_fit,
	rounded: Props_rounded,
	corners: Props_corners,
	rounded_tl: Props_rounded_tl,
	rounded_tr: Props_rounded_tr,
	rounded_br: Props_rounded_br,
	rounded_bl: Props_rounded_bl,
	border: Props_border,
	border_sides: Props_border_sides,
	border_color: Props_border_color,
	border_over: Props_border_over,
	-- A function may return nil: a solid border.
	border_dash: Props_border_dash,
	outline: Props_outline,
	outline_color: Props_outline_color,
	outline_offset: Props_outline_offset,
	shadow: Props_shadow,
	inset_shadow: Props_inset_shadow,
	opacity: Props_opacity,
	brightness: Props_brightness,
	contrast: Props_contrast,
	saturate: Props_saturate,
	blur: Props_blur,
	backdrop_blur: Props_backdrop_blur,
	fade: Props_fade,
	fade_x: Props_fade_x,
	fade_sides: Props_fade_sides,
	fade_overflow: Props_fade_overflow,
	snap: Props_snap,
	clip_inset: Props_clip_inset,
	layer: Props_layer,
	fit: Props_fit,
	tint: Props_tint,
	cursor: Props_cursor,
	-- Text.
	text_size: Props_text_size,
	weight: Props_weight,
	bold: Props_bold,
	italic: Props_italic,
	align: Props_align,
	font: Props_font,
	wrap: Props_wrap,
	truncate: Props_truncate,
	lines: Props_lines,
	line_height: Props_line_height,
	letter_spacing: Props_letter_spacing,
	-- Tabular figures: every digit the same width.
	tabular: Props_tabular,
	-- Underlined text (inherited, as CSS's `text-decoration`), and how far
	-- below the baseline the line sits.
	underline: Props_underline,
	underline_offset: Props_underline_offset,
	-- Transforms: after layout, about `origin` (fractions of the node).
	x: Props_x,
	y: Props_y,
	scale: Props_scale,
	scale_x: Props_scale_x,
	scale_y: Props_scale_y,
	rotate: Props_rotate,
	skew: Props_skew,
	skew_x: Props_skew_x,
	skew_y: Props_skew_y,
	origin: Props_origin,
	-- The subtree's theme: a theme's name (`"zgx-light"`), or tokens to
	-- override (`{ accent = "#e5484d" }`, with `theme = name` to start from).
	theme: Props_theme,
	-- Identity, for selectors, refs and animations.
	id: Props_id,
	-- Space-separated class names.
	class: Props_class,
	-- Styles chosen from a `ui.variants` definition: `button { tone =
	-- "ghost" }`, or the recipe itself for its defaults. The element's own
	-- properties win over them.
	variants: Props_variants,
	ref: Props_ref,
	focusable: Props_focusable,
	-- What assistive technology takes it for (ARIA's `role`), instead of
	-- what it is: a clickable row as an "option", a row of buttons as a
	-- "toolbar". It keeps what it does.
	role: Props_role,
	-- A section title of this level, 1 to 6 (`h1`…`h6`): screen readers
	-- list headings and move between them.
	heading: Props_heading,
	-- Whether this option, tab or item is the chosen one (`aria-selected`).
	selected: Props_selected,
	-- Whether a switch, checkbox or radio of the app's own is on
	-- (`aria-checked`), and whether what a control opens shows
	-- (`aria-expanded`): a custom disclosure's.
	checked: Props_checked,
	expanded: Props_expanded,
	-- Announce changes to the text in it (`aria-live`): a count, a "Sent"
	-- note, a message arriving.
	live: Props_live,
	-- Read after its name, as a hint (`aria-description`).
	description: Props_description,
	-- What is wrong with its value (`aria-invalid`), followed: nil while it
	-- is fine. Assistive technology hears it with the field.
	error: Props_error,
	-- Left out of the accessibility tree with what it holds (`aria-hidden`):
	-- decoration, or what is said elsewhere.
	aria_hidden: Props_aria_hidden,
	-- One Tab stop for the focusable things inside; the arrows along this
	-- axis (and Home, End) move among them, as ARIA's composite widgets
	-- behave. `role = "listbox"` is vertical, "toolbar" and "tablist"
	-- horizontal, without asking.
	focus_group: Props_focus_group,
	-- A press on it (or on what inside takes no input) moves the window; a
	-- double click zooms it. Buttons and inputs inside keep their clicks.
	-- CSS pointer-events: "none" (or false) lets the pointer through to what
	-- is under the node (presses, hover, the wheel, drops, a native view);
	-- descendants with handlers still take it. A state or function follows.
	pointer_events: Props_pointer_events,
	window_drag: Props_window_drag,
	-- The pointer selects the text in it and everything it holds, as on a
	-- web page (CSS `user-select: text`): drag, double/triple press, Shift,
	-- ⌘A / Ctrl+A, ⌘C / Ctrl+C. `false` keeps a part out. Off unless set.
	selectable: Props_selectable,
	-- A selectable node's highlight and selected-text colours (CSS
	-- `::selection`), over the theme's `$selection` and `$selection.text`
	-- tokens; a state or function follows.
	selection_color: Props_selection_color,
	selected_text_color: Props_selected_text_color,
	-- Shown over it while the pointer rests on it (after 300 ms; at once
	-- within a second of another): text in the theme's look, or a node of
	-- the app's own. Above it, centred, unless `tooltip_placement` says.
	tooltip: Props_tooltip,
	tooltip_placement: Props_tooltip_placement,
	-- A hover card the pointer can move into and click in: it stays while
	-- the pointer is over the node or the card.
	tooltip_interactive: Props_tooltip_interactive,
	-- Milliseconds the pointer rests before it shows (300; once one
	-- shows, the next shows at once). Keyboard focus shows it too.
	tooltip_delay: Props_tooltip_delay,
	-- A condition (true disables), or the styles it shows while disabled.
	disabled: Props_disabled,
	hover: Props_hover,
	active: Props_active,
	focus: Props_focus,
	-- Size animation ("height", "width", "size"; `true` is "height") or
	-- position animation ("position"). A `layout_id` node mounting as
	-- another with the same id unmounts (or while it is mounted) moves and
	-- scales from that one's box.
	layout: Props_layout,
	layout_id: Props_layout_id,
	layout_ease: Props_layout_ease,
	-- Milliseconds.
	layout_duration: Props_layout_duration,
	layout_enter: Props_layout_enter,
	layout_clip: Props_layout_clip,
	-- The content shows through the bitmap's alpha: `mask = bitmap`, or
	-- `{ bitmap, w = …, h = …, x = …, y = …, sweep = … }`.
	mask: Props_mask,
	bg_shader: Props_bg_shader,
	-- Up to four vec4s: lists of numbers, or colors (0 to 1).
	shader_params: Props_shader_params,
	-- Events.
	on_click: Props_on_click,
	-- Primary pointer down and up.
	on_press: Props_on_press,
	on_release: Props_on_release,
	on_hover: Props_on_hover,
	on_pointer_move: Props_on_pointer_move,
	-- Return true to consume the key.
	on_key: Props_on_key,
	on_key_up: Props_on_key_up,
	on_focus: Props_on_focus,
	on_blur: Props_on_blur,
	-- Return true to take the gesture from the scroll views around it.
	on_wheel: Props_on_wheel,
	on_pinch: Props_on_pinch,
	on_mount: Props_on_mount,
	-- When layout gives it a new size, and at its first layout.
	on_resize: Props_on_resize,
	-- When any of it comes into view or all of it leaves (inside every
	-- clip and scroll viewport above it), and at its first layout: for
	-- building what only matters on screen.
	on_visible: Props_on_visible,
	-- Drag and drop within the window. Payloads are what `on_drag`
	-- returns: the script's own plain values, so they are `any` here.
	-- A source: `on_drag` returns the payload once the pointer moves 4
	-- points; `drag_preview` builds what follows the pointer.
	on_drag: Props_on_drag,
	drag_preview: Props_drag_preview,
	on_drag_end: Props_on_drag_end,
	-- What the drag carries to other apps as it leaves the window (macOS):
	-- Finder takes files, Mail text, a browser links.
	drag_export: Props_drag_export,
	-- Released outside every window of the app (macOS): a tab torn off its
	-- window. `grab_x`/`grab_y` are where the source was held.
	on_drag_outside: Props_on_drag_outside,
	-- The drag moves its window instead, with no preview (macOS): a
	-- window's only tab. Dropped on another window's destination it drops
	-- there; elsewhere the window stays where it was let go.
	drag_moves_window: Props_drag_moves_window,
	-- A destination: `drop_when` filters payloads (default: all).
	drop_when: Props_drop_when,
	on_drag_over: Props_on_drag_over,
	on_drag_leave: Props_on_drag_leave,
	on_drop: Props_on_drop,
	-- A secondary click (control-click on macOS) shows the returned
	-- `menu_item`s, `submenu`s and separators as a menu at the pointer;
	-- returning nil (or nothing) shows none.
	context_menu: Props_context_menu,
	-- The context menu panel's look: bg, rounded, p, gap, backdrop_blur,
	-- border, shadow, w, …
	context_menu_style: Props_context_menu_style,
	-- Open the context menu under the node, from its left edge (a tab's),
	-- rather than at the pointer.
	context_menu_below: Props_context_menu_below,
	[number]: never,
	-- Required unless `field` gives it.
	value: (State<boolean>)?,
	-- One of a `ui.form`'s fields (`form.fields.email`): its value, its
	-- leaving, its error (and, a one-line input, Enter submitting the form).
	field: FormField?,
	label: string?,
	-- Neither on nor off while true: a dash, and "mixed" to assistive
	-- technology (a "select all" box with some of its group on).
	mixed: State<boolean>?,
}
export type InputProps = {
	-- Size and position.
	w: Props_w,
	h: Props_h,
	size: Props_size,
	min_w: Props_min_w,
	max_w: Props_max_w,
	min_h: Props_min_h,
	max_h: Props_max_h,
	w_full: Props_w_full,
	h_full: Props_h_full,
	aspect: Props_aspect,
	absolute: Props_absolute,
	relative: Props_relative,
	top: Props_top,
	left: Props_left,
	inset_start: Props_inset_start,
	inset_end: Props_inset_end,
	right: Props_right,
	bottom: Props_bottom,
	-- Spacing.
	p: Props_p,
	px: Props_px,
	py: Props_py,
	pt: Props_pt,
	pr: Props_pr,
	pb: Props_pb,
	pl: Props_pl,
	ps: Props_ps,
	pe: Props_pe,
	m: Props_m,
	mx: Props_mx,
	my: Props_my,
	mt: Props_mt,
	mr: Props_mr,
	mb: Props_mb,
	ml: Props_ml,
	ms: Props_ms,
	me: Props_me,
	gap: Props_gap,
	gap_x: Props_gap_x,
	gap_y: Props_gap_y,
	-- Flex and grid.
	direction: Props_direction,
	dir: Props_dir,
	grow: Props_grow,
	shrink: Props_shrink,
	basis: Props_basis,
	flex_wrap: Props_flex_wrap,
	wrap_reverse: Props_wrap_reverse,
	items: Props_items,
	justify: Props_justify,
	self: Props_self,
	content: Props_content,
	grid: Props_grid,
	cols: Props_cols,
	rows: Props_rows,
	col_start: Props_col_start,
	row_start: Props_row_start,
	col_span: Props_col_span,
	row_span: Props_row_span,
	col_span_full: Props_col_span_full,
	row_span_full: Props_row_span_full,
	overflow: Props_overflow,
	overflow_x: Props_overflow_x,
	overflow_y: Props_overflow_y,
	hidden: Props_hidden,
	invisible: Props_invisible,
	-- Paint.
	bg: Props_bg,
	gradient: Props_gradient,
	color: Props_color,
	bg_image: Props_bg_image,
	bg_fit: Props_bg_fit,
	rounded: Props_rounded,
	corners: Props_corners,
	rounded_tl: Props_rounded_tl,
	rounded_tr: Props_rounded_tr,
	rounded_br: Props_rounded_br,
	rounded_bl: Props_rounded_bl,
	border: Props_border,
	border_sides: Props_border_sides,
	border_color: Props_border_color,
	border_over: Props_border_over,
	-- A function may return nil: a solid border.
	border_dash: Props_border_dash,
	outline: Props_outline,
	outline_color: Props_outline_color,
	outline_offset: Props_outline_offset,
	shadow: Props_shadow,
	inset_shadow: Props_inset_shadow,
	opacity: Props_opacity,
	brightness: Props_brightness,
	contrast: Props_contrast,
	saturate: Props_saturate,
	blur: Props_blur,
	backdrop_blur: Props_backdrop_blur,
	fade: Props_fade,
	fade_x: Props_fade_x,
	fade_sides: Props_fade_sides,
	fade_overflow: Props_fade_overflow,
	snap: Props_snap,
	clip_inset: Props_clip_inset,
	layer: Props_layer,
	fit: Props_fit,
	tint: Props_tint,
	cursor: Props_cursor,
	-- Text.
	text_size: Props_text_size,
	weight: Props_weight,
	bold: Props_bold,
	italic: Props_italic,
	align: Props_align,
	font: Props_font,
	wrap: Props_wrap,
	truncate: Props_truncate,
	lines: Props_lines,
	line_height: Props_line_height,
	letter_spacing: Props_letter_spacing,
	-- Tabular figures: every digit the same width.
	tabular: Props_tabular,
	-- Underlined text (inherited, as CSS's `text-decoration`), and how far
	-- below the baseline the line sits.
	underline: Props_underline,
	underline_offset: Props_underline_offset,
	-- Transforms: after layout, about `origin` (fractions of the node).
	x: Props_x,
	y: Props_y,
	scale: Props_scale,
	scale_x: Props_scale_x,
	scale_y: Props_scale_y,
	rotate: Props_rotate,
	skew: Props_skew,
	skew_x: Props_skew_x,
	skew_y: Props_skew_y,
	origin: Props_origin,
	-- The subtree's theme: a theme's name (`"zgx-light"`), or tokens to
	-- override (`{ accent = "#e5484d" }`, with `theme = name` to start from).
	theme: Props_theme,
	-- Identity, for selectors, refs and animations.
	id: Props_id,
	-- Space-separated class names.
	class: Props_class,
	-- Styles chosen from a `ui.variants` definition: `button { tone =
	-- "ghost" }`, or the recipe itself for its defaults. The element's own
	-- properties win over them.
	variants: Props_variants,
	ref: Props_ref,
	focusable: Props_focusable,
	-- What assistive technology takes it for (ARIA's `role`), instead of
	-- what it is: a clickable row as an "option", a row of buttons as a
	-- "toolbar". It keeps what it does.
	role: Props_role,
	-- A section title of this level, 1 to 6 (`h1`…`h6`): screen readers
	-- list headings and move between them.
	heading: Props_heading,
	-- Whether this option, tab or item is the chosen one (`aria-selected`).
	selected: Props_selected,
	-- Whether a switch, checkbox or radio of the app's own is on
	-- (`aria-checked`), and whether what a control opens shows
	-- (`aria-expanded`): a custom disclosure's.
	checked: Props_checked,
	expanded: Props_expanded,
	-- Announce changes to the text in it (`aria-live`): a count, a "Sent"
	-- note, a message arriving.
	live: Props_live,
	-- Read after its name, as a hint (`aria-description`).
	description: Props_description,
	-- What is wrong with its value (`aria-invalid`), followed: nil while it
	-- is fine. Assistive technology hears it with the field.
	error: Props_error,
	-- Left out of the accessibility tree with what it holds (`aria-hidden`):
	-- decoration, or what is said elsewhere.
	aria_hidden: Props_aria_hidden,
	-- One Tab stop for the focusable things inside; the arrows along this
	-- axis (and Home, End) move among them, as ARIA's composite widgets
	-- behave. `role = "listbox"` is vertical, "toolbar" and "tablist"
	-- horizontal, without asking.
	focus_group: Props_focus_group,
	-- A press on it (or on what inside takes no input) moves the window; a
	-- double click zooms it. Buttons and inputs inside keep their clicks.
	-- CSS pointer-events: "none" (or false) lets the pointer through to what
	-- is under the node (presses, hover, the wheel, drops, a native view);
	-- descendants with handlers still take it. A state or function follows.
	pointer_events: Props_pointer_events,
	window_drag: Props_window_drag,
	-- The pointer selects the text in it and everything it holds, as on a
	-- web page (CSS `user-select: text`): drag, double/triple press, Shift,
	-- ⌘A / Ctrl+A, ⌘C / Ctrl+C. `false` keeps a part out. Off unless set.
	selectable: Props_selectable,
	-- A selectable node's highlight and selected-text colours (CSS
	-- `::selection`), over the theme's `$selection` and `$selection.text`
	-- tokens; a state or function follows.
	selection_color: Props_selection_color,
	selected_text_color: Props_selected_text_color,
	-- Shown over it while the pointer rests on it (after 300 ms; at once
	-- within a second of another): text in the theme's look, or a node of
	-- the app's own. Above it, centred, unless `tooltip_placement` says.
	tooltip: Props_tooltip,
	tooltip_placement: Props_tooltip_placement,
	-- A hover card the pointer can move into and click in: it stays while
	-- the pointer is over the node or the card.
	tooltip_interactive: Props_tooltip_interactive,
	-- Milliseconds the pointer rests before it shows (300; once one
	-- shows, the next shows at once). Keyboard focus shows it too.
	tooltip_delay: Props_tooltip_delay,
	-- A condition (true disables), or the styles it shows while disabled.
	disabled: Props_disabled,
	hover: Props_hover,
	active: Props_active,
	focus: Props_focus,
	-- Size animation ("height", "width", "size"; `true` is "height") or
	-- position animation ("position"). A `layout_id` node mounting as
	-- another with the same id unmounts (or while it is mounted) moves and
	-- scales from that one's box.
	layout: Props_layout,
	layout_id: Props_layout_id,
	layout_ease: Props_layout_ease,
	-- Milliseconds.
	layout_duration: Props_layout_duration,
	layout_enter: Props_layout_enter,
	layout_clip: Props_layout_clip,
	-- The content shows through the bitmap's alpha: `mask = bitmap`, or
	-- `{ bitmap, w = …, h = …, x = …, y = …, sweep = … }`.
	mask: Props_mask,
	bg_shader: Props_bg_shader,
	-- Up to four vec4s: lists of numbers, or colors (0 to 1).
	shader_params: Props_shader_params,
	-- Events.
	on_click: Props_on_click,
	-- Primary pointer down and up.
	on_press: Props_on_press,
	on_release: Props_on_release,
	on_hover: Props_on_hover,
	on_pointer_move: Props_on_pointer_move,
	-- Return true to consume the key.
	on_key: Props_on_key,
	on_key_up: Props_on_key_up,
	on_focus: Props_on_focus,
	on_blur: Props_on_blur,
	-- Return true to take the gesture from the scroll views around it.
	on_wheel: Props_on_wheel,
	on_pinch: Props_on_pinch,
	on_mount: Props_on_mount,
	-- When layout gives it a new size, and at its first layout.
	on_resize: Props_on_resize,
	-- When any of it comes into view or all of it leaves (inside every
	-- clip and scroll viewport above it), and at its first layout: for
	-- building what only matters on screen.
	on_visible: Props_on_visible,
	-- Drag and drop within the window. Payloads are what `on_drag`
	-- returns: the script's own plain values, so they are `any` here.
	-- A source: `on_drag` returns the payload once the pointer moves 4
	-- points; `drag_preview` builds what follows the pointer.
	on_drag: Props_on_drag,
	drag_preview: Props_drag_preview,
	on_drag_end: Props_on_drag_end,
	-- What the drag carries to other apps as it leaves the window (macOS):
	-- Finder takes files, Mail text, a browser links.
	drag_export: Props_drag_export,
	-- Released outside every window of the app (macOS): a tab torn off its
	-- window. `grab_x`/`grab_y` are where the source was held.
	on_drag_outside: Props_on_drag_outside,
	-- The drag moves its window instead, with no preview (macOS): a
	-- window's only tab. Dropped on another window's destination it drops
	-- there; elsewhere the window stays where it was let go.
	drag_moves_window: Props_drag_moves_window,
	-- A destination: `drop_when` filters payloads (default: all).
	drop_when: Props_drop_when,
	on_drag_over: Props_on_drag_over,
	on_drag_leave: Props_on_drag_leave,
	on_drop: Props_on_drop,
	-- A secondary click (control-click on macOS) shows the returned
	-- `menu_item`s, `submenu`s and separators as a menu at the pointer;
	-- returning nil (or nothing) shows none.
	context_menu: Props_context_menu,
	-- The context menu panel's look: bg, rounded, p, gap, backdrop_blur,
	-- border, shadow, w, …
	context_menu_style: Props_context_menu_style,
	-- Open the context menu under the node, from its left edge (a tab's),
	-- rather than at the pointer.
	context_menu_below: Props_context_menu_below,
	[number]: never,
	-- A state, both ways; or, controlled as React's inputs are, a function
	-- (or text) it shows, edits going out by `on_change` and `on_commit`.
	-- Focused, it keeps what is being typed.
	-- Required unless `field` gives it.
	value: (State<string> | (() -> string | number) | string)?,
	-- One of a `ui.form`'s fields (`form.fields.email`): its value, its
	-- leaving, its error (and, a one-line input, Enter submitting the form).
	field: FormField?,
	-- Each edit's text (a controlled input).
	on_change: ((text: string) -> ...unknown)?,
	-- The text on Enter, or on leaving the input changed: where an
	-- inspector's field applies, and undo records.
	on_commit: ((text: string) -> ...unknown)?,
	label: string?,
	-- input only: several lines (textarea always has them).
	multiline: boolean?,
	-- Shown while the value is empty (never part of it), in the input's
	-- typography and `placeholder_color`.
	placeholder: Reactive<string>?,
	placeholder_color: Reactive<Color>?,
	-- Focusing it selects all its text (an address bar).
	select_on_focus: boolean?,
	-- A password field: its text shows as a bullet per character, and
	-- copying or cutting it gives nothing.
	secure: boolean?,
	-- Check its spelling as it is typed (HTML's `spellcheck`): the system's
	-- checker marks misspelled words with a dotted line, and its context
	-- menu offers guesses, Learn Spelling and Ignore Spelling. Not in a
	-- password field. A state or function follows it (a setting).
	spellcheck: (boolean | Readable<boolean> | (() -> boolean))?,
	-- Several lines only: numbered in a gutter at the left (a code editor).
	line_numbers: boolean?,
	-- Several lines only: a line's colours, asked for the lines in view and
	-- again only for one that changes. Given the line and what the line
	-- before left (0 before the first), it returns spans `{ from, to,
	-- color }` (bytes counted from 1, as `string.find` gives them; `color`
	-- a colour or `"$token"`) and what the line leaves the next (a block
	-- comment or string running on: any number of the highlighter's). It
	-- must depend on nothing but its arguments.
	highlight: ((line: string, state: number) -> ({ { any } }?, number?))?,
	-- Several lines only: ranges of the whole text marked behind it (a
	-- search's matches), `{ { from, to, color? }, … }` in bytes counted
	-- from 1; only those in view are drawn.
	decorations: (State<any> | (() -> { { any } }))?,
	-- Several lines only: a search's matches marked behind the text, found
	-- by the editor where the text is held; `current` (from 1) stronger.
	find: {
		query: Reactive<string>,
		case: Reactive<boolean>?,
		current: Reactive<number>?,
		limit: number?,
		color: Reactive<Color>?,
		current_color: Reactive<Color>?,
	}?,
	-- Several lines only: the colour behind the caret's row.
	current_line: Reactive<Color>?,
	-- Several lines only: what Tab inserts (`"    "`), instead of moving
	-- focus; with lines selected it indents each, Shift+Tab outdents.
	tab: string?,
	-- Where the caret is whenever it moves: its line and column from 1 (the
	-- column in characters) and the selection's anchor and focus in bytes
	-- from 0 (as `selection_bytes`).
	on_caret: ((line: number, column: number, anchor: number, focus: number) -> ())?,
}
-- `value` from 0 to 1, or nil: indeterminate (work of unknown size), a
-- stretch of the bar sweeping across.
export type ProgressProps = {
	-- Size and position.
	w: Props_w,
	h: Props_h,
	size: Props_size,
	min_w: Props_min_w,
	max_w: Props_max_w,
	min_h: Props_min_h,
	max_h: Props_max_h,
	w_full: Props_w_full,
	h_full: Props_h_full,
	aspect: Props_aspect,
	absolute: Props_absolute,
	relative: Props_relative,
	top: Props_top,
	left: Props_left,
	inset_start: Props_inset_start,
	inset_end: Props_inset_end,
	right: Props_right,
	bottom: Props_bottom,
	-- Spacing.
	p: Props_p,
	px: Props_px,
	py: Props_py,
	pt: Props_pt,
	pr: Props_pr,
	pb: Props_pb,
	pl: Props_pl,
	ps: Props_ps,
	pe: Props_pe,
	m: Props_m,
	mx: Props_mx,
	my: Props_my,
	mt: Props_mt,
	mr: Props_mr,
	mb: Props_mb,
	ml: Props_ml,
	ms: Props_ms,
	me: Props_me,
	gap: Props_gap,
	gap_x: Props_gap_x,
	gap_y: Props_gap_y,
	-- Flex and grid.
	direction: Props_direction,
	dir: Props_dir,
	grow: Props_grow,
	shrink: Props_shrink,
	basis: Props_basis,
	flex_wrap: Props_flex_wrap,
	wrap_reverse: Props_wrap_reverse,
	items: Props_items,
	justify: Props_justify,
	self: Props_self,
	content: Props_content,
	grid: Props_grid,
	cols: Props_cols,
	rows: Props_rows,
	col_start: Props_col_start,
	row_start: Props_row_start,
	col_span: Props_col_span,
	row_span: Props_row_span,
	col_span_full: Props_col_span_full,
	row_span_full: Props_row_span_full,
	overflow: Props_overflow,
	overflow_x: Props_overflow_x,
	overflow_y: Props_overflow_y,
	hidden: Props_hidden,
	invisible: Props_invisible,
	-- Paint.
	bg: Props_bg,
	gradient: Props_gradient,
	color: Props_color,
	bg_image: Props_bg_image,
	bg_fit: Props_bg_fit,
	rounded: Props_rounded,
	corners: Props_corners,
	rounded_tl: Props_rounded_tl,
	rounded_tr: Props_rounded_tr,
	rounded_br: Props_rounded_br,
	rounded_bl: Props_rounded_bl,
	border: Props_border,
	border_sides: Props_border_sides,
	border_color: Props_border_color,
	border_over: Props_border_over,
	-- A function may return nil: a solid border.
	border_dash: Props_border_dash,
	outline: Props_outline,
	outline_color: Props_outline_color,
	outline_offset: Props_outline_offset,
	shadow: Props_shadow,
	inset_shadow: Props_inset_shadow,
	opacity: Props_opacity,
	brightness: Props_brightness,
	contrast: Props_contrast,
	saturate: Props_saturate,
	blur: Props_blur,
	backdrop_blur: Props_backdrop_blur,
	fade: Props_fade,
	fade_x: Props_fade_x,
	fade_sides: Props_fade_sides,
	fade_overflow: Props_fade_overflow,
	snap: Props_snap,
	clip_inset: Props_clip_inset,
	layer: Props_layer,
	fit: Props_fit,
	tint: Props_tint,
	cursor: Props_cursor,
	-- Text.
	text_size: Props_text_size,
	weight: Props_weight,
	bold: Props_bold,
	italic: Props_italic,
	align: Props_align,
	font: Props_font,
	wrap: Props_wrap,
	truncate: Props_truncate,
	lines: Props_lines,
	line_height: Props_line_height,
	letter_spacing: Props_letter_spacing,
	-- Tabular figures: every digit the same width.
	tabular: Props_tabular,
	-- Underlined text (inherited, as CSS's `text-decoration`), and how far
	-- below the baseline the line sits.
	underline: Props_underline,
	underline_offset: Props_underline_offset,
	-- Transforms: after layout, about `origin` (fractions of the node).
	x: Props_x,
	y: Props_y,
	scale: Props_scale,
	scale_x: Props_scale_x,
	scale_y: Props_scale_y,
	rotate: Props_rotate,
	skew: Props_skew,
	skew_x: Props_skew_x,
	skew_y: Props_skew_y,
	origin: Props_origin,
	-- The subtree's theme: a theme's name (`"zgx-light"`), or tokens to
	-- override (`{ accent = "#e5484d" }`, with `theme = name` to start from).
	theme: Props_theme,
	-- Identity, for selectors, refs and animations.
	id: Props_id,
	-- Space-separated class names.
	class: Props_class,
	-- Styles chosen from a `ui.variants` definition: `button { tone =
	-- "ghost" }`, or the recipe itself for its defaults. The element's own
	-- properties win over them.
	variants: Props_variants,
	ref: Props_ref,
	focusable: Props_focusable,
	-- What assistive technology takes it for (ARIA's `role`), instead of
	-- what it is: a clickable row as an "option", a row of buttons as a
	-- "toolbar". It keeps what it does.
	role: Props_role,
	-- A section title of this level, 1 to 6 (`h1`…`h6`): screen readers
	-- list headings and move between them.
	heading: Props_heading,
	-- Whether this option, tab or item is the chosen one (`aria-selected`).
	selected: Props_selected,
	-- Whether a switch, checkbox or radio of the app's own is on
	-- (`aria-checked`), and whether what a control opens shows
	-- (`aria-expanded`): a custom disclosure's.
	checked: Props_checked,
	expanded: Props_expanded,
	-- Announce changes to the text in it (`aria-live`): a count, a "Sent"
	-- note, a message arriving.
	live: Props_live,
	-- Read after its name, as a hint (`aria-description`).
	description: Props_description,
	-- What is wrong with its value (`aria-invalid`), followed: nil while it
	-- is fine. Assistive technology hears it with the field.
	error: Props_error,
	-- Left out of the accessibility tree with what it holds (`aria-hidden`):
	-- decoration, or what is said elsewhere.
	aria_hidden: Props_aria_hidden,
	-- One Tab stop for the focusable things inside; the arrows along this
	-- axis (and Home, End) move among them, as ARIA's composite widgets
	-- behave. `role = "listbox"` is vertical, "toolbar" and "tablist"
	-- horizontal, without asking.
	focus_group: Props_focus_group,
	-- A press on it (or on what inside takes no input) moves the window; a
	-- double click zooms it. Buttons and inputs inside keep their clicks.
	-- CSS pointer-events: "none" (or false) lets the pointer through to what
	-- is under the node (presses, hover, the wheel, drops, a native view);
	-- descendants with handlers still take it. A state or function follows.
	pointer_events: Props_pointer_events,
	window_drag: Props_window_drag,
	-- The pointer selects the text in it and everything it holds, as on a
	-- web page (CSS `user-select: text`): drag, double/triple press, Shift,
	-- ⌘A / Ctrl+A, ⌘C / Ctrl+C. `false` keeps a part out. Off unless set.
	selectable: Props_selectable,
	-- A selectable node's highlight and selected-text colours (CSS
	-- `::selection`), over the theme's `$selection` and `$selection.text`
	-- tokens; a state or function follows.
	selection_color: Props_selection_color,
	selected_text_color: Props_selected_text_color,
	-- Shown over it while the pointer rests on it (after 300 ms; at once
	-- within a second of another): text in the theme's look, or a node of
	-- the app's own. Above it, centred, unless `tooltip_placement` says.
	tooltip: Props_tooltip,
	tooltip_placement: Props_tooltip_placement,
	-- A hover card the pointer can move into and click in: it stays while
	-- the pointer is over the node or the card.
	tooltip_interactive: Props_tooltip_interactive,
	-- Milliseconds the pointer rests before it shows (300; once one
	-- shows, the next shows at once). Keyboard focus shows it too.
	tooltip_delay: Props_tooltip_delay,
	-- A condition (true disables), or the styles it shows while disabled.
	disabled: Props_disabled,
	hover: Props_hover,
	active: Props_active,
	focus: Props_focus,
	-- Size animation ("height", "width", "size"; `true` is "height") or
	-- position animation ("position"). A `layout_id` node mounting as
	-- another with the same id unmounts (or while it is mounted) moves and
	-- scales from that one's box.
	layout: Props_layout,
	layout_id: Props_layout_id,
	layout_ease: Props_layout_ease,
	-- Milliseconds.
	layout_duration: Props_layout_duration,
	layout_enter: Props_layout_enter,
	layout_clip: Props_layout_clip,
	-- The content shows through the bitmap's alpha: `mask = bitmap`, or
	-- `{ bitmap, w = …, h = …, x = …, y = …, sweep = … }`.
	mask: Props_mask,
	bg_shader: Props_bg_shader,
	-- Up to four vec4s: lists of numbers, or colors (0 to 1).
	shader_params: Props_shader_params,
	-- Events.
	on_click: Props_on_click,
	-- Primary pointer down and up.
	on_press: Props_on_press,
	on_release: Props_on_release,
	on_hover: Props_on_hover,
	on_pointer_move: Props_on_pointer_move,
	-- Return true to consume the key.
	on_key: Props_on_key,
	on_key_up: Props_on_key_up,
	on_focus: Props_on_focus,
	on_blur: Props_on_blur,
	-- Return true to take the gesture from the scroll views around it.
	on_wheel: Props_on_wheel,
	on_pinch: Props_on_pinch,
	on_mount: Props_on_mount,
	-- When layout gives it a new size, and at its first layout.
	on_resize: Props_on_resize,
	-- When any of it comes into view or all of it leaves (inside every
	-- clip and scroll viewport above it), and at its first layout: for
	-- building what only matters on screen.
	on_visible: Props_on_visible,
	-- Drag and drop within the window. Payloads are what `on_drag`
	-- returns: the script's own plain values, so they are `any` here.
	-- A source: `on_drag` returns the payload once the pointer moves 4
	-- points; `drag_preview` builds what follows the pointer.
	on_drag: Props_on_drag,
	drag_preview: Props_drag_preview,
	on_drag_end: Props_on_drag_end,
	-- What the drag carries to other apps as it leaves the window (macOS):
	-- Finder takes files, Mail text, a browser links.
	drag_export: Props_drag_export,
	-- Released outside every window of the app (macOS): a tab torn off its
	-- window. `grab_x`/`grab_y` are where the source was held.
	on_drag_outside: Props_on_drag_outside,
	-- The drag moves its window instead, with no preview (macOS): a
	-- window's only tab. Dropped on another window's destination it drops
	-- there; elsewhere the window stays where it was let go.
	drag_moves_window: Props_drag_moves_window,
	-- A destination: `drop_when` filters payloads (default: all).
	drop_when: Props_drop_when,
	on_drag_over: Props_on_drag_over,
	on_drag_leave: Props_on_drag_leave,
	on_drop: Props_on_drop,
	-- A secondary click (control-click on macOS) shows the returned
	-- `menu_item`s, `submenu`s and separators as a menu at the pointer;
	-- returning nil (or nothing) shows none.
	context_menu: Props_context_menu,
	-- The context menu panel's look: bg, rounded, p, gap, backdrop_blur,
	-- border, shadow, w, …
	context_menu_style: Props_context_menu_style,
	-- Open the context menu under the node, from its left edge (a tab's),
	-- rather than at the pointer.
	context_menu_below: Props_context_menu_below,
	[number]: never,
	value: State<number> | State<number?>,
	label: string?,
}
export type SliderProps = {
	-- Size and position.
	w: Props_w,
	h: Props_h,
	size: Props_size,
	min_w: Props_min_w,
	max_w: Props_max_w,
	min_h: Props_min_h,
	max_h: Props_max_h,
	w_full: Props_w_full,
	h_full: Props_h_full,
	aspect: Props_aspect,
	absolute: Props_absolute,
	relative: Props_relative,
	top: Props_top,
	left: Props_left,
	inset_start: Props_inset_start,
	inset_end: Props_inset_end,
	right: Props_right,
	bottom: Props_bottom,
	-- Spacing.
	p: Props_p,
	px: Props_px,
	py: Props_py,
	pt: Props_pt,
	pr: Props_pr,
	pb: Props_pb,
	pl: Props_pl,
	ps: Props_ps,
	pe: Props_pe,
	m: Props_m,
	mx: Props_mx,
	my: Props_my,
	mt: Props_mt,
	mr: Props_mr,
	mb: Props_mb,
	ml: Props_ml,
	ms: Props_ms,
	me: Props_me,
	gap: Props_gap,
	gap_x: Props_gap_x,
	gap_y: Props_gap_y,
	-- Flex and grid.
	direction: Props_direction,
	dir: Props_dir,
	grow: Props_grow,
	shrink: Props_shrink,
	basis: Props_basis,
	flex_wrap: Props_flex_wrap,
	wrap_reverse: Props_wrap_reverse,
	items: Props_items,
	justify: Props_justify,
	self: Props_self,
	content: Props_content,
	grid: Props_grid,
	cols: Props_cols,
	rows: Props_rows,
	col_start: Props_col_start,
	row_start: Props_row_start,
	col_span: Props_col_span,
	row_span: Props_row_span,
	col_span_full: Props_col_span_full,
	row_span_full: Props_row_span_full,
	overflow: Props_overflow,
	overflow_x: Props_overflow_x,
	overflow_y: Props_overflow_y,
	hidden: Props_hidden,
	invisible: Props_invisible,
	-- Paint.
	bg: Props_bg,
	gradient: Props_gradient,
	color: Props_color,
	bg_image: Props_bg_image,
	bg_fit: Props_bg_fit,
	rounded: Props_rounded,
	corners: Props_corners,
	rounded_tl: Props_rounded_tl,
	rounded_tr: Props_rounded_tr,
	rounded_br: Props_rounded_br,
	rounded_bl: Props_rounded_bl,
	border: Props_border,
	border_sides: Props_border_sides,
	border_color: Props_border_color,
	border_over: Props_border_over,
	-- A function may return nil: a solid border.
	border_dash: Props_border_dash,
	outline: Props_outline,
	outline_color: Props_outline_color,
	outline_offset: Props_outline_offset,
	shadow: Props_shadow,
	inset_shadow: Props_inset_shadow,
	opacity: Props_opacity,
	brightness: Props_brightness,
	contrast: Props_contrast,
	saturate: Props_saturate,
	blur: Props_blur,
	backdrop_blur: Props_backdrop_blur,
	fade: Props_fade,
	fade_x: Props_fade_x,
	fade_sides: Props_fade_sides,
	fade_overflow: Props_fade_overflow,
	snap: Props_snap,
	clip_inset: Props_clip_inset,
	layer: Props_layer,
	fit: Props_fit,
	tint: Props_tint,
	cursor: Props_cursor,
	-- Text.
	text_size: Props_text_size,
	weight: Props_weight,
	bold: Props_bold,
	italic: Props_italic,
	align: Props_align,
	font: Props_font,
	wrap: Props_wrap,
	truncate: Props_truncate,
	lines: Props_lines,
	line_height: Props_line_height,
	letter_spacing: Props_letter_spacing,
	-- Tabular figures: every digit the same width.
	tabular: Props_tabular,
	-- Underlined text (inherited, as CSS's `text-decoration`), and how far
	-- below the baseline the line sits.
	underline: Props_underline,
	underline_offset: Props_underline_offset,
	-- Transforms: after layout, about `origin` (fractions of the node).
	x: Props_x,
	y: Props_y,
	scale: Props_scale,
	scale_x: Props_scale_x,
	scale_y: Props_scale_y,
	rotate: Props_rotate,
	skew: Props_skew,
	skew_x: Props_skew_x,
	skew_y: Props_skew_y,
	origin: Props_origin,
	-- The subtree's theme: a theme's name (`"zgx-light"`), or tokens to
	-- override (`{ accent = "#e5484d" }`, with `theme = name` to start from).
	theme: Props_theme,
	-- Identity, for selectors, refs and animations.
	id: Props_id,
	-- Space-separated class names.
	class: Props_class,
	-- Styles chosen from a `ui.variants` definition: `button { tone =
	-- "ghost" }`, or the recipe itself for its defaults. The element's own
	-- properties win over them.
	variants: Props_variants,
	ref: Props_ref,
	focusable: Props_focusable,
	-- What assistive technology takes it for (ARIA's `role`), instead of
	-- what it is: a clickable row as an "option", a row of buttons as a
	-- "toolbar". It keeps what it does.
	role: Props_role,
	-- A section title of this level, 1 to 6 (`h1`…`h6`): screen readers
	-- list headings and move between them.
	heading: Props_heading,
	-- Whether this option, tab or item is the chosen one (`aria-selected`).
	selected: Props_selected,
	-- Whether a switch, checkbox or radio of the app's own is on
	-- (`aria-checked`), and whether what a control opens shows
	-- (`aria-expanded`): a custom disclosure's.
	checked: Props_checked,
	expanded: Props_expanded,
	-- Announce changes to the text in it (`aria-live`): a count, a "Sent"
	-- note, a message arriving.
	live: Props_live,
	-- Read after its name, as a hint (`aria-description`).
	description: Props_description,
	-- What is wrong with its value (`aria-invalid`), followed: nil while it
	-- is fine. Assistive technology hears it with the field.
	error: Props_error,
	-- Left out of the accessibility tree with what it holds (`aria-hidden`):
	-- decoration, or what is said elsewhere.
	aria_hidden: Props_aria_hidden,
	-- One Tab stop for the focusable things inside; the arrows along this
	-- axis (and Home, End) move among them, as ARIA's composite widgets
	-- behave. `role = "listbox"` is vertical, "toolbar" and "tablist"
	-- horizontal, without asking.
	focus_group: Props_focus_group,
	-- A press on it (or on what inside takes no input) moves the window; a
	-- double click zooms it. Buttons and inputs inside keep their clicks.
	-- CSS pointer-events: "none" (or false) lets the pointer through to what
	-- is under the node (presses, hover, the wheel, drops, a native view);
	-- descendants with handlers still take it. A state or function follows.
	pointer_events: Props_pointer_events,
	window_drag: Props_window_drag,
	-- The pointer selects the text in it and everything it holds, as on a
	-- web page (CSS `user-select: text`): drag, double/triple press, Shift,
	-- ⌘A / Ctrl+A, ⌘C / Ctrl+C. `false` keeps a part out. Off unless set.
	selectable: Props_selectable,
	-- A selectable node's highlight and selected-text colours (CSS
	-- `::selection`), over the theme's `$selection` and `$selection.text`
	-- tokens; a state or function follows.
	selection_color: Props_selection_color,
	selected_text_color: Props_selected_text_color,
	-- Shown over it while the pointer rests on it (after 300 ms; at once
	-- within a second of another): text in the theme's look, or a node of
	-- the app's own. Above it, centred, unless `tooltip_placement` says.
	tooltip: Props_tooltip,
	tooltip_placement: Props_tooltip_placement,
	-- A hover card the pointer can move into and click in: it stays while
	-- the pointer is over the node or the card.
	tooltip_interactive: Props_tooltip_interactive,
	-- Milliseconds the pointer rests before it shows (300; once one
	-- shows, the next shows at once). Keyboard focus shows it too.
	tooltip_delay: Props_tooltip_delay,
	-- A condition (true disables), or the styles it shows while disabled.
	disabled: Props_disabled,
	hover: Props_hover,
	active: Props_active,
	focus: Props_focus,
	-- Size animation ("height", "width", "size"; `true` is "height") or
	-- position animation ("position"). A `layout_id` node mounting as
	-- another with the same id unmounts (or while it is mounted) moves and
	-- scales from that one's box.
	layout: Props_layout,
	layout_id: Props_layout_id,
	layout_ease: Props_layout_ease,
	-- Milliseconds.
	layout_duration: Props_layout_duration,
	layout_enter: Props_layout_enter,
	layout_clip: Props_layout_clip,
	-- The content shows through the bitmap's alpha: `mask = bitmap`, or
	-- `{ bitmap, w = …, h = …, x = …, y = …, sweep = … }`.
	mask: Props_mask,
	bg_shader: Props_bg_shader,
	-- Up to four vec4s: lists of numbers, or colors (0 to 1).
	shader_params: Props_shader_params,
	-- Events.
	on_click: Props_on_click,
	-- Primary pointer down and up.
	on_press: Props_on_press,
	on_release: Props_on_release,
	on_hover: Props_on_hover,
	on_pointer_move: Props_on_pointer_move,
	-- Return true to consume the key.
	on_key: Props_on_key,
	on_key_up: Props_on_key_up,
	on_focus: Props_on_focus,
	on_blur: Props_on_blur,
	-- Return true to take the gesture from the scroll views around it.
	on_wheel: Props_on_wheel,
	on_pinch: Props_on_pinch,
	on_mount: Props_on_mount,
	-- When layout gives it a new size, and at its first layout.
	on_resize: Props_on_resize,
	-- When any of it comes into view or all of it leaves (inside every
	-- clip and scroll viewport above it), and at its first layout: for
	-- building what only matters on screen.
	on_visible: Props_on_visible,
	-- Drag and drop within the window. Payloads are what `on_drag`
	-- returns: the script's own plain values, so they are `any` here.
	-- A source: `on_drag` returns the payload once the pointer moves 4
	-- points; `drag_preview` builds what follows the pointer.
	on_drag: Props_on_drag,
	drag_preview: Props_drag_preview,
	on_drag_end: Props_on_drag_end,
	-- What the drag carries to other apps as it leaves the window (macOS):
	-- Finder takes files, Mail text, a browser links.
	drag_export: Props_drag_export,
	-- Released outside every window of the app (macOS): a tab torn off its
	-- window. `grab_x`/`grab_y` are where the source was held.
	on_drag_outside: Props_on_drag_outside,
	-- The drag moves its window instead, with no preview (macOS): a
	-- window's only tab. Dropped on another window's destination it drops
	-- there; elsewhere the window stays where it was let go.
	drag_moves_window: Props_drag_moves_window,
	-- A destination: `drop_when` filters payloads (default: all).
	drop_when: Props_drop_when,
	on_drag_over: Props_on_drag_over,
	on_drag_leave: Props_on_drag_leave,
	on_drop: Props_on_drop,
	-- A secondary click (control-click on macOS) shows the returned
	-- `menu_item`s, `submenu`s and separators as a menu at the pointer;
	-- returning nil (or nothing) shows none.
	context_menu: Props_context_menu,
	-- The context menu panel's look: bg, rounded, p, gap, backdrop_blur,
	-- border, shadow, w, …
	context_menu_style: Props_context_menu_style,
	-- Open the context menu under the node, from its left edge (a tab's),
	-- rather than at the pointer.
	context_menu_below: Props_context_menu_below,
	[number]: never,
	-- Required unless `field` gives it.
	value: (State<number>)?,
	-- One of a `ui.form`'s fields (`form.fields.email`): its value, its
	-- leaving, its error (and, a one-line input, Enter submitting the form).
	field: FormField?,
	label: string?,
	-- 0 and 1 by default; min < max.
	min: number?,
	max: number?,
	-- What an arrow moves (ten with Shift, or Page Up and Down), and what
	-- a drag lands on; a hundredth of the range without one.
	step: number?,
}
-- One of a few options side by side (AppKit's segmented control; a radio
-- group to assistive technology): the arrows move the choice.
export type SegmentedOption = string | number | { label: string, value: any? }
export type SegmentedProps = {
	-- Size and position.
	w: Props_w,
	h: Props_h,
	size: Props_size,
	min_w: Props_min_w,
	max_w: Props_max_w,
	min_h: Props_min_h,
	max_h: Props_max_h,
	w_full: Props_w_full,
	h_full: Props_h_full,
	aspect: Props_aspect,
	absolute: Props_absolute,
	relative: Props_relative,
	top: Props_top,
	left: Props_left,
	inset_start: Props_inset_start,
	inset_end: Props_inset_end,
	right: Props_right,
	bottom: Props_bottom,
	-- Spacing.
	p: Props_p,
	px: Props_px,
	py: Props_py,
	pt: Props_pt,
	pr: Props_pr,
	pb: Props_pb,
	pl: Props_pl,
	ps: Props_ps,
	pe: Props_pe,
	m: Props_m,
	mx: Props_mx,
	my: Props_my,
	mt: Props_mt,
	mr: Props_mr,
	mb: Props_mb,
	ml: Props_ml,
	ms: Props_ms,
	me: Props_me,
	gap: Props_gap,
	gap_x: Props_gap_x,
	gap_y: Props_gap_y,
	-- Flex and grid.
	direction: Props_direction,
	dir: Props_dir,
	grow: Props_grow,
	shrink: Props_shrink,
	basis: Props_basis,
	flex_wrap: Props_flex_wrap,
	wrap_reverse: Props_wrap_reverse,
	items: Props_items,
	justify: Props_justify,
	self: Props_self,
	content: Props_content,
	grid: Props_grid,
	cols: Props_cols,
	rows: Props_rows,
	col_start: Props_col_start,
	row_start: Props_row_start,
	col_span: Props_col_span,
	row_span: Props_row_span,
	col_span_full: Props_col_span_full,
	row_span_full: Props_row_span_full,
	overflow: Props_overflow,
	overflow_x: Props_overflow_x,
	overflow_y: Props_overflow_y,
	hidden: Props_hidden,
	invisible: Props_invisible,
	-- Paint.
	bg: Props_bg,
	gradient: Props_gradient,
	color: Props_color,
	bg_image: Props_bg_image,
	bg_fit: Props_bg_fit,
	rounded: Props_rounded,
	corners: Props_corners,
	rounded_tl: Props_rounded_tl,
	rounded_tr: Props_rounded_tr,
	rounded_br: Props_rounded_br,
	rounded_bl: Props_rounded_bl,
	border: Props_border,
	border_sides: Props_border_sides,
	border_color: Props_border_color,
	border_over: Props_border_over,
	-- A function may return nil: a solid border.
	border_dash: Props_border_dash,
	outline: Props_outline,
	outline_color: Props_outline_color,
	outline_offset: Props_outline_offset,
	shadow: Props_shadow,
	inset_shadow: Props_inset_shadow,
	opacity: Props_opacity,
	brightness: Props_brightness,
	contrast: Props_contrast,
	saturate: Props_saturate,
	blur: Props_blur,
	backdrop_blur: Props_backdrop_blur,
	fade: Props_fade,
	fade_x: Props_fade_x,
	fade_sides: Props_fade_sides,
	fade_overflow: Props_fade_overflow,
	snap: Props_snap,
	clip_inset: Props_clip_inset,
	layer: Props_layer,
	fit: Props_fit,
	tint: Props_tint,
	cursor: Props_cursor,
	-- Text.
	text_size: Props_text_size,
	weight: Props_weight,
	bold: Props_bold,
	italic: Props_italic,
	align: Props_align,
	font: Props_font,
	wrap: Props_wrap,
	truncate: Props_truncate,
	lines: Props_lines,
	line_height: Props_line_height,
	letter_spacing: Props_letter_spacing,
	-- Tabular figures: every digit the same width.
	tabular: Props_tabular,
	-- Underlined text (inherited, as CSS's `text-decoration`), and how far
	-- below the baseline the line sits.
	underline: Props_underline,
	underline_offset: Props_underline_offset,
	-- Transforms: after layout, about `origin` (fractions of the node).
	x: Props_x,
	y: Props_y,
	scale: Props_scale,
	scale_x: Props_scale_x,
	scale_y: Props_scale_y,
	rotate: Props_rotate,
	skew: Props_skew,
	skew_x: Props_skew_x,
	skew_y: Props_skew_y,
	origin: Props_origin,
	-- The subtree's theme: a theme's name (`"zgx-light"`), or tokens to
	-- override (`{ accent = "#e5484d" }`, with `theme = name` to start from).
	theme: Props_theme,
	-- Identity, for selectors, refs and animations.
	id: Props_id,
	-- Space-separated class names.
	class: Props_class,
	-- Styles chosen from a `ui.variants` definition: `button { tone =
	-- "ghost" }`, or the recipe itself for its defaults. The element's own
	-- properties win over them.
	variants: Props_variants,
	ref: Props_ref,
	focusable: Props_focusable,
	-- What assistive technology takes it for (ARIA's `role`), instead of
	-- what it is: a clickable row as an "option", a row of buttons as a
	-- "toolbar". It keeps what it does.
	role: Props_role,
	-- A section title of this level, 1 to 6 (`h1`…`h6`): screen readers
	-- list headings and move between them.
	heading: Props_heading,
	-- Whether this option, tab or item is the chosen one (`aria-selected`).
	selected: Props_selected,
	-- Whether a switch, checkbox or radio of the app's own is on
	-- (`aria-checked`), and whether what a control opens shows
	-- (`aria-expanded`): a custom disclosure's.
	checked: Props_checked,
	expanded: Props_expanded,
	-- Announce changes to the text in it (`aria-live`): a count, a "Sent"
	-- note, a message arriving.
	live: Props_live,
	-- Read after its name, as a hint (`aria-description`).
	description: Props_description,
	-- What is wrong with its value (`aria-invalid`), followed: nil while it
	-- is fine. Assistive technology hears it with the field.
	error: Props_error,
	-- Left out of the accessibility tree with what it holds (`aria-hidden`):
	-- decoration, or what is said elsewhere.
	aria_hidden: Props_aria_hidden,
	-- One Tab stop for the focusable things inside; the arrows along this
	-- axis (and Home, End) move among them, as ARIA's composite widgets
	-- behave. `role = "listbox"` is vertical, "toolbar" and "tablist"
	-- horizontal, without asking.
	focus_group: Props_focus_group,
	-- A press on it (or on what inside takes no input) moves the window; a
	-- double click zooms it. Buttons and inputs inside keep their clicks.
	-- CSS pointer-events: "none" (or false) lets the pointer through to what
	-- is under the node (presses, hover, the wheel, drops, a native view);
	-- descendants with handlers still take it. A state or function follows.
	pointer_events: Props_pointer_events,
	window_drag: Props_window_drag,
	-- The pointer selects the text in it and everything it holds, as on a
	-- web page (CSS `user-select: text`): drag, double/triple press, Shift,
	-- ⌘A / Ctrl+A, ⌘C / Ctrl+C. `false` keeps a part out. Off unless set.
	selectable: Props_selectable,
	-- A selectable node's highlight and selected-text colours (CSS
	-- `::selection`), over the theme's `$selection` and `$selection.text`
	-- tokens; a state or function follows.
	selection_color: Props_selection_color,
	selected_text_color: Props_selected_text_color,
	-- Shown over it while the pointer rests on it (after 300 ms; at once
	-- within a second of another): text in the theme's look, or a node of
	-- the app's own. Above it, centred, unless `tooltip_placement` says.
	tooltip: Props_tooltip,
	tooltip_placement: Props_tooltip_placement,
	-- A hover card the pointer can move into and click in: it stays while
	-- the pointer is over the node or the card.
	tooltip_interactive: Props_tooltip_interactive,
	-- Milliseconds the pointer rests before it shows (300; once one
	-- shows, the next shows at once). Keyboard focus shows it too.
	tooltip_delay: Props_tooltip_delay,
	-- A condition (true disables), or the styles it shows while disabled.
	disabled: Props_disabled,
	hover: Props_hover,
	active: Props_active,
	focus: Props_focus,
	-- Size animation ("height", "width", "size"; `true` is "height") or
	-- position animation ("position"). A `layout_id` node mounting as
	-- another with the same id unmounts (or while it is mounted) moves and
	-- scales from that one's box.
	layout: Props_layout,
	layout_id: Props_layout_id,
	layout_ease: Props_layout_ease,
	-- Milliseconds.
	layout_duration: Props_layout_duration,
	layout_enter: Props_layout_enter,
	layout_clip: Props_layout_clip,
	-- The content shows through the bitmap's alpha: `mask = bitmap`, or
	-- `{ bitmap, w = …, h = …, x = …, y = …, sweep = … }`.
	mask: Props_mask,
	bg_shader: Props_bg_shader,
	-- Up to four vec4s: lists of numbers, or colors (0 to 1).
	shader_params: Props_shader_params,
	-- Events.
	on_click: Props_on_click,
	-- Primary pointer down and up.
	on_press: Props_on_press,
	on_release: Props_on_release,
	on_hover: Props_on_hover,
	on_pointer_move: Props_on_pointer_move,
	-- Return true to consume the key.
	on_key: Props_on_key,
	on_key_up: Props_on_key_up,
	on_focus: Props_on_focus,
	on_blur: Props_on_blur,
	-- Return true to take the gesture from the scroll views around it.
	on_wheel: Props_on_wheel,
	on_pinch: Props_on_pinch,
	on_mount: Props_on_mount,
	-- When layout gives it a new size, and at its first layout.
	on_resize: Props_on_resize,
	-- When any of it comes into view or all of it leaves (inside every
	-- clip and scroll viewport above it), and at its first layout: for
	-- building what only matters on screen.
	on_visible: Props_on_visible,
	-- Drag and drop within the window. Payloads are what `on_drag`
	-- returns: the script's own plain values, so they are `any` here.
	-- A source: `on_drag` returns the payload once the pointer moves 4
	-- points; `drag_preview` builds what follows the pointer.
	on_drag: Props_on_drag,
	drag_preview: Props_drag_preview,
	on_drag_end: Props_on_drag_end,
	-- What the drag carries to other apps as it leaves the window (macOS):
	-- Finder takes files, Mail text, a browser links.
	drag_export: Props_drag_export,
	-- Released outside every window of the app (macOS): a tab torn off its
	-- window. `grab_x`/`grab_y` are where the source was held.
	on_drag_outside: Props_on_drag_outside,
	-- The drag moves its window instead, with no preview (macOS): a
	-- window's only tab. Dropped on another window's destination it drops
	-- there; elsewhere the window stays where it was let go.
	drag_moves_window: Props_drag_moves_window,
	-- A destination: `drop_when` filters payloads (default: all).
	drop_when: Props_drop_when,
	on_drag_over: Props_on_drag_over,
	on_drag_leave: Props_on_drag_leave,
	on_drop: Props_on_drop,
	-- A secondary click (control-click on macOS) shows the returned
	-- `menu_item`s, `submenu`s and separators as a menu at the pointer;
	-- returning nil (or nothing) shows none.
	context_menu: Props_context_menu,
	-- The context menu panel's look: bg, rounded, p, gap, backdrop_blur,
	-- border, shadow, w, …
	context_menu_style: Props_context_menu_style,
	-- Open the context menu under the node, from its left edge (a tab's),
	-- rather than at the pointer.
	context_menu_below: Props_context_menu_below,
	[number]: never,
	-- Required unless `field` gives it.
	value: (State<any>)?,
	-- One of a `ui.form`'s fields (`form.fields.email`): its value, its
	-- leaving, its error (and, a one-line input, Enter submitting the form).
	field: FormField?,
	-- A list of strings or of numbers as it is (Luau's tables are
	-- invariant: `{ string }` is not a `{ SegmentedOption }`).
	options: { string } | { number } | { SegmentedOption },
	label: string?,
}
-- A number field with buttons that step it; the arrows step it, Page Up
-- and Down ten steps, Home and End to its ends; typing applies on Enter.
export type StepperProps = {
	-- Size and position.
	w: Props_w,
	h: Props_h,
	size: Props_size,
	min_w: Props_min_w,
	max_w: Props_max_w,
	min_h: Props_min_h,
	max_h: Props_max_h,
	w_full: Props_w_full,
	h_full: Props_h_full,
	aspect: Props_aspect,
	absolute: Props_absolute,
	relative: Props_relative,
	top: Props_top,
	left: Props_left,
	inset_start: Props_inset_start,
	inset_end: Props_inset_end,
	right: Props_right,
	bottom: Props_bottom,
	-- Spacing.
	p: Props_p,
	px: Props_px,
	py: Props_py,
	pt: Props_pt,
	pr: Props_pr,
	pb: Props_pb,
	pl: Props_pl,
	ps: Props_ps,
	pe: Props_pe,
	m: Props_m,
	mx: Props_mx,
	my: Props_my,
	mt: Props_mt,
	mr: Props_mr,
	mb: Props_mb,
	ml: Props_ml,
	ms: Props_ms,
	me: Props_me,
	gap: Props_gap,
	gap_x: Props_gap_x,
	gap_y: Props_gap_y,
	-- Flex and grid.
	direction: Props_direction,
	dir: Props_dir,
	grow: Props_grow,
	shrink: Props_shrink,
	basis: Props_basis,
	flex_wrap: Props_flex_wrap,
	wrap_reverse: Props_wrap_reverse,
	items: Props_items,
	justify: Props_justify,
	self: Props_self,
	content: Props_content,
	grid: Props_grid,
	cols: Props_cols,
	rows: Props_rows,
	col_start: Props_col_start,
	row_start: Props_row_start,
	col_span: Props_col_span,
	row_span: Props_row_span,
	col_span_full: Props_col_span_full,
	row_span_full: Props_row_span_full,
	overflow: Props_overflow,
	overflow_x: Props_overflow_x,
	overflow_y: Props_overflow_y,
	hidden: Props_hidden,
	invisible: Props_invisible,
	-- Paint.
	bg: Props_bg,
	gradient: Props_gradient,
	color: Props_color,
	bg_image: Props_bg_image,
	bg_fit: Props_bg_fit,
	rounded: Props_rounded,
	corners: Props_corners,
	rounded_tl: Props_rounded_tl,
	rounded_tr: Props_rounded_tr,
	rounded_br: Props_rounded_br,
	rounded_bl: Props_rounded_bl,
	border: Props_border,
	border_sides: Props_border_sides,
	border_color: Props_border_color,
	border_over: Props_border_over,
	-- A function may return nil: a solid border.
	border_dash: Props_border_dash,
	outline: Props_outline,
	outline_color: Props_outline_color,
	outline_offset: Props_outline_offset,
	shadow: Props_shadow,
	inset_shadow: Props_inset_shadow,
	opacity: Props_opacity,
	brightness: Props_brightness,
	contrast: Props_contrast,
	saturate: Props_saturate,
	blur: Props_blur,
	backdrop_blur: Props_backdrop_blur,
	fade: Props_fade,
	fade_x: Props_fade_x,
	fade_sides: Props_fade_sides,
	fade_overflow: Props_fade_overflow,
	snap: Props_snap,
	clip_inset: Props_clip_inset,
	layer: Props_layer,
	fit: Props_fit,
	tint: Props_tint,
	cursor: Props_cursor,
	-- Text.
	text_size: Props_text_size,
	weight: Props_weight,
	bold: Props_bold,
	italic: Props_italic,
	align: Props_align,
	font: Props_font,
	wrap: Props_wrap,
	truncate: Props_truncate,
	lines: Props_lines,
	line_height: Props_line_height,
	letter_spacing: Props_letter_spacing,
	-- Tabular figures: every digit the same width.
	tabular: Props_tabular,
	-- Underlined text (inherited, as CSS's `text-decoration`), and how far
	-- below the baseline the line sits.
	underline: Props_underline,
	underline_offset: Props_underline_offset,
	-- Transforms: after layout, about `origin` (fractions of the node).
	x: Props_x,
	y: Props_y,
	scale: Props_scale,
	scale_x: Props_scale_x,
	scale_y: Props_scale_y,
	rotate: Props_rotate,
	skew: Props_skew,
	skew_x: Props_skew_x,
	skew_y: Props_skew_y,
	origin: Props_origin,
	-- The subtree's theme: a theme's name (`"zgx-light"`), or tokens to
	-- override (`{ accent = "#e5484d" }`, with `theme = name` to start from).
	theme: Props_theme,
	-- Identity, for selectors, refs and animations.
	id: Props_id,
	-- Space-separated class names.
	class: Props_class,
	-- Styles chosen from a `ui.variants` definition: `button { tone =
	-- "ghost" }`, or the recipe itself for its defaults. The element's own
	-- properties win over them.
	variants: Props_variants,
	ref: Props_ref,
	focusable: Props_focusable,
	-- What assistive technology takes it for (ARIA's `role`), instead of
	-- what it is: a clickable row as an "option", a row of buttons as a
	-- "toolbar". It keeps what it does.
	role: Props_role,
	-- A section title of this level, 1 to 6 (`h1`…`h6`): screen readers
	-- list headings and move between them.
	heading: Props_heading,
	-- Whether this option, tab or item is the chosen one (`aria-selected`).
	selected: Props_selected,
	-- Whether a switch, checkbox or radio of the app's own is on
	-- (`aria-checked`), and whether what a control opens shows
	-- (`aria-expanded`): a custom disclosure's.
	checked: Props_checked,
	expanded: Props_expanded,
	-- Announce changes to the text in it (`aria-live`): a count, a "Sent"
	-- note, a message arriving.
	live: Props_live,
	-- Read after its name, as a hint (`aria-description`).
	description: Props_description,
	-- What is wrong with its value (`aria-invalid`), followed: nil while it
	-- is fine. Assistive technology hears it with the field.
	error: Props_error,
	-- Left out of the accessibility tree with what it holds (`aria-hidden`):
	-- decoration, or what is said elsewhere.
	aria_hidden: Props_aria_hidden,
	-- One Tab stop for the focusable things inside; the arrows along this
	-- axis (and Home, End) move among them, as ARIA's composite widgets
	-- behave. `role = "listbox"` is vertical, "toolbar" and "tablist"
	-- horizontal, without asking.
	focus_group: Props_focus_group,
	-- A press on it (or on what inside takes no input) moves the window; a
	-- double click zooms it. Buttons and inputs inside keep their clicks.
	-- CSS pointer-events: "none" (or false) lets the pointer through to what
	-- is under the node (presses, hover, the wheel, drops, a native view);
	-- descendants with handlers still take it. A state or function follows.
	pointer_events: Props_pointer_events,
	window_drag: Props_window_drag,
	-- The pointer selects the text in it and everything it holds, as on a
	-- web page (CSS `user-select: text`): drag, double/triple press, Shift,
	-- ⌘A / Ctrl+A, ⌘C / Ctrl+C. `false` keeps a part out. Off unless set.
	selectable: Props_selectable,
	-- A selectable node's highlight and selected-text colours (CSS
	-- `::selection`), over the theme's `$selection` and `$selection.text`
	-- tokens; a state or function follows.
	selection_color: Props_selection_color,
	selected_text_color: Props_selected_text_color,
	-- Shown over it while the pointer rests on it (after 300 ms; at once
	-- within a second of another): text in the theme's look, or a node of
	-- the app's own. Above it, centred, unless `tooltip_placement` says.
	tooltip: Props_tooltip,
	tooltip_placement: Props_tooltip_placement,
	-- A hover card the pointer can move into and click in: it stays while
	-- the pointer is over the node or the card.
	tooltip_interactive: Props_tooltip_interactive,
	-- Milliseconds the pointer rests before it shows (300; once one
	-- shows, the next shows at once). Keyboard focus shows it too.
	tooltip_delay: Props_tooltip_delay,
	-- A condition (true disables), or the styles it shows while disabled.
	disabled: Props_disabled,
	hover: Props_hover,
	active: Props_active,
	focus: Props_focus,
	-- Size animation ("height", "width", "size"; `true` is "height") or
	-- position animation ("position"). A `layout_id` node mounting as
	-- another with the same id unmounts (or while it is mounted) moves and
	-- scales from that one's box.
	layout: Props_layout,
	layout_id: Props_layout_id,
	layout_ease: Props_layout_ease,
	-- Milliseconds.
	layout_duration: Props_layout_duration,
	layout_enter: Props_layout_enter,
	layout_clip: Props_layout_clip,
	-- The content shows through the bitmap's alpha: `mask = bitmap`, or
	-- `{ bitmap, w = …, h = …, x = …, y = …, sweep = … }`.
	mask: Props_mask,
	bg_shader: Props_bg_shader,
	-- Up to four vec4s: lists of numbers, or colors (0 to 1).
	shader_params: Props_shader_params,
	-- Events.
	on_click: Props_on_click,
	-- Primary pointer down and up.
	on_press: Props_on_press,
	on_release: Props_on_release,
	on_hover: Props_on_hover,
	on_pointer_move: Props_on_pointer_move,
	-- Return true to consume the key.
	on_key: Props_on_key,
	on_key_up: Props_on_key_up,
	on_focus: Props_on_focus,
	on_blur: Props_on_blur,
	-- Return true to take the gesture from the scroll views around it.
	on_wheel: Props_on_wheel,
	on_pinch: Props_on_pinch,
	on_mount: Props_on_mount,
	-- When layout gives it a new size, and at its first layout.
	on_resize: Props_on_resize,
	-- When any of it comes into view or all of it leaves (inside every
	-- clip and scroll viewport above it), and at its first layout: for
	-- building what only matters on screen.
	on_visible: Props_on_visible,
	-- Drag and drop within the window. Payloads are what `on_drag`
	-- returns: the script's own plain values, so they are `any` here.
	-- A source: `on_drag` returns the payload once the pointer moves 4
	-- points; `drag_preview` builds what follows the pointer.
	on_drag: Props_on_drag,
	drag_preview: Props_drag_preview,
	on_drag_end: Props_on_drag_end,
	-- What the drag carries to other apps as it leaves the window (macOS):
	-- Finder takes files, Mail text, a browser links.
	drag_export: Props_drag_export,
	-- Released outside every window of the app (macOS): a tab torn off its
	-- window. `grab_x`/`grab_y` are where the source was held.
	on_drag_outside: Props_on_drag_outside,
	-- The drag moves its window instead, with no preview (macOS): a
	-- window's only tab. Dropped on another window's destination it drops
	-- there; elsewhere the window stays where it was let go.
	drag_moves_window: Props_drag_moves_window,
	-- A destination: `drop_when` filters payloads (default: all).
	drop_when: Props_drop_when,
	on_drag_over: Props_on_drag_over,
	on_drag_leave: Props_on_drag_leave,
	on_drop: Props_on_drop,
	-- A secondary click (control-click on macOS) shows the returned
	-- `menu_item`s, `submenu`s and separators as a menu at the pointer;
	-- returning nil (or nothing) shows none.
	context_menu: Props_context_menu,
	-- The context menu panel's look: bg, rounded, p, gap, backdrop_blur,
	-- border, shadow, w, …
	context_menu_style: Props_context_menu_style,
	-- Open the context menu under the node, from its left edge (a tab's),
	-- rather than at the pointer.
	context_menu_below: Props_context_menu_below,
	[number]: never,
	-- Required unless `field` gives it.
	value: (State<number>)?,
	-- One of a `ui.form`'s fields (`form.fields.email`): its value, its
	-- leaving, its error (and, a one-line input, Enter submitting the form).
	field: FormField?,
	label: string?,
	min: number?,
	max: number?,
	step: number?,
	-- Digits shown after the point.
	decimals: number?,
}
-- A summary that shows and hides its children (HTML's `<details>`).
export type DisclosureProps = {
	-- Size and position.
	w: Props_w,
	h: Props_h,
	size: Props_size,
	min_w: Props_min_w,
	max_w: Props_max_w,
	min_h: Props_min_h,
	max_h: Props_max_h,
	w_full: Props_w_full,
	h_full: Props_h_full,
	aspect: Props_aspect,
	absolute: Props_absolute,
	relative: Props_relative,
	top: Props_top,
	left: Props_left,
	inset_start: Props_inset_start,
	inset_end: Props_inset_end,
	right: Props_right,
	bottom: Props_bottom,
	-- Spacing.
	p: Props_p,
	px: Props_px,
	py: Props_py,
	pt: Props_pt,
	pr: Props_pr,
	pb: Props_pb,
	pl: Props_pl,
	ps: Props_ps,
	pe: Props_pe,
	m: Props_m,
	mx: Props_mx,
	my: Props_my,
	mt: Props_mt,
	mr: Props_mr,
	mb: Props_mb,
	ml: Props_ml,
	ms: Props_ms,
	me: Props_me,
	gap: Props_gap,
	gap_x: Props_gap_x,
	gap_y: Props_gap_y,
	-- Flex and grid.
	direction: Props_direction,
	dir: Props_dir,
	grow: Props_grow,
	shrink: Props_shrink,
	basis: Props_basis,
	flex_wrap: Props_flex_wrap,
	wrap_reverse: Props_wrap_reverse,
	items: Props_items,
	justify: Props_justify,
	self: Props_self,
	content: Props_content,
	grid: Props_grid,
	cols: Props_cols,
	rows: Props_rows,
	col_start: Props_col_start,
	row_start: Props_row_start,
	col_span: Props_col_span,
	row_span: Props_row_span,
	col_span_full: Props_col_span_full,
	row_span_full: Props_row_span_full,
	overflow: Props_overflow,
	overflow_x: Props_overflow_x,
	overflow_y: Props_overflow_y,
	hidden: Props_hidden,
	invisible: Props_invisible,
	-- Paint.
	bg: Props_bg,
	gradient: Props_gradient,
	color: Props_color,
	bg_image: Props_bg_image,
	bg_fit: Props_bg_fit,
	rounded: Props_rounded,
	corners: Props_corners,
	rounded_tl: Props_rounded_tl,
	rounded_tr: Props_rounded_tr,
	rounded_br: Props_rounded_br,
	rounded_bl: Props_rounded_bl,
	border: Props_border,
	border_sides: Props_border_sides,
	border_color: Props_border_color,
	border_over: Props_border_over,
	-- A function may return nil: a solid border.
	border_dash: Props_border_dash,
	outline: Props_outline,
	outline_color: Props_outline_color,
	outline_offset: Props_outline_offset,
	shadow: Props_shadow,
	inset_shadow: Props_inset_shadow,
	opacity: Props_opacity,
	brightness: Props_brightness,
	contrast: Props_contrast,
	saturate: Props_saturate,
	blur: Props_blur,
	backdrop_blur: Props_backdrop_blur,
	fade: Props_fade,
	fade_x: Props_fade_x,
	fade_sides: Props_fade_sides,
	fade_overflow: Props_fade_overflow,
	snap: Props_snap,
	clip_inset: Props_clip_inset,
	layer: Props_layer,
	fit: Props_fit,
	tint: Props_tint,
	cursor: Props_cursor,
	-- Text.
	text_size: Props_text_size,
	weight: Props_weight,
	bold: Props_bold,
	italic: Props_italic,
	align: Props_align,
	font: Props_font,
	wrap: Props_wrap,
	truncate: Props_truncate,
	lines: Props_lines,
	line_height: Props_line_height,
	letter_spacing: Props_letter_spacing,
	-- Tabular figures: every digit the same width.
	tabular: Props_tabular,
	-- Underlined text (inherited, as CSS's `text-decoration`), and how far
	-- below the baseline the line sits.
	underline: Props_underline,
	underline_offset: Props_underline_offset,
	-- Transforms: after layout, about `origin` (fractions of the node).
	x: Props_x,
	y: Props_y,
	scale: Props_scale,
	scale_x: Props_scale_x,
	scale_y: Props_scale_y,
	rotate: Props_rotate,
	skew: Props_skew,
	skew_x: Props_skew_x,
	skew_y: Props_skew_y,
	origin: Props_origin,
	-- The subtree's theme: a theme's name (`"zgx-light"`), or tokens to
	-- override (`{ accent = "#e5484d" }`, with `theme = name` to start from).
	theme: Props_theme,
	-- Identity, for selectors, refs and animations.
	id: Props_id,
	-- Space-separated class names.
	class: Props_class,
	-- Styles chosen from a `ui.variants` definition: `button { tone =
	-- "ghost" }`, or the recipe itself for its defaults. The element's own
	-- properties win over them.
	variants: Props_variants,
	ref: Props_ref,
	focusable: Props_focusable,
	-- What assistive technology takes it for (ARIA's `role`), instead of
	-- what it is: a clickable row as an "option", a row of buttons as a
	-- "toolbar". It keeps what it does.
	role: Props_role,
	-- A section title of this level, 1 to 6 (`h1`…`h6`): screen readers
	-- list headings and move between them.
	heading: Props_heading,
	-- Whether this option, tab or item is the chosen one (`aria-selected`).
	selected: Props_selected,
	-- Whether a switch, checkbox or radio of the app's own is on
	-- (`aria-checked`), and whether what a control opens shows
	-- (`aria-expanded`): a custom disclosure's.
	checked: Props_checked,
	expanded: Props_expanded,
	-- Announce changes to the text in it (`aria-live`): a count, a "Sent"
	-- note, a message arriving.
	live: Props_live,
	-- Read after its name, as a hint (`aria-description`).
	description: Props_description,
	-- What is wrong with its value (`aria-invalid`), followed: nil while it
	-- is fine. Assistive technology hears it with the field.
	error: Props_error,
	-- Left out of the accessibility tree with what it holds (`aria-hidden`):
	-- decoration, or what is said elsewhere.
	aria_hidden: Props_aria_hidden,
	-- One Tab stop for the focusable things inside; the arrows along this
	-- axis (and Home, End) move among them, as ARIA's composite widgets
	-- behave. `role = "listbox"` is vertical, "toolbar" and "tablist"
	-- horizontal, without asking.
	focus_group: Props_focus_group,
	-- A press on it (or on what inside takes no input) moves the window; a
	-- double click zooms it. Buttons and inputs inside keep their clicks.
	-- CSS pointer-events: "none" (or false) lets the pointer through to what
	-- is under the node (presses, hover, the wheel, drops, a native view);
	-- descendants with handlers still take it. A state or function follows.
	pointer_events: Props_pointer_events,
	window_drag: Props_window_drag,
	-- The pointer selects the text in it and everything it holds, as on a
	-- web page (CSS `user-select: text`): drag, double/triple press, Shift,
	-- ⌘A / Ctrl+A, ⌘C / Ctrl+C. `false` keeps a part out. Off unless set.
	selectable: Props_selectable,
	-- A selectable node's highlight and selected-text colours (CSS
	-- `::selection`), over the theme's `$selection` and `$selection.text`
	-- tokens; a state or function follows.
	selection_color: Props_selection_color,
	selected_text_color: Props_selected_text_color,
	-- Shown over it while the pointer rests on it (after 300 ms; at once
	-- within a second of another): text in the theme's look, or a node of
	-- the app's own. Above it, centred, unless `tooltip_placement` says.
	tooltip: Props_tooltip,
	tooltip_placement: Props_tooltip_placement,
	-- A hover card the pointer can move into and click in: it stays while
	-- the pointer is over the node or the card.
	tooltip_interactive: Props_tooltip_interactive,
	-- Milliseconds the pointer rests before it shows (300; once one
	-- shows, the next shows at once). Keyboard focus shows it too.
	tooltip_delay: Props_tooltip_delay,
	-- A condition (true disables), or the styles it shows while disabled.
	disabled: Props_disabled,
	hover: Props_hover,
	active: Props_active,
	focus: Props_focus,
	-- Size animation ("height", "width", "size"; `true` is "height") or
	-- position animation ("position"). A `layout_id` node mounting as
	-- another with the same id unmounts (or while it is mounted) moves and
	-- scales from that one's box.
	layout: Props_layout,
	layout_id: Props_layout_id,
	layout_ease: Props_layout_ease,
	-- Milliseconds.
	layout_duration: Props_layout_duration,
	layout_enter: Props_layout_enter,
	layout_clip: Props_layout_clip,
	-- The content shows through the bitmap's alpha: `mask = bitmap`, or
	-- `{ bitmap, w = …, h = …, x = …, y = …, sweep = … }`.
	mask: Props_mask,
	bg_shader: Props_bg_shader,
	-- Up to four vec4s: lists of numbers, or colors (0 to 1).
	shader_params: Props_shader_params,
	-- Events.
	on_click: Props_on_click,
	-- Primary pointer down and up.
	on_press: Props_on_press,
	on_release: Props_on_release,
	on_hover: Props_on_hover,
	on_pointer_move: Props_on_pointer_move,
	-- Return true to consume the key.
	on_key: Props_on_key,
	on_key_up: Props_on_key_up,
	on_focus: Props_on_focus,
	on_blur: Props_on_blur,
	-- Return true to take the gesture from the scroll views around it.
	on_wheel: Props_on_wheel,
	on_pinch: Props_on_pinch,
	on_mount: Props_on_mount,
	-- When layout gives it a new size, and at its first layout.
	on_resize: Props_on_resize,
	-- When any of it comes into view or all of it leaves (inside every
	-- clip and scroll viewport above it), and at its first layout: for
	-- building what only matters on screen.
	on_visible: Props_on_visible,
	-- Drag and drop within the window. Payloads are what `on_drag`
	-- returns: the script's own plain values, so they are `any` here.
	-- A source: `on_drag` returns the payload once the pointer moves 4
	-- points; `drag_preview` builds what follows the pointer.
	on_drag: Props_on_drag,
	drag_preview: Props_drag_preview,
	on_drag_end: Props_on_drag_end,
	-- What the drag carries to other apps as it leaves the window (macOS):
	-- Finder takes files, Mail text, a browser links.
	drag_export: Props_drag_export,
	-- Released outside every window of the app (macOS): a tab torn off its
	-- window. `grab_x`/`grab_y` are where the source was held.
	on_drag_outside: Props_on_drag_outside,
	-- The drag moves its window instead, with no preview (macOS): a
	-- window's only tab. Dropped on another window's destination it drops
	-- there; elsewhere the window stays where it was let go.
	drag_moves_window: Props_drag_moves_window,
	-- A destination: `drop_when` filters payloads (default: all).
	drop_when: Props_drop_when,
	on_drag_over: Props_on_drag_over,
	on_drag_leave: Props_on_drag_leave,
	on_drop: Props_on_drop,
	-- A secondary click (control-click on macOS) shows the returned
	-- `menu_item`s, `submenu`s and separators as a menu at the pointer;
	-- returning nil (or nothing) shows none.
	context_menu: Props_context_menu,
	-- The context menu panel's look: bg, rounded, p, gap, backdrop_blur,
	-- border, shadow, w, …
	context_menu_style: Props_context_menu_style,
	-- Open the context menu under the node, from its left edge (a tab's),
	-- rather than at the pointer.
	context_menu_below: Props_context_menu_below,
	[number]: Child,
	-- The name assistive technology reads for it, instead of its text: a
	-- switch, a button showing only an icon. A state or function is
	-- followed: a list row's name made from what it shows now.
	label: (string | Readable<string> | (() -> string))?,
	open: State<boolean>,
	summary: string | Node,
}
-- Sections, one open at a time: `open` is its `id`, or nil.
export type AccordionProps = {
	-- Size and position.
	w: Props_w,
	h: Props_h,
	size: Props_size,
	min_w: Props_min_w,
	max_w: Props_max_w,
	min_h: Props_min_h,
	max_h: Props_max_h,
	w_full: Props_w_full,
	h_full: Props_h_full,
	aspect: Props_aspect,
	absolute: Props_absolute,
	relative: Props_relative,
	top: Props_top,
	left: Props_left,
	inset_start: Props_inset_start,
	inset_end: Props_inset_end,
	right: Props_right,
	bottom: Props_bottom,
	-- Spacing.
	p: Props_p,
	px: Props_px,
	py: Props_py,
	pt: Props_pt,
	pr: Props_pr,
	pb: Props_pb,
	pl: Props_pl,
	ps: Props_ps,
	pe: Props_pe,
	m: Props_m,
	mx: Props_mx,
	my: Props_my,
	mt: Props_mt,
	mr: Props_mr,
	mb: Props_mb,
	ml: Props_ml,
	ms: Props_ms,
	me: Props_me,
	gap: Props_gap,
	gap_x: Props_gap_x,
	gap_y: Props_gap_y,
	-- Flex and grid.
	direction: Props_direction,
	dir: Props_dir,
	grow: Props_grow,
	shrink: Props_shrink,
	basis: Props_basis,
	flex_wrap: Props_flex_wrap,
	wrap_reverse: Props_wrap_reverse,
	items: Props_items,
	justify: Props_justify,
	self: Props_self,
	content: Props_content,
	grid: Props_grid,
	cols: Props_cols,
	rows: Props_rows,
	col_start: Props_col_start,
	row_start: Props_row_start,
	col_span: Props_col_span,
	row_span: Props_row_span,
	col_span_full: Props_col_span_full,
	row_span_full: Props_row_span_full,
	overflow: Props_overflow,
	overflow_x: Props_overflow_x,
	overflow_y: Props_overflow_y,
	hidden: Props_hidden,
	invisible: Props_invisible,
	-- Paint.
	bg: Props_bg,
	gradient: Props_gradient,
	color: Props_color,
	bg_image: Props_bg_image,
	bg_fit: Props_bg_fit,
	rounded: Props_rounded,
	corners: Props_corners,
	rounded_tl: Props_rounded_tl,
	rounded_tr: Props_rounded_tr,
	rounded_br: Props_rounded_br,
	rounded_bl: Props_rounded_bl,
	border: Props_border,
	border_sides: Props_border_sides,
	border_color: Props_border_color,
	border_over: Props_border_over,
	-- A function may return nil: a solid border.
	border_dash: Props_border_dash,
	outline: Props_outline,
	outline_color: Props_outline_color,
	outline_offset: Props_outline_offset,
	shadow: Props_shadow,
	inset_shadow: Props_inset_shadow,
	opacity: Props_opacity,
	brightness: Props_brightness,
	contrast: Props_contrast,
	saturate: Props_saturate,
	blur: Props_blur,
	backdrop_blur: Props_backdrop_blur,
	fade: Props_fade,
	fade_x: Props_fade_x,
	fade_sides: Props_fade_sides,
	fade_overflow: Props_fade_overflow,
	snap: Props_snap,
	clip_inset: Props_clip_inset,
	layer: Props_layer,
	fit: Props_fit,
	tint: Props_tint,
	cursor: Props_cursor,
	-- Text.
	text_size: Props_text_size,
	weight: Props_weight,
	bold: Props_bold,
	italic: Props_italic,
	align: Props_align,
	font: Props_font,
	wrap: Props_wrap,
	truncate: Props_truncate,
	lines: Props_lines,
	line_height: Props_line_height,
	letter_spacing: Props_letter_spacing,
	-- Tabular figures: every digit the same width.
	tabular: Props_tabular,
	-- Underlined text (inherited, as CSS's `text-decoration`), and how far
	-- below the baseline the line sits.
	underline: Props_underline,
	underline_offset: Props_underline_offset,
	-- Transforms: after layout, about `origin` (fractions of the node).
	x: Props_x,
	y: Props_y,
	scale: Props_scale,
	scale_x: Props_scale_x,
	scale_y: Props_scale_y,
	rotate: Props_rotate,
	skew: Props_skew,
	skew_x: Props_skew_x,
	skew_y: Props_skew_y,
	origin: Props_origin,
	-- The subtree's theme: a theme's name (`"zgx-light"`), or tokens to
	-- override (`{ accent = "#e5484d" }`, with `theme = name` to start from).
	theme: Props_theme,
	-- Identity, for selectors, refs and animations.
	id: Props_id,
	-- Space-separated class names.
	class: Props_class,
	-- Styles chosen from a `ui.variants` definition: `button { tone =
	-- "ghost" }`, or the recipe itself for its defaults. The element's own
	-- properties win over them.
	variants: Props_variants,
	ref: Props_ref,
	focusable: Props_focusable,
	-- What assistive technology takes it for (ARIA's `role`), instead of
	-- what it is: a clickable row as an "option", a row of buttons as a
	-- "toolbar". It keeps what it does.
	role: Props_role,
	-- A section title of this level, 1 to 6 (`h1`…`h6`): screen readers
	-- list headings and move between them.
	heading: Props_heading,
	-- Whether this option, tab or item is the chosen one (`aria-selected`).
	selected: Props_selected,
	-- Whether a switch, checkbox or radio of the app's own is on
	-- (`aria-checked`), and whether what a control opens shows
	-- (`aria-expanded`): a custom disclosure's.
	checked: Props_checked,
	expanded: Props_expanded,
	-- Announce changes to the text in it (`aria-live`): a count, a "Sent"
	-- note, a message arriving.
	live: Props_live,
	-- Read after its name, as a hint (`aria-description`).
	description: Props_description,
	-- What is wrong with its value (`aria-invalid`), followed: nil while it
	-- is fine. Assistive technology hears it with the field.
	error: Props_error,
	-- Left out of the accessibility tree with what it holds (`aria-hidden`):
	-- decoration, or what is said elsewhere.
	aria_hidden: Props_aria_hidden,
	-- One Tab stop for the focusable things inside; the arrows along this
	-- axis (and Home, End) move among them, as ARIA's composite widgets
	-- behave. `role = "listbox"` is vertical, "toolbar" and "tablist"
	-- horizontal, without asking.
	focus_group: Props_focus_group,
	-- A press on it (or on what inside takes no input) moves the window; a
	-- double click zooms it. Buttons and inputs inside keep their clicks.
	-- CSS pointer-events: "none" (or false) lets the pointer through to what
	-- is under the node (presses, hover, the wheel, drops, a native view);
	-- descendants with handlers still take it. A state or function follows.
	pointer_events: Props_pointer_events,
	window_drag: Props_window_drag,
	-- The pointer selects the text in it and everything it holds, as on a
	-- web page (CSS `user-select: text`): drag, double/triple press, Shift,
	-- ⌘A / Ctrl+A, ⌘C / Ctrl+C. `false` keeps a part out. Off unless set.
	selectable: Props_selectable,
	-- A selectable node's highlight and selected-text colours (CSS
	-- `::selection`), over the theme's `$selection` and `$selection.text`
	-- tokens; a state or function follows.
	selection_color: Props_selection_color,
	selected_text_color: Props_selected_text_color,
	-- Shown over it while the pointer rests on it (after 300 ms; at once
	-- within a second of another): text in the theme's look, or a node of
	-- the app's own. Above it, centred, unless `tooltip_placement` says.
	tooltip: Props_tooltip,
	tooltip_placement: Props_tooltip_placement,
	-- A hover card the pointer can move into and click in: it stays while
	-- the pointer is over the node or the card.
	tooltip_interactive: Props_tooltip_interactive,
	-- Milliseconds the pointer rests before it shows (300; once one
	-- shows, the next shows at once). Keyboard focus shows it too.
	tooltip_delay: Props_tooltip_delay,
	-- A condition (true disables), or the styles it shows while disabled.
	disabled: Props_disabled,
	hover: Props_hover,
	active: Props_active,
	focus: Props_focus,
	-- Size animation ("height", "width", "size"; `true` is "height") or
	-- position animation ("position"). A `layout_id` node mounting as
	-- another with the same id unmounts (or while it is mounted) moves and
	-- scales from that one's box.
	layout: Props_layout,
	layout_id: Props_layout_id,
	layout_ease: Props_layout_ease,
	-- Milliseconds.
	layout_duration: Props_layout_duration,
	layout_enter: Props_layout_enter,
	layout_clip: Props_layout_clip,
	-- The content shows through the bitmap's alpha: `mask = bitmap`, or
	-- `{ bitmap, w = …, h = …, x = …, y = …, sweep = … }`.
	mask: Props_mask,
	bg_shader: Props_bg_shader,
	-- Up to four vec4s: lists of numbers, or colors (0 to 1).
	shader_params: Props_shader_params,
	-- Events.
	on_click: Props_on_click,
	-- Primary pointer down and up.
	on_press: Props_on_press,
	on_release: Props_on_release,
	on_hover: Props_on_hover,
	on_pointer_move: Props_on_pointer_move,
	-- Return true to consume the key.
	on_key: Props_on_key,
	on_key_up: Props_on_key_up,
	on_focus: Props_on_focus,
	on_blur: Props_on_blur,
	-- Return true to take the gesture from the scroll views around it.
	on_wheel: Props_on_wheel,
	on_pinch: Props_on_pinch,
	on_mount: Props_on_mount,
	-- When layout gives it a new size, and at its first layout.
	on_resize: Props_on_resize,
	-- When any of it comes into view or all of it leaves (inside every
	-- clip and scroll viewport above it), and at its first layout: for
	-- building what only matters on screen.
	on_visible: Props_on_visible,
	-- Drag and drop within the window. Payloads are what `on_drag`
	-- returns: the script's own plain values, so they are `any` here.
	-- A source: `on_drag` returns the payload once the pointer moves 4
	-- points; `drag_preview` builds what follows the pointer.
	on_drag: Props_on_drag,
	drag_preview: Props_drag_preview,
	on_drag_end: Props_on_drag_end,
	-- What the drag carries to other apps as it leaves the window (macOS):
	-- Finder takes files, Mail text, a browser links.
	drag_export: Props_drag_export,
	-- Released outside every window of the app (macOS): a tab torn off its
	-- window. `grab_x`/`grab_y` are where the source was held.
	on_drag_outside: Props_on_drag_outside,
	-- The drag moves its window instead, with no preview (macOS): a
	-- window's only tab. Dropped on another window's destination it drops
	-- there; elsewhere the window stays where it was let go.
	drag_moves_window: Props_drag_moves_window,
	-- A destination: `drop_when` filters payloads (default: all).
	drop_when: Props_drop_when,
	on_drag_over: Props_on_drag_over,
	on_drag_leave: Props_on_drag_leave,
	on_drop: Props_on_drop,
	-- A secondary click (control-click on macOS) shows the returned
	-- `menu_item`s, `submenu`s and separators as a menu at the pointer;
	-- returning nil (or nothing) shows none.
	context_menu: Props_context_menu,
	-- The context menu panel's look: bg, rounded, p, gap, backdrop_blur,
	-- border, shadow, w, …
	context_menu_style: Props_context_menu_style,
	-- Open the context menu under the node, from its left edge (a tab's),
	-- rather than at the pointer.
	context_menu_below: Props_context_menu_below,
	[number]: Node,
	open: State<any>,
}
export type SectionProps = {
	-- Size and position.
	w: Props_w,
	h: Props_h,
	size: Props_size,
	min_w: Props_min_w,
	max_w: Props_max_w,
	min_h: Props_min_h,
	max_h: Props_max_h,
	w_full: Props_w_full,
	h_full: Props_h_full,
	aspect: Props_aspect,
	absolute: Props_absolute,
	relative: Props_relative,
	top: Props_top,
	left: Props_left,
	inset_start: Props_inset_start,
	inset_end: Props_inset_end,
	right: Props_right,
	bottom: Props_bottom,
	-- Spacing.
	p: Props_p,
	px: Props_px,
	py: Props_py,
	pt: Props_pt,
	pr: Props_pr,
	pb: Props_pb,
	pl: Props_pl,
	ps: Props_ps,
	pe: Props_pe,
	m: Props_m,
	mx: Props_mx,
	my: Props_my,
	mt: Props_mt,
	mr: Props_mr,
	mb: Props_mb,
	ml: Props_ml,
	ms: Props_ms,
	me: Props_me,
	gap: Props_gap,
	gap_x: Props_gap_x,
	gap_y: Props_gap_y,
	-- Flex and grid.
	direction: Props_direction,
	dir: Props_dir,
	grow: Props_grow,
	shrink: Props_shrink,
	basis: Props_basis,
	flex_wrap: Props_flex_wrap,
	wrap_reverse: Props_wrap_reverse,
	items: Props_items,
	justify: Props_justify,
	self: Props_self,
	content: Props_content,
	grid: Props_grid,
	cols: Props_cols,
	rows: Props_rows,
	col_start: Props_col_start,
	row_start: Props_row_start,
	col_span: Props_col_span,
	row_span: Props_row_span,
	col_span_full: Props_col_span_full,
	row_span_full: Props_row_span_full,
	overflow: Props_overflow,
	overflow_x: Props_overflow_x,
	overflow_y: Props_overflow_y,
	hidden: Props_hidden,
	invisible: Props_invisible,
	-- Paint.
	bg: Props_bg,
	gradient: Props_gradient,
	color: Props_color,
	bg_image: Props_bg_image,
	bg_fit: Props_bg_fit,
	rounded: Props_rounded,
	corners: Props_corners,
	rounded_tl: Props_rounded_tl,
	rounded_tr: Props_rounded_tr,
	rounded_br: Props_rounded_br,
	rounded_bl: Props_rounded_bl,
	border: Props_border,
	border_sides: Props_border_sides,
	border_color: Props_border_color,
	border_over: Props_border_over,
	-- A function may return nil: a solid border.
	border_dash: Props_border_dash,
	outline: Props_outline,
	outline_color: Props_outline_color,
	outline_offset: Props_outline_offset,
	shadow: Props_shadow,
	inset_shadow: Props_inset_shadow,
	opacity: Props_opacity,
	brightness: Props_brightness,
	contrast: Props_contrast,
	saturate: Props_saturate,
	blur: Props_blur,
	backdrop_blur: Props_backdrop_blur,
	fade: Props_fade,
	fade_x: Props_fade_x,
	fade_sides: Props_fade_sides,
	fade_overflow: Props_fade_overflow,
	snap: Props_snap,
	clip_inset: Props_clip_inset,
	layer: Props_layer,
	fit: Props_fit,
	tint: Props_tint,
	cursor: Props_cursor,
	-- Text.
	text_size: Props_text_size,
	weight: Props_weight,
	bold: Props_bold,
	italic: Props_italic,
	align: Props_align,
	font: Props_font,
	wrap: Props_wrap,
	truncate: Props_truncate,
	lines: Props_lines,
	line_height: Props_line_height,
	letter_spacing: Props_letter_spacing,
	-- Tabular figures: every digit the same width.
	tabular: Props_tabular,
	-- Underlined text (inherited, as CSS's `text-decoration`), and how far
	-- below the baseline the line sits.
	underline: Props_underline,
	underline_offset: Props_underline_offset,
	-- Transforms: after layout, about `origin` (fractions of the node).
	x: Props_x,
	y: Props_y,
	scale: Props_scale,
	scale_x: Props_scale_x,
	scale_y: Props_scale_y,
	rotate: Props_rotate,
	skew: Props_skew,
	skew_x: Props_skew_x,
	skew_y: Props_skew_y,
	origin: Props_origin,
	-- The subtree's theme: a theme's name (`"zgx-light"`), or tokens to
	-- override (`{ accent = "#e5484d" }`, with `theme = name` to start from).
	theme: Props_theme,
	-- Space-separated class names.
	class: Props_class,
	-- Styles chosen from a `ui.variants` definition: `button { tone =
	-- "ghost" }`, or the recipe itself for its defaults. The element's own
	-- properties win over them.
	variants: Props_variants,
	ref: Props_ref,
	focusable: Props_focusable,
	-- What assistive technology takes it for (ARIA's `role`), instead of
	-- what it is: a clickable row as an "option", a row of buttons as a
	-- "toolbar". It keeps what it does.
	role: Props_role,
	-- A section title of this level, 1 to 6 (`h1`…`h6`): screen readers
	-- list headings and move between them.
	heading: Props_heading,
	-- Whether this option, tab or item is the chosen one (`aria-selected`).
	selected: Props_selected,
	-- Whether a switch, checkbox or radio of the app's own is on
	-- (`aria-checked`), and whether what a control opens shows
	-- (`aria-expanded`): a custom disclosure's.
	checked: Props_checked,
	expanded: Props_expanded,
	-- Announce changes to the text in it (`aria-live`): a count, a "Sent"
	-- note, a message arriving.
	live: Props_live,
	-- Read after its name, as a hint (`aria-description`).
	description: Props_description,
	-- What is wrong with its value (`aria-invalid`), followed: nil while it
	-- is fine. Assistive technology hears it with the field.
	error: Props_error,
	-- Left out of the accessibility tree with what it holds (`aria-hidden`):
	-- decoration, or what is said elsewhere.
	aria_hidden: Props_aria_hidden,
	-- One Tab stop for the focusable things inside; the arrows along this
	-- axis (and Home, End) move among them, as ARIA's composite widgets
	-- behave. `role = "listbox"` is vertical, "toolbar" and "tablist"
	-- horizontal, without asking.
	focus_group: Props_focus_group,
	-- A press on it (or on what inside takes no input) moves the window; a
	-- double click zooms it. Buttons and inputs inside keep their clicks.
	-- CSS pointer-events: "none" (or false) lets the pointer through to what
	-- is under the node (presses, hover, the wheel, drops, a native view);
	-- descendants with handlers still take it. A state or function follows.
	pointer_events: Props_pointer_events,
	window_drag: Props_window_drag,
	-- The pointer selects the text in it and everything it holds, as on a
	-- web page (CSS `user-select: text`): drag, double/triple press, Shift,
	-- ⌘A / Ctrl+A, ⌘C / Ctrl+C. `false` keeps a part out. Off unless set.
	selectable: Props_selectable,
	-- A selectable node's highlight and selected-text colours (CSS
	-- `::selection`), over the theme's `$selection` and `$selection.text`
	-- tokens; a state or function follows.
	selection_color: Props_selection_color,
	selected_text_color: Props_selected_text_color,
	-- Shown over it while the pointer rests on it (after 300 ms; at once
	-- within a second of another): text in the theme's look, or a node of
	-- the app's own. Above it, centred, unless `tooltip_placement` says.
	tooltip: Props_tooltip,
	tooltip_placement: Props_tooltip_placement,
	-- A hover card the pointer can move into and click in: it stays while
	-- the pointer is over the node or the card.
	tooltip_interactive: Props_tooltip_interactive,
	-- Milliseconds the pointer rests before it shows (300; once one
	-- shows, the next shows at once). Keyboard focus shows it too.
	tooltip_delay: Props_tooltip_delay,
	-- A condition (true disables), or the styles it shows while disabled.
	disabled: Props_disabled,
	hover: Props_hover,
	active: Props_active,
	focus: Props_focus,
	-- Size animation ("height", "width", "size"; `true` is "height") or
	-- position animation ("position"). A `layout_id` node mounting as
	-- another with the same id unmounts (or while it is mounted) moves and
	-- scales from that one's box.
	layout: Props_layout,
	layout_id: Props_layout_id,
	layout_ease: Props_layout_ease,
	-- Milliseconds.
	layout_duration: Props_layout_duration,
	layout_enter: Props_layout_enter,
	layout_clip: Props_layout_clip,
	-- The content shows through the bitmap's alpha: `mask = bitmap`, or
	-- `{ bitmap, w = …, h = …, x = …, y = …, sweep = … }`.
	mask: Props_mask,
	bg_shader: Props_bg_shader,
	-- Up to four vec4s: lists of numbers, or colors (0 to 1).
	shader_params: Props_shader_params,
	-- Events.
	on_click: Props_on_click,
	-- Primary pointer down and up.
	on_press: Props_on_press,
	on_release: Props_on_release,
	on_hover: Props_on_hover,
	on_pointer_move: Props_on_pointer_move,
	-- Return true to consume the key.
	on_key: Props_on_key,
	on_key_up: Props_on_key_up,
	on_focus: Props_on_focus,
	on_blur: Props_on_blur,
	-- Return true to take the gesture from the scroll views around it.
	on_wheel: Props_on_wheel,
	on_pinch: Props_on_pinch,
	on_mount: Props_on_mount,
	-- When layout gives it a new size, and at its first layout.
	on_resize: Props_on_resize,
	-- When any of it comes into view or all of it leaves (inside every
	-- clip and scroll viewport above it), and at its first layout: for
	-- building what only matters on screen.
	on_visible: Props_on_visible,
	-- Drag and drop within the window. Payloads are what `on_drag`
	-- returns: the script's own plain values, so they are `any` here.
	-- A source: `on_drag` returns the payload once the pointer moves 4
	-- points; `drag_preview` builds what follows the pointer.
	on_drag: Props_on_drag,
	drag_preview: Props_drag_preview,
	on_drag_end: Props_on_drag_end,
	-- What the drag carries to other apps as it leaves the window (macOS):
	-- Finder takes files, Mail text, a browser links.
	drag_export: Props_drag_export,
	-- Released outside every window of the app (macOS): a tab torn off its
	-- window. `grab_x`/`grab_y` are where the source was held.
	on_drag_outside: Props_on_drag_outside,
	-- The drag moves its window instead, with no preview (macOS): a
	-- window's only tab. Dropped on another window's destination it drops
	-- there; elsewhere the window stays where it was let go.
	drag_moves_window: Props_drag_moves_window,
	-- A destination: `drop_when` filters payloads (default: all).
	drop_when: Props_drop_when,
	on_drag_over: Props_on_drag_over,
	on_drag_leave: Props_on_drag_leave,
	on_drop: Props_on_drop,
	-- A secondary click (control-click on macOS) shows the returned
	-- `menu_item`s, `submenu`s and separators as a menu at the pointer;
	-- returning nil (or nothing) shows none.
	context_menu: Props_context_menu,
	-- The context menu panel's look: bg, rounded, p, gap, backdrop_blur,
	-- border, shadow, w, …
	context_menu_style: Props_context_menu_style,
	-- Open the context menu under the node, from its left edge (a tab's),
	-- rather than at the pointer.
	context_menu_below: Props_context_menu_below,
	[number]: Child,
	-- The name assistive technology reads for it, instead of its text: a
	-- switch, a button showing only an icon. A state or function is
	-- followed: a list row's name made from what it shows now.
	label: (string | Readable<string> | (() -> string))?,
	id: any,
	title: string,
}
export type SeparatorProps = {
	-- Size and position.
	w: Props_w,
	h: Props_h,
	size: Props_size,
	min_w: Props_min_w,
	max_w: Props_max_w,
	min_h: Props_min_h,
	max_h: Props_max_h,
	w_full: Props_w_full,
	h_full: Props_h_full,
	aspect: Props_aspect,
	absolute: Props_absolute,
	relative: Props_relative,
	top: Props_top,
	left: Props_left,
	inset_start: Props_inset_start,
	inset_end: Props_inset_end,
	right: Props_right,
	bottom: Props_bottom,
	-- Spacing.
	p: Props_p,
	px: Props_px,
	py: Props_py,
	pt: Props_pt,
	pr: Props_pr,
	pb: Props_pb,
	pl: Props_pl,
	ps: Props_ps,
	pe: Props_pe,
	m: Props_m,
	mx: Props_mx,
	my: Props_my,
	mt: Props_mt,
	mr: Props_mr,
	mb: Props_mb,
	ml: Props_ml,
	ms: Props_ms,
	me: Props_me,
	gap: Props_gap,
	gap_x: Props_gap_x,
	gap_y: Props_gap_y,
	-- Flex and grid.
	direction: Props_direction,
	dir: Props_dir,
	grow: Props_grow,
	shrink: Props_shrink,
	basis: Props_basis,
	flex_wrap: Props_flex_wrap,
	wrap_reverse: Props_wrap_reverse,
	items: Props_items,
	justify: Props_justify,
	self: Props_self,
	content: Props_content,
	grid: Props_grid,
	cols: Props_cols,
	rows: Props_rows,
	col_start: Props_col_start,
	row_start: Props_row_start,
	col_span: Props_col_span,
	row_span: Props_row_span,
	col_span_full: Props_col_span_full,
	row_span_full: Props_row_span_full,
	overflow: Props_overflow,
	overflow_x: Props_overflow_x,
	overflow_y: Props_overflow_y,
	hidden: Props_hidden,
	invisible: Props_invisible,
	-- Paint.
	bg: Props_bg,
	gradient: Props_gradient,
	color: Props_color,
	bg_image: Props_bg_image,
	bg_fit: Props_bg_fit,
	rounded: Props_rounded,
	corners: Props_corners,
	rounded_tl: Props_rounded_tl,
	rounded_tr: Props_rounded_tr,
	rounded_br: Props_rounded_br,
	rounded_bl: Props_rounded_bl,
	border: Props_border,
	border_sides: Props_border_sides,
	border_color: Props_border_color,
	border_over: Props_border_over,
	-- A function may return nil: a solid border.
	border_dash: Props_border_dash,
	outline: Props_outline,
	outline_color: Props_outline_color,
	outline_offset: Props_outline_offset,
	shadow: Props_shadow,
	inset_shadow: Props_inset_shadow,
	opacity: Props_opacity,
	brightness: Props_brightness,
	contrast: Props_contrast,
	saturate: Props_saturate,
	blur: Props_blur,
	backdrop_blur: Props_backdrop_blur,
	fade: Props_fade,
	fade_x: Props_fade_x,
	fade_sides: Props_fade_sides,
	fade_overflow: Props_fade_overflow,
	snap: Props_snap,
	clip_inset: Props_clip_inset,
	layer: Props_layer,
	fit: Props_fit,
	tint: Props_tint,
	cursor: Props_cursor,
	-- Text.
	text_size: Props_text_size,
	weight: Props_weight,
	bold: Props_bold,
	italic: Props_italic,
	align: Props_align,
	font: Props_font,
	wrap: Props_wrap,
	truncate: Props_truncate,
	lines: Props_lines,
	line_height: Props_line_height,
	letter_spacing: Props_letter_spacing,
	-- Tabular figures: every digit the same width.
	tabular: Props_tabular,
	-- Underlined text (inherited, as CSS's `text-decoration`), and how far
	-- below the baseline the line sits.
	underline: Props_underline,
	underline_offset: Props_underline_offset,
	-- Transforms: after layout, about `origin` (fractions of the node).
	x: Props_x,
	y: Props_y,
	scale: Props_scale,
	scale_x: Props_scale_x,
	scale_y: Props_scale_y,
	rotate: Props_rotate,
	skew: Props_skew,
	skew_x: Props_skew_x,
	skew_y: Props_skew_y,
	origin: Props_origin,
	-- The subtree's theme: a theme's name (`"zgx-light"`), or tokens to
	-- override (`{ accent = "#e5484d" }`, with `theme = name` to start from).
	theme: Props_theme,
	-- Identity, for selectors, refs and animations.
	id: Props_id,
	-- Space-separated class names.
	class: Props_class,
	-- Styles chosen from a `ui.variants` definition: `button { tone =
	-- "ghost" }`, or the recipe itself for its defaults. The element's own
	-- properties win over them.
	variants: Props_variants,
	ref: Props_ref,
	focusable: Props_focusable,
	-- What assistive technology takes it for (ARIA's `role`), instead of
	-- what it is: a clickable row as an "option", a row of buttons as a
	-- "toolbar". It keeps what it does.
	role: Props_role,
	-- A section title of this level, 1 to 6 (`h1`…`h6`): screen readers
	-- list headings and move between them.
	heading: Props_heading,
	-- Whether this option, tab or item is the chosen one (`aria-selected`).
	selected: Props_selected,
	-- Whether a switch, checkbox or radio of the app's own is on
	-- (`aria-checked`), and whether what a control opens shows
	-- (`aria-expanded`): a custom disclosure's.
	checked: Props_checked,
	expanded: Props_expanded,
	-- Announce changes to the text in it (`aria-live`): a count, a "Sent"
	-- note, a message arriving.
	live: Props_live,
	-- Read after its name, as a hint (`aria-description`).
	description: Props_description,
	-- What is wrong with its value (`aria-invalid`), followed: nil while it
	-- is fine. Assistive technology hears it with the field.
	error: Props_error,
	-- Left out of the accessibility tree with what it holds (`aria-hidden`):
	-- decoration, or what is said elsewhere.
	aria_hidden: Props_aria_hidden,
	-- One Tab stop for the focusable things inside; the arrows along this
	-- axis (and Home, End) move among them, as ARIA's composite widgets
	-- behave. `role = "listbox"` is vertical, "toolbar" and "tablist"
	-- horizontal, without asking.
	focus_group: Props_focus_group,
	-- A press on it (or on what inside takes no input) moves the window; a
	-- double click zooms it. Buttons and inputs inside keep their clicks.
	-- CSS pointer-events: "none" (or false) lets the pointer through to what
	-- is under the node (presses, hover, the wheel, drops, a native view);
	-- descendants with handlers still take it. A state or function follows.
	pointer_events: Props_pointer_events,
	window_drag: Props_window_drag,
	-- The pointer selects the text in it and everything it holds, as on a
	-- web page (CSS `user-select: text`): drag, double/triple press, Shift,
	-- ⌘A / Ctrl+A, ⌘C / Ctrl+C. `false` keeps a part out. Off unless set.
	selectable: Props_selectable,
	-- A selectable node's highlight and selected-text colours (CSS
	-- `::selection`), over the theme's `$selection` and `$selection.text`
	-- tokens; a state or function follows.
	selection_color: Props_selection_color,
	selected_text_color: Props_selected_text_color,
	-- Shown over it while the pointer rests on it (after 300 ms; at once
	-- within a second of another): text in the theme's look, or a node of
	-- the app's own. Above it, centred, unless `tooltip_placement` says.
	tooltip: Props_tooltip,
	tooltip_placement: Props_tooltip_placement,
	-- A hover card the pointer can move into and click in: it stays while
	-- the pointer is over the node or the card.
	tooltip_interactive: Props_tooltip_interactive,
	-- Milliseconds the pointer rests before it shows (300; once one
	-- shows, the next shows at once). Keyboard focus shows it too.
	tooltip_delay: Props_tooltip_delay,
	-- A condition (true disables), or the styles it shows while disabled.
	disabled: Props_disabled,
	hover: Props_hover,
	active: Props_active,
	focus: Props_focus,
	-- Size animation ("height", "width", "size"; `true` is "height") or
	-- position animation ("position"). A `layout_id` node mounting as
	-- another with the same id unmounts (or while it is mounted) moves and
	-- scales from that one's box.
	layout: Props_layout,
	layout_id: Props_layout_id,
	layout_ease: Props_layout_ease,
	-- Milliseconds.
	layout_duration: Props_layout_duration,
	layout_enter: Props_layout_enter,
	layout_clip: Props_layout_clip,
	-- The content shows through the bitmap's alpha: `mask = bitmap`, or
	-- `{ bitmap, w = …, h = …, x = …, y = …, sweep = … }`.
	mask: Props_mask,
	bg_shader: Props_bg_shader,
	-- Up to four vec4s: lists of numbers, or colors (0 to 1).
	shader_params: Props_shader_params,
	-- Events.
	on_click: Props_on_click,
	-- Primary pointer down and up.
	on_press: Props_on_press,
	on_release: Props_on_release,
	on_hover: Props_on_hover,
	on_pointer_move: Props_on_pointer_move,
	-- Return true to consume the key.
	on_key: Props_on_key,
	on_key_up: Props_on_key_up,
	on_focus: Props_on_focus,
	on_blur: Props_on_blur,
	-- Return true to take the gesture from the scroll views around it.
	on_wheel: Props_on_wheel,
	on_pinch: Props_on_pinch,
	on_mount: Props_on_mount,
	-- When layout gives it a new size, and at its first layout.
	on_resize: Props_on_resize,
	-- When any of it comes into view or all of it leaves (inside every
	-- clip and scroll viewport above it), and at its first layout: for
	-- building what only matters on screen.
	on_visible: Props_on_visible,
	-- Drag and drop within the window. Payloads are what `on_drag`
	-- returns: the script's own plain values, so they are `any` here.
	-- A source: `on_drag` returns the payload once the pointer moves 4
	-- points; `drag_preview` builds what follows the pointer.
	on_drag: Props_on_drag,
	drag_preview: Props_drag_preview,
	on_drag_end: Props_on_drag_end,
	-- What the drag carries to other apps as it leaves the window (macOS):
	-- Finder takes files, Mail text, a browser links.
	drag_export: Props_drag_export,
	-- Released outside every window of the app (macOS): a tab torn off its
	-- window. `grab_x`/`grab_y` are where the source was held.
	on_drag_outside: Props_on_drag_outside,
	-- The drag moves its window instead, with no preview (macOS): a
	-- window's only tab. Dropped on another window's destination it drops
	-- there; elsewhere the window stays where it was let go.
	drag_moves_window: Props_drag_moves_window,
	-- A destination: `drop_when` filters payloads (default: all).
	drop_when: Props_drop_when,
	on_drag_over: Props_on_drag_over,
	on_drag_leave: Props_on_drag_leave,
	on_drop: Props_on_drop,
	-- A secondary click (control-click on macOS) shows the returned
	-- `menu_item`s, `submenu`s and separators as a menu at the pointer;
	-- returning nil (or nothing) shows none.
	context_menu: Props_context_menu,
	-- The context menu panel's look: bg, rounded, p, gap, backdrop_blur,
	-- border, shadow, w, …
	context_menu_style: Props_context_menu_style,
	-- Open the context menu under the node, from its left edge (a tab's),
	-- rather than at the pointer.
	context_menu_below: Props_context_menu_below,
	[number]: never,
	vertical: boolean?,
}
-- A tab of `ui.tabs`: its `id` (unique), what it shows, whether it closes;
-- other fields are the app's own, kept as the tabs move and close.
export type TabItem = { id: string, label: string, closable: boolean? }
-- Tabs choosing a panel: one Tab stop, the arrows choose; closable tabs
-- close with their ×, Delete or a middle click; reorderable ones drag, or
-- move with ⌘ (Ctrl) Shift and an arrow; too many to fit scroll, with a
-- menu of them all at the strip's end.
export type TabsProps = {
	-- Size and position.
	w: Props_w,
	h: Props_h,
	size: Props_size,
	min_w: Props_min_w,
	max_w: Props_max_w,
	min_h: Props_min_h,
	max_h: Props_max_h,
	w_full: Props_w_full,
	h_full: Props_h_full,
	aspect: Props_aspect,
	absolute: Props_absolute,
	relative: Props_relative,
	top: Props_top,
	left: Props_left,
	inset_start: Props_inset_start,
	inset_end: Props_inset_end,
	right: Props_right,
	bottom: Props_bottom,
	-- Spacing.
	p: Props_p,
	px: Props_px,
	py: Props_py,
	pt: Props_pt,
	pr: Props_pr,
	pb: Props_pb,
	pl: Props_pl,
	ps: Props_ps,
	pe: Props_pe,
	m: Props_m,
	mx: Props_mx,
	my: Props_my,
	mt: Props_mt,
	mr: Props_mr,
	mb: Props_mb,
	ml: Props_ml,
	ms: Props_ms,
	me: Props_me,
	gap: Props_gap,
	gap_x: Props_gap_x,
	gap_y: Props_gap_y,
	-- Flex and grid.
	direction: Props_direction,
	dir: Props_dir,
	grow: Props_grow,
	shrink: Props_shrink,
	basis: Props_basis,
	flex_wrap: Props_flex_wrap,
	wrap_reverse: Props_wrap_reverse,
	justify: Props_justify,
	self: Props_self,
	content: Props_content,
	grid: Props_grid,
	cols: Props_cols,
	rows: Props_rows,
	col_start: Props_col_start,
	row_start: Props_row_start,
	col_span: Props_col_span,
	row_span: Props_row_span,
	col_span_full: Props_col_span_full,
	row_span_full: Props_row_span_full,
	overflow: Props_overflow,
	overflow_x: Props_overflow_x,
	overflow_y: Props_overflow_y,
	hidden: Props_hidden,
	invisible: Props_invisible,
	-- Paint.
	bg: Props_bg,
	gradient: Props_gradient,
	color: Props_color,
	bg_image: Props_bg_image,
	bg_fit: Props_bg_fit,
	rounded: Props_rounded,
	corners: Props_corners,
	rounded_tl: Props_rounded_tl,
	rounded_tr: Props_rounded_tr,
	rounded_br: Props_rounded_br,
	rounded_bl: Props_rounded_bl,
	border: Props_border,
	border_sides: Props_border_sides,
	border_color: Props_border_color,
	border_over: Props_border_over,
	-- A function may return nil: a solid border.
	border_dash: Props_border_dash,
	outline: Props_outline,
	outline_color: Props_outline_color,
	outline_offset: Props_outline_offset,
	shadow: Props_shadow,
	inset_shadow: Props_inset_shadow,
	opacity: Props_opacity,
	brightness: Props_brightness,
	contrast: Props_contrast,
	saturate: Props_saturate,
	blur: Props_blur,
	backdrop_blur: Props_backdrop_blur,
	fade: Props_fade,
	fade_x: Props_fade_x,
	fade_sides: Props_fade_sides,
	fade_overflow: Props_fade_overflow,
	snap: Props_snap,
	clip_inset: Props_clip_inset,
	layer: Props_layer,
	fit: Props_fit,
	tint: Props_tint,
	cursor: Props_cursor,
	-- Text.
	text_size: Props_text_size,
	weight: Props_weight,
	bold: Props_bold,
	italic: Props_italic,
	align: Props_align,
	font: Props_font,
	wrap: Props_wrap,
	truncate: Props_truncate,
	lines: Props_lines,
	line_height: Props_line_height,
	letter_spacing: Props_letter_spacing,
	-- Tabular figures: every digit the same width.
	tabular: Props_tabular,
	-- Underlined text (inherited, as CSS's `text-decoration`), and how far
	-- below the baseline the line sits.
	underline: Props_underline,
	underline_offset: Props_underline_offset,
	-- Transforms: after layout, about `origin` (fractions of the node).
	x: Props_x,
	y: Props_y,
	scale: Props_scale,
	scale_x: Props_scale_x,
	scale_y: Props_scale_y,
	rotate: Props_rotate,
	skew: Props_skew,
	skew_x: Props_skew_x,
	skew_y: Props_skew_y,
	origin: Props_origin,
	-- The subtree's theme: a theme's name (`"zgx-light"`), or tokens to
	-- override (`{ accent = "#e5484d" }`, with `theme = name` to start from).
	theme: Props_theme,
	-- Identity, for selectors, refs and animations.
	id: Props_id,
	-- Space-separated class names.
	class: Props_class,
	-- Styles chosen from a `ui.variants` definition: `button { tone =
	-- "ghost" }`, or the recipe itself for its defaults. The element's own
	-- properties win over them.
	variants: Props_variants,
	ref: Props_ref,
	focusable: Props_focusable,
	-- What assistive technology takes it for (ARIA's `role`), instead of
	-- what it is: a clickable row as an "option", a row of buttons as a
	-- "toolbar". It keeps what it does.
	role: Props_role,
	-- A section title of this level, 1 to 6 (`h1`…`h6`): screen readers
	-- list headings and move between them.
	heading: Props_heading,
	-- Whether this option, tab or item is the chosen one (`aria-selected`).
	selected: Props_selected,
	-- Whether a switch, checkbox or radio of the app's own is on
	-- (`aria-checked`), and whether what a control opens shows
	-- (`aria-expanded`): a custom disclosure's.
	checked: Props_checked,
	expanded: Props_expanded,
	-- Announce changes to the text in it (`aria-live`): a count, a "Sent"
	-- note, a message arriving.
	live: Props_live,
	-- Read after its name, as a hint (`aria-description`).
	description: Props_description,
	-- What is wrong with its value (`aria-invalid`), followed: nil while it
	-- is fine. Assistive technology hears it with the field.
	error: Props_error,
	-- Left out of the accessibility tree with what it holds (`aria-hidden`):
	-- decoration, or what is said elsewhere.
	aria_hidden: Props_aria_hidden,
	-- One Tab stop for the focusable things inside; the arrows along this
	-- axis (and Home, End) move among them, as ARIA's composite widgets
	-- behave. `role = "listbox"` is vertical, "toolbar" and "tablist"
	-- horizontal, without asking.
	focus_group: Props_focus_group,
	-- A press on it (or on what inside takes no input) moves the window; a
	-- double click zooms it. Buttons and inputs inside keep their clicks.
	-- CSS pointer-events: "none" (or false) lets the pointer through to what
	-- is under the node (presses, hover, the wheel, drops, a native view);
	-- descendants with handlers still take it. A state or function follows.
	pointer_events: Props_pointer_events,
	window_drag: Props_window_drag,
	-- The pointer selects the text in it and everything it holds, as on a
	-- web page (CSS `user-select: text`): drag, double/triple press, Shift,
	-- ⌘A / Ctrl+A, ⌘C / Ctrl+C. `false` keeps a part out. Off unless set.
	selectable: Props_selectable,
	-- A selectable node's highlight and selected-text colours (CSS
	-- `::selection`), over the theme's `$selection` and `$selection.text`
	-- tokens; a state or function follows.
	selection_color: Props_selection_color,
	selected_text_color: Props_selected_text_color,
	-- Shown over it while the pointer rests on it (after 300 ms; at once
	-- within a second of another): text in the theme's look, or a node of
	-- the app's own. Above it, centred, unless `tooltip_placement` says.
	tooltip: Props_tooltip,
	tooltip_placement: Props_tooltip_placement,
	-- A hover card the pointer can move into and click in: it stays while
	-- the pointer is over the node or the card.
	tooltip_interactive: Props_tooltip_interactive,
	-- Milliseconds the pointer rests before it shows (300; once one
	-- shows, the next shows at once). Keyboard focus shows it too.
	tooltip_delay: Props_tooltip_delay,
	-- A condition (true disables), or the styles it shows while disabled.
	disabled: Props_disabled,
	hover: Props_hover,
	active: Props_active,
	focus: Props_focus,
	-- Size animation ("height", "width", "size"; `true` is "height") or
	-- position animation ("position"). A `layout_id` node mounting as
	-- another with the same id unmounts (or while it is mounted) moves and
	-- scales from that one's box.
	layout: Props_layout,
	layout_id: Props_layout_id,
	layout_ease: Props_layout_ease,
	-- Milliseconds.
	layout_duration: Props_layout_duration,
	layout_enter: Props_layout_enter,
	layout_clip: Props_layout_clip,
	-- The content shows through the bitmap's alpha: `mask = bitmap`, or
	-- `{ bitmap, w = …, h = …, x = …, y = …, sweep = … }`.
	mask: Props_mask,
	bg_shader: Props_bg_shader,
	-- Up to four vec4s: lists of numbers, or colors (0 to 1).
	shader_params: Props_shader_params,
	-- Events.
	on_click: Props_on_click,
	-- Primary pointer down and up.
	on_press: Props_on_press,
	on_release: Props_on_release,
	on_hover: Props_on_hover,
	on_pointer_move: Props_on_pointer_move,
	-- Return true to consume the key.
	on_key: Props_on_key,
	on_key_up: Props_on_key_up,
	on_focus: Props_on_focus,
	on_blur: Props_on_blur,
	-- Return true to take the gesture from the scroll views around it.
	on_wheel: Props_on_wheel,
	on_pinch: Props_on_pinch,
	on_mount: Props_on_mount,
	-- When layout gives it a new size, and at its first layout.
	on_resize: Props_on_resize,
	-- When any of it comes into view or all of it leaves (inside every
	-- clip and scroll viewport above it), and at its first layout: for
	-- building what only matters on screen.
	on_visible: Props_on_visible,
	-- Drag and drop within the window. Payloads are what `on_drag`
	-- returns: the script's own plain values, so they are `any` here.
	-- A source: `on_drag` returns the payload once the pointer moves 4
	-- points; `drag_preview` builds what follows the pointer.
	on_drag: Props_on_drag,
	drag_preview: Props_drag_preview,
	on_drag_end: Props_on_drag_end,
	-- What the drag carries to other apps as it leaves the window (macOS):
	-- Finder takes files, Mail text, a browser links.
	drag_export: Props_drag_export,
	-- Released outside every window of the app (macOS): a tab torn off its
	-- window. `grab_x`/`grab_y` are where the source was held.
	on_drag_outside: Props_on_drag_outside,
	-- The drag moves its window instead, with no preview (macOS): a
	-- window's only tab. Dropped on another window's destination it drops
	-- there; elsewhere the window stays where it was let go.
	drag_moves_window: Props_drag_moves_window,
	-- A destination: `drop_when` filters payloads (default: all).
	drop_when: Props_drop_when,
	on_drag_over: Props_on_drag_over,
	on_drag_leave: Props_on_drag_leave,
	on_drop: Props_on_drop,
	-- A secondary click (control-click on macOS) shows the returned
	-- `menu_item`s, `submenu`s and separators as a menu at the pointer;
	-- returning nil (or nothing) shows none.
	context_menu: Props_context_menu,
	-- The context menu panel's look: bg, rounded, p, gap, backdrop_blur,
	-- border, shadow, w, …
	context_menu_style: Props_context_menu_style,
	-- Open the context menu under the node, from its left edge (a tab's),
	-- rather than at the pointer.
	context_menu_below: Props_context_menu_below,
	[number]: never,
	-- The tabs, two-way: closing removes one, reordering moves it.
	items: State<{ TabItem }>,
	-- The chosen tab's id; closing it chooses its neighbour.
	value: State<string>,
	-- The chosen tab's panel, built as it is chosen.
	panel: (id: string) -> Node,
	reorderable: boolean?,
	-- Asked before a tab closes: false keeps it (an editor asking to save).
	on_close: ((id: string) -> boolean?)?,
	label: string?,
}
-- An item of `ui.tree`: `children` (items) open under it; `lazy = true`
-- without children: loading until the app adds them (`on_open`).
export type TreeNode = { id: string, label: string?, children: { TreeNode }?, lazy: boolean? }
-- A tree (WAI-ARIA's): one Tab stop keeping focus; Up and Down move, Right
-- opens then enters, Left closes then goes up, Home, End, typeahead, Enter
-- activates. Virtual rows.
export type TreeProps = {
	-- Size and position.
	w: Props_w,
	h: Props_h,
	size: Props_size,
	min_w: Props_min_w,
	max_w: Props_max_w,
	min_h: Props_min_h,
	max_h: Props_max_h,
	w_full: Props_w_full,
	h_full: Props_h_full,
	aspect: Props_aspect,
	absolute: Props_absolute,
	relative: Props_relative,
	top: Props_top,
	left: Props_left,
	inset_start: Props_inset_start,
	inset_end: Props_inset_end,
	right: Props_right,
	bottom: Props_bottom,
	-- Spacing.
	p: Props_p,
	px: Props_px,
	py: Props_py,
	pt: Props_pt,
	pr: Props_pr,
	pb: Props_pb,
	pl: Props_pl,
	ps: Props_ps,
	pe: Props_pe,
	m: Props_m,
	mx: Props_mx,
	my: Props_my,
	mt: Props_mt,
	mr: Props_mr,
	mb: Props_mb,
	ml: Props_ml,
	ms: Props_ms,
	me: Props_me,
	gap: Props_gap,
	gap_x: Props_gap_x,
	gap_y: Props_gap_y,
	-- Flex and grid.
	direction: Props_direction,
	dir: Props_dir,
	grow: Props_grow,
	shrink: Props_shrink,
	basis: Props_basis,
	flex_wrap: Props_flex_wrap,
	wrap_reverse: Props_wrap_reverse,
	justify: Props_justify,
	self: Props_self,
	content: Props_content,
	grid: Props_grid,
	cols: Props_cols,
	rows: Props_rows,
	col_start: Props_col_start,
	row_start: Props_row_start,
	col_span: Props_col_span,
	row_span: Props_row_span,
	col_span_full: Props_col_span_full,
	row_span_full: Props_row_span_full,
	overflow: Props_overflow,
	overflow_x: Props_overflow_x,
	overflow_y: Props_overflow_y,
	hidden: Props_hidden,
	invisible: Props_invisible,
	-- Paint.
	bg: Props_bg,
	gradient: Props_gradient,
	color: Props_color,
	bg_image: Props_bg_image,
	bg_fit: Props_bg_fit,
	rounded: Props_rounded,
	corners: Props_corners,
	rounded_tl: Props_rounded_tl,
	rounded_tr: Props_rounded_tr,
	rounded_br: Props_rounded_br,
	rounded_bl: Props_rounded_bl,
	border: Props_border,
	border_sides: Props_border_sides,
	border_color: Props_border_color,
	border_over: Props_border_over,
	-- A function may return nil: a solid border.
	border_dash: Props_border_dash,
	outline: Props_outline,
	outline_color: Props_outline_color,
	outline_offset: Props_outline_offset,
	shadow: Props_shadow,
	inset_shadow: Props_inset_shadow,
	opacity: Props_opacity,
	brightness: Props_brightness,
	contrast: Props_contrast,
	saturate: Props_saturate,
	blur: Props_blur,
	backdrop_blur: Props_backdrop_blur,
	fade: Props_fade,
	fade_x: Props_fade_x,
	fade_sides: Props_fade_sides,
	fade_overflow: Props_fade_overflow,
	snap: Props_snap,
	clip_inset: Props_clip_inset,
	layer: Props_layer,
	fit: Props_fit,
	tint: Props_tint,
	cursor: Props_cursor,
	-- Text.
	text_size: Props_text_size,
	weight: Props_weight,
	bold: Props_bold,
	italic: Props_italic,
	align: Props_align,
	font: Props_font,
	wrap: Props_wrap,
	truncate: Props_truncate,
	lines: Props_lines,
	line_height: Props_line_height,
	letter_spacing: Props_letter_spacing,
	-- Tabular figures: every digit the same width.
	tabular: Props_tabular,
	-- Underlined text (inherited, as CSS's `text-decoration`), and how far
	-- below the baseline the line sits.
	underline: Props_underline,
	underline_offset: Props_underline_offset,
	-- Transforms: after layout, about `origin` (fractions of the node).
	x: Props_x,
	y: Props_y,
	scale: Props_scale,
	scale_x: Props_scale_x,
	scale_y: Props_scale_y,
	rotate: Props_rotate,
	skew: Props_skew,
	skew_x: Props_skew_x,
	skew_y: Props_skew_y,
	origin: Props_origin,
	-- The subtree's theme: a theme's name (`"zgx-light"`), or tokens to
	-- override (`{ accent = "#e5484d" }`, with `theme = name` to start from).
	theme: Props_theme,
	-- Identity, for selectors, refs and animations.
	id: Props_id,
	-- Space-separated class names.
	class: Props_class,
	-- Styles chosen from a `ui.variants` definition: `button { tone =
	-- "ghost" }`, or the recipe itself for its defaults. The element's own
	-- properties win over them.
	variants: Props_variants,
	ref: Props_ref,
	focusable: Props_focusable,
	-- What assistive technology takes it for (ARIA's `role`), instead of
	-- what it is: a clickable row as an "option", a row of buttons as a
	-- "toolbar". It keeps what it does.
	role: Props_role,
	-- A section title of this level, 1 to 6 (`h1`…`h6`): screen readers
	-- list headings and move between them.
	heading: Props_heading,
	-- Whether this option, tab or item is the chosen one (`aria-selected`).
	selected: Props_selected,
	-- Whether a switch, checkbox or radio of the app's own is on
	-- (`aria-checked`), and whether what a control opens shows
	-- (`aria-expanded`): a custom disclosure's.
	checked: Props_checked,
	expanded: Props_expanded,
	-- Announce changes to the text in it (`aria-live`): a count, a "Sent"
	-- note, a message arriving.
	live: Props_live,
	-- Read after its name, as a hint (`aria-description`).
	description: Props_description,
	-- What is wrong with its value (`aria-invalid`), followed: nil while it
	-- is fine. Assistive technology hears it with the field.
	error: Props_error,
	-- Left out of the accessibility tree with what it holds (`aria-hidden`):
	-- decoration, or what is said elsewhere.
	aria_hidden: Props_aria_hidden,
	-- One Tab stop for the focusable things inside; the arrows along this
	-- axis (and Home, End) move among them, as ARIA's composite widgets
	-- behave. `role = "listbox"` is vertical, "toolbar" and "tablist"
	-- horizontal, without asking.
	focus_group: Props_focus_group,
	-- A press on it (or on what inside takes no input) moves the window; a
	-- double click zooms it. Buttons and inputs inside keep their clicks.
	-- CSS pointer-events: "none" (or false) lets the pointer through to what
	-- is under the node (presses, hover, the wheel, drops, a native view);
	-- descendants with handlers still take it. A state or function follows.
	pointer_events: Props_pointer_events,
	window_drag: Props_window_drag,
	-- The pointer selects the text in it and everything it holds, as on a
	-- web page (CSS `user-select: text`): drag, double/triple press, Shift,
	-- ⌘A / Ctrl+A, ⌘C / Ctrl+C. `false` keeps a part out. Off unless set.
	selectable: Props_selectable,
	-- A selectable node's highlight and selected-text colours (CSS
	-- `::selection`), over the theme's `$selection` and `$selection.text`
	-- tokens; a state or function follows.
	selection_color: Props_selection_color,
	selected_text_color: Props_selected_text_color,
	-- Shown over it while the pointer rests on it (after 300 ms; at once
	-- within a second of another): text in the theme's look, or a node of
	-- the app's own. Above it, centred, unless `tooltip_placement` says.
	tooltip: Props_tooltip,
	tooltip_placement: Props_tooltip_placement,
	-- A hover card the pointer can move into and click in: it stays while
	-- the pointer is over the node or the card.
	tooltip_interactive: Props_tooltip_interactive,
	-- Milliseconds the pointer rests before it shows (300; once one
	-- shows, the next shows at once). Keyboard focus shows it too.
	tooltip_delay: Props_tooltip_delay,
	-- A condition (true disables), or the styles it shows while disabled.
	disabled: Props_disabled,
	hover: Props_hover,
	active: Props_active,
	focus: Props_focus,
	-- Size animation ("height", "width", "size"; `true` is "height") or
	-- position animation ("position"). A `layout_id` node mounting as
	-- another with the same id unmounts (or while it is mounted) moves and
	-- scales from that one's box.
	layout: Props_layout,
	layout_id: Props_layout_id,
	layout_ease: Props_layout_ease,
	-- Milliseconds.
	layout_duration: Props_layout_duration,
	layout_enter: Props_layout_enter,
	layout_clip: Props_layout_clip,
	-- The content shows through the bitmap's alpha: `mask = bitmap`, or
	-- `{ bitmap, w = …, h = …, x = …, y = …, sweep = … }`.
	mask: Props_mask,
	bg_shader: Props_bg_shader,
	-- Up to four vec4s: lists of numbers, or colors (0 to 1).
	shader_params: Props_shader_params,
	-- Events.
	on_click: Props_on_click,
	-- Primary pointer down and up.
	on_press: Props_on_press,
	on_release: Props_on_release,
	on_hover: Props_on_hover,
	on_pointer_move: Props_on_pointer_move,
	-- Return true to consume the key.
	on_key: Props_on_key,
	on_key_up: Props_on_key_up,
	on_focus: Props_on_focus,
	on_blur: Props_on_blur,
	-- Return true to take the gesture from the scroll views around it.
	on_wheel: Props_on_wheel,
	on_pinch: Props_on_pinch,
	on_mount: Props_on_mount,
	-- When layout gives it a new size, and at its first layout.
	on_resize: Props_on_resize,
	-- When any of it comes into view or all of it leaves (inside every
	-- clip and scroll viewport above it), and at its first layout: for
	-- building what only matters on screen.
	on_visible: Props_on_visible,
	-- Drag and drop within the window. Payloads are what `on_drag`
	-- returns: the script's own plain values, so they are `any` here.
	-- A source: `on_drag` returns the payload once the pointer moves 4
	-- points; `drag_preview` builds what follows the pointer.
	on_drag: Props_on_drag,
	drag_preview: Props_drag_preview,
	on_drag_end: Props_on_drag_end,
	-- What the drag carries to other apps as it leaves the window (macOS):
	-- Finder takes files, Mail text, a browser links.
	drag_export: Props_drag_export,
	-- Released outside every window of the app (macOS): a tab torn off its
	-- window. `grab_x`/`grab_y` are where the source was held.
	on_drag_outside: Props_on_drag_outside,
	-- The drag moves its window instead, with no preview (macOS): a
	-- window's only tab. Dropped on another window's destination it drops
	-- there; elsewhere the window stays where it was let go.
	drag_moves_window: Props_drag_moves_window,
	-- A destination: `drop_when` filters payloads (default: all).
	drop_when: Props_drop_when,
	on_drag_over: Props_on_drag_over,
	on_drag_leave: Props_on_drag_leave,
	on_drop: Props_on_drop,
	-- A secondary click (control-click on macOS) shows the returned
	-- `menu_item`s, `submenu`s and separators as a menu at the pointer;
	-- returning nil (or nothing) shows none.
	context_menu: Props_context_menu,
	-- The context menu panel's look: bg, rounded, p, gap, backdrop_blur,
	-- border, shadow, w, …
	context_menu_style: Props_context_menu_style,
	-- Open the context menu under the node, from its left edge (a tab's),
	-- rather than at the pointer.
	context_menu_below: Props_context_menu_below,
	[number]: never,
	items: State<{ TreeNode }> | State<{ any }>,
	-- The open items' ids, both ways.
	open: State<{ string }>,
	-- The current item's id, both ways.
	value: State<string?> | State<any>,
	-- An item opened: load its children now (a lazy one).
	on_open: ((id: string) -> ...unknown)?,
	-- Enter or a double click.
	on_activate: ((id: string) -> ...unknown)?,
	label: string?,
}
-- A column of `ui.data_grid`: its cells show each row's field `id`.
export type GridColumn = {
	id: string,
	title: string?,
	width: number?,
	min_width: number?,
	sortable: boolean?,
	editable: boolean?,
	align: ("start" | "end")?,
}
export type GridSort = { column: string, descending: boolean? }
-- A data grid (WAI-ARIA's grid): one Tab stop keeping focus; the arrows,
-- Home, End and Page keys move; Enter, F2 or typing edits, Enter keeps and
-- Escape drops; Up from the first row reaches the header, where Enter
-- sorts. A header's click sorts, its drag moves the column, its edge's drag
-- resizes it. Virtual rows.
export type DataGridProps = {
	-- Size and position.
	w: Props_w,
	h: Props_h,
	size: Props_size,
	min_w: Props_min_w,
	max_w: Props_max_w,
	min_h: Props_min_h,
	max_h: Props_max_h,
	w_full: Props_w_full,
	h_full: Props_h_full,
	aspect: Props_aspect,
	absolute: Props_absolute,
	relative: Props_relative,
	top: Props_top,
	left: Props_left,
	inset_start: Props_inset_start,
	inset_end: Props_inset_end,
	right: Props_right,
	bottom: Props_bottom,
	-- Spacing.
	p: Props_p,
	px: Props_px,
	py: Props_py,
	pt: Props_pt,
	pr: Props_pr,
	pb: Props_pb,
	pl: Props_pl,
	ps: Props_ps,
	pe: Props_pe,
	m: Props_m,
	mx: Props_mx,
	my: Props_my,
	mt: Props_mt,
	mr: Props_mr,
	mb: Props_mb,
	ml: Props_ml,
	ms: Props_ms,
	me: Props_me,
	gap: Props_gap,
	gap_x: Props_gap_x,
	gap_y: Props_gap_y,
	-- Flex and grid.
	direction: Props_direction,
	dir: Props_dir,
	grow: Props_grow,
	shrink: Props_shrink,
	basis: Props_basis,
	flex_wrap: Props_flex_wrap,
	wrap_reverse: Props_wrap_reverse,
	items: Props_items,
	justify: Props_justify,
	self: Props_self,
	content: Props_content,
	grid: Props_grid,
	cols: Props_cols,
	col_start: Props_col_start,
	row_start: Props_row_start,
	col_span: Props_col_span,
	row_span: Props_row_span,
	col_span_full: Props_col_span_full,
	row_span_full: Props_row_span_full,
	overflow: Props_overflow,
	overflow_x: Props_overflow_x,
	overflow_y: Props_overflow_y,
	hidden: Props_hidden,
	invisible: Props_invisible,
	-- Paint.
	bg: Props_bg,
	gradient: Props_gradient,
	color: Props_color,
	bg_image: Props_bg_image,
	bg_fit: Props_bg_fit,
	rounded: Props_rounded,
	corners: Props_corners,
	rounded_tl: Props_rounded_tl,
	rounded_tr: Props_rounded_tr,
	rounded_br: Props_rounded_br,
	rounded_bl: Props_rounded_bl,
	border: Props_border,
	border_sides: Props_border_sides,
	border_color: Props_border_color,
	border_over: Props_border_over,
	-- A function may return nil: a solid border.
	border_dash: Props_border_dash,
	outline: Props_outline,
	outline_color: Props_outline_color,
	outline_offset: Props_outline_offset,
	shadow: Props_shadow,
	inset_shadow: Props_inset_shadow,
	opacity: Props_opacity,
	brightness: Props_brightness,
	contrast: Props_contrast,
	saturate: Props_saturate,
	blur: Props_blur,
	backdrop_blur: Props_backdrop_blur,
	fade: Props_fade,
	fade_x: Props_fade_x,
	fade_sides: Props_fade_sides,
	fade_overflow: Props_fade_overflow,
	snap: Props_snap,
	clip_inset: Props_clip_inset,
	layer: Props_layer,
	fit: Props_fit,
	tint: Props_tint,
	cursor: Props_cursor,
	-- Text.
	text_size: Props_text_size,
	weight: Props_weight,
	bold: Props_bold,
	italic: Props_italic,
	align: Props_align,
	font: Props_font,
	wrap: Props_wrap,
	truncate: Props_truncate,
	lines: Props_lines,
	line_height: Props_line_height,
	letter_spacing: Props_letter_spacing,
	-- Tabular figures: every digit the same width.
	tabular: Props_tabular,
	-- Underlined text (inherited, as CSS's `text-decoration`), and how far
	-- below the baseline the line sits.
	underline: Props_underline,
	underline_offset: Props_underline_offset,
	-- Transforms: after layout, about `origin` (fractions of the node).
	x: Props_x,
	y: Props_y,
	scale: Props_scale,
	scale_x: Props_scale_x,
	scale_y: Props_scale_y,
	rotate: Props_rotate,
	skew: Props_skew,
	skew_x: Props_skew_x,
	skew_y: Props_skew_y,
	origin: Props_origin,
	-- The subtree's theme: a theme's name (`"zgx-light"`), or tokens to
	-- override (`{ accent = "#e5484d" }`, with `theme = name` to start from).
	theme: Props_theme,
	-- Identity, for selectors, refs and animations.
	id: Props_id,
	-- Space-separated class names.
	class: Props_class,
	-- Styles chosen from a `ui.variants` definition: `button { tone =
	-- "ghost" }`, or the recipe itself for its defaults. The element's own
	-- properties win over them.
	variants: Props_variants,
	ref: Props_ref,
	focusable: Props_focusable,
	-- What assistive technology takes it for (ARIA's `role`), instead of
	-- what it is: a clickable row as an "option", a row of buttons as a
	-- "toolbar". It keeps what it does.
	role: Props_role,
	-- A section title of this level, 1 to 6 (`h1`…`h6`): screen readers
	-- list headings and move between them.
	heading: Props_heading,
	-- Whether this option, tab or item is the chosen one (`aria-selected`).
	selected: Props_selected,
	-- Whether a switch, checkbox or radio of the app's own is on
	-- (`aria-checked`), and whether what a control opens shows
	-- (`aria-expanded`): a custom disclosure's.
	checked: Props_checked,
	expanded: Props_expanded,
	-- Announce changes to the text in it (`aria-live`): a count, a "Sent"
	-- note, a message arriving.
	live: Props_live,
	-- Read after its name, as a hint (`aria-description`).
	description: Props_description,
	-- What is wrong with its value (`aria-invalid`), followed: nil while it
	-- is fine. Assistive technology hears it with the field.
	error: Props_error,
	-- Left out of the accessibility tree with what it holds (`aria-hidden`):
	-- decoration, or what is said elsewhere.
	aria_hidden: Props_aria_hidden,
	-- One Tab stop for the focusable things inside; the arrows along this
	-- axis (and Home, End) move among them, as ARIA's composite widgets
	-- behave. `role = "listbox"` is vertical, "toolbar" and "tablist"
	-- horizontal, without asking.
	focus_group: Props_focus_group,
	-- A press on it (or on what inside takes no input) moves the window; a
	-- double click zooms it. Buttons and inputs inside keep their clicks.
	-- CSS pointer-events: "none" (or false) lets the pointer through to what
	-- is under the node (presses, hover, the wheel, drops, a native view);
	-- descendants with handlers still take it. A state or function follows.
	pointer_events: Props_pointer_events,
	window_drag: Props_window_drag,
	-- The pointer selects the text in it and everything it holds, as on a
	-- web page (CSS `user-select: text`): drag, double/triple press, Shift,
	-- ⌘A / Ctrl+A, ⌘C / Ctrl+C. `false` keeps a part out. Off unless set.
	selectable: Props_selectable,
	-- A selectable node's highlight and selected-text colours (CSS
	-- `::selection`), over the theme's `$selection` and `$selection.text`
	-- tokens; a state or function follows.
	selection_color: Props_selection_color,
	selected_text_color: Props_selected_text_color,
	-- Shown over it while the pointer rests on it (after 300 ms; at once
	-- within a second of another): text in the theme's look, or a node of
	-- the app's own. Above it, centred, unless `tooltip_placement` says.
	tooltip: Props_tooltip,
	tooltip_placement: Props_tooltip_placement,
	-- A hover card the pointer can move into and click in: it stays while
	-- the pointer is over the node or the card.
	tooltip_interactive: Props_tooltip_interactive,
	-- Milliseconds the pointer rests before it shows (300; once one
	-- shows, the next shows at once). Keyboard focus shows it too.
	tooltip_delay: Props_tooltip_delay,
	-- A condition (true disables), or the styles it shows while disabled.
	disabled: Props_disabled,
	hover: Props_hover,
	active: Props_active,
	focus: Props_focus,
	-- Size animation ("height", "width", "size"; `true` is "height") or
	-- position animation ("position"). A `layout_id` node mounting as
	-- another with the same id unmounts (or while it is mounted) moves and
	-- scales from that one's box.
	layout: Props_layout,
	layout_id: Props_layout_id,
	layout_ease: Props_layout_ease,
	-- Milliseconds.
	layout_duration: Props_layout_duration,
	layout_enter: Props_layout_enter,
	layout_clip: Props_layout_clip,
	-- The content shows through the bitmap's alpha: `mask = bitmap`, or
	-- `{ bitmap, w = …, h = …, x = …, y = …, sweep = … }`.
	mask: Props_mask,
	bg_shader: Props_bg_shader,
	-- Up to four vec4s: lists of numbers, or colors (0 to 1).
	shader_params: Props_shader_params,
	-- Events.
	on_click: Props_on_click,
	-- Primary pointer down and up.
	on_press: Props_on_press,
	on_release: Props_on_release,
	on_hover: Props_on_hover,
	on_pointer_move: Props_on_pointer_move,
	-- Return true to consume the key.
	on_key: Props_on_key,
	on_key_up: Props_on_key_up,
	on_focus: Props_on_focus,
	on_blur: Props_on_blur,
	-- Return true to take the gesture from the scroll views around it.
	on_wheel: Props_on_wheel,
	on_pinch: Props_on_pinch,
	on_mount: Props_on_mount,
	-- When layout gives it a new size, and at its first layout.
	on_resize: Props_on_resize,
	-- When any of it comes into view or all of it leaves (inside every
	-- clip and scroll viewport above it), and at its first layout: for
	-- building what only matters on screen.
	on_visible: Props_on_visible,
	-- Drag and drop within the window. Payloads are what `on_drag`
	-- returns: the script's own plain values, so they are `any` here.
	-- A source: `on_drag` returns the payload once the pointer moves 4
	-- points; `drag_preview` builds what follows the pointer.
	on_drag: Props_on_drag,
	drag_preview: Props_drag_preview,
	on_drag_end: Props_on_drag_end,
	-- What the drag carries to other apps as it leaves the window (macOS):
	-- Finder takes files, Mail text, a browser links.
	drag_export: Props_drag_export,
	-- Released outside every window of the app (macOS): a tab torn off its
	-- window. `grab_x`/`grab_y` are where the source was held.
	on_drag_outside: Props_on_drag_outside,
	-- The drag moves its window instead, with no preview (macOS): a
	-- window's only tab. Dropped on another window's destination it drops
	-- there; elsewhere the window stays where it was let go.
	drag_moves_window: Props_drag_moves_window,
	-- A destination: `drop_when` filters payloads (default: all).
	drop_when: Props_drop_when,
	on_drag_over: Props_on_drag_over,
	on_drag_leave: Props_on_drag_leave,
	on_drop: Props_on_drop,
	-- A secondary click (control-click on macOS) shows the returned
	-- `menu_item`s, `submenu`s and separators as a menu at the pointer;
	-- returning nil (or nothing) shows none.
	context_menu: Props_context_menu,
	-- The context menu panel's look: bg, rounded, p, gap, backdrop_blur,
	-- border, shadow, w, …
	context_menu_style: Props_context_menu_style,
	-- Open the context menu under the node, from its left edge (a tab's),
	-- rather than at the pointer.
	context_menu_below: Props_context_menu_below,
	[number]: never,
	-- A state is written as columns are resized and moved.
	columns: { GridColumn } | State<{ GridColumn }> | State<{ any }>,
	-- The records shown, sorted as `sort` says.
	rows: State<{ any }>,
	-- The sort, both ways: nil shows `rows` as they are.
	sort: (State<GridSort?> | State<any>)?,
	-- The current cell, both ways: its row an index into `rows`.
	value: (State<{ row: number, column: string }?> | State<any>)?,
	-- The selected rows, both ways: indices into `rows`.
	selection: (State<{ number }> | State<any>)?,
	-- A cell's text from its value (and column and record): `"$1.25"`.
	cell: ((value: any, column: string, record: any) -> string)?,
	-- An edit kept (else it is written into the row).
	on_edit: ((row: number, column: string, text: string) -> ...unknown)?,
	-- Enter on a cell that doesn't edit, or a double click.
	on_activate: ((row: number) -> ...unknown)?,
	label: string?,
}
-- A month of days to choose a date from (WAI-ARIA's date picker grid): one
-- Tab stop keeping focus; Left and Right move a day, Up and Down a week,
-- Home and End to the week's ends, Page Up and Down a month (Shift: a
-- year); Enter or Space chooses.
export type CalendarProps = {
	-- Size and position.
	w: Props_w,
	h: Props_h,
	size: Props_size,
	min_w: Props_min_w,
	max_w: Props_max_w,
	min_h: Props_min_h,
	max_h: Props_max_h,
	w_full: Props_w_full,
	h_full: Props_h_full,
	aspect: Props_aspect,
	absolute: Props_absolute,
	relative: Props_relative,
	top: Props_top,
	left: Props_left,
	inset_start: Props_inset_start,
	inset_end: Props_inset_end,
	right: Props_right,
	bottom: Props_bottom,
	-- Spacing.
	p: Props_p,
	px: Props_px,
	py: Props_py,
	pt: Props_pt,
	pr: Props_pr,
	pb: Props_pb,
	pl: Props_pl,
	ps: Props_ps,
	pe: Props_pe,
	m: Props_m,
	mx: Props_mx,
	my: Props_my,
	mt: Props_mt,
	mr: Props_mr,
	mb: Props_mb,
	ml: Props_ml,
	ms: Props_ms,
	me: Props_me,
	gap: Props_gap,
	gap_x: Props_gap_x,
	gap_y: Props_gap_y,
	-- Flex and grid.
	direction: Props_direction,
	dir: Props_dir,
	grow: Props_grow,
	shrink: Props_shrink,
	basis: Props_basis,
	flex_wrap: Props_flex_wrap,
	wrap_reverse: Props_wrap_reverse,
	items: Props_items,
	justify: Props_justify,
	self: Props_self,
	content: Props_content,
	grid: Props_grid,
	cols: Props_cols,
	rows: Props_rows,
	col_start: Props_col_start,
	row_start: Props_row_start,
	col_span: Props_col_span,
	row_span: Props_row_span,
	col_span_full: Props_col_span_full,
	row_span_full: Props_row_span_full,
	overflow: Props_overflow,
	overflow_x: Props_overflow_x,
	overflow_y: Props_overflow_y,
	hidden: Props_hidden,
	invisible: Props_invisible,
	-- Paint.
	bg: Props_bg,
	gradient: Props_gradient,
	color: Props_color,
	bg_image: Props_bg_image,
	bg_fit: Props_bg_fit,
	rounded: Props_rounded,
	corners: Props_corners,
	rounded_tl: Props_rounded_tl,
	rounded_tr: Props_rounded_tr,
	rounded_br: Props_rounded_br,
	rounded_bl: Props_rounded_bl,
	border: Props_border,
	border_sides: Props_border_sides,
	border_color: Props_border_color,
	border_over: Props_border_over,
	-- A function may return nil: a solid border.
	border_dash: Props_border_dash,
	outline: Props_outline,
	outline_color: Props_outline_color,
	outline_offset: Props_outline_offset,
	shadow: Props_shadow,
	inset_shadow: Props_inset_shadow,
	opacity: Props_opacity,
	brightness: Props_brightness,
	contrast: Props_contrast,
	saturate: Props_saturate,
	blur: Props_blur,
	backdrop_blur: Props_backdrop_blur,
	fade: Props_fade,
	fade_x: Props_fade_x,
	fade_sides: Props_fade_sides,
	fade_overflow: Props_fade_overflow,
	snap: Props_snap,
	clip_inset: Props_clip_inset,
	layer: Props_layer,
	fit: Props_fit,
	tint: Props_tint,
	cursor: Props_cursor,
	-- Text.
	text_size: Props_text_size,
	weight: Props_weight,
	bold: Props_bold,
	italic: Props_italic,
	align: Props_align,
	font: Props_font,
	wrap: Props_wrap,
	truncate: Props_truncate,
	lines: Props_lines,
	line_height: Props_line_height,
	letter_spacing: Props_letter_spacing,
	-- Tabular figures: every digit the same width.
	tabular: Props_tabular,
	-- Underlined text (inherited, as CSS's `text-decoration`), and how far
	-- below the baseline the line sits.
	underline: Props_underline,
	underline_offset: Props_underline_offset,
	-- Transforms: after layout, about `origin` (fractions of the node).
	x: Props_x,
	y: Props_y,
	scale: Props_scale,
	scale_x: Props_scale_x,
	scale_y: Props_scale_y,
	rotate: Props_rotate,
	skew: Props_skew,
	skew_x: Props_skew_x,
	skew_y: Props_skew_y,
	origin: Props_origin,
	-- The subtree's theme: a theme's name (`"zgx-light"`), or tokens to
	-- override (`{ accent = "#e5484d" }`, with `theme = name` to start from).
	theme: Props_theme,
	-- Identity, for selectors, refs and animations.
	id: Props_id,
	-- Space-separated class names.
	class: Props_class,
	-- Styles chosen from a `ui.variants` definition: `button { tone =
	-- "ghost" }`, or the recipe itself for its defaults. The element's own
	-- properties win over them.
	variants: Props_variants,
	ref: Props_ref,
	focusable: Props_focusable,
	-- What assistive technology takes it for (ARIA's `role`), instead of
	-- what it is: a clickable row as an "option", a row of buttons as a
	-- "toolbar". It keeps what it does.
	role: Props_role,
	-- A section title of this level, 1 to 6 (`h1`…`h6`): screen readers
	-- list headings and move between them.
	heading: Props_heading,
	-- Whether this option, tab or item is the chosen one (`aria-selected`).
	selected: Props_selected,
	-- Whether a switch, checkbox or radio of the app's own is on
	-- (`aria-checked`), and whether what a control opens shows
	-- (`aria-expanded`): a custom disclosure's.
	checked: Props_checked,
	expanded: Props_expanded,
	-- Announce changes to the text in it (`aria-live`): a count, a "Sent"
	-- note, a message arriving.
	live: Props_live,
	-- Read after its name, as a hint (`aria-description`).
	description: Props_description,
	-- What is wrong with its value (`aria-invalid`), followed: nil while it
	-- is fine. Assistive technology hears it with the field.
	error: Props_error,
	-- Left out of the accessibility tree with what it holds (`aria-hidden`):
	-- decoration, or what is said elsewhere.
	aria_hidden: Props_aria_hidden,
	-- One Tab stop for the focusable things inside; the arrows along this
	-- axis (and Home, End) move among them, as ARIA's composite widgets
	-- behave. `role = "listbox"` is vertical, "toolbar" and "tablist"
	-- horizontal, without asking.
	focus_group: Props_focus_group,
	-- A press on it (or on what inside takes no input) moves the window; a
	-- double click zooms it. Buttons and inputs inside keep their clicks.
	-- CSS pointer-events: "none" (or false) lets the pointer through to what
	-- is under the node (presses, hover, the wheel, drops, a native view);
	-- descendants with handlers still take it. A state or function follows.
	pointer_events: Props_pointer_events,
	window_drag: Props_window_drag,
	-- The pointer selects the text in it and everything it holds, as on a
	-- web page (CSS `user-select: text`): drag, double/triple press, Shift,
	-- ⌘A / Ctrl+A, ⌘C / Ctrl+C. `false` keeps a part out. Off unless set.
	selectable: Props_selectable,
	-- A selectable node's highlight and selected-text colours (CSS
	-- `::selection`), over the theme's `$selection` and `$selection.text`
	-- tokens; a state or function follows.
	selection_color: Props_selection_color,
	selected_text_color: Props_selected_text_color,
	-- Shown over it while the pointer rests on it (after 300 ms; at once
	-- within a second of another): text in the theme's look, or a node of
	-- the app's own. Above it, centred, unless `tooltip_placement` says.
	tooltip: Props_tooltip,
	tooltip_placement: Props_tooltip_placement,
	-- A hover card the pointer can move into and click in: it stays while
	-- the pointer is over the node or the card.
	tooltip_interactive: Props_tooltip_interactive,
	-- Milliseconds the pointer rests before it shows (300; once one
	-- shows, the next shows at once). Keyboard focus shows it too.
	tooltip_delay: Props_tooltip_delay,
	-- A condition (true disables), or the styles it shows while disabled.
	disabled: Props_disabled,
	hover: Props_hover,
	active: Props_active,
	focus: Props_focus,
	-- Size animation ("height", "width", "size"; `true` is "height") or
	-- position animation ("position"). A `layout_id` node mounting as
	-- another with the same id unmounts (or while it is mounted) moves and
	-- scales from that one's box.
	layout: Props_layout,
	layout_id: Props_layout_id,
	layout_ease: Props_layout_ease,
	-- Milliseconds.
	layout_duration: Props_layout_duration,
	layout_enter: Props_layout_enter,
	layout_clip: Props_layout_clip,
	-- The content shows through the bitmap's alpha: `mask = bitmap`, or
	-- `{ bitmap, w = …, h = …, x = …, y = …, sweep = … }`.
	mask: Props_mask,
	bg_shader: Props_bg_shader,
	-- Up to four vec4s: lists of numbers, or colors (0 to 1).
	shader_params: Props_shader_params,
	-- Events.
	on_click: Props_on_click,
	-- Primary pointer down and up.
	on_press: Props_on_press,
	on_release: Props_on_release,
	on_hover: Props_on_hover,
	on_pointer_move: Props_on_pointer_move,
	-- Return true to consume the key.
	on_key: Props_on_key,
	on_key_up: Props_on_key_up,
	on_focus: Props_on_focus,
	on_blur: Props_on_blur,
	-- Return true to take the gesture from the scroll views around it.
	on_wheel: Props_on_wheel,
	on_pinch: Props_on_pinch,
	on_mount: Props_on_mount,
	-- When layout gives it a new size, and at its first layout.
	on_resize: Props_on_resize,
	-- When any of it comes into view or all of it leaves (inside every
	-- clip and scroll viewport above it), and at its first layout: for
	-- building what only matters on screen.
	on_visible: Props_on_visible,
	-- Drag and drop within the window. Payloads are what `on_drag`
	-- returns: the script's own plain values, so they are `any` here.
	-- A source: `on_drag` returns the payload once the pointer moves 4
	-- points; `drag_preview` builds what follows the pointer.
	on_drag: Props_on_drag,
	drag_preview: Props_drag_preview,
	on_drag_end: Props_on_drag_end,
	-- What the drag carries to other apps as it leaves the window (macOS):
	-- Finder takes files, Mail text, a browser links.
	drag_export: Props_drag_export,
	-- Released outside every window of the app (macOS): a tab torn off its
	-- window. `grab_x`/`grab_y` are where the source was held.
	on_drag_outside: Props_on_drag_outside,
	-- The drag moves its window instead, with no preview (macOS): a
	-- window's only tab. Dropped on another window's destination it drops
	-- there; elsewhere the window stays where it was let go.
	drag_moves_window: Props_drag_moves_window,
	-- A destination: `drop_when` filters payloads (default: all).
	drop_when: Props_drop_when,
	on_drag_over: Props_on_drag_over,
	on_drag_leave: Props_on_drag_leave,
	on_drop: Props_on_drop,
	-- A secondary click (control-click on macOS) shows the returned
	-- `menu_item`s, `submenu`s and separators as a menu at the pointer;
	-- returning nil (or nothing) shows none.
	context_menu: Props_context_menu,
	-- The context menu panel's look: bg, rounded, p, gap, backdrop_blur,
	-- border, shadow, w, …
	context_menu_style: Props_context_menu_style,
	-- Open the context menu under the node, from its left edge (a tab's),
	-- rather than at the pointer.
	context_menu_below: Props_context_menu_below,
	[number]: never,
	-- The date, `"2026-10-23"`, both ways; nil: none chosen.
	value: State<string?> | State<any>,
	-- The first and last days that can be chosen.
	min: string?,
	max: string?,
	-- The week's first day: 0 for Sunday, 1 for Monday.
	first_weekday: number?,
	-- Days that can't be chosen (a weekend, a booked one); `weekday` is 0
	-- for Sunday to 6 for Saturday.
	unavailable: ((date: string, weekday: number) -> boolean)?,
	label: string?,
}
-- A button showing a date that opens a calendar to choose it; chosen, the
-- calendar closes. Down opens it too.
export type DatePickerProps = {
	-- Size and position.
	w: Props_w,
	h: Props_h,
	size: Props_size,
	min_w: Props_min_w,
	max_w: Props_max_w,
	min_h: Props_min_h,
	max_h: Props_max_h,
	w_full: Props_w_full,
	h_full: Props_h_full,
	aspect: Props_aspect,
	absolute: Props_absolute,
	relative: Props_relative,
	top: Props_top,
	left: Props_left,
	inset_start: Props_inset_start,
	inset_end: Props_inset_end,
	right: Props_right,
	bottom: Props_bottom,
	-- Spacing.
	p: Props_p,
	px: Props_px,
	py: Props_py,
	pt: Props_pt,
	pr: Props_pr,
	pb: Props_pb,
	pl: Props_pl,
	ps: Props_ps,
	pe: Props_pe,
	m: Props_m,
	mx: Props_mx,
	my: Props_my,
	mt: Props_mt,
	mr: Props_mr,
	mb: Props_mb,
	ml: Props_ml,
	ms: Props_ms,
	me: Props_me,
	gap: Props_gap,
	gap_x: Props_gap_x,
	gap_y: Props_gap_y,
	-- Flex and grid.
	direction: Props_direction,
	dir: Props_dir,
	grow: Props_grow,
	shrink: Props_shrink,
	basis: Props_basis,
	flex_wrap: Props_flex_wrap,
	wrap_reverse: Props_wrap_reverse,
	items: Props_items,
	justify: Props_justify,
	self: Props_self,
	content: Props_content,
	grid: Props_grid,
	cols: Props_cols,
	rows: Props_rows,
	col_start: Props_col_start,
	row_start: Props_row_start,
	col_span: Props_col_span,
	row_span: Props_row_span,
	col_span_full: Props_col_span_full,
	row_span_full: Props_row_span_full,
	overflow: Props_overflow,
	overflow_x: Props_overflow_x,
	overflow_y: Props_overflow_y,
	hidden: Props_hidden,
	invisible: Props_invisible,
	-- Paint.
	bg: Props_bg,
	gradient: Props_gradient,
	color: Props_color,
	bg_image: Props_bg_image,
	bg_fit: Props_bg_fit,
	rounded: Props_rounded,
	corners: Props_corners,
	rounded_tl: Props_rounded_tl,
	rounded_tr: Props_rounded_tr,
	rounded_br: Props_rounded_br,
	rounded_bl: Props_rounded_bl,
	border: Props_border,
	border_sides: Props_border_sides,
	border_color: Props_border_color,
	border_over: Props_border_over,
	-- A function may return nil: a solid border.
	border_dash: Props_border_dash,
	outline: Props_outline,
	outline_color: Props_outline_color,
	outline_offset: Props_outline_offset,
	shadow: Props_shadow,
	inset_shadow: Props_inset_shadow,
	opacity: Props_opacity,
	brightness: Props_brightness,
	contrast: Props_contrast,
	saturate: Props_saturate,
	blur: Props_blur,
	backdrop_blur: Props_backdrop_blur,
	fade: Props_fade,
	fade_x: Props_fade_x,
	fade_sides: Props_fade_sides,
	fade_overflow: Props_fade_overflow,
	snap: Props_snap,
	clip_inset: Props_clip_inset,
	layer: Props_layer,
	fit: Props_fit,
	tint: Props_tint,
	cursor: Props_cursor,
	-- Text.
	text_size: Props_text_size,
	weight: Props_weight,
	bold: Props_bold,
	italic: Props_italic,
	align: Props_align,
	font: Props_font,
	wrap: Props_wrap,
	truncate: Props_truncate,
	lines: Props_lines,
	line_height: Props_line_height,
	letter_spacing: Props_letter_spacing,
	-- Tabular figures: every digit the same width.
	tabular: Props_tabular,
	-- Underlined text (inherited, as CSS's `text-decoration`), and how far
	-- below the baseline the line sits.
	underline: Props_underline,
	underline_offset: Props_underline_offset,
	-- Transforms: after layout, about `origin` (fractions of the node).
	x: Props_x,
	y: Props_y,
	scale: Props_scale,
	scale_x: Props_scale_x,
	scale_y: Props_scale_y,
	rotate: Props_rotate,
	skew: Props_skew,
	skew_x: Props_skew_x,
	skew_y: Props_skew_y,
	origin: Props_origin,
	-- The subtree's theme: a theme's name (`"zgx-light"`), or tokens to
	-- override (`{ accent = "#e5484d" }`, with `theme = name` to start from).
	theme: Props_theme,
	-- Identity, for selectors, refs and animations.
	id: Props_id,
	-- Space-separated class names.
	class: Props_class,
	-- Styles chosen from a `ui.variants` definition: `button { tone =
	-- "ghost" }`, or the recipe itself for its defaults. The element's own
	-- properties win over them.
	variants: Props_variants,
	ref: Props_ref,
	focusable: Props_focusable,
	-- What assistive technology takes it for (ARIA's `role`), instead of
	-- what it is: a clickable row as an "option", a row of buttons as a
	-- "toolbar". It keeps what it does.
	role: Props_role,
	-- A section title of this level, 1 to 6 (`h1`…`h6`): screen readers
	-- list headings and move between them.
	heading: Props_heading,
	-- Whether this option, tab or item is the chosen one (`aria-selected`).
	selected: Props_selected,
	-- Whether a switch, checkbox or radio of the app's own is on
	-- (`aria-checked`), and whether what a control opens shows
	-- (`aria-expanded`): a custom disclosure's.
	checked: Props_checked,
	expanded: Props_expanded,
	-- Announce changes to the text in it (`aria-live`): a count, a "Sent"
	-- note, a message arriving.
	live: Props_live,
	-- Read after its name, as a hint (`aria-description`).
	description: Props_description,
	-- What is wrong with its value (`aria-invalid`), followed: nil while it
	-- is fine. Assistive technology hears it with the field.
	error: Props_error,
	-- Left out of the accessibility tree with what it holds (`aria-hidden`):
	-- decoration, or what is said elsewhere.
	aria_hidden: Props_aria_hidden,
	-- One Tab stop for the focusable things inside; the arrows along this
	-- axis (and Home, End) move among them, as ARIA's composite widgets
	-- behave. `role = "listbox"` is vertical, "toolbar" and "tablist"
	-- horizontal, without asking.
	focus_group: Props_focus_group,
	-- A press on it (or on what inside takes no input) moves the window; a
	-- double click zooms it. Buttons and inputs inside keep their clicks.
	-- CSS pointer-events: "none" (or false) lets the pointer through to what
	-- is under the node (presses, hover, the wheel, drops, a native view);
	-- descendants with handlers still take it. A state or function follows.
	pointer_events: Props_pointer_events,
	window_drag: Props_window_drag,
	-- The pointer selects the text in it and everything it holds, as on a
	-- web page (CSS `user-select: text`): drag, double/triple press, Shift,
	-- ⌘A / Ctrl+A, ⌘C / Ctrl+C. `false` keeps a part out. Off unless set.
	selectable: Props_selectable,
	-- A selectable node's highlight and selected-text colours (CSS
	-- `::selection`), over the theme's `$selection` and `$selection.text`
	-- tokens; a state or function follows.
	selection_color: Props_selection_color,
	selected_text_color: Props_selected_text_color,
	-- Shown over it while the pointer rests on it (after 300 ms; at once
	-- within a second of another): text in the theme's look, or a node of
	-- the app's own. Above it, centred, unless `tooltip_placement` says.
	tooltip: Props_tooltip,
	tooltip_placement: Props_tooltip_placement,
	-- A hover card the pointer can move into and click in: it stays while
	-- the pointer is over the node or the card.
	tooltip_interactive: Props_tooltip_interactive,
	-- Milliseconds the pointer rests before it shows (300; once one
	-- shows, the next shows at once). Keyboard focus shows it too.
	tooltip_delay: Props_tooltip_delay,
	-- A condition (true disables), or the styles it shows while disabled.
	disabled: Props_disabled,
	hover: Props_hover,
	active: Props_active,
	focus: Props_focus,
	-- Size animation ("height", "width", "size"; `true` is "height") or
	-- position animation ("position"). A `layout_id` node mounting as
	-- another with the same id unmounts (or while it is mounted) moves and
	-- scales from that one's box.
	layout: Props_layout,
	layout_id: Props_layout_id,
	layout_ease: Props_layout_ease,
	-- Milliseconds.
	layout_duration: Props_layout_duration,
	layout_enter: Props_layout_enter,
	layout_clip: Props_layout_clip,
	-- The content shows through the bitmap's alpha: `mask = bitmap`, or
	-- `{ bitmap, w = …, h = …, x = …, y = …, sweep = … }`.
	mask: Props_mask,
	bg_shader: Props_bg_shader,
	-- Up to four vec4s: lists of numbers, or colors (0 to 1).
	shader_params: Props_shader_params,
	-- Events.
	on_click: Props_on_click,
	-- Primary pointer down and up.
	on_press: Props_on_press,
	on_release: Props_on_release,
	on_hover: Props_on_hover,
	on_pointer_move: Props_on_pointer_move,
	-- Return true to consume the key.
	on_key: Props_on_key,
	on_key_up: Props_on_key_up,
	on_focus: Props_on_focus,
	on_blur: Props_on_blur,
	-- Return true to take the gesture from the scroll views around it.
	on_wheel: Props_on_wheel,
	on_pinch: Props_on_pinch,
	on_mount: Props_on_mount,
	-- When layout gives it a new size, and at its first layout.
	on_resize: Props_on_resize,
	-- When any of it comes into view or all of it leaves (inside every
	-- clip and scroll viewport above it), and at its first layout: for
	-- building what only matters on screen.
	on_visible: Props_on_visible,
	-- Drag and drop within the window. Payloads are what `on_drag`
	-- returns: the script's own plain values, so they are `any` here.
	-- A source: `on_drag` returns the payload once the pointer moves 4
	-- points; `drag_preview` builds what follows the pointer.
	on_drag: Props_on_drag,
	drag_preview: Props_drag_preview,
	on_drag_end: Props_on_drag_end,
	-- What the drag carries to other apps as it leaves the window (macOS):
	-- Finder takes files, Mail text, a browser links.
	drag_export: Props_drag_export,
	-- Released outside every window of the app (macOS): a tab torn off its
	-- window. `grab_x`/`grab_y` are where the source was held.
	on_drag_outside: Props_on_drag_outside,
	-- The drag moves its window instead, with no preview (macOS): a
	-- window's only tab. Dropped on another window's destination it drops
	-- there; elsewhere the window stays where it was let go.
	drag_moves_window: Props_drag_moves_window,
	-- A destination: `drop_when` filters payloads (default: all).
	drop_when: Props_drop_when,
	on_drag_over: Props_on_drag_over,
	on_drag_leave: Props_on_drag_leave,
	on_drop: Props_on_drop,
	-- A secondary click (control-click on macOS) shows the returned
	-- `menu_item`s, `submenu`s and separators as a menu at the pointer;
	-- returning nil (or nothing) shows none.
	context_menu: Props_context_menu,
	-- The context menu panel's look: bg, rounded, p, gap, backdrop_blur,
	-- border, shadow, w, …
	context_menu_style: Props_context_menu_style,
	-- Open the context menu under the node, from its left edge (a tab's),
	-- rather than at the pointer.
	context_menu_below: Props_context_menu_below,
	[number]: never,
	value: State<string?> | State<any>,
	min: string?,
	max: string?,
	first_weekday: number?,
	unavailable: ((date: string, weekday: number) -> boolean)?,
	-- How the button writes the date (`"2026-10-23"` unless set).
	format: ((date: string) -> string)?,
	label: string?,
}
-- A time of day in hour and minute segments, each a spin button: Up and
-- Down step, digits type, Left and Right move between them, Backspace
-- clears one (and the value is nil until each has one).
export type TimeFieldProps = {
	-- Size and position.
	w: Props_w,
	h: Props_h,
	size: Props_size,
	min_w: Props_min_w,
	max_w: Props_max_w,
	min_h: Props_min_h,
	max_h: Props_max_h,
	w_full: Props_w_full,
	h_full: Props_h_full,
	aspect: Props_aspect,
	absolute: Props_absolute,
	relative: Props_relative,
	top: Props_top,
	left: Props_left,
	inset_start: Props_inset_start,
	inset_end: Props_inset_end,
	right: Props_right,
	bottom: Props_bottom,
	-- Spacing.
	p: Props_p,
	px: Props_px,
	py: Props_py,
	pt: Props_pt,
	pr: Props_pr,
	pb: Props_pb,
	pl: Props_pl,
	ps: Props_ps,
	pe: Props_pe,
	m: Props_m,
	mx: Props_mx,
	my: Props_my,
	mt: Props_mt,
	mr: Props_mr,
	mb: Props_mb,
	ml: Props_ml,
	ms: Props_ms,
	me: Props_me,
	gap: Props_gap,
	gap_x: Props_gap_x,
	gap_y: Props_gap_y,
	-- Flex and grid.
	direction: Props_direction,
	dir: Props_dir,
	grow: Props_grow,
	shrink: Props_shrink,
	basis: Props_basis,
	flex_wrap: Props_flex_wrap,
	wrap_reverse: Props_wrap_reverse,
	items: Props_items,
	justify: Props_justify,
	self: Props_self,
	content: Props_content,
	grid: Props_grid,
	cols: Props_cols,
	rows: Props_rows,
	col_start: Props_col_start,
	row_start: Props_row_start,
	col_span: Props_col_span,
	row_span: Props_row_span,
	col_span_full: Props_col_span_full,
	row_span_full: Props_row_span_full,
	overflow: Props_overflow,
	overflow_x: Props_overflow_x,
	overflow_y: Props_overflow_y,
	hidden: Props_hidden,
	invisible: Props_invisible,
	-- Paint.
	bg: Props_bg,
	gradient: Props_gradient,
	color: Props_color,
	bg_image: Props_bg_image,
	bg_fit: Props_bg_fit,
	rounded: Props_rounded,
	corners: Props_corners,
	rounded_tl: Props_rounded_tl,
	rounded_tr: Props_rounded_tr,
	rounded_br: Props_rounded_br,
	rounded_bl: Props_rounded_bl,
	border: Props_border,
	border_sides: Props_border_sides,
	border_color: Props_border_color,
	border_over: Props_border_over,
	-- A function may return nil: a solid border.
	border_dash: Props_border_dash,
	outline: Props_outline,
	outline_color: Props_outline_color,
	outline_offset: Props_outline_offset,
	shadow: Props_shadow,
	inset_shadow: Props_inset_shadow,
	opacity: Props_opacity,
	brightness: Props_brightness,
	contrast: Props_contrast,
	saturate: Props_saturate,
	blur: Props_blur,
	backdrop_blur: Props_backdrop_blur,
	fade: Props_fade,
	fade_x: Props_fade_x,
	fade_sides: Props_fade_sides,
	fade_overflow: Props_fade_overflow,
	snap: Props_snap,
	clip_inset: Props_clip_inset,
	layer: Props_layer,
	fit: Props_fit,
	tint: Props_tint,
	cursor: Props_cursor,
	-- Text.
	text_size: Props_text_size,
	weight: Props_weight,
	bold: Props_bold,
	italic: Props_italic,
	align: Props_align,
	font: Props_font,
	wrap: Props_wrap,
	truncate: Props_truncate,
	lines: Props_lines,
	line_height: Props_line_height,
	letter_spacing: Props_letter_spacing,
	-- Tabular figures: every digit the same width.
	tabular: Props_tabular,
	-- Underlined text (inherited, as CSS's `text-decoration`), and how far
	-- below the baseline the line sits.
	underline: Props_underline,
	underline_offset: Props_underline_offset,
	-- Transforms: after layout, about `origin` (fractions of the node).
	x: Props_x,
	y: Props_y,
	scale: Props_scale,
	scale_x: Props_scale_x,
	scale_y: Props_scale_y,
	rotate: Props_rotate,
	skew: Props_skew,
	skew_x: Props_skew_x,
	skew_y: Props_skew_y,
	origin: Props_origin,
	-- The subtree's theme: a theme's name (`"zgx-light"`), or tokens to
	-- override (`{ accent = "#e5484d" }`, with `theme = name` to start from).
	theme: Props_theme,
	-- Identity, for selectors, refs and animations.
	id: Props_id,
	-- Space-separated class names.
	class: Props_class,
	-- Styles chosen from a `ui.variants` definition: `button { tone =
	-- "ghost" }`, or the recipe itself for its defaults. The element's own
	-- properties win over them.
	variants: Props_variants,
	ref: Props_ref,
	focusable: Props_focusable,
	-- What assistive technology takes it for (ARIA's `role`), instead of
	-- what it is: a clickable row as an "option", a row of buttons as a
	-- "toolbar". It keeps what it does.
	role: Props_role,
	-- A section title of this level, 1 to 6 (`h1`…`h6`): screen readers
	-- list headings and move between them.
	heading: Props_heading,
	-- Whether this option, tab or item is the chosen one (`aria-selected`).
	selected: Props_selected,
	-- Whether a switch, checkbox or radio of the app's own is on
	-- (`aria-checked`), and whether what a control opens shows
	-- (`aria-expanded`): a custom disclosure's.
	checked: Props_checked,
	expanded: Props_expanded,
	-- Announce changes to the text in it (`aria-live`): a count, a "Sent"
	-- note, a message arriving.
	live: Props_live,
	-- Read after its name, as a hint (`aria-description`).
	description: Props_description,
	-- What is wrong with its value (`aria-invalid`), followed: nil while it
	-- is fine. Assistive technology hears it with the field.
	error: Props_error,
	-- Left out of the accessibility tree with what it holds (`aria-hidden`):
	-- decoration, or what is said elsewhere.
	aria_hidden: Props_aria_hidden,
	-- One Tab stop for the focusable things inside; the arrows along this
	-- axis (and Home, End) move among them, as ARIA's composite widgets
	-- behave. `role = "listbox"` is vertical, "toolbar" and "tablist"
	-- horizontal, without asking.
	focus_group: Props_focus_group,
	-- A press on it (or on what inside takes no input) moves the window; a
	-- double click zooms it. Buttons and inputs inside keep their clicks.
	-- CSS pointer-events: "none" (or false) lets the pointer through to what
	-- is under the node (presses, hover, the wheel, drops, a native view);
	-- descendants with handlers still take it. A state or function follows.
	pointer_events: Props_pointer_events,
	window_drag: Props_window_drag,
	-- The pointer selects the text in it and everything it holds, as on a
	-- web page (CSS `user-select: text`): drag, double/triple press, Shift,
	-- ⌘A / Ctrl+A, ⌘C / Ctrl+C. `false` keeps a part out. Off unless set.
	selectable: Props_selectable,
	-- A selectable node's highlight and selected-text colours (CSS
	-- `::selection`), over the theme's `$selection` and `$selection.text`
	-- tokens; a state or function follows.
	selection_color: Props_selection_color,
	selected_text_color: Props_selected_text_color,
	-- Shown over it while the pointer rests on it (after 300 ms; at once
	-- within a second of another): text in the theme's look, or a node of
	-- the app's own. Above it, centred, unless `tooltip_placement` says.
	tooltip: Props_tooltip,
	tooltip_placement: Props_tooltip_placement,
	-- A hover card the pointer can move into and click in: it stays while
	-- the pointer is over the node or the card.
	tooltip_interactive: Props_tooltip_interactive,
	-- Milliseconds the pointer rests before it shows (300; once one
	-- shows, the next shows at once). Keyboard focus shows it too.
	tooltip_delay: Props_tooltip_delay,
	-- A condition (true disables), or the styles it shows while disabled.
	disabled: Props_disabled,
	hover: Props_hover,
	active: Props_active,
	focus: Props_focus,
	-- Size animation ("height", "width", "size"; `true` is "height") or
	-- position animation ("position"). A `layout_id` node mounting as
	-- another with the same id unmounts (or while it is mounted) moves and
	-- scales from that one's box.
	layout: Props_layout,
	layout_id: Props_layout_id,
	layout_ease: Props_layout_ease,
	-- Milliseconds.
	layout_duration: Props_layout_duration,
	layout_enter: Props_layout_enter,
	layout_clip: Props_layout_clip,
	-- The content shows through the bitmap's alpha: `mask = bitmap`, or
	-- `{ bitmap, w = …, h = …, x = …, y = …, sweep = … }`.
	mask: Props_mask,
	bg_shader: Props_bg_shader,
	-- Up to four vec4s: lists of numbers, or colors (0 to 1).
	shader_params: Props_shader_params,
	-- Events.
	on_click: Props_on_click,
	-- Primary pointer down and up.
	on_press: Props_on_press,
	on_release: Props_on_release,
	on_hover: Props_on_hover,
	on_pointer_move: Props_on_pointer_move,
	-- Return true to consume the key.
	on_key: Props_on_key,
	on_key_up: Props_on_key_up,
	on_focus: Props_on_focus,
	on_blur: Props_on_blur,
	-- Return true to take the gesture from the scroll views around it.
	on_wheel: Props_on_wheel,
	on_pinch: Props_on_pinch,
	on_mount: Props_on_mount,
	-- When layout gives it a new size, and at its first layout.
	on_resize: Props_on_resize,
	-- When any of it comes into view or all of it leaves (inside every
	-- clip and scroll viewport above it), and at its first layout: for
	-- building what only matters on screen.
	on_visible: Props_on_visible,
	-- Drag and drop within the window. Payloads are what `on_drag`
	-- returns: the script's own plain values, so they are `any` here.
	-- A source: `on_drag` returns the payload once the pointer moves 4
	-- points; `drag_preview` builds what follows the pointer.
	on_drag: Props_on_drag,
	drag_preview: Props_drag_preview,
	on_drag_end: Props_on_drag_end,
	-- What the drag carries to other apps as it leaves the window (macOS):
	-- Finder takes files, Mail text, a browser links.
	drag_export: Props_drag_export,
	-- Released outside every window of the app (macOS): a tab torn off its
	-- window. `grab_x`/`grab_y` are where the source was held.
	on_drag_outside: Props_on_drag_outside,
	-- The drag moves its window instead, with no preview (macOS): a
	-- window's only tab. Dropped on another window's destination it drops
	-- there; elsewhere the window stays where it was let go.
	drag_moves_window: Props_drag_moves_window,
	-- A destination: `drop_when` filters payloads (default: all).
	drop_when: Props_drop_when,
	on_drag_over: Props_on_drag_over,
	on_drag_leave: Props_on_drag_leave,
	on_drop: Props_on_drop,
	-- A secondary click (control-click on macOS) shows the returned
	-- `menu_item`s, `submenu`s and separators as a menu at the pointer;
	-- returning nil (or nothing) shows none.
	context_menu: Props_context_menu,
	-- The context menu panel's look: bg, rounded, p, gap, backdrop_blur,
	-- border, shadow, w, …
	context_menu_style: Props_context_menu_style,
	-- Open the context menu under the node, from its left edge (a tab's),
	-- rather than at the pointer.
	context_menu_below: Props_context_menu_below,
	[number]: never,
	-- The time, `"09:30"` (24-hour), both ways.
	value: State<string?> | State<any>,
	-- Hours 1 to 12 and an AM/PM segment.
	hour12: boolean?,
	-- What Up and Down step the minutes by.
	minute_step: number?,
	label: string?,
}
export type ComboItem = string | { label: string, value: any?, detail: string? }
-- A field with suggestions listed under it as it is typed in (WAI-ARIA's
-- combobox): Down and Up move, Enter chooses, Escape closes or clears; a
-- click chooses. Focus stays in the field; the list is virtual.
export type ComboboxProps = {
	-- Size and position.
	w: Props_w,
	h: Props_h,
	size: Props_size,
	min_w: Props_min_w,
	max_w: Props_max_w,
	min_h: Props_min_h,
	max_h: Props_max_h,
	w_full: Props_w_full,
	h_full: Props_h_full,
	aspect: Props_aspect,
	absolute: Props_absolute,
	relative: Props_relative,
	top: Props_top,
	left: Props_left,
	inset_start: Props_inset_start,
	inset_end: Props_inset_end,
	right: Props_right,
	bottom: Props_bottom,
	-- Spacing.
	p: Props_p,
	px: Props_px,
	py: Props_py,
	pt: Props_pt,
	pr: Props_pr,
	pb: Props_pb,
	pl: Props_pl,
	ps: Props_ps,
	pe: Props_pe,
	m: Props_m,
	mx: Props_mx,
	my: Props_my,
	mt: Props_mt,
	mr: Props_mr,
	mb: Props_mb,
	ml: Props_ml,
	ms: Props_ms,
	me: Props_me,
	gap: Props_gap,
	gap_x: Props_gap_x,
	gap_y: Props_gap_y,
	-- Flex and grid.
	direction: Props_direction,
	dir: Props_dir,
	grow: Props_grow,
	shrink: Props_shrink,
	basis: Props_basis,
	flex_wrap: Props_flex_wrap,
	wrap_reverse: Props_wrap_reverse,
	justify: Props_justify,
	self: Props_self,
	content: Props_content,
	grid: Props_grid,
	cols: Props_cols,
	rows: Props_rows,
	col_start: Props_col_start,
	row_start: Props_row_start,
	col_span: Props_col_span,
	row_span: Props_row_span,
	col_span_full: Props_col_span_full,
	row_span_full: Props_row_span_full,
	overflow: Props_overflow,
	overflow_x: Props_overflow_x,
	overflow_y: Props_overflow_y,
	hidden: Props_hidden,
	invisible: Props_invisible,
	-- Paint.
	bg: Props_bg,
	gradient: Props_gradient,
	color: Props_color,
	bg_image: Props_bg_image,
	bg_fit: Props_bg_fit,
	rounded: Props_rounded,
	corners: Props_corners,
	rounded_tl: Props_rounded_tl,
	rounded_tr: Props_rounded_tr,
	rounded_br: Props_rounded_br,
	rounded_bl: Props_rounded_bl,
	border: Props_border,
	border_sides: Props_border_sides,
	border_color: Props_border_color,
	border_over: Props_border_over,
	-- A function may return nil: a solid border.
	border_dash: Props_border_dash,
	outline: Props_outline,
	outline_color: Props_outline_color,
	outline_offset: Props_outline_offset,
	shadow: Props_shadow,
	inset_shadow: Props_inset_shadow,
	opacity: Props_opacity,
	brightness: Props_brightness,
	contrast: Props_contrast,
	saturate: Props_saturate,
	blur: Props_blur,
	backdrop_blur: Props_backdrop_blur,
	fade: Props_fade,
	fade_x: Props_fade_x,
	fade_sides: Props_fade_sides,
	fade_overflow: Props_fade_overflow,
	snap: Props_snap,
	clip_inset: Props_clip_inset,
	layer: Props_layer,
	fit: Props_fit,
	tint: Props_tint,
	cursor: Props_cursor,
	-- Text.
	text_size: Props_text_size,
	weight: Props_weight,
	bold: Props_bold,
	italic: Props_italic,
	align: Props_align,
	font: Props_font,
	wrap: Props_wrap,
	truncate: Props_truncate,
	lines: Props_lines,
	line_height: Props_line_height,
	letter_spacing: Props_letter_spacing,
	-- Tabular figures: every digit the same width.
	tabular: Props_tabular,
	-- Underlined text (inherited, as CSS's `text-decoration`), and how far
	-- below the baseline the line sits.
	underline: Props_underline,
	underline_offset: Props_underline_offset,
	-- Transforms: after layout, about `origin` (fractions of the node).
	x: Props_x,
	y: Props_y,
	scale: Props_scale,
	scale_x: Props_scale_x,
	scale_y: Props_scale_y,
	rotate: Props_rotate,
	skew: Props_skew,
	skew_x: Props_skew_x,
	skew_y: Props_skew_y,
	origin: Props_origin,
	-- The subtree's theme: a theme's name (`"zgx-light"`), or tokens to
	-- override (`{ accent = "#e5484d" }`, with `theme = name` to start from).
	theme: Props_theme,
	-- Identity, for selectors, refs and animations.
	id: Props_id,
	-- Space-separated class names.
	class: Props_class,
	-- Styles chosen from a `ui.variants` definition: `button { tone =
	-- "ghost" }`, or the recipe itself for its defaults. The element's own
	-- properties win over them.
	variants: Props_variants,
	ref: Props_ref,
	focusable: Props_focusable,
	-- What assistive technology takes it for (ARIA's `role`), instead of
	-- what it is: a clickable row as an "option", a row of buttons as a
	-- "toolbar". It keeps what it does.
	role: Props_role,
	-- A section title of this level, 1 to 6 (`h1`…`h6`): screen readers
	-- list headings and move between them.
	heading: Props_heading,
	-- Whether this option, tab or item is the chosen one (`aria-selected`).
	selected: Props_selected,
	-- Whether a switch, checkbox or radio of the app's own is on
	-- (`aria-checked`), and whether what a control opens shows
	-- (`aria-expanded`): a custom disclosure's.
	checked: Props_checked,
	expanded: Props_expanded,
	-- Announce changes to the text in it (`aria-live`): a count, a "Sent"
	-- note, a message arriving.
	live: Props_live,
	-- Read after its name, as a hint (`aria-description`).
	description: Props_description,
	-- What is wrong with its value (`aria-invalid`), followed: nil while it
	-- is fine. Assistive technology hears it with the field.
	error: Props_error,
	-- Left out of the accessibility tree with what it holds (`aria-hidden`):
	-- decoration, or what is said elsewhere.
	aria_hidden: Props_aria_hidden,
	-- One Tab stop for the focusable things inside; the arrows along this
	-- axis (and Home, End) move among them, as ARIA's composite widgets
	-- behave. `role = "listbox"` is vertical, "toolbar" and "tablist"
	-- horizontal, without asking.
	focus_group: Props_focus_group,
	-- A press on it (or on what inside takes no input) moves the window; a
	-- double click zooms it. Buttons and inputs inside keep their clicks.
	-- CSS pointer-events: "none" (or false) lets the pointer through to what
	-- is under the node (presses, hover, the wheel, drops, a native view);
	-- descendants with handlers still take it. A state or function follows.
	pointer_events: Props_pointer_events,
	window_drag: Props_window_drag,
	-- The pointer selects the text in it and everything it holds, as on a
	-- web page (CSS `user-select: text`): drag, double/triple press, Shift,
	-- ⌘A / Ctrl+A, ⌘C / Ctrl+C. `false` keeps a part out. Off unless set.
	selectable: Props_selectable,
	-- A selectable node's highlight and selected-text colours (CSS
	-- `::selection`), over the theme's `$selection` and `$selection.text`
	-- tokens; a state or function follows.
	selection_color: Props_selection_color,
	selected_text_color: Props_selected_text_color,
	-- Shown over it while the pointer rests on it (after 300 ms; at once
	-- within a second of another): text in the theme's look, or a node of
	-- the app's own. Above it, centred, unless `tooltip_placement` says.
	tooltip: Props_tooltip,
	tooltip_placement: Props_tooltip_placement,
	-- A hover card the pointer can move into and click in: it stays while
	-- the pointer is over the node or the card.
	tooltip_interactive: Props_tooltip_interactive,
	-- Milliseconds the pointer rests before it shows (300; once one
	-- shows, the next shows at once). Keyboard focus shows it too.
	tooltip_delay: Props_tooltip_delay,
	-- A condition (true disables), or the styles it shows while disabled.
	disabled: Props_disabled,
	hover: Props_hover,
	active: Props_active,
	focus: Props_focus,
	-- Size animation ("height", "width", "size"; `true` is "height") or
	-- position animation ("position"). A `layout_id` node mounting as
	-- another with the same id unmounts (or while it is mounted) moves and
	-- scales from that one's box.
	layout: Props_layout,
	layout_id: Props_layout_id,
	layout_ease: Props_layout_ease,
	-- Milliseconds.
	layout_duration: Props_layout_duration,
	layout_enter: Props_layout_enter,
	layout_clip: Props_layout_clip,
	-- The content shows through the bitmap's alpha: `mask = bitmap`, or
	-- `{ bitmap, w = …, h = …, x = …, y = …, sweep = … }`.
	mask: Props_mask,
	bg_shader: Props_bg_shader,
	-- Up to four vec4s: lists of numbers, or colors (0 to 1).
	shader_params: Props_shader_params,
	-- Events.
	on_click: Props_on_click,
	-- Primary pointer down and up.
	on_press: Props_on_press,
	on_release: Props_on_release,
	on_hover: Props_on_hover,
	on_pointer_move: Props_on_pointer_move,
	-- Return true to consume the key.
	on_key: Props_on_key,
	on_key_up: Props_on_key_up,
	on_focus: Props_on_focus,
	on_blur: Props_on_blur,
	-- Return true to take the gesture from the scroll views around it.
	on_wheel: Props_on_wheel,
	on_pinch: Props_on_pinch,
	on_mount: Props_on_mount,
	-- When layout gives it a new size, and at its first layout.
	on_resize: Props_on_resize,
	-- When any of it comes into view or all of it leaves (inside every
	-- clip and scroll viewport above it), and at its first layout: for
	-- building what only matters on screen.
	on_visible: Props_on_visible,
	-- Drag and drop within the window. Payloads are what `on_drag`
	-- returns: the script's own plain values, so they are `any` here.
	-- A source: `on_drag` returns the payload once the pointer moves 4
	-- points; `drag_preview` builds what follows the pointer.
	on_drag: Props_on_drag,
	drag_preview: Props_drag_preview,
	on_drag_end: Props_on_drag_end,
	-- What the drag carries to other apps as it leaves the window (macOS):
	-- Finder takes files, Mail text, a browser links.
	drag_export: Props_drag_export,
	-- Released outside every window of the app (macOS): a tab torn off its
	-- window. `grab_x`/`grab_y` are where the source was held.
	on_drag_outside: Props_on_drag_outside,
	-- The drag moves its window instead, with no preview (macOS): a
	-- window's only tab. Dropped on another window's destination it drops
	-- there; elsewhere the window stays where it was let go.
	drag_moves_window: Props_drag_moves_window,
	-- A destination: `drop_when` filters payloads (default: all).
	drop_when: Props_drop_when,
	on_drag_over: Props_on_drag_over,
	on_drag_leave: Props_on_drag_leave,
	on_drop: Props_on_drop,
	-- A secondary click (control-click on macOS) shows the returned
	-- `menu_item`s, `submenu`s and separators as a menu at the pointer;
	-- returning nil (or nothing) shows none.
	context_menu: Props_context_menu,
	-- The context menu panel's look: bg, rounded, p, gap, backdrop_blur,
	-- border, shadow, w, …
	context_menu_style: Props_context_menu_style,
	-- Open the context menu under the node, from its left edge (a tab's),
	-- rather than at the pointer.
	context_menu_below: Props_context_menu_below,
	[number]: never,
	-- What the field holds, both ways.
	text: State<string>,
	-- The app's options for the text now: a computed filtering, or a state
	-- an async fetch fills.
	items: State<{ ComboItem }> | State<{ any }>,
	-- The chosen option's value, written as one is chosen.
	value: State<any>?,
	-- One of a `ui.form`'s fields (`form.fields.email`): its value, its
	-- leaving, its error (and, a one-line input, Enter submitting the form).
	field: FormField?,
	-- While true, the list says it is searching.
	loading: State<boolean>?,
	-- What the list says with no options ("No results").
	empty: string?,
	label: string?,
}
export type PaletteCommand = { id: string, title: string?, detail: string?, shortcut: string? }
-- A field over a list of commands, filtered by a fuzzy match: Up and Down
-- move, Enter runs, Escape closes. Without `commands`, the app's own
-- (`ui.command`), each run as its menu item would be.
export type CommandPaletteProps = {
	-- Size and position.
	w: Props_w,
	h: Props_h,
	size: Props_size,
	min_w: Props_min_w,
	max_w: Props_max_w,
	min_h: Props_min_h,
	max_h: Props_max_h,
	w_full: Props_w_full,
	h_full: Props_h_full,
	aspect: Props_aspect,
	absolute: Props_absolute,
	relative: Props_relative,
	top: Props_top,
	left: Props_left,
	inset_start: Props_inset_start,
	inset_end: Props_inset_end,
	right: Props_right,
	bottom: Props_bottom,
	-- Spacing.
	p: Props_p,
	px: Props_px,
	py: Props_py,
	pt: Props_pt,
	pr: Props_pr,
	pb: Props_pb,
	pl: Props_pl,
	ps: Props_ps,
	pe: Props_pe,
	m: Props_m,
	mx: Props_mx,
	my: Props_my,
	mt: Props_mt,
	mr: Props_mr,
	mb: Props_mb,
	ml: Props_ml,
	ms: Props_ms,
	me: Props_me,
	gap: Props_gap,
	gap_x: Props_gap_x,
	gap_y: Props_gap_y,
	-- Flex and grid.
	direction: Props_direction,
	dir: Props_dir,
	grow: Props_grow,
	shrink: Props_shrink,
	basis: Props_basis,
	flex_wrap: Props_flex_wrap,
	wrap_reverse: Props_wrap_reverse,
	items: Props_items,
	justify: Props_justify,
	self: Props_self,
	content: Props_content,
	grid: Props_grid,
	cols: Props_cols,
	rows: Props_rows,
	col_start: Props_col_start,
	row_start: Props_row_start,
	col_span: Props_col_span,
	row_span: Props_row_span,
	col_span_full: Props_col_span_full,
	row_span_full: Props_row_span_full,
	overflow: Props_overflow,
	overflow_x: Props_overflow_x,
	overflow_y: Props_overflow_y,
	hidden: Props_hidden,
	invisible: Props_invisible,
	-- Paint.
	bg: Props_bg,
	gradient: Props_gradient,
	color: Props_color,
	bg_image: Props_bg_image,
	bg_fit: Props_bg_fit,
	rounded: Props_rounded,
	corners: Props_corners,
	rounded_tl: Props_rounded_tl,
	rounded_tr: Props_rounded_tr,
	rounded_br: Props_rounded_br,
	rounded_bl: Props_rounded_bl,
	border: Props_border,
	border_sides: Props_border_sides,
	border_color: Props_border_color,
	border_over: Props_border_over,
	-- A function may return nil: a solid border.
	border_dash: Props_border_dash,
	outline: Props_outline,
	outline_color: Props_outline_color,
	outline_offset: Props_outline_offset,
	shadow: Props_shadow,
	inset_shadow: Props_inset_shadow,
	opacity: Props_opacity,
	brightness: Props_brightness,
	contrast: Props_contrast,
	saturate: Props_saturate,
	blur: Props_blur,
	backdrop_blur: Props_backdrop_blur,
	fade: Props_fade,
	fade_x: Props_fade_x,
	fade_sides: Props_fade_sides,
	fade_overflow: Props_fade_overflow,
	snap: Props_snap,
	clip_inset: Props_clip_inset,
	layer: Props_layer,
	fit: Props_fit,
	tint: Props_tint,
	cursor: Props_cursor,
	-- Text.
	text_size: Props_text_size,
	weight: Props_weight,
	bold: Props_bold,
	italic: Props_italic,
	align: Props_align,
	font: Props_font,
	wrap: Props_wrap,
	truncate: Props_truncate,
	lines: Props_lines,
	line_height: Props_line_height,
	letter_spacing: Props_letter_spacing,
	-- Tabular figures: every digit the same width.
	tabular: Props_tabular,
	-- Underlined text (inherited, as CSS's `text-decoration`), and how far
	-- below the baseline the line sits.
	underline: Props_underline,
	underline_offset: Props_underline_offset,
	-- Transforms: after layout, about `origin` (fractions of the node).
	x: Props_x,
	y: Props_y,
	scale: Props_scale,
	scale_x: Props_scale_x,
	scale_y: Props_scale_y,
	rotate: Props_rotate,
	skew: Props_skew,
	skew_x: Props_skew_x,
	skew_y: Props_skew_y,
	origin: Props_origin,
	-- The subtree's theme: a theme's name (`"zgx-light"`), or tokens to
	-- override (`{ accent = "#e5484d" }`, with `theme = name` to start from).
	theme: Props_theme,
	-- Identity, for selectors, refs and animations.
	id: Props_id,
	-- Space-separated class names.
	class: Props_class,
	-- Styles chosen from a `ui.variants` definition: `button { tone =
	-- "ghost" }`, or the recipe itself for its defaults. The element's own
	-- properties win over them.
	variants: Props_variants,
	ref: Props_ref,
	focusable: Props_focusable,
	-- What assistive technology takes it for (ARIA's `role`), instead of
	-- what it is: a clickable row as an "option", a row of buttons as a
	-- "toolbar". It keeps what it does.
	role: Props_role,
	-- A section title of this level, 1 to 6 (`h1`…`h6`): screen readers
	-- list headings and move between them.
	heading: Props_heading,
	-- Whether this option, tab or item is the chosen one (`aria-selected`).
	selected: Props_selected,
	-- Whether a switch, checkbox or radio of the app's own is on
	-- (`aria-checked`), and whether what a control opens shows
	-- (`aria-expanded`): a custom disclosure's.
	checked: Props_checked,
	expanded: Props_expanded,
	-- Announce changes to the text in it (`aria-live`): a count, a "Sent"
	-- note, a message arriving.
	live: Props_live,
	-- Read after its name, as a hint (`aria-description`).
	description: Props_description,
	-- What is wrong with its value (`aria-invalid`), followed: nil while it
	-- is fine. Assistive technology hears it with the field.
	error: Props_error,
	-- Left out of the accessibility tree with what it holds (`aria-hidden`):
	-- decoration, or what is said elsewhere.
	aria_hidden: Props_aria_hidden,
	-- One Tab stop for the focusable things inside; the arrows along this
	-- axis (and Home, End) move among them, as ARIA's composite widgets
	-- behave. `role = "listbox"` is vertical, "toolbar" and "tablist"
	-- horizontal, without asking.
	focus_group: Props_focus_group,
	-- A press on it (or on what inside takes no input) moves the window; a
	-- double click zooms it. Buttons and inputs inside keep their clicks.
	-- CSS pointer-events: "none" (or false) lets the pointer through to what
	-- is under the node (presses, hover, the wheel, drops, a native view);
	-- descendants with handlers still take it. A state or function follows.
	pointer_events: Props_pointer_events,
	window_drag: Props_window_drag,
	-- The pointer selects the text in it and everything it holds, as on a
	-- web page (CSS `user-select: text`): drag, double/triple press, Shift,
	-- ⌘A / Ctrl+A, ⌘C / Ctrl+C. `false` keeps a part out. Off unless set.
	selectable: Props_selectable,
	-- A selectable node's highlight and selected-text colours (CSS
	-- `::selection`), over the theme's `$selection` and `$selection.text`
	-- tokens; a state or function follows.
	selection_color: Props_selection_color,
	selected_text_color: Props_selected_text_color,
	-- Shown over it while the pointer rests on it (after 300 ms; at once
	-- within a second of another): text in the theme's look, or a node of
	-- the app's own. Above it, centred, unless `tooltip_placement` says.
	tooltip: Props_tooltip,
	tooltip_placement: Props_tooltip_placement,
	-- A hover card the pointer can move into and click in: it stays while
	-- the pointer is over the node or the card.
	tooltip_interactive: Props_tooltip_interactive,
	-- Milliseconds the pointer rests before it shows (300; once one
	-- shows, the next shows at once). Keyboard focus shows it too.
	tooltip_delay: Props_tooltip_delay,
	-- A condition (true disables), or the styles it shows while disabled.
	disabled: Props_disabled,
	hover: Props_hover,
	active: Props_active,
	focus: Props_focus,
	-- Size animation ("height", "width", "size"; `true` is "height") or
	-- position animation ("position"). A `layout_id` node mounting as
	-- another with the same id unmounts (or while it is mounted) moves and
	-- scales from that one's box.
	layout: Props_layout,
	layout_id: Props_layout_id,
	layout_ease: Props_layout_ease,
	-- Milliseconds.
	layout_duration: Props_layout_duration,
	layout_enter: Props_layout_enter,
	layout_clip: Props_layout_clip,
	-- The content shows through the bitmap's alpha: `mask = bitmap`, or
	-- `{ bitmap, w = …, h = …, x = …, y = …, sweep = … }`.
	mask: Props_mask,
	bg_shader: Props_bg_shader,
	-- Up to four vec4s: lists of numbers, or colors (0 to 1).
	shader_params: Props_shader_params,
	-- Events.
	on_click: Props_on_click,
	-- Primary pointer down and up.
	on_press: Props_on_press,
	on_release: Props_on_release,
	on_hover: Props_on_hover,
	on_pointer_move: Props_on_pointer_move,
	-- Return true to consume the key.
	on_key: Props_on_key,
	on_key_up: Props_on_key_up,
	on_focus: Props_on_focus,
	on_blur: Props_on_blur,
	-- Return true to take the gesture from the scroll views around it.
	on_wheel: Props_on_wheel,
	on_pinch: Props_on_pinch,
	on_mount: Props_on_mount,
	-- When layout gives it a new size, and at its first layout.
	on_resize: Props_on_resize,
	-- When any of it comes into view or all of it leaves (inside every
	-- clip and scroll viewport above it), and at its first layout: for
	-- building what only matters on screen.
	on_visible: Props_on_visible,
	-- Drag and drop within the window. Payloads are what `on_drag`
	-- returns: the script's own plain values, so they are `any` here.
	-- A source: `on_drag` returns the payload once the pointer moves 4
	-- points; `drag_preview` builds what follows the pointer.
	on_drag: Props_on_drag,
	drag_preview: Props_drag_preview,
	on_drag_end: Props_on_drag_end,
	-- What the drag carries to other apps as it leaves the window (macOS):
	-- Finder takes files, Mail text, a browser links.
	drag_export: Props_drag_export,
	-- Released outside every window of the app (macOS): a tab torn off its
	-- window. `grab_x`/`grab_y` are where the source was held.
	on_drag_outside: Props_on_drag_outside,
	-- The drag moves its window instead, with no preview (macOS): a
	-- window's only tab. Dropped on another window's destination it drops
	-- there; elsewhere the window stays where it was let go.
	drag_moves_window: Props_drag_moves_window,
	-- A destination: `drop_when` filters payloads (default: all).
	drop_when: Props_drop_when,
	on_drag_over: Props_on_drag_over,
	on_drag_leave: Props_on_drag_leave,
	on_drop: Props_on_drop,
	-- A secondary click (control-click on macOS) shows the returned
	-- `menu_item`s, `submenu`s and separators as a menu at the pointer;
	-- returning nil (or nothing) shows none.
	context_menu: Props_context_menu,
	-- The context menu panel's look: bg, rounded, p, gap, backdrop_blur,
	-- border, shadow, w, …
	context_menu_style: Props_context_menu_style,
	-- Open the context menu under the node, from its left edge (a tab's),
	-- rather than at the pointer.
	context_menu_below: Props_context_menu_below,
	[number]: never,
	open: State<boolean>,
	commands: { PaletteCommand }?,
	-- Given the chosen id; without it, the app's command runs.
	on_run: ((id: string) -> ...unknown)?,
}
export type SelectOption = string | number | { label: string, value: any?, disabled: boolean? }
-- A button showing the chosen option that opens a menu of them over
-- itself; closed, typing chooses (as a native select), Down opens it.
export type SelectProps = {
	-- Size and position.
	w: Props_w,
	h: Props_h,
	size: Props_size,
	min_w: Props_min_w,
	max_w: Props_max_w,
	min_h: Props_min_h,
	max_h: Props_max_h,
	w_full: Props_w_full,
	h_full: Props_h_full,
	aspect: Props_aspect,
	absolute: Props_absolute,
	relative: Props_relative,
	top: Props_top,
	left: Props_left,
	inset_start: Props_inset_start,
	inset_end: Props_inset_end,
	right: Props_right,
	bottom: Props_bottom,
	-- Spacing.
	p: Props_p,
	px: Props_px,
	py: Props_py,
	pt: Props_pt,
	pr: Props_pr,
	pb: Props_pb,
	pl: Props_pl,
	ps: Props_ps,
	pe: Props_pe,
	m: Props_m,
	mx: Props_mx,
	my: Props_my,
	mt: Props_mt,
	mr: Props_mr,
	mb: Props_mb,
	ml: Props_ml,
	ms: Props_ms,
	me: Props_me,
	gap: Props_gap,
	gap_x: Props_gap_x,
	gap_y: Props_gap_y,
	-- Flex and grid.
	direction: Props_direction,
	dir: Props_dir,
	grow: Props_grow,
	shrink: Props_shrink,
	basis: Props_basis,
	flex_wrap: Props_flex_wrap,
	wrap_reverse: Props_wrap_reverse,
	items: Props_items,
	justify: Props_justify,
	self: Props_self,
	content: Props_content,
	grid: Props_grid,
	cols: Props_cols,
	rows: Props_rows,
	col_start: Props_col_start,
	row_start: Props_row_start,
	col_span: Props_col_span,
	row_span: Props_row_span,
	col_span_full: Props_col_span_full,
	row_span_full: Props_row_span_full,
	overflow: Props_overflow,
	overflow_x: Props_overflow_x,
	overflow_y: Props_overflow_y,
	hidden: Props_hidden,
	invisible: Props_invisible,
	-- Paint.
	bg: Props_bg,
	gradient: Props_gradient,
	color: Props_color,
	bg_image: Props_bg_image,
	bg_fit: Props_bg_fit,
	rounded: Props_rounded,
	corners: Props_corners,
	rounded_tl: Props_rounded_tl,
	rounded_tr: Props_rounded_tr,
	rounded_br: Props_rounded_br,
	rounded_bl: Props_rounded_bl,
	border: Props_border,
	border_sides: Props_border_sides,
	border_color: Props_border_color,
	border_over: Props_border_over,
	-- A function may return nil: a solid border.
	border_dash: Props_border_dash,
	outline: Props_outline,
	outline_color: Props_outline_color,
	outline_offset: Props_outline_offset,
	shadow: Props_shadow,
	inset_shadow: Props_inset_shadow,
	opacity: Props_opacity,
	brightness: Props_brightness,
	contrast: Props_contrast,
	saturate: Props_saturate,
	blur: Props_blur,
	backdrop_blur: Props_backdrop_blur,
	fade: Props_fade,
	fade_x: Props_fade_x,
	fade_sides: Props_fade_sides,
	fade_overflow: Props_fade_overflow,
	snap: Props_snap,
	clip_inset: Props_clip_inset,
	layer: Props_layer,
	fit: Props_fit,
	tint: Props_tint,
	cursor: Props_cursor,
	-- Text.
	text_size: Props_text_size,
	weight: Props_weight,
	bold: Props_bold,
	italic: Props_italic,
	align: Props_align,
	font: Props_font,
	wrap: Props_wrap,
	truncate: Props_truncate,
	lines: Props_lines,
	line_height: Props_line_height,
	letter_spacing: Props_letter_spacing,
	-- Tabular figures: every digit the same width.
	tabular: Props_tabular,
	-- Underlined text (inherited, as CSS's `text-decoration`), and how far
	-- below the baseline the line sits.
	underline: Props_underline,
	underline_offset: Props_underline_offset,
	-- Transforms: after layout, about `origin` (fractions of the node).
	x: Props_x,
	y: Props_y,
	scale: Props_scale,
	scale_x: Props_scale_x,
	scale_y: Props_scale_y,
	rotate: Props_rotate,
	skew: Props_skew,
	skew_x: Props_skew_x,
	skew_y: Props_skew_y,
	origin: Props_origin,
	-- The subtree's theme: a theme's name (`"zgx-light"`), or tokens to
	-- override (`{ accent = "#e5484d" }`, with `theme = name` to start from).
	theme: Props_theme,
	-- Identity, for selectors, refs and animations.
	id: Props_id,
	-- Space-separated class names.
	class: Props_class,
	-- Styles chosen from a `ui.variants` definition: `button { tone =
	-- "ghost" }`, or the recipe itself for its defaults. The element's own
	-- properties win over them.
	variants: Props_variants,
	ref: Props_ref,
	focusable: Props_focusable,
	-- What assistive technology takes it for (ARIA's `role`), instead of
	-- what it is: a clickable row as an "option", a row of buttons as a
	-- "toolbar". It keeps what it does.
	role: Props_role,
	-- A section title of this level, 1 to 6 (`h1`…`h6`): screen readers
	-- list headings and move between them.
	heading: Props_heading,
	-- Whether this option, tab or item is the chosen one (`aria-selected`).
	selected: Props_selected,
	-- Whether a switch, checkbox or radio of the app's own is on
	-- (`aria-checked`), and whether what a control opens shows
	-- (`aria-expanded`): a custom disclosure's.
	checked: Props_checked,
	expanded: Props_expanded,
	-- Announce changes to the text in it (`aria-live`): a count, a "Sent"
	-- note, a message arriving.
	live: Props_live,
	-- Read after its name, as a hint (`aria-description`).
	description: Props_description,
	-- What is wrong with its value (`aria-invalid`), followed: nil while it
	-- is fine. Assistive technology hears it with the field.
	error: Props_error,
	-- Left out of the accessibility tree with what it holds (`aria-hidden`):
	-- decoration, or what is said elsewhere.
	aria_hidden: Props_aria_hidden,
	-- One Tab stop for the focusable things inside; the arrows along this
	-- axis (and Home, End) move among them, as ARIA's composite widgets
	-- behave. `role = "listbox"` is vertical, "toolbar" and "tablist"
	-- horizontal, without asking.
	focus_group: Props_focus_group,
	-- A press on it (or on what inside takes no input) moves the window; a
	-- double click zooms it. Buttons and inputs inside keep their clicks.
	-- CSS pointer-events: "none" (or false) lets the pointer through to what
	-- is under the node (presses, hover, the wheel, drops, a native view);
	-- descendants with handlers still take it. A state or function follows.
	pointer_events: Props_pointer_events,
	window_drag: Props_window_drag,
	-- The pointer selects the text in it and everything it holds, as on a
	-- web page (CSS `user-select: text`): drag, double/triple press, Shift,
	-- ⌘A / Ctrl+A, ⌘C / Ctrl+C. `false` keeps a part out. Off unless set.
	selectable: Props_selectable,
	-- A selectable node's highlight and selected-text colours (CSS
	-- `::selection`), over the theme's `$selection` and `$selection.text`
	-- tokens; a state or function follows.
	selection_color: Props_selection_color,
	selected_text_color: Props_selected_text_color,
	-- Shown over it while the pointer rests on it (after 300 ms; at once
	-- within a second of another): text in the theme's look, or a node of
	-- the app's own. Above it, centred, unless `tooltip_placement` says.
	tooltip: Props_tooltip,
	tooltip_placement: Props_tooltip_placement,
	-- A hover card the pointer can move into and click in: it stays while
	-- the pointer is over the node or the card.
	tooltip_interactive: Props_tooltip_interactive,
	-- Milliseconds the pointer rests before it shows (300; once one
	-- shows, the next shows at once). Keyboard focus shows it too.
	tooltip_delay: Props_tooltip_delay,
	-- A condition (true disables), or the styles it shows while disabled.
	disabled: Props_disabled,
	hover: Props_hover,
	active: Props_active,
	focus: Props_focus,
	-- Size animation ("height", "width", "size"; `true` is "height") or
	-- position animation ("position"). A `layout_id` node mounting as
	-- another with the same id unmounts (or while it is mounted) moves and
	-- scales from that one's box.
	layout: Props_layout,
	layout_id: Props_layout_id,
	layout_ease: Props_layout_ease,
	-- Milliseconds.
	layout_duration: Props_layout_duration,
	layout_enter: Props_layout_enter,
	layout_clip: Props_layout_clip,
	-- The content shows through the bitmap's alpha: `mask = bitmap`, or
	-- `{ bitmap, w = …, h = …, x = …, y = …, sweep = … }`.
	mask: Props_mask,
	bg_shader: Props_bg_shader,
	-- Up to four vec4s: lists of numbers, or colors (0 to 1).
	shader_params: Props_shader_params,
	-- Events.
	on_click: Props_on_click,
	-- Primary pointer down and up.
	on_press: Props_on_press,
	on_release: Props_on_release,
	on_hover: Props_on_hover,
	on_pointer_move: Props_on_pointer_move,
	-- Return true to consume the key.
	on_key: Props_on_key,
	on_key_up: Props_on_key_up,
	on_focus: Props_on_focus,
	on_blur: Props_on_blur,
	-- Return true to take the gesture from the scroll views around it.
	on_wheel: Props_on_wheel,
	on_pinch: Props_on_pinch,
	on_mount: Props_on_mount,
	-- When layout gives it a new size, and at its first layout.
	on_resize: Props_on_resize,
	-- When any of it comes into view or all of it leaves (inside every
	-- clip and scroll viewport above it), and at its first layout: for
	-- building what only matters on screen.
	on_visible: Props_on_visible,
	-- Drag and drop within the window. Payloads are what `on_drag`
	-- returns: the script's own plain values, so they are `any` here.
	-- A source: `on_drag` returns the payload once the pointer moves 4
	-- points; `drag_preview` builds what follows the pointer.
	on_drag: Props_on_drag,
	drag_preview: Props_drag_preview,
	on_drag_end: Props_on_drag_end,
	-- What the drag carries to other apps as it leaves the window (macOS):
	-- Finder takes files, Mail text, a browser links.
	drag_export: Props_drag_export,
	-- Released outside every window of the app (macOS): a tab torn off its
	-- window. `grab_x`/`grab_y` are where the source was held.
	on_drag_outside: Props_on_drag_outside,
	-- The drag moves its window instead, with no preview (macOS): a
	-- window's only tab. Dropped on another window's destination it drops
	-- there; elsewhere the window stays where it was let go.
	drag_moves_window: Props_drag_moves_window,
	-- A destination: `drop_when` filters payloads (default: all).
	drop_when: Props_drop_when,
	on_drag_over: Props_on_drag_over,
	on_drag_leave: Props_on_drag_leave,
	on_drop: Props_on_drop,
	-- A secondary click (control-click on macOS) shows the returned
	-- `menu_item`s, `submenu`s and separators as a menu at the pointer;
	-- returning nil (or nothing) shows none.
	context_menu: Props_context_menu,
	-- The context menu panel's look: bg, rounded, p, gap, backdrop_blur,
	-- border, shadow, w, …
	context_menu_style: Props_context_menu_style,
	-- Open the context menu under the node, from its left edge (a tab's),
	-- rather than at the pointer.
	context_menu_below: Props_context_menu_below,
	[number]: never,
	-- Required unless `field` gives it.
	value: (State<any>)?,
	-- One of a `ui.form`'s fields (`form.fields.email`): its value, its
	-- leaving, its error (and, a one-line input, Enter submitting the form).
	field: FormField?,
	options: { string } | { number } | { SelectOption },
	-- Shown while no option is chosen.
	placeholder: string?,
	label: string?,
}
-- Two panes (its two children) and a divider between them that drags to
-- resize the first; the second fills the rest.
export type SplitProps = {
	-- Size and position.
	w: Props_w,
	h: Props_h,
	min_w: Props_min_w,
	max_w: Props_max_w,
	min_h: Props_min_h,
	max_h: Props_max_h,
	w_full: Props_w_full,
	h_full: Props_h_full,
	aspect: Props_aspect,
	absolute: Props_absolute,
	relative: Props_relative,
	top: Props_top,
	left: Props_left,
	inset_start: Props_inset_start,
	inset_end: Props_inset_end,
	right: Props_right,
	bottom: Props_bottom,
	-- Spacing.
	p: Props_p,
	px: Props_px,
	py: Props_py,
	pt: Props_pt,
	pr: Props_pr,
	pb: Props_pb,
	pl: Props_pl,
	ps: Props_ps,
	pe: Props_pe,
	m: Props_m,
	mx: Props_mx,
	my: Props_my,
	mt: Props_mt,
	mr: Props_mr,
	mb: Props_mb,
	ml: Props_ml,
	ms: Props_ms,
	me: Props_me,
	gap: Props_gap,
	gap_x: Props_gap_x,
	gap_y: Props_gap_y,
	-- Flex and grid.
	direction: Props_direction,
	dir: Props_dir,
	grow: Props_grow,
	shrink: Props_shrink,
	basis: Props_basis,
	flex_wrap: Props_flex_wrap,
	wrap_reverse: Props_wrap_reverse,
	items: Props_items,
	justify: Props_justify,
	self: Props_self,
	content: Props_content,
	grid: Props_grid,
	cols: Props_cols,
	rows: Props_rows,
	col_start: Props_col_start,
	row_start: Props_row_start,
	col_span: Props_col_span,
	row_span: Props_row_span,
	col_span_full: Props_col_span_full,
	row_span_full: Props_row_span_full,
	overflow: Props_overflow,
	overflow_x: Props_overflow_x,
	overflow_y: Props_overflow_y,
	hidden: Props_hidden,
	invisible: Props_invisible,
	-- Paint.
	bg: Props_bg,
	gradient: Props_gradient,
	color: Props_color,
	bg_image: Props_bg_image,
	bg_fit: Props_bg_fit,
	rounded: Props_rounded,
	corners: Props_corners,
	rounded_tl: Props_rounded_tl,
	rounded_tr: Props_rounded_tr,
	rounded_br: Props_rounded_br,
	rounded_bl: Props_rounded_bl,
	border: Props_border,
	border_sides: Props_border_sides,
	border_color: Props_border_color,
	border_over: Props_border_over,
	-- A function may return nil: a solid border.
	border_dash: Props_border_dash,
	outline: Props_outline,
	outline_color: Props_outline_color,
	outline_offset: Props_outline_offset,
	shadow: Props_shadow,
	inset_shadow: Props_inset_shadow,
	opacity: Props_opacity,
	brightness: Props_brightness,
	contrast: Props_contrast,
	saturate: Props_saturate,
	blur: Props_blur,
	backdrop_blur: Props_backdrop_blur,
	fade: Props_fade,
	fade_x: Props_fade_x,
	fade_sides: Props_fade_sides,
	fade_overflow: Props_fade_overflow,
	snap: Props_snap,
	clip_inset: Props_clip_inset,
	layer: Props_layer,
	fit: Props_fit,
	tint: Props_tint,
	cursor: Props_cursor,
	-- Text.
	text_size: Props_text_size,
	weight: Props_weight,
	bold: Props_bold,
	italic: Props_italic,
	align: Props_align,
	font: Props_font,
	wrap: Props_wrap,
	truncate: Props_truncate,
	lines: Props_lines,
	line_height: Props_line_height,
	letter_spacing: Props_letter_spacing,
	-- Tabular figures: every digit the same width.
	tabular: Props_tabular,
	-- Underlined text (inherited, as CSS's `text-decoration`), and how far
	-- below the baseline the line sits.
	underline: Props_underline,
	underline_offset: Props_underline_offset,
	-- Transforms: after layout, about `origin` (fractions of the node).
	x: Props_x,
	y: Props_y,
	scale: Props_scale,
	scale_x: Props_scale_x,
	scale_y: Props_scale_y,
	rotate: Props_rotate,
	skew: Props_skew,
	skew_x: Props_skew_x,
	skew_y: Props_skew_y,
	origin: Props_origin,
	-- The subtree's theme: a theme's name (`"zgx-light"`), or tokens to
	-- override (`{ accent = "#e5484d" }`, with `theme = name` to start from).
	theme: Props_theme,
	-- Identity, for selectors, refs and animations.
	id: Props_id,
	-- Space-separated class names.
	class: Props_class,
	-- Styles chosen from a `ui.variants` definition: `button { tone =
	-- "ghost" }`, or the recipe itself for its defaults. The element's own
	-- properties win over them.
	variants: Props_variants,
	ref: Props_ref,
	focusable: Props_focusable,
	-- What assistive technology takes it for (ARIA's `role`), instead of
	-- what it is: a clickable row as an "option", a row of buttons as a
	-- "toolbar". It keeps what it does.
	role: Props_role,
	-- A section title of this level, 1 to 6 (`h1`…`h6`): screen readers
	-- list headings and move between them.
	heading: Props_heading,
	-- Whether this option, tab or item is the chosen one (`aria-selected`).
	selected: Props_selected,
	-- Whether a switch, checkbox or radio of the app's own is on
	-- (`aria-checked`), and whether what a control opens shows
	-- (`aria-expanded`): a custom disclosure's.
	checked: Props_checked,
	expanded: Props_expanded,
	-- Announce changes to the text in it (`aria-live`): a count, a "Sent"
	-- note, a message arriving.
	live: Props_live,
	-- Read after its name, as a hint (`aria-description`).
	description: Props_description,
	-- What is wrong with its value (`aria-invalid`), followed: nil while it
	-- is fine. Assistive technology hears it with the field.
	error: Props_error,
	-- Left out of the accessibility tree with what it holds (`aria-hidden`):
	-- decoration, or what is said elsewhere.
	aria_hidden: Props_aria_hidden,
	-- One Tab stop for the focusable things inside; the arrows along this
	-- axis (and Home, End) move among them, as ARIA's composite widgets
	-- behave. `role = "listbox"` is vertical, "toolbar" and "tablist"
	-- horizontal, without asking.
	focus_group: Props_focus_group,
	-- A press on it (or on what inside takes no input) moves the window; a
	-- double click zooms it. Buttons and inputs inside keep their clicks.
	-- CSS pointer-events: "none" (or false) lets the pointer through to what
	-- is under the node (presses, hover, the wheel, drops, a native view);
	-- descendants with handlers still take it. A state or function follows.
	pointer_events: Props_pointer_events,
	window_drag: Props_window_drag,
	-- The pointer selects the text in it and everything it holds, as on a
	-- web page (CSS `user-select: text`): drag, double/triple press, Shift,
	-- ⌘A / Ctrl+A, ⌘C / Ctrl+C. `false` keeps a part out. Off unless set.
	selectable: Props_selectable,
	-- A selectable node's highlight and selected-text colours (CSS
	-- `::selection`), over the theme's `$selection` and `$selection.text`
	-- tokens; a state or function follows.
	selection_color: Props_selection_color,
	selected_text_color: Props_selected_text_color,
	-- Shown over it while the pointer rests on it (after 300 ms; at once
	-- within a second of another): text in the theme's look, or a node of
	-- the app's own. Above it, centred, unless `tooltip_placement` says.
	tooltip: Props_tooltip,
	tooltip_placement: Props_tooltip_placement,
	-- A hover card the pointer can move into and click in: it stays while
	-- the pointer is over the node or the card.
	tooltip_interactive: Props_tooltip_interactive,
	-- Milliseconds the pointer rests before it shows (300; once one
	-- shows, the next shows at once). Keyboard focus shows it too.
	tooltip_delay: Props_tooltip_delay,
	-- A condition (true disables), or the styles it shows while disabled.
	disabled: Props_disabled,
	hover: Props_hover,
	active: Props_active,
	focus: Props_focus,
	-- Size animation ("height", "width", "size"; `true` is "height") or
	-- position animation ("position"). A `layout_id` node mounting as
	-- another with the same id unmounts (or while it is mounted) moves and
	-- scales from that one's box.
	layout: Props_layout,
	layout_id: Props_layout_id,
	layout_ease: Props_layout_ease,
	-- Milliseconds.
	layout_duration: Props_layout_duration,
	layout_enter: Props_layout_enter,
	layout_clip: Props_layout_clip,
	-- The content shows through the bitmap's alpha: `mask = bitmap`, or
	-- `{ bitmap, w = …, h = …, x = …, y = …, sweep = … }`.
	mask: Props_mask,
	bg_shader: Props_bg_shader,
	-- Up to four vec4s: lists of numbers, or colors (0 to 1).
	shader_params: Props_shader_params,
	-- Events.
	on_click: Props_on_click,
	-- Primary pointer down and up.
	on_press: Props_on_press,
	on_release: Props_on_release,
	on_hover: Props_on_hover,
	on_pointer_move: Props_on_pointer_move,
	-- Return true to consume the key.
	on_key: Props_on_key,
	on_key_up: Props_on_key_up,
	on_focus: Props_on_focus,
	on_blur: Props_on_blur,
	-- Return true to take the gesture from the scroll views around it.
	on_wheel: Props_on_wheel,
	on_pinch: Props_on_pinch,
	on_mount: Props_on_mount,
	-- When layout gives it a new size, and at its first layout.
	on_resize: Props_on_resize,
	-- When any of it comes into view or all of it leaves (inside every
	-- clip and scroll viewport above it), and at its first layout: for
	-- building what only matters on screen.
	on_visible: Props_on_visible,
	-- Drag and drop within the window. Payloads are what `on_drag`
	-- returns: the script's own plain values, so they are `any` here.
	-- A source: `on_drag` returns the payload once the pointer moves 4
	-- points; `drag_preview` builds what follows the pointer.
	on_drag: Props_on_drag,
	drag_preview: Props_drag_preview,
	on_drag_end: Props_on_drag_end,
	-- What the drag carries to other apps as it leaves the window (macOS):
	-- Finder takes files, Mail text, a browser links.
	drag_export: Props_drag_export,
	-- Released outside every window of the app (macOS): a tab torn off its
	-- window. `grab_x`/`grab_y` are where the source was held.
	on_drag_outside: Props_on_drag_outside,
	-- The drag moves its window instead, with no preview (macOS): a
	-- window's only tab. Dropped on another window's destination it drops
	-- there; elsewhere the window stays where it was let go.
	drag_moves_window: Props_drag_moves_window,
	-- A destination: `drop_when` filters payloads (default: all).
	drop_when: Props_drop_when,
	on_drag_over: Props_on_drag_over,
	on_drag_leave: Props_on_drag_leave,
	on_drop: Props_on_drop,
	-- A secondary click (control-click on macOS) shows the returned
	-- `menu_item`s, `submenu`s and separators as a menu at the pointer;
	-- returning nil (or nothing) shows none.
	context_menu: Props_context_menu,
	-- The context menu panel's look: bg, rounded, p, gap, backdrop_blur,
	-- border, shadow, w, …
	context_menu_style: Props_context_menu_style,
	-- Open the context menu under the node, from its left edge (a tab's),
	-- rather than at the pointer.
	context_menu_below: Props_context_menu_below,
	[number]: Child,
	-- The name assistive technology reads for it, instead of its text: a
	-- switch, a button showing only an icon. A state or function is
	-- followed: a list row's name made from what it shows now.
	label: (string | Readable<string> | (() -> string))?,
	-- The first pane's size in points, both ways: the divider writes it as
	-- it drags (a `ui.persist` keeps it across launches).
	size: State<number>,
	-- One pane over the other, instead of side by side.
	vertical: boolean?,
	-- `size` is the second pane's (an inspector at the right), and the
	-- first fills the rest.
	second: boolean?,
	-- The sized pane's smallest and largest size.
	min: number?,
	max: number?,
	-- The other pane's smallest size: the divider stops short of it.
	min_rest: number?,
}
-- A virtualized list: only visible rows are built.
export type ListProps = {
	-- Size and position.
	w: Props_w,
	h: Props_h,
	size: Props_size,
	min_w: Props_min_w,
	max_w: Props_max_w,
	min_h: Props_min_h,
	max_h: Props_max_h,
	w_full: Props_w_full,
	h_full: Props_h_full,
	aspect: Props_aspect,
	absolute: Props_absolute,
	relative: Props_relative,
	top: Props_top,
	left: Props_left,
	inset_start: Props_inset_start,
	inset_end: Props_inset_end,
	right: Props_right,
	bottom: Props_bottom,
	-- Spacing.
	p: Props_p,
	px: Props_px,
	py: Props_py,
	pt: Props_pt,
	pr: Props_pr,
	pb: Props_pb,
	pl: Props_pl,
	ps: Props_ps,
	pe: Props_pe,
	m: Props_m,
	mx: Props_mx,
	my: Props_my,
	mt: Props_mt,
	mr: Props_mr,
	mb: Props_mb,
	ml: Props_ml,
	ms: Props_ms,
	me: Props_me,
	gap: Props_gap,
	gap_x: Props_gap_x,
	gap_y: Props_gap_y,
	-- Flex and grid.
	direction: Props_direction,
	dir: Props_dir,
	grow: Props_grow,
	shrink: Props_shrink,
	basis: Props_basis,
	flex_wrap: Props_flex_wrap,
	wrap_reverse: Props_wrap_reverse,
	items: Props_items,
	justify: Props_justify,
	self: Props_self,
	content: Props_content,
	grid: Props_grid,
	cols: Props_cols,
	rows: Props_rows,
	col_start: Props_col_start,
	row_start: Props_row_start,
	col_span: Props_col_span,
	row_span: Props_row_span,
	col_span_full: Props_col_span_full,
	row_span_full: Props_row_span_full,
	overflow: Props_overflow,
	overflow_x: Props_overflow_x,
	overflow_y: Props_overflow_y,
	hidden: Props_hidden,
	invisible: Props_invisible,
	-- Paint.
	bg: Props_bg,
	gradient: Props_gradient,
	color: Props_color,
	bg_image: Props_bg_image,
	bg_fit: Props_bg_fit,
	rounded: Props_rounded,
	corners: Props_corners,
	rounded_tl: Props_rounded_tl,
	rounded_tr: Props_rounded_tr,
	rounded_br: Props_rounded_br,
	rounded_bl: Props_rounded_bl,
	border: Props_border,
	border_sides: Props_border_sides,
	border_color: Props_border_color,
	border_over: Props_border_over,
	-- A function may return nil: a solid border.
	border_dash: Props_border_dash,
	outline: Props_outline,
	outline_color: Props_outline_color,
	outline_offset: Props_outline_offset,
	shadow: Props_shadow,
	inset_shadow: Props_inset_shadow,
	opacity: Props_opacity,
	brightness: Props_brightness,
	contrast: Props_contrast,
	saturate: Props_saturate,
	blur: Props_blur,
	backdrop_blur: Props_backdrop_blur,
	fade: Props_fade,
	fade_x: Props_fade_x,
	fade_sides: Props_fade_sides,
	fade_overflow: Props_fade_overflow,
	snap: Props_snap,
	clip_inset: Props_clip_inset,
	layer: Props_layer,
	fit: Props_fit,
	tint: Props_tint,
	cursor: Props_cursor,
	-- Text.
	text_size: Props_text_size,
	weight: Props_weight,
	bold: Props_bold,
	italic: Props_italic,
	align: Props_align,
	font: Props_font,
	wrap: Props_wrap,
	truncate: Props_truncate,
	lines: Props_lines,
	line_height: Props_line_height,
	letter_spacing: Props_letter_spacing,
	-- Tabular figures: every digit the same width.
	tabular: Props_tabular,
	-- Underlined text (inherited, as CSS's `text-decoration`), and how far
	-- below the baseline the line sits.
	underline: Props_underline,
	underline_offset: Props_underline_offset,
	-- Transforms: after layout, about `origin` (fractions of the node).
	x: Props_x,
	y: Props_y,
	scale: Props_scale,
	scale_x: Props_scale_x,
	scale_y: Props_scale_y,
	rotate: Props_rotate,
	skew: Props_skew,
	skew_x: Props_skew_x,
	skew_y: Props_skew_y,
	origin: Props_origin,
	-- The subtree's theme: a theme's name (`"zgx-light"`), or tokens to
	-- override (`{ accent = "#e5484d" }`, with `theme = name` to start from).
	theme: Props_theme,
	-- Identity, for selectors, refs and animations.
	id: Props_id,
	-- Space-separated class names.
	class: Props_class,
	-- Styles chosen from a `ui.variants` definition: `button { tone =
	-- "ghost" }`, or the recipe itself for its defaults. The element's own
	-- properties win over them.
	variants: Props_variants,
	ref: Props_ref,
	focusable: Props_focusable,
	-- What assistive technology takes it for (ARIA's `role`), instead of
	-- what it is: a clickable row as an "option", a row of buttons as a
	-- "toolbar". It keeps what it does.
	role: Props_role,
	-- A section title of this level, 1 to 6 (`h1`…`h6`): screen readers
	-- list headings and move between them.
	heading: Props_heading,
	-- Whether this option, tab or item is the chosen one (`aria-selected`).
	selected: Props_selected,
	-- Whether a switch, checkbox or radio of the app's own is on
	-- (`aria-checked`), and whether what a control opens shows
	-- (`aria-expanded`): a custom disclosure's.
	checked: Props_checked,
	expanded: Props_expanded,
	-- Announce changes to the text in it (`aria-live`): a count, a "Sent"
	-- note, a message arriving.
	live: Props_live,
	-- Read after its name, as a hint (`aria-description`).
	description: Props_description,
	-- What is wrong with its value (`aria-invalid`), followed: nil while it
	-- is fine. Assistive technology hears it with the field.
	error: Props_error,
	-- Left out of the accessibility tree with what it holds (`aria-hidden`):
	-- decoration, or what is said elsewhere.
	aria_hidden: Props_aria_hidden,
	-- One Tab stop for the focusable things inside; the arrows along this
	-- axis (and Home, End) move among them, as ARIA's composite widgets
	-- behave. `role = "listbox"` is vertical, "toolbar" and "tablist"
	-- horizontal, without asking.
	focus_group: Props_focus_group,
	-- A press on it (or on what inside takes no input) moves the window; a
	-- double click zooms it. Buttons and inputs inside keep their clicks.
	-- CSS pointer-events: "none" (or false) lets the pointer through to what
	-- is under the node (presses, hover, the wheel, drops, a native view);
	-- descendants with handlers still take it. A state or function follows.
	pointer_events: Props_pointer_events,
	window_drag: Props_window_drag,
	-- The pointer selects the text in it and everything it holds, as on a
	-- web page (CSS `user-select: text`): drag, double/triple press, Shift,
	-- ⌘A / Ctrl+A, ⌘C / Ctrl+C. `false` keeps a part out. Off unless set.
	selectable: Props_selectable,
	-- A selectable node's highlight and selected-text colours (CSS
	-- `::selection`), over the theme's `$selection` and `$selection.text`
	-- tokens; a state or function follows.
	selection_color: Props_selection_color,
	selected_text_color: Props_selected_text_color,
	-- Shown over it while the pointer rests on it (after 300 ms; at once
	-- within a second of another): text in the theme's look, or a node of
	-- the app's own. Above it, centred, unless `tooltip_placement` says.
	tooltip: Props_tooltip,
	tooltip_placement: Props_tooltip_placement,
	-- A hover card the pointer can move into and click in: it stays while
	-- the pointer is over the node or the card.
	tooltip_interactive: Props_tooltip_interactive,
	-- Milliseconds the pointer rests before it shows (300; once one
	-- shows, the next shows at once). Keyboard focus shows it too.
	tooltip_delay: Props_tooltip_delay,
	-- A condition (true disables), or the styles it shows while disabled.
	disabled: Props_disabled,
	hover: Props_hover,
	active: Props_active,
	focus: Props_focus,
	-- Size animation ("height", "width", "size"; `true` is "height") or
	-- position animation ("position"). A `layout_id` node mounting as
	-- another with the same id unmounts (or while it is mounted) moves and
	-- scales from that one's box.
	layout: Props_layout,
	layout_id: Props_layout_id,
	layout_ease: Props_layout_ease,
	-- Milliseconds.
	layout_duration: Props_layout_duration,
	layout_enter: Props_layout_enter,
	layout_clip: Props_layout_clip,
	-- The content shows through the bitmap's alpha: `mask = bitmap`, or
	-- `{ bitmap, w = …, h = …, x = …, y = …, sweep = … }`.
	mask: Props_mask,
	bg_shader: Props_bg_shader,
	-- Up to four vec4s: lists of numbers, or colors (0 to 1).
	shader_params: Props_shader_params,
	-- Events.
	on_click: Props_on_click,
	-- Primary pointer down and up.
	on_press: Props_on_press,
	on_release: Props_on_release,
	on_hover: Props_on_hover,
	on_pointer_move: Props_on_pointer_move,
	-- Return true to consume the key.
	on_key: Props_on_key,
	on_key_up: Props_on_key_up,
	on_focus: Props_on_focus,
	on_blur: Props_on_blur,
	-- Return true to take the gesture from the scroll views around it.
	on_wheel: Props_on_wheel,
	on_pinch: Props_on_pinch,
	on_mount: Props_on_mount,
	-- When layout gives it a new size, and at its first layout.
	on_resize: Props_on_resize,
	-- When any of it comes into view or all of it leaves (inside every
	-- clip and scroll viewport above it), and at its first layout: for
	-- building what only matters on screen.
	on_visible: Props_on_visible,
	-- Drag and drop within the window. Payloads are what `on_drag`
	-- returns: the script's own plain values, so they are `any` here.
	-- A source: `on_drag` returns the payload once the pointer moves 4
	-- points; `drag_preview` builds what follows the pointer.
	on_drag: Props_on_drag,
	drag_preview: Props_drag_preview,
	on_drag_end: Props_on_drag_end,
	-- What the drag carries to other apps as it leaves the window (macOS):
	-- Finder takes files, Mail text, a browser links.
	drag_export: Props_drag_export,
	-- Released outside every window of the app (macOS): a tab torn off its
	-- window. `grab_x`/`grab_y` are where the source was held.
	on_drag_outside: Props_on_drag_outside,
	-- The drag moves its window instead, with no preview (macOS): a
	-- window's only tab. Dropped on another window's destination it drops
	-- there; elsewhere the window stays where it was let go.
	drag_moves_window: Props_drag_moves_window,
	-- A destination: `drop_when` filters payloads (default: all).
	drop_when: Props_drop_when,
	on_drag_over: Props_on_drag_over,
	on_drag_leave: Props_on_drag_leave,
	on_drop: Props_on_drop,
	-- A secondary click (control-click on macOS) shows the returned
	-- `menu_item`s, `submenu`s and separators as a menu at the pointer;
	-- returning nil (or nothing) shows none.
	context_menu: Props_context_menu,
	-- The context menu panel's look: bg, rounded, p, gap, backdrop_blur,
	-- border, shadow, w, …
	context_menu_style: Props_context_menu_style,
	-- Open the context menu under the node, from its left edge (a tab's),
	-- rather than at the pointer.
	context_menu_below: Props_context_menu_below,
	[number]: never,
	-- The name assistive technology reads for it: "Messages".
	label: string?,
	-- The row count (with `heights`, their length).
	count: Reactive<number>?,
	-- An interactive scrollbar over the trailing edge (none by default).
	scrollbar: boolean?,
	-- Its rows are one Tab stop, and the arrows, Page Up and Down, Home and
	-- End move focus through every row (mounting it), as a list box's do.
	keyboard: boolean?,
	-- Builds row `index` (from 1); with `key`, the row of item `key`, given
	-- its index as a state that follows where the row is.
	row: ((index: number) -> Node) | ((key: any, index: State<number>) -> Node),
	-- Each row's item's key (a number, string or boolean), as a function of
	-- its index: a row follows its item as the list changes, moved rather
	-- than written again (rows a removal or an insertion shifts keep their
	-- nodes, text and layout).
	key: ((index: number) -> any)?,
	-- Every row's height in points (28 by default); with `measured`, the
	-- estimate for rows not yet measured.
	height: number?,
	-- Rows take their content's height (wrapped text, images), measured as
	-- they mount; the rest are estimated (`height`), and the rows in view
	-- stay put as estimates are measured. With `key`, heights go with
	-- their items.
	measured: boolean?,
	-- Row `index`'s plain text: what copying a selection over rows the list
	-- has not mounted takes. Without it, a row's text is what its row's
	-- description shows (its texts, a row's side by side with tabs).
	row_text: ((index: number) -> string?)?,
	-- With `measured` or `heights`: resting at the end, it stays there as
	-- rows are added and measured (a chat); scrolled up, it stays put.
	follow: boolean?,
	-- Each row's own height instead, in order: a list, a state holding one
	-- (a host's shared value costs the script nothing), or a function
	-- returning one (tracked). The row count is its length.
	heights: ({ number } | Readable<{ number }> | (() -> { number }))?,
	-- Rows (from 1, in order) that stick to the top while their section
	-- scrolls past, each pushed up by the next: section headers. With
	-- `heights`.
	sticky: ({ number } | Readable<{ number }> | (() -> { number }))?,
	-- How far below the top the sticky rows stick (CSS's sticky `top`).
	sticky_top: number?,
	-- Rows built beyond each edge (2 by default).
	overscan: number?,
	offset: State<number>?,
}
-- Where a modal's panel sits in the window: lined up with its start,
-- centre or end across (`x`) and down (`y`), `inset` points from the edges
-- (a number, or { top, right, bottom, left }). Centred unless set.
export type ModalPlacement = {
	x: ("start" | "center" | "end")?,
	y: ("start" | "center" | "end")?,
	inset: (number | { number })?,
}
export type LayerProps = {
	-- Size and position.
	w: Props_w,
	h: Props_h,
	size: Props_size,
	min_w: Props_min_w,
	max_w: Props_max_w,
	min_h: Props_min_h,
	max_h: Props_max_h,
	w_full: Props_w_full,
	h_full: Props_h_full,
	aspect: Props_aspect,
	absolute: Props_absolute,
	relative: Props_relative,
	top: Props_top,
	left: Props_left,
	inset_start: Props_inset_start,
	inset_end: Props_inset_end,
	right: Props_right,
	bottom: Props_bottom,
	-- Spacing.
	p: Props_p,
	px: Props_px,
	py: Props_py,
	pt: Props_pt,
	pr: Props_pr,
	pb: Props_pb,
	pl: Props_pl,
	ps: Props_ps,
	pe: Props_pe,
	m: Props_m,
	mx: Props_mx,
	my: Props_my,
	mt: Props_mt,
	mr: Props_mr,
	mb: Props_mb,
	ml: Props_ml,
	ms: Props_ms,
	me: Props_me,
	gap: Props_gap,
	gap_x: Props_gap_x,
	gap_y: Props_gap_y,
	-- Flex and grid.
	direction: Props_direction,
	dir: Props_dir,
	grow: Props_grow,
	shrink: Props_shrink,
	basis: Props_basis,
	flex_wrap: Props_flex_wrap,
	wrap_reverse: Props_wrap_reverse,
	items: Props_items,
	justify: Props_justify,
	self: Props_self,
	content: Props_content,
	grid: Props_grid,
	cols: Props_cols,
	rows: Props_rows,
	col_start: Props_col_start,
	row_start: Props_row_start,
	col_span: Props_col_span,
	row_span: Props_row_span,
	col_span_full: Props_col_span_full,
	row_span_full: Props_row_span_full,
	overflow: Props_overflow,
	overflow_x: Props_overflow_x,
	overflow_y: Props_overflow_y,
	hidden: Props_hidden,
	invisible: Props_invisible,
	-- Paint.
	bg: Props_bg,
	gradient: Props_gradient,
	color: Props_color,
	bg_image: Props_bg_image,
	bg_fit: Props_bg_fit,
	rounded: Props_rounded,
	corners: Props_corners,
	rounded_tl: Props_rounded_tl,
	rounded_tr: Props_rounded_tr,
	rounded_br: Props_rounded_br,
	rounded_bl: Props_rounded_bl,
	border: Props_border,
	border_sides: Props_border_sides,
	border_color: Props_border_color,
	border_over: Props_border_over,
	-- A function may return nil: a solid border.
	border_dash: Props_border_dash,
	outline: Props_outline,
	outline_color: Props_outline_color,
	outline_offset: Props_outline_offset,
	shadow: Props_shadow,
	inset_shadow: Props_inset_shadow,
	opacity: Props_opacity,
	brightness: Props_brightness,
	contrast: Props_contrast,
	saturate: Props_saturate,
	blur: Props_blur,
	backdrop_blur: Props_backdrop_blur,
	fade: Props_fade,
	fade_x: Props_fade_x,
	fade_sides: Props_fade_sides,
	fade_overflow: Props_fade_overflow,
	snap: Props_snap,
	clip_inset: Props_clip_inset,
	layer: Props_layer,
	fit: Props_fit,
	tint: Props_tint,
	cursor: Props_cursor,
	-- Text.
	text_size: Props_text_size,
	weight: Props_weight,
	bold: Props_bold,
	italic: Props_italic,
	align: Props_align,
	font: Props_font,
	wrap: Props_wrap,
	truncate: Props_truncate,
	lines: Props_lines,
	line_height: Props_line_height,
	letter_spacing: Props_letter_spacing,
	-- Tabular figures: every digit the same width.
	tabular: Props_tabular,
	-- Underlined text (inherited, as CSS's `text-decoration`), and how far
	-- below the baseline the line sits.
	underline: Props_underline,
	underline_offset: Props_underline_offset,
	-- Transforms: after layout, about `origin` (fractions of the node).
	x: Props_x,
	y: Props_y,
	scale: Props_scale,
	scale_x: Props_scale_x,
	scale_y: Props_scale_y,
	rotate: Props_rotate,
	skew: Props_skew,
	skew_x: Props_skew_x,
	skew_y: Props_skew_y,
	origin: Props_origin,
	-- The subtree's theme: a theme's name (`"zgx-light"`), or tokens to
	-- override (`{ accent = "#e5484d" }`, with `theme = name` to start from).
	theme: Props_theme,
	-- Identity, for selectors, refs and animations.
	id: Props_id,
	-- Space-separated class names.
	class: Props_class,
	-- Styles chosen from a `ui.variants` definition: `button { tone =
	-- "ghost" }`, or the recipe itself for its defaults. The element's own
	-- properties win over them.
	variants: Props_variants,
	ref: Props_ref,
	focusable: Props_focusable,
	-- What assistive technology takes it for (ARIA's `role`), instead of
	-- what it is: a clickable row as an "option", a row of buttons as a
	-- "toolbar". It keeps what it does.
	role: Props_role,
	-- A section title of this level, 1 to 6 (`h1`…`h6`): screen readers
	-- list headings and move between them.
	heading: Props_heading,
	-- Whether this option, tab or item is the chosen one (`aria-selected`).
	selected: Props_selected,
	-- Whether a switch, checkbox or radio of the app's own is on
	-- (`aria-checked`), and whether what a control opens shows
	-- (`aria-expanded`): a custom disclosure's.
	checked: Props_checked,
	expanded: Props_expanded,
	-- Announce changes to the text in it (`aria-live`): a count, a "Sent"
	-- note, a message arriving.
	live: Props_live,
	-- Read after its name, as a hint (`aria-description`).
	description: Props_description,
	-- What is wrong with its value (`aria-invalid`), followed: nil while it
	-- is fine. Assistive technology hears it with the field.
	error: Props_error,
	-- Left out of the accessibility tree with what it holds (`aria-hidden`):
	-- decoration, or what is said elsewhere.
	aria_hidden: Props_aria_hidden,
	-- One Tab stop for the focusable things inside; the arrows along this
	-- axis (and Home, End) move among them, as ARIA's composite widgets
	-- behave. `role = "listbox"` is vertical, "toolbar" and "tablist"
	-- horizontal, without asking.
	focus_group: Props_focus_group,
	-- A press on it (or on what inside takes no input) moves the window; a
	-- double click zooms it. Buttons and inputs inside keep their clicks.
	-- CSS pointer-events: "none" (or false) lets the pointer through to what
	-- is under the node (presses, hover, the wheel, drops, a native view);
	-- descendants with handlers still take it. A state or function follows.
	pointer_events: Props_pointer_events,
	window_drag: Props_window_drag,
	-- The pointer selects the text in it and everything it holds, as on a
	-- web page (CSS `user-select: text`): drag, double/triple press, Shift,
	-- ⌘A / Ctrl+A, ⌘C / Ctrl+C. `false` keeps a part out. Off unless set.
	selectable: Props_selectable,
	-- A selectable node's highlight and selected-text colours (CSS
	-- `::selection`), over the theme's `$selection` and `$selection.text`
	-- tokens; a state or function follows.
	selection_color: Props_selection_color,
	selected_text_color: Props_selected_text_color,
	-- Shown over it while the pointer rests on it (after 300 ms; at once
	-- within a second of another): text in the theme's look, or a node of
	-- the app's own. Above it, centred, unless `tooltip_placement` says.
	tooltip: Props_tooltip,
	tooltip_placement: Props_tooltip_placement,
	-- A hover card the pointer can move into and click in: it stays while
	-- the pointer is over the node or the card.
	tooltip_interactive: Props_tooltip_interactive,
	-- Milliseconds the pointer rests before it shows (300; once one
	-- shows, the next shows at once). Keyboard focus shows it too.
	tooltip_delay: Props_tooltip_delay,
	-- A condition (true disables), or the styles it shows while disabled.
	disabled: Props_disabled,
	hover: Props_hover,
	active: Props_active,
	focus: Props_focus,
	-- Size animation ("height", "width", "size"; `true` is "height") or
	-- position animation ("position"). A `layout_id` node mounting as
	-- another with the same id unmounts (or while it is mounted) moves and
	-- scales from that one's box.
	layout: Props_layout,
	layout_id: Props_layout_id,
	layout_ease: Props_layout_ease,
	-- Milliseconds.
	layout_duration: Props_layout_duration,
	layout_enter: Props_layout_enter,
	layout_clip: Props_layout_clip,
	-- The content shows through the bitmap's alpha: `mask = bitmap`, or
	-- `{ bitmap, w = …, h = …, x = …, y = …, sweep = … }`.
	mask: Props_mask,
	bg_shader: Props_bg_shader,
	-- Up to four vec4s: lists of numbers, or colors (0 to 1).
	shader_params: Props_shader_params,
	-- Events.
	on_click: Props_on_click,
	-- Primary pointer down and up.
	on_press: Props_on_press,
	on_release: Props_on_release,
	on_hover: Props_on_hover,
	on_pointer_move: Props_on_pointer_move,
	-- Return true to consume the key.
	on_key: Props_on_key,
	on_key_up: Props_on_key_up,
	on_focus: Props_on_focus,
	on_blur: Props_on_blur,
	-- Return true to take the gesture from the scroll views around it.
	on_wheel: Props_on_wheel,
	on_pinch: Props_on_pinch,
	on_mount: Props_on_mount,
	-- When layout gives it a new size, and at its first layout.
	on_resize: Props_on_resize,
	-- When any of it comes into view or all of it leaves (inside every
	-- clip and scroll viewport above it), and at its first layout: for
	-- building what only matters on screen.
	on_visible: Props_on_visible,
	-- Drag and drop within the window. Payloads are what `on_drag`
	-- returns: the script's own plain values, so they are `any` here.
	-- A source: `on_drag` returns the payload once the pointer moves 4
	-- points; `drag_preview` builds what follows the pointer.
	on_drag: Props_on_drag,
	drag_preview: Props_drag_preview,
	on_drag_end: Props_on_drag_end,
	-- What the drag carries to other apps as it leaves the window (macOS):
	-- Finder takes files, Mail text, a browser links.
	drag_export: Props_drag_export,
	-- Released outside every window of the app (macOS): a tab torn off its
	-- window. `grab_x`/`grab_y` are where the source was held.
	on_drag_outside: Props_on_drag_outside,
	-- The drag moves its window instead, with no preview (macOS): a
	-- window's only tab. Dropped on another window's destination it drops
	-- there; elsewhere the window stays where it was let go.
	drag_moves_window: Props_drag_moves_window,
	-- A destination: `drop_when` filters payloads (default: all).
	drop_when: Props_drop_when,
	on_drag_over: Props_on_drag_over,
	on_drag_leave: Props_on_drag_leave,
	on_drop: Props_on_drop,
	-- A secondary click (control-click on macOS) shows the returned
	-- `menu_item`s, `submenu`s and separators as a menu at the pointer;
	-- returning nil (or nothing) shows none.
	context_menu: Props_context_menu,
	-- The context menu panel's look: bg, rounded, p, gap, backdrop_blur,
	-- border, shadow, w, …
	context_menu_style: Props_context_menu_style,
	-- Open the context menu under the node, from its left edge (a tab's),
	-- rather than at the pointer.
	context_menu_below: Props_context_menu_below,
	[number]: Child,
	open: State<boolean>?,
	label: string?,
	placement: ModalPlacement?,
}
export type ModalProps = {
	-- Size and position.
	w: Props_w,
	h: Props_h,
	size: Props_size,
	min_w: Props_min_w,
	max_w: Props_max_w,
	min_h: Props_min_h,
	max_h: Props_max_h,
	w_full: Props_w_full,
	h_full: Props_h_full,
	aspect: Props_aspect,
	absolute: Props_absolute,
	relative: Props_relative,
	top: Props_top,
	left: Props_left,
	inset_start: Props_inset_start,
	inset_end: Props_inset_end,
	right: Props_right,
	bottom: Props_bottom,
	-- Spacing.
	p: Props_p,
	px: Props_px,
	py: Props_py,
	pt: Props_pt,
	pr: Props_pr,
	pb: Props_pb,
	pl: Props_pl,
	ps: Props_ps,
	pe: Props_pe,
	m: Props_m,
	mx: Props_mx,
	my: Props_my,
	mt: Props_mt,
	mr: Props_mr,
	mb: Props_mb,
	ml: Props_ml,
	ms: Props_ms,
	me: Props_me,
	gap: Props_gap,
	gap_x: Props_gap_x,
	gap_y: Props_gap_y,
	-- Flex and grid.
	direction: Props_direction,
	dir: Props_dir,
	grow: Props_grow,
	shrink: Props_shrink,
	basis: Props_basis,
	flex_wrap: Props_flex_wrap,
	wrap_reverse: Props_wrap_reverse,
	items: Props_items,
	justify: Props_justify,
	self: Props_self,
	content: Props_content,
	grid: Props_grid,
	cols: Props_cols,
	rows: Props_rows,
	col_start: Props_col_start,
	row_start: Props_row_start,
	col_span: Props_col_span,
	row_span: Props_row_span,
	col_span_full: Props_col_span_full,
	row_span_full: Props_row_span_full,
	overflow: Props_overflow,
	overflow_x: Props_overflow_x,
	overflow_y: Props_overflow_y,
	hidden: Props_hidden,
	invisible: Props_invisible,
	-- Paint.
	bg: Props_bg,
	gradient: Props_gradient,
	color: Props_color,
	bg_image: Props_bg_image,
	bg_fit: Props_bg_fit,
	rounded: Props_rounded,
	corners: Props_corners,
	rounded_tl: Props_rounded_tl,
	rounded_tr: Props_rounded_tr,
	rounded_br: Props_rounded_br,
	rounded_bl: Props_rounded_bl,
	border: Props_border,
	border_sides: Props_border_sides,
	border_color: Props_border_color,
	border_over: Props_border_over,
	-- A function may return nil: a solid border.
	border_dash: Props_border_dash,
	outline: Props_outline,
	outline_color: Props_outline_color,
	outline_offset: Props_outline_offset,
	shadow: Props_shadow,
	inset_shadow: Props_inset_shadow,
	opacity: Props_opacity,
	brightness: Props_brightness,
	contrast: Props_contrast,
	saturate: Props_saturate,
	blur: Props_blur,
	backdrop_blur: Props_backdrop_blur,
	fade: Props_fade,
	fade_x: Props_fade_x,
	fade_sides: Props_fade_sides,
	fade_overflow: Props_fade_overflow,
	snap: Props_snap,
	clip_inset: Props_clip_inset,
	layer: Props_layer,
	fit: Props_fit,
	tint: Props_tint,
	cursor: Props_cursor,
	-- Text.
	text_size: Props_text_size,
	weight: Props_weight,
	bold: Props_bold,
	italic: Props_italic,
	align: Props_align,
	font: Props_font,
	wrap: Props_wrap,
	truncate: Props_truncate,
	lines: Props_lines,
	line_height: Props_line_height,
	letter_spacing: Props_letter_spacing,
	-- Tabular figures: every digit the same width.
	tabular: Props_tabular,
	-- Underlined text (inherited, as CSS's `text-decoration`), and how far
	-- below the baseline the line sits.
	underline: Props_underline,
	underline_offset: Props_underline_offset,
	-- Transforms: after layout, about `origin` (fractions of the node).
	x: Props_x,
	y: Props_y,
	scale: Props_scale,
	scale_x: Props_scale_x,
	scale_y: Props_scale_y,
	rotate: Props_rotate,
	skew: Props_skew,
	skew_x: Props_skew_x,
	skew_y: Props_skew_y,
	origin: Props_origin,
	-- The subtree's theme: a theme's name (`"zgx-light"`), or tokens to
	-- override (`{ accent = "#e5484d" }`, with `theme = name` to start from).
	theme: Props_theme,
	-- Identity, for selectors, refs and animations.
	id: Props_id,
	-- Space-separated class names.
	class: Props_class,
	-- Styles chosen from a `ui.variants` definition: `button { tone =
	-- "ghost" }`, or the recipe itself for its defaults. The element's own
	-- properties win over them.
	variants: Props_variants,
	ref: Props_ref,
	focusable: Props_focusable,
	-- What assistive technology takes it for (ARIA's `role`), instead of
	-- what it is: a clickable row as an "option", a row of buttons as a
	-- "toolbar". It keeps what it does.
	role: Props_role,
	-- A section title of this level, 1 to 6 (`h1`…`h6`): screen readers
	-- list headings and move between them.
	heading: Props_heading,
	-- Whether this option, tab or item is the chosen one (`aria-selected`).
	selected: Props_selected,
	-- Whether a switch, checkbox or radio of the app's own is on
	-- (`aria-checked`), and whether what a control opens shows
	-- (`aria-expanded`): a custom disclosure's.
	checked: Props_checked,
	expanded: Props_expanded,
	-- Announce changes to the text in it (`aria-live`): a count, a "Sent"
	-- note, a message arriving.
	live: Props_live,
	-- Read after its name, as a hint (`aria-description`).
	description: Props_description,
	-- What is wrong with its value (`aria-invalid`), followed: nil while it
	-- is fine. Assistive technology hears it with the field.
	error: Props_error,
	-- Left out of the accessibility tree with what it holds (`aria-hidden`):
	-- decoration, or what is said elsewhere.
	aria_hidden: Props_aria_hidden,
	-- One Tab stop for the focusable things inside; the arrows along this
	-- axis (and Home, End) move among them, as ARIA's composite widgets
	-- behave. `role = "listbox"` is vertical, "toolbar" and "tablist"
	-- horizontal, without asking.
	focus_group: Props_focus_group,
	-- A press on it (or on what inside takes no input) moves the window; a
	-- double click zooms it. Buttons and inputs inside keep their clicks.
	-- CSS pointer-events: "none" (or false) lets the pointer through to what
	-- is under the node (presses, hover, the wheel, drops, a native view);
	-- descendants with handlers still take it. A state or function follows.
	pointer_events: Props_pointer_events,
	window_drag: Props_window_drag,
	-- The pointer selects the text in it and everything it holds, as on a
	-- web page (CSS `user-select: text`): drag, double/triple press, Shift,
	-- ⌘A / Ctrl+A, ⌘C / Ctrl+C. `false` keeps a part out. Off unless set.
	selectable: Props_selectable,
	-- A selectable node's highlight and selected-text colours (CSS
	-- `::selection`), over the theme's `$selection` and `$selection.text`
	-- tokens; a state or function follows.
	selection_color: Props_selection_color,
	selected_text_color: Props_selected_text_color,
	-- Shown over it while the pointer rests on it (after 300 ms; at once
	-- within a second of another): text in the theme's look, or a node of
	-- the app's own. Above it, centred, unless `tooltip_placement` says.
	tooltip: Props_tooltip,
	tooltip_placement: Props_tooltip_placement,
	-- A hover card the pointer can move into and click in: it stays while
	-- the pointer is over the node or the card.
	tooltip_interactive: Props_tooltip_interactive,
	-- Milliseconds the pointer rests before it shows (300; once one
	-- shows, the next shows at once). Keyboard focus shows it too.
	tooltip_delay: Props_tooltip_delay,
	-- A condition (true disables), or the styles it shows while disabled.
	disabled: Props_disabled,
	hover: Props_hover,
	active: Props_active,
	focus: Props_focus,
	-- Size animation ("height", "width", "size"; `true` is "height") or
	-- position animation ("position"). A `layout_id` node mounting as
	-- another with the same id unmounts (or while it is mounted) moves and
	-- scales from that one's box.
	layout: Props_layout,
	layout_id: Props_layout_id,
	layout_ease: Props_layout_ease,
	-- Milliseconds.
	layout_duration: Props_layout_duration,
	layout_enter: Props_layout_enter,
	layout_clip: Props_layout_clip,
	-- The content shows through the bitmap's alpha: `mask = bitmap`, or
	-- `{ bitmap, w = …, h = …, x = …, y = …, sweep = … }`.
	mask: Props_mask,
	bg_shader: Props_bg_shader,
	-- Up to four vec4s: lists of numbers, or colors (0 to 1).
	shader_params: Props_shader_params,
	-- Events.
	on_click: Props_on_click,
	-- Primary pointer down and up.
	on_press: Props_on_press,
	on_release: Props_on_release,
	on_hover: Props_on_hover,
	on_pointer_move: Props_on_pointer_move,
	-- Return true to consume the key.
	on_key: Props_on_key,
	on_key_up: Props_on_key_up,
	on_focus: Props_on_focus,
	on_blur: Props_on_blur,
	-- Return true to take the gesture from the scroll views around it.
	on_wheel: Props_on_wheel,
	on_pinch: Props_on_pinch,
	on_mount: Props_on_mount,
	-- When layout gives it a new size, and at its first layout.
	on_resize: Props_on_resize,
	-- When any of it comes into view or all of it leaves (inside every
	-- clip and scroll viewport above it), and at its first layout: for
	-- building what only matters on screen.
	on_visible: Props_on_visible,
	-- Drag and drop within the window. Payloads are what `on_drag`
	-- returns: the script's own plain values, so they are `any` here.
	-- A source: `on_drag` returns the payload once the pointer moves 4
	-- points; `drag_preview` builds what follows the pointer.
	on_drag: Props_on_drag,
	drag_preview: Props_drag_preview,
	on_drag_end: Props_on_drag_end,
	-- What the drag carries to other apps as it leaves the window (macOS):
	-- Finder takes files, Mail text, a browser links.
	drag_export: Props_drag_export,
	-- Released outside every window of the app (macOS): a tab torn off its
	-- window. `grab_x`/`grab_y` are where the source was held.
	on_drag_outside: Props_on_drag_outside,
	-- The drag moves its window instead, with no preview (macOS): a
	-- window's only tab. Dropped on another window's destination it drops
	-- there; elsewhere the window stays where it was let go.
	drag_moves_window: Props_drag_moves_window,
	-- A destination: `drop_when` filters payloads (default: all).
	drop_when: Props_drop_when,
	on_drag_over: Props_on_drag_over,
	on_drag_leave: Props_on_drag_leave,
	on_drop: Props_on_drop,
	-- A secondary click (control-click on macOS) shows the returned
	-- `menu_item`s, `submenu`s and separators as a menu at the pointer;
	-- returning nil (or nothing) shows none.
	context_menu: Props_context_menu,
	-- The context menu panel's look: bg, rounded, p, gap, backdrop_blur,
	-- border, shadow, w, …
	context_menu_style: Props_context_menu_style,
	-- Open the context menu under the node, from its left edge (a tab's),
	-- rather than at the pointer.
	context_menu_below: Props_context_menu_below,
	[number]: Child,
	open: State<boolean>,
	-- Its content, built each time it opens (a form starting from what it
	-- edits, its state new), and kept while it closes.
	build: (() -> Node)?,
	label: string?,
	placement: ModalPlacement?,
	autofocus: boolean?,
	-- false: a click outside doesn't close it (a dialog that must be
	-- answered; Escape still does).
	dismiss_on_backdrop: boolean?,
}
-- Where an anchored popover or menu opens: on `side` of its trigger
-- (flipping when that side lacks room), lined up with its start, centre
-- or end, `offset` points away (bottom, start, 4 by default).
export type Placement = {
	side: ("bottom" | "top" | "left" | "right")?,
	align: ("start" | "center" | "end")?,
	offset: number?,
}
export type PopoverProps = {
	-- Size and position.
	w: Props_w,
	h: Props_h,
	size: Props_size,
	min_w: Props_min_w,
	max_w: Props_max_w,
	min_h: Props_min_h,
	max_h: Props_max_h,
	w_full: Props_w_full,
	h_full: Props_h_full,
	aspect: Props_aspect,
	absolute: Props_absolute,
	relative: Props_relative,
	top: Props_top,
	left: Props_left,
	inset_start: Props_inset_start,
	inset_end: Props_inset_end,
	right: Props_right,
	bottom: Props_bottom,
	-- Spacing.
	p: Props_p,
	px: Props_px,
	py: Props_py,
	pt: Props_pt,
	pr: Props_pr,
	pb: Props_pb,
	pl: Props_pl,
	ps: Props_ps,
	pe: Props_pe,
	m: Props_m,
	mx: Props_mx,
	my: Props_my,
	mt: Props_mt,
	mr: Props_mr,
	mb: Props_mb,
	ml: Props_ml,
	ms: Props_ms,
	me: Props_me,
	gap: Props_gap,
	gap_x: Props_gap_x,
	gap_y: Props_gap_y,
	-- Flex and grid.
	direction: Props_direction,
	dir: Props_dir,
	grow: Props_grow,
	shrink: Props_shrink,
	basis: Props_basis,
	flex_wrap: Props_flex_wrap,
	wrap_reverse: Props_wrap_reverse,
	items: Props_items,
	justify: Props_justify,
	self: Props_self,
	content: Props_content,
	grid: Props_grid,
	cols: Props_cols,
	rows: Props_rows,
	col_start: Props_col_start,
	row_start: Props_row_start,
	col_span: Props_col_span,
	row_span: Props_row_span,
	col_span_full: Props_col_span_full,
	row_span_full: Props_row_span_full,
	overflow: Props_overflow,
	overflow_x: Props_overflow_x,
	overflow_y: Props_overflow_y,
	hidden: Props_hidden,
	invisible: Props_invisible,
	-- Paint.
	bg: Props_bg,
	gradient: Props_gradient,
	color: Props_color,
	bg_image: Props_bg_image,
	bg_fit: Props_bg_fit,
	rounded: Props_rounded,
	corners: Props_corners,
	rounded_tl: Props_rounded_tl,
	rounded_tr: Props_rounded_tr,
	rounded_br: Props_rounded_br,
	rounded_bl: Props_rounded_bl,
	border: Props_border,
	border_sides: Props_border_sides,
	border_color: Props_border_color,
	border_over: Props_border_over,
	-- A function may return nil: a solid border.
	border_dash: Props_border_dash,
	outline: Props_outline,
	outline_color: Props_outline_color,
	outline_offset: Props_outline_offset,
	shadow: Props_shadow,
	inset_shadow: Props_inset_shadow,
	opacity: Props_opacity,
	brightness: Props_brightness,
	contrast: Props_contrast,
	saturate: Props_saturate,
	blur: Props_blur,
	backdrop_blur: Props_backdrop_blur,
	fade: Props_fade,
	fade_x: Props_fade_x,
	fade_sides: Props_fade_sides,
	fade_overflow: Props_fade_overflow,
	snap: Props_snap,
	clip_inset: Props_clip_inset,
	layer: Props_layer,
	fit: Props_fit,
	tint: Props_tint,
	cursor: Props_cursor,
	-- Text.
	text_size: Props_text_size,
	weight: Props_weight,
	bold: Props_bold,
	italic: Props_italic,
	align: Props_align,
	font: Props_font,
	wrap: Props_wrap,
	truncate: Props_truncate,
	lines: Props_lines,
	line_height: Props_line_height,
	letter_spacing: Props_letter_spacing,
	-- Tabular figures: every digit the same width.
	tabular: Props_tabular,
	-- Underlined text (inherited, as CSS's `text-decoration`), and how far
	-- below the baseline the line sits.
	underline: Props_underline,
	underline_offset: Props_underline_offset,
	-- Transforms: after layout, about `origin` (fractions of the node).
	x: Props_x,
	y: Props_y,
	scale: Props_scale,
	scale_x: Props_scale_x,
	scale_y: Props_scale_y,
	rotate: Props_rotate,
	skew: Props_skew,
	skew_x: Props_skew_x,
	skew_y: Props_skew_y,
	origin: Props_origin,
	-- The subtree's theme: a theme's name (`"zgx-light"`), or tokens to
	-- override (`{ accent = "#e5484d" }`, with `theme = name` to start from).
	theme: Props_theme,
	-- Identity, for selectors, refs and animations.
	id: Props_id,
	-- Space-separated class names.
	class: Props_class,
	-- Styles chosen from a `ui.variants` definition: `button { tone =
	-- "ghost" }`, or the recipe itself for its defaults. The element's own
	-- properties win over them.
	variants: Props_variants,
	ref: Props_ref,
	focusable: Props_focusable,
	-- What assistive technology takes it for (ARIA's `role`), instead of
	-- what it is: a clickable row as an "option", a row of buttons as a
	-- "toolbar". It keeps what it does.
	role: Props_role,
	-- A section title of this level, 1 to 6 (`h1`…`h6`): screen readers
	-- list headings and move between them.
	heading: Props_heading,
	-- Whether this option, tab or item is the chosen one (`aria-selected`).
	selected: Props_selected,
	-- Whether a switch, checkbox or radio of the app's own is on
	-- (`aria-checked`), and whether what a control opens shows
	-- (`aria-expanded`): a custom disclosure's.
	checked: Props_checked,
	expanded: Props_expanded,
	-- Announce changes to the text in it (`aria-live`): a count, a "Sent"
	-- note, a message arriving.
	live: Props_live,
	-- Read after its name, as a hint (`aria-description`).
	description: Props_description,
	-- What is wrong with its value (`aria-invalid`), followed: nil while it
	-- is fine. Assistive technology hears it with the field.
	error: Props_error,
	-- Left out of the accessibility tree with what it holds (`aria-hidden`):
	-- decoration, or what is said elsewhere.
	aria_hidden: Props_aria_hidden,
	-- One Tab stop for the focusable things inside; the arrows along this
	-- axis (and Home, End) move among them, as ARIA's composite widgets
	-- behave. `role = "listbox"` is vertical, "toolbar" and "tablist"
	-- horizontal, without asking.
	focus_group: Props_focus_group,
	-- A press on it (or on what inside takes no input) moves the window; a
	-- double click zooms it. Buttons and inputs inside keep their clicks.
	-- CSS pointer-events: "none" (or false) lets the pointer through to what
	-- is under the node (presses, hover, the wheel, drops, a native view);
	-- descendants with handlers still take it. A state or function follows.
	pointer_events: Props_pointer_events,
	window_drag: Props_window_drag,
	-- The pointer selects the text in it and everything it holds, as on a
	-- web page (CSS `user-select: text`): drag, double/triple press, Shift,
	-- ⌘A / Ctrl+A, ⌘C / Ctrl+C. `false` keeps a part out. Off unless set.
	selectable: Props_selectable,
	-- A selectable node's highlight and selected-text colours (CSS
	-- `::selection`), over the theme's `$selection` and `$selection.text`
	-- tokens; a state or function follows.
	selection_color: Props_selection_color,
	selected_text_color: Props_selected_text_color,
	-- Shown over it while the pointer rests on it (after 300 ms; at once
	-- within a second of another): text in the theme's look, or a node of
	-- the app's own. Above it, centred, unless `tooltip_placement` says.
	tooltip: Props_tooltip,
	tooltip_placement: Props_tooltip_placement,
	-- A hover card the pointer can move into and click in: it stays while
	-- the pointer is over the node or the card.
	tooltip_interactive: Props_tooltip_interactive,
	-- Milliseconds the pointer rests before it shows (300; once one
	-- shows, the next shows at once). Keyboard focus shows it too.
	tooltip_delay: Props_tooltip_delay,
	-- A condition (true disables), or the styles it shows while disabled.
	disabled: Props_disabled,
	hover: Props_hover,
	active: Props_active,
	focus: Props_focus,
	-- Size animation ("height", "width", "size"; `true` is "height") or
	-- position animation ("position"). A `layout_id` node mounting as
	-- another with the same id unmounts (or while it is mounted) moves and
	-- scales from that one's box.
	layout: Props_layout,
	layout_id: Props_layout_id,
	layout_ease: Props_layout_ease,
	-- Milliseconds.
	layout_duration: Props_layout_duration,
	layout_enter: Props_layout_enter,
	layout_clip: Props_layout_clip,
	-- The content shows through the bitmap's alpha: `mask = bitmap`, or
	-- `{ bitmap, w = …, h = …, x = …, y = …, sweep = … }`.
	mask: Props_mask,
	bg_shader: Props_bg_shader,
	-- Up to four vec4s: lists of numbers, or colors (0 to 1).
	shader_params: Props_shader_params,
	-- Events.
	on_click: Props_on_click,
	-- Primary pointer down and up.
	on_press: Props_on_press,
	on_release: Props_on_release,
	on_hover: Props_on_hover,
	on_pointer_move: Props_on_pointer_move,
	-- Return true to consume the key.
	on_key: Props_on_key,
	on_key_up: Props_on_key_up,
	on_focus: Props_on_focus,
	on_blur: Props_on_blur,
	-- Return true to take the gesture from the scroll views around it.
	on_wheel: Props_on_wheel,
	on_pinch: Props_on_pinch,
	on_mount: Props_on_mount,
	-- When layout gives it a new size, and at its first layout.
	on_resize: Props_on_resize,
	-- When any of it comes into view or all of it leaves (inside every
	-- clip and scroll viewport above it), and at its first layout: for
	-- building what only matters on screen.
	on_visible: Props_on_visible,
	-- Drag and drop within the window. Payloads are what `on_drag`
	-- returns: the script's own plain values, so they are `any` here.
	-- A source: `on_drag` returns the payload once the pointer moves 4
	-- points; `drag_preview` builds what follows the pointer.
	on_drag: Props_on_drag,
	drag_preview: Props_drag_preview,
	on_drag_end: Props_on_drag_end,
	-- What the drag carries to other apps as it leaves the window (macOS):
	-- Finder takes files, Mail text, a browser links.
	drag_export: Props_drag_export,
	-- Released outside every window of the app (macOS): a tab torn off its
	-- window. `grab_x`/`grab_y` are where the source was held.
	on_drag_outside: Props_on_drag_outside,
	-- The drag moves its window instead, with no preview (macOS): a
	-- window's only tab. Dropped on another window's destination it drops
	-- there; elsewhere the window stays where it was let go.
	drag_moves_window: Props_drag_moves_window,
	-- A destination: `drop_when` filters payloads (default: all).
	drop_when: Props_drop_when,
	on_drag_over: Props_on_drag_over,
	on_drag_leave: Props_on_drag_leave,
	on_drop: Props_on_drop,
	-- A secondary click (control-click on macOS) shows the returned
	-- `menu_item`s, `submenu`s and separators as a menu at the pointer;
	-- returning nil (or nothing) shows none.
	context_menu: Props_context_menu,
	-- The context menu panel's look: bg, rounded, p, gap, backdrop_blur,
	-- border, shadow, w, …
	context_menu_style: Props_context_menu_style,
	-- Open the context menu under the node, from its left edge (a tab's),
	-- rather than at the pointer.
	context_menu_below: Props_context_menu_below,
	[number]: Child,
	open: State<boolean>,
	-- Its content, built each time it opens, and kept while it closes.
	build: (() -> Node)?,
	-- The trigger, shown where the popover is written.
	anchor: Node,
	label: string?,
	placement: Placement?,
	-- Focus its first control as it opens (true); false keeps the panel
	-- itself focused (a menu whose search field waits to be clicked).
	autofocus: boolean?,
	-- A menu only: the `id` of its item to open over the trigger, as a
	-- pop-up button's (a select's) opens with the chosen item over it.
	open_over: (() -> string?)?,
	-- A popover only: an arrow at its trigger, reaching this many points
	-- from the panel's edge (AppKit's popovers have one).
	arrow: number?,
	-- At least as wide as its trigger, as a select's list is.
	match_anchor_width: boolean?,
	-- A popover only: false keeps it open on a click outside.
	dismiss_on_backdrop: boolean?,
}
-- `ui.menu_item { "Copy", on_click = … }`: the label first.
-- The label (`{ "Copy" }`), or `label` with content of its own after it
-- (an icon, the label, a shortcut).
export type MenuItemProps = {
	-- Size and position.
	w: Props_w,
	h: Props_h,
	size: Props_size,
	min_w: Props_min_w,
	max_w: Props_max_w,
	min_h: Props_min_h,
	max_h: Props_max_h,
	w_full: Props_w_full,
	h_full: Props_h_full,
	aspect: Props_aspect,
	absolute: Props_absolute,
	relative: Props_relative,
	top: Props_top,
	left: Props_left,
	inset_start: Props_inset_start,
	inset_end: Props_inset_end,
	right: Props_right,
	bottom: Props_bottom,
	-- Spacing.
	p: Props_p,
	px: Props_px,
	py: Props_py,
	pt: Props_pt,
	pr: Props_pr,
	pb: Props_pb,
	pl: Props_pl,
	ps: Props_ps,
	pe: Props_pe,
	m: Props_m,
	mx: Props_mx,
	my: Props_my,
	mt: Props_mt,
	mr: Props_mr,
	mb: Props_mb,
	ml: Props_ml,
	ms: Props_ms,
	me: Props_me,
	gap: Props_gap,
	gap_x: Props_gap_x,
	gap_y: Props_gap_y,
	-- Flex and grid.
	direction: Props_direction,
	dir: Props_dir,
	grow: Props_grow,
	shrink: Props_shrink,
	basis: Props_basis,
	flex_wrap: Props_flex_wrap,
	wrap_reverse: Props_wrap_reverse,
	items: Props_items,
	justify: Props_justify,
	self: Props_self,
	content: Props_content,
	grid: Props_grid,
	cols: Props_cols,
	rows: Props_rows,
	col_start: Props_col_start,
	row_start: Props_row_start,
	col_span: Props_col_span,
	row_span: Props_row_span,
	col_span_full: Props_col_span_full,
	row_span_full: Props_row_span_full,
	overflow: Props_overflow,
	overflow_x: Props_overflow_x,
	overflow_y: Props_overflow_y,
	hidden: Props_hidden,
	invisible: Props_invisible,
	-- Paint.
	bg: Props_bg,
	gradient: Props_gradient,
	color: Props_color,
	bg_image: Props_bg_image,
	bg_fit: Props_bg_fit,
	rounded: Props_rounded,
	corners: Props_corners,
	rounded_tl: Props_rounded_tl,
	rounded_tr: Props_rounded_tr,
	rounded_br: Props_rounded_br,
	rounded_bl: Props_rounded_bl,
	border: Props_border,
	border_sides: Props_border_sides,
	border_color: Props_border_color,
	border_over: Props_border_over,
	-- A function may return nil: a solid border.
	border_dash: Props_border_dash,
	outline: Props_outline,
	outline_color: Props_outline_color,
	outline_offset: Props_outline_offset,
	shadow: Props_shadow,
	inset_shadow: Props_inset_shadow,
	opacity: Props_opacity,
	brightness: Props_brightness,
	contrast: Props_contrast,
	saturate: Props_saturate,
	blur: Props_blur,
	backdrop_blur: Props_backdrop_blur,
	fade: Props_fade,
	fade_x: Props_fade_x,
	fade_sides: Props_fade_sides,
	fade_overflow: Props_fade_overflow,
	snap: Props_snap,
	clip_inset: Props_clip_inset,
	layer: Props_layer,
	fit: Props_fit,
	tint: Props_tint,
	cursor: Props_cursor,
	-- Text.
	text_size: Props_text_size,
	weight: Props_weight,
	bold: Props_bold,
	italic: Props_italic,
	align: Props_align,
	font: Props_font,
	wrap: Props_wrap,
	truncate: Props_truncate,
	lines: Props_lines,
	line_height: Props_line_height,
	letter_spacing: Props_letter_spacing,
	-- Tabular figures: every digit the same width.
	tabular: Props_tabular,
	-- Underlined text (inherited, as CSS's `text-decoration`), and how far
	-- below the baseline the line sits.
	underline: Props_underline,
	underline_offset: Props_underline_offset,
	-- Transforms: after layout, about `origin` (fractions of the node).
	x: Props_x,
	y: Props_y,
	scale: Props_scale,
	scale_x: Props_scale_x,
	scale_y: Props_scale_y,
	rotate: Props_rotate,
	skew: Props_skew,
	skew_x: Props_skew_x,
	skew_y: Props_skew_y,
	origin: Props_origin,
	-- The subtree's theme: a theme's name (`"zgx-light"`), or tokens to
	-- override (`{ accent = "#e5484d" }`, with `theme = name` to start from).
	theme: Props_theme,
	-- Identity, for selectors, refs and animations.
	id: Props_id,
	-- Space-separated class names.
	class: Props_class,
	-- Styles chosen from a `ui.variants` definition: `button { tone =
	-- "ghost" }`, or the recipe itself for its defaults. The element's own
	-- properties win over them.
	variants: Props_variants,
	ref: Props_ref,
	focusable: Props_focusable,
	-- What assistive technology takes it for (ARIA's `role`), instead of
	-- what it is: a clickable row as an "option", a row of buttons as a
	-- "toolbar". It keeps what it does.
	role: Props_role,
	-- A section title of this level, 1 to 6 (`h1`…`h6`): screen readers
	-- list headings and move between them.
	heading: Props_heading,
	-- Whether this option, tab or item is the chosen one (`aria-selected`).
	selected: Props_selected,
	expanded: Props_expanded,
	-- Announce changes to the text in it (`aria-live`): a count, a "Sent"
	-- note, a message arriving.
	live: Props_live,
	-- Read after its name, as a hint (`aria-description`).
	description: Props_description,
	-- What is wrong with its value (`aria-invalid`), followed: nil while it
	-- is fine. Assistive technology hears it with the field.
	error: Props_error,
	-- Left out of the accessibility tree with what it holds (`aria-hidden`):
	-- decoration, or what is said elsewhere.
	aria_hidden: Props_aria_hidden,
	-- One Tab stop for the focusable things inside; the arrows along this
	-- axis (and Home, End) move among them, as ARIA's composite widgets
	-- behave. `role = "listbox"` is vertical, "toolbar" and "tablist"
	-- horizontal, without asking.
	focus_group: Props_focus_group,
	-- A press on it (or on what inside takes no input) moves the window; a
	-- double click zooms it. Buttons and inputs inside keep their clicks.
	-- CSS pointer-events: "none" (or false) lets the pointer through to what
	-- is under the node (presses, hover, the wheel, drops, a native view);
	-- descendants with handlers still take it. A state or function follows.
	pointer_events: Props_pointer_events,
	window_drag: Props_window_drag,
	-- The pointer selects the text in it and everything it holds, as on a
	-- web page (CSS `user-select: text`): drag, double/triple press, Shift,
	-- ⌘A / Ctrl+A, ⌘C / Ctrl+C. `false` keeps a part out. Off unless set.
	selectable: Props_selectable,
	-- A selectable node's highlight and selected-text colours (CSS
	-- `::selection`), over the theme's `$selection` and `$selection.text`
	-- tokens; a state or function follows.
	selection_color: Props_selection_color,
	selected_text_color: Props_selected_text_color,
	-- Shown over it while the pointer rests on it (after 300 ms; at once
	-- within a second of another): text in the theme's look, or a node of
	-- the app's own. Above it, centred, unless `tooltip_placement` says.
	tooltip: Props_tooltip,
	tooltip_placement: Props_tooltip_placement,
	-- A hover card the pointer can move into and click in: it stays while
	-- the pointer is over the node or the card.
	tooltip_interactive: Props_tooltip_interactive,
	-- Milliseconds the pointer rests before it shows (300; once one
	-- shows, the next shows at once). Keyboard focus shows it too.
	tooltip_delay: Props_tooltip_delay,
	-- A condition (true disables), or the styles it shows while disabled.
	disabled: Props_disabled,
	hover: Props_hover,
	active: Props_active,
	focus: Props_focus,
	-- Size animation ("height", "width", "size"; `true` is "height") or
	-- position animation ("position"). A `layout_id` node mounting as
	-- another with the same id unmounts (or while it is mounted) moves and
	-- scales from that one's box.
	layout: Props_layout,
	layout_id: Props_layout_id,
	layout_ease: Props_layout_ease,
	-- Milliseconds.
	layout_duration: Props_layout_duration,
	layout_enter: Props_layout_enter,
	layout_clip: Props_layout_clip,
	-- The content shows through the bitmap's alpha: `mask = bitmap`, or
	-- `{ bitmap, w = …, h = …, x = …, y = …, sweep = … }`.
	mask: Props_mask,
	bg_shader: Props_bg_shader,
	-- Up to four vec4s: lists of numbers, or colors (0 to 1).
	shader_params: Props_shader_params,
	-- Events.
	on_click: Props_on_click,
	-- Primary pointer down and up.
	on_press: Props_on_press,
	on_release: Props_on_release,
	on_hover: Props_on_hover,
	on_pointer_move: Props_on_pointer_move,
	-- Return true to consume the key.
	on_key: Props_on_key,
	on_key_up: Props_on_key_up,
	on_focus: Props_on_focus,
	on_blur: Props_on_blur,
	-- Return true to take the gesture from the scroll views around it.
	on_wheel: Props_on_wheel,
	on_pinch: Props_on_pinch,
	on_mount: Props_on_mount,
	-- When layout gives it a new size, and at its first layout.
	on_resize: Props_on_resize,
	-- When any of it comes into view or all of it leaves (inside every
	-- clip and scroll viewport above it), and at its first layout: for
	-- building what only matters on screen.
	on_visible: Props_on_visible,
	-- Drag and drop within the window. Payloads are what `on_drag`
	-- returns: the script's own plain values, so they are `any` here.
	-- A source: `on_drag` returns the payload once the pointer moves 4
	-- points; `drag_preview` builds what follows the pointer.
	on_drag: Props_on_drag,
	drag_preview: Props_drag_preview,
	on_drag_end: Props_on_drag_end,
	-- What the drag carries to other apps as it leaves the window (macOS):
	-- Finder takes files, Mail text, a browser links.
	drag_export: Props_drag_export,
	-- Released outside every window of the app (macOS): a tab torn off its
	-- window. `grab_x`/`grab_y` are where the source was held.
	on_drag_outside: Props_on_drag_outside,
	-- The drag moves its window instead, with no preview (macOS): a
	-- window's only tab. Dropped on another window's destination it drops
	-- there; elsewhere the window stays where it was let go.
	drag_moves_window: Props_drag_moves_window,
	-- A destination: `drop_when` filters payloads (default: all).
	drop_when: Props_drop_when,
	on_drag_over: Props_on_drag_over,
	on_drag_leave: Props_on_drag_leave,
	on_drop: Props_on_drop,
	-- A secondary click (control-click on macOS) shows the returned
	-- `menu_item`s, `submenu`s and separators as a menu at the pointer;
	-- returning nil (or nothing) shows none.
	context_menu: Props_context_menu,
	-- The context menu panel's look: bg, rounded, p, gap, backdrop_blur,
	-- border, shadow, w, …
	context_menu_style: Props_context_menu_style,
	-- Open the context menu under the node, from its left edge (a tab's),
	-- rather than at the pointer.
	context_menu_below: Props_context_menu_below,
	[number]: Child,
	label: string?,
	-- Chosen, it leaves the menu open (a toggle, a pin).
	keep_open: boolean?,
	-- A check in its gutter while true (a toggle item); `radio = true`: a
	-- dot, one of a group (a radio item). Told to assistive technology.
	checked: (boolean | Readable<boolean> | (() -> boolean))?,
	radio: boolean?,
	-- The keys that do it, shown at its end: keys ("cmd+c": ⌘C on macOS,
	-- Ctrl+C elsewhere), or text shown as it is ("⌘C").
	shortcut: string?,
}
export type SubmenuProps = {
	-- Size and position.
	w: Props_w,
	h: Props_h,
	size: Props_size,
	min_w: Props_min_w,
	max_w: Props_max_w,
	min_h: Props_min_h,
	max_h: Props_max_h,
	w_full: Props_w_full,
	h_full: Props_h_full,
	aspect: Props_aspect,
	absolute: Props_absolute,
	relative: Props_relative,
	top: Props_top,
	left: Props_left,
	inset_start: Props_inset_start,
	inset_end: Props_inset_end,
	right: Props_right,
	bottom: Props_bottom,
	-- Spacing.
	p: Props_p,
	px: Props_px,
	py: Props_py,
	pt: Props_pt,
	pr: Props_pr,
	pb: Props_pb,
	pl: Props_pl,
	ps: Props_ps,
	pe: Props_pe,
	m: Props_m,
	mx: Props_mx,
	my: Props_my,
	mt: Props_mt,
	mr: Props_mr,
	mb: Props_mb,
	ml: Props_ml,
	ms: Props_ms,
	me: Props_me,
	gap: Props_gap,
	gap_x: Props_gap_x,
	gap_y: Props_gap_y,
	-- Flex and grid.
	direction: Props_direction,
	dir: Props_dir,
	grow: Props_grow,
	shrink: Props_shrink,
	basis: Props_basis,
	flex_wrap: Props_flex_wrap,
	wrap_reverse: Props_wrap_reverse,
	items: Props_items,
	justify: Props_justify,
	self: Props_self,
	content: Props_content,
	grid: Props_grid,
	cols: Props_cols,
	rows: Props_rows,
	col_start: Props_col_start,
	row_start: Props_row_start,
	col_span: Props_col_span,
	row_span: Props_row_span,
	col_span_full: Props_col_span_full,
	row_span_full: Props_row_span_full,
	overflow: Props_overflow,
	overflow_x: Props_overflow_x,
	overflow_y: Props_overflow_y,
	hidden: Props_hidden,
	invisible: Props_invisible,
	-- Paint.
	bg: Props_bg,
	gradient: Props_gradient,
	color: Props_color,
	bg_image: Props_bg_image,
	bg_fit: Props_bg_fit,
	rounded: Props_rounded,
	corners: Props_corners,
	rounded_tl: Props_rounded_tl,
	rounded_tr: Props_rounded_tr,
	rounded_br: Props_rounded_br,
	rounded_bl: Props_rounded_bl,
	border: Props_border,
	border_sides: Props_border_sides,
	border_color: Props_border_color,
	border_over: Props_border_over,
	-- A function may return nil: a solid border.
	border_dash: Props_border_dash,
	outline: Props_outline,
	outline_color: Props_outline_color,
	outline_offset: Props_outline_offset,
	shadow: Props_shadow,
	inset_shadow: Props_inset_shadow,
	opacity: Props_opacity,
	brightness: Props_brightness,
	contrast: Props_contrast,
	saturate: Props_saturate,
	blur: Props_blur,
	backdrop_blur: Props_backdrop_blur,
	fade: Props_fade,
	fade_x: Props_fade_x,
	fade_sides: Props_fade_sides,
	fade_overflow: Props_fade_overflow,
	snap: Props_snap,
	clip_inset: Props_clip_inset,
	layer: Props_layer,
	fit: Props_fit,
	tint: Props_tint,
	cursor: Props_cursor,
	-- Text.
	text_size: Props_text_size,
	weight: Props_weight,
	bold: Props_bold,
	italic: Props_italic,
	align: Props_align,
	font: Props_font,
	wrap: Props_wrap,
	truncate: Props_truncate,
	lines: Props_lines,
	line_height: Props_line_height,
	letter_spacing: Props_letter_spacing,
	-- Tabular figures: every digit the same width.
	tabular: Props_tabular,
	-- Underlined text (inherited, as CSS's `text-decoration`), and how far
	-- below the baseline the line sits.
	underline: Props_underline,
	underline_offset: Props_underline_offset,
	-- Transforms: after layout, about `origin` (fractions of the node).
	x: Props_x,
	y: Props_y,
	scale: Props_scale,
	scale_x: Props_scale_x,
	scale_y: Props_scale_y,
	rotate: Props_rotate,
	skew: Props_skew,
	skew_x: Props_skew_x,
	skew_y: Props_skew_y,
	origin: Props_origin,
	-- The subtree's theme: a theme's name (`"zgx-light"`), or tokens to
	-- override (`{ accent = "#e5484d" }`, with `theme = name` to start from).
	theme: Props_theme,
	-- Identity, for selectors, refs and animations.
	id: Props_id,
	-- Space-separated class names.
	class: Props_class,
	-- Styles chosen from a `ui.variants` definition: `button { tone =
	-- "ghost" }`, or the recipe itself for its defaults. The element's own
	-- properties win over them.
	variants: Props_variants,
	ref: Props_ref,
	focusable: Props_focusable,
	-- What assistive technology takes it for (ARIA's `role`), instead of
	-- what it is: a clickable row as an "option", a row of buttons as a
	-- "toolbar". It keeps what it does.
	role: Props_role,
	-- A section title of this level, 1 to 6 (`h1`…`h6`): screen readers
	-- list headings and move between them.
	heading: Props_heading,
	-- Whether this option, tab or item is the chosen one (`aria-selected`).
	selected: Props_selected,
	-- Whether a switch, checkbox or radio of the app's own is on
	-- (`aria-checked`), and whether what a control opens shows
	-- (`aria-expanded`): a custom disclosure's.
	checked: Props_checked,
	expanded: Props_expanded,
	-- Announce changes to the text in it (`aria-live`): a count, a "Sent"
	-- note, a message arriving.
	live: Props_live,
	-- Read after its name, as a hint (`aria-description`).
	description: Props_description,
	-- What is wrong with its value (`aria-invalid`), followed: nil while it
	-- is fine. Assistive technology hears it with the field.
	error: Props_error,
	-- Left out of the accessibility tree with what it holds (`aria-hidden`):
	-- decoration, or what is said elsewhere.
	aria_hidden: Props_aria_hidden,
	-- One Tab stop for the focusable things inside; the arrows along this
	-- axis (and Home, End) move among them, as ARIA's composite widgets
	-- behave. `role = "listbox"` is vertical, "toolbar" and "tablist"
	-- horizontal, without asking.
	focus_group: Props_focus_group,
	-- A press on it (or on what inside takes no input) moves the window; a
	-- double click zooms it. Buttons and inputs inside keep their clicks.
	-- CSS pointer-events: "none" (or false) lets the pointer through to what
	-- is under the node (presses, hover, the wheel, drops, a native view);
	-- descendants with handlers still take it. A state or function follows.
	pointer_events: Props_pointer_events,
	window_drag: Props_window_drag,
	-- The pointer selects the text in it and everything it holds, as on a
	-- web page (CSS `user-select: text`): drag, double/triple press, Shift,
	-- ⌘A / Ctrl+A, ⌘C / Ctrl+C. `false` keeps a part out. Off unless set.
	selectable: Props_selectable,
	-- A selectable node's highlight and selected-text colours (CSS
	-- `::selection`), over the theme's `$selection` and `$selection.text`
	-- tokens; a state or function follows.
	selection_color: Props_selection_color,
	selected_text_color: Props_selected_text_color,
	-- Shown over it while the pointer rests on it (after 300 ms; at once
	-- within a second of another): text in the theme's look, or a node of
	-- the app's own. Above it, centred, unless `tooltip_placement` says.
	tooltip: Props_tooltip,
	tooltip_placement: Props_tooltip_placement,
	-- A hover card the pointer can move into and click in: it stays while
	-- the pointer is over the node or the card.
	tooltip_interactive: Props_tooltip_interactive,
	-- Milliseconds the pointer rests before it shows (300; once one
	-- shows, the next shows at once). Keyboard focus shows it too.
	tooltip_delay: Props_tooltip_delay,
	-- A condition (true disables), or the styles it shows while disabled.
	disabled: Props_disabled,
	hover: Props_hover,
	active: Props_active,
	focus: Props_focus,
	-- Size animation ("height", "width", "size"; `true` is "height") or
	-- position animation ("position"). A `layout_id` node mounting as
	-- another with the same id unmounts (or while it is mounted) moves and
	-- scales from that one's box.
	layout: Props_layout,
	layout_id: Props_layout_id,
	layout_ease: Props_layout_ease,
	-- Milliseconds.
	layout_duration: Props_layout_duration,
	layout_enter: Props_layout_enter,
	layout_clip: Props_layout_clip,
	-- The content shows through the bitmap's alpha: `mask = bitmap`, or
	-- `{ bitmap, w = …, h = …, x = …, y = …, sweep = … }`.
	mask: Props_mask,
	bg_shader: Props_bg_shader,
	-- Up to four vec4s: lists of numbers, or colors (0 to 1).
	shader_params: Props_shader_params,
	-- Events.
	on_click: Props_on_click,
	-- Primary pointer down and up.
	on_press: Props_on_press,
	on_release: Props_on_release,
	on_hover: Props_on_hover,
	on_pointer_move: Props_on_pointer_move,
	-- Return true to consume the key.
	on_key: Props_on_key,
	on_key_up: Props_on_key_up,
	on_focus: Props_on_focus,
	on_blur: Props_on_blur,
	-- Return true to take the gesture from the scroll views around it.
	on_wheel: Props_on_wheel,
	on_pinch: Props_on_pinch,
	on_mount: Props_on_mount,
	-- When layout gives it a new size, and at its first layout.
	on_resize: Props_on_resize,
	-- When any of it comes into view or all of it leaves (inside every
	-- clip and scroll viewport above it), and at its first layout: for
	-- building what only matters on screen.
	on_visible: Props_on_visible,
	-- Drag and drop within the window. Payloads are what `on_drag`
	-- returns: the script's own plain values, so they are `any` here.
	-- A source: `on_drag` returns the payload once the pointer moves 4
	-- points; `drag_preview` builds what follows the pointer.
	on_drag: Props_on_drag,
	drag_preview: Props_drag_preview,
	on_drag_end: Props_on_drag_end,
	-- What the drag carries to other apps as it leaves the window (macOS):
	-- Finder takes files, Mail text, a browser links.
	drag_export: Props_drag_export,
	-- Released outside every window of the app (macOS): a tab torn off its
	-- window. `grab_x`/`grab_y` are where the source was held.
	on_drag_outside: Props_on_drag_outside,
	-- The drag moves its window instead, with no preview (macOS): a
	-- window's only tab. Dropped on another window's destination it drops
	-- there; elsewhere the window stays where it was let go.
	drag_moves_window: Props_drag_moves_window,
	-- A destination: `drop_when` filters payloads (default: all).
	drop_when: Props_drop_when,
	on_drag_over: Props_on_drag_over,
	on_drag_leave: Props_on_drag_leave,
	on_drop: Props_on_drop,
	-- A secondary click (control-click on macOS) shows the returned
	-- `menu_item`s, `submenu`s and separators as a menu at the pointer;
	-- returning nil (or nothing) shows none.
	context_menu: Props_context_menu,
	-- The context menu panel's look: bg, rounded, p, gap, backdrop_blur,
	-- border, shadow, w, …
	context_menu_style: Props_context_menu_style,
	-- Open the context menu under the node, from its left edge (a tab's),
	-- rather than at the pointer.
	context_menu_below: Props_context_menu_below,
	[number]: Child,
	open: State<boolean>,
	label: string?,
	-- Its trigger row, drawn by the app (a `ui.menu_item` with an icon, the
	-- label, a value, a chevron); the label and a "›" otherwise. It opens
	-- when the pointer moves onto it.
	trigger: Node?,
}
export type WebViewProps = {
	-- Size and position.
	w: Props_w,
	h: Props_h,
	size: Props_size,
	min_w: Props_min_w,
	max_w: Props_max_w,
	min_h: Props_min_h,
	max_h: Props_max_h,
	w_full: Props_w_full,
	h_full: Props_h_full,
	aspect: Props_aspect,
	absolute: Props_absolute,
	relative: Props_relative,
	top: Props_top,
	left: Props_left,
	inset_start: Props_inset_start,
	inset_end: Props_inset_end,
	right: Props_right,
	bottom: Props_bottom,
	-- Spacing.
	p: Props_p,
	px: Props_px,
	py: Props_py,
	pt: Props_pt,
	pr: Props_pr,
	pb: Props_pb,
	pl: Props_pl,
	ps: Props_ps,
	pe: Props_pe,
	m: Props_m,
	mx: Props_mx,
	my: Props_my,
	mt: Props_mt,
	mr: Props_mr,
	mb: Props_mb,
	ml: Props_ml,
	ms: Props_ms,
	me: Props_me,
	gap: Props_gap,
	gap_x: Props_gap_x,
	gap_y: Props_gap_y,
	-- Flex and grid.
	direction: Props_direction,
	dir: Props_dir,
	grow: Props_grow,
	shrink: Props_shrink,
	basis: Props_basis,
	flex_wrap: Props_flex_wrap,
	wrap_reverse: Props_wrap_reverse,
	items: Props_items,
	justify: Props_justify,
	self: Props_self,
	content: Props_content,
	grid: Props_grid,
	cols: Props_cols,
	rows: Props_rows,
	col_start: Props_col_start,
	row_start: Props_row_start,
	col_span: Props_col_span,
	row_span: Props_row_span,
	col_span_full: Props_col_span_full,
	row_span_full: Props_row_span_full,
	overflow: Props_overflow,
	overflow_x: Props_overflow_x,
	overflow_y: Props_overflow_y,
	hidden: Props_hidden,
	invisible: Props_invisible,
	-- Paint.
	bg: Props_bg,
	gradient: Props_gradient,
	color: Props_color,
	bg_image: Props_bg_image,
	bg_fit: Props_bg_fit,
	rounded: Props_rounded,
	corners: Props_corners,
	rounded_tl: Props_rounded_tl,
	rounded_tr: Props_rounded_tr,
	rounded_br: Props_rounded_br,
	rounded_bl: Props_rounded_bl,
	border: Props_border,
	border_sides: Props_border_sides,
	border_color: Props_border_color,
	border_over: Props_border_over,
	-- A function may return nil: a solid border.
	border_dash: Props_border_dash,
	outline: Props_outline,
	outline_color: Props_outline_color,
	outline_offset: Props_outline_offset,
	shadow: Props_shadow,
	inset_shadow: Props_inset_shadow,
	opacity: Props_opacity,
	brightness: Props_brightness,
	contrast: Props_contrast,
	saturate: Props_saturate,
	blur: Props_blur,
	backdrop_blur: Props_backdrop_blur,
	fade: Props_fade,
	fade_x: Props_fade_x,
	fade_sides: Props_fade_sides,
	fade_overflow: Props_fade_overflow,
	snap: Props_snap,
	clip_inset: Props_clip_inset,
	layer: Props_layer,
	fit: Props_fit,
	tint: Props_tint,
	cursor: Props_cursor,
	-- Text.
	text_size: Props_text_size,
	weight: Props_weight,
	bold: Props_bold,
	italic: Props_italic,
	align: Props_align,
	font: Props_font,
	wrap: Props_wrap,
	truncate: Props_truncate,
	lines: Props_lines,
	line_height: Props_line_height,
	letter_spacing: Props_letter_spacing,
	-- Tabular figures: every digit the same width.
	tabular: Props_tabular,
	-- Underlined text (inherited, as CSS's `text-decoration`), and how far
	-- below the baseline the line sits.
	underline: Props_underline,
	underline_offset: Props_underline_offset,
	-- Transforms: after layout, about `origin` (fractions of the node).
	x: Props_x,
	y: Props_y,
	scale: Props_scale,
	scale_x: Props_scale_x,
	scale_y: Props_scale_y,
	rotate: Props_rotate,
	skew: Props_skew,
	skew_x: Props_skew_x,
	skew_y: Props_skew_y,
	origin: Props_origin,
	-- The subtree's theme: a theme's name (`"zgx-light"`), or tokens to
	-- override (`{ accent = "#e5484d" }`, with `theme = name` to start from).
	theme: Props_theme,
	-- Identity, for selectors, refs and animations.
	id: Props_id,
	-- Space-separated class names.
	class: Props_class,
	-- Styles chosen from a `ui.variants` definition: `button { tone =
	-- "ghost" }`, or the recipe itself for its defaults. The element's own
	-- properties win over them.
	variants: Props_variants,
	ref: Props_ref,
	focusable: Props_focusable,
	-- What assistive technology takes it for (ARIA's `role`), instead of
	-- what it is: a clickable row as an "option", a row of buttons as a
	-- "toolbar". It keeps what it does.
	role: Props_role,
	-- A section title of this level, 1 to 6 (`h1`…`h6`): screen readers
	-- list headings and move between them.
	heading: Props_heading,
	-- Whether this option, tab or item is the chosen one (`aria-selected`).
	selected: Props_selected,
	-- Whether a switch, checkbox or radio of the app's own is on
	-- (`aria-checked`), and whether what a control opens shows
	-- (`aria-expanded`): a custom disclosure's.
	checked: Props_checked,
	expanded: Props_expanded,
	-- Announce changes to the text in it (`aria-live`): a count, a "Sent"
	-- note, a message arriving.
	live: Props_live,
	-- Read after its name, as a hint (`aria-description`).
	description: Props_description,
	-- What is wrong with its value (`aria-invalid`), followed: nil while it
	-- is fine. Assistive technology hears it with the field.
	error: Props_error,
	-- Left out of the accessibility tree with what it holds (`aria-hidden`):
	-- decoration, or what is said elsewhere.
	aria_hidden: Props_aria_hidden,
	-- One Tab stop for the focusable things inside; the arrows along this
	-- axis (and Home, End) move among them, as ARIA's composite widgets
	-- behave. `role = "listbox"` is vertical, "toolbar" and "tablist"
	-- horizontal, without asking.
	focus_group: Props_focus_group,
	-- A press on it (or on what inside takes no input) moves the window; a
	-- double click zooms it. Buttons and inputs inside keep their clicks.
	-- CSS pointer-events: "none" (or false) lets the pointer through to what
	-- is under the node (presses, hover, the wheel, drops, a native view);
	-- descendants with handlers still take it. A state or function follows.
	pointer_events: Props_pointer_events,
	window_drag: Props_window_drag,
	-- The pointer selects the text in it and everything it holds, as on a
	-- web page (CSS `user-select: text`): drag, double/triple press, Shift,
	-- ⌘A / Ctrl+A, ⌘C / Ctrl+C. `false` keeps a part out. Off unless set.
	selectable: Props_selectable,
	-- A selectable node's highlight and selected-text colours (CSS
	-- `::selection`), over the theme's `$selection` and `$selection.text`
	-- tokens; a state or function follows.
	selection_color: Props_selection_color,
	selected_text_color: Props_selected_text_color,
	-- Shown over it while the pointer rests on it (after 300 ms; at once
	-- within a second of another): text in the theme's look, or a node of
	-- the app's own. Above it, centred, unless `tooltip_placement` says.
	tooltip: Props_tooltip,
	tooltip_placement: Props_tooltip_placement,
	-- A hover card the pointer can move into and click in: it stays while
	-- the pointer is over the node or the card.
	tooltip_interactive: Props_tooltip_interactive,
	-- Milliseconds the pointer rests before it shows (300; once one
	-- shows, the next shows at once). Keyboard focus shows it too.
	tooltip_delay: Props_tooltip_delay,
	-- A condition (true disables), or the styles it shows while disabled.
	disabled: Props_disabled,
	hover: Props_hover,
	active: Props_active,
	focus: Props_focus,
	-- Size animation ("height", "width", "size"; `true` is "height") or
	-- position animation ("position"). A `layout_id` node mounting as
	-- another with the same id unmounts (or while it is mounted) moves and
	-- scales from that one's box.
	layout: Props_layout,
	layout_id: Props_layout_id,
	layout_ease: Props_layout_ease,
	-- Milliseconds.
	layout_duration: Props_layout_duration,
	layout_enter: Props_layout_enter,
	layout_clip: Props_layout_clip,
	-- The content shows through the bitmap's alpha: `mask = bitmap`, or
	-- `{ bitmap, w = …, h = …, x = …, y = …, sweep = … }`.
	mask: Props_mask,
	bg_shader: Props_bg_shader,
	-- Up to four vec4s: lists of numbers, or colors (0 to 1).
	shader_params: Props_shader_params,
	-- Events.
	on_click: Props_on_click,
	-- Primary pointer down and up.
	on_press: Props_on_press,
	on_release: Props_on_release,
	on_hover: Props_on_hover,
	on_pointer_move: Props_on_pointer_move,
	-- Return true to consume the key.
	on_key: Props_on_key,
	on_key_up: Props_on_key_up,
	on_focus: Props_on_focus,
	on_blur: Props_on_blur,
	-- Return true to take the gesture from the scroll views around it.
	on_wheel: Props_on_wheel,
	on_pinch: Props_on_pinch,
	on_mount: Props_on_mount,
	-- When layout gives it a new size, and at its first layout.
	on_resize: Props_on_resize,
	-- When any of it comes into view or all of it leaves (inside every
	-- clip and scroll viewport above it), and at its first layout: for
	-- building what only matters on screen.
	on_visible: Props_on_visible,
	-- Drag and drop within the window. Payloads are what `on_drag`
	-- returns: the script's own plain values, so they are `any` here.
	-- A source: `on_drag` returns the payload once the pointer moves 4
	-- points; `drag_preview` builds what follows the pointer.
	on_drag: Props_on_drag,
	drag_preview: Props_drag_preview,
	on_drag_end: Props_on_drag_end,
	-- What the drag carries to other apps as it leaves the window (macOS):
	-- Finder takes files, Mail text, a browser links.
	drag_export: Props_drag_export,
	-- Released outside every window of the app (macOS): a tab torn off its
	-- window. `grab_x`/`grab_y` are where the source was held.
	on_drag_outside: Props_on_drag_outside,
	-- The drag moves its window instead, with no preview (macOS): a
	-- window's only tab. Dropped on another window's destination it drops
	-- there; elsewhere the window stays where it was let go.
	drag_moves_window: Props_drag_moves_window,
	-- A destination: `drop_when` filters payloads (default: all).
	drop_when: Props_drop_when,
	on_drag_over: Props_on_drag_over,
	on_drag_leave: Props_on_drag_leave,
	on_drop: Props_on_drop,
	-- A secondary click (control-click on macOS) shows the returned
	-- `menu_item`s, `submenu`s and separators as a menu at the pointer;
	-- returning nil (or nothing) shows none.
	context_menu: Props_context_menu,
	-- The context menu panel's look: bg, rounded, p, gap, backdrop_blur,
	-- border, shadow, w, …
	context_menu_style: Props_context_menu_style,
	-- Open the context menu under the node, from its left edge (a tab's),
	-- rather than at the pointer.
	context_menu_below: Props_context_menu_below,
	transparent: boolean?,
	-- "default" (shared cookies and storage), "ephemeral", or a name.
	profile: string?,
	inspectable: boolean?,
	user_agent: string?,
	-- `window.zgx.postMessage(value)` from the page reaches on_message.
	messages: boolean?,
	new_windows: boolean?,
	[number]: never,
	-- A `ui.web()` controller, so the page outlives the node; the options
	-- above make one otherwise.
	web: Web?,
	-- The page: a URL (followed when it changes), or `html` with `base_url`.
	src: Reactive<string>?,
	html: string?,
	base_url: string?,
	on_navigate: ((e: WebEvent) -> ...unknown)?,
	on_load: ((url: string) -> ...unknown)?,
	on_title: ((title: string) -> ...unknown)?,
	-- What the page posted, as Luau data: JSON from the page, so `any`.
	-- `origin` is the sending frame's (any page, any iframe, can post):
	-- check it before acting on a message.
	on_message: ((value: any, origin: string, main_frame: boolean) -> ...unknown)?,
	on_error: ((url: string, err: string) -> ...unknown)?,
	on_new_window: ((url: string) -> ...unknown)?,
}
export type TerminalProps = {
	-- Size and position.
	w: Props_w,
	h: Props_h,
	size: Props_size,
	min_w: Props_min_w,
	max_w: Props_max_w,
	min_h: Props_min_h,
	max_h: Props_max_h,
	w_full: Props_w_full,
	h_full: Props_h_full,
	aspect: Props_aspect,
	absolute: Props_absolute,
	relative: Props_relative,
	top: Props_top,
	left: Props_left,
	inset_start: Props_inset_start,
	inset_end: Props_inset_end,
	right: Props_right,
	bottom: Props_bottom,
	-- Spacing.
	p: Props_p,
	px: Props_px,
	py: Props_py,
	pt: Props_pt,
	pr: Props_pr,
	pb: Props_pb,
	pl: Props_pl,
	ps: Props_ps,
	pe: Props_pe,
	m: Props_m,
	mx: Props_mx,
	my: Props_my,
	mt: Props_mt,
	mr: Props_mr,
	mb: Props_mb,
	ml: Props_ml,
	ms: Props_ms,
	me: Props_me,
	gap: Props_gap,
	gap_x: Props_gap_x,
	gap_y: Props_gap_y,
	-- Flex and grid.
	direction: Props_direction,
	dir: Props_dir,
	grow: Props_grow,
	shrink: Props_shrink,
	basis: Props_basis,
	flex_wrap: Props_flex_wrap,
	wrap_reverse: Props_wrap_reverse,
	items: Props_items,
	justify: Props_justify,
	self: Props_self,
	content: Props_content,
	grid: Props_grid,
	cols: Props_cols,
	rows: Props_rows,
	col_start: Props_col_start,
	row_start: Props_row_start,
	col_span: Props_col_span,
	row_span: Props_row_span,
	col_span_full: Props_col_span_full,
	row_span_full: Props_row_span_full,
	overflow: Props_overflow,
	overflow_x: Props_overflow_x,
	overflow_y: Props_overflow_y,
	hidden: Props_hidden,
	invisible: Props_invisible,
	-- Paint.
	bg: Props_bg,
	gradient: Props_gradient,
	color: Props_color,
	bg_image: Props_bg_image,
	bg_fit: Props_bg_fit,
	rounded: Props_rounded,
	corners: Props_corners,
	rounded_tl: Props_rounded_tl,
	rounded_tr: Props_rounded_tr,
	rounded_br: Props_rounded_br,
	rounded_bl: Props_rounded_bl,
	border: Props_border,
	border_sides: Props_border_sides,
	border_color: Props_border_color,
	border_over: Props_border_over,
	-- A function may return nil: a solid border.
	border_dash: Props_border_dash,
	outline: Props_outline,
	outline_color: Props_outline_color,
	outline_offset: Props_outline_offset,
	shadow: Props_shadow,
	inset_shadow: Props_inset_shadow,
	opacity: Props_opacity,
	brightness: Props_brightness,
	contrast: Props_contrast,
	saturate: Props_saturate,
	blur: Props_blur,
	backdrop_blur: Props_backdrop_blur,
	fade: Props_fade,
	fade_x: Props_fade_x,
	fade_sides: Props_fade_sides,
	fade_overflow: Props_fade_overflow,
	snap: Props_snap,
	clip_inset: Props_clip_inset,
	layer: Props_layer,
	fit: Props_fit,
	tint: Props_tint,
	cursor: Props_cursor,
	-- Text.
	text_size: Props_text_size,
	weight: Props_weight,
	bold: Props_bold,
	italic: Props_italic,
	align: Props_align,
	font: Props_font,
	wrap: Props_wrap,
	truncate: Props_truncate,
	lines: Props_lines,
	line_height: Props_line_height,
	letter_spacing: Props_letter_spacing,
	-- Tabular figures: every digit the same width.
	tabular: Props_tabular,
	-- Underlined text (inherited, as CSS's `text-decoration`), and how far
	-- below the baseline the line sits.
	underline: Props_underline,
	underline_offset: Props_underline_offset,
	-- Transforms: after layout, about `origin` (fractions of the node).
	x: Props_x,
	y: Props_y,
	scale: Props_scale,
	scale_x: Props_scale_x,
	scale_y: Props_scale_y,
	rotate: Props_rotate,
	skew: Props_skew,
	skew_x: Props_skew_x,
	skew_y: Props_skew_y,
	origin: Props_origin,
	-- The subtree's theme: a theme's name (`"zgx-light"`), or tokens to
	-- override (`{ accent = "#e5484d" }`, with `theme = name` to start from).
	theme: Props_theme,
	-- Identity, for selectors, refs and animations.
	id: Props_id,
	-- Space-separated class names.
	class: Props_class,
	-- Styles chosen from a `ui.variants` definition: `button { tone =
	-- "ghost" }`, or the recipe itself for its defaults. The element's own
	-- properties win over them.
	variants: Props_variants,
	ref: Props_ref,
	focusable: Props_focusable,
	-- What assistive technology takes it for (ARIA's `role`), instead of
	-- what it is: a clickable row as an "option", a row of buttons as a
	-- "toolbar". It keeps what it does.
	role: Props_role,
	-- A section title of this level, 1 to 6 (`h1`…`h6`): screen readers
	-- list headings and move between them.
	heading: Props_heading,
	-- Whether this option, tab or item is the chosen one (`aria-selected`).
	selected: Props_selected,
	-- Whether a switch, checkbox or radio of the app's own is on
	-- (`aria-checked`), and whether what a control opens shows
	-- (`aria-expanded`): a custom disclosure's.
	checked: Props_checked,
	expanded: Props_expanded,
	-- Announce changes to the text in it (`aria-live`): a count, a "Sent"
	-- note, a message arriving.
	live: Props_live,
	-- Read after its name, as a hint (`aria-description`).
	description: Props_description,
	-- What is wrong with its value (`aria-invalid`), followed: nil while it
	-- is fine. Assistive technology hears it with the field.
	error: Props_error,
	-- Left out of the accessibility tree with what it holds (`aria-hidden`):
	-- decoration, or what is said elsewhere.
	aria_hidden: Props_aria_hidden,
	-- One Tab stop for the focusable things inside; the arrows along this
	-- axis (and Home, End) move among them, as ARIA's composite widgets
	-- behave. `role = "listbox"` is vertical, "toolbar" and "tablist"
	-- horizontal, without asking.
	focus_group: Props_focus_group,
	-- A press on it (or on what inside takes no input) moves the window; a
	-- double click zooms it. Buttons and inputs inside keep their clicks.
	-- CSS pointer-events: "none" (or false) lets the pointer through to what
	-- is under the node (presses, hover, the wheel, drops, a native view);
	-- descendants with handlers still take it. A state or function follows.
	pointer_events: Props_pointer_events,
	window_drag: Props_window_drag,
	-- The pointer selects the text in it and everything it holds, as on a
	-- web page (CSS `user-select: text`): drag, double/triple press, Shift,
	-- ⌘A / Ctrl+A, ⌘C / Ctrl+C. `false` keeps a part out. Off unless set.
	selectable: Props_selectable,
	-- A selectable node's highlight and selected-text colours (CSS
	-- `::selection`), over the theme's `$selection` and `$selection.text`
	-- tokens; a state or function follows.
	selection_color: Props_selection_color,
	selected_text_color: Props_selected_text_color,
	-- Shown over it while the pointer rests on it (after 300 ms; at once
	-- within a second of another): text in the theme's look, or a node of
	-- the app's own. Above it, centred, unless `tooltip_placement` says.
	tooltip: Props_tooltip,
	tooltip_placement: Props_tooltip_placement,
	-- A hover card the pointer can move into and click in: it stays while
	-- the pointer is over the node or the card.
	tooltip_interactive: Props_tooltip_interactive,
	-- Milliseconds the pointer rests before it shows (300; once one
	-- shows, the next shows at once). Keyboard focus shows it too.
	tooltip_delay: Props_tooltip_delay,
	-- A condition (true disables), or the styles it shows while disabled.
	disabled: Props_disabled,
	hover: Props_hover,
	active: Props_active,
	focus: Props_focus,
	-- Size animation ("height", "width", "size"; `true` is "height") or
	-- position animation ("position"). A `layout_id` node mounting as
	-- another with the same id unmounts (or while it is mounted) moves and
	-- scales from that one's box.
	layout: Props_layout,
	layout_id: Props_layout_id,
	layout_ease: Props_layout_ease,
	-- Milliseconds.
	layout_duration: Props_layout_duration,
	layout_enter: Props_layout_enter,
	layout_clip: Props_layout_clip,
	-- The content shows through the bitmap's alpha: `mask = bitmap`, or
	-- `{ bitmap, w = …, h = …, x = …, y = …, sweep = … }`.
	mask: Props_mask,
	bg_shader: Props_bg_shader,
	-- Up to four vec4s: lists of numbers, or colors (0 to 1).
	shader_params: Props_shader_params,
	-- Events.
	on_click: Props_on_click,
	-- Primary pointer down and up.
	on_press: Props_on_press,
	on_release: Props_on_release,
	on_hover: Props_on_hover,
	on_pointer_move: Props_on_pointer_move,
	-- Return true to consume the key.
	on_key: Props_on_key,
	on_key_up: Props_on_key_up,
	on_focus: Props_on_focus,
	on_blur: Props_on_blur,
	-- Return true to take the gesture from the scroll views around it.
	on_wheel: Props_on_wheel,
	on_pinch: Props_on_pinch,
	on_mount: Props_on_mount,
	-- When layout gives it a new size, and at its first layout.
	on_resize: Props_on_resize,
	-- When any of it comes into view or all of it leaves (inside every
	-- clip and scroll viewport above it), and at its first layout: for
	-- building what only matters on screen.
	on_visible: Props_on_visible,
	-- Drag and drop within the window. Payloads are what `on_drag`
	-- returns: the script's own plain values, so they are `any` here.
	-- A source: `on_drag` returns the payload once the pointer moves 4
	-- points; `drag_preview` builds what follows the pointer.
	on_drag: Props_on_drag,
	drag_preview: Props_drag_preview,
	on_drag_end: Props_on_drag_end,
	-- What the drag carries to other apps as it leaves the window (macOS):
	-- Finder takes files, Mail text, a browser links.
	drag_export: Props_drag_export,
	-- Released outside every window of the app (macOS): a tab torn off its
	-- window. `grab_x`/`grab_y` are where the source was held.
	on_drag_outside: Props_on_drag_outside,
	-- The drag moves its window instead, with no preview (macOS): a
	-- window's only tab. Dropped on another window's destination it drops
	-- there; elsewhere the window stays where it was let go.
	drag_moves_window: Props_drag_moves_window,
	-- A destination: `drop_when` filters payloads (default: all).
	drop_when: Props_drop_when,
	on_drag_over: Props_on_drag_over,
	on_drag_leave: Props_on_drag_leave,
	on_drop: Props_on_drop,
	-- A secondary click (control-click on macOS) shows the returned
	-- `menu_item`s, `submenu`s and separators as a menu at the pointer;
	-- returning nil (or nothing) shows none.
	context_menu: Props_context_menu,
	-- The context menu panel's look: bg, rounded, p, gap, backdrop_blur,
	-- border, shadow, w, …
	context_menu_style: Props_context_menu_style,
	-- Open the context menu under the node, from its left edge (a tab's),
	-- rather than at the pointer.
	context_menu_below: Props_context_menu_below,
	[number]: never,
	term: Term,
}
-- `from`: only what that plugin (by name) contributes.
export type SlotProps = {
	-- Size and position.
	w: Props_w,
	h: Props_h,
	size: Props_size,
	min_w: Props_min_w,
	max_w: Props_max_w,
	min_h: Props_min_h,
	max_h: Props_max_h,
	w_full: Props_w_full,
	h_full: Props_h_full,
	aspect: Props_aspect,
	absolute: Props_absolute,
	relative: Props_relative,
	top: Props_top,
	left: Props_left,
	inset_start: Props_inset_start,
	inset_end: Props_inset_end,
	right: Props_right,
	bottom: Props_bottom,
	-- Spacing.
	p: Props_p,
	px: Props_px,
	py: Props_py,
	pt: Props_pt,
	pr: Props_pr,
	pb: Props_pb,
	pl: Props_pl,
	ps: Props_ps,
	pe: Props_pe,
	m: Props_m,
	mx: Props_mx,
	my: Props_my,
	mt: Props_mt,
	mr: Props_mr,
	mb: Props_mb,
	ml: Props_ml,
	ms: Props_ms,
	me: Props_me,
	gap: Props_gap,
	gap_x: Props_gap_x,
	gap_y: Props_gap_y,
	-- Flex and grid.
	direction: Props_direction,
	dir: Props_dir,
	grow: Props_grow,
	shrink: Props_shrink,
	basis: Props_basis,
	flex_wrap: Props_flex_wrap,
	wrap_reverse: Props_wrap_reverse,
	items: Props_items,
	justify: Props_justify,
	self: Props_self,
	content: Props_content,
	grid: Props_grid,
	cols: Props_cols,
	rows: Props_rows,
	col_start: Props_col_start,
	row_start: Props_row_start,
	col_span: Props_col_span,
	row_span: Props_row_span,
	col_span_full: Props_col_span_full,
	row_span_full: Props_row_span_full,
	overflow: Props_overflow,
	overflow_x: Props_overflow_x,
	overflow_y: Props_overflow_y,
	hidden: Props_hidden,
	invisible: Props_invisible,
	-- Paint.
	bg: Props_bg,
	gradient: Props_gradient,
	color: Props_color,
	bg_image: Props_bg_image,
	bg_fit: Props_bg_fit,
	rounded: Props_rounded,
	corners: Props_corners,
	rounded_tl: Props_rounded_tl,
	rounded_tr: Props_rounded_tr,
	rounded_br: Props_rounded_br,
	rounded_bl: Props_rounded_bl,
	border: Props_border,
	border_sides: Props_border_sides,
	border_color: Props_border_color,
	border_over: Props_border_over,
	-- A function may return nil: a solid border.
	border_dash: Props_border_dash,
	outline: Props_outline,
	outline_color: Props_outline_color,
	outline_offset: Props_outline_offset,
	shadow: Props_shadow,
	inset_shadow: Props_inset_shadow,
	opacity: Props_opacity,
	brightness: Props_brightness,
	contrast: Props_contrast,
	saturate: Props_saturate,
	blur: Props_blur,
	backdrop_blur: Props_backdrop_blur,
	fade: Props_fade,
	fade_x: Props_fade_x,
	fade_sides: Props_fade_sides,
	fade_overflow: Props_fade_overflow,
	snap: Props_snap,
	clip_inset: Props_clip_inset,
	layer: Props_layer,
	fit: Props_fit,
	tint: Props_tint,
	cursor: Props_cursor,
	-- Text.
	text_size: Props_text_size,
	weight: Props_weight,
	bold: Props_bold,
	italic: Props_italic,
	align: Props_align,
	font: Props_font,
	wrap: Props_wrap,
	truncate: Props_truncate,
	lines: Props_lines,
	line_height: Props_line_height,
	letter_spacing: Props_letter_spacing,
	-- Tabular figures: every digit the same width.
	tabular: Props_tabular,
	-- Underlined text (inherited, as CSS's `text-decoration`), and how far
	-- below the baseline the line sits.
	underline: Props_underline,
	underline_offset: Props_underline_offset,
	-- Transforms: after layout, about `origin` (fractions of the node).
	x: Props_x,
	y: Props_y,
	scale: Props_scale,
	scale_x: Props_scale_x,
	scale_y: Props_scale_y,
	rotate: Props_rotate,
	skew: Props_skew,
	skew_x: Props_skew_x,
	skew_y: Props_skew_y,
	origin: Props_origin,
	-- The subtree's theme: a theme's name (`"zgx-light"`), or tokens to
	-- override (`{ accent = "#e5484d" }`, with `theme = name` to start from).
	theme: Props_theme,
	-- Identity, for selectors, refs and animations.
	id: Props_id,
	-- Space-separated class names.
	class: Props_class,
	-- Styles chosen from a `ui.variants` definition: `button { tone =
	-- "ghost" }`, or the recipe itself for its defaults. The element's own
	-- properties win over them.
	variants: Props_variants,
	ref: Props_ref,
	focusable: Props_focusable,
	-- What assistive technology takes it for (ARIA's `role`), instead of
	-- what it is: a clickable row as an "option", a row of buttons as a
	-- "toolbar". It keeps what it does.
	role: Props_role,
	-- A section title of this level, 1 to 6 (`h1`…`h6`): screen readers
	-- list headings and move between them.
	heading: Props_heading,
	-- Whether this option, tab or item is the chosen one (`aria-selected`).
	selected: Props_selected,
	-- Whether a switch, checkbox or radio of the app's own is on
	-- (`aria-checked`), and whether what a control opens shows
	-- (`aria-expanded`): a custom disclosure's.
	checked: Props_checked,
	expanded: Props_expanded,
	-- Announce changes to the text in it (`aria-live`): a count, a "Sent"
	-- note, a message arriving.
	live: Props_live,
	-- Read after its name, as a hint (`aria-description`).
	description: Props_description,
	-- What is wrong with its value (`aria-invalid`), followed: nil while it
	-- is fine. Assistive technology hears it with the field.
	error: Props_error,
	-- Left out of the accessibility tree with what it holds (`aria-hidden`):
	-- decoration, or what is said elsewhere.
	aria_hidden: Props_aria_hidden,
	-- One Tab stop for the focusable things inside; the arrows along this
	-- axis (and Home, End) move among them, as ARIA's composite widgets
	-- behave. `role = "listbox"` is vertical, "toolbar" and "tablist"
	-- horizontal, without asking.
	focus_group: Props_focus_group,
	-- A press on it (or on what inside takes no input) moves the window; a
	-- double click zooms it. Buttons and inputs inside keep their clicks.
	-- CSS pointer-events: "none" (or false) lets the pointer through to what
	-- is under the node (presses, hover, the wheel, drops, a native view);
	-- descendants with handlers still take it. A state or function follows.
	pointer_events: Props_pointer_events,
	window_drag: Props_window_drag,
	-- The pointer selects the text in it and everything it holds, as on a
	-- web page (CSS `user-select: text`): drag, double/triple press, Shift,
	-- ⌘A / Ctrl+A, ⌘C / Ctrl+C. `false` keeps a part out. Off unless set.
	selectable: Props_selectable,
	-- A selectable node's highlight and selected-text colours (CSS
	-- `::selection`), over the theme's `$selection` and `$selection.text`
	-- tokens; a state or function follows.
	selection_color: Props_selection_color,
	selected_text_color: Props_selected_text_color,
	-- Shown over it while the pointer rests on it (after 300 ms; at once
	-- within a second of another): text in the theme's look, or a node of
	-- the app's own. Above it, centred, unless `tooltip_placement` says.
	tooltip: Props_tooltip,
	tooltip_placement: Props_tooltip_placement,
	-- A hover card the pointer can move into and click in: it stays while
	-- the pointer is over the node or the card.
	tooltip_interactive: Props_tooltip_interactive,
	-- Milliseconds the pointer rests before it shows (300; once one
	-- shows, the next shows at once). Keyboard focus shows it too.
	tooltip_delay: Props_tooltip_delay,
	-- A condition (true disables), or the styles it shows while disabled.
	disabled: Props_disabled,
	hover: Props_hover,
	active: Props_active,
	focus: Props_focus,
	-- Size animation ("height", "width", "size"; `true` is "height") or
	-- position animation ("position"). A `layout_id` node mounting as
	-- another with the same id unmounts (or while it is mounted) moves and
	-- scales from that one's box.
	layout: Props_layout,
	layout_id: Props_layout_id,
	layout_ease: Props_layout_ease,
	-- Milliseconds.
	layout_duration: Props_layout_duration,
	layout_enter: Props_layout_enter,
	layout_clip: Props_layout_clip,
	-- The content shows through the bitmap's alpha: `mask = bitmap`, or
	-- `{ bitmap, w = …, h = …, x = …, y = …, sweep = … }`.
	mask: Props_mask,
	bg_shader: Props_bg_shader,
	-- Up to four vec4s: lists of numbers, or colors (0 to 1).
	shader_params: Props_shader_params,
	-- Events.
	on_click: Props_on_click,
	-- Primary pointer down and up.
	on_press: Props_on_press,
	on_release: Props_on_release,
	on_hover: Props_on_hover,
	on_pointer_move: Props_on_pointer_move,
	-- Return true to consume the key.
	on_key: Props_on_key,
	on_key_up: Props_on_key_up,
	on_focus: Props_on_focus,
	on_blur: Props_on_blur,
	-- Return true to take the gesture from the scroll views around it.
	on_wheel: Props_on_wheel,
	on_pinch: Props_on_pinch,
	on_mount: Props_on_mount,
	-- When layout gives it a new size, and at its first layout.
	on_resize: Props_on_resize,
	-- When any of it comes into view or all of it leaves (inside every
	-- clip and scroll viewport above it), and at its first layout: for
	-- building what only matters on screen.
	on_visible: Props_on_visible,
	-- Drag and drop within the window. Payloads are what `on_drag`
	-- returns: the script's own plain values, so they are `any` here.
	-- A source: `on_drag` returns the payload once the pointer moves 4
	-- points; `drag_preview` builds what follows the pointer.
	on_drag: Props_on_drag,
	drag_preview: Props_drag_preview,
	on_drag_end: Props_on_drag_end,
	-- What the drag carries to other apps as it leaves the window (macOS):
	-- Finder takes files, Mail text, a browser links.
	drag_export: Props_drag_export,
	-- Released outside every window of the app (macOS): a tab torn off its
	-- window. `grab_x`/`grab_y` are where the source was held.
	on_drag_outside: Props_on_drag_outside,
	-- The drag moves its window instead, with no preview (macOS): a
	-- window's only tab. Dropped on another window's destination it drops
	-- there; elsewhere the window stays where it was let go.
	drag_moves_window: Props_drag_moves_window,
	-- A destination: `drop_when` filters payloads (default: all).
	drop_when: Props_drop_when,
	on_drag_over: Props_on_drag_over,
	on_drag_leave: Props_on_drag_leave,
	on_drop: Props_on_drop,
	-- A secondary click (control-click on macOS) shows the returned
	-- `menu_item`s, `submenu`s and separators as a menu at the pointer;
	-- returning nil (or nothing) shows none.
	context_menu: Props_context_menu,
	-- The context menu panel's look: bg, rounded, p, gap, backdrop_blur,
	-- border, shadow, w, …
	context_menu_style: Props_context_menu_style,
	-- Open the context menu under the node, from its left edge (a tab's),
	-- rather than at the pointer.
	context_menu_below: Props_context_menu_below,
	[number]: never,
	context: unknown?,
	from: string?,
}
-- How `ui.each` rows slide when the list moves them: `true` (a spring,
-- bounce .1 over 400 ms), an ease, `{ duration, ease }`, `{ bounce,
-- duration }` or `{ spring = anime.spring {...} }`.
export type Motion = boolean | Easing | { duration: number?, ease: Easing?, bounce: number?, spring: AnimeEase? }
-- `ui.each`'s options; the rest style the box the rows lay out in.
export type EachOptions = {
	-- Size and position.
	w: Props_w,
	h: Props_h,
	size: Props_size,
	min_w: Props_min_w,
	max_w: Props_max_w,
	min_h: Props_min_h,
	max_h: Props_max_h,
	w_full: Props_w_full,
	h_full: Props_h_full,
	aspect: Props_aspect,
	absolute: Props_absolute,
	relative: Props_relative,
	top: Props_top,
	left: Props_left,
	inset_start: Props_inset_start,
	inset_end: Props_inset_end,
	right: Props_right,
	bottom: Props_bottom,
	-- Spacing.
	p: Props_p,
	px: Props_px,
	py: Props_py,
	pt: Props_pt,
	pr: Props_pr,
	pb: Props_pb,
	pl: Props_pl,
	ps: Props_ps,
	pe: Props_pe,
	m: Props_m,
	mx: Props_mx,
	my: Props_my,
	mt: Props_mt,
	mr: Props_mr,
	mb: Props_mb,
	ml: Props_ml,
	ms: Props_ms,
	me: Props_me,
	gap: Props_gap,
	gap_x: Props_gap_x,
	gap_y: Props_gap_y,
	-- Flex and grid.
	direction: Props_direction,
	dir: Props_dir,
	grow: Props_grow,
	shrink: Props_shrink,
	basis: Props_basis,
	flex_wrap: Props_flex_wrap,
	wrap_reverse: Props_wrap_reverse,
	items: Props_items,
	justify: Props_justify,
	self: Props_self,
	content: Props_content,
	grid: Props_grid,
	cols: Props_cols,
	rows: Props_rows,
	col_start: Props_col_start,
	row_start: Props_row_start,
	col_span: Props_col_span,
	row_span: Props_row_span,
	col_span_full: Props_col_span_full,
	row_span_full: Props_row_span_full,
	overflow: Props_overflow,
	overflow_x: Props_overflow_x,
	overflow_y: Props_overflow_y,
	hidden: Props_hidden,
	invisible: Props_invisible,
	-- Paint.
	bg: Props_bg,
	gradient: Props_gradient,
	color: Props_color,
	bg_image: Props_bg_image,
	bg_fit: Props_bg_fit,
	rounded: Props_rounded,
	corners: Props_corners,
	rounded_tl: Props_rounded_tl,
	rounded_tr: Props_rounded_tr,
	rounded_br: Props_rounded_br,
	rounded_bl: Props_rounded_bl,
	border: Props_border,
	border_sides: Props_border_sides,
	border_color: Props_border_color,
	border_over: Props_border_over,
	-- A function may return nil: a solid border.
	border_dash: Props_border_dash,
	outline: Props_outline,
	outline_color: Props_outline_color,
	outline_offset: Props_outline_offset,
	shadow: Props_shadow,
	inset_shadow: Props_inset_shadow,
	opacity: Props_opacity,
	brightness: Props_brightness,
	contrast: Props_contrast,
	saturate: Props_saturate,
	blur: Props_blur,
	backdrop_blur: Props_backdrop_blur,
	fade: Props_fade,
	fade_x: Props_fade_x,
	fade_sides: Props_fade_sides,
	fade_overflow: Props_fade_overflow,
	snap: Props_snap,
	clip_inset: Props_clip_inset,
	layer: Props_layer,
	fit: Props_fit,
	tint: Props_tint,
	cursor: Props_cursor,
	-- Text.
	text_size: Props_text_size,
	weight: Props_weight,
	bold: Props_bold,
	italic: Props_italic,
	align: Props_align,
	font: Props_font,
	wrap: Props_wrap,
	truncate: Props_truncate,
	lines: Props_lines,
	line_height: Props_line_height,
	letter_spacing: Props_letter_spacing,
	-- Tabular figures: every digit the same width.
	tabular: Props_tabular,
	-- Underlined text (inherited, as CSS's `text-decoration`), and how far
	-- below the baseline the line sits.
	underline: Props_underline,
	underline_offset: Props_underline_offset,
	-- Transforms: after layout, about `origin` (fractions of the node).
	x: Props_x,
	y: Props_y,
	scale: Props_scale,
	scale_x: Props_scale_x,
	scale_y: Props_scale_y,
	rotate: Props_rotate,
	skew: Props_skew,
	skew_x: Props_skew_x,
	skew_y: Props_skew_y,
	origin: Props_origin,
	-- The subtree's theme: a theme's name (`"zgx-light"`), or tokens to
	-- override (`{ accent = "#e5484d" }`, with `theme = name` to start from).
	theme: Props_theme,
	-- Identity, for selectors, refs and animations.
	id: Props_id,
	-- Space-separated class names.
	class: Props_class,
	-- Styles chosen from a `ui.variants` definition: `button { tone =
	-- "ghost" }`, or the recipe itself for its defaults. The element's own
	-- properties win over them.
	variants: Props_variants,
	ref: Props_ref,
	focusable: Props_focusable,
	-- What assistive technology takes it for (ARIA's `role`), instead of
	-- what it is: a clickable row as an "option", a row of buttons as a
	-- "toolbar". It keeps what it does.
	role: Props_role,
	-- A section title of this level, 1 to 6 (`h1`…`h6`): screen readers
	-- list headings and move between them.
	heading: Props_heading,
	-- Whether this option, tab or item is the chosen one (`aria-selected`).
	selected: Props_selected,
	-- Whether a switch, checkbox or radio of the app's own is on
	-- (`aria-checked`), and whether what a control opens shows
	-- (`aria-expanded`): a custom disclosure's.
	checked: Props_checked,
	expanded: Props_expanded,
	-- Announce changes to the text in it (`aria-live`): a count, a "Sent"
	-- note, a message arriving.
	live: Props_live,
	-- Read after its name, as a hint (`aria-description`).
	description: Props_description,
	-- What is wrong with its value (`aria-invalid`), followed: nil while it
	-- is fine. Assistive technology hears it with the field.
	error: Props_error,
	-- Left out of the accessibility tree with what it holds (`aria-hidden`):
	-- decoration, or what is said elsewhere.
	aria_hidden: Props_aria_hidden,
	-- One Tab stop for the focusable things inside; the arrows along this
	-- axis (and Home, End) move among them, as ARIA's composite widgets
	-- behave. `role = "listbox"` is vertical, "toolbar" and "tablist"
	-- horizontal, without asking.
	focus_group: Props_focus_group,
	-- A press on it (or on what inside takes no input) moves the window; a
	-- double click zooms it. Buttons and inputs inside keep their clicks.
	-- CSS pointer-events: "none" (or false) lets the pointer through to what
	-- is under the node (presses, hover, the wheel, drops, a native view);
	-- descendants with handlers still take it. A state or function follows.
	pointer_events: Props_pointer_events,
	window_drag: Props_window_drag,
	-- The pointer selects the text in it and everything it holds, as on a
	-- web page (CSS `user-select: text`): drag, double/triple press, Shift,
	-- ⌘A / Ctrl+A, ⌘C / Ctrl+C. `false` keeps a part out. Off unless set.
	selectable: Props_selectable,
	-- A selectable node's highlight and selected-text colours (CSS
	-- `::selection`), over the theme's `$selection` and `$selection.text`
	-- tokens; a state or function follows.
	selection_color: Props_selection_color,
	selected_text_color: Props_selected_text_color,
	-- Shown over it while the pointer rests on it (after 300 ms; at once
	-- within a second of another): text in the theme's look, or a node of
	-- the app's own. Above it, centred, unless `tooltip_placement` says.
	tooltip: Props_tooltip,
	tooltip_placement: Props_tooltip_placement,
	-- A hover card the pointer can move into and click in: it stays while
	-- the pointer is over the node or the card.
	tooltip_interactive: Props_tooltip_interactive,
	-- Milliseconds the pointer rests before it shows (300; once one
	-- shows, the next shows at once). Keyboard focus shows it too.
	tooltip_delay: Props_tooltip_delay,
	-- A condition (true disables), or the styles it shows while disabled.
	disabled: Props_disabled,
	hover: Props_hover,
	active: Props_active,
	focus: Props_focus,
	-- Size animation ("height", "width", "size"; `true` is "height") or
	-- position animation ("position"). A `layout_id` node mounting as
	-- another with the same id unmounts (or while it is mounted) moves and
	-- scales from that one's box.
	layout: Props_layout,
	layout_id: Props_layout_id,
	layout_ease: Props_layout_ease,
	-- Milliseconds.
	layout_duration: Props_layout_duration,
	layout_enter: Props_layout_enter,
	layout_clip: Props_layout_clip,
	-- The content shows through the bitmap's alpha: `mask = bitmap`, or
	-- `{ bitmap, w = …, h = …, x = …, y = …, sweep = … }`.
	mask: Props_mask,
	bg_shader: Props_bg_shader,
	-- Up to four vec4s: lists of numbers, or colors (0 to 1).
	shader_params: Props_shader_params,
	-- Events.
	on_click: Props_on_click,
	-- Primary pointer down and up.
	on_press: Props_on_press,
	on_release: Props_on_release,
	on_hover: Props_on_hover,
	on_pointer_move: Props_on_pointer_move,
	-- Return true to consume the key.
	on_key: Props_on_key,
	on_key_up: Props_on_key_up,
	on_focus: Props_on_focus,
	on_blur: Props_on_blur,
	-- Return true to take the gesture from the scroll views around it.
	on_wheel: Props_on_wheel,
	on_pinch: Props_on_pinch,
	on_mount: Props_on_mount,
	-- When layout gives it a new size, and at its first layout.
	on_resize: Props_on_resize,
	-- When any of it comes into view or all of it leaves (inside every
	-- clip and scroll viewport above it), and at its first layout: for
	-- building what only matters on screen.
	on_visible: Props_on_visible,
	-- Drag and drop within the window. Payloads are what `on_drag`
	-- returns: the script's own plain values, so they are `any` here.
	-- A source: `on_drag` returns the payload once the pointer moves 4
	-- points; `drag_preview` builds what follows the pointer.
	on_drag: Props_on_drag,
	drag_preview: Props_drag_preview,
	on_drag_end: Props_on_drag_end,
	-- What the drag carries to other apps as it leaves the window (macOS):
	-- Finder takes files, Mail text, a browser links.
	drag_export: Props_drag_export,
	-- Released outside every window of the app (macOS): a tab torn off its
	-- window. `grab_x`/`grab_y` are where the source was held.
	on_drag_outside: Props_on_drag_outside,
	-- The drag moves its window instead, with no preview (macOS): a
	-- window's only tab. Dropped on another window's destination it drops
	-- there; elsewhere the window stays where it was let go.
	drag_moves_window: Props_drag_moves_window,
	-- A destination: `drop_when` filters payloads (default: all).
	drop_when: Props_drop_when,
	on_drag_over: Props_on_drag_over,
	on_drag_leave: Props_on_drag_leave,
	on_drop: Props_on_drop,
	-- A secondary click (control-click on macOS) shows the returned
	-- `menu_item`s, `submenu`s and separators as a menu at the pointer;
	-- returning nil (or nothing) shows none.
	context_menu: Props_context_menu,
	-- The context menu panel's look: bg, rounded, p, gap, backdrop_blur,
	-- border, shadow, w, …
	context_menu_style: Props_context_menu_style,
	-- Open the context menu under the node, from its left edge (a tab's),
	-- rather than at the pointer.
	context_menu_below: Props_context_menu_below,
	[number]: never,
	-- A row's identity: rows keep their state as the list reorders. The
	-- item is `any`, as `build`'s is.
	key: ((item: any) -> string | number)?,
	animate: Motion?,
	-- anime.js parameters played on rows that come and go.
	enter: ElementParams?,
	exit: ElementParams?,
	-- The first rows play `enter` too.
	initial: boolean?,
}
-- `ui.presence`'s options; sizing makes it a box its content fills.
export type PresenceOptions = {
	-- Size and position.
	w: Props_w,
	h: Props_h,
	size: Props_size,
	min_w: Props_min_w,
	max_w: Props_max_w,
	min_h: Props_min_h,
	max_h: Props_max_h,
	w_full: Props_w_full,
	h_full: Props_h_full,
	aspect: Props_aspect,
	absolute: Props_absolute,
	relative: Props_relative,
	top: Props_top,
	left: Props_left,
	inset_start: Props_inset_start,
	inset_end: Props_inset_end,
	right: Props_right,
	bottom: Props_bottom,
	-- Spacing.
	p: Props_p,
	px: Props_px,
	py: Props_py,
	pt: Props_pt,
	pr: Props_pr,
	pb: Props_pb,
	pl: Props_pl,
	ps: Props_ps,
	pe: Props_pe,
	m: Props_m,
	mx: Props_mx,
	my: Props_my,
	mt: Props_mt,
	mr: Props_mr,
	mb: Props_mb,
	ml: Props_ml,
	ms: Props_ms,
	me: Props_me,
	gap: Props_gap,
	gap_x: Props_gap_x,
	gap_y: Props_gap_y,
	-- Flex and grid.
	direction: Props_direction,
	dir: Props_dir,
	grow: Props_grow,
	shrink: Props_shrink,
	basis: Props_basis,
	flex_wrap: Props_flex_wrap,
	wrap_reverse: Props_wrap_reverse,
	items: Props_items,
	justify: Props_justify,
	self: Props_self,
	content: Props_content,
	grid: Props_grid,
	cols: Props_cols,
	rows: Props_rows,
	col_start: Props_col_start,
	row_start: Props_row_start,
	col_span: Props_col_span,
	row_span: Props_row_span,
	col_span_full: Props_col_span_full,
	row_span_full: Props_row_span_full,
	overflow: Props_overflow,
	overflow_x: Props_overflow_x,
	overflow_y: Props_overflow_y,
	hidden: Props_hidden,
	invisible: Props_invisible,
	-- Paint.
	bg: Props_bg,
	gradient: Props_gradient,
	color: Props_color,
	bg_image: Props_bg_image,
	bg_fit: Props_bg_fit,
	rounded: Props_rounded,
	corners: Props_corners,
	rounded_tl: Props_rounded_tl,
	rounded_tr: Props_rounded_tr,
	rounded_br: Props_rounded_br,
	rounded_bl: Props_rounded_bl,
	border: Props_border,
	border_sides: Props_border_sides,
	border_color: Props_border_color,
	border_over: Props_border_over,
	-- A function may return nil: a solid border.
	border_dash: Props_border_dash,
	outline: Props_outline,
	outline_color: Props_outline_color,
	outline_offset: Props_outline_offset,
	shadow: Props_shadow,
	inset_shadow: Props_inset_shadow,
	opacity: Props_opacity,
	brightness: Props_brightness,
	contrast: Props_contrast,
	saturate: Props_saturate,
	blur: Props_blur,
	backdrop_blur: Props_backdrop_blur,
	fade: Props_fade,
	fade_x: Props_fade_x,
	fade_sides: Props_fade_sides,
	fade_overflow: Props_fade_overflow,
	snap: Props_snap,
	clip_inset: Props_clip_inset,
	layer: Props_layer,
	fit: Props_fit,
	tint: Props_tint,
	cursor: Props_cursor,
	-- Text.
	text_size: Props_text_size,
	weight: Props_weight,
	bold: Props_bold,
	italic: Props_italic,
	align: Props_align,
	font: Props_font,
	wrap: Props_wrap,
	truncate: Props_truncate,
	lines: Props_lines,
	line_height: Props_line_height,
	letter_spacing: Props_letter_spacing,
	-- Tabular figures: every digit the same width.
	tabular: Props_tabular,
	-- Underlined text (inherited, as CSS's `text-decoration`), and how far
	-- below the baseline the line sits.
	underline: Props_underline,
	underline_offset: Props_underline_offset,
	-- Transforms: after layout, about `origin` (fractions of the node).
	x: Props_x,
	y: Props_y,
	scale: Props_scale,
	scale_x: Props_scale_x,
	scale_y: Props_scale_y,
	rotate: Props_rotate,
	skew: Props_skew,
	skew_x: Props_skew_x,
	skew_y: Props_skew_y,
	origin: Props_origin,
	-- The subtree's theme: a theme's name (`"zgx-light"`), or tokens to
	-- override (`{ accent = "#e5484d" }`, with `theme = name` to start from).
	theme: Props_theme,
	-- Identity, for selectors, refs and animations.
	id: Props_id,
	-- Space-separated class names.
	class: Props_class,
	-- Styles chosen from a `ui.variants` definition: `button { tone =
	-- "ghost" }`, or the recipe itself for its defaults. The element's own
	-- properties win over them.
	variants: Props_variants,
	ref: Props_ref,
	focusable: Props_focusable,
	-- What assistive technology takes it for (ARIA's `role`), instead of
	-- what it is: a clickable row as an "option", a row of buttons as a
	-- "toolbar". It keeps what it does.
	role: Props_role,
	-- A section title of this level, 1 to 6 (`h1`…`h6`): screen readers
	-- list headings and move between them.
	heading: Props_heading,
	-- Whether this option, tab or item is the chosen one (`aria-selected`).
	selected: Props_selected,
	-- Whether a switch, checkbox or radio of the app's own is on
	-- (`aria-checked`), and whether what a control opens shows
	-- (`aria-expanded`): a custom disclosure's.
	checked: Props_checked,
	expanded: Props_expanded,
	-- Announce changes to the text in it (`aria-live`): a count, a "Sent"
	-- note, a message arriving.
	live: Props_live,
	-- Read after its name, as a hint (`aria-description`).
	description: Props_description,
	-- What is wrong with its value (`aria-invalid`), followed: nil while it
	-- is fine. Assistive technology hears it with the field.
	error: Props_error,
	-- Left out of the accessibility tree with what it holds (`aria-hidden`):
	-- decoration, or what is said elsewhere.
	aria_hidden: Props_aria_hidden,
	-- One Tab stop for the focusable things inside; the arrows along this
	-- axis (and Home, End) move among them, as ARIA's composite widgets
	-- behave. `role = "listbox"` is vertical, "toolbar" and "tablist"
	-- horizontal, without asking.
	focus_group: Props_focus_group,
	-- A press on it (or on what inside takes no input) moves the window; a
	-- double click zooms it. Buttons and inputs inside keep their clicks.
	-- CSS pointer-events: "none" (or false) lets the pointer through to what
	-- is under the node (presses, hover, the wheel, drops, a native view);
	-- descendants with handlers still take it. A state or function follows.
	pointer_events: Props_pointer_events,
	window_drag: Props_window_drag,
	-- The pointer selects the text in it and everything it holds, as on a
	-- web page (CSS `user-select: text`): drag, double/triple press, Shift,
	-- ⌘A / Ctrl+A, ⌘C / Ctrl+C. `false` keeps a part out. Off unless set.
	selectable: Props_selectable,
	-- A selectable node's highlight and selected-text colours (CSS
	-- `::selection`), over the theme's `$selection` and `$selection.text`
	-- tokens; a state or function follows.
	selection_color: Props_selection_color,
	selected_text_color: Props_selected_text_color,
	-- Shown over it while the pointer rests on it (after 300 ms; at once
	-- within a second of another): text in the theme's look, or a node of
	-- the app's own. Above it, centred, unless `tooltip_placement` says.
	tooltip: Props_tooltip,
	tooltip_placement: Props_tooltip_placement,
	-- A hover card the pointer can move into and click in: it stays while
	-- the pointer is over the node or the card.
	tooltip_interactive: Props_tooltip_interactive,
	-- Milliseconds the pointer rests before it shows (300; once one
	-- shows, the next shows at once). Keyboard focus shows it too.
	tooltip_delay: Props_tooltip_delay,
	-- A condition (true disables), or the styles it shows while disabled.
	disabled: Props_disabled,
	hover: Props_hover,
	active: Props_active,
	focus: Props_focus,
	-- Size animation ("height", "width", "size"; `true` is "height") or
	-- position animation ("position"). A `layout_id` node mounting as
	-- another with the same id unmounts (or while it is mounted) moves and
	-- scales from that one's box.
	layout: Props_layout,
	layout_id: Props_layout_id,
	layout_ease: Props_layout_ease,
	-- Milliseconds.
	layout_duration: Props_layout_duration,
	layout_enter: Props_layout_enter,
	layout_clip: Props_layout_clip,
	-- The content shows through the bitmap's alpha: `mask = bitmap`, or
	-- `{ bitmap, w = …, h = …, x = …, y = …, sweep = … }`.
	mask: Props_mask,
	bg_shader: Props_bg_shader,
	-- Up to four vec4s: lists of numbers, or colors (0 to 1).
	shader_params: Props_shader_params,
	-- Events.
	on_click: Props_on_click,
	-- Primary pointer down and up.
	on_press: Props_on_press,
	on_release: Props_on_release,
	on_hover: Props_on_hover,
	on_pointer_move: Props_on_pointer_move,
	-- Return true to consume the key.
	on_key: Props_on_key,
	on_key_up: Props_on_key_up,
	on_focus: Props_on_focus,
	on_blur: Props_on_blur,
	-- Return true to take the gesture from the scroll views around it.
	on_wheel: Props_on_wheel,
	on_pinch: Props_on_pinch,
	on_mount: Props_on_mount,
	-- When layout gives it a new size, and at its first layout.
	on_resize: Props_on_resize,
	-- When any of it comes into view or all of it leaves (inside every
	-- clip and scroll viewport above it), and at its first layout: for
	-- building what only matters on screen.
	on_visible: Props_on_visible,
	-- Drag and drop within the window. Payloads are what `on_drag`
	-- returns: the script's own plain values, so they are `any` here.
	-- A source: `on_drag` returns the payload once the pointer moves 4
	-- points; `drag_preview` builds what follows the pointer.
	on_drag: Props_on_drag,
	drag_preview: Props_drag_preview,
	on_drag_end: Props_on_drag_end,
	-- What the drag carries to other apps as it leaves the window (macOS):
	-- Finder takes files, Mail text, a browser links.
	drag_export: Props_drag_export,
	-- Released outside every window of the app (macOS): a tab torn off its
	-- window. `grab_x`/`grab_y` are where the source was held.
	on_drag_outside: Props_on_drag_outside,
	-- The drag moves its window instead, with no preview (macOS): a
	-- window's only tab. Dropped on another window's destination it drops
	-- there; elsewhere the window stays where it was let go.
	drag_moves_window: Props_drag_moves_window,
	-- A destination: `drop_when` filters payloads (default: all).
	drop_when: Props_drop_when,
	on_drag_over: Props_on_drag_over,
	on_drag_leave: Props_on_drag_leave,
	on_drop: Props_on_drop,
	-- A secondary click (control-click on macOS) shows the returned
	-- `menu_item`s, `submenu`s and separators as a menu at the pointer;
	-- returning nil (or nothing) shows none.
	context_menu: Props_context_menu,
	-- The context menu panel's look: bg, rounded, p, gap, backdrop_blur,
	-- border, shadow, w, …
	context_menu_style: Props_context_menu_style,
	-- Open the context menu under the node, from its left edge (a tab's),
	-- rather than at the pointer.
	context_menu_below: Props_context_menu_below,
	[number]: never,
	enter: ElementParams?,
	exit: ElementParams?,
	-- The new content enters once the old has left.
	mode: "wait"?,
	-- The first content plays `enter` too.
	initial: boolean?,
}
-- What `ui.switch` matches: a case's key equals the value (or its text).
export type CaseKey = string | number | boolean

-- Durations are milliseconds, as in `anime`.
export type TweenEnd = number | string | { number }
export type TweenOptions = { duration: number?, ease: Easing?, from: TweenEnd?, to: TweenEnd?, [number]: never }
export type SpringOptions = { stiffness: number?, damping: number?, from: TweenEnd?, to: TweenEnd?, [number]: never }
export type LoopOptions = {
	duration: number?,
	ease: Easing?,
	-- Back and forth (true by default), or restart each cycle.
	yoyo: boolean?,
	-- Caps the frames it asks for.
	fps: number?,
	from: TweenEnd?,
	to: TweenEnd?,
	[number]: never,
}

-- `theme = …` on an element.
export type ThemeSpec = string | { [string]: string | number | { [string]: any } }
-- A theme as a table (or the JSON a theme file holds).
export type ThemeTable = {
	name: string,
	type: ("dark" | "light" | "high-contrast" | "high-contrast-light")?,
	extends: string?,
	tokens: { [string]: string | number | { [string]: any } }?,
}
export type ThemeApi = {
	-- Apply the theme `name` (stops following the system).
	set: (name: string) -> (),
	-- Apply `name` unless the app chose a theme: a default reloads keep.
	default: (name: string) -> (),
	-- The applied theme's name, as state: `current:get()` (tracked),
	-- `current:set(name)` applies one.
	current: State<string>,
	-- The applied theme's kind (tracked).
	kind: () -> ("dark" | "light" | "high-contrast" | "high-contrast-light"),
	names: () -> { string },
	-- Read a theme file (zgx's, or a VS Code colour theme) beside the
	-- script, register it and watch it; returns the theme's name.
	load: (path: string) -> string,
	register: (theme: ThemeTable) -> string,
	-- Declare the app's tokens with defaults in terms of the standard ones.
	tokens: (tokens: { [string]: string | number | { [string]: any } }) -> (),
	-- A token's value (tracked): colours as "#rrggbbaa", numbers, font names.
	get: (name: TokenName | string) -> any,
	follow_system: (themes: { dark: string, light: string } | false) -> (),
	-- The design lint: styles that hard-code a colour, spacing, radius,
	-- type size, weight, font or shadow warn ("warn", true) or fail
	-- ("error"), naming the token to use. `ZGX_DESIGN_LINT` overrides it.
	strict: (mode: "warn" | "error" | "off" | boolean) -> (),
}

declare ui: {
	theme: ThemeApi,
	-- The system's display settings, as CSS's media queries read them
	-- (`prefers-reduced-motion`, `prefers-contrast: more`,
	-- `prefers-reduced-transparency`): each follows the system (`:get()`
	-- tracked). The themes' tokens already take them (zero motion
	-- durations, stronger borders, opaque glass); read them for the rest.
	prefers: {
		reduced_motion: Readable<boolean>,
		more_contrast: Readable<boolean>,
		reduced_transparency: Readable<boolean>,
	},
	-- Call `fn` every `ms` milliseconds on zgx's clock (no frames between);
	-- returning false stops it. Returns a function that stops it. It lives as
	-- long as what made it: a component, a builder, or the script.
	every: (ms: number, fn: () -> ...unknown) -> () -> (),
	-- Call `fn` once, after `ms` milliseconds; returns a function that cancels it.
	after: (ms: number, fn: () -> ()) -> () -> (),
	column: (props: ContainerProps?) -> Node,
	row: (props: ContainerProps?) -> Node,
	div: (props: ContainerProps?) -> Node,
	-- Children stack in one box, each aligned on its own.
	overlay: (props: ContainerProps?) -> Node,
	button: (props: ContainerProps?) -> Node,
	-- A div with `grid = true`.
	grid: (props: ContainerProps?) -> Node,
	scroll: (props: ScrollProps?) -> Node,
	scroll_x: (props: ScrollProps?) -> Node,
	text: (content: Text | TextProps) -> Node,
	span: (content: Text | SpanProps) -> Span,
	rich: (props: RichProps) -> Node,
	image: (props: ImageProps) -> Node,
	svg: (props: SvgProps) -> Node,
	canvas: (props: CanvasProps) -> Node,
	webview: (props: WebViewProps) -> Node,
	-- `ui.web(opts)`: a page for this run. `ui.web(id, opts)`: the host's
	-- page for `id`, made with `opts` the first time, kept across reloads.
	web: ((opts: WebOptions?) -> Web) & ((id: string, opts: WebOptions?) -> Web),
	terminal: (props: TerminalProps) -> Node,
	-- The session for `id`, started the first time; `ui.term(id)` without
	-- options only looks it up.
	term: (id: string, opts: TermOptions?) -> Term,
	command: (opts: CommandOptions) -> (),
	-- Keys ("cmd+shift+z") as this platform shows them: "⇧⌘Z" on macOS,
	-- "Ctrl+Shift+Z" elsewhere (`cmd` is Command on macOS, Control
	-- elsewhere); a lone modifier ("cmd") as itself, "⌘" or "Ctrl".
	shortcut_text: (keys: string) -> string,
	window: WindowApi,
	checkbox: (props: CheckboxProps) -> Node,
	input: (props: InputProps) -> Node,
	textarea: (props: InputProps) -> Node,
	progress: (props: ProgressProps) -> Node,
	slider: (props: SliderProps) -> Node,
	-- Resizable panes: `ui.split { size = width, Sidebar(), Content() }`;
	-- the divider drags (no script runs), takes the arrow keys focused,
	-- and resets on a double click.
	split: (props: SplitProps) -> Node,
	segmented: (props: SegmentedProps) -> Node,
	tabs: (props: TabsProps) -> Node,
	tree: (props: TreeProps) -> Node,
	data_grid: (props: DataGridProps) -> Node,
	calendar: (props: CalendarProps) -> Node,
	date_picker: (props: DatePickerProps) -> Node,
	time_field: (props: TimeFieldProps) -> Node,
	combobox: (props: ComboboxProps) -> Node,
	command_palette: (props: CommandPaletteProps) -> Node,
	select: (props: SelectProps) -> Node,
	stepper: (props: StepperProps) -> Node,
	disclosure: (props: DisclosureProps) -> Node,
	accordion: (props: AccordionProps) -> Node,
	-- An accordion's section: `ui.section { id = "a", title = "Shipping", … }`.
	section: (props: SectionProps) -> Node,
	-- A line between groups, across (`vertical = true`: down a toolbar).
	separator: (props: SeparatorProps?) -> Node,
	modal: (props: ModalProps) -> Node,
	-- Over the window and non-modal (toasts): no focus, no backdrop; the
	-- pointer passes through but for what it holds, which stays live while
	-- a dialog holds focus. Raised over everything as it opens (`open`, a
	-- state; open unless given).
	layer: (props: LayerProps) -> Node,
	popover: (props: PopoverProps) -> Node,
	menu: (props: PopoverProps) -> Node,
	menu_item: (props: MenuItemProps) -> Node,
	submenu: (props: SubmenuProps) -> Node,
	list: (props: ListProps) -> Node,
	-- `node` while `cond` is truthy, else `otherwise` (or nothing).
	show: (cond: boolean | Readable<unknown> | (() -> unknown), node: Node, otherwise: Node?) -> Node,
	switch: (value: string | Readable<unknown> | (() -> unknown), cases: { [CaseKey]: Node }, default: Node?) -> Node,
	-- A row per item of a list state; `build` gets the item and its index
	-- (from 1). The item is `any`: annotate it to check it,
	-- `function(tab: Tab, i)` (Luau infers a generic callback's parameter
	-- from its uses, which misfires). A row whose item changes is built
	-- again; a `build` taking a third parameter gets the row's item as
	-- state instead, kept current, and its row updates in place.
	each: (list: State<{ any }>, build: (item: any, index: number, row: State<any>) -> Node, opts: EachOptions?) -> Node,
	-- What `build` makes of the value, entering and exiting as it changes;
	-- nil shows nothing. The value is `any`, as `each`'s item is.
	presence: (value: string | Readable<unknown> | (() -> unknown), build: (value: any) -> Node, opts: PresenceOptions?) -> Node,
	-- Where plugins' contributions to `name` go.
	-- A plugin extension point. `context` reaches what is contributed to
	-- it: a contributed component gets it as `props.slot` (one component,
	-- an instance per slot: a panel per pane).
	slot: (name: string, props: SlotProps?) -> Node,
	contribute: (name: string, node: Node) -> (),
	-- The script's (or module's) state for `key`, kept across reloads.
	state: <T>(key: string, initial: T) -> State<T>,
	-- State shared with Rust and every script.
	shared: <T>(key: string, initial: T) -> State<T>,
	-- State saved across runs, in the host's storage (`ScriptHost::app_id`).
	persist: <T>(key: string, initial: T) -> State<T>,
	-- A component instance's own state.
	use_state: <T>(initial: T) -> State<T>,
	computed: <T>(f: () -> T) -> State<T>,
	-- A list state's items in order, kept in order as it changes: a write
	-- re-places the items it wrote (`by` and `filter` called for those
	-- alone), and a new spec (a spec function's reads changed) sorts it all
	-- again. `by` is a field's name (read in Rust) or a function of an
	-- item; `filter` keeps the items it is true for. (Annotate the view's
	-- type: `local view: State<{ Row }> = ui.sorted(rows, …)`.)
	sorted: (list: State<any>, spec: SortedSpec<any> | (() -> SortedSpec<any>)) -> State<any>,
	-- A component's styles by variant (shadcn's `cva`): `base`, then the
	-- chosen option of each axis in `variants` (by name), then the matching
	-- `compound` rules, then the element's own properties; each replaces
	-- what it covers (`p` an earlier `px`). Returns a recipe: call it with
	-- the options (names, or states or functions choosing them) and give
	-- the result to an element as `variants = …`.
	variants: <D>(definition: D) -> (choice: VariantChoice<D, Readable<any> | (() -> any)>) -> Variants,
	-- A style value the design lint lets through, with the reason.
	raw: <T>(value: T, reason: string) -> T,
	-- Markdown parsed into blocks (`t`: p, h, code, ul, ol, quote, hr,
	-- table, img; runs `{ x, b?, i?, c?, s?, a?, f? }`). With `key` (a
	-- reply's id), text growing under it parses only its new end, and its
	-- earlier blocks are the same tables as before.
	markdown: (text: string, opts: MarkdownOptions?) -> { MarkdownBlock },
	effect: (f: () -> ...unknown) -> (),
	-- One update for many writes; returns what `f` returns.
	batch: <R...>(f: () -> R...) -> R...,
	-- Runs once it is mounted: a component instance, or (outside one) the
	-- app's interface, again after each reload.
	on_mount: (f: () -> ...unknown) -> (),
	ref: () -> Ref,
	-- `Card { title = "…", child }`: `build(props)` runs as each instance
	-- mounts; children also arrive as `props.children`. Annotate `props`
	-- with your own type (and `[number]: Child` if it takes children).
	component: <P>(build: (props: P) -> Node) -> (props: P) -> Node,
	-- A host action (`ScriptHost::action`) as a handler. `arg` is plain
	-- data (what state holds: no functions); Luau cannot say so of a
	-- table typed as a record, so it is `unknown` here.
	action: (name: string, arg: unknown?) -> Action,
	-- Call a host action now.
	invoke: (name: string, arg: unknown?) -> (),
	-- JSON's null, as a value that keeps its key in a table (nil drops it):
	-- `{ caps = ui.null }` reaches Rust as `{"caps": null}`.
	null: any,
	-- Runs `f` when what is being built goes away: a component unmounting,
	-- an effect running again, the script reloading.
	on_cleanup: (f: () -> ()) -> (),
	-- Milliseconds since the epoch, to the microsecond (`os.time` counts
	-- whole seconds). Not tracked.
	now: () -> number,
	-- `t` (or a new table) marked a JSON object: empty, it sends `{}`, not
	-- `[]` (a Lua table can't tell). An empty object from the host comes
	-- marked, so it goes back as one.
	object: <T>(t: T?) -> T,
	-- A host function's value, now (`ScriptHost::function`): a read of data the
	-- host keeps, so large data is never copied into the script whole.
	call_sync: (name: string, arg: unknown?) -> any,
	-- Call a host action for its result (`ScriptHost::async_action`; a
	-- plain action settles at once with nil): a promise of what it returns.
	call: (name: string, arg: unknown?) -> Promise,
	-- Run `f(...)` as a task: `p:await()` inside it waits for a promise
	-- without blocking anything else.
	task: <A...>(f: (A...) -> (), A...) -> (),
	-- What `load` returns, loaded in a task (it may await), as states:
	-- `data`, `loading`, `error` and `status`. It loads again on `reload()`
	-- and when a state it read before its first await changes; only the
	-- last load's answer is kept. `initial`: `data` until one answers.
	resource: <T>(load: () -> T, opts: { initial: T }?) -> Resource<T>,
	-- A form's fields, checked as they are typed, and its submit with its
	-- pending and failure states; controls bind with `field`.
	form: (spec: FormSpec) -> Form,
	-- An HTTP request through the host's transport (`ScriptHost::fetcher`):
	-- a promise of a `Response`. A plugin needs the host's grant.
	fetch: (url: string, opts: FetchOptions?) -> Promise<Response>,
	-- The clipboard's text (nil if it holds none), and putting text on it.
	clipboard: {
		read: () -> string?,
		write: (text: string) -> boolean,
	},
	-- The files the user picks: a promise of a list of `File` (empty if
	-- they cancel).
	open_files: (opts: FilePickerOptions?) -> Promise<{ File }>,
	-- Where the user chooses to save: a promise of a `File`, or nil.
	save_file: (opts: FilePickerOptions?) -> Promise<File?>,
	-- A file the user picked, read as text off the main thread: a promise.
	read_file: (file: File | string) -> Promise<string>,
	-- Write `text` to a file the user picked (beside it, then over it).
	write_file: (file: File | string, text: string) -> Promise<nil>,
	-- What the app was launched with after its script (`zgx-run app/
	-- notes.txt` gives `{ "notes.txt" }`).
	args: { string },
	-- Files and URLs the system asks the app to open (a file opened with it,
	-- a link to it, a second launch's), the launch's included: the files
	-- are the user's choice, as a dialog's are (`ui.read_file` takes them).
	-- The app's only: a plugin's is an error.
	on_open: (handler: (e: { files: { File }, urls: { string } }) -> ()) -> (),
	-- `T` comes from the state's `set`, so a literal fits a state of a
	-- union (`ui.set(density, "compact")` for `"comfortable" | "compact"`)
	-- and anything else is still an error.
	set: <T>(state: { read set: (self: any, value: T) -> () }, value: T) -> Action,
	toggle: (state: State<boolean>) -> Action,
	add: (state: State<number>, by: number?) -> Action,
	-- The state's truthiness (or number) drives a 0-to-1 scalar, or
	-- `from` to `to`. Durations are milliseconds, as in `anime`.
	tween: (state: State<any>, opts: TweenOptions?) -> Binding,
	spring: (state: State<any>, opts: SpringOptions?) -> Binding,
	loop: (opts: LoopOptions?) -> Binding,
	-- White pixels with the alpha `alpha(x, y)` gives (0 to 1), for masks.
	bitmap: (w: number, h: number, alpha: (x: number, y: number) -> number) -> Bitmap,
	-- A WGSL fill: `fn fill(input: FillInput) -> vec4<f32>`, with input.p,
	-- input.size, input.time, input.scale and input.params (four vec4s).
	shader: (source: string, opts: { fps: number? }?) -> Shader,
	-- Sorts `list` in place, stably, by a field of each item (`"price"`) or
	-- a function of it (called once an item); returns it. Use it over
	-- `table.sort` with a comparator for long lists: that calls Luau some
	-- 1.7 million times for 100,000 rows. Nil keys go last.
	sort: <T>(list: { T }, by: string | (item: T) -> any, descending: boolean?) -> { T },
	-- A toast in the window's corner: it goes after `duration` ms (5000;
	-- 7000 a warning; an error stays), or at its ×. Returns its id.
	toast: (opts: ToastOptions) -> number,
	dismiss_toast: (id: number) -> (),
	-- JSON: `encode(value, { pretty = true })` writes what state could hold
	-- (lists as arrays, tables as objects, `ui.null` as null); `decode(text)`
	-- gives tables the script may change, and fails saying where.
	json: {
		encode: (value: any, opts: { pretty: boolean? }?) -> string,
		decode: (text: string) -> any,
	},
	-- `n` formatted, as `Intl.NumberFormat` would: `ui.number(1234.5)` is
	-- "1,234.50", `{ style = "percent" }` takes a fraction (0.05 is "5%"),
	-- `{ notation = "compact" }` gives "1.2K", `{ sign = "always" }` "+3".
	number: (n: number, opts: NumberOptions?) -> string,
	path: () -> Path,
	linear: (x1: number, y1: number, x2: number, y2: number, colors: { read [number]: Color }) -> Brush,
}

---------------------------------------------------------------------------
-- anime: anime.js v4's API.

-- `anime.eases.*()`, `anime.spring {…}`, `anime.createSpring {…}`: an
-- ease, callable as `ease(t)`.
declare extern type AnimeEase with
end
-- What eases take in `ui` (layout, tweens, `each` motion): a name
-- ("outQuint", "inOut(3)", "steps(4)", "cubicBezier(.2,0,0,1)") or an
-- anime ease.
export type Easing = string | AnimeEase
-- anime's own parameters also take functions of progress.
export type Ease = Easing | ((t: number) -> number)
-- `anime.stagger(…)`, as a value, delay or duration.
declare extern type Stagger with
end
-- `anime.utils.round(decimals)` and friends called without a value: a
-- modifier that runs in Rust, callable as `modifier(n)`.
declare extern type AnimeModifier with
end
export type Modifier = AnimeModifier | ((value: number) -> number)
-- `anime.onScroll(…)`, as an animation's `autoplay`.
declare extern type ScrollObserver with
end
-- One axis of `anime.svg.createMotionPath(d)`.
declare extern type MotionPathValue with
end

-- A function value: `(target, index, total)`. Targets are elements or the
-- script's tables, so `any`.
export type AnimFunction = (target: any, index: number, total: number) -> (number | string)
-- Numbers, strings with units ("10px", "1turn", "+=10"), colors, staggers,
-- functions of (target, index, total), keyframe lists (`{ 0, 240 }`) and
-- tween tables.
export type AnimValue = number | string | Stagger | MotionPathValue | AnimFunction | { read [number]: AnimValue } | Tween
export type AnimNumber = number | Stagger | AnimFunction
export type Composition = "replace" | "none" | "add"
export type Tween = {
	to: AnimValue?,
	from: AnimValue?,
	duration: AnimNumber?,
	delay: AnimNumber?,
	ease: Ease?,
	composition: Composition?,
	modifier: Modifier?,
	[number]: never,
}
-- Positions in a timeline: 500, "+=100", "-=100", "<", "<<", "<+=50",
-- "label", "label+=50".
export type Position = number | string
export type Callback = (anim: Animation) -> ...unknown

export type Timing = {
	-- Names the animation (`anim.id`).
	id: string?,
	duration: AnimNumber?,
	delay: AnimNumber?,
	ease: Ease?,
	composition: Composition?,
	modifier: Modifier?,
	-- A count, or true for ever.
	loop: (number | boolean)?,
	-- Each timing name also takes its snake_case spelling (`on_complete`).
	loopDelay: number?,
	loop_delay: number?,
	alternate: boolean?,
	reversed: boolean?,
	autoplay: (boolean | ScrollObserver)?,
	frameRate: number?,
	frame_rate: number?,
	playbackRate: number?,
	playback_rate: number?,
	playbackEase: Ease?,
	playback_ease: Ease?,
	-- A list of property sets, or `{ ["0%"] = …, ["50%"] = … }`.
	keyframes: ({ ElementKeys } | { [string]: ElementKeys })?,
	onBegin: Callback?,
	on_begin: Callback?,
	onBeforeUpdate: Callback?,
	on_before_update: Callback?,
	onUpdate: Callback?,
	on_update: Callback?,
	onRender: Callback?,
	on_render: Callback?,
	onLoop: Callback?,
	on_loop: Callback?,
	onPause: Callback?,
	on_pause: Callback?,
	onComplete: Callback?,
	on_complete: Callback?,
	-- No array part: an unknown key is an error.
	[number]: never,
}

-- An element's animatable properties: a misspelled one (`bgColor`) is an
-- error here rather than at the first frame.
export type ElementKeys = {
	aspect: AnimValue?,
	backdrop_blur: AnimValue?,
	background: AnimValue?,
	backgroundColor: AnimValue?,
	background_color: AnimValue?,
	basis: AnimValue?,
	bg: AnimValue?,
	blur: AnimValue?,
	border: AnimValue?,
	borderBottomLeftRadius: AnimValue?,
	borderBottomRightRadius: AnimValue?,
	borderColor: AnimValue?,
	borderRadius: AnimValue?,
	borderTopLeftRadius: AnimValue?,
	borderTopRightRadius: AnimValue?,
	borderWidth: AnimValue?,
	border_color: AnimValue?,
	border_dash: AnimValue?,
	border_radius: AnimValue?,
	border_width: AnimValue?,
	bottom: AnimValue?,
	brightness: AnimValue?,
	contrast: AnimValue?,
	saturate: AnimValue?,
	shaderTime: AnimValue?,
	shader_time: AnimValue?,
	shaderParam0: AnimValue?,
	shaderParam1: AnimValue?,
	shaderParam2: AnimValue?,
	shaderParam3: AnimValue?,
	shaderParam4: AnimValue?,
	shaderParam5: AnimValue?,
	shaderParam6: AnimValue?,
	shaderParam7: AnimValue?,
	shaderParam8: AnimValue?,
	shaderParam9: AnimValue?,
	shaderParam10: AnimValue?,
	shaderParam11: AnimValue?,
	shaderParam12: AnimValue?,
	shaderParam13: AnimValue?,
	shaderParam14: AnimValue?,
	shaderParam15: AnimValue?,
	clipBottom: AnimValue?,
	clipLeft: AnimValue?,
	clipRight: AnimValue?,
	clipTop: AnimValue?,
	clip_bottom: AnimValue?,
	clip_left: AnimValue?,
	clip_right: AnimValue?,
	clip_top: AnimValue?,
	col_span: AnimValue?,
	col_start: AnimValue?,
	color: AnimValue?,
	cols: AnimValue?,
	fade: AnimValue?,
	fade_x: AnimValue?,
	filter: AnimValue?,
	fontSize: AnimValue?,
	fontWeight: AnimValue?,
	font_size: AnimValue?,
	gap: AnimValue?,
	gap_x: AnimValue?,
	gap_y: AnimValue?,
	grow: AnimValue?,
	h: AnimValue?,
	height: AnimValue?,
	left: AnimValue?,
	letter_spacing: AnimValue?,
	line_height: AnimValue?,
	lines: AnimValue?,
	m: AnimValue?,
	margin: AnimValue?,
	marginBottom: AnimValue?,
	marginLeft: AnimValue?,
	marginRight: AnimValue?,
	marginTop: AnimValue?,
	maskX: AnimValue?,
	maskY: AnimValue?,
	mask_x: AnimValue?,
	mask_y: AnimValue?,
	maxHeight: AnimValue?,
	maxWidth: AnimValue?,
	max_h: AnimValue?,
	max_w: AnimValue?,
	mb: AnimValue?,
	minHeight: AnimValue?,
	minWidth: AnimValue?,
	min_h: AnimValue?,
	min_w: AnimValue?,
	ml: AnimValue?,
	mr: AnimValue?,
	mt: AnimValue?,
	mx: AnimValue?,
	my: AnimValue?,
	opacity: AnimValue?,
	origin: AnimValue?,
	p: AnimValue?,
	padding: AnimValue?,
	paddingBottom: AnimValue?,
	paddingLeft: AnimValue?,
	paddingRight: AnimValue?,
	paddingTop: AnimValue?,
	pb: AnimValue?,
	pl: AnimValue?,
	pr: AnimValue?,
	pt: AnimValue?,
	px: AnimValue?,
	py: AnimValue?,
	right: AnimValue?,
	rotate: AnimValue?,
	rotateZ: AnimValue?,
	rotate_z: AnimValue?,
	rotation: AnimValue?,
	rounded: AnimValue?,
	rounded_bl: AnimValue?,
	rounded_br: AnimValue?,
	rounded_tl: AnimValue?,
	rounded_tr: AnimValue?,
	row_span: AnimValue?,
	row_start: AnimValue?,
	rows: AnimValue?,
	scale: AnimValue?,
	scaleX: AnimValue?,
	scaleY: AnimValue?,
	scale_x: AnimValue?,
	scale_y: AnimValue?,
	shrink: AnimValue?,
	size: AnimValue?,
	skew: AnimValue?,
	skewX: AnimValue?,
	skewY: AnimValue?,
	skew_x: AnimValue?,
	skew_y: AnimValue?,
	text_size: AnimValue?,
	tint: AnimValue?,
	top: AnimValue?,
	translateX: AnimValue?,
	translateY: AnimValue?,
	translate_x: AnimValue?,
	translate_y: AnimValue?,
	w: AnimValue?,
	weight: AnimValue?,
	width: AnimValue?,
	x: AnimValue?,
	y: AnimValue?,
	[number]: never,
}
-- An element animation's parameters: its properties and `Timing`'s (the
-- two written out: a type function cannot take what refers to Animation).
export type ElementParams = {
	aspect: AnimValue?,
	backdrop_blur: AnimValue?,
	background: AnimValue?,
	backgroundColor: AnimValue?,
	background_color: AnimValue?,
	basis: AnimValue?,
	bg: AnimValue?,
	blur: AnimValue?,
	border: AnimValue?,
	borderBottomLeftRadius: AnimValue?,
	borderBottomRightRadius: AnimValue?,
	borderColor: AnimValue?,
	borderRadius: AnimValue?,
	borderTopLeftRadius: AnimValue?,
	borderTopRightRadius: AnimValue?,
	borderWidth: AnimValue?,
	border_color: AnimValue?,
	border_dash: AnimValue?,
	border_radius: AnimValue?,
	border_width: AnimValue?,
	bottom: AnimValue?,
	brightness: AnimValue?,
	contrast: AnimValue?,
	saturate: AnimValue?,
	shaderTime: AnimValue?,
	shader_time: AnimValue?,
	shaderParam0: AnimValue?,
	shaderParam1: AnimValue?,
	shaderParam2: AnimValue?,
	shaderParam3: AnimValue?,
	shaderParam4: AnimValue?,
	shaderParam5: AnimValue?,
	shaderParam6: AnimValue?,
	shaderParam7: AnimValue?,
	shaderParam8: AnimValue?,
	shaderParam9: AnimValue?,
	shaderParam10: AnimValue?,
	shaderParam11: AnimValue?,
	shaderParam12: AnimValue?,
	shaderParam13: AnimValue?,
	shaderParam14: AnimValue?,
	shaderParam15: AnimValue?,
	clipBottom: AnimValue?,
	clipLeft: AnimValue?,
	clipRight: AnimValue?,
	clipTop: AnimValue?,
	clip_bottom: AnimValue?,
	clip_left: AnimValue?,
	clip_right: AnimValue?,
	clip_top: AnimValue?,
	col_span: AnimValue?,
	col_start: AnimValue?,
	color: AnimValue?,
	cols: AnimValue?,
	fade: AnimValue?,
	fade_x: AnimValue?,
	filter: AnimValue?,
	fontSize: AnimValue?,
	fontWeight: AnimValue?,
	font_size: AnimValue?,
	gap: AnimValue?,
	gap_x: AnimValue?,
	gap_y: AnimValue?,
	grow: AnimValue?,
	h: AnimValue?,
	height: AnimValue?,
	left: AnimValue?,
	letter_spacing: AnimValue?,
	line_height: AnimValue?,
	lines: AnimValue?,
	m: AnimValue?,
	margin: AnimValue?,
	marginBottom: AnimValue?,
	marginLeft: AnimValue?,
	marginRight: AnimValue?,
	marginTop: AnimValue?,
	maskX: AnimValue?,
	maskY: AnimValue?,
	mask_x: AnimValue?,
	mask_y: AnimValue?,
	maxHeight: AnimValue?,
	maxWidth: AnimValue?,
	max_h: AnimValue?,
	max_w: AnimValue?,
	mb: AnimValue?,
	minHeight: AnimValue?,
	minWidth: AnimValue?,
	min_h: AnimValue?,
	min_w: AnimValue?,
	ml: AnimValue?,
	mr: AnimValue?,
	mt: AnimValue?,
	mx: AnimValue?,
	my: AnimValue?,
	opacity: AnimValue?,
	origin: AnimValue?,
	p: AnimValue?,
	padding: AnimValue?,
	paddingBottom: AnimValue?,
	paddingLeft: AnimValue?,
	paddingRight: AnimValue?,
	paddingTop: AnimValue?,
	pb: AnimValue?,
	pl: AnimValue?,
	pr: AnimValue?,
	pt: AnimValue?,
	px: AnimValue?,
	py: AnimValue?,
	right: AnimValue?,
	rotate: AnimValue?,
	rotateZ: AnimValue?,
	rotate_z: AnimValue?,
	rotation: AnimValue?,
	rounded: AnimValue?,
	rounded_bl: AnimValue?,
	rounded_br: AnimValue?,
	rounded_tl: AnimValue?,
	rounded_tr: AnimValue?,
	row_span: AnimValue?,
	row_start: AnimValue?,
	rows: AnimValue?,
	scale: AnimValue?,
	scaleX: AnimValue?,
	scaleY: AnimValue?,
	scale_x: AnimValue?,
	scale_y: AnimValue?,
	shrink: AnimValue?,
	size: AnimValue?,
	skew: AnimValue?,
	skewX: AnimValue?,
	skewY: AnimValue?,
	skew_x: AnimValue?,
	skew_y: AnimValue?,
	text_size: AnimValue?,
	tint: AnimValue?,
	top: AnimValue?,
	translateX: AnimValue?,
	translateY: AnimValue?,
	translate_x: AnimValue?,
	translate_y: AnimValue?,
	w: AnimValue?,
	weight: AnimValue?,
	width: AnimValue?,
	x: AnimValue?,
	y: AnimValue?,
	-- Names the animation (`anim.id`).
	id: string?,
	duration: AnimNumber?,
	delay: AnimNumber?,
	ease: Ease?,
	composition: Composition?,
	modifier: Modifier?,
	-- A count, or true for ever.
	loop: (number | boolean)?,
	loopDelay: number?,
	loop_delay: number?,
	alternate: boolean?,
	reversed: boolean?,
	autoplay: (boolean | ScrollObserver)?,
	frameRate: number?,
	frame_rate: number?,
	playbackRate: number?,
	playback_rate: number?,
	playbackEase: Ease?,
	playback_ease: Ease?,
	-- A list of property sets, or `{ ["0%"] = …, ["50%"] = … }`.
	keyframes: ({ ElementKeys } | { [string]: ElementKeys })?,
	onBegin: Callback?,
	on_begin: Callback?,
	onBeforeUpdate: Callback?,
	on_before_update: Callback?,
	onUpdate: Callback?,
	on_update: Callback?,
	onRender: Callback?,
	on_render: Callback?,
	onLoop: Callback?,
	on_loop: Callback?,
	onPause: Callback?,
	on_pause: Callback?,
	onComplete: Callback?,
	on_complete: Callback?,
	[number]: never,
}
-- A state or plain table animates any of its fields.
export type TableParams = {
	-- Names the animation (`anim.id`).
	id: string?,
	duration: AnimNumber?,
	delay: AnimNumber?,
	ease: Ease?,
	composition: Composition?,
	modifier: Modifier?,
	-- A count, or true for ever.
	loop: (number | boolean)?,
	-- Each timing name also takes its snake_case spelling (`on_complete`).
	loopDelay: number?,
	loop_delay: number?,
	alternate: boolean?,
	reversed: boolean?,
	autoplay: (boolean | ScrollObserver)?,
	frameRate: number?,
	frame_rate: number?,
	playbackRate: number?,
	playback_rate: number?,
	playbackEase: Ease?,
	playback_ease: Ease?,
	-- A list of property sets, or `{ ["0%"] = …, ["50%"] = … }`.
	keyframes: ({ ElementKeys } | { [string]: ElementKeys })?,
	onBegin: Callback?,
	on_begin: Callback?,
	onBeforeUpdate: Callback?,
	on_before_update: Callback?,
	onUpdate: Callback?,
	on_update: Callback?,
	onRender: Callback?,
	on_render: Callback?,
	onLoop: Callback?,
	on_loop: Callback?,
	onPause: Callback?,
	on_pause: Callback?,
	onComplete: Callback?,
	on_complete: Callback?,
	[string]: AnimValue,
}
-- Selectors ("#id", ".class", "button", "button.primary", comma lists),
-- elements, refs, or lists of them.
export type ElementTargets = string | Element | Ref | { read [number]: Element | Ref | string }
export type TimelineParams = {
	-- Names the animation (`anim.id`).
	id: string?,
	duration: AnimNumber?,
	delay: AnimNumber?,
	ease: Ease?,
	composition: Composition?,
	modifier: Modifier?,
	-- A count, or true for ever.
	loop: (number | boolean)?,
	-- Each timing name also takes its snake_case spelling (`on_complete`).
	loopDelay: number?,
	loop_delay: number?,
	alternate: boolean?,
	reversed: boolean?,
	autoplay: (boolean | ScrollObserver)?,
	frameRate: number?,
	frame_rate: number?,
	playbackRate: number?,
	playback_rate: number?,
	playbackEase: Ease?,
	playback_ease: Ease?,
	-- A list of property sets, or `{ ["0%"] = …, ["50%"] = … }`.
	keyframes: ({ ElementKeys } | { [string]: ElementKeys })?,
	onBegin: Callback?,
	on_begin: Callback?,
	onBeforeUpdate: Callback?,
	on_before_update: Callback?,
	onUpdate: Callback?,
	on_update: Callback?,
	onRender: Callback?,
	on_render: Callback?,
	onLoop: Callback?,
	on_loop: Callback?,
	onPause: Callback?,
	on_pause: Callback?,
	onComplete: Callback?,
	on_complete: Callback?,
	-- No array part: an unknown key is an error.
	[number]: never,
	defaults: Merge<Timing, { [string]: AnimValue }>?,
}

-- An animation, timer or timeline. Methods work as `a:play()` and
-- `a.play()`, and return it.
declare extern type Animation with
	-- The `id` parameter, or a number.
	id: number | string
	-- Settable: seeks.
	currentTime: number
	-- Settable: seeks to that fraction.
	progress: number
	iterationProgress: number
	iterationCurrentTime: number
	currentIteration: number
	duration: number
	iterationDuration: number
	-- Settable: pauses or resumes.
	paused: boolean
	began: boolean
	completed: boolean
	reversed: boolean
	-- Settable.
	speed: number
	playbackRate: number
	labels: { [string]: number }
	function play(self): Animation
	function pause(self): Animation
	function resume(self): Animation
	function restart(self): Animation
	function reverse(self): Animation
	function alternate(self): Animation
	function seek(self, time: number): Animation
	function complete(self): Animation
	function cancel(self): Animation
	function revert(self): Animation
	function reset(self): Animation
	function stretch(self, duration: number): Animation
	function refresh(self): Animation
	-- Builds it now (its targets resolve when it first plays otherwise).
	function init(self): Animation
	-- anime.js `then` (a Luau keyword; `a["then"](fn)` works too).
	function done(self, f: (anim: Animation) -> ...unknown): Animation
	-- Timelines: `add(targets, params, position)`, or `add(timerParams,
	-- position)`.
	function add(self, targets: ElementTargets | Timing, params: (ElementParams | Position)?, position: Position?): Animation
	function set(self, targets: ElementTargets, params: ElementKeys, position: Position?): Animation
	function call(self, f: () -> ...unknown, position: Position?): Animation
	function label(self, name: string, position: Position?): Animation
	function sync(self, animation: Animation, position: Position?): Animation
	function remove(self, targets: ElementTargets): Animation
end

export type Axis = boolean | { snap: (number | { number })? }
export type DraggableCallback = (draggable: Draggable) -> ...unknown
export type DraggableParams = {
	x: Axis?,
	y: Axis?,
	-- A step or points, for both axes.
	snap: (number | { number })?,
	-- A selector, element or ref, or `{ top, right, bottom, left }`.
	container: (ElementTargets | Sides)?,
	containerPadding: (number | { number })?,
	containerFriction: number?,
	dragSpeed: number?,
	velocityMultiplier: number?,
	minVelocity: number?,
	maxVelocity: number?,
	releaseMass: number?,
	releaseStiffness: number?,
	releaseDamping: number?,
	-- false keeps the cursor.
	cursor: boolean?,
	onGrab: DraggableCallback?,
	onDrag: DraggableCallback?,
	onUpdate: DraggableCallback?,
	onRelease: DraggableCallback?,
	onSnap: DraggableCallback?,
	onSettle: DraggableCallback?,
	[number]: never,
}
declare extern type Draggable with
	x: number
	y: number
	isDragging: boolean
	enabled: boolean
	velocity: number
	function enable(self): ()
	function disable(self): ()
	function reset(self): ()
	function revert(self): ()
	function stop(self): ()
	function setX(self, x: number): ()
	function setY(self, y: number): ()
end

export type ScrollParams = {
	-- The nearest scroll ancestor by default.
	container: ElementTargets?,
	target: ElementTargets?,
	-- "<container edge> <target edge>": start, center, end, %, points,
	-- +=/-=. "end start" and "start end" by default.
	enter: string?,
	leave: string?,
	-- true, a smoothing amount (0 to 1), or an ease name.
	sync: (boolean | number | string)?,
	-- Plays again each time it enters (true by default).
	["repeat"]: boolean?,
	onEnter: Callback?,
	onLeave: Callback?,
	onUpdate: Callback?,
	on_update: Callback?,
	onSync: Callback?,
	[number]: never,
}
export type StaggerParams = {
	start: number?,
	-- "first", "center", "last", "random", or an index (from 0).
	from: ("first" | "center" | "last" | "random" | number)?,
	reversed: boolean?,
	ease: Ease?,
	-- `{ columns, rows }`.
	grid: { number }?,
	axis: ("x" | "y")?,
	-- The index a target staggers by: `(target, index, total)`.
	use: AnimFunction?,
	total: number?,
	modifier: Modifier?,
	[number]: never,
}
-- `createAnimatable(targets, { x = 400, y = { duration = 600 } })`: each
-- property is a function: `a.x(value, duration?, ease?)` animates to
-- `value`, `a.x()` reads it. The names are the script's.
export type Animatable = {
	revert: () -> (),
	cancel: () -> (),
	[string]: (value: (number | string)?, duration: number?, ease: Ease?) -> (number | string)?,
}
declare extern type Scope with
	-- Runs `f(scope)`; what it creates takes the scope's defaults and
	-- reverts with it.
	function add(self, f: (scope: Scope) -> ...unknown): Scope
	function revert(self): ()
end
export type EaseMaker = (...number) -> AnimeEase

-- Besides elements, animations and utils target a state's or a plain
-- table's fields: the script's own data, so `{ [string]: any }`.
declare anime: {
	animate: ((targets: ElementTargets, params: ElementParams) -> Animation)
		& ((targets: State<any> | { [string]: any }, params: TableParams) -> Animation),
	createTimeline: (params: TimelineParams?) -> Animation,
	-- anime.js v3's name.
	timeline: (params: TimelineParams?) -> Animation,
	-- Endless unless given a duration.
	createTimer: (params: Timing?) -> Animation,
	createAnimatable: (targets: ElementTargets | State<any> | { [string]: any }, params: { [string]: number | Ease | Modifier | Composition | Tween }) -> Animatable,
	createDraggable: (target: ElementTargets, params: DraggableParams?) -> Draggable,
	onScroll: (params: ScrollParams?) -> ScrollObserver,
	-- A value (5, "1rem") or a range (`{ -100, 100 }`).
	stagger: (value: number | string | { read [number]: number | string }, params: StaggerParams?) -> Stagger,
	createSpring: (params: { mass: number?, stiffness: number?, damping: number?, velocity: number? }?) -> AnimeEase,
	-- A spring of that perceived bounce, settling in `duration` ms.
	spring: (params: { bounce: number?, duration: number? }?) -> AnimeEase,
	createScope: (params: { defaults: Merge<Timing, { [string]: AnimValue }>? }?) -> Scope,
	eases: {
		linear: EaseMaker,
		none: EaseMaker,
		["in"]: EaseMaker,
		out: EaseMaker,
		inOut: EaseMaker,
		outIn: EaseMaker,
		inQuad: EaseMaker,
		outQuad: EaseMaker,
		inOutQuad: EaseMaker,
		outInQuad: EaseMaker,
		inCubic: EaseMaker,
		outCubic: EaseMaker,
		inOutCubic: EaseMaker,
		outInCubic: EaseMaker,
		inQuart: EaseMaker,
		outQuart: EaseMaker,
		inOutQuart: EaseMaker,
		outInQuart: EaseMaker,
		inQuint: EaseMaker,
		outQuint: EaseMaker,
		inOutQuint: EaseMaker,
		outInQuint: EaseMaker,
		inSine: EaseMaker,
		outSine: EaseMaker,
		inOutSine: EaseMaker,
		outInSine: EaseMaker,
		inCirc: EaseMaker,
		outCirc: EaseMaker,
		inOutCirc: EaseMaker,
		outInCirc: EaseMaker,
		inExpo: EaseMaker,
		outExpo: EaseMaker,
		inOutExpo: EaseMaker,
		outInExpo: EaseMaker,
		inBounce: EaseMaker,
		outBounce: EaseMaker,
		inOutBounce: EaseMaker,
		outInBounce: EaseMaker,
		inBack: EaseMaker,
		outBack: EaseMaker,
		inOutBack: EaseMaker,
		outInBack: EaseMaker,
		inElastic: EaseMaker,
		outElastic: EaseMaker,
		inOutElastic: EaseMaker,
		outInElastic: EaseMaker,
		steps: EaseMaker,
		cubicBezier: EaseMaker,
		irregular: EaseMaker,
	},
	engine: {
		speed: number,
		playbackRate: number,
		-- nil: every display frame.
		fps: number?,
		timeUnit: "ms" | "s",
		read currentTime: number,
		read paused: boolean,
		pause: () -> (),
		resume: () -> (),
	},
	utils: {
		set: (targets: ElementTargets | State<any> | { [string]: any }, params: { [string]: AnimValue }) -> (),
		-- The current value: a number (angles in degrees) or a string.
		get: (target: ElementTargets | State<any> | { [string]: any }, property: string) -> (number | string)?,
		-- Stops every animation of the targets.
		remove: (targets: ElementTargets | State<any> | { [string]: any }) -> (),
		-- Animated values return to the node's own: all, or the named ones.
		cleanInlineStyles: (targets: ElementTargets | Animation, props: (string | { string })?) -> (),
		-- Without decimals, whole bounds give whole numbers.
		random: (min: number?, max: number?, decimals: number?) -> number,
		randomPick: <T>(items: { T }) -> T?,
		shuffle: <T>(items: { T }) -> { T },
		-- `round(value, decimals)`; `round(decimals)` is a modifier.
		round: ((value: number, decimals: number) -> number) & ((decimals: number) -> AnimeModifier),
		clamp: (value: number, min: number, max: number) -> number,
		snap: (value: number, step: number) -> number,
		wrap: (value: number, min: number, max: number) -> number,
		mapRange: (value: number, inLow: number, inHigh: number, outLow: number, outHigh: number) -> number,
		lerp: (a: number, b: number, t: number) -> number,
		-- Frame-rate independent `lerp`: `dt` in ms.
		damp: (a: number, b: number, dt: number, factor: number) -> number,
		degToRad: (deg: number) -> number,
		radToDeg: (rad: number) -> number,
		roundPad: (value: number, decimals: number) -> string,
		padStart: (value: string | number, length: number, pad: string?) -> string,
		padEnd: (value: string | number, length: number, pad: string?) -> string,
		createSeededRandom: (seed: number?) -> (min: number?, max: number?, decimals: number?) -> number,
	},
	svg: {
		-- Animate `x` (or translateX), `y` and `rotate` with these to follow
		-- the path.
		createMotionPath: (d: string) -> {
			translateX: MotionPathValue,
			translateY: MotionPathValue,
			rotate: MotionPathValue,
			x: MotionPathValue,
			y: MotionPathValue,
			length: number,
		},
		getTotalLength: (d: string) -> number,
	},
	text: {
		-- A row of one text per character (`chars`) or word (`words`), each
		-- with a class to animate; `style` styles each piece, `props` the row.
		split: (text: string, params: {
			words: (boolean | { class: string? })?,
			chars: (boolean | { class: string? })?,
			class: string?,
			style: Props?,
			props: Props?,
		}?) -> Node,
	},
}
