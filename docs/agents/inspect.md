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
zgx inspect frames 5                  # the last frames: where their time went, what ran and why
zgx inspect snapshot '#messages' --out list.png   # what it looks like, as the window last drew it
```

`frames` is where each frame's time went and why the app did what it did: the frame's time, the states written (named ones by name, the rest counted) and the runs they caused with their total; then its phases, the costliest first (each counted once, with its instructions where the platform counts them); then the costliest runs, each with its time, what it is and the state that queued it:

```text
frame 131 (1.42 ms): state.luau::messages, 8 unnamed → 20 runs, 293 µs
  phases: update 310 µs (1840K instr), text 402 µs, layout 260 µs (1210K instr), paint 190 µs, encode 120 µs, present 95 µs
  153 µs @67 binding (markdown_view components/transcript.luau:32) ← a state
  28 µs @102 binding #transcript (list #transcript components/transcript.luau:73) ← a state
  19 µs effect 124 ← state.luau::messages
```

A binding is a node's (`@67`, its kind or the script line that made it); an effect a script's `ui.effect` (its line) or the engine's. The phases are `ZGX_PROFILE`'s (input, script, update, style, layout, text, damage, paint, encode, submit, present), measured only while devtools serve, so no restart: a frame with no writes (a scroll, an animation's layout) shows with its phases alone. Use it to find what a write costs, what reads too much, and which phase a slow frame spent.

A tree line is `@n #id role "name or text" ="value" x,y wxh` and flags (`focused`, `disabled`, `checked`), indented two spaces a level, in document order. Boxes with nothing to say (no id, role or text) are left out and their children shown in their place; a button's own label text is folded into the button. `--all` shows every node. Same tree, same text: compare runs with `diff`.

`snapshot` is a PNG of a node's box as the window last drew it, at the window's pixels (`--width 600` halves it until it fits). Without `--out` it stays beside the socket, readable by its user alone, and the answer says where; the app keeps its last 16. As MCP, `zgx_snapshot` answers the picture itself. Pictures come from what a macOS window shows; elsewhere, and in `TestApp`, the answer says there is none.

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

## As MCP tools

`zgx inspect mcp` serves the same commands as MCP tools on stdio (`zgx_tree`, `zgx_node`, `zgx_find`, `zgx_state`, `zgx_set_state`, `zgx_click`, `zgx_type`, `zgx_key`, `zgx_scroll`, `zgx_command`, `zgx_errors`, `zgx_lint`, `zgx_frames`, `zgx_snapshot`, `zgx_wait`, `zgx_sleep`, `zgx_apps`), each answering the text the CLI prints, with `isError` when it failed. Every tool takes an optional `app` (a pid) for when several apps run.

```sh
claude mcp add zgx -- zgx inspect mcp      # Claude Code; any MCP client runs the same command
```

## Diagnose a slow interaction

A scroll, a keystroke or a streamed reply that stutters: find what runs on each of its frames and which script line made it, then fix that line.

1. Run a release build with devtools: `cargo run --release -p zgx-run -- --inspect app/` (a debug build's times are several times a release build's, so compare like with like). An app already running with devtools needs no restart.
2. Do the interaction a few times, through devtools so it is the same each time: `zgx inspect scroll '#feed' 400` (or `click`, `type`, `key`), or by hand.
3. Read what ran: `zgx inspect frames 5`. Each frame gives its time, the states written, where the time went by phase, and its runs, the costliest first:

   ```text
   frame 212 (19.80 ms): state.luau::rows, 3 unnamed → 41 runs, 9840 µs
     phases: update 10120 µs (52310K instr), layout 6400 µs (30120K instr), text 1900 µs, paint 820 µs, present 560 µs
     6120 µs @58 binding (column components/row.luau:14) ← state.luau::rows
     2210 µs app.luau:88 effect ← state.luau::rows
     …
   ```

4. The costliest phase says where to look: `update` and `script` are the runs below it; `layout`, `text`, `paint` and `encode` are the interface's size and drawing (see the table). Map each costly run to its source line (`components/row.luau:14`, `app.luau:88`), then look at what that line reads and does. `zgx inspect node @58` shows the node, and `zgx inspect lint` lists what the linter already knows is costly.
5. Change it, save (the app reloads), and repeat steps 2–3 until the frame fits: 8.3 ms at 120 Hz, 16.7 ms at 60.

What the runs tell you:

| You see | It usually is | Do instead |
| --- | --- | --- |
| many `binding` runs of a list's rows on every write | rows reading the whole list (`list:get()[i]`) in their functions | `ui.list { data = state, key, row = function(item, i, row) … }`: a row reads its own item |
| a `binding` run costing milliseconds, on a node holding thousands of children | `ui.each` over a long list: every row built and laid out | `ui.list`, which builds the rows in view |
| the same `effect` line in every frame, with no write you made | an effect writing state that it (or another) reads, or `ui.every` faster than it needs | write only when the value changes; a slower timer; `ui.tween`/`ui.spring` for motion |
| a `binding` run for a whole long text or list after a small change | `items:update(function(l) … end)` or `set` of a rebuilt table | `push`, `set_key`, `append`, `patch`: they write the path that changed |
| few or cheap runs, yet `layout`, `text` or `paint` is most of the frame | the cost is the interface's size or drawing, not the script | a box laid out from its content (no `w`/`h`), a long text re-measured, a blur or shadow redrawn; `zgx inspect tree --depth 2` shows which boxes are large or deep enough to cost |
| a slow frame with no writes and no runs | a scroll or an animation laying out or drawing a lot | the same as above: the phases say which |

## Recipes

- Send a chat message and read the reply: `type '#composer' 'hi'`, `key Enter`, `wait '#message-m4' --text '.'`, `tree '#message-m4'`.
- Find why a button does nothing: `node '"Save"'` (is it `disabled`? where was it made?), `errors`.
- Check a state change took: `click '"Add"'`, `state count`.
- Reproduce in a test: the same targets work in `TestApp` (`app.click("\"Add\"")`, `app.type_text("#composer", "hi")`, `app.keys("Enter")`, `app.tree()`).

## Safety and cost

Devtools are built only with zgx-run's `inspect` feature (on by default for development) or zgx-script's `inspect`; an app built without it cannot be driven, whatever its environment says. Even when built in, nothing listens unless the host serves it: there is no thread, socket or bookkeeping until then. The socket belongs to the user who started the app: a Unix socket (mode 0600) in `$TMPDIR/zgx-inspect-<uid>` (0700); on Windows a loopback port that answers only requests carrying the token written beside it in `%LOCALAPPDATA%\zgx-inspect`.

A Rust host serves devtools with `zgx_script::inspect::serve(Session::new(&cx.ui, handle, &host, Some(cx.inspector.clone())), "my app", |task| cx.tasks.spawn(task))`, where `handle` is what `cx.render(host.view())` returned.
