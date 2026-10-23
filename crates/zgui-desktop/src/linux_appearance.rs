//! The desktop's appearance settings on Linux, from the XDG settings portal
//! (`org.freedesktop.portal.Settings`, which GNOME, KDE and others back):
//! dark or light, the accent colour (the text selection takes it), high
//! contrast, reduced motion (or GNOME's animations turned off) and GNOME's
//! text size (`text-scaling-factor`, Large Text in its accessibility
//! settings), which scales the themes' font tokens, and KDE's wheel lines
//! (`WheelScrollLines`, how far a wheel notch scrolls), and GNOME's window
//! buttons (`button-layout`, which an undecorated window's title bar draws).
//! They
//! are read at start, before the first window shows, and a thread follows
//! `SettingChanged`, asleep until one arrives; the event loop then gives them
//! to every window's themes.
use dbus::{
    arg::{RefArg, Variant},
    blocking::Connection,
    message::MatchRule,
};
use std::{
    sync::{Arc, Mutex},
    thread,
    time::{Duration, Instant},
};
use zgui::scene::Color;

const SERVICE: &str = "org.freedesktop.portal.Desktop";
const PATH: &str = "/org/freedesktop/portal/desktop";
const SETTINGS: &str = "org.freedesktop.portal.Settings";
const APPEARANCE: &str = "org.freedesktop.appearance";
const GNOME: &str = "org.gnome.desktop.interface";
const GNOME_WM: &str = "org.gnome.desktop.wm.preferences";
/// kdeglobals' `[KDE]` group, as KDE's portal gives it.
const KDE: &str = "org.kde.kdeglobals.KDE";

/// The portal's settings as last read.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct Appearance {
    /// Dark or light, if the user prefers one (`color-scheme`).
    pub scheme: Option<zgui::theme::Appearance>,
    /// The user's accent colour, if they chose one.
    pub accent: Option<Color>,
    /// High contrast (`contrast`).
    pub contrast: bool,
    /// Reduced motion (`reduced-motion`), or GNOME's animations turned off.
    pub reduced_motion: bool,
    /// GNOME's text size, as a factor (`text-scaling-factor`).
    pub text_scale: Option<f32>,
    /// The lines a wheel notch scrolls, if the desktop sets them (KDE's
    /// `WheelScrollLines`, 3 by default; GNOME has no such setting).
    pub wheel_lines: Option<u32>,
    /// Which window buttons go where (GNOME's `button-layout`).
    pub buttons: Option<crate::window_chrome::ButtonLayout>,
}

static CURRENT: Mutex<Appearance> = Mutex::new(Appearance {
    scheme: None,
    accent: None,
    contrast: false,
    reduced_motion: false,
    text_scale: None,
    wheel_lines: None,
    buttons: None,
});

/// The settings as last read (the default until the portal answers).
pub(crate) fn current() -> Appearance {
    *CURRENT.lock().unwrap()
}

/// Follows the portal's settings until dropped.
pub(crate) struct Watcher {
    stop: Arc<crate::linux_dbus::Stop>,
    worker: Option<thread::JoinHandle<()>>,
}
impl Drop for Watcher {
    fn drop(&mut self) {
        self.stop.stop();
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

/// Read the settings now, then follow them; `changed` runs (on the
/// watcher's thread) whenever what `current` returns changes. A portal that
/// doesn't answer within a moment (one D-Bus is still starting) doesn't hold
/// up the first window: the watcher reads it again, waiting longer. None
/// without a session bus.
pub(crate) fn watch(changed: impl Fn() + Send + 'static) -> Option<Watcher> {
    let connection = crate::linux_dbus::session().ok()?;
    let (read_now, answered) = read(&connection, Duration::from_millis(300));
    set(read_now);
    let stop = Arc::new(crate::linux_dbus::Stop::new()?);
    let told = stop.clone();
    let worker = thread::Builder::new()
        .name("zgui-appearance".into())
        .spawn(move || {
            if !answered && set(read(&connection, Duration::from_secs(10)).0) {
                changed();
            }
            // By path, not sender: signals carry the portal's unique name,
            // not the well-known one.
            let mut rule = MatchRule::new_signal(SETTINGS, "SettingChanged");
            rule.path = Some(PATH.into());
            let subscribed = connection.add_match(
                rule,
                move |(namespace, key, Variant(value)): (
                    String,
                    String,
                    Variant<Box<dyn RefArg>>,
                ),
                      _,
                      _| {
                    let mut next = current();
                    apply(&mut next, &namespace, &key, &*value);
                    if set(next) {
                        changed();
                    }
                    true
                },
            );
            if subscribed.is_err() {
                return;
            }
            crate::linux_dbus::serve(&connection, &told);
        })
        .ok()?;
    Some(Watcher {
        stop,
        worker: Some(worker),
    })
}

/// Read the settings, giving the portal `budget` in all to answer; whether
/// it answered in time. No portal, or one without a setting, answers at once.
fn read(connection: &Connection, budget: Duration) -> (Appearance, bool) {
    const UNKNOWN_METHOD: &str = "org.freedesktop.DBus.Error.UnknownMethod";
    let deadline = Instant::now() + budget;
    let mut appearance = Appearance::default();
    // Version 1 has no ReadOne; its Read wraps the value in a second variant.
    let mut version_one = false;
    for (namespace, key) in [
        (APPEARANCE, "color-scheme"),
        (APPEARANCE, "accent-color"),
        (APPEARANCE, "contrast"),
        (APPEARANCE, "reduced-motion"),
        (GNOME, "enable-animations"),
        (GNOME, "text-scaling-factor"),
        (KDE, "WheelScrollLines"),
        (GNOME_WM, "button-layout"),
    ] {
        let left = deadline.saturating_duration_since(Instant::now());
        if left.is_zero() {
            return (appearance, false);
        }
        let proxy = connection.with_proxy(SERVICE, PATH, left);
        let call = |method: &str| {
            proxy
                .method_call(SETTINGS, method, (namespace, key))
                .map(|(Variant(value),): (Variant<Box<dyn RefArg>>,)| value)
        };
        let value = match (!version_one).then(|| call("ReadOne")) {
            Some(Err(e)) if e.name() != Some(UNKNOWN_METHOD) => Err(e),
            Some(Ok(value)) => Ok(Some(value)),
            _ => {
                version_one = true;
                call("Read").map(|outer| Some(outer.as_iter()?.next()?.box_clone()))
            }
        };
        match value {
            Ok(Some(value)) => apply(&mut appearance, namespace, key, &*value),
            Err(e) if e.name() == Some("org.freedesktop.DBus.Error.NoReply") => {
                return (appearance, false);
            }
            Err(e) if e.name() == Some("org.freedesktop.DBus.Error.ServiceUnknown") => break,
            // A setting this desktop doesn't have.
            _ => {}
        }
    }
    (appearance, true)
}

/// One setting's value into `appearance`.
fn apply(appearance: &mut Appearance, namespace: &str, key: &str, value: &dyn RefArg) {
    use zgui::theme::Appearance::{Dark, Light};
    match (namespace, key) {
        (APPEARANCE, "color-scheme") => {
            appearance.scheme = match value.as_u64() {
                Some(1) => Some(Dark),
                Some(2) => Some(Light),
                _ => None,
            }
        }
        (APPEARANCE, "accent-color") => appearance.accent = accent(value),
        (APPEARANCE, "contrast") => appearance.contrast = value.as_u64() == Some(1),
        (APPEARANCE, "reduced-motion") => appearance.reduced_motion = value.as_u64() == Some(1),
        // A bool's RefArg reads as an integer.
        (GNOME, "enable-animations") => appearance.reduced_motion = value.as_i64() == Some(0),
        (GNOME, "text-scaling-factor") => appearance.text_scale = value.as_f64().map(|s| s as f32),
        // KDE's portal gives kdeglobals' entries as their text.
        (KDE, "WheelScrollLines") => {
            appearance.wheel_lines = value
                .as_i64()
                .or_else(|| value.as_str()?.trim().parse().ok())
                .and_then(|lines| u32::try_from(lines).ok())
                .filter(|&lines| lines > 0)
        }
        (GNOME_WM, "button-layout") => {
            appearance.buttons = value
                .as_str()
                .map(crate::window_chrome::ButtonLayout::parse)
        }
        _ => {}
    }
}
/// Store `appearance`; whether it changed.
fn set(appearance: Appearance) -> bool {
    let mut current = CURRENT.lock().unwrap();
    std::mem::replace(&mut *current, appearance) != appearance
}

/// The portal's `accent-color`: `(ddd)` sRGB in 0..1, any channel outside
/// it meaning the user chose none.
fn accent(value: &dyn RefArg) -> Option<Color> {
    let channels: Vec<f64> = value.as_iter()?.filter_map(|c| c.as_f64()).collect();
    let [r, g, b] = channels[..] else {
        return None;
    };
    let channel = |c: f64| (0. ..=1.).contains(&c).then(|| (c * 255.).round() as u8);
    Some(Color(channel(r)?, channel(g)?, channel(b)?, 255))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settings_read_as_the_portal_defines_them() {
        use zgui::theme::Appearance::{Dark, Light};
        let mut a = Appearance::default();
        apply(&mut a, APPEARANCE, "color-scheme", &1u32);
        assert_eq!(a.scheme, Some(Dark));
        apply(&mut a, APPEARANCE, "color-scheme", &2u32);
        assert_eq!(a.scheme, Some(Light));
        apply(&mut a, APPEARANCE, "color-scheme", &0u32);
        assert_eq!(a.scheme, None, "no preference");
        apply(&mut a, APPEARANCE, "contrast", &1u32);
        assert!(a.contrast);
        apply(
            &mut a,
            GNOME_WM,
            "button-layout",
            &"appmenu:close".to_owned(),
        );
        assert_eq!(
            a.buttons,
            Some(crate::window_chrome::ButtonLayout::parse(":close"))
        );
        apply(&mut a, APPEARANCE, "reduced-motion", &1u32);
        assert!(a.reduced_motion);
        apply(&mut a, GNOME, "enable-animations", &true);
        assert!(!a.reduced_motion, "GNOME's animations on");
        apply(&mut a, GNOME, "enable-animations", &false);
        assert!(a.reduced_motion);
        apply(&mut a, GNOME, "text-scaling-factor", &1.25f64);
        assert_eq!(a.text_scale, Some(1.25), "Large Text");
        apply(&mut a, KDE, "WheelScrollLines", &"5".to_string());
        assert_eq!(a.wheel_lines, Some(5), "as kdeglobals' text");
        apply(&mut a, KDE, "WheelScrollLines", &7i32);
        assert_eq!(a.wheel_lines, Some(7));
        apply(&mut a, KDE, "WheelScrollLines", &"0".to_string());
        assert_eq!(a.wheel_lines, None, "no lines: the default");
        apply(&mut a, "org.example", "contrast", &0u32);
        assert!(a.contrast, "other namespaces change nothing");
    }

    #[test]
    fn accents_are_srgb_triples_in_range() {
        let triple = |r: f64, g: f64, b: f64| -> Box<dyn RefArg> { Box::new((r, g, b)) };
        assert_eq!(accent(&*triple(1., 0.5, 0.)), Some(Color(255, 128, 0, 255)));
        assert_eq!(
            accent(&*triple(-1., 0., 0.)),
            None,
            "out of range: no accent chosen"
        );
        assert_eq!(accent(&*triple(0., 0., 2.)), None);
        assert_eq!(accent(&(1u32) as &dyn RefArg), None);
    }
}
