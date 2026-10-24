//! Bounded keyboard-context predicate parser. Context stack is root to focus.
use std::{collections::BTreeMap, fmt};
/// One level of the keyboard context stack: the flags and `name == value`
/// attributes a component sets for key bindings' predicates (see
/// [`ContextPredicate`]).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct KeyContext {
    pub(crate) entries: BTreeMap<String, Option<String>>,
}
impl KeyContext {
    /// An empty context.
    pub fn new() -> Self {
        Self::default()
    }
    /// Set the flag `name`.
    pub fn flag(mut self, name: impl Into<String>) -> Self {
        self.entries.insert(name.into(), None);
        self
    }
    /// Set the attribute `name` to `value`, which `name == value` predicates match.
    pub fn attribute(mut self, name: impl Into<String>, value: impl Into<String>) -> Self {
        self.entries.insert(name.into(), Some(value.into()));
        self
    }
    pub(crate) fn extend(&mut self, other: &Self) {
        self.entries.extend(other.entries.clone());
    }
}
/// A condition on the keyboard context stack under which a key binding applies,
/// parsed from text such as `Workspace > (Editor && mode == insert)`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ContextPredicate {
    /// A context has `name`, as a flag or an attribute (`Editor`).
    Flag(String),
    /// A context's attribute (first) is the value (second) (`mode == insert`).
    Equal(String, String),
    /// A context's attribute (first) is not the value (second), or is unset (`mode
    /// != normal`).
    NotEqual(String, String),
    /// No context in the stack matches it (`!Terminal`).
    Not(Box<Self>),
    /// Both match the same context (`a && b`).
    And(Box<Self>, Box<Self>),
    /// Either matches (`a || b`).
    Or(Box<Self>, Box<Self>),
    /// The second matches a context that has an ancestor the first matches, at any
    /// depth (`Workspace > Editor`).
    Descendant(Box<Self>, Box<Self>),
}
/// Why a context predicate did not parse, and where.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ContextParseError {
    /// The UTF-8 byte offset where parsing failed.
    pub offset: usize,
    /// What was wrong.
    pub message: &'static str,
}
impl fmt::Display for ContextParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} at byte {}", self.message, self.offset)
    }
}
impl std::error::Error for ContextParseError {}
impl ContextPredicate {
    /// Parse identifiers, ==, !=, !, &&, ||, > and parentheses. Input is limited
    /// to 4096 bytes and 64 nested expressions to keep configuration work bounded.
    pub fn parse(source: &str) -> Result<Self, ContextParseError> {
        let mut parser = Parser { source, offset: 0 };
        if source.len() > 4096 {
            return Err(parser.error("context predicate exceeds 4096 bytes"));
        }
        let result = parser.expression(0, 0)?;
        parser.whitespace();
        if parser.offset != source.len() {
            return Err(parser.error("unexpected token"));
        }
        Ok(result)
    }
    /// Whether the predicate holds at any level of `stack`, which runs from the
    /// root to the focused component.
    pub fn matches(&self, stack: &[KeyContext]) -> bool {
        (0..stack.len())
            .rev()
            .any(|index| self.at(stack, index, stack))
    }
    fn at(&self, stack: &[KeyContext], index: usize, all: &[KeyContext]) -> bool {
        let Some(context) = stack.get(index) else {
            return false;
        };
        match self {
            Self::Flag(name) => context.entries.contains_key(name),
            Self::Equal(name, value) => {
                context.entries.get(name).and_then(Option::as_ref) == Some(value)
            }
            Self::NotEqual(name, value) => {
                context.entries.get(name).and_then(Option::as_ref) != Some(value)
            }
            Self::Not(predicate) => !(0..all.len()).any(|i| predicate.at(all, i, all)),
            Self::And(a, b) => a.at(stack, index, all) && b.at(stack, index, all),
            Self::Or(a, b) => a.at(stack, index, all) || b.at(stack, index, all),
            Self::Descendant(parent, child) => (0..index).any(|ancestor| {
                parent.at(stack, ancestor, all)
                    && child.at(
                        &stack[ancestor + 1..=index],
                        index - ancestor - 1,
                        &stack[ancestor + 1..=index],
                    )
            }),
        }
    }
}
struct Parser<'a> {
    source: &'a str,
    offset: usize,
}
impl Parser<'_> {
    fn error(&self, message: &'static str) -> ContextParseError {
        ContextParseError {
            offset: self.offset,
            message,
        }
    }
    fn whitespace(&mut self) {
        while let Some(c) = self.source[self.offset..].chars().next() {
            if !c.is_whitespace() {
                break;
            }
            self.offset += c.len_utf8();
        }
    }
    fn consume(&mut self, value: &str) -> bool {
        self.whitespace();
        if self.source[self.offset..].starts_with(value) {
            self.offset += value.len();
            true
        } else {
            false
        }
    }
    fn identifier(&mut self) -> Result<String, ContextParseError> {
        self.whitespace();
        let start = self.offset;
        while let Some(c) = self.source[self.offset..].chars().next() {
            if !(c.is_alphanumeric() || matches!(c, '_' | '-' | '.' | '/')) {
                break;
            }
            self.offset += c.len_utf8();
        }
        if self.offset == start {
            Err(self.error("expected identifier"))
        } else {
            Ok(self.source[start..self.offset].into())
        }
    }
    fn expression(&mut self, min: u8, depth: usize) -> Result<ContextPredicate, ContextParseError> {
        if depth >= 64 {
            return Err(self.error("context nesting exceeds 64"));
        }
        let mut left = if self.consume("!") {
            ContextPredicate::Not(Box::new(self.expression(4, depth + 1)?))
        } else if self.consume("(") {
            let expr = self.expression(0, depth + 1)?;
            if !self.consume(")") {
                return Err(self.error("expected closing parenthesis"));
            }
            expr
        } else {
            let name = self.identifier()?;
            if self.consume("==") {
                ContextPredicate::Equal(name, self.identifier()?)
            } else if self.consume("!=") {
                ContextPredicate::NotEqual(name, self.identifier()?)
            } else {
                ContextPredicate::Flag(name)
            }
        };
        loop {
            self.whitespace();
            let tail = &self.source[self.offset..];
            let (operator, precedence) = if tail.starts_with("||") {
                ("||", 2)
            } else if tail.starts_with("&&") {
                ("&&", 3)
            } else if tail.starts_with('>') {
                (">", 1)
            } else {
                break;
            };
            if precedence < min {
                break;
            }
            self.offset += operator.len();
            let right = Box::new(self.expression(precedence + 1, depth + 1)?);
            let a = Box::new(left);
            left = match operator {
                "||" => ContextPredicate::Or(a, right),
                "&&" => ContextPredicate::And(a, right),
                _ => ContextPredicate::Descendant(a, right),
            };
        }
        Ok(left)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn scoped_attributes_operators_and_descendant_matching() {
        let stack = [
            KeyContext::new().flag("Workspace"),
            KeyContext::new().flag("Editor").attribute("mode", "insert"),
        ];
        for source in [
            "Workspace > (Editor && mode == insert)",
            "!Terminal && Editor",
            "mode != normal && Editor",
            "Terminal || Editor",
        ] {
            assert!(
                ContextPredicate::parse(source).unwrap().matches(&stack),
                "{source}"
            );
        }
        for source in [
            "Workspace && Editor",
            "Editor > Workspace",
            "!Workspace",
            "Editor && mode == normal",
        ] {
            assert!(
                !ContextPredicate::parse(source).unwrap().matches(&stack),
                "{source}"
            );
        }
    }
    #[test]
    fn malformed_and_excessive_predicates_fail() {
        for source in [
            "",
            "Editor &&",
            "Editor = insert",
            "(Editor",
            "Editor)",
            "!!",
            "Editor > > Terminal",
        ] {
            assert!(ContextPredicate::parse(source).is_err(), "{source}");
        }
        assert!(ContextPredicate::parse(&"!".repeat(65)).is_err());
        assert!(ContextPredicate::parse(&"a".repeat(4097)).is_err());
    }
}
