//! Linux file chooser requests with explicit owner cancellation and parent lifetime.
use crate::{
    FileDialogError, FileDialogOptions, FileDialogResult, file_dialog::Kind, portal_parent::Parent,
};
use dbus::{
    arg::{PropMap, RefArg, Variant},
    message::MatchRule,
};
use std::{
    io::Read,
    os::unix::ffi::{OsStrExt, OsStringExt},
    path::PathBuf,
    sync::{Arc, Mutex},
    time::Duration,
};
use winit::window::Window;
const SERVICE: &str = "org.freedesktop.portal.Desktop";
fn unavailable(_: impl std::fmt::Display) -> FileDialogError {
    FileDialogError::BackendUnavailable
}
fn option<T: RefArg + 'static>(map: &mut PropMap, key: &str, value: T) {
    map.insert(key.into(), Variant(Box::new(value)));
}
pub(crate) fn select(
    kind: Kind,
    options: FileDialogOptions,
    parent: Arc<Window>,
    cancel: Arc<crate::file_dialog::Cancel>,
) -> FileDialogResult<Vec<PathBuf>> {
    if cancel.is_cancelled() {
        return Err(FileDialogError::WindowClosed);
    }
    // This guard must be destroyed BEFORE the Arc parent, including every error return.
    let exported = Parent::new(&parent).ok_or(FileDialogError::BackendUnavailable)?;
    let connection = crate::linux_dbus::session().map_err(unavailable)?;
    let mut random = [0u8; 16];
    std::fs::File::open("/dev/urandom")
        .and_then(|mut f| f.read_exact(&mut random))
        .map_err(unavailable)?;
    let token = format!(
        "zgui_{}",
        random
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect::<String>()
    );
    let sender = connection
        .unique_name()
        .trim_start_matches(':')
        .replace('.', "_");
    let expected = format!("/org/freedesktop/portal/desktop/request/{sender}/{token}");
    // Subscribe before invoking FileChooser to avoid the fast-response race.
    let responses = Arc::new(Mutex::new(Vec::<(String, u32, PropMap)>::new()));
    let received = responses.clone();
    let mut rule = MatchRule::new_signal("org.freedesktop.portal.Request", "Response");
    rule.sender = Some(SERVICE.into());
    connection
        .add_match(rule, move |(code, values): (u32, PropMap), _, message| {
            if let Some(path) = message.path() {
                received
                    .lock()
                    .unwrap()
                    .push((path.to_string(), code, values));
            }
            true
        })
        .map_err(unavailable)?;
    let mut values = PropMap::new();
    option(&mut values, "handle_token", token);
    option(&mut values, "modal", true);
    option(
        &mut values,
        "multiple",
        matches!(kind, Kind::OpenFiles | Kind::Folders),
    );
    option(
        &mut values,
        "directory",
        matches!(kind, Kind::Folder | Kind::Folders),
    );
    if let Some(directory) = options.directory {
        let mut bytes = directory.as_os_str().as_bytes().to_vec();
        bytes.push(0);
        option(&mut values, "current_folder", bytes);
    }
    if let Some(name) = options.file_name {
        option(&mut values, "current_name", name);
    }
    if !options.filters.is_empty() {
        let filters: Vec<(String, Vec<(u32, String)>)> = options
            .filters
            .into_iter()
            .map(|f| {
                (
                    f.name,
                    f.extensions
                        .into_iter()
                        .map(|e| {
                            (
                                0,
                                if e == "*" {
                                    "*".into()
                                } else {
                                    format!("*.{e}")
                                },
                            )
                        })
                        .collect(),
                )
            })
            .collect();
        option(&mut values, "filters", filters);
    }
    if cancel.is_cancelled() {
        return Err(FileDialogError::WindowClosed);
    }
    let proxy = connection.with_proxy(
        SERVICE,
        "/org/freedesktop/portal/desktop",
        Duration::from_secs(3),
    );
    let (path,): (dbus::Path<'static>,) = proxy
        .method_call(
            "org.freedesktop.portal.FileChooser",
            if matches!(kind, Kind::Save) {
                "SaveFile"
            } else {
                "OpenFile"
            },
            (exported.to_string(), options.title, values),
        )
        .map_err(unavailable)?;
    // Older portals may return a different path. We subscribed to all Response
    // paths on this owned connection, so either response can already be queued.
    let _expected_path = expected;
    loop {
        if cancel.is_cancelled() {
            let request = connection.with_proxy(SERVICE, path.clone(), Duration::from_millis(500));
            let _: Result<(), _> =
                request.method_call("org.freedesktop.portal.Request", "Close", ());
            // Closing this dedicated bus connection additionally abandons the request.
            return Err(FileDialogError::WindowClosed);
        }
        let response = {
            let mut pending = responses.lock().unwrap();
            pending
                .iter()
                .position(|(p, _, _)| p.as_str() == &*path)
                .map(|index| pending.swap_remove(index))
        };
        if let Some((_, code, values)) = response {
            return match code {
                1 => Ok(None),
                0 => {
                    let uris = values
                        .get("uris")
                        .and_then(|v| v.0.as_iter())
                        .ok_or(FileDialogError::BackendUnavailable)?;
                    let paths = uris
                        .map(|v| v.as_str().and_then(file_uri))
                        .collect::<Option<Vec<_>>>()
                        .ok_or(FileDialogError::BackendUnavailable)?;
                    Ok(Some(paths))
                }
                _ => Err(FileDialogError::BackendUnavailable),
            };
        }
        // Asleep until the portal answers or the request is cancelled.
        match cancel.wake() {
            Some(wake) => {
                if !crate::linux_dbus::wait(&connection, wake) && !cancel.is_cancelled() {
                    return Err(FileDialogError::BackendUnavailable);
                }
            }
            None => {
                connection
                    .process(Duration::from_millis(30))
                    .map_err(unavailable)?;
            }
        }
    }
}
pub(crate) fn file_uri(uri: &str) -> Option<PathBuf> {
    let raw = uri.strip_prefix("file://")?;
    let raw = raw
        .strip_prefix("localhost/")
        .map(|s| format!("/{s}"))
        .unwrap_or_else(|| raw.to_owned());
    if !raw.starts_with('/') {
        return None;
    }
    let mut bytes = Vec::new();
    let mut input = raw.bytes();
    while let Some(b) = input.next() {
        if b == b'%' {
            let a = (input.next()? as char).to_digit(16)?;
            let b = (input.next()? as char).to_digit(16)?;
            bytes.push((a * 16 + b) as u8)
        } else {
            bytes.push(b)
        }
    }
    if bytes.contains(&0) {
        return None;
    }
    Some(std::ffi::OsString::from_vec(bytes).into())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn file_uris_preserve_unix_bytes_and_reject_remote_paths() {
        assert_eq!(
            file_uri("file:///tmp/a%20b"),
            Some(PathBuf::from("/tmp/a b"))
        );
        assert_eq!(
            file_uri("file://localhost/tmp/a"),
            Some(PathBuf::from("/tmp/a"))
        );
        assert!(file_uri("file://remote/tmp/a").is_none());
        assert!(file_uri("file:///a%00b").is_none());
        assert!(file_uri("file:///a%xx").is_none());
    }
}
