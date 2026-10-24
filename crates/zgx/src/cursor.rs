//! Platform-independent pointer cursor styles.
/// The pointer's shape, named after CSS `cursor` values.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Cursor {
    /// The arrow (CSS `default`).
    #[default]
    Default,
    /// A pointing hand, over links and buttons (CSS `pointer`).
    Pointer,
    /// The I-beam, over text (CSS `text`).
    Text,
    /// The I-beam for vertical text (CSS `vertical-text`).
    VerticalText,
    /// A crosshair (CSS `crosshair`).
    Crosshair,
    /// Something can be moved (CSS `move`).
    Move,
    /// An open hand: something can be grabbed (CSS `grab`).
    Grab,
    /// A closed hand: something is being dragged (CSS `grabbing`).
    Grabbing,
    /// The action isn't allowed (CSS `not-allowed`).
    NotAllowed,
    /// Busy; the app can't be used meanwhile (CSS `wait`).
    Wait,
    /// Busy, but the app can still be used (CSS `progress`).
    Progress,
    /// Help is available (CSS `help`).
    Help,
    /// A context menu is available (CSS `context-menu`).
    ContextMenu,
    /// A drop here copies (CSS `copy`).
    Copy,
    /// A drop here makes a link or alias (CSS `alias`).
    Alias,
    /// Nothing can be dropped here (CSS `no-drop`).
    NoDrop,
    /// Scrolls in any direction (CSS `all-scroll`).
    AllScroll,
    /// A column can be resized across (CSS `col-resize`).
    ColResize,
    /// A row can be resized down (CSS `row-resize`).
    RowResize,
    /// Resizes left and right (CSS `ew-resize`).
    EwResize,
    /// Resizes up and down (CSS `ns-resize`).
    NsResize,
    /// Resizes towards the top right and bottom left (CSS `nesw-resize`).
    NeswResize,
    /// Resizes towards the top left and bottom right (CSS `nwse-resize`).
    NwseResize,
    /// Zooms in (CSS `zoom-in`).
    ZoomIn,
    /// Zooms out (CSS `zoom-out`).
    ZoomOut,
}
