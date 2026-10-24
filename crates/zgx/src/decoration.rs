//! Optional detailed paint styles; ordinary quads keep their existing fast path.
use crate::{
    canvas::{Brush, GradientStop},
    scene::{BoxShadow, Insets},
};
use std::sync::Arc;
/// A radius for each corner of a box, in logical points.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Corners {
    /// The top-left corner's.
    pub top_left: f32,
    /// The top-right corner's.
    pub top_right: f32,
    /// The bottom-right corner's.
    pub bottom_right: f32,
    /// The bottom-left corner's.
    pub bottom_left: f32,
}
impl Corners {
    /// The same `radius` at every corner.
    pub fn all(radius: f32) -> Self {
        Self {
            top_left: radius,
            top_right: radius,
            bottom_right: radius,
            bottom_left: radius,
        }
    }
}
/// How a border is drawn.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum BorderStyle {
    /// One unbroken line.
    #[default]
    Solid,
    /// Dashes `length` long with `gap` between them.
    Dashed {
        /// Each dash's length, in logical points (above 0).
        length: f32,
        /// The room between dashes, in logical points (above 0).
        gap: f32,
    },
}
/// What fills a box, in place of or over its `fill` colour.
#[derive(Clone, Debug, PartialEq)]
pub enum Background {
    /// A brush: one colour, a gradient or stripes.
    Brush(Brush),
    /// Angle in clockwise radians; endpoints span the allocated box.
    Linear {
        /// The direction, in radians clockwise from left to right.
        angle: f32,
        /// The colours along it, in offset order.
        stops: Arc<[GradientStop]>,
    },
    /// An image filling the box as `fit` places it (centred; CSS
    /// `background-size` cover/contain, `object-fit` alike), over the
    /// box's `fill` colour, cut to its rounded corners, under its border
    /// and children.
    Image {
        /// The decoded image.
        image: Arc<crate::image::ImageData>,
        /// How it is fitted to the box.
        fit: crate::style::ObjectFit,
    },
    /// A fragment shader filling the box (see `FillShader`), over its
    /// `fill` colour where it returns transparency, cut to its rounded
    /// corners, under its border and children. Its uniforms are the node's
    /// (`Scene::set_shader_uniforms`).
    Shader(Arc<FillShader>),
}
/// A fill written as a WGSL function the GPU runs for every pixel of a box:
///
/// ```wgsl
/// fn fill(input: FillInput) -> vec4<f32>
/// ```
///
/// returning a straight-alpha colour. `FillInput` holds `p` (the pixel's
/// logical position from the box's top-left), `size` (the box's logical
/// size), `time` (seconds, as the node's uniforms set it), `scale` (device
/// pixels per logical pixel) and `params` (`array<vec4<f32>, 4>`: the
/// node's sixteen user floats, colours or anything else). Its helper
/// functions and constants may sit alongside; names starting `zgx_` are
/// reserved. A shader that fails to compile fills with the box's colour.
#[derive(Debug)]
pub struct FillShader {
    id: u64,
    source: Arc<str>,
    fps: Option<f32>,
}
impl FillShader {
    /// `source`, its time advancing at most `fps` times a second (a dither
    /// drifting at 30 fps redraws at 30 fps): `None` for every frame.
    pub fn new(source: impl Into<Arc<str>>, fps: Option<f32>) -> Arc<Self> {
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
        Arc::new(Self {
            id: NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed),
            source: source.into(),
            fps: fps.filter(|fps| fps.is_finite() && *fps > 0.),
        })
    }
    /// Stable for this shader's lifetime: renderers cache its pipeline by it.
    pub fn id(&self) -> u64 {
        self.id
    }
    /// Its WGSL source.
    pub fn source(&self) -> &str {
        &self.source
    }
    /// The most times a second its time advances; `None`, every frame.
    pub fn fps(&self) -> Option<f32> {
        self.fps
    }
}
impl PartialEq for FillShader {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}
/// A shader fill's per-node uniforms (`Scene::set_shader_uniforms`).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ShaderUniforms {
    /// Seconds.
    pub time: f32,
    /// Sixteen user values, as four vec4s.
    pub params: [[f32; 4]; 4],
}
/// What a box's plain style can't say: per-corner radii, per-side and
/// dashed borders, gradient, image and shader backgrounds, more shadows.
/// Each unset field keeps the plain style's.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Decoration {
    /// What fills the box; `None`, the style's `fill` colour.
    pub background: Option<Background>,
    /// Its corner radii; `None`, the style's `radius` at every corner.
    pub corners: Option<Corners>,
    /// Its border widths; `None`, the style's `border_width` on every side.
    pub border_widths: Option<Insets>,
    /// Solid or dashed.
    pub border_style: BorderStyle,
    /// Some(empty) explicitly clears all shadows; None inherits the legacy shadow.
    pub shadows: Option<Arc<[BoxShadow]>>,
    /// Shadows inside the box (CSS's `inset`), over its fill and under its
    /// border and children: the box less itself moved by the offset and
    /// shrunk by the spread, cut to its corners (a 1-point top highlight is
    /// `inset 0 1px`). Drawn sharp: their blur is not applied.
    pub inset_shadows: Option<Arc<[BoxShadow]>>,
}
