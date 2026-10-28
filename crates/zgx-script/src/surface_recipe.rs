//! Lower script surface recipes into typed core values; no Lua objects escape.
use super::color_of;
use crate::Value;
use std::sync::Arc;
use zgx::{scene::Color, style::StrokeAlign, surface::*};

fn field<T>(
    v: &Value,
    key: &str,
    parse: impl FnOnce(&Value) -> Option<T>,
) -> Option<SurfaceField<T>> {
    Some(match v.field(key) {
        None => SurfaceField::Missing,
        Some(Value::Nil) => SurfaceField::Clear,
        Some(value) => SurfaceField::Value(parse(value)?),
    })
}
fn number(v: &Value) -> Option<f32> {
    v.as_f64().filter(|n| n.is_finite()).map(|n| n as f32)
}
fn boolean(v: &Value) -> Option<bool> {
    if let Value::Bool(v) = v {
        Some(*v)
    } else {
        None
    }
}
fn string(v: &Value) -> Option<Arc<str>> {
    if let Value::Str(v) = v {
        Some(Arc::from(v.as_ref()))
    } else {
        None
    }
}
fn rgba(v: &Value) -> Option<[f64; 4]> {
    let values = [
        v.field("red")?.as_f64()?,
        v.field("green")?.as_f64()?,
        v.field("blue")?.as_f64()?,
        v.field("alpha").and_then(Value::as_f64).unwrap_or(1.),
    ];
    values
        .iter()
        .all(|value| value.is_finite())
        .then_some(values)
}
fn bytes(v: [f64; 4]) -> Color {
    let c = v.map(|n| (n.clamp(0., 1.) * 255.).round() as u8);
    Color(c[0], c[1], c[2], c[3])
}
fn color(v: &Value) -> Option<SurfaceColor> {
    if let Some(source) = color_of(v) {
        return Some(SurfaceColor {
            source,
            ..Default::default()
        });
    }
    let raw = v.field("color");
    let source_rgba = raw.and_then(|raw| raw.field("light")).and_then(rgba);
    let source_dark_rgba = raw.and_then(|raw| raw.field("dark")).and_then(rgba);
    Some(SurfaceColor {
        role: v.field("role").and_then(string),
        source: source_rgba.map(bytes).unwrap_or(Color(0, 0, 0, 0)),
        resolved: v.field("resolvedColor").and_then(color_of),
        source_rgba,
        source_dark: source_dark_rgba.map(bytes),
        source_dark_rgba,
        kind: raw.and_then(|raw| raw.field("kind")).and_then(string),
        color_space: raw
            .and_then(|raw| raw.field("light"))
            .and_then(|c| c.field("colorSpace"))
            .and_then(string),
    })
}
fn insets(v: &Value) -> Option<[f32; 4]> {
    if let Value::List(values) = v {
        if values.len() != 4 {
            return None;
        }
        return Some([
            number(&values[0])?,
            number(&values[1])?,
            number(&values[2])?,
            number(&values[3])?,
        ]);
    }
    let n = |key| v.field(key).and_then(number).unwrap_or(0.);
    Some([n("top"), n("trailing"), n("bottom"), n("leading")])
}
fn radius(v: &Value) -> Option<SurfaceRadius> {
    let value = v.field("value").and_then(number).unwrap_or(0.);
    Some(match v.field("mode") {
        Some(Value::Str(mode)) if mode.as_ref() == "matchSurface" => SurfaceRadius::MatchSurface,
        Some(Value::Str(mode)) if mode.as_ref() == "fixed" => SurfaceRadius::Fixed(value),
        Some(Value::Str(mode)) if mode.as_ref() == "concentric" => SurfaceRadius::Concentric(value),
        _ => return None,
    })
}
fn geometry(v: Option<&Value>) -> Option<SurfaceGeometry> {
    let Some(v) = v else {
        return Some(SurfaceGeometry::default());
    };
    if matches!(v, Value::Nil) {
        return Some(SurfaceGeometry {
            opacity: SurfaceField::Clear,
            insets: SurfaceField::Clear,
            radius: SurfaceField::Clear,
            clips_to_surface: SurfaceField::Clear,
        });
    }
    Some(SurfaceGeometry {
        opacity: field(v, "opacity", number)?,
        insets: field(v, "insets", insets)?,
        radius: field(v, "radius", radius)?,
        clips_to_surface: field(v, "clipsToSurface", boolean)?,
    })
}
fn layer(v: &Value) -> Option<SurfaceLayer> {
    Some(SurfaceLayer {
        color: field(v, "color", color)?,
        blend: field(v, "blendMode", |v| SurfaceBlend::parse(string(v)?.as_ref()))?,
        geometry: geometry(v.field("geometry"))?,
        enabled: field(v, "isEnabled", boolean)?,
    })
}
fn level(v: &Value) -> Option<SurfaceLevel> {
    Some(match string(v)?.as_ref() {
        "belowContent" => SurfaceLevel::BelowContent,
        "aboveContent" => SurfaceLevel::AboveContent,
        _ => return None,
    })
}
fn stroke(v: &Value) -> Option<SurfaceStroke> {
    Some(SurfaceStroke {
        id: v.field("id").and_then(string).unwrap_or_default(),
        layer: layer(v)?,
        width: field(v, "width", number)?,
        alignment: field(v, "alignment", |v| {
            Some(match string(v)?.as_ref() {
                "inside" => StrokeAlign::Inside,
                "center" => StrokeAlign::Center,
                "outside" => StrokeAlign::Outside,
                _ => return None,
            })
        })?,
        level: field(v, "level", level)?,
    })
}
fn shadow(v: &Value) -> Option<SurfaceShadow> {
    Some(SurfaceShadow {
        id: v.field("id").and_then(string).unwrap_or_default(),
        layer: layer(v)?,
        blur: field(v, "blurRadius", number)?,
        x: field(v, "offsetX", number)?,
        y: field(v, "offsetY", number)?,
        spread: field(v, "spread", number)?,
        inner: field(v, "inner", boolean)?,
        knockout: field(v, "knockout", boolean)?,
    })
}
fn glass(v: &Value) -> Option<SurfaceGlass> {
    Some(SurfaceGlass {
        enabled: field(v, "isEnabled", boolean)?,
        tint: field(v, "tint", color)?,
        uses_tint: field(v, "usesTint", boolean)?,
        geometry: geometry(v.field("geometry"))?,
        style: field(v, "style", string)?,
        material: field(v, "windowMaterial", string)?,
    })
}
fn shape(v: &Value) -> Option<SurfaceShape> {
    Some(SurfaceShape {
        corner_radius: field(v, "cornerRadius", number)?,
        corner_curve: field(v, "cornerCurve", string)?,
    })
}
fn array<T>(
    v: Option<&Value>,
    parse: impl Fn(&Value) -> Option<T>,
) -> Option<(Vec<T>, SurfaceField<()>)> {
    match v {
        None => Some((Vec::new(), SurfaceField::Missing)),
        Some(Value::Nil) => Some((Vec::new(), SurfaceField::Clear)),
        Some(Value::List(values)) => Some((
            values.iter().map(parse).collect::<Option<Vec<_>>>()?,
            SurfaceField::Value(()),
        )),
        // Empty Lua tables can arrive as empty maps.
        Some(Value::Map(values)) if values.is_empty() => {
            Some((Vec::new(), SurfaceField::Value(())))
        }
        _ => None,
    }
}
pub(super) fn parse(v: &Value) -> Option<SurfaceRecipe> {
    let (strokes, strokes_state) = array(v.field("strokes"), stroke)?;
    let (shadows, shadows_state) = array(v.field("shadows"), shadow)?;
    Some(SurfaceRecipe {
        glass: field(v, "glass", glass)?,
        backdrop: field(v, "backdrop", layer)?,
        fill: field(v, "fill", layer)?,
        fill2: field(v, "fill2", layer)?,
        strokes,
        shadows,
        strokes_state,
        shadows_state,
        shape: field(v, "shape", shape)?,
        content_opacity: field(v, "contentOpacity", number)?,
        clips_content: field(v, "clipsContent", boolean)?,
        content_insets: field(v, "contentInsets", insets)?,
        inner_shadow: field(v, "innerShadow", shadow)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn source_metadata_alpha_nulls_and_array_order_survive_lowering() {
        let source = Value::from_json(r##"{
            "glass":{"isEnabled":false,"tint":null},
            "backdrop":null,
            "fill":{"isEnabled":true,"blendMode":"plusDarker",
                "color":{"role":"theme:surface","resolvedColor":"#102030",
                    "color":{"kind":"adaptive","light":{"red":0.1,"green":0.2,"blue":0.3,"alpha":0.05,"colorSpace":"extendedSRGB"},
                        "dark":{"red":0.4,"green":0.5,"blue":0.6,"alpha":0.2}}},
                "geometry":{"opacity":0.25,"insets":{"bottom":-1},"radius":{"mode":"concentric","value":0},"clipsToSurface":false}},
            "strokes":[{"id":"first","width":null},{"id":"second","alignment":"outside","level":"belowContent"}],
            "shadows":null
        }"##).unwrap();
        let recipe = parse(&source).unwrap();
        assert!(!recipe.glass_enabled());
        assert!(matches!(recipe.backdrop, SurfaceField::Clear));
        assert!(matches!(
            recipe.glass.value().unwrap().tint,
            SurfaceField::Clear
        ));
        assert!(matches!(recipe.fill2, SurfaceField::Missing));
        assert!(matches!(recipe.shadows_state, SurfaceField::Clear));
        assert_eq!(&*recipe.strokes[0].id, "first");
        assert_eq!(&*recipe.strokes[1].id, "second");
        assert!(matches!(recipe.strokes[0].width, SurfaceField::Clear));
        let fill = recipe.fill.value().unwrap();
        assert_eq!(fill.blend.value(), Some(&SurfaceBlend::PlusDarker));
        assert_eq!(fill.geometry.opacity.value(), Some(&0.25));
        assert_eq!(fill.geometry.insets.value(), Some(&[0., 0., -1., 0.]));
        let color = fill.color.value().unwrap();
        assert_eq!(color.source_rgba, Some([0.1, 0.2, 0.3, 0.05]));
        assert_eq!(color.source_dark_rgba, Some([0.4, 0.5, 0.6, 0.2]));
        assert_eq!(color.resolved, Some(Color(16, 32, 48, 255)));
        assert_eq!(color.role.as_deref(), Some("theme:surface"));
        assert_eq!(color.color_space.as_deref(), Some("extendedSRGB"));
    }

    #[test]
    fn an_empty_array_remains_supplied_and_invalid_blend_is_rejected() {
        let source = Value::from_json(r#"{"strokes":[],"fill":{"blendMode":"invented"}}"#).unwrap();
        assert!(parse(&source).is_none());
        let source = Value::from_json(r#"{"strokes":[]}"#).unwrap();
        let recipe = parse(&source).unwrap();
        assert!(recipe.strokes.is_empty());
        assert!(matches!(recipe.strokes_state, SurfaceField::Value(())));
        assert!(matches!(recipe.shadows_state, SurfaceField::Missing));
    }
}
