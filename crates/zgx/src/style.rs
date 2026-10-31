//! Fluent, sparse styling for retained views.
//!
//! Dimensions, spacing, text sizes, blur and translations use logical pixels,
//! except explicit percentage dimension methods.
//! Style patches only replace explicitly supplied properties; chaining follows
//! last-write precedence, including individual padding and margin sides.
use crate::scene::{Align, BoxShadow, Color, Insets, Justify, Layout, LocalTransform, Transform};

/// Where a stroke (`Styled::stroke`) lies against the box's edge, as a
/// design tool aligns it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum StrokeAlign {
    /// Within the box, over its content.
    #[default]
    Inside,
    /// Half within, half outside.
    Center,
    /// Wholly outside.
    Outside,
}
/// How image pixels fit the allocated padded content box. Alignment is centered.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ObjectFit {
    /// Stretch independently along each axis (the default).
    #[default]
    Fill,
    /// Preserve aspect ratio and show the entire image.
    Contain,
    /// Preserve aspect ratio and crop to fill the content box.
    Cover,
    /// Keep intrinsic pixel dimensions, clipping excess content.
    None,
    /// Use intrinsic size unless shrinking is needed to contain the image.
    ScaleDown,
}

/// Defines the fields a [`Styles`] holds (`StylesFields`), each one's bit
/// in `Styles::set` (`bit`), and what visits only the fields of given bits:
/// one list, so no field can be left out of any.
/// An `Option<f32>` in four bytes, for `Styles`' many optional numbers: a
/// style is copied whole when a style function returns it and when it is
/// kept, so each field's four bytes of tag were copied with it. `None` is
/// one NaN payload no value is stored as: a NaN set is kept as the
/// standard NaN. It compares and prints as the `Option<f32>` it holds (a
/// NaN equal to nothing, as there).
#[derive(Clone, Copy)]
pub(crate) struct Opt32(u32);
impl Opt32 {
    /// The bits standing for `None`: a quiet NaN with a payload `f32::NAN`
    /// (`0x7fc0_0000`) and arithmetic never produce.
    const NONE: u32 = 0x7fc0_57e5;
    pub(crate) const fn none() -> Self {
        Self(Self::NONE)
    }
    #[inline]
    pub(crate) fn get(self) -> Option<f32> {
        match self.0 {
            Self::NONE => None,
            bits => Some(f32::from_bits(bits)),
        }
    }
    #[inline]
    pub(crate) fn is_some(self) -> bool {
        self.0 != Self::NONE
    }
    #[inline]
    pub(crate) fn is_none(self) -> bool {
        self.0 == Self::NONE
    }
}
impl Default for Opt32 {
    fn default() -> Self {
        Self::none()
    }
}
impl From<Option<f32>> for Opt32 {
    #[inline]
    fn from(value: Option<f32>) -> Self {
        match value {
            None => Self::none(),
            Some(value) if value.is_nan() => Self(f32::NAN.to_bits()),
            Some(value) => Self(value.to_bits()),
        }
    }
}
impl From<Opt32> for Option<f32> {
    #[inline]
    fn from(value: Opt32) -> Self {
        value.get()
    }
}
/// As `Option<f32>` compares, on the bits: the same bits are equal but for
/// a NaN (equal to nothing); other bits only for 0 and -0.
impl PartialEq for Opt32 {
    #[inline]
    fn eq(&self, other: &Self) -> bool {
        let (a, b) = (self.0, other.0);
        match a == b {
            true => a == Self::NONE || !f32::from_bits(a).is_nan(),
            false => (a | b) << 1 == 0,
        }
    }
}
impl PartialEq<Option<f32>> for Opt32 {
    #[inline]
    fn eq(&self, other: &Option<f32>) -> bool {
        self.get() == *other
    }
}
impl std::fmt::Debug for Opt32 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.get().fmt(f)
    }
}

/// The properties few styles set: no scene the gate runs, nor an example
/// app, sets one. Each kept inline made every `Styles` bigger, copied when
/// a style function returns one and kept several times over by each
/// restyled node; together they are 72 bytes. Unset (most styles), they
/// cost a pointer (`OverflowSlot`). Several animate every frame (filters,
/// the clip inset, a mask's offset): set, their box comes from a small pool
/// and goes back to it, so a style function setting them each frame
/// allocates nothing once running.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Overflow {
    pub(crate) object_fit: Option<ObjectFit>,
    pub(crate) layout: Option<Layout>,
    pub(crate) italic: Option<bool>,
    pub(crate) blur: Opt32,
    pub(crate) edge_fade: Opt32,
    pub(crate) filter_blur: Opt32,
    /// Colour filters (`Styled::brightness` and the rest).
    pub(crate) brightness: Opt32,
    pub(crate) contrast: Opt32,
    pub(crate) saturate: Opt32,
    /// Where `RareStyles::mask` lies over the node: animated every frame.
    pub(crate) mask_offset: Option<(f32, f32)>,
    /// `Styled::clip_inset`: animated every frame.
    pub(crate) clip_inset: Option<crate::scene::ClipInset>,
    pub(crate) isolated: Option<bool>,
}
impl Overflow {
    /// Every property unset.
    pub(crate) const DEFAULT: Self = Self {
        object_fit: None,
        layout: None,
        italic: None,
        blur: Opt32::none(),
        edge_fade: Opt32::none(),
        filter_blur: Opt32::none(),
        brightness: Opt32::none(),
        contrast: Opt32::none(),
        saturate: Opt32::none(),
        mask_offset: None,
        clip_inset: None,
        isolated: None,
    };
}
/// Read where no style has set one (`OverflowSlot::get`).
static UNSET: Overflow = Overflow::DEFAULT;
/// A style's `Overflow`, allocated once one of its properties is set. It
/// reads and compares as the `Overflow` it holds (unset:
/// `Overflow::DEFAULT`). It owns its allocation as a `Box` would, through a
/// pointer: `Clone for Styles` copies a style bitwise before replacing each
/// field that owns something, which a `Box` (unique) must not be copied by.
#[derive(Default)]
pub(crate) struct OverflowSlot(Option<std::ptr::NonNull<Overflow>>);
// SAFETY: it owns an `Overflow` (plain data) alone, as a `Box` would.
unsafe impl Send for OverflowSlot {}
// SAFETY: as for `Send`; shared, it is only read.
unsafe impl Sync for OverflowSlot {}
impl OverflowSlot {
    #[inline]
    pub(crate) fn get(&self) -> &Overflow {
        match self.0 {
            // SAFETY: owned by `self` (from `Box::into_raw`), so valid while
            // `self` is borrowed.
            Some(overflow) => unsafe { overflow.as_ref() },
            None => &UNSET,
        }
    }
    /// Whether it holds an allocation.
    #[inline]
    pub(crate) fn is_allocated(&self) -> bool {
        self.0.is_some()
    }
    /// For writing, if allocated.
    #[inline]
    pub(crate) fn get_allocated_mut(&mut self) -> Option<&mut Overflow> {
        // SAFETY: owned by `self`, which is borrowed mutably.
        self.0.map(|overflow| unsafe { &mut *overflow.as_ptr() })
    }
    /// For writing: allocated (from the pool) if it was not.
    #[inline]
    pub(crate) fn get_mut(&mut self) -> &mut Overflow {
        let overflow = match self.0 {
            Some(overflow) => overflow,
            None => self.allocate(),
        };
        // SAFETY: owned by `self`, which is borrowed mutably.
        unsafe { &mut *overflow.as_ptr() }
    }
    /// Out of line, as are giving it back and writing paint
    /// (`StylesFields::set_overflow_paint`): inline, the pool's thread-local
    /// access made every style effect run that might reach it bigger and
    /// slower (style +1.9%), unset as the properties nearly always are.
    #[inline(never)]
    fn allocate(&mut self) -> std::ptr::NonNull<Overflow> {
        *self
            .0
            .insert(std::ptr::NonNull::from(Box::leak(overflow_pool::take())))
    }
}
impl Clone for OverflowSlot {
    fn clone(&self) -> Self {
        Self(self.0.map(|_| {
            let mut copy = overflow_pool::take();
            *copy = *self.get();
            std::ptr::NonNull::from(Box::leak(copy))
        }))
    }
}
impl Drop for OverflowSlot {
    #[inline]
    fn drop(&mut self) {
        if let Some(overflow) = self.0.take() {
            give_back(overflow);
        }
    }
}
#[inline(never)]
fn give_back(overflow: std::ptr::NonNull<Overflow>) {
    // SAFETY: owned by the slot it was taken from (from `Box::leak`), so
    // given back once.
    overflow_pool::give(unsafe { Box::from_raw(overflow.as_ptr()) });
}
impl PartialEq for OverflowSlot {
    fn eq(&self, other: &Self) -> bool {
        self.get() == other.get()
    }
}
impl std::fmt::Debug for OverflowSlot {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.get().fmt(f)
    }
}
/// Boxes of `Overflow` let go of, for the next style setting one: a style
/// function setting one every frame takes back the box the last frame's
/// style gave up. At most 16 are kept (1 KB): in a steady animation
/// each style takes one as the last gives one back.
mod overflow_pool {
    use super::Overflow;
    use std::cell::RefCell;
    const KEPT: usize = 16;
    thread_local! {
        // The boxes themselves are what is kept: each is handed out whole.
        #[allow(clippy::vec_box)]
        static SPARE: RefCell<Vec<Box<Overflow>>> = const { RefCell::new(Vec::new()) };
    }
    pub(super) fn take() -> Box<Overflow> {
        match SPARE
            .try_with(|spare| spare.borrow_mut().pop())
            .ok()
            .flatten()
        {
            Some(mut overflow) => {
                *overflow = Overflow::DEFAULT;
                overflow
            }
            None => Box::new(Overflow::DEFAULT),
        }
    }
    pub(super) fn give(overflow: Box<Overflow>) {
        let _ = SPARE.try_with(|spare| {
            let mut spare = spare.borrow_mut();
            if spare.len() < KEPT {
                spare.push(overflow);
            }
        });
    }
}

macro_rules! styles {
    (
        $( $(#[$doc:meta])* $field:ident: $ty:ty = $bit:ident; )*
        @overflow { $( $ofield:ident = $obit:ident; )* }
    ) => {
        /// The properties a [`Styles`] holds, read through it (`Deref`);
        /// written through `Styles::fields_mut`, which marks them set.
        #[doc(hidden)]
        #[derive(Debug, Default, PartialEq)]
        pub struct StylesFields {
            $( $(#[$doc])* pub(crate) $field: $ty, )*
            /// The properties few styles set (`Overflow`), apart.
            pub(crate) overflow: OverflowSlot,
        }
        /// Each field's bit in `Styles::set`.
        #[allow(dead_code)]
        pub(crate) mod bit {
            styles!(@bits 0u32; $($bit)* $($obit)*);
            /// Each field's bit's index.
            pub(crate) mod index {
                styles!(@index 0u32; $($bit)* $($obit)*);
            }
        }
        impl StylesFields {
            /// The bits of the fields that differ from their defaults.
            fn mask(&self) -> u64 {
                let mut mask = 0;
                $( if self.$field != <$ty>::default() { mask |= bit::$bit; } )*
                $( if self.overflow().$ofield != Overflow::DEFAULT.$ofield { mask |= bit::$obit; } )*
                mask
            }
            /// Whether the fields of `bits` are equal in both.
            fn eq_in(&self, other: &Self, mut bits: u64) -> bool {
                while bits != 0 {
                    let low = bits & bits.wrapping_neg();
                    bits ^= low;
                    if !self.eq_field(other, low) {
                        return false;
                    }
                }
                true
            }
            /// Whether the field of bit `field` is equal in both. Out of
            /// line: inlined into `eq_in`'s loop, every field's comparison
            /// was hoisted out of it, and all of them made each time.
            #[inline(never)]
            fn eq_field(&self, other: &Self, field: u64) -> bool {
                match field.trailing_zeros() {
                    $( bit::index::$bit => self.$field == other.$field, )*
                    $( bit::index::$obit => self.overflow().$ofield == other.overflow().$ofield, )*
                    _ => true,
                }
            }
        }
    };
    (@bits $n:expr; $first:ident $($rest:ident)*) => {
        pub(crate) const $first: u64 = 1 << ($n);
        styles!(@bits $n + 1; $($rest)*);
    };
    (@bits $n:expr;) => {};
    (@index $n:expr; $first:ident $($rest:ident)*) => {
        pub(crate) const $first: u32 = $n;
        styles!(@index $n + 1; $($rest)*);
    };
    (@index $n:expr;) => {};
}
styles! {
    layout_options: Option<std::sync::Arc<crate::layout::LayoutOptions>> = LAYOUT_OPTIONS;
    absolute: Option<bool> = ABSOLUTE;
    width: Opt32 = WIDTH;
    height: Opt32 = HEIGHT;
    width_percent: Opt32 = WIDTH_PERCENT;
    height_percent: Opt32 = HEIGHT_PERCENT;
    padding: Option<Insets> = PADDING;
    margin: Option<Insets> = MARGIN;
    gap: Opt32 = GAP;
    clip: Option<bool> = CLIP;
    flex_grow: Opt32 = FLEX_GROW;
    flex_shrink: Opt32 = FLEX_SHRINK;
    align: Option<Align> = ALIGN;
    justify: Option<Justify> = JUSTIFY;
    text_wrap: Option<bool> = TEXT_WRAP;
    background: Option<Color> = BACKGROUND;
    cursor: Option<crate::cursor::Cursor> = CURSOR;
    text_color: Option<Color> = TEXT_COLOR;
    text_size: Opt32 = TEXT_SIZE;
    line_height: Option<crate::text_layout::LineHeight> = LINE_HEIGHT;
    font_family: Option<crate::text_layout::FontFamily> = FONT_FAMILY;
    text_align: Option<crate::text_layout::TextAlign> = TEXT_ALIGN;
    font_weight: Option<u16> = FONT_WEIGHT;
    radius: Opt32 = RADIUS;
    border_width: Opt32 = BORDER_WIDTH;
    border_color: Option<Color> = BORDER_COLOR;
    opacity: Opt32 = OPACITY;
    transform: Option<Transform> = TRANSFORM;
    local_transform: TransformPatch = LOCAL_TRANSFORM;
    padding_sides: u8 = PADDING_SIDES;
    margin_sides: u8 = MARGIN_SIDES;
    /// Set (to none) by `bg`, `rounded` and `border`, so on most nodes.
    paint_background: Option<Option<crate::decoration::Background>> = PAINT_BACKGROUND;
    corners: Option<Option<crate::decoration::Corners>> = CORNERS;
    border_edges: Option<Option<Insets>> = BORDER_EDGES;
    /// Properties few views set (see `RareStyles`).
    rare: Option<std::sync::Arc<RareStyles>> = RARE;
    /// Properties set by theme tokens (`.bg(tok::SURFACE_RAISED)`), resolved
    /// when the node is styled; see `crate::theme`.
    tokens: Option<std::sync::Arc<crate::theme::Bindings>> = TOKENS;
    // The properties few styles set (`Overflow`): each a bit in `set` as
    // the others, its value apart.
    @overflow {
        object_fit = OBJECT_FIT;
        layout = LAYOUT;
        italic = ITALIC;
        blur = BLUR;
        edge_fade = EDGE_FADE;
        filter_blur = FILTER_BLUR;
        brightness = BRIGHTNESS;
        contrast = CONTRAST;
        saturate = SATURATE;
        mask_offset = MASK_OFFSET;
        clip_inset = CLIP_INSET;
        isolated = ISOLATED;
    }
}
/// The paint properties among `Overflow`'s (`compose_style::Paint`'s), as
/// one: a style effect moves them between styles together.
#[derive(Clone, Copy, Default, PartialEq)]
pub(crate) struct OverflowPaint {
    pub(crate) blur: Opt32,
    pub(crate) edge_fade: Opt32,
    pub(crate) filter_blur: Opt32,
    pub(crate) brightness: Opt32,
    pub(crate) contrast: Opt32,
    pub(crate) saturate: Opt32,
    pub(crate) mask_offset: Option<(f32, f32)>,
    pub(crate) clip_inset: Option<crate::scene::ClipInset>,
}
impl OverflowPaint {
    /// None set: one test a field, without branching between them (an
    /// animated style's every run asks).
    #[inline]
    fn is_unset(&self) -> bool {
        self.blur.is_none()
            & self.edge_fade.is_none()
            & self.filter_blur.is_none()
            & self.brightness.is_none()
            & self.contrast.is_none()
            & self.saturate.is_none()
            & self.mask_offset.is_none()
            & self.clip_inset.is_none()
    }
}
impl StylesFields {
    /// The properties few styles set (`Overflow`).
    #[inline]
    pub(crate) fn overflow(&self) -> &Overflow {
        self.overflow.get()
    }
    /// The same, to write (with its bits set: `Styles::fields_mut`).
    #[inline]
    pub(crate) fn overflow_mut(&mut self) -> &mut Overflow {
        self.overflow.get_mut()
    }
    /// Its paint properties among them.
    #[inline]
    pub(crate) fn overflow_paint(&self) -> OverflowPaint {
        let o = self.overflow();
        OverflowPaint {
            blur: o.blur,
            edge_fade: o.edge_fade,
            filter_blur: o.filter_blur,
            brightness: o.brightness,
            contrast: o.contrast,
            saturate: o.saturate,
            mask_offset: o.mask_offset,
            clip_inset: o.clip_inset,
        }
    }
    /// Write them; all unset over none set writes nothing (and allocates
    /// nothing): unallocated, they read unset already.
    #[inline]
    pub(crate) fn set_overflow_paint(&mut self, paint: OverflowPaint) {
        if !self.overflow.is_allocated() && paint.is_unset() {
            return;
        }
        self.write_overflow_paint(paint);
    }
    /// Write them, unguarded (`compose_style::Paint::put` tests its own).
    #[inline(never)]
    pub(crate) fn write_overflow_paint(&mut self, paint: OverflowPaint) {
        let o = self.overflow_mut();
        (
            o.blur,
            o.edge_fade,
            o.filter_blur,
            o.brightness,
            o.contrast,
            o.saturate,
            o.mask_offset,
            o.clip_inset,
        ) = (
            paint.blur,
            paint.edge_fade,
            paint.filter_blur,
            paint.brightness,
            paint.contrast,
            paint.saturate,
            paint.mask_offset,
            paint.clip_inset,
        );
    }
    /// Take them out, leaving them unset: those of `set` (`Styles::set`
    /// bits), the others unset already. Allocates nothing.
    #[inline]
    pub(crate) fn take_overflow_paint(&mut self, set: u64) -> OverflowPaint {
        let Some(o) = self.overflow.get_allocated_mut() else {
            return OverflowPaint::default();
        };
        macro_rules! taken {
            ($field:ident, $bit:ident) => {
                match set & bit::$bit {
                    0 => Default::default(),
                    _ => std::mem::take(&mut o.$field),
                }
            };
        }
        OverflowPaint {
            blur: taken!(blur, BLUR),
            edge_fade: taken!(edge_fade, EDGE_FADE),
            filter_blur: taken!(filter_blur, FILTER_BLUR),
            brightness: taken!(brightness, BRIGHTNESS),
            contrast: taken!(contrast, CONTRAST),
            saturate: taken!(saturate, SATURATE),
            mask_offset: taken!(mask_offset, MASK_OFFSET),
            clip_inset: taken!(clip_inset, CLIP_INSET),
        }
    }
}
/// A set of style properties, each unset until a builder sets it: a
/// view's own, or a reusable patch merged over one ([`Styled::style`]).
///
/// Its fields are read through it and written through `fields_mut`, which
/// marks them in `set`: comparing, the commonest thing done with styles
/// (a style effect's every re-run), visits only the fields set in either.
#[derive(Default)]
pub struct Styles {
    fields: StylesFields,
    /// A bit (`bit`) for each field that may differ from its default: a
    /// field whose bit is clear is its default.
    set: u64,
}
impl std::ops::Deref for Styles {
    type Target = StylesFields;
    #[inline]
    fn deref(&self) -> &StylesFields {
        &self.fields
    }
}
impl std::fmt::Debug for Styles {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.fields.fmt(f)
    }
}
impl PartialEq for Styles {
    #[inline]
    fn eq(&self, other: &Self) -> bool {
        self.fields.eq_in(&other.fields, self.set | other.set)
    }
}
impl From<StylesFields> for Styles {
    fn from(fields: StylesFields) -> Self {
        Self {
            set: fields.mask(),
            fields,
        }
    }
}
impl Styles {
    /// The fields to write, those of `bits` (`bit`) marked set: a field
    /// written must have its bit here.
    #[inline]
    pub(crate) fn fields_mut(&mut self, bits: u64) -> &mut StylesFields {
        self.set |= bits;
        &mut self.fields
    }
    /// The fields to take, those of `bits` marked unset: the caller leaves
    /// each of them at its default (`take`n out).
    #[inline]
    pub(crate) fn fields_taken(&mut self, bits: u64) -> &mut StylesFields {
        self.set &= !bits;
        &mut self.fields
    }
    /// The fields that may be set (see `set`).
    #[inline]
    pub(crate) fn set_bits(&self) -> u64 {
        self.set
    }
}
impl Styles {
    /// Whether `self` and `other` are equal but for their box fields (width,
    /// height, their percentages and margins: what a growing bar changes),
    /// and if so, whether those are equal too (then `==`), comparing only
    /// fields set in either: a style effect re-run's two comparisons in
    /// one pass, ignoring `without`'s fields. Out of line so the effect
    /// calling it keeps its size.
    #[inline]
    pub(crate) fn eq_then_sizes(&self, other: &Self, without: u64) -> Option<bool> {
        let sizes =
            bit::WIDTH | bit::HEIGHT | bit::WIDTH_PERCENT | bit::HEIGHT_PERCENT | bit::MARGIN;
        let set = (self.set | other.set) & !without;
        self.fields
            .eq_in(&other.fields, set & !sizes)
            .then(|| self.fields.eq_in(&other.fields, set & sizes))
    }
}
/// As derived, but copying the plain fields as one block: a derived clone
/// moves each of some fifty `Option`s on its own (a load and store per tag
/// and per value), which a style effect re-running every frame pays for.
impl Clone for Styles {
    #[inline]
    fn clone(&self) -> Self {
        // SAFETY: `self` is a valid, aligned `Styles`, so reading it bitwise
        // gives a valid value: `copy`, whose six owning fields alias
        // `self`'s. They are `Arc`s, enums of `Arc`s and an `OverflowSlot`
        // (a pointer, no `Box`, whose uniqueness an alias would break), and
        // `ManuallyDrop` keeps `copy` from dropping them, so `self` keeps
        // sole ownership of its own.
        let mut copy = std::mem::ManuallyDrop::new(unsafe { std::ptr::read(self) });
        // SAFETY: each aliasing field is overwritten with its own clone;
        // `ptr::write` drops nothing, so `self`'s are not released. Every
        // other field is `Copy` (`styles_clone_covers_every_owning_field`
        // fails to compile otherwise), so `copy` then owns exactly what it
        // holds. A clone that panicked would leave `copy` undropped, leaking
        // the clones made so far but freeing nothing of `self`'s.
        unsafe {
            let (copy, own) = (&mut copy.fields, &self.fields);
            std::ptr::write(&mut copy.layout_options, own.layout_options.clone());
            std::ptr::write(&mut copy.font_family, own.font_family.clone());
            std::ptr::write(&mut copy.paint_background, own.paint_background.clone());
            std::ptr::write(&mut copy.rare, own.rare.clone());
            std::ptr::write(&mut copy.tokens, own.tokens.clone());
            std::ptr::write(&mut copy.overflow, own.overflow.clone());
        }
        std::mem::ManuallyDrop::into_inner(copy)
    }
}
/// Fails to compile if a field that is not `Copy` is added to `Styles`
/// without `Clone for Styles` cloning it: every other field is copied here
/// with `..*styles`, which only `Copy` fields allow.
#[allow(dead_code)]
fn styles_clone_covers_every_owning_field(styles: &StylesFields) -> StylesFields {
    StylesFields {
        layout_options: None,
        font_family: None,
        paint_background: None,
        rare: None,
        tokens: None,
        overflow: OverflowSlot::default(),
        ..*styles
    }
}
/// Scale, rotation, skew, tilt, perspective and origin, each set or not
/// (see `Styled::scale`).
/// Inline rather than among `RareStyles`: animations rewrite it every frame,
/// and a bit per component keeps it a third the size of four `Option`s.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct TransformPatch {
    pub(crate) values: LocalTransform,
    pub(crate) set: u8,
}
impl TransformPatch {
    const SCALE: u8 = 1;
    const ROTATE: u8 = 2;
    const SKEW: u8 = 4;
    const ORIGIN: u8 = 8;
    const ROTATE_X: u8 = 16;
    const ROTATE_Y: u8 = 32;
    const PERSPECTIVE: u8 = 64;
    const TILT: u8 = Self::ROTATE_X | Self::ROTATE_Y | Self::PERSPECTIVE;
    /// `self` with what `over` sets laid over it.
    #[inline]
    pub(crate) fn over(mut self, over: Self) -> Self {
        // Most styles set no transform; few of those that do tilt.
        if over.set == 0 {
            return self;
        }
        let pick = |bit: u8| over.set & bit != 0;
        if pick(Self::SCALE) {
            self.values.scale = over.values.scale;
        }
        if pick(Self::ROTATE) {
            self.values.rotate = over.values.rotate;
        }
        if pick(Self::SKEW) {
            self.values.skew = over.values.skew;
        }
        if pick(Self::ORIGIN) {
            self.values.origin = over.values.origin;
        }
        if pick(Self::TILT) {
            if pick(Self::ROTATE_X) {
                self.values.rotate_x = over.values.rotate_x;
            }
            if pick(Self::ROTATE_Y) {
                self.values.rotate_y = over.values.rotate_y;
            }
            if pick(Self::PERSPECTIVE) {
                self.values.perspective = over.values.perspective;
            }
        }
        self.set |= over.set;
        self
    }
    /// `base` with the set components replaced.
    #[inline]
    pub(crate) fn resolve(self, base: LocalTransform) -> LocalTransform {
        Self {
            values: base,
            set: 0,
        }
        .over(self)
        .values
    }
    /// As `resolve`, normalized (`LocalTransform::normalized`), over a
    /// normalized `base` (as the scene keeps them): with nothing overridden,
    /// `base` as it is.
    #[inline]
    pub(crate) fn resolve_finite(self, base: LocalTransform) -> LocalTransform {
        if self.set == 0 {
            base
        } else {
            self.resolve(base).normalized()
        }
    }
}
/// Properties few views set, kept apart and shared so that every `Styles`
/// (a view's, its node's style effect's and that effect's memo) stays small:
/// a `Styles` is copied and compared on every re-run.
/// The largest of what `RareStyles` holds, and what few of the styles that
/// have one set: apart, so a node with a `min_width` doesn't carry a
/// shader's uniforms (144 of its bytes) on each copy.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct ColdStyles {
    /// `Styled::shader_uniforms`.
    pub(crate) shader_uniforms: Option<crate::decoration::ShaderUniforms>,
    /// `Styled::layer_uniforms`.
    pub(crate) layer_uniforms: Option<crate::decoration::ShaderUniforms>,
    /// `Styled::grid_template_columns` and `grid_template_rows`.
    pub(crate) grid_template: Option<std::sync::Arc<crate::grid_tracks::GridTemplate>>,
    /// `Styled::grid_area`, `grid_column` and `grid_row`.
    pub(crate) grid_place: Option<std::sync::Arc<crate::grid_tracks::GridPlace>>,
    /// `Styled::z`.
    pub(crate) z: Option<i32>,
    /// `Styled::sticky_top`.
    pub(crate) sticky_top: Option<f32>,
    /// `Styled::object_position`.
    pub(crate) object_position: Option<(f32, f32)>,
    /// `Styled::text_shadow` and `text_shadows`.
    pub(crate) text_shadows: Option<std::sync::Arc<[crate::scene::TextShadow]>>,
    /// `Styled::w_calc` and the like.
    pub(crate) size_calcs: Option<std::sync::Arc<crate::size_calc::SizeCalcs>>,
    /// `Styled::text_balance`.
    pub(crate) text_balance: Option<bool>,
}
/// How an image sits in its box: `Styled::object_fit` and `object_position`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct ImagePlacement {
    pub(crate) fit: ObjectFit,
    pub(crate) position: (f32, f32),
}
impl Default for ImagePlacement {
    fn default() -> Self {
        Self {
            fit: ObjectFit::default(),
            position: (0.5, 0.5),
        }
    }
}
/// What of `styles`' size is worked out from its containing block.
fn size_calcs(styles: &mut Styles) -> &mut crate::size_calc::SizeCalcs {
    // A parent lays out what has options with taffy, the only layout that
    // reads these: a value that changes nothing, to say so.
    styles.layout_options_mut().reverse.get_or_insert(false);
    let cold = styles.rare_mut().cold.get_or_insert_default();
    std::sync::Arc::make_mut(cold.size_calcs.get_or_insert_default())
}
/// Where `styles` lies in its grid by name or line.
fn grid_place(styles: &mut Styles) -> &mut crate::grid_tracks::GridPlace {
    // Its parent maps it for taffy (`map_cold_child`) where it maps what has
    // options: a value that changes nothing, to say so.
    styles.layout_options_mut().reverse.get_or_insert(false);
    let cold = styles.rare_mut().cold.get_or_insert_default();
    std::sync::Arc::make_mut(cold.grid_place.get_or_insert_default())
}
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct RareStyles {
    pub(crate) min_width: Option<f32>,
    pub(crate) max_width: Option<f32>,
    pub(crate) min_height: Option<f32>,
    pub(crate) max_height: Option<f32>,
    pub(crate) fade_edges: Option<[f32; 2]>,
    pub(crate) fade_insets: Option<[f32; 2]>,
    pub(crate) fade_edges_x: Option<[f32; 2]>,
    pub(crate) fade_insets_x: Option<[f32; 2]>,
    pub(crate) fade_overflow: Option<bool>,
    pub(crate) snap: Option<bool>,
    /// `Styled::direction`: which way its lines run.
    pub(crate) direction: Option<crate::scene::Direction>,
    pub(crate) border_style: Option<crate::decoration::BorderStyle>,
    /// `Styled::border_over`.
    pub(crate) border_over: Option<bool>,
    /// `Styled::outline`, `outline_offset` (width and colour; offset).
    pub(crate) outline: Option<Option<(f32, Color)>>,
    pub(crate) outline_offset: Option<f32>,
    /// What fewer styles still set (`ColdStyles`).
    pub(crate) cold: Option<Box<ColdStyles>>,
    pub(crate) shadows: Option<Option<std::sync::Arc<[BoxShadow]>>>,
    /// `Styled::inset_shadows`.
    pub(crate) inset_shadows: Option<Option<std::sync::Arc<[BoxShadow]>>>,
    /// `Styled::clip_feather`.
    pub(crate) clip_feather: Option<f32>,
    pub(crate) shadow: Option<Option<BoxShadow>>,
    pub(crate) letter_spacing: Option<crate::text_layout::LetterSpacing>,
    /// `Styled::text_underline`, `underline_offset`.
    pub(crate) underline: Option<bool>,
    pub(crate) underline_offset: Option<f32>,
    pub(crate) text_overflow: Option<crate::text_layout::TextOverflow>,
    pub(crate) line_clamp: Option<Option<std::num::NonZeroU32>>,
    pub(crate) font_features: Option<crate::text_layout::FontFeatures>,
    pub(crate) font_fallbacks: Option<std::sync::Arc<[crate::text_layout::FontFamily]>>,
    /// A mask image and the size it is drawn at (`Styled::mask`).
    pub(crate) mask: Option<Option<MaskImage>>,
    /// `Styled::layer_shader`, `layer_uniforms`.
    pub(crate) layer_shader: Option<Option<std::sync::Arc<crate::decoration::FillShader>>>,
    /// `Styled::blend`.
    pub(crate) blend: Option<crate::scene::BlendMode>,
    /// `Styled::relative`: a containing block for absolute descendants.
    pub(crate) relative: Option<bool>,
    /// `Styled::zoom`, `zoom_fixed`.
    pub(crate) zoom: Option<ZoomStyle>,
}
/// A node's zoom as a style gives it (`Styled::zoom`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum ZoomStyle {
    /// Its subtree's lengths times this.
    By(f32),
    /// At its own size, whatever zooms are around it.
    Fixed,
}
/// A mask image and the size (logical pixels) it is drawn at.
pub(crate) type MaskImage = (std::sync::Arc<crate::image::ImageData>, (f32, f32));
impl RareStyles {
    const NONE: Self = Self {
        min_width: None,
        max_width: None,
        min_height: None,
        max_height: None,
        fade_edges: None,
        fade_insets: None,
        fade_edges_x: None,
        fade_insets_x: None,
        fade_overflow: None,
        snap: None,
        direction: None,
        border_style: None,
        border_over: None,
        outline: None,
        outline_offset: None,
        cold: None,
        shadows: None,
        inset_shadows: None,
        clip_feather: None,
        shadow: None,
        letter_spacing: None,
        underline: None,
        underline_offset: None,
        text_overflow: None,
        line_clamp: None,
        font_features: None,
        font_fallbacks: None,
        mask: None,
        layer_shader: None,
        blend: None,
        relative: None,
        zoom: None,
    };
    /// Overlay explicitly supplied properties, preserving all others.
    fn merge(&mut self, other: &Self) {
        if other.min_width.is_some() {
            self.min_width = other.min_width;
        }
        if other.max_width.is_some() {
            self.max_width = other.max_width;
        }
        if other.min_height.is_some() {
            self.min_height = other.min_height;
        }
        if other.max_height.is_some() {
            self.max_height = other.max_height;
        }
        if other.fade_edges.is_some() {
            self.fade_edges = other.fade_edges;
        }
        if other.fade_insets.is_some() {
            self.fade_insets = other.fade_insets;
        }
        if other.fade_edges_x.is_some() {
            self.fade_edges_x = other.fade_edges_x;
        }
        if other.fade_insets_x.is_some() {
            self.fade_insets_x = other.fade_insets_x;
        }
        if other.fade_overflow.is_some() {
            self.fade_overflow = other.fade_overflow;
        }
        if other.snap.is_some() {
            self.snap = other.snap;
        }
        if other.direction.is_some() {
            self.direction = other.direction;
        }
        if other.border_over.is_some() {
            self.border_over = other.border_over;
        }
        if other.outline.is_some() {
            self.outline = other.outline;
        }
        if other.outline_offset.is_some() {
            self.outline_offset = other.outline_offset;
        }
        if let Some(theirs) = &other.cold {
            let mine = self.cold.get_or_insert_default();
            if theirs.shader_uniforms.is_some() {
                mine.shader_uniforms = theirs.shader_uniforms;
            }
            if let Some(sizes) = &theirs.size_calcs {
                match &mut mine.size_calcs {
                    Some(mine) => std::sync::Arc::make_mut(mine).merge(sizes),
                    None => mine.size_calcs = Some(sizes.clone()),
                }
            }
            if theirs.layer_uniforms.is_some() {
                mine.layer_uniforms = theirs.layer_uniforms;
            }
            if theirs.object_position.is_some() {
                mine.object_position = theirs.object_position;
            }
            if theirs.text_balance.is_some() {
                mine.text_balance = theirs.text_balance;
            }
            if theirs.text_shadows.is_some() {
                mine.text_shadows = theirs.text_shadows.clone();
            }
            if theirs.z.is_some() {
                mine.z = theirs.z;
            }
            if let Some(template) = &theirs.grid_template {
                match &mut mine.grid_template {
                    Some(mine) => {
                        let mine = std::sync::Arc::make_mut(mine);
                        if template.columns.is_some() {
                            mine.columns = template.columns.clone();
                        }
                        if template.rows.is_some() {
                            mine.rows = template.rows.clone();
                        }
                        if template.areas.is_some() {
                            mine.areas = template.areas.clone();
                        }
                        if template.flow.is_some() {
                            mine.flow = template.flow;
                        }
                    }
                    None => mine.grid_template = Some(template.clone()),
                }
            }
            if let Some(place) = &theirs.grid_place {
                match &mut mine.grid_place {
                    Some(mine) => {
                        let mine = std::sync::Arc::make_mut(mine);
                        if place.column.is_some() {
                            mine.column = place.column.clone();
                        }
                        if place.row.is_some() {
                            mine.row = place.row.clone();
                        }
                    }
                    None => mine.grid_place = Some(place.clone()),
                }
            }
        }
        if other.border_style.is_some() {
            self.border_style = other.border_style;
        }
        if other.shadows.is_some() {
            self.shadows = other.shadows.clone();
        }
        if other.inset_shadows.is_some() {
            self.inset_shadows = other.inset_shadows.clone();
        }
        if other.clip_feather.is_some() {
            self.clip_feather = other.clip_feather;
        }
        if other.shadow.is_some() {
            self.shadow = other.shadow;
        }
        if other.letter_spacing.is_some() {
            self.letter_spacing = other.letter_spacing;
        }
        if other.underline.is_some() {
            self.underline = other.underline;
        }
        if other.underline_offset.is_some() {
            self.underline_offset = other.underline_offset;
        }
        if other.text_overflow.is_some() {
            self.text_overflow = other.text_overflow;
        }
        if other.line_clamp.is_some() {
            self.line_clamp = other.line_clamp;
        }
        if other.font_features.is_some() {
            self.font_features = other.font_features.clone();
        }
        if other.font_fallbacks.is_some() {
            self.font_fallbacks = other.font_fallbacks.clone();
        }
        if other.mask.is_some() {
            self.mask = other.mask.clone();
        }
        if other.layer_shader.is_some() {
            self.layer_shader = other.layer_shader.clone();
        }
        if other.blend.is_some() {
            self.blend = other.blend;
        }
        if other.relative.is_some() {
            self.relative = other.relative;
        }
        if other.zoom.is_some() {
            self.zoom = other.zoom;
        }
    }
}
static NO_RARE: RareStyles = RareStyles::NONE;
impl Styles {
    /// Its rare properties, if it has any.
    #[inline(always)]
    pub(crate) fn rare_set(&self) -> Option<&RareStyles> {
        self.rare.as_deref()
    }
    pub(crate) fn rare(&self) -> &RareStyles {
        self.rare.as_deref().unwrap_or(&NO_RARE)
    }
    pub(crate) fn rare_mut(&mut self) -> &mut RareStyles {
        std::sync::Arc::make_mut(
            self.fields_mut(bit::RARE)
                .rare
                .get_or_insert_with(Default::default),
        )
    }
    /// Whether any typography field (what text inherits) is set.
    pub(crate) fn sets_typography(&self) -> bool {
        self.rare().text_overflow.is_some()
            || self.rare().line_clamp.is_some()
            || self.text_color.is_some()
            || self.text_size.is_some()
            || self.text_wrap.is_some()
            || self.font_family.is_some()
            || self.rare().font_features.is_some()
            || self.text_align.is_some()
            || self.rare().font_fallbacks.is_some()
            || self.font_weight.is_some()
            || self.overflow().italic.is_some()
            || self.line_height.is_some()
            || self.rare().letter_spacing.is_some()
            || self.rare().underline.is_some()
            || self.rare().underline_offset.is_some()
    }
    pub(crate) fn layout_options_mut(&mut self) -> &mut crate::layout::LayoutOptions {
        std::sync::Arc::make_mut(
            self.fields_mut(bit::LAYOUT_OPTIONS)
                .layout_options
                .get_or_insert_with(Default::default),
        )
    }
    fn clear_layout_spacing(&mut self, margin: bool, mask: u8) {
        let Some(options) = &mut self.fields_mut(bit::LAYOUT_OPTIONS).layout_options else {
            return;
        };
        let options = std::sync::Arc::make_mut(options);
        let edges = if margin {
            &mut options.margin
        } else {
            &mut options.padding
        };
        if mask & 1 != 0 {
            edges.top = None;
        }
        if mask & 2 != 0 {
            edges.right = None;
        }
        if mask & 4 != 0 {
            edges.bottom = None;
        }
        if mask & 8 != 0 {
            edges.left = None;
        }
        if options.is_empty() {
            self.fields_mut(bit::LAYOUT_OPTIONS).layout_options = None;
        }
    }
    /// No properties set.
    pub fn new() -> Self {
        Self::default()
    }
    /// Whether any property is set by a theme token.
    pub fn has_tokens(&self) -> bool {
        self.tokens.is_some()
    }
    /// Overlay explicitly supplied properties, preserving all others.
    pub fn merge(&mut self, other: &Self) {
        // Nothing set: nothing to overlay.
        if other.set == 0 {
            return;
        }
        if self.tokens.is_some() || other.tokens.is_some() {
            crate::theme::merge(self, other);
        }
        if other.padding.is_some() {
            self.clear_layout_spacing(
                false,
                if other.padding_sides == 0 {
                    15
                } else {
                    other.padding_sides
                },
            );
        }
        if other.margin.is_some() {
            self.clear_layout_spacing(
                true,
                if other.margin_sides == 0 {
                    15
                } else {
                    other.margin_sides
                },
            );
        }
        if let Some(options) = &mut self.fields_mut(bit::LAYOUT_OPTIONS).layout_options
            && (other.gap.is_some()
                || other.clip.is_some()
                || other.rare().min_width.is_some()
                || other.rare().max_width.is_some()
                || other.rare().min_height.is_some()
                || other.rare().max_height.is_some()
                || other.overflow().layout.is_some())
        {
            let options = std::sync::Arc::make_mut(options);
            if other.gap.is_some() {
                options.gap_x = None;
                options.gap_y = None;
            }
            if other.clip.is_some() {
                options.clip_x = None;
                options.clip_y = None;
            }
            if other.rare().min_width.is_some() {
                options.min_width = None;
            }
            if other.rare().max_width.is_some() {
                options.max_width = None;
            }
            if other.rare().min_height.is_some() {
                options.min_height = None;
            }
            if other.rare().max_height.is_some() {
                options.max_height = None;
            }
            if other.overflow().layout.is_some() {
                options.reverse = None;
            }
        }
        if let Some(options) = &other.layout_options {
            if let Some(current) = &self.layout_options {
                let mut merged = **current;
                merged.merge(**options);
                if merged != **current {
                    self.fields_mut(bit::LAYOUT_OPTIONS).layout_options =
                        Some(std::sync::Arc::new(merged));
                }
            } else {
                self.fields_mut(bit::LAYOUT_OPTIONS).layout_options = Some(options.clone());
            }
        }
        if let Some(rare) = &other.rare {
            match &mut self.fields_mut(bit::RARE).rare {
                Some(own) => std::sync::Arc::make_mut(own).merge(rare),
                None => self.fields_mut(bit::RARE).rare = Some(rare.clone()),
            }
        }
        if other.paint_background.is_some() {
            self.fields_mut(bit::PAINT_BACKGROUND).paint_background =
                other.paint_background.clone();
        }
        if other.corners.is_some() {
            self.fields_mut(bit::CORNERS).corners = other.corners;
        }
        if other.border_edges.is_some() {
            self.fields_mut(bit::BORDER_EDGES).border_edges = other.border_edges;
        }
        if other.overflow().object_fit.is_some() {
            self.fields_mut(bit::OBJECT_FIT).overflow_mut().object_fit =
                other.overflow().object_fit;
        }
        if other.absolute.is_some() {
            self.fields_mut(bit::ABSOLUTE).absolute = other.absolute;
        }
        if other.overflow().layout.is_some() {
            self.fields_mut(bit::LAYOUT).overflow_mut().layout = other.overflow().layout;
        }
        if other.width.is_some() || other.width_percent.is_some() {
            self.fields_mut(bit::WIDTH).width = other.width;
            self.fields_mut(bit::WIDTH_PERCENT).width_percent = other.width_percent;
        }
        if other.height.is_some() || other.height_percent.is_some() {
            self.fields_mut(bit::HEIGHT).height = other.height;
            self.fields_mut(bit::HEIGHT_PERCENT).height_percent = other.height_percent;
        }
        if other.gap.is_some() {
            self.fields_mut(bit::GAP).gap = other.gap;
        }
        if other.clip.is_some() {
            self.fields_mut(bit::CLIP).clip = other.clip;
        }
        if other.flex_grow.is_some() {
            self.fields_mut(bit::FLEX_GROW).flex_grow = other.flex_grow;
        }
        if other.flex_shrink.is_some() {
            self.fields_mut(bit::FLEX_SHRINK).flex_shrink = other.flex_shrink;
        }
        if other.align.is_some() {
            self.fields_mut(bit::ALIGN).align = other.align;
        }
        if other.justify.is_some() {
            self.fields_mut(bit::JUSTIFY).justify = other.justify;
        }
        if other.text_wrap.is_some() {
            self.fields_mut(bit::TEXT_WRAP).text_wrap = other.text_wrap;
        }
        if other.cursor.is_some() {
            self.fields_mut(bit::CURSOR).cursor = other.cursor;
        }
        if other.background.is_some() {
            self.fields_mut(bit::BACKGROUND).background = other.background;
        }
        if other.text_color.is_some() {
            self.fields_mut(bit::TEXT_COLOR).text_color = other.text_color;
        }
        if other.text_align.is_some() {
            self.fields_mut(bit::TEXT_ALIGN).text_align = other.text_align;
        }
        if other.font_family.is_some() {
            self.fields_mut(bit::FONT_FAMILY).font_family = other.font_family.clone();
        }
        if other.font_weight.is_some() {
            self.fields_mut(bit::FONT_WEIGHT).font_weight = other.font_weight;
        }
        if other.overflow().italic.is_some() {
            self.fields_mut(bit::ITALIC).overflow_mut().italic = other.overflow().italic;
        }
        if other.text_size.is_some() {
            self.fields_mut(bit::TEXT_SIZE).text_size = other.text_size;
        }
        if other.line_height.is_some() {
            self.fields_mut(bit::LINE_HEIGHT).line_height = other.line_height;
        }
        if other.radius.is_some() {
            self.fields_mut(bit::RADIUS).radius = other.radius;
        }
        if other.border_width.is_some() {
            self.fields_mut(bit::BORDER_WIDTH).border_width = other.border_width;
        }
        if other.border_color.is_some() {
            self.fields_mut(bit::BORDER_COLOR).border_color = other.border_color;
        }
        if other.opacity.is_some() {
            self.fields_mut(bit::OPACITY).opacity = other.opacity;
        }
        if other.overflow().blur.is_some() {
            self.fields_mut(bit::BLUR).overflow_mut().blur = other.overflow().blur;
        }
        if other.overflow().edge_fade.is_some() {
            self.fields_mut(bit::EDGE_FADE).overflow_mut().edge_fade = other.overflow().edge_fade;
        }
        if other.overflow().filter_blur.is_some() {
            self.fields_mut(bit::FILTER_BLUR).overflow_mut().filter_blur =
                other.overflow().filter_blur;
        }
        if other.overflow().brightness.is_some() {
            self.fields_mut(bit::BRIGHTNESS).overflow_mut().brightness =
                other.overflow().brightness;
        }
        if other.overflow().contrast.is_some() {
            self.fields_mut(bit::CONTRAST).overflow_mut().contrast = other.overflow().contrast;
        }
        if other.overflow().saturate.is_some() {
            self.fields_mut(bit::SATURATE).overflow_mut().saturate = other.overflow().saturate;
        }
        if other.overflow().mask_offset.is_some() {
            self.fields_mut(bit::MASK_OFFSET).overflow_mut().mask_offset =
                other.overflow().mask_offset;
        }
        if other.overflow().clip_inset.is_some() {
            self.fields_mut(bit::CLIP_INSET).overflow_mut().clip_inset =
                other.overflow().clip_inset;
        }
        if other.transform.is_some() {
            self.fields_mut(bit::TRANSFORM).transform = other.transform;
        }
        let local_transform = self.local_transform.over(other.local_transform);
        self.fields_mut(bit::LOCAL_TRANSFORM).local_transform = local_transform;
        if other.overflow().isolated.is_some() {
            self.fields_mut(bit::ISOLATED).overflow_mut().isolated = other.overflow().isolated;
        }
        if let Some(value) = other.padding {
            if self.padding.is_some() && self.padding_sides == 0 {
                self.fields_mut(bit::PADDING_SIDES).padding_sides = 15;
            }
            let mask = if other.padding_sides == 0 {
                15
            } else {
                other.padding_sides
            };
            merge_insets(
                self.fields_mut(bit::PADDING)
                    .padding
                    .get_or_insert_with(Insets::default),
                value,
                mask,
            );
            self.fields_mut(bit::PADDING_SIDES).padding_sides |= mask;
        }
        if let Some(value) = other.margin {
            if self.margin.is_some() && self.margin_sides == 0 {
                self.fields_mut(bit::MARGIN_SIDES).margin_sides = 15;
            }
            let mask = if other.margin_sides == 0 {
                15
            } else {
                other.margin_sides
            };
            merge_insets(
                self.fields_mut(bit::MARGIN)
                    .margin
                    .get_or_insert_with(Insets::default),
                value,
                mask,
            );
            self.fields_mut(bit::MARGIN_SIDES).margin_sides |= mask;
        }
    }
}
pub(crate) fn merge_insets(target: &mut Insets, source: Insets, mask: u8) {
    if mask & 1 != 0 {
        target.top = source.top;
    }
    if mask & 2 != 0 {
        target.right = source.right;
    }
    if mask & 4 != 0 {
        target.bottom = source.bottom;
    }
    if mask & 8 != 0 {
        target.left = source.left;
    }
}
/// Take a setter's value; a theme token stands for a placeholder value the
/// setter writes as any other, and the property is bound to the token (its
/// value is written when the node is styled). A later value replaces the
/// binding. One path through the setter keeps chained setters moving the
/// style in place.
macro_rules! value_or_token {
    ($self:ident, $value:ident, $field:expr) => {
        let ($value, token) = crate::theme::split($value.into());
        if token.is_some() || $self.styles_mut().tokens.is_some() {
            $self = crate::theme::rebound($self, $field, token);
        }
    };
}
/// Shared fluent styling API for views and reusable style patches.
///
/// Colours, lengths, typography and shadows take a value or a theme token
/// (`.bg(tok::SURFACE_RAISED)`, `.p(tok::SPACE_3)`; see [`crate::theme`]).
pub trait Styled: Sized {
    /// The styles the builders set.
    fn styles_mut(&mut self) -> &mut Styles;
    /// Set `field` from the theme token `token` (what the typed setters do
    /// for tokens; scripts use it for `"$name"` values).
    fn token(
        mut self,
        field: crate::theme::Field,
        token: impl Into<crate::theme::TokenId>,
    ) -> Self {
        crate::theme::bind_in(self.styles_mut(), field, token.into());
        self
    }
    /// Set the colour `field` to `t` of the way from the token `from` to
    /// `to` (a colour or a token), both read from the node's theme: an
    /// animation between theme colours that follows theme changes.
    fn token_mix(
        mut self,
        field: crate::theme::Field,
        from: impl Into<crate::theme::TokenId>,
        to: impl Into<crate::theme::Themed<Color>>,
        t: f32,
    ) -> Self {
        crate::theme::bind_mix(self.styles_mut(), field, from.into(), to.into(), t);
        self
    }
    /// Restore flex display after `hidden` or `grid`, preserving its direction.
    fn flex(mut self) -> Self {
        self.styles_mut().layout_options_mut().display = Some(crate::layout::Display::Flex);
        self
    }
    /// Merge advanced layout options without changing unrelated style properties.
    fn layout_options(mut self, options: crate::layout::LayoutOptions) -> Self {
        self.styles_mut().layout_options_mut().merge(options);
        self
    }
    /// Lay the children out in a grid (CSS `display: grid`; see `grid_cols`).
    fn grid(mut self) -> Self {
        self.styles_mut().layout_options_mut().display = Some(crate::layout::Display::Grid);
        self
    }
    /// Remove the view from layout and interaction (CSS `display: none`); it
    /// keeps its component state. `flex` or `grid` shows it again.
    fn hidden(mut self) -> Self {
        self.styles_mut().layout_options_mut().display = Some(crate::layout::Display::None);
        self
    }
    /// Paint the view again after `invisible`.
    fn visible(mut self) -> Self {
        self.styles_mut().layout_options_mut().visible = Some(true);
        self
    }
    /// Keep the view's room in layout but paint none of it and take no
    /// input (CSS `visibility: hidden`).
    fn invisible(mut self) -> Self {
        self.styles_mut().layout_options_mut().visible = Some(false);
        self
    }
    /// Wrap the children onto more lines when they overflow the row or
    /// column (CSS `flex-wrap: wrap`).
    fn flex_wrap(mut self) -> Self {
        self.styles_mut().layout_options_mut().wrap = Some(crate::layout::FlexWrap::Wrap);
        self
    }
    /// Wrap, with each new line before the last (`flex-wrap: wrap-reverse`).
    fn flex_wrap_reverse(mut self) -> Self {
        self.styles_mut().layout_options_mut().wrap = Some(crate::layout::FlexWrap::Reverse);
        self
    }
    /// Keep the children on one line (`flex-wrap: nowrap`).
    fn flex_nowrap(mut self) -> Self {
        self.styles_mut().layout_options_mut().wrap = Some(crate::layout::FlexWrap::NoWrap);
        self
    }
    /// Lay the children out in a row, last first (`flex-direction:
    /// row-reverse`).
    fn flex_row_reverse(mut self) -> Self {
        self.styles_mut()
            .fields_mut(bit::LAYOUT)
            .overflow_mut()
            .layout = Some(Layout::Row);
        self.styles_mut().layout_options_mut().reverse = Some(true);
        self
    }
    /// Lay the children out in a column, last first (`flex-direction:
    /// column-reverse`).
    fn flex_col_reverse(mut self) -> Self {
        self.styles_mut()
            .fields_mut(bit::LAYOUT)
            .overflow_mut()
            .layout = Some(Layout::Column);
        self.styles_mut().layout_options_mut().reverse = Some(true);
        self
    }
    /// The size the view starts from along its parent's main axis before
    /// growing or shrinking (CSS `flex-basis`).
    fn flex_basis(mut self, value: impl Into<crate::layout::Length>) -> Self {
        self.styles_mut().layout_options_mut().basis = Some(value.into());
        self
    }
    /// Align this child across its parent's axis, overriding the parent's
    /// `items_*` (CSS `align-self`).
    fn align_self(mut self, value: Align) -> Self {
        self.styles_mut().layout_options_mut().align_self = Some(value);
        self
    }
    /// How a wrapping container spaces its lines across its axis (CSS
    /// `align-content`).
    fn align_content(mut self, value: crate::layout::ContentAlign) -> Self {
        self.styles_mut().layout_options_mut().align_content = Some(value);
        self
    }
    /// The room between columns, a length or percentage (CSS `column-gap`).
    fn gap_x(mut self, value: impl Into<crate::layout::Length>) -> Self {
        self.styles_mut().layout_options_mut().gap_x = Some(value.into());
        self
    }
    /// The room between rows, a length or percentage (CSS `row-gap`).
    fn gap_y(mut self, value: impl Into<crate::layout::Length>) -> Self {
        self.styles_mut().layout_options_mut().gap_y = Some(value.into());
        self
    }
    /// Width over height: the missing dimension follows from the other (CSS
    /// `aspect-ratio`).
    fn aspect_ratio(mut self, value: f32) -> Self {
        self.styles_mut().layout_options_mut().aspect_ratio = Some(value);
        self
    }
    /// Paint and hit it by `z` among its siblings, higher over lower (`0`, as
    /// written, keeps document order). Unlike CSS's `z-index` it orders
    /// siblings only; layout, focus and the accessibility tree keep document
    /// order, as in CSS.
    fn z(mut self, z: i32) -> Self {
        self.styles_mut().rare_mut().cold.get_or_insert_default().z = Some(z);
        self
    }
    /// Inside a scroll view, hold `top` below its top once scrolling would
    /// take this past, as far as its parent's box goes (CSS's `position:
    /// sticky`); it paints over its siblings unless it has a `z`.
    fn sticky_top(mut self, top: f32) -> Self {
        let cold = self.styles_mut().rare_mut().cold.get_or_insert_default();
        cold.sticky_top = Some(top);
        self
    }
    /// Its width from its containing block's (CSS's `calc()`, `min()`,
    /// `max()` and `clamp()`): `"calc(100% - 24px)".parse()`.
    fn w_calc(mut self, width: impl Into<std::sync::Arc<crate::size_calc::SizeCalc>>) -> Self {
        self.styles_mut().fields_mut(bit::WIDTH).width = Opt32::none();
        self.styles_mut()
            .fields_mut(bit::WIDTH_PERCENT)
            .width_percent = Opt32::none();
        size_calcs(self.styles_mut()).width = Some(width.into());
        self
    }
    /// Its height from its containing block's, as `w_calc`.
    fn h_calc(mut self, height: impl Into<std::sync::Arc<crate::size_calc::SizeCalc>>) -> Self {
        self.styles_mut().fields_mut(bit::HEIGHT).height = Opt32::none();
        self.styles_mut()
            .fields_mut(bit::HEIGHT_PERCENT)
            .height_percent = Opt32::none();
        size_calcs(self.styles_mut()).height = Some(height.into());
        self
    }
    /// Its smallest width from its containing block's, as `w_calc`.
    fn min_w_calc(mut self, width: impl Into<std::sync::Arc<crate::size_calc::SizeCalc>>) -> Self {
        size_calcs(self.styles_mut()).min_width = Some(width.into());
        self
    }
    /// Its largest width from its containing block's, as `w_calc`.
    fn max_w_calc(mut self, width: impl Into<std::sync::Arc<crate::size_calc::SizeCalc>>) -> Self {
        size_calcs(self.styles_mut()).max_width = Some(width.into());
        self
    }
    /// Its smallest height from its containing block's, as `w_calc`.
    fn min_h_calc(mut self, height: impl Into<std::sync::Arc<crate::size_calc::SizeCalc>>) -> Self {
        size_calcs(self.styles_mut()).min_height = Some(height.into());
        self
    }
    /// Its largest height from its containing block's, as `w_calc`.
    fn max_h_calc(mut self, height: impl Into<std::sync::Arc<crate::size_calc::SizeCalc>>) -> Self {
        size_calcs(self.styles_mut()).max_height = Some(height.into());
        self
    }
    /// A grid's columns of any size (CSS `grid-template-columns`), over
    /// `grid_cols`: `"220 1fr auto".parse()`.
    fn grid_template_columns(mut self, tracks: crate::grid_tracks::GridTracks) -> Self {
        let cold = self.styles_mut().rare_mut().cold.get_or_insert_default();
        let template = cold.grid_template.get_or_insert_default();
        std::sync::Arc::make_mut(template).columns = Some(std::sync::Arc::new(tracks));
        self
    }
    /// A grid's rows of any size (CSS `grid-template-rows`), over `grid_rows`.
    fn grid_template_rows(mut self, tracks: crate::grid_tracks::GridTracks) -> Self {
        let cold = self.styles_mut().rare_mut().cold.get_or_insert_default();
        let template = cold.grid_template.get_or_insert_default();
        std::sync::Arc::make_mut(template).rows = Some(std::sync::Arc::new(tracks));
        self
    }
    /// A grid's named areas (CSS `grid-template-areas`), which its items
    /// place themselves in (`grid_area`).
    fn grid_template_areas(mut self, areas: crate::grid_tracks::GridAreas) -> Self {
        let cold = self.styles_mut().rare_mut().cold.get_or_insert_default();
        let template = cold.grid_template.get_or_insert_default();
        std::sync::Arc::make_mut(template).areas = Some(std::sync::Arc::new(areas));
        self
    }
    /// How a grid places items without a place (CSS `grid-auto-flow`).
    fn grid_flow(mut self, flow: crate::grid_tracks::GridFlow) -> Self {
        let cold = self.styles_mut().rare_mut().cold.get_or_insert_default();
        let template = cold.grid_template.get_or_insert_default();
        std::sync::Arc::make_mut(template).flow = Some(flow);
        self
    }
    /// In its grid's area `name` (CSS `grid-area`).
    fn grid_area(self, name: &str) -> Self {
        let area = crate::grid_tracks::GridLines::area(name);
        self.grid_column(area.clone()).grid_row(area)
    }
    /// Its columns in its grid, by line, name or span (CSS `grid-column`):
    /// `"side / main"`, `"2 / span 3"`.
    fn grid_column(mut self, lines: crate::grid_tracks::GridLines) -> Self {
        grid_place(self.styles_mut()).column = Some(lines);
        self
    }
    /// Its rows in its grid, as `grid_column` (CSS `grid-row`).
    fn grid_row(mut self, lines: crate::grid_tracks::GridLines) -> Self {
        grid_place(self.styles_mut()).row = Some(lines);
        self
    }
    /// A grid's number of equal columns (at least 1).
    fn grid_cols(mut self, value: u16) -> Self {
        self.styles_mut().layout_options_mut().columns = Some(value.max(1));
        self
    }
    /// A grid's number of equal rows (at least 1).
    fn grid_rows(mut self, value: u16) -> Self {
        self.styles_mut().layout_options_mut().rows = Some(value.max(1));
        self
    }
    /// The grid column this child starts at, from 1; negative counts from
    /// the end (CSS `grid-column-start`).
    fn col_start(mut self, value: i16) -> Self {
        self.styles_mut().layout_options_mut().column_start = Some(value);
        self
    }
    /// The grid row this child starts at, from 1; negative counts from the
    /// end (CSS `grid-row-start`).
    fn row_start(mut self, value: i16) -> Self {
        self.styles_mut().layout_options_mut().row_start = Some(value);
        self
    }
    /// How many grid columns this child spans (at least 1).
    fn col_span(mut self, value: u16) -> Self {
        self.styles_mut().layout_options_mut().column_span = Some(value.max(1));
        self
    }
    /// How many grid rows this child spans (at least 1).
    fn row_span(mut self, value: u16) -> Self {
        self.styles_mut().layout_options_mut().row_span = Some(value.max(1));
        self
    }
    /// Span every grid column (CSS `grid-column: 1 / -1`).
    fn col_span_full(mut self) -> Self {
        self.styles_mut().layout_options_mut().column_full = Some(true);
        self
    }
    /// Span every grid row (CSS `grid-row: 1 / -1`).
    fn row_span_full(mut self) -> Self {
        self.styles_mut().layout_options_mut().row_full = Some(true);
        self
    }
    /// The smallest width, as a percentage of the parent's content width.
    fn min_w_percent(mut self, percent: f32) -> Self {
        self.styles_mut().layout_options_mut().min_width =
            Some(crate::layout::Length::Percent(percent / 100.));
        self
    }
    /// The largest width, as a percentage of the parent's content width.
    fn max_w_percent(mut self, percent: f32) -> Self {
        self.styles_mut().layout_options_mut().max_width =
            Some(crate::layout::Length::Percent(percent / 100.));
        self
    }
    /// The smallest height, as a percentage of the parent's content height.
    fn min_h_percent(mut self, percent: f32) -> Self {
        self.styles_mut().layout_options_mut().min_height =
            Some(crate::layout::Length::Percent(percent / 100.));
        self
    }
    /// The largest height, as a percentage of the parent's content height.
    fn max_h_percent(mut self, percent: f32) -> Self {
        self.styles_mut().layout_options_mut().max_height =
            Some(crate::layout::Length::Percent(percent / 100.));
        self
    }
    /// An `absolute` view's inset from its containing block's top edge.
    fn top(mut self, value: impl Into<crate::layout::Length>) -> Self {
        self.styles_mut().layout_options_mut().inset.top = Some(value.into());
        self
    }
    /// An `absolute` view's inset from its containing block's right edge.
    fn right(mut self, value: impl Into<crate::layout::Length>) -> Self {
        self.styles_mut().layout_options_mut().inset.right = Some(value.into());
        self
    }
    /// An `absolute` view's inset from its containing block's bottom edge.
    fn bottom(mut self, value: impl Into<crate::layout::Length>) -> Self {
        self.styles_mut().layout_options_mut().inset.bottom = Some(value.into());
        self
    }
    /// An `absolute` view's inset from its containing block's left edge.
    fn left(mut self, value: impl Into<crate::layout::Length>) -> Self {
        self.styles_mut().layout_options_mut().inset.left = Some(value.into());
        self
    }
    /// Padding on every side, as a percentage of the parent's content width
    /// (as CSS resolves percentage padding).
    fn p_percent(mut self, percent: f32) -> Self {
        let options = self.styles_mut().layout_options_mut();
        options.padding.top = Some(crate::layout::Length::Percent(percent / 100.));
        options.padding.right = Some(crate::layout::Length::Percent(percent / 100.));
        options.padding.bottom = Some(crate::layout::Length::Percent(percent / 100.));
        options.padding.left = Some(crate::layout::Length::Percent(percent / 100.));
        self
    }
    /// Left and right padding, as a percentage of the parent's content width.
    fn px_percent(mut self, percent: f32) -> Self {
        let options = self.styles_mut().layout_options_mut();
        options.padding.right = Some(crate::layout::Length::Percent(percent / 100.));
        options.padding.left = Some(crate::layout::Length::Percent(percent / 100.));
        self
    }
    /// Top and bottom padding, as a percentage of the parent's content width.
    fn py_percent(mut self, percent: f32) -> Self {
        let options = self.styles_mut().layout_options_mut();
        options.padding.top = Some(crate::layout::Length::Percent(percent / 100.));
        options.padding.bottom = Some(crate::layout::Length::Percent(percent / 100.));
        self
    }
    /// Top padding, as a percentage of the parent's content width.
    fn pt_percent(mut self, percent: f32) -> Self {
        let options = self.styles_mut().layout_options_mut();
        options.padding.top = Some(crate::layout::Length::Percent(percent / 100.));
        self
    }
    /// Right padding, as a percentage of the parent's content width.
    fn pr_percent(mut self, percent: f32) -> Self {
        let options = self.styles_mut().layout_options_mut();
        options.padding.right = Some(crate::layout::Length::Percent(percent / 100.));
        self
    }
    /// Bottom padding, as a percentage of the parent's content width.
    fn pb_percent(mut self, percent: f32) -> Self {
        let options = self.styles_mut().layout_options_mut();
        options.padding.bottom = Some(crate::layout::Length::Percent(percent / 100.));
        self
    }
    /// Left padding, as a percentage of the parent's content width.
    fn pl_percent(mut self, percent: f32) -> Self {
        let options = self.styles_mut().layout_options_mut();
        options.padding.left = Some(crate::layout::Length::Percent(percent / 100.));
        self
    }
    /// A margin on every side, as a percentage of the parent's content width.
    fn m_percent(mut self, percent: f32) -> Self {
        let options = self.styles_mut().layout_options_mut();
        options.margin.top = Some(crate::layout::Length::Percent(percent / 100.));
        options.margin.right = Some(crate::layout::Length::Percent(percent / 100.));
        options.margin.bottom = Some(crate::layout::Length::Percent(percent / 100.));
        options.margin.left = Some(crate::layout::Length::Percent(percent / 100.));
        self
    }
    /// Automatic margins on every side: the view takes its parent's free
    /// space around it, centring it (CSS `margin: auto`).
    fn m_auto(mut self) -> Self {
        let options = self.styles_mut().layout_options_mut();
        options.margin.top = Some(crate::layout::Length::Auto);
        options.margin.right = Some(crate::layout::Length::Auto);
        options.margin.bottom = Some(crate::layout::Length::Auto);
        options.margin.left = Some(crate::layout::Length::Auto);
        self
    }
    /// Left and right margins, as a percentage of the parent's content width.
    fn mx_percent(mut self, percent: f32) -> Self {
        let options = self.styles_mut().layout_options_mut();
        options.margin.right = Some(crate::layout::Length::Percent(percent / 100.));
        options.margin.left = Some(crate::layout::Length::Percent(percent / 100.));
        self
    }
    /// Automatic left and right margins: the view is centred across.
    fn mx_auto(mut self) -> Self {
        let options = self.styles_mut().layout_options_mut();
        options.margin.right = Some(crate::layout::Length::Auto);
        options.margin.left = Some(crate::layout::Length::Auto);
        self
    }
    /// Top and bottom margins, as a percentage of the parent's content width.
    fn my_percent(mut self, percent: f32) -> Self {
        let options = self.styles_mut().layout_options_mut();
        options.margin.top = Some(crate::layout::Length::Percent(percent / 100.));
        options.margin.bottom = Some(crate::layout::Length::Percent(percent / 100.));
        self
    }
    /// Automatic top and bottom margins: centred down.
    fn my_auto(mut self) -> Self {
        let options = self.styles_mut().layout_options_mut();
        options.margin.top = Some(crate::layout::Length::Auto);
        options.margin.bottom = Some(crate::layout::Length::Auto);
        self
    }
    /// A top margin, as a percentage of the parent's content width.
    fn mt_percent(mut self, percent: f32) -> Self {
        let options = self.styles_mut().layout_options_mut();
        options.margin.top = Some(crate::layout::Length::Percent(percent / 100.));
        self
    }
    /// An automatic top margin: the free space goes above the view, pushing
    /// it to the bottom.
    fn mt_auto(mut self) -> Self {
        let options = self.styles_mut().layout_options_mut();
        options.margin.top = Some(crate::layout::Length::Auto);
        self
    }
    /// A right margin, as a percentage of the parent's content width.
    fn mr_percent(mut self, percent: f32) -> Self {
        let options = self.styles_mut().layout_options_mut();
        options.margin.right = Some(crate::layout::Length::Percent(percent / 100.));
        self
    }
    /// An automatic right margin: the free space goes after the view,
    /// pushing what follows it to the end.
    fn mr_auto(mut self) -> Self {
        let options = self.styles_mut().layout_options_mut();
        options.margin.right = Some(crate::layout::Length::Auto);
        self
    }
    /// A bottom margin, as a percentage of the parent's content width.
    fn mb_percent(mut self, percent: f32) -> Self {
        let options = self.styles_mut().layout_options_mut();
        options.margin.bottom = Some(crate::layout::Length::Percent(percent / 100.));
        self
    }
    /// An automatic bottom margin: the free space goes below the view.
    fn mb_auto(mut self) -> Self {
        let options = self.styles_mut().layout_options_mut();
        options.margin.bottom = Some(crate::layout::Length::Auto);
        self
    }
    /// A left margin, as a percentage of the parent's content width.
    fn ml_percent(mut self, percent: f32) -> Self {
        let options = self.styles_mut().layout_options_mut();
        options.margin.left = Some(crate::layout::Length::Percent(percent / 100.));
        self
    }
    /// An automatic left margin: the free space goes before the view,
    /// pushing it to the end.
    fn ml_auto(mut self) -> Self {
        let options = self.styles_mut().layout_options_mut();
        options.margin.left = Some(crate::layout::Length::Auto);
        self
    }
    /// Take the view out of normal flow and position it against its
    /// containing block: the nearest ancestor that is `relative`, found
    /// without passing an `absolute` one; with none, its parent. `left`,
    /// `top`, `right` and `bottom` count from that block's padding box
    /// (inside its border), percentages of its size; without insets it
    /// sits at its parent's content origin plus margins. Absolute views do
    /// not contribute to their parent's intrinsic size or flex space, and
    /// still paint in their parent's place, under its clip.
    fn absolute(mut self) -> Self {
        self.styles_mut().fields_mut(bit::ABSOLUTE).absolute = Some(true);
        self
    }
    /// Make the view the containing block of its `absolute` descendants
    /// (CSS `position: relative`), and keep it in normal flow (undoing
    /// `absolute`). It changes nothing about its own layout.
    fn relative(mut self) -> Self {
        self.styles_mut().fields_mut(bit::ABSOLUTE).absolute = Some(false);
        self.styles_mut().rare_mut().relative = Some(true);
        self
    }
    /// Apply a reusable transformation while building a view or style patch.
    fn apply(self, transform: impl FnOnce(Self) -> Self) -> Self {
        transform(self)
    }
    /// Apply a transformation only when `condition` is true.
    ///
    /// This decision is made immediately, not reactively. Use a view's
    /// `reactive_style` method for styles that follow signal changes.
    fn when(self, condition: bool, transform: impl FnOnce(Self) -> Self) -> Self {
        if condition { transform(self) } else { self }
    }
    /// Apply a transformation with the contained value, if present.
    ///
    /// Like `when`, this runs while building the description, without creating
    /// a subscription. The closure is not called for `None`.
    fn when_some<T>(self, value: Option<T>, transform: impl FnOnce(Self, T) -> Self) -> Self {
        match value {
            Some(value) => transform(self, value),
            None => self,
        }
    }
    /// Merge `style`'s set properties over these.
    fn style(mut self, style: Styles) -> Self {
        self.styles_mut().merge(&style);
        self
    }
    /// Where a picture lies in its box where it doesn't fill it, as CSS
    /// `object-position`: fractions of the room to spare, `(0.5, 0.5)` centred
    /// (the default), `(0.5, 0.)` keeping its top when `cover` crops a portrait.
    fn object_position(mut self, x: f32, y: f32) -> Self {
        self.styles_mut()
            .rare_mut()
            .cold
            .get_or_insert_default()
            .object_position = Some((x, y));
        self
    }
    /// A shadow under a text's glyphs, as CSS `text-shadow`: its glyphs
    /// again, moved by `x`, `y` points and painted in `color`. `blur` is the
    /// Gaussian's sigma in points (0: sharp), each glyph's coverage blurred
    /// once and kept. It paints outside the text's box, as CSS's does.
    /// Inherited, as CSS's is: on a container it shadows the text inside.
    fn text_shadow(self, x: f32, y: f32, blur: f32, color: Color) -> Self {
        self.text_shadows([crate::scene::TextShadow {
            offset: crate::scene::Transform { x, y },
            blur,
            color,
        }])
    }
    /// Several shadows under a text's glyphs, the first on top (CSS's list);
    /// none: no shadow.
    fn text_shadows(
        mut self,
        shadows: impl Into<std::sync::Arc<[crate::scene::TextShadow]>>,
    ) -> Self {
        self.styles_mut()
            .rare_mut()
            .cold
            .get_or_insert_default()
            .text_shadows = Some(shadows.into());
        self
    }
    /// Set centered image fitting within its padded content box. Not inherited.
    fn object_fit(mut self, fit: ObjectFit) -> Self {
        self.styles_mut()
            .fields_mut(bit::OBJECT_FIT)
            .overflow_mut()
            .object_fit = Some(fit);
        self
    }
    /// Fit the image inside its box, keeping its aspect ratio
    /// ([`ObjectFit::Contain`]).
    fn object_contain(self) -> Self {
        self.object_fit(ObjectFit::Contain)
    }
    /// Fill the box with the image, keeping its aspect ratio and cropping
    /// the rest ([`ObjectFit::Cover`]).
    fn object_cover(self) -> Self {
        self.object_fit(ObjectFit::Cover)
    }
    /// The width in points, or a theme token; replaces `w_percent`.
    fn w(mut self, value: impl Into<crate::theme::Themed<f32>>) -> Self {
        value_or_token!(self, value, crate::theme::Field::Width);
        self.styles_mut().fields_mut(bit::WIDTH).width = Some(value).into();
        self.styles_mut()
            .fields_mut(bit::WIDTH_PERCENT)
            .width_percent = Opt32::none();
        self
    }
    /// The height in points, or a theme token; replaces `h_percent`.
    fn h(mut self, value: impl Into<crate::theme::Themed<f32>>) -> Self {
        value_or_token!(self, value, crate::theme::Field::Height);
        self.styles_mut().fields_mut(bit::HEIGHT).height = Some(value).into();
        self.styles_mut()
            .fields_mut(bit::HEIGHT_PERCENT)
            .height_percent = Opt32::none();
        self
    }
    /// Width as a percentage of the parent's definite content width; 100 fills it.
    /// Indefinite parent widths use intrinsic sizing. Flex allocation and min/max
    /// constraints still apply. Negative and non-finite values normalize to zero.
    fn w_percent(mut self, percent: f32) -> Self {
        self.styles_mut().fields_mut(bit::WIDTH).width = Opt32::none();
        self.styles_mut()
            .fields_mut(bit::WIDTH_PERCENT)
            .width_percent = Some(if percent.is_finite() {
            percent.max(0.) / 100.
        } else {
            0.
        })
        .into();
        self
    }
    /// Height as a percentage of the parent's definite content height.
    /// Uses the same normalization and intrinsic fallback as `w_percent`.
    fn h_percent(mut self, percent: f32) -> Self {
        self.styles_mut().fields_mut(bit::HEIGHT).height = Opt32::none();
        self.styles_mut()
            .fields_mut(bit::HEIGHT_PERCENT)
            .height_percent = Some(if percent.is_finite() {
            percent.max(0.) / 100.
        } else {
            0.
        })
        .into();
        self
    }
    /// Fill the parent's definite content width, subject to flex and min/max rules.
    fn w_full(self) -> Self {
        self.w_percent(100.)
    }
    /// Fill the parent's definite content height, subject to flex and min/max rules.
    fn h_full(self) -> Self {
        self.h_percent(100.)
    }
    /// The smallest width in points (CSS `min-width`); replaces
    /// `min_w_percent`. `min_w(0.)` lets a flex child shrink below its
    /// content.
    fn min_w(mut self, value: f32) -> Self {
        if let Some(options) = &mut self
            .styles_mut()
            .fields_mut(bit::LAYOUT_OPTIONS)
            .layout_options
        {
            std::sync::Arc::make_mut(options).min_width = None;
        }
        self.styles_mut().rare_mut().min_width = Some(value);
        self
    }
    /// The largest width in points (CSS `max-width`); replaces
    /// `max_w_percent`.
    fn max_w(mut self, value: f32) -> Self {
        if let Some(options) = &mut self
            .styles_mut()
            .fields_mut(bit::LAYOUT_OPTIONS)
            .layout_options
        {
            std::sync::Arc::make_mut(options).max_width = None;
        }
        self.styles_mut().rare_mut().max_width = Some(value);
        self
    }
    /// The smallest height in points (CSS `min-height`); replaces
    /// `min_h_percent`.
    fn min_h(mut self, value: f32) -> Self {
        if let Some(options) = &mut self
            .styles_mut()
            .fields_mut(bit::LAYOUT_OPTIONS)
            .layout_options
        {
            std::sync::Arc::make_mut(options).min_height = None;
        }
        self.styles_mut().rare_mut().min_height = Some(value);
        self
    }
    /// The largest height in points (CSS `max-height`); replaces
    /// `max_h_percent`.
    fn max_h(mut self, value: f32) -> Self {
        if let Some(options) = &mut self
            .styles_mut()
            .fields_mut(bit::LAYOUT_OPTIONS)
            .layout_options
        {
            std::sync::Arc::make_mut(options).max_height = None;
        }
        self.styles_mut().rare_mut().max_height = Some(value);
        self
    }
    /// The room between children, both ways, in points or a theme token;
    /// replaces `gap_x` and `gap_y`.
    fn gap(mut self, value: impl Into<crate::theme::Themed<f32>>) -> Self {
        value_or_token!(self, value, crate::theme::Field::Gap);
        if let Some(options) = &mut self
            .styles_mut()
            .fields_mut(bit::LAYOUT_OPTIONS)
            .layout_options
        {
            let options = std::sync::Arc::make_mut(options);
            options.gap_x = None;
            options.gap_y = None;
        }
        self.styles_mut().fields_mut(bit::GAP).gap = Some(value).into();
        self
    }
    /// How much of its parent's free space this child takes, in proportion
    /// to its siblings' (CSS `flex-grow`).
    fn flex_grow(mut self, value: f32) -> Self {
        self.styles_mut().fields_mut(bit::FLEX_GROW).flex_grow = Some(value).into();
        self
    }
    /// How much this child gives up when its parent is short of space, in
    /// proportion to its siblings' (CSS `flex-shrink`; 1 unless set). It
    /// shrinks no smaller than its content unless `min_w`/`min_h` allow.
    fn flex_shrink(mut self, value: f32) -> Self {
        self.styles_mut().fields_mut(bit::FLEX_SHRINK).flex_shrink = Some(value).into();
        self
    }
    /// Fill the background with `brush` (a colour, or a gradient between two
    /// points) instead of the `bg` colour.
    fn bg_fill(mut self, brush: crate::canvas::Brush) -> Self {
        self.styles_mut()
            .fields_mut(bit::PAINT_BACKGROUND)
            .paint_background = Some(Some(crate::decoration::Background::Brush(brush)));
        self
    }
    /// Fill the background with a linear gradient through `stops`, at `angle`
    /// clockwise radians, its ends spanning the box (CSS `linear-gradient`).
    fn bg_gradient(
        mut self,
        angle: f32,
        stops: impl Into<std::sync::Arc<[crate::canvas::GradientStop]>>,
    ) -> Self {
        self.styles_mut()
            .fields_mut(bit::PAINT_BACKGROUND)
            .paint_background = Some(Some(crate::decoration::Background::Linear {
            angle,
            stops: stops.into(),
        }));
        self
    }
    /// A background image filling the view as `fit` places it (centred),
    /// over its `bg` colour, cut to its rounded corners, under its border
    /// and children. Set after `bg`, which clears it.
    fn bg_image(
        mut self,
        image: std::sync::Arc<crate::image::ImageData>,
        fit: crate::style::ObjectFit,
    ) -> Self {
        self.styles_mut()
            .fields_mut(bit::PAINT_BACKGROUND)
            .paint_background = Some(Some(crate::decoration::Background::Image { image, fit }));
        self
    }
    /// A radial gradient (CSS `radial-gradient`): an ellipse the box's shape
    /// about `center` (fractions of the box, `(0.5, 0.5)` its middle), `radius`
    /// 1 reaching its farthest corner, in two or three `stops` from the centre
    /// to the edge. Drawn by a shader for each pixel: no raster, and nothing
    /// for other boxes; boxes only, not a canvas's `Brush`.
    fn bg_radial(
        self,
        center: (f32, f32),
        radius: f32,
        stops: impl Into<crate::decoration::RadialStops>,
    ) -> Self {
        self.bg_shader(crate::decoration::radial_shader())
            .shader_uniforms(crate::decoration::radial_uniforms(
                center,
                radius,
                stops.into(),
            ))
    }
    /// Fill the view with a shader (see `crate::decoration::FillShader`):
    /// a WGSL `fill` function run per pixel on the GPU, over its `bg`
    /// colour, cut to its rounded corners, under its border and children.
    /// Set after `bg`, which clears it.
    fn bg_shader(mut self, shader: std::sync::Arc<crate::decoration::FillShader>) -> Self {
        self.styles_mut()
            .fields_mut(bit::PAINT_BACKGROUND)
            .paint_background = Some(Some(crate::decoration::Background::Shader(shader)));
        self
    }
    /// The uniforms its `bg_shader` draws with: time and user parameters.
    /// Animating them per frame repaints the view alone (or set them with
    /// `Scene::set_shader_uniforms` directly, skipping style resolution).
    fn shader_uniforms(mut self, uniforms: crate::decoration::ShaderUniforms) -> Self {
        self.styles_mut()
            .rare_mut()
            .cold
            .get_or_insert_default()
            .shader_uniforms = Some(uniforms);
        self
    }
    /// Round each corner by its own radius; replaces `rounded`.
    fn rounded_corners(mut self, corners: crate::decoration::Corners) -> Self {
        self.styles_mut().fields_mut(bit::CORNERS).corners = Some(Some(corners));
        self
    }
    /// A border with its own width on each side; replaces `border`.
    fn border_edges(mut self, widths: Insets) -> Self {
        self.styles_mut().fields_mut(bit::BORDER_EDGES).border_edges = Some(Some(widths));
        self
    }
    /// Paint the border over the view's content, as an inset stroke (a CSS
    /// outline drawn inside): a photo filling a framed thumbnail stays
    /// under its frame. Solid borders only; a dashed one paints with the fill.
    fn border_over(mut self, value: bool) -> Self {
        self.styles_mut().rare_mut().border_over = Some(value);
        self
    }
    /// An outline (CSS `outline`): a ring `width` wide outside the box at
    /// `outline_offset` (0 by default), its corners the box's radii grown
    /// alike, over the view's content, taking no room in layout and not cut
    /// by its own `overflow_hidden`.
    fn outline(mut self, width: f32, color: Color) -> Self {
        self.styles_mut().rare_mut().outline = Some(Some((width, color)));
        self
    }
    /// The outline's colour, a value or a theme token, keeping its width
    /// (1 point if none was set).
    fn outline_color(mut self, value: impl Into<crate::theme::Themed<Color>>) -> Self {
        value_or_token!(self, value, crate::theme::Field::OutlineColor);
        let width = match self.styles_mut().rare_mut().outline {
            Some(Some((width, _))) => width,
            _ => 1.,
        };
        self.styles_mut().rare_mut().outline = Some(Some((width, value)));
        self
    }
    /// The outline's width, keeping its colour (the text colour's default
    /// black if none was set).
    fn outline_width(mut self, width: f32) -> Self {
        let color = match self.styles_mut().rare_mut().outline {
            Some(Some((_, color))) => color,
            _ => Color(0, 0, 0, 255),
        };
        self.styles_mut().rare_mut().outline = Some(Some((width.max(0.), color)));
        self
    }
    /// How far outside the box the outline starts (CSS `outline-offset`);
    /// negative draws it over the box's edge.
    fn outline_offset(mut self, offset: f32) -> Self {
        self.styles_mut().rare_mut().outline_offset = Some(offset);
        self
    }
    /// No outline.
    fn outline_none(mut self) -> Self {
        self.styles_mut().rare_mut().outline = Some(None);
        self
    }
    /// A design tool's stroke: `width` of `color` aligned inside the box (a
    /// border over its content), centred on its edge or outside it (an
    /// outline at offset `-width / 2` or 0).
    fn stroke(self, width: f32, color: Color, align: StrokeAlign) -> Self {
        match align {
            StrokeAlign::Inside => self.border(width).border_color(color).border_over(true),
            StrokeAlign::Center => self.outline(width, color).outline_offset(-width / 2.),
            StrokeAlign::Outside => self.outline(width, color).outline_offset(0.),
        }
    }
    /// Draw the border solid or dashed.
    fn border_style(mut self, style: crate::decoration::BorderStyle) -> Self {
        self.styles_mut().rare_mut().border_style = Some(style);
        self
    }
    /// Shadows under the box, in order (CSS `box-shadow`); replaces `shadow`.
    fn shadows(mut self, shadows: impl Into<std::sync::Arc<[BoxShadow]>>) -> Self {
        self.styles_mut().rare_mut().shadows = Some(Some(shadows.into()));
        self
    }
    /// Where it clips, fade its descendants out over `px` points inside its
    /// rounded edge instead of cutting them there (see
    /// `Decoration::clip_feather`): with a large radius and an animated
    /// clip inset, a soft oval that opens.
    fn clip_feather(mut self, px: f32) -> Self {
        self.styles_mut().rare_mut().clip_feather =
            Some(if px.is_finite() { px.max(0.) } else { 0. });
        self
    }
    /// Shadows inside the box (CSS's `inset`): see `Decoration::inset_shadows`.
    fn inset_shadows(mut self, shadows: impl Into<std::sync::Arc<[BoxShadow]>>) -> Self {
        self.styles_mut().rare_mut().inset_shadows = Some(Some(shadows.into()));
        self
    }
    /// The pointer's shape over the view.
    fn cursor(mut self, cursor: crate::cursor::Cursor) -> Self {
        self.styles_mut().fields_mut(bit::CURSOR).cursor = Some(cursor);
        self
    }
    /// The background colour, or a theme token; clears `bg_fill`,
    /// `bg_gradient`, `bg_image` and `bg_shader`.
    fn bg(mut self, value: impl Into<crate::theme::Themed<Color>>) -> Self {
        value_or_token!(self, value, crate::theme::Field::Background);
        self.styles_mut().fields_mut(bit::BACKGROUND).background = Some(value);
        self.styles_mut()
            .fields_mut(bit::PAINT_BACKGROUND)
            .paint_background = Some(None);
        self
    }
    /// The text colour, or a theme token. Inherited.
    fn text_color(mut self, value: impl Into<crate::theme::Themed<Color>>) -> Self {
        value_or_token!(self, value, crate::theme::Field::TextColor);
        self.styles_mut().fields_mut(bit::TEXT_COLOR).text_color = Some(value);
        self
    }
    /// Ordered explicit font families tried before platform fallback.
    fn font_fallbacks(
        mut self,
        families: impl Into<std::sync::Arc<[crate::text_layout::FontFamily]>>,
    ) -> Self {
        self.styles_mut().rare_mut().font_fallbacks = Some(families.into());
        self
    }
    /// How text past its last line or its width ends: cut, or with an
    /// ellipsis.
    fn text_overflow(mut self, overflow: crate::text_layout::TextOverflow) -> Self {
        self.styles_mut().rare_mut().text_overflow = Some(overflow);
        self
    }
    /// Clamp displayed visual lines; zero removes an inherited clamp.
    fn line_clamp(mut self, lines: u32) -> Self {
        self.styles_mut().rare_mut().line_clamp = Some(std::num::NonZeroU32::new(lines));
        self
    }
    /// One line, ending with an ellipsis if it doesn't fit.
    fn truncate(self) -> Self {
        self.line_clamp(1)
            .text_overflow(crate::text_layout::TextOverflow::Ellipsis)
    }
    /// How lines of text align in their box: at the line's start, left,
    /// centred or right. Inherited.
    fn text_align(mut self, alignment: crate::text_layout::TextAlign) -> Self {
        self.styles_mut().fields_mut(bit::TEXT_ALIGN).text_align = Some(alignment);
        self
    }
    /// Override inherited OpenType features; an empty set restores font defaults.
    fn font_features(mut self, features: crate::text_layout::FontFeatures) -> Self {
        self.styles_mut().rare_mut().font_features = Some(features);
        self
    }
    /// The font family, or a theme token. Inherited.
    fn font_family(
        mut self,
        family: impl Into<crate::theme::Themed<crate::text_layout::FontFamily>>,
    ) -> Self {
        value_or_token!(self, family, crate::theme::Field::FontFamily);
        self.styles_mut().fields_mut(bit::FONT_FAMILY).font_family = Some(family);
        self
    }
    /// The font weight, from 1 to 1000 (400 regular, 700 bold), or a theme
    /// token. Inherited.
    fn font_weight(mut self, weight: impl Into<crate::theme::Themed<u16>>) -> Self {
        value_or_token!(self, weight, crate::theme::Field::FontWeight);
        self.styles_mut().fields_mut(bit::FONT_WEIGHT).font_weight = Some(weight.clamp(1, 1000));
        self
    }
    /// Bold text (weight 700).
    fn font_bold(self) -> Self {
        self.font_weight(700)
    }
    /// Italic text, or upright. Inherited.
    fn italic(mut self, italic: bool) -> Self {
        self.styles_mut()
            .fields_mut(bit::ITALIC)
            .overflow_mut()
            .italic = Some(italic);
        self
    }
    /// The font size in points, or a theme token. Inherited.
    fn text_size(mut self, value: impl Into<crate::theme::Themed<f32>>) -> Self {
        value_or_token!(self, value, crate::theme::Field::TextSize);
        self.styles_mut().fields_mut(bit::TEXT_SIZE).text_size = Some(value).into();
        self
    }
    /// Set inherited line spacing in logical pixels. Invalid/non-positive values
    /// use normal spacing; positive values smaller than the font size are allowed.
    fn line_height(mut self, pixels: impl Into<crate::theme::Themed<f32>>) -> Self {
        value_or_token!(self, pixels, crate::theme::Field::LineHeight);
        self.styles_mut().fields_mut(bit::LINE_HEIGHT).line_height =
            Some(crate::text_layout::LineHeight::px(pixels));
        self
    }
    /// Reset inherited line spacing to the normal metric for each text size.
    fn line_height_normal(mut self) -> Self {
        self.styles_mut().fields_mut(bit::LINE_HEIGHT).line_height =
            Some(crate::text_layout::LineHeight::NORMAL);
        self
    }
    /// Inherited extra glyph advance in logical pixels. Zero resets inherited tracking.
    /// Non-finite values normalize to zero; finite values clamp to ±1,000,000 pixels.
    fn letter_spacing(mut self, pixels: impl Into<crate::theme::Themed<f32>>) -> Self {
        value_or_token!(self, pixels, crate::theme::Field::LetterSpacing);
        self.styles_mut().rare_mut().letter_spacing =
            Some(crate::text_layout::LetterSpacing::px(pixels));
        self
    }

    /// Underlined text, or not; inherited, like CSS's `text-decoration`
    /// (a hover style can underline a link's label).
    fn text_underline(mut self, on: bool) -> Self {
        self.styles_mut().rare_mut().underline = Some(on);
        self
    }
    /// How far below the baseline an underline sits (CSS's
    /// `text-underline-offset`; unset, a tenth of the font size). Inherited.
    fn underline_offset(mut self, px: f32) -> Self {
        self.styles_mut().rare_mut().underline_offset = px.is_finite().then_some(px);
        self
    }

    /// Round every corner by this radius in points, or a theme token;
    /// replaces `rounded_corners`.
    fn rounded(mut self, value: impl Into<crate::theme::Themed<f32>>) -> Self {
        value_or_token!(self, value, crate::theme::Field::Radius);
        self.styles_mut().fields_mut(bit::RADIUS).radius = Some(value).into();
        self.styles_mut().fields_mut(bit::CORNERS).corners = Some(None);
        self
    }
    /// A border this wide on every side, in points or a theme token;
    /// replaces `border_edges`.
    fn border(mut self, value: impl Into<crate::theme::Themed<f32>>) -> Self {
        value_or_token!(self, value, crate::theme::Field::BorderWidth);
        self.styles_mut().fields_mut(bit::BORDER_WIDTH).border_width = Some(value).into();
        self.styles_mut().fields_mut(bit::BORDER_EDGES).border_edges = Some(None);
        self
    }
    /// The border's colour, or a theme token.
    fn border_color(mut self, value: impl Into<crate::theme::Themed<Color>>) -> Self {
        value_or_token!(self, value, crate::theme::Field::BorderColor);
        self.styles_mut().fields_mut(bit::BORDER_COLOR).border_color = Some(value);
        self
    }
    /// The view's opacity, 0 to 1, or a theme token. The subtree composites
    /// item by item unless `isolated`.
    fn opacity(mut self, value: impl Into<crate::theme::Themed<f32>>) -> Self {
        value_or_token!(self, value, crate::theme::Field::Opacity);
        self.styles_mut().fields_mut(bit::OPACITY).opacity = Some(value).into();
        self
    }
    /// Backdrop blur (CSS `backdrop-filter: blur()`): what lies behind the
    /// node, within its bounds, blurs by this sigma in logical pixels, as
    /// frosted glass; the node's own content stays sharp. For blurring the
    /// node's content itself, see `filter_blur`. The clearer name is
    /// `backdrop_blur`: Luau's `blur` is the content blur, and `backdrop_blur`
    /// this one.
    fn blur(mut self, value: impl Into<crate::theme::Themed<f32>>) -> Self {
        value_or_token!(self, value, crate::theme::Field::Blur);
        self.styles_mut().fields_mut(bit::BLUR).overflow_mut().blur = Some(value).into();
        self
    }
    /// Backdrop blur (CSS `backdrop-filter: blur()`), by its clear name: what
    /// lies behind blurs, not the node (`filter_blur`). `blur` is this too.
    fn backdrop_blur(self, value: impl Into<crate::theme::Themed<f32>>) -> Self {
        self.blur(value)
    }
    /// Content blur (CSS `filter: blur()`): the node and its subtree blur by
    /// this sigma in logical pixels, spreading past its bounds, under its
    /// opacity. The subtree paints once into a cached layer; animating the
    /// sigma (a fade in from blurred text) only composites it again. Unlike
    /// `blur`, what lies behind is untouched.
    fn filter_blur(mut self, value: f32) -> Self {
        self.styles_mut()
            .fields_mut(bit::FILTER_BLUR)
            .overflow_mut()
            .filter_blur = Some(value).into();
        self
    }
    /// CSS `filter: brightness()`: the view and its subtree, as a group,
    /// with colours scaled (1 leaves them, 0 is black, past 1 brighter), as
    /// a button brightens on hover. Applied as the GPU writes each colour:
    /// no layer, and animating it repaints only the view's area.
    fn brightness(mut self, value: f32) -> Self {
        self.styles_mut()
            .fields_mut(bit::BRIGHTNESS)
            .overflow_mut()
            .brightness = Some(value).into();
        self
    }
    /// CSS `filter: contrast()`: colours pushed from (or, below 1, towards)
    /// mid grey.
    fn contrast(mut self, value: f32) -> Self {
        self.styles_mut()
            .fields_mut(bit::CONTRAST)
            .overflow_mut()
            .contrast = Some(value).into();
        self
    }
    /// CSS `filter: saturate()`: 0 is greyscale, past 1 more vivid.
    fn saturate(mut self, value: f32) -> Self {
        self.styles_mut()
            .fields_mut(bit::SATURATE)
            .overflow_mut()
            .saturate = Some(value).into();
        self
    }
    /// Show the node's content through `image`'s alpha (CSS `mask-image`,
    /// no repeat, nearest sampling), drawn `size` (logical pixels) large at
    /// `mask_offset` from the node's origin; outside the image nothing shows.
    /// The subtree paints once into a cached layer, so moving the mask (a
    /// shimmer, a progress fill) only composites it again.
    fn mask(mut self, image: std::sync::Arc<crate::image::ImageData>, size: (f32, f32)) -> Self {
        self.styles_mut().rare_mut().mask = Some(Some((image, size)));
        self
    }
    /// Mix the view with what lies beneath it by `mode`, as CSS
    /// `mix-blend-mode` (see `crate::scene::BlendMode`): its subtree paints
    /// once into a cached layer that blends with the backdrop as it
    /// composites.
    fn blend(mut self, mode: crate::scene::BlendMode) -> Self {
        self.styles_mut().rare_mut().blend = Some(mode);
        self
    }
    /// Composite the view's subtree through `shader`'s WGSL `layer`
    /// function (see `crate::scene::LayerShader`): painted once into a
    /// cached layer, each pixel of its box passes through the shader as it
    /// composites, so animating its uniforms costs no repaint.
    fn layer_shader(mut self, shader: std::sync::Arc<crate::decoration::FillShader>) -> Self {
        self.styles_mut().rare_mut().layer_shader = Some(Some(shader));
        self
    }
    /// The uniforms its `layer_shader` draws with: time and user parameters
    /// (or set them with `Scene::set_layer_uniforms` directly).
    fn layer_uniforms(mut self, uniforms: crate::decoration::ShaderUniforms) -> Self {
        self.styles_mut()
            .rare_mut()
            .cold
            .get_or_insert_default()
            .layer_uniforms = Some(uniforms);
        self
    }
    /// CSS `filter: hue-rotate()`: colours turned `degrees` round the hue
    /// circle. Unlike `brightness` and `saturate`, paints its subtree into a
    /// layer that composites through a shader, so it costs what a layer
    /// does, and only here. Several of these apply in the order given; it
    /// takes the place of a `layer_shader`.
    fn hue_rotate(self, degrees: f32) -> Self {
        self.color_filter(crate::scene::hue_rotate_matrix(degrees))
    }
    /// CSS `filter: sepia()`, `amount` from 0 to 1 (see `hue_rotate`).
    fn sepia(self, amount: f32) -> Self {
        self.color_filter(crate::scene::sepia_matrix(amount))
    }
    /// CSS `filter: invert()`, `amount` from 0 to 1 (see `hue_rotate`).
    fn invert(self, amount: f32) -> Self {
        self.color_filter(crate::scene::invert_matrix(amount))
    }
    #[doc(hidden)]
    fn color_filter(mut self, matrix: crate::scene::ColorMatrix) -> Self {
        let shader = crate::decoration::color_filter_shader();
        let rare = self.styles_mut().rare_mut();
        let cold = rare.cold.get_or_insert_default();
        // After one already set, this one applies to what it made.
        let before = (rare.layer_shader.as_ref())
            .and_then(|set| set.as_ref())
            .filter(|set| std::sync::Arc::ptr_eq(set, &shader))
            .and(cold.layer_uniforms)
            .map(|uniforms| [uniforms.params[0], uniforms.params[1], uniforms.params[2]]);
        let matrix = crate::scene::compose_color(Some(matrix), before).unwrap_or(matrix);
        cold.layer_uniforms = Some(crate::decoration::ShaderUniforms {
            time: 0.,
            params: [matrix[0], matrix[1], matrix[2], [0.; 4]],
        });
        rare.layer_shader = Some(Some(shader));
        self
    }
    /// Clip the view and its descendants to its box inset by these pixels
    /// (CSS `clip-path: inset(top right bottom left)`); negative ones reach
    /// past it, so shadows and rings aren't cut. Animating it composites
    /// again without layout. With `overflow_hidden` and rounded corners,
    /// the inset box is rounded alike.
    fn clip_inset(mut self, top: f32, right: f32, bottom: f32, left: f32) -> Self {
        self.styles_mut()
            .fields_mut(bit::CLIP_INSET)
            .overflow_mut()
            .clip_inset = Some(crate::scene::ClipInset::px(top, right, bottom, left));
        self
    }
    /// `clip_inset` in percent of the view's own size (top and bottom of its
    /// height, left and right of its width), following it as it resizes:
    /// `clip_inset_percent(0., 0., 100., 0.)` closes it to its top edge.
    fn clip_inset_percent(mut self, top: f32, right: f32, bottom: f32, left: f32) -> Self {
        use crate::layout::Length::Percent;
        self.styles_mut()
            .fields_mut(bit::CLIP_INSET)
            .overflow_mut()
            .clip_inset = Some(crate::scene::ClipInset {
            top: Percent(top / 100.),
            right: Percent(right / 100.),
            bottom: Percent(bottom / 100.),
            left: Percent(left / 100.),
        });
        self
    }
    /// `clip_inset` with each side's length (pixels or percent) as given.
    fn clip_inset_lengths(mut self, inset: crate::scene::ClipInset) -> Self {
        self.styles_mut()
            .fields_mut(bit::CLIP_INSET)
            .overflow_mut()
            .clip_inset = Some(inset);
        self
    }
    /// Where `mask`'s image lies, from the node's laid-out origin.
    fn mask_offset(mut self, x: f32, y: f32) -> Self {
        self.styles_mut()
            .fields_mut(bit::MASK_OFFSET)
            .overflow_mut()
            .mask_offset = Some((x, y));
        self
    }
    /// Fade the view's top and bottom edges out over `value` points.
    fn edge_fade(mut self, value: f32) -> Self {
        self.styles_mut()
            .fields_mut(bit::EDGE_FADE)
            .overflow_mut()
            .edge_fade = Some(value).into();
        self
    }
    /// Render the subtree as a group into a cached layer, then composite it
    /// once: `isolated(true).opacity(0.5)` fades overlapping children
    /// together.
    fn isolated(mut self, value: bool) -> Self {
        self.styles_mut()
            .fields_mut(bit::ISOLATED)
            .overflow_mut()
            .isolated = Some(value);
        self
    }
    /// Wrap text at the content width (true), or keep each line whole.
    /// Inherited.
    fn text_wrap(mut self, value: bool) -> Self {
        self.styles_mut().fields_mut(bit::TEXT_WRAP).text_wrap = Some(value);
        self
    }
    /// Wrap text with its lines balanced (CSS's `text-wrap: balance`): at
    /// the narrowest width that keeps as many lines, so a heading's last line
    /// isn't left short. For up to six lines; its box is as it was.
    fn text_balance(mut self, balance: bool) -> Self {
        if balance {
            self.styles_mut().fields_mut(bit::TEXT_WRAP).text_wrap = Some(true);
        }
        let cold = self.styles_mut().rare_mut().cold.get_or_insert_default();
        cold.text_balance = Some(balance);
        self
    }
    /// `w(width).h(height)`.
    fn size(self, width: f32, height: f32) -> Self {
        self.w(width).h(height)
    }
    /// Padding on every side, in points or a theme token.
    fn p(mut self, value: impl Into<crate::theme::Themed<f32>>) -> Self {
        value_or_token!(self, value, crate::theme::Field::Padding(15));
        let style = self.styles_mut();
        if style.padding.is_some() && style.padding_sides == 0 {
            style.fields_mut(bit::PADDING_SIDES).padding_sides = 15;
        }
        merge_insets(
            style
                .fields_mut(bit::PADDING)
                .padding
                .get_or_insert_with(Insets::default),
            Insets::all(value),
            15,
        );
        style.fields_mut(bit::PADDING_SIDES).padding_sides |= 15;
        style.clear_layout_spacing(false, 15);
        self
    }
    /// Left and right padding, in points or a theme token.
    fn px(mut self, value: impl Into<crate::theme::Themed<f32>>) -> Self {
        value_or_token!(self, value, crate::theme::Field::Padding(10));
        let style = self.styles_mut();
        if style.padding.is_some() && style.padding_sides == 0 {
            style.fields_mut(bit::PADDING_SIDES).padding_sides = 15;
        }
        merge_insets(
            style
                .fields_mut(bit::PADDING)
                .padding
                .get_or_insert_with(Insets::default),
            Insets::all(value),
            10,
        );
        style.fields_mut(bit::PADDING_SIDES).padding_sides |= 10;
        style.clear_layout_spacing(false, 10);
        self
    }
    /// Top and bottom padding, in points or a theme token.
    fn py(mut self, value: impl Into<crate::theme::Themed<f32>>) -> Self {
        value_or_token!(self, value, crate::theme::Field::Padding(5));
        let style = self.styles_mut();
        if style.padding.is_some() && style.padding_sides == 0 {
            style.fields_mut(bit::PADDING_SIDES).padding_sides = 15;
        }
        merge_insets(
            style
                .fields_mut(bit::PADDING)
                .padding
                .get_or_insert_with(Insets::default),
            Insets::all(value),
            5,
        );
        style.fields_mut(bit::PADDING_SIDES).padding_sides |= 5;
        style.clear_layout_spacing(false, 5);
        self
    }
    /// Top padding, in points or a theme token.
    fn pt(mut self, value: impl Into<crate::theme::Themed<f32>>) -> Self {
        value_or_token!(self, value, crate::theme::Field::Padding(1));
        let style = self.styles_mut();
        if style.padding.is_some() && style.padding_sides == 0 {
            style.fields_mut(bit::PADDING_SIDES).padding_sides = 15;
        }
        merge_insets(
            style
                .fields_mut(bit::PADDING)
                .padding
                .get_or_insert_with(Insets::default),
            Insets::all(value),
            1,
        );
        style.fields_mut(bit::PADDING_SIDES).padding_sides |= 1;
        style.clear_layout_spacing(false, 1);
        self
    }
    /// Right padding, in points or a theme token.
    fn pr(mut self, value: impl Into<crate::theme::Themed<f32>>) -> Self {
        value_or_token!(self, value, crate::theme::Field::Padding(2));
        let style = self.styles_mut();
        if style.padding.is_some() && style.padding_sides == 0 {
            style.fields_mut(bit::PADDING_SIDES).padding_sides = 15;
        }
        merge_insets(
            style
                .fields_mut(bit::PADDING)
                .padding
                .get_or_insert_with(Insets::default),
            Insets::all(value),
            2,
        );
        style.fields_mut(bit::PADDING_SIDES).padding_sides |= 2;
        style.clear_layout_spacing(false, 2);
        self
    }
    /// Bottom padding, in points or a theme token.
    fn pb(mut self, value: impl Into<crate::theme::Themed<f32>>) -> Self {
        value_or_token!(self, value, crate::theme::Field::Padding(4));
        let style = self.styles_mut();
        if style.padding.is_some() && style.padding_sides == 0 {
            style.fields_mut(bit::PADDING_SIDES).padding_sides = 15;
        }
        merge_insets(
            style
                .fields_mut(bit::PADDING)
                .padding
                .get_or_insert_with(Insets::default),
            Insets::all(value),
            4,
        );
        style.fields_mut(bit::PADDING_SIDES).padding_sides |= 4;
        style.clear_layout_spacing(false, 4);
        self
    }
    /// Left padding, in points or a theme token.
    fn pl(mut self, value: impl Into<crate::theme::Themed<f32>>) -> Self {
        value_or_token!(self, value, crate::theme::Field::Padding(8));
        let style = self.styles_mut();
        if style.padding.is_some() && style.padding_sides == 0 {
            style.fields_mut(bit::PADDING_SIDES).padding_sides = 15;
        }
        merge_insets(
            style
                .fields_mut(bit::PADDING)
                .padding
                .get_or_insert_with(Insets::default),
            Insets::all(value),
            8,
        );
        style.fields_mut(bit::PADDING_SIDES).padding_sides |= 8;
        style.clear_layout_spacing(false, 8);
        self
    }
    /// A margin on every side, in points or a theme token.
    fn m(mut self, value: impl Into<crate::theme::Themed<f32>>) -> Self {
        value_or_token!(self, value, crate::theme::Field::Margin(15));
        let style = self.styles_mut();
        if style.margin.is_some() && style.margin_sides == 0 {
            style.fields_mut(bit::MARGIN_SIDES).margin_sides = 15;
        }
        merge_insets(
            style
                .fields_mut(bit::MARGIN)
                .margin
                .get_or_insert_with(Insets::default),
            Insets::all(value),
            15,
        );
        style.fields_mut(bit::MARGIN_SIDES).margin_sides |= 15;
        style.clear_layout_spacing(true, 15);
        self
    }
    /// Left and right margins, in points or a theme token.
    fn mx(mut self, value: impl Into<crate::theme::Themed<f32>>) -> Self {
        value_or_token!(self, value, crate::theme::Field::Margin(10));
        let style = self.styles_mut();
        if style.margin.is_some() && style.margin_sides == 0 {
            style.fields_mut(bit::MARGIN_SIDES).margin_sides = 15;
        }
        merge_insets(
            style
                .fields_mut(bit::MARGIN)
                .margin
                .get_or_insert_with(Insets::default),
            Insets::all(value),
            10,
        );
        style.fields_mut(bit::MARGIN_SIDES).margin_sides |= 10;
        style.clear_layout_spacing(true, 10);
        self
    }
    /// Top and bottom margins, in points or a theme token.
    fn my(mut self, value: impl Into<crate::theme::Themed<f32>>) -> Self {
        value_or_token!(self, value, crate::theme::Field::Margin(5));
        let style = self.styles_mut();
        if style.margin.is_some() && style.margin_sides == 0 {
            style.fields_mut(bit::MARGIN_SIDES).margin_sides = 15;
        }
        merge_insets(
            style
                .fields_mut(bit::MARGIN)
                .margin
                .get_or_insert_with(Insets::default),
            Insets::all(value),
            5,
        );
        style.fields_mut(bit::MARGIN_SIDES).margin_sides |= 5;
        style.clear_layout_spacing(true, 5);
        self
    }
    /// A top margin, in points or a theme token.
    fn mt(mut self, value: impl Into<crate::theme::Themed<f32>>) -> Self {
        value_or_token!(self, value, crate::theme::Field::Margin(1));
        let style = self.styles_mut();
        if style.margin.is_some() && style.margin_sides == 0 {
            style.fields_mut(bit::MARGIN_SIDES).margin_sides = 15;
        }
        merge_insets(
            style
                .fields_mut(bit::MARGIN)
                .margin
                .get_or_insert_with(Insets::default),
            Insets::all(value),
            1,
        );
        style.fields_mut(bit::MARGIN_SIDES).margin_sides |= 1;
        style.clear_layout_spacing(true, 1);
        self
    }
    /// A right margin, in points or a theme token.
    fn mr(mut self, value: impl Into<crate::theme::Themed<f32>>) -> Self {
        value_or_token!(self, value, crate::theme::Field::Margin(2));
        let style = self.styles_mut();
        if style.margin.is_some() && style.margin_sides == 0 {
            style.fields_mut(bit::MARGIN_SIDES).margin_sides = 15;
        }
        merge_insets(
            style
                .fields_mut(bit::MARGIN)
                .margin
                .get_or_insert_with(Insets::default),
            Insets::all(value),
            2,
        );
        style.fields_mut(bit::MARGIN_SIDES).margin_sides |= 2;
        style.clear_layout_spacing(true, 2);
        self
    }
    /// A bottom margin, in points or a theme token.
    fn mb(mut self, value: impl Into<crate::theme::Themed<f32>>) -> Self {
        value_or_token!(self, value, crate::theme::Field::Margin(4));
        let style = self.styles_mut();
        if style.margin.is_some() && style.margin_sides == 0 {
            style.fields_mut(bit::MARGIN_SIDES).margin_sides = 15;
        }
        merge_insets(
            style
                .fields_mut(bit::MARGIN)
                .margin
                .get_or_insert_with(Insets::default),
            Insets::all(value),
            4,
        );
        style.fields_mut(bit::MARGIN_SIDES).margin_sides |= 4;
        style.clear_layout_spacing(true, 4);
        self
    }
    /// A left margin, in points or a theme token.
    fn ml(mut self, value: impl Into<crate::theme::Themed<f32>>) -> Self {
        value_or_token!(self, value, crate::theme::Field::Margin(8));
        let style = self.styles_mut();
        if style.margin.is_some() && style.margin_sides == 0 {
            style.fields_mut(bit::MARGIN_SIDES).margin_sides = 15;
        }
        merge_insets(
            style
                .fields_mut(bit::MARGIN)
                .margin
                .get_or_insert_with(Insets::default),
            Insets::all(value),
            8,
        );
        style.fields_mut(bit::MARGIN_SIDES).margin_sides |= 8;
        style.clear_layout_spacing(true, 8);
        self
    }
    /// Lay the children out in a row, first first.
    fn flex_row(mut self) -> Self {
        if let Some(options) = &mut self
            .styles_mut()
            .fields_mut(bit::LAYOUT_OPTIONS)
            .layout_options
        {
            std::sync::Arc::make_mut(options).reverse = None;
        }
        self.styles_mut()
            .fields_mut(bit::LAYOUT)
            .overflow_mut()
            .layout = Some(Layout::Row);
        self
    }
    /// Lay the children out in a column, first first.
    fn flex_col(mut self) -> Self {
        if let Some(options) = &mut self
            .styles_mut()
            .fields_mut(bit::LAYOUT_OPTIONS)
            .layout_options
        {
            std::sync::Arc::make_mut(options).reverse = None;
        }
        self.styles_mut()
            .fields_mut(bit::LAYOUT)
            .overflow_mut()
            .layout = Some(Layout::Column);
        self
    }
    /// Stack the children in one area (see [`crate::compose::overlay`]).
    fn overlay(mut self) -> Self {
        self.styles_mut()
            .fields_mut(bit::LAYOUT)
            .overflow_mut()
            .layout = Some(Layout::Overlay);
        self
    }
    /// Align the children at the start across the axis (CSS `align-items:
    /// flex-start`).
    fn items_start(mut self) -> Self {
        self.styles_mut().fields_mut(bit::ALIGN).align = Some(Align::Start);
        self
    }
    /// Centre the children across the axis.
    fn items_center(mut self) -> Self {
        self.styles_mut().fields_mut(bit::ALIGN).align = Some(Align::Center);
        self
    }
    /// Align a row's children by their first lines' baselines.
    fn items_baseline(mut self) -> Self {
        self.styles_mut().fields_mut(bit::ALIGN).align = Some(Align::Baseline);
        self
    }
    /// Align the children at the end across the axis.
    fn items_end(mut self) -> Self {
        self.styles_mut().fields_mut(bit::ALIGN).align = Some(Align::End);
        self
    }
    /// Stretch the children across the axis, unless they size themselves on
    /// it (a column's default).
    fn items_stretch(mut self) -> Self {
        self.styles_mut().fields_mut(bit::ALIGN).align = Some(Align::Stretch);
        self
    }
    /// Pack the children at the start of the axis.
    fn justify_start(mut self) -> Self {
        self.styles_mut().fields_mut(bit::JUSTIFY).justify = Some(Justify::Start);
        self
    }
    /// Centre the children along the axis.
    fn justify_center(mut self) -> Self {
        self.styles_mut().fields_mut(bit::JUSTIFY).justify = Some(Justify::Center);
        self
    }
    /// Pack the children at the end of the axis.
    fn justify_end(mut self) -> Self {
        self.styles_mut().fields_mut(bit::JUSTIFY).justify = Some(Justify::End);
        self
    }
    /// Spread the free space between the children, none at the ends.
    fn justify_between(mut self) -> Self {
        self.styles_mut().fields_mut(bit::JUSTIFY).justify = Some(Justify::SpaceBetween);
        self
    }
    /// Spread the free space evenly between the children and the ends.
    fn justify_evenly(mut self) -> Self {
        self.styles_mut().fields_mut(bit::JUSTIFY).justify = Some(Justify::SpaceEvenly);
        self
    }
    /// Spread the free space around each child: half as much at the ends as
    /// between.
    fn justify_around(mut self) -> Self {
        self.styles_mut().fields_mut(bit::JUSTIFY).justify = Some(Justify::SpaceAround);
        self
    }
    /// An overlay's children fill its height (its default, with
    /// `items_stretch` across). Along a row or column, the same as
    /// `justify_start`.
    fn justify_stretch(mut self) -> Self {
        self.styles_mut().fields_mut(bit::JUSTIFY).justify = Some(Justify::Stretch);
        self
    }
    /// Fade content out over `top` and `bottom` bands at the edges of this
    /// node's clip (with `overflow_hidden` or a scroll view), per pixel.
    fn fade_edges(mut self, top: f32, bottom: f32) -> Self {
        self.styles_mut().rare_mut().fade_edges = Some([top, bottom]);
        self
    }
    /// End `fade_edges`' bands this far in from the clip's top and bottom
    /// edges, hiding content past them: a scroll view running on under a bar
    /// fades out by the bar's edge.
    fn fade_insets(mut self, top: f32, bottom: f32) -> Self {
        self.styles_mut().rare_mut().fade_insets = Some([top, bottom]);
        self
    }
    /// `fade_edges` at the left and right edges: a horizontally scrolling
    /// row fading the overflow at its right edge.
    fn fade_edges_x(mut self, left: f32, right: f32) -> Self {
        self.styles_mut().rare_mut().fade_edges_x = Some([left, right]);
        self
    }
    /// `fade_insets` at the left and right edges.
    fn fade_insets_x(mut self, left: f32, right: f32) -> Self {
        self.styles_mut().rare_mut().fade_insets_x = Some([left, right]);
        self
    }
    /// Fade an edge only as far as content runs past it: a scroll view at
    /// its top shows no top band, which grows to its width as the content
    /// scrolls under that edge.
    fn fade_overflow(mut self, value: bool) -> Self {
        self.styles_mut().rare_mut().fade_overflow = Some(value);
        self
    }
    /// Lay this node's edges on whole points after layout (percentages and
    /// ratios leave fractions, which blur a 1-point border across pixels);
    /// its size follows its rounded edges.
    fn snap(mut self, value: bool) -> Self {
        self.styles_mut().rare_mut().snap = Some(value);
        self
    }
    /// Which way lines run in it and what it holds (CSS's `direction`):
    /// right to left, rows run from the right, a column's children start at
    /// its right, text starts at the right, and the `ps`/`pe`, `ms`/`me`,
    /// `start`/`end` edges follow. `pl`, `ml` and `left` stay on the left.
    fn direction(mut self, value: crate::scene::Direction) -> Self {
        self.styles_mut().rare_mut().direction = Some(value);
        self
    }
    /// Zoom what it holds, as CSS's `zoom`: its lengths (sizes, padding,
    /// text, borders, shadows) `zoom` times as large, laid out again at that
    /// size and drawn crisply at it, rather than scaled after layout as
    /// `scale` is. Zooms multiply with its ancestors' and the window's
    /// (`Ui::set_zoom`). 1 leaves it as it is.
    fn zoom(mut self, zoom: f32) -> Self {
        self.styles_mut().rare_mut().zoom = Some(ZoomStyle::By(zoom));
        self
    }
    /// Keep it at its own size whatever zooms its ancestors and the window
    /// have: a toolbar that stays put, a canvas that zooms itself.
    fn zoom_fixed(mut self) -> Self {
        self.styles_mut().rare_mut().zoom = Some(ZoomStyle::Fixed);
        self
    }
    /// `direction(Direction::Rtl)`: Arabic, Hebrew, Persian, Urdu.
    fn rtl(self) -> Self {
        self.direction(crate::scene::Direction::Rtl)
    }
    /// Padding at the start of a line (CSS's `padding-inline-start`): the
    /// left left to right, the right right to left.
    fn ps(mut self, value: impl Into<crate::layout::Length>) -> Self {
        self.styles_mut().layout_options_mut().padding_inline.start = Some(value.into());
        self
    }
    /// Padding at the end of a line (`padding-inline-end`).
    fn pe(mut self, value: impl Into<crate::layout::Length>) -> Self {
        self.styles_mut().layout_options_mut().padding_inline.end = Some(value.into());
        self
    }
    /// A margin at the start of a line (`margin-inline-start`).
    fn ms(mut self, value: impl Into<crate::layout::Length>) -> Self {
        self.styles_mut().layout_options_mut().margin_inline.start = Some(value.into());
        self
    }
    /// A margin at the end of a line (`margin-inline-end`).
    fn me(mut self, value: impl Into<crate::layout::Length>) -> Self {
        self.styles_mut().layout_options_mut().margin_inline.end = Some(value.into());
        self
    }
    /// An absolute node's inset from the start of a line
    /// (`inset-inline-start`): `left` left to right, `right` right to left.
    fn start(mut self, value: impl Into<crate::layout::Length>) -> Self {
        self.styles_mut().layout_options_mut().inset_inline.start = Some(value.into());
        self
    }
    /// Its inset from the end of a line (`inset-inline-end`).
    fn end(mut self, value: impl Into<crate::layout::Length>) -> Self {
        self.styles_mut().layout_options_mut().inset_inline.end = Some(value.into());
        self
    }
    /// Fade all four edges (CSS order: top, right, bottom, left), each band
    /// independent; 0 leaves an edge sharp.
    fn fade_sides(self, top: f32, right: f32, bottom: f32, left: f32) -> Self {
        self.fade_edges(top, bottom).fade_edges_x(left, right)
    }
    /// Clip the subtree to the view's box, and to its rounded corners (CSS
    /// `overflow: hidden`).
    fn overflow_hidden(mut self) -> Self {
        self.styles_mut().fields_mut(bit::CLIP).clip = Some(true);
        if let Some(options) = &mut self
            .styles_mut()
            .fields_mut(bit::LAYOUT_OPTIONS)
            .layout_options
        {
            let options = std::sync::Arc::make_mut(options);
            options.clip_x = None;
            options.clip_y = None;
        }
        self
    }
    /// Let the subtree paint past the view's box (the default).
    fn overflow_visible(mut self) -> Self {
        self.styles_mut().fields_mut(bit::CLIP).clip = Some(false);
        if let Some(options) = &mut self
            .styles_mut()
            .fields_mut(bit::LAYOUT_OPTIONS)
            .layout_options
        {
            let options = std::sync::Arc::make_mut(options);
            options.clip_x = None;
            options.clip_y = None;
        }
        self
    }
    /// Clip the subtree at the view's left and right edges only.
    fn overflow_x_hidden(mut self) -> Self {
        self.styles_mut().layout_options_mut().clip_x = Some(true);
        self
    }
    /// Clip the subtree at the view's top and bottom edges only.
    fn overflow_y_hidden(mut self) -> Self {
        self.styles_mut().layout_options_mut().clip_y = Some(true);
        self
    }
    /// Let the subtree paint past the view's left and right edges.
    fn overflow_x_visible(mut self) -> Self {
        self.styles_mut().layout_options_mut().clip_x = Some(false);
        self
    }
    /// Let the subtree paint past the view's top and bottom edges.
    fn overflow_y_visible(mut self) -> Self {
        self.styles_mut().layout_options_mut().clip_y = Some(false);
        self
    }
    /// A 1-point border on every side.
    fn border_1(self) -> Self {
        self.border(1.)
    }
    /// `flex_grow(1.)`: take the parent's free space.
    fn grow(self) -> Self {
        self.flex_grow(1.)
    }
    /// `flex_shrink(0.)`: keep the view's size when its parent is short of
    /// space.
    fn shrink_0(self) -> Self {
        self.flex_shrink(0.)
    }
    /// One shadow under the box, or a theme token; replaces `shadows`.
    fn shadow(mut self, value: impl Into<crate::theme::Themed<BoxShadow>>) -> Self {
        value_or_token!(self, value, crate::theme::Field::Shadow);
        self.styles_mut().rare_mut().shadow = Some(Some(value));
        self.styles_mut().rare_mut().shadows = Some(None);
        self
    }
    /// No shadow.
    fn shadow_none(mut self) -> Self {
        self.styles_mut().rare_mut().shadow = Some(None);
        self.styles_mut().rare_mut().shadows = Some(None);
        self
    }
    /// Move the view by `x`, `y` points after layout, without moving its
    /// siblings: a compositor-only change.
    fn translate(mut self, x: f32, y: f32) -> Self {
        self.styles_mut().fields_mut(bit::TRANSFORM).transform = Some(Transform { x, y });
        self
    }
    /// Scale uniformly about the transform origin, after layout: the
    /// subtree composites as a cached layer, so animating it repaints none
    /// of its content (see `Scene::set_local_transform`).
    fn scale(self, factor: f32) -> Self {
        self.scale_xy(factor, factor)
    }
    /// Scale by `x` across and `y` down about the transform origin, as
    /// `scale`.
    fn scale_xy(mut self, x: f32, y: f32) -> Self {
        let patch = &mut self
            .styles_mut()
            .fields_mut(bit::LOCAL_TRANSFORM)
            .local_transform;
        patch.values.scale = (x, y);
        patch.set |= TransformPatch::SCALE;
        self
    }
    /// Rotate clockwise by `radians` about the transform origin, as `scale`.
    fn rotate(mut self, radians: f32) -> Self {
        let patch = &mut self
            .styles_mut()
            .fields_mut(bit::LOCAL_TRANSFORM)
            .local_transform;
        patch.values.rotate = radians;
        patch.set |= TransformPatch::ROTATE;
        self
    }
    /// Skew by angles in radians, as CSS `skew(x, y)`.
    fn skew(mut self, x: f32, y: f32) -> Self {
        let patch = &mut self
            .styles_mut()
            .fields_mut(bit::LOCAL_TRANSFORM)
            .local_transform;
        patch.values.skew = (x, y);
        patch.set |= TransformPatch::SKEW;
        self
    }
    /// Tilt by `radians` about the horizontal axis through the transform
    /// origin, as CSS `rotateX`: positive tips the top away. Seen in
    /// `perspective`; without one it only shortens the view.
    fn rotate_x(mut self, radians: f32) -> Self {
        let patch = &mut self
            .styles_mut()
            .fields_mut(bit::LOCAL_TRANSFORM)
            .local_transform;
        patch.values.rotate_x = radians;
        patch.set |= TransformPatch::ROTATE_X;
        self
    }
    /// Tilt by `radians` about the vertical axis through the transform
    /// origin, as CSS `rotateY`: positive turns the right side away.
    fn rotate_y(mut self, radians: f32) -> Self {
        let patch = &mut self
            .styles_mut()
            .fields_mut(bit::LOCAL_TRANSFORM)
            .local_transform;
        patch.values.rotate_y = radians;
        patch.set |= TransformPatch::ROTATE_Y;
        self
    }
    /// See a tilt (`rotate_x`, `rotate_y`) from `distance` logical points
    /// away, as CSS `perspective()`: the side nearer shows larger. A tilted
    /// view composites as a projected layer, without repainting.
    fn perspective(mut self, distance: f32) -> Self {
        let patch = &mut self
            .styles_mut()
            .fields_mut(bit::LOCAL_TRANSFORM)
            .local_transform;
        patch.values.perspective = distance;
        patch.set |= TransformPatch::PERSPECTIVE;
        self
    }
    /// The pivot of `scale`, `rotate` and `skew`, as fractions of the
    /// view's own size: (0.5, 0.5), its centre, by default.
    fn transform_origin(mut self, x: f32, y: f32) -> Self {
        let patch = &mut self
            .styles_mut()
            .fields_mut(bit::LOCAL_TRANSFORM)
            .local_transform;
        patch.values.origin = (x, y);
        patch.set |= TransformPatch::ORIGIN;
        self
    }
}
impl Styled for Styles {
    fn styles_mut(&mut self) -> &mut Styles {
        self
    }
}
impl Styled for &mut Styles {
    fn styles_mut(&mut self) -> &mut Styles {
        self
    }
}
/// Opaque color from `0xRRGGBB`.
pub const fn rgb(value: u32) -> Color {
    Color((value >> 16) as u8, (value >> 8) as u8, value as u8, 255)
}
/// Color from `0xRRGGBBAA`, with alpha in the least significant byte.
pub const fn rgba(value: u32) -> Color {
    Color(
        (value >> 24) as u8,
        (value >> 16) as u8,
        (value >> 8) as u8,
        value as u8,
    )
}

#[cfg(test)]
mod tests {
    #[test]
    fn backdrop_blur_is_blur_by_its_clear_name() {
        assert!(Styles::new().backdrop_blur(4.) == Styles::new().blur(4.));
        assert!(Styles::new().backdrop_blur(4.) != Styles::new().filter_blur(4.));
    }

    use super::*;

    /// A style setting none of the properties few set (`Overflow`) holds
    /// no allocation for them, nor does one given their paint unset; one
    /// setting them reads, compares, merges and clones them as any other.
    #[test]
    fn rare_properties_cost_nothing_unset_and_behave_as_others_set() {
        let plain = Styles::new().w(10.).opacity(0.5).bg(Color(1, 2, 3, 255));
        assert!(!plain.fields.overflow.is_allocated());
        let mut painted = plain.clone();
        painted
            .fields_mut(bit::BLUR)
            .set_overflow_paint(OverflowPaint::default());
        assert!(!painted.fields.overflow.is_allocated());
        let rare = || {
            Styles::new()
                .w(10.)
                .brightness(0.5)
                .clip_inset(1., 2., 3., 4.)
                .italic(true)
        };
        let styles = rare();
        assert!(styles.fields.overflow.is_allocated());
        assert_eq!(styles.overflow().brightness, Some(0.5));
        assert_eq!(styles.overflow().italic, Some(true));
        assert!(styles == rare() && styles.clone() == styles);
        assert!(styles != Styles::new().w(10.).brightness(0.6));
        let mut merged = Styles::new().h(5.);
        merged.merge(&styles);
        assert_eq!(merged.overflow().clip_inset, styles.overflow().clip_inset);
        assert_eq!(merged.overflow().brightness, Some(0.5));
        assert_eq!(merged.set_bits() & bit::BRIGHTNESS, bit::BRIGHTNESS);
    }
    /// A style function setting a rare property every frame takes back the
    /// box the last frame's style gave up: once running, none is allocated.
    #[test]
    fn a_rare_property_set_every_frame_reuses_its_box() {
        let address = |styles: &Styles| styles.fields.overflow.0.map(|o| o.as_ptr() as usize);
        let first = Styles::new().contrast(1.5);
        let at = address(&first);
        drop(first);
        let again = Styles::new().contrast(1.2);
        assert_eq!(address(&again), at);
        assert_eq!(again.overflow().contrast, Some(1.2));
        // Taken from the pool, it holds nothing of the last style.
        assert_eq!(again.overflow().brightness, None);
    }
    /// Values an optional number takes: unset, zeros, ordinary numbers,
    /// infinities and NaNs (one with `Opt32`'s own payload, which must
    /// still read back as a number, not as unset).
    fn optional_numbers() -> [Option<f32>; 10] {
        [
            None,
            Some(0.),
            Some(-0.),
            Some(1.5),
            Some(f32::MIN_POSITIVE),
            Some(f32::INFINITY),
            Some(f32::NEG_INFINITY),
            Some(f32::NAN),
            Some(-f32::NAN),
            Some(f32::from_bits(Opt32::NONE)),
        ]
    }
    /// `Opt32` holds what an `Option<f32>` holds and compares and prints as
    /// it does: a NaN equal to nothing, itself included.
    #[test]
    fn an_optional_number_in_four_bytes_behaves_as_an_option() {
        assert_eq!(std::mem::size_of::<Opt32>(), 4);
        for a in optional_numbers() {
            let x = Opt32::from(a);
            assert_eq!(x.is_some(), a.is_some(), "{a:?}");
            assert_eq!(x.is_none(), a.is_none(), "{a:?}");
            match (x.get(), a) {
                (None, None) => {}
                (Some(got), Some(set)) if set.is_nan() => assert!(got.is_nan(), "{a:?}"),
                (Some(got), Some(set)) => assert_eq!(got.to_bits(), set.to_bits(), "{a:?}"),
                (got, set) => panic!("{set:?} read back as {got:?}"),
            }
            assert_eq!(format!("{x:?}"), format!("{a:?}"));
            for b in optional_numbers() {
                assert_eq!(x == Opt32::from(b), a == b, "{a:?} {b:?}");
                assert_eq!(x == b, a == b, "{a:?} {b:?}");
            }
        }
    }
    /// Each optional number of `Styles` compares, and counts as set, as the
    /// `Option<f32>` it was: two styles differing in it alone are equal
    /// exactly when those options are, and it differs from its default
    /// exactly when it is set.
    #[test]
    fn styles_optional_numbers_compare_as_options() {
        macro_rules! each {
            ($($field:ident $bit:ident),* $(,)?) => {$(
                each!(@one $field $bit |styles: &mut Styles, value: Opt32| {
                    styles.fields_mut(bit::$bit).$field = value;
                });
            )*};
            (overflow $($field:ident $bit:ident),* $(,)?) => {$(
                each!(@one $field $bit |styles: &mut Styles, value: Opt32| {
                    styles.fields_mut(bit::$bit).overflow_mut().$field = value;
                });
            )*};
            (@one $field:ident $bit:ident $write:expr) => {
                for a in optional_numbers() {
                    let with = |value: Option<f32>| {
                        let mut styles = Styles::default();
                        $write(&mut styles, value.into());
                        styles
                    };
                    assert_eq!(with(a).fields.mask() != 0, a.is_some(), "{} {a:?}", stringify!($field));
                    for b in optional_numbers() {
                        assert_eq!(with(a) == with(b), a == b, "{} {a:?} {b:?}", stringify!($field));
                    }
                }
            };
        }
        each!(
            width WIDTH, height HEIGHT, width_percent WIDTH_PERCENT,
            height_percent HEIGHT_PERCENT, gap GAP, flex_grow FLEX_GROW,
            flex_shrink FLEX_SHRINK, text_size TEXT_SIZE, radius RADIUS,
            border_width BORDER_WIDTH, opacity OPACITY,
        );
        each!(overflow
            blur BLUR, edge_fade EDGE_FADE, filter_blur FILTER_BLUR,
            brightness BRIGHTNESS, contrast CONTRAST, saturate SATURATE,
        );
    }
    /// Comparing only the fields set in either (`Styles::set`) agrees with
    /// comparing every field, however the styles were made: by builders,
    /// merged, from fields, with paint taken out, or set and set back.
    #[test]
    fn styles_compare_as_all_their_fields_do() {
        use crate::layout::LayoutOptions;
        let mut cases = vec![
            Styles::new(),
            Styles::new().mt(3.).h(10.),
            Styles::new().mt(3.).h(11.),
            Styles::new().p(4.).bg(rgb(0x112233)).rounded(2.),
            Styles::new().opacity(0.5).text_color(rgb(0xffffff)),
            Styles::new().opacity(0.5),
            Styles::new().translate(1., 2.).scale(2.),
            Styles::new().w(10.).w_percent(50.),
            Styles::new().min_w(3.).font_weight(700),
            Styles::new()
                .gap(2.)
                .layout_options(LayoutOptions::default()),
            Styles::from(StylesFields {
                height: Some(10.).into(),
                margin: Some(Insets {
                    top: 3.,
                    ..Insets::default()
                }),
                margin_sides: 1,
                ..StylesFields::default()
            }),
        ];
        let mut merged = Styles::new().p(1.).bg(rgb(0x445566));
        merged.merge(&Styles::new().opacity(0.25).mt(2.));
        cases.push(merged);
        // Paint set, then taken out: as if never set.
        let mut taken = Styles::new().opacity(0.5).mt(3.).h(10.);
        let fields = taken.fields_taken(bit::OPACITY);
        fields.opacity = Opt32::none();
        cases.push(taken);
        // Set, then set back: its bit stays, its value is the default's.
        let mut back = Styles::new().gap(2.);
        back.fields_mut(bit::GAP).gap = Opt32::none();
        cases.push(back);
        for a in &cases {
            for b in &cases {
                assert_eq!(a == b, **a == **b, "{a:?} against {b:?}");
                let sizes = |s: &StylesFields| {
                    let mut s = StylesFields {
                        overflow: s.overflow.clone(),
                        rare: s.rare.clone(),
                        tokens: s.tokens.clone(),
                        layout_options: s.layout_options.clone(),
                        font_family: s.font_family.clone(),
                        paint_background: s.paint_background.clone(),
                        ..*s
                    };
                    (
                        s.width,
                        s.height,
                        s.width_percent,
                        s.height_percent,
                        s.margin,
                    ) = (
                        Opt32::none(),
                        Opt32::none(),
                        Opt32::none(),
                        Opt32::none(),
                        None,
                    );
                    s
                };
                assert_eq!(
                    a.eq_then_sizes(b, 0),
                    (sizes(a) == sizes(b)).then_some(a == b)
                );
            }
        }
    }
    /// `Clone for Styles` copies bitwise and then clones the owning fields:
    /// each clone holds its own reference, and dropping either side releases
    /// only its own (`cargo +nightly miri test -p zgx --lib style::` checks
    /// the aliasing too).
    #[test]
    fn a_clone_owns_its_shared_fields() {
        use std::sync::Arc;
        let options = Arc::new(crate::layout::LayoutOptions::default());
        let rare = Arc::new(RareStyles::default());
        let tokens = Arc::new(crate::theme::Bindings::default());
        let stops: Arc<[crate::canvas::GradientStop]> = Arc::from([]);
        let family: Arc<str> = Arc::from("Inter");
        let original = Styles::from(StylesFields {
            layout_options: Some(options.clone()),
            font_family: Some(crate::text_layout::FontFamily::Named(family.clone())),
            paint_background: Some(Some(crate::decoration::Background::Linear {
                angle: 1.,
                stops: stops.clone(),
            })),
            rare: Some(rare.clone()),
            tokens: Some(tokens.clone()),
            width: Some(12.).into(),
            ..StylesFields::default()
        });
        let copy = original.clone();
        assert_eq!(copy, original);
        let counts = || {
            [
                Arc::strong_count(&options),
                Arc::strong_count(&rare),
                Arc::strong_count(&tokens),
                Arc::strong_count(&stops),
                Arc::strong_count(&family),
            ]
        };
        assert_eq!(counts(), [3; 5]);
        drop(original);
        assert_eq!(counts(), [2; 5]);
        assert_eq!(copy.width, Some(12.));
        drop(copy);
        assert_eq!(counts(), [1; 5]);
    }
    #[test]
    fn percentage_patches_normalize_invalid_values_and_replace_pixel_axes() {
        let patch = Styles::new().size(80., 30.).w_percent(50.).h_full();
        assert_eq!(patch.width, None);
        assert_eq!(patch.height, None);
        assert_eq!(patch.width_percent, Some(0.5));
        assert_eq!(patch.height_percent, Some(1.));
        let pixels = patch.style(Styles::new().w(20.));
        assert_eq!(pixels.width, Some(20.));
        assert_eq!(pixels.width_percent, None);
        assert_eq!(pixels.height_percent, Some(1.));
        for invalid in [-1., f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
            assert_eq!(
                Styles::new().w_percent(invalid),
                Styles::new().w_percent(0.)
            );
        }
        assert_eq!(Styles::new().w_percent(150.).width_percent, Some(1.5));
    }
    #[test]
    fn conditional_transformations_preserve_sparse_patch_edges() {
        fn horizontal<S: Styled>(style: S) -> S {
            style.px(12.)
        }

        let patch = Styles::new()
            .apply(horizontal)
            .when(false, |_| panic!("false branch must not run"))
            .when_some(None::<f32>, |_, _| panic!("missing value must not run"))
            .when(true, |style| style.pl(14.))
            .when_some(Some(String::from("owned")), |style, label| {
                assert_eq!(label, "owned");
                style.pr(16.)
            });
        assert_eq!(patch.padding_sides, 2 | 8);
        let result = Styles::new().p(8.).pt(3.).style(patch);
        assert_eq!(
            result.padding,
            Some(Insets {
                top: 3.,
                right: 16.,
                bottom: 8.,
                left: 14.,
            })
        );
    }
    #[test]
    fn patches_only_override_specified_properties() {
        let base = Styles::new()
            .size(120., 40.)
            .gap(8.)
            .bg(rgb(0x102030))
            .opacity(0.5);
        let result = base.clone().style(Styles::new().w(200.).border_1());
        assert_eq!(result.width, Some(200.));
        assert_eq!(result.height, base.height);
        assert_eq!(result.gap, base.gap);
        assert_eq!(result.background, base.background);
        assert_eq!(result.opacity, base.opacity);
        assert_eq!(result.border_width, Some(1.));
        assert_eq!(result.clone().style(Styles::new()), result);
    }
    #[test]
    fn side_spacing_obeys_chain_and_merge_precedence() {
        let style = Styles::new().p(8.).px(12.).pt(3.).m(4.).mb(9.);
        assert_eq!(
            style.padding,
            Some(Insets {
                top: 3.,
                right: 12.,
                bottom: 8.,
                left: 12.
            })
        );
        assert_eq!(
            style.margin,
            Some(Insets {
                top: 4.,
                right: 4.,
                bottom: 9.,
                left: 4.
            })
        );
        let merged = style.style(Styles::new().py(7.).ml(2.));
        assert_eq!(
            merged.padding,
            Some(Insets {
                top: 7.,
                right: 12.,
                bottom: 7.,
                left: 12.
            })
        );
        assert_eq!(
            merged.margin,
            Some(Insets {
                top: 4.,
                right: 4.,
                bottom: 9.,
                left: 2.
            })
        );
        assert_eq!(merged.p(1.).padding, Some(Insets::all(1.)));
    }
    #[test]
    fn effect_reset_and_color_order_are_explicit() {
        let shadow = BoxShadow {
            color: rgba(0x10203040),
            offset: Transform { x: 1., y: 2. },
            blur_radius: 4.,
            spread: 0.,
        };
        let style = Styles::new()
            .shadow(shadow)
            .overflow_hidden()
            .isolated(true)
            .style(
                Styles::new()
                    .shadow_none()
                    .overflow_visible()
                    .isolated(false),
            );
        assert_eq!(style.rare().shadow, Some(None));
        assert_eq!(style.clip, Some(false));
        assert_eq!(style.overflow().isolated, Some(false));
        assert_eq!(rgb(0x102030), Color(16, 32, 48, 255));
        assert_eq!(rgba(0x10203040), Color(16, 32, 48, 64));
    }
}
