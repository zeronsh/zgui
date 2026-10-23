# Components, children, providers, and slots

Application code should describe a tree of components and children. The scene is the retained rendering backend; manually appending scene nodes is not the intended translation of a view body. The composition API connects that declarative structure to the existing fine-grained reactive runtime, widget behavior, layout cache, and damage tracker.

A component is constructed once for each mounted instance. Its local state belongs to that instance and survives updates to text and other reactive properties. Reading a signal while constructing a component does not subscribe an enclosing dynamic branch to that signal. Reactive text and dynamic selectors install their own subscriptions. This separation prevents an unrelated title or list update from rebuilding an entire application.

## Rust view syntax

A view function returns a child tree. Components create state and resolve services during mounting; ordinary container builders describe their children:

```rust
use zgui::{
    compose::{View, button, column, component, text_signal},
    style::Styled,
};

fn counter() -> View {
    component(|cx| {
        let count = cx.state(0_u32);
        column().gap(8.0).p(12.0)
            .child(text_signal({
                let count = count.clone();
                move || format!("Count: {}", count.get())
            }))
            .child(button().child(zgui::compose::text("Increment")).on_click(move || {
                count.update(|value| *value += 1);
            }))
    })
}
```

`Ui::mount(view)` adds a retained view tree and returns a handle; `Ui::render(view)` replaces the root children after the new tree mounts successfully. If construction panics, newly mounted nodes/resources are cleaned up and the previous root remains available. Conditional replacement has the same ownership cleanup and preserves its previous branch on construction failure. This does not roll back arbitrary application state writes.

Initial reactive bindings belong to the public mount that created them, including bindings created by its initial conditional or keyed children. If one panics, that mount is removed even when an outer batch or active effect deferred the callback until after `mount` returned. Unrelated mounted siblings survive. Such late failures do not restore root children that `render` already replaced. A panic in a subsequent reactive update leaves the existing view mounted; application code must correct the failing dependency before retrying. Panics propagate to the caller rather than becoming native `Application::run` errors.

The UI owns the mounted tree, so dropping a handle does not remove the view. Call `handle.unmount()` to dispose it explicitly. In a native application, `WindowContext::render(view)` supplies the window's task runner automatically.

A component can call `cx.service::<Model>()` to obtain the nearest typed provider, `cx.state(initial)` for local state, and `cx.tasks()` for component-owned asynchronous work. For a headless host, wrap the tree in `provide(TaskRunner::from_executor(executor), tree)`. The [complete model example](../crates/zgui/examples/model.rs) preserves the supplied language sample, including its yielding handlers, error cases, and independent Count and Title subscriptions. Run it with `cargo run -p zgui --example model`.

The native version is `cargo run -p zgui-desktop --example components`. It renders the same Model, Count, Title, and Dialog body, with fluent background, rounded corners, padding, spacing, hover, pressed, and keyboard-focus styles. The style closures return sparse overrides, so a focus border does not replace an unrelated hover background. Layout and paint changes continue through the retained scene's existing invalidation paths.

```rust
use zgui::{compose::{button, text}, scene::Color, style::Styled};

let action = button()
    .w(72.0).h(40.0).rounded(8.0)
    .bg(Color(42, 53, 76, 255))
    .hover(|style| style.bg(Color(58, 76, 108, 255)))
    .focus(|style| style.border(2.0).border_color(Color(112, 174, 255, 255)))
    .child(text("+"));
```

## Mapping the proposed language

| Language construct | Meaning in the Rust component tree |
| --- | --- |
| `service Model` | An ordinary Rust type containing shared list and signal handles. |
| `provider Shared` / `provide Shared` | A typed provider enclosing a subtree. Descendants resolve the nearest provider of that type. |
| `view App` | A mounted component with its own initialization, local state, children, and resources. |
| `state done: Int = 0` | A signal created once during the component's initialization. |
| `column { ... }` / `row { ... }` | Container elements with child elements. |
| `text expression` | A reactive text binding that tracks the expression's actual reads. |
| `slot body` | Caller-supplied children with the caller's lexical provider context. |
| `on click` | An activation handler, shared by pointer, keyboard, and accessibility activation. |
| `task.yield()` | A local future that yields to the executor before continuing. |
| `catch` | Ordinary Rust `Result` matching with the language's error mapping preserved. |

Provider lookup and slot ownership are separate concerns. Slot content captures the caller's service context, so an internal provider in a dialog cannot silently change what `Model` means inside the caller's body. Once mounted, that body belongs to its mounted subtree: removing the dialog also disposes the body, its subscriptions, handlers, and owned tasks.

The sample's list has an initial length of zero and a logical capacity of four. Three sequential appends run inside one reactive batch after yielding. A batch delays notifications; it is not a transaction. A subsequent append sequence can insert one item and then return the capacity error, leaving four items. Editing a row does not invalidate length readers, and writing the same row value twice produces only one notification. These behaviors are preserved independently of how the view syntax is written.

## Update and ownership model

The retained component tree does not perform a whole-tree render after each state write. A text expression updates its existing text node; a structural condition reevaluates its selector and replaces only the selected child subtree. Component initialization runs without capturing dependencies for the surrounding selector. Effects created by that component still track their own reads normally.

State that must survive unmounting belongs in a longer-lived service, usually keyed by model identity. Local component state is intentionally released when its owning component is removed. The same rule applies to event subscriptions and asynchronous work owned by a component.

The Rust API is a target for a future template compiler, not an implementation of the parser. Language-level error typing, name resolution, and generated source diagnostics remain compiler responsibilities. Application authors can use Rust functions and typed data directly while retaining the same component, child, provider, and slot boundaries.

Headless `TaskRunner::from_executor` uses a weak spawning capability: a running component task can schedule another task without borrowing the executor during its own poll. New work starts on a later tick, and retaining the runner does not keep a disposed executor alive.

## Selectable text

Display text is not selectable unless asked, as in native applications.
`.selectable(true)` on any view makes its text, and everything inside it,
selectable the way a web page is (CSS `user-select: text`):

```rust
use zgui::{compose::prelude::*, widgets::Ui};
let mut ui = Ui::new(640., 480.);
ui.mount(column().w(600.).gap(12.).selectable(true).children([
    text("Heading").text_size(24.),
    rich_text().w_full().text_wrap(true)
        .child(text_span("One selection spans "))
        .child(text_span("every text node").font_bold())
        .child(text_span(" in document order."))
        .into(),
    text("Kept out of selections").selectable(false),
]));
ui.prepare_frame();
```

One selection runs from an anchor to a focus across the region's text
nodes, paragraphs, list rows, nested layouts and inline spans alike, in
document order. Dragging selects; past the last line it reaches the end, and
above the first the start. A double press selects a word and a triple press a
paragraph (the text between hard line breaks), and dragging after either
extends in whole words or paragraphs. Shift extends from the anchor.
⌘A / Ctrl+A selects the region last pressed in, unless focus is in an editor;
⌘C / Ctrl+C copies it as plain text. The pointer shows an I-beam over the
region's text. Shift with the arrows extends the focus from the keyboard, from
one text node into the next: by a grapheme, by a word with Option (macOS) or
Control, to the visual line's edge with ⌘ (macOS) or Home/End, a line up or
down at the caret's x, and to the region's start or end with ⌘↑/⌘↓ (macOS) or
Control+Home/End; the innermost scroll viewport follows the focus. Without a
selection, or with an editor focused, those keys are left to the app.
`.selectable(false)` keeps a part out (CSS `user-select:
none`); an inner `.selectable(true)` is part of the outer region.

Links and buttons inside still click; a drag that starts on one moves past
four points, cancels its press and selects instead. Editors inside select
their own text, and pressing one ends the region's selection, as does a press
on anything that is not a control (buttons, links and menu items keep it, so
a Copy button can copy it). A press that selects by dragging does not also
click an enclosing clickable view.

Copied text keeps each node's own line breaks and joins soft wraps. Between
nodes it follows layout, as CSS margins set blocks apart: comparing the boxes
holding the two nodes in their nearest shared ancestor, it puts a tab between
boxes side by side, a blank line between boxes set apart by half the text's
size or more (paragraphs), and otherwise a line break. Hidden, transparent and
unselectable text, inline icons and an ellipsis are left out; truncated text
copies whole. `Ui::selected_text`, `text_selection`, `select_text`,
`select_all_text` and `clear_text_selection` read and set the selection, and
`text_selection_revision` changes whenever it does. `select_word_at(x, y)`
selects the word at a window point as a double press there does (a touch
screen's long press), returning false where no selectable text is.
`selected_html` gives it as HTML (paragraphs, line breaks, bold, italic,
code, underline and strikethrough runs); the desktop host's ⌘C / Ctrl+C and
the menu's Copy (`Platform::set_clipboard_rich_text`) put both on the clipboard where the platform takes HTML (macOS, Windows, X11) and
plain text alone on Wayland.

Assistive technology reads the selection from the region: its semantic node
(a document, as a browser's page is, unless the view has a role of its own: a window or a scroll view keeps it) carries
`SemanticNode::document_selection`, which the desktop host projects as an
AccessKit text selection over the labels' text runs, so a screen reader reads
the selected text across nodes. Each label in the region is a paragraph to it,
as a browser's blocks are: its last text run ends with a line break that is
not in the label's text (a position after it is the label's end), so reading
and word or line navigation stop between labels instead of running them
together. A selection that assistive technology sets
across the region's labels selects as the pointer would
(`Ui::select_accessible_text`).

Holding a drag past an edge of a scroll viewport (`scroll`, `scroll_x`, a
virtual list) scrolls the innermost one holding the selection's focus and
extends over what scrolls into view, faster the farther the pointer is, as an
editor's drag does; it stops at the viewport's end and on release. A
selection in a virtual list outlives the rows it recycles: an end in an
unmounted row keeps the row's index and path, the rows between stay selected,
and a row that mounts again shows its part. Copy takes the mounted rows as
they show (the visible ones and overscan). Give the list `.row_text(|index|
Some(text))` (`None` past its end) and it copies the rows out of view too, in
order and whole, so select-all and copy on a long transcript copies all of
it. Without `row_text`, nothing extra is kept, and only mounted rows copy.

A secondary press (a control-click on macOS) on selected text opens a menu
with Copy and Select All, unless the region has a `context_menu` of its own;
elsewhere the press goes on to an enclosing view's menu. Copy uses the
`PlatformService` clipboard the desktop host provides. On Linux the host also
makes the selection the primary selection (middle-click paste), on X11 and
Wayland, once a gesture ends (`Ui::is_selecting_text` says whether one is
still making it).

The highlight is styled as CSS's `::selection` is. Its background is the
theme's `selection` token (`selection.inactive` while the window is at the
back). The glyphs inside it repaint in `selection.text` (`selection.text.inactive`),
which is transparent by default, so they keep their own colours. Its rects
round by `selection.radius` (0). Override them for a subtree with
`theme_scope(ThemeOverride::new().with(tok::SELECTION_TEXT, color), view)`,
or on one region with `.selection_color(color)` and `.selected_text_color(color)`
(`_when` variants follow reactive reads), which hold in either window state. A
glyph recolours when its centre lies in a highlight rect, from the glyphs
already laid out on the GPU and software paths: changing a colour or the
selection reshapes and lays out nothing, and repaints only the highlighted
rects. Measured with retired instructions (`selection_headless`), a drag costs
308.3 k per move with text recolouring against 307.8 k without it, and an idle
frame with a selection still costs nothing.

On macOS the desktop host gives `selection` and `selection.inactive` the
system's own selection colours (`NSColor.selectedTextBackgroundColor`,
`unemphasizedSelectedTextBackgroundColor`). They follow the user's accent,
re-read when the window activates or the appearance changes. They apply over
the built-in themes only, so an app theme or `theme_scope` that sets these
tokens keeps its own (`Themes::set_system_tokens`). Windows and Linux keep the
theme's colours for now.

The system's display settings reach the themes in the same way, reported as
`Themes::system_preferences()` (`SystemPreferences`, tracked, as CSS's
`prefers-reduced-motion`, `prefers-contrast` and `prefers-reduced-transparency`).
On macOS, Reduce motion sets the `motion.duration.*` tokens to zero. Increase
contrast makes `border.subtle` and `border` as strong as `text.secondary`,
`border.strong` as `text.primary`, and `text.tertiary` as `text.secondary`.
Reduce transparency makes `glass.tint` the raised surface and `glass.blur` zero,
and any view with a backdrop blur paints its background opaque without the blur
(a clear one gets the raised surface). The host follows changes as they happen
(`NSWorkspaceAccessibilityDisplayOptionsDidChangeNotification`). Windows reports
its animation and contrast settings with the tokens it already sets.

The highlight paints under the glyphs in the theme's `selection` colour, or
`selection.inactive` while the window is in the background (desktop hosts call
`Ui::set_window_active`). It follows layout and reshaping. Removing a selected
node drops its part, and unmounting the region ends the selection.

Text that is not selectable costs nothing: no listeners, nodes or layouts. A
selectable region adds one listener; a press shapes the text it lands on, and
the layouts a gesture moves over are kept until it ends. Changing the
selection repaints only where a highlight changed, and an unchanged one
repaints nothing. Hit testing maps the pointer through local transforms (scale, rotation, skew), as the scene's does.

## Editable text

`text_input(label, value)` and `text_area(label, value)` mount the same retained editor used by the native host. The first argument is its accessible name, and the `Signal<String>` is synchronized in both directions. IME preedit remains local until committed. Selection, undo, clipboard, and native IME placement use the mounted editor's lifecycle.

```rust
use zgui::{compose::prelude::*, text_layout::FontFamily, widgets::Ui};
let mut ui = Ui::new(640., 480.);
let name = ui.signal(String::new());
let notes = ui.signal(String::new());
ui.mount(column().gap(12.).children([
    text_input("Name", name.clone()).w(320.),
    text_area("Notes", notes).size(480., 180.).text_wrap(true).font_family(FontFamily::Monospace),
]));
name.set("Ada".into());
ui.prepare_frame(); // Native hosts do this automatically before rendering/input.
```

Multiline editors handle Page Up/Down and Shift+Page Up/Down using the padded
viewport height, retaining one visual line of overlap when space permits.
Paging and Up/Down share the preferred horizontal caret position, including
through shorter lines. Paging scrolls with the caret and clamps at document
boundaries; Shift retains the selection anchor. It does not change committed
text or undo history, and remains available in read-only editors. Single-line
editors and Control/Alt/Meta-modified paging leave the default unhandled so
application or ancestor behavior can respond.

Pointer selection supports Shift-click to extend the current anchor, double-click
to select a Unicode word-boundary segment, and triple-click to select a visual
line (all text in single-line fields). Punctuation and whitespace are selectable
segments; word boundaries expand to whole graphemes. Visual-line selection
excludes hard newline terminators. Dragging after a double/triple-click extends
in whole word/line units and reverses around the original unit's opposite edge.
Shift takes precedence over the click count. Selection remains available in
read-only editors and cancels preedit without editing the committed model.

Holding a captured selection pointer outside an editor autoscrolls the document
and extends the selection even without further pointer movement. Single-line
fields scroll horizontally; multiline fields also scroll vertically, with
horizontal scrolling disabled by wrapping. Word/line drags keep their selection
units. Speed increases with distance outside the viewport and is bounded; a
scheduler stall contributes at most 50 ms of motion. Hit testing clamps to the
visible edge before extending selection, rather than jumping to an arbitrarily
distant document position.

The core retains at most one interaction deadline, armed only while scrolling
can advance. Release, cancellation, loss of capture/focus, disabling, removal,
changed external text, reentry or an exhausted scroll direction stops scheduled
work. Read-only fields remain selectable. Desktop hosts drive this automatically
and cancel captured interactions when hidden or suspended. Custom/headless hosts
combine `Ui::next_interaction_deadline()` with their event-loop deadline and call
`Ui::advance_interactions(now)` when due; `Ui::cancel_interactions()` cancels
captured gestures when a host becomes inactive. The fallible advance method
returns whether a tick ran. Idle editors schedule no interaction wakeups.

The native host forwards pointer modifiers and tracks clicks per target and
button within the system's double-click time and four logical pixels, cycling
through counts 1, 2, 3, then 1. The time is the person's setting, as native
controls follow it: macOS's and Windows' mouse settings, GNOME's `double-click`
and KDE's `DoubleClickInterval` through the settings portal, read again when
the window is activated (`InputDispatcher::set_double_click_time`; 500 ms until
a host sets it). Movement outside that distance, leaving the target,
cancellation, deactivation, disabling or removal resets the chain. Event handlers
can inspect `EventContext::pointer_modifiers()` and `click_count()`; counts apply
to pointer-down events. Custom hosts use `Ui::dispatch_with_modifiers` (or its
fallible counterpart). Existing event literals remain unchanged, and ordinary
`Ui::dispatch` retains unmodified single-click behavior for synthetic input.

A secondary click (Control-click on macOS) in an editor opens its context menu, as a
browser's text fields do: Cut, Copy, Paste and Select All, through the window's
clipboard (`Platform`). Each is disabled where it would do nothing: Cut and Copy with
nothing selected or in a password field, Cut and Paste in a read-only one, and Paste
with an empty clipboard. A click outside the selection first puts the caret under the
pointer (on macOS it selects the word there, as AppKit's text views do). A
`.context_menu(..)` of the app's own replaces it.

`.spellcheck(true)` checks an editor's spelling as it is typed, as HTML's
`spellcheck` attribute does, through the system's checker (`Platform::misspellings`;
macOS's `NSSpellChecker`, Windows' `ISpellChecker`, the Linux desktop's Enchant). Misspelled words get a dotted line
in the theme's `danger` colour, except the word the caret is in, which is still being
typed. An edit checks again only the paragraphs it touched, and the marks elsewhere
move with the text. On a misspelled word the context menu starts with the checker's
guesses (choosing one replaces the word), then Learn Spelling and Ignore Spelling
(Ignore lasts for the session). It is off by default and never applies to a password
field; without it, nothing is checked or stored.

Use `.read_only(true)` for selectable output or `.read_only_when(move || locked.get())` for a reactive editing policy. Read-only editors remain focusable and support selection, navigation, scrolling and copy. Typing, paste, deletion, undo/redo, IME composition and accessibility value replacement cannot change their text; cut copies the selection without deleting it. External model updates remain permitted. Entering read-only mode cancels existing preedit while retaining focus, selection and history. The native host disables IME for the read-only editor, and accessibility exposes its read-only state while retaining selection actions. These options apply to editor roots, including components that return an editor; they do not propagate from containers. Direct access to the low-level `TextEditor` remains an application mutation API.

Committed editor text uses LF line endings: CRLF and lone CR become LF, then `text_input` removes line breaks. This applies to initial values, external model updates, typing, paste, accessibility replacements, and IME commits. The bound signal receives the canonical text. Tabs remain intact; ordinary text events still reject other control characters as a whole. Native preedit stays unmodified so its cursor offsets remain valid. Canonical-equivalent model updates preserve selection, preedit, and history, and initial normalization does not create an undo entry.

Undo goes back a run at a time, as AppKit's text views do. Consecutive typed
graphemes join one step, as do consecutive deletions one way (Backspace, or
forward Delete). A caret move, a selection change, a line break, a different
kind of edit, a paste or multi-character insert, or undo/redo ends a run.
Option+Backspace / Control+Backspace delete a word; ⌘⌫ and ⌘⌦ on macOS delete
to the line's start or end (at its edge, the line break). On macOS the Control
keys work as in every Cocoa text field: A and E go to the line's start and end,
F, B, N and P move a character or line, D and H delete forward and back, and K
deletes to the line's end. In read-only editors the deleting ones are refused
like other edits.

External model replacements record a single changed UTF-8 span after removing the common prefix and suffix, so appending a small token does not retain another whole-document undo snapshot. The default limits remain 100 entries and 4 MiB of inserted/deleted payload **per undo or redo stack**; metadata and allocator overhead are additional. A changed replacement still moves the caret to the document end, and undo restores the prior selection. Canonical model ingress borrows the signal text instead of cloning the full document. Comparing replacements still takes time proportional to document length; this is not a rope or constant-time append API.

Selection and read-only refreshes retain unchanged paint text and semantic value storage. The semantic text-input update reuses its string capacity when committed text changes. These avoid document copies in the core refresh path; cold text shaping and native accessibility projection have separate allocation costs.

Editors accept the ordinary dimension, padding, decoration, typography, focus, and disabled styles. Their internals remain stable when layout reallocates their width. `text_area` preserves explicit line breaks and scrolls horizontally for long lines by default. Add `.text_wrap(true)` to wrap at the allocated content width. This inherited style also supports reactive changes; visual wrapping never inserts model newlines. Single-line `text_input` stays unwrapped. Both editor constructors are leaves and reject child views. Unmounting releases input handlers and model subscriptions. Custom headless hosts should call `Ui::prepare_frame()` before flushing or rendering the scene so editor viewports follow allocated layout.

### Virtual lists

`virtual_list` mounts fixed-height rows from a reactive count and an indexed key
lookup. It evaluates keys only for the visible range plus overscan, so scrolling
100,000 items does not allocate or scan 100,000 keys. Ordinary fluent styles set
the viewport; its allocated content size controls the range after layout,
including changes from parent layout or padding.

```rust
use zgui::{compose::prelude::*, widgets::Ui};
let mut ui = Ui::new(640., 480.);
let offset = ui.signal(0.0);
let list = virtual_list(
    offset.clone(), 28.0, 2,
    || 100_000,
    |index| index,
    |index, key, _cx| {
        row().h(28.).child(text_signal(move || {
            format!("Item {key}, position {}", index.get())
        }))
    },
).w(640.).h(480.);
ui.mount(list);
ui.prepare_frame();
offset.set(28_000.0); // Bidirectional logical-pixel position; automatically clamped.
```

Row factories receive the component context, inherited providers and typography.
A key that remains visible keeps its component, local state, subscriptions and
node identity when reordered. Its `Signal<usize>` updates to the current index.
Rows leaving the overscan range are unmounted; persistent row state belongs in
the model, indexed by key. Visible keys must be unique. The list scrolls with the
wheel and passes scrolling to an enclosing viewport when it reaches an edge.
Headless hosts call `Ui::prepare_frame` before flushing or inspecting allocated
geometry; native hosts do this automatically.

For rows whose allocated heights change, use `variable_virtual_list` with a
shared `VariableHeights` index. The same component context, fluent viewport
styles, retained keys, scrollbars and keyboard navigation apply:

```rust
use zgui::{compose::prelude::*, widgets::Ui};
let mut ui = Ui::new(640., 480.);
let offset = ui.signal(0.);
let heights = VariableHeights::new(&ui.runtime, 100_000, 28.);
ui.mount(variable_virtual_list(
    offset.clone(), heights.clone(), 2,
    |index| index,
    |index, key, _cx| {
        row().w_full().h_full().child(text_signal(move || {
            format!("Item {key}, position {}", index.get())
        }))
    },
).size(640., 480.).scrollbar(true).keyboard_navigation(true));
ui.prepare_frame();
heights.set_height(3, 84.).unwrap();
ui.prepare_frame();
```

Heights must be finite and positive. `try_new`, `set_height` and `resize`
return `HeightError` for invalid heights, invalid indices or allocation failure;
equal writes do not notify subscribers. Batch related mutations through the
runtime. Point updates preserve the first visible index and its intra-row
position, clamping that position if the row shrinks. Explicit changed scroll
requests win over compensation. Count changes preserve/clamp the pixel offset.
Heights belong to **positions**: reorder them alongside keyed data. The viewport
does not relocate its anchor by key after a reorder.

With `variable_virtual_list`, applications supply allocated heights; use
`measured_virtual_list` below to populate the cache from natural child layout. `row_offset`, `row_at`,
`row_height` and `content_height` expose indexed geometry. Page keys move by
viewport pixels through the prefix index, progressing at least one row even
when the focused row is taller than the viewport. The native
`variable_virtual_list` example exercises asynchronous growth and anchoring.

The fixed-height path uses constant metadata space. Variable heights use about
20 bytes per row plus vector capacity and reactive handles; point updates and
prefix lookup take O(log n), and geometry iteration takes O(log n + mounted
rows). Growing the count initializes only added prefix-tree entries; it does
not rebuild the previous rows. Shrinking occasionally releases excess capacity.
Coordinates remain finite but have ordinary floating-point precision limits at
very large offsets.

### Naturally measured virtual rows

`measured_virtual_list` shares the same child/component architecture while
allocating row heights from their natural layout. Give it a `VariableHeights`
cache initialized with a reasonable estimate, then compose normal children:

```rust
use zgui::{compose::prelude::*, widgets::Ui};
let mut ui = Ui::new(640., 480.);
let heights = VariableHeights::new(&ui.runtime, 100_000, 72.);
let message = ui.signal("A streamed message".to_owned());
let read = message.clone();
ui.mount(measured_virtual_list(
    ui.signal(0.), heights.clone(), 2, |index| index,
    move |_, key, _cx| {
        let read = read.clone();
        column().w_full().p(8.).gap(4.)
            .child(text(format!("Message {key}")))
            .child(text_signal(move || read.get()).w_full().text_wrap(true))
    },
).size(640., 480.).scrollbar(true).keyboard_navigation(true));
ui.prepare_frame();
message.set("A longer streamed message that reflows naturally within each row.".into());
ui.prepare_frame();
```

The row wrapper measures natural children at the allocated viewport width,
including their margins and padding. A layout pass publishes mounted measurements
as a batch; equal sizes leave the index alone. Retained rows resize without
reconstructing their components. Wrapped text, additional children and native
width changes all update mounted heights. Keyboard destinations remain pending
until measurement settles, so newly discovered tall rows cannot discard focus.

Unvisited or offscreen rows retain **estimates**, including after width, font or
content changes. Their cached heights refresh when they mount again. Scrollbar
extent and pixel offsets are approximate until relevant rows have been measured;
this API does not lay out the entire collection to obtain an exact total. Heights
are positional, so reorder cached heights with data. Use a separate cache for
independently sized list instances; competing width-dependent measurements in one
cache can produce a feedback error.

Measured mode applies a **one logical pixel minimum** to this shared cache at
mount time and to later height writes/growth. Empty content therefore occupies
one pixel; remove truly hidden items from the model. Applying the floor scans the
cache once in O(n); repeated mounts of that cache do not repeat the scan. This
prevents undersized estimates from mounting an entire collection. Overestimated
rows use geometric discovery, doubling the tentative range until measured rows
cover the viewport, then pruning to visible rows plus overscan. Construction may
temporarily exceed the final mounted count. Cyclic content/height dependencies
still report the bounded layout feedback error described below.

## Allocation feedback errors

A size-dependent row constructor or disposal callback can repeatedly change its own viewport. `Ui::try_prepare_frame` returns `LayoutFeedbackError` after at most 64 stabilization passes instead of hanging. Reentrant preparation defers notifications to the outer call. Correct the model dependency before retrying; application mutations are not rolled back. `try_dispatch` also reports this error without delivering input against unstable geometry. The convenience `prepare_frame` and `dispatch` methods panic with the same diagnostic, while native drawing and input propagate it through `Application::run`.

```rust
use zgui::{widgets::Ui, input::InputEvent};
# fn prepare() -> Result<(), zgui::widgets::LayoutFeedbackError> {
let mut ui = Ui::new(640., 480.);
ui.try_prepare_frame()?;
ui.try_dispatch(InputEvent::PointerMove { x: 10., y: 10. })?;
# Ok(())
# }
```

Keyed reconciliation stages new children before removing previously mounted ones. If a constructor panics, staged nodes, subscriptions and retained resources are disposed, while the prior child set, order and instance-local state remain intact. A later changed key set can retry construction. This guarantees ownership cleanup, not rollback of arbitrary model writes performed by user constructors. Old and staged children coexist temporarily during successful construction; retained size remains proportional to the requested child set. The lower-level `KeyedChildren` API follows the same initializer-failure policy.

A checkbox binds a `Signal<bool>` in both directions. Pointer activation on either
its indicator or label, Space, Enter, and accessibility activation toggle the
same value. External writes update its indicator and accessible checked state.
The indicator is drawn as the platform draws one: a rounded 16-point box,
outlined while off and filled with the theme's accent while on, its checkmark
drawing on over 150 ms when a `TaskRunner` is present (at once without one).
`.mixed(signal)` shows a dash and tells assistive technology "mixed" while the
signal is true. A disabled checkbox dims to 45% unless it has a disabled style of
its own. The box keeps its size whatever the label's typography.

```rust
use zgui::{compose::prelude::*, widgets::Ui};
let mut ui = Ui::new(400., 200.);
let notifications = ui.signal(false);
let view = ui.mount(
    column().gap(12.).text_size(16.).child(
        checkbox("Enable notifications", notifications.clone())
            .focus(|style| style.border(1.).border_color(rgb(0x5599ff)))
            .disabled_style(|style| style.opacity(0.4)),
    ),
);
notifications.set(true);
```

Checkboxes inherit typography and accept ordinary layout, decoration, reactive,
hover, active, focus, and disabled styles. `.disabled_when(...)` prevents user
activation while preserving external model updates. An optional `.on_click(...)`
callback observes the value **after** the toggle. The label is both visible text
and the accessible name. A checkbox owns its indicator and label; additional
`.child(...)` or `.children(...)` calls are rejected at mount. Compose surrounding
content in a row or column. Removing the view disposes its subscriptions and
input listeners.

`slider(label, value, min..=max)` is a horizontal numeric control backed by a
`Signal<f32>`. For example, `slider("Volume", volume, 0.0..=100.0).w(320.).h(44.)`
creates a retained slider with an accessible name. Place a separate `text` beside
it when a visible label is wanted. A slider owns its rail and thumb and rejects
additional children. Root backgrounds, padding, borders, sizing and interaction
styles use the same fluent API as other views; inherited text color styles the
thumb. The rail uses the theme hover color and its fill uses the theme accent.

The rail follows the actual allocated content box, including reactive dimensions,
parent layout constraints and padding. Pointer dragging captures the pointer;
arrow keys and accessibility increment/decrement actions move by one percent of
the range, and Home/End select its endpoints. Disabled sliders ignore input while
still following external model changes. The range must be finite and increasing;
external NaN values become the minimum and values outside the range clamp to an
endpoint. Internal `f64` arithmetic supports the entire finite `f32` range.

`progress(label, value)` displays determinate progress from a `Signal<f32>` in
`0..=1`. It exports a read-only accessible progress indicator; the accessible
label is not drawn. Values outside the range clamp to an endpoint and NaN
normalizes to zero, including external writes. The component owns its fill and
rejects extra children. Use a surrounding row or column for a visible label.

```rust
use zgui::{compose::prelude::*, widgets::Ui};
let mut ui = Ui::new(640., 480.);
let fraction = ui.signal(0.25);
ui.mount(column().gap(8.).text_color(rgb(0x5ea5ff))
    .child(text("Downloading"))
    .child(progress("Download", fraction.clone())
        .w(320.).h(16.).p(2.).bg(rgb(0x232e40))));
fraction.set(0.75);
```

The fill inherits `text_color`; `bg` decorates the track. Dimensions and padding
follow allocated layout. A full-width fill translates under a rectangular content
clip, so changing only the value performs no layout work. Equal values remain
idle; resizing updates retained geometry. As with other views, `rounded` decorates
the root background and does not imply rounded clipping of descendants.

Decoded images are leaf views:

```rust
use std::sync::Arc;
use zgui::{compose::prelude::*, image::ImageData, widgets::Ui};
let mut ui = Ui::new(400., 200.);
let source = Arc::new(ImageData::new(1, 1, vec![40, 120, 220, 255]).unwrap());
let current = ui.signal(source);
ui.mount(image_signal("Blue swatch", move || current.get())
    .w(120.).h(80.).p(8.).bg(rgb(0x202020)).overflow_hidden());
```

`image(label, Arc<ImageData>)` retains a fixed source; `image_signal(label, source)`
tracks reactive reads and replaces the decoded pixels without replacing the view
or its bitmap node. The label is the accessible image description. Additional
children are rejected, including children attached through a component's root.
Decoding and file/network I/O happen outside the core view constructor; callers
supply immutable decoded RGBA data and may share it between views.

An unspecified dimension uses the corresponding source pixel dimension plus
padding. Source dimension changes update intrinsic sizing in either direction.
Explicit sizes and layout constraints allocate the outer box; pixels stretch to
its content box after padding by default. `.object_contain()` preserves aspect
ratio and centers the full image; `.object_cover()` fills the content box and
clips the excess. `.object_fit(ObjectFit::None)` uses natural pixel dimensions,
and `ObjectFit::ScaleDown` contains the image without upscaling. Fitting changes
bitmap placement inside the allocated box, not the outer intrinsic sizing rule.
Background,
border, shadow, clipping, opacity, blur and interaction styles apply to the outer
retained view. A same-identity source write is idle; replacing pixels with another
image of the same dimensions requires paint without layout. Unmounting releases
owned subscriptions and image references.

`scroll(offset)` mounts ordinary retained children inside a vertical viewport:

```rust
use zgui::{compose::prelude::*, widgets::Ui};
let mut ui = Ui::new(500., 400.);
let offset = ui.signal(0.0_f32);
ui.mount(scroll(offset).w(320.).h(240.).p(12.)
    .child(column().gap(8.).children((0..20).map(|n| text(format!("Row {n}"))))));
```

The default viewport is 320 × 240 logical pixels. Padding surrounds a clipped
content box, and children stack vertically. Put a styled `column` or `row` inside
when content needs its own gaps, alignment or other layout. Root styles decorate
and size the viewport. Content width follows the viewport's allocated content
width; content height comes from ordinary child layout, including reactive text,
conditional views and image sizes. No explicit content-height argument is needed.

The offset follows external writes and clamps when content or viewport dimensions
change. NaN becomes zero and infinities clamp to an endpoint. Wheel input scrolls
this viewport while it can move and bubbles to an outer scroller at an edge;
a delta crossing an edge is consumed by the inner viewport for that event.
The desktop host turns a mouse wheel's lines into pixels as browsers do:
40 per line on macOS and Linux (Chromium's and WebKit's line step), and on
Windows, which reports notches, 100 per notch (the system's default three lines).
Trackpads' pixel deltas pass as they are.

`Ui::set_elastic_scrolling(true)`, which the macOS host turns on, gives scroll views
macOS's and Safari's rubber band. A trackpad gesture pushed past an edge stretches
the view: its content moves on, ever less for each further pixel (AppKit's curve),
while the offset an app reads stays in range. Pulling back unwinds the stretch
before the content scrolls, and lifting the fingers springs it back, critically
damped, in about 0.4 s. A flick's momentum that reaches an edge bounces. It
stretches the view the gesture is latched to; otherwise, as a page bounces rather
than a box in it, the outermost view under the pointer that scrolls along that
axis. The host reports the gesture's phases as `InputEvent::ScrollGesture`. A mouse
wheel, and scrolling the app does, never stretch. Off, a dispatch checks one flag.
Virtual lists do not stretch yet. Disabled viewports ignore input while still following model updates. Offset-only
changes translate the retained subtree without layout or remounting children.
All children remain mounted offscreen; use `virtual_list` for large collections
that need bounded row construction and memory.

When keyboard navigation, accessibility or a direct focus request moves focus to
a descendant, an ordinary `scroll` viewport reveals it with the smallest vertical
movement needed. Nested viewports reveal from the inside outward, including focus
requests inside reactive batches. Padding remains outside the visible content
area. An oversized focused control aligns its nearest edge, or stays put if it
already spans the viewport. Manual wheel and model offset changes remain in
place until a new focus transition; there is no continuous focus snap-back.
This behavior applies to ordinary retained `scroll` children, not offscreen rows
that a `virtual_list` has unmounted.

`scroll_x(offset)` provides the same retained viewport behavior on the horizontal
axis. Its ordinary children stack in a row, their intrinsic widths determine the
scroll extent, and content height follows the allocated viewport height. Use a
nested styled `row` when content needs gaps or alignment. For example:

```rust
use zgui::{compose::prelude::*, widgets::Ui};
let mut ui = Ui::new(500., 200.);
let offset = ui.signal(0.0_f32);
ui.mount(scroll_x(offset).w(320.).h(100.).p(8.)
    .children((0..8).map(|n| button().w(100.).h(70.).child(text(format!("Card {n}"))))));
```

Horizontal viewports consume finite horizontal wheel deltas; vertical-only wheel
input bubbles to an enclosing vertical scroller. Accessibility scroll actions use
the viewport's declared axis. Focus reveal follows that same axis, including
mixed horizontal/vertical nesting and batched focus requests. Both scroll variants
retain offscreen children and apply offset changes through translation without
layout. Both variants hide their scrollbar by default; `.scrollbar(true)` adds an
interactive overlay without changing content allocation.


Use `.scrollbar(true)` on `scroll`, `scroll_x`, `virtual_list`, `variable_virtual_list` or `measured_virtual_list` for an interactive
overlay along the content box's trailing edge. The 8-pixel track is inside padding;
the proportional thumb is at least 24 pixels, bounded by the track length. Root
opacity, clipping and disabled state also apply to the scrollbar. The track uses
the theme hover color, the thumb uses the accent color, and keyboard focus changes
the thumb to the theme text color. This opt-in property also passes through
component roots; attaching it to another kind of view is a mount error.

`Ui::set_overlay_scrollbars(true)` makes scrollbars behave like macOS's overlay ones
(and GTK's, or a browser's on a trackpad Mac). A scrollbar is hidden at rest: it
takes no space and no hits, so the pointer reaches the content under it, while
assistive technology still has it. It shows when the user scrolls its view (wheel,
keys, a drag). It stays a second after the scrolling stops, then fades out over
200 ms. With the pointer on it, or the keyboard focus, it stays, and the track
shows only under the pointer. Scrolling done by the app (`offset.set`,
`follow_end`) does not show it, as a page's script scrolling does not. The fade is
stepped through `Ui::next_interaction_deadline`, so an idle window schedules
nothing. The desktop host follows macOS's "Show scroll bars" setting
(`NSScroller.preferredScrollerStyle`) at startup and whenever the window comes to
the front. Elsewhere, and by default, a scrollbar shows whenever its view has
something to scroll.

Dragging the thumb captures the pointer and preserves the initial grab position.
Clicking elsewhere on the track pages by one viewport. Focused scrollbars support
arrow keys (40 logical pixels), Home/End, PageUp/PageDown and accessible numeric
value/increment/decrement actions. Offset changes translate the thumb without
layout. When overflow disappears the scrollbar stops painting, releases focus and
capture, leaves the accessibility tree and permits hits on underlying content;
it returns if the extent grows again. Ordinary children and virtual rows keep
their existing retention behavior.

Opt into virtual-row keyboard navigation with `.keyboard_navigation(true)`:

```rust
use zgui::{compose::prelude::*, widgets::Ui};
let mut ui = Ui::new(500., 400.);
let offset = ui.signal(0.0_f32);
ui.mount(virtual_list(offset, 28., 2, || 1_000_000, |index| index,
    |_, key, _| text(format!("Item {key}")))
    .w(320.).h(240.).keyboard_navigation(true).scrollbar(true));
```

The list viewport and its mounted row wrappers become focusable. From the viewport,
ArrowUp/Down enters the first visible row; from a focused wrapper those keys move
one row. Home/End jump to the collection endpoints and PageUp/PageDown move by a
viewport's row count. The target range is mounted before its wrapper receives
focus, including when navigation occurs inside a reactive batch. Work and mounted
rows remain bounded by the viewport and overscan; endpoint navigation does not
construct or scan every item. Focus paints a theme hover background and a 2-pixel
accent marker over the row's leading edge, preserving interior child hit testing.

Interactive descendants retain their own keyboard handling. For example, arrow
and Home/End keys in a text editor edit its text instead of navigating the list.
Tab continues ordinary focus traversal through the viewport, mounted wrappers and
interactive descendants. Accessibility exposes row list-item roles, one-based
positions and the total set size. Disabled lists reject navigation.

Reordering a key within the retained range preserves its wrapper, local state and
focus while updating its index. If that key moves outside the mounted range, or
the focused row is removed or scrolled out, focus returns to the viewport. The
framework deliberately does not scan the whole collection to find an offscreen
key's new index. A subsequent navigation key chooses a new row. This opt-in method
is only valid on virtual lists (including a component that returns one); leaving
it off preserves the existing focus behavior of application-created row contents.

Modal and anchored popup panels use ordinary component children:

```rust
use zgui::{compose::prelude::*, widgets::Ui};
let mut ui = Ui::new(600., 400.);
let open = ui.signal(false);
let launch = open.clone();
let dismiss = open.clone();
ui.mount(column()
    .child(button().child(text("Settings")).on_click(move || { launch.set(true); }))
    .child(modal("Settings", open).w(360.).gap(12.)
        .child(text("These children retain their local state while closed."))
        .child(button().child(text("Done")).on_click(move || { dismiss.set(false); }))));
```

`view.tooltip("Copy link")` shows a tooltip over a view, as a browser's and the
system's behave. It appears 300 ms after the pointer comes to rest on the view, or at
once within a second of another tooltip (as a tooltip provider's `timeout`). It goes
when the pointer leaves or presses, or when a key is pressed anywhere. One shows at a
time, fading in over 140 ms and out over 100. It takes no focus and lets the pointer
through. Text takes the theme's look (an overlay card, small text);
`Tooltip::from(view)` shows a view of the app's own. `.placement(..)` moves it (above
and centred by default), and `.interactive(true)` makes it a hover card: it stays
while the pointer is on the view or the card, with a moment's grace to cross, takes
clicks and typing, and closes on Escape. Delays and fades run on the window's
`TaskRunner`; without one (a headless `Ui`) it shows and goes at once. A key closes
it through one document listener that exists only while a tooltip shows. Call
`.tooltip(..)` last, since what follows styles the tooltip's panel. zgui-script's
`tooltip` property is this.

`modal(label, open)` centers its panel in the current window and dims the backdrop.
`popover(label, open, anchor)` keeps the anchor in ordinary layout and places an
anchored modal popup below it, flipping above when needed and clamping to the
window. Its `.child`/`.children` populate the popup panel. Anchor scrolling,
translations, content sizing and window resizing update placement automatically.
Neither constructor requires parent IDs or application scene mutation.

Fluent styles and `.id(...)` apply to the panel. A private positioning node keeps
user `.translate(...)` additive. Default panels have 16-pixel padding, a theme
surface background and rounded corners; modal width defaults to 360 pixels and
popup width to 240. Unspecified height follows children. Default maximum dimensions
fit the window with a small margin; explicit maximum-size styles override them.
Use a scroll child when panel content needs scrolling. Panels escape the clipping
and layout constraints around their logical owner through an internal overlay
portal. Provider lookup, lexical slots, local state and resource ownership remain
with that owner. `ViewHandle::node()` identifies the logical owner; panel IDs
resolve through the same handle's `find` method.

Both kinds trap focus while open, initially preferring an enabled child control.
An empty panel provides fallback focus. Escape dismisses the top active popup;
backdrop clicks dismiss by default, configurable with `.dismiss_on_backdrop(false)`.
Blank panel space does not count as backdrop, and dismissal does not activate a
control beneath it. Closing restores eligible prior focus. Nested dialogs close
with their parent. Sibling dialogs stack in opening order; closing or removing a
lower sibling preserves the upper dialog and repairs its eventual focus return.
Disabling a logical ancestor closes its open portal and resets the open signal;
reenabling the ancestor requires an explicit new open request.

Closed panels retain children, state and subscriptions while leaving paint, input
and accessibility inactive. Removing the logical owner removes its portal,
subscriptions and focus scope immediately. Constructor failures clean up staged
portals. Replacing an application tree can replace an open dialog without leaving
a stale scope. These popovers are focus-trapping dialog popups. Use `menu` for action menus
with menu-specific roles and keyboard item navigation.


Action menus share the retained portal lifecycle:

```rust
use zgui::{compose::prelude::*, widgets::Ui};
let mut ui = Ui::new(600., 400.);
let open = ui.signal(false);
let show = open.clone();
let saved = ui.signal(false);
ui.mount(menu("Document actions", open,
    button().child(text("Actions")).on_click(move || { show.set(true); }))
    .p(4.).gap(2.)
    .child(menu_item("Save").on_click(move || { saved.set(true); }))
    .child(menu_item("Export").disabled(true)));
```

`menu(label, open, anchor)` exposes an accessible menu, and `menu_item(label)`
creates a focusable action with its own visible label. Items use ordinary fluent
styles and `.on_click`; additional children on an item are rejected. Components,
providers, conditional views and keyed collections may produce menu items.
Activating an item closes its enclosing menu chain and restores focus **before**
running the callback, so the action may safely open another dialog. Trigger
semantics retain their existing role/name and expose a menu popup with expanded
state. Disabled items are skipped by this framework's navigation policy.

Opening prefers the first enabled item. Up/Down wrap through enabled items;
Home/End choose the first/last item. Printable keys perform case-insensitive prefix
search with a 750-millisecond continuation window and a 256-byte UTF-8 storage
bound; repeated letters cycle matching items. Escape closes the current menu.
Tab/Shift-Tab close the menu chain and continue ordinary traversal from its trigger,
including within an enclosing dialog. Interactive non-item children keep their
own editing keys. If a focused keyed item disappears, menu navigation remains
available through the active scope.

Long menus automatically scroll within the available window height. Wheel input
moves retained content and keyboard focus reveals the selected item. Width follows
the panel's allocated content width, and reactive gaps/alignment reach the menu
content. Menus remain vertically arranged; row-style layout is not a menubar API.
Use `submenu("More", child_open).children(...)` within a menu for a cascading
menu. Its retained trigger fills the parent row and displays a trailing chevron.
Fluent styles and `.id(...)` describe the child panel; `.trigger_id(...)` identifies
the owned trigger. `.disabled(...)` and `.disabled_when(...)` disable the trigger
and close an open child. Ordinary components, providers and keyed children work
inside both levels.

Right, Enter or clicking the trigger opens its child without closing the parent;
Left or Escape closes just the child and restores its trigger. Opening a sibling
closes the previous child. A child action closes the whole menu chain before its
callback. Clicking an ancestor item or sibling trigger works in one gesture;
clicking outside all menus closes the chain and consumes the gesture. The optional
`.dismiss_on_backdrop(false)` keeps outside clicks from closing the menu. Submenus
open beside the full trigger row, flip left when necessary, and follow scrolling,
transforms and window resizing. Accessibility exposes child menus beneath their
logical parent menu despite their separate portal placement. Opening is explicit
by click or keyboard; hovering does not automatically open a child.


The [WAI-ARIA menu pattern](https://www.w3.org/WAI/ARIA/apg/patterns/menubar/)
was used as a keyboard/semantics reference. zgui's navigation skips disabled items;
the APG web pattern keeps disabled items focusable. This native API does not claim
full APG conformance.


## Accessibility

Controls report their role, name and state to assistive technology
themselves (AccessKit: VoiceOver, Narrator, Orca). These say what a plain view
means, as ARIA's attributes do in HTML; a view that sets none pays nothing:

- `.accessible_label(name)`: the name read for it (`aria-label`).
- `.role(Role::…)`: what it is taken for, keeping what it does. Use
  `ListBox` and `ListBoxOption` for a choice among clickable rows, and
  `Toolbar`, `List` / `ListItem`, `TabList` / `Tab`, `Status` and so on.
  Buttons, options, tabs, list items and headings are named by their text,
  leaving out empty and hidden text.
- `.heading(level)`: a section title (`h1`…`h6`). VoiceOver's rotor lists
  headings, and a heading in a selectable document keeps its text in the
  document's.
- `.selected_when(|| …)`: whether an option, tab or item is chosen
  (`aria-selected`), followed.
- `.live(Live::Polite)` or `Live::Assertive`: announce changes to the text in
  it (`aria-live`). `Role::Status` is polite by default.
- `.accessible_description(text)`: read after the name (`aria-description`).
- `.accessible_hidden()`: left out with its subtree (`aria-hidden`), for
  decoration.
- `.focus_group(FocusGroup::Vertical)` (or `Horizontal`, `Both`): one Tab stop
  for the focusable views inside, as ARIA's composite widgets are. Tab lands on
  the one last focused there (else the selected one, else the first), and the
  arrows along the axis, Home and End move among them. A field inside keeps its
  arrows. `ListBox` is vertical and `Toolbar` and `TabList` horizontal without
  asking, and a virtual list with `keyboard_navigation(true)` is one stop too.
- Escape in a text field (with no composition to cancel) goes on to the dialog
  or menu around it, which closes and gives focus back.

```rust
use zgui::{compose::prelude::*, semantics::Role};
# let mut ui = zgui::widgets::Ui::new(300., 200.);
let inbox = ui.signal(true);
let read = inbox.clone();
let mailboxes = column().role(Role::ListBox).accessible_label("Mailboxes").child(
    row()
        .role(Role::ListBoxOption)
        .selected_when(move || read.get())
        .child(text("●").accessible_hidden())
        .child(text("Inbox")),
);
# ui.mount(mailboxes);
```

## Owned input handlers

Attach `.on_event(|event| { ... })` to any view for pointer, wheel, keyboard,
text/IME, focus and activation events. The callback receives `&mut EventContext`;
inspect its `event` and `phase` (`Capture`, `Target`, or `Bubble`). Ancestors receive
capture and bubble phases; the hit/focused root receives the target phase. Portal
panels route through their physical overlay rather than their logical owner.

```rust
use zgui::{compose::prelude::*, input::{EventPhase, InputEvent, Key}, widgets::Ui};
let ui = Ui::new(400., 300.);
let value = ui.signal(0);
let counter = value.clone();
let control = div().focusable(true).on_event(move |event| {
    if event.phase == EventPhase::Target
        && matches!(event.event, InputEvent::KeyDown { key: Key::ArrowRight, .. })
    {
        counter.update(|value| *value += 1);
        event.prevent_default();
    }
});
```

Repeated `.on_event(...)` calls append listeners in declaration order. Component,
provider and slot wrappers append their listeners after those declared by the
underlying view, preserving the component's behavior. Listeners live with the
mounted root and are released on unmount, including keyed/conditional removal.
Disabled roots and descendants follow the dispatcher's inherited disabled policy.

`.on_document_event(|event| { ... })` hears every event the document is given
while the view is mounted, before it is routed. That includes one that goes nowhere,
such as a key pressed with nothing focused. It is a browser's
`document.addEventListener`, for what answers input anywhere (a tooltip that any key
closes). It only observes: the event still goes where it goes. With no such
listener, dispatch checks an empty list.

`prevent_default()` cancels default actions without stopping propagation;
`stop_propagation()` stops travel to another node, while
`stop_immediate_propagation()` also stops remaining listeners on the current node.
Built-in handlers follow user listeners, including editor handlers. A prevented
activation does not invoke `on_click`, and a prevented editor input event does not
edit the value. Ancestor capture can intercept input before it reaches a child. `focus()`, `capture_pointer()` and
`release_pointer()` request the dispatcher's corresponding operations.
Prevented wheel, navigation, slider and scrollbar actions also leave their model
unchanged. Focus/blur notifications and pointer-release/cancel cleanup still run;
preventing a release must not leave a control dragging or visually pressed.

On desktop, preventing `KeyDown` also suppresses the printable `Text` event
associated with that native keystroke and its clipboard shortcut default. Raw
keyboard cancellation does not suppress a separate IME commit; handle
`ImeCommit` explicitly when needed. Ordinary editor Space inserts through `Text`,
while its key release suppresses button activation. Focus and pointer-release
cleanup still run after default prevention.

An event listener alone does not make a passive view focusable. `.focusable(true)`
opts its root into focus and normal Tab traversal, without changing its semantic
role; `.focusable(false)` overrides a control's default focusability. A wrapper's
explicit focusability overrides the inner view. Use `.focus(...)` to provide a
visible focus style for custom controls. Input handlers do not replace a custom
control's responsibility to expose suitable accessibility semantics.


Inherited `.line_height(pixels)` applies to labels, boxed text and editable text.
Use `.line_height_normal()` to reset an inherited explicit value; ordinary sparse
patches preserve the inherited metric. Reactive changes update geometry while
retaining component state and editor selection. See the [line-height styling
contract](styling.md#line-height) for exact pixel spacing and clipping behavior.


Component disposal detaches scene ownership before listener captures and retained
resources are destroyed. Destructors can remove the same subtree or its ancestor
without leaving stale scene IDs. Task completion checks and task-guard destruction
run outside the component task-list borrow, so a custom executor can reenter the
task capability; new live guards remain owned until unmount.
