//! What a screen reader hears of new output: the lines output scrolled in,
//! what was added to a line, or a line that changed, not the whole screen
//! nor a whole line again for each character the shell echoes.

/// The text a reader is told of a screen going from `before` to `after`
/// (each row's text, top first): empty when nothing worth saying changed.
pub(crate) fn news(before: &[&str], after: &[&str]) -> String {
    let rows = after.len();
    let mut pieces: Vec<&str> = Vec::new();
    if before.len() == rows && before != after {
        // Output scrolled the screen by `by` lines: those rows moved up and
        // as many came in at the bottom.
        let scrolled = (1..rows).find(|by| after[..rows - by] == before[*by..]);
        match scrolled {
            Some(by) => pieces.extend(after[rows - by..].iter().copied()),
            None => {
                for (was, now) in before.iter().zip(after) {
                    if was == now {
                        continue;
                    }
                    // A line grown at its end (an echo, a progress counter)
                    // says what it gained.
                    pieces.push(now.strip_prefix(*was).unwrap_or(now));
                }
            }
        }
    }
    let pieces: Vec<&str> = pieces
        .into_iter()
        .map(str::trim)
        .filter(|piece| !piece.is_empty())
        .collect();
    pieces.join("\n")
}

#[cfg(test)]
mod tests {
    use super::news;

    #[test]
    fn a_reader_hears_what_is_new() {
        // An echo: what the line gained.
        assert_eq!(news(&["$ ls", ""], &["$ ls -l", ""]), "-l");
        // Output on empty rows below the prompt.
        assert_eq!(
            news(&["$ ls", "", ""], &["$ ls", "a.txt", "b.txt"]),
            "a.txt\nb.txt"
        );
        // A full screen scrolled by two: the two lines that came in.
        assert_eq!(news(&["1", "2", "3", "4"], &["3", "4", "5", "6"]), "5\n6");
        // A line rewritten in place (a progress bar) whole.
        assert_eq!(news(&["[==  ] 40%", ""], &["[=== ] 60%", ""]), "[=== ] 60%");
        // Nothing new, or only blanks.
        assert_eq!(news(&["a", "b"], &["a", "b"]), "");
        assert_eq!(news(&["a", "b"], &["a", ""]), "");
    }
}
