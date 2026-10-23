//! Fine-grained reactive state and a retained, damage-tracked scene.
//! The core has no platform dependencies and can be targeted by template compilers.
pub mod collections;
pub mod reactive;
pub mod scene;
pub mod task;
pub mod view;
pub mod virtual_list;

pub mod input;
pub mod semantics;
pub mod text_edit;

pub mod widgets;

pub mod keyed;

pub mod image;

pub mod components;

pub mod timer;

pub mod frame;

pub mod rich_text;
mod scroll_keys;
pub mod selection;
pub mod text_layout;

pub mod affine;
pub mod background;

pub mod compose;
mod compose_menu;
mod compose_menu_scroll;
mod compose_modal;
mod compose_popover;
mod compose_rich;
mod compose_scrollbar;
mod compose_style;
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
