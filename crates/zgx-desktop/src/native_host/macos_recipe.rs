//! AppKit planes for typed surface recipes, independent of Gaussian blur.
use super::*;
use crate::native_host::recipe::{self, value};
use zgx::surface::{
    SurfaceBlend, SurfaceColor, SurfaceField, SurfaceLayer, SurfaceLevel, SurfaceRecipe,
    SurfaceShadow,
};

pub(super) struct RecipePlanes {
    pub above: Retained<BackdropView>,
    shadow: Retained<BackdropView>,
    backdrop: Retained<BackdropView>,
    fill: Retained<BackdropView>,
    edges: Retained<BackdropView>,
    glass: Option<NativeGlass>,
}

#[derive(Clone, Copy)]
pub(super) struct Frame {
    pub size: NSSize,
    pub surface: Rect,
    pub radius: f32,
    pub scale: f32,
    pub retina: f64,
    pub dark: bool,
}

pub(super) fn glass_enabled(recipe: &SurfaceRecipe) -> bool {
    recipe
        .glass
        .value()
        .is_some_and(|glass| value(&glass.enabled, false))
}

impl RecipePlanes {
    pub fn new(host: &Surfaces, below: &BackdropView, enabled: bool) -> Self {
        let view = || host.backdrop_view(BackdropMaterial::Liquid);
        let shadow = view();
        let backdrop = view();
        let fill = view();
        let edges = view();
        let above = view();
        below.addSubview(&shadow);
        let glass = enabled.then(|| host.native_glass());
        if let Some(glass) = &glass {
            // Keep an empty, retained native content view. Typed paint is
            // owned by the separate recipe planes rather than this legacy fill.
            below.addSubview(&glass.container);
        }
        below.addSubview(&backdrop);
        below.addSubview(&fill);
        below.addSubview(&edges);
        Self {
            above,
            shadow,
            backdrop,
            fill,
            edges,
            glass,
        }
    }

    pub fn sync(&self, source: &SurfaceRecipe, frame: Frame) {
        let Frame {
            size,
            surface,
            radius,
            scale,
            retina,
            dark,
        } = frame;
        let bounds = NSRect::new(NSPoint::ZERO, size);
        for plane in [
            &self.shadow,
            &self.backdrop,
            &self.fill,
            &self.edges,
            &self.above,
        ] {
            plane.setFrame(bounds);
            if let Some(layer) = plane.layer() {
                // These planes contain only our decoration layers.
                unsafe { layer.setSublayers(None) };
                layer.setContentsScale(retina);
            }
        }
        if let Some(glass) = &self.glass {
            let spec = source.glass.value().expect("material enabled by source");
            let geometry = recipe::geometry(&spec.geometry, surface, radius, scale, true);
            glass.container.setFrame(ns(geometry.rect));
            glass
                .effect
                .setFrame(NSRect::new(NSPoint::ZERO, glass.container.bounds().size));
            glass.fill.setFrame(glass.effect.bounds());
            if let Some(layer) = glass.container.layer() {
                rounded_layer(&layer, geometry.radius);
                layer.setMasksToBounds(true);
                layer.setOpacity(geometry.opacity);
            }
            if glass.liquid {
                let tint = if value(&spec.uses_tint, false) {
                    spec.tint.value().map(|color| native_color(color, dark))
                } else {
                    None
                };
                unsafe {
                    let _: () = msg_send![&*glass.effect, setStyle: if spec.style.value().is_some_and(|style| style.as_ref() == "clear") { 1_isize } else { 0_isize }];
                    let _: () = msg_send![&*glass.effect, setTintColor: tint.as_deref()];
                    let _: () =
                        msg_send![&*glass.effect, setCornerRadius: f64::from(geometry.radius)];
                }
            }
        }
        for shadow in &source.shadows {
            self.shadow(shadow, frame, false);
        }
        if let Some(layer) = source.backdrop.value() {
            paint(&self.backdrop, layer, frame);
        }
        for layer in [&source.fill, &source.fill2]
            .into_iter()
            .filter_map(SurfaceField::value)
        {
            paint(&self.fill, layer, frame);
        }
        if let Some(shadow) = source.inner_shadow.value() {
            self.shadow(shadow, frame, true);
        }
        for stroke in &source.strokes {
            if !value(&stroke.layer.enabled, true) {
                continue;
            }
            let Some(color) = stroke.layer.color.value() else {
                continue;
            };
            let (geometry, width) = recipe::stroke(stroke, surface, radius, scale);
            if width <= 0. {
                continue;
            }
            let target =
                if value(&stroke.level, SurfaceLevel::AboveContent) == SurfaceLevel::BelowContent {
                    &self.edges
                } else {
                    &self.above
                };
            let Some(parent) = target.layer() else {
                continue;
            };
            let layer = stroke_layer(
                geometry,
                width,
                size.height as f32,
                top_down(&parent),
                retina,
            );
            layer.setBorderColor(Some(&native_color(color, dark).CGColor()));
            layer.setOpacity(geometry.opacity);
            blend(&layer, value(&stroke.layer.blend, SurfaceBlend::Normal));
            attach(&parent, &layer, geometry.clips, surface, radius);
        }
    }

    fn shadow(&self, spec: &SurfaceShadow, frame: Frame, force_inner: bool) {
        let Frame {
            surface,
            radius,
            scale,
            retina,
            dark,
            ..
        } = frame;
        if !value(&spec.layer.enabled, true) {
            return;
        }
        let Some(parent) = (if force_inner {
            self.edges.layer()
        } else {
            self.shadow.layer()
        }) else {
            return;
        };
        let geometry = recipe::geometry(&spec.layer.geometry, surface, radius, scale, false);
        let black = SurfaceColor {
            source: zgx::scene::Color(0, 0, 0, 255),
            ..Default::default()
        };
        let color = native_color(spec.layer.color.value().unwrap_or(&black), dark);
        let layer = CALayer::new();
        no_actions(&layer);
        layer.setContentsScale(retina);
        let spread = value(&spec.spread, 0.) * scale;
        let rect = Rect::new(
            geometry.rect.x - spread,
            geometry.rect.y - spread,
            (geometry.rect.width + spread * 2.).max(0.),
            (geometry.rect.height + spread * 2.).max(0.),
        );
        layer.setFrame(in_space(
            rect,
            parent.bounds().size.height as f32,
            top_down(&parent),
        ));
        rounded_layer(&layer, (geometry.radius + spread).max(0.));
        let inner = force_inner || value(&spec.inner, false);
        let blur = (value(&spec.blur, 0.) * scale).max(0.);
        let x = value(&spec.x, 0.) * scale;
        let y = value(&spec.y, 0.) * scale;
        let down = top_down(&parent);
        let shape = rounded_path(
            (rect.width, rect.height),
            [geometry.radius + spread; 4],
            down,
        );
        let path = if inner {
            inverse_path(
                (rect.width, rect.height),
                geometry.radius + spread,
                blur * 4. + x.abs() + y.abs() + 1.,
                down,
            )
        } else {
            shape
        };
        layer.setShadowPath(Some(&path));
        layer.setShadowColor(Some(&color.CGColor()));
        layer.setShadowOpacity(1.);
        layer.setShadowRadius(f64::from(blur));
        layer.setShadowOffset(CGSize::new(
            f64::from(x),
            f64::from(if down { y } else { -y }),
        ));
        layer.setOpacity(geometry.opacity);
        if !value(&spec.knockout, true) {
            layer.setBackgroundColor(Some(&color.CGColor()));
        }
        blend(&layer, value(&spec.layer.blend, SurfaceBlend::Normal));
        // Shadow knockout masks only the shadow, never its native content.
        if inner || value(&spec.knockout, true) {
            let mask = CAShapeLayer::new();
            no_actions(&mask);
            mask.setFrame(parent.bounds());
            let mask_path = if inner {
                translated_shape(
                    rect,
                    geometry.radius + spread,
                    parent.bounds().size.height as f32,
                    down,
                )
            } else {
                inverse_shape(
                    rect,
                    geometry.radius + spread,
                    parent.bounds().size.width as f32,
                    parent.bounds().size.height as f32,
                    down,
                )
            };
            mask.setPath(Some(&mask_path));
            mask.setFillRule(unsafe { objc2_quartz_core::kCAFillRuleEvenOdd });
            mask.setFillColor(Some(&NSColor::whiteColor().CGColor()));
            let holder = CALayer::new();
            no_actions(&holder);
            holder.setFrame(parent.bounds());
            unsafe { holder.setMask(Some(&mask)) };
            holder.addSublayer(&layer);
            attach(&parent, &holder, geometry.clips, surface, radius);
        } else {
            attach(&parent, &layer, geometry.clips, surface, radius);
        }
    }
}

fn stroke_layer(
    geometry: recipe::Geometry,
    width: f32,
    height: f32,
    down: bool,
    retina: f64,
) -> Retained<CALayer> {
    let layer = CALayer::new();
    no_actions(&layer);
    layer.setContentsScale(retina);
    // CALayer borders lie inside their frame. Expand from the resolved
    // centerline to preserve all three alignments and the source insets.
    let rect = Rect::new(
        geometry.rect.x - width / 2.,
        geometry.rect.y - width / 2.,
        geometry.rect.width + width,
        geometry.rect.height + width,
    );
    layer.setFrame(in_space(rect, height, down));
    rounded_layer(&layer, geometry.radius + width / 2.);
    layer.setBorderWidth(f64::from(width));
    layer
}

fn native_color(color: &SurfaceColor, dark: bool) -> Retained<NSColor> {
    let [r, g, b, a] = recipe::color(color, dark);
    NSColor::colorWithSRGBRed_green_blue_alpha(r, g, b, a)
}
fn blend(layer: &CALayer, mode: SurfaceBlend) {
    let token = match mode {
        SurfaceBlend::PlusLighter => Some("plusL"),
        SurfaceBlend::PlusDarker => Some("plusD"),
        _ => None,
    };
    unsafe {
        if let Some(token) = token {
            let _: () = msg_send![layer, setCompositingFilter: &*NSString::from_str(token)];
            return;
        }
        let name = match mode {
            SurfaceBlend::Color => Some("CIColorBlendMode"),
            SurfaceBlend::Overlay => Some("CIOverlayBlendMode"),
            SurfaceBlend::Multiply => Some("CIMultiplyBlendMode"),
            SurfaceBlend::Screen => Some("CIScreenBlendMode"),
            _ => None,
        };
        let filter = name.and_then(|name| {
            let class = AnyClass::get(c"CIFilter")?;
            let filter: Option<Retained<AnyObject>> =
                msg_send![class,filterWithName:&*NSString::from_str(name)];
            filter
        });
        let _: () = msg_send![layer,setCompositingFilter:filter.as_deref()];
    }
}
fn paint(plane: &BackdropView, spec: &SurfaceLayer, frame: Frame) {
    let Frame {
        surface,
        radius,
        scale,
        retina,
        dark,
        ..
    } = frame;
    if !value(&spec.enabled, false) {
        return;
    }
    let (Some(parent), Some(color)) = (plane.layer(), spec.color.value()) else {
        return;
    };
    let geometry = recipe::geometry(&spec.geometry, surface, radius, scale, true);
    let layer = CALayer::new();
    no_actions(&layer);
    layer.setContentsScale(retina);
    layer.setFrame(in_space(
        geometry.rect,
        parent.bounds().size.height as f32,
        top_down(&parent),
    ));
    rounded_layer(&layer, geometry.radius);
    layer.setBackgroundColor(Some(&native_color(color, dark).CGColor()));
    layer.setOpacity(geometry.opacity);
    blend(&layer, value(&spec.blend, SurfaceBlend::Normal));
    attach(&parent, &layer, geometry.clips, surface, radius);
}
fn attach(parent: &CALayer, layer: &CALayer, clips: bool, surface: Rect, radius: f32) {
    if !clips {
        parent.addSublayer(layer);
        return;
    }
    let holder = CALayer::new();
    no_actions(&holder);
    holder.setFrame(parent.bounds());
    let mask = CALayer::new();
    no_actions(&mask);
    mask.setFrame(in_space(
        surface,
        parent.bounds().size.height as f32,
        top_down(parent),
    ));
    mask.setBackgroundColor(Some(&NSColor::whiteColor().CGColor()));
    rounded_layer(&mask, radius);
    unsafe { holder.setMask(Some(&mask)) };
    holder.addSublayer(layer);
    parent.addSublayer(&holder);
}
fn translated_shape(rect: Rect, radius: f32, height: f32, down: bool) -> CFRetained<CGMutablePath> {
    let path = rounded_path((rect.width, rect.height), [radius; 4], down);
    let frame = in_space(rect, height, down);
    let transform = objc2_core_foundation::CGAffineTransform {
        a: 1.,
        b: 0.,
        c: 0.,
        d: 1.,
        tx: frame.origin.x,
        ty: frame.origin.y,
    };
    let result = CGMutablePath::new();
    unsafe {
        CGMutablePath::add_path(Some(&result), &transform, Some(&path));
    }
    result
}
fn inverse_shape(
    rect: Rect,
    radius: f32,
    width: f32,
    height: f32,
    down: bool,
) -> CFRetained<CGMutablePath> {
    let result = CGMutablePath::new();
    unsafe {
        CGMutablePath::add_rect(
            Some(&result),
            std::ptr::null(),
            CGRect::new(CGPoint::ZERO, CGSize::new(width as f64, height as f64)),
        );
    }
    let hole = translated_shape(rect, radius, height, down);
    unsafe {
        CGMutablePath::add_path(Some(&result), std::ptr::null(), Some(&hole));
    }
    result
}
fn inverse_path(size: (f32, f32), radius: f32, pad: f32, _down: bool) -> CFRetained<CGMutablePath> {
    let result = CGMutablePath::new();
    unsafe {
        CGMutablePath::add_rect(
            Some(&result),
            std::ptr::null(),
            CGRect::new(
                CGPoint::new(-pad as f64, -pad as f64),
                CGSize::new((size.0 + 2. * pad) as f64, (size.1 + 2. * pad) as f64),
            ),
        );
    }
    // add_rect winds counter-clockwise. Reverse the rounded hole so the
    // shadowPath's non-zero winding rule really subtracts its interior.
    let hole = rounded_path(size, [radius; 4], false);
    unsafe {
        CGMutablePath::add_path(Some(&result), std::ptr::null(), Some(&hole));
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use objc2_core_graphics::CGPath;

    #[test]
    fn recipe_strokes_share_the_native_clip_curve_without_losing_the_border() {
        use zgx::{style::StrokeAlign, surface::SurfaceStroke};
        for scale in [0.75, 1., 1.5, 2.] {
            for (alignment, outset) in [
                (StrokeAlign::Inside, 0.),
                (StrokeAlign::Center, 0.25),
                (StrokeAlign::Outside, 0.5),
            ] {
                let surface = Rect::new(20. * scale, 30. * scale, 120. * scale, 80. * scale);
                let spec = SurfaceStroke {
                    alignment: SurfaceField::Value(alignment),
                    width: SurfaceField::Value(0.5),
                    ..Default::default()
                };
                let (geometry, width) = recipe::stroke(&spec, surface, 16. * scale, scale);
                for down in [true, false] {
                    let border = stroke_layer(geometry, width, 300., down, 2.);
                    let clip = CALayer::new();
                    // Changing a reused continuous layer must reset its curve.
                    unsafe {
                        let _: () =
                            msg_send![&*clip, setCornerCurve: &*NSString::from_str("continuous")];
                    }
                    rounded_layer(&clip, 16. * scale);
                    let curve = |layer: &CALayer| -> Retained<NSString> {
                        unsafe { msg_send![layer, cornerCurve] }
                    };
                    assert_eq!(curve(&border).to_string(), "circular");
                    assert_eq!(curve(&border), curve(&clip));
                    assert_eq!(border.borderWidth(), f64::from(0.5 * scale));
                    let expected = surface.expand(outset * scale);
                    assert_eq!(border.frame(), in_space(expected, 300., down));
                    assert_eq!(border.cornerRadius(), f64::from((16. + outset) * scale));
                    assert_eq!(border.contentsScale(), 2.);
                }
            }
        }
    }

    #[test]
    fn additive_layers_keep_source_tokens_and_reset_to_source_over() {
        let layer = CALayer::new();
        for (mode, token) in [
            (SurfaceBlend::PlusLighter, "plusL"),
            (SurfaceBlend::PlusDarker, "plusD"),
        ] {
            blend(&layer, mode);
            let native = layer.compositingFilter().unwrap();
            assert_eq!(
                native.downcast_ref::<NSString>().unwrap().to_string(),
                token
            );
        }
        blend(&layer, SurfaceBlend::Color);
        assert!(layer.compositingFilter().is_some());
        blend(&layer, SurfaceBlend::Normal);
        assert!(layer.compositingFilter().is_none());
    }

    #[test]
    fn inner_shadow_source_has_a_hole_with_nonzero_winding() {
        let path = inverse_path((100., 80.), 10., 20., true);
        // shadowPath always uses nonzero winding, independently of a
        // CAShapeLayer mask's fillRule.
        let contains = |x, y| unsafe {
            CGPath::contains_point(Some(&path), std::ptr::null(), CGPoint::new(x, y), false)
        };
        assert!(!contains(50., 40.));
        assert!(!contains(5., 40.));
        assert!(contains(-5., 40.));
        assert!(!contains(-25., 40.));
    }
}
