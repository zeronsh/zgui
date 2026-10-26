# zgx inspect: driving a running app (for agents)

Read this once. It is everything you need to see and drive a zgx Luau app while it runs: what is on screen, what its state holds, what went wrong, and clicks, typing and keys, by the same names its tests use.

## Start

```sh
zgx-run --inspect path/to/app/        # or ZGX_INSPECT_SOCKET=1 with any host that serves it
zgx inspect list                      # the apps serving devtools: pid and script
zgx inspect tree                      # what is on screen
```

With one app running, every command finds it; with several, add `--app PID`. Text answers by default (compact, a node a line); `--json` prints the answer as JSON (`{"ok":true,"text":…,"data":…}`, or `{"ok":false,"error":…}`) with exit code 0 on success, 1 on failure, 2 for bad usage and 3 when no app answers.

## Targets

Every command that takes a node takes a target. The same syntax works in Rust tests (`TestApp::click`, `find`, `text`…).

| Target | Names |
| --- | --- |
| `#send` | the node with `id = "send"` |
| `"Send"` | the node whose accessible name or text is exactly `Send` (a button, not the text inside it) |
| `role:button` | by role, ARIA's names: `button`, `textbox`, `checkbox`, `option`, `listbox`, `article`, `heading`, `link`, `text`… |
| `@12` | the node numbered 12 in `tree` (stable while it is mounted) |
| `role:button "Send"` | parts together narrow |
| `#sidebar >> "Inbox"` | `>>` looks inside what is before it |
| `#messages row 3` | a list's third mounted row |
| `"Copy"[2]` | the second of several matches |

A target matching several nodes is an error listing them (`@n role "name" box`), so you can name the one you meant with `@n`, a role, `>>` or `[n]`. Actions never pick one silently.

## Reading

```sh
zgx inspect tree                      # the whole view (200 lines at most; the rest counted)
zgx inspect tree '#transcript' --depth 3
zgx inspect node '"Copy"'             # kind, the script line that made it, role, name, value, state, box, text
zgx inspect find '"Copy"'             # every node a target matches
zgx inspect state                     # named states (ui.state, ui.persist, ui.shared) and their values
zgx inspect state messages --full     # one state; long values are cut at 200 bytes unless --full
zgx inspect errors                    # the script error, type errors, lint findings, layout warnings ("none")
zgx inspect lint                      # zgx lint over the app's scripts: file:line:col rule message
```

A tree line is `@n #id role "name or text" ="value" x,y wxh` and flags (`focused`, `disabled`, `checked`), indented two spaces a level, in document order. Boxes with nothing to say (no id, role or text) are left out and their children shown in their place; a button's own label text is folded into the button. `--all` shows every node. Same tree, same text: compare runs with `diff`.

A module's states are named `module.luau::name`; give the name alone when only one module has it.

## Acting

```sh
zgx inspect click '"Send"'
zgx inspect type '#composer' 'Write me a function'     # clicks it, then types
zgx inspect key Enter                                  # Escape, Tab, cmd+k, alt+ArrowDown, shift+Tab, F5…
zgx inspect scroll '#transcript' 400                   # points; down is positive
zgx inspect command new                                # a ui.command, as its menu item or shortcut runs it
zgx inspect set current '"c1"'                         # a named state, the value as JSON
zgx inspect wait '"Copy"' --timeout 8000               # until it shows
zgx inspect wait '#reply' --text 'words'               # until its text holds this
zgx inspect wait '#spinner' --gone
zgx inspect sleep 500                                  # let the app run
```

`cmd` is Command on macOS and Control elsewhere, as commands' keys are. A key goes where focus is: `type` or `click` an input first. After an action the interface is laid out again before the next command reads it, so `click` then `tree` shows the result; for what takes time (a fetch, a streamed reply, an animation), `wait` on what it will show rather than sleeping.

## Recipes

- Send a chat message and read the reply: `type '#composer' 'hi'`, `key Enter`, `wait '#message-m4' --text '.'`, `tree '#message-m4'`.
- Find why a button does nothing: `node '"Save"'` (is it `disabled`? where was it made?), `errors`.
- Check a state change took: `click '"Add"'`, `state count`.
- Reproduce in a test: the same targets work in `TestApp` (`app.click("\"Add\"")`, `app.type_text("#composer", "hi")`, `app.keys("Enter")`, `app.tree()`).

## Safety and cost

Devtools are built only with zgx-run's `inspect` feature (on by default for development) or zgx-script's `inspect`; an app built without it cannot be driven, whatever its environment says. Even when built in, nothing listens unless the host serves it: there is no thread, socket or bookkeeping until then. The socket belongs to the user who started the app: a Unix socket (mode 0600) in `$TMPDIR/zgx-inspect-<uid>` (0700); on Windows a loopback port that answers only requests carrying the token written beside it in `%LOCALAPPDATA%\zgx-inspect`.

A Rust host serves devtools with `zgx_script::inspect::serve(Session::new(&cx.ui, handle, &host, Some(cx.inspector.clone())), "my app", |task| cx.tasks.spawn(task))`, where `handle` is what `cx.render(host.view())` returned.
