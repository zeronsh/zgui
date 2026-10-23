//! The desktop's appearance settings on Linux, from the XDG settings portal
//! (`org.freedesktop.portal.Settings`, which GNOME, KDE and others back):
//! today the accent colour, which the text selection takes, as on macOS.
//! A thread reads the settings at start and follows `SettingChanged`; the
//! event loop then gives them to every window's themes.
use dbus::{
    arg::{RefArg, Variant},
    blocking::Connection,
    message::MatchRule,
};
use std::{
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    thread,
    time::Duration,
};
use zgui::scene::Color;

const SERVICE: &str = "org.freedesktop.portal.Desktop";
const PATH: &str = "/org/freedesktop/portal/desktop";
const SETTINGS: &str = "org.freedesktop.portal.Settings";
const APPEARANCE: &str = "org.freedesktop.appearance";

/// The portal's settings as last read.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct Appearance {
    /// The user's accent colour, if they chose one.
    pub accent: Option<Color>,
}

static CURRENT: Mutex<Appearance> = Mutex::new(Appearance { accent: None });

/// The settings as last read (the default until the portal answers).
pub(crate) fn current() -> Appearance {
    *CURRENT.lock().unwrap()
}

/// Follows the portal's settings until dropped.
pub(crate) struct Watcher {
    stop: Arc<AtomicBool>,
    worker: Option<thread::JoinHandle<()>>,
}
impl Drop for Watcher {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

/// Read the settings and follow them; `changed` runs (on the watcher's
/// thread) whenever what `current` returns changes. None without a session
/// bus.
pub(crate) fn watch(changed: impl Fn() + Send + 'static) -> Option<Watcher> {
    let connection = Connection::new_session().ok()?;
    let stop = Arc::new(AtomicBool::new(false));
    let running = stop.clone();
    let worker = thread::Builder::new()
        .name("zgui-appearance".into())
        .spawn(move || {
            let proxy = connection.with_proxy(SERVICE, PATH, Duration::from_secs(2));
            let read = |key: &str| -> Option<Box<dyn RefArg>> {
                // Version 2's ReadOne; version 1's Read wraps the value in a
                // second variant.
                if let Ok((Variant(value),)) = proxy
                    .method_call::<(Variant<Box<dyn RefArg>>,), _, _, _>(
                        SETTINGS,
                        "ReadOne",
                        (APPEARANCE, key),
                    )
                {
                    return Some(value);
                }
                let (Variant(outer),): (Variant<Box<dyn RefArg>>,) = proxy
                    .method_call(SETTINGS, "Read", (APPEARANCE, key))
                    .ok()?;
                outer.as_iter()?.next().map(|inner| inner.box_clone())
            };
            set(Appearance {
                accent: read("accent-color").as_deref().and_then(accent),
            });
            changed();
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
                    if namespace == APPEARANCE && key == "accent-color" {
                        let mut next = current();
                        next.accent = accent(&*value);
                        if set(next) {
                            changed();
                        }
                    }
                    true
                },
            );
            if subscribed.is_err() {
                return;
            }
            while !running.load(Ordering::Acquire) {
                if connection.process(Duration::from_millis(250)).is_err() {
                    return;
                }
            }
        })
        .ok()?;
    Some(Watcher {
        stop,
        worker: Some(worker),
    })
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
