//! A WebView2 page's keys that the page left alone, for the app's shortcuts
//! (a menu item's key, a zgx key binding), as a browser hands a page's
//! unused keys to its own.
//!
//! WebView2 says nothing of what a page did with a key, so a script of the
//! page's ([`SCRIPT`]) posts each trusted key with Ctrl, Alt or the Windows
//! key held (or a function key) that the page did not `preventDefault`,
//! once every listener of the page's has run. Any script in the page can
//! post the same message, so the host takes one only as a key it saw
//! pressed: WebView2's `AcceleratorKeyPressed` for that page within the
//! last second, the same key and modifiers, each taken once
//! ([`Pressed`]). The browser's own keys (editing, find, print, reload,
//! zoom, history, devtools) stay the browser's ([`browsers`]).
#![cfg_attr(not(windows), allow(dead_code))]
use std::time::{Duration, Instant};
use zgx::input::{Key, Modifiers};

/// The page's half: top frame only (an iframe has no `chrome.webview`).
pub(super) const SCRIPT: &str = "(() => { const view = window.chrome && window.chrome.webview; if (!view || window !== window.top) return; addEventListener('keydown', (e) => { if (!e.isTrusted || !(e.ctrlKey || e.altKey || e.metaKey || /^F[0-9]+$/.test(e.key))) return; setTimeout(() => { if (!e.defaultPrevented) view.postMessage(['zgx:key', e.key, e.code, [e.ctrlKey, e.altKey, e.shiftKey, e.metaKey].map(Number).join('')].join('\\u0001')); }, 0); }, true); })();";

/// How long a pressed key waits for the page's word on it.
const WAIT: Duration = Duration::from_secs(1);

/// The keys WebView2 saw pressed in a page, not yet matched: at most a
/// few, each a second old at most.
#[derive(Default)]
pub(super) struct Pressed(Vec<(u32, Modifiers, Instant)>);
impl Pressed {
    /// Virtual key `vk` with `modifiers` was pressed in the page at `at`.
    pub(super) fn record(&mut self, vk: u32, modifiers: Modifiers, at: Instant) {
        self.0
            .retain(|(_, _, when)| at.duration_since(*when) < WAIT);
        if self.0.len() < 16 {
            self.0.push((vk, modifiers, at));
        }
    }
    /// Whether the page's forwarded key (`vk` with `modifiers`, heard at
    /// `at`) was pressed in the last second; taken, so it counts once.
    pub(super) fn take(&mut self, vk: u32, modifiers: Modifiers, at: Instant) -> bool {
        self.0
            .retain(|(_, _, when)| at.saturating_duration_since(*when) < WAIT);
        match self
            .0
            .iter()
            .position(|(key, held, _)| *key == vk && *held == modifiers)
        {
            Some(index) => {
                self.0.remove(index);
                true
            }
            None => false,
        }
    }
}

/// The virtual keys whose presses the app took from the page (reserved
/// shortcuts): their releases are the app's too, whatever has changed
/// since (the reservation, the focus), so the page never sees half a key.
/// A held key's repeats are one press; other keys' releases, modifiers'
/// among them, stay the page's.
#[derive(Default)]
pub(super) struct Swallowed(Vec<u32>);
impl Swallowed {
    /// The app took the press of `vk`.
    pub(super) fn press(&mut self, vk: u32) {
        if !self.0.contains(&vk) {
            self.0.push(vk);
        }
    }
    /// `vk` was released: whether its press was the app's (once).
    pub(super) fn release(&mut self, vk: u32) -> bool {
        match self.0.iter().position(|key| *key == vk) {
            Some(index) => {
                self.0.swap_remove(index);
                true
            }
            None => false,
        }
    }
}

/// A key the page's script forwarded: zgx's key, the virtual key it is
/// on, and the modifiers held.
#[derive(Debug, PartialEq)]
pub(super) struct Forwarded {
    pub key: Key,
    pub vk: u32,
    pub modifiers: Modifiers,
}

/// The page's message, if it is [`SCRIPT`]'s: `zgx:key`, the DOM `key`,
/// its `code` and the modifiers held (`1` or `0` for Ctrl, Alt, Shift and
/// the Windows key), parted by U+0001, which no key's name or code has.
pub(super) fn parse(body: &str) -> Option<Forwarded> {
    let mut fields = body.strip_prefix("zgx:key\u{1}")?.split('\u{1}');
    let (key, code, held) = (fields.next()?, fields.next()?, fields.next()?);
    if fields.next().is_some() {
        return None;
    }
    let held: Vec<bool> = held
        .chars()
        .map(|c| match c {
            '1' => Some(true),
            '0' => Some(false),
            _ => None,
        })
        .collect::<Option<_>>()?;
    let [control, alt, shift, meta] = held[..] else {
        return None;
    };
    let modifiers = Modifiers {
        control,
        alt,
        shift,
        meta,
    };
    let vk = vk_of(code)?;
    Some(Forwarded {
        key: key_of(key, code, modifiers)?,
        vk,
        modifiers,
    })
}

/// The virtual key of a key's DOM `code` (its physical key), for the keys
/// shortcuts use.
fn vk_of(code: &str) -> Option<u32> {
    let letter = |rest: &str| {
        let mut chars = rest.chars();
        match (chars.next(), chars.next()) {
            (Some(c), None) => Some(c),
            _ => None,
        }
    };
    if let Some(c) = code.strip_prefix("Key").and_then(letter)
        && c.is_ascii_uppercase()
    {
        return Some(c as u32);
    }
    if let Some(c) = code.strip_prefix("Digit").and_then(letter)
        && c.is_ascii_digit()
    {
        return Some(c as u32);
    }
    if let Some(n) = code.strip_prefix('F').and_then(|n| n.parse::<u32>().ok())
        && (1..=24).contains(&n)
    {
        return Some(0x70 + n - 1);
    }
    Some(match code {
        "Backspace" => 0x08,
        "Tab" => 0x09,
        "Enter" | "NumpadEnter" => 0x0D,
        "Escape" => 0x1B,
        "Space" => 0x20,
        "PageUp" => 0x21,
        "PageDown" => 0x22,
        "End" => 0x23,
        "Home" => 0x24,
        "ArrowLeft" => 0x25,
        "ArrowUp" => 0x26,
        "ArrowRight" => 0x27,
        "ArrowDown" => 0x28,
        "Insert" => 0x2D,
        "Delete" => 0x2E,
        "ContextMenu" => 0x5D,
        "Semicolon" => 0xBA,
        "Equal" => 0xBB,
        "Comma" => 0xBC,
        "Minus" => 0xBD,
        "Period" => 0xBE,
        "Slash" => 0xBF,
        "Backquote" => 0xC0,
        "BracketLeft" => 0xDB,
        "Backslash" => 0xDC,
        "BracketRight" => 0xDD,
        "Quote" => 0xDE,
        _ => return None,
    })
}

/// zgx's key for virtual key `vk` (a key WebView2 saw pressed), with its
/// US-layout character for a letter, digit or punctuation key: for the
/// shortcuts the app keeps, asked before the page has the key. A letter is
/// lowercase (shortcuts bind both cases).
pub(super) fn key_of_vk(vk: u32) -> Option<Key> {
    Some(match vk {
        0x41..=0x5A | 0x30..=0x39 => {
            Key::Character(char::from_u32(vk)?.to_ascii_lowercase().to_string())
        }
        0x70..=0x87 => Key::Function((vk - 0x70 + 1) as u8),
        0x08 => Key::Backspace,
        0x09 => Key::Tab,
        0x0D => Key::Enter,
        0x1B => Key::Escape,
        0x20 => Key::Space,
        0x21 => Key::PageUp,
        0x22 => Key::PageDown,
        0x23 => Key::End,
        0x24 => Key::Home,
        0x25 => Key::ArrowLeft,
        0x26 => Key::ArrowUp,
        0x27 => Key::ArrowRight,
        0x28 => Key::ArrowDown,
        0x2D => Key::Insert,
        0x2E => Key::Delete,
        0x5D => Key::ContextMenu,
        0xBA => Key::Character(";".into()),
        0xBB => Key::Character("=".into()),
        0xBC => Key::Character(",".into()),
        0xBD => Key::Character("-".into()),
        0xBE => Key::Character(".".into()),
        0xBF => Key::Character("/".into()),
        0xC0 => Key::Character("`".into()),
        0xDB => Key::Character("[".into()),
        0xDC => Key::Character("\\".into()),
        0xDD => Key::Character("]".into()),
        0xDE => Key::Character("'".into()),
        _ => return None,
    })
}

/// zgx's key for a DOM `key` and `code`: with Ctrl or the Windows key held
/// (not Alt), a character outside ASCII is the physical key's on a US
/// layout, as the window's own shortcuts take it.
fn key_of(key: &str, code: &str, modifiers: Modifiers) -> Option<Key> {
    Some(match key {
        " " | "Spacebar" => Key::Space,
        "Tab" => Key::Tab,
        "Enter" => Key::Enter,
        "Escape" => Key::Escape,
        "Backspace" => Key::Backspace,
        "Delete" => Key::Delete,
        "ArrowLeft" => Key::ArrowLeft,
        "ArrowRight" => Key::ArrowRight,
        "ArrowUp" => Key::ArrowUp,
        "ArrowDown" => Key::ArrowDown,
        "Home" => Key::Home,
        "End" => Key::End,
        "PageUp" => Key::PageUp,
        "PageDown" => Key::PageDown,
        "Insert" => Key::Insert,
        "ContextMenu" => Key::ContextMenu,
        _ if key.starts_with('F') && key.len() > 1 => {
            Key::Function(key[1..].parse().ok().filter(|n| (1..=24).contains(n))?)
        }
        _ if key.chars().count() == 1 => {
            let c = key.chars().next()?;
            if c.is_ascii() || modifiers.alt || !(modifiers.control || modifiers.meta) {
                Key::Character(key.into())
            } else {
                let vk = vk_of(code)?;
                let us = char::from_u32(vk).filter(char::is_ascii_alphanumeric)?;
                Key::Character(us.to_ascii_lowercase().to_string())
            }
        }
        _ => return None,
    })
}

/// Whether `key` with `modifiers` is the browser's in a page: editing
/// (cut, copy, paste, select all, undo, redo) and WebView2's browser keys
/// (find, print, reload, zoom, back and forward, devtools), which act
/// whatever the page does and so never also reach the app.
pub(super) fn browsers(key: &Key, modifiers: Modifiers) -> bool {
    let Modifiers {
        control,
        alt,
        shift,
        meta,
    } = modifiers;
    if meta {
        return false;
    }
    let ctrl_only = control && !alt;
    match key {
        Key::Character(c) if ctrl_only => matches!(
            (c.to_ascii_lowercase().as_str(), shift),
            ("c" | "x" | "v" | "a" | "z" | "y" | "f" | "g" | "p" | "r", _)
                | ("i" | "j", true)
                | ("=" | "+" | "-" | "0", _)
        ),
        Key::Insert => (ctrl_only && !shift) || (shift && !control && !alt),
        Key::Delete => shift && !control && !alt,
        Key::ArrowLeft | Key::ArrowRight => alt && !control,
        Key::Function(3 | 5 | 12) => !alt,
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ctrl() -> Modifiers {
        Modifiers {
            control: true,
            ..Default::default()
        }
    }
    fn message(key: &str, code: &str, ctrl: bool, shift: bool) -> String {
        let bit = |on: bool| if on { '1' } else { '0' };
        format!(
            "zgx:key\u{1}{key}\u{1}{code}\u{1}{}0{}0",
            bit(ctrl),
            bit(shift)
        )
    }

    /// A page posting a key no one pressed (any script in it can) gets
    /// nothing; a key pressed counts once, and only for a second.
    #[test]
    fn only_a_key_pressed_in_the_page_is_taken_and_only_once() {
        let now = Instant::now();
        let forwarded = parse(&message("u", "KeyU", true, false)).unwrap();
        assert_eq!(forwarded.key, Key::Character("u".into()));
        let mut pressed = Pressed::default();
        assert!(
            !pressed.take(forwarded.vk, forwarded.modifiers, now),
            "a fake"
        );
        pressed.record(0x55, ctrl(), now);
        let shift = Modifiers {
            shift: true,
            ..ctrl()
        };
        assert!(!pressed.take(0x55, shift, now), "other modifiers");
        assert!(!pressed.take(0x4A, ctrl(), now), "another key");
        assert!(pressed.take(forwarded.vk, forwarded.modifiers, now));
        assert!(
            !pressed.take(forwarded.vk, forwarded.modifiers, now),
            "once"
        );
        pressed.record(0x55, ctrl(), now);
        assert!(
            !pressed.take(0x55, ctrl(), now + Duration::from_millis(1500)),
            "too late"
        );
    }

    /// The release of a key the app took is the app's, once, even after the
    /// reservation went; another key's release, or a modifier's, is not.
    #[test]
    fn a_taken_press_takes_its_release_once() {
        let mut swallowed = Swallowed::default();
        assert!(!swallowed.release(0x4A), "nothing taken");
        swallowed.press(0x4A);
        swallowed.press(0x4A); // Held: a repeat.
        swallowed.press(0x74);
        assert!(!swallowed.release(0x11), "Ctrl's release is the page's");
        assert!(!swallowed.release(0x55), "another key's");
        assert!(swallowed.release(0x4A));
        assert!(!swallowed.release(0x4A), "once, repeats included");
        assert!(swallowed.release(0x74));
    }

    #[test]
    fn a_virtual_key_is_the_key_its_code_is() {
        for (vk, key) in [
            (0x55, Key::Character("u".into())),
            (0x31, Key::Character("1".into())),
            (0x74, Key::Function(5)),
            (0x25, Key::ArrowLeft),
            (0xBB, Key::Character("=".into())),
        ] {
            assert_eq!(key_of_vk(vk), Some(key), "{vk:#x}");
        }
        assert_eq!(key_of_vk(0x10), None, "Shift alone");
        // The page's half agrees: the same key from its code's virtual key.
        let forwarded = parse(&message("u", "KeyU", true, false)).unwrap();
        assert_eq!(key_of_vk(forwarded.vk), Some(forwarded.key));
    }

    #[test]
    fn the_script_posts_what_parse_reads() {
        // Its separator, as JavaScript reads the escape, and its prefix.
        assert!(SCRIPT.contains(r"join('\u0001')"));
        assert!(SCRIPT.contains("['zgx:key', e.key, e.code,"));
        assert!(SCRIPT.contains("e.isTrusted") && SCRIPT.contains("e.defaultPrevented"));
    }

    #[test]
    fn the_scripts_messages_parse_and_others_do_not() {
        let f5 = parse("zgx:key\u{1}F5\u{1}F5\u{1}0000").unwrap();
        assert_eq!((f5.key, f5.vk), (Key::Function(5), 0x74));
        let left = parse("zgx:key\u{1}ArrowLeft\u{1}ArrowLeft\u{1}0100").unwrap();
        assert_eq!(
            (left.key, left.vk, left.modifiers.alt),
            (Key::ArrowLeft, 0x25, true)
        );
        // A Russian layout's letter, with Ctrl: the US key's.
        let cyrillic = parse(&message("г", "KeyU", true, false)).unwrap();
        assert_eq!(cyrillic.key, Key::Character("u".into()));
        for other in [
            "",
            "hello",
            "zgx:key\u{1}u\u{1}KeyU",
            "zgx:key\u{1}u\u{1}KeyU\u{1}10",
            "zgx:key\u{1}u\u{1}KeyU\u{1}1x00",
            "zgx:key\u{1}u\u{1}KeyU\u{1}1000\u{1}more",
            "zgx:key\u{1}u\u{1}Mystery\u{1}1000",
            r#"{"zgx:key":["u","KeyU",true,false,false,false]}"#,
        ] {
            assert_eq!(parse(other), None, "{other}");
        }
    }

    #[test]
    fn editing_and_the_browsers_keys_stay_the_browsers() {
        let shift = |m: Modifiers| Modifiers { shift: true, ..m };
        let alt = Modifiers {
            alt: true,
            ..Default::default()
        };
        let c = |s: &str| Key::Character(s.into());
        for (key, modifiers) in [
            (c("c"), ctrl()),
            (c("V"), ctrl()),
            (c("z"), shift(ctrl())),
            (c("f"), ctrl()),
            (c("p"), ctrl()),
            (c("r"), ctrl()),
            (c("i"), shift(ctrl())),
            (c("="), ctrl()),
            (c("0"), ctrl()),
            (Key::Function(5), Modifiers::default()),
            (Key::Function(12), Modifiers::default()),
            (Key::ArrowLeft, alt),
            (Key::Insert, shift(Modifiers::default())),
        ] {
            assert!(browsers(&key, modifiers), "{key:?} {modifiers:?}");
        }
        for (key, modifiers) in [
            (c("u"), ctrl()),
            (c("k"), ctrl()),
            (c("l"), shift(ctrl())),
            (c("j"), ctrl()),
            (Key::Function(2), Modifiers::default()),
            (Key::ArrowLeft, ctrl()),
        ] {
            assert!(!browsers(&key, modifiers), "{key:?} {modifiers:?}");
        }
    }
}
