//! A window's icon on X11 (`_NET_WM_ICON`, which taskbars, docks and window
//! switchers show): the app's icon from the icon theme, by its application
//! id or else its executable's name, as GTK takes a window's default icon
//! from the program's name. Desktops that match windows to installed
//! desktop files by their class find the same icon; this one also shows for
//! an app with none installed, and where a panel reads the window's own.
//! Wayland has no window icon: the compositor takes the desktop file's.
//!
//! The theme is hicolor, every theme's fallback (where apps install their
//! icons), under `$XDG_DATA_HOME` and `$XDG_DATA_DIRS`: a 64 pixel PNG
//! first, then other sizes, then the scalable SVG drawn at 64, then
//! `pixmaps`. It is looked up and decoded once, as the first window opens.
use std::path::{Path, PathBuf};
use winit::window::{Icon, Window};

/// Sizes looked for, in order: the one panels draw from, then up, then
/// down.
const SIZES: [u32; 7] = [64, 48, 96, 128, 256, 32, 24];

/// Give `window` (X11) the app's icon, named `name` (its application id) or
/// else its executable's.
pub(crate) fn apply(window: &Window, name: Option<&str>) {
    thread_local! {
        static ICON: std::cell::OnceCell<Option<Icon>> = const { std::cell::OnceCell::new() };
    }
    let icon = ICON.with(|icon| {
        icon.get_or_init(|| {
            let name = name.map(str::to_owned).or_else(executable)?;
            let rgba = find(&name, &data_dirs())?;
            Icon::from_rgba(rgba.2, rgba.0, rgba.1).ok()
        })
        .clone()
    });
    if icon.is_some() {
        window.set_window_icon(icon);
    }
}

fn executable() -> Option<String> {
    let path = std::env::current_exe().ok()?;
    Some(path.file_stem()?.to_string_lossy().into_owned())
}

/// `$XDG_DATA_HOME` then `$XDG_DATA_DIRS`, with the spec's defaults.
fn data_dirs() -> Vec<PathBuf> {
    let home = std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .filter(|path| path.is_absolute())
        .or_else(|| Some(PathBuf::from(std::env::var_os("HOME")?).join(".local/share")));
    let dirs = std::env::var("XDG_DATA_DIRS")
        .ok()
        .filter(|dirs| !dirs.is_empty())
        .unwrap_or_else(|| "/usr/local/share:/usr/share".into());
    home.into_iter()
        .chain(
            dirs.split(':')
                .map(PathBuf::from)
                .filter(|path| path.is_absolute()),
        )
        .collect()
}

/// Icon `name` from the data directories `dirs`: width, height and straight
/// RGBA pixels.
fn find(name: &str, dirs: &[PathBuf]) -> Option<(u32, u32, Vec<u8>)> {
    // An icon name, not a path.
    if name.is_empty() || name.contains('/') {
        return None;
    }
    let decode = |path: &Path| -> Option<(u32, u32, Vec<u8>)> {
        let bytes = std::fs::read(path).ok()?;
        let image = if path.extension().is_some_and(|ext| ext == "svg") {
            zgui_gpu::assets::decode_svg(&bytes, SIZES[0], SIZES[0]).ok()?
        } else {
            zgui_gpu::assets::decode_image(&bytes).ok()?
        };
        Some((image.width(), image.height(), image.pixels().to_vec()))
    };
    let hicolor = |dir: &PathBuf| dir.join("icons/hicolor");
    for size in SIZES {
        for dir in dirs {
            let path = hicolor(dir).join(format!("{size}x{size}/apps/{name}.png"));
            if let Some(icon) = path.is_file().then(|| decode(&path)).flatten() {
                return Some(icon);
            }
        }
    }
    dirs.iter()
        .map(|dir| hicolor(dir).join(format!("scalable/apps/{name}.svg")))
        .chain(
            dirs.iter().flat_map(|dir| {
                ["png", "svg"].map(|ext| dir.join(format!("pixmaps/{name}.{ext}")))
            }),
        )
        .find(|path| path.is_file())
        .and_then(|path| decode(&path))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A `size` pixel square PNG.
    fn png(size: u32) -> Vec<u8> {
        let mut bytes = Vec::new();
        image::RgbaImage::from_pixel(size, size, image::Rgba([10, 20, 30, 255]))
            .write_to(
                &mut std::io::Cursor::new(&mut bytes),
                image::ImageFormat::Png,
            )
            .unwrap();
        bytes
    }

    #[test]
    fn an_icon_is_found_by_name_its_best_size_first() {
        let root = std::env::temp_dir().join(format!("zgui-icons-{}", std::process::id()));
        let (home, system) = (root.join("home"), root.join("system"));
        let put = |dir: &Path, relative: &str, size: u32| {
            let path = dir.join(relative);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, png(size)).unwrap();
        };
        put(&system, "icons/hicolor/48x48/apps/dev.zgui.Probe.png", 48);
        put(&system, "icons/hicolor/32x32/apps/dev.zgui.Probe.png", 32);
        put(&home, "pixmaps/plain.png", 16);
        let dirs = [home.clone(), system.clone()];
        // 48 comes before 32 (64 there is none of).
        let found = find("dev.zgui.Probe", &dirs).unwrap();
        assert_eq!((found.0, found.1, found.2.len()), (48, 48, 48 * 48 * 4));
        // Pixmaps last.
        assert_eq!(find("plain", &dirs).map(|icon| icon.0), Some(16));
        // A 64 pixel one in the user's own directory comes first.
        put(&home, "icons/hicolor/64x64/apps/dev.zgui.Probe.png", 64);
        assert_eq!(find("dev.zgui.Probe", &dirs).map(|icon| icon.0), Some(64));
        assert_eq!(find("missing", &dirs), None);
        assert_eq!(find("../plain", &dirs), None, "a name, not a path");
        std::fs::remove_dir_all(root).unwrap();
    }
}
