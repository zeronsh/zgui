//! A window's zoom kept with its frame (`WindowOptions::frame_autosave`),
//! so it opens at the zoom it was left at, as a browser keeps a site's: a
//! line per name in `zooms` beside the app's other state
//! (`~/Library/Application Support/zgx/<executable>` on macOS,
//! `%LOCALAPPDATA%\zgx\<executable>` on Windows,
//! `$XDG_STATE_HOME/zgx/<executable>` elsewhere). A window at 100% has no
//! line. Read as the window opens and written as its zoom changes, nothing
//! else.
use std::path::{Path, PathBuf};

/// Where zooms are kept.
fn path() -> Option<PathBuf> {
    let var = |name: &str| {
        std::env::var_os(name)
            .map(PathBuf::from)
            .filter(|path| path.is_absolute())
    };
    let base = if cfg!(target_os = "macos") {
        var("HOME")?.join("Library/Application Support")
    } else if cfg!(windows) {
        var("LOCALAPPDATA")?
    } else {
        var("XDG_STATE_HOME").or_else(|| Some(var("HOME")?.join(".local/state")))?
    };
    let app = std::env::current_exe()
        .ok()
        .and_then(|path| Some(path.file_stem()?.to_string_lossy().into_owned()))
        .unwrap_or_else(|| "zgx".into());
    Some(base.join("zgx").join(app).join("zooms"))
}

/// A name as a line keeps it: one line, the tab its separator.
fn key(name: &str) -> String {
    name.replace(['\t', '\n', '\r'], " ")
}

/// The zoom window `name` was left at, if not 100%.
pub(crate) fn load(name: &str) -> Option<f64> {
    load_from(&path()?, name)
}

/// Keep `zoom` for window `name` (100%: forget it).
pub(crate) fn save(name: &str, zoom: f64) {
    if let Some(path) = path() {
        let _ = store_to(&path, name, zoom);
    }
}

fn load_from(path: &Path, name: &str) -> Option<f64> {
    let name = key(name);
    std::fs::read_to_string(path)
        .ok()?
        .lines()
        .filter_map(|line| line.split_once('\t'))
        .find(|(each, _)| *each == name)
        .and_then(|(_, zoom)| zoom.trim().parse::<f64>().ok())
        .filter(|zoom| zoom.is_finite() && *zoom > 0.)
}

fn store_to(path: &Path, name: &str, zoom: f64) -> std::io::Result<()> {
    let name = key(name);
    let old = std::fs::read_to_string(path).unwrap_or_default();
    let mut text: String = old
        .lines()
        .filter(|line| line.split_once('\t').is_some_and(|(each, _)| each != name))
        .flat_map(|line| [line, "\n"])
        .collect();
    if zoom != 1. {
        text.push_str(&format!("{name}\t{zoom}\n"));
    }
    if text == old {
        return Ok(());
    }
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    // A whole file or the old one, never half: written beside it, then
    // renamed over it.
    let temporary = path.with_extension("tmp");
    std::fs::write(&temporary, text)?;
    std::fs::rename(&temporary, path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_windows_zoom_is_kept_by_its_name_and_100_percent_forgets_it() {
        let dir = std::env::temp_dir().join(format!("zgx-zooms-{}", std::process::id()));
        let path = dir.join("zooms");
        let _ = std::fs::remove_dir_all(&dir);
        assert_eq!(load_from(&path, "main"), None, "nothing kept");
        store_to(&path, "main", 1.25).unwrap();
        store_to(&path, "inspector\twith a tab", 0.9).unwrap();
        assert_eq!(load_from(&path, "main"), Some(1.25));
        assert_eq!(load_from(&path, "inspector\twith a tab"), Some(0.9));
        store_to(&path, "main", 1.5).unwrap();
        assert_eq!(load_from(&path, "main"), Some(1.5), "the last one");
        store_to(&path, "main", 1.).unwrap();
        assert_eq!(load_from(&path, "main"), None, "100%: no line");
        assert_eq!(
            load_from(&path, "inspector\twith a tab"),
            Some(0.9),
            "others stay"
        );
        std::fs::write(&path, "main\tnot a number\nother\t-2\n").unwrap();
        assert_eq!(load_from(&path, "main"), None);
        assert_eq!(load_from(&path, "other"), None);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
