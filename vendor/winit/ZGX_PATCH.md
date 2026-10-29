# zgx patches to winit 0.30.13

Vendored from crates.io `winit` 0.30.13 (Apache-2.0, `LICENSE`) and applied
through `[patch.crates-io]` in the workspace `Cargo.toml`. Changes are marked
"zgx patch" in the source. Its examples, tests and docs are left out.

## Why

On Wayland a popup past the window's edge is an `xdg_popup`, made with
`xdg_surface.get_popup(parent, positioner)`: its parent is the window's
`xdg_surface`. winit exposes the window's `xdg_toplevel`, from which the
protocol has no way back to the `xdg_surface`, and a second `xdg_surface` on
the window's `wl_surface` is a protocol error (`already_constructed`). The
positioner's rectangles are relative to the window geometry, which starts
above the surface by the title bar winit draws where the compositor draws
none (Weston): zgx must know where.

## What

1. `WindowExtWayland::xdg_surface`, beside `xdg_toplevel`
   (`src/platform/wayland.rs`; `src/platform_impl/linux/wayland/window/mod.rs`).
2. `WindowExtWayland::xdg_window_geometry_origin`: where
   `set_window_geometry` puts the geometry on the surface (the frame's
   location, `(0, 0)` without one; `window/state.rs`).

Drop these when a winit release exposes the same (its popup work, 0.31).

## X11: `_NET_WM_SYNC_REQUEST`

A window manager resizing a window (dragging its border) resized it as fast
as the pointer moved, ahead of the frames drawn for each size, which then
lag and show garbage at the edges under a compositor. GTK, Chromium and Qt
take part in `_NET_WM_SYNC_REQUEST`: the window manager waits, before its
next resize, for the window to say it drew the last.

3. A window gets a SYNC counter (`_NET_WM_SYNC_REQUEST_COUNTER`) and names
   the protocol in `WM_PROTOCOLS` (`window.rs`; x11rb's `sync` feature,
   `Cargo.toml`). The counter goes with the window (`mod.rs`).
4. The window manager's request (a `WM_PROTOCOLS` client message) is kept
   (`event_processor.rs`), and the counter set to its value at the next
   `Window::pre_present_notify`, which zgx calls as it presents a frame,
   or at once when the configure that follows brings no new size (no frame
   follows it).

## X11: `_NET_WM_USER_TIME`

Window managers weigh a new window against focus stealing by the time of
the user's last input in its app (EWMH's `_NET_WM_USER_TIME`), which GTK
sets on its windows: one the app opens on its own, long after the user
turned to another app, need not take the focus. winit set none.

5. The connection keeps the last key or button press's time (`xdisplay.rs`,
   from `event_processor.rs`), and each press sets it on its window.
6. A new window carries it before it maps (`window.rs`); before any press,
   none, so an app's first window is focused as it launches.

## Windows: a wake without a second message round

The event loop's message-only window asked itself for an internal
`WM_PAINT` (`RedrawWindow(RDW_INTERNALPAINT)`) on every message it got,
a leftover of loops that learned their queue was empty from that paint.
0.30's loop learns it from `PeekMessage` (`dispatch_peeked_messages`), and
the window's `WM_PAINT` only validates and passes on. So each user event
(zgx's animation frame wakes, one a refresh) paid a `RedrawWindow` call and
a second message round (`PeekMessage`, `DispatchMessage`, `DefWindowProc`)
for nothing.

7. `thread_event_target_callback` no longer calls `RedrawWindow`
   (`src/platform_impl/windows/event_loop.rs`). zgx's `animation_workload`
   draws a frame in 2.43 million cycles where it took 2.67 (GPUI 2.69);
   the Windows pacing, latency, resize, drag, popup, window, desktop and
   touch probes pass as before.

## Windows: user events woken for by an event while the loop waits

A user event sent from another thread (`EventLoopProxy::send_event`) woke
the loop with a posted message: the sending thread paid a `PostMessageW`,
and the loop a `PeekMessageW` and a dispatch to take it. zgx's compositor
clock sends one each refresh, so an animating window paid them every
frame (2.3% and 4.3% of its CPU in `NtUserPostMessage` and
`NtUserPeekMessage`).

8. The event loop waits on an event of its own beside its message queue
   (`ProxyWake` in `src/platform_impl/windows/event_loop.rs`). A proxy
   sets it instead of posting while the loop is parked in its wait, and
   counts the user events it stands for; the loop, woken by it, hands the
   runner that many placeholders, as the messages would have. Away from
   the wait (handling events, or inside the system's modal loops: a live
   resize, a menu, `DoDragDrop`, which pump messages but never wait on the
   event), proxies post as before. A send racing the loop out of its wait
   may wake it both ways; a placeholder that then finds the channel empty
   does nothing, where it panicked (or, in `pump_events`, blocked).
   `animation_workload`: 2.400 million cycles a frame where it took 2.495
   (medians of 6 alternating runs); the pacing, latency, drag (62 frames
   in the second Windows had the drag) and resize probes as before. Tested
   by `crates/zgx-desktop/tests/proxy_wake.rs`: 2000 user events from
   another thread, in bursts and pauses, each arrives once and in order.

## Windows: the app mode read afresh when it changes

A window that follows the system's light or dark mode re-reads it on
`WM_SETTINGCHANGE` (`try_theme`, uxtheme's `ShouldAppsUseDarkMode`). uxtheme
answers that from the mode it read when the process first asked, until
`RefreshImmersiveColorPolicyState` reads it again, which nothing called: a
window switched in Settings while running kept its title bar and never
heard `ThemeChanged`, so its content kept its mode too.

9. `WM_SETTINGCHANGE` calls `RefreshImmersiveColorPolicyState` (uxtheme
   ordinal 104, `dark_mode::refresh_color_policy`) before `try_theme`
   (`src/platform_impl/windows/{event_loop,dark_mode}.rs`). Checked by
   `scripts/windows_settings_smoke.py --both`: switched both ways while
   the window runs, its title bar and its content follow within a second.

## X11: no waiting on an input method that has gone

libX11 learns that an input method's server went (ibus-x11 killed or
crashed) from its window's `DestroyNotify`, which only filtering an event
handles, and winit hears of it then (`xim_destroy_callback`). A synchronous
call in between, `XSetICFocus` (which reaches `XGetICValues`) or the preedit
spot's `XSetICValues`, waits in `_XimRead` for a reply that never comes, and
the event loop with it: on CI, native-ime's app hung as its window closed,
ibus-x11 gone (zgx#564). Before focusing, unfocusing or moving the spot, the
input method's server is asked for by its selection (`@server=name`, the
X server answers): owned by no one, the input method counts as destroyed,
as the callback would have it (`ime/mod.rs`, `InputMethod::server_name` in
`ime/input_method.rs`).

## X11: a wheel button counts on its press

XInput2 reports a pointer's wheel buttons (4 to 7) as presses and releases;
winit made a `MouseWheel` of each, so a pointer with no scroll valuators
(XTEST's, a remote desktop's) scrolled two lines a notch and Control with
the wheel zoomed two levels (zgx's zoom smoke). A wheel button's release is
dropped (`xinput2_button_input` in `x11/event_processor.rs`); an evdev
wheel's buttons are emulated and skipped as before, its valuators scroll.

## macOS: the Services menu on a window's selection

AppKit offers a view's selection to the Services menu (a translator, Make
New Sticky Note, Search With…) only if the view answers
`validRequestorForSendType:returnType:` and writes the selection on
`writeSelectionToPasteboard:types:`, and puts a returned text in its place on
`readSelectionFromPasteboard:`. winit's view did none, so Services never
acted on zgx's text.

10. `platform::macos::SelectionServices` and `set_selection_services`: an
    app's answers (the selected text, whether it can be replaced, replacing
    it), kept on the main thread; winit's view answers the three messages
    from it for plain text, and passes any other request to `NSView`
    (`view.rs`).

## Text dragged in from other apps

A browser's or an editor's selection dragged over a window carries text, and
an app's text fields take it where it is let go. winit hears only drags
carrying files (`WindowEvent::DroppedFile`): with no position while one
moves, and no way to say whether the window takes it there, so the pointer
shows no copy badge and every release would look taken.

11. `platform::text_drop::TextDrops` and `set_text_drops`: an app's answers,
    kept on the main thread, to a drag carrying text and no files: whether
    it would drop at a point (physical pixels in the content, as
    `CursorMoved`'s), that it left, and its text let go of at a point,
    whether it was taken. The answer is the platform's: the pointer's badge,
    and whether a release drops at all. The text is read once, as it drops.
    - macOS (`window_delegate.rs`): the window also registers
      `NSPasteboardTypeString`; `draggingEntered:` and a new
      `draggingUpdated:` answer a copy where the app would take the text,
      `performDragOperation:` hands it over, `draggingExited:` says it
      left. A drag of the app's own (it has a `draggingSource`) is the
      app's already, and is refused.
    - X11 (`dnd.rs`, `event_processor.rs`): XDND's `text/plain;charset=utf-8`,
      `UTF8_STRING` or `text/plain` (in that order) when no `text/uri-list`
      is offered: each `XdndPosition`, translated to the window, is asked
      and answered in `XdndStatus` (`XdndActionCopy`), not reset when
      refused; the text is converted once accepted, and `XdndDrop` drops
      it once it arrives (or as it does), `XdndFinished` saying whether it
      was taken.
    - Windows (`drop_handler.rs`): a data object with `CF_UNICODETEXT` and
      no `CF_HDROP`: `DragEnter` and `DragOver` answer `DROPEFFECT_COPY`
      where the app would take it (the point through `ScreenToClient`),
      `Drop` reads the text (`GlobalLock`, `Win32_System_Memory`) and
      answers whether it was taken, `DragLeave` says it left. The point
      `DragEnter`, `DragOver` and `Drop` get is a `POINTL` by value, as COM
      passes it (`definitions.rs` declared a pointer, never read before).
    Wayland's drags reach zgx's own data device, not winit.

## Where dragged files are (macOS, Windows, X11)

A drag holds the pointer: while files from Finder, Explorer or a file
manager are over a window, the window hears no mouse moves, only the drag's
own calls, and winit's `HoveredFile` and `DroppedFile` carry no position. An
app placing them at the pointer's last place dropped them on whatever it was
last over before the drag came in.

12. `window_delegate.rs`: a drag carrying files sends `CursorMoved`, at the
    drag's point in the content, as it enters (before `HoveredFile`), as it
    moves to a new place (`draggingUpdated:`), and as it drops (before
    `DroppedFile`), and `CursorLeft` as it leaves (after
    `HoveredFileCancelled`). `scripts/macos_native_checks_probe.py` drops
    two files on a box with the pointer last elsewhere.
    - Windows (`drop_handler.rs`): the same, from `IDropTarget`'s
      `DragEnter`, `DragOver` (when the point moved) and `Drop`, whose
      `POINTL` is in the screen's pixels (`ScreenToClient`), and
      `CursorLeft` after `DragLeave`'s `HoveredFileCancelled`. An OLE drag
      from Explorer sends the window no `WM_MOUSEMOVE` at all.
    - X11 (`event_processor.rs`, `dnd.rs`): each `XdndPosition` of a drag
      carrying files, translated to the window, sends `CursorMoved` when it
      moved (before the first `HoveredFile`, which waits for the data), and
      `XdndLeave` `CursorLeft` after `HoveredFileCancelled`; `XdndDrop`
      carries no point, so files drop at the last position's.
      `scripts/file_drop_smoke.py` drops with the pointer last over a card,
      at scale 1 and 1.5 (`file-drop-scaled`).
    Wayland's drags reach zgx's own data device, whose drop carries its
    point.

## X11: a closing window, or an ending app, waits on no input method

An input method's server that is alive but answers nothing (ibus-x11 hung,
or stopped) held an app's close forever. A window being hidden for its close
lost the focus, or took a FocusIn queued before, and `XSetICFocus` and
`XUnsetICFocus` reach `XGetICValues`, a round trip. Then the event loop's end
destroyed every context (`XDestroyIC` waits for its reply) and closed the
input method (zgx#564). A hidden window (`Visibility::No`, or gone) now takes
and gives up no input context focus (`EventProcessor::hidden` in
`event_processor.rs`). `Ime`'s drop no longer destroys contexts or closes the
input method: its connection closes with the event loop, and the server frees
them then (`ime/mod.rs`). zgx's `ime-server-stopped` probe stops ibus-x11 as
the app closes its focused window; the app exits.

## A native view's unhandled keys reach the app (macOS)

A web page that leaves a key alone hands it back: WebKit runs the key's
command up the responder chain (`doCommandBySelector:`) and, if it reaches
the end unperformed, re-sends the key to the app (`-[NSApplication
sendEvent:]`), which offers it to the menus and then sends it to the first
responder, whose responder chain passes it up. A page inside winit's view
never got its keys back: the view's `doCommandBySelector:`, an input
method's callback for its own keys, took every command coming up the chain
and performed nothing, so WebKit found each key handled.

13. `view.rs`: `doCommandBySelector:` and `keyDown:` coming up the chain
    from a view inside winit's (`is_first_responder` false) are not the
    view's own keys. The command goes on up the chain (NSResponder's), and
    the key is a `KeyboardInput` with no text and no input method, for the
    app's shortcuts.
14. `platform::macos::key_event_of`: the `KeyEvent` a key `NSEvent` is, as
    the view reports it, for an app asking whether a key a native view
    keeps (a terminal's) is one of its shortcuts.

`scripts/macos_key_routing_probe.py` (zgx) presses a menu key, a key the
page takes and a binding in no menu with a page and a terminal focused.

## Wayland: an input method hears what kind of text a field takes

`ImePurpose` had only Normal, Password and Terminal, so text-input-v3's
content purpose could not say email, URL, number, digits or phone (zgx's
`InputMode`, HTML's `inputmode`). `ImePurpose` (`#[non_exhaustive]`) gains
`Email`, `Url`, `Number`, `Digits` and `Phone` (`window.rs`), sent as
those content purposes (`seat/text_input/mod.rs`); other platforms ignore a
purpose, as before. zgx-desktop sets it as an editor takes the input method.

## macOS: a trackpad's swipe between pages

Safari, Finder and Xcode go back and forward on a trackpad's swipe between
pages: two fingers across (the system's default) or three, as System
Settings' Trackpad has it. Three fingers reach a view as `swipeWithEvent:`,
which winit's left to the responder chain; two fingers are scroll events,
which an app hands to AppKit's swipe tracking (`-[NSEvent
trackSwipeEventWithOptions:…]`) when the gesture starts where nothing
scrolls that way, and winit had no way to.

15. `platform::macos::NavigationSwipes` and `set_navigation_swipes`: an
    app's answer, kept on the main thread, to a swipe over a window that
    went back or forward, from where it began (physical pixels in the
    content, as `CursorMoved`'s). `view.rs`: `swipeWithEvent:` reports a
    three-finger swipe by its `deltaX` (to the right, positive: back);
    with no answer, or vertically, it goes up the chain as before.
16. `platform::macos::begin_navigation_swipe`: the gesture scrolling over a
    window swipes from its next scroll, for an app that found the gesture's
    first scroll moved nothing it is over (each scroll view at its edge) and
    something there navigates. As WebKit does once its web process says it
    took no scroll, the view starts AppKit's tracking from that later scroll
    of the gesture (`trackSwipeEventWithOptions` locked to its direction,
    clamped, −1 to 1), the fingers still down and
    `+[NSEvent isSwipeTrackingFromScrollEventsEnabled]` (the system's "Swipe
    between pages" set to two fingers; `swipes_from_scrolls`). The app
    hears its scroll gesture end there (a `MouseWheel` of nothing, phase
    `Ended`); the gesture's remaining scrolls, momentum's too, are the
    swipe's until a new touch or a mouse's wheel; one AppKit reports ended
    (the fingers carried it through) reaches `swiped`, back when it went
    right. A request no gesture takes is forgotten as its gesture ends.
    `wantsScrollEventsForSwipeTrackingOnAxis:` answers yes across while an
    app takes swipes. objc2-app-kit's `block2` feature, for the tracking's
    handler (`Cargo.toml`).

`scripts/macos_swipe_probe.py` (zgx) checks the three-finger path, and the
two-finger one where the system swipes with two fingers.
