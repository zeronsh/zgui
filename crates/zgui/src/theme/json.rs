//! A small JSON reader for theme files. It also takes what VS Code's theme
//! files use beyond JSON: `//` and `/* */` comments and trailing commas.
//! Object members keep their order.

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Json {
    Null,
    Bool(bool),
    Number(f64),
    String(String),
    Array(Vec<Json>),
    Object(Vec<(String, Json)>),
}
impl Json {
    pub(crate) fn get(&self, key: &str) -> Option<&Json> {
        match self {
            Json::Object(members) => members.iter().find(|(k, _)| k == key).map(|(_, v)| v),
            _ => None,
        }
    }
    pub(crate) fn as_str(&self) -> Option<&str> {
        match self {
            Json::String(s) => Some(s),
            _ => None,
        }
    }
}

/// Parse `text`, or describe where it stops being JSON (`line:column`).
pub(crate) fn parse(text: &str) -> Result<Json, String> {
    let mut parser = Parser {
        text: text.as_bytes(),
        at: 0,
        depth: 0,
    };
    let value = parser.value()?;
    parser.space()?;
    if parser.at < parser.text.len() {
        return Err(parser.error("trailing characters"));
    }
    Ok(value)
}

/// How deeply arrays and objects nest at most, as serde_json's limit: a
/// theme nests a few levels, and a file nesting thousands would overflow
/// the stack reading it.
const DEPTH: u32 = 128;

struct Parser<'a> {
    text: &'a [u8],
    at: usize,
    depth: u32,
}
impl Parser<'_> {
    fn error(&self, what: &str) -> String {
        let before = &self.text[..self.at.min(self.text.len())];
        let line = before.iter().filter(|&&b| b == b'\n').count() + 1;
        let column = before.iter().rev().take_while(|&&b| b != b'\n').count() + 1;
        format!("{line}:{column}: {what}")
    }
    fn peek(&self) -> Option<u8> {
        self.text.get(self.at).copied()
    }
    /// Skip whitespace and comments.
    fn space(&mut self) -> Result<(), String> {
        loop {
            match self.peek() {
                Some(b' ' | b'\t' | b'\n' | b'\r') => self.at += 1,
                Some(b'/') if self.text.get(self.at + 1) == Some(&b'/') => {
                    while self.peek().is_some_and(|b| b != b'\n') {
                        self.at += 1;
                    }
                }
                Some(b'/') if self.text.get(self.at + 1) == Some(&b'*') => {
                    self.at += 2;
                    loop {
                        match self.peek() {
                            None => return Err(self.error("an unclosed comment")),
                            Some(b'*') if self.text.get(self.at + 1) == Some(&b'/') => {
                                self.at += 2;
                                break;
                            }
                            _ => self.at += 1,
                        }
                    }
                }
                _ => return Ok(()),
            }
        }
    }
    fn expect(&mut self, byte: u8) -> Result<(), String> {
        self.space()?;
        if self.peek() == Some(byte) {
            self.at += 1;
            Ok(())
        } else {
            Err(self.error(&format!("expected `{}`", byte as char)))
        }
    }
    fn value(&mut self) -> Result<Json, String> {
        if self.depth == DEPTH {
            return Err(self.error("nested too deeply"));
        }
        self.depth += 1;
        let value = self.nested();
        self.depth -= 1;
        value
    }
    /// A value, inside `depth` arrays and objects.
    fn nested(&mut self) -> Result<Json, String> {
        self.space()?;
        match self.peek() {
            Some(b'{') => {
                self.at += 1;
                let mut members = Vec::new();
                loop {
                    self.space()?;
                    if self.peek() == Some(b'}') {
                        self.at += 1;
                        return Ok(Json::Object(members));
                    }
                    let key = self.string()?;
                    self.expect(b':')?;
                    let value = self.value()?;
                    members.push((key, value));
                    self.space()?;
                    match self.peek() {
                        Some(b',') => self.at += 1,
                        Some(b'}') => {}
                        _ => return Err(self.error("expected `,` or `}`")),
                    }
                }
            }
            Some(b'[') => {
                self.at += 1;
                let mut items = Vec::new();
                loop {
                    self.space()?;
                    if self.peek() == Some(b']') {
                        self.at += 1;
                        return Ok(Json::Array(items));
                    }
                    items.push(self.value()?);
                    self.space()?;
                    match self.peek() {
                        Some(b',') => self.at += 1,
                        Some(b']') => {}
                        _ => return Err(self.error("expected `,` or `]`")),
                    }
                }
            }
            Some(b'"') => self.string().map(Json::String),
            Some(b't') => self.word("true", Json::Bool(true)),
            Some(b'f') => self.word("false", Json::Bool(false)),
            Some(b'n') => self.word("null", Json::Null),
            Some(b'-' | b'0'..=b'9') => {
                let start = self.at;
                while self
                    .peek()
                    .is_some_and(|b| matches!(b, b'-' | b'+' | b'.' | b'e' | b'E' | b'0'..=b'9'))
                {
                    self.at += 1;
                }
                std::str::from_utf8(&self.text[start..self.at])
                    .ok()
                    .and_then(|n| n.parse().ok())
                    .map(Json::Number)
                    .ok_or_else(|| self.error("a malformed number"))
            }
            _ => Err(self.error("expected a value")),
        }
    }
    fn word(&mut self, word: &str, value: Json) -> Result<Json, String> {
        if self.text[self.at..].starts_with(word.as_bytes()) {
            self.at += word.len();
            Ok(value)
        } else {
            Err(self.error("expected a value"))
        }
    }
    fn string(&mut self) -> Result<String, String> {
        self.space()?;
        if self.peek() != Some(b'"') {
            return Err(self.error("expected a string"));
        }
        self.at += 1;
        let mut out = Vec::new();
        loop {
            match self.peek() {
                None => return Err(self.error("an unclosed string")),
                Some(b'"') => {
                    self.at += 1;
                    break;
                }
                Some(b'\\') => {
                    let escape = self.text.get(self.at + 1).copied();
                    self.at += 2;
                    match escape {
                        Some(b'n') => out.push(b'\n'),
                        Some(b't') => out.push(b'\t'),
                        Some(b'r') => out.push(b'\r'),
                        Some(b'b') => out.push(8),
                        Some(b'f') => out.push(12),
                        Some(b'u') => {
                            let hex = self
                                .text
                                .get(self.at..self.at + 4)
                                .and_then(|h| std::str::from_utf8(h).ok())
                                .and_then(|h| u32::from_str_radix(h, 16).ok())
                                .ok_or_else(|| self.error("a malformed \\u escape"))?;
                            self.at += 4;
                            let c = char::from_u32(hex).unwrap_or('\u{fffd}');
                            out.extend_from_slice(c.encode_utf8(&mut [0; 4]).as_bytes());
                        }
                        Some(other) => out.push(other),
                        None => return Err(self.error("an unclosed string")),
                    }
                }
                Some(byte) => {
                    out.push(byte);
                    self.at += 1;
                }
            }
        }
        String::from_utf8(out).map_err(|_| self.error("a string that is not UTF-8"))
    }
}

/// Write `text` as a JSON string literal.
pub(crate) fn quote(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 2);
    out.push('"');
    for c in text.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn reads_json_with_comments_and_trailing_commas() {
        let value = parse(
            r#"{
                // a comment
                "name": "Test \"theme\"", /* another */
                "list": [1, -2.5e1, true, null,],
                "nested": { "a": "é", },
            }"#,
        )
        .unwrap();
        assert_eq!(
            value.get("name").and_then(Json::as_str),
            Some("Test \"theme\"")
        );
        assert_eq!(
            value.get("list"),
            Some(&Json::Array(vec![
                Json::Number(1.),
                Json::Number(-25.),
                Json::Bool(true),
                Json::Null
            ]))
        );
        assert_eq!(
            value
                .get("nested")
                .and_then(|n| n.get("a"))
                .and_then(Json::as_str),
            Some("é")
        );
        let error = parse("{\n  \"a\": ,\n}").unwrap_err();
        assert!(error.starts_with("2:8"), "{error}");
        assert_eq!(quote("a\"b"), r#""a\"b""#);
    }
    #[test]
    fn deep_nesting_is_an_error_not_a_stack_overflow() {
        let nested = |n| format!("{}{}", "[".repeat(n), "]".repeat(n));
        assert!(parse(&nested(DEPTH as usize)).is_ok());
        let error = parse(&nested(DEPTH as usize + 1)).unwrap_err();
        assert!(error.ends_with("nested too deeply"), "{error}");
        assert!(parse(&"{\"a\":".repeat(100_000)).is_err());
    }
}
