# Plugins in a Luau app: `zgx.json`'s `expose` and `plugins`

Status: built for 0.1.7. Each exposed shared key and action asks a grant
(`shared:<key>`, `action:<name>`), so `allow` decides per plugin.

A Rust host decides what plugins reach: `expose_shared`, `expose_action`
and `expose_hook` on the app's side, `grant` (or `set_plugins`) on each
plugin's. An app run by `zgx run` has no Rust of its own, so today it can't
have plugins at all. Its `zgx.json` takes the same two sides:

```json
{
  "title": "Tasks",
  "expose": {
    "shared": { "tasks": "read", "focus": "write" },
    "actions": ["complete"],
    "hooks": { "#tasks": "override", "hook:toolbar.add": "open" }
  },
  "plugins": {
    "focus": { "path": "plugins/focus", "allow": ["override:#tasks"] },
    "stamps": {
      "path": "plugins/stamps",
      "allow": ["override:#empty", "override:@app/components/task_row"]
    }
  }
}
```

## The app's side: `expose`

`expose` is the app's whole plugin surface, kept in one place.

- **`shared`** maps each `ui.shared` key to `"read"` (`expose_shared_read`)
  or `"write"` (`expose_shared`).
- **`actions`** are names the app's script defines with
  `ui.actions { complete = function(id) … end }`, which is new. Plugins
  call them with `ui.invoke("complete", id)` or `ui.action("complete", id)`,
  as they call a Rust host's. The app's own scripts reach them the same
  way. An action named in `expose` that the script never defines is an
  error at load.
- **`hooks`** maps a target to `"open"` (adding before or after needs no
  grant: today's default) or `"override"` (replacing it, or filtering a
  list, needs `override:<target>`). `"*": "override"` lets a granted plugin
  replace anything; leaving it out keeps that from every plugin.
- **Anything the app doesn't expose, no plugin reaches,** whatever its
  `allow` says.

## Each plugin's side: `plugins`

- **`path`** is the plugin's directory (its `init.luau`) or file, from
  the app's directory.
- **`allow`** is its grants:
  - `fetch`, `clipboard`, `files`, `terminals`, `web` and `windows`, as
    `Capability` names them;
  - `shared:<key>` and `action:<name>`, for what `expose` asks a
    capability for;
  - `override:<target or @app/module>`.
- **What `allow` can't exceed:** `shared`, `action` and `override` grants
  must name something `expose` offers. Granting more is an error at load,
  naming the plugin and the grant: ``plugin `stamps` is allowed
  `override:#empty`, which the app doesn't expose (add it to
  expose.hooks)``.
- **What a plugin asks for at run time** but wasn't granted fails as it
  does under a Rust host, naming what it needs: ``plugin `focus` needs
  `override:#tasks` (zgx.json: plugins.focus.allow)``. The message names
  zgx.json when the grants came from it.

## One loader

A new function, `zgx_script::app_config::apply(host, dir)`, reads
`zgx.json`'s `expose` and `plugins` and calls `expose_*`, `actions` and
`set_plugins` on the host. Three callers share it:

- `zgx run`;
- `zgx test`'s `mount()`;
- `zgx audit` (#988's `audit_app`).

zgx-run's own `zgx.json` reading (window, `packages`) moves beside it, so
one parser knows every key and an unknown key is reported once, as it is
now.

## Cost

Nothing for an app without `plugins`. `ui.actions` is a lazy `ui`
function, so a script that doesn't call it builds nothing. An action call
goes through the existing `invoke` path, finding a Luau function where it
would find a Rust one.

## Verified by

- The plugins example runs from `zgx.json` alone: the same tests as
  `plugins_example.rs`, through `mount()`, with no Rust host.
- Load-time errors: a grant beyond `expose`, an exposed action the script
  doesn't define, an unknown capability, a missing path.
- Run-time errors name zgx.json.
- A Rust host's `expose_*` and `grant` behave as before.
