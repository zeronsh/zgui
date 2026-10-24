#[cfg(not(feature = "std"))]
use alloc::{string::String, vec::Vec};
use core::ops::Range;

use crate::{AttrsOwned, HashMap, ShapeGlyph};

/// Key for caching shape runs.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ShapeRunKey {
    pub text: String,
    pub default_attrs: AttrsOwned,
    /// zgui patch: `default_attrs`'s hash, kept with them (see
    /// `ShapeRunKey::set_defaults`): a key is hashed for every word shaped,
    /// and hashing the attributes (a family name among them) each time
    /// cost more than the text.
    pub default_attrs_hash: u64,
    pub attrs_spans: Vec<(Range<usize>, AttrsOwned)>,
    /// zgui patch: shaped right to left. The same text shapes differently
    /// in each direction; without it in the key, a word kept from one
    /// direction was served for the other.
    pub rtl: bool,
}

impl ShapeRunKey {
    /// A key for `text` under `defaults`.
    pub fn new(text: String, default_attrs: AttrsOwned) -> Self {
        let mut key = Self {
            text,
            default_attrs_hash: 0,
            default_attrs,
            attrs_spans: Vec::new(),
            rtl: false,
        };
        key.default_attrs_hash = Self::hash_attrs(&key.default_attrs);
        key
    }
    /// Set the default attributes, and their hash with them.
    pub fn set_defaults(&mut self, default_attrs: AttrsOwned) {
        self.default_attrs_hash = Self::hash_attrs(&default_attrs);
        self.default_attrs = default_attrs;
    }
    fn hash_attrs(attrs: &AttrsOwned) -> u64 {
        use core::hash::{BuildHasher, BuildHasherDefault};
        BuildHasherDefault::<rustc_hash::FxHasher>::default().hash_one(attrs)
    }
}

impl core::hash::Hash for ShapeRunKey {
    fn hash<H: core::hash::Hasher>(&self, state: &mut H) {
        self.text.hash(state);
        self.default_attrs_hash.hash(state);
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
    /// far larger than what it holds (after a burst of shaping) is shrunk,
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
        if self.cache.capacity() > 4 * self.cache.len() + 64 {
            self.cache.shrink_to_fit();
        }
    }
}

impl core::fmt::Debug for ShapeRunCache {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_tuple("ShapeRunCache").finish()
    }
}
