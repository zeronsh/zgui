//! Immutable SVG source. Parsing and device-scale rasterization belong to the renderer.
use crate::{
    affine::Affine,
    scene::{Color, Rect},
};
use std::sync::Arc;
/// SVG source to paint, with an optional tint and paint transform. Clones share
/// the bytes.
#[derive(Clone, Debug)]
pub struct SvgData {
    source_id: u64,
    bytes: Arc<[u8]>,
    tint: Option<Color>,
    /// Tinted with the text colour the view inherits (CSS `currentColor`).
    current_color: bool,
    transform: Affine,
}
impl SvgData {
    /// Bounded source storage; XML validity is checked by the renderer. SVG text
    /// and external resources follow the renderer's restricted SVG asset policy.
    pub fn new(bytes: impl Into<Arc<[u8]>>) -> Result<Self, &'static str> {
        let bytes = bytes.into();
        if bytes.is_empty() || bytes.len() > 4 * 1024 * 1024 {
            return Err("SVG source requires 1 byte..=4 MiB");
        }
        Ok(Self {
            source_id: crate::image::next_identity()?,
            bytes,
            tint: None,
            current_color: false,
            transform: Affine::IDENTITY,
        })
    }
    /// Immutable source identity, preserved by tint and affine refinements.
    pub fn source_id(&self) -> u64 {
        self.source_id
    }
    /// The SVG source.
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }
    /// The colour it paints in, if tinted.
    pub fn tint(&self) -> Option<Color> {
        self.tint
    }
    /// A copy painted in `color`: each pixel takes its red, green and blue, and its
    /// alpha is scaled by the colour's. Replaces `current_color`.
    pub fn tinted(&self, color: Color) -> Self {
        Self {
            tint: Some(color),
            current_color: false,
            ..self.clone()
        }
    }
    /// Tinted, wherever it is shown, with the text colour its view inherits
    /// (CSS `currentColor`): an icon in a button takes the button's colour,
    /// hover and theme changes included.
    pub fn current_color(&self) -> Self {
        Self {
            tint: None,
            current_color: true,
            ..self.clone()
        }
    }
    /// Whether it paints in its view's text colour (see `current_color`).
    pub fn follows_text_color(&self) -> bool {
        self.current_color
    }
    /// A copy painted through `transform` about its box's centre; layout is
    /// unchanged, and a non-finite matrix becomes the identity.
    pub fn transformed(&self, transform: Affine) -> Self {
        Self {
            transform: if transform.is_finite() {
                transform
            } else {
                Affine::IDENTITY
            },
            ..self.clone()
        }
    }
    /// Its paint transform (the identity unless `transformed`).
    pub fn transform(&self) -> Affine {
        self.transform
    }
    /// Whether both rasterise to the same pixels: the same source and tint.
    pub fn same_pixels(&self, other: &Self) -> bool {
        self.bytes == other.bytes && self.tint == other.tint
    }
    /// Its transform about the centre of `bounds`, the box it is laid out in.
    pub fn paint_transform(&self, bounds: Rect) -> Affine {
        if self.transform == Affine::IDENTITY {
            self.transform
        } else {
            self.transform.around(
                bounds.x + bounds.width * 0.5,
                bounds.y + bounds.height * 0.5,
            )
        }
    }
    /// Where it paints when laid out in `bounds`: the box covering `bounds` under
    /// its transform; empty when the transform is singular.
    pub fn paint_bounds(&self, bounds: Rect) -> Rect {
        if self.transform.inverse().is_none() {
            Rect::default()
        } else {
            self.paint_transform(bounds).bounds(bounds)
        }
    }
}

impl PartialEq for SvgData {
    fn eq(&self, other: &Self) -> bool {
        self.same_pixels(other) && self.transform == other.transform
    }
}
