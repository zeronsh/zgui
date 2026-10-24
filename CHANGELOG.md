# Changes

zgx is 0.1 and used from git; this lists what changed for apps, newest first.

## Unreleased (October 2026)

### Changed defaults: what to check in an existing app

**Children stretch across their container**, as in CSS's flexbox (`align-items: stretch`). A `column`'s children are as wide as it, a `row`'s as tall, unless they size themselves on that axis. `div` and zgx-script's `ui.column`, `ui.row` and `ui.div` do the same.
- *Migrate:* where buttons, chips or labels now span a column, or boxes in a row are taller than their content, state `.items_start()` (`items = "start"`) on the container, or give the child a size.

**Children shrink to fit**, as CSS's `flex-shrink: 1`, each no further than its content's narrowest (`min-width: auto`: a text's longest word, a box's content). A box that clips (a scroll view, `overflow_hidden`) may shrink to nothing. Before, nothing shrank and a too-wide row overflowed.
- *Migrate:* where a fixed-size box (a sidebar, an icon, a 120-point tile) now narrows in a small window, state `.shrink_0()` (`shrink = 0`). Where a truncated label or a pane should shrink past its content, state `.min_w(0.)` (`min_w = 0`), as CSS needs `min-width: 0`.

**`cmd` in a script's shortcuts is Control off macOS** (`ui.command { key = "cmd+n" }`): Command on macOS, Control on Windows and Linux, as each platform's menus use them. `super` and `meta` name the key itself.
- *Migrate:* nothing for menus written for macOS; a shortcut that meant the Windows key writes `super`.

**A window runs the way the user's language does:** right to left under Arabic, Hebrew, Persian or Urdu.
- *Migrate:* an app whose interface is not translated can set `.direction(Direction::Ltr)` (`dir = "ltr"`) on its root to keep it left to right. `ZGX_DIRECTION=rtl` tries a layout either way.

**Focus moves on when what has it leaves** (a toolbar's button unmounting with its message): to the nearest focusable ancestor, else the next Tab stop, else the one before, rather than to the window.

**Escape in a text field** with no composition to cancel goes to the dialog or menu around it, which closes.

**The component library is a crate of its own, `zgx-components`.** The data grid, tree, calendar, date picker and time field, combobox, select, tabs, toasts, command palette, segmented control, stepper, disclosure, accordion and separator moved out of `zgx::compose`; the primitives they are made of (text, editors, buttons, checkboxes, sliders, progress, scroll views and virtual lists, menus, popovers, modals and tooltips, selection, drag) stay. zgx-script's `ui.*` for them is unchanged, through its `components` feature (on by default).
- *Migrate:* add `zgx-components` beside `zgx` and import them from it (`use zgx_components::prelude::*;` beside `zgx::compose::prelude::*`). The local-time hook is `zgx::local_time::set_offset` (zgx-desktop sets it). A component's own semantics are set with the new `NodeHandle::update_semantics`.

### Layout

- CSS's automatic minimum (`min-width`/`min-height: auto`) for flex items; text's min-content is its longest word.
- Right to left (`View::rtl`, `direction`; `dir` in scripts): rows run from the right, columns and text start at the right, wrapping rows mirror, scroll views start at their right and scrollbars sit at the start edge. Padding, margins and insets given as left and right stay physical. Logical edges follow the direction: `ps`/`pe`, `ms`/`me`, `start`/`end` (`inset_start`/`inset_end` in scripts).
- Flexed sizes snap to a 1/256-point grid, so shared-out room no longer drifts.
- Container queries in scripts: a ref's `width` and `height` are followed, settling in the frame they change.

### Text and editing

- Display text is selectable on opt-in (`selectable`): across nodes, by word and paragraph, with Shift and the arrows, copied as text and HTML, styled as CSS's `::selection`.
- Editors: spellchecking as you type (`spellcheck`, with the system's checker on each platform), a context menu, undo in runs, the primary selection on Linux, and arrows that move through right-to-left text the way it shows.
- IME: the clause being converted is underlined and takes the candidate window. Known gap: reconversion is not offered (winit's text input client reports no surrounding text).

### Accessibility

- Roles, headings with levels, selection, live regions, descriptions and hidden decoration (`role`, `heading`, `selected`, `live`, `description`, `aria_hidden`). Labels can follow state.
- Focus groups (`focus_group`; listboxes, toolbars and tab lists without asking) are one Tab stop with arrow keys. A keyboard-navigable list is one stop (`ui.list { keyboard = true }`).
- VoiceOver finds headings (accesskit_macos is vendored with `AXHeading`). Narrator moves between them by level. Empty text leaves the tree.

### Controls and platforms

- A component library, `zgx-components`: a data grid (virtual rows, sorting, moving and resizing columns, cell editing, row selection, copying as a spreadsheet's), a tree view (lazy children), a calendar, a date picker and a time field, a combobox (async, virtual suggestions), a select with typeahead, tabs (closable, reorderable, overflowing), toasts, a command palette, a segmented control, a stepper, a disclosure, an accordion and a separator, each keyboard complete, accessible and right-to-left aware.

- Checkboxes drawn as the platform draws them, with a mixed state and a disabled look.
- zgx's own tooltips (`View::tooltip`), and scrollbars that hide at rest where the system's do.
- Touch and pen: a finger taps, holds, drags and pans; a long press on text selects its word.
- macOS: rubber-band overscroll; Reduce motion, Increase contrast and Reduce transparency reach the themes; popups reach past their window.
- Windows: Inset and Hidden title bars with snap layouts, Acrylic and Mica glass, WebView2 web views, the system's text size and accent, one instance per application ID, and [shipping notes](docs/windows.md#shipping-a-zgx-app-on-windows).
- Linux: the desktop's dark or light, high contrast, reduced motion, text size, accent and interface font; KDE's wheel speed; app IDs on Wayland and X11; zenity file dialogs without a portal.

### Scripting

- `zgx-run` runs a Luau app (`app.luau`, and a `zgx.json` for its window), and `ui.window` declares more windows.
- `zgx lint` finds slow or wrong patterns, and runs on every reload in debug builds.
- Long list states are cheap to filter: a filter's rows come back as the state's own.
- A `ui.computed` read in a handler or `ui.batch`, right after a write it depends on, has the new value (it had its old one until the handler returned).

### Performance

- Mounting allocates a third as much. A column taller than its window no longer measures every row twice.
- Text that changes every frame shapes and allocates less. Idle Linux apps' D-Bus threads sleep.
