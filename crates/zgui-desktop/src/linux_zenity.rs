//! A file chooser where no portal answers (a window manager alone:
//! Openbox, i3, a sway without xdg-desktop-portal): zenity's, which is
//! GTK's, as rfd and Chromium fall back to GTK's own. It is modal to its
//! window (attached to it on X11 with zenity 3), and the request's cancel
//! (its future dropped, its window closed) ends it; meanwhile the waiting
//! thread sleeps on the process and the cancel together.
use crate::{FileDialogError, FileDialogOptions, FileDialogResult, file_dialog::Kind};
use std::{
    io::Read,
    os::fd::{AsRawFd, FromRawFd, OwnedFd},
    path::PathBuf,
    process::{Command, Stdio},
};
use winit::{
    raw_window_handle::{HasWindowHandle, RawWindowHandle},
    window::Window,
};

/// Between the paths of a multiple selection: no path holds it.
const SEPARATOR: char = '\u{1f}';

/// Whether zenity is on the `PATH`.
pub(crate) fn available() -> bool {
    std::env::var_os("PATH")
        .is_some_and(|path| std::env::split_paths(&path).any(|dir| dir.join("zenity").is_file()))
}

/// zenity's arguments for a `kind` request with `options`, attached to the
/// X11 window `xid`.
pub(crate) fn arguments(kind: Kind, options: &FileDialogOptions, xid: Option<u64>) -> Vec<String> {
    let mut args = vec!["--file-selection".to_owned(), "--modal".to_owned()];
    if !options.title.is_empty() {
        args.push(format!("--title={}", options.title));
    }
    match kind {
        Kind::OpenFile => {}
        Kind::OpenFiles => args.push("--multiple".into()),
        Kind::Folder => args.push("--directory".into()),
        Kind::Folders => args.extend(["--directory".into(), "--multiple".into()]),
        Kind::Save => args.extend(["--save".into(), "--confirm-overwrite".into()]),
    }
    if matches!(kind, Kind::OpenFiles | Kind::Folders) {
        args.push(format!("--separator={SEPARATOR}"));
    }
    // A folder to start in ends with a slash; a file name goes in it.
    let start = match (&options.directory, &options.file_name) {
        (Some(dir), Some(name)) => Some(dir.join(name).display().to_string()),
        (Some(dir), None) => Some(format!("{}/", dir.display())),
        (None, Some(name)) => Some(name.clone()),
        (None, None) => None,
    };
    if let Some(start) = start {
        args.push(format!("--filename={start}"));
    }
    if !matches!(kind, Kind::Folder | Kind::Folders) {
        for filter in &options.filters {
            let patterns: Vec<String> = filter
                .extensions
                .iter()
                .map(|e| {
                    if e == "*" {
                        "*".into()
                    } else {
                        format!("*.{e}")
                    }
                })
                .collect();
            args.push(format!(
                "--file-filter={} | {}",
                filter.name,
                patterns.join(" ")
            ));
        }
    }
    if let Some(xid) = xid {
        args.push(format!("--attach={xid}"));
    }
    args
}

/// What zenity printed: one path, or several by `SEPARATOR`.
pub(crate) fn paths(output: &str) -> Vec<PathBuf> {
    output
        .trim_end_matches('\n')
        .split(SEPARATOR)
        .filter(|path| !path.is_empty())
        .map(PathBuf::from)
        .collect()
}

/// Show zenity's chooser for `parent` and wait for it, or for `cancel`.
pub(crate) fn select(
    kind: Kind,
    options: &FileDialogOptions,
    parent: &Window,
    cancel: &crate::file_dialog::Cancel,
) -> FileDialogResult<Vec<PathBuf>> {
    let xid = parent.window_handle().ok().and_then(|h| match h.as_raw() {
        RawWindowHandle::Xlib(handle) => Some(handle.window),
        RawWindowHandle::Xcb(handle) => Some(handle.window.get() as u64),
        _ => None,
    });
    let mut child = Command::new("zenity")
        .args(arguments(kind, options, xid))
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|_| FileDialogError::BackendUnavailable)?;
    // Read as it writes: a long selection fills the pipe before it exits.
    let mut stdout = child
        .stdout
        .take()
        .ok_or(FileDialogError::BackendUnavailable)?;
    let reader = std::thread::spawn(move || {
        let mut output = String::new();
        let _ = stdout.read_to_string(&mut output);
        output
    });
    // SAFETY: a descriptor of ours for the child we spawned, closed with it.
    let process = unsafe { libc::syscall(libc::SYS_pidfd_open, child.id(), 0) };
    if process >= 0 {
        let process = unsafe { OwnedFd::from_raw_fd(process as i32) };
        let wake = cancel.wake().map_or(-1, |wake| wake.as_raw_fd());
        loop {
            let mut fds = [
                libc::pollfd {
                    fd: process.as_raw_fd(),
                    events: libc::POLLIN,
                    revents: 0,
                },
                libc::pollfd {
                    fd: wake,
                    events: libc::POLLIN,
                    revents: 0,
                },
            ];
            let n = unsafe { libc::poll(fds.as_mut_ptr(), 2, -1) };
            if fds[1].revents != 0 || cancel.is_cancelled() {
                let _ = child.kill();
                let _ = child.wait();
                return Err(FileDialogError::WindowClosed);
            }
            if fds[0].revents != 0
                || (n < 0
                    && std::io::Error::last_os_error().kind() != std::io::ErrorKind::Interrupted)
            {
                break;
            }
        }
    }
    let status = child
        .wait()
        .map_err(|_| FileDialogError::BackendUnavailable)?;
    let output = reader.join().unwrap_or_default();
    match status.code() {
        Some(0) => Ok(Some(paths(&output))),
        // Cancel, or the window closed.
        Some(1) => Ok(None),
        _ => Err(FileDialogError::BackendUnavailable),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::FileFilter;

    #[test]
    fn requests_become_zenity_arguments() {
        let options = FileDialogOptions {
            title: "Open fixture".into(),
            directory: Some("/tmp/docs".into()),
            file_name: None,
            filters: vec![FileFilter {
                name: "Text".into(),
                extensions: vec!["txt".into(), "md".into()],
            }],
        };
        assert_eq!(
            arguments(Kind::OpenFiles, &options, Some(42)),
            [
                "--file-selection",
                "--modal",
                "--title=Open fixture",
                "--multiple",
                "--separator=\u{1f}",
                "--filename=/tmp/docs/",
                "--file-filter=Text | *.txt *.md",
                "--attach=42",
            ]
        );
        let save = FileDialogOptions {
            file_name: Some("notes.txt".into()),
            ..options.clone()
        };
        let args = arguments(Kind::Save, &save, None);
        assert!(args.contains(&"--save".to_owned()));
        assert!(args.contains(&"--filename=/tmp/docs/notes.txt".to_owned()));
        let folders = arguments(Kind::Folder, &options, None);
        assert!(folders.contains(&"--directory".to_owned()));
        assert!(!folders.iter().any(|a| a.starts_with("--file-filter")));
    }

    #[test]
    fn zenitys_output_reads_as_paths() {
        assert_eq!(paths("/a b/c.txt\n"), [PathBuf::from("/a b/c.txt")]);
        assert_eq!(
            paths("/x\u{1f}/y z\n"),
            [PathBuf::from("/x"), PathBuf::from("/y z")]
        );
        assert!(paths("\n").is_empty());
    }
}
