//! The system's spelling checker on Linux, for editors that check
//! (`View::spellcheck`): Enchant (`libenchant-2`), the library GNOME's and
//! KDE's text fields check through, over Hunspell's (or another provider's)
//! dictionaries. It is loaded when an editor first checks, with the
//! dictionary for the user's language (`LC_ALL`, `LC_MESSAGES`, `LANG`), so
//! an app that checks nothing never loads it. Without Enchant or a
//! dictionary, nothing is flagged. Learn Spelling adds to the user's own
//! dictionary; Ignore Spelling lasts until the app quits.
use std::{
    ffi::{CStr, c_char, c_int, c_void},
    ops::Range,
    sync::{Mutex, OnceLock},
};
use unicode_segmentation::UnicodeSegmentation;

type Check = unsafe extern "C" fn(*mut c_void, *const c_char, isize) -> c_int;
type Suggest =
    unsafe extern "C" fn(*mut c_void, *const c_char, isize, *mut usize) -> *mut *mut c_char;
type FreeList = unsafe extern "C" fn(*mut c_void, *mut *mut c_char);
type Add = unsafe extern "C" fn(*mut c_void, *const c_char, isize);

/// An open dictionary and the functions that use it. The broker that opened
/// it is never freed: the dictionary lives as long as the app.
struct Dictionary {
    _library: libloading::Library,
    dict: *mut c_void,
    check: Check,
    suggest: Suggest,
    free_list: FreeList,
    add: Add,
    add_to_session: Add,
}
// SAFETY: used only behind `DICTIONARY`'s mutex, one call at a time.
unsafe impl Send for Dictionary {}

impl Dictionary {
    /// Enchant's dictionary for the first of `tags` it has.
    fn open(tags: &[String]) -> Option<Self> {
        // SAFETY: Enchant's documented C API, its symbols typed as declared
        // in enchant.h; the library is kept loaded with the dictionary.
        unsafe {
            let library = libloading::Library::new("libenchant-2.so.2").ok()?;
            let init: libloading::Symbol<unsafe extern "C" fn() -> *mut c_void> =
                library.get(b"enchant_broker_init\0").ok()?;
            let request: libloading::Symbol<
                unsafe extern "C" fn(*mut c_void, *const c_char) -> *mut c_void,
            > = library.get(b"enchant_broker_request_dict\0").ok()?;
            let check: Check = *library.get(b"enchant_dict_check\0").ok()?;
            let suggest: Suggest = *library.get(b"enchant_dict_suggest\0").ok()?;
            let free_list: FreeList = *library.get(b"enchant_dict_free_string_list\0").ok()?;
            let add: Add = *library.get(b"enchant_dict_add\0").ok()?;
            let add_to_session: Add = *library.get(b"enchant_dict_add_to_session\0").ok()?;
            let broker = init();
            if broker.is_null() {
                return None;
            }
            let dict = tags.iter().find_map(|tag| {
                let tag = std::ffi::CString::new(tag.as_str()).ok()?;
                let dict = request(broker, tag.as_ptr());
                (!dict.is_null()).then_some(dict)
            })?;
            Some(Self {
                _library: library,
                dict,
                check,
                suggest,
                free_list,
                add,
                add_to_session,
            })
        }
    }
    fn misspelled(&self, word: &str) -> bool {
        // SAFETY: a pointer and length into `word`, which outlives the call.
        unsafe { (self.check)(self.dict, word.as_ptr().cast(), word.len() as isize) > 0 }
    }
    fn guesses(&self, word: &str) -> Vec<String> {
        let mut count = 0;
        // SAFETY: as in `misspelled`; the list Enchant returns is read, then
        // given back to it to free.
        unsafe {
            let list = (self.suggest)(
                self.dict,
                word.as_ptr().cast(),
                word.len() as isize,
                &mut count,
            );
            if list.is_null() {
                return Vec::new();
            }
            let guesses = (0..count)
                .map(|i| CStr::from_ptr(*list.add(i)).to_string_lossy().into_owned())
                .collect();
            (self.free_list)(self.dict, list);
            guesses
        }
    }
}

/// The dictionary tags for the user's language, most specific first:
/// `de_CH.UTF-8` gives `de_CH`, then `de`. English without a language set.
fn languages() -> Vec<String> {
    let locale = ["LC_ALL", "LC_MESSAGES", "LANG"]
        .iter()
        .filter_map(|name| std::env::var(name).ok())
        .find(|value| !value.is_empty())
        .unwrap_or_default();
    tags(&locale)
}
fn tags(locale: &str) -> Vec<String> {
    let name = locale.split(['.', '@']).next().unwrap_or("");
    if name.is_empty() || name == "C" || name == "POSIX" {
        return vec!["en_US".into(), "en".into()];
    }
    let mut tags = vec![name.to_string()];
    if let Some((language, _)) = name.split_once('_') {
        tags.push(language.to_string());
    }
    tags
}

fn with<T>(default: T, f: impl FnOnce(&Dictionary) -> T) -> T {
    static DICTIONARY: OnceLock<Option<Mutex<Dictionary>>> = OnceLock::new();
    match DICTIONARY.get_or_init(|| Dictionary::open(&languages()).map(Mutex::new)) {
        Some(dictionary) => f(&dictionary.lock().unwrap()),
        None => default,
    }
}

/// The words of `text` worth checking, as byte ranges: those with a letter
/// and no digit (not "v2" or "1st"), apostrophes within them kept.
fn words(text: &str) -> impl Iterator<Item = (usize, &str)> {
    text.split_word_bound_indices().filter(|(_, word)| {
        word.chars().any(char::is_alphabetic) && !word.chars().any(|c| c.is_ascii_digit())
    })
}

pub(crate) fn misspellings(text: &str) -> Vec<Range<usize>> {
    with(Vec::new(), |dictionary| {
        words(text)
            .filter(|(_, word)| dictionary.misspelled(word))
            .map(|(at, word)| at..at + word.len())
            .collect()
    })
}
pub(crate) fn guesses(word: &str) -> Vec<String> {
    with(Vec::new(), |dictionary| dictionary.guesses(word))
}
pub(crate) fn learn(word: &str) {
    with((), |dictionary| {
        // SAFETY: as in `Dictionary::misspelled`.
        unsafe { (dictionary.add)(dictionary.dict, word.as_ptr().cast(), word.len() as isize) }
    });
}
pub(crate) fn ignore(word: &str) {
    with((), |dictionary| {
        // SAFETY: as in `Dictionary::misspelled`.
        unsafe {
            (dictionary.add_to_session)(dictionary.dict, word.as_ptr().cast(), word.len() as isize)
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn locales_name_dictionaries() {
        assert_eq!(tags("de_CH.UTF-8"), ["de_CH", "de"]);
        assert_eq!(tags("fr_FR@euro"), ["fr_FR", "fr"]);
        assert_eq!(tags("C.UTF-8"), ["en_US", "en"]);
        assert_eq!(tags(""), ["en_US", "en"]);
    }

    #[test]
    fn words_skip_numbers_and_punctuation() {
        let found: Vec<_> = words("Don't fix v2, 1st: héllo!").map(|(_, w)| w).collect();
        assert_eq!(found, ["Don't", "fix", "héllo"]);
    }

    #[test]
    fn the_system_checker_finds_misspellings_as_byte_ranges() {
        // Where Enchant and an English dictionary are installed (CI's Linux
        // runner, most desktops); elsewhere there is nothing to check.
        let Some(dictionary) = Dictionary::open(&["en_US".into()]) else {
            eprintln!("no Enchant English dictionary: skipped");
            return;
        };
        let text = "Thé quikc brown fox";
        let found: Vec<_> = words(text)
            .filter(|(_, word)| dictionary.misspelled(word))
            .map(|(at, word)| at..at + word.len())
            .collect();
        assert!(
            found.iter().any(|r| &text[r.clone()] == "quikc"),
            "{found:?}"
        );
        assert!(
            !found.iter().any(|r| &text[r.clone()] == "brown"),
            "{found:?}"
        );
        assert!(dictionary.guesses("quikc").iter().any(|g| g == "quick"));
    }
}
