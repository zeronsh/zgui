// Copyright 2022 The AccessKit Authors. All rights reserved.
// Licensed under the Apache License, Version 2.0 (found in
// the LICENSE-APACHE file) or the MIT license (found in
// the LICENSE-MIT file), at your option.

mod action;
mod adapter;
mod callback;
mod context;
mod error;
mod events;
mod filters;
mod node;
mod rect;
#[cfg(feature = "simplified-api")]
pub mod simplified;
mod table; // zgx patch: Table and TableCell (ZGX_PATCH.md).
mod text_attributes;
mod util;

pub use accesskit_consumer::NodeId;
pub use atspi_common::{
    CoordType, Granularity, InterfaceSet, Layer, RelationType, Role, ScrollType, State, StateSet,
};

pub use action::*;
pub use adapter::{Adapter, next_adapter_id};
pub use callback::AdapterCallback;
pub use context::{ActionHandlerNoMut, ActionHandlerWrapper, AppContext};
pub use error::*;
pub use events::*;
pub use node::{CacheNode, CellExtents, NodeIdOrRoot, PlatformNode, PlatformRoot};
pub use rect::*;
pub use util::WindowBounds;

/// zgx patch: a node whose author ID is `EMBED`, then a bus name, an object
/// path and anything else (a count that changes to embed again), each on a
/// line of its own, embeds that AT-SPI object of another process (a web
/// page's root) as its last child (ZGX_PATCH.md).
pub const EMBED: &str = "zgx-atspi-embed\n";
/// The bus name and path an author ID embeds (`EMBED`).
pub fn embedded(author_id: &str) -> Option<(String, String)> {
    let mut parts = author_id.strip_prefix(EMBED)?.split('\n');
    Some((parts.next()?.into(), parts.next()?.into()))
}

#[cfg(test)]
mod embed_tests {
    /// zgx patch: an author ID embeds an object only with the prefix, a bus
    /// name and a path; the rest (a count) is free.
    #[test]
    fn an_author_id_names_the_object_it_embeds() {
        let id = format!("{}org.webkit.app-1.WebProcess\n/org/a11y/webkit/accessible/2\n7", crate::EMBED);
        assert_eq!(
            crate::embedded(&id),
            Some(("org.webkit.app-1.WebProcess".into(), "/org/a11y/webkit/accessible/2".into()))
        );
        assert_eq!(crate::embedded("plain id"), None);
        assert_eq!(crate::embedded(&format!("{}bus-only", crate::EMBED)), None);
    }
}
