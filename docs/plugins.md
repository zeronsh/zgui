# Plugins

A zgx-script app can be extended by plugins: scripts that run beside it, each in a VM of its own, and add to the app, filter what it shows or replace parts of it. They can reach places the app never prepared, the way WordPress plugins reach into a theme. The app decides what they may see and do. This guide walks through the example in `crates/zgx-script/examples/plugins` (run it with `cargo run --release -p zgx-script --example plugins`). The reference is [state.md](reference/state.md#plugins-and-slots), and the design is [design/plugin-hooks.md](design/plugin-hooks.md).

## The example

A task list. Two plugins extend it:

- **focus** adds a Focus button beside the app's Add button. While focus is on, it filters the list down to the tasks not yet done.
- **stamps** puts a mark before the title and replaces the empty state with its own. It also replaces the app's row module with one that wraps the original and adds a "done" badge from a package the app shares.

```
examples/plugins/          the app
  app.luau
  components/task_row.luau required as @app/components/task_row
  plugins/focus/init.luau
  plugins/stamps/init.luau, task_row.luau
examples/plugins_kit/      a package (`kit`), outside the app
examples/support/plugins_host.rs   the host: tasks, actions, grants
```

## Making an app hookable

**Every element with an `id` is a target.** A plugin names it as `#id`, adds before or after it, and, if trusted, replaces or filters it. Nothing else is needed: an app with ids is already extensible.

**Declare the points you mean to keep.** `hook = "toolbar.add"` names a stable point that plugins target as `hook:toolbar.add`. Ids are your internals: a plugin targeting one works today and may break when the app changes, which the docs of a plugin system should say plainly. Declared hooks are the ones you promise to keep. The example's Add button carries both:

```lua
ui.button { id = "add", hook = "toolbar.add", "Add", on_click = add }
```

**Keep sensitive elements out of reach.** `hook = false` (or `private = true`) on an element puts it and everything inside it out of every plugin's reach, however it builds: a password field, a destructive confirm, a payment form. The example's Clear button is one:

```lua
ui.button { id = "clear", hook = false, "Clear done", on_click = ui.action("clear_done") }
```

Plugins may also target an element by role and name, as the app writes them (`role:button "Add"`, or `"Add"`). That is convenient and the most fragile form; prefer declared hooks.

**Let modules be replaced where that makes sense.** A module the app requires as `@app/<path>` (from its own directory) can be replaced by a trusted plugin. A relative `require("./x")` never can. The example requires its row as `@app/components/task_row` on purpose.

## What plugins do

| | call | needs |
|---|---|---|
| add beside an element | `ui.contribute { target, before = node }`, `after = node` | nothing, unless the host asks a capability for that target (`expose_hook`) |
| put a node in its place | `ui.contribute { target, replace = node }` | `Capability::Override(target)` |
| pass a prop through a function | `ui.filter { target, text = fn, … }` (text, label, hidden, disabled, tooltip, items, order) | `Capability::Override(target)` |
| replace an app module | `ui.override { module = "@app/…", with = "./file" }` | `Capability::Override(module)`, a trust grant (below) |
| add to a slot the app made | `ui.contribute(name, node)` | nothing |

From the focus plugin:

```lua
ui.contribute { target = "hook:toolbar.add", after = ui.button { id = "focus", on_click = ui.toggle(focus), … } }
ui.filter {
	target = "#tasks",
	items = function(items)
		if not focus:get() then return items end
		…
	end,
}
```

A filter runs in the plugin's VM. It runs again whenever what it reads changes: the list it filters, and `focus`, the shared state it reads. It never runs per frame. A filter that fails is reported as the plugin's error, and the value shows as it was.

What a plugin adds is described to assistive technology as the plugin's ("From the plugin focus"), so a contributed "Confirm" next to the app's can be traced.

Before and after work among a container's children. Inside a `ui.show` or `ui.switch`, which lays out as the one element it shows, an element can be replaced or filtered but not given siblings. The app's root element, and the panes of a split or a library component, are not targets.

## The trust model

Capabilities are what the host grants each plugin (`Plugin { name, entry, grants }`), and the example's host grants them per plugin:

```rust
Plugin { name: "focus".into(), entry, grants: vec![Capability::Override("#tasks".into())] },
Plugin { name: "stamps".into(), entry, grants: vec![
    Capability::Override("#empty".into()),
    Capability::Override("@app/components/task_row".into()),
] },
```

**`Override` on an element** lets a plugin change what that element shows: its replacement or its filtered props. The plugin's code still runs in its own VM, seeing only what it was given. It can change how the element looks and reads, but not reach the app's state or handlers. Grant it for elements whose content you are willing to let the plugin decide.

**`Override` on a module is a trust grant.** The replacement runs in the app's VM as the app's own code, with everything the app's scripts may do: its state, its actions, its packages. That is why stamps' row can `require("@app-original/components/task_row")` (the module it replaces; only the replacement may reach it) and `require("@kit/badge")`. Grant `Override("@app/…")` as you would install the plugin's code into the app, and `Override("*")` only to a plugin you trust with the whole app.

The host can also ask a capability before anyone adds beside a target: `host.expose_hook("#checkout", Some(Capability::Named("payments".into())))`.

## What plugins see of the app

A plugin sees nothing of the app's own `ui.state`. What it reads and changes, the host exposes:

- **Shared state.** `host.shared("tasks", …)` is state Rust and every script share. The host exposes a key to plugins in one of two ways:
  - `host.expose_shared("focus", None)` lets plugins read and write it (`ui.shared("focus", false)`).
  - `host.expose_shared_read("tasks", None)` lets them read it and follow it, but not change it. `get`, `peek`, `len`, `at`, templates (`{tasks}`), filters and a list's items read it. `set`, `update`, `push` and every other write from a plugin is an error naming the key, as is binding it where it is written (an input's `value`). The app's own scripts write it as before.

  Pass a capability instead of `None` to expose it to the plugins granted that capability only. The example shares its tasks read only: the focus plugin counts the open ones on its button and filters the list, but only the app and the host's actions change them. `focus` is the plugin's to set.
- **Actions.** `host.action("complete", …)` is a Rust handler that scripts call with `ui.action("complete", id)` (as a handler) or `ui.invoke`. `host.expose_action("complete", None)` lets plugins call it too. The example exposes completing a task but not adding one or clearing what is done: those stay the app's.
- **Everything else** (fetching, the clipboard, files, windows, web views, terminals) takes its own capability; see the table in [scripting.md](scripting.md#plugins).

## Packages

Modules several apps share go in a package, a folder outside the app: `ScriptHost::package("kit", dir)` or zgx.json's `"packages": { "kit": "../plugins_kit" }`. The app requires them as `@kit/…`, and a `.luaurc` alias into the folder lets `zgx check` follow them. Packages are read only, run as the app's code, and are bundled with it. Plugins reach none, except through an app module they were trusted to replace.

## Checking

`zgx lint <dir>` and `zgx check <dir>` cover plugins as they do the app. `check` resolves `@app/…` and `@app-original/…` against the app's directory, the nearest holding an `app.luau`. The example's test (`tests/plugins_example.rs`) runs the app, its host and both plugins headless: each contribution, the filter following shared state, the host's actions, and the replaced empty state.
