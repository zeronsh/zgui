//! Detailed panel paint lowered into retained vector content.
use crate::GpuError;
use zgx::{
    canvas::*,
    decoration::{Background, BorderStyle, Corners},
    scene::{Insets, QuadStyle, Rect},
};
fn radii(c: Corners, w: f32, h: f32) -> [(f32, f32); 4] {
    let r = [
        c.top_left.max(0.),
        c.top_right.max(0.),
        c.bottom_right.max(0.),
        c.bottom_left.max(0.),
    ];
    let factor = [
        (w, r[0] + r[1]),
        (w, r[3] + r[2]),
        (h, r[0] + r[3]),
        (h, r[1] + r[2]),
    ]
    .into_iter()
    .fold(1_f32, |scale, (size, sum)| {
        if sum > 0. {
            scale.min(size / sum)
        } else {
            scale
        }
    })
    .clamp(0., 1.);
    r.map(|r| (r * factor, r * factor))
}
fn rounded(rect: Rect, r: [(f32, f32); 4]) -> Path {
    let (x, y, right, bottom) = (rect.x, rect.y, rect.x + rect.width, rect.y + rect.height);
    let k = 0.552_284_8;
    let mut p = Path::builder();
    p.move_to(x + r[0].0, y)
        .line_to(right - r[1].0, y)
        .cubic_to(
            Point::new(right - r[1].0 * (1. - k), y),
            Point::new(right, y + r[1].1 * (1. - k)),
            Point::new(right, y + r[1].1),
        )
        .line_to(right, bottom - r[2].1)
        .cubic_to(
            Point::new(right, bottom - r[2].1 * (1. - k)),
            Point::new(right - r[2].0 * (1. - k), bottom),
            Point::new(right - r[2].0, bottom),
        )
        .line_to(x + r[3].0, bottom)
        .cubic_to(
            Point::new(x + r[3].0 * (1. - k), bottom),
            Point::new(x, bottom - r[3].1 * (1. - k)),
            Point::new(x, bottom - r[3].1),
        )
        .line_to(x, y + r[0].1)
        .cubic_to(
            Point::new(x, y + r[0].1 * (1. - k)),
            Point::new(x + r[0].0 * (1. - k), y),
            Point::new(x + r[0].0, y),
        )
        .close();
    p.build().expect("finite normalized corners")
}
/// A box's detailed decoration (`style.decoration`) as canvas commands, at
/// `width` by `height`: what rasterizes when it can't draw analytically.
pub fn canvas(style: &QuadStyle, width: f32, height: f32) -> Result<Canvas, GpuError> {
    let Some(detail) = &style.decoration else {
        return Err(GpuError("missing detailed decoration".into()));
    };
    let corners = detail.corners.unwrap_or(Corners::all(style.radius));
    if ![
        width,
        height,
        corners.top_left,
        corners.top_right,
        corners.bottom_right,
        corners.bottom_left,
    ]
    .iter()
    .all(|v| v.is_finite())
    {
        return Err(GpuError("nonfinite decoration dimensions".into()));
    }
    let radius = radii(corners, width, height);
    let outer = rounded(Rect::new(0., 0., width, height), radius);
    let mut canvas = Canvas::new();
    let background = match &detail.background {
        None => Brush::Solid(style.fill),
        Some(Background::Brush(brush)) => brush.clone(),
        Some(Background::Linear { angle, stops }) => {
            if !angle.is_finite() {
                return Err(GpuError("nonfinite gradient angle".into()));
            }
            let (sin, cos) = angle.sin_cos();
            let half = (width * cos.abs() + height * sin.abs()) * 0.5;
            Brush::linear(
                Point::new(width * 0.5 - cos * half, height * 0.5 - sin * half),
                Point::new(width * 0.5 + cos * half, height * 0.5 + sin * half),
                stops.clone(),
            )
            .map_err(|e| GpuError(e.into()))?
        }
        // Drawn over the fill by the renderer, not rasterized.
        Some(Background::Image { .. } | Background::Shader(_)) => Brush::Solid(style.fill),
    };
    canvas.fill(outer.clone(), background);
    let borders = detail
        .border_widths
        .unwrap_or(Insets::all(style.border_width));
    if ![borders.left, borders.top, borders.right, borders.bottom]
        .iter()
        .all(|v| v.is_finite())
    {
        return Err(GpuError("nonfinite border width".into()));
    }
    let left = borders.left.clamp(0., width);
    let right = borders.right.clamp(0., width - left);
    let top = borders.top.clamp(0., height);
    let bottom = borders.bottom.clamp(0., height - top);
    let maximum = left.max(right).max(top).max(bottom);
    if maximum > 0. && style.border_color.3 > 0 {
        let mut ring = Path::builder();
        ring.append(&outer);
        let inner = Rect::new(
            left,
            top,
            (width - left - right).max(0.),
            (height - top - bottom).max(0.),
        );
        if inner.width > 0. && inner.height > 0. {
            ring.append(&rounded(
                inner,
                [
                    ((radius[0].0 - left).max(0.), (radius[0].1 - top).max(0.)),
                    ((radius[1].0 - right).max(0.), (radius[1].1 - top).max(0.)),
                    (
                        (radius[2].0 - right).max(0.),
                        (radius[2].1 - bottom).max(0.),
                    ),
                    ((radius[3].0 - left).max(0.), (radius[3].1 - bottom).max(0.)),
                ],
            ));
        }
        let ring = ring.build().unwrap();
        let frame = (Rect::new(0., 0., width, height), radius);
        styled_ring(
            &mut canvas,
            ring,
            frame,
            maximum,
            detail.border_style,
            style.border_color,
        )?;
    }
    Ok(canvas)
}
/// A ring inside `outer` (rounded by its radii), `width` wide at most, drawn
/// solid, dashed (its ring stroked along `outer`, cut to the ring) or dotted
/// (`BorderStyle::DOTTED`: round dots `width` across along its middle,
/// spaced evenly all round), in `color`. `ring` is its area.
fn styled_ring(
    canvas: &mut Canvas,
    ring: Path,
    (outer, radius): (Rect, [(f32, f32); 4]),
    width: f32,
    style: BorderStyle,
    color: zgx::scene::Color,
) -> Result<(), GpuError> {
    let BorderStyle::Dashed { length, gap } = style else {
        canvas.fill_rule(ring, color, FillRule::EvenOdd);
        return Ok(());
    };
    if !length.is_finite()
        || !gap.is_finite()
        || length < 0.
        || gap < 0.
        || length > 0. && gap <= 0.
    {
        return Err(GpuError("invalid dashed border".into()));
    }
    if length > 0. {
        canvas
            .clip(ring, FillRule::EvenOdd)
            .stroke(
                rounded(outer, radius),
                color,
                Stroke {
                    width: width * 2.,
                    dash: vec![length, gap],
                    ..Default::default()
                },
            )
            .restore();
        return Ok(());
    }
    // Dots on its middle line: the box inset by half the width, its corners
    // as much less round. As many as fit at their pitch, then spread evenly.
    let half = width / 2.;
    let middle = Rect::new(
        outer.x + half,
        outer.y + half,
        (outer.width - width).max(0.),
        (outer.height - width).max(0.),
    );
    let corners = radius.map(|(x, y)| ((x - half).max(0.), (y - half).max(0.)));
    let around = perimeter(middle, corners);
    let pitch = width + if gap > 0. { gap } else { width };
    if around <= 0. || pitch <= 0. {
        return Ok(());
    }
    let dots = (around / pitch).round().max(1.);
    canvas.stroke(
        rounded(middle, corners),
        color,
        Stroke {
            width,
            cap: LineCap::Round,
            dash: vec![0., around / dots],
            ..Default::default()
        },
    );
    Ok(())
}
/// How far round `rect` rounded by `radius` it is: its sides less the
/// corners, and each corner a quarter of an ellipse (Ramanujan's).
fn perimeter(rect: Rect, radius: [(f32, f32); 4]) -> f32 {
    let sides = 2. * (rect.width + rect.height) - radius.iter().map(|(x, y)| x + y).sum::<f32>();
    let quarter = |(a, b): (f32, f32)| {
        std::f32::consts::FRAC_PI_4 * (3. * (a + b) - ((3. * a + b) * (a + 3. * b)).sqrt())
    };
    sides + radius.into_iter().map(quarter).sum::<f32>()
}
/// A dashed or dotted outline (`Scene::outline_style`) of a box of `style`
/// `width` by `height`, on a canvas the size of the outline's outer box
/// (`Outline::outer`), at its origin: a ring of the outline's width, its
/// corners the box's radii grown by its offset and width (square ones stay
/// square), as `outline_quad` draws a solid one.
pub fn outline_canvas(
    style: &QuadStyle,
    outline: &zgx::scene::Outline,
    outline_style: BorderStyle,
    width: f32,
    height: f32,
) -> Result<Canvas, GpuError> {
    let grown = outline.offset + outline.width;
    let outer = Rect::new(0., 0., width + 2. * grown, height + 2. * grown);
    if !(outer.width.is_finite() && outer.height.is_finite())
        || outer.width <= 0.
        || outer.height <= 0.
    {
        return Err(GpuError("invalid outline".into()));
    }
    let corners = shadow_corners(style, width, height, 0.).map(|radius| {
        let radius = if radius > 0. {
            (radius + grown).max(0.)
        } else {
            0.
        };
        (radius, radius)
    });
    let w = outline.width.min(outer.width / 2.).min(outer.height / 2.);
    let inner = Rect::new(w, w, outer.width - 2. * w, outer.height - 2. * w);
    let mut ring = Path::builder();
    ring.append(&rounded(outer, corners));
    if inner.width > 0. && inner.height > 0. {
        ring.append(&rounded(
            inner,
            corners.map(|(x, y)| ((x - w).max(0.), (y - w).max(0.))),
        ));
    }
    let mut canvas = Canvas::new();
    styled_ring(
        &mut canvas,
        ring.build().unwrap(),
        (outer, corners),
        w,
        outline_style,
        outline.color,
    )?;
    Ok(canvas)
}
/// How much of a box shadow covers a point at signed `distance` from its
/// box (negative inside): the box blurred by a Gaussian of `sigma`
/// (`BoxShadow::blur_radius`, half the CSS blur), 0.5·erfc(d / (sigma·√2)),
/// so half at the edge. `draw.wgsl` computes the same.
pub fn shadow_coverage(distance: f32, sigma: f32) -> f32 {
    if sigma <= 0.001 {
        return if distance <= 0. { 1. } else { 0. };
    }
    let x = distance / (sigma * std::f32::consts::SQRT_2);
    let x2 = x * x;
    // Winitzki's erf, within about 1e-4.
    let erf =
        x.signum() * (1. - (-x2 * (1.273_239_5 + 0.147 * x2) / (1. + 0.147 * x2)).exp()).sqrt();
    0.5 - 0.5 * erf
}

/// Corner radius used by analytic outer shadows, with the same per-corner policy.
pub fn shadow_corners(style: &QuadStyle, width: f32, height: f32, spread: f32) -> [f32; 4] {
    let c = style
        .decoration
        .as_ref()
        .and_then(|d| d.corners)
        .unwrap_or(Corners::all(style.radius));
    radii(
        Corners {
            top_left: c.top_left + spread,
            top_right: c.top_right + spread,
            bottom_right: c.bottom_right + spread,
            bottom_left: c.bottom_left + spread,
        },
        width,
        height,
    )
    .map(|r| r.0)
}

#[cfg(test)]
mod shadow_tests {
    use super::shadow_coverage;
    #[test]
    fn shadow_coverage_is_a_blurred_edge() {
        // Half at the box's edge, as a blurred box is (not fully opaque up
        // to it, which drew a hard, dark band).
        assert!((shadow_coverage(0., 12.) - 0.5).abs() < 1e-3);
        // One sigma out and in: erfc(1/√2)/2 ≈ 0.159.
        assert!((shadow_coverage(12., 12.) - 0.1587).abs() < 2e-3);
        assert!((shadow_coverage(-12., 12.) - 0.8413).abs() < 2e-3);
        // Gone by three sigmas, falling all the way.
        assert!(shadow_coverage(36., 12.) < 0.002);
        let mut last = 1.;
        for d in -40..40 {
            let c = shadow_coverage(d as f32, 12.);
            assert!(c <= last + 1e-6);
            last = c;
        }
        // No blur: a hard edge.
        assert_eq!(shadow_coverage(0.5, 0.), 0.);
        assert_eq!(shadow_coverage(-0.5, 0.), 1.);
    }
}
