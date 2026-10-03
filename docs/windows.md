# Windows desktop support

Use stable Rust with the `x86_64-pc-windows-msvc` target and Visual Studio Build
Tools (Desktop development with C++, including the Windows SDK). From PowerShell:

```powershell
cargo run -p zgui-desktop --example components --locked
cargo run -p zgui-desktop --example platform_services --locked
```

Windows uses the existing winit event loop and wgpu renderer (DX12/Vulkan),
arboard clipboard, AccessKit Windows accessibility backend, and rfd native file
pickers and message boxes. Both file pickers and prompts receive their owning
native window. Application menus use `cx.app_menu_bar()` and Control shortcuts.
No additional Windows-only dependency is needed.

`cx.dialogs.clone()` gives host integrations a deferred native-window provider.
Call `dialogs.native_window()` on the UI executor after window creation. It
returns `None` during initial UI construction, suspension, and after closure.
The returned `Arc<winit::window::Window>` exposes current size, scale, and raw
window handles; retaining it prolongs the native window lifetime, so release it
when the integration is detached. `cx.native_window()` is a convenience accessor.

For a foreign native event loop such as Chromium Embedded Framework, call
`Application::run_with_pump(pump, build)` instead of invoking its pump from a UI
task. On Windows the callback runs before native message dispatch while winit's
application handler is installed and idle. A nested pump inside an application
callback can repeatedly consume paints that winit defers, preventing the pump
from ever reaching idle. An outer `pump_app_events` loop alone is also unsuitable:
winit removes its handler between calls. Arrange periodic wakeups while idle,
for example a UI task awaiting a 10 ms timer, to keep the foreign loop advancing.
`cargo run -p zgui-desktop --example native_message_pump --locked` checks the
native callback and shutdown path. This probe ran successfully on Windows.

## Limitations

- `Application::application_id` opts into OS activation callbacks, which are not
  implemented on Windows. Running an application with an ID returns an explicit
  error. Handle launch arguments and protocol registration in the application.
- Cancelling a file-picker or prompt future invalidates result delivery but
  does not dismiss its native dialog. A pending native dialog can retain its
  parent until dismissed. Backend failures can appear as cancellation.
- Rendered application menus are supported; native Win32 menu bars are not.
- The Linux benchmark sampler and platform-specific smoke scripts do not run
  on Windows.

## Validation

On a Windows host, the desktop library tests and the following native lifecycle
test passed. The latter creates real windows and GPU surfaces, renders an
editor, suspends and recreates the host, verifies retained text and input/IME
cleanup, and checks the deferred native-window provider across those transitions:

```powershell
cargo test -p zgui-desktop --lib --locked
cargo test -p zgui-desktop --lib host_suspend_recreates_native_resources_and_retains_editor_state --locked -- --ignored --test-threads=1
```

The standard CI matrix includes Windows compilation, library/example tests,
doctests, formatting and Clippy. The native lifecycle probe requires a desktop
and GPU and remains opt-in. Passing it does not establish real keyboard/IME
candidate interaction, dialog interaction, cross-process clipboard exchange,
screen-reader behavior, DPI transitions, or GPU performance; those need separate
native qualification.
