//! A locale file's messages, parsed: what `I18n` shows, and what types and
//! lint read without a `Ui`.
use super::message::{Argument, Pattern};
use crate::theme::json::{self, Json};
use std::rc::Rc;

/// One locale's messages, by their dotted keys (`{"inbox": {"count": …}}`
/// is `inbox.count`), in the order the file has them.
#[derive(Default)]
pub struct Catalog {
    pub(crate) messages: Vec<(Rc<str>, Rc<Pattern>)>,
    /// Each message's text, to tell a reload what changed.
    pub(crate) texts: Vec<Rc<str>>,
    /// Where each key is in `messages`.
    index: rustc_hash::FxHashMap<Rc<str>, usize>,
}

/// What is wrong with a locale file or one of its messages.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct I18nError {
    /// The locale file's locale.
    pub locale: String,
    /// The message's key; empty for the file as a whole.
    pub key: String,
    /// Where in the message (1-based), for a message that doesn't parse;
    /// in the file, for a file that isn't JSON; else 0.
    pub line: usize,
    /// The column of that line (1-based).
    pub column: usize,
    /// What is wrong, in words.
    pub message: String,
}

impl std::fmt::Display for I18nError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.locale)?;
        if !self.key.is_empty() {
            write!(f, " {}", self.key)?;
        }
        if self.line > 0 {
            write!(f, " {}:{}", self.line, self.column)?;
        }
        write!(f, ": {}", self.message)
    }
}
impl std::error::Error for I18nError {}

impl Catalog {
    /// Parse a locale file (JSON, nested keys flattened). Messages that
    /// don't parse are left out and reported, the rest kept: the catalog
    /// and the errors, if any.
    pub fn parse(locale: &str, text: &str) -> (Catalog, Vec<I18nError>) {
        let mut catalog = Catalog::default();
        let mut errors = Vec::new();
        let error = |key: &str, line, column, message: String| I18nError {
            locale: locale.to_owned(),
            key: key.to_owned(),
            line,
            column,
            message,
        };
        let root = match json::parse(text) {
            Ok(root) => root,
            Err(message) => {
                // `line:column: …`, as the theme reader says it.
                let mut parts = message.splitn(3, ':');
                let (line, column) = match (parts.next(), parts.next()) {
                    (Some(l), Some(c)) => {
                        (l.trim().parse().unwrap_or(0), c.trim().parse().unwrap_or(0))
                    }
                    _ => (0, 0),
                };
                errors.push(error("", line, column, message));
                return (catalog, errors);
            }
        };
        let mut todo = vec![(String::new(), &root)];
        let mut found = Vec::new();
        while let Some((prefix, value)) = todo.pop() {
            match value {
                Json::Object(members) => {
                    // Reversed onto the stack: taken in the file's order.
                    for (key, value) in members.iter().rev() {
                        let key = if prefix.is_empty() {
                            key.clone()
                        } else {
                            format!("{prefix}.{key}")
                        };
                        todo.push((key, value));
                    }
                }
                Json::String(text) => found.push((prefix, text.as_str())),
                _ if prefix.is_empty() => {
                    errors.push(error("", 0, 0, "a locale file is an object".to_owned()))
                }
                _ => errors.push(error(&prefix, 0, 0, "a message is a string".to_owned())),
            }
        }
        for (key, text) in found {
            match Pattern::parse(text) {
                Ok(pattern) => {
                    let key: Rc<str> = key.into();
                    // A key given twice (`"a.b"` and `{"a": {"b"}}`): the last.
                    match catalog.index.get(&key) {
                        Some(&at) => {
                            catalog.messages[at].1 = Rc::new(pattern);
                            catalog.texts[at] = text.into();
                        }
                        None => {
                            catalog.index.insert(key.clone(), catalog.messages.len());
                            catalog.messages.push((key, Rc::new(pattern)));
                            catalog.texts.push(text.into());
                        }
                    }
                }
                Err(e) => errors.push(error(&key, e.line, e.column, e.message)),
            }
        }
        (catalog, errors)
    }

    /// Its keys and the arguments each message names.
    pub fn keys(&self) -> impl Iterator<Item = (&str, Vec<Argument>)> {
        self.messages
            .iter()
            .map(|(key, pattern)| (&**key, pattern.arguments()))
    }

    /// How many messages it has.
    pub fn len(&self) -> usize {
        self.messages.len()
    }

    /// Whether it has none.
    pub fn is_empty(&self) -> bool {
        self.messages.is_empty()
    }

    pub(crate) fn get(&self, key: &str) -> Option<(&Rc<Pattern>, &Rc<str>)> {
        let at = *self.index.get(key)?;
        Some((&self.messages[at].1, &self.texts[at]))
    }
}
