//! ICU MessageFormat, the part apps use: `{name}`, `{n, number}`,
//! `{d, date, short|medium|long|full}`, `{n, plural, …}` (with `offset:`,
//! `=0` and `#`), `{n, selectordinal, …}` and `{x, select, …}`. Apostrophes
//! quote as ICU's do: `''` is one, and `'{…}'` is literal text.
use std::rc::Rc;

/// A parsed message.
#[derive(Debug, PartialEq)]
pub struct Pattern {
    parts: Vec<Part>,
    /// Whether formatting reads the locale (a number, date or plural).
    pub(crate) by_locale: bool,
}

#[derive(Debug, PartialEq)]
enum Part {
    Text(Box<str>),
    /// `{name}`: a string as it is, a number formatted.
    Arg(Box<str>),
    Number(Box<str>),
    Date(Box<str>, DateLength),
    Plural {
        name: Box<str>,
        ordinal: bool,
        offset: f64,
        cases: Vec<(Case, Pattern)>,
    },
    Select {
        name: Box<str>,
        cases: Vec<(Box<str>, Pattern)>,
    },
    /// `#` in a plural: its number less the offset.
    Hash,
}

#[derive(Debug, PartialEq)]
enum Case {
    Exact(f64),
    Category(Box<str>),
}

/// A date's length (`{d, date, medium}`; medium if none is given).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum DateLength {
    /// Numeric (`10/28/26`).
    Short,
    /// Abbreviated (`Oct 28, 2026`).
    Medium,
    /// Spelled out (`October 28, 2026`).
    Long,
    /// Spelled out (as long, for now).
    Full,
}

/// What a message's argument is used as, for types and lint.
#[derive(Clone, Debug, PartialEq)]
pub enum ArgumentKind {
    /// `{name}`: any text or number.
    Plain,
    /// `{n, number}`.
    Number,
    /// `{n, plural, …}`, with the categories and exact values it has.
    Plural(Vec<String>),
    /// `{n, selectordinal, …}`.
    Ordinal(Vec<String>),
    /// `{x, select, …}`, with its cases.
    Select(Vec<String>),
    /// `{d, date, …}`: milliseconds since 1970.
    Date,
}

/// An argument a message names, and how it uses it.
#[derive(Clone, Debug, PartialEq)]
pub struct Argument {
    /// Its name in the message.
    pub name: String,
    /// How the message uses it.
    pub kind: ArgumentKind,
}

/// Where a message stops parsing: 1-based line and column within it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ParseError {
    /// The line (1-based).
    pub line: usize,
    /// The column of that line (1-based).
    pub column: usize,
    /// What is wrong, in words.
    pub message: String,
}

impl Pattern {
    /// Parse a message.
    pub fn parse(text: &str) -> Result<Pattern, ParseError> {
        let mut parser = Parser {
            text,
            at: 0,
            plural_depth: 0,
        };
        let pattern = parser.pattern(false)?;
        if parser.at < text.len() {
            return Err(parser.error("an unmatched '}'"));
        }
        Ok(pattern)
    }

    /// The arguments it names, once each, in order of first use.
    pub fn arguments(&self) -> Vec<Argument> {
        let mut found: Vec<Argument> = Vec::new();
        self.collect(&mut found);
        found
    }
    fn collect(&self, found: &mut Vec<Argument>) {
        let mut add = |name: &str, kind: ArgumentKind| {
            match found.iter_mut().find(|a| a.name == name) {
                // A plain use says less than any other.
                Some(existing) if existing.kind == ArgumentKind::Plain => existing.kind = kind,
                Some(_) => {}
                None => found.push(Argument {
                    name: name.to_owned(),
                    kind,
                }),
            }
        };
        for part in &self.parts {
            match part {
                Part::Arg(name) => add(name, ArgumentKind::Plain),
                Part::Number(name) => add(name, ArgumentKind::Number),
                Part::Date(name, _) => add(name, ArgumentKind::Date),
                Part::Plural {
                    name,
                    ordinal,
                    cases,
                    ..
                } => {
                    let names = cases
                        .iter()
                        .map(|(case, _)| match case {
                            Case::Exact(n) => format!("={n}"),
                            Case::Category(c) => c.to_string(),
                        })
                        .collect();
                    add(
                        name,
                        if *ordinal {
                            ArgumentKind::Ordinal(names)
                        } else {
                            ArgumentKind::Plural(names)
                        },
                    );
                }
                Part::Select { name, cases } => add(
                    name,
                    ArgumentKind::Select(cases.iter().map(|(c, _)| c.to_string()).collect()),
                ),
                Part::Text(_) | Part::Hash => {}
            }
        }
        for part in &self.parts {
            match part {
                Part::Plural { cases, .. } => cases.iter().for_each(|(_, p)| p.collect(found)),
                Part::Select { cases, .. } => cases.iter().for_each(|(_, p)| p.collect(found)),
                _ => {}
            }
        }
    }

    /// The text, its arguments looked up by `arg` (`None`: shown as
    /// `{name}`, and told to `missing`).
    pub(crate) fn format(
        &self,
        out: &mut String,
        locale: &super::format::Formatters,
        arg: &mut dyn FnMut(&str) -> Option<super::ArgValue>,
        missing: &mut dyn FnMut(&str),
        hash: Option<f64>,
    ) {
        use super::ArgValue;
        for part in &self.parts {
            match part {
                Part::Text(text) => out.push_str(text),
                Part::Hash => match hash {
                    Some(n) => locale.number(out, n),
                    None => out.push('#'),
                },
                Part::Arg(name) | Part::Number(name) | Part::Date(name, _) => {
                    match (arg(name), part) {
                        (Some(ArgValue::Number(n)), Part::Date(_, length)) => {
                            locale.date(out, n, *length)
                        }
                        (Some(ArgValue::Date(ms)), Part::Date(_, length)) => {
                            locale.date(out, ms, *length)
                        }
                        (Some(ArgValue::Number(n)), _) => locale.number(out, n),
                        (Some(ArgValue::Date(ms)), _) => locale.date(out, ms, DateLength::Medium),
                        (Some(ArgValue::Text(text)), _) => out.push_str(&text),
                        (None, _) => {
                            missing(name);
                            out.push('{');
                            out.push_str(name);
                            out.push('}');
                        }
                    }
                }
                Part::Plural {
                    name,
                    ordinal,
                    offset,
                    cases,
                } => {
                    let n = match arg(name) {
                        Some(ArgValue::Number(n) | ArgValue::Date(n)) => n,
                        Some(ArgValue::Text(text)) => text.trim().parse().unwrap_or(f64::NAN),
                        None => {
                            missing(name);
                            f64::NAN
                        }
                    };
                    let exact = cases
                        .iter()
                        .find(|(case, _)| matches!(case, Case::Exact(e) if *e == n));
                    let chosen = exact.or_else(|| {
                        let category = locale.plural(n - offset, *ordinal);
                        cases
                            .iter()
                            .find(|(case, _)| matches!(case, Case::Category(c) if **c == *category))
                    });
                    let chosen = chosen.or_else(|| {
                        cases
                            .iter()
                            .find(|(case, _)| matches!(case, Case::Category(c) if &**c == "other"))
                    });
                    if let Some((_, pattern)) = chosen {
                        pattern.format(out, locale, arg, missing, Some(n - offset));
                    }
                }
                Part::Select { name, cases } => {
                    let value = match arg(name) {
                        Some(ArgValue::Text(text)) => Some(text),
                        Some(ArgValue::Number(n) | ArgValue::Date(n)) => {
                            Some(Rc::from(n.to_string()))
                        }
                        None => {
                            missing(name);
                            None
                        }
                    };
                    let chosen = value
                        .and_then(|v| cases.iter().find(|(case, _)| **case == *v))
                        .or_else(|| cases.iter().find(|(case, _)| &**case == "other"));
                    if let Some((_, pattern)) = chosen {
                        pattern.format(out, locale, arg, missing, hash);
                    }
                }
            }
        }
    }
}

struct Parser<'a> {
    text: &'a str,
    at: usize,
    /// Inside a plural's case: `#` is its number there.
    plural_depth: usize,
}

impl<'a> Parser<'a> {
    fn error(&self, message: &str) -> ParseError {
        let before = &self.text[..self.at.min(self.text.len())];
        let line = before.matches('\n').count() + 1;
        let column = before.rsplit('\n').next().map_or(0, |l| l.chars().count()) + 1;
        ParseError {
            line,
            column,
            message: message.to_owned(),
        }
    }
    fn peek(&self) -> Option<char> {
        self.text[self.at..].chars().next()
    }
    fn bump(&mut self) -> Option<char> {
        let c = self.peek()?;
        self.at += c.len_utf8();
        Some(c)
    }
    fn skip_space(&mut self) {
        while self.peek().is_some_and(char::is_whitespace) {
            self.bump();
        }
    }
    fn expect(&mut self, c: char) -> Result<(), ParseError> {
        self.skip_space();
        if self.peek() == Some(c) {
            self.bump();
            Ok(())
        } else {
            Err(self.error(&format!("expected '{c}'")))
        }
    }
    /// A name or keyword: up to a space, comma or brace.
    fn word(&mut self) -> Result<&'a str, ParseError> {
        self.skip_space();
        let start = self.at;
        while self
            .peek()
            .is_some_and(|c| !c.is_whitespace() && !matches!(c, ',' | '{' | '}'))
        {
            self.bump();
        }
        if self.at == start {
            return Err(self.error("expected a name"));
        }
        Ok(&self.text[start..self.at])
    }

    /// Text and arguments up to a `}` this pattern doesn't own (`nested`)
    /// or the end.
    fn pattern(&mut self, nested: bool) -> Result<Pattern, ParseError> {
        let mut parts = Vec::new();
        let mut text = String::new();
        let mut by_locale = false;
        let flush = |text: &mut String, parts: &mut Vec<Part>| {
            if !text.is_empty() {
                parts.push(Part::Text(std::mem::take(text).into()));
            }
        };
        while let Some(c) = self.peek() {
            match c {
                '}' => break,
                '{' => {
                    flush(&mut text, &mut parts);
                    let part = self.argument()?;
                    by_locale |= match &part {
                        Part::Number(_) | Part::Date(..) | Part::Plural { .. } => true,
                        Part::Select { cases, .. } => cases.iter().any(|(_, p)| p.by_locale),
                        // A plain argument may be given a number.
                        Part::Arg(_) => true,
                        Part::Text(_) | Part::Hash => false,
                    };
                    parts.push(part);
                }
                '#' if self.plural_depth > 0 => {
                    self.bump();
                    flush(&mut text, &mut parts);
                    parts.push(Part::Hash);
                    by_locale = true;
                }
                '\'' => {
                    self.bump();
                    match self.peek() {
                        Some('\'') => {
                            self.bump();
                            text.push('\'');
                        }
                        Some('{' | '}' | '#' | '|') => {
                            // Quoted up to the next lone apostrophe.
                            while let Some(q) = self.bump() {
                                if q == '\'' {
                                    if self.peek() == Some('\'') {
                                        self.bump();
                                        text.push('\'');
                                    } else {
                                        break;
                                    }
                                } else {
                                    text.push(q);
                                }
                            }
                        }
                        _ => text.push('\''),
                    }
                }
                _ => {
                    self.bump();
                    text.push(c);
                }
            }
        }
        if !nested && self.peek() == Some('}') {
            return Err(self.error("an unmatched '}'"));
        }
        flush(&mut text, &mut parts);
        Ok(Pattern { parts, by_locale })
    }

    /// `{…}`, from its `{`.
    fn argument(&mut self) -> Result<Part, ParseError> {
        self.bump();
        let name: Box<str> = self.word()?.into();
        self.skip_space();
        match self.bump() {
            Some('}') => return Ok(Part::Arg(name)),
            Some(',') => {}
            _ => return Err(self.error("expected ',' or '}'")),
        }
        let kind = self.word()?.to_owned();
        self.skip_space();
        let part = match kind.as_str() {
            "number" => {
                if self.peek() == Some(',') {
                    // A style (integer, percent…): formatted as a number.
                    self.bump();
                    self.word()?;
                }
                Part::Number(name)
            }
            "date" => {
                let mut length = DateLength::Medium;
                if self.peek() == Some(',') {
                    self.bump();
                    length = match self.word()? {
                        "short" => DateLength::Short,
                        "medium" => DateLength::Medium,
                        "long" => DateLength::Long,
                        "full" => DateLength::Full,
                        _ => return Err(self.error("a date is short, medium, long or full")),
                    };
                }
                Part::Date(name, length)
            }
            "plural" | "selectordinal" => {
                self.expect(',')?;
                self.skip_space();
                let mut offset = 0.;
                if self.text[self.at..].starts_with("offset:") {
                    self.at += "offset:".len();
                    offset = self
                        .word()?
                        .parse()
                        .map_err(|_| self.error("an offset is a number"))?;
                }
                let mut cases = Vec::new();
                self.plural_depth += 1;
                loop {
                    self.skip_space();
                    if self.peek() == Some('}') {
                        break;
                    }
                    let key = self.word()?.to_owned();
                    let case = match key.strip_prefix('=') {
                        Some(n) => Case::Exact(
                            n.parse()
                                .map_err(|_| self.error("an exact case is '=' and a number"))?,
                        ),
                        None => Case::Category(key.into()),
                    };
                    self.expect('{')?;
                    let pattern = self.pattern(true)?;
                    self.expect('}')?;
                    cases.push((case, pattern));
                }
                self.plural_depth -= 1;
                if !cases
                    .iter()
                    .any(|(c, _)| matches!(c, Case::Category(c) if &**c == "other"))
                {
                    return Err(self.error("a plural needs an 'other' case"));
                }
                Part::Plural {
                    name,
                    ordinal: kind == "selectordinal",
                    offset,
                    cases,
                }
            }
            "select" => {
                self.expect(',')?;
                let mut cases = Vec::new();
                // `#` inside a select inside a plural is still the plural's.
                loop {
                    self.skip_space();
                    if self.peek() == Some('}') {
                        break;
                    }
                    let key: Box<str> = self.word()?.into();
                    self.expect('{')?;
                    let pattern = self.pattern(true)?;
                    self.expect('}')?;
                    cases.push((key, pattern));
                }
                if !cases.iter().any(|(c, _)| &**c == "other") {
                    return Err(self.error("a select needs an 'other' case"));
                }
                Part::Select { name, cases }
            }
            _ => {
                return Err(self.error("expected number, date, plural, selectordinal or select"));
            }
        };
        self.expect('}')?;
        Ok(part)
    }
}
