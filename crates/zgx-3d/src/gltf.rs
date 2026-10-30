//! glTF 2.0 models (feature `gltf`): a `.glb`, or a `.gltf` with its
//! buffers inline (`data:` URIs) or beside it, read into a [`Model`] that a
//! world adds as a group ([`World::add_model`]).
//!
//! What it reads: the default scene's node tree (each node's matrix or
//! translation, rotation and scale), its meshes' triangles (positions,
//! normals, made smooth where a mesh has none, and texture coordinates, of
//! any of glTF's component types), and their metallic-roughness materials'
//! factors: colour, metal, roughness, emission, blending (`BLEND`) and
//! `KHR_materials_unlit` and `KHR_materials_emissive_strength`, and their
//! base colour, metal and roughness, normal, emission and occlusion
//! textures (PNG or JPEG, in the file, inline or beside it), and its
//! animations' translations, rotations and scales (`Model::clips`,
//! `Placed::pose`), skins moving its meshes by their joints, and morph
//! targets (positions and normals; its weights, and clips moving them).
//! Accessors may be sparse. Materials' `doubleSided` and alpha modes are
//! read, and where their maps sit: a second set of coordinates (`TEXCOORD_1`),
//! samplers' wrapping and `KHR_texture_transform`, and vertex colours
//! (`COLOR_0`). Not yet: points and lines (skipped); geometry compressed with Draco
//! or meshopt, an error where the file requires it.
use crate::{
    Geometry, MapPlacement, Material, MorphTarget, Object, ObjectId, Quat, Texture, Transform,
    Vec3, Vertex, World, Wrap, vec3, world::Surface,
};
use serde_json::Value;

/// Why a model could not be read.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ModelError(String);
impl std::fmt::Display for ModelError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "glTF: {}", self.0)
    }
}
impl std::error::Error for ModelError {}
fn error<T>(message: impl Into<String>) -> Result<T, ModelError> {
    Err(ModelError(message.into()))
}

/// A glTF model's node tree, its meshes made: add it to worlds with
/// [`World::add_model`], as often as wanted (they share its geometry).
#[derive(Clone, Debug)]
pub struct Model {
    nodes: Vec<Node>,
    roots: Vec<usize>,
    clips: Vec<crate::Clip>,
    skins: Vec<ModelSkin>,
}
#[derive(Clone, Debug)]
struct Node {
    transform: Transform,
    children: Vec<usize>,
    meshes: Vec<Mesh>,
    /// The skin moving its skinned meshes, by index.
    skin: Option<usize>,
    /// Its meshes' morph weights: its own, else its mesh's.
    weights: Vec<f32>,
}
/// A skin: its joints (nodes) and each one's inverse bind matrix.
#[derive(Clone, Debug)]
struct ModelSkin {
    joints: Vec<usize>,
    inverse_binds: Vec<crate::Mat4>,
}

/// A mesh's textures, in `Map::ALL`'s order.
type Maps = [Option<Texture>; crate::texture::MAPS];
/// Where they sit on it, in the same order.
type Placements = [MapPlacement; crate::texture::MAPS];
/// A material, read.
type Look = (Material, Maps, Surface, Placements);
/// A primitive, read.
type Mesh = (Geometry, Material, Maps, Surface, Placements);

/// Deeper than this, a node tree is taken for a cycle.
const MAX_DEPTH: usize = 256;

impl Model {
    /// A model from a `.glb` file's bytes, or a `.gltf` file's whose
    /// buffers are inline.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, ModelError> {
        Self::from_bytes_with(bytes, |_| None)
    }
    /// A model from a `.glb` or `.gltf` file's bytes, `resolve` giving the
    /// bytes of each buffer it names by URI (a file beside it).
    pub fn from_bytes_with(
        bytes: &[u8],
        mut resolve: impl FnMut(&str) -> Option<Vec<u8>>,
    ) -> Result<Self, ModelError> {
        let (json, bin) = if bytes.starts_with(b"glTF") {
            glb(bytes)?
        } else {
            (bytes, None)
        };
        let doc: Value =
            serde_json::from_slice(json).map_err(|e| ModelError(format!("its JSON: {e}")))?;
        // Its geometry can't be read without these: better said than drawn
        // as nothing. (Other extensions it requires change how it looks.)
        for required in list(&doc, "extensionsRequired") {
            if let Some(name) = required
                .as_str()
                .filter(|name| COMPRESSED_GEOMETRY.contains(name))
            {
                return error(format!("it needs {name}, which isn't read yet"));
            }
        }
        let buffers = list(&doc, "buffers")
            .iter()
            .enumerate()
            .map(
                |(i, buffer)| match buffer.get("uri").and_then(Value::as_str) {
                    None if i == 0 => bin
                        .map(<[u8]>::to_vec)
                        .ok_or_else(|| ModelError("buffer 0 has no data".into())),
                    None => error(format!("buffer {i} has no URI")),
                    Some(uri) => match uri.strip_prefix("data:") {
                        Some(data) => data
                            .split_once(";base64,")
                            .and_then(|(_, b64)| base64(b64))
                            .ok_or_else(|| ModelError(format!("buffer {i}'s data URI"))),
                        None => resolve(uri)
                            .ok_or_else(|| ModelError(format!("buffer {i} ({uri}) not found"))),
                    },
                },
            )
            .collect::<Result<Vec<_>, _>>()?;
        let reader = Reader {
            doc: &doc,
            buffers: &buffers,
        };
        // Textures: each image a material uses, decoded once. One that
        // can't be read leaves its meshes their factors alone.
        let mut images: Vec<Option<Option<Texture>>> = vec![None; list(&doc, "images").len()];
        let materials = list(&doc, "materials")
            .iter()
            .map(|value| {
                let maps: Maps = MAP_TEXTURES.map(|path| {
                    let image = material_image(&doc, value, path)?;
                    images
                        .get_mut(image)?
                        .get_or_insert_with(|| {
                            let bytes = reader.image(image, &mut resolve)?;
                            Texture::decode(&bytes).ok()
                        })
                        .clone()
                });
                let placements = MAP_TEXTURES.map(|path| map_placement(&doc, value, path));
                (material(value), maps, surface(value), placements)
            })
            .collect::<Vec<_>>();
        let meshes = list(&doc, "meshes")
            .iter()
            .map(|mesh| reader.mesh(mesh, &materials))
            .collect::<Result<Vec<_>, _>>()?;
        let nodes = list(&doc, "nodes")
            .iter()
            .map(|node| {
                let children = indices(node, "children");
                let meshes = match node.get("mesh").and_then(Value::as_u64) {
                    Some(m) => meshes
                        .get(m as usize)
                        .cloned()
                        .ok_or_else(|| ModelError(format!("no mesh {m}")))?,
                    None => Vec::new(),
                };
                let weights = node.get("weights").or_else(|| {
                    let mesh = node.get("mesh").and_then(Value::as_u64)?;
                    list(&doc, "meshes").get(mesh as usize)?.get("weights")
                });
                Ok(Node {
                    transform: transform(node),
                    children,
                    meshes,
                    skin: node.get("skin").and_then(Value::as_u64).map(|s| s as usize),
                    weights: weights
                        .and_then(Value::as_array)
                        .map(|w| {
                            w.iter()
                                .filter_map(Value::as_f64)
                                .map(|w| w as f32)
                                .collect()
                        })
                        .unwrap_or_default(),
                })
            })
            .collect::<Result<Vec<_>, ModelError>>()?;
        if let Some(bad) = nodes
            .iter()
            .flat_map(|n| &n.children)
            .find(|&&c| c >= nodes.len())
        {
            return error(format!("no node {bad}"));
        }
        let scene = doc.get("scene").and_then(Value::as_u64).unwrap_or(0) as usize;
        let roots = match list(&doc, "scenes").get(scene) {
            Some(scene) => indices(scene, "nodes"),
            // No scenes: every node no other holds.
            None => (0..nodes.len())
                .filter(|&i| !nodes.iter().any(|n| n.children.contains(&i)))
                .collect(),
        };
        if let Some(bad) = roots.iter().find(|&&r| r >= nodes.len()) {
            return error(format!("no node {bad}"));
        }
        let clips = list(&doc, "animations")
            .iter()
            .map(|animation| reader.clip(animation, nodes.len()))
            .collect::<Result<Vec<_>, _>>()?;
        let skins = list(&doc, "skins")
            .iter()
            .map(|skin| {
                let joints = indices(skin, "joints");
                if joints.iter().any(|&j| j >= nodes.len()) || joints.len() > crate::MAX_JOINTS {
                    return error("a skin's joints are nodes, at most 256");
                }
                let inverse_binds = match skin.get("inverseBindMatrices").and_then(Value::as_u64) {
                    Some(at) => reader
                        .floats(at as usize, 16)?
                        .as_chunks::<16>()
                        .0
                        .iter()
                        .map(|m| crate::Mat4 {
                            columns: std::array::from_fn(|c| std::array::from_fn(|r| m[c * 4 + r])),
                        })
                        .collect(),
                    None => vec![crate::Mat4::IDENTITY; joints.len()],
                };
                if inverse_binds.len() != joints.len() {
                    return error("a skin's inverse bind matrices don't match its joints");
                }
                Ok(ModelSkin {
                    joints,
                    inverse_binds,
                })
            })
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Self {
            nodes,
            roots,
            clips,
            skins,
        })
    }
    /// Its animations (pose one with `Placed::pose`).
    pub fn clips(&self) -> &[crate::Clip] {
        &self.clips
    }
    /// Its animation named `name`, if it has one.
    pub fn clip(&self, name: &str) -> Option<&crate::Clip> {
        self.clips.iter().find(|clip| clip.name() == Some(name))
    }
    /// The box holding its meshes, in its own frame: least and greatest
    /// corners. `None` if it has no meshes.
    pub fn bounds(&self) -> Option<(Vec3, Vec3)> {
        let mut out: Option<(Vec3, Vec3)> = None;
        let mut stack: Vec<(usize, crate::Mat4, usize)> = self
            .roots
            .iter()
            .map(|&r| (r, crate::Mat4::IDENTITY, 0))
            .collect();
        while let Some((index, parent, depth)) = stack.pop() {
            if depth > MAX_DEPTH {
                continue;
            }
            let node = &self.nodes[index];
            let matrix = parent * node.transform.matrix();
            for (geometry, ..) in &node.meshes {
                let (low, high) = geometry.bounds();
                for i in 0..8 {
                    let p = matrix.project(vec3(
                        if i & 1 == 0 { low.x } else { high.x },
                        if i & 2 == 0 { low.y } else { high.y },
                        if i & 4 == 0 { low.z } else { high.z },
                    ));
                    out = Some(out.map_or((p, p), |(l, h)| (l.min(p), h.max(p))));
                }
            }
            stack.extend(node.children.iter().map(|&c| (c, matrix, depth + 1)));
        }
        out
    }
}

impl World {
    /// Add `model`: a group (at the origin, to place as any object) holding
    /// its nodes as groups and their meshes. Its id.
    pub fn add_model(&self, model: &Model) -> ObjectId {
        self.place_model(model).root
    }
    /// Add `model` as `add_model` does: its root, and its nodes' objects,
    /// to pose its clips on (`Placed::pose`).
    pub fn place_model(&self, model: &Model) -> crate::Placed {
        let root = self.add(Object::group());
        let mut nodes = vec![None; model.nodes.len()];
        let mut meshes = vec![Vec::new(); model.nodes.len()];
        // Skinned meshes, given their skins once every joint is placed.
        let mut skinned = Vec::new();
        let mut stack: Vec<(usize, ObjectId, usize)> =
            model.roots.iter().rev().map(|&r| (r, root, 0)).collect();
        while let Some((index, parent, depth)) = stack.pop() {
            if depth > MAX_DEPTH {
                continue;
            }
            let node = &model.nodes[index];
            let id = self.add(Object::group().transformed(node.transform).child_of(parent));
            nodes[index] = Some(id);
            for (geometry, material, maps, surface, placements) in &node.meshes {
                let mesh = self.add(Object::mesh(geometry.clone(), *material).child_of(id));
                if surface.double_sided {
                    self.set_double_sided(mesh, true);
                }
                if surface.alpha_cutoff.is_some() {
                    self.set_alpha_cutoff(mesh, surface.alpha_cutoff);
                }
                for (map, texture) in crate::Map::ALL.into_iter().zip(maps) {
                    if texture.is_some() {
                        self.set_map(mesh, map, texture.clone());
                    }
                }
                for (map, placement) in crate::Map::ALL.into_iter().zip(placements) {
                    if *placement != MapPlacement::default() {
                        self.set_map_placement(mesh, map, Some(*placement));
                    }
                }
                if let Some(skin) = node.skin.and_then(|s| model.skins.get(s)) {
                    skinned.push((mesh, skin));
                }
                if geometry.morphs().is_some() && !node.weights.is_empty() {
                    self.set_morph_weights(mesh, &node.weights);
                }
                meshes[index].push(mesh);
            }
            stack.extend(node.children.iter().rev().map(|&c| (c, id, depth + 1)));
        }
        for (mesh, skin) in skinned {
            // A joint its scene doesn't show leaves the mesh as bound.
            let joints: Option<Vec<ObjectId>> = skin.joints.iter().map(|&j| nodes[j]).collect();
            if let Some(joints) = joints
                && let Ok(skin) = crate::Skin::new(joints, skin.inverse_binds.clone())
            {
                self.set_skin(mesh, Some(skin));
            }
        }
        crate::Placed {
            root,
            nodes,
            meshes,
        }
    }
}

/// Extensions a file's geometry may be compressed with.
const COMPRESSED_GEOMETRY: [&str; 3] = [
    "KHR_draco_mesh_compression",
    "KHR_meshopt_compression",
    "EXT_meshopt_compression",
];

/// A `.glb`'s JSON chunk and its binary chunk, if any.
fn glb(bytes: &[u8]) -> Result<(&[u8], Option<&[u8]>), ModelError> {
    let word = |at: usize| -> Option<u32> {
        Some(u32::from_le_bytes(bytes.get(at..at + 4)?.try_into().ok()?))
    };
    if word(4) != Some(2) {
        return error("only glTF 2 binaries are read");
    }
    let total = (word(8).unwrap_or(0) as usize).min(bytes.len());
    let (mut at, mut json, mut bin) = (12, None, None);
    while at + 8 <= total {
        let (Some(length), Some(kind)) = (word(at), word(at + 4)) else {
            break;
        };
        let start = at + 8;
        let Some(chunk) = start
            .checked_add(length as usize)
            .and_then(|end| bytes.get(start..end.min(total)))
        else {
            return error("a chunk runs past the file");
        };
        match kind {
            0x4E4F_534A if json.is_none() => json = Some(chunk),
            0x004E_4942 if bin.is_none() => bin = Some(chunk),
            _ => {}
        }
        at = start + (length as usize).next_multiple_of(4);
    }
    Ok((json.ok_or_else(|| ModelError("no JSON chunk".into()))?, bin))
}

fn list<'a>(value: &'a Value, key: &str) -> &'a [Value] {
    value
        .get(key)
        .and_then(Value::as_array)
        .map_or(&[], Vec::as_slice)
}
fn indices(value: &Value, key: &str) -> Vec<usize> {
    list(value, key)
        .iter()
        .filter_map(Value::as_u64)
        .map(|i| i as usize)
        .collect()
}
fn floats<const N: usize>(value: Option<&Value>) -> Option<[f32; N]> {
    let array = value?.as_array()?;
    if array.len() != N {
        return None;
    }
    let mut out = [0.; N];
    for (slot, v) in out.iter_mut().zip(array) {
        *slot = v.as_f64()? as f32;
    }
    out.iter().all(|v| v.is_finite()).then_some(out)
}

/// A node's place in its parent's frame: its matrix taken apart, or its
/// translation, rotation and scale.
fn transform(node: &Value) -> Transform {
    if let Some(m) = floats::<16>(node.get("matrix")) {
        let column = |c: usize| vec3(m[c * 4], m[c * 4 + 1], m[c * 4 + 2]);
        let (x, y, z) = (column(0), column(1), column(2));
        let mut scale = vec3(x.length(), y.length(), z.length());
        // A mirror: one axis turned over.
        if x.cross(y).dot(z) < 0. {
            scale.x = -scale.x;
        }
        let unit = |v: Vec3, s: f32| if s.abs() > 1e-12 { v * (1. / s) } else { v };
        let (x, y, z) = (unit(x, scale.x), unit(y, scale.y), unit(z, scale.z));
        return Transform {
            position: column(3),
            rotation: rotation_of(x, y, z),
            scale,
        };
    }
    let [tx, ty, tz] = floats::<3>(node.get("translation")).unwrap_or([0.; 3]);
    let [x, y, z, w] = floats::<4>(node.get("rotation")).unwrap_or([0., 0., 0., 1.]);
    let [sx, sy, sz] = floats::<3>(node.get("scale")).unwrap_or([1.; 3]);
    Transform {
        position: vec3(tx, ty, tz),
        rotation: Quat { x, y, z, w }.normalized(),
        scale: vec3(sx, sy, sz),
    }
}
/// The rotation taking the axes to unit columns `x`, `y`, `z`.
fn rotation_of(x: Vec3, y: Vec3, z: Vec3) -> Quat {
    let trace = x.x + y.y + z.z;
    let q = if trace > 0. {
        let s = (trace + 1.).sqrt() * 2.;
        Quat {
            w: 0.25 * s,
            x: (y.z - z.y) / s,
            y: (z.x - x.z) / s,
            z: (x.y - y.x) / s,
        }
    } else if x.x > y.y && x.x > z.z {
        let s = (1. + x.x - y.y - z.z).sqrt() * 2.;
        Quat {
            w: (y.z - z.y) / s,
            x: 0.25 * s,
            y: (y.x + x.y) / s,
            z: (z.x + x.z) / s,
        }
    } else if y.y > z.z {
        let s = (1. + y.y - x.x - z.z).sqrt() * 2.;
        Quat {
            w: (z.x - x.z) / s,
            x: (y.x + x.y) / s,
            y: 0.25 * s,
            z: (z.y + y.z) / s,
        }
    } else {
        let s = (1. + z.z - x.x - y.y).sqrt() * 2.;
        Quat {
            w: (x.y - y.x) / s,
            x: (z.x + x.z) / s,
            y: (z.y + y.z) / s,
            z: 0.25 * s,
        }
    };
    q.normalized()
}

/// A material's factors (glTF's are in linear light, as zgx-3d's), its
/// emission scaled by `KHR_materials_emissive_strength`.
fn material(value: &Value) -> Material {
    let pbr = value.get("pbrMetallicRoughness");
    let number = |key: &str, default: f32| {
        pbr.and_then(|p| p.get(key))
            .and_then(Value::as_f64)
            .map_or(default, |v| v as f32)
    };
    let mut color = floats::<4>(pbr.and_then(|p| p.get("baseColorFactor"))).unwrap_or([1.; 4]);
    if value.get("alphaMode").and_then(Value::as_str) != Some("BLEND") {
        color[3] = 1.;
    } else if color[3] >= 1. && pbr.is_some_and(|p| p.get("baseColorTexture").is_some()) {
        // See-through by its texture alone: only an alpha under 1 draws it
        // with the translucent meshes, reading that texture's.
        color[3] = BLENDED_BY_TEXTURE;
    }
    Material {
        color,
        metallic: number("metallicFactor", 1.).clamp(0., 1.),
        roughness: number("roughnessFactor", 1.).clamp(0., 1.),
        emissive: {
            let strength = value
                .get("extensions")
                .and_then(|e| e.get("KHR_materials_emissive_strength"))
                .and_then(|e| e.get("emissiveStrength"))
                .and_then(Value::as_f64)
                .filter(|s| s.is_finite() && *s >= 0.)
                .unwrap_or(1.) as f32;
            floats::<3>(value.get("emissiveFactor"))
                .unwrap_or([0.; 3])
                .map(|c| c * strength)
        },
        unlit: value
            .get("extensions")
            .is_some_and(|e| e.get("KHR_materials_unlit").is_some()),
    }
}

/// A blended material's colour alpha when only its texture has any.
const BLENDED_BY_TEXTURE: f32 = 0.9999;
/// How a material's faces show: `doubleSided`, and `MASK` as a cutoff for
/// its texture's alpha alone (its factor's taken out: a mesh is drawn
/// opaque, its colour's alpha 1 here).
fn surface(value: &Value) -> Surface {
    let masked = value.get("alphaMode").and_then(Value::as_str) == Some("MASK");
    let cutoff = value
        .get("alphaCutoff")
        .and_then(Value::as_f64)
        .map_or(0.5, |c| c as f32);
    let factor = floats::<4>(
        value
            .get("pbrMetallicRoughness")
            .and_then(|p| p.get("baseColorFactor")),
    )
    .map_or(1., |c| c[3]);
    Surface {
        double_sided: value
            .get("doubleSided")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        alpha_cutoff: masked.then(|| if factor > 0. { cutoff / factor } else { 2. }),
    }
}

/// Where a material names each of its maps' textures (`Map::ALL`'s order).
const MAP_TEXTURES: [&[&str]; crate::texture::MAPS] = [
    &["pbrMetallicRoughness", "baseColorTexture"],
    &["pbrMetallicRoughness", "metallicRoughnessTexture"],
    &["normalTexture"],
    &["emissiveTexture"],
    &["occlusionTexture"],
];
/// The image of the texture `material` names at `path`, if it has one.
fn material_image(doc: &Value, material: &Value, path: &[&str]) -> Option<usize> {
    let texture = path
        .iter()
        .try_fold(material, |at, key| at.get(key))?
        .get("index")?
        .as_u64()?;
    let source = list(doc, "textures").get(texture as usize)?.get("source")?;
    source.as_u64().map(|i| i as usize)
}
/// Where the texture `material` names at `path` sits: its `texCoord`, its
/// sampler's wrapping and its `KHR_texture_transform`.
fn map_placement(doc: &Value, material: &Value, path: &[&str]) -> MapPlacement {
    let mut placement = MapPlacement::new();
    let Some(info) = path.iter().try_fold(material, |at, key| at.get(key)) else {
        return placement;
    };
    let mut set = info.get("texCoord").and_then(Value::as_u64);
    if let Some(transform) = info
        .get("extensions")
        .and_then(|e| e.get("KHR_texture_transform"))
    {
        if let Some([u, v]) = floats::<2>(transform.get("offset")) {
            placement = placement.offset(u, v);
        }
        if let Some(radians) = transform.get("rotation").and_then(Value::as_f64) {
            placement = placement.rotated(radians as f32);
        }
        if let Some([u, v]) = floats::<2>(transform.get("scale")) {
            placement = placement.scaled(u, v);
        }
        set = transform.get("texCoord").and_then(Value::as_u64).or(set);
    }
    let sampler = info
        .get("index")
        .and_then(Value::as_u64)
        .and_then(|texture| list(doc, "textures").get(texture as usize)?.get("sampler"))
        .and_then(Value::as_u64)
        .and_then(|sampler| list(doc, "samplers").get(sampler as usize));
    let wrap = |key: &str| match sampler.and_then(|s| s.get(key)).and_then(Value::as_u64) {
        Some(33071) => Wrap::Clamp,
        Some(33648) => Wrap::Mirror,
        _ => Wrap::Repeat,
    };
    placement
        .uv_set(set.map_or(0, |set| set.min(u64::from(u8::MAX)) as u8))
        .wrap(wrap("wrapS"), wrap("wrapT"))
}

struct Reader<'a> {
    doc: &'a Value,
    buffers: &'a [Vec<u8>],
}
impl Reader<'_> {
    /// An animation's channels moving nodes (of `nodes`): translations,
    /// rotations, scales and morph target weights (a channel a target).
    fn clip(&self, animation: &Value, nodes: usize) -> Result<crate::Clip, ModelError> {
        use crate::animation::{Channel, Interpolation, Path};
        let samplers = list(animation, "samplers");
        let mut channels = Vec::new();
        for channel in list(animation, "channels") {
            let target = channel.get("target");
            let node = target
                .and_then(|t| t.get("node"))
                .and_then(Value::as_u64)
                .map(|n| n as usize);
            let path = match target.and_then(|t| t.get("path")).and_then(Value::as_str) {
                Some("translation") => Some(Path::Translation),
                Some("rotation") => Some(Path::Rotation),
                Some("scale") => Some(Path::Scale),
                Some("weights") => None,
                _ => continue,
            };
            let Some(node) = node.filter(|&n| n < nodes) else {
                continue;
            };
            let Some(sampler) = channel
                .get("sampler")
                .and_then(Value::as_u64)
                .and_then(|s| samplers.get(s as usize))
            else {
                return error("an animation channel without its sampler");
            };
            let interpolation = match sampler.get("interpolation").and_then(Value::as_str) {
                Some("STEP") => Interpolation::Step,
                Some("CUBICSPLINE") => Interpolation::Cubic,
                _ => Interpolation::Linear,
            };
            let accessor = |key: &str| {
                sampler
                    .get(key)
                    .and_then(Value::as_u64)
                    .map(|i| i as usize)
                    .ok_or_else(|| ModelError(format!("an animation sampler without its {key}")))
            };
            let times = self.floats(accessor("input")?, 1)?;
            let per = if interpolation == Interpolation::Cubic {
                3
            } else {
                1
            };
            let width = match path {
                Some(Path::Rotation) => 4,
                Some(_) => 3,
                None => 1,
            };
            let flat = self.floats(accessor("output")?, width)?;
            let values: Vec<[f32; 4]> = match path {
                Some(_) => flat
                    .chunks_exact(width)
                    .map(|v| [v[0], v[1], v[2], if width == 4 { v[3] } else { 0. }])
                    .collect(),
                None => Vec::new(),
            };
            let keys = times.len() * per;
            if times.is_empty()
                || (path.is_some() && values.len() != keys)
                || (path.is_none() && (flat.is_empty() || !flat.len().is_multiple_of(keys)))
            {
                return error("an animation sampler's keys and values don't match");
            }
            // Falling, or not numbers (NaN), times can't be keys.
            let rising = |t: &[f32]| {
                matches!(
                    t[0].partial_cmp(&t[1]),
                    Some(std::cmp::Ordering::Less | std::cmp::Ordering::Equal)
                )
            };
            if !times.windows(2).all(rising) {
                return error("an animation sampler's times don't rise");
            }
            let Some(path) = path else {
                // Weights: each key holds every target's. A channel a
                // target, each sampled as any other.
                let targets = flat.len() / keys;
                for target in 0..targets {
                    channels.push(Channel {
                        node,
                        path: Path::Weight(target),
                        interpolation,
                        times: times.clone(),
                        values: (0..keys)
                            .map(|key| [flat[key * targets + target], 0., 0., 0.])
                            .collect(),
                    });
                }
                continue;
            };
            channels.push(Channel {
                node,
                path,
                interpolation,
                times,
                values,
            });
        }
        let duration = channels
            .iter()
            .filter_map(|c| c.times.last().copied())
            .fold(0., f32::max);
        Ok(crate::Clip {
            name: animation
                .get("name")
                .and_then(Value::as_str)
                .map(str::to_owned),
            duration,
            channels,
        })
    }
    /// Image `index`'s encoded bytes: from a buffer view, a data URI or a
    /// file `resolve` gives.
    fn image(
        &self,
        index: usize,
        resolve: &mut impl FnMut(&str) -> Option<Vec<u8>>,
    ) -> Option<Vec<u8>> {
        let image = list(self.doc, "images").get(index)?;
        if let Some(uri) = image.get("uri").and_then(Value::as_str) {
            return match uri.strip_prefix("data:") {
                Some(data) => data.split_once(";base64,").and_then(|(_, b64)| base64(b64)),
                None => resolve(uri),
            };
        }
        let view = list(self.doc, "bufferViews").get(image.get("bufferView")?.as_u64()? as usize)?;
        let number = |key: &str| view.get(key).and_then(Value::as_u64).unwrap_or(0) as usize;
        let buffer = self.buffers.get(number("buffer"))?;
        let start = number("byteOffset");
        buffer
            .get(start..start.checked_add(number("byteLength"))?)
            .map(<[u8]>::to_vec)
    }
    /// A mesh's triangle primitives as geometries in their materials.
    fn mesh(&self, mesh: &Value, materials: &[Look]) -> Result<Vec<Mesh>, ModelError> {
        let mut out = Vec::new();
        for primitive in list(mesh, "primitives") {
            // Triangles only: points, lines, strips and fans are skipped.
            if primitive.get("mode").and_then(Value::as_u64).unwrap_or(4) != 4 {
                continue;
            }
            let attributes = primitive.get("attributes");
            let attribute = |name: &str| {
                attributes
                    .and_then(|a| a.get(name))
                    .and_then(Value::as_u64)
                    .map(|i| i as usize)
            };
            let Some(position) = attribute("POSITION") else {
                return error("a primitive without positions");
            };
            let positions = self.floats(position, 3)?;
            let count = positions.len() / 3;
            let normals = attribute("NORMAL")
                .map(|i| self.floats(i, 3))
                .transpose()?
                .filter(|n| n.len() == count * 3);
            let uvs = attribute("TEXCOORD_0")
                .map(|i| self.floats(i, 2))
                .transpose()?
                .filter(|uv| uv.len() == count * 2);
            let indices = match primitive.get("indices").and_then(Value::as_u64) {
                Some(i) => self.indices(i as usize)?,
                None => (0..count as u32).collect(),
            };
            let mut vertices: Vec<Vertex> = (0..count)
                .map(|i| Vertex {
                    position: [positions[i * 3], positions[i * 3 + 1], positions[i * 3 + 2]],
                    normal: normals
                        .as_ref()
                        .map_or([0.; 3], |n| [n[i * 3], n[i * 3 + 1], n[i * 3 + 2]]),
                    uv: uvs
                        .as_ref()
                        .map_or([0.; 2], |uv| [uv[i * 2], uv[i * 2 + 1]]),
                })
                .collect();
            let indices: Vec<u32> = indices[..indices.len() / 3 * 3].to_vec();
            if normals.is_none() {
                smooth_normals(&mut vertices, &indices);
            }
            let (mut material, maps, surface, placements) = primitive
                .get("material")
                .and_then(Value::as_u64)
                .and_then(|m| materials.get(m as usize).cloned())
                .unwrap_or_else(|| {
                    let plain = Material {
                        color: [1.; 4],
                        metallic: 1.,
                        roughness: 1.,
                        ..Material::default()
                    };
                    (
                        plain,
                        Maps::default(),
                        Surface::default(),
                        Placements::default(),
                    )
                });
            let mut geometry =
                Geometry::new(vertices, indices).map_err(|e| ModelError(e.into()))?;
            // Its second coordinates, if a map reads them.
            if placements.iter().any(|p| p.uv_set == 1)
                && let Some(uvs) = attribute("TEXCOORD_1")
                    .map(|i| self.floats(i, 2))
                    .transpose()?
                    .filter(|uv| uv.len() == count * 2)
            {
                geometry = geometry
                    .with_second_uvs(uvs.as_chunks::<2>().0.to_vec())
                    .map_err(|e| ModelError(e.into()))?;
            }
            // Its vertices' colours, red, green and blue, with alpha or not.
            if let Some(index) = attribute("COLOR_0") {
                let with_alpha = self.accessor_type(index) == Some("VEC4");
                let components = if with_alpha { 4 } else { 3 };
                let flat = self.floats(index, components)?;
                if flat.len() == count * components {
                    let colors: Vec<[f32; 4]> = flat
                        .chunks_exact(components)
                        .map(|c| [c[0], c[1], c[2], c.get(3).copied().unwrap_or(1.)])
                        .collect();
                    // See-through by its vertices alone, as by a texture.
                    if material.color[3] >= 1.
                        && colors.iter().any(|c| c[3] < 1.)
                        && self.blended(primitive)
                    {
                        material.color[3] = BLENDED_BY_TEXTURE;
                    }
                    geometry = geometry
                        .with_colors(colors)
                        .map_err(|e| ModelError(e.into()))?;
                }
            }
            // Skinned: each vertex's four joints and their weights.
            if let (Some(joints), Some(weights)) = (attribute("JOINTS_0"), attribute("WEIGHTS_0")) {
                let (joints, weights) = (self.floats(joints, 4)?, self.floats(weights, 4)?);
                if joints.len() == count * 4 && weights.len() == count * 4 {
                    let joints = joints
                        .as_chunks::<4>()
                        .0
                        .iter()
                        .map(|j| {
                            std::array::from_fn(|i| j[i].clamp(0., f32::from(u16::MAX)) as u16)
                        })
                        .collect();
                    let weights = weights.as_chunks::<4>().0.to_vec();
                    geometry = geometry
                        .with_skin(joints, weights)
                        .map_err(|e| ModelError(e.into()))?;
                }
            }
            // Its morph targets, all or none: one that can't be read leaves
            // the mesh as it is.
            let targets: Option<Vec<MorphTarget>> = list(primitive, "targets")
                .iter()
                .map(|target| self.morph_target(target, count))
                .collect();
            if let Some(targets) = targets.filter(|t| !t.is_empty()) {
                geometry = geometry
                    .with_morph_targets(targets)
                    .map_err(|e| ModelError(e.into()))?;
            }
            out.push((geometry, material, maps, surface, placements));
        }
        Ok(out)
    }
    /// Accessor `index`'s element type (`VEC3`), if it says.
    fn accessor_type(&self, index: usize) -> Option<&str> {
        list(self.doc, "accessors")
            .get(index)?
            .get("type")?
            .as_str()
    }
    /// Whether `primitive`'s material blends.
    fn blended(&self, primitive: &Value) -> bool {
        primitive
            .get("material")
            .and_then(Value::as_u64)
            .and_then(|m| list(self.doc, "materials").get(m as usize))
            .and_then(|m| m.get("alphaMode"))
            .and_then(Value::as_str)
            == Some("BLEND")
    }
    /// A primitive's morph target for its `count` vertices, or `None` if it
    /// can't be read.
    fn morph_target(&self, target: &Value, count: usize) -> Option<MorphTarget> {
        let read = |name: &str| -> Option<Vec<[f32; 3]>> {
            let Some(index) = target.get(name).and_then(Value::as_u64) else {
                return Some(Vec::new());
            };
            let flat = self.floats(index as usize, 3).ok()?;
            (flat.len() == count * 3).then(|| flat.as_chunks::<3>().0.to_vec())
        };
        let (mut positions, normals) = (read("POSITION")?, read("NORMAL")?);
        // A target moving normals alone moves no vertex.
        positions.resize(count, [0.; 3]);
        Some(MorphTarget::new(positions).with_normals(normals))
    }
    /// Accessor `index`'s view, its bytes from its start, its element's
    /// stride and size, and its count and component type.
    fn view(&self, index: usize, components: usize) -> Result<Accessor<'_>, ModelError> {
        let Some(accessor) = list(self.doc, "accessors").get(index) else {
            return error(format!("no accessor {index}"));
        };
        let count = accessor.get("count").and_then(Value::as_u64).unwrap_or(0) as usize;
        let kind = accessor
            .get("componentType")
            .and_then(Value::as_u64)
            .unwrap_or(0);
        let size = match kind {
            5120 | 5121 => 1,
            5122 | 5123 => 2,
            5125 | 5126 => 4,
            _ => return error(format!("accessor {index}'s component type {kind}")),
        };
        let element = size * components;
        let normalized = accessor
            .get("normalized")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let Some(view_index) = accessor.get("bufferView").and_then(Value::as_u64) else {
            // No view: all zeros (a file can claim any count: the sizes it
            // has the bytes for, sparse ones the most compact, stay small).
            if count > MAX_ZEROS {
                return error(format!("accessor {index} holds too many zeros"));
            }
            return Ok(Accessor {
                bytes: &[],
                stride: element,
                count,
                kind,
                normalized,
            });
        };
        let Some(view) = list(self.doc, "bufferViews").get(view_index as usize) else {
            return error(format!("no buffer view {view_index}"));
        };
        let number =
            |v: &Value, key: &str| v.get(key).and_then(Value::as_u64).unwrap_or(0) as usize;
        let Some(buffer) = self.buffers.get(number(view, "buffer")) else {
            return error(format!("buffer view {view_index}'s buffer"));
        };
        let start = number(view, "byteOffset") + number(accessor, "byteOffset");
        let stride = match number(view, "byteStride") {
            0 => element,
            s => s,
        };
        let needed = if count == 0 {
            0
        } else {
            (count - 1) * stride + element
        };
        // Within its view, and its view within its buffer.
        let end = number(view, "byteOffset") + number(view, "byteLength");
        if start + needed > end || end > buffer.len() {
            return error(format!("accessor {index} runs past its buffer"));
        }
        Ok(Accessor {
            bytes: &buffer[start..start + needed],
            stride,
            count,
            kind,
            normalized,
        })
    }
    /// Accessor `index`'s sparse part, if it has one.
    fn sparse(&self, index: usize, components: usize) -> Result<Option<Sparse<'_>>, ModelError> {
        let accessor = &list(self.doc, "accessors")[index];
        let Some(sparse) = accessor.get("sparse") else {
            return Ok(None);
        };
        let number = |key: &str| accessor.get(key).and_then(Value::as_u64).unwrap_or(0);
        let total = number("count") as usize;
        let count = sparse.get("count").and_then(Value::as_u64).unwrap_or(0) as usize;
        if count > total {
            return error(format!("accessor {index} replaces more than it holds"));
        }
        let (Some(indices), Some(values)) = (sparse.get("indices"), sparse.get("values")) else {
            return error(format!("accessor {index}'s sparse part is incomplete"));
        };
        let kind = indices
            .get("componentType")
            .and_then(Value::as_u64)
            .unwrap_or(0);
        let size = match kind {
            5121 => 1,
            5123 => 2,
            5125 => 4,
            _ => return error(format!("accessor {index}'s sparse indices of type {kind}")),
        };
        let at = self.section(index, indices, count * size)?;
        let mut slots = Vec::with_capacity(count);
        for b in at.chunks_exact(size) {
            let slot = match kind {
                5121 => usize::from(b[0]),
                5123 => usize::from(u16::from_le_bytes([b[0], b[1]])),
                _ => u32::from_le_bytes([b[0], b[1], b[2], b[3]]) as usize,
            };
            if slot >= total {
                return error(format!("accessor {index}'s sparse index {slot} is past it"));
            }
            slots.push(slot);
        }
        let element = component_size(number("componentType")) * components;
        let values = self.section(index, values, count * element)?;
        Ok(Some(Sparse { slots, values }))
    }
    /// `length` bytes at `part`'s `bufferView` and `byteOffset`.
    fn section(&self, index: usize, part: &Value, length: usize) -> Result<&[u8], ModelError> {
        let number = |v: &Value, key: &str| v.get(key).and_then(Value::as_u64).unwrap_or(0);
        let past = || {
            ModelError(format!(
                "accessor {index}'s sparse part runs past its buffer"
            ))
        };
        let view = usize::try_from(number(part, "bufferView")).map_err(|_| past())?;
        let view = list(self.doc, "bufferViews").get(view).ok_or_else(past)?;
        let buffer = self
            .buffers
            .get(number(view, "buffer") as usize)
            .ok_or_else(past)?;
        let view_start = number(view, "byteOffset");
        let view_end = view_start
            .checked_add(number(view, "byteLength"))
            .ok_or_else(past)?;
        let start = view_start
            .checked_add(number(part, "byteOffset"))
            .ok_or_else(past)?;
        let end = start.checked_add(length as u64).ok_or_else(past)?;
        if end > view_end || view_end > buffer.len() as u64 {
            return Err(past());
        }
        Ok(&buffer[start as usize..end as usize])
    }
    /// Accessor `index`'s `components` numbers an element, as floats.
    fn floats(&self, index: usize, components: usize) -> Result<Vec<f32>, ModelError> {
        let a = self.view(index, components)?;
        let mut out = Vec::with_capacity(a.count * components);
        let size = component_size(a.kind);
        if a.bytes.is_empty() {
            out.resize(a.count * components, 0.);
        } else {
            for i in 0..a.count {
                for c in 0..components {
                    let at = i * a.stride + c * size;
                    out.push(float(&a.bytes[at..at + size], a.kind, a.normalized));
                }
            }
        }
        if let Some(sparse) = self.sparse(index, components)? {
            let values = sparse.values.chunks_exact(size * components);
            for (&slot, element) in sparse.slots.iter().zip(values) {
                for (out, b) in out[slot * components..]
                    .iter_mut()
                    .zip(element.chunks_exact(size))
                {
                    *out = float(b, a.kind, a.normalized);
                }
            }
        }
        Ok(out)
    }
    /// Accessor `index` as indices (unsigned bytes, shorts or ints).
    fn indices(&self, index: usize) -> Result<Vec<u32>, ModelError> {
        let a = self.view(index, 1)?;
        let read = |b: &[u8]| match a.kind {
            5121 => Ok(u32::from(b[0])),
            5123 => Ok(u32::from(u16::from_le_bytes([b[0], b[1]]))),
            5125 => Ok(u32::from_le_bytes([b[0], b[1], b[2], b[3]])),
            kind => error(format!("indices of component type {kind}")),
        };
        let mut out = if a.bytes.is_empty() {
            vec![0; a.count]
        } else {
            (0..a.count)
                .map(|i| read(&a.bytes[i * a.stride..]))
                .collect::<Result<Vec<u32>, ModelError>>()?
        };
        if let Some(sparse) = self.sparse(index, 1)? {
            let values = sparse.values.chunks_exact(component_size(a.kind));
            for (&slot, b) in sparse.slots.iter().zip(values) {
                out[slot] = read(b)?;
            }
        }
        Ok(out)
    }
}
/// What a sparse accessor replaces: which elements, and their new values,
/// packed.
struct Sparse<'a> {
    slots: Vec<usize>,
    values: &'a [u8],
}
/// Elements an accessor with no buffer view of its own may hold: more than
/// any mesh or clip, far less than memory.
const MAX_ZEROS: usize = 1 << 26;
/// Bytes a component of glTF's type `kind` takes.
fn component_size(kind: u64) -> usize {
    match kind {
        5120 | 5121 => 1,
        5122 | 5123 => 2,
        _ => 4,
    }
}
/// The component at `b`, of glTF's type `kind`, as a float.
fn float(b: &[u8], kind: u64, normalized: bool) -> f32 {
    let v = match (kind, normalized) {
        (5126, _) => f32::from_le_bytes([b[0], b[1], b[2], b[3]]),
        (5121, true) => f32::from(b[0]) / 255.,
        (5121, false) => f32::from(b[0]),
        (5120, true) => (f32::from(b[0] as i8) / 127.).max(-1.),
        (5120, false) => f32::from(b[0] as i8),
        (5123, true) => f32::from(u16::from_le_bytes([b[0], b[1]])) / 65535.,
        (5123, false) => f32::from(u16::from_le_bytes([b[0], b[1]])),
        (5122, true) => (f32::from(i16::from_le_bytes([b[0], b[1]])) / 32767.).max(-1.),
        (5122, false) => f32::from(i16::from_le_bytes([b[0], b[1]])),
        _ => u32::from_le_bytes([b[0], b[1], b[2], b[3]]) as f32,
    };
    if v.is_finite() { v } else { 0. }
}
struct Accessor<'a> {
    bytes: &'a [u8],
    stride: usize,
    count: usize,
    kind: u64,
    normalized: bool,
}

/// Each vertex's normal: the sum of its triangles' (weighted by area),
/// made unit.
fn smooth_normals(vertices: &mut [Vertex], indices: &[u32]) {
    let mut sums = vec![Vec3::ZERO; vertices.len()];
    for t in indices.as_chunks::<3>().0 {
        let [a, b, c] = t.map(|i| i as usize);
        if a.max(b).max(c) >= vertices.len() {
            continue;
        }
        let p = |i: usize| Vec3::from(vertices[i].position);
        let n = (p(b) - p(a)).cross(p(c) - p(a));
        for i in [a, b, c] {
            sums[i] = sums[i] + n;
        }
    }
    for (vertex, sum) in vertices.iter_mut().zip(sums) {
        let n = if sum.length() > 1e-12 {
            sum.normalized()
        } else {
            Vec3::Y
        };
        vertex.normal = n.to_array();
    }
}

/// Standard base64 (padding optional), or `None` if it isn't.
fn base64(text: &str) -> Option<Vec<u8>> {
    let mut out = Vec::with_capacity(text.len() * 3 / 4);
    let (mut bits, mut held) = (0_u32, 0);
    for byte in text.bytes() {
        let value = match byte {
            b'A'..=b'Z' => byte - b'A',
            b'a'..=b'z' => byte - b'a' + 26,
            b'0'..=b'9' => byte - b'0' + 52,
            b'+' | b'-' => 62,
            b'/' | b'_' => 63,
            b'=' => break,
            b' ' | b'\n' | b'\r' | b'\t' => continue,
            _ => return None,
        };
        bits = bits << 6 | u32::from(value);
        held += 6;
        if held >= 8 {
            held -= 8;
            out.push((bits >> held) as u8);
        }
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// A `.glb` of `doc` and `bin`.
    fn glb_of(doc: &Value, bin: &[u8]) -> Vec<u8> {
        let mut json = serde_json::to_vec(doc).unwrap();
        while !json.len().is_multiple_of(4) {
            json.push(b' ');
        }
        let mut bin = bin.to_vec();
        while !bin.len().is_multiple_of(4) {
            bin.push(0);
        }
        let total = 12 + 8 + json.len() + 8 + bin.len();
        let mut out = Vec::new();
        out.extend(b"glTF");
        out.extend(2_u32.to_le_bytes());
        out.extend((total as u32).to_le_bytes());
        out.extend((json.len() as u32).to_le_bytes());
        out.extend(0x4E4F_534A_u32.to_le_bytes());
        out.extend(&json);
        out.extend((bin.len() as u32).to_le_bytes());
        out.extend(0x004E_4942_u32.to_le_bytes());
        out.extend(&bin);
        out
    }

    /// A unit square facing +z (two triangles, u16 indices), its buffer, and
    /// a document placing it as `nodes` say.
    fn square(nodes: Value, extra: impl FnOnce(&mut Value)) -> (Value, Vec<u8>) {
        let positions: [[f32; 3]; 4] = [
            [-0.5, -0.5, 0.],
            [0.5, -0.5, 0.],
            [0.5, 0.5, 0.],
            [-0.5, 0.5, 0.],
        ];
        let indices: [u16; 6] = [0, 1, 2, 0, 2, 3];
        let mut bin: Vec<u8> = bytemuck::cast_slice(&positions).to_vec();
        bin.extend(bytemuck::cast_slice(&indices));
        let mut doc = json!({
            "asset": { "version": "2.0" },
            "buffers": [{ "byteLength": bin.len() }],
            "bufferViews": [
                { "buffer": 0, "byteOffset": 0, "byteLength": 48 },
                { "buffer": 0, "byteOffset": 48, "byteLength": 12 },
            ],
            "accessors": [
                { "bufferView": 0, "componentType": 5126, "count": 4, "type": "VEC3" },
                { "bufferView": 1, "componentType": 5123, "count": 6, "type": "SCALAR" },
            ],
            "materials": [{
                "pbrMetallicRoughness": { "baseColorFactor": [1.0, 0.0, 0.0, 0.5], "metallicFactor": 0.0, "roughnessFactor": 0.25 },
                "alphaMode": "BLEND",
            }],
            "meshes": [{ "primitives": [{ "attributes": { "POSITION": 0 }, "indices": 1, "material": 0 }] }],
            "nodes": nodes,
            "scenes": [{ "nodes": [0] }],
            "scene": 0,
        });
        extra(&mut doc);
        (doc, bin)
    }

    #[test]
    fn a_glb_is_read_into_meshes_placed_by_its_nodes() {
        let (doc, bin) = square(
            json!([
                { "translation": [0.0, 0.0, -1.0], "children": [1] },
                { "mesh": 0, "scale": [2.0, 2.0, 2.0] },
            ]),
            |_| {},
        );
        let model = Model::from_bytes(&glb_of(&doc, &bin)).unwrap();
        let (low, high) = model.bounds().unwrap();
        assert_eq!((low, high), (vec3(-1., -1., -1.), vec3(1., 1., -1.)));
        let world = World::new();
        let root = world.add_model(&model);
        world.set_camera(crate::Camera::looking_at([0., 0., 3.], [0., 0., 0.]));
        let hit = world.pick((50., 50.), (100., 100.)).expect("the square");
        assert!((hit.point.z + 1.).abs() < 1e-4, "{:?}", hit.point);
        let mesh = world.object(hit.object).unwrap();
        assert_eq!(mesh.material.color, [1., 0., 0., 0.5]);
        assert_eq!(
            (mesh.material.metallic, mesh.material.roughness),
            (0., 0.25)
        );
        // Made smooth: the square's normals face +z.
        let normal = mesh.geometry.unwrap().vertices()[0].normal;
        assert_eq!(normal, [0., 0., 1.]);
        // Under the model's group: moving it moves the square.
        world.set_transform(root, Transform::at([5., 0., 0.]));
        assert_eq!(world.pick((50., 50.), (100., 100.)), None);
    }

    #[test]
    fn a_gltf_reads_its_buffers_inline_or_beside_it() {
        let (mut doc, bin) = square(json!([{ "mesh": 0 }]), |_| {});
        let encoded: String = {
            const ABC: &[u8; 64] =
                b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
            bin.chunks(3)
                .flat_map(|c| {
                    let n = (u32::from(c[0]) << 16)
                        | (u32::from(*c.get(1).unwrap_or(&0)) << 8)
                        | u32::from(*c.get(2).unwrap_or(&0));
                    (0..4).map(move |i| {
                        if i > c.len() {
                            '='
                        } else {
                            ABC[(n >> (18 - 6 * i) & 63) as usize] as char
                        }
                    })
                })
                .collect()
        };
        doc["buffers"][0]["uri"] = json!(format!("data:application/octet-stream;base64,{encoded}"));
        let inline = Model::from_bytes(&serde_json::to_vec(&doc).unwrap()).unwrap();
        assert!(inline.bounds().is_some());
        doc["buffers"][0]["uri"] = json!("square.bin");
        let text = serde_json::to_vec(&doc).unwrap();
        assert!(
            Model::from_bytes(&text)
                .unwrap_err()
                .to_string()
                .contains("square.bin")
        );
        let beside =
            Model::from_bytes_with(&text, |uri| (uri == "square.bin").then(|| bin.clone()));
        assert_eq!(beside.unwrap().bounds(), inline.bounds());
    }

    #[test]
    fn a_matrix_is_taken_apart() {
        let (doc, bin) = square(
            // Turned a quarter about y, scaled 2, moved 3 along x.
            json!([{ "mesh": 0, "matrix": [0.0, 0.0, -2.0, 0.0, 0.0, 2.0, 0.0, 0.0, 2.0, 0.0, 0.0, 0.0, 3.0, 0.0, 0.0, 1.0] }]),
            |_| {},
        );
        let model = Model::from_bytes(&glb_of(&doc, &bin)).unwrap();
        let t = model.nodes[0].transform;
        assert!((t.position.x - 3.).abs() < 1e-5);
        assert!((t.scale - vec3(2., 2., 2.)).length() < 1e-5);
        let turned = t.rotation.rotate(vec3(1., 0., 0.));
        assert!((turned - vec3(0., 0., -1.)).length() < 1e-5, "{turned:?}");
        let (low, high) = model.bounds().unwrap();
        assert!(
            (low - vec3(3., -1., -1.)).length() < 1e-4 && (high - vec3(3., 1., 1.)).length() < 1e-4
        );
    }

    #[test]
    fn broken_files_say_what_is_wrong() {
        let (doc, bin) = square(json!([{ "mesh": 0 }]), |doc| {
            doc["accessors"][1]["count"] = json!(600);
        });
        let past = Model::from_bytes(&glb_of(&doc, &bin)).unwrap_err();
        assert!(past.to_string().contains("runs past"), "{past}");
        let (doc, bin) = square(json!([{ "mesh": 0, "children": [7] }]), |_| {});
        assert!(
            Model::from_bytes(&glb_of(&doc, &bin))
                .unwrap_err()
                .to_string()
                .contains("no node 7")
        );
        assert!(Model::from_bytes(b"glTF\x01\0\0\0").is_err());
        assert!(Model::from_bytes(b"not json").is_err());
        // A cycle draws nothing deeper, and ends.
        let (doc, bin) = square(json!([{ "mesh": 0, "children": [0] }]), |_| {});
        let model = Model::from_bytes(&glb_of(&doc, &bin)).unwrap();
        World::new().add_model(&model);
        assert!(model.bounds().is_some());
    }

    /// A 2 by 1 PNG: red, then blue.
    const RED_BLUE: &str = "iVBORw0KGgoAAAANSUhEUgAAAAIAAAABCAYAAAD0In+KAAAADklEQVR4nGP4z8AAQv8BD/kD/YURmXYAAAAASUVORK5CYII=";

    #[test]
    fn a_base_colour_texture_comes_with_its_mesh() {
        let (doc, bin) = square(json!([{ "mesh": 0 }]), |doc| {
            doc["images"] = json!([
                { "uri": format!("data:image/png;base64,{RED_BLUE}") },
                { "uri": "data:image/png;base64,bm90IGEgcG5n" },
            ]);
            doc["textures"] = json!([{ "source": 0 }, { "source": 1 }]);
            doc["materials"][0]["pbrMetallicRoughness"]["baseColorTexture"] = json!({ "index": 0 });
        });
        let model = Model::from_bytes(&glb_of(&doc, &bin)).unwrap();
        let texture = model.nodes[0].meshes[0].2[0].clone().expect("its texture");
        assert_eq!((texture.width(), texture.height()), (2, 1));
        assert_eq!(
            texture.pixels().unwrap().pixels(),
            [255, 0, 0, 255, 0, 0, 255, 255]
        );
        let world = World::new();
        world.add_model(&model);
        world.set_camera(crate::Camera::looking_at([0., 0., 3.], [0., 0., 0.]));
        let hit = world.pick((50., 50.), (100., 100.)).unwrap();
        assert_eq!(world.texture(hit.object), Some(texture));
        // An image that doesn't decode leaves the mesh its colour alone.
        let (doc, bin) = square(json!([{ "mesh": 0 }]), |doc| {
            doc["images"] = json!([{ "uri": "data:image/png;base64,bm90IGEgcG5n" }]);
            doc["textures"] = json!([{ "source": 0 }]);
            doc["materials"][0]["pbrMetallicRoughness"]["baseColorTexture"] = json!({ "index": 0 });
        });
        let model = Model::from_bytes(&glb_of(&doc, &bin)).unwrap();
        assert!(model.nodes[0].meshes[0].2[0].is_none());
    }

    #[test]
    fn a_clip_moves_its_node_when_posed() {
        // The square's node slid from 0 to 2 along x over a second.
        let (doc, mut bin) = square(json!([{ "mesh": 0 }]), |_| {});
        let at = bin.len();
        let times: [f32; 2] = [0., 1.];
        let places: [f32; 6] = [0., 0., 0., 2., 0., 0.];
        bin.extend(bytemuck::cast_slice(&times));
        bin.extend(bytemuck::cast_slice(&places));
        let mut doc = doc;
        doc["buffers"][0]["byteLength"] = json!(bin.len());
        let views = doc["bufferViews"].as_array_mut().unwrap();
        views.push(json!({ "buffer": 0, "byteOffset": at, "byteLength": 8 }));
        views.push(json!({ "buffer": 0, "byteOffset": at + 8, "byteLength": 24 }));
        let accessors = doc["accessors"].as_array_mut().unwrap();
        accessors
            .push(json!({ "bufferView": 2, "componentType": 5126, "count": 2, "type": "SCALAR" }));
        accessors
            .push(json!({ "bufferView": 3, "componentType": 5126, "count": 2, "type": "VEC3" }));
        doc["animations"] = json!([{
            "name": "slide",
            "samplers": [{ "input": 2, "output": 3 }],
            "channels": [{ "sampler": 0, "target": { "node": 0, "path": "translation" } }],
        }]);
        let model = Model::from_bytes(&glb_of(&doc, &bin)).unwrap();
        let clip = model.clip("slide").expect("the clip");
        assert_eq!((clip.duration(), model.clips().len()), (1., 1));
        let world = World::new();
        let placed = world.place_model(&model);
        let node = placed.nodes[0].unwrap();
        placed.pose(&world, clip, 0.5);
        assert_eq!(
            world.object(node).unwrap().transform.position,
            vec3(1., 0., 0.)
        );
        let revision = world.revision();
        placed.pose(&world, clip, 0.5);
        assert_eq!(
            world.revision(),
            revision,
            "posed the same: nothing to draw"
        );
        placed.pose(&world, clip, 7.);
        assert_eq!(
            world.object(node).unwrap().transform.position,
            vec3(2., 0., 0.)
        );
    }

    #[test]
    fn a_skinned_mesh_is_placed_by_its_joints() {
        // The square's every vertex bound wholly to its skin's second joint,
        // 5 along x: the mesh shows there, whatever its own node's place.
        let (doc, mut bin) = square(
            json!([
                { "mesh": 0, "skin": 0, "translation": [0.0, 9.0, 0.0], "children": [1, 2] },
                {},
                { "translation": [5.0, 0.0, 0.0] },
            ]),
            |_| {},
        );
        let at = bin.len();
        bin.extend([1_u8, 0, 0, 0].repeat(4));
        bin.extend(bytemuck::cast_slice(&[[1_f32, 0., 0., 0.]; 4]));
        let mut doc = doc;
        doc["buffers"][0]["byteLength"] = json!(bin.len());
        let views = doc["bufferViews"].as_array_mut().unwrap();
        views.push(json!({ "buffer": 0, "byteOffset": at, "byteLength": 16 }));
        views.push(json!({ "buffer": 0, "byteOffset": at + 16, "byteLength": 64 }));
        let accessors = doc["accessors"].as_array_mut().unwrap();
        accessors
            .push(json!({ "bufferView": 2, "componentType": 5121, "count": 4, "type": "VEC4" }));
        accessors
            .push(json!({ "bufferView": 3, "componentType": 5126, "count": 4, "type": "VEC4" }));
        let attributes = &mut doc["meshes"][0]["primitives"][0]["attributes"];
        attributes["JOINTS_0"] = json!(2);
        attributes["WEIGHTS_0"] = json!(3);
        // Its joints moved with the node: placed by the node's 9 up too.
        doc["skins"] = json!([{ "joints": [1, 2] }]);
        let model = Model::from_bytes(&glb_of(&doc, &bin)).unwrap();
        let world = World::new();
        let placed = world.place_model(&model);
        world.set_camera(crate::Camera::looking_at([5., 9., 3.], [5., 9., 0.]));
        let hit = world
            .pick((50., 50.), (100., 100.))
            .expect("the square, at its joint");
        assert!(
            (hit.point - vec3(5., 9., 0.)).length() < 1e-3,
            "{:?}",
            hit.point
        );
        // The joint moved back: the mesh with it.
        world.set_transform(placed.nodes[2].unwrap(), Transform::default());
        assert_eq!(world.pick((50., 50.), (100., 100.)), None);
        // A skin naming a node that isn't one is an error.
        let (doc, bin) = square(json!([{ "mesh": 0, "skin": 0 }]), |doc| {
            doc["skins"] = json!([{ "joints": [4] }]);
        });
        assert!(Model::from_bytes(&glb_of(&doc, &bin)).is_err());
    }

    #[test]
    fn a_material_says_how_its_faces_show() {
        let shown = |extra: Value| {
            let (doc, bin) = square(json!([{ "mesh": 0 }]), |doc| {
                for (key, value) in extra.as_object().unwrap() {
                    doc["materials"][0][key] = value.clone();
                }
            });
            let model = Model::from_bytes(&glb_of(&doc, &bin)).unwrap();
            model.nodes[0].meshes[0].3
        };
        assert_eq!(shown(json!({})), Surface::default());
        assert!(shown(json!({ "doubleSided": true })).double_sided);
        assert_eq!(
            shown(json!({ "alphaMode": "MASK" })).alpha_cutoff,
            Some(0.5 / 0.5),
            "glTF's default cutoff, its colour factor's alpha (0.5) taken out"
        );
        assert_eq!(
            shown(json!({ "alphaMode": "MASK", "alphaCutoff": 0.25 })).alpha_cutoff,
            Some(0.5)
        );
        assert_eq!(
            shown(json!({ "alphaMode": "BLEND", "alphaCutoff": 0.25 })).alpha_cutoff,
            None,
            "a cutoff means nothing but with MASK"
        );
    }

    #[test]
    fn a_blended_material_with_alpha_in_its_texture_alone_is_translucent() {
        let alpha = |blend: bool, textured: bool| {
            let (doc, bin) = square(json!([{ "mesh": 0 }]), |doc| {
                let material = &mut doc["materials"][0];
                material["pbrMetallicRoughness"]["baseColorFactor"] = json!([1.0, 1.0, 1.0, 1.0]);
                material["alphaMode"] = json!(if blend { "BLEND" } else { "OPAQUE" });
                if textured {
                    material["pbrMetallicRoughness"]["baseColorTexture"] = json!({ "index": 0 });
                    doc["textures"] = json!([{ "source": 0 }]);
                    doc["images"] = json!([{ "uri": format!("data:image/png;base64,{RED_BLUE}") }]);
                }
            });
            let model = Model::from_bytes(&glb_of(&doc, &bin)).unwrap();
            model.nodes[0].meshes[0].1.color[3]
        };
        assert!(alpha(true, true) < 1., "blended, by its texture");
        assert_eq!(alpha(true, false), 1., "no alpha anywhere: opaque");
        assert_eq!(
            alpha(false, true),
            1.,
            "not blended: its texture's alpha unread"
        );
    }

    #[test]
    fn compressed_geometry_it_needs_is_an_error_not_an_empty_model() {
        let (mut doc, bin) = square(json!([{ "mesh": 0 }]), |_| {});
        doc["extensionsRequired"] = json!(["KHR_draco_mesh_compression"]);
        let error = Model::from_bytes(&glb_of(&doc, &bin)).unwrap_err();
        assert!(
            error.to_string().contains("KHR_draco_mesh_compression"),
            "{error}"
        );
        // Used, not required: its plain data is there, and read.
        doc["extensionsUsed"] = doc["extensionsRequired"].take();
        doc["extensionsRequired"] = json!([]);
        assert!(Model::from_bytes(&glb_of(&doc, &bin)).is_ok());
        // One changing only how it looks still loads.
        doc["extensionsRequired"] = json!(["KHR_materials_sheen"]);
        assert!(Model::from_bytes(&glb_of(&doc, &bin)).is_ok());
    }

    /// `square`'s document and buffer with one morph target moving it 2
    /// along x, bound to its primitive.
    fn morphing_square(nodes: Value, extra: impl FnOnce(&mut Value)) -> (Value, Vec<u8>) {
        let (mut doc, mut bin) = square(nodes, |_| {});
        let at = bin.len();
        bin.extend(bytemuck::cast_slice(&[[2_f32, 0., 0.]; 4]));
        doc["buffers"][0]["byteLength"] = json!(bin.len());
        doc["bufferViews"]
            .as_array_mut()
            .unwrap()
            .push(json!({ "buffer": 0, "byteOffset": at, "byteLength": 48 }));
        doc["accessors"]
            .as_array_mut()
            .unwrap()
            .push(json!({ "bufferView": 2, "componentType": 5126, "count": 4, "type": "VEC3" }));
        doc["meshes"][0]["primitives"][0]["targets"] = json!([{ "POSITION": 2 }]);
        extra(&mut doc);
        (doc, bin)
    }

    #[test]
    fn a_morph_target_comes_with_its_weights() {
        let picked_at = |doc: &Value, bin: &[u8]| {
            let model = Model::from_bytes(&glb_of(doc, bin)).unwrap();
            let world = World::new();
            world.add_model(&model);
            world.set_camera(crate::Camera::looking_at([2., 0., 3.], [2., 0., 0.]));
            world.pick((50., 50.), (100., 100.)).is_some()
        };
        // At rest the square is about the origin, out of a view of x = 2.
        let (doc, bin) = morphing_square(json!([{ "mesh": 0 }]), |_| {});
        assert!(!picked_at(&doc, &bin));
        // The mesh's weights, and a node's in their place.
        let (doc, bin) = morphing_square(json!([{ "mesh": 0 }]), |doc| {
            doc["meshes"][0]["weights"] = json!([1.0]);
        });
        assert!(picked_at(&doc, &bin));
        let (doc, bin) = morphing_square(json!([{ "mesh": 0, "weights": [0.0] }]), |doc| {
            doc["meshes"][0]["weights"] = json!([1.0]);
        });
        assert!(!picked_at(&doc, &bin));
    }

    #[test]
    fn a_clip_moves_morph_weights() {
        // Its one target's weight, from 0 to 1 over a second.
        let (mut doc, mut bin) = morphing_square(json!([{ "mesh": 0 }]), |_| {});
        let at = bin.len();
        bin.extend(bytemuck::cast_slice(&[0_f32, 1., 0., 1.]));
        doc["buffers"][0]["byteLength"] = json!(bin.len());
        let views = doc["bufferViews"].as_array_mut().unwrap();
        views.push(json!({ "buffer": 0, "byteOffset": at, "byteLength": 8 }));
        views.push(json!({ "buffer": 0, "byteOffset": at + 8, "byteLength": 8 }));
        let accessors = doc["accessors"].as_array_mut().unwrap();
        for view in [3, 4] {
            accessors.push(
                json!({ "bufferView": view, "componentType": 5126, "count": 2, "type": "SCALAR" }),
            );
        }
        doc["animations"] = json!([{
            "name": "grow",
            "samplers": [{ "input": 3, "output": 4 }],
            "channels": [{ "sampler": 0, "target": { "node": 0, "path": "weights" } }],
        }]);
        let model = Model::from_bytes(&glb_of(&doc, &bin)).unwrap();
        let clip = model.clip("grow").expect("the clip");
        assert_eq!(clip.duration(), 1.);
        let world = World::new();
        let placed = world.place_model(&model);
        world.set_camera(crate::Camera::looking_at([2., 0., 3.], [2., 0., 0.]));
        placed.pose(&world, clip, 0.);
        assert!(world.pick((50., 50.), (100., 100.)).is_none(), "at rest");
        placed.pose(&world, clip, 1.);
        assert!(world.pick((50., 50.), (100., 100.)).is_some(), "at full");
        let revision = world.revision();
        placed.pose(&world, clip, 1.);
        assert_eq!(
            world.revision(),
            revision,
            "posed the same: nothing to draw"
        );
    }

    /// Bytes appended to `doc`'s buffer as a new buffer view: its index.
    fn add_view(doc: &mut Value, bin: &mut Vec<u8>, bytes: &[u8]) -> usize {
        let at = bin.len();
        bin.extend(bytes);
        while !bin.len().is_multiple_of(4) {
            bin.push(0);
        }
        doc["buffers"][0]["byteLength"] = json!(bin.len());
        let views = doc["bufferViews"].as_array_mut().unwrap();
        views.push(json!({ "buffer": 0, "byteOffset": at, "byteLength": bytes.len() }));
        views.len() - 1
    }

    #[test]
    fn a_maps_placement_is_read_from_its_texture_and_sampler() {
        let (doc, bin) = square(json!([{ "mesh": 0 }]), |doc| {
            doc["samplers"] = json!([{ "wrapS": 33071, "wrapT": 33648 }, { "wrapS": 10497 }]);
            doc["textures"] = json!([{ "sampler": 0 }, { "sampler": 1 }, {}]);
            doc["materials"][0]["pbrMetallicRoughness"]["baseColorTexture"] = json!({
                "index": 0,
                "texCoord": 1,
                "extensions": { "KHR_texture_transform": {
                    "offset": [0.25, 0.5],
                    "rotation": 1.5,
                    "scale": [2.0, 3.0],
                }},
            });
            // A transform's coordinates are those it says, over the map's.
            doc["materials"][0]["normalTexture"] = json!({
                "index": 2,
                "texCoord": 1,
                "extensions": { "KHR_texture_transform": { "texCoord": 0 } },
            });
            doc["materials"][0]["emissiveTexture"] = json!({ "index": 1 });
        });
        let model = Model::from_bytes(&glb_of(&doc, &bin)).unwrap();
        let [base, _, normal, emission, _] = model.nodes[0].meshes[0].4;
        assert_eq!(
            base,
            MapPlacement::new()
                .offset(0.25, 0.5)
                .rotated(1.5)
                .scaled(2., 3.)
                .uv_set(1)
                .wrap(Wrap::Clamp, Wrap::Mirror)
        );
        assert_eq!(normal, MapPlacement::new());
        assert_eq!(emission, MapPlacement::new(), "a repeat is no placement");
        let world = World::new();
        let placed = world.place_model(&model);
        let mesh = placed.meshes[0][0];
        assert_eq!(
            world.map_placement(mesh, crate::Map::BaseColour),
            Some(base)
        );
        assert_eq!(world.map_placement(mesh, crate::Map::Emission), None);
    }

    #[test]
    fn second_coordinates_are_read_for_a_map_that_uses_them() {
        let with_second_uvs = |extra: fn(&mut Value)| {
            let (mut doc, mut bin) = square(json!([{ "mesh": 0 }]), |_| {});
            let uvs = [[0_f32, 0.], [1., 0.], [1., 1.], [0., 1.]];
            let view = add_view(&mut doc, &mut bin, bytemuck::cast_slice(&uvs));
            doc["accessors"].as_array_mut().unwrap().push(
                json!({ "bufferView": view, "componentType": 5126, "count": 4, "type": "VEC2" }),
            );
            let accessor = doc["accessors"].as_array().unwrap().len() - 1;
            doc["meshes"][0]["primitives"][0]["attributes"]["TEXCOORD_1"] = json!(accessor);
            doc["textures"] = json!([{}]);
            extra(&mut doc);
            Model::from_bytes(&glb_of(&doc, &bin)).unwrap()
        };
        let model = with_second_uvs(|doc| {
            doc["materials"][0]["occlusionTexture"] = json!({ "index": 0, "texCoord": 1 });
        });
        let uvs = model.nodes[0].meshes[0].0.second_uvs().unwrap();
        assert_eq!(uvs[2], [1., 1.]);
        // Unused, as a lightmap's often is, they're left out.
        let model = with_second_uvs(|doc| {
            doc["materials"][0]["occlusionTexture"] = json!({ "index": 0 });
        });
        assert!(model.nodes[0].meshes[0].0.second_uvs().is_none());
    }

    #[test]
    fn vertex_colours_are_read_in_any_form_and_blend_by_their_alpha() {
        // `bytes` of `kind` as `COLOR_0`, in a material of `extra`.
        fn with_colors(
            bytes: &[u8],
            kind: u64,
            shape: &str,
            normalized: bool,
            extra: fn(&mut Value),
        ) -> Node {
            let (mut doc, mut bin) = square(json!([{ "mesh": 0 }]), extra);
            let view = add_view(&mut doc, &mut bin, bytes);
            doc["accessors"].as_array_mut().unwrap().push(json!({
                "bufferView": view, "componentType": kind, "count": 4, "type": shape, "normalized": normalized,
            }));
            let accessor = doc["accessors"].as_array().unwrap().len() - 1;
            doc["meshes"][0]["primitives"][0]["attributes"]["COLOR_0"] = json!(accessor);
            let model = Model::from_bytes(&glb_of(&doc, &bin)).unwrap();
            model.nodes[0].clone()
        }
        // Bytes, normalized, without alpha: alpha 1.
        let bytes = [255, 0, 51, 0, 255, 0, 0, 0, 255, 255, 255, 255];
        let node = with_colors(&bytes, 5121, "VEC3", true, |_| {});
        assert_eq!(
            node.meshes[0].0.colors().unwrap()[0],
            [1., 0., 0.2, 1.],
            "scaled, and opaque"
        );
        // Floats with alpha, in a blending material: drawn translucent.
        let floats: [[f32; 4]; 4] = [[1., 0.5, 0.25, 0.5]; 4];
        let blend = |doc: &mut Value| {
            doc["materials"][0]["alphaMode"] = json!("BLEND");
            doc["materials"][0]["pbrMetallicRoughness"]["baseColorFactor"] =
                json!([1.0, 1.0, 1.0, 1.0]);
        };
        let node = with_colors(bytemuck::cast_slice(&floats), 5126, "VEC4", false, blend);
        let (geometry, material, ..) = &node.meshes[0];
        assert_eq!(geometry.colors().unwrap()[1], [1., 0.5, 0.25, 0.5]);
        assert!(material.color[3] < 1., "its vertices see through");
        // Solid vertices in a blending material stay as they were.
        let node = with_colors(
            bytemuck::cast_slice(&[[1_f32; 4]; 4]),
            5126,
            "VEC4",
            false,
            blend,
        );
        assert_eq!(node.meshes[0].1.color[3], 1.);
    }

    #[test]
    fn a_sparse_accessor_replaces_what_it_lists() {
        // The square's positions, its corner 2 moved up to y = 3, by each
        // size of sparse index.
        let index_bytes: [(u64, Vec<u8>); 3] = [
            (5121, vec![2]),
            (5123, 2_u16.to_le_bytes().to_vec()),
            (5125, 2_u32.to_le_bytes().to_vec()),
        ];
        for (kind, bytes) in index_bytes {
            let (mut doc, mut bin) = square(json!([{ "mesh": 0 }]), |_| {});
            let indices = add_view(&mut doc, &mut bin, &bytes);
            let values = add_view(&mut doc, &mut bin, bytemuck::cast_slice(&[0.5_f32, 3., 0.]));
            doc["accessors"][0]["sparse"] = json!({
                "count": 1,
                "indices": { "bufferView": indices, "componentType": kind },
                "values": { "bufferView": values },
            });
            let model = Model::from_bytes(&glb_of(&doc, &bin)).unwrap();
            let v = model.nodes[0].meshes[0].0.vertices();
            assert_eq!(v[2].position, [0.5, 3., 0.], "index type {kind}");
            assert_eq!(v[0].position, [-0.5, -0.5, 0.], "the rest as they were");
        }
    }

    #[test]
    fn a_sparse_accessor_with_no_view_is_zeros_but_for_what_it_lists() {
        // As Blender exports a morph target: no base, three of the four
        // corners moved 2 along x.
        let (mut doc, mut bin) = square(json!([{ "mesh": 0, "weights": [1.0] }]), |_| {});
        let indices = add_view(&mut doc, &mut bin, bytemuck::cast_slice(&[0_u16, 1, 3]));
        let values = add_view(
            &mut doc,
            &mut bin,
            bytemuck::cast_slice(&[[2_f32, 0., 0.]; 3]),
        );
        doc["accessors"].as_array_mut().unwrap().push(json!({
            "componentType": 5126, "count": 4, "type": "VEC3",
            "sparse": {
                "count": 3,
                "indices": { "bufferView": indices, "componentType": 5123 },
                "values": { "bufferView": values },
            },
        }));
        doc["meshes"][0]["primitives"][0]["targets"] = json!([{ "POSITION": 2 }]);
        let model = Model::from_bytes(&glb_of(&doc, &bin)).unwrap();
        let geometry = &model.nodes[0].meshes[0].0;
        let morphs = geometry.morphs().expect("its target");
        let deltas = morphs.deltas();
        assert_eq!(deltas[morphs.at(1, 0)], [2., 0., 0., 0.]);
        assert_eq!(deltas[morphs.at(2, 0)], [0.; 4], "not listed: unmoved");
    }

    #[test]
    fn a_clip_reads_a_sparse_output() {
        // Three keys at x 0, then 4 and 8 by sparse values over zeros.
        let (mut doc, mut bin) = square(json!([{ "mesh": 0 }]), |_| {});
        let times = add_view(&mut doc, &mut bin, bytemuck::cast_slice(&[0_f32, 1., 2.]));
        let indices = add_view(&mut doc, &mut bin, &[1, 2]);
        let values = add_view(
            &mut doc,
            &mut bin,
            bytemuck::cast_slice(&[4_f32, 0., 0., 8., 0., 0.]),
        );
        let accessors = doc["accessors"].as_array_mut().unwrap();
        accessors.push(
            json!({ "bufferView": times, "componentType": 5126, "count": 3, "type": "SCALAR" }),
        );
        accessors.push(json!({
            "componentType": 5126, "count": 3, "type": "VEC3",
            "sparse": {
                "count": 2,
                "indices": { "bufferView": indices, "componentType": 5121 },
                "values": { "bufferView": values },
            },
        }));
        doc["animations"] = json!([{
            "name": "slide",
            "samplers": [{ "input": 2, "output": 3 }],
            "channels": [{ "sampler": 0, "target": { "node": 0, "path": "translation" } }],
        }]);
        let model = Model::from_bytes(&glb_of(&doc, &bin)).unwrap();
        let world = World::new();
        let placed = world.place_model(&model);
        placed.pose(&world, model.clip("slide").unwrap(), 2.);
        assert_eq!(
            world
                .object(placed.nodes[0].unwrap())
                .unwrap()
                .transform
                .position,
            vec3(8., 0., 0.)
        );
    }

    #[test]
    fn sparse_indices_are_read_as_indices() {
        // The triangles' last index replaced: the second triangle changes.
        let (mut doc, mut bin) = square(json!([{ "mesh": 0 }]), |_| {});
        let indices = add_view(&mut doc, &mut bin, &[5]);
        let values = add_view(&mut doc, &mut bin, bytemuck::cast_slice(&[1_u16]));
        doc["accessors"][1]["sparse"] = json!({
            "count": 1,
            "indices": { "bufferView": indices, "componentType": 5121 },
            "values": { "bufferView": values },
        });
        let model = Model::from_bytes(&glb_of(&doc, &bin)).unwrap();
        assert_eq!(model.nodes[0].meshes[0].0.indices(), [0, 1, 2, 0, 2, 1]);
    }

    #[test]
    fn broken_sparse_accessors_say_what_is_wrong() {
        let broken = |sparse: Value| {
            let (mut doc, mut bin) = square(json!([{ "mesh": 0 }]), |_| {});
            let view = add_view(&mut doc, &mut bin, &[9; 12]);
            let mut sparse = sparse;
            for part in ["indices", "values"] {
                if sparse.get(part).is_some() {
                    sparse[part]["bufferView"] = json!(view);
                }
            }
            doc["accessors"][0]["sparse"] = sparse;
            Model::from_bytes(&glb_of(&doc, &bin))
                .unwrap_err()
                .to_string()
        };
        let indices = json!({ "componentType": 5121 });
        let values = json!({});
        // An index past the accessor: 9 of 4.
        let past = broken(json!({ "count": 1, "indices": indices, "values": values }));
        assert!(past.contains("past it"), "{past}");
        // More than the view holds, and more than the accessor holds.
        let long = broken(json!({ "count": 4, "indices": indices, "values": values }));
        assert!(long.contains("past"), "{long}");
        let more = broken(json!({ "count": 5, "indices": indices, "values": values }));
        assert!(more.contains("more than it holds"), "{more}");
        let missing = broken(json!({ "count": 1, "indices": indices }));
        assert!(missing.contains("incomplete"), "{missing}");
        let kind =
            broken(json!({ "count": 1, "indices": { "componentType": 5126 }, "values": values }));
        assert!(kind.contains("sparse indices"), "{kind}");
        // A file can't ask for more zeros than memory.
        let (mut doc, bin) = square(json!([{ "mesh": 0 }]), |_| {});
        doc["accessors"]
            .as_array_mut()
            .unwrap()
            .push(json!({ "componentType": 5126, "count": 1_u64 << 40, "type": "VEC3" }));
        doc["meshes"][0]["primitives"][0]["attributes"]["POSITION"] = json!(2);
        let huge = Model::from_bytes(&glb_of(&doc, &bin))
            .unwrap_err()
            .to_string();
        assert!(huge.contains("too many zeros"), "{huge}");
    }

    #[test]
    fn base64_reads_padded_and_url_safe_text() {
        assert_eq!(base64("aGVsbG8=").unwrap(), b"hello");
        assert_eq!(base64("aGVsbG8").unwrap(), b"hello");
        assert_eq!(base64("_-8").unwrap(), [0xff, 0xef]);
        assert_eq!(base64("a*b"), None);
    }
}
