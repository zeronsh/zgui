# Luau reference

One page per element and API group of [zgx-script](../scripting.md). Each gives the signature, every option with its type and default, a short example and the gotchas. The examples are complete scripts: `cargo test -p zgx-script --test reference_docs` runs every one headless and type checks it in strict mode.

| Page | |
| --- | --- |
| [Containers](containers.md) | `column`, `row`, `div`, `overlay`, `grid` |
| [Text](text.md) | `text`, `rich`, `span`, `markdown` |
| [Controls](controls.md) | `button`, `input`, `textarea`, `checkbox`, `slider`, `progress` |
| [Images and canvas](media.md) | `image`, `svg`, `canvas`, `path`, `linear`, `bitmap`, `shader` |
| [Scrolling](scrolling.md) | `scroll`, `scroll_x`, `list` |
| [Dynamic content](dynamic.md) | `show`, `switch`, `each`, `presence` |
| [Overlays and menus](overlays.md) | `modal`, `popover`, `menu`, `menu_item`, `submenu`, `context_menu` |
| [Web views, terminals, commands](native.md) | `webview`, `web`, `terminal`, `term`, `command` |
| [State and components](state.md) | `state`, `shared`, `persist`, `use_state`, `computed`, `effect`, `batch`, `ref`, `component`, `on_mount`, `slot`, `contribute`, actions |
| [Animation](anime.md) | the `anime` global (anime.js v4), `tween`, `spring`, `loop` |
| [Properties](properties.md) | every style property, grouped |
| [Design systems](design.md) | `variants`, `raw`, `theme.strict`: component variants and the design lint |
| [Events](events.md) | pointer, keyboard, focus, lifecycle, drag and drop |
| [App structure](app-structure.md) | splitting an app across modules with shared state |

## Reading the signatures

Signatures are Luau types from [`zgx.d.luau`](../../crates/zgx-script/types/zgx.d.luau), the file editors and `zgx check` use. Its `export type`s are global type names a script can use in its own annotations:

| Type | |
| --- | --- |
| `Node` | what element functions return; a child |
| `Child` | a node, a string (text, `{name}` shows state), a function returning text, a list of them, `false` or `nil` |
| `State<T>` | Rust-owned state holding a `T` (see [State](state.md)) |
| `Reactive<T>` | what a property takes: a `T`, a state of a `T`, a function returning one, or a [binding](anime.md#bindings) |
| `Props` | every element's properties: styles, identity, events (no children) |
| `ContainerProps` | `Props` and children |
| `Styles` | plain style values, for `hover`, `active`, `focus`, `disabled` |
| `Color`, `Length` | `"#rrggbb"`, `"rgb(…)"`, `0xrrggbb`; points or `"50%"`, `"auto"` |
| `Element`, `Ref` | a mounted node, and what follows one (see [Events](events.md#elements)) |
| `Merge<A, B>` | `A` with `B`'s properties: `Merge<ContainerProps, { title: string }>` |
| `VariantProps<R>` | a `ui.variants` recipe's axes as props (see [Design systems](design.md)) |

Each element function also has its own props type (`InputProps`, `ListProps`, `EachOptions`…), named on its page. Every props and options table is closed: an unknown key is an error, which luau-lsp reports as the key's type not being `number` (`Expected this to be 'number', but got '"bgg"'`, for `bgg`). A missing required option (an input's `value`) is reported as a missing field.

## Checking

`zgx check app.luau` (or a directory) runs luau-lsp in strict mode with these types; [scripting](../scripting.md#types) has the editor setup. In a debug build a hot-reloading host runs the same check after every save and shows the errors in the window's banner, when luau-lsp is installed.

Luau infers a callback's parameters from how its body uses them when the callback is passed to a generic function, which misfires, so the few callbacks whose argument is the script's own data take it as `any`: `each`'s row function, `presence`'s content function, drag payloads, a web page's messages. Annotate those parameters to have them checked:

```lua
type Task = { id: string, title: string, done: boolean }
local tasks = ui.state("tasks", {} :: { Task })
return ui.each(tasks, function(task: Task)
	return ui.text { task.title }
end)
```

State takes the type of its initial value. Give `nil` and empty tables a type, as above: `ui.state("selected", nil :: string?)`.
