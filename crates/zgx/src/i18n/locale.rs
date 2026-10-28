//! Which of an app's locales to show: the one asked for, else the user's
//! preferred languages in order, each matched against what the app has.
use std::sync::OnceLock;

static LANGUAGES: OnceLock<fn() -> Vec<String>> = OnceLock::new();

/// Tell zgx how to read the user's preferred languages, most preferred
/// first, as BCP 47 tags: set once, by the host (zgx-desktop reads macOS's
/// and Windows'). Until then they come from the environment.
pub fn set_system_languages(languages: fn() -> Vec<String>) {
    let _ = LANGUAGES.set(languages);
}

/// The user's preferred languages, most preferred first: the host's
/// answer, else `LANGUAGE` (a colon-separated list), `LC_ALL`,
/// `LC_MESSAGES` or `LANG` (`pt_BR.UTF-8` read as `pt-BR`).
pub fn system_languages() -> Vec<String> {
    if let Some(languages) = LANGUAGES.get() {
        return languages();
    }
    let mut found = Vec::new();
    if let Ok(list) = std::env::var("LANGUAGE") {
        found.extend(list.split(':').filter_map(tag_of));
    }
    found.extend(
        ["LC_ALL", "LC_MESSAGES", "LANG"]
            .into_iter()
            .find_map(|name| std::env::var(name).ok().as_deref().and_then(tag_of)),
    );
    found
}

/// A POSIX locale name or a tag, as a BCP 47 tag: `pt_BR.UTF-8` is `pt-BR`;
/// `C` and `POSIX` are none.
fn tag_of(name: &str) -> Option<String> {
    let name = name.split(['.', '@']).next()?.trim();
    if name.is_empty() || name == "C" || name == "POSIX" {
        return None;
    }
    Some(name.replace('_', "-"))
}

/// `tag` and the tags it falls back to, most specific first: `zh-Hant-TW`,
/// `zh-Hant`, `zh`.
pub(crate) fn parents(tag: &str) -> impl Iterator<Item = &str> {
    std::iter::successors(Some(tag), |t| t.rfind('-').map(|at| &t[..at]))
}

/// Whether two tags name the same locale (case and `_` aside).
pub(crate) fn same(a: &str, b: &str) -> bool {
    a.len() == b.len()
        && a.bytes().zip(b.bytes()).all(|(x, y)| {
            x.eq_ignore_ascii_case(&y) || (x, y) == (b'_', b'-') || (x, y) == (b'-', b'_')
        })
}

/// The best of `available` for `preferred` (most preferred first), else
/// `source`: a locale itself, else one it falls back to (`pt-BR` to `pt`),
/// else one of its language (`pt` to `pt-BR`).
pub(crate) fn negotiate<'a>(
    preferred: impl IntoIterator<Item = impl AsRef<str>>,
    available: &'a [std::rc::Rc<str>],
    source: &'a std::rc::Rc<str>,
) -> &'a std::rc::Rc<str> {
    for wanted in preferred {
        let wanted = wanted.as_ref().replace('_', "-");
        for tag in parents(&wanted) {
            if let Some(found) = available.iter().find(|a| same(a, tag)) {
                return found;
            }
        }
        let language = wanted.split('-').next().unwrap_or_default();
        if let Some(found) = available
            .iter()
            .find(|a| same(a.split('-').next().unwrap_or_default(), language))
        {
            return found;
        }
    }
    source
}

#[cfg(test)]
mod tests {
    use std::rc::Rc;

    #[test]
    fn negotiation_prefers_exact_then_parent_then_language() {
        let available: Vec<Rc<str>> =
            vec!["en".into(), "pt".into(), "zh-Hant".into(), "fr-CA".into()];
        let source: Rc<str> = "en".into();
        assert_eq!(&**super::negotiate(["pt-BR"], &available, &source), "pt");
        assert_eq!(
            &**super::negotiate(["zh-Hant-TW"], &available, &source),
            "zh-Hant"
        );
        assert_eq!(&**super::negotiate(["fr"], &available, &source), "fr-CA");
        assert_eq!(
            &**super::negotiate(["de", "pt_PT"], &available, &source),
            "pt"
        );
        assert_eq!(&**super::negotiate(["de"], &available, &source), "en");
        assert_eq!(
            &**super::negotiate(["ZH-hant"], &available, &source),
            "zh-Hant"
        );
    }

    #[test]
    fn posix_names_read_as_tags() {
        assert_eq!(super::tag_of("pt_BR.UTF-8").as_deref(), Some("pt-BR"));
        assert_eq!(super::tag_of("C.UTF-8"), None);
        assert_eq!(super::tag_of("sr_RS@latin").as_deref(), Some("sr-RS"));
    }
}
