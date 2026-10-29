# Controls: button, input, textarea, checkbox, slider, progress, segmented, stepper, select, combobox, command_palette, calendar, date_picker, time_field

Controls take keyboard focus, show it with an accent ring, and (except `button`) bind a state both ways: the control shows the state, and the user's changes write it. They take any [property](properties.md) and [event](events.md); their defaults below are only defaults.

```luau
ui.button(props: ContainerProps?) -> Node
ui.input(props: InputProps) -> Node
ui.textarea(props: InputProps) -> Node
ui.checkbox(props: CheckboxProps) -> Node
ui.slider(props: SliderProps) -> Node
ui.progress(props: ProgressProps) -> Node
ui.segmented(props: SegmentedProps) -> Node
ui.stepper(props: StepperProps) -> Node
ui.select(props: SelectProps) -> Node
ui.combobox(props: ComboboxProps) -> Node
ui.command_palette(props: CommandPaletteProps) -> Node
ui.calendar(props: CalendarProps) -> Node
ui.date_picker(props: DatePickerProps) -> Node
ui.time_field(props: TimeFieldProps) -> Node
```

Every control dims while `disabled` (to 45% opacity, as native controls do) unless it says how it looks then.

## button

A focusable box whose children lie in a row; `on_click` runs on a click, Enter (as it goes down) or Space (as it comes up), as a browser's button does.

| Option | Type | Default | |
| --- | --- | --- | --- |
| children | `Child` | none | its face: text, icons, anything |
| `on_click` | `Handler \| Action` | none | |
| `disabled` | `boolean \| Readable<boolean> \| (() -> boolean) \| Styles` | not disabled | a condition, or the styles shown while disabled |
| `px`, `py` | `Length` | `12`, `8` | |
| `gap` | `number` | `6` | |
| `items` | `Align` | `"center"` | |
| `bg` | `Color` | `"$button.bg"` | `hover` defaults to `"$button.hover"` |
| `rounded` | `number` | `6` | |
| `focus` | `Styles` | an accent ring | `focus = { border = 0 }` removes it |
| `variant` | `"primary"`, `"ghost"`, `"danger"`, `"default"` | `"default"` | how it looks where its own styles don't say: `primary` the accent (`$accent`, `$accent.text`, `$accent.hover` under the pointer), `ghost` no background until hovered (`$hover`), `danger` `$danger` |

## input and textarea

A text editor bound to a string state: selection, undo, clipboard and IME included. `input` is one line (pasted line breaks are removed); `textarea` keeps them.

| Option | Type | Default | |
| --- | --- | --- | --- |
| `value` | `State<string>`, or `() -> string \| number` | required | the text, both ways; or, controlled as React's inputs are, a function it shows, edits going out by `on_change` and `on_commit`. Focused, it keeps what is being typed (the function's answer to an edit, a number formatted, waits), and shows a change from elsewhere at once (a send clearing the draft); left, it shows the function's text again |
| `on_change` | `(text: string) -> ()` | none | a controlled input's every edit |
| `on_commit` | `(text: string) -> ()` | none | a controlled input's text on Enter, or on leaving it changed: where an inspector's field applies, and an undo step records |
| `label` | `string` | `""` | its accessible name (not drawn) |
| `placeholder` | `Reactive<string>` | none | shown while the value is empty, never part of it |
| `placeholder_color` | `Reactive<Color>` | `"#8a9ab0"` | |
| `caret_color` | `Reactive<Color>` | the text's colour | the caret's colour; a `"$token"` follows the theme |
| `caret_width` | `number` | the app's (`1`, or the system's text-cursor width) | the caret's width in points |
| `select_on_focus` | `boolean` | `false` | focusing selects all the text (an address bar) |
| `inputmode` | `"text" \| "decimal" \| "numeric" \| "tel" \| "search" \| "email" \| "url" \| "none"` | `"text"` | what kind of text it takes (HTML's `inputmode`): which touch keyboard Windows shows (digits, `@`, `.com`) and what a Wayland input method offers; `"none"` shows no virtual keyboard, for a field with its own keypad. A hint, never a check of what is typed |
| `spellcheck` | `boolean \| Readable<boolean> \| (() -> boolean)` | `false` | check the spelling as it is typed (HTML's `spellcheck`): misspelled words get a dotted line (not the word being typed), and the context menu offers the system checker's guesses, Learn Spelling and Ignore Spelling. Never in a password field |
| `multiline` | `boolean` | `false` (`textarea`: always) | `input` with several lines |
| `w`, `h` | `Length` | `240` wide (an intrinsic width), `36` high (`textarea`: `156`) | |
| `px`, `py` | `Length` | `8`, `6` | |
| `bg`, `rounded` | | `"$input.bg"`, `"$radius.md"` | |
| `shrink` | `number` | `1` | a row too narrow for it shrinks it, to nothing if need be (it scrolls its text); `shrink = 0` holds it |
| `wrap` | `boolean` | `false` | `textarea`: wrap long lines rather than scroll them |
| `line_numbers` | `boolean` | `false` | `textarea`: number the lines in a gutter at the left, the caret's brighter (a code editor); only the lines in view are built |
| `highlight` | `(line, state) -> (spans, state)` | none | `textarea`: style each line (a code editor's syntax, a Markdown composer's emphasis): spans `{ from, to, color }` in bytes from 1 (as `string.find` gives them), with any of `weight`, `italic`, `strike` and `bg` by name ([styled spans](#styled-spans)), and what the line leaves the next (a block comment running on; `0` before the first). Asked for the lines in view, and again only for one that changes or whose line before leaves another state; it must depend on its arguments alone |
| `decorations` | state or `() -> { { from, to, color? } }` | none | `textarea`: ranges of the whole text marked behind it (a search's matches), in bytes from 1; only those in view are drawn |
| `find` | `{ query, case?, current?, limit?, color?, current_color? }` | none | `textarea`: a search's matches marked behind the text, found by the editor where the text is held (no list made in the script); `current` (from 1) shows in `current_color` (`$warning`), the rest in `color` (`$selection`); `case = false` folds ASCII case |
| `current_line` | `Reactive<Color>` | none | `textarea`: the colour behind the caret's row |
| `tab` | `string` | none | `textarea`: what Tab inserts (`"    "`) instead of moving focus; with lines selected it indents each, Shift+Tab outdents, one step of undo |
| `on_caret` | `(line, column, anchor, focus) -> ()` | none | `textarea`: where the caret is whenever it moves (a status bar): line and column from 1, the selection in bytes from 0 |

Its element (`on_mount`, a [ref](state.md#refs)) selects text: `el:select_all()`, `el:set_selection(a, b)`, `local a, b = el:selection()`, in characters, or in bytes from 0 as `string.find` counts them, `el:set_selection_bytes(a, b)` and `el:selection_bytes()` (a selection set so scrolls into view, focused or not); `el:replace_selection(text)` types `text` over the selection, one step of undo ([events](events.md#elements)). Each sees the text a handler just set, and leaves its state holding what it changed.

### Styled spans

A `highlight` span is `{ from, to, color }` (`color` may be left out), with these by name:

| Field | Type | |
| --- | --- | --- |
| `weight` | `number` | its font weight (`600`, `700`) |
| `italic` | `boolean` | italic |
| `strike` | `boolean` | a line through it, in its colour |
| `bg` | `Color` | a wash behind it (inline code) |

Where spans overlap, each of `color`, `weight` and `bg` is the first's (by `from`) that gives it, and `italic` and `strike` hold if any gives them: a code span inside bold text keeps the bold weight and shows its own colour, and delimiters greyed inside a bold span stay bold. A line whose spans give a `weight` or `italic` is shaped with them, so its width, wrapping, carets, clicks and selection follow what shows; other lines (colours, strikes and washes only) are shaped as plain text, as before.

```lua
local draft = ui.state("draft", "Some **bold** and `code`")
return ui.textarea {
	value = draft,
	wrap = true,
	highlight = function(line, state)
		local spans = {}
		local function each(pattern: string, style: (a: number, b: number) -> ())
			local at = 1
			while true do
				local a, b = string.find(line, pattern, at)
				if not a or not b then
					break
				end
				style(a, b)
				at = b + 1
			end
		end
		each("%*%*.-%*%*", function(a, b)
			table.insert(spans, { a, b, weight = 600 })
			table.insert(spans, { a, a + 1, "$text.tertiary" })
			table.insert(spans, { b - 1, b, "$text.tertiary" })
		end)
		each("`[^`]+`", function(a, b)
			table.insert(spans, { a, b, "$accent", bg = "$input.bg" })
		end)
		return spans, state
	end,
}
```

A long text (more than 64 KiB, or 2,000 lines), or one with `highlight`, is laid out by its lines: the lines in view are shaped and drawn, a line keeps its shape until it changes, and where each line starts is a few bytes a line, so a file of 100,000 lines opens in about 20 ms and a keystroke costs about a millisecond ([the code editor example](../../crates/zgx-script/examples/code_editor), `code_editor_bench`). A string over 64 KiB in state is held in chunks of a few kilobytes: the state, the textarea bound to it, its layout and its accessibility value share one copy, and a keystroke's new value shares every chunk but the one it changed (a script reading the state still reads a string). Assistive technology reads such a text a page at a time, as browsers' code editors give it: the lines in view and a page each side (or around the caret, scrolled away), a text run a line, so a keystroke sends the runs of the line it changed.

## checkbox

A toggle drawn as the platform draws one, a rounded 16-point box beside its label, bound to a boolean state: outlined while off; filled with the accent while on, its checkmark drawing on; a dash while `mixed`. A click on either, or Space, toggles it; Enter goes on to the form (its default action), as a browser's checkbox and AppKit's do. Its colours are the theme's `$accent`, `$accent.text`, `$input.bg` and `$border.strong`, so the system's accent shows where the host gives it. Disabled, it dims.

| Option | Type | Default | |
| --- | --- | --- | --- |
| `value` | `State<boolean>` | required | |
| `label` | `string` | `""` | shown beside the box, and its accessible name |
| `mixed` | `State<boolean>` | none | neither on nor off while true: a dash, and "mixed" to assistive technology (a "select all" box with some of its group on) |
| `on_click` | `Handler \| Action` | none | runs after the value toggled |
| `gap`, `px`, `py` | | `6`, `4`, `4` | |

## slider

`step` sets what an arrow moves (ten steps with Shift, or Page Up and Down) and what a drag lands on; without one, an arrow moves a hundredth of the range.

A horizontal number control: dragging, the arrow keys (1% of the range) and Home/End move it.

| Option | Type | Default | |
| --- | --- | --- | --- |
| `value` | `State<number>` | required | clamped to the range |
| `min`, `max` | `number` | `0`, `1` | finite, `min < max` |
| `label` | `string` | `""` | its accessible name (not drawn) |
| `w`, `h` | `Length` | `240`, `36` | |
| `px`, `py` | `Length` | `8`, `6` | |
| `bg`, `rounded` | | `"$input.bg"`, `"$radius.md"` | the rail |
| `color` | `Color` | inherited | the thumb |

## segmented

One of a few options side by side, as AppKit's segmented control: a radio group to assistive technology, one Tab stop, the arrows (mirrored right to left) moving the choice, Home and End to the ends.

| Option | Type | Default | |
| --- | --- | --- | --- |
| `value` | `State<any>` | required | the chosen option's value, both ways |
| `options` | `{ string \| number \| { label, value } }` | required | what each shows; a bare string or number is its own value |
| `label` | `string` | `""` | the group's accessible name |

## stepper

A number field with buttons that step it, a spin button to assistive technology. The arrows step it, Page Up and Down ten steps, Home and End to its ends; what is typed applies on Enter or on leaving the field, and anything that isn't a number goes back to the value.

| Option | Type | Default | |
| --- | --- | --- | --- |
| `value` | `State<number>` | required | held between `min` and `max`, rounded to `decimals` |
| `min`, `max` | `number` | none | |
| `step` | `number` | `1` | |
| `decimals` | `number` | `0` | digits shown after the point |
| `label` | `string` | `""` | its accessible name |

## select

A button showing the chosen option that opens a menu of them over itself, the chosen one under the pointer, as AppKit's pop-up button (and WAI-ARIA's select-only combobox: assistive technology hears its label, the chosen option as its value, and whether it is open). The menu is at least as wide as the button. Closed and focused, typing chooses the next option starting with what was typed (the same letter again goes on through them), and Down, Up, Enter and Space open it; open, it is a menu.

| Option | Type | Default | |
| --- | --- | --- | --- |
| `value` | `State<any>` | required | the chosen option's value, both ways |
| `options` | `{ string \| number \| { label, value, disabled } }` | required | a bare string or number is its own value; a disabled option shows but can't be chosen |
| `placeholder` | `string` | `""` | shown while no option is chosen |
| `label` | `string` | `""` | its accessible name |

## combobox

A field with suggestions listed under it as it is typed in, as WAI-ARIA's editable combobox. Focus stays in the field: typing shows the list with its first option highlighted, Down and Up (and Page Down and Up) move the highlight, which assistive technology hears as the field's active descendant, Enter chooses, Escape closes the list (and, closed, clears the field); a click on an option chooses it. Choosing sets `value` to its value and the field to its label. The options are the app's, for the text now: a computed filtering a list, or a state an async call fills (with `loading` while it does). The list is virtual: thousands of options build only what shows.

```lua
local query = ui.state("query", "")
local city = ui.state("city", nil :: string?)
local cities = { "Paris", "Prague", "Porto", "Lisbon" }
local matches = ui.computed(function()
	local q = string.lower(query:get())
	local out = {}
	for _, c in cities do
		if string.find(string.lower(c), q, 1, true) then
			table.insert(out, { label = c, value = string.lower(c) })
		end
	end
	return out
end)
return ui.combobox { label = "City", text = query, items = matches, value = city, w = 240 }
```

| Option | Type | Default | |
| --- | --- | --- | --- |
| `text` | `State<string>` | required | what the field holds, both ways |
| `items` | `State<{ string \| { label, value?, detail? } }>` | required | the options to show; a `detail` shows after the label |
| `value` | `State<any>` | none | the chosen option's value |
| `loading` | `State<boolean>` | none | while true, the list says it is searching |
| `empty` | `string` | `"No results"` | what the list says with no options |

## command_palette

A field over a list of commands near the window's top, filtered by a fuzzy match as it is typed in (the letters in order; words' starts and runs of letters rank first), as VS Code's and Raycast's: Up and Down move, Enter runs the highlighted command after the palette closes, Escape closes it. Without `commands` it lists the app's own (`ui.command`), with their menus and shortcuts, and runs the chosen one as its menu item would.

```lua
local open = ui.state("palette", false)
ui.command {
	id = "palette",
	title = "Command Palette",
	menu = "View",
	key = "cmd+shift+p",
	run = function()
		open:set(true)
	end,
}
ui.command { id = "theme", title = "Toggle Theme", menu = "View", run = function() end }
return ui.command_palette { open = open }
```

| Option | Type | Default | |
| --- | --- | --- | --- |
| `open` | `State<boolean>` | required | |
| `commands` | `{ { id, title?, detail?, shortcut? } }` | the app's commands | `shortcut`: keys (`"cmd+shift+p"`) shown as the platform shows them, or text as it is |
| `on_run` | `(id: string) -> ()` | runs the app's command | |

## calendar, date_picker, time_field

Dates are strings in ISO 8601's form, `"2026-10-23"`, and times are 24-hour `"09:30"`. Both are nil while none is chosen.

`calendar` is a month of days, as WAI-ARIA's date picker grid. It is one Tab stop and keeps focus; its current day is its active descendant, which assistive technology reads as "Friday, 23 October 2026". Left and Right move a day (mirrored right to left), Up and Down a week, and Home and End go to the week's ends. Page Up and Down go a month, with Shift a year; Enter or Space chooses. Its arrows change the month too. Today has a ring; days outside `min` and `max`, or `unavailable`, are dimmed and can't be chosen. `date_picker` is a button showing the date that opens a calendar (so does Down); choosing a day closes it and puts focus back on the button, and Escape closes it without choosing.

`time_field` holds an hour and a minute segment (with `hour12`, also AM/PM). Each segment is a spin button: Up and Down step it (the minutes by `minute_step`), Page Up and Down step further, digits type into it and focus moves on once no more digits can follow, Left and Right move between segments, and Backspace clears one. "Today" and "now" are local where the host says what the local time is (zgx-desktop does), otherwise UTC.

```lua
local due = ui.state("due", nil :: string?)
local start = ui.state("start", "09:30" :: string?)
return ui.row {
	gap = 12,
	ui.date_picker {
		label = "Due date",
		value = due,
		min = "2026-01-01",
		unavailable = function(_date: string, weekday: number)
			return weekday == 0 or weekday == 6
		end,
	},
	ui.time_field { label = "Start", value = start, minute_step = 15, hour12 = true },
}
```

| Option | Type | Default | |
| --- | --- | --- | --- |
| `value` | `State<string?>` | required | the date (`"2026-10-23"`) or time (`"09:30"`), both ways |
| `min`, `max` | `string` | none | `calendar`, `date_picker`: the first and last days that can be chosen |
| `first_weekday` | `number` | `0` | the week's first day: `0` for Sunday, `1` for Monday |
| `unavailable` | `(date: string, weekday: number) -> boolean` | none | days that can't be chosen; `weekday` runs from 0 for Sunday |
| `format` | `(date: string) -> string` | ISO's | how a `date_picker`'s button writes its date |
| `hour12` | `boolean` | `false` | `time_field`: hours 1 to 12 and an AM/PM segment |
| `minute_step` | `number` | `1` | `time_field`: what Up and Down step the minutes by |
| `label` | `string` | `""` | its accessible name |

In Rust (`zgx-components`): `calendar(label, value, CalendarOptions { … })`, `date_picker(…)` and `time_field(label, value, TimeOptions { … })`, with `CalendarDate` and `TimeOfDay` values (`CalendarDate::today()`, `add_days`, `add_months`, `weekday`, `parse`). `CalendarOptions` also takes month and weekday names for other languages.

## progress

A determinate bar from a 0-to-1 state; it takes no input.

| Option | Type | Default | |
| --- | --- | --- | --- |
| `value` | `State<number?>` | required | 0 to 1, clamped; `nil` is indeterminate (work of unknown size): a stretch of the bar sweeps across, and assistive technology hears no value |
| `label` | `string` | `""` | its accessible name (not drawn) |
| `w`, `h` | `Length` | `240`, `8` | |
| `bg` | `Color` | `"$input.bg"` | the track |
| `color` | `Color` | inherited | the fill |

## Forms

`ui.form { fields, validate, submit, on_success }` holds a form's fields and its submit, as React Hook Form does; a control given `field = form.fields.email` binds to one:

```lua
local form = ui.form {
	fields = { name = "", email = "", role = "Engineer" },
	validate = {
		name = function(name)
			return if name == "" then "A name is required" else nil
		end,
		email = function(email)
			return if email:find("@") then nil else "That is not an email address"
		end,
	},
	submit = function(values)
		-- In a task: it awaits, and raises what went wrong.
		local response = ui.fetch("https://api.example/people", { method = "POST", body = values }):await()
		if not response.ok then
			error(`The server answered {response.status}`, 0)
		end
		return response.json
	end,
}
local function field(name: string, label: string)
	local f = form.fields[name]
	return ui.column {
		gap = 4,
		ui.text { label },
		ui.input { id = name, label = label, field = f },
		ui.presence(f.error, function(problem: string)
			return ui.text { problem, color = "$danger" }
		end),
	}
end
return ui.column {
	gap = 12,
	field("name", "Name"),
	field("email", "Email"),
	ui.segmented { field = form.fields.role, options = { "Engineer", "Designer" } },
	ui.presence(form.failure, function(message: string)
		return ui.text { message, color = "$danger" }
	end),
	ui.button {
		function()
			return if form.pending:get() then "Saving…" else "Save"
		end,
		disabled = form.pending,
		on_click = form.submit,
	},
}
```

| `ui.form` option | |
| --- | --- |
| `fields` | each field's name and first value |
| `validate` | a field's check: `check(value, values)` returns what is wrong (a message) or nil. It runs as the field changes, and as another field it reads (`values.name`) does |
| `submit` | `submit(values)`, run as a task once every check passes: it may await, and what it raises is the form's `failure` |
| `on_success` | `on_success(result, values)`: what `submit` returned |

| On the form | |
| --- | --- |
| `fields.name` | a field: `value` (its state), `touched` (it has been left), `problem` (what its check says now), `error` (`problem` once it has been left or a submit tried: what to show) |
| `values.name` | a field's value state |
| `valid`, `attempted`, `pending`, `failure` | every check passes; a submit has been tried; a submit is under way; what the last one raised |
| `submit()` | checks every field: one wrong shows so, and the first wrong (top, then left) takes focus; all fine, runs `submit`. A handler as it is (`on_click = form.submit`) |
| `reset(values?)` | back to the first values (or `values`), nothing shown wrong |
| `cancel()` | stops a submit under way (a Cancel button): not `pending` any more, and what it awaits is not heard. A form gone (its component, a reload) cancels its own |

`field` binds a control's `value`, marks the field left as it loses focus, gives it the field's `error`, and on a one-line input makes Enter submit the form. It works on `input`, `textarea`, `checkbox`, `slider`, `segmented`, `stepper`, `select` and `combobox`; `on_blur` and `on_key` given beside it still run.

Any control takes `error` (a string, state or function; nil while fine): assistive technology hears the field is invalid (`aria-invalid`), and why after its name, as it reaches it.

## Example

```lua
local name = ui.state("name", "")
local notes = ui.state("notes", "")
local loud = ui.state("loud", false)
local volume = ui.state("volume", 40)
local field: Ref = ui.ref()
return ui.column {
	p = 24,
	gap = 12,
	w = 360,
	items = "stretch",
	ui.input { label = "Name", value = name, placeholder = "Your name", ref = field },
	ui.textarea { label = "Notes", value = notes, placeholder = "Anything else", h = 90, wrap = true },
	ui.checkbox { label = "Loud", value = loud },
	ui.row {
		gap = 10,
		items = "center",
		ui.slider { label = "Volume", value = volume, min = 0, max = 100, grow = 1 },
		ui.text { "{volume}", w = 32 },
	},
	ui.progress {
		label = "Volume",
		value = ui.computed(function()
			return volume:get() / 100
		end),
		w_full = true,
		color = "#5ea5ff",
	},
	ui.row {
		gap = 8,
		ui.button {
			on_click = function()
				name:set("")
				local el = field:peek()
				if el then
					el:focus()
				end
			end,
			"Clear",
		},
		ui.button {
			disabled = function()
				return name:get() == ""
			end,
			on_click = ui.action("save", "profile"),
			"Save",
		},
	},
}
```

## Gotchas

- A button's `label` is not read (a type error): its face is its children. Inputs', sliders' and progress bars' `label` is only their accessible name; put a `ui.text` beside them to show one. A checkbox draws its label.
- `value` is a state, not a value: `value = "text"` or a plain number is an error. An input's state holds a string; bound to a number state it shows the number and writes text back on the first edit.
- A control writes its state only once the user changes it: mounting never rewrites the script's value (a slider does not round `0.1`).
- `ui.computed` makes a read-only derived value for a `progress`; a slider or input bound to a computed would have its edits overwritten when the inputs change.
- Inputs are 240 points wide whatever their text; a long value scrolls inside. Use `w_full = true` (or a stretching parent) for a field that fills its row.
- Controls take no children, except `button`.
