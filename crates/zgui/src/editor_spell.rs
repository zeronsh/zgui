//! Spellchecking an editor (`View::spellcheck`), as a browser's text fields
//! do: the system's checker (`Platform::misspellings`) marks misspelled
//! words with a dotted line, and the context menu offers its guesses, Learn
//! Spelling and Ignore Spelling. An edit checks again only the paragraphs it
//! touched; the marks elsewhere move with the text.
use crate::{platform::PlatformService, widgets::EditorHandle, widgets::Ui};
use std::{cell::RefCell, ops::Range, rc::Rc};

pub(crate) fn mount(ui: &mut Ui, editor: EditorHandle, platform: Rc<PlatformService>) {
    // The text last checked, and its misspellings.
    let checked: Rc<RefCell<(String, Vec<Range<usize>>)>> = Rc::default();
    let value = editor.value.clone();
    ui.bind(editor.node, move || {
        value.with(|_| ());
        let text = editor.text();
        let mut checked = checked.borrow_mut();
        let (old, ranges) = &mut *checked;
        if *old == text && !old.is_empty() {
            return;
        }
        let (start, end, shift) = changed_paragraphs(old, &text);
        let old_end = (end as isize - shift) as usize;
        let mut next: Vec<Range<usize>> =
            ranges.iter().filter(|r| r.end <= start).cloned().collect();
        next.extend(
            platform
                .0
                .misspellings(&text[start..end])
                .into_iter()
                .filter(|r| r.start < r.end && r.end <= end - start)
                .map(|r| r.start + start..r.end + start),
        );
        next.extend(
            ranges
                .iter()
                .filter(|r| r.start >= old_end)
                .map(|r| (r.start as isize + shift) as usize..(r.end as isize + shift) as usize),
        );
        *old = text;
        *ranges = next.clone();
        drop(checked);
        editor.set_misspellings(next);
    });
}

/// Where `old` and `new` differ, widened to whole paragraphs of `new`
/// (`start..end`), and how much longer `new` is.
fn changed_paragraphs(old: &str, new: &str) -> (usize, usize, isize) {
    let prefix = old
        .char_indices()
        .zip(new.chars())
        .find(|((_, a), b)| a != b)
        .map_or(old.len().min(new.len()), |((i, _), _)| i);
    let suffix = old[prefix..]
        .chars()
        .rev()
        .zip(new[prefix..].chars().rev())
        .take_while(|(a, b)| a == b)
        .map(|(a, _)| a.len_utf8())
        .sum::<usize>();
    let changed_end = new.len() - suffix;
    let start = new[..prefix].rfind('\n').map_or(0, |i| i + 1);
    let end = new[changed_end..]
        .find('\n')
        .map_or(new.len(), |i| changed_end + i);
    (start, end, new.len() as isize - old.len() as isize)
}

/// The context menu's spelling items for the misspelled word the selection
/// is in: its guesses, Learn Spelling and Ignore Spelling. None elsewhere.
pub(crate) fn menu_items(
    editor: &EditorHandle,
    platform: &Rc<PlatformService>,
) -> Vec<crate::compose::View> {
    use crate::compose::menu_item;
    let Some(range) = editor.misspelling_at_selection() else {
        return Vec::new();
    };
    let text = editor.text();
    let Some(word) = text.get(range.clone()).map(str::to_owned) else {
        return Vec::new();
    };
    let guesses = platform.0.spelling_guesses(&word);
    let mut items: Vec<crate::compose::View> = guesses
        .into_iter()
        .take(5)
        .enumerate()
        .map(|(index, guess)| {
            let (editor, range) = (editor.clone(), range.clone());
            menu_item(guess.clone())
                .id(format!("spelling-guess-{index}"))
                .on_click(move || {
                    editor.set_selection(range.start, range.end);
                    editor.paste(&guess);
                })
        })
        .collect();
    if items.is_empty() {
        items.push(
            menu_item("No Guesses Found")
                .id("spelling-none")
                .disabled(true),
        );
    }
    let forget = |editor: &EditorHandle, word: &str| {
        let text = editor.text();
        let left: Vec<_> = editor
            .misspellings()
            .into_iter()
            .filter(|r| text.get(r.clone()) != Some(word))
            .collect();
        editor.set_misspellings(left);
    };
    let (learn, ignore) = (
        (editor.clone(), platform.clone(), word.clone()),
        (editor.clone(), platform.clone(), word),
    );
    items.push(
        menu_item("Learn Spelling")
            .id("spelling-learn")
            .on_click(move || {
                let (editor, platform, word) = &learn;
                platform.0.learn_spelling(word);
                forget(editor, word);
            }),
    );
    items.push(
        menu_item("Ignore Spelling")
            .id("spelling-ignore")
            .on_click(move || {
                let (editor, platform, word) = &ignore;
                platform.0.ignore_spelling(word);
                forget(editor, word);
            }),
    );
    items
}

#[cfg(test)]
mod tests {
    use super::changed_paragraphs;
    #[test]
    fn an_edit_rechecks_only_the_paragraphs_it_touched() {
        assert_eq!(changed_paragraphs("", "ab cd"), (0, 5, 5));
        // Typing in the second paragraph.
        assert_eq!(
            changed_paragraphs("one\ntwo\nsix", "one\ntwoo\nsix"),
            (4, 8, 1)
        );
        // Joining two paragraphs: the joined one.
        assert_eq!(changed_paragraphs("one\ntwo", "onetwo"), (0, 6, -1));
        assert_eq!(changed_paragraphs("héllo", "héllo!"), (0, 7, 1));
    }
}
