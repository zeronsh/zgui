# Typed retained drag and drop

A drag source produces a Rust value after a primary-button gesture travels four logical pixels. A normal click still activates the source. A target accepts only its payload type and can apply an additional predicate:

```rust
#[derive(Clone)]
struct Card { name: String }
let source = div().size(100., 80.)
    .child(text("Drag me"))
    .on_drag(|| Card { name: "Card".into() })
    .drag_preview(|card: &Card| div().p(12.).child(text(&card.name)))
    .on_drag_end(|_: &Card, accepted| println!("accepted: {accepted}"));
let destination = div().size(240., 120.)
    .on_drop_when(|card: &Card| !card.name.is_empty(), |card: &Card, _| {
        println!("Dropped {}", card.name);
    });
```

The preview is an ordinary component tree, retained for the gesture and moved by compositor translation. Its top-left follows the pointer minus the grab point (`DragEvent::grab_x`/`grab_y`, the press position within the source), so a same-sized preview stays under the spot that was pressed. `drag_preview_at_cursor(offset, build)` instead places a badge-style preview at a fixed offset from the pointer. Custom gestures calling `EventContext::start_drag_from(payload, x, y)` supply the press position; `start_drag` uses the current pointer. It is excluded from drag hit testing and disposed on completion, cancellation or source removal. Sources and destinations obey visibility, disabled state and focus scopes. The nearest accepted destination wins; acceptance is checked again at release. Escape, native cancellation, hiding the source, host interaction cancellation and removal release the session. A completed drag suppresses source click activation. Raw `InputEvent::Drag` supplies Start/Move/Over/Leave/Drop/End phases for application hover styling and custom gestures; `EventContext::accept_drag` explicitly accepts an Over event.

Payloads remain inside one window and do not initiate native OS drag operations. No gesture timer is allocated and idle windows do not wake for drag handling.

`on_files_drop` receives native paths grouped at the event-loop dispatch boundary. Winit 0.30.13 emits the X11 XdndDrop path list synchronously and queues the macOS pasteboard filename list in one callback, so grouping introduces no timeout. The grouping boundary is a dispatch batch, not a transaction identifier supplied by winit. Legacy per-path `FileDrop` events remain separately available through `on_event`. Group assembly rejects a whole batch above 1,024 paths or 1 MiB of encoded path bytes and emits `FilesDropRejected`; it never delivers a truncated grouped payload. Native file events from winit carry no pointer position; their targeting uses the host's last logical pointer location. Wayland transport and native external-file validation have separate platform evidence.

`cargo test -p zgui --test input drag_drop::` checks typed acceptance, rejection, nested destinations, source/target removal, hidden-source cancellation, preview disposal, click suppression and grouped-path routing. `cargo run -p zgui-desktop --example drag_drop` runs the three-card gallery. The X11 fixture `scripts/drag_drop_smoke.py` exercises native pointer delivery, accepted drop, visible preview and Escape cancellation; screenshots and logs are in `docs/platform-validation/drag-drop`. `scripts/file_drop_smoke.py` uses a real GTK Xdnd source to drop two files, including a filename with a space, and then cancels a second drag with Escape; evidence is in `docs/platform-validation/file-drop`.
