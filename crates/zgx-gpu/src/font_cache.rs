//! The system's fonts without reading every font file at every launch.
//! Loading them opens and parses each installed file (about 370 files, 10 ms
//! and 100 M instructions on macOS), and the first text in each new style
//! read every face's weight axis from its file as well. Both are kept in a
//! cache file, used while every font directory is unchanged (their
//! modification times and sizes, as fontconfig checks its cache): adding,
//! removing or renaming a font changes its directory. The font files
//! themselves (about 370 stats, a fifth of the cache's cost) are checked
//! after launch, off the main thread: one changed in place drops the cache
//! for the next launch. Linux, where fontdb takes its directories from
//! fontconfig, loads as before.
use cosmic_text::{
    FontSystem,
    fontdb::{self, Language, Source, Stretch, Style, Weight},
};
use std::{
    path::{Path, PathBuf},
    time::UNIX_EPOCH,
};

/// Bump when fontdb or cosmic-text change what they read from a font file.
const FORMAT: &[u8] = b"zgx system fonts 2\n";

pub(crate) fn system_fonts() -> FontSystem {
    let mut fonts = cached_system_fonts();
    platform_families(&mut fonts);
    #[cfg(feature = "hyphens")]
    fonts.set_hyphenator(Some(std::sync::Arc::new(hyphenation_points)));
    fonts
}

/// Where `word` may break with a hyphen, in `locale`'s language (English
/// for one hypher has no patterns for): byte offsets inside it.
#[cfg(feature = "hyphens")]
fn hyphenation_points(word: &str, locale: &str) -> Vec<usize> {
    let code = locale.as_bytes();
    let lang = (code.len() >= 2)
        .then(|| {
            hypher::Lang::from_iso([code[0].to_ascii_lowercase(), code[1].to_ascii_lowercase()])
        })
        .flatten()
        .unwrap_or(hypher::Lang::English);
    let mut at = 0;
    let mut points = Vec::new();
    for syllable in hypher::hyphenate(word, lang) {
        at += syllable.len();
        if at < word.len() {
            points.push(at);
        }
    }
    points
}

/// The system's own faces for the generic families. cosmic-text names
/// Linux's (Open Sans, Noto Sans Mono, DejaVu Serif), which Windows has
/// none of, so sans-serif text took the first face of the fallback list and
/// monospace any fixed-width face. Windows 11's interface face is Segoe UI
/// Variable, whose optical size axis zgx sets from the text size as
/// Windows does (its Small, Text and Display cuts).
fn platform_families(fonts: &mut FontSystem) {
    #[cfg(windows)]
    {
        let db = fonts.db_mut();
        let pick = |names: &[&'static str]| {
            names.iter().copied().find(|name| {
                db.faces()
                    .any(|face| face.families.iter().any(|(family, _)| family == name))
            })
        };
        let sans = pick(&["Segoe UI Variable", "Segoe UI"]);
        let mono = pick(&["Cascadia Mono", "Consolas"]);
        let serif = pick(&["Times New Roman"]);
        if let Some(sans) = sans {
            db.set_sans_serif_family(sans);
        }
        if let Some(mono) = mono {
            db.set_monospace_family(mono);
        }
        if let Some(serif) = serif {
            db.set_serif_family(serif);
        }
    }
    // Nor does macOS have Noto Sans Mono. With no face of its own,
    // monospaced text searched every fixed-width face for each word it
    // shaped anew (a terminal's every line), to settle on Menlo for Latin
    // text. Menlo, on every Mac, is now the family's face; characters it
    // lacks fall back to other faces as on Windows and Linux.
    #[cfg(target_os = "macos")]
    {
        let db = fonts.db_mut();
        if db
            .faces()
            .any(|face| face.families.iter().any(|(family, _)| family == "Menlo"))
        {
            db.set_monospace_family("Menlo");
        }
    }
    // cosmic-text's monospace is Noto Sans Mono, which few Linux systems
    // install: monospaced text then took any fixed-width face, and on one
    // with DejaVu that was DejaVu Sans Mono Oblique, slanting every code
    // block. A family with an upright face is taken instead, a common one
    // first.
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        let db = fonts.db_mut();
        let upright = |name: &str| {
            db.faces().any(|face| {
                face.style == Style::Normal
                    && face.families.iter().any(|(family, _)| family == name)
            })
        };
        if !upright(db.family_name(&fontdb::Family::Monospace)) {
            let common = [
                "DejaVu Sans Mono",
                "Liberation Mono",
                "Ubuntu Mono",
                "Cascadia Mono",
                "Source Code Pro",
                "Hack",
                "Fira Mono",
                "Noto Mono",
            ];
            let mono = common
                .into_iter()
                .find(|name| upright(name))
                .map(str::to_owned)
                .or_else(|| {
                    db.faces()
                        .find(|face| face.monospaced && face.style == Style::Normal)
                        .and_then(|face| face.families.first())
                        .map(|(family, _)| family.clone())
                });
            if let Some(mono) = mono {
                db.set_monospace_family(mono);
            }
        }
    }
    // The desktop's interface face, as GTK and Qt apps take it, where the
    // host learned one; cosmic-text's Open Sans is seldom installed, and
    // its stand-in rarely the desktop's.
    #[cfg(not(windows))]
    if let Some(family) = DESKTOP_SANS.lock().unwrap().clone() {
        let db = fonts.db_mut();
        if db
            .faces()
            .any(|face| face.families.iter().any(|(name, _)| *name == family))
        {
            db.set_sans_serif_family(family);
        }
    }
}

#[cfg(not(windows))]
static DESKTOP_SANS: std::sync::Mutex<Option<String>> = std::sync::Mutex::new(None);

/// The desktop's interface face (Linux: GNOME's `font-name`, KDE's `font`),
/// for the generic sans-serif family (CSS's `sans-serif` and `system-ui`),
/// as GTK and Qt apps use it; a family the system lacks is passed over.
/// Fonts loaded from then on take it: the host sets it before its first
/// window. Elsewhere the platform's own face stands.
pub fn set_desktop_sans_serif(family: Option<String>) {
    #[cfg(not(windows))]
    {
        *DESKTOP_SANS.lock().unwrap() = family;
    }
    #[cfg(windows)]
    let _ = family;
}

fn cached_system_fonts() -> FontSystem {
    let Some(cache) = cache_file() else {
        return FontSystem::new();
    };
    if let Some((fonts, files)) = std::fs::read(&cache).ok().and_then(|bytes| decode(&bytes)) {
        check_later(cache, files);
        return fonts;
    }
    let mut fonts = FontSystem::new();
    if let Some(bytes) = encode(&mut fonts) {
        // Written whole and renamed, so another process never reads half.
        let partial = cache.with_extension(format!("{}.partial", std::process::id()));
        let written = cache
            .parent()
            .is_some_and(|dir| std::fs::create_dir_all(dir).is_ok())
            && std::fs::write(&partial, bytes).is_ok()
            && std::fs::rename(&partial, &cache).is_ok();
        if !written {
            let _ = std::fs::remove_file(&partial);
        }
    }
    fonts
}

/// Font files with the modification time and size each had when cached.
type Stamps = Vec<(PathBuf, (u64, u64))>;

/// Once the app has started, whether a font file changed in place since the
/// cache was written: if one did, the next launch loads the fonts again.
fn check_later(cache: PathBuf, files: Stamps) {
    let _ = std::thread::Builder::new()
        .name("zgx font cache check".into())
        .spawn(move || {
            std::thread::sleep(std::time::Duration::from_secs(2));
            if stale(&files) {
                let _ = std::fs::remove_file(cache);
            }
        });
}

/// Whether any of `files` is no longer as stamped.
fn stale(files: &[(PathBuf, (u64, u64))]) -> bool {
    files.iter().any(|(path, at)| stamp(path) != *at)
}

fn cache_file() -> Option<PathBuf> {
    if cfg!(target_os = "macos") {
        Some(PathBuf::from(std::env::var_os("HOME")?).join("Library/Caches/zgx/system-fonts"))
    } else if cfg!(windows) {
        Some(PathBuf::from(std::env::var_os("LOCALAPPDATA")?).join("zgx\\system-fonts"))
    } else {
        None
    }
}

/// The directories fontdb 0.23's `load_system_fonts` reads, and directories
/// whose contents decide that list without being read for fonts.
fn font_roots() -> (Vec<PathBuf>, Vec<PathBuf>) {
    let (mut roots, mut parents) = (Vec::new(), Vec::new());
    if cfg!(target_os = "macos") {
        roots.extend(["/Library/Fonts", "/System/Library/Fonts"].map(PathBuf::from));
        let assets = Path::new("/System/Library/AssetsV2");
        if let Ok(entries) = std::fs::read_dir(assets) {
            let mut fonts: Vec<_> = entries
                .flatten()
                .filter(|e| {
                    e.file_name()
                        .to_string_lossy()
                        .starts_with("com_apple_MobileAsset_Font")
                })
                .map(|e| e.path())
                .collect();
            fonts.sort();
            roots.extend(fonts);
        }
        parents.push(assets.to_owned());
        roots.push("/Network/Library/Fonts".into());
        if let Some(home) = std::env::var_os("HOME") {
            roots.push(Path::new(&home).join("Library/Fonts"));
        }
    } else if cfg!(windows) {
        let system = std::env::var_os("SYSTEMROOT").map_or("C:\\Windows".into(), PathBuf::from);
        roots.push(system.join("Fonts"));
        if let Some(home) = std::env::var_os("USERPROFILE") {
            let home = Path::new(&home);
            roots.push(home.join("AppData\\Local\\Microsoft\\Windows\\Fonts"));
            roots.push(home.join("AppData\\Roaming\\Microsoft\\Windows\\Fonts"));
        }
    }
    (roots, parents)
}

/// Every path whose change could change the loaded faces: the font
/// directories and each directory below them, then each font file.
fn watched(fonts: &FontSystem) -> Option<(Vec<PathBuf>, Vec<PathBuf>)> {
    let (roots, mut paths) = font_roots();
    let mut seen = std::collections::HashSet::new();
    let mut pending = roots;
    while let Some(dir) = pending.pop() {
        if let Ok(entries) = std::fs::read_dir(&dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                // As fontdb, through symbolic links, each directory once.
                if path.is_dir() && std::fs::canonicalize(&path).is_ok_and(|c| seen.insert(c)) {
                    pending.push(path);
                }
            }
        }
        paths.push(dir);
    }
    let mut files = Vec::new();
    for face in fonts.db().faces() {
        let Source::File(path) = &face.source else {
            return None;
        };
        files.push(path.clone());
    }
    for list in [&mut paths, &mut files] {
        list.sort();
        list.dedup();
    }
    Some((paths, files))
}

/// A path's modification time and size, or zeros where it does not exist.
fn stamp(path: &Path) -> (u64, u64) {
    std::fs::metadata(path).map_or((0, 0), |meta| {
        let modified = meta
            .modified()
            .ok()
            .and_then(|t| t.duration_since(UNIX_EPOCH).ok());
        (modified.map_or(1, |d| d.as_nanos() as u64), meta.len())
    })
}

fn encode(fonts: &mut FontSystem) -> Option<Vec<u8>> {
    let (dirs, files) = watched(fonts)?;
    let mut out = Writer(FORMAT.to_vec());
    for paths in [&dirs, &files] {
        out.u32(paths.len() as u32);
        for path in paths {
            out.str(path.to_str()?);
            let (modified, len) = stamp(path);
            out.u64(modified);
            out.u64(len);
        }
    }
    let ids: Vec<_> = fonts.db().faces().map(|face| face.id).collect();
    out.u32(ids.len() as u32);
    for id in ids {
        let axis = fonts.weight_axis(id);
        let face = fonts.db().face(id)?;
        let Source::File(path) = &face.source else {
            return None;
        };
        out.str(path.to_str()?);
        out.u32(face.index);
        out.u32(face.families.len() as u32);
        for (name, language) in &face.families {
            out.str(name);
            out.0
                .push(u8::from(*language == Language::English_UnitedStates));
        }
        out.str(&face.post_script_name);
        out.0.push(face.style as u8);
        out.u16(face.weight.0);
        out.u16(face.stretch.to_number());
        out.0.push(u8::from(face.monospaced));
        match axis {
            Some((min, max)) => {
                out.0.push(1);
                out.u32(min.to_bits());
                out.u32(max.to_bits());
            }
            None => out.0.push(0),
        }
    }
    Some(out.0)
}

/// The cached fonts, if every font directory is as it was, with the font
/// files' stamps to check later (`check_later`).
fn decode(bytes: &[u8]) -> Option<(FontSystem, Stamps)> {
    let mut input = Reader(bytes.strip_prefix(FORMAT)?);
    for _ in 0..input.u32()? {
        let path = input.str()?;
        if stamp(Path::new(path)) != (input.u64()?, input.u64()?) {
            return None;
        }
    }
    let files = (0..input.u32()?)
        .map(|_| Some((PathBuf::from(input.str()?), (input.u64()?, input.u64()?))))
        .collect::<Option<Vec<_>>>()?;
    let mut db = fontdb::Database::new();
    let mut axes = Vec::new();
    for _ in 0..input.u32()? {
        let source = Source::File(input.str()?.into());
        let index = input.u32()?;
        let families = (0..input.u32()?)
            .map(|_| {
                let name = input.str()?.to_owned();
                // Only whether a name is the US English one is kept: fontdb
                // lists it first and nothing here reads the others' languages.
                let english = input.u8()? == 1;
                Some((
                    name,
                    if english {
                        Language::English_UnitedStates
                    } else {
                        Language::Unknown
                    },
                ))
            })
            .collect::<Option<Vec<_>>>()?;
        let post_script_name = input.str()?.to_owned();
        let style = [Style::Normal, Style::Italic, Style::Oblique]
            .get(input.u8()? as usize)
            .copied()?;
        let weight = Weight(input.u16()?);
        let stretch = stretch(input.u16()?)?;
        let monospaced = input.u8()? == 1;
        let axis = match input.u8()? {
            1 => Some((f32::from_bits(input.u32()?), f32::from_bits(input.u32()?))),
            _ => None,
        };
        let id = db.push_face_info(fontdb::FaceInfo {
            id: fontdb::ID::dummy(),
            source,
            index,
            families,
            post_script_name,
            style,
            weight,
            stretch,
            monospaced,
        });
        axes.push((id, axis));
    }
    if !input.0.is_empty() {
        return None;
    }
    let mut fonts = FontSystem::new_with_db(db);
    for (id, axis) in axes {
        fonts.set_weight_axis(id, axis);
    }
    Some((fonts, files))
}

fn stretch(number: u16) -> Option<Stretch> {
    use Stretch::*;
    [
        UltraCondensed,
        ExtraCondensed,
        Condensed,
        SemiCondensed,
        Normal,
        SemiExpanded,
        Expanded,
        ExtraExpanded,
        UltraExpanded,
    ]
    .get(usize::from(number).checked_sub(1)?)
    .copied()
}

struct Writer(Vec<u8>);
impl Writer {
    fn u16(&mut self, v: u16) {
        self.0.extend_from_slice(&v.to_le_bytes());
    }
    fn u32(&mut self, v: u32) {
        self.0.extend_from_slice(&v.to_le_bytes());
    }
    fn u64(&mut self, v: u64) {
        self.0.extend_from_slice(&v.to_le_bytes());
    }
    fn str(&mut self, s: &str) {
        self.u32(s.len() as u32);
        self.0.extend_from_slice(s.as_bytes());
    }
}

struct Reader<'a>(&'a [u8]);
impl<'a> Reader<'a> {
    fn take(&mut self, n: usize) -> Option<&'a [u8]> {
        let (head, rest) = self.0.split_at_checked(n)?;
        self.0 = rest;
        Some(head)
    }
    fn u8(&mut self) -> Option<u8> {
        Some(self.take(1)?[0])
    }
    fn u16(&mut self) -> Option<u16> {
        Some(u16::from_le_bytes(self.take(2)?.try_into().ok()?))
    }
    fn u32(&mut self) -> Option<u32> {
        Some(u32::from_le_bytes(self.take(4)?.try_into().ok()?))
    }
    fn u64(&mut self) -> Option<u64> {
        Some(u64::from_le_bytes(self.take(8)?.try_into().ok()?))
    }
    fn str(&mut self) -> Option<&'a str> {
        let len = self.u32()? as usize;
        std::str::from_utf8(self.take(len)?).ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// With no font installed, text shapes to nothing, plain or not, rather
    /// than panicking (which left a window drawing nothing at all).
    #[test]
    fn text_shapes_to_nothing_with_no_font_installed() {
        use cosmic_text::{Attrs, Buffer, Metrics, Shaping};
        let mut fonts = FontSystem::new_with_locale_and_db("en-US".into(), fontdb::Database::new());
        for shaping in [Shaping::Basic, Shaping::Advanced] {
            let mut buffer = Buffer::new_empty(Metrics::new(14., 20.));
            buffer.set_text("Hello, wörld\nمرحبا", &Attrs::new(), shaping, None);
            buffer.shape_until_scroll(&mut fonts, false);
            let runs: Vec<_> = buffer.layout_runs().collect();
            assert!(!runs.is_empty());
            assert!(runs.iter().all(|run| run.glyphs.is_empty()));
        }
    }

    /// The desktop's face stands for sans-serif when it is installed, and
    /// is passed over when not.
    #[cfg(not(windows))]
    #[test]
    fn the_desktops_face_is_sans_serif() {
        let mut fonts = FontSystem::new();
        let default = fonts
            .db()
            .family_name(&fontdb::Family::SansSerif)
            .to_owned();
        let Some(installed) = fonts
            .db()
            .faces()
            .flat_map(|face| face.families.iter().map(|(name, _)| name.clone()))
            .find(|name| *name != default)
        else {
            return; // One family installed, or none.
        };
        set_desktop_sans_serif(Some("No Such Family zgx".into()));
        platform_families(&mut fonts);
        assert_eq!(fonts.db().family_name(&fontdb::Family::SansSerif), default);
        set_desktop_sans_serif(Some(installed.clone()));
        platform_families(&mut fonts);
        set_desktop_sans_serif(None);
        assert_eq!(
            fonts.db().family_name(&fontdb::Family::SansSerif),
            installed
        );
    }

    /// The cached database is the loaded one: the same faces, in the same
    /// order (matching breaks ties by it), with the same weight axes.
    #[test]
    fn a_decoded_cache_holds_the_loaded_faces() {
        let mut loaded = FontSystem::new();
        let Some(bytes) = encode(&mut loaded) else {
            return; // No system fonts cache on this platform.
        };
        let (mut cached, files) = decode(&bytes).expect("an unchanged system decodes");
        assert!(!stale(&files), "an unchanged font file is not stale");
        if let Some(first) = files.first() {
            let touched = [(first.0.clone(), (first.1.0 ^ 1, first.1.1))];
            assert!(stale(&touched), "a font file changed in place is");
        }
        let faces = |fonts: &FontSystem| -> Vec<_> {
            fonts
                .db()
                .faces()
                .map(|f| {
                    let families: Vec<_> =
                        f.families.iter().map(|(name, _)| name.clone()).collect();
                    let source = format!("{:?}", f.source);
                    (
                        source,
                        f.index,
                        families,
                        f.post_script_name.clone(),
                        f.style,
                        f.weight,
                        f.stretch,
                        f.monospaced,
                    )
                })
                .collect()
        };
        assert_eq!(faces(&loaded), faces(&cached));
        let ids: Vec<_> = loaded.db().faces().map(|f| f.id).collect();
        let cached_ids: Vec<_> = cached.db().faces().map(|f| f.id).collect();
        for (a, b) in ids.into_iter().zip(cached_ids) {
            assert_eq!(loaded.weight_axis(a), cached.weight_axis(b));
        }
        assert!(
            decode(&bytes[..bytes.len() - 1]).is_none(),
            "a truncated cache is not used"
        );
        // The first font directory's modification time, as if it changed
        // (Linux watches none: fontconfig has its own cache).
        let dirs = u32::from_le_bytes(bytes[FORMAT.len()..][..4].try_into().unwrap());
        if dirs > 0 {
            let mut changed = bytes.clone();
            let first = FORMAT.len() + 4;
            let len = u32::from_le_bytes(changed[first..first + 4].try_into().unwrap()) as usize;
            changed[first + 4 + len] ^= 1;
            assert!(decode(&changed).is_none(), "a changed directory reloads");
        }
    }
}
