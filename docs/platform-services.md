# Native dialogs and application menus

`WindowContext::dialogs` provides asynchronous open-file, open-files, open-folder,
open-folders, and save-file selection. Linux uses XDG Desktop Portal with native
Wayland/X11 parent identifiers and owned cancellable requests. An unavailable
portal reports `BackendUnavailable`. macOS uses native AppKit sheets; Windows
uses native owned file dialogs. The UI thread remains
available while the user chooses a path. Selecting a save destination does not
create or overwrite the file.

```no_run
use zgui_desktop::{FileDialogOptions, FileFilter, WindowContext};
fn choose_document(cx: &mut WindowContext, scope: &mut zgui::view::ViewScope) {
    let options = FileDialogOptions::new("Open document")
        .filter(FileFilter::new("Text", ["txt", "md"]));
    scope.retain(cx.prompt_open_file(options, |result| {
        if let Ok(Some(path)) = result {
            println!("Selected {}", path.display());
        }
    }));
}
```

For multiple files or folders, clone `cx.dialogs` into a task on `cx.tasks` and
await the corresponding method. Invoke dialogs after the native window exists;
requesting one while suspended returns `WindowUnavailable`. A result from a
previous native-window generation is rejected after suspend/resume. Closing the
window invalidates delivery and cancels its UI tasks.

The returned `ScopedTask` must be retained. On Linux, dropping it or closing the
owner cancels the file-picker request through `org.freedesktop.portal.Request.Close`.
The worker releases its Wayland export before dropping the retained native parent;
it never blocks the UI thread waiting for the picker. Each picker owns its D-Bus
connection and request handle. Responses subscribe before request creation to
avoid the fast-response race. Linux distinguishes user cancellation (`Ok(None)`)
from backend errors. On macOS, owner close/suspension invalidates result delivery
and dismisses owned sheets, including nested overwrite confirmations and prompts.
Dropping an individual request future cancels delivery but does not itself dismiss
its sheet. Backend failures there may also appear as no selection. Prompts have
the separate backend limitations described below. No callback runs on a worker.

On Windows, cancelling a future or closing/suspending the owner invalidates
delivery but does not dismiss the native dialog. The backend retains its parent
until the dialog is dismissed, and backend errors may also appear as no selection.

The portal cancellation contract follows the [official Request interface](https://flatpak.github.io/xdg-desktop-portal/docs/doc-org.freedesktop.portal.Request.html).

Filters contain extensions without a leading dot. Native platform extension
and overwrite-confirmation behavior is preserved. A selected path is not a
promise that a later filesystem operation will succeed. A suitable GTK/GNOME/KDE
portal backend is needed on Linux; the wlroots screenshot/screencast backend
alone does not provide file selection.

## Shared menu model

`Application::menus` accepts validated `AppMenu` trees containing actions,
submenus, and separators. Action identifiers must be nonempty and unique across
the tree. Items can be disabled. macOS presents this model in its native
application menu. Linux and Windows use the rendered menu implementation;
mount `cx.app_menu_bar()` in the content layout for retained, accessible menus
with keyboard traversal, disabled-item skipping, Escape dismissal, focus
restoration, and nested submenus. `has_native_app_menu()` exposes the distinction.

```no_run
use zgui_desktop::{AppMenu, AppMenuEntry, Application};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    Application::new().menus(vec![AppMenu::new("File", [
        AppMenuEntry::item("Open…", "open"),
        AppMenuEntry::item("Save", "save").enabled(false),
    ])])?.run(|cx| {
        cx.on_menu_action(|action| println!("{}", action.0));
        if !zgui_desktop::has_native_app_menu() {
            cx.render(cx.app_menu_bar());
        }
    })
}
```

A selected `MenuAction` routes through the focused component's typed action
handlers first. Call `prevent_default()` there to mark it handled; otherwise
`WindowContext::on_menu_action` receives it. Native macOS selection targets the
focused live window; explicit `WindowHandle::dispatch_menu_action` targets that
handle's window. Unknown and disabled identifiers do not dispatch. `WindowFactory::set_menus` replaces the validated model for current and future
windows. `set_menu_state(action, enabled, checked)` updates command state without
changing labels or shortcuts. Updates apply on the next UI event-loop turn;
sequential state updates compose from the latest model. Native replacement drops
the previous macOS menu before installing its replacement. Open rendered menus
close during replacement.

`AppMenuEntry::checked(bool)` creates a controlled check item, including the
accessible checked state and native macOS checkmark. Update its state in the
command callback; choosing the item does not mutate the application model.
`MenuAccelerator::primary(Key::Character("s".into()))` uses Command on macOS and
Control on Linux and Windows. Explicit logical key/modifier combinations are also supported;
letters/digits, named navigation keys, and F1–F35 are accepted. Duplicate shortcuts
are rejected. Linux and Windows shortcuts register through scoped root keymaps, preserving
focused component bindings and typed-action precedence; disabled entries have no
binding and replacement unregisters previous shortcuts. macOS displays and
handles the same shortcuts in the native menu. Standard native OS-role items
(About/Services/Hide) remain separate from this application-command model.

Run `cargo run -p zgui-desktop --example platform_services` for an interactive
example. `scripts/native_services_smoke.py` exercises a real GTK file portal in
an isolated Sway session; it does not inject dialog results into application code.
The [macOS continuation packet](platform-validation/macos-continuation/README.md)
records real AppKit open/folder/save/cancel, two-file selection and prompt results, including owner
close while an overwrite confirmation is nested above a save sheet. Native menu
label placement, disabled command suppression, accelerators, replacement and Quit
were observed after fixes. A separate standard application menu now precedes the
application command menus. Later direct `NSMenuItem` property observations verify
checked/unchecked and enabled/disabled transitions on the actual AppKit menu,
independently of the model log. Checkmark screenshot pixels were not captured.

Dependencies include exact `rfd 0.17.2` (MIT), `open 5.4.4`, Linux `dbus 0.9.12` / `dbus-crossroads 0.5.3`, and macOS `muda 0.18.0` (MIT/Apache-2.0). The Wayland parent exporter is adapted from rfd/ashpd with its MIT attribution retained in `src/portal_parent/LICENSE`; pinned Wayland protocol crates match the workspace lock.

Native prompts use `cx.dialogs.prompt(PromptOptions::new(title, body).buttons(PromptButtons::OkCancel)).await`. Results are typed `PromptResponse` values and retain the same owner-generation cancellation rules as file pickers. On Linux this uses the `zenity` executable from the user's desktop installation; unlike file pickers it is not an XDG portal and rfd's Linux prompt backend does not attach the native parent. A missing prompt backend can be reported as cancellation by rfd. macOS uses an AppKit alert attached to the parent window. Applications requiring strict modal blocking or backend-failure distinction should not infer those guarantees from this API.

Windows prompts use an owned native message box on a worker, keeping the native
parent alive until dismissal. Cancellation invalidates result delivery without
dismissing that message box.

`open_url("https://example.com").await` asks the OS default handler to open a URI on a worker. It rejects malformed schemes and control characters. Success means the launcher accepted the request, not that the remote resource loaded.

`Application::application_id("org.example.Editor")?.on_application_event(...)` delivers typed `ApplicationEvent::OpenUrls` and `Reopen` callbacks on the UI thread. Linux owns the corresponding session-bus name and implements `org.freedesktop.Application.Open` and `Activate` at the application ID's slash-separated object path. Install the application's `.desktop` file, URI MIME associations, and D-Bus activation service separately for OS launching; this library does not modify the user's registration. macOS owns AppleEvent handlers for URL/document opens and reopen while preserving winit's application delegate. Routes are installed again after native launch initialization and removed on teardown. URL schemes still require the application's bundle `CFBundleURLTypes` registration. These callbacks are opt-in through the application ID. Native Mac reopen and OS URL launch/delivery have been observed: a test receiver registered in the user's Applications directory received the exact `zgui-smoke:accepted` URL in its retained UI after the launcher returned success. Earlier missing-registration failures are preserved. This test-only registration does not add application packaging to the framework.

Windows has no native application-activation backend. Opting into
`Application::application_id` makes `run` return an explicit error; applications
must handle launch arguments and register URI schemes themselves.

Applications that remain available after their last window closes can set `quit_on_last_window_close(false)` and create windows through the event callback's `WindowFactory`; `WindowFactory::quit()` exits explicitly. Result delivery to closed windows is suppressed. Retaining the application does not retain a closed window's UI.

`WindowOptions` supports `bounds`, `display`, `min_size`, `fullscreen`, and `decorations`. `WindowBounds` uses physical pixels; minimum sizes and ordinary width/height options use logical pixels. `WindowHandle` exposes bounds, fullscreen, decorations, minimum-size and drag commands. Call `drag_window()` from the primary pointer-down handler of a custom titlebar. Fullscreen restoration is handled by the native window system. `WindowContext::window_info` provides reactive current bounds, displays, fullscreen state, capabilities and native request errors; `refresh_window_info()` requests a fresh native snapshot.

On Wayland, global window position and arbitrary placement are unavailable, so bounds have `position: None` and placement requests report an error while size changes are still applied. A display selector can identify a fullscreen output, but it does not grant ordinary-window placement. Compositors may override requested sizes or fullscreen state. The native Mac fixture now records display bounds at 2× scale, fullscreen entry/exit, restored window size and minimum-size enforcement. It exposed a borderless maximize-query event loop, fixed by avoiding that mutating native query for borderless windows. Custom-titlebar drag and physical monitor/scale transitions still need independent observation. Automated dragging moved neither the custom nor standard decorated AppKit titlebar; native diagnostics reported a left-down event but zero pressed buttons. This does not establish a framework drag defect or pass; use a true held-native gesture and the `--decorated` control fixture.


`dynamic_menus` and `scripts/native_menu_smoke.py` demonstrate live two-window updates. Checked menu items project to AccessKit `MenuItemCheckBox`; a component action handler can prevent default to override the application fallback.

External Wayland file drops use an owned `wl_data_device` queue on the application's display. The receiver negotiates Copy and reads `text/uri-list` without blocking the UI thread. It delivers surface-local logical coordinates, per-path compatibility events, and one `FilesDrop` batch. A transfer finishes only after the UI accepts a drop (for example through `View::on_files_drop`); rejected regions, closed owners and failed transfers do not falsely report successful protocol completion. Surface routing is removed on suspend and teardown, and the worker joins before releasing its owned display connection.

The URI transport accepts at most 1 MiB of encoded transfer data and 1,024 local file paths, with a three-second stalled-transfer deadline. Invalid/remote/NUL-bearing URIs reject the complete batch, with typed `FilesDropRejected` reasons. Nonblocking transfer polling keeps input responsive. Leaving the target clears hover and cancels an uncommitted receiver operation. Escape during an external drag is controlled by the source/compositor; the tested GTK3/Sway combination did not forward Escape to the source and still dropped on release. This differs from zgui's typed internal drag, which owns and handles Escape cancellation.
