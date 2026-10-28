//! Typed, sparse surface recipes for native material decoration.
//!
//! Missing, explicit clears and values remain distinct until profile resolution.
//! These values contain no script objects; platforms retain only typed Rust data.
use crate::{scene::Color, style::StrokeAlign};
use std::sync::Arc;

/// A source field: omitted, explicitly null, or supplied.
#[derive(Clone, Debug, Default, PartialEq)]
pub enum SurfaceField<T> {
    /// No value was present in this patch.
    #[default]
    Missing,
    /// An explicit null clears the inherited value.
    Clear,
    /// A supplied value.
    Value(T),
}
impl<T> SurfaceField<T> {
    /// The supplied value, excluding omitted fields and explicit clears.
    pub fn value(&self) -> Option<&T> {
        match self {
            Self::Value(value) => Some(value),
            _ => None,
        }
    }
    /// Whether this patch explicitly supplies or clears the field.
    pub fn is_present(&self) -> bool {
        !matches!(self, Self::Missing)
    }
}
/// Surface compositing operations, retained independently from paint colors.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SurfaceBlend {
    /// Source over.
    Normal,
    /// Overlay compositing.
    Overlay,
    /// Hue and saturation from the source.
    Color,
    /// Additive lighter compositing.
    PlusLighter,
    /// Additive darker compositing.
    PlusDarker,
    /// Multiply compositing.
    Multiply,
    /// Screen compositing.
    Screen,
}
impl SurfaceBlend {
    /// The source spelling, passed without approximation to native compositors.
    pub fn name(self) -> &'static str {
        match self {
            Self::Normal => "normal",
            Self::Overlay => "overlay",
            Self::Color => "color",
            Self::PlusLighter => "plusLighter",
            Self::PlusDarker => "plusDarker",
            Self::Multiply => "multiply",
            Self::Screen => "screen",
        }
    }
    /// Read a supported source spelling.
    pub fn parse(name: &str) -> Option<Self> {
        Some(match name {
            "normal" => Self::Normal,
            "overlay" => Self::Overlay,
            "color" => Self::Color,
            "plusLighter" => Self::PlusLighter,
            "plusDarker" => Self::PlusDarker,
            "multiply" => Self::Multiply,
            "screen" => Self::Screen,
            _ => return None,
        })
    }
}
/// A source color together with its separately resolved semantic color.
#[derive(Clone, Debug, PartialEq)]
pub struct SurfaceColor {
    /// Semantic role, retained for diagnostics and later profile resolution.
    pub role: Option<Arc<str>>,
    /// The source's light/fixed fallback as a conventional paint color.
    pub source: Color,
    /// The semantic color resolved by the caller; source alpha remains separate.
    pub resolved: Option<Color>,
    /// Source light/fixed RGBA before byte quantization.
    pub source_rgba: Option<[f64; 4]>,
    /// An adaptive source's dark fallback.
    pub source_dark: Option<Color>,
    /// Source dark RGBA before byte quantization.
    pub source_dark_rgba: Option<[f64; 4]>,
    /// Source kind, such as fixed, adaptive or canvas.
    pub kind: Option<Arc<str>>,
    /// Source color space, such as extendedSRGB.
    pub color_space: Option<Arc<str>>,
}
impl Default for SurfaceColor {
    fn default() -> Self {
        Self {
            role: None,
            source: Color(0, 0, 0, 0),
            resolved: None,
            source_rgba: None,
            source_dark: None,
            source_dark_rgba: None,
            kind: None,
            color_space: None,
        }
    }
}
/// How a decoration's corner radius relates to its surface.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum SurfaceRadius {
    /// Follow the surface's shape.
    MatchSurface,
    /// Use a fixed point radius.
    Fixed(f32),
    /// Inset concentrically from the surface radius.
    Concentric(f32),
}
/// Geometry kept separate from source color alpha.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SurfaceGeometry {
    /// Layer opacity, multiplied at compositing time.
    pub opacity: SurfaceField<f32>,
    /// Top, trailing, bottom, leading insets.
    pub insets: SurfaceField<[f32; 4]>,
    /// Radius relation to the underlying surface.
    pub radius: SurfaceField<SurfaceRadius>,
    /// Whether the layer is cut to the surface itself.
    pub clips_to_surface: SurfaceField<bool>,
}
/// A background, fill, stroke paint or shadow paint layer.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SurfaceLayer {
    /// Unflattened source and resolved semantic color.
    pub color: SurfaceField<SurfaceColor>,
    /// Native blend operation.
    pub blend: SurfaceField<SurfaceBlend>,
    /// Opacity and shape refinements.
    pub geometry: SurfaceGeometry,
    /// Explicit enabled state; absent remains absent.
    pub enabled: SurfaceField<bool>,
}
/// Position relative to a surface's native content.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SurfaceLevel {
    /// Under native and scripted content.
    BelowContent,
    /// Over native content and under subsequent overlay paint.
    AboveContent,
}
/// One ID-addressed stroke, in source array order.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SurfaceStroke {
    /// Identity used for patch merging.
    pub id: Arc<str>,
    /// Stroke paint, blend and geometry.
    pub layer: SurfaceLayer,
    /// Point width, without inventing a default for omitted values.
    pub width: SurfaceField<f32>,
    /// Inside, centered or outside the surface edge.
    pub alignment: SurfaceField<StrokeAlign>,
    /// Placement relative to native content.
    pub level: SurfaceField<SurfaceLevel>,
}
/// One ID-addressed shadow, retaining source geometry and blend.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SurfaceShadow {
    /// Identity used for patch merging.
    pub id: Arc<str>,
    /// Shadow paint, blend and geometry.
    pub layer: SurfaceLayer,
    /// Source blur radius in points.
    pub blur: SurfaceField<f32>,
    /// Horizontal offset.
    pub x: SurfaceField<f32>,
    /// Vertical offset, positive down.
    pub y: SurfaceField<f32>,
    /// Optional shadow spread.
    pub spread: SurfaceField<f32>,
    /// Whether the shadow is inside the shape.
    pub inner: SurfaceField<bool>,
    /// Remove the original surface from the shadow source.
    pub knockout: SurfaceField<bool>,
}
/// A native material, independent from non-glass decoration layers.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SurfaceGlass {
    /// Material enabled state; disabled does not request Gaussian blur.
    pub enabled: SurfaceField<bool>,
    /// Native tint color, preserving the source alpha.
    pub tint: SurfaceField<SurfaceColor>,
    /// Whether to apply the tint.
    pub uses_tint: SurfaceField<bool>,
    /// Material shape refinements.
    pub geometry: SurfaceGeometry,
    /// Native style name, when supplied.
    pub style: SurfaceField<Arc<str>>,
    /// Native window/material name, when supplied.
    pub material: SurfaceField<Arc<str>>,
}
/// Source shape refinements.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SurfaceShape {
    /// Source point radius.
    pub corner_radius: SurfaceField<f32>,
    /// Source curve name, when supplied.
    pub corner_curve: SurfaceField<Arc<str>>,
}
/// A complete surface or a sparse patch, with no script objects retained.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SurfaceRecipe {
    /// Native glass; explicit disabled glass still permits decoration layers.
    pub glass: SurfaceField<SurfaceGlass>,
    /// Backdrop paint.
    pub backdrop: SurfaceField<SurfaceLayer>,
    /// Fill above the backdrop/material.
    pub fill: SurfaceField<SurfaceLayer>,
    /// Additional fill, kept as a separate source layer.
    pub fill2: SurfaceField<SurfaceLayer>,
    /// Strokes in source order, keyed by their IDs when patched.
    pub strokes: Vec<SurfaceStroke>,
    /// Shadows in source order, keyed by their IDs when patched.
    pub shadows: Vec<SurfaceShadow>,
    /// Omitted, cleared or supplied stroke array, preserving explicit null.
    pub strokes_state: SurfaceField<()>,
    /// Omitted, cleared or supplied shadow array, preserving explicit null.
    pub shadows_state: SurfaceField<()>,
    /// Source shape refinements.
    pub shape: SurfaceField<SurfaceShape>,
    /// Content opacity, separate from decoration opacity.
    pub content_opacity: SurfaceField<f32>,
    /// Whether content is clipped to the source shape.
    pub clips_content: SurfaceField<bool>,
    /// Top, trailing, bottom, leading content insets.
    pub content_insets: SurfaceField<[f32; 4]>,
    /// Optional dedicated inner shadow.
    pub inner_shadow: SurfaceField<SurfaceShadow>,
}

impl SurfaceRecipe {
    /// Native glass is requested only when explicitly enabled in the resolved profile.
    pub fn glass_enabled(&self) -> bool {
        self.glass
            .value()
            .and_then(|glass| glass.enabled.value())
            .copied()
            .unwrap_or(false)
    }
    /// Conservative paint/damage bounds; the native compositor keeps source blur values.
    pub fn visual_bounds(&self, rect: crate::scene::Rect) -> crate::scene::Rect {
        use crate::scene::Rect;
        fn inset(rect: Rect, geometry: &SurfaceGeometry) -> Rect {
            let [top, right, bottom, left] = geometry.insets.value().copied().unwrap_or([0.; 4]);
            Rect::new(
                rect.x + left,
                rect.y + top,
                (rect.width - left - right).max(0.),
                (rect.height - top - bottom).max(0.),
            )
        }
        fn visible(rect: Rect, area: Rect, geometry: &SurfaceGeometry) -> Rect {
            if geometry.clips_to_surface.value() == Some(&true) {
                area.intersection(rect).unwrap_or_default()
            } else {
                area
            }
        }
        let mut out = rect;
        for layer in [&self.backdrop, &self.fill, &self.fill2]
            .into_iter()
            .filter_map(SurfaceField::value)
        {
            if layer.enabled.value() == Some(&true) {
                out = out.union(visible(rect, inset(rect, &layer.geometry), &layer.geometry));
            }
        }
        for stroke in &self.strokes {
            if stroke.layer.enabled.value() == Some(&false) {
                continue;
            }
            let width = stroke.width.value().copied().unwrap_or(0.5).max(0.);
            let spread = match stroke
                .alignment
                .value()
                .copied()
                .unwrap_or(StrokeAlign::Inside)
            {
                StrokeAlign::Inside => 0.,
                StrokeAlign::Center => width * 0.5,
                StrokeAlign::Outside => width,
            };
            let area = inset(rect, &stroke.layer.geometry).expand(spread);
            out = out.union(visible(rect, area, &stroke.layer.geometry));
        }
        for shadow in self.shadows.iter().chain(self.inner_shadow.value()) {
            if shadow.layer.enabled.value() == Some(&false) {
                continue;
            }
            let blur = shadow.blur.value().copied().unwrap_or(0.).max(0.);
            let spread = shadow.spread.value().copied().unwrap_or(0.).max(0.);
            let x = shadow.x.value().copied().unwrap_or(0.);
            let y = shadow.y.value().copied().unwrap_or(0.);
            let origin = inset(rect, &shadow.layer.geometry);
            let area = Rect::new(origin.x + x, origin.y + y, origin.width, origin.height)
                .expand(3. * blur + spread);
            out = out.union(visible(rect, area, &shadow.layer.geometry));
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scene::{QuadStyle, Rect};

    #[test]
    fn external_insets_strokes_and_shadows_extend_paint_but_explicit_clips_bound_them() {
        let rect = Rect::new(20., 30., 100., 80.);
        let mut recipe = SurfaceRecipe {
            strokes: vec![SurfaceStroke {
                width: SurfaceField::Value(2.),
                alignment: SurfaceField::Value(StrokeAlign::Outside),
                layer: SurfaceLayer {
                    geometry: SurfaceGeometry {
                        insets: SurfaceField::Value([-3., -4., -5., -6.]),
                        ..Default::default()
                    },
                    ..Default::default()
                },
                ..Default::default()
            }],
            shadows: vec![SurfaceShadow {
                blur: SurfaceField::Value(2.),
                y: SurfaceField::Value(4.),
                layer: SurfaceLayer {
                    geometry: SurfaceGeometry {
                        insets: SurfaceField::Value([-2.; 4]),
                        ..Default::default()
                    },
                    ..Default::default()
                },
                ..Default::default()
            }],
            ..Default::default()
        };
        assert_eq!(recipe.visual_bounds(rect), Rect::new(12., 25., 116., 97.));
        let quad = QuadStyle {
            decoration: Some(Arc::new(crate::decoration::Decoration {
                surface_recipe: Some(Arc::new(recipe.clone())),
                ..Default::default()
            })),
            ..Default::default()
        };
        assert_eq!(quad.paint_bounds(rect), recipe.visual_bounds(rect));
        recipe.strokes[0].layer.geometry.clips_to_surface = SurfaceField::Value(true);
        recipe.shadows[0].layer.geometry.clips_to_surface = SurfaceField::Value(true);
        assert_eq!(recipe.visual_bounds(rect), rect);
    }
}
