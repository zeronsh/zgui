//! `ui.scene3d`: a 3D scene (zgx-3d) in a script.
//!
//! `ui.scene3d { camera = …, orbit = …, background = …, ui.mesh { … },
//! ui.light { … }, ui.group { … } }` is a box like any other (sized and
//! styled with the usual properties) showing a `zgx_3d::World`. Each
//! object's numbers, vectors and colours are literals, states, functions or
//! `ui.tween`/`ui.spring` bindings. A literal is set once; anything else
//! is read by an effect of the object's own, run again only when what it
//! reads changes, so a still scene runs no script and draws nothing, and a
//! spring turning a model steps in Rust with no script per frame. Dragging
//! turns the camera about its target (`orbit`), scrolling or pinching moves
//! it in and out, `auto_rotate` turns it on its own.
use crate::{
    Vm,
    node::{Anim, AnimUd, Kind, Node, fill, node_ud, owner, parse_color, themed_color},
    value::{State, Value},
};
use mlua::{Function, Lua, Table, UserData};
use std::rc::Rc;
use zgx::{compose::prelude::*, reactive::Signal, theme::Themed};
use zgx_3d::{
    Camera, Geometry, Light, Material, Object, ObjectId, Orbit, Quat, Transform, Vec3, World,
    srgb_to_linear, vec3,
};

/// A number: as written, a state's, a function's, or a binding's.
pub(crate) enum Num {
    Lit(f32),
    State(Signal<Value>),
    Fn(Function),
    Anim(Rc<Anim>),
}
/// Three numbers, each its own, or a whole `{ x, y, z }` from a state or a
/// function.
pub(crate) enum Vec3Spec {
    Parts([Num; 3]),
    Whole(crate::node::Readable),
}
/// A colour: as written (hex, a name, a `$token`), or a state's or a
/// function's.
pub(crate) enum ColorSpec {
    Lit(Themed<zgx::scene::Color>),
    State(Signal<Value>),
    Fn(Function),
}
pub(crate) struct MaterialSpec {
    color: Option<ColorSpec>,
    metallic: Option<Num>,
    roughness: Option<Num>,
    emissive: Option<ColorSpec>,
    opacity: Option<Num>,
    unlit: bool,
}
pub(crate) enum LightKind {
    Directional,
    Point,
    Hemisphere,
}
pub(crate) struct LightSpec {
    kind: LightKind,
    color: Option<ColorSpec>,
    ground: Option<ColorSpec>,
    intensity: Option<Num>,
    direction: Option<Vec3Spec>,
    range: f32,
}
pub(crate) enum Shape {
    Mesh(Geometry, MaterialSpec),
    Light(LightSpec),
    Group(Vec<Rc<ObjectSpec>>),
}
/// An object of a scene (`ui.mesh`, `ui.light`, `ui.group`).
pub(crate) struct ObjectSpec {
    shape: Shape,
    position: Option<Vec3Spec>,
    /// Degrees about X, Y, Z.
    rotation: Option<Vec3Spec>,
    scale: Option<Vec3Spec>,
    /// Called with what was clicked on it (or a mesh in it): `{ x, y, z,
    /// distance }`.
    on_click: Option<Function>,
    /// Called with `true` as the pointer comes over it (or a mesh in it),
    /// `false` as it leaves.
    on_hover: Option<Function>,
}
pub(crate) struct OrbitSpec {
    /// Degrees a second it turns on its own.
    auto_rotate: Option<Num>,
    zoom: bool,
}
/// What `ui.scene3d` shows (`Kind::Scene3d`).
pub(crate) struct Scene3dSpec {
    label: String,
    fov: Option<Num>,
    position: Option<Vec3Spec>,
    target: Option<Vec3Spec>,
    orbit: Option<OrbitSpec>,
    background: Option<ColorSpec>,
    objects: Vec<Rc<ObjectSpec>>,
}

/// A script's mistake, said: made in one place, out of the way of what
/// goes right.
#[cold]
#[inline(never)]
fn wrong(message: std::fmt::Arguments) -> mlua::Error {
    mlua::Error::runtime(message.to_string())
}

/// A 3D object's userdata (`ui.mesh` and its kind).
pub(crate) struct Object3DUd(pub Rc<ObjectSpec>);
impl UserData for Object3DUd {}

fn num(value: mlua::Value, what: &str) -> mlua::Result<Num> {
    match value {
        mlua::Value::Number(n) => Ok(Num::Lit(n as f32)),
        mlua::Value::Integer(n) => Ok(Num::Lit(n as f32)),
        mlua::Value::Function(f) => Ok(Num::Fn(f)),
        mlua::Value::UserData(ud) if ud.is::<State>() => {
            Ok(Num::State(ud.borrow::<State>()?.0.clone()))
        }
        mlua::Value::UserData(ud) if ud.is::<AnimUd>() => {
            Ok(Num::Anim(ud.borrow::<AnimUd>()?.0.clone()))
        }
        other => Err(wrong(format_args!(
            "{what} must be a number, a ui.state, a function or a ui.tween/ui.spring, not {}",
            other.type_name()
        ))),
    }
}
fn opt_num(table: &Table, key: &str) -> mlua::Result<Option<Num>> {
    match table.get::<mlua::Value>(key)? {
        mlua::Value::Nil => Ok(None),
        value => num(value, &format!("`{key}`")).map(Some),
    }
}
fn vec3_spec(value: mlua::Value, what: &str) -> mlua::Result<Vec3Spec> {
    match value {
        mlua::Value::Table(t) => {
            let part = |i: i64| -> mlua::Result<Num> {
                match t.get::<mlua::Value>(i)? {
                    mlua::Value::Nil => Ok(Num::Lit(0.)),
                    v => num(v, what),
                }
            };
            Ok(Vec3Spec::Parts([part(1)?, part(2)?, part(3)?]))
        }
        mlua::Value::Function(f) => Ok(Vec3Spec::Whole(crate::node::Readable::Function(f))),
        mlua::Value::UserData(ud) if ud.is::<State>() => Ok(Vec3Spec::Whole(
            crate::node::Readable::State(ud.borrow::<State>()?.0.clone()),
        )),
        other => Err(wrong(format_args!(
            "{what} must be {{ x, y, z }} (numbers, states, functions or bindings), a ui.state or a function, not {}",
            other.type_name()
        ))),
    }
}
fn opt_vec3(table: &Table, key: &str) -> mlua::Result<Option<Vec3Spec>> {
    match table.get::<mlua::Value>(key)? {
        mlua::Value::Nil => Ok(None),
        value => vec3_spec(value, &format!("`{key}`")).map(Some),
    }
}
fn color_spec(value: mlua::Value, what: &str) -> mlua::Result<ColorSpec> {
    match value {
        mlua::Value::String(s) => {
            let text = s.to_str()?.to_string();
            let themed = themed_color(&Value::Str(text.as_str().into()))
                .or_else(|| parse_color(&text).map(Themed::Value))
                .ok_or_else(|| wrong(format_args!("{what}: not a colour: {text}")))?;
            Ok(ColorSpec::Lit(themed))
        }
        mlua::Value::Function(f) => Ok(ColorSpec::Fn(f)),
        mlua::Value::UserData(ud) if ud.is::<State>() => {
            Ok(ColorSpec::State(ud.borrow::<State>()?.0.clone()))
        }
        other => Err(wrong(format_args!(
            "{what} must be a colour (\"#rrggbb\", a name, \"$token\"), a ui.state or a function, not {}",
            other.type_name()
        ))),
    }
}
fn opt_color(table: &Table, key: &str) -> mlua::Result<Option<ColorSpec>> {
    match table.get::<mlua::Value>(key)? {
        mlua::Value::Nil => Ok(None),
        value => color_spec(value, &format!("`{key}`")).map(Some),
    }
}
/// The objects among `table`'s list entries.
fn objects(table: &Table, what: &str) -> mlua::Result<Vec<Rc<ObjectSpec>>> {
    let mut out = Vec::new();
    for value in table.sequence_values::<mlua::Value>() {
        match value? {
            mlua::Value::UserData(ud) if ud.is::<Object3DUd>() => {
                out.push(ud.borrow::<Object3DUd>()?.0.clone())
            }
            mlua::Value::Nil => {}
            other => {
                return Err(wrong(format_args!(
                    "{what} holds 3D objects (ui.mesh, ui.light, ui.group), not {}",
                    other.type_name()
                )));
            }
        }
    }
    Ok(out)
}
/// `shape`, placed as `table` says (`position`, `rotation` in degrees,
/// `scale`: a number or three), as an object's userdata.
fn placed(lua: &Lua, table: &Table, shape: Shape) -> mlua::Result<mlua::AnyUserData> {
    let scale = match table.get::<mlua::Value>("scale")? {
        mlua::Value::Nil => None,
        // One number scales all three axes alike.
        value @ (mlua::Value::Number(_) | mlua::Value::Integer(_) | mlua::Value::Function(_)) => {
            Some(Vec3Spec::Parts([
                num(value.clone(), "`scale`")?,
                num(value.clone(), "`scale`")?,
                num(value, "`scale`")?,
            ]))
        }
        mlua::Value::UserData(ud) if ud.is::<AnimUd>() => Some(Vec3Spec::Parts([
            num(mlua::Value::UserData(ud.clone()), "`scale`")?,
            num(mlua::Value::UserData(ud.clone()), "`scale`")?,
            num(mlua::Value::UserData(ud), "`scale`")?,
        ])),
        value => Some(vec3_spec(value, "`scale`")?),
    };
    lua.create_userdata(Object3DUd(Rc::new(ObjectSpec {
        shape,
        position: opt_vec3(table, "position")?,
        rotation: opt_vec3(table, "rotation")?,
        scale,
        on_click: opt_fn(table, "on_click")?,
        on_hover: opt_fn(table, "on_hover")?,
    })))
}
fn opt_fn(table: &Table, key: &str) -> mlua::Result<Option<Function>> {
    match table.get::<mlua::Value>(key)? {
        mlua::Value::Nil => Ok(None),
        mlua::Value::Function(f) => Ok(Some(f)),
        other => Err(wrong(format_args!(
            "`{key}` of a 3D object takes a function, not {}",
            other.type_name()
        ))),
    }
}
fn material(table: Option<Table>) -> mlua::Result<MaterialSpec> {
    let Some(table) = table else {
        return Ok(MaterialSpec {
            color: None,
            metallic: None,
            roughness: None,
            emissive: None,
            opacity: None,
            unlit: false,
        });
    };
    Ok(MaterialSpec {
        color: opt_color(&table, "color")?,
        metallic: opt_num(&table, "metallic")?,
        roughness: opt_num(&table, "roughness")?,
        emissive: opt_color(&table, "emissive")?,
        opacity: opt_num(&table, "opacity")?,
        unlit: table.get::<Option<bool>>("unlit")?.unwrap_or(false),
    })
}
/// A literal number of `table`, or `default`.
fn lit(table: &Table, key: &str, default: f32) -> mlua::Result<f32> {
    Ok(table.get::<Option<f32>>(key)?.unwrap_or(default))
}
fn geometry(table: &Table) -> mlua::Result<Geometry> {
    let kind: String = table
        .get::<Option<String>>("geometry")?
        .unwrap_or_else(|| "box".into());
    let size = |default: f32| -> mlua::Result<[f32; 3]> {
        match table.get::<mlua::Value>("size")? {
            mlua::Value::Nil => Ok([default; 3]),
            mlua::Value::Table(t) => Ok([
                t.get::<Option<f32>>(1)?.unwrap_or(default),
                t.get::<Option<f32>>(2)?.unwrap_or(default),
                t.get::<Option<f32>>(3)?.unwrap_or(default),
            ]),
            mlua::Value::Number(n) => Ok([n as f32; 3]),
            mlua::Value::Integer(n) => Ok([n as f32; 3]),
            other => Err(wrong(format_args!(
                "ui.mesh `size` must be a number or {{ x, y, z }} of numbers, not {}",
                other.type_name()
            ))),
        }
    };
    let segments = table
        .get::<Option<u32>>("segments")?
        .unwrap_or(32)
        .clamp(3, 256);
    Ok(match kind.as_str() {
        "box" | "cube" => {
            let [x, y, z] = size(1.)?;
            Geometry::cuboid(x, y, z)
        }
        "sphere" => Geometry::sphere(lit(table, "radius", 0.5)?, segments),
        "plane" => {
            let [x, _, z] = size(1.)?;
            Geometry::plane(x, z)
        }
        "cylinder" => Geometry::cylinder(
            lit(table, "radius", 0.5)?,
            lit(table, "height", 1.)?,
            segments,
        ),
        "cone" => Geometry::cone(
            lit(table, "radius", 0.5)?,
            lit(table, "height", 1.)?,
            segments,
        ),
        "torus" => Geometry::torus(
            lit(table, "radius", 0.5)?,
            lit(table, "tube", 0.15)?,
            segments,
        ),
        other => {
            return Err(wrong(format_args!(
                "ui.mesh `geometry` is box, sphere, plane, cylinder, cone or torus, not {other}"
            )));
        }
    })
}

/// `ui.scene3d`, `ui.mesh`, `ui.light`, `ui.group`: one closure type for
/// the four (one instantiation of mlua's callback for them all).
pub(crate) fn install(lua: &Lua, ui: &Table) -> mlua::Result<()> {
    type Make = fn(&Lua, Table) -> mlua::Result<mlua::AnyUserData>;
    let made: [(&str, Make); 4] = [
        ("mesh", mesh),
        ("light", light),
        ("group", group),
        ("scene3d", scene3d),
    ];
    for (name, make) in made {
        ui.set(
            name,
            lua.create_function(move |lua, table: Table| make(lua, table))?,
        )?;
    }
    Ok(())
}

/// `ui.mesh { geometry, size, material, position, rotation, scale }`.
fn mesh(lua: &Lua, table: Table) -> mlua::Result<mlua::AnyUserData> {
    let shape = Shape::Mesh(geometry(&table)?, material(table.get("material")?)?);
    placed(lua, &table, shape)
}

/// `ui.light { kind, color, ground, intensity, direction, range, position }`.
fn light(lua: &Lua, table: Table) -> mlua::Result<mlua::AnyUserData> {
    let kind = match table.get::<Option<String>>("kind")?.as_deref() {
        None | Some("directional") | Some("sun") => LightKind::Directional,
        Some("point") => LightKind::Point,
        Some("hemisphere") | Some("ambient") => LightKind::Hemisphere,
        Some(other) => {
            return Err(wrong(format_args!(
                "ui.light `kind` is directional, point or hemisphere, not {other}"
            )));
        }
    };
    let shape = Shape::Light(LightSpec {
        kind,
        color: opt_color(&table, "color")?,
        ground: opt_color(&table, "ground")?,
        intensity: opt_num(&table, "intensity")?,
        direction: opt_vec3(&table, "direction")?,
        range: lit(&table, "range", 10.)?,
    });
    placed(lua, &table, shape)
}

/// `ui.group { position, rotation, scale, …objects }`.
fn group(lua: &Lua, table: Table) -> mlua::Result<mlua::AnyUserData> {
    let children = objects(&table, "ui.group")?;
    placed(lua, &table, Shape::Group(children))
}

/// `ui.scene3d { camera, orbit, background, label, …objects, …box props }`.
fn scene3d(lua: &Lua, table: Table) -> mlua::Result<mlua::AnyUserData> {
    let camera: Option<Table> = table.get("camera")?;
    let (fov, position, target) = match &camera {
        Some(camera) => (
            opt_num(camera, "fov")?,
            opt_vec3(camera, "position")?,
            opt_vec3(camera, "target")?,
        ),
        None => (None, None, None),
    };
    let orbit = match table.get::<mlua::Value>("orbit")? {
        mlua::Value::Nil | mlua::Value::Boolean(false) => None,
        mlua::Value::Boolean(true) => Some(OrbitSpec {
            auto_rotate: None,
            zoom: true,
        }),
        mlua::Value::Table(orbit) => Some(OrbitSpec {
            auto_rotate: opt_num(&orbit, "auto_rotate")?,
            zoom: orbit.get::<Option<bool>>("zoom")?.unwrap_or(true),
        }),
        other => {
            return Err(wrong(format_args!(
                "ui.scene3d `orbit` must be true or {{ auto_rotate, zoom }}, not {}",
                other.type_name()
            )));
        }
    };
    let spec = Scene3dSpec {
        label: table
            .get::<Option<String>>("label")?
            .unwrap_or_else(|| "3D scene".into()),
        fov,
        position,
        target,
        orbit,
        background: opt_color(&table, "background")?,
        objects: objects(&table, "ui.scene3d")?,
    };
    // Its box's own properties: the table's named keys alone but
    // the scene's (its list holds the objects, not elements).
    let props = lua.create_table()?;
    for pair in table.pairs::<mlua::Value, mlua::Value>() {
        let (key, value) = pair?;
        if let mlua::Value::String(name) = &key
            && !matches!(&*name.as_bytes(), b"camera" | b"orbit" | b"background")
        {
            props.set(key, value)?;
        }
    }
    node_ud(
        lua,
        fill(
            lua,
            Node::new(owner(lua)?, Kind::Scene3d(Rc::new(spec))),
            Some(&props),
        )?,
    )
}

/// A number as it is now, read where it changes.
enum Live {
    Lit(f32),
    State(Signal<Value>),
    Fn(Function),
    /// A binding's value (`anim::drive`), mapped onto `from`..`to`.
    Driven(Signal<f32>, Option<(f32, f32)>),
}
impl Live {
    fn of(num: &Num, cx: &mut Context) -> Self {
        match num {
            Num::Lit(n) => Self::Lit(*n),
            Num::State(s) => Self::State(s.clone()),
            Num::Fn(f) => Self::Fn(f.clone()),
            Num::Anim(anim) => {
                let range = match (&anim.from, &anim.to) {
                    (Some(from), Some(to)) => from
                        .as_f64()
                        .zip(to.as_f64())
                        .map(|(a, b)| (a as f32, b as f32)),
                    _ => None,
                };
                Self::Driven(crate::anim::drive(cx, anim), range)
            }
        }
    }
    fn dynamic(&self) -> bool {
        !matches!(self, Self::Lit(_))
    }
    fn get(&self, vm: &Vm) -> f32 {
        match self {
            Self::Lit(n) => *n,
            Self::State(s) => crate::lower::number(&s.get()),
            Self::Fn(f) => vm
                .call::<Value>(f, ())
                .map(|v| crate::lower::number(&v))
                .map_err(|e| vm.report(e))
                .unwrap_or(0.),
            Self::Driven(s, None) => s.get(),
            Self::Driven(s, Some((from, to))) => from + (to - from) * s.get(),
        }
    }
}
enum LiveVec {
    Parts([Live; 3]),
    Whole(crate::node::Readable),
}
impl LiveVec {
    fn of(spec: &Vec3Spec, cx: &mut Context) -> Self {
        match spec {
            Vec3Spec::Parts([x, y, z]) => {
                Self::Parts([Live::of(x, cx), Live::of(y, cx), Live::of(z, cx)])
            }
            Vec3Spec::Whole(r) => Self::Whole(r.clone()),
        }
    }
    fn dynamic(&self) -> bool {
        match self {
            Self::Parts(parts) => parts.iter().any(Live::dynamic),
            Self::Whole(_) => true,
        }
    }
    fn get(&self, vm: &Vm) -> Vec3 {
        match self {
            Self::Parts([x, y, z]) => vec3(x.get(vm), y.get(vm), z.get(vm)),
            Self::Whole(r) => {
                let value = match r {
                    crate::node::Readable::State(s) => s.get(),
                    crate::node::Readable::Function(f) => vm
                        .call::<Value>(f, ())
                        .map_err(|e| vm.report(e))
                        .unwrap_or_default(),
                };
                let at = |i: usize| value.list().get(i).map(crate::lower::number).unwrap_or(0.);
                vec3(at(0), at(1), at(2))
            }
        }
    }
}
enum LiveColor {
    Lit(Themed<zgx::scene::Color>),
    State(Signal<Value>),
    Fn(Function),
}
impl LiveColor {
    fn of(spec: &ColorSpec) -> Self {
        match spec {
            ColorSpec::Lit(c) => Self::Lit(*c),
            ColorSpec::State(s) => Self::State(s.clone()),
            ColorSpec::Fn(f) => Self::Fn(f.clone()),
        }
    }
    /// Its colour in linear light, read where it changes (a token: the
    /// theme's, followed).
    fn get(&self, vm: &Vm, theme: &zgx::theme::ThemeScope) -> [f32; 4] {
        let color = match self {
            Self::Lit(Themed::Value(c)) => Some(*c),
            Self::Lit(Themed::Token(token)) => theme.get(*token).color(),
            Self::State(s) => value_color(&s.get(), theme),
            Self::Fn(f) => vm
                .call::<Value>(f, ())
                .map_err(|e| vm.report(e))
                .ok()
                .and_then(|v| value_color(&v, theme)),
        };
        let zgx::scene::Color(r, g, b, a) = color.unwrap_or(zgx::scene::Color(204, 204, 204, 255));
        [
            srgb_to_linear(f32::from(r) / 255.),
            srgb_to_linear(f32::from(g) / 255.),
            srgb_to_linear(f32::from(b) / 255.),
            f32::from(a) / 255.,
        ]
    }
}
fn value_color(value: &Value, theme: &zgx::theme::ThemeScope) -> Option<zgx::scene::Color> {
    match themed_color(value)? {
        Themed::Value(c) => Some(c),
        Themed::Token(token) => theme.get(token).color(),
    }
}

/// What one object reads, as it is now.
struct LiveObject {
    position: Option<LiveVec>,
    rotation: Option<LiveVec>,
    scale: Option<LiveVec>,
}
impl LiveObject {
    fn dynamic(&self) -> bool {
        [&self.position, &self.rotation, &self.scale]
            .iter()
            .any(|v| v.as_ref().is_some_and(LiveVec::dynamic))
    }
    fn transform(&self, vm: &Vm) -> Transform {
        let rotation = self
            .rotation
            .as_ref()
            .map(|r| r.get(vm))
            .unwrap_or(Vec3::ZERO);
        Transform {
            position: self
                .position
                .as_ref()
                .map(|p| p.get(vm))
                .unwrap_or(Vec3::ZERO),
            rotation: Quat::from_euler(
                rotation.x.to_radians(),
                rotation.y.to_radians(),
                rotation.z.to_radians(),
            ),
            scale: self.scale.as_ref().map(|s| s.get(vm)).unwrap_or(Vec3::ONE),
        }
    }
}
struct LiveMaterial {
    color: Option<LiveColor>,
    metallic: Option<Live>,
    roughness: Option<Live>,
    emissive: Option<LiveColor>,
    opacity: Option<Live>,
    unlit: bool,
}
impl LiveMaterial {
    fn of(spec: &MaterialSpec, cx: &mut Context) -> Self {
        Self {
            color: spec.color.as_ref().map(LiveColor::of),
            metallic: spec.metallic.as_ref().map(|n| Live::of(n, cx)),
            roughness: spec.roughness.as_ref().map(|n| Live::of(n, cx)),
            emissive: spec.emissive.as_ref().map(LiveColor::of),
            opacity: spec.opacity.as_ref().map(|n| Live::of(n, cx)),
            unlit: spec.unlit,
        }
    }
    fn dynamic(&self) -> bool {
        // A colour's token follows the theme: read in an effect too.
        let color = |c: &Option<LiveColor>| {
            c.as_ref()
                .is_some_and(|c| !matches!(c, LiveColor::Lit(Themed::Value(_))))
        };
        color(&self.color)
            || color(&self.emissive)
            || [&self.metallic, &self.roughness, &self.opacity]
                .iter()
                .any(|n| n.as_ref().is_some_and(Live::dynamic))
    }
    fn get(&self, vm: &Vm, theme: &zgx::theme::ThemeScope) -> Material {
        let mut material = Material::default();
        if let Some(color) = &self.color {
            material.color = color.get(vm, theme);
        }
        if let Some(n) = &self.metallic {
            material.metallic = n.get(vm).clamp(0., 1.);
        }
        if let Some(n) = &self.roughness {
            material.roughness = n.get(vm).clamp(0., 1.);
        }
        if let Some(c) = &self.emissive {
            let [r, g, b, _] = c.get(vm, theme);
            material.emissive = [r, g, b];
        }
        if let Some(n) = &self.opacity {
            material.color[3] *= n.get(vm).clamp(0., 1.);
        }
        material.unlit = self.unlit;
        material
    }
}

/// `effect` followed while `cx` lives: boxed, one instantiation of the
/// effect for every one here.
fn follow(cx: &mut Context, effect: Box<dyn FnMut()>) {
    let runtime = cx.runtime();
    cx.retain(runtime.effect(effect));
}

/// `spec` lowered: a GPU view of its world, the world's objects added and
/// what they read followed by effects owned by `cx`.
pub(crate) fn lower(spec: &Scene3dSpec, vm: &Rc<Vm>, cx: &mut Context) -> zgx::compose::View {
    let world = World::new();
    let theme = cx.theme();
    let mut handled = Vec::new();
    for object in &spec.objects {
        add(&world, object, None, vm, cx, &theme, &mut handled);
    }
    // The background, and the camera (orbiting, if asked: dragged, scrolled
    // and turning on its own).
    if let Some(background) = &spec.background {
        let background = LiveColor::of(background);
        let (world, vm, theme) = (world.clone(), vm.clone(), theme.clone());
        follow(
            cx,
            Box::new(move || {
                let [r, g, b, _] = background.get(&vm, &theme);
                world.set_background_linear(Some([r, g, b, 1.]));
            }),
        );
    }
    let fov = spec.fov.as_ref().map(|n| Live::of(n, cx));
    let position = spec.position.as_ref().map(|v| LiveVec::of(v, cx));
    let target = spec.target.as_ref().map(|v| LiveVec::of(v, cx));
    let base = Camera::default();
    let place =
        move |position: &Option<LiveVec>, target: &Option<LiveVec>, vm: &Vm| -> (Vec3, Vec3) {
            (
                position
                    .as_ref()
                    .map(|p| p.get(vm))
                    .unwrap_or(base.position),
                target.as_ref().map(|t| t.get(vm)).unwrap_or(base.target),
            )
        };
    let orbit = spec.orbit.as_ref().map(|orbit_spec| {
        let (at, looking) = vm.runtime.untracked(|| place(&position, &target, vm));
        let offset = at - looking;
        let distance = offset.length().max(1e-3);
        let state = cx.state(Orbit {
            target: looking,
            azimuth: offset.x.atan2(offset.z),
            elevation: (offset.y / distance).clamp(-1., 1.).asin(),
            distance,
            ..Orbit::default()
        });
        (state, orbit_spec)
    });
    {
        let (world, vm) = (world.clone(), vm.clone());
        let orbit_state = orbit.as_ref().map(|(state, _)| state.clone());
        follow(
            cx,
            Box::new(move || {
                let mut camera = Camera::default();
                if let Some(fov) = &fov {
                    camera.fov = fov.get(&vm).clamp(1., 170.).to_radians();
                }
                match &orbit_state {
                    Some(orbit) => {
                        let mut orbit = orbit.get();
                        if let Some(target) = &target {
                            orbit.target = target.get(&vm);
                        }
                        camera = orbit.camera(camera);
                    }
                    None => {
                        let (at, looking) = place(&position, &target, &vm);
                        camera.position = at;
                        camera.target = looking;
                    }
                }
                world.set_camera(camera);
            }),
        );
    }
    if let Some((orbit, spec)) = &orbit
        && let Some(rate) = &spec.auto_rotate
    {
        let rate = Live::of(rate, cx);
        let (orbit, vm, frames) = (orbit.clone(), vm.clone(), cx.frames());
        cx.tasks().spawn(async move {
            let mut last = frames.next().await.time;
            loop {
                let degrees = vm.runtime.untracked(|| rate.get(&vm));
                if degrees == 0. {
                    // Not turning: look again in a while, no frame asked.
                    zgx::timer::sleep(std::time::Duration::from_millis(250)).await;
                    last = std::time::Instant::now();
                    continue;
                }
                let frame = frames.next().await;
                let seconds = frame.time.saturating_duration_since(last).as_secs_f32();
                last = frame.time;
                orbit.update(|o| o.azimuth += (degrees * seconds.min(0.1)).to_radians());
            }
        });
    }
    // The world lives as long as its box.
    cx.retain(world.clone());
    let mut view = gpu_view(spec.label.clone(), world.view());
    if let Some((orbit, spec)) = orbit {
        let dragging = Rc::new(std::cell::Cell::new(None::<(f32, f32)>));
        let zoom = spec.zoom;
        view = view.on_event(move |event| {
            use zgx::input::InputEvent;
            if event.phase == zgx::input::EventPhase::Capture {
                return;
            }
            match event.event {
                InputEvent::PointerDown { x, y, .. } => dragging.set(Some((x, y))),
                InputEvent::PointerMove { x, y } => {
                    if let Some((from_x, from_y)) = dragging.get() {
                        dragging.set(Some((x, y)));
                        orbit.update(|o| o.drag(x - from_x, y - from_y));
                    }
                }
                InputEvent::PointerUp { .. } | InputEvent::PointerCancel => dragging.set(None),
                InputEvent::Scroll { delta_y, .. } if zoom => {
                    orbit.update(|o| o.zoom((delta_y * 0.004).exp()));
                    event.prevent_default();
                }
                InputEvent::Pinch { delta, .. } if zoom => {
                    orbit.update(|o| o.zoom(1. + delta));
                }
                _ => {}
            }
        });
    }
    if !handled.is_empty() {
        view = picking(view, world, handled, vm.clone());
    }
    view
}

/// How far a press may move and still click (points): further, it is a
/// drag (orbiting the camera), and clicks nothing.
const CLICK_SLOP: f32 = 4.;

/// `view` picking its world's objects with handlers (`on_click`,
/// `on_hover`): what the pointer is over is found as it moves (only if an
/// object hears hovering) and where a press ends near where it began. A hit
/// on a mesh reaches the nearest of it and its groups that hears it.
fn picking(
    view: zgx::compose::View,
    world: World,
    handled: Vec<(ObjectId, Rc<ObjectSpec>)>,
    vm: Rc<Vm>,
) -> zgx::compose::View {
    use std::cell::{Cell, RefCell};
    use zgx::input::{EventPhase, InputEvent, PointerButton};
    let hovering = handled.iter().any(|(_, spec)| spec.on_hover.is_some());
    let handle = Rc::new(RefCell::new(None::<zgx::compose::NodeHandle>));
    let mounted = handle.clone();
    let pressed = Cell::new(None::<(f32, f32)>);
    let over = RefCell::new(None::<(ObjectId, Rc<ObjectSpec>)>);
    view.on_mount(move |node| *mounted.borrow_mut() = Some(node))
        .on_event(move |event| {
            if event.phase == EventPhase::Capture {
                return;
            }
            // What is at window point `x`, `y`, in the box.
            let pick = |x: f32, y: f32| {
                let scene = handle.borrow().as_ref()?.scene()?;
                let bounds = scene.borrow().bounds(event.current_target);
                world.pick((x - bounds.x, y - bounds.y), (bounds.width, bounds.height))
            };
            let hover = |now: Option<(ObjectId, Rc<ObjectSpec>)>| {
                let before = over.replace(now.clone());
                if before.as_ref().map(|(id, _)| *id) == now.as_ref().map(|(id, _)| *id) {
                    return;
                }
                for (side, is_over) in [(before, false), (now, true)] {
                    if let Some((_, spec)) = side
                        && let Some(f) = &spec.on_hover
                    {
                        call(&vm, f, mlua::Value::Boolean(is_over));
                    }
                }
            };
            match event.event {
                InputEvent::PointerDown {
                    x,
                    y,
                    button: PointerButton::Primary,
                } => pressed.set(Some((x, y))),
                InputEvent::PointerUp {
                    x,
                    y,
                    button: PointerButton::Primary,
                } => {
                    if let Some((from_x, from_y)) = pressed.take()
                        && (x - from_x).hypot(y - from_y) <= CLICK_SLOP
                        && let Some(hit) = pick(x, y)
                        && let Some((_, spec)) =
                            hearing(&world, &handled, hit.object, |s| s.on_click.is_some())
                        && let Some(f) = &spec.on_click
                    {
                        let at = vm.lua.create_table().and_then(|t| {
                            t.set("x", hit.point.x)?;
                            t.set("y", hit.point.y)?;
                            t.set("z", hit.point.z)?;
                            t.set("distance", hit.distance)?;
                            Ok(t)
                        });
                        match at {
                            Ok(at) => call(&vm, f, mlua::Value::Table(at)),
                            Err(error) => vm.report(error),
                        }
                    }
                }
                InputEvent::PointerMove { x, y } if hovering => {
                    let now = pick(x, y).and_then(|hit| {
                        hearing(&world, &handled, hit.object, |s| s.on_hover.is_some())
                    });
                    hover(now);
                }
                InputEvent::PointerLeave | InputEvent::PointerCancel => {
                    pressed.set(None);
                    if hovering {
                        hover(None);
                    }
                }
                _ => {}
            }
        })
}
/// The object hearing what `wants` says that a hit on `id` reaches: it or
/// its nearest group that does.
fn hearing(
    world: &World,
    handled: &[(ObjectId, Rc<ObjectSpec>)],
    id: ObjectId,
    wants: fn(&ObjectSpec) -> bool,
) -> Option<(ObjectId, Rc<ObjectSpec>)> {
    let mut at = Some(id);
    while let Some(id) = at {
        if let Some((_, spec)) = handled.iter().find(|(o, spec)| *o == id && wants(spec)) {
            return Some((id, spec.clone()));
        }
        at = world.object(id).and_then(|object| object.parent);
    }
    None
}
/// Call a script's handler `f` with `arg`, in one batch, its error reported.
fn call(vm: &Rc<Vm>, f: &Function, arg: mlua::Value) {
    vm.runtime.clone().batch(|| {
        if let Err(error) = vm.call::<()>(f, arg) {
            vm.report(error);
        }
    });
}

/// Add `spec` (a child of `parent`) to `world`, following what it reads.
fn add(
    world: &World,
    spec: &Rc<ObjectSpec>,
    parent: Option<ObjectId>,
    vm: &Rc<Vm>,
    cx: &mut Context,
    theme: &Rc<zgx::theme::ThemeScope>,
    handled: &mut Vec<(ObjectId, Rc<ObjectSpec>)>,
) {
    let live = LiveObject {
        position: spec.position.as_ref().map(|v| LiveVec::of(v, cx)),
        rotation: spec.rotation.as_ref().map(|v| LiveVec::of(v, cx)),
        scale: spec.scale.as_ref().map(|v| LiveVec::of(v, cx)),
    };
    match &spec.shape {
        Shape::Light(light) => {
            let color = light.color.as_ref().map(LiveColor::of);
            let ground = light.ground.as_ref().map(LiveColor::of);
            let intensity = light.intensity.as_ref().map(|n| Live::of(n, cx));
            let direction = light.direction.as_ref().map(|v| LiveVec::of(v, cx));
            let (kind, range) = (
                match light.kind {
                    LightKind::Directional => 0,
                    LightKind::Point => 1,
                    LightKind::Hemisphere => 2,
                },
                light.range,
            );
            let read = {
                let (vm, theme) = (vm.clone(), theme.clone());
                move || -> Light {
                    let rgb = |c: &Option<LiveColor>, default: [f32; 3]| {
                        c.as_ref().map_or(default, |c| {
                            let [r, g, b, _] = c.get(&vm, &theme);
                            [r, g, b]
                        })
                    };
                    let intensity = intensity.as_ref().map_or(1., |n| n.get(&vm));
                    match kind {
                        0 => Light::Directional {
                            direction: direction
                                .as_ref()
                                .map_or(vec3(-0.5, -1., -0.4), |d| d.get(&vm)),
                            color: rgb(&color, [1.; 3]),
                            intensity,
                        },
                        1 => Light::Point {
                            position: live.position.as_ref().map_or(Vec3::ZERO, |p| p.get(&vm)),
                            color: rgb(&color, [1.; 3]),
                            intensity,
                            range,
                        },
                        _ => Light::Hemisphere {
                            sky: rgb(&color, [0.6, 0.7, 0.85]),
                            ground: rgb(&ground, [0.2, 0.18, 0.15]),
                            intensity,
                        },
                    }
                }
            };
            let id = world.add_light(vm.runtime.untracked(&read));
            let world = world.clone();
            follow(cx, Box::new(move || world.set_light(id, read())));
        }
        Shape::Mesh(..) | Shape::Group(_) => {
            let (geometry, material) = match &spec.shape {
                Shape::Mesh(geometry, material) => {
                    (Some(geometry.clone()), Some(LiveMaterial::of(material, cx)))
                }
                _ => (None, None),
            };
            let object = Object {
                geometry,
                material: material
                    .as_ref()
                    .map(|m| vm.runtime.untracked(|| m.get(vm, theme)))
                    .unwrap_or_default(),
                transform: vm.runtime.untracked(|| live.transform(vm)),
                visible: true,
                parent,
            };
            let id = world.add(object);
            if spec.on_click.is_some() || spec.on_hover.is_some() {
                handled.push((id, spec.clone()));
            }
            if live.dynamic() {
                let (world, vm) = (world.clone(), vm.clone());
                follow(
                    cx,
                    Box::new(move || {
                        let transform = live.transform(&vm);
                        world.set_transform(id, transform);
                    }),
                );
            }
            if let Some(material) = material.filter(LiveMaterial::dynamic) {
                let (world, vm, theme) = (world.clone(), vm.clone(), theme.clone());
                follow(
                    cx,
                    Box::new(move || {
                        let material = material.get(&vm, &theme);
                        world.set_material(id, material);
                    }),
                );
            }
            if let Shape::Group(children) = &spec.shape {
                for child in children {
                    add(world, child, Some(id), vm, cx, theme, handled);
                }
            }
        }
    }
}
