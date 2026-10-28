//! Localized text: locale files of ICU MessageFormat messages, the locale
//! the user prefers, and text that follows both.
//!
//! An app makes an [`I18n`] and provides it to its views; nothing lives on
//! the `Ui`, so an app without one pays nothing. Each message key a view
//! reads gets a signal of its own, made on that first read: a locale switch
//! or a reloaded file sets only the keys whose text changed, and only the
//! text reading them formats again.
//!
//! ```ignore
//! let i18n = I18n::load(&ui, "locales")?;
//! ui.mount(provide(i18n.clone(), app()));
//! text_message(i18n.t("inbox.count").arg("n", count));
//! i18n.set_locale(Some("ar"));
//! ```
mod catalog;
mod format;
mod locale;
mod message;

pub use catalog::{Catalog, I18nError};
pub use format::plural_categories;
pub use locale::{set_system_languages, system_languages};
pub use message::{Argument, ArgumentKind, DateLength, ParseError, Pattern};

use crate::reactive::{Effect, Runtime, Signal};
use crate::scene::Direction;
use std::{
    cell::RefCell,
    path::{Path, PathBuf},
    rc::{Rc, Weak},
    sync::Arc,
};

/// A value a message's argument takes.
#[derive(Clone, Debug, PartialEq)]
pub enum ArgValue {
    /// A number: formatted for the locale, and what plurals choose by.
    Number(f64),
    /// Text, as it is: what selects choose by.
    Text(Rc<str>),
    /// A date, in milliseconds since 1970 began (UTC).
    Date(f64),
}
macro_rules! number_args {
    ($($t:ty),*) => {$(
        impl From<$t> for ArgValue {
            fn from(n: $t) -> Self {
                ArgValue::Number(n as f64)
            }
        }
    )*};
}
number_args!(f64, f32, i8, i16, i32, i64, u8, u16, u32, u64, usize, isize);
impl From<&str> for ArgValue {
    fn from(text: &str) -> Self {
        ArgValue::Text(text.into())
    }
}
impl From<String> for ArgValue {
    fn from(text: String) -> Self {
        ArgValue::Text(text.into())
    }
}
impl From<Rc<str>> for ArgValue {
    fn from(text: Rc<str>) -> Self {
        ArgValue::Text(text)
    }
}

/// What a message's argument reads: a value, or a signal or closure read
/// (and tracked) each time the message formats.
#[derive(Clone)]
pub enum Arg {
    /// A value, as it is.
    Value(ArgValue),
    /// Read each time the message formats.
    Read(Rc<dyn Fn() -> ArgValue>),
}
/// A value or signal a message's argument can take ([`Message::arg`]).
pub trait IntoArg {
    /// The argument.
    fn into_arg(self) -> Arg;
}
impl<T: Into<ArgValue>> IntoArg for T {
    fn into_arg(self) -> Arg {
        Arg::Value(self.into())
    }
}
impl<T: Clone + Into<ArgValue> + 'static> IntoArg for Signal<T> {
    fn into_arg(self) -> Arg {
        Arg::Read(Rc::new(move || self.get().into()))
    }
}

/// Something wrong found while formatting or loading, told once each to
/// [`I18n::on_issue`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Issue {
    /// What it is.
    pub kind: IssueKind,
    /// The locale it was found in (for a missing key, the one shown).
    pub locale: String,
    /// The message's key; empty for a locale asked for.
    pub key: String,
    /// What, in words (a parse error's, with its line and column).
    pub detail: String,
}
/// What an [`Issue`] is.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum IssueKind {
    /// No locale has the key: its text is the key.
    MissingKey,
    /// The message names an argument it wasn't given: shown as `{name}`.
    MissingArgument,
    /// A message that doesn't parse (or a file that isn't JSON): its key
    /// falls back to another locale's.
    Parse,
    /// A locale asked for that the app has nothing of.
    UnknownLocale,
}

/// The locale shown and the way it runs.
#[derive(Clone, PartialEq)]
struct Current {
    locale: Arc<str>,
    direction: Direction,
}

/// What a key shows now: its text and pattern (none: missing everywhere),
/// and the locale numbers and plurals in it follow (none if it has none).
#[derive(Clone)]
struct Resolved {
    text: Option<Rc<str>>,
    pattern: Option<Rc<Pattern>>,
    formats_in: Option<Rc<str>>,
    /// `formats_in`'s formatters (the source's if none), found once here
    /// rather than at each format.
    formatters: Option<Rc<format::Formatters>>,
}
impl PartialEq for Resolved {
    fn eq(&self, other: &Self) -> bool {
        self.text == other.text && self.formats_in == other.formats_in
    }
}

/// An app's localized messages: every locale's file, the locale shown, and
/// a signal per key read. Cheap to clone (one `Rc`); provide it to views
/// (`provide(i18n, view)`, read with `Context::service`).
#[derive(Clone)]
pub struct I18n(Rc<Inner>);

/// An issue told: what, in which locale, for which key.
type Told = (IssueKind, Rc<str>, Rc<str>);
/// Who hears of issues (`I18n::on_issue`).
type IssueListener = Rc<dyn Fn(&Issue)>;

struct Inner {
    runtime: Runtime,
    source: Rc<str>,
    catalogs: RefCell<Vec<(Rc<str>, Catalog)>>,
    /// The locale asked for (`None`: the system's preferred languages).
    requested: Signal<Option<String>>,
    current: Signal<Current>,
    /// Where a key is looked for, in order: the locale shown, the ones it
    /// falls back to that the app has, the source.
    chain: RefCell<Vec<Rc<str>>>,
    keys: RefCell<rustc_hash::FxHashMap<Rc<str>, Signal<Resolved>>>,
    formatters: RefCell<Vec<(Rc<str>, Rc<format::Formatters>)>>,
    told: RefCell<rustc_hash::FxHashSet<Told>>,
    on_issue: RefCell<Option<IssueListener>>,
    /// Issues found before anyone listened (a load's parse errors).
    pending: RefCell<Vec<Issue>>,
    files: RefCell<Vec<PathBuf>>,
    /// Re-negotiates when another locale is asked for.
    _follow: RefCell<Option<Effect>>,
}

impl I18n {
    /// A catalog with no locales yet, its keys defined by `source`'s file.
    pub fn new(runtime: &Runtime, source: &str) -> I18n {
        let source: Rc<str> = source.into();
        let current = Current {
            locale: Arc::from(&*source),
            direction: Direction::of_language(&source),
        };
        let i18n = I18n(Rc::new(Inner {
            runtime: runtime.clone(),
            requested: runtime.signal(None),
            current: runtime.signal(current),
            chain: RefCell::new(vec![source.clone()]),
            source,
            catalogs: RefCell::default(),
            keys: RefCell::default(),
            formatters: RefCell::default(),
            told: RefCell::default(),
            on_issue: RefCell::default(),
            pending: RefCell::default(),
            files: RefCell::default(),
            _follow: RefCell::default(),
        }));
        // Asked for another locale: the keys whose text that changes.
        let weak: Weak<Inner> = Rc::downgrade(&i18n.0);
        let requested = i18n.0.requested.clone();
        let mut first = true;
        let follow = runtime.effect(move || {
            requested.with(|_| ());
            if std::mem::take(&mut first) {
                return;
            }
            if let Some(inner) = weak.upgrade() {
                let runtime = inner.runtime.clone();
                runtime.untracked(|| I18n(inner).negotiate());
            }
        });
        *i18n.0._follow.borrow_mut() = Some(follow);
        i18n
    }

    /// Every locale file (`en.json`, `pt-BR.json`: a locale's name is its
    /// file's) in `dir`, with `en` the source; the locale negotiated. A
    /// message that doesn't parse is an [`Issue`]; a directory that can't
    /// be read, an error.
    pub fn load(ui: &crate::widgets::Ui, dir: impl AsRef<Path>) -> std::io::Result<I18n> {
        Self::load_dir(&ui.runtime, dir, "en")
    }

    /// [`I18n::load`] with a runtime and the source locale.
    pub fn load_dir(
        runtime: &Runtime,
        dir: impl AsRef<Path>,
        source: &str,
    ) -> std::io::Result<I18n> {
        let i18n = I18n::new(runtime, source);
        let mut files: Vec<PathBuf> = std::fs::read_dir(dir.as_ref())?
            .filter_map(|entry| entry.ok().map(|e| e.path()))
            .filter(|path| path.extension().is_some_and(|e| e == "json"))
            .collect();
        files.sort();
        for path in &files {
            let Some(locale) = path.file_stem().and_then(|s| s.to_str()) else {
                continue;
            };
            let text = std::fs::read_to_string(path)?;
            // Its errors are issues, kept until something listens.
            let _ = i18n.insert(locale, &text);
        }
        *i18n.0.files.borrow_mut() = files;
        Ok(i18n)
    }

    /// Add `locale`'s file, or replace it (a reload): its keys are the
    /// file's, so one taken out falls back. Only keys whose text changed
    /// tell their readers; how many did. A message that doesn't parse is
    /// left out (it falls back) and reported, with the rest kept.
    pub fn insert(&self, locale: &str, json: &str) -> Result<usize, Vec<I18nError>> {
        let (catalog, errors) = Catalog::parse(locale, json);
        {
            let mut catalogs = self.0.catalogs.borrow_mut();
            match catalogs.iter_mut().find(|(l, _)| locale::same(l, locale)) {
                Some((_, old)) => *old = catalog,
                None => catalogs.push((locale.into(), catalog)),
            }
        }
        for error in &errors {
            self.tell(Issue {
                kind: IssueKind::Parse,
                locale: error.locale.clone(),
                key: error.key.clone(),
                detail: error.to_string(),
            });
        }
        let changed = self.negotiate();
        if errors.is_empty() {
            Ok(changed)
        } else {
            Err(errors)
        }
    }

    /// The locale files [`I18n::load_dir`] read: what a watcher or a bundle
    /// keeps.
    pub fn files(&self) -> Vec<PathBuf> {
        self.0.files.borrow().clone()
    }

    /// The locales it has, the source first.
    pub fn locales(&self) -> Vec<String> {
        let catalogs = self.0.catalogs.borrow();
        let mut found: Vec<String> = catalogs.iter().map(|(l, _)| l.to_string()).collect();
        if let Some(at) = found.iter().position(|l| locale::same(l, &self.0.source)) {
            found[..=at].rotate_right(1);
        }
        found
    }

    /// The locale asked for: `None` follows the system's preferred
    /// languages. Setting it re-negotiates.
    pub fn requested(&self) -> Signal<Option<String>> {
        self.0.requested.clone()
    }

    /// Ask for `locale` (`None`: the system's again).
    pub fn set_locale(&self, locale: Option<&str>) {
        self.0.requested.set(locale.map(str::to_owned));
    }

    /// The locale shown (tracked): the one asked for or preferred, as the
    /// app has it.
    pub fn current(&self) -> Arc<str> {
        self.0.current.with(|c| c.locale.clone())
    }

    /// The way the locale shown runs (tracked).
    pub fn direction(&self) -> Direction {
        self.0.current.with(|c| c.direction)
    }

    /// Read the system's preferred languages again (they changed) and
    /// re-negotiate if none was asked for.
    pub fn refresh_system(&self) {
        self.negotiate();
    }

    /// Tell `listener` about each issue found from now (and those found
    /// before it listened), once each.
    pub fn on_issue(&self, listener: impl Fn(&Issue) + 'static) {
        let listener: Rc<dyn Fn(&Issue)> = Rc::new(listener);
        *self.0.on_issue.borrow_mut() = Some(listener.clone());
        let pending = std::mem::take(&mut *self.0.pending.borrow_mut());
        for issue in &pending {
            listener(issue);
        }
    }

    /// The message `key`, with no arguments yet.
    pub fn t(&self, key: &str) -> Message {
        Message {
            inner: Rc::new(MessageInner {
                i18n: self.clone(),
                key: key.into(),
                args: smallvec::SmallVec::new(),
                signal: std::cell::OnceCell::new(),
            }),
        }
    }

    /// Choose the locale again, and set the keys whose text that changes:
    /// how many.
    fn negotiate(&self) -> usize {
        let inner = &self.0;
        let requested = inner.requested.with_untracked(Clone::clone);
        let available: Vec<Rc<str>> = inner
            .catalogs
            .borrow()
            .iter()
            .map(|(l, _)| l.clone())
            .collect();
        let chosen = match &requested {
            Some(asked) => {
                let wanted = &asked.replace('_', "-");
                let chosen = locale::negotiate([wanted], &available, &inner.source).clone();
                let known = locale::parents(wanted)
                    .any(|t| available.iter().any(|a| locale::same(a, t)))
                    || available.iter().any(|a| {
                        locale::same(
                            a.split('-').next().unwrap_or_default(),
                            wanted.split('-').next().unwrap_or_default(),
                        )
                    });
                if !known && !available.is_empty() {
                    self.tell(Issue {
                        kind: IssueKind::UnknownLocale,
                        locale: asked.clone(),
                        key: String::new(),
                        detail: format!("no locale file for {wanted}"),
                    });
                }
                chosen
            }
            None => {
                locale::negotiate(locale::system_languages(), &available, &inner.source).clone()
            }
        };
        let mut chain: Vec<Rc<str>> = Vec::new();
        for tag in locale::parents(&chosen) {
            if let Some(found) = available.iter().find(|a| locale::same(a, tag))
                && !chain.iter().any(|c| Rc::ptr_eq(c, found))
            {
                chain.push(found.clone());
            }
        }
        if !chain.iter().any(|c| locale::same(c, &inner.source)) {
            // The catalog's own name: keys are looked up by it.
            let source = available.iter().find(|a| locale::same(a, &inner.source));
            chain.push(source.unwrap_or(&inner.source).clone());
        }
        *inner.chain.borrow_mut() = chain;
        let shown = Current {
            locale: Arc::from(&*chosen),
            direction: Direction::of_language(&chosen),
        };
        // Keys first, then the locale: a reader of both formats once.
        let keys: Vec<(Rc<str>, Signal<Resolved>)> = inner
            .keys
            .borrow()
            .iter()
            .map(|(k, s)| (k.clone(), s.clone()))
            .collect();
        let mut changed = 0;
        inner.runtime.batch(|| {
            for (key, signal) in keys {
                if signal.set(self.resolve(&key)) {
                    changed += 1;
                }
            }
            inner.current.set(shown);
        });
        changed
    }

    /// Where `key`'s text is now: the first locale of the chain that has it.
    fn resolve(&self, key: &str) -> Resolved {
        let inner = &self.0;
        let catalogs = inner.catalogs.borrow();
        let chain = inner.chain.borrow();
        for locale in chain.iter() {
            let found = catalogs
                .iter()
                .find(|(l, _)| Rc::ptr_eq(l, locale))
                .and_then(|(_, catalog)| catalog.get(key));
            if let Some((pattern, text)) = found {
                // Numbers and plurals follow the locale shown.
                let formats_in = pattern.by_locale.then(|| chain[0].clone());
                let formatters = self.formatters(formats_in.as_ref().unwrap_or(&chain[0]));
                return Resolved {
                    text: Some(text.clone()),
                    pattern: Some(pattern.clone()),
                    formats_in,
                    formatters: Some(formatters),
                };
            }
        }
        Resolved {
            text: None,
            pattern: None,
            formats_in: None,
            formatters: None,
        }
    }

    /// `key`'s signal, made on its first read.
    fn key(&self, key: &Rc<str>) -> Signal<Resolved> {
        if let Some(signal) = self.0.keys.borrow().get(key) {
            return signal.clone();
        }
        let signal = self.0.runtime.signal(self.resolve(key));
        self.0.keys.borrow_mut().insert(key.clone(), signal.clone());
        signal
    }

    fn formatters(&self, locale: &Rc<str>) -> Rc<format::Formatters> {
        let mut cache = self.0.formatters.borrow_mut();
        if let Some((_, found)) = cache.iter().find(|(l, _)| Rc::ptr_eq(l, locale)) {
            return found.clone();
        }
        let made = Rc::new(format::Formatters::new(locale));
        cache.push((locale.clone(), made.clone()));
        made
    }

    /// Tell the listener (or keep for it) an issue not told before.
    fn tell(&self, issue: Issue) {
        let id = (issue.kind, Rc::from(&*issue.locale), Rc::from(&*issue.key));
        if !self.0.told.borrow_mut().insert(id) {
            return;
        }
        let listener = self.0.on_issue.borrow().clone();
        match listener {
            Some(listener) => listener(&issue),
            None => self.0.pending.borrow_mut().push(issue),
        }
    }
}

/// A message to show: a key and its arguments. Cheap to clone; its text
/// ([`Message::get`]) follows the locale, reloads and the signals its
/// arguments read.
#[derive(Clone)]
pub struct Message {
    inner: Rc<MessageInner>,
}
#[derive(Clone)]
struct MessageInner {
    i18n: I18n,
    key: Rc<str>,
    args: smallvec::SmallVec<[(Rc<str>, Arg); 2]>,
    /// Its key's signal, found on the first format.
    signal: std::cell::OnceCell<Signal<Resolved>>,
}

impl Message {
    /// The message with argument `name` taking `value`: a number, text, or
    /// a signal of one (read each time it formats).
    pub fn arg(mut self, name: &str, value: impl IntoArg) -> Self {
        let inner = Rc::make_mut(&mut self.inner);
        let value = value.into_arg();
        match inner.args.iter_mut().find(|(n, _)| &**n == name) {
            Some((_, old)) => *old = value,
            None => inner.args.push((name.into(), value)),
        }
        self
    }

    /// [`Message::arg`] read from `read` each time it formats (its reads
    /// tracked): a computed value without a signal of its own.
    pub fn arg_with(self, name: &str, read: impl Fn() -> ArgValue + 'static) -> Self {
        self.arg_read(name, Rc::new(read))
    }
    fn arg_read(mut self, name: &str, read: Rc<dyn Fn() -> ArgValue>) -> Self {
        let inner = Rc::make_mut(&mut self.inner);
        match inner.args.iter_mut().find(|(n, _)| &**n == name) {
            Some((_, old)) => *old = Arg::Read(read),
            None => inner.args.push((name.into(), Arg::Read(read))),
        }
        self
    }

    /// Its key.
    pub fn key(&self) -> &str {
        &self.inner.key
    }

    /// Its text now, in the locale shown: tracked (its key, and the
    /// signals its arguments read), so a computed or text reading it
    /// follows.
    pub fn get(&self) -> String {
        let inner = &*self.inner;
        let i18n = &inner.i18n;
        let signal = inner.signal.get_or_init(|| i18n.key(&inner.key));
        let found = signal.with(|r| {
            let size = r.text.as_ref().map_or(0, |t| t.len());
            (r.pattern.clone().zip(r.formatters.clone()), size)
        });
        let (Some((pattern, formatters)), size) = found else {
            i18n.tell(Issue {
                kind: IssueKind::MissingKey,
                locale: i18n.0.current.with_untracked(|c| c.locale.to_string()),
                key: inner.key.to_string(),
                detail: format!("no locale has {}", inner.key),
            });
            return inner.key.to_string();
        };
        let mut out = String::with_capacity(size + 8);
        let mut missing = Vec::new();
        pattern.format(
            &mut out,
            &formatters,
            &mut |name: &str| {
                inner
                    .args
                    .iter()
                    .find(|(n, _)| &**n == name)
                    .map(|(_, arg)| match arg {
                        Arg::Value(value) => value.clone(),
                        Arg::Read(read) => read(),
                    })
            },
            &mut |name: &str| missing.push(name.to_owned()),
            None,
        );
        for name in missing {
            i18n.tell(Issue {
                kind: IssueKind::MissingArgument,
                locale: i18n.0.current.with_untracked(|c| c.locale.to_string()),
                key: inner.key.to_string(),
                detail: format!("{} names {{{name}}}, not given", inner.key),
            });
        }
        out
    }
}

impl std::fmt::Debug for Message {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Message")
            .field("key", &self.inner.key)
            .finish()
    }
}
