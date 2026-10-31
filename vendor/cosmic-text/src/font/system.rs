use crate::{Attrs, Family, FamilyOwned, Font, FontMatchAttrs, HashMap, ShapeBuffer};
use alloc::boxed::Box;
use alloc::collections::BTreeSet;
use alloc::string::String;
use alloc::sync::Arc;
use alloc::vec::Vec;
use core::fmt;
use core::ops::{Deref, DerefMut};
use fontdb::{FaceInfo, Query, Style};
use skrifa::raw::{ReadError, TableProvider as _};
use skrifa::MetadataProvider;

// re-export fontdb and harfrust
pub use fontdb;
pub use harfrust;

use super::fallback::{Fallback, Fallbacks, MonospaceFallbackInfo, PlatformFallback};

// The fields are used in the derived Ord implementation for sorting fallback candidates.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct FontMatchKey {
    pub(crate) not_emoji: bool,
    pub(crate) font_weight_diff: u16,
    pub(crate) font_stretch_diff: u16,
    pub(crate) font_style_diff: u8,
    pub(crate) font_weight: u16,
    pub(crate) font_stretch: u16,
    pub(crate) id: fontdb::ID,
    pub(crate) variable_weight_match: bool,
}

impl FontMatchKey {
    fn new(
        attrs: &Attrs,
        face: &FaceInfo,
        db: &fontdb::Database,
        weight_axes: &mut HashMap<fontdb::ID, Option<(f32, f32)>>,
    ) -> FontMatchKey {
        // TODO: smarter way of detecting emoji
        let not_emoji = !face.post_script_name.contains("Emoji");
        let font_weight_diff = attrs.weight.0.abs_diff(face.weight.0);

        // zgx patch: the face's weight axis is read from its file once, not
        // for every new family, weight and style asked for.
        let variable_weight_match = font_weight_diff != 0
            && weight_axes
                .entry(face.id)
                .or_insert_with(|| read_weight_axis(db, face.id))
                .is_some_and(|(min, max)| {
                    let w = attrs.weight.0 as f32;
                    w >= min && w <= max
                });
        let font_weight = face.weight.0;
        let font_stretch_diff = attrs.stretch.to_number().abs_diff(face.stretch.to_number());
        let font_stretch = face.stretch.to_number();
        let font_style_diff = match (attrs.style, face.style) {
            (Style::Normal, Style::Normal)
            | (Style::Italic, Style::Italic)
            | (Style::Oblique, Style::Oblique) => 0,
            (Style::Italic, Style::Oblique) | (Style::Oblique, Style::Italic) => 1,
            (Style::Normal, Style::Italic)
            | (Style::Normal, Style::Oblique)
            | (Style::Italic, Style::Normal)
            | (Style::Oblique, Style::Normal) => 2,
        };
        let id = face.id;
        FontMatchKey {
            not_emoji,
            font_weight_diff,
            font_stretch_diff,
            font_style_diff,
            font_weight,
            font_stretch,
            id,
            variable_weight_match,
        }
    }
}

/// zgx patch: a face's `wght` axis range, from its file.
fn read_weight_axis(db: &fontdb::Database, id: fontdb::ID) -> Option<(f32, f32)> {
    db.with_face_data(id, |font_data, face_index| {
        let font_ref = skrifa::FontRef::from_index(font_data, face_index).ok()?;
        let axis = font_ref.axes().get_by_tag(skrifa::Tag::new(b"wght"))?;
        Some((axis.min_value(), axis.max_value()))
    })
    .flatten()
}

struct FontCachedCodepointSupportInfo {
    supported: Vec<u32>,
    not_supported: Vec<u32>,
}

impl FontCachedCodepointSupportInfo {
    const SUPPORTED_MAX_SZ: usize = 512;
    const NOT_SUPPORTED_MAX_SZ: usize = 1024;

    fn new() -> Self {
        Self {
            supported: Vec::with_capacity(Self::SUPPORTED_MAX_SZ),
            not_supported: Vec::with_capacity(Self::NOT_SUPPORTED_MAX_SZ),
        }
    }

    #[inline(always)]
    fn unknown_has_codepoint(
        &mut self,
        font_codepoints: &[u32],
        codepoint: u32,
        supported_insert_pos: usize,
        not_supported_insert_pos: usize,
    ) -> bool {
        let ret = font_codepoints.contains(&codepoint);
        if ret {
            // don't bother inserting if we are going to truncate the entry away
            if supported_insert_pos != Self::SUPPORTED_MAX_SZ {
                self.supported.insert(supported_insert_pos, codepoint);
                self.supported.truncate(Self::SUPPORTED_MAX_SZ);
            }
        } else {
            // don't bother inserting if we are going to truncate the entry away
            if not_supported_insert_pos != Self::NOT_SUPPORTED_MAX_SZ {
                self.not_supported
                    .insert(not_supported_insert_pos, codepoint);
                self.not_supported.truncate(Self::NOT_SUPPORTED_MAX_SZ);
            }
        }
        ret
    }

    #[inline(always)]
    fn has_codepoint(&mut self, font_codepoints: &[u32], codepoint: u32) -> bool {
        match self.supported.binary_search(&codepoint) {
            Ok(_) => true,
            Err(supported_insert_pos) => match self.not_supported.binary_search(&codepoint) {
                Ok(_) => false,
                Err(not_supported_insert_pos) => self.unknown_has_codepoint(
                    font_codepoints,
                    codepoint,
                    supported_insert_pos,
                    not_supported_insert_pos,
                ),
            },
        }
    }
}

/// zgx: whether any text has asked for hyphenation (`FontFeatures::HYPHENS`), set by
/// the caller the first time one does (`note_hyphens`): until then, shaping a
/// line looks for nothing.
static HYPHENS_IN_USE: core::sync::atomic::AtomicBool = core::sync::atomic::AtomicBool::new(false);

/// zgx: some text hyphenates: lines shaped from now on look for their points.
pub fn note_hyphens() {
    HYPHENS_IN_USE.store(true, core::sync::atomic::Ordering::Relaxed);
}

/// zgx: a word's hyphenation points, as byte offsets inside it, given the
/// word and a locale (`FontSystem::set_hyphenator`).
pub type Hyphenator = std::sync::Arc<dyn Fn(&str, &str) -> Vec<usize> + Send + Sync>;

/// Access to the system fonts.
pub struct FontSystem {
    /// The locale of the system.
    locale: String,

    /// The underlying font database.
    db: fontdb::Database,

    /// Cache for loaded fonts from the database.
    /// zgx: by optical size too (0: the font's default).
    font_cache: HashMap<(fontdb::ID, fontdb::Weight, u16), Option<Arc<Font>>>,

    /// Sorted unique ID's of all Monospace fonts in DB
    monospace_font_ids: Vec<fontdb::ID>,

    /// Sorted unique ID's of all Monospace fonts in DB per script.
    /// A font may support multiple scripts of course, so the same ID
    /// may appear in multiple map value vecs.
    per_script_monospace_font_ids: HashMap<[u8; 4], Vec<fontdb::ID>>,

    /// Cache for font codepoint support info
    font_codepoint_support_info_cache: HashMap<fontdb::ID, FontCachedCodepointSupportInfo>,

    /// Cache for font matches.
    font_matches_cache: HashMap<FontMatchAttrs, Arc<Vec<FontMatchKey>>>,

    /// zgx patch: the matches and first font of the last attributes runs
    /// shaped with, newest last (`FontSystem::first_font`).
    first_font: Vec<FirstFont>,

    /// zgx patch: the faces `query_face` found, by family, weight and style.
    face_queries: Vec<(FamilyOwned, fontdb::Weight, fontdb::Style, Option<fontdb::ID>)>,

    /// zgx: where a word may break with a hyphen (`FontFeatures::HYPHENS`).
    hyphenator: Option<Hyphenator>,

    /// zgx patch: each face's `wght` axis range (`None`: not variable in
    /// weight), read from its file the first time a match needs it.
    weight_axes: HashMap<fontdb::ID, Option<(f32, f32)>>,

    /// Scratch buffer for shaping and laying out.
    pub(crate) shape_buffer: ShapeBuffer,

    /// Buffer for use in `FontFallbackIter`.
    pub(crate) monospace_fallbacks_buffer: BTreeSet<MonospaceFallbackInfo>,

    /// Cache for shaped runs
    #[cfg(feature = "shape-run-cache")]
    pub shape_run_cache: crate::ShapeRunCache,

    /// List of fallbacks
    pub(crate) dyn_fallback: Box<dyn Fallback>,

    /// List of fallbacks
    pub(crate) fallbacks: Fallbacks,
}

impl fmt::Debug for FontSystem {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("FontSystem")
            .field("locale", &self.locale)
            .field("db", &self.db)
            .finish_non_exhaustive()
    }
}

/// zgx patch: attributes kept by `FontSystem::first_font` (an interface's
/// few text styles: sizes, weights, families).
const FIRST_FONTS: usize = 8;

/// zgx patch: the font matches of a run's attributes, and the first font
/// of their fallback (see `FontSystem::first_font`).
#[derive(Debug)]
struct FirstFont {
    attrs: FontMatchAttrs,
    optical: u16,
    fonts: Arc<Vec<FontMatchKey>>,
    font: Arc<Font>,
}

impl FontSystem {
    const FONT_MATCHES_CACHE_SIZE_LIMIT: usize = 256;
    /// Create a new [`FontSystem`], that allows access to any installed system fonts
    ///
    /// # Timing
    ///
    /// This function takes some time to run. On the release build, it can take up to a second,
    /// while debug builds can take up to ten times longer. For this reason, it should only be
    /// called once, and the resulting [`FontSystem`] should be shared.
    pub fn new() -> Self {
        Self::new_with_fonts(core::iter::empty())
    }

    /// Create a new [`FontSystem`] with a pre-specified set of fonts.
    pub fn new_with_fonts(fonts: impl IntoIterator<Item = fontdb::Source>) -> Self {
        let locale = Self::get_locale();
        log::debug!("Locale: {locale}");

        let mut db = fontdb::Database::new();

        Self::load_fonts(&mut db, fonts.into_iter());

        //TODO: configurable default fonts
        db.set_monospace_family("Noto Sans Mono");
        db.set_sans_serif_family("Open Sans");
        db.set_serif_family("DejaVu Serif");

        Self::new_with_locale_and_db_and_fallback(locale, db, PlatformFallback)
    }

    /// zgx patch: a [`FontSystem`] over a database the caller loaded (from a
    /// cache of the system's fonts, say), with [`Self::new`]'s locale and
    /// default families.
    pub fn new_with_db(mut db: fontdb::Database) -> Self {
        db.set_monospace_family("Noto Sans Mono");
        db.set_sans_serif_family("Open Sans");
        db.set_serif_family("DejaVu Serif");
        Self::new_with_locale_and_db_and_fallback(Self::get_locale(), db, PlatformFallback)
    }

    /// zgx patch: the face's `wght` axis range (`None`: not variable in
    /// weight), as font matching reads it.
    pub fn weight_axis(&mut self, id: fontdb::ID) -> Option<(f32, f32)> {
        let db = &self.db;
        *self
            .weight_axes
            .entry(id)
            .or_insert_with(|| read_weight_axis(db, id))
    }

    /// zgx patch: gives font matching a face's `wght` axis range (from a
    /// cache), so it need not read the face's file.
    pub fn set_weight_axis(&mut self, id: fontdb::ID, axis: Option<(f32, f32)>) {
        self.weight_axes.insert(id, axis);
    }

    /// Create a new [`FontSystem`] with a pre-specified locale, font database and font fallback list.
    pub fn new_with_locale_and_db_and_fallback(
        locale: String,
        db: fontdb::Database,
        impl_fallback: impl Fallback + 'static,
    ) -> Self {
        let mut monospace_font_ids = db
            .faces()
            .filter(|face_info| {
                face_info.monospaced && !face_info.post_script_name.contains("Emoji")
            })
            .map(|face_info| face_info.id)
            .collect::<Vec<_>>();
        monospace_font_ids.sort();

        let mut per_script_monospace_font_ids: HashMap<[u8; 4], BTreeSet<fontdb::ID>> =
            HashMap::default();

        if cfg!(feature = "monospace_fallback") {
            for &id in &monospace_font_ids {
                db.with_face_data(id, |font_data, face_index| {
                    let face = skrifa::FontRef::from_index(font_data, face_index)?;
                    for script in face
                        .gpos()?
                        .script_list()?
                        .script_records()
                        .iter()
                        .chain(face.gsub()?.script_list()?.script_records().iter())
                    {
                        per_script_monospace_font_ids
                            .entry(script.script_tag().into_bytes())
                            .or_default()
                            .insert(id);
                    }
                    Ok::<_, ReadError>(())
                });
            }
        }

        let per_script_monospace_font_ids = per_script_monospace_font_ids
            .into_iter()
            .map(|(k, v)| (k, Vec::from_iter(v)))
            .collect();

        let fallbacks = Fallbacks::new(&impl_fallback, &[], &locale);

        Self {
            locale,
            db,
            monospace_font_ids,
            per_script_monospace_font_ids,
            font_cache: HashMap::default(),
            font_matches_cache: HashMap::default(),
            first_font: Vec::new(),
            face_queries: Vec::new(),
            hyphenator: None,
            weight_axes: HashMap::default(),
            font_codepoint_support_info_cache: HashMap::default(),
            monospace_fallbacks_buffer: BTreeSet::default(),
            #[cfg(feature = "shape-run-cache")]
            shape_run_cache: crate::ShapeRunCache::default(),
            shape_buffer: ShapeBuffer::default(),
            dyn_fallback: Box::new(impl_fallback),
            fallbacks,
        }
    }

    /// Create a new [`FontSystem`] with a pre-specified locale and font database.
    pub fn new_with_locale_and_db(locale: String, db: fontdb::Database) -> Self {
        Self::new_with_locale_and_db_and_fallback(locale, db, PlatformFallback)
    }

    /// Get the locale.
    pub fn locale(&self) -> &str {
        &self.locale
    }

    /// zgx: where words with `FontFeatures::HYPHENS` may break, a hyphen drawn at
    /// the end of the line: given a word and the system's locale, the byte
    /// offsets inside it. `None`, the default: none do, but at soft hyphens.
    pub fn set_hyphenator(&mut self, hyphenator: Option<Hyphenator>) {
        self.hyphenator = hyphenator;
    }

    /// zgx: whether words may be hyphenated (a hyphenator is set).
    pub(crate) fn hyphenates(&self) -> bool {
        HYPHENS_IN_USE.load(core::sync::atomic::Ordering::Relaxed) && self.hyphenator.is_some()
    }

    /// zgx: the hyphenation points of `word`, if words are hyphenated.
    pub(crate) fn hyphenate(&self, word: &str) -> Option<Vec<usize>> {
        let hyphenator = self.hyphenator.as_ref()?;
        Some(hyphenator(word, &self.locale))
    }

    /// Get the database.
    pub const fn db(&self) -> &fontdb::Database {
        &self.db
    }

    /// Get a mutable reference to the database.
    pub fn db_mut(&mut self) -> &mut fontdb::Database {
        self.font_matches_cache.clear();
        self.first_font.clear();
        self.face_queries.clear();
        &mut self.db
    }

    /// zgx patch: the face `db().query` gives for one family at a weight
    /// and style, found once: a query reads every installed face's names
    /// (hundreds on macOS, more on Windows), and a text with fallback
    /// families asks for each family's face whenever it is laid out anew.
    /// Forgotten when the database can change (`db_mut`).
    pub fn query_face(
        &mut self,
        family: Family<'_>,
        weight: fontdb::Weight,
        style: fontdb::Style,
    ) -> Option<fontdb::ID> {
        if let Some((.., id)) = self
            .face_queries
            .iter()
            .find(|(f, w, s, _)| *w == weight && *s == style && f.as_family() == family)
        {
            return *id;
        }
        let id = self.db.query(&fontdb::Query {
            families: &[family],
            weight,
            style,
            ..Default::default()
        });
        // An app's few text styles; more are asked for again.
        if self.face_queries.len() == 64 {
            self.face_queries.clear();
        }
        self.face_queries
            .push((FamilyOwned::new(family), weight, style, id));
        id
    }

    /// Consume this [`FontSystem`] and return the locale and database.
    pub fn into_locale_and_db(self) -> (String, fontdb::Database) {
        (self.locale, self.db)
    }

    /// Get a font by its ID and weight.
    pub fn get_font(&mut self, id: fontdb::ID, weight: fontdb::Weight) -> Option<Arc<Font>> {
        self.get_font_at(id, weight, 0)
    }

    /// zgx: the font instanced at an optical size (pixels; 0 for its
    /// default), for fonts with an `opsz` axis.
    pub fn get_font_at(
        &mut self,
        id: fontdb::ID,
        weight: fontdb::Weight,
        optical: u16,
    ) -> Option<Arc<Font>> {
        // A font without the axis is one instance at every size.
        if optical > 0 && !self.get_font_at(id, weight, 0)?.has_optical() {
            return self.get_font_at(id, weight, 0);
        }
        self.font_cache
            .entry((id, weight, optical))
            .or_insert_with(|| {
                #[cfg(feature = "std")]
                unsafe {
                    self.db.make_shared_face_data(id);
                }
                if let Some(font) = Font::new_at(&self.db, id, weight, optical) {
                    Some(Arc::new(font))
                } else {
                    log::warn!(
                        "failed to load font '{}'",
                        self.db.face(id)?.post_script_name
                    );
                    None
                }
            })
            .clone()
    }

    pub fn is_monospace(&self, id: fontdb::ID) -> bool {
        self.monospace_font_ids.binary_search(&id).is_ok()
    }

    pub fn get_monospace_ids_for_scripts(
        &self,
        scripts: impl Iterator<Item = [u8; 4]>,
    ) -> Vec<fontdb::ID> {
        let mut ret = scripts
            .filter_map(|script| self.per_script_monospace_font_ids.get(&script))
            .flat_map(|ids| ids.iter().copied())
            .collect::<Vec<_>>();
        ret.sort();
        ret.dedup();
        ret
    }

    #[inline(always)]
    pub fn get_font_supported_codepoints_in_word(
        &mut self,
        id: fontdb::ID,
        weight: fontdb::Weight,
        word: &str,
    ) -> Option<usize> {
        self.get_font(id, weight).map(|font| {
            let code_points = font.unicode_codepoints();
            let cache = self
                .font_codepoint_support_info_cache
                .entry(id)
                .or_insert_with(FontCachedCodepointSupportInfo::new);
            word.chars()
                .filter(|ch| cache.has_codepoint(code_points, u32::from(*ch)))
                .count()
        })
    }

    /// zgx patch: the font matches of `attrs`, and the first font a
    /// fallback iterator over them gives (its default family's face), if a
    /// run last shaped with the same attributes found them. Each word of a
    /// run looked its matches up by an owned key and walked them to its
    /// family's face again; the same attributes give the same of both.
    pub(crate) fn first_font(&self, attrs: &Attrs<'_>) -> Option<(Arc<Vec<FontMatchKey>>, Arc<Font>)> {
        self.first_font
            .iter()
            .rev()
            // A font without an optical size axis is the same instance at
            // every size (`get_font_at`).
            .find(|first| {
                first.attrs.matches(attrs)
                    && (first.optical == attrs.optical || !first.font.has_optical())
            })
            .map(|first| (first.fonts.clone(), first.font.clone()))
    }

    /// zgx patch: keep what `first_font` gives for `attrs`.
    pub(crate) fn keep_first_font(
        &mut self,
        attrs: &Attrs<'_>,
        fonts: &Arc<Vec<FontMatchKey>>,
        font: &Arc<Font>,
    ) {
        if self.first_font.len() == FIRST_FONTS {
            self.first_font.remove(0);
        }
        self.first_font.push(FirstFont {
            attrs: attrs.into(),
            optical: attrs.optical,
            fonts: fonts.clone(),
            font: font.clone(),
        });
    }

    pub fn get_font_matches(&mut self, attrs: &Attrs<'_>) -> Arc<Vec<FontMatchKey>> {
        // Clear the cache first if it reached the size limit
        if self.font_matches_cache.len() >= Self::FONT_MATCHES_CACHE_SIZE_LIMIT {
            log::trace!("clear font mache cache");
            self.font_matches_cache.clear();
            self.first_font.clear();
        }

        self.font_matches_cache
            //TODO: do not create AttrsOwned unless entry does not already exist
            .entry(attrs.into())
            .or_insert_with(|| {
                #[cfg(all(feature = "std", not(target_arch = "wasm32")))]
                let now = std::time::Instant::now();

                let mut font_match_keys = self
                    .db
                    .faces()
                    .map(|face| FontMatchKey::new(attrs, face, &self.db, &mut self.weight_axes))
                    .collect::<Vec<_>>();

                // Sort so we get the keys with weight_offset=0 first
                font_match_keys.sort();

                // db.query is better than above, but returns just one font
                let query = Query {
                    families: &[attrs.family],
                    weight: attrs.weight,
                    stretch: attrs.stretch,
                    style: attrs.style,
                };

                if let Some(id) = self.db.query(&query) {
                    if let Some(i) = font_match_keys
                        .iter()
                        .enumerate()
                        .find(|(_i, key)| key.id == id)
                        .map(|(i, _)| i)
                    {
                        // if exists move to front
                        let match_key = font_match_keys.remove(i);
                        font_match_keys.insert(0, match_key);
                    } else if let Some(face) = self.db.face(id) {
                        // else insert in front
                        let match_key =
                            FontMatchKey::new(attrs, face, &self.db, &mut self.weight_axes);
                        font_match_keys.insert(0, match_key);
                    } else {
                        log::error!("Could not get face from db, that should've been there.");
                    }
                }

                #[cfg(all(feature = "std", not(target_arch = "wasm32")))]
                {
                    let elapsed = now.elapsed();
                    log::debug!("font matches for {attrs:?} in {elapsed:?}");
                }

                Arc::new(font_match_keys)
            })
            .clone()
    }

    #[cfg(feature = "std")]
    fn get_locale() -> String {
        sys_locale::get_locale().unwrap_or_else(|| {
            log::warn!("failed to get system locale, falling back to en-US");
            String::from("en-US")
        })
    }

    #[cfg(not(feature = "std"))]
    fn get_locale() -> String {
        String::from("en-US")
    }

    #[cfg(feature = "std")]
    fn load_fonts(db: &mut fontdb::Database, fonts: impl Iterator<Item = fontdb::Source>) {
        #[cfg(not(target_arch = "wasm32"))]
        let now = std::time::Instant::now();

        db.load_system_fonts();

        for source in fonts {
            db.load_font_source(source);
        }

        #[cfg(not(target_arch = "wasm32"))]
        log::debug!(
            "Parsed {} font faces in {}ms.",
            db.len(),
            now.elapsed().as_millis()
        );
    }

    #[cfg(not(feature = "std"))]
    fn load_fonts(db: &mut fontdb::Database, fonts: impl Iterator<Item = fontdb::Source>) {
        for source in fonts {
            db.load_font_source(source);
        }
    }
}

/// A value borrowed together with an [`FontSystem`]
#[derive(Debug)]
pub struct BorrowedWithFontSystem<'a, T> {
    pub(crate) inner: &'a mut T,
    pub(crate) font_system: &'a mut FontSystem,
}

impl<T> Deref for BorrowedWithFontSystem<'_, T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        self.inner
    }
}

impl<T> DerefMut for BorrowedWithFontSystem<'_, T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.inner
    }
}
