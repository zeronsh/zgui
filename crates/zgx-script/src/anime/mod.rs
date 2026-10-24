//! anime.js v4 for scripts: `animate`, timelines, timers, animatables,
//! `stagger`, every easing and spring, `utils` and `engine`, run by a Rust
//! engine that writes compositor properties straight to the scene.
pub(crate) mod api;
pub(crate) mod build;
pub(crate) mod draggable;
pub(crate) mod ease;
pub(crate) mod engine;
pub(crate) mod scroll;
pub(crate) mod stagger;
pub(crate) mod svg;
pub(crate) mod target;
pub(crate) mod value;

pub(crate) use engine::Engine;
pub(crate) use target::Key;
