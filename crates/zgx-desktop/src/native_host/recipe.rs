//! Effective native decoration geometry. Sparse source data remains unchanged.
use zgx::{
    scene::Rect,
    style::StrokeAlign,
    surface::{SurfaceColor, SurfaceField, SurfaceGeometry, SurfaceRadius, SurfaceStroke},
};

pub(super) fn value<T: Copy>(field: &SurfaceField<T>, fallback: T) -> T {
    field.value().copied().unwrap_or(fallback)
}

pub(super) fn color(color: &SurfaceColor, dark: bool) -> [f64; 4] {
    let source = if dark {
        color.source_dark.unwrap_or(color.source)
    } else {
        color.source
    };
    let rgba = if dark {
        color.source_dark_rgba.or(color.source_rgba)
    } else {
        color.source_rgba
    };
    let source = rgba.unwrap_or([
        f64::from(source.0) / 255.,
        f64::from(source.1) / 255.,
        f64::from(source.2) / 255.,
        f64::from(source.3) / 255.,
    ]);
    if let Some(resolved) = color.resolved {
        [
            f64::from(resolved.0) / 255.,
            f64::from(resolved.1) / 255.,
            f64::from(resolved.2) / 255.,
            source[3],
        ]
    } else {
        source
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct Geometry {
    pub rect: Rect,
    pub radius: f32,
    pub opacity: f32,
    pub clips: bool,
}

pub(super) fn geometry(
    spec: &SurfaceGeometry,
    surface: Rect,
    radius: f32,
    scale: f32,
    clips: bool,
) -> Geometry {
    let [top, right, bottom, left] = value(&spec.insets, [0.; 4]).map(|v| v * scale);
    let rect = Rect {
        x: surface.x + left,
        y: surface.y + top,
        width: (surface.width - left - right).max(0.),
        height: (surface.height - top - bottom).max(0.),
    };
    let radius = match value(&spec.radius, SurfaceRadius::Concentric(0.)) {
        SurfaceRadius::Fixed(r) => r * scale,
        SurfaceRadius::MatchSurface => radius,
        SurfaceRadius::Concentric(_) => {
            radius
                .max(0.)
                .min(surface.width / 2.)
                .min(surface.height / 2.)
                - top.min(right).min(bottom).min(left)
        }
    }
    .max(0.)
    .min(rect.width / 2.)
    .min(rect.height / 2.);
    Geometry {
        rect,
        radius,
        opacity: value(&spec.opacity, 1.).clamp(0., 1.),
        clips: value(&spec.clips_to_surface, clips),
    }
}

pub(super) fn stroke(
    spec: &SurfaceStroke,
    surface: Rect,
    radius: f32,
    scale: f32,
) -> (Geometry, f32) {
    let mut geometry = geometry(&spec.layer.geometry, surface, radius, scale, false);
    let width = (value(&spec.width, 0.5) * scale).max(0.);
    let offset = match value(&spec.alignment, StrokeAlign::Inside) {
        StrokeAlign::Inside => width / 2.,
        StrokeAlign::Center => 0.,
        StrokeAlign::Outside => -width / 2.,
    };
    geometry.rect.x += offset;
    geometry.rect.y += offset;
    geometry.rect.width = (geometry.rect.width - offset * 2.).max(0.);
    geometry.rect.height = (geometry.rect.height - offset * 2.).max(0.);
    geometry.radius = (geometry.radius - offset)
        .max(0.)
        .min(geometry.rect.width / 2.)
        .min(geometry.rect.height / 2.);
    (geometry, width)
}

#[cfg(test)]
mod tests {
    use super::*;
    use zgx::{scene::Color, surface::SurfaceBlend};

    #[test]
    fn semantic_rgb_preserves_source_alpha_and_separate_geometry_opacity() {
        let source = SurfaceColor {
            source: Color(1, 2, 3, 128),
            source_rgba: Some([0.1, 0.2, 0.3, 0.5]),
            resolved: Some(Color(20, 40, 60, 255)),
            ..Default::default()
        };
        assert_eq!(
            color(&source, false),
            [20. / 255., 40. / 255., 60. / 255., 0.5]
        );
        let geometry = SurfaceGeometry {
            opacity: SurfaceField::Value(0.2),
            ..Default::default()
        };
        let resolved = super::geometry(
            &geometry,
            Rect {
                x: 0.,
                y: 0.,
                width: 100.,
                height: 80.,
            },
            10.,
            1.,
            false,
        );
        assert_eq!(resolved.opacity, 0.2);
        assert_eq!(
            color(&source, false)[3] * f64::from(resolved.opacity),
            0.5 * f64::from(0.2_f32)
        );
    }

    #[test]
    fn strokes_use_source_defaults_and_explicit_outside_geometry() {
        let surface = Rect {
            x: 10.,
            y: 20.,
            width: 100.,
            height: 80.,
        };
        let mut spec = SurfaceStroke::default();
        spec.layer.blend = SurfaceField::Value(SurfaceBlend::PlusLighter);
        let (inside, width) = stroke(&spec, surface, 10., 1.);
        assert_eq!(width, 0.5);
        assert_eq!(
            inside.rect,
            Rect {
                x: 10.25,
                y: 20.25,
                width: 99.5,
                height: 79.5
            }
        );
        assert_eq!(inside.radius, 9.75);
        assert!(!inside.clips);
        spec.width = SurfaceField::Value(1.);
        spec.alignment = SurfaceField::Value(StrokeAlign::Outside);
        let (outside, _) = stroke(&spec, surface, 10., 2.);
        assert_eq!(
            outside.rect,
            Rect {
                x: 9.,
                y: 19.,
                width: 102.,
                height: 82.
            }
        );
        assert_eq!(outside.radius, 11.);
    }

    #[test]
    fn radius_modes_follow_recovered_raw_and_effective_parent_rules() {
        let surface = Rect::new(0., 0., 100., 80.);
        let mut spec = SurfaceGeometry {
            insets: SurfaceField::Value([-10.; 4]),
            radius: SurfaceField::Value(SurfaceRadius::Concentric(999.)),
            ..Default::default()
        };
        assert_eq!(geometry(&spec, surface, 60., 1., false).radius, 50.);
        spec.radius = SurfaceField::Value(SurfaceRadius::MatchSurface);
        assert_eq!(geometry(&spec, surface, 60., 1., false).radius, 50.);
        spec.insets = SurfaceField::Value([-30.; 4]);
        assert_eq!(geometry(&spec, surface, 60., 1., false).radius, 60.);
        spec.radius = SurfaceField::Value(SurfaceRadius::Concentric(999.));
        assert_eq!(geometry(&spec, surface, 60., 1., false).radius, 70.);
    }
}
