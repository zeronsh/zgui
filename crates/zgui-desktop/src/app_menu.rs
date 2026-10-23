//! One menu model for macOS's application menu and an accessible Linux menu bar.
use std::{collections::HashSet, fmt, rc::Rc};
use zgui::compose::{self, Context, View};
use zgui::style::Styled;

/// Stable application-defined command identifier shared by native and rendered menus.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct MenuAction(pub String);
impl From<&str> for MenuAction {
    fn from(value: &str) -> Self {
        Self(value.into())
    }
}
impl From<String> for MenuAction {
    fn from(value: String) -> Self {
        Self(value)
    }
}
/// A single logical-key shortcut shared by native macOS and Linux key routing.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MenuAccelerator(pub zgui::actions::Keystroke);
impl MenuAccelerator {
    /// `key` with `modifiers`.
    pub fn new(key: zgui::input::Key, modifiers: zgui::input::Modifiers) -> Self {
        Self(zgui::actions::Keystroke::new(key, modifiers))
    }
    /// `key` with the platform's command modifier: ⌘ on macOS, Ctrl elsewhere.
    pub fn primary(key: zgui::input::Key) -> Self {
        Self::new(
            key,
            zgui::input::Modifiers {
                control: !cfg!(target_os = "macos"),
                meta: cfg!(target_os = "macos"),
                ..Default::default()
            },
        )
    }
    fn name(&self) -> Result<String, AppMenuError> {
        use zgui::input::Key;
        let key = match &self.0.key {
            Key::Character(s)
                if s.chars().count() == 1 && s.chars().all(|c| c.is_ascii_alphanumeric()) =>
            {
                s.to_ascii_uppercase()
            }
            // Punctuation keys, by the character they type unshifted.
            Key::Character(s) if s.chars().count() == 1 && "`\\[],=-.';/".contains(s.as_str()) => {
                s.clone()
            }
            Key::Function(n) if (1..=35).contains(n) => format!("F{n}"),
            Key::Character(_) | Key::Function(_) => {
                return Err(AppMenuError(
                    "menu accelerators require one ASCII letter/digit or a supported named key"
                        .into(),
                ));
            }
            key => format!("{key:?}"),
        };
        let m = self.0.modifiers;
        let mut parts = Vec::new();
        if m.control {
            parts.push("Ctrl".to_owned())
        }
        if m.meta {
            parts.push("Super".to_owned())
        }
        if m.alt {
            parts.push("Alt".to_owned())
        }
        if m.shift {
            parts.push("Shift".to_owned())
        }
        parts.push(key);
        Ok(parts.join("+"))
    }
    /// The shortcut as a menu shows it (`Super+Shift+Z`), or empty if it can't
    /// be one.
    pub fn label(&self) -> String {
        self.name().unwrap_or_default()
    }
}
/// A menu: a top-level menu in the menu bar, or a submenu.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AppMenu {
    /// The title it shows.
    pub label: String,
    /// Its items, in order.
    pub entries: Vec<AppMenuEntry>,
}
impl AppMenu {
    /// A menu titled `label` holding `entries`.
    pub fn new(label: impl Into<String>, entries: impl IntoIterator<Item = AppMenuEntry>) -> Self {
        Self {
            label: label.into(),
            entries: entries.into_iter().collect(),
        }
    }
}
/// An edit command the platform implements for whatever has focus, native
/// views included: on macOS it is AppKit's own item, sent to the first
/// responder (so a web page's selection copies). zgui's own view doesn't
/// answer those actions, so there the item is disabled, and a disabled item
/// still swallows its shortcut; while zgui's view has the keyboard, its
/// shortcut goes straight to the view instead (see `edit_keys`), for zgui's
/// editors to handle. Its shortcut (⌘C for Copy) and its action
/// (`standard:copy`) are its own: no other item may have them. Elsewhere
/// it is an ordinary item in zgui's menu bar, whose choosing reaches the
/// app as that action.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StandardItem {
    /// Undoes the last edit.
    Undo,
    /// Redoes the last undone edit.
    Redo,
    /// Cuts the selection to the clipboard.
    Cut,
    /// Copies the selection to the clipboard.
    Copy,
    /// Pastes the clipboard.
    Paste,
    /// Selects everything in the focused view.
    SelectAll,
}
impl StandardItem {
    /// Its title, as the menu shows it.
    pub fn label(self) -> &'static str {
        match self {
            Self::Undo => "Undo",
            Self::Redo => "Redo",
            Self::Cut => "Cut",
            Self::Copy => "Copy",
            Self::Paste => "Paste",
            Self::SelectAll => "Select All",
        }
    }
    /// Its shortcut, as [`MenuAccelerator`] names one (macOS's).
    fn shortcut_name(self) -> &'static str {
        match self {
            Self::Undo => "Super+Z",
            Self::Redo => "Super+Shift+Z",
            Self::Cut => "Super+X",
            Self::Copy => "Super+C",
            Self::Paste => "Super+V",
            Self::SelectAll => "Super+A",
        }
    }
    /// The action a rendered menu bar dispatches for it.
    pub fn action(self) -> MenuAction {
        MenuAction(format!(
            "standard:{}",
            self.label().to_lowercase().replace(' ', "-")
        ))
    }
}

/// One entry of an [`AppMenu`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AppMenuEntry {
    /// A command that dispatches `action` when chosen.
    Item {
        /// The title it shows.
        label: String,
        /// What it dispatches to the focused window.
        action: MenuAction,
        /// Whether it can be chosen; a disabled item shows dimmed.
        enabled: bool,
        /// A check mark: `Some(true)` checked, `Some(false)` checkable but not
        /// checked, `None` not a toggle.
        checked: Option<bool>,
        /// Its keyboard shortcut.
        accelerator: Option<MenuAccelerator>,
    },
    /// A nested menu.
    Submenu(AppMenu),
    /// A divider between groups of entries.
    Separator,
    /// An edit command the platform implements (see [`StandardItem`]).
    Standard(StandardItem),
}
impl AppMenuEntry {
    /// An enabled item titled `label` that dispatches `action`, unchecked and
    /// without a shortcut.
    pub fn item(label: impl Into<String>, action: impl Into<MenuAction>) -> Self {
        Self::Item {
            label: label.into(),
            action: action.into(),
            enabled: true,
            checked: None,
            accelerator: None,
        }
    }
    /// The item with a check mark, shown when `checked` (see `Item::checked`).
    /// Other entries are unchanged.
    pub fn checked(mut self, checked: bool) -> Self {
        if let Self::Item { checked: value, .. } = &mut self {
            *value = Some(checked)
        }
        self
    }
    /// The item with the shortcut `accelerator`. Other entries are unchanged.
    pub fn accelerator(mut self, accelerator: MenuAccelerator) -> Self {
        if let Self::Item {
            accelerator: value, ..
        } = &mut self
        {
            *value = Some(accelerator)
        }
        self
    }
    /// The item enabled or disabled. Other entries are unchanged.
    pub fn enabled(mut self, enabled: bool) -> Self {
        if let Self::Item { enabled: value, .. } = &mut self {
            *value = enabled;
        }
        self
    }
}
/// Why a menu model or menu request was rejected, in words.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AppMenuError(pub String);
impl fmt::Display for AppMenuError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}
impl std::error::Error for AppMenuError {}
/// macOS exposes the application menu in the system menu bar. Linux windows use
/// [`app_menu_bar`], as winit has no GTK menubar host or universal Wayland app menu.
pub fn has_native_app_menu() -> bool {
    cfg!(target_os = "macos")
}

pub(crate) fn validate(menus: &[AppMenu]) -> Result<(), AppMenuError> {
    fn visit(
        menus: &[AppMenu],
        ids: &mut HashSet<String>,
        shortcuts: &mut HashSet<String>,
    ) -> Result<(), AppMenuError> {
        for menu in menus {
            if menu.label.is_empty() || menu.label.contains('\0') {
                return Err(AppMenuError(
                    "menu labels must be nonempty and contain no NUL".into(),
                ));
            }
            for entry in &menu.entries {
                match entry {
                    AppMenuEntry::Item {
                        label,
                        action,
                        accelerator,
                        ..
                    } => {
                        if let Some(shortcut) = accelerator
                            && !shortcuts.insert(shortcut.name()?)
                        {
                            return Err(AppMenuError("menu accelerators must be unique".into()));
                        }
                        if label.is_empty()
                            || label.contains('\0')
                            || action.0.is_empty()
                            || action.0.contains('\0')
                            || !ids.insert(action.0.clone())
                        {
                            return Err(AppMenuError("menu items need valid labels and unique nonempty action identifiers".into()));
                        }
                    }
                    AppMenuEntry::Submenu(menu) => {
                        visit(std::slice::from_ref(menu), ids, shortcuts)?
                    }
                    AppMenuEntry::Standard(item) => {
                        if !shortcuts.insert(item.shortcut_name().to_owned()) {
                            return Err(AppMenuError(format!(
                                "the standard {} item's shortcut is another item's",
                                item.label()
                            )));
                        }
                        if !ids.insert(item.action().0) {
                            return Err(AppMenuError(format!(
                                "the standard {} item appears twice, or its action is another item's",
                                item.label()
                            )));
                        }
                    }
                    AppMenuEntry::Separator => {}
                }
            }
        }
        Ok(())
    }
    visit(menus, &mut HashSet::new(), &mut HashSet::new())
}
pub(crate) fn enabled_action(menus: &[AppMenu], action: &MenuAction) -> bool {
    menus
        .iter()
        .flat_map(|m| &m.entries)
        .any(|entry| match entry {
            AppMenuEntry::Item {
                action: id,
                enabled,
                ..
            } => *enabled && id == action,
            AppMenuEntry::Submenu(menu) => enabled_action(std::slice::from_ref(menu), action),
            AppMenuEntry::Separator | AppMenuEntry::Standard(_) => false,
        })
}
/// Accessible retained menus with keyboard traversal, Escape dismissal, focus
/// restoration, and nested submenus. Mount inside a Linux window's content layout.
/// The same model can also be rendered explicitly on any platform.
pub fn app_menu_bar(
    menus: Vec<AppMenu>,
    dispatch: impl Fn(MenuAction) + 'static,
) -> Result<View, AppMenuError> {
    validate(&menus)?;
    let dispatch: Rc<dyn Fn(MenuAction)> = Rc::new(dispatch);
    Ok(compose::component(move |cx| {
        let menus = menus
            .into_iter()
            .map(|menu| {
                let open = cx.state(false);
                let toggle = open.clone();
                let anchor = compose::button()
                    .h(30.)
                    .p(6.)
                    .child(menu.label.clone())
                    .on_click(move || {
                        toggle.set(!toggle.get());
                    });
                compose::menu(menu.label, open, anchor).children(entries(
                    cx,
                    menu.entries,
                    dispatch.clone(),
                ))
            })
            .collect::<Vec<_>>();
        compose::row().gap(4.).children(menus)
    }))
}
fn entries(
    cx: &mut Context,
    entries: Vec<AppMenuEntry>,
    dispatch: Rc<dyn Fn(MenuAction)>,
) -> Vec<View> {
    entries
        .into_iter()
        .map(|entry| match entry {
            AppMenuEntry::Item {
                label,
                action,
                enabled,
                checked,
                accelerator,
            } => {
                let label = format!(
                    "{}{}{}",
                    if checked == Some(true) {
                        "✓ "
                    } else if checked.is_some() {
                        "  "
                    } else {
                        ""
                    },
                    label,
                    accelerator
                        .map(|a| format!("    {}", a.label()))
                        .unwrap_or_default()
                );
                let dispatch = dispatch.clone();
                let item = compose::menu_item(label);
                let item = if let Some(checked) = checked {
                    item.menu_checked(checked)
                } else {
                    item
                };
                item.h(30.)
                    .p(6.)
                    .disabled(!enabled)
                    .on_click(move || dispatch(action.clone()))
            }
            AppMenuEntry::Submenu(menu) => compose::submenu(menu.label, cx.state(false))
                .children(self::entries(cx, menu.entries, dispatch.clone())),
            AppMenuEntry::Separator => compose::row()
                .h(1.)
                .bg(zgui::scene::Color(100, 100, 100, 255)),
            AppMenuEntry::Standard(item) => {
                let dispatch = dispatch.clone();
                compose::menu_item(item.label())
                    .h(30.)
                    .p(6.)
                    .on_click(move || dispatch(item.action()))
            }
        })
        .collect()
}

pub(crate) fn keymap(menus: &[AppMenu]) -> zgui::actions::Keymap {
    use zgui::actions::{Action, KeyBinding, Keymap};
    fn collect(menus: &[AppMenu], mut map: Keymap) -> Keymap {
        for menu in menus {
            for entry in &menu.entries {
                match entry {
                    AppMenuEntry::Item {
                        action,
                        enabled: true,
                        accelerator: Some(key),
                        ..
                    } => {
                        let mut stroke = key.0.clone();
                        if let zgui::input::Key::Character(s) = &mut stroke.key {
                            *s = s.to_ascii_lowercase();
                        }
                        map = map.bind(
                            KeyBinding::new([stroke.clone()], Action::new(action.clone()))
                                .expect("single menu shortcut"),
                        );
                        if let zgui::input::Key::Character(s) = &mut stroke.key {
                            let upper = s.to_ascii_uppercase();
                            if upper != *s {
                                *s = upper;
                                map = map.bind(
                                    KeyBinding::new([stroke], Action::new(action.clone()))
                                        .expect("single menu shortcut"),
                                );
                            }
                        }
                    }
                    AppMenuEntry::Submenu(menu) => map = collect(std::slice::from_ref(menu), map),
                    _ => {}
                }
            }
        }
        map
    }
    collect(menus, Keymap::new())
}
pub(crate) fn update_entry(
    menus: &mut [AppMenu],
    action: &MenuAction,
    enabled: Option<bool>,
    checked: Option<bool>,
) -> bool {
    for menu in menus {
        for entry in &mut menu.entries {
            match entry {
                AppMenuEntry::Item {
                    action: id,
                    enabled: current,
                    checked: check,
                    ..
                } if id == action => {
                    if let Some(value) = enabled {
                        *current = value
                    }
                    if let Some(value) = checked {
                        *check = Some(value)
                    }
                    return true;
                }
                AppMenuEntry::Submenu(menu) => {
                    if update_entry(std::slice::from_mut(menu), action, enabled, checked) {
                        return true;
                    }
                }
                _ => {}
            }
        }
    }
    false
}

/// A menu model as an AppKit submenu: items, check items, shortcuts, the
/// standard edit items and nested submenus.
#[cfg(target_os = "macos")]
fn native_submenu(model: &AppMenu) -> Result<muda::Submenu, AppMenuError> {
    let menu = muda::Submenu::new(&model.label, true);
    for entry in &model.entries {
        let result = match entry {
            AppMenuEntry::Item {
                label,
                action,
                enabled,
                checked,
                accelerator,
            } => {
                let accelerator = accelerator
                    .as_ref()
                    .map(|a| {
                        a.name()?
                            .parse::<muda::accelerator::KeyAccelerator>()
                            .map_err(|e| AppMenuError(e.to_string()))
                    })
                    .transpose()?;
                if let Some(checked) = checked {
                    let item = muda::CheckMenuItem::with_id(
                        action.0.clone(),
                        label,
                        *enabled,
                        *checked,
                        None,
                    );
                    item.set_key_accelerator(accelerator)
                        .map_err(|e| AppMenuError(e.to_string()))?;
                    menu.append(&item)
                } else {
                    let item = muda::MenuItem::with_id(action.0.clone(), label, *enabled, None);
                    item.set_key_accelerator(accelerator)
                        .map_err(|e| AppMenuError(e.to_string()))?;
                    menu.append(&item)
                }
            }
            AppMenuEntry::Submenu(child) => menu.append(&native_submenu(child)?),
            AppMenuEntry::Separator => menu.append(&muda::PredefinedMenuItem::separator()),
            AppMenuEntry::Standard(item) => menu.append(&match item {
                StandardItem::Undo => muda::PredefinedMenuItem::undo(None),
                StandardItem::Redo => muda::PredefinedMenuItem::redo(None),
                StandardItem::Cut => muda::PredefinedMenuItem::cut(None),
                StandardItem::Copy => muda::PredefinedMenuItem::copy(None),
                StandardItem::Paste => muda::PredefinedMenuItem::paste(None),
                StandardItem::SelectAll => muda::PredefinedMenuItem::select_all(None),
            }),
        };
        result.map_err(|e| AppMenuError(e.to_string()))?;
    }
    Ok(menu)
}

/// Shows `entries` as a native pop-up menu at `(x, y)`, in points from the
/// top left of the window's content, as AppKit's own controls do (a held
/// back button's history, a context menu): press, drag to an item and let
/// go, or click. A chosen item arrives as a menu action, like the menu
/// bar's. Returns once the menu closes.
#[cfg(target_os = "macos")]
pub(crate) fn show_context_menu(
    window: &winit::window::Window,
    entries: &[AppMenuEntry],
    x: f64,
    y: f64,
) -> Result<(), AppMenuError> {
    use muda::ContextMenu;
    use winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};
    // As the menu bar's are: unique actions and shortcuts, valid labels.
    validate(&[AppMenu::new("Menu", entries.iter().cloned())])?;
    let menu = native_submenu(&AppMenu::new("", entries.iter().cloned()))?;
    let handle = window
        .window_handle()
        .map_err(|e| AppMenuError(e.to_string()))?;
    let RawWindowHandle::AppKit(handle) = handle.as_raw() else {
        return Err(AppMenuError("not an AppKit window".into()));
    };
    let position = muda::dpi::Position::Logical(muda::dpi::LogicalPosition::new(x, y));
    // SAFETY: winit's content view, alive while its window is.
    unsafe { menu.show_context_menu_for_nsview(handle.ns_view.as_ptr(), Some(position)) };
    Ok(())
}
#[cfg(target_os = "macos")]
pub(crate) struct NativeMenu {
    menu: muda::Menu,
    /// The standard edit items it has, whose shortcuts zgui's view takes.
    standard: Vec<StandardItem>,
}
#[cfg(target_os = "macos")]
impl NativeMenu {
    pub(crate) fn new(menus: &[AppMenu]) -> Result<Self, AppMenuError> {
        validate(menus)?;
        let menu = muda::Menu::new();
        // AppKit reserves the first submenu for the application and replaces
        // its title with the process name. Keep user menus in subsequent slots,
        // including after dynamic model replacement.
        let application = muda::Submenu::new("Application", true);
        application
            .append_items(&[
                &muda::PredefinedMenuItem::services(None),
                &muda::PredefinedMenuItem::separator(),
                &muda::PredefinedMenuItem::hide(None),
                &muda::PredefinedMenuItem::hide_others(None),
                &muda::PredefinedMenuItem::show_all(None),
                &muda::PredefinedMenuItem::separator(),
                &muda::PredefinedMenuItem::quit(None),
            ])
            .map_err(|e| AppMenuError(e.to_string()))?;
        menu.append(&application)
            .map_err(|e| AppMenuError(e.to_string()))?;
        for model in menus {
            menu.append(&native_submenu(model)?)
                .map_err(|e| AppMenuError(e.to_string()))?;
        }
        fn standard_items(model: &AppMenu, out: &mut Vec<StandardItem>) {
            for entry in &model.entries {
                match entry {
                    AppMenuEntry::Standard(item) => out.push(*item),
                    AppMenuEntry::Submenu(child) => standard_items(child, out),
                    _ => {}
                }
            }
        }
        let mut standard = Vec::new();
        for model in menus {
            standard_items(model, &mut standard);
        }
        Ok(Self { menu, standard })
    }
    pub(crate) fn install(&self) {
        self.menu.init_for_nsapp();
        edit_keys::watch(&self.standard);
    }
}
/// The standard edit items' shortcuts while zgui's own view has the keyboard.
/// AppKit offers a ⌘ key to the menu bar before the view, and the item for it
/// (⌘V: Paste) is disabled there, since zgui's view doesn't answer `paste:`;
/// a disabled item still swallows its key, with a beep, so zgui's editors
/// never heard ⌘V, ⌘C, ⌘X, ⌘A or ⌘Z. A local monitor sees each key before the
/// menu does: one of those, for a zgui window whose view has the keyboard,
/// goes straight to the view, which hands it to zgui as any key. A page or
/// other native view with the keyboard gets the menu items as before.
#[cfg(target_os = "macos")]
mod edit_keys {
    use super::StandardItem;
    use objc2::MainThreadMarker;
    use objc2::rc::Retained;
    use objc2_app_kit::{NSEvent, NSEventMask, NSEventModifierFlags};
    use std::cell::{Cell, RefCell};
    use std::ptr::NonNull;

    thread_local! {
        static WATCHING: Cell<bool> = const { Cell::new(false) };
        /// (key, shift) for each standard item in the installed menu bar.
        static SHORTCUTS: RefCell<Vec<(&'static str, bool)>> = const { RefCell::new(Vec::new()) };
    }

    fn shortcut(item: StandardItem) -> (&'static str, bool) {
        match item {
            StandardItem::Undo => ("z", false),
            StandardItem::Redo => ("z", true),
            StandardItem::Cut => ("x", false),
            StandardItem::Copy => ("c", false),
            StandardItem::Paste => ("v", false),
            StandardItem::SelectAll => ("a", false),
        }
    }

    /// Whether `event` is one of the menu bar's standard edit shortcuts.
    fn is_edit_shortcut(event: &NSEvent) -> bool {
        let flags = event.modifierFlags() & NSEventModifierFlags::DeviceIndependentFlagsMask;
        if !flags.contains(NSEventModifierFlags::Command)
            || flags.intersects(NSEventModifierFlags::Control | NSEventModifierFlags::Option)
        {
            return false;
        }
        let shift = flags.contains(NSEventModifierFlags::Shift);
        let Some(mut key) = event
            .charactersIgnoringModifiers()
            .map(|k| k.to_string().to_lowercase())
        else {
            return false;
        };
        // A layout that types no Latin letters (Cyrillic, Greek): the key in
        // the ANSI position, as AppKit matches menu shortcuts then. A Latin
        // layout (Dvorak) keeps its letters.
        if !key.is_ascii() {
            key = match event.keyCode() {
                0 => "a",
                6 => "z",
                7 => "x",
                8 => "c",
                9 => "v",
                _ => return false,
            }
            .to_owned();
        }
        SHORTCUTS.with(|keys| keys.borrow().iter().any(|(k, s)| *k == key && *s == shift))
    }

    /// The zgui view `event` should go to: its window is key and zgui's view
    /// (winit's, its content view) is the first responder.
    fn zgui_view(event: &NSEvent) -> Option<Retained<objc2_app_kit::NSView>> {
        let mtm = MainThreadMarker::new()?;
        let window = event.window(mtm).filter(|w| w.isKeyWindow())?;
        let content = window.contentView()?;
        let responder = window.firstResponder()?;
        let is_content = std::ptr::eq(
            Retained::as_ptr(&responder) as *const u8,
            Retained::as_ptr(&content) as *const u8,
        );
        // winit's view, or the subclass AccessKit makes of it at run time.
        let mut class = Some(content.class());
        let mut winit = false;
        while let Some(c) = class {
            if c.name().to_str() == Ok("WinitView") {
                winit = true;
                break;
            }
            class = c.superclass();
        }
        (is_content && winit).then_some(content)
    }

    /// ZGUI_TRACE_KEYS: what the monitor makes of a ⌘ key, on stderr.
    fn trace(event: &NSEvent) {
        let Some(mtm) = MainThreadMarker::new() else {
            return;
        };
        let window = event.window(mtm);
        let responder = window
            .as_ref()
            .and_then(|w| w.firstResponder())
            .map(|r| r.class().name().to_string_lossy().into_owned());
        let content = window
            .as_ref()
            .and_then(|w| w.contentView())
            .map(|c| c.class().name().to_string_lossy().into_owned());
        eprintln!(
            "TRACE edit-keys {:?} shortcut={} key-window={:?} responder={:?} content={:?} to-view={}",
            event.charactersIgnoringModifiers().map(|k| k.to_string()),
            is_edit_shortcut(event),
            window.as_ref().map(|w| w.isKeyWindow()),
            responder,
            content,
            zgui_view(event).is_some()
        );
    }

    pub(super) fn watch(items: &[StandardItem]) {
        SHORTCUTS
            .with(|keys| *keys.borrow_mut() = items.iter().map(|item| shortcut(*item)).collect());
        if WATCHING.replace(true) {
            return;
        }
        let block = block2::RcBlock::new(|event: NonNull<NSEvent>| -> *mut NSEvent {
            // SAFETY: AppKit passes a valid event for the handler's duration.
            let key = unsafe { event.as_ref() };
            if std::env::var_os("ZGUI_TRACE_KEYS").is_some()
                && key.modifierFlags().contains(NSEventModifierFlags::Command)
            {
                trace(key);
            }
            if is_edit_shortcut(key)
                && let Some(view) = zgui_view(key)
            {
                // To the view, not the menu bar: taken here, AppKit sends it nowhere else.
                view.keyDown(key);
                return std::ptr::null_mut();
            }
            event.as_ptr()
        });
        // The monitor lives as long as the app.
        let monitor = unsafe {
            NSEvent::addLocalMonitorForEventsMatchingMask_handler(NSEventMask::KeyDown, &block)
        };
        std::mem::forget(monitor);
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        use objc2_app_kit::NSEventType;
        use objc2_foundation::{NSPoint, NSString};

        fn key(characters: &str, flags: NSEventModifierFlags) -> Retained<NSEvent> {
            key_at(characters, flags, 0)
        }
        fn key_at(characters: &str, flags: NSEventModifierFlags, code: u16) -> Retained<NSEvent> {
            let text = NSString::from_str(characters);
            NSEvent::keyEventWithType_location_modifierFlags_timestamp_windowNumber_context_characters_charactersIgnoringModifiers_isARepeat_keyCode(
                NSEventType::KeyDown, NSPoint::new(0., 0.), flags, 0., 0, None, &text, &text, false, code,
            )
            .unwrap()
        }

        #[test]
        fn takes_only_the_menu_bars_edit_shortcuts() {
            let command = NSEventModifierFlags::Command;
            SHORTCUTS.with(|keys| {
                *keys.borrow_mut() = [StandardItem::Paste, StandardItem::Copy, StandardItem::Redo]
                    .iter()
                    .map(|i| shortcut(*i))
                    .collect()
            });
            assert!(is_edit_shortcut(&key("v", command)));
            assert!(is_edit_shortcut(&key("c", command)));
            assert!(
                is_edit_shortcut(&key("Z", command | NSEventModifierFlags::Shift)),
                "redo"
            );
            assert!(
                !is_edit_shortcut(&key("z", command)),
                "undo isn't in this menu bar"
            );
            assert!(!is_edit_shortcut(&key("x", command)), "nor cut");
            assert!(
                !is_edit_shortcut(&key("v", NSEventModifierFlags::empty())),
                "a plain v is typing"
            );
            assert!(
                !is_edit_shortcut(&key("v", command | NSEventModifierFlags::Option)),
                "⌥⌘V is something else"
            );
            assert!(
                !is_edit_shortcut(&key("v", command | NSEventModifierFlags::Shift)),
                "nor ⇧⌘V"
            );
            assert!(
                !is_edit_shortcut(&key("t", command)),
                "an app shortcut stays with the menu bar"
            );
            // A Russian layout's ⌘C types "с": the key in C's place.
            assert!(is_edit_shortcut(&key_at("с", command, 8)), "⌘C on Cyrillic");
            assert!(
                !is_edit_shortcut(&key_at("щ", command, 31)),
                "a key in no edit shortcut's place"
            );
        }
    }
}

#[cfg(target_os = "macos")]
impl Drop for NativeMenu {
    fn drop(&mut self) {
        self.menu.remove_for_nsapp();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn a_standard_items_shortcut_and_action_are_its_own() {
        use zgui::input::{Key, Modifiers};
        let command = |shift| Modifiers {
            meta: true,
            shift,
            ..Default::default()
        };
        let c = |shift| MenuAccelerator::new(Key::Character("c".into()), command(shift));
        let clash = [AppMenu::new(
            "Edit",
            [
                AppMenuEntry::Standard(StandardItem::Copy),
                AppMenuEntry::item("Copy Path", "copy-path").accelerator(c(false)),
            ],
        )];
        assert!(validate(&clash).is_err(), "⌘C is Copy's");
        let taken = [AppMenu::new(
            "Edit",
            [
                AppMenuEntry::Standard(StandardItem::Paste),
                AppMenuEntry::item("Paste", "standard:paste"),
            ],
        )];
        assert!(validate(&taken).is_err(), "so is its action");
        let fine = [AppMenu::new(
            "Edit",
            [
                AppMenuEntry::Standard(StandardItem::Copy),
                AppMenuEntry::Standard(StandardItem::Paste),
                AppMenuEntry::item("Copy Path", "copy-path").accelerator(c(true)),
            ],
        )];
        assert_eq!(validate(&fine), Ok(()));
    }
    #[test]
    fn rendered_menu_keyboard_dispatches_shared_identifier_and_skips_disabled() {
        use zgui::{
            input::{InputEvent, Key, Modifiers},
            widgets::Ui,
        };
        let mut ui = Ui::new(600., 400.);
        let actions = Rc::new(std::cell::RefCell::new(Vec::new()));
        let output = actions.clone();
        let menus = vec![AppMenu::new(
            "File",
            [
                AppMenuEntry::item("Open", "open"),
                AppMenuEntry::item("Unavailable", "disabled").enabled(false),
                AppMenuEntry::item("Save", "save"),
            ],
        )];
        let _view =
            ui.mount(app_menu_bar(menus, move |action| output.borrow_mut().push(action)).unwrap());
        ui.prepare_frame();
        for key in [Key::Tab, Key::Enter, Key::ArrowDown, Key::Enter] {
            ui.dispatch(InputEvent::KeyDown {
                key: key.clone(),
                modifiers: Modifiers::default(),
                repeat: false,
            });
            ui.dispatch(InputEvent::KeyUp {
                key,
                modifiers: Modifiers::default(),
            });
            ui.prepare_frame();
        }
        assert_eq!(*actions.borrow(), vec![MenuAction::from("save")]);
    }
    #[test]
    fn duplicate_nested_actions_are_rejected_before_mount_or_native_creation() {
        let model = vec![AppMenu::new(
            "File",
            [
                AppMenuEntry::item("Open", "open"),
                AppMenuEntry::Submenu(AppMenu::new("More", [AppMenuEntry::item("Again", "open")])),
            ],
        )];
        assert!(validate(&model).is_err());
    }
    #[test]
    fn disabled_and_unknown_commands_cannot_dispatch() {
        let menus = vec![AppMenu::new(
            "File",
            [
                AppMenuEntry::item("Open", "open"),
                AppMenuEntry::item("Save", "save").enabled(false),
            ],
        )];
        assert!(enabled_action(&menus, &"open".into()));
        assert!(!enabled_action(&menus, &"save".into()));
        assert!(!enabled_action(&menus, &"unknown".into()));
    }
}

#[cfg(test)]
mod dynamic_tests {
    use super::*;
    use zgui::{
        actions::Action,
        input::{EventPhase, InputEvent, Key, Modifiers},
        widgets::Ui,
    };
    #[test]
    fn checked_menu_items_publish_accessible_controlled_state() {
        let mut ui = Ui::new(400., 300.);
        let open = ui.signal(true);
        let view = ui.mount(
            compose::menu("File", open, compose::button().child("File"))
                .child(compose::menu_item("Enabled").menu_checked(true)),
        );
        ui.prepare_frame();
        let semantics = ui.semantics.borrow();
        let (_, item) = semantics
            .iter()
            .find(|(_, s)| s.label == "Enabled" && s.role == zgui::semantics::Role::MenuItem)
            .unwrap();
        assert_eq!(item.checked, Some(true));
        drop(semantics);
        let update = crate::accessibility::AccessibilityTree::new().update(
            &ui.scene.borrow(),
            &ui.semantics.borrow(),
            None,
            "Menu",
            1.,
        );
        assert!(
            update
                .nodes
                .iter()
                .any(|(_, node)| node.role() == accesskit::Role::MenuItemCheckBox
                    && node.toggled() == Some(accesskit::Toggled::True))
        );
        view.unmount();
    }
    #[test]
    fn replacement_unregisters_shortcuts_and_focused_action_can_override() {
        let mut ui = Ui::new(400., 300.);
        let root = ui.scene.borrow().root();
        let output = Rc::new(std::cell::RefCell::new(Vec::new()));
        let fallback = output.clone();
        let _listener = ui.input.listen(root, move |event| {
            if event.phase != EventPhase::Capture
                && !event.default_prevented()
                && let InputEvent::Action(action) = &event.event
                && let Some(action) = action.downcast_ref::<MenuAction>()
            {
                fallback.borrow_mut().push(action.0.clone());
                event.prevent_default();
            }
        });
        let model = vec![AppMenu::new(
            "File",
            [AppMenuEntry::item("Save", "save")
                .accelerator(MenuAccelerator::primary(Key::Character("s".into())))],
        )];
        let binding = ui.input.bind_keys(root, keymap(&model));
        let override_count = Rc::new(std::cell::Cell::new(0));
        let count = override_count.clone();
        let view = ui.mount(compose::button().child("Focused").on_action::<MenuAction>(
            move |_, event| {
                count.set(count.get() + 1);
                event.prevent_default();
            },
        ));
        ui.prepare_frame();
        ui.input.focus(&ui.scene, Some(view.node()));
        let press = || InputEvent::KeyDown {
            key: Key::Character("s".into()),
            modifiers: Modifiers {
                control: !cfg!(target_os = "macos"),
                meta: cfg!(target_os = "macos"),
                ..Default::default()
            },
            repeat: false,
        };
        ui.dispatch(press());
        assert_eq!(override_count.get(), 1);
        assert!(output.borrow().is_empty());
        view.unmount();
        ui.input.focus(&ui.scene, None);
        ui.dispatch(press());
        assert_eq!(*output.borrow(), ["save"]);
        drop(binding);
        let replacement = vec![AppMenu::new(
            "Tools",
            [AppMenuEntry::item("Disabled", "save")
                .enabled(false)
                .accelerator(MenuAccelerator::primary(Key::Character("s".into())))],
        )];
        let _binding = ui.input.bind_keys(root, keymap(&replacement));
        ui.dispatch(press());
        assert_eq!(*output.borrow(), ["save"]);
        // Direct application actions still route through the same typed channel.
        ui.input
            .dispatch_action(&ui.scene, Action::new(MenuAction("new".into())));
        assert_eq!(*output.borrow(), ["save", "new"]);
    }
    #[test]
    fn updates_preserve_shortcut_and_reject_ambiguous_accelerators() {
        let shortcut = MenuAccelerator::primary(zgui::input::Key::Character("s".into()));
        let mut model = vec![AppMenu::new(
            "File",
            [AppMenuEntry::item("Save", "save").accelerator(shortcut.clone())],
        )];
        assert!(update_entry(
            &mut model,
            &"save".into(),
            Some(false),
            Some(true)
        ));
        assert!(!enabled_action(&model, &"save".into()));
        assert!(
            matches!(&model[0].entries[0],AppMenuEntry::Item{checked:Some(true),accelerator:Some(a),..} if a==&shortcut)
        );
        assert!(!update_entry(
            &mut model,
            &"missing".into(),
            Some(true),
            None
        ));
        model[0]
            .entries
            .push(AppMenuEntry::item("Other", "other").accelerator(shortcut));
        assert!(validate(&model).is_err());
    }
}
#[cfg(test)]
mod live_model_test {
    use super::*;
    #[test]
    fn replacing_rendered_bar_settles_without_changing_editor_ownership() {
        use zgui::widgets::Ui;
        let mut ui = Ui::new(500., 300.);
        let model = ui.signal(vec![AppMenu::new(
            "File",
            [AppMenuEntry::item("Save", "save")],
        )]);
        let read = model.clone();
        let view = ui.mount(compose::switch(
            move || read.get(),
            |menus, _| app_menu_bar(menus, |_| {}).unwrap(),
        ));
        ui.prepare_frame();
        model.set(vec![AppMenu::new(
            "File",
            [AppMenuEntry::item("Save", "save")
                .enabled(false)
                .checked(true)],
        )]);
        ui.prepare_frame();
        model.set(vec![AppMenu::new(
            "Tools",
            [AppMenuEntry::item("New", "new")],
        )]);
        ui.prepare_frame();
        view.unmount();
    }
}
