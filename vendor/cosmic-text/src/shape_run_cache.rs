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
}

impl ShapeRunKey {
    /// A key for `text` under `defaults`.
    pub fn new(text: String, default_attrs: AttrsOwned) -> Self {
        let mut key = Self {
            text,
            default_attrs_hash: 0,
            default_attrs,
            attrs_spans: Vec::new(),
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
    }
}

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

    /// Insert cache item with current age
    pub fn insert(&mut self, key: ShapeRunKey, glyphs: Vec<ShapeGlyph>) {
        self.cache.insert(key, (self.age, glyphs));
    }

    /// Remove anything in the cache with an age older than `keep_ages`
    pub fn trim(&mut self, keep_ages: u64) {
        self.cache
            .retain(|_key, (age, _glyphs)| *age + keep_ages >= self.age);
        // Increase age
        self.age += 1;
    }
}

impl core::fmt::Debug for ShapeRunCache {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_tuple("ShapeRunCache").finish()
    }
}
