# Design systems: variants and the design lint

A design system in zgx is three layers: theme tokens hold the values (`"$accent"`, `"$space.3"`; see [theming](../scripting.md#theming)), `ui.variants` turns them into each component's styles, and the design lint keeps code from going around them.

```luau
ui.variants<D>(definition: D) -> (choice: VariantChoice<D, …>) -> Variants
ui.raw<T>(value: T, reason: string) -> T
ui.theme.strict(mode: "warn" | "error" | "off" | boolean) -> ()
type VariantProps<R>  -- a recipe's variant props, for a component's own props type
```

## Variants

`ui.variants` is shadcn's `cva`: a component's base styles, the options of each of its axes (tone, size…), the rules for combinations, and the defaults. It returns a recipe; calling the recipe with the options chosen gives what an element takes as `variants = …`.

| Key | Type | |
| --- | --- | --- |
| `base` | style table | what every choice starts from |
| `variants` | `{ [axis]: { [option]: style table } }` | each axis's options; an on-off axis names them `[true]` and `[false]` |
| `compound` | `{ { [axis]: option \| { option }, …styles } }` | styles for combinations: a rule applies when each axis it names has one of the options it lists |
| `defaults` | `{ [axis]: option }` | the option an axis takes when the choice leaves it out; without one, the axis adds nothing |

A style table holds [properties](properties.md) as plain values (tokens included) and `hover`, `active`, `focus` and `disabled` tables of them. The choice is a table of options by axis name: each a name, a state holding one, or a function returning one. `variants = recipe` alone takes every default.

Layers apply in order: `base`, the axes in name order, the matching `compound` rules in their order, then the element's own properties. Each layer's property replaces what it covers in the layers before (`p` an earlier `px` or `pt`, `rounded` the corners, `gap` `gap_x`), as `tailwind-merge` does for shadcn: a caller's `p = 0` takes all the padding a variant gave, and its `bg` wins over every tone's. Two axes that set the same property warn when they are defined: which wins would be their names' order, so set it in one of them or decide in a compound rule.

```lua
local button = ui.variants {
	base = { h = "$control.height", px = "$space.3", rounded = "$radius.md", items = "center", justify = "center" },
	variants = {
		tone = {
			solid = { bg = "$accent", color = "$accent.text", hover = { bg = "$accent.hover" } },
			ghost = { color = "$text.primary", hover = { bg = "$hover" }, active = { bg = "$pressed" } },
			danger = { bg = "$danger", color = "$accent.text" },
		},
		size = { sm = { h = 28, px = "$space.2", text_size = "$font.size.sm" }, md = {} },
		wide = { [true] = { w_full = true }, [false] = {} },
	},
	compound = { { tone = "ghost", size = "sm", px = "$space.1" } },
	defaults = { tone = "solid", size = "md", wide = false },
}

type ButtonProps = Merge<Merge<ContainerProps, VariantProps<typeof(button)>>, { children: { Child }? }>
local Button = ui.component(function(props: ButtonProps)
	return ui.button { variants = button { tone = props.tone, size = props.size, wide = props.wide }, props.children }
end)

local danger = ui.state("danger", false)
return ui.column {
	gap = "$space.2",
	p = "$space.4",
	Button { "Save" },
	Button { tone = "ghost", size = "sm", "Cancel" },
	-- Chosen by a function: restyled in Rust when `danger` changes.
	ui.button {
		variants = button { tone = function() return if danger:get() then "danger" else "solid" end },
		on_click = function() danger:toggle() end,
		"Delete",
	},
	ui.button { variants = button, p = 0, "Its own padding wins" },
}
```

**Types.** The recipe's choice is typed from the definition: an option that does not exist (`tone = "loud"`) and an axis that does not (`tnoe = …`) are errors in the editor and in `zgx check`. `VariantProps<typeof(button)>` is the axes as props, to merge into a component's own props type. Pass each axis through by name (`button { tone = props.tone }`), as shadcn does with `cva`: passing a whole props table does not check. The definition's style tables are checked as the script loads (`unknown style \`bgg\` in \`variants.tone.solid\``).

**Cost.** The definition's tables are parsed once, as `ui.variants` runs (a module runs once). A choice of names resolves as the element is built, into the element's literal styles: the same as writing them out. A choice by state or function resolves in Rust each time it changes, cached per combination, and restyles the node in place: its properties and its interaction styles swap without calling the script (a function choosing an option runs when what it reads changes, as `ui.computed` does). A colour change repaints; a size change lays out.

## The design lint

With the lint on, a style that hard-codes what a theme token is for reports where it is and which token to use:

```text
warning: app.luau:12: design lint: `bg = "#232e40"` hard-codes a colour. Use a theme token, so it
follows the theme and stays on the design system's scale. "$surface.raised" is exactly this in the
current theme: `bg = "$surface.raised"`. If no token fits, declare one for the app
(`ui.theme.tokens { app = { … } }`) and use that; for a deliberate exception,
`bg = ui.raw("#232e40", "why")`.
```

| Mode | |
| --- | --- |
| `ui.theme.strict("warn")` (or `true`) | each hard-coded value warns once per call site: stderr and `ScriptHost::warnings()` |
| `ui.theme.strict("error")` | each is an error: the script fails to load (shown over the window), or a function property's value is not applied |
| `ui.theme.strict("off")` (or `false`) | no check; the default |

`ZGX_DESIGN_LINT=off|warn|error` overrides the script (for a CI run, or an agent's), and a Rust host may call `ScriptHost::design_lint`. The mode is the host's: call `ui.theme.strict` first thing in `app.luau`, before modules build anything.

| Checked | Not checked |
| --- | --- |
| colours: `bg`, `color`, `border_color`, `outline_color`, `tint` | sizes: `w`, `h`, `min_*`, `max_*`, percentages, `"auto"` |
| spacing: `p*`, `m*`, `gap*` | `0`, and fully transparent colours |
| corner radii: `rounded`, `rounded_*` | border and outline widths, opacity, transforms, line height |
| type: `text_size`, `weight`, `font` | state values (the app's data, such as a colour the user picked) |
| shadows: `shadow`, `inset_shadow` | theme definitions: `ui.theme.tokens`, `ui.theme.register`, theme files |

It checks literal styles on elements, in `hover` and the other interaction tables and in `ui.variants`, as they are read, and what a function property returns each time it runs (reported at the function's line). The nearest token is the closest defined colour, or the closest step of the matching scale (`space.*`, `radius.*`, `font.size.*`, `font.weight.*`) in the current theme. Off, it costs nothing; on, a literal is classified as it is read and only a hard-coded one looks for a token.

`ui.raw(value, reason)` lets one value through, and needs the reason, so every exception is greppable and says why:

```lua
ui.theme.strict("warn")
return ui.row {
	gap = "$space.2",
	p = "$space.3",
	ui.div { w = 16, h = 16, rounded = "$radius.full", bg = ui.raw("#e5484d", "the brand mark's red, not a theme colour") },
	ui.text { "zgx", color = "$text.primary", text_size = "$font.size.lg" },
}
```
