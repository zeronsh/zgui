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

A target matching several nodes is an error listing them (`@n role "name" box`, `hidden` after those out of layout), so you can name the one you meant with `@n`, a role, `>>` or `[n]`. Actions never pick one silently, with one exception: nodes hidden from layout (`hidden`, or inside something hidden, as a tab's panel kept mounted while another shows) match too, and where exactly one of the matches shows, the target names that one, the one a person can reach.

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
zgx inspect frames --since fling      # only the frames after `mark fling`; --since 412 after frame 412
zgx inspect frames 5 --runs 20        # more of each frame's runs (5 by default); --all for every one
zgx inspect frames --keep 2000        # keep more frames (480 by default: four seconds at 120 Hz)
zgx inspect mark fling                # mark now, to read only what follows
zgx inspect snapshot '#messages' --out list.png   # what it looks like, as the window last drew it
zgx inspect search "overflow"         # find in page: the count, the current match, the first five
zgx inspect search --next             # the window's find steps on; --prev back, --all every match, --close
```

`search` is the window's find in page (Command-F), as a user's: case and accents ignored, over all the window's text, fields' values and virtual lists' rows they have not mounted (counted whole before it answers, for at most 2 s; `+` after the count if not done). It answers how many, which is current, and the first five (`--all`, every one; `n` as JSON), each where it is (`@node`, or a list's `@node row 1500`), its range and the text around it (`…[match]…`), and its box while it shows. The window's bar shows the same, its marks too:

```text
6 matches for "cafe", 1 current
* 1 @12 [Café] one
  2 @14 [cafe] two
```

`frames` is where each frame's time went and why the app did what it did: the frame's time, the states written (named ones by name, the rest counted) and the runs they caused with their total; then its phases, the costliest first (each counted once, with its instructions where the platform counts them); then the costliest runs, each with its time, what it is and the state that queued it:

```text
frame 131 (1.42 ms): state.luau::messages, 8 unnamed → 20 runs, 293 µs
  phases: update 310 µs (1840K instr), text 402 µs, layout 260 µs (1210K instr), paint 190 µs, encode 120 µs, present 95 µs
  153 µs @67 binding (markdown_view components/transcript.luau:32) ← a state
  28 µs @102 binding #transcript (list #transcript components/transcript.luau:73) ← a state
  19 µs effect 124 ← state.luau::messages
```

Where the engine took a slower path, or waited, the frame says why, and where the renderer measures the GPU's own time (macOS's Metal frames) it says that too:

```text
frame 3230 (6.95 ms, then 6.12 ms waiting for the display): page_scroll, 1 unnamed → 3 runs, 7 µs
  phases: present 6442 µs (147K instr), paint 307 µs, encode 101 µs
  gpu: 1210 µs (its own time, within present)
  why: scroll @537 repainted its view: @2071 (text) draws over it, so moving its pixels would move that too (keep what draws over a scroll view outside its box)
  why: waited a refresh more to keep the last frame's cadence, until 8 frames in a row are ready 1.5 ms before their refresh (0 so far)
```

The notes say which fast path a frame missed and what stopped it: a scroll that repainted its view rather than moving its pixels (a node drawing over the scroll view, what lies under it showing through, a move between device pixels), a frame that missed its refresh or was held to the last frame's cadence, scroll input applied all at once while the window wasn't presenting. `gpu` is the GPU's time alone; `present` also counts waiting for the window server and the display, so a `present` well above `gpu` is the system's time, not the frame's.

A binding is a node's (`@67`, its kind or the script line that made it); an effect a script's `ui.effect` (its line) or the engine's. The phases are `ZGX_PROFILE`'s (input, script, update, style, layout, text, damage, paint, encode, submit, present, wait), measured only while devtools serve, so no restart: a frame with no writes (a scroll, an animation's layout) shows with its phases alone. `wait` is pacing, not work: on macOS a frame is held to render and hand over at its point in a refresh, up to a refresh long (8.3 ms at 120 Hz, 16.7 at 60), so the frame's time leaves it out and says it apart (`then 6.10 ms waiting for the display`). `present` includes waiting for the GPU to finish the frame. Use it to find what a write costs, what reads too much, and which phase a slow frame spent.

A tree line is `@n #id role "name or text" ="value" x,y wxh` and flags (`focused`, `disabled`, `checked`), indented two spaces a level, in document order. Boxes with nothing to say (no id, role or text) are left out and their children shown in their place; a button's own label text is folded into the button. `--all` shows every node. Same tree, same text: compare runs with `diff`.

`snapshot` is a PNG of a node's box as the window last drew it, at the window's pixels (`--width 600` halves it until it fits). Without `--out` it stays beside the socket, readable by its user alone, and the answer says where; the app keeps its last 16. As MCP, `zgx_snapshot` answers the picture itself. Pictures come from what a macOS window shows; elsewhere, and in `TestApp`, the answer says there is none.

A module's states are named `module.luau::name`; give the name alone when only one module has it.

A hidden, minimized or covered window draws no frames (macOS stops a covered window's refreshes): `frames` says so first, and a `replay` is refused rather than answering "0 frames", since its events would only wait for the window. Keep the window in front and uncovered while measuring. For a test session that shouldn't need the screen to itself, `ZGX_RENDER_WHEN_COVERED=1` in the app's environment keeps a covered window drawing, so input and hot reloads don't stall; it isn't for measuring what a person sees, and it's never on by default, since a window nobody can see does no work. On Windows it changes nothing: the system isn't asked, so a covered window draws anyway ([Limitations](../windows.md#limitations)).

## What it holds

```sh
zgx inspect memory                    # JSON: a line for each holder
zgx inspect memory --human            # a table
```

What the running app holds, measured when asked, with nothing to build or switch on: compare two answers and the line that grew says what to look at. A line is `{"area","name","n","bytes"}`, with `n` (entries) and `bytes` where the holder can tell:

| Area | Lines |
| --- | --- |
| `process` | `footprint` (macOS, what Activity Monitor shows), `resident` (Linux) or `private` (Windows) |
| `heap` | mimalloc's memory committed, at its peak and in use (the gap is what it keeps for later); or the system allocator's in use and free |
| `gpu` | the glyph atlas, image textures, layers (allocated, and `layers live`: what their content takes of that), canvases, SVGs, vertex buffers, and the device's allocations where wgpu counts them |
| `gpu layer` | each isolated layer, largest first: `@n #id`, its content's pixels, its scale, its live MiB, and whether it is transformed or parked; its bytes are what its texture allocates |
| `text` | shaped nodes, prepared lines, measures, shaped runs, rasterized glyphs and outlines |
| `scene` | nodes mounted, nodes removed and not yet freed, slots |
| `devtools` | the frame log and the effect names it keeps (`--inspect` only) |
| `luau` | each VM's heap, as it stands (no collection) |
| `images` | decoded pictures and SVGs a node shows, and those cached for when one does again |

The areas overlap: `heap` holds `text`, `scene`, `luau` and `images`, and much of `gpu` is in the `process` line only. An area's lines add up; areas don't.

## Acting

```sh
zgx inspect click '"Send"'
zgx inspect hover '#row'                               # the pointer there: what shows on hover
zgx inspect right-click '#row'                         # its context menu opens; its items are nodes to click
zgx inspect drag '#card' '#done'                       # from one to another
zgx inspect drag '#slider' 340,120                     # or to a point, window points
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

`hover`, `right-click` and `drag` take a target, whose middle they use, or `x,y` in window points. `drag` presses, moves past the drag threshold and on in steps, as a hand does, so what lies between sees it pass (`on_drag_over`), then releases. Everything happens within the window: a drag can't leave it, so `on_drag_outside` and other apps' drops need a real one.

`cmd` is Command on macOS and Control elsewhere, as commands' keys are. A key goes where focus is: `type` or `click` an input first. After an action the interface is laid out again before the next command reads it, so `click` then `tree` shows the result; for what takes time (a fetch, a streamed reply, an animation), `wait` on what it will show rather than sleeping.

## Popovers, menus and other popups

A popover's, menu's, select's, date picker's, tooltip's, context menu's or modal's panel is in `tree` under the node that opens it, and targets reach inside it like anywhere else. A closed panel's content is kept, unseen: `find` lists it, marked `hidden`, and an action at it (`click`, `hover`, `scroll`, `snapshot`) fails with `doesn't show`, rather than landing on whatever is under it.

Open it first, wait for it, then act inside:

```sh
zgx inspect click '#more'                 # the trigger
zgx inspect wait '#share'                 # the panel shows a frame later
zgx inspect click 'role:menuitem "Share"' # a submenu opens the same way
zgx inspect click '#copy-link'
zgx inspect right-click '#row' && zgx inspect wait 'role:menuitem "Cut"'
zgx inspect hover '#save' && zgx inspect wait '"Save the file"'   # a tooltip, after its delay
zgx inspect key Escape                    # closes the innermost
```

A select's options are `menuitemradio`, as AppKit's pop-up button has them; a combobox's are `option`; a date picker's month is a `grid`.

On macOS a popup is a window of its own and may lie past the main window's edge. Its boxes are in the main window's points all the same, so clicks and targets reach it. `snapshot` pictures it from its own surface, and `memory` counts its renderers as `gpu popup windows`. A popup opens in the scene first and gets its window at the next frame, so a reading taken between the two says `popup windows not yet drawn`.

## The app's other windows

`windows` lists the main window and the windows the app declares (`ui.window`, an item of `ui.windows`), each with its id, size, title and popups open. Any command takes `window` (`--window` on the CLI, `window` on every MCP tool) and answers for that window: an id from `windows`, or an item's key, which names its window as `zgx test`'s `app:window` does. Without it, the main window answers. A window answers while it is open; one that closes is gone from `windows`. Popups answer for the window they open from, as above.

```sh
zgx inspect windows                       # main  1100x720
                                          # windows0:img01.png  420x360  "img01.png"
zgx inspect --window img01.png tree       # the detail window's interface
zgx inspect --window img01.png click '#favorite'
zgx inspect --window windows1:shots state # states are the app's, from any window
```

## Gestures: record once, replay

`scroll` sends one wheel event straight to the interface. A real gesture is more: a trackpad's fingers and then its momentum, as the platform reports them over a second or two, which the window resamples to its refreshes and which stretches an edge. To see how that feels, record one and play it back through the window's own input path:

```sh
zgx inspect record                                    # then scroll in the window, by hand
zgx inspect record stop --out fling.tape              # what the window received, with times and phases
zgx inspect replay '#messages' fling.tape             # played back there, as if received again
```

```text
playing 292 events over 2408 ms: 288 frames, every 8.33 ms (median), longest gap 16.74 ms, 3 over 1.5 frames; work 0.82 ms median, 1.32 ms most (frame 290)
```

One run measures little: a window's frames differ that much from one replay to the next (a fresh window against one long running, other apps drawing). `--repeat N` plays the tape N times and gives each run's line and then the spread, least to most with the median, of the interval, late frames, work and longest gap:

```sh
zgx inspect replay '#messages' fling.tape --repeat 3
```

A tape keeps the timing the window received. Recorded while the window fell behind (running at half its refresh rate, say), the platform coalesced its events, an event every other refresh, and replaying the tape repeats that: a fix to the slowness can't show in it. `record stop` warns when the events came further apart than the display refreshes; `--every MS` spreads a tape's motion over events that far apart, each recorded event's motion divided evenly over the time before it:

```sh
zgx inspect replay '#messages' fling.tape --every 8.33     # as a window keeping up at 120 Hz is sent them
```

The answer comes once the tape and what follows it (springing back from an edge) have played: how many frames, the time between them, frames shown late (more than half a refresh past their turn), and the work in each. `frames` then has every frame of it. Nothing reaches the desktop: the events go to this window alone, through the same path as the platform's, so the trackpad's phases, resampling and elastic edges run as they do for a hand. A tape is text, an event a line (`8.312 pixel 0 -6.75 moved`: milliseconds, pixel motion in points or wheel `lines`, across and down, the phase), so one can be written or edited too. Desktop windows record and replay; headless hosts and `TestApp` say they cannot.

## Switches: one app, two paths

A change measured across a rebuild and a relaunch compares two windows as much as two builds. The engine's fast paths can instead be switched off and on in the running app, and the same replay measured each way:

```sh
zgx inspect switch                          # each switch, on or off, and what it does
zgx inspect switch scroll_copy off          # scrolls repaint their view rather than moving its pixels
zgx inspect replay '#messages' fling.tape --repeat 3
zgx inspect switch scroll_copy on
```

`scroll_copy` (a scroll moves what is drawn and paints the strip scrolled in), `refresh_grid` (macOS: steady frames hand over at a set point before a refresh; off, as soon as drawn) and `partial_repaint` (a frame paints only what changed; off, the whole window).

## As MCP tools

`zgx inspect mcp` serves the same commands as MCP tools on stdio (`zgx_windows`, `zgx_tree`, `zgx_node`, `zgx_find`, `zgx_state`, `zgx_set_state`, `zgx_click`, `zgx_type`, `zgx_key`, `zgx_scroll`, `zgx_command`, `zgx_errors`, `zgx_lint`, `zgx_frames`, `zgx_mark`, `zgx_switch`, `zgx_search`, `zgx_snapshot`, `zgx_record`, `zgx_replay`, `zgx_wait`, `zgx_sleep`, `zgx_apps`), each answering the text the CLI prints, with `isError` when it failed. Every tool takes an optional `app` (a pid) for when several apps run, and an optional `window` (an id from `zgx_windows`, or an item's key) for another of the app's windows.

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
| `why: scroll @n repainted its view: @m … draws over it` | a node (a sticky header, a floating nav) painting over the scroll view, so its pixels can't simply move | keep what draws over a scroll view outside its box: make the scroll view start beside it |
| `why: … shows under it` | the scroll view has no opaque fill of its own, so what lies beneath shows through | give the scroll view an opaque `bg`, exactly its box |
| `present` far above `gpu`, frames 16.7 ms apart at 120 Hz with `waited a refresh more to keep the last frame's cadence` | the GPU or the window server took long once, and the frames after kept the slower cadence | make each frame cheaper (the notes above), then measure with `--repeat` |

## Recipes

- Send a chat message and read the reply: `type '#composer' 'hi'`, `key Enter`, `wait '#message-m4' --text '.'`, `tree '#message-m4'`.
- Find why a button does nothing: `node '"Save"'` (is it `disabled`? where was it made?), `errors`.
- Check a state change took: `click '"Add"'`, `state count`.
- Reproduce in a test: the same targets work in `TestApp` (`app.click("\"Add\"")`, `app.type_text("#composer", "hi")`, `app.keys("Enter")`, `app.tree()`).

## Safety and cost

Devtools are built only with zgx-run's `inspect` feature (on by default for development) or zgx-script's `inspect`; an app built without it cannot be driven, whatever its environment says. Even when built in, nothing listens unless the host serves it: there is no thread, socket or bookkeeping until then. The socket belongs to the user who started the app: a Unix socket (mode 0600) in `$TMPDIR/zgx-inspect-<uid>` (0700); on Windows a loopback port that answers only requests carrying the token written beside it in `%LOCALAPPDATA%\zgx-inspect`.

A Rust host serves devtools with `zgx_script::inspect::serve(Session::new(&cx.ui, handle, &host, Some(cx.inspector.clone())), "my app", |task| cx.tasks.spawn(task))`, where `handle` is what `cx.render(host.view())` returned.
