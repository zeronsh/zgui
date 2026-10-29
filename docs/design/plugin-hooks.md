# Plugin hooks

Plugins add to an app at places the app did not prepare for, as WordPress plugins hook into a theme: next to the app's Save button, in place of its empty state, with a filter on a list's items. Slots (`ui.slot` and `ui.contribute(name, node)`) stay as they are, for the places an app does prepare.

Status: (a) hook points, before/after/replace and the capabilities; (b) filters; (c) `@app` module overrides and packages; and (e) the worked example (`crates/zgx-script/examples/plugins`) and the guide, [plugins.md](../plugins.md), are built; (d), types, follows.

## Targets

Every element with an `id` is a hook point. A plugin may also name an element the way the inspector and agents do (`crate::driver`'s parser, so there is one selector language):

| target | matches |
|---|---|
| `#save` | the element whose `id` is `save` |
| `hook:toolbar.save` | the element that declares `hook = "toolbar.save"` |
| `role:button "Save"` | a button whose name, as the app writes it, is `Save` |
| `"Save"` | any element named `Save` |

A plugin's target is one part, matched as the interface is built, so what only shows once it runs is refused: `>>` chains, `@n`, `row` and `[n]` are errors that say so. A role and a name are the ones the app's source gives: an element's `role` (or the role its kind has: a button is a `button`, an input a `textbox`), and its `label`, its literal text or its first child's literal text. A name from state is not static and is not matched; such an element is reached by its `id`.

**Opting out:** `private = true` (or `hook = false`) takes an element and everything in it out of reach: a password field, a confirm a plugin must not cover. A plugin's target never matches inside one, however the inside builds (a scroll's rows, a component mounting later).

**Stable points:** `hook = "name"` declares a point the app documents and keeps. Every other `id` and every selector is the app's internals: targeting them works, and may break with the app's next version. `zgx types` (d) emits the declared hooks and every id, so a plugin's checker sees which it uses.

Only the app's own elements are targets. What a plugin adds is never matched by another plugin's target, nor by its own.

## Placing

```lua
ui.contribute { target = "#save", before = ui.button { label = "Preview", on_click = preview } }
ui.contribute { target = "hook:toolbar.save", after = ui.text { "Saved by sync" } }
ui.contribute { target = "#empty-state", replace = ui.text { "Nothing here yet" } }
```

- `before` and `after` add siblings, in the matched element's parent, in plugin order. They are open by default, like slots; `ScriptHost::expose_hook(target, Some(capability))` makes a target ask a capability first.
- `replace` puts the plugin's node in the element's place, and needs `Capability::Override(target)` (the target as the plugin writes it) or `Capability::Override("*")`. Of several, the last plugin's wins.
- A contribution names exactly one of the three; an error says what it needs when it is not granted.

What a plugin adds is told apart as its own: its accessible description says "From the plugin `<name>`" (after the element's own description, if it has one), so a contributed "Confirm" can be traced by a screen reader user, the inspector and an agent.

## Filtering

```lua
ui.filter { target = "#title", text = function(text) return string.upper(text) end }
ui.filter { target = "#save", disabled = function(disabled) return disabled or not signed_in:get() end }
```

A filter passes one of an element's props through a function of the plugin's: `text` (its own, or a button's first child's), `label`, `hidden`, `disabled`, `tooltip` (its written text), `items` (a `ui.each`'s or a `ui.list`'s `data`) and `order` (its children's, by id: those returned first, the rest after; a changed order rebuilds the children in a region, an equal one keeps them). The function takes the value as it is and returns it changed; each is a computed source of the app's script that calls the plugin's function, so it runs in the plugin's VM again when what either reads changes, never per frame. Several apply in plugin order. It needs `Override`, as replacing does. The element is lowered as a copy with those props changed; its handlers, styles and state are the app's. A filter that fails is reported as the plugin's error and the value shows unchanged.

## Matching and reactivity

Targets are matched as the interface is lowered, once per element built, never per frame. An element built again (a `ui.show` turning back on, a row of `ui.each` coming in) is matched again. So a match that goes takes its contributions with it, and one that appears gets them; nothing is re-run while the element stays. A plugin's reload, or a change to which plugins load, reloads the app, which lowers it again.

Before, after and replace work in the parent's children, as the parent lowers them: the app's root element, and children an element lowers itself (a split's panes, a library component's parts), are not reached in (a).

## Permissions

Unchanged from plugins: a plugin sees what it is granted (`expose_action`, `expose_shared`, the capabilities) and nothing else of the app's. Its nodes run in its own VM, against its own state and the shared state it was given. Hooks add one capability (`Override`) and one exposure (`expose_hook`).

## Cost

With no plugin contributing to a target, lowering checks one `Option` per children list: a container's children are lowered as before, and with contributions by a separate, out-of-line (cold) loop. Elements themselves are not looked at. With contributions, each element built is looked up by id and hook name (hash maps) and against the selectors that name neither (a short list). Measured with 1,000 ids, with and without selector-bearing plugins, in the PR that lands each part; the exact gate is ≤0.

## Plan

- **(a)** Hook points, before/after/replace, `Capability::Override`, `expose_hook`, private elements, the plugin's accessible description.
- **(b)** Filters: `ui.filter { target, text = fn, … }` over text, label, hidden, disabled, tooltip, list items and child order, run as computeds in the plugin's VM: when what they read changes, never per frame. Needs `Override`.
- **(c)** Overrides and packages: an `@app/…` module (only `@app` requires; a relative require stays private) replaced by a plugin's module with `Override`, with `@app-original/…` reachable only from inside the override, never from plugins. Packages: `ScriptHost::package`, `zgx.json`-listed `.luaurc` aliases outside the app's directory, an allowed-roots read-only sandbox in place of `within(root)`; `zgx bundle` includes them.
- **(d)** Types: `zgx types` emits `HookName`, each hook's context type and typed `ui.contribute` overloads.
- **(e)** Docs and a worked example.

Not yet: a function form of `replace` (given the app's node), and `first = true` to go before every other plugin.
