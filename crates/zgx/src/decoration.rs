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
    /// Dashes `length` long with `gap` between them. A `length` of 0 draws
    /// dots (CSS's `dotted`, `BorderStyle::DOTTED`): round, as wide as the
    /// line, `gap` apart (0: as far apart as they are wide), spaced evenly
    /// round the box.
    Dashed {
        /// Each dash's length, in logical points (above 0; 0, a dot).
        length: f32,
        /// The room between dashes, in logical points (above 0; for dots,
        /// 0 or more).
        gap: f32,
    },
}
impl BorderStyle {
    /// Round dots as wide as the line, as far apart as they are wide, evenly
    /// round the box (CSS's `dotted`): `Dashed` of length 0.
    pub const DOTTED: Self = Self::Dashed {
        length: 0.,
        gap: 0.,
    };
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
/// The layer shader `hue_rotate`, `sepia` and `invert` share: the layer's
/// colours through the matrix in its uniforms, in the straight sRGB values
/// CSS filters them in. One for a thread: its pipeline is made once.
pub fn color_filter_shader() -> Arc<FillShader> {
    thread_local! {
        static SHADER: Arc<FillShader> = FillShader::new(
            "fn layer(input: LayerInput) -> vec4<f32> {
    let c = input.color.rgb;
    let r = dot(input.params[0].xyz, c) + input.params[0].w;
    let g = dot(input.params[1].xyz, c) + input.params[1].w;
    let b = dot(input.params[2].xyz, c) + input.params[2].w;
    return vec4(clamp(vec3(r, g, b), vec3(0.0), vec3(1.0)), input.color.a);
}",
            None,
        );
    }
    SHADER.with(Arc::clone)
}

/// The fill shader `Styled::bg_radial` shares: `params[0]` its centre, as
/// fractions of the box, its size against CSS's default ellipse reaching the
/// farthest corner, and where its middle stop lies; `params[1..4]` its three
/// colours, mixed premultiplied as CSS's are. One for a thread: its pipeline
/// is made once.
pub fn radial_shader() -> Arc<FillShader> {
    thread_local! {
        static SHADER: Arc<FillShader> = FillShader::new(
            "fn fill(input: FillInput) -> vec4<f32> {
    let g = input.params[0];
    let centre = g.xy;
    let far = max(centre, vec2(1.0) - centre) * 1.41421356 * max(g.z, 0.0001);
    let t = clamp(length((input.p / input.size - centre) / far), 0.0, 1.0);
    let a = vec4(input.params[1].rgb * input.params[1].a, input.params[1].a);
    let b = vec4(input.params[2].rgb * input.params[2].a, input.params[2].a);
    let c = vec4(input.params[3].rgb * input.params[3].a, input.params[3].a);
    let m = select(
        mix(b, c, (t - g.w) / max(1.0 - g.w, 0.00001)),
        mix(a, b, t / max(g.w, 0.00001)),
        t < g.w,
    );
    return vec4(m.rgb / max(m.a, 0.00001), m.a);
}",
            None,
        );
    }
    SHADER.with(Arc::clone)
}
/// A radial gradient's stops: two or three, from the centre to the edge, as
/// an array of either length. More can't be written: they don't fit its
/// shader's uniforms (native gradients wait for 0.2).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RadialStops {
    first: crate::canvas::GradientStop,
    middle: Option<crate::canvas::GradientStop>,
    last: crate::canvas::GradientStop,
}
impl From<[crate::canvas::GradientStop; 2]> for RadialStops {
    fn from([first, last]: [crate::canvas::GradientStop; 2]) -> Self {
        Self {
            first,
            middle: None,
            last,
        }
    }
}
impl From<[crate::canvas::GradientStop; 3]> for RadialStops {
    fn from([first, middle, last]: [crate::canvas::GradientStop; 3]) -> Self {
        Self {
            first,
            middle: Some(middle),
            last,
        }
    }
}
impl RadialStops {
    /// Its stops, from the centre out.
    pub(crate) fn stops(&self) -> impl Iterator<Item = crate::canvas::GradientStop> {
        [Some(self.first), self.middle, Some(self.last)]
            .into_iter()
            .flatten()
    }
}
/// A linear or radial gradient over a node's box, at whatever size it is
/// laid out. As a mask (`Scene::set_mask_gradient`, CSS's `mask-image`
/// with a gradient) its alpha shows the subtree: where it's opaque the
/// subtree shows, where it's transparent it doesn't, its colours aside.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum Gradient {
    /// Along `angle` (radians clockwise from left to right), its ends
    /// spanning the box, as `Background::Linear` (CSS's `linear-gradient`).
    Linear {
        /// The direction, in radians clockwise from left to right.
        angle: f32,
        /// The alphas along it, in offset order.
        stops: Arc<[crate::canvas::GradientStop]>,
    },
    /// An ellipse the box's shape about `center` (fractions of the box),
    /// `radius` 1 reaching its farthest corner, as `Styled::bg_radial`
    /// (CSS's `radial-gradient`): a vignette.
    Radial {
        /// Its middle, in fractions of the box.
        center: (f32, f32),
        /// How far it reaches: 1, the farthest corner.
        radius: f32,
        /// The alphas from the middle out.
        stops: RadialStops,
    },
}
impl Gradient {
    /// Its alpha over a `width` by `height` box, one texel a point (at most
    /// `MASK_TEXELS` a side, each then covering more), sampled at each
    /// texel's middle: a mask image (`ImageMask`) of the box.
    pub(crate) fn image(&self, width: f32, height: f32) -> Option<crate::image::ImageData> {
        if !(width.is_finite() && height.is_finite()) || width <= 0. || height <= 0. {
            return None;
        }
        let (columns, rows) = (
            (width.ceil() as u32).clamp(1, MASK_TEXELS),
            (height.ceil() as u32).clamp(1, MASK_TEXELS),
        );
        let stops: Vec<crate::canvas::GradientStop> = match self {
            Self::Linear { stops, .. } => stops.to_vec(),
            Self::Radial { stops, .. } => stops.stops().collect(),
        };
        let alpha = |t: f32| -> u8 {
            let t = if t.is_finite() { t.clamp(0., 1.) } else { 0. };
            let at = stops.iter().position(|stop| stop.offset >= t);
            let a = match at {
                None => stops.last().map_or(255., |stop| f32::from(stop.color.3)),
                Some(0) => f32::from(stops[0].color.3),
                Some(i) => {
                    let (before, after) = (stops[i - 1], stops[i]);
                    let span = after.offset - before.offset;
                    let f = if span > 0. {
                        (t - before.offset) / span
                    } else {
                        1.
                    };
                    f32::from(before.color.3)
                        + (f32::from(after.color.3) - f32::from(before.color.3)) * f
                }
            };
            a.round().clamp(0., 255.) as u8
        };
        let t: Box<dyn Fn(f32, f32) -> f32> = match *self {
            Self::Linear { angle, .. } => {
                let (sin, cos) = angle.sin_cos();
                let half = (width * cos.abs() + height * sin.abs()) * 0.5;
                Box::new(move |x, y| {
                    let along = (x - width * 0.5) * cos + (y - height * 0.5) * sin;
                    if half > 0. {
                        along / (2. * half) + 0.5
                    } else {
                        0.
                    }
                })
            }
            Self::Radial { center, radius, .. } => {
                // The farthest corner, in fractions of the box.
                let far = [(0., 0.), (1., 0.), (0., 1.), (1., 1.)]
                    .into_iter()
                    .map(|(x, y): (f32, f32)| (x - center.0).hypot(y - center.1))
                    .fold(0f32, f32::max);
                let reach = radius * far;
                Box::new(move |x, y| {
                    let d = (x / width - center.0).hypot(y / height - center.1);
                    if reach > 0. { d / reach } else { 1. }
                })
            }
        };
        let mut pixels = Vec::with_capacity((columns * rows * 4) as usize);
        for row in 0..rows {
            let y = (row as f32 + 0.5) * height / rows as f32;
            for column in 0..columns {
                let x = (column as f32 + 0.5) * width / columns as f32;
                pixels.extend([255, 255, 255, alpha(t(x, y))]);
            }
        }
        crate::image::ImageData::new(columns, rows, pixels).ok()
    }
}
/// The most texels a gradient mask's image has a side (`Gradient::image`).
const MASK_TEXELS: u32 = 512;
/// `Styled::bg_radial`'s uniforms: `center` in fractions of the box, `radius`
/// 1 for CSS's default.
pub fn radial_uniforms(center: (f32, f32), radius: f32, stops: RadialStops) -> ShaderUniforms {
    let color = |stop: &crate::canvas::GradientStop| {
        let c = stop.color;
        [
            f32::from(c.0) / 255.,
            f32::from(c.1) / 255.,
            f32::from(c.2) / 255.,
            f32::from(c.3) / 255.,
        ]
    };
    let (first, last) = (color(&stops.first), color(&stops.last));
    let (middle, at) = match &stops.middle {
        Some(middle) => (color(middle), middle.offset),
        None => (mix_colors(first, last), 0.5),
    };
    ShaderUniforms {
        time: 0.,
        params: [[center.0, center.1, radius, at], first, middle, last],
    }
}
/// Halfway between two straight colours, as a premultiplied mix is.
fn mix_colors(a: [f32; 4], b: [f32; 4]) -> [f32; 4] {
    let alpha = (a[3] + b[3]) / 2.;
    let channel = |k: usize| {
        if alpha > 0. {
            (a[k] * a[3] + b[k] * b[3]) / 2. / alpha
        } else {
            0.
        }
    };
    [channel(0), channel(1), channel(2), alpha]
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
    /// `inset 0 1px`), their blur as CSS's: softened into the box from that
    /// edge, half their colour on it.
    pub inset_shadows: Option<Arc<[BoxShadow]>>,
    /// Where it clips (`overflow: hidden`), its descendants fade out over
    /// this many points inside its rounded edge, rather than being cut at
    /// it: a soft oval reveal, its clip inset animated. Computed per pixel
    /// in the shader quads already run for rounded clips: no layer, no
    /// mask. 0 cuts.
    pub clip_feather: f32,
}
