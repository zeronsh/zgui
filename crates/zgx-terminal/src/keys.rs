//! What keys, the mouse and pastes send a program, as xterm sends them.
use alacritty_terminal::term::TermMode;
use zgx::input::{Key, Modifiers, PointerButton};

/// xterm's modifier parameter: 1 plus shift 1, alt 2, control 4.
fn parameter(modifiers: Modifiers) -> u8 {
    1 + u8::from(modifiers.shift) + 2 * u8::from(modifiers.alt) + 4 * u8::from(modifiers.control)
}

/// What `key` sends with `modifiers` in `mode`, or `None` for a key that
/// types text (the text event sends it) or that the app keeps (⌘ chords on
/// macOS).
pub(crate) fn encode(key: &Key, modifiers: Modifiers, mode: TermMode) -> Option<Vec<u8>> {
    if modifiers.meta {
        return None;
    }
    let m = parameter(modifiers);
    let plain = m == 1;
    let esc = |bytes: &[u8]| -> Vec<u8> {
        if modifiers.alt {
            [b"\x1b".as_slice(), bytes].concat()
        } else {
            bytes.to_vec()
        }
    };
    // A cursor key: CSI or SS3 (application mode) unmodified, CSI 1;m
    // modified.
    let cursor = |final_byte: u8| -> Vec<u8> {
        if plain {
            if mode.contains(TermMode::APP_CURSOR) {
                vec![0x1b, b'O', final_byte]
            } else {
                vec![0x1b, b'[', final_byte]
            }
        } else {
            format!("\x1b[1;{m}{}", final_byte as char).into_bytes()
        }
    };
    let tilde = |number: u8| -> Vec<u8> {
        if plain {
            format!("\x1b[{number}~").into_bytes()
        } else {
            format!("\x1b[{number};{m}~").into_bytes()
        }
    };
    Some(match key {
        Key::Enter => esc(b"\r"),
        Key::Tab if modifiers.shift => b"\x1b[Z".to_vec(),
        Key::Tab => esc(b"\t"),
        Key::Backspace if modifiers.control => esc(b"\x08"),
        Key::Backspace => esc(b"\x7f"),
        Key::Escape => esc(b"\x1b"),
        Key::Space if modifiers.control => esc(b"\0"),
        Key::Space if modifiers.alt && !cfg!(target_os = "macos") => b"\x1b ".to_vec(),
        Key::Space => return None,
        Key::ArrowUp => cursor(b'A'),
        Key::ArrowDown => cursor(b'B'),
        Key::ArrowRight => cursor(b'C'),
        Key::ArrowLeft => cursor(b'D'),
        Key::Home => cursor(b'H'),
        Key::End => cursor(b'F'),
        Key::Insert => tilde(2),
        Key::Delete => tilde(3),
        Key::PageUp => tilde(5),
        Key::PageDown => tilde(6),
        Key::Function(n @ 1..=4) => {
            let final_byte = b'P' + (n - 1);
            if plain {
                vec![0x1b, b'O', final_byte]
            } else {
                format!("\x1b[1;{m}{}", final_byte as char).into_bytes()
            }
        }
        Key::Function(n @ 5..=20) => {
            const NUMBERS: [u8; 16] = [
                15, 17, 18, 19, 20, 21, 23, 24, 25, 26, 28, 29, 31, 32, 33, 34,
            ];
            tilde(NUMBERS[usize::from(*n) - 5])
        }
        Key::Function(_) | Key::ContextMenu => return None,
        Key::Character(text) => {
            let mut chars = text.chars();
            let (Some(c), None) = (chars.next(), chars.next()) else {
                return None;
            };
            if modifiers.control {
                let control = match c.to_ascii_lowercase() {
                    c @ 'a'..='z' => c as u8 & 0x1f,
                    '@' | '2' | ' ' => 0,
                    '[' | '3' => 0x1b,
                    '\\' | '4' => 0x1c,
                    ']' | '5' => 0x1d,
                    '^' | '6' => 0x1e,
                    '_' | '-' | '/' | '7' => 0x1f,
                    '?' | '8' => 0x7f,
                    _ => return None,
                };
                esc(&[control])
            } else if modifiers.alt && !cfg!(target_os = "macos") {
                // Option types its own characters on macOS (the text
                // event); elsewhere Alt is Meta: ESC before the key.
                esc(c.to_string().as_bytes())
            } else {
                return None;
            }
        }
    })
}

/// `text` as a paste: bracketed when the program asked (with any end
/// marker in it taken out, so the paste can't end itself early), else with
/// its line ends as Enter.
pub(crate) fn paste(text: &str, mode: TermMode) -> Vec<u8> {
    if mode.contains(TermMode::BRACKETED_PASTE) {
        let inner = text.replace("\x1b[201~", "").replace('\x1b', "");
        format!("\x1b[200~{inner}\x1b[201~").into_bytes()
    } else {
        text.replace("\r\n", "\r").replace('\n', "\r").into_bytes()
    }
}

/// A mouse report's event.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Mouse {
    Press(PointerButton),
    Release(PointerButton),
    /// Moved, with this button held (or none).
    Move(Option<PointerButton>),
    /// The wheel, a line up (`true`) or down.
    Wheel(bool),
}

/// The report of `event` at cell (`column`, `line`), 0-based, if the
/// program's mouse mode wants it.
pub(crate) fn mouse(
    event: Mouse,
    column: usize,
    line: usize,
    modifiers: Modifiers,
    mode: TermMode,
) -> Option<Vec<u8>> {
    if !mode.intersects(TermMode::MOUSE_MODE) {
        return None;
    }
    let button_code = |button: PointerButton| match button {
        PointerButton::Primary => Some(0),
        PointerButton::Middle => Some(1),
        PointerButton::Secondary => Some(2),
        PointerButton::Other(_) => None,
    };
    let (mut code, release) = match event {
        Mouse::Press(button) => (button_code(button)?, false),
        Mouse::Release(button) => (button_code(button)?, true),
        Mouse::Move(held) => {
            match held {
                Some(_) if !mode.intersects(TermMode::MOUSE_DRAG | TermMode::MOUSE_MOTION) => {
                    return None;
                }
                None if !mode.contains(TermMode::MOUSE_MOTION) => return None,
                _ => {}
            }
            (32 + held.and_then(button_code).unwrap_or(3), false)
        }
        Mouse::Wheel(up) => (if up { 64 } else { 65 }, false),
    };
    code += 4 * u8::from(modifiers.shift)
        + 8 * u8::from(modifiers.alt)
        + 16 * u8::from(modifiers.control);
    if mode.contains(TermMode::SGR_MOUSE) {
        let end = if release { 'm' } else { 'M' };
        return Some(format!("\x1b[<{code};{};{}{end}", column + 1, line + 1).into_bytes());
    }
    // X10/normal: release is button 3; positions offset by 32, one byte
    // each (two in UTF-8 mode, up to 2015).
    if release {
        code = (code & !3) | 3;
    }
    let mut report = vec![0x1b, b'[', b'M', 32 + code];
    for position in [column, line] {
        let value = position + 1 + 32;
        if mode.contains(TermMode::UTF8_MOUSE) {
            let c = char::from_u32(value.min(2047) as u32)?;
            let mut buffer = [0; 4];
            report.extend_from_slice(c.encode_utf8(&mut buffer).as_bytes());
        } else {
            report.push(value.min(255) as u8);
        }
    }
    Some(report)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mods(shift: bool, alt: bool, control: bool) -> Modifiers {
        Modifiers {
            shift,
            alt,
            control,
            meta: false,
        }
    }

    #[test]
    fn keys_send_what_xterm_sends() {
        let none = mods(false, false, false);
        let normal = TermMode::empty();
        let send = |key: Key, m: Modifiers, mode: TermMode| encode(&key, m, mode);
        assert_eq!(send(Key::Enter, none, normal).unwrap(), b"\r");
        assert_eq!(send(Key::Backspace, none, normal).unwrap(), b"\x7f");
        assert_eq!(
            send(Key::Tab, mods(true, false, false), normal).unwrap(),
            b"\x1b[Z"
        );
        assert_eq!(send(Key::ArrowUp, none, normal).unwrap(), b"\x1b[A");
        assert_eq!(
            send(Key::ArrowUp, none, TermMode::APP_CURSOR).unwrap(),
            b"\x1bOA"
        );
        assert_eq!(
            send(Key::ArrowLeft, mods(false, false, true), normal).unwrap(),
            b"\x1b[1;5D"
        );
        assert_eq!(
            send(Key::Delete, mods(true, false, false), normal).unwrap(),
            b"\x1b[3;2~"
        );
        assert_eq!(send(Key::Function(1), none, normal).unwrap(), b"\x1bOP");
        assert_eq!(send(Key::Function(5), none, normal).unwrap(), b"\x1b[15~");
        assert_eq!(
            send(Key::Function(12), mods(false, false, true), normal).unwrap(),
            b"\x1b[24;5~"
        );
        let ctrl = mods(false, false, true);
        assert_eq!(
            send(Key::Character("c".into()), ctrl, normal).unwrap(),
            b"\x03"
        );
        assert_eq!(
            send(Key::Character("[".into()), ctrl, normal).unwrap(),
            b"\x1b"
        );
        assert_eq!(send(Key::Space, ctrl, normal).unwrap(), b"\0");
        // Plain characters are the text event's to send.
        assert_eq!(send(Key::Character("a".into()), none, normal), None);
        assert_eq!(send(Key::Space, none, normal), None);
        // The app's: ⌘ chords.
        let meta = Modifiers { meta: true, ..none };
        assert_eq!(send(Key::Character("t".into()), meta, normal), None);
    }

    #[test]
    fn pastes_are_bracketed_and_cant_end_themselves() {
        assert_eq!(paste("a\nb\r\nc", TermMode::empty()), b"a\rb\rc");
        assert_eq!(
            paste("x\x1b[201~rm -rf", TermMode::BRACKETED_PASTE),
            b"\x1b[200~xrm -rf\x1b[201~"
        );
    }

    #[test]
    fn mouse_reports_in_each_encoding() {
        let none = mods(false, false, false);
        let click = TermMode::MOUSE_REPORT_CLICK;
        let left = PointerButton::Primary;
        assert_eq!(
            mouse(Mouse::Press(left), 0, 0, none, TermMode::empty()),
            None
        );
        assert_eq!(
            mouse(Mouse::Press(left), 2, 4, none, click).unwrap(),
            b"\x1b[M\x20\x23\x25"
        );
        assert_eq!(
            mouse(Mouse::Release(left), 2, 4, none, click).unwrap(),
            b"\x1b[M\x23\x23\x25"
        );
        let sgr = click | TermMode::SGR_MOUSE;
        assert_eq!(
            mouse(Mouse::Press(left), 2, 4, mods(false, false, true), sgr).unwrap(),
            b"\x1b[<16;3;5M"
        );
        assert_eq!(
            mouse(Mouse::Release(left), 2, 4, none, sgr).unwrap(),
            b"\x1b[<0;3;5m"
        );
        assert_eq!(
            mouse(Mouse::Wheel(true), 0, 0, none, sgr).unwrap(),
            b"\x1b[<64;1;1M"
        );
        // Motion only as the mode asks.
        assert_eq!(mouse(Mouse::Move(None), 0, 0, none, sgr), None);
        assert_eq!(mouse(Mouse::Move(Some(left)), 0, 0, none, sgr), None);
        let drag = sgr | TermMode::MOUSE_DRAG;
        assert_eq!(
            mouse(Mouse::Move(Some(left)), 1, 1, none, drag).unwrap(),
            b"\x1b[<32;2;2M"
        );
        let motion = sgr | TermMode::MOUSE_MOTION;
        assert_eq!(
            mouse(Mouse::Move(None), 1, 1, none, motion).unwrap(),
            b"\x1b[<35;2;2M"
        );
    }
}
