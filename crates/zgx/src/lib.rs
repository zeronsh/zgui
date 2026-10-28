//! Fine-grained reactive state and a retained, damage-tracked scene.
//! The core has no platform dependencies and can be targeted by template compilers.
#![warn(missing_docs)]
pub mod collections;
pub mod reactive;
pub mod scene;
pub mod task;
pub mod view;
pub mod virtual_list;

pub mod input;
pub mod semantics;
pub mod text;
pub mod text_edit;
mod words;

pub mod widgets;

pub mod keyed;

pub mod image;

pub mod gpu_view;

pub mod components;

pub mod i18n;
pub mod local_time;
pub mod timer;

pub mod env;
pub mod frame;
pub mod profile;
pub mod switches;
#[cfg(feature = "trace")]
pub mod trace;

pub mod caret_blink;
pub(crate) mod editor_lines;
pub(crate) mod editor_menu;
pub(crate) mod editor_spell;
pub(crate) mod elastic;
pub mod find;
mod find_text;
pub mod rich_text;
mod scroll_keys;
pub mod selection;
pub mod text_layout;
mod typeahead;
pub mod zoom;

pub mod affine;
pub mod background;

pub mod compose;
mod compose_checkbox;
mod compose_menu;
mod compose_menu_scroll;
mod compose_modal;
mod compose_popover;
mod compose_rich;
mod compose_scrollbar;
mod compose_split;
mod compose_style;
mod compose_tooltip;
mod compose_virtual_keyboard;
pub mod style;
pub mod theme;

#[cfg(doctest)]
#[doc = include_str!("../../../docs/composition.md")]
mod composition_guide {}
#[cfg(doctest)]
#[doc = include_str!("../../../docs/styling.md")]
mod styling_guide {}

pub mod actions;

mod action_context;

pub mod canvas;

pub mod animation;

pub mod layout;

pub mod image_cache;

pub mod decoration;

pub mod svg;

pub mod cursor;

pub mod inspect;

mod compose_context_menu;
mod compose_drag;

#[cfg(target_os = "macos")]
pub mod native_surface;

pub mod native_view;
/// The clipboard and file dialogs, as a service a host provides.
pub mod platform;
pub mod terminal;
pub mod web_view;

/// Gives a map's room back once it is mostly empty (a page of thousands of
/// nodes gone): maps keep their peak capacity otherwise. Only past 1024
/// slots and under a quarter used, so rehashing is rare.
pub(crate) fn shrink_sparse<K: std::hash::Hash + Eq, V, S: std::hash::BuildHasher>(
    map: &mut std::collections::HashMap<K, V, S>,
) {
    if map.capacity() > 1024 && map.len() * 4 < map.capacity() {
        map.shrink_to(map.len() * 2);
    }
}

/// The hasher of keys from an app's data (a list's row keys): fast, and
/// seeded once per process from std's random seed, so keys made to collide
/// cannot slow every lookup, as they can a fixed hasher's. What depends on
/// order iterates in the list's own (a map's differs from run to run).
pub(crate) type KeyHasher = foldhash::fast::FixedState;
pub(crate) fn key_hasher() -> KeyHasher {
    static SEED: std::sync::OnceLock<u64> = std::sync::OnceLock::new();
    foldhash::fast::FixedState::with_seed(*SEED.get_or_init(|| {
        use std::hash::BuildHasher;
        std::collections::hash_map::RandomState::new().hash_one(0_u8)
    }))
}

/// Runs `f`, level `level` of a recursion over a tree (mounting, lowering,
/// layout), with room on the stack for it: when the thread's stack runs
/// low, `f` runs on a fresh segment instead. Trees nest as deep as an app
/// makes them and a debug build's frames are large; a 1 MB stack (the main
/// thread's on Windows) would otherwise end the process. The stack is
/// looked at every fourth level, which costs less than the levels between;
/// the room kept covers them.
pub fn deep<R>(level: u32, f: impl FnOnce() -> R) -> R {
    if stack_low(level) {
        stacker::grow(STACK_SEGMENT, f)
    } else {
        f()
    }
}
/// Whether level `level` of a recursion should go on a fresh stack segment
/// (`deep`), for a caller that would rather not move what it passes into a
/// closure on the common path.
pub(crate) fn stack_low(level: u32) -> bool {
    level.is_multiple_of(4) && stacker::remaining_stack().is_none_or(|left| left < 256 * 1024)
}
/// The stack a recursion goes on when the thread's runs low (`deep`).
pub(crate) const STACK_SEGMENT: usize = 2 * 1024 * 1024;
