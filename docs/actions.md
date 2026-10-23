# Typed actions and keyboard contexts

Components register keyboard bindings and action handlers without owning the
native event loop. Bindings are resolved along the focused component's ancestor
path before editor mutation and ordinary key handling. A modal focus scope bounds
that path. `on_action` receives target/bubble events; call `prevent_default()` to
mark the action handled, and `stop_propagation()` to stop ancestor handlers.

```rust
use zgui::{
    actions::{Action, ContextPredicate, KeyBinding, KeyContext, Keymap, Keystroke},
    compose::prelude::*,
    input::{Key, Modifiers},
};
struct Save;
let bindings = Keymap::new().bind(
    KeyBinding::new(
        [Keystroke::new(Key::Function(5), Modifiers::default())],
        Action::new(Save),
    ).unwrap().when_predicate(
        ContextPredicate::parse("Editor && mode == insert && !Terminal").unwrap()
    )
);
let view = column().keymap(bindings)
    .on_action(|_: &Save, cx| {
        // Save the application's document here.
        cx.prevent_default();
        cx.stop_propagation();
    })
    .child(button().child("Document").keymap(
        Keymap::new().key_context(KeyContext::new().flag("Editor").attribute("mode", "insert"))
    ));
```

`Keymap::context("Editor")` adds a flag, and `KeyBinding::when("Editor")`
requires that flag. Parsed predicates additionally support `==`, `!=`, `!`,
`&&`, `||`, parentheses and ancestor `>` relationships. `&&` requires its
positive operands to match the same context node; `Workspace > Editor`
explicitly names an ancestor relationship. Negation excludes a match anywhere
in the considered path. Configuration parsing rejects malformed expressions,
inputs over 4096 bytes and nesting beyond 64 expressions.

A key sequence has one through four strokes. Closer scopes and later bindings
have precedence. A later longer sequence can defer an earlier exact binding;
a newer exact binding overrides older extensions. Ambiguous input has a single
one-second interaction deadline. Completing a sequence dispatches candidates in
priority order until a handler prevents the default. `Action::new(NoAction)`
explicitly suppresses a binding. Unhandled candidates fall back to ordinary
key handling.

Timeout or mismatch first tries a deferred exact command, then replays the
original key and native text events if unhandled. A mismatch subsequently tries
the new stroke independently. Replayed input skips keymap resolution, respects
editor/default prevention and stops if focus changes or ownership disappears.
The desktop host preserves the actual native text payload while waiting, avoiding
reconstructed characters or duplicate insertion. Custom hosts should dispatch
native `Text` events when `has_pending_keys()` is true even if the prefix key was
prevented, and schedule `next_key_deadline()` / `advance_key_sequence()`; `Ui`'s
existing interaction scheduling already integrates these methods.

Focus changes, native blur, pointer press, composition, registration changes,
disabling, subtree disposal and host interaction cancellation clear pending
input. Idle views have no key deadline or polling. F1–F35 and Insert survive the
desktop mapping alongside existing character/navigation/editing keys. On a layout of other letters (Russian, Greek, Hebrew, Arabic), a
key pressed with Ctrl or Cmd (and not Alt, which with Ctrl is Windows' AltGr)
whose character is outside ASCII arrives as the US letter on that key, as
browsers and GTK take shortcuts: Ctrl and the C key copy whatever the layout
types there, and a binding of `"c"` matches it. Typing is untouched. The Rust
configuration API is independently designed; this is not a GPUI keymap JSON
parser or a promise of identical precedence for every mixed-context rule set.

`InputDispatcher::dispatch_action(&scene, Action::new(value))` lets native menu
handlers invoke the same typed route. Its `DispatchResult::default_prevented`
indicates whether a listener marked the action handled. With no focused node,
it targets the active focus scope or scene root; applications can attach a
root listener or retain their native-menu fallback.

Keymaps retain action values and unregister with their owning subtree. Ordinary
component wrappers merge their keymaps with their inner component. Callback
execution and captured-value destruction occur outside keymap registry borrows.

Run `cargo run -p zgui-desktop --example actions` for the editor/F5/chord example.
It also accepts native file drops. `InputEvent::FileHover`, `FileDrop` and
`FileHoverCancelled` route to the region under the host's last logical pointer
position. Paths remain `PathBuf`; the native host sends one event per path,
matching winit's event interface. This is not a batched `ExternalPaths` payload,
and it does not implement initiating an operating-system file drag. Native
backends that do not report a current pointer position during external drags
can limit precise region targeting; synthetic routing tests are not native
cross-desktop validation.

Validation: `crates/zgui/tests/actions.rs` covers scoped commands before editor
mutation, bubbling and native dispatch, chord cancellation, reentrant unmount,
file payload routing and context predicates. Unit tests cover expiry, malformed
configuration and captured-action destructor reentry. The native X11 harness
`scripts/actions_smoke.py` verifies F5, function-key and character chords, timer
wakeup and unmatched prefix replay against the actual edited model.
