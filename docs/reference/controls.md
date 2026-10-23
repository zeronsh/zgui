# Controls: button, input, textarea, checkbox, slider, progress, segmented, stepper

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
```

Every control dims while `disabled` (to 45% opacity, as native controls do) unless it says how it looks then.

## button

A focusable box whose children lie in a row; `on_click` runs on a click, Space or Enter.

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

## input and textarea

A text editor bound to a string state: selection, undo, clipboard and IME included. `input` is one line (pasted line breaks are removed); `textarea` keeps them.

| Option | Type | Default | |
| --- | --- | --- | --- |
| `value` | `State<string>`, or `() -> string \| number` | required | the text, both ways; or, controlled as React's inputs are, a function it shows, edits going out by `on_change` and `on_commit`. Focused, it keeps what is being typed; left, it shows the function's text again |
| `on_change` | `(text: string) -> ()` | none | a controlled input's every edit |
| `on_commit` | `(text: string) -> ()` | none | a controlled input's text on Enter, or on leaving it changed: where an inspector's field applies, and an undo step records |
| `label` | `string` | `""` | its accessible name (not drawn) |
| `placeholder` | `Reactive<string>` | none | shown while the value is empty, never part of it |
| `placeholder_color` | `Reactive<Color>` | `"#8a9ab0"` | |
| `select_on_focus` | `boolean` | `false` | focusing selects all the text (an address bar) |
| `spellcheck` | `boolean \| Readable<boolean> \| (() -> boolean)` | `false` | check the spelling as it is typed (HTML's `spellcheck`): misspelled words get a dotted line (not the word being typed), and the context menu offers the system checker's guesses, Learn Spelling and Ignore Spelling. Never in a password field |
| `multiline` | `boolean` | `false` (`textarea`: always) | `input` with several lines |
| `w`, `h` | `Length` | `240` wide (an intrinsic width), `36` high (`textarea`: `156`) | |
| `px`, `py` | `Length` | `8`, `6` | |
| `bg`, `rounded` | | `"$input.bg"`, `"$radius.md"` | |
| `shrink` | `number` | `1` | a row too narrow for it shrinks it, to nothing if need be (it scrolls its text); `shrink = 0` holds it |
| `wrap` | `boolean` | `false` | `textarea`: wrap long lines rather than scroll them |
| `line_numbers` | `boolean` | `false` | `textarea`: number the lines in a gutter at the left, the caret's brighter (a code editor); only the lines in view are built |

Its element (`on_mount`, a [ref](state.md#refs)) selects text: `el:select_all()`, `el:set_selection(a, b)`, `local a, b = el:selection()`, in characters ([events](events.md#elements)).

## checkbox

A toggle drawn as the platform draws one, a rounded 16-point box beside its label, bound to a boolean state: outlined while off; filled with the accent while on, its checkmark drawing on; a dash while `mixed`. A click on either, Space or Enter toggles it. Its colours are the theme's `$accent`, `$accent.text`, `$input.bg` and `$border.strong`, so the system's accent shows where the host gives it. Disabled, it dims.

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

## progress

A determinate bar from a 0-to-1 state; it takes no input.

| Option | Type | Default | |
| --- | --- | --- | --- |
| `value` | `State<number?>` | required | 0 to 1, clamped; `nil` is indeterminate (work of unknown size): a stretch of the bar sweeps across, and assistive technology hears no value |
| `label` | `string` | `""` | its accessible name (not drawn) |
| `w`, `h` | `Length` | `240`, `8` | |
| `bg` | `Color` | `"$input.bg"` | the track |
| `color` | `Color` | inherited | the fill |

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
