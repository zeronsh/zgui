#[cfg(not(feature = "std"))]
use alloc::{string::String, vec::Vec};
use core::ops::Range;

use crate::{AttrsOwned, HashMap, ShapeGlyph};
#[cfg(not(feature = "std"))]
use alloc::sync::Arc;
#[cfg(feature = "std")]
use std::sync::Arc;

/// Key for caching shape runs.
///
/// zgui patch: a key's default attributes are compared and hashed by an id
/// the cache gives them (`ShapeRunCache::intern`), not whole: every word
/// looked up compared its key's attributes (a family name among them), and
/// a word shaped cloned them. `default_attrs` is what `default_attrs_id`
/// stands for, kept to see when a run's attributes change.
#[derive(Clone, Debug)]
pub struct ShapeRunKey {
    pub text: String,
    pub default_attrs: Arc<AttrsOwned>,
    pub default_attrs_id: u32,
    pub attrs_spans: Vec<(Range<usize>, AttrsOwned)>,
    /// zgui patch: shaped right to left. The same text shapes differently
    /// in each direction; without it in the key, a word kept from one
    /// direction was served for the other.
    pub rtl: bool,
}

impl ShapeRunKey {
    /// A key for `text` under `defaults`, which `cache` has given an id.
    pub fn new(text: String, default_attrs: AttrsOwned, cache: &mut ShapeRunCache) -> Self {
        Self {
            text,
            default_attrs_id: cache.intern(&default_attrs),
            default_attrs: Arc::new(default_attrs),
            attrs_spans: Vec::new(),
            rtl: false,
        }
    }
    /// Set the default attributes, and their id with them.
    pub fn set_defaults(&mut self, default_attrs: AttrsOwned, cache: &mut ShapeRunCache) {
        self.default_attrs_id = cache.intern(&default_attrs);
        self.default_attrs = Arc::new(default_attrs);
    }
}

impl PartialEq for ShapeRunKey {
    fn eq(&self, other: &Self) -> bool {
        self.text == other.text
            && self.default_attrs_id == other.default_attrs_id
            && self.rtl == other.rtl
            && self.attrs_spans == other.attrs_spans
    }
}

impl Eq for ShapeRunKey {}

impl core::hash::Hash for ShapeRunKey {
    fn hash<H: core::hash::Hasher>(&self, state: &mut H) {
        self.text.hash(state);
        self.default_attrs_id.hash(state);
        self.attrs_spans.hash(state);
        self.rtl.hash(state);
    }
}

/// Ages between sweeps of the cache (see `ShapeRunCache::trim`).
const TRIM_PERIOD: u64 = 8;

/// A helper structure for caching shape runs.
#[derive(Clone, Default)]
pub struct ShapeRunCache {
    age: u64,
    cache: HashMap<ShapeRunKey, (u64, Vec<ShapeGlyph>)>,
    /// zgui patch: the ids of the default attributes keys have held (see
    /// `ShapeRunKey`): a few, kept for good.
    ids: HashMap<AttrsOwned, u32>,
}

impl ShapeRunCache {
    /// Get cache item, updating age if found
    pub fn get(&mut self, key: &ShapeRunKey) -> Option<&Vec<ShapeGlyph>> {
        self.cache.get_mut(key).map(|(age, glyphs)| {
            *age = self.age;
            &*glyphs
        })
    }

    /// zgui patch: how many runs it holds.
    pub fn len(&self) -> usize {
        self.cache.len()
    }

    /// zgui patch: whether it holds none.
    pub fn is_empty(&self) -> bool {
        self.cache.is_empty()
    }

    /// zgui patch: the id of `attrs` in keys (see `ShapeRunKey`).
    pub fn intern(&mut self, attrs: &AttrsOwned) -> u32 {
        if let Some(id) = self.ids.get(attrs) {
            return *id;
        }
        let id = self.ids.len() as u32;
        self.ids.insert(attrs.clone(), id);
        id
    }

    /// Insert cache item with current age
    pub fn insert(&mut self, key: ShapeRunKey, glyphs: Vec<ShapeGlyph>) {
        self.cache.insert(key, (self.age, glyphs));
    }

    /// Remove anything in the cache with an age older than `keep_ages`
    ///
    /// zgui patch: the sweep visits every word, and a frame that shapes a
    /// few changed values called this each time over a table of thousands;
    /// it runs every `TRIM_PERIOD` ages instead, so a word lives between
    /// `keep_ages` and `keep_ages + TRIM_PERIOD` ages unused. A table left
    /// over twice what it holds (after a burst of shaping) is shrunk,
    /// where callers dropped it whole before.
    pub fn trim(&mut self, keep_ages: u64) {
        // Increase age
        self.age += 1;
        if self.age % TRIM_PERIOD != 0 {
            return;
        }
        let age = self.age;
        self.cache
            .retain(|_key, (used, _glyphs)| *used + keep_ages + TRIM_PERIOD >= age);
        if self.cache.capacity() > 2 * self.cache.len() + 64 {
            self.cache.shrink_to_fit();
        }
    }
}

impl core::fmt::Debug for ShapeRunCache {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_tuple("ShapeRunCache").finish()
    }
}
