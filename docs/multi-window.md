# Popups, drags and windows across the screen

On macOS a zgui app's menus, drags and windows behave as AppKit's do, past the edges of their window and between windows. Nothing here costs an app that doesn't use it: no popup window exists before the pointer first reaches something that opens one, and an idle window does no work.

| What | API | Where |
| --- | --- | --- |
| Menus, popovers, selects, tooltips past the window | as before: `menu`, `popover`, `.tooltip`; `open_over` for a pop-up button | macOS, Windows, X11, Wayland |
| A popover's arrow at its trigger | `View::arrow` / Luau `arrow` | everywhere (drawn in the window where popups stay in it) |
| A drag's preview over every window and app | any drag with a `drag_preview` | macOS, X11 |
| Drag out to other apps | `View::drag_export` / Luau `drag_export` | macOS, Windows, X11 |
| Drags between the app's windows | any drag and drop; `DragEvent::visiting` | macOS, X11 |
| Tear a tab off into a window of its own | `View::on_drag_outside`, `WindowOptions::under_pointer` | macOS, X11; Windows places the new window |
| A window's only tab moves the window | `View::drag_moves_window` | macOS, X11 |
| Window menu, ⌘`, AppKit tabs | `WindowOptions::{tabbing, tabbing_identifier}` | macOS |
| New windows cascading from the front one | a window with no position | macOS, Windows, X11 |
| Windows reopening where they were left | `WindowOptions::frame_autosave` / Luau `ui.window { frame_autosave }` | macOS, Windows, X11; Wayland keeps the size and maximized state |

Elsewhere each one falls back to what the window alone can do: popups stay inside it, and drags stay in it. The source files' `todo(windows)`, `todo(x11)` and `todo(wayland)` notes say what each platform needs, and #193 lists what X11 needs for drags between windows and out to other apps.

## Popups past the window

Popups need nothing new. An open menu, submenu, select, popover, context menu or tooltip is a popup root of its window's scene. It keeps the window's focus, keys and dismissal, and the host shows it in a window of its own, flipped and clamped to the screen its trigger is on ([application](application.md#popups-past-the-window)). Menus open as their trigger is pressed: drag to an item and release to choose it. A pop-up button's menu opens with the chosen item over the button (`View::open_over(|| id)`, Luau `open_over`). `.arrow(8.)` gives a popover an arrow pointing at its trigger from whichever side it opened on. Right to left, a menu lines up with its trigger's right edge and submenus open leftward. A popup open as its window moves goes with it. As the window minimizes, goes into or out of fullscreen, or leaves the Space being shown, the popup closes, as AppKit's menus and popovers do. On macOS the theme's `menu.bg` and `popover.bg` are clear over AppKit's menu and popover materials ([styling](styling.md)).

## Drags

A drag's preview is its own window, over every window and app. It is drawn once, then moved by the window server. A drag nothing takes slides back to its source ([drag and drop](drag-drop.md)).

- **Out to other apps.** `drag_export` says what a drag carries: text, links, files or images. As the drag leaves the app's windows, AppKit takes it over, and Finder, Mail or a browser can take it. `on_drag_end` hears whether one did.
- **Between windows.** Over another of the app's windows, that window's destinations hear the drag (`DragEvent::visiting`), and a release drops there. The source's destinations hear nothing meanwhile.
- **Tear-off.** Released outside every window, `on_drag_outside` hears it, with where the source was held (`grab_x`, `grab_y`). Open a window there with `WindowOptions::under_pointer`. A source that takes it this way has its preview go, rather than slide back. Torn off a fullscreen window, `tab_windows` opens the tab's window fullscreen too (`WindowOptions::fullscreen`, read from `WindowInfo::fullscreen`), in a Space of its own, as Safari does; a window opened beside a fullscreen one would land on another Space.
- **A window's only tab.** `drag_moves_window` makes the drag move its window instead, with no preview. Dropped on another window's tab strip, it merges there; elsewhere the window stays where it was let go.

## Windows

Every macOS app has a Window menu: its own if it has one titled "Window", else a standard one. AppKit lists the windows there, and ⌘` cycles through them.

A window opened with no position cascades from the front one. `frame_autosave` reopens a window where it was left, at the size it was left, whatever its title bar. `tabbing` decides whether a window joins AppKit's window tabs: `Disallowed` suits windows with tabs of their own.

## Rust

Tabs that move between windows and tear off (the whole program is `cargo run -p zgui-desktop --example tab_windows`):

```rust
#[derive(Clone)]
struct TabDrag(String);

fn tab(title: &str, tabs: Signal<Vec<String>>, windows: WindowFactory) -> View {
    let (name, alone, gone) = (title.to_string(), tabs.clone(), tabs.clone());
    div()
        .child(text(title))
        .on_drag(move || TabDrag(name.clone()))
        .drag_preview(|drag: &TabDrag| div().child(text(&drag.0)))
        // Its window's only tab moves the window.
        .drag_moves_window(move |_: &TabDrag| alone.with_untracked(|tabs| tabs.len() == 1))
        // Torn off: a window of its own, the tab under the pointer.
        .on_drag_outside(move |drag: &TabDrag, event| {
            let title = drag.0.clone();
            gone.update(|tabs| tabs.retain(|tab| *tab != title));
            windows.open(
                WindowOptions {
                    title: title.clone(),
                    under_pointer: Some((event.grab_x as f64, event.grab_y as f64)),
                    tabbing: Tabbing::Disallowed,
                    ..Default::default()
                },
                move |cx| {
                    cx.render(text(&title));
                },
            );
        })
}

// A strip of another window takes the tab.
let strip = row().on_drop(move |drag: &TabDrag, _| {
    tabs.update(|tabs| tabs.push(drag.0.clone()));
});

// A chip Finder, Mail or Safari take.
let link = div()
    .child(text("zgui on GitHub"))
    .on_drag(|| "https://github.com/zeronsh/zgui")
    .drag_export(|url: &&str| vec![DragItem::Url(url.to_string())]);
```

## Luau

Popups, drag-out and windows (`cargo run -p zgui-script --example popups` shows the first two):

```luau
local size = ui.state("size", false)
local chosen = ui.state("chosen", "Medium")
local inspector = ui.state("inspector", false)

-- A pop-up button: its menu opens with the chosen size over the button.
local sizes: { Node } = {}
for _, label in { "Small", "Medium", "Large" } do
	table.insert(sizes, ui.menu_item { label, id = label, on_click = ui.set(chosen, label) })
end
local select = ui.menu {
	open = size,
	anchor = ui.button { ui.text(function()
		return chosen:get()
	end) },
	open_over = function()
		return chosen:get()
	end,
	sizes,
}

-- Dragged into Safari, it is a link; into Mail, text.
local link = ui.column {
	px = 12,
	h = 28,
	justify = "center",
	on_drag = function()
		return "https://github.com/zeronsh/zgui"
	end,
	drag_export = function(url)
		return { url = url, text = url }
	end,
	ui.text("zgui on GitHub"),
}

-- A window that reopens where it was left, out of AppKit's tabs.
ui.window {
	id = "inspector",
	title = "Inspector",
	open = inspector,
	frame_autosave = "inspector",
	tabbing = "disallowed",
	ui.text("Inspector"),
}

-- A popover with an arrow at its trigger.
local details = ui.state("details", false)
local popover = ui.popover {
	open = details,
	anchor = ui.button { on_click = ui.set(details, true), "Details…" },
	placement = { side = "right", align = "center" },
	arrow = 8,
	p = 14,
	ui.text("Rename"),
}

return ui.row {
	gap = 8,
	p = 14,
	select,
	link,
	popover,
	ui.button { on_click = ui.set(inspector, true), "Inspector" },
}
```

Luau also has `on_drag_outside` and `drag_moves_window` ([events](reference/events.md)). Opening a torn-off tab's window from a script would need a window per tab, which `ui.window`'s declared windows don't express yet.

## Measuring

| Probe | What it measures |
| --- | --- |
| `popup_probe` | click to menu shown, press-drag-release, idle before and after, deactivation |
| `drag_probe` | instructions per pointer move: within a window, over another window, moving a window by its tab; plus tear-off |
| `window_probe` | the Window menu, the cascade, the autosave, and what ⌘` cycles |
| `corners_probe` | a menu as its window moves (it follows), minimizes (it closes), on another display (it opens there), with a dozen windows in the Window menu; `--fullscreen` takes the screen into fullscreen and back |

Each probe posts synthetic AppKit events to its own windows and never moves the user's pointer. None of them drags anything that carries an export out of a window: AppKit's drag session follows the real pointer and would drop on whatever app is under it. ⌘` is a system hotkey, so it can only be checked by hand.
