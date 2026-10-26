//! What animations write to: elements (by property), Rust-owned state, and
//! plain script tables (anime.js "object targets").
use super::value::{V, format_color, format_number};
use crate::{
    element::Element,
    node::{Lit, Prop},
    value::Value,
};
use std::rc::Rc;
use zgx::{
    reactive::Signal,
    scene::{Color, NodeKind},
};

/// An animatable property.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(crate) enum Key {
    X,
    Y,
    /// A layout animation's translation, over the node's own (`x`, `y`):
    /// Rust's moves and a script's compose. Scripts do not name it.
    OffsetX,
    OffsetY,
    Opacity,
    Blur,
    /// CSS filter color functions, on the compositor.
    Brightness,
    Contrast,
    Saturate,
    Scale,
    ScaleX,
    ScaleY,
    Rotate,
    Skew,
    SkewX,
    SkewY,
    /// Tilts (CSS `rotateX`, `rotateY`) and the distance they are seen from.
    RotateX,
    RotateY,
    Perspective,
    /// The mask's position (CSS `mask-position`).
    MaskX,
    MaskY,
    /// The clip inset's sides (CSS `clip-path: inset(...)`).
    ClipTop,
    ClipRight,
    ClipBottom,
    ClipLeft,
    /// A shader fill's time (0) or parameter (1 to 16).
    Shader(u8),
    /// A layer shader's time (0) or parameter (1 to 16).
    Layer(u8),
    Style(Prop),
    /// A state's value or a table field.
    Field(Rc<str>),
}

/// What 100% of a length is.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Basis {
    OwnWidth,
    OwnHeight,
    ParentWidth,
    ParentHeight,
}
/// How written values convert to a property's canonical unit.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Unit {
    Length(Basis),
    Angle,
    Factor,
    Color,
    /// State and table values: kept as written, units included.
    Free,
}

impl Key {
    /// anime.js property names (camelCase CSS), and this library's own.
    pub fn for_element(name: &str) -> Option<Key> {
        Some(match name {
            "x" | "translateX" | "translate_x" => Key::X,
            "y" | "translateY" | "translate_y" => Key::Y,
            "opacity" => Key::Opacity,
            "brightness" => Key::Brightness,
            "contrast" => Key::Contrast,
            "saturate" => Key::Saturate,
            "blur" | "filter" => Key::Blur,
            "maskX" | "mask_x" => Key::MaskX,
            "clipTop" | "clip_top" => Key::ClipTop,
            "clipRight" | "clip_right" => Key::ClipRight,
            "clipBottom" | "clip_bottom" => Key::ClipBottom,
            "clipLeft" | "clip_left" => Key::ClipLeft,
            "shaderTime" | "shader_time" => Key::Shader(0),
            name if name.starts_with("shaderParam") => {
                let n: u8 = name["shaderParam".len()..].parse().ok()?;
                if n > 15 {
                    return None;
                }
                Key::Shader(n + 1)
            }
            "layerTime" | "layer_time" => Key::Layer(0),
            name if name.starts_with("layerParam") => {
                let n: u8 = name["layerParam".len()..].parse().ok()?;
                if n > 15 {
                    return None;
                }
                Key::Layer(n + 1)
            }
            "maskY" | "mask_y" => Key::MaskY,
            "scale" => Key::Scale,
            "scaleX" | "scale_x" => Key::ScaleX,
            "scaleY" | "scale_y" => Key::ScaleY,
            "rotate" | "rotateZ" | "rotate_z" | "rotation" => Key::Rotate,
            "skew" => Key::Skew,
            "skewX" | "skew_x" => Key::SkewX,
            "skewY" | "skew_y" => Key::SkewY,
            "rotateX" | "rotate_x" => Key::RotateX,
            "rotateY" | "rotate_y" => Key::RotateY,
            "perspective" => Key::Perspective,
            "backgroundColor" | "background" | "background_color" => Key::Style(Prop::Bg),
            "borderColor" => Key::Style(Prop::BorderColor),
            "borderRadius" | "border_radius" => Key::Style(Prop::Rounded),
            "borderTopLeftRadius" | "rounded_tl" => Key::Style(Prop::RoundedTl),
            "borderTopRightRadius" | "rounded_tr" => Key::Style(Prop::RoundedTr),
            "borderBottomRightRadius" | "rounded_br" => Key::Style(Prop::RoundedBr),
            "borderBottomLeftRadius" | "rounded_bl" => Key::Style(Prop::RoundedBl),
            "borderWidth" | "border_width" => Key::Style(Prop::Border),
            "width" => Key::Style(Prop::W),
            "height" => Key::Style(Prop::H),
            "minWidth" => Key::Style(Prop::MinW),
            "maxWidth" => Key::Style(Prop::MaxW),
            "minHeight" => Key::Style(Prop::MinH),
            "maxHeight" => Key::Style(Prop::MaxH),
            "padding" => Key::Style(Prop::P),
            "paddingTop" => Key::Style(Prop::Pt),
            "paddingRight" => Key::Style(Prop::Pr),
            "paddingBottom" => Key::Style(Prop::Pb),
            "paddingLeft" => Key::Style(Prop::Pl),
            "margin" => Key::Style(Prop::M),
            "marginTop" => Key::Style(Prop::Mt),
            "marginRight" => Key::Style(Prop::Mr),
            "marginBottom" => Key::Style(Prop::Mb),
            "marginLeft" => Key::Style(Prop::Ml),
            "fontSize" | "font_size" => Key::Style(Prop::TextSize),
            "fontWeight" => Key::Style(Prop::Weight),
            name => {
                let prop = Prop::parse(name)?;
                match prop {
                    Prop::X => Key::X,
                    Prop::Y => Key::Y,
                    Prop::Opacity => Key::Opacity,
                    prop if prop.animatable() => Key::Style(prop),
                    _ => return None,
                }
            }
        })
    }
    pub fn unit(&self) -> Unit {
        match self {
            Key::X | Key::OffsetX => Unit::Length(Basis::OwnWidth),
            Key::OffsetY => Unit::Length(Basis::OwnHeight),
            Key::Blur | Key::MaskX => Unit::Length(Basis::OwnWidth),
            Key::MaskY => Unit::Length(Basis::OwnHeight),
            Key::ClipTop | Key::ClipBottom => Unit::Length(Basis::OwnHeight),
            Key::ClipLeft | Key::ClipRight => Unit::Length(Basis::OwnWidth),
            Key::Y => Unit::Length(Basis::OwnHeight),
            Key::Opacity | Key::Scale | Key::ScaleX | Key::ScaleY => Unit::Factor,
            Key::Shader(_) | Key::Layer(_) => Unit::Factor,
            Key::Brightness | Key::Contrast | Key::Saturate => Unit::Factor,
            Key::Rotate | Key::Skew | Key::SkewX | Key::SkewY => Unit::Angle,
            Key::RotateX | Key::RotateY => Unit::Angle,
            Key::Perspective => Unit::Factor,
            Key::Style(prop) => match prop.ty() {
                crate::node::Ty::Color => Unit::Color,
                _ => match prop {
                    Prop::H | Prop::MinH | Prop::MaxH | Prop::Top | Prop::Bottom => {
                        Unit::Length(Basis::ParentHeight)
                    }
                    Prop::Opacity | Prop::Weight => Unit::Factor,
                    _ => Unit::Length(Basis::ParentWidth),
                },
            },
            Key::Field(_) => Unit::Free,
        }
    }
}

thread_local! {
    /// Set when a build resolved a percentage against a size of zero.
    pub(crate) static UNMEASURED: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

#[derive(Clone)]
pub(crate) enum Target {
    Element(Rc<Element>),
    State(Signal<Value>),
    Table(mlua::Table),
}
impl Target {
    /// Identifies the target while it lives.
    pub fn addr(&self) -> usize {
        match self {
            Target::Element(e) => Rc::as_ptr(e) as usize,
            Target::State(s) => s.addr(),
            Target::Table(t) => t.to_pointer() as usize,
        }
    }
    pub fn same(&self, other: &Target) -> bool {
        self.addr() == other.addr()
    }
    pub fn key_for(&self, name: &str) -> Result<Key, String> {
        match self {
            Target::Element(_) => Key::for_element(name)
                .ok_or_else(|| format!("`{name}` is not an animatable property")),
            Target::State(_) => Ok(Key::Field(name.into())),
            Target::Table(_) => Ok(Key::Field(name.into())),
        }
    }
    /// What 100% of `basis` is for this target, in points.
    pub fn basis(&self, basis: Basis) -> f64 {
        let Target::Element(element) = self else {
            return 0.;
        };
        let size = element
            .read(|scene, id| {
                let own = scene.bounds(id);
                let parent = scene.parent(id).map(|p| scene.bounds(p)).unwrap_or(own);
                match basis {
                    Basis::OwnWidth => own.width,
                    Basis::OwnHeight => own.height,
                    Basis::ParentWidth => parent.width,
                    Basis::ParentHeight => parent.height,
                }
            })
            .unwrap_or(0.) as f64;
        if size == 0. {
            UNMEASURED.with(|u| u.set(true));
        }
        size
    }

    /// The current value, in the key's canonical unit.
    pub fn read(&self, key: &Key) -> V {
        match self {
            Target::Element(e) => read_element(e, key),
            Target::State(s) => value_to_v(&s.with_untracked(Clone::clone)),
            Target::Table(t) => match key {
                Key::Field(name) => value_to_v(&t.raw_get::<Value>(&**name).unwrap_or_default()),
                _ => V::Num(0.),
            },
        }
    }

    /// Writes `v`. `unit` is the unit the script wrote values in, kept for
    /// state and table targets.
    pub fn write(&self, key: &Key, v: &V, unit: Option<&str>) -> bool {
        match self {
            Target::Element(e) => {
                write_element(e, key, v);
                true
            }
            Target::State(s) => {
                s.set(v_to_value(v, unit));
                false
            }
            Target::Table(t) => {
                if let Key::Field(name) = key {
                    let _ = match v_to_value(v, unit) {
                        Value::Num(n) => t.raw_set(&**name, n),
                        Value::Str(s) => t.raw_set(&**name, &*s),
                        Value::Bool(b) => t.raw_set(&**name, b),
                        _ => Ok(()),
                    };
                }
                false
            }
        }
    }
}

fn color_to_v(c: Color) -> V {
    V::Color([c.0 as f64, c.1 as f64, c.2 as f64, c.3 as f64 / 255.])
}
pub(crate) fn v_to_color(c: [f64; 4]) -> Color {
    let byte = |v: f64| v.round().clamp(0., 255.) as u8;
    Color(byte(c[0]), byte(c[1]), byte(c[2]), byte(c[3] * 255.))
}

fn read_element(e: &Element, key: &Key) -> V {
    let local = || e.local();
    match key {
        Key::X => V::Num(e.translate().0 as f64),
        Key::Y => V::Num(e.translate().1 as f64),
        Key::OffsetX => V::Num(e.offset().0 as f64),
        Key::OffsetY => V::Num(e.offset().1 as f64),
        Key::Opacity => V::Num(e.opacity() as f64),
        Key::Blur => V::Num(e.blur() as f64),
        Key::Brightness => V::Num(e.color()[0] as f64),
        Key::Contrast => V::Num(e.color()[1] as f64),
        Key::Saturate => V::Num(e.color()[2] as f64),
        Key::MaskX => V::Num(e.mask_offset().0 as f64),
        Key::MaskY => V::Num(e.mask_offset().1 as f64),
        Key::ClipTop => V::Num(e.clip_inset()[0] as f64),
        Key::ClipRight => V::Num(e.clip_inset()[1] as f64),
        Key::ClipBottom => V::Num(e.clip_inset()[2] as f64),
        Key::ClipLeft => V::Num(e.clip_inset()[3] as f64),
        Key::Shader(slot) => {
            let u = e.shader_uniforms();
            V::Num(match *slot {
                0 => u.time,
                n => u.params[(n as usize - 1) / 4][(n as usize - 1) % 4],
            } as f64)
        }
        Key::Layer(slot) => {
            let u = e.layer_uniforms();
            V::Num(match *slot {
                0 => u.time,
                n => u.params[(n as usize - 1) / 4][(n as usize - 1) % 4],
            } as f64)
        }
        Key::Scale | Key::ScaleX => V::Num(local().scale.0 as f64),
        Key::ScaleY => V::Num(local().scale.1 as f64),
        Key::Rotate => V::Num(local().rotate as f64),
        Key::Skew | Key::SkewX => V::Num(local().skew.0 as f64),
        Key::SkewY => V::Num(local().skew.1 as f64),
        Key::RotateX => V::Num(local().rotate_x as f64),
        Key::RotateY => V::Num(local().rotate_y as f64),
        Key::Perspective => V::Num(local().perspective as f64),
        Key::Style(prop) => {
            if let Some((_, lit)) = e.animated.borrow().styles.iter().find(|(p, _)| p == prop) {
                return match lit {
                    Lit::Num(n) => V::Num(*n as f64),
                    Lit::Color(c) => color_to_v(*c),
                    other => V::Discrete(lit_to_value(other)),
                };
            }
            if let (
                Prop::RoundedTl | Prop::RoundedTr | Prop::RoundedBr | Prop::RoundedBl,
                Some(c),
            ) = (prop, e.base_corners())
            {
                // The node's literal radii: the scene may not have them yet.
                return V::Num(match prop {
                    Prop::RoundedTl => c.top_left,
                    Prop::RoundedTr => c.top_right,
                    Prop::RoundedBr => c.bottom_right,
                    _ => c.bottom_left,
                } as f64);
            }
            e.read(|scene, id| {
                let quad = match scene.kind(id) {
                    NodeKind::Panel { quad, .. } | NodeKind::Quad(quad) => Some(quad.clone()),
                    _ => None,
                };
                let bounds = scene.bounds(id);
                let style = scene.style(id);
                match prop {
                    Prop::Bg => color_to_v(match scene.kind(id) {
                        NodeKind::Rect(c) => *c,
                        _ => quad.map_or(Color(0, 0, 0, 0), |q| q.fill),
                    }),
                    Prop::TextColor => color_to_v(match scene.kind(id) {
                        NodeKind::Text { color, .. } => *color,
                        _ => Color(255, 255, 255, 255),
                    }),
                    Prop::BorderColor => {
                        color_to_v(quad.map_or(Color(0, 0, 0, 0), |q| q.border_color))
                    }
                    Prop::Rounded => V::Num(quad.map_or(0., |q| q.radius) as f64),
                    Prop::RoundedTl | Prop::RoundedTr | Prop::RoundedBr | Prop::RoundedBl => {
                        let c = quad.map_or(zgx::decoration::Corners::all(0.), |q| {
                            q.decoration
                                .as_ref()
                                .and_then(|d| d.corners)
                                .unwrap_or(zgx::decoration::Corners::all(q.radius))
                        });
                        V::Num(match prop {
                            Prop::RoundedTl => c.top_left,
                            Prop::RoundedTr => c.top_right,
                            Prop::RoundedBr => c.bottom_right,
                            _ => c.bottom_left,
                        } as f64)
                    }
                    Prop::Border => V::Num(quad.map_or(0., |q| q.border_width) as f64),
                    Prop::W => V::Num(bounds.width as f64),
                    Prop::H => V::Num(bounds.height as f64),
                    Prop::TextSize => V::Num(match scene.kind(id) {
                        NodeKind::Text { font_size, .. } => *font_size as f64,
                        _ => 16.,
                    }),
                    Prop::Gap => V::Num(style.gap as f64),
                    Prop::P | Prop::Px | Prop::Py | Prop::Pt | Prop::Pr | Prop::Pb | Prop::Pl => {
                        let edges = style.padding_edges.unwrap_or(zgx::scene::Insets {
                            top: style.padding,
                            right: style.padding,
                            bottom: style.padding,
                            left: style.padding,
                        });
                        V::Num(match prop {
                            Prop::Pt | Prop::Py => edges.top,
                            Prop::Pr => edges.right,
                            Prop::Pb => edges.bottom,
                            _ => edges.left,
                        } as f64)
                    }
                    Prop::MinW => V::Num(style.min_width.unwrap_or(0.) as f64),
                    Prop::MaxW => V::Num(style.max_width.unwrap_or(f32::MAX) as f64),
                    Prop::MinH => V::Num(style.min_height.unwrap_or(0.) as f64),
                    Prop::MaxH => V::Num(style.max_height.unwrap_or(f32::MAX) as f64),
                    Prop::Blur => V::Num(scene.effects(id).filter_blur as f64),
                    _ => V::Num(0.),
                }
            })
            .unwrap_or(V::Num(0.))
        }
        Key::Field(_) => V::Num(0.),
    }
}

fn write_element(e: &Element, key: &Key, v: &V) {
    let n = || v.num().unwrap_or(0.) as f32;
    let local = || e.local();
    match key {
        Key::X => e.set_translate((n(), e.translate().1)),
        Key::Y => e.set_translate((e.translate().0, n())),
        Key::OffsetX => e.set_offset((n(), e.offset().1)),
        Key::OffsetY => e.set_offset((e.offset().0, n())),
        Key::Opacity => e.set_opacity(n().clamp(0., 1.)),
        Key::Blur => e.set_blur(n().max(0.)),
        Key::Brightness => e.set_color_part(0, n().max(0.)),
        Key::Contrast => e.set_color_part(1, n().max(0.)),
        Key::Saturate => e.set_color_part(2, n().max(0.)),
        Key::MaskX => e.set_mask_offset((n(), e.mask_offset().1)),
        Key::MaskY => e.set_mask_offset((e.mask_offset().0, n())),
        Key::ClipTop => e.set_clip_side(0, n()),
        Key::ClipRight => e.set_clip_side(1, n()),
        Key::ClipBottom => e.set_clip_side(2, n()),
        Key::ClipLeft => e.set_clip_side(3, n()),
        Key::Shader(slot) => e.set_shader_slot(*slot as usize, n()),
        Key::Layer(slot) => e.set_layer_slot(*slot as usize, n()),
        Key::Scale => e.set_local(crate::transform::LocalTransform {
            scale: (n(), n()),
            ..local()
        }),
        Key::ScaleX => {
            let l = local();
            e.set_local(crate::transform::LocalTransform {
                scale: (n(), l.scale.1),
                ..l
            })
        }
        Key::ScaleY => {
            let l = local();
            e.set_local(crate::transform::LocalTransform {
                scale: (l.scale.0, n()),
                ..l
            })
        }
        Key::Rotate => e.set_local(crate::transform::LocalTransform {
            rotate: n(),
            ..local()
        }),
        Key::RotateX => e.set_local(crate::transform::LocalTransform {
            rotate_x: n(),
            ..local()
        }),
        Key::RotateY => e.set_local(crate::transform::LocalTransform {
            rotate_y: n(),
            ..local()
        }),
        Key::Perspective => e.set_local(crate::transform::LocalTransform {
            perspective: n().max(0.),
            ..local()
        }),
        Key::Skew => e.set_local(crate::transform::LocalTransform {
            skew: (n(), n()),
            ..local()
        }),
        Key::SkewX => {
            let l = local();
            e.set_local(crate::transform::LocalTransform {
                skew: (n(), l.skew.1),
                ..l
            })
        }
        Key::SkewY => {
            let l = local();
            e.set_local(crate::transform::LocalTransform {
                skew: (l.skew.0, n()),
                ..l
            })
        }
        Key::Style(prop) => {
            let lit = match v {
                V::Num(x) => Some(match prop.ty() {
                    crate::node::Ty::Pair => Lit::Pair(*x as f32, *x as f32),
                    _ => Lit::Num(*x as f32),
                }),
                V::Color(c) => Some(Lit::Color(v_to_color(*c))),
                V::Discrete(value) => crate::node::lit(value, prop.ty()),
                V::Complex { .. } => None,
            };
            if let Some(lit) = lit {
                e.set_style(*prop, lit);
            }
        }
        Key::Field(_) => {}
    }
}

fn lit_to_value(lit: &Lit) -> Value {
    match lit {
        Lit::Num(n) => Value::Num(*n as f64),
        Lit::Bool(b) => Value::Bool(*b),
        Lit::Str(s) => Value::Str(s.clone()),
        Lit::Color(c) => Value::Str(format_color(color_to_array(*c)).into()),
        Lit::Pair(a, b) => Value::List(vec![Value::Num(*a as f64), Value::Num(*b as f64)].into()),
        Lit::Len(zgx::layout::Length::Px(n)) => Value::Num(*n as f64),
        _ => Value::Nil,
    }
}
fn color_to_array(c: Color) -> [f64; 4] {
    [c.0 as f64, c.1 as f64, c.2 as f64, c.3 as f64 / 255.]
}

/// A state or table value as an animatable value.
pub(crate) fn value_to_v(value: &Value) -> V {
    match value {
        Value::Num(n) => V::Num(*n),
        Value::Bool(b) => V::Num(*b as u8 as f64),
        Value::Str(s) => match super::value::parse_str(s).kind {
            super::value::RawKind::Num(n, _) => V::Num(n),
            super::value::RawKind::Color(c) => V::Color(c),
            super::value::RawKind::Complex { pieces, numbers } => V::Complex { pieces, numbers },
            super::value::RawKind::Other(v) => V::Discrete(v),
        },
        other => V::Discrete(other.clone()),
    }
}
fn v_to_value(v: &V, unit: Option<&str>) -> Value {
    match v {
        V::Num(n) => match unit {
            Some(unit) => Value::Str(format!("{}{unit}", format_number(*n)).into()),
            None => Value::Num(*n),
        },
        V::Color(c) => Value::Str(format_color(*c).into()),
        V::Complex { pieces, numbers } => Value::Str(super::value::compose(pieces, numbers).into()),
        V::Discrete(value) => value.clone(),
    }
}
