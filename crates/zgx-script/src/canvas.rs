//! Drawing for `ui.canvas`: a script's `draw(c, width, height)` records
//! paths and fills into zgx's retained canvas. It runs again only when the
//! canvas's size or the state it reads changes; equal drawings keep their
//! pixels. Colours may name theme tokens (`"$accent"`): the drawing reads
//! them as it reads state, so a theme change draws it again.
use crate::node::parse_color;
use mlua::{Lua, Table, UserData, UserDataMethods};
use std::{cell::RefCell, rc::Rc};
use zgx::theme::ThemeScope;
use zgx::{
    canvas::{
        Brush, Canvas, FillRule, GradientStop, LineCap, LineJoin, Path, PathBuilder, Point, Stroke,
    },
    scene::Color,
};

/// The canvas a draw function records into.
pub(crate) struct Recorder(pub RefCell<Canvas>);

/// `ui.path()`: a path built by chained calls.
pub(crate) struct PathUd(RefCell<Option<PathBuilder>>, RefCell<Option<Path>>);
impl PathUd {
    fn path(&self) -> mlua::Result<Path> {
        if let Some(builder) = self.0.borrow_mut().take() {
            let path = builder.build().map_err(mlua::Error::runtime)?;
            *self.1.borrow_mut() = Some(path);
        }
        self.1
            .borrow()
            .clone()
            .ok_or_else(|| mlua::Error::runtime("empty path"))
    }
    fn with(&self, f: impl FnOnce(&mut PathBuilder)) -> mlua::Result<()> {
        let mut builder = self.0.borrow_mut();
        let builder = builder
            .as_mut()
            .ok_or_else(|| mlua::Error::runtime("a drawn path cannot be extended"))?;
        f(builder);
        Ok(())
    }
}
impl UserData for PathUd {
    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        methods.add_function("move", |_, (this, x, y): (mlua::AnyUserData, f32, f32)| {
            this.borrow::<PathUd>()?.with(|b| {
                b.move_to(x, y);
            })?;
            Ok(this)
        });
        methods.add_function("line", |_, (this, x, y): (mlua::AnyUserData, f32, f32)| {
            this.borrow::<PathUd>()?.with(|b| {
                b.line_to(x, y);
            })?;
            Ok(this)
        });
        methods.add_function(
            "quad",
            |_, (this, cx, cy, x, y): (mlua::AnyUserData, f32, f32, f32, f32)| {
                this.borrow::<PathUd>()?.with(|b| {
                    b.quadratic_to(cx, cy, x, y);
                })?;
                Ok(this)
            },
        );
        methods.add_function(
            "cubic",
            |_, (this, ax, ay, bx, by, x, y): (mlua::AnyUserData, f32, f32, f32, f32, f32, f32)| {
                this.borrow::<PathUd>()?.with(|b| {
                    b.cubic_to(Point::new(ax, ay), Point::new(bx, by), Point::new(x, y));
                })?;
                Ok(this)
            },
        );
        // Angles in degrees, clockwise from +x, as screen coordinates run.
        methods.add_function(
            "arc",
            |_, (this, cx, cy, r, start, sweep): (mlua::AnyUserData, f32, f32, f32, f32, f32)| {
                this.borrow::<PathUd>()?.with(|b| {
                    b.arc(
                        Point::new(cx, cy),
                        r,
                        start.to_radians(),
                        sweep.to_radians(),
                    );
                })?;
                Ok(this)
            },
        );
        // A rectangle (corners rounded by `r`) or an ellipse in a box, as a
        // closed subpath: a path that is just one is drawn as that shape.
        methods.add_function(
            "rect",
            |_, (this, x, y, w, h, r): (mlua::AnyUserData, f32, f32, f32, f32, Option<f32>)| {
                this.borrow::<PathUd>()?.with(|b| {
                    b.rounded_rect(zgx::scene::Rect::new(x, y, w, h), r.unwrap_or(0.));
                })?;
                Ok(this)
            },
        );
        methods.add_function(
            "ellipse",
            |_, (this, x, y, w, h): (mlua::AnyUserData, f32, f32, f32, f32)| {
                this.borrow::<PathUd>()?.with(|b| {
                    b.ellipse(zgx::scene::Rect::new(x, y, w, h));
                })?;
                Ok(this)
            },
        );
        methods.add_function("close", |_, this: mlua::AnyUserData| {
            this.borrow::<PathUd>()?.with(|b| {
                b.close();
            })?;
            Ok(this)
        });
    }
}

/// `ui.linear(x1, y1, x2, y2, stops)`: a gradient brush for canvases.
#[derive(Clone)]
pub(crate) struct BrushUd(pub Brush);
impl UserData for BrushUd {}

thread_local! {
    /// The theme of the canvas drawing now (`draw_in`), whose tokens its
    /// colours name.
    static SCOPE: RefCell<Option<Rc<ThemeScope>>> = const { RefCell::new(None) };
}
/// Run `f`, a canvas's draw, with `$token` colours resolved in `scope`.
pub(crate) fn draw_in<R>(scope: Rc<ThemeScope>, f: impl FnOnce() -> R) -> R {
    struct Restore(Option<Rc<ThemeScope>>);
    impl Drop for Restore {
        fn drop(&mut self) {
            SCOPE.with(|s| *s.borrow_mut() = self.0.take());
        }
    }
    let _restore = Restore(SCOPE.with(|s| s.borrow_mut().replace(scope)));
    f()
}
fn token_color(name: &str) -> mlua::Result<Color> {
    let scope = SCOPE.with(|s| s.borrow().clone()).ok_or_else(|| {
        mlua::Error::runtime(format!(
            "`${name}`: theme colours resolve while a canvas draws (call this in `draw`)"
        ))
    })?;
    scope
        .get(zgx::theme::token(name))
        .color()
        .ok_or_else(|| mlua::Error::runtime(format!("`${name}` is not a colour token")))
}
fn color(v: &mlua::Value) -> mlua::Result<Color> {
    let text = match v {
        mlua::Value::String(s) => s.to_string_lossy(),
        mlua::Value::Integer(n) => format!("#{n:06x}"),
        mlua::Value::Number(n) => format!("#{:06x}", *n as u32),
        other => {
            return Err(mlua::Error::runtime(format!(
                "a color is a string or number, not {}",
                other.type_name()
            )));
        }
    };
    if let Some(name) = text.strip_prefix('$') {
        return token_color(name);
    }
    parse_color(&text).ok_or_else(|| mlua::Error::runtime(format!("bad color `{text}`")))
}
fn brush(v: &mlua::Value) -> mlua::Result<Brush> {
    match v {
        mlua::Value::UserData(ud) if ud.is::<BrushUd>() => Ok(ud.borrow::<BrushUd>()?.0.clone()),
        other => Ok(Brush::Solid(color(other)?)),
    }
}
fn stroke(width: Option<f32>, options: Option<Table>) -> mlua::Result<Stroke> {
    let mut style = Stroke {
        width: width.unwrap_or(1.),
        ..Default::default()
    };
    if let Some(o) = options {
        style.cap = match o.get::<Option<String>>("cap")?.as_deref() {
            Some("round") => LineCap::Round,
            Some("square") => LineCap::Square,
            _ => LineCap::Butt,
        };
        style.join = match o.get::<Option<String>>("join")?.as_deref() {
            Some("round") => LineJoin::Round,
            Some("bevel") => LineJoin::Bevel,
            _ => LineJoin::Miter,
        };
        if let Some(dash) = o.get::<Option<Vec<f32>>>("dash")? {
            style.dash = dash;
        }
        style.dash_offset = o.get::<Option<f32>>("dash_offset")?.unwrap_or(0.);
    }
    Ok(style)
}
/// A rounded rectangle, a shape renderers draw without rasterizing it.
fn rect_path(x: f32, y: f32, w: f32, h: f32, r: f32) -> mlua::Result<Path> {
    Path::rounded_rect(zgx::scene::Rect::new(x, y, w, h), r).map_err(mlua::Error::runtime)
}
fn path_arg(v: &mlua::AnyUserData) -> mlua::Result<Path> {
    v.borrow::<PathUd>()?.path()
}

impl UserData for Recorder {
    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        methods.add_method(
            "rect",
            |_, c, (x, y, w, h, paint, radius): (f32, f32, f32, f32, mlua::Value, Option<f32>)| {
                c.0.borrow_mut()
                    .fill(rect_path(x, y, w, h, radius.unwrap_or(0.))?, brush(&paint)?);
                Ok(())
            },
        );
        methods.add_method(
            "circle",
            |_, c, (cx, cy, r, paint): (f32, f32, f32, mlua::Value)| {
                let r = r.max(0.);
                let circle = Path::ellipse(zgx::scene::Rect::new(cx - r, cy - r, 2. * r, 2. * r))
                    .map_err(mlua::Error::runtime)?;
                c.0.borrow_mut().fill(circle, brush(&paint)?);
                Ok(())
            },
        );
        methods.add_method(
            "line",
            |_,
             c,
             (x1, y1, x2, y2, paint, width, options): (
                f32,
                f32,
                f32,
                f32,
                mlua::Value,
                Option<f32>,
                Option<Table>,
            )| {
                c.0.borrow_mut().stroke(
                    Path::line(Point::new(x1, y1), Point::new(x2, y2))
                        .map_err(mlua::Error::runtime)?,
                    brush(&paint)?,
                    stroke(width, options)?,
                );
                Ok(())
            },
        );
        methods.add_method(
            "fill",
            |_, c, (path, paint, rule): (mlua::AnyUserData, mlua::Value, Option<String>)| {
                let rule = if rule.as_deref() == Some("evenodd") {
                    FillRule::EvenOdd
                } else {
                    FillRule::Winding
                };
                c.0.borrow_mut()
                    .fill_rule(path_arg(&path)?, brush(&paint)?, rule);
                Ok(())
            },
        );
        methods.add_method(
            "stroke",
            |_,
             c,
             (path, paint, width, options): (
                mlua::AnyUserData,
                mlua::Value,
                Option<f32>,
                Option<Table>,
            )| {
                let path = path_arg(&path)?;
                // `draw`: the fraction of the stroke drawn (or `{ start, end }`),
                // as anime.js draws SVG lines progressively.
                let draw = match &options {
                    Some(o) => match o.get::<mlua::Value>("draw")? {
                        mlua::Value::Integer(n) => Some((0., n as f64)),
                        mlua::Value::Number(n) => Some((0., n)),
                        mlua::Value::Table(t) => Some((t.get::<f64>(1)?, t.get::<f64>(2)?)),
                        _ => None,
                    },
                    None => None,
                };
                let mut style = stroke(width, options)?;
                if let Some((start, end)) = draw {
                    let length = crate::anime::svg::canvas_length(&path) as f32;
                    let (start, end) = (start.clamp(0., 1.) as f32, end.clamp(0., 1.) as f32);
                    if end <= start {
                        return Ok(());
                    }
                    style.dash = vec![length * (end - start), length * 2. + 1.];
                    style.dash_offset = -length * start;
                }
                c.0.borrow_mut().stroke(path, brush(&paint)?, style);
                Ok(())
            },
        );
        methods.add_method("clip", |_, c, path: mlua::AnyUserData| {
            c.0.borrow_mut().clip(path_arg(&path)?, FillRule::Winding);
            Ok(())
        });
        methods.add_method("restore", |_, c, ()| {
            c.0.borrow_mut().restore();
            Ok(())
        });
    }
}

pub(crate) fn install(lua: &Lua, ui: &Table) -> mlua::Result<()> {
    ui.set(
        "path",
        lua.create_function(|_, ()| {
            Ok(PathUd(
                RefCell::new(Some(Path::builder())),
                RefCell::new(None),
            ))
        })?,
    )?;
    ui.set(
        "linear",
        lua.create_function(|_, (x1, y1, x2, y2, stops): (f32, f32, f32, f32, Table)| {
            let colors: Vec<mlua::Value> = stops.sequence_values().collect::<mlua::Result<_>>()?;
            let n = colors.len().max(2) - 1;
            let stops: Vec<GradientStop> = colors
                .iter()
                .enumerate()
                .map(|(i, c)| {
                    Ok(GradientStop {
                        offset: i as f32 / n as f32,
                        color: color(c)?,
                    })
                })
                .collect::<mlua::Result<_>>()?;
            Brush::linear(Point::new(x1, y1), Point::new(x2, y2), stops)
                .map(BrushUd)
                .map_err(mlua::Error::runtime)
        })?,
    )?;
    Ok(())
}
