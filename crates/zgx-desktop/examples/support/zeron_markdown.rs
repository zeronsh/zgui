//! Markdown for agent replies, parsed into blocks that remember where their
//! text sits in the source. Streaming fades (the "veil") key on source byte
//! offsets, so every rendered piece of text must map back to its source.
//! Also: a small syntax highlighter for code blocks and a line diff for edits.

use std::ops::Range;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Align {
    Left,
    Center,
    Right,
}

/// A run of source text: `text` is exactly `source[start..start + text.len()]`
/// except that line breaks inside paragraphs read as spaces.
#[derive(Clone, PartialEq, Debug)]
pub struct Run {
    pub text: String,
    pub start: usize,
}

#[derive(Clone, PartialEq, Debug)]
pub enum Block {
    Para(Run),
    Heading(u8, Run),
    /// `depth` counts nesting from 0; `marker` is "•" or "3."; `task` is a
    /// checkbox state for `- [ ]` / `- [x]` items.
    Item {
        depth: usize,
        marker: String,
        task: Option<bool>,
        run: Run,
    },
    Quote(Run),
    Code {
        lang: String,
        run: Run,
        closed: bool,
    },
    Table {
        aligns: Vec<Align>,
        header: Vec<Run>,
        rows: Vec<Vec<Run>>,
    },
    Rule,
}

fn line_ranges(source: &str) -> Vec<Range<usize>> {
    let mut ranges = Vec::new();
    let mut start = 0;
    for (i, byte) in source.bytes().enumerate() {
        if byte == b'\n' {
            ranges.push(start..i);
            start = i + 1;
        }
    }
    if start <= source.len() {
        ranges.push(start..source.len());
    }
    ranges
}

fn table_cells(line: &str, offset: usize) -> Vec<Run> {
    let trimmed_start = line.len() - line.trim_start().len();
    let mut body = &line[trimmed_start..];
    let mut base = offset + trimmed_start;
    if let Some(rest) = body.strip_prefix('|') {
        body = rest;
        base += 1;
    }
    let body = body.trim_end();
    let body = body.strip_suffix('|').unwrap_or(body);
    let mut cells = Vec::new();
    let mut cell_start = 0;
    let bytes = body.as_bytes();
    let mut i = 0;
    while i <= bytes.len() {
        let end = i == bytes.len();
        // `\|` escapes a pipe inside a cell.
        if end || (bytes[i] == b'|' && (i == 0 || bytes[i - 1] != b'\\')) {
            let raw = &body[cell_start..i];
            let lead = raw.len() - raw.trim_start().len();
            let text = raw.trim();
            cells.push(Run {
                text: text.to_owned(),
                start: base + cell_start + lead,
            });
            cell_start = i + 1;
        }
        i += 1;
    }
    cells
}

fn is_separator(line: &str) -> Option<Vec<Align>> {
    let cells: Vec<&str> = line
        .trim()
        .trim_start_matches('|')
        .trim_end_matches('|')
        .split('|')
        .map(str::trim)
        .collect();
    if cells.is_empty() {
        return None;
    }
    cells
        .iter()
        .map(|cell| {
            let dashes = cell.trim_matches(':');
            (!dashes.is_empty() && dashes.bytes().all(|b| b == b'-')).then(|| {
                match (cell.starts_with(':'), cell.ends_with(':')) {
                    (true, true) => Align::Center,
                    (false, true) => Align::Right,
                    _ => Align::Left,
                }
            })
        })
        .collect()
}

/// List marker at the start of `line` (after indentation): returns
/// (marker display, byte length of marker incl. following space).
fn list_marker(line: &str) -> Option<(String, usize)> {
    let bytes = line.as_bytes();
    if bytes.len() >= 2 && matches!(bytes[0], b'-' | b'*' | b'+') && bytes[1] == b' ' {
        return Some(("•".into(), 2));
    }
    let digits = line.bytes().take_while(u8::is_ascii_digit).count();
    if (1..=3).contains(&digits)
        && bytes.len() > digits + 1
        && matches!(bytes[digits], b'.' | b')')
        && bytes[digits + 1] == b' '
    {
        return Some((format!("{}.", &line[..digits]), digits + 2));
    }
    None
}

pub fn parse(source: &str) -> Vec<Block> {
    let lines = line_ranges(source);
    let mut blocks = Vec::new();
    let mut para: Option<Run> = None;
    let flush = |para: &mut Option<Run>, blocks: &mut Vec<Block>| {
        if let Some(run) = para.take()
            && !run.text.trim().is_empty()
        {
            blocks.push(Block::Para(run));
        }
    };
    let mut i = 0;
    while i < lines.len() {
        let range = lines[i].clone();
        let line = &source[range.clone()];
        let indent = line.len() - line.trim_start().len();
        let trimmed = line.trim_start();
        // Fenced code, possibly still streaming (unclosed).
        if trimmed.starts_with("```") || trimmed.starts_with("~~~") {
            flush(&mut para, &mut blocks);
            let fence = &trimmed[..3];
            let lang = trimmed[3..].trim().to_owned();
            let body_start = lines.get(i + 1).map_or(source.len(), |r| r.start);
            let mut j = i + 1;
            let mut closed = false;
            while j < lines.len() {
                if source[lines[j].clone()].trim_start().starts_with(fence) {
                    closed = true;
                    break;
                }
                j += 1;
            }
            let body_end = if closed {
                lines[j].start.saturating_sub(1).max(body_start)
            } else {
                source.len()
            };
            blocks.push(Block::Code {
                lang,
                run: Run {
                    text: source[body_start..body_end].to_owned(),
                    start: body_start,
                },
                closed,
            });
            i = j + 1;
            continue;
        }
        if trimmed.is_empty() {
            flush(&mut para, &mut blocks);
            i += 1;
            continue;
        }
        // Tables: a pipe row followed by a separator row.
        if trimmed.contains('|')
            && let Some(next) = lines.get(i + 1)
            && let Some(aligns) = is_separator(&source[next.clone()])
        {
            flush(&mut para, &mut blocks);
            let header = table_cells(line, range.start);
            let mut rows = Vec::new();
            let mut j = i + 2;
            while j < lines.len() {
                let row_line = &source[lines[j].clone()];
                if !row_line.contains('|') || row_line.trim().is_empty() {
                    break;
                }
                rows.push(table_cells(row_line, lines[j].start));
                j += 1;
            }
            blocks.push(Block::Table {
                aligns,
                header,
                rows,
            });
            i = j;
            continue;
        }
        if para.is_none() {
            // Headings.
            let hashes = trimmed.bytes().take_while(|b| *b == b'#').count();
            if (1..=6).contains(&hashes) && trimmed.as_bytes().get(hashes) == Some(&b' ') {
                let offset = range.start + indent + hashes + 1;
                blocks.push(Block::Heading(
                    hashes.min(4) as u8,
                    Run {
                        text: source[offset..range.end].trim_end().to_owned(),
                        start: offset,
                    },
                ));
                i += 1;
                continue;
            }
            // Rules.
            let compact: String = trimmed.chars().filter(|c| !c.is_whitespace()).collect();
            if compact.len() >= 3
                && (compact.bytes().all(|b| b == b'-')
                    || compact.bytes().all(|b| b == b'*')
                    || compact.bytes().all(|b| b == b'_'))
            {
                blocks.push(Block::Rule);
                i += 1;
                continue;
            }
        }
        // List items (a new item also ends a paragraph).
        if let Some((marker, marker_len)) = list_marker(trimmed) {
            flush(&mut para, &mut blocks);
            let mut offset = range.start + indent + marker_len;
            let mut text = &source[offset..range.end];
            let task = if let Some(rest) = text.strip_prefix("[ ] ") {
                offset += 4;
                text = rest;
                Some(false)
            } else if let Some(rest) = text
                .strip_prefix("[x] ")
                .or_else(|| text.strip_prefix("[X] "))
            {
                offset += 4;
                text = rest;
                Some(true)
            } else {
                None
            };
            let mut run = Run {
                text: text.to_owned(),
                start: offset,
            };
            // Lazy continuation lines belong to the item.
            let mut j = i + 1;
            while j < lines.len() {
                let next = &source[lines[j].clone()];
                let next_trimmed = next.trim_start();
                if next_trimmed.is_empty()
                    || list_marker(next_trimmed).is_some()
                    || next_trimmed.starts_with("```")
                    || next.len() - next_trimmed.len() < 2
                {
                    break;
                }
                // The gap from the previous text reads as one space.
                let gap = lines[j].start + (next.len() - next_trimmed.len())
                    - (run.start + run.text.len());
                run.text.push_str(&" ".repeat(gap));
                run.text.push_str(next_trimmed);
                j += 1;
            }
            blocks.push(Block::Item {
                depth: indent / 2,
                marker: if task.is_some() {
                    String::new()
                } else {
                    marker
                },
                task,
                run,
            });
            i = j;
            continue;
        }
        if let Some(quote) = trimmed.strip_prefix('>') {
            flush(&mut para, &mut blocks);
            let skip = usize::from(quote.starts_with(' '));
            let offset = range.start + indent + 1 + skip;
            blocks.push(Block::Quote(Run {
                text: source[offset..range.end].to_owned(),
                start: offset,
            }));
            i += 1;
            continue;
        }
        // Paragraph text; line breaks read as spaces (same byte length).
        match &mut para {
            Some(run) => {
                let gap = range.start - (run.start + run.text.len());
                run.text.push_str(&" ".repeat(gap));
                run.text.push_str(line);
            }
            None => {
                para = Some(Run {
                    text: trimmed.to_owned(),
                    start: range.start + indent,
                });
            }
        }
        i += 1;
    }
    flush(&mut para, &mut blocks);
    blocks
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Style {
    Plain,
    Bold,
    Italic,
    BoldItalic,
    Strike,
    Code,
    Link,
}

/// An inline piece of a run, with its source offset.
#[derive(Clone, PartialEq, Debug)]
pub struct Piece {
    pub text: String,
    pub start: usize,
    pub style: Style,
}

/// Split a run into styled pieces: `code`, **bold**, *italic*, ***both***,
/// ~~strike~~ and [links](url) (shown as their text).
pub fn inline(run: &Run) -> Vec<Piece> {
    let text = &run.text;
    let mut pieces = Vec::new();
    let mut plain_start = 0;
    let mut i = 0;
    let push_plain = |pieces: &mut Vec<Piece>, from: usize, to: usize| {
        if to > from {
            pieces.push(Piece {
                text: text[from..to].to_owned(),
                start: run.start + from,
                style: Style::Plain,
            });
        }
    };
    let bytes = text.as_bytes();
    while i < bytes.len() {
        let rest = &text[i..];
        let mut matched: Option<(usize, usize, usize, Style)> = None; // (content start, content end, next i, style)
        if let Some(after) = rest.strip_prefix('`') {
            if let Some(end) = after.find('`') {
                matched = Some((i + 1, i + 1 + end, i + end + 2, Style::Code));
            }
        } else if let Some(after) = rest.strip_prefix("***") {
            if let Some(end) = after.find("***") {
                matched = Some((i + 3, i + 3 + end, i + end + 6, Style::BoldItalic));
            }
        } else if rest.starts_with("**") || rest.starts_with("__") {
            let marker = &rest[..2];
            if let Some(end) = rest[2..].find(marker)
                && end > 0
            {
                matched = Some((i + 2, i + 2 + end, i + end + 4, Style::Bold));
            }
        } else if let Some(after) = rest.strip_prefix("~~") {
            if let Some(end) = after.find("~~")
                && end > 0
            {
                matched = Some((i + 2, i + 2 + end, i + end + 4, Style::Strike));
            }
        } else if rest.starts_with('*')
            && !rest[1..].starts_with(' ')
            && (i == 0 || !bytes[i - 1].is_ascii_alphanumeric())
        {
            if let Some(end) = rest[1..].find('*')
                && end > 0
            {
                matched = Some((i + 1, i + 1 + end, i + end + 2, Style::Italic));
            }
        } else if rest.starts_with('[')
            && let Some(close) = rest.find("](")
            && let Some(paren) = rest[close..].find(')')
            && !rest[1..close].contains('\n')
        {
            matched = Some((i + 1, i + close, i + close + paren + 1, Style::Link));
        }
        match matched {
            Some((from, to, next, style)) => {
                push_plain(&mut pieces, plain_start, i);
                pieces.push(Piece {
                    text: text[from..to].to_owned(),
                    start: run.start + from,
                    style,
                });
                i = next;
                plain_start = next;
            }
            None => {
                i += rest.chars().next().map_or(1, char::len_utf8);
            }
        }
    }
    push_plain(&mut pieces, plain_start, bytes.len());
    pieces
}

// Syntax highlighting ---------------------------------------------------------

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Token {
    Text,
    Keyword,
    String,
    Number,
    Comment,
    Function,
    Type,
    Punct,
}

const KEYWORDS: &[&str] = &[
    "fn",
    "let",
    "mut",
    "pub",
    "use",
    "mod",
    "impl",
    "struct",
    "enum",
    "trait",
    "match",
    "if",
    "else",
    "for",
    "while",
    "loop",
    "return",
    "break",
    "continue",
    "in",
    "as",
    "where",
    "self",
    "Self",
    "crate",
    "super",
    "async",
    "await",
    "move",
    "ref",
    "const",
    "static",
    "type",
    "dyn",
    "unsafe",
    "true",
    "false",
    "None",
    "Some",
    "Ok",
    "Err",
    "function",
    "const",
    "var",
    "class",
    "export",
    "import",
    "from",
    "default",
    "new",
    "this",
    "null",
    "undefined",
    "interface",
    "extends",
    "implements",
    "def",
    "lambda",
    "pass",
    "and",
    "or",
    "not",
    "is",
    "with",
    "yield",
    "try",
    "except",
    "catch",
    "finally",
    "throw",
    "raise",
    "func",
    "package",
    "go",
    "defer",
    "chan",
    "select",
    "switch",
    "case",
    "do",
    "then",
    "fi",
    "done",
    "echo",
    "export",
    "local",
    "True",
    "False",
    "nil",
    "void",
    "int",
    "bool",
    "string",
    "struct",
    "typeof",
    "instanceof",
];

/// Split code into highlighted tokens (byte ranges into `code`).
pub fn highlight(code: &str, lang: &str) -> Vec<(Range<usize>, Token)> {
    let hash_comments = matches!(
        lang,
        "sh" | "bash"
            | "zsh"
            | "shell"
            | "console"
            | "python"
            | "py"
            | "toml"
            | "yaml"
            | "yml"
            | "ruby"
            | "rb"
    );
    let bytes = code.as_bytes();
    let mut tokens = Vec::new();
    let mut i = 0;
    let mut text_start = 0;
    let flush = |tokens: &mut Vec<(Range<usize>, Token)>, from: usize, to: usize| {
        if to > from {
            tokens.push((from..to, Token::Text));
        }
    };
    while i < bytes.len() {
        let c = bytes[i];
        let start = i;
        let token = if (c == b'/' && bytes.get(i + 1) == Some(&b'/'))
            || (c == b'#' && hash_comments)
            || (c == b'-' && bytes.get(i + 1) == Some(&b'-') && lang == "sql")
        {
            while i < bytes.len() && bytes[i] != b'\n' {
                i += 1;
            }
            Some(Token::Comment)
        } else if c == b'/' && bytes.get(i + 1) == Some(&b'*') {
            i += 2;
            while i + 1 < bytes.len() && !(bytes[i] == b'*' && bytes[i + 1] == b'/') {
                i += 1;
            }
            i = (i + 2).min(bytes.len());
            Some(Token::Comment)
        } else if c == b'"' || c == b'\'' || c == b'`' {
            // A lone apostrophe in Rust is usually a lifetime.
            if c == b'\''
                && lang == "rust"
                && bytes.get(i + 2) != Some(&b'\'')
                && bytes.get(i + 1) != Some(&b'\\')
            {
                i += 1;
                None
            } else {
                i += 1;
                while i < bytes.len() && bytes[i] != c && bytes[i] != b'\n' {
                    if bytes[i] == b'\\' {
                        i += 1;
                    }
                    i += 1;
                }
                i = (i + 1).min(bytes.len());
                Some(Token::String)
            }
        } else if c.is_ascii_digit() {
            while i < bytes.len()
                && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'_' || bytes[i] == b'.')
            {
                i += 1;
            }
            Some(Token::Number)
        } else if c.is_ascii_alphabetic() || c == b'_' {
            while i < bytes.len() && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'_') {
                i += 1;
            }
            let word = &code[start..i];
            if KEYWORDS.contains(&word) {
                Some(Token::Keyword)
            } else if bytes.get(i) == Some(&b'(') || (bytes.get(i) == Some(&b'!') && lang == "rust")
            {
                Some(Token::Function)
            } else if word.starts_with(|c: char| c.is_ascii_uppercase()) {
                Some(Token::Type)
            } else {
                None
            }
        } else if b"{}[]();,.:=<>+-*/&|!?%^~@".contains(&c) {
            i += 1;
            Some(Token::Punct)
        } else {
            i += code[i..].chars().next().map_or(1, char::len_utf8);
            None
        };
        if let Some(token) = token {
            flush(&mut tokens, text_start, start);
            tokens.push((start..i, token));
            text_start = i;
        }
    }
    flush(&mut tokens, text_start, bytes.len());
    tokens
}

// Line diff ---------------------------------------------------------------

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Change {
    Same,
    Added,
    Removed,
}

/// A line diff (LCS), trimmed to changed lines with `context` lines around
/// them; `None` marks an elided gap.
pub fn line_diff(old: &str, new: &str, context: usize) -> Vec<Option<(Change, String)>> {
    let a: Vec<&str> = old.lines().collect();
    let b: Vec<&str> = new.lines().collect();
    // Guard the quadratic table; huge edits show as replace-all.
    let full: Vec<(Change, String)> = if a.len() * b.len() > 4_000_000 {
        a.iter()
            .map(|l| (Change::Removed, (*l).to_owned()))
            .chain(b.iter().map(|l| (Change::Added, (*l).to_owned())))
            .collect()
    } else {
        let mut lcs = vec![vec![0u32; b.len() + 1]; a.len() + 1];
        for i in (0..a.len()).rev() {
            for j in (0..b.len()).rev() {
                lcs[i][j] = if a[i] == b[j] {
                    lcs[i + 1][j + 1] + 1
                } else {
                    lcs[i + 1][j].max(lcs[i][j + 1])
                };
            }
        }
        let (mut i, mut j) = (0, 0);
        let mut out = Vec::new();
        while i < a.len() || j < b.len() {
            if i < a.len() && j < b.len() && a[i] == b[j] {
                out.push((Change::Same, a[i].to_owned()));
                i += 1;
                j += 1;
            } else if j < b.len() && (i == a.len() || lcs[i][j + 1] >= lcs[i + 1][j]) {
                out.push((Change::Added, b[j].to_owned()));
                j += 1;
            } else {
                out.push((Change::Removed, a[i].to_owned()));
                i += 1;
            }
        }
        out
    };
    let changed: Vec<usize> = full
        .iter()
        .enumerate()
        .filter(|(_, (c, _))| *c != Change::Same)
        .map(|(i, _)| i)
        .collect();
    let keep = |i: usize| changed.iter().any(|&c| c.abs_diff(i) <= context);
    let mut out = Vec::new();
    let mut gap = false;
    for (i, line) in full.into_iter().enumerate() {
        if keep(i) {
            out.push(Some(line));
            gap = false;
        } else if !gap {
            out.push(None);
            gap = true;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn runs_map_back_to_source() {
        let source = "# Title\n\nSome **bold** and `code`\nwrapped.\n\n- one\n- [x] two\n  more\n\n| a | b |\n|---|--:|\n| 1 | 2 |\n\n```rust\nfn x() {}\n```\n";
        for block in parse(source) {
            let runs: Vec<Run> = match block {
                Block::Para(r) | Block::Heading(_, r) | Block::Quote(r) => vec![r],
                Block::Item { run, .. } => vec![run],
                Block::Code { run, .. } => vec![run],
                Block::Table { header, rows, .. } => header
                    .into_iter()
                    .chain(rows.into_iter().flatten())
                    .collect(),
                Block::Rule => vec![],
            };
            for run in runs {
                let slice = &source[run.start..run.start + run.text.len()];
                assert_eq!(
                    slice.replace('\n', " "),
                    run.text.replace('\n', " "),
                    "{run:?}"
                );
                for piece in inline(&run) {
                    let slice = &source[piece.start..piece.start + piece.text.len()];
                    assert_eq!(slice.replace('\n', " "), piece.text);
                }
            }
        }
    }
}
