# zgx patch to accesskit_windows 0.34.0

Vendored from crates.io `accesskit_windows` 0.34.0 (MIT OR Apache-2.0; the
published crate ships no license files) and applied through
`[patch.crates-io]` in the workspace `Cargo.toml`. Changes are marked
"zgx patch" in the source.

## Why

Gaps Narrator meets in zgx's windows, and a cost every window paid (item 6). AccessKit gives a heading's level to UI Automation only as the generic
`Level` property and the ARIA `level`, never as `HeadingLevel` (UIA
property 30173). Narrator's heading navigation (scan mode H, the headings
list) and Windows' other screen readers go by `HeadingLevel`, so zgx's
headings read as "heading" but cannot be moved between. Chromium and
WebView2 set it. 0.35.1 and upstream main still leave it out.

## What

1. `heading_level` in `src/node.rs`: a heading's level 1 to 9 as
   `HeadingLevel1` to `HeadingLevel9`, exposed as
   `UIA_HeadingLevelPropertyId`. Other nodes keep UIA's default,
   `HeadingLevel_None`.
2. A test, `src/tests/heading.rs`, reading it through the UIA client.
3. `name` in `src/node.rs`: a table's cell or header (`Cell`, `GridCell`,
   `ColumnHeader`, `RowHeader`) with no label, or an empty one, is named by
   its visible descendants' text, as ARIA names cells and headers and as
   Chromium does. AccessKit derives a name from contents only for buttons,
   check boxes, links, menu items and radio buttons, so a zgx grid's cells,
   whose text is a child, had no UIA Name: Narrator's table navigation read
   them as blank. Tested by `src/tests/cell_name.rs`.
4. A form field's error (`aria-invalid`, `aria-errormessage`), from the
   vendored accesskit_consumer (vendor/accesskit_consumer/ZGX_PATCH.md):
   `IsDataValidForForm` is false while the field is invalid; its error
   message's text follows the description in `FullDescription`, so
   Narrator reads it with the field ("Email, edit, An email is required,
   Invalid"), and `DescribedBy` gives the nodes that describe it
   (`src/node.rs`, `description`, `is_data_valid_for_form`, the
   `DescribedBy` property). Not `ControllerFor`, as Edge gives
   `aria-errormessage`: Narrator reads an edit's `ControllerFor` as a
   suggestion list ("Suggestions available"). The crate's `accesskit_consumer` dependency
   is the vendored one (`path`), so it builds alone too.
5. Events a client listens for, and no others. UIA tells a fragment root
   which events clients add handlers for (`IRawElementProviderAdviseEvents`,
   in `src/node.rs`), and the context counts them (`Context::advise`).
   After a tree update, events no client listens for are dropped
   (`Context::wanted`; text changes before their elements are made).
   - Focus changes are always raised: legacy WinEvent clients follow them.
   - Everything is raised while a screen reader runs (`SPI_GETSCREENREADER`):
     it may take events through the MSAA bridge, where no handler is advised.
   - Why: the touch keyboard's host (TextInputHost) attaches to the focused
     window, listening for focus, structure, selection, menus and a few
     properties, but not text or name changes. A dashboard's live labels
     raised about 1,200 text changes and 830 name changes a second anyway,
     and UIA called back into the provider about ten times for each: about
     18,000 calls and 30 ms of the UI thread a second, for no one. Chromium
     does the same through `AdviseEventAdded`.
   - Tested by `context::tests`. A client that subscribes later receives
     what it subscribed to: a native one counting name changes on that
     dashboard received 2458 in 3 s, against 2489 before the patch.
6. Only UI Automation's request for the root builds the tree. `handle_wm_getobject`
   asked the activation handler for it on any `WM_GETOBJECT` for the client
   area: `UiaRootObjectId`, MSAA's `OBJID_CLIENT`, and the ids 0 and above.
   - Why: when a window is shown, Windows' text services (msctf, through the
     IME glue's `CtfImeDispatchDefImeMessage`) call `AccessibleObjectFromEvent`
     on it in-thread. That sends `OBJID_CLIENT`, then 0, 1, 0 and 2, on every
     desktop, with no assistive technology running. Through accesskit_winit
     that was `InitialTreeRequested`, so every Windows window built and
     synced its full tree from startup. In zgx's lab and vlist benches that
     cost 15–22% of each frame against `ZGX_ACCESSIBILITY=0`.
   - Now those requests get the placeholder root (role `Window`, no
     children), and the activation handler is not asked
     (`Context::tree_requested`). `UiaRootObjectId` asks at once, as before,
     whether the adapter is inactive or holding the placeholder. Narrator,
     Accessibility Insights, and NVDA on UIA windows ask for the root that way.
   - A client reading past the placeholder asks for the tree: its children,
     its focus, an element at a point, a pattern, or a property beyond what
     a probe reads of a window (role, name, class, process, place, enabled
     and focus state). The placeholder posts `activation_message()`
     (`AccessKitActivate`) to the window. The subclassing adapter handles it
     with `Adapter::activate`, on the window's thread, and a window using
     `Adapter` directly passes the message to it. The read itself is
     answered from the placeholder. The tree replacing it raises a focus
     event (`update_if_active`'s placeholder path), and the client reads
     again.
   - `ZGX_ACCESSIBILITY_LOG` logs the placeholder's requests and what read
     past it.
   - Tested by `adapter::placeholder_tests`.
     NVDA is not tested yet. For a window with a UIA provider
     (`UiaHasServerSideProvider` sends `UiaRootObjectId`), it should take the
     same path as Narrator; an NVDA that reads such a window through MSAA
     instead takes the read-past-the-placeholder path.
7. A text range finds text within it: `ITextRangeProvider::FindText`, which
   upstream leaves a stub returning nothing (`text.rs`).
   - It finds the first match of the text within the range, or the last with
     `backward`, compared without case with `ignoreCase`, from one character
     boundary to another. It returns a range over the match, or none where
     there is no match.
   - Why: Windows Speech Recognition's "select <word>", Narrator's find, and
     UI Automation test tools search a field's text this way. They got
     nothing from every zgx field and document.
   - Tested by `tests::find_text` (text, case, backward, within the range,
     none, bounds). `scripts/windows_uia_find_smoke.py` checks it on a real
     zgx window.
   - To offer upstream: it uses only the crate's own range and position API.

Run the crate's tests on Windows with
`cargo test --manifest-path vendor/accesskit_windows/Cargo.toml --locked`.
Upstream's `simple::focus` brings its window to the foreground, and fails
when Windows refuses that (a run from a background terminal).

Drop the patch when an upstream release does the same.
