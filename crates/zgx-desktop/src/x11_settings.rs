//! The X settings (XSETTINGS) a desktop's settings daemon publishes to its
//! session's X11 clients: xfsettingsd's on XFCE, gsd-xsettings' on GNOME's
//! X11 session, xsettingsd's elsewhere. GTK on X11 takes the double-click
//! time and the drag threshold from them (`Net/DoubleClickTime`,
//! `Net/DndDragThreshold`) ahead of anything else, and so do zgx's windows
//! there, and its theme (`Net/ThemeName`), dark where XFCE's dark themes
//! are: on XFCE the settings portal, where one runs, answers with GNOME's
//! defaults, not the person's settings. Read with the system's other
//! settings (`system_colors::sync`): at startup and as a window is
//! activated, which is where a change made in the settings app arrives.
use std::sync::atomic::{AtomicBool, Ordering};
use x11rb::{
    connection::Connection, protocol::xproto::ConnectionExt as _, rust_connection::RustConnection,
};

/// Whether the app's windows are on X11.
static ON_X11: AtomicBool = AtomicBool::new(false);

/// The app's windows are on X11: its settings are read from now on. An app
/// on Wayland never connects, which would start Xwayland where it starts on
/// demand.
pub(crate) fn enable() {
    ON_X11.store(true, Ordering::Relaxed);
}

/// The settings zgx takes from XSETTINGS.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct Settings {
    /// `Net/DoubleClickTime`, in milliseconds.
    pub double_click: Option<u32>,
    /// `Net/DndDragThreshold`, in pixels.
    pub drag_threshold: Option<u32>,
    /// `Gtk/OverlayScrolling`: whether scrollbars hide until their view
    /// scrolls.
    pub overlay_scrolling: Option<bool>,
    /// Whether `Net/ThemeName`, the GTK theme, is a dark one: named so
    /// (`Adwaita-dark`, `Greybird-dark`, `Arc-Dark`), as dark themes are.
    pub dark_theme: Option<bool>,
    /// `Gtk/EnablePrimaryPaste`: whether a middle click pastes the primary
    /// selection.
    pub primary_paste: Option<bool>,
    /// The family of `Gtk/FontName`, the interface font (a Pango
    /// description, `"Noto Sans 10"`).
    pub font: Option<String>,
}

/// The settings the session's manager publishes now: none off X11, or with
/// no manager running.
pub(crate) fn current() -> Settings {
    if !ON_X11.load(Ordering::Relaxed) {
        return Settings::default();
    }
    // The connection and the two atoms, kept: a read is then two requests.
    thread_local! {
        static CONNECTION: Option<(RustConnection, u32, u32)> = connect();
    }
    CONNECTION
        .with(|connection| {
            let (connection, selection, property) = connection.as_ref()?;
            read(connection, *selection, *property)
        })
        .unwrap_or_default()
}

fn connect() -> Option<(RustConnection, u32, u32)> {
    let (connection, screen) = x11rb::connect(None).ok()?;
    let atom = |name: &str| {
        Some(
            connection
                .intern_atom(false, name.as_bytes())
                .ok()?
                .reply()
                .ok()?
                .atom,
        )
    };
    let selection = atom(&format!("_XSETTINGS_S{screen}"))?;
    let property = atom("_XSETTINGS_SETTINGS")?;
    Some((connection, selection, property))
}

/// The settings on the window owning `selection`, the manager's.
fn read(connection: &impl Connection, selection: u32, property: u32) -> Option<Settings> {
    let owner = connection
        .get_selection_owner(selection)
        .ok()?
        .reply()
        .ok()?
        .owner;
    if owner == x11rb::NONE {
        return None;
    }
    let reply = connection
        .get_property(false, owner, property, property, 0, u32::MAX / 4)
        .ok()?
        .reply()
        .ok()?;
    Some(parse(&reply.value))
}

/// `_XSETTINGS_SETTINGS`' value, as the XSETTINGS specification lays it
/// out: a byte order, a serial and the settings, each typed (an integer, a
/// string or a colour), named and padded to four bytes. What zgx doesn't use
/// is skipped, and a value cut short ends the reading.
fn parse(data: &[u8]) -> Settings {
    let mut settings = Settings::default();
    // `MSBFirst` is 1, `LSBFirst` 0.
    let big = data.first() == Some(&1);
    let u16_at = |at: usize| {
        let bytes: [u8; 2] = data.get(at..at + 2)?.try_into().ok()?;
        Some(if big {
            u16::from_be_bytes(bytes)
        } else {
            u16::from_le_bytes(bytes)
        })
    };
    let u32_at = |at: usize| {
        let bytes: [u8; 4] = data.get(at..at + 4)?.try_into().ok()?;
        Some(if big {
            u32::from_be_bytes(bytes)
        } else {
            u32::from_le_bytes(bytes)
        })
    };
    let padded = |n: usize| n.div_ceil(4) * 4;
    let Some(count) = u32_at(8) else {
        return settings;
    };
    let mut at = 12;
    for _ in 0..count {
        let (Some(&kind), Some(length)) = (data.get(at), u16_at(at + 2)) else {
            break;
        };
        let Some(name) = data.get(at + 4..at + 4 + usize::from(length)) else {
            break;
        };
        // The name, then the serial it last changed at.
        at += 4 + padded(usize::from(length)) + 4;
        match kind {
            0 => {
                let Some(value) = u32_at(at) else {
                    break;
                };
                at += 4;
                let positive = u32::try_from(value as i32).ok().filter(|&v| v > 0);
                match name {
                    b"Net/DoubleClickTime" => settings.double_click = positive,
                    b"Net/DndDragThreshold" => settings.drag_threshold = positive,
                    b"Gtk/OverlayScrolling" => settings.overlay_scrolling = Some(value != 0),
                    b"Gtk/EnablePrimaryPaste" => settings.primary_paste = Some(value != 0),
                    _ => {}
                }
            }
            1 => {
                let Some(length) = u32_at(at) else {
                    break;
                };
                let Some(text) = data.get(at + 4..at + 4 + length as usize) else {
                    break;
                };
                match name {
                    b"Net/ThemeName" => {
                        settings.dark_theme = Some(text.to_ascii_lowercase().ends_with(b"-dark"))
                    }
                    b"Gtk/FontName" => {
                        settings.font = std::str::from_utf8(text)
                            .ok()
                            .and_then(crate::linux_appearance::pango_family)
                    }
                    _ => {}
                }
                at += 4 + padded(length as usize);
            }
            2 => at += 8,
            _ => break,
        }
    }
    settings
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Settings as a manager writes them, in `big` or little endian.
    fn encode(big: bool, settings: &[(&str, Value)]) -> Vec<u8> {
        let u16 = |v: u16| {
            if big {
                v.to_be_bytes()
            } else {
                v.to_le_bytes()
            }
        };
        let u32 = |v: u32| {
            if big {
                v.to_be_bytes()
            } else {
                v.to_le_bytes()
            }
        };
        let pad = |out: &mut Vec<u8>| out.resize(out.len().div_ceil(4) * 4, 0);
        let mut out = vec![u8::from(big), 0, 0, 0];
        out.extend(u32(7));
        out.extend(u32(settings.len() as u32));
        for (name, value) in settings {
            let kind = match value {
                Value::Int(_) => 0,
                Value::Str(_) => 1,
                Value::Color => 2,
            };
            out.extend([kind, 0]);
            out.extend(u16(name.len() as u16));
            out.extend(name.as_bytes());
            pad(&mut out);
            out.extend(u32(3));
            match value {
                Value::Int(v) => out.extend(u32(*v as u32)),
                Value::Str(s) => {
                    out.extend(u32(s.len() as u32));
                    out.extend(s.as_bytes());
                    pad(&mut out);
                }
                Value::Color => out.extend([0xff; 8]),
            }
        }
        out
    }
    enum Value {
        Int(i32),
        Str(&'static str),
        Color,
    }

    #[test]
    fn the_double_click_time_and_drag_threshold_are_read_past_the_rest() {
        for big in [false, true] {
            let data = encode(
                big,
                &[
                    ("Net/ThemeName", Value::Str("Greybird-Dark")),
                    ("Gtk/ColorPalette", Value::Color),
                    ("Gtk/FontName", Value::Str("Noto Sans Bold 10")),
                    ("Net/DoubleClickTime", Value::Int(650)),
                    ("Xft/DPI", Value::Int(98_304)),
                    ("Net/DndDragThreshold", Value::Int(14)),
                    ("Gtk/OverlayScrolling", Value::Int(0)),
                    ("Gtk/EnablePrimaryPaste", Value::Int(0)),
                ],
            );
            assert_eq!(
                parse(&data),
                Settings {
                    double_click: Some(650),
                    drag_threshold: Some(14),
                    overlay_scrolling: Some(false),
                    dark_theme: Some(true),
                    primary_paste: Some(false),
                    font: Some("Noto Sans".into()),
                },
                "big endian: {big}"
            );
        }
    }

    #[test]
    fn a_value_cut_short_or_out_of_range_is_none() {
        let data = encode(false, &[("Net/DndDragThreshold", Value::Int(-1))]);
        assert_eq!(parse(&data), Settings::default());
        let data = encode(false, &[("Net/DoubleClickTime", Value::Int(400))]);
        assert_eq!(parse(&data[..data.len() - 2]), Settings::default());
        assert_eq!(parse(&[]), Settings::default());
        let data = encode(true, &[("Net/ThemeName", Value::Str("Adwaita"))]);
        assert_eq!(parse(&data).dark_theme, Some(false), "a light theme");
    }
}
