//! A command palette: a field over a list of commands, filtered as it is
//! typed in by a fuzzy match (the letters in order, words' starts and runs
//! counting most), as VS Code's ⌘⇧P and Raycast: Up and Down move, Enter
//! runs the highlighted command (closing the palette first), Escape closes
//! it. It opens near the window's top, over a backdrop; the list is
//! virtual.
use std::rc::Rc;
use zgui::{
    compose::{
        ModalPlacement, PlacementEdge as Edge, View, column, component, modal, row, text,
        text_input, text_signal, virtual_list,
    },
    input::{EventPhase, InputEvent, Key},
    reactive::Signal,
    scene::Color,
    semantics::Role,
    style::{Styled, Styles},
    theme::tok,
};

/// A command a [`command_palette`] lists: its id (what `run` is given),
/// title, a detail (its menu, a category) and the keys that run it.
#[derive(Clone, Debug, PartialEq)]
pub struct PaletteCommand {
    /// What `run` is given.
    pub id: String,
    /// What it shows, and what typing matches.
    pub title: String,
    /// Shown before the title (its menu: "File"), and matched too.
    pub detail: Option<String>,
    /// The keys that run it, shown at its end ("⌘S").
    pub shortcut: Option<String>,
}
impl PaletteCommand {
    /// A command titled `title`.
    pub fn new(id: impl Into<String>, title: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            title: title.into(),
            detail: None,
            shortcut: None,
        }
    }
    /// With a detail before its title.
    pub fn detail(mut self, detail: impl Into<String>) -> Self {
        self.detail = Some(detail.into());
        self
    }
    /// With the keys that run it.
    pub fn shortcut(mut self, keys: impl Into<String>) -> Self {
        self.shortcut = Some(keys.into());
        self
    }
}

/// How well `query` matches `candidate`: its letters in order (any case),
/// each scoring more at a word's start or right after the last match;
/// `None` if they aren't all there. An empty query matches everything.
pub fn fuzzy_score(query: &str, candidate: &str) -> Option<i32> {
    let query: Vec<char> = query
        .chars()
        .flat_map(char::to_lowercase)
        .filter(|c| !c.is_whitespace())
        .collect();
    if query.is_empty() {
        return Some(0);
    }
    let chars: Vec<char> = candidate.chars().collect();
    let mut score = 0;
    let mut at = 0;
    let mut last: Option<usize> = None;
    for (i, c) in chars.iter().enumerate() {
        if at == query.len() {
            break;
        }
        if c.to_lowercase().next() != Some(query[at]) {
            continue;
        }
        let start = i == 0
            || !chars[i - 1].is_alphanumeric()
            || (c.is_uppercase() && chars[i - 1].is_lowercase());
        score += 1;
        if start {
            score += 8;
        }
        // Runs count most: "save" in "Save All" over "Stash Away Valid Edits".
        if last.is_some_and(|l| l + 1 == i) {
            score += 10;
        }
        last = Some(i);
        at += 1;
    }
    // Shorter candidates rank first among equals.
    (at == query.len()).then(|| score * 100 - chars.len() as i32)
}

/// A row's height.
const ROW: f32 = 34.;

/// A palette of what `commands` gives (read as it opens and as typed in),
/// shown while `open`; Enter runs `run` with the highlighted command's id,
/// after the palette closes.
pub fn command_palette(
    open: Signal<bool>,
    commands: impl Fn() -> Vec<PaletteCommand> + 'static,
    run: impl Fn(&str) + 'static,
) -> View {
    let (commands, run) = (Rc::new(commands), Rc::new(run));
    component(move |cx| {
        let runtime = cx.runtime();
        let query = cx.state(String::new());
        let highlight = runtime.signal(0usize);
        let offset = runtime.signal(0_f32);
        // What matches, best first.
        let matches = {
            let (query, commands, open) = (query.clone(), commands.clone(), open.clone());
            let shown = runtime.signal(Vec::<PaletteCommand>::new());
            let out = shown.clone();
            cx.retain(runtime.effect(move || {
                if !open.get() {
                    return;
                }
                let q = query.get();
                let mut scored: Vec<(i32, PaletteCommand)> = commands()
                    .into_iter()
                    .filter_map(|c| {
                        let haystack = match &c.detail {
                            Some(d) => format!("{d}: {}", c.title),
                            None => c.title.clone(),
                        };
                        let title = fuzzy_score(&q, &c.title);
                        let both = fuzzy_score(&q, &haystack);
                        title.max(both).map(|s| (s, c))
                    })
                    .collect();
                scored.sort_by_key(|s| std::cmp::Reverse(s.0));
                out.set(scored.into_iter().map(|(_, c)| c).collect());
            }));
            shown
        };
        // A new query or opening: the best match highlighted, at the top.
        {
            let (matches, highlight, offset) = (matches.clone(), highlight.clone(), offset.clone());
            cx.retain(runtime.effect(move || {
                matches.with(|_| ());
                highlight.set(0);
                offset.set(0.);
            }));
        }
        // Opening starts from nothing typed.
        {
            let (open, query) = (open.clone(), query.clone());
            cx.retain(runtime.effect(move || {
                if open.get() {
                    query.set(String::new());
                }
            }));
        }
        let execute = {
            let (matches, open, run) = (matches.clone(), open.clone(), run.clone());
            Rc::new(move |i: usize| {
                let Some(command) = matches.with_untracked(|m| m.get(i).cloned()) else {
                    return;
                };
                open.set(false);
                run(&command.id);
            })
        };
        let visible = 9.;
        let field = {
            let (matches, highlight, execute, offset) = (
                matches.clone(),
                highlight.clone(),
                execute.clone(),
                offset.clone(),
            );
            text_input("Command", query.clone())
                .role(Role::ComboBox)
                .placeholder("Type a command")
                .w_full()
                .on_event(move |cx| {
                    if cx.phase == EventPhase::Bubble {
                        return;
                    }
                    let InputEvent::KeyDown { key, .. } = &cx.event else {
                        return;
                    };
                    let n = matches.with_untracked(Vec::len);
                    let at = highlight.with_untracked(|h| *h);
                    let next = match key {
                        Key::ArrowDown if n > 0 => (at + 1) % n,
                        Key::ArrowUp if n > 0 => (at + n - 1) % n,
                        Key::PageDown if n > 0 => (at + 8).min(n - 1),
                        Key::PageUp => at.saturating_sub(8),
                        Key::Enter => {
                            execute(at);
                            cx.prevent_default();
                            cx.stop_propagation();
                            return;
                        }
                        _ => return,
                    };
                    highlight.set(next);
                    // Kept in the list's view.
                    let top = next as f32 * ROW;
                    let now = offset.with_untracked(|o| *o);
                    let height = visible * ROW;
                    if top < now {
                        offset.set(top);
                    } else if top + ROW > now + height {
                        offset.set(top + ROW - height);
                    }
                    cx.prevent_default();
                    cx.stop_propagation();
                })
        };
        let row_view = {
            let (matches, highlight, execute) =
                (matches.clone(), highlight.clone(), execute.clone());
            move |index: Signal<usize>, _: String, _: &mut zgui::compose::Context| {
                let item = {
                    let (matches, index) = (matches.clone(), index.clone());
                    move || matches.with(|m| m.get(index.get()).cloned())
                };
                let (title, detail, keys) = (item.clone(), item.clone(), item.clone());
                let (selected, styled, hover) =
                    (highlight.clone(), highlight.clone(), highlight.clone());
                let (at, at_styled, at_event) = (index.clone(), index.clone(), index.clone());
                let execute = execute.clone();
                row()
                    .role(Role::ListBoxOption)
                    .selected_when(move || selected.get() == at.get())
                    .h(ROW)
                    .px(10.)
                    .gap(8.)
                    .items_center()
                    .rounded(6.)
                    .reactive_style(move || {
                        if styled.get() == at_styled.get() {
                            Styles::new().bg(tok::SELECTION_ITEM)
                        } else {
                            Styles::new().bg(Color(0, 0, 0, 0))
                        }
                    })
                    .on_event(move |cx| {
                        if cx.phase == EventPhase::Capture {
                            return;
                        }
                        match cx.event {
                            InputEvent::PointerDown { .. } => {
                                cx.prevent_default();
                                cx.stop_propagation();
                                execute(at_event.get());
                            }
                            InputEvent::PointerMove { .. }
                                if hover.with_untracked(|h| *h) != at_event.get() =>
                            {
                                hover.set(at_event.get());
                            }
                            _ => {}
                        }
                    })
                    .child(
                        text_signal(move || {
                            detail()
                                .and_then(|c| c.detail)
                                .map(|d| format!("{d}:"))
                                .unwrap_or_default()
                        })
                        .text_color(tok::TEXT_SECONDARY),
                    )
                    .child(
                        text_signal(move || title().map(|c| c.title).unwrap_or_default())
                            .grow()
                            .truncate(),
                    )
                    .child(
                        text_signal(move || keys().and_then(|c| c.shortcut).unwrap_or_default())
                            .text_color(tok::TEXT_TERTIARY)
                            .text_size(tok::FONT_SIZE_SM),
                    )
            }
        };
        let (count, keys, height, empty) = (
            matches.clone(),
            matches.clone(),
            matches.clone(),
            matches.clone(),
        );
        let list = virtual_list(
            offset,
            ROW,
            4,
            move || count.with(Vec::len),
            move |i| keys.with(|m| m.get(i).map(|c| c.id.clone()).unwrap_or_default()),
            row_view,
        )
        .role(Role::ListBox)
        .reactive_style(move || Styles::new().h((height.with(Vec::len) as f32).min(visible) * ROW));
        modal("Command palette", open.clone())
            .lazy()
            .modal_placement(ModalPlacement {
                x: Edge::Center,
                y: Edge::Start,
                insets: [80., 16., 16., 16.],
            })
            .w(560.)
            .p(8.)
            .gap(6.)
            .child(field)
            .child(list)
            .child(zgui::compose::switch(
                move || empty.with(Vec::is_empty),
                |none, _| {
                    if none {
                        text("No matching commands")
                            .p(8.)
                            .text_color(tok::TEXT_SECONDARY)
                    } else {
                        column()
                    }
                },
            ))
    })
}
