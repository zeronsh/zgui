use std::io;
use std::os::raw::*;
use std::path::{Path, PathBuf};
use std::str::Utf8Error;
use std::sync::Arc;

use percent_encoding::percent_decode;
use x11rb::protocol::xproto::{self, ConnectionExt};

use super::atoms::AtomName::None as DndNone;
use super::atoms::*;
use super::{util, CookieResultExt, X11Error, XConnection};

#[derive(Debug, Clone, Copy)]
pub enum DndState {
    Accepted,
    Rejected,
}

#[derive(Debug)]
pub enum DndDataParseError {
    EmptyData,
    InvalidUtf8(#[allow(dead_code)] Utf8Error),
    HostnameSpecified(#[allow(dead_code)] String),
    UnexpectedProtocol(#[allow(dead_code)] String),
    UnresolvablePath(#[allow(dead_code)] io::Error),
}

impl From<Utf8Error> for DndDataParseError {
    fn from(e: Utf8Error) -> Self {
        DndDataParseError::InvalidUtf8(e)
    }
}

impl From<io::Error> for DndDataParseError {
    fn from(e: io::Error) -> Self {
        DndDataParseError::UnresolvablePath(e)
    }
}

pub struct Dnd {
    xconn: Arc<XConnection>,
    // Populated by XdndEnter event handler
    pub version: Option<c_long>,
    pub type_list: Option<Vec<xproto::Atom>>,
    // Populated by XdndPosition event handler
    pub source_window: Option<xproto::Window>,
    // Populated by SelectionNotify event handler (triggered by XdndPosition event handler)
    pub result: Option<Result<Vec<PathBuf>, DndDataParseError>>,
    /// zgx patch: a drag carrying text, not files (`platform::text_drop`).
    pub text: Option<TextDrag>,
    /// zgx patch: where a drag carrying files was last said to be
    /// (`CursorMoved`), in the window's pixels: the source holds the
    /// pointer, so the window hears no motion while it is over it.
    pub file_point: Option<(f64, f64)>,
}

/// zgx patch: a drag carrying text over a window, as `platform::text_drop`
/// hears it.
#[derive(Debug, Default)]
pub struct TextDrag {
    /// Where it last was, in the window's pixels.
    pub point: (f64, f64),
    /// Whether the window would take it there (the last `XdndStatus`).
    pub accepted: bool,
    /// Its text was asked for (`XdndSelection` converted).
    pub asked: bool,
    /// Its text, once it arrived (`None` inside: none could be had).
    pub text: Option<Option<String>>,
    /// Let go of before its text arrived: the source to finish with then.
    pub dropped: Option<xproto::Window>,
}

impl Dnd {
    pub fn new(xconn: Arc<XConnection>) -> Result<Self, X11Error> {
        Ok(Dnd {
            xconn,
            version: None,
            type_list: None,
            source_window: None,
            result: None,
            text: None,
            file_point: None,
        })
    }

    pub fn reset(&mut self) {
        self.version = None;
        self.type_list = None;
        self.source_window = None;
        self.result = None;
        self.text = None;
        self.file_point = None;
    }

    /// zgx patch: the target a drag's text is asked for in, if it carries
    /// text and no files, most exact first.
    pub fn text_target(&self) -> Option<xproto::Atom> {
        let atoms = self.xconn.atoms();
        let types = self.type_list.as_ref()?;
        if types.contains(&atoms[TextUriList]) {
            return None;
        }
        [atoms[TextPlainUtf8], atoms[UTF8_STRING], atoms[TextPlain]]
            .into_iter()
            .find(|target| types.contains(target))
    }

    /// zgx patch: whether the window would take the text drag, as a copy.
    pub unsafe fn send_text_status(
        &self,
        this_window: xproto::Window,
        target_window: xproto::Window,
        accepted: bool,
    ) -> Result<(), X11Error> {
        let atoms = self.xconn.atoms();
        let action = if accepted { atoms[XdndActionCopy] } else { atoms[DndNone] };
        self.xconn
            .send_client_msg(target_window, target_window, atoms[XdndStatus] as _, None, [
                this_window,
                accepted as u32,
                0,
                0,
                action as _,
            ])?
            .ignore_error();
        Ok(())
    }

    /// zgx patch: the text drag is over: whether its text was taken.
    pub unsafe fn send_text_finished(
        &self,
        this_window: xproto::Window,
        target_window: xproto::Window,
        taken: bool,
    ) -> Result<(), X11Error> {
        let atoms = self.xconn.atoms();
        let action = if taken { atoms[XdndActionCopy] } else { atoms[DndNone] };
        self.xconn
            .send_client_msg(target_window, target_window, atoms[XdndFinished] as _, None, [
                this_window,
                taken as u32,
                action as _,
                0,
                0,
            ])?
            .ignore_error();
        Ok(())
    }

    /// zgx patch: ask the source for the drag's text in `target` (it
    /// arrives as `SelectionNotify`).
    pub unsafe fn convert_text(
        &self,
        window: xproto::Window,
        target: xproto::Atom,
        time: xproto::Timestamp,
    ) {
        let atoms = self.xconn.atoms();
        self.xconn
            .xcb_connection()
            .convert_selection(window, atoms[XdndSelection], target, atoms[XdndSelection], time)
            .expect_then_ignore_error("Failed to send XdndSelection event")
    }

    /// zgx patch: the drag's text, as the source put it on `window`, of
    /// whatever type it names it (GTK the target's, others `UTF8_STRING`).
    pub unsafe fn read_text(&self, window: xproto::Window) -> Option<String> {
        let atoms = self.xconn.atoms();
        let conn = self.xconn.xcb_connection();
        let mut data = Vec::new();
        let mut offset = 0;
        loop {
            let reply = conn
                .get_property(
                    true,
                    window,
                    atoms[XdndSelection],
                    xproto::AtomEnum::ANY,
                    offset,
                    1 << 20,
                )
                .ok()?
                .reply()
                .ok()?;
            if reply.format != 8 {
                return None;
            }
            data.extend_from_slice(&reply.value);
            if reply.bytes_after == 0 {
                break;
            }
            offset += (reply.value.len() / 4) as u32;
        }
        Some(String::from_utf8_lossy(&data).into_owned())
    }

    pub unsafe fn send_status(
        &self,
        this_window: xproto::Window,
        target_window: xproto::Window,
        state: DndState,
    ) -> Result<(), X11Error> {
        let atoms = self.xconn.atoms();
        let (accepted, action) = match state {
            DndState::Accepted => (1, atoms[XdndActionPrivate]),
            DndState::Rejected => (0, atoms[DndNone]),
        };
        self.xconn
            .send_client_msg(target_window, target_window, atoms[XdndStatus] as _, None, [
                this_window,
                accepted,
                0,
                0,
                action as _,
            ])?
            .ignore_error();

        Ok(())
    }

    pub unsafe fn send_finished(
        &self,
        this_window: xproto::Window,
        target_window: xproto::Window,
        state: DndState,
    ) -> Result<(), X11Error> {
        let atoms = self.xconn.atoms();
        let (accepted, action) = match state {
            DndState::Accepted => (1, atoms[XdndActionPrivate]),
            DndState::Rejected => (0, atoms[DndNone]),
        };
        self.xconn
            .send_client_msg(target_window, target_window, atoms[XdndFinished] as _, None, [
                this_window,
                accepted,
                action as _,
                0,
                0,
            ])?
            .ignore_error();

        Ok(())
    }

    pub unsafe fn get_type_list(
        &self,
        source_window: xproto::Window,
    ) -> Result<Vec<xproto::Atom>, util::GetPropertyError> {
        let atoms = self.xconn.atoms();
        self.xconn.get_property(
            source_window,
            atoms[XdndTypeList],
            xproto::Atom::from(xproto::AtomEnum::ATOM),
        )
    }

    pub unsafe fn convert_selection(&self, window: xproto::Window, time: xproto::Timestamp) {
        let atoms = self.xconn.atoms();
        self.xconn
            .xcb_connection()
            .convert_selection(
                window,
                atoms[XdndSelection],
                atoms[TextUriList],
                atoms[XdndSelection],
                time,
            )
            .expect_then_ignore_error("Failed to send XdndSelection event")
    }

    pub unsafe fn read_data(
        &self,
        window: xproto::Window,
    ) -> Result<Vec<c_uchar>, util::GetPropertyError> {
        let atoms = self.xconn.atoms();
        self.xconn.get_property(window, atoms[XdndSelection], atoms[TextUriList])
    }

    pub fn parse_data(&self, data: &mut [c_uchar]) -> Result<Vec<PathBuf>, DndDataParseError> {
        if !data.is_empty() {
            let mut path_list = Vec::new();
            let decoded = percent_decode(data).decode_utf8()?.into_owned();
            for uri in decoded.split("\r\n").filter(|u| !u.is_empty()) {
                // The format is specified as protocol://host/path
                // However, it's typically simply protocol:///path
                let path_str = if uri.starts_with("file://") {
                    let path_str = uri.replace("file://", "");
                    if !path_str.starts_with('/') {
                        // A hostname is specified
                        // Supporting this case is beyond the scope of my mental health
                        return Err(DndDataParseError::HostnameSpecified(path_str));
                    }
                    path_str
                } else {
                    // Only the file protocol is supported
                    return Err(DndDataParseError::UnexpectedProtocol(uri.to_owned()));
                };

                let path = Path::new(&path_str).canonicalize()?;
                path_list.push(path);
            }
            Ok(path_list)
        } else {
            Err(DndDataParseError::EmptyData)
        }
    }
}
