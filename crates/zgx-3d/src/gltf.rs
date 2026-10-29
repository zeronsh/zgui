//! glTF 2.0 models (feature `gltf`): a `.glb`, or a `.gltf` with its
//! buffers inline (`data:` URIs) or beside it, read into a [`Model`] that a
//! world adds as a group ([`World::add_model`]).
//!
//! What it reads: the default scene's node tree (each node's matrix or
//! translation, rotation and scale), its meshes' triangles (positions,
//! normals, made smooth where a mesh has none, and texture coordinates, of
//! any of glTF's component types), and their metallic-roughness materials'
//! factors: colour, metal, roughness, emission, blending (`BLEND`) and
//! `KHR_materials_unlit`. Not yet: textures, skins, morph targets,
//! animations, sparse accessors, points and lines (skipped).
use crate::{Geometry, Material, Object, ObjectId, Quat, Transform, Vec3, Vertex, World, vec3};
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
}
#[derive(Clone, Debug)]
struct Node {
    transform: Transform,
    children: Vec<usize>,
    meshes: Vec<(Geometry, Material)>,
}

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
        let materials = list(&doc, "materials")
            .iter()
            .map(material)
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
                Ok(Node {
                    transform: transform(node),
                    children,
                    meshes,
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
        Ok(Self { nodes, roots })
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
            for (geometry, _) in &node.meshes {
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
        let root = self.add(Object::group());
        let mut stack: Vec<(usize, ObjectId, usize)> =
            model.roots.iter().rev().map(|&r| (r, root, 0)).collect();
        while let Some((index, parent, depth)) = stack.pop() {
            if depth > MAX_DEPTH {
                continue;
            }
            let node = &model.nodes[index];
            let id = self.add(Object::group().transformed(node.transform).child_of(parent));
            for (geometry, material) in &node.meshes {
                self.add(Object::mesh(geometry.clone(), *material).child_of(id));
            }
            stack.extend(node.children.iter().rev().map(|&c| (c, id, depth + 1)));
        }
        root
    }
}

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

/// A material's factors (glTF's are in linear light, as zgx-3d's). Where
/// a texture (not read yet) would scale a factor, its factor alone would
/// mislead (a textured helmet's factors of 1 make a glowing mirror): a
/// textured emission is none, and a textured metalness a dielectric.
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
    }
    let textured = |owner: Option<&Value>, key: &str| owner.is_some_and(|o| o.get(key).is_some());
    let metal_textured = textured(pbr, "metallicRoughnessTexture");
    Material {
        color,
        metallic: if metal_textured {
            0.
        } else {
            number("metallicFactor", 1.).clamp(0., 1.)
        },
        roughness: if metal_textured {
            0.5
        } else {
            number("roughnessFactor", 1.).clamp(0., 1.)
        },
        emissive: if textured(Some(value), "emissiveTexture") {
            [0.; 3]
        } else {
            floats::<3>(value.get("emissiveFactor")).unwrap_or([0.; 3])
        },
        unlit: value
            .get("extensions")
            .is_some_and(|e| e.get("KHR_materials_unlit").is_some()),
    }
}

struct Reader<'a> {
    doc: &'a Value,
    buffers: &'a [Vec<u8>],
}
impl Reader<'_> {
    /// A mesh's triangle primitives as geometries in their materials.
    fn mesh(
        &self,
        mesh: &Value,
        materials: &[Material],
    ) -> Result<Vec<(Geometry, Material)>, ModelError> {
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
            let material = primitive
                .get("material")
                .and_then(Value::as_u64)
                .and_then(|m| materials.get(m as usize).copied())
                .unwrap_or_else(|| Material {
                    color: [1.; 4],
                    metallic: 1.,
                    roughness: 1.,
                    ..Material::default()
                });
            let geometry = Geometry::new(vertices, indices).map_err(|e| ModelError(e.into()))?;
            out.push((geometry, material));
        }
        Ok(out)
    }
    /// Accessor `index`'s view, its bytes from its start, its element's
    /// stride and size, and its count and component type.
    fn view(&self, index: usize, components: usize) -> Result<Accessor<'_>, ModelError> {
        let Some(accessor) = list(self.doc, "accessors").get(index) else {
            return error(format!("no accessor {index}"));
        };
        if accessor.get("sparse").is_some() {
            return error("sparse accessors are not read yet");
        }
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
            // No view: all zeros.
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
    /// Accessor `index`'s `components` numbers an element, as floats.
    fn floats(&self, index: usize, components: usize) -> Result<Vec<f32>, ModelError> {
        let a = self.view(index, components)?;
        let mut out = Vec::with_capacity(a.count * components);
        if a.bytes.is_empty() {
            out.resize(a.count * components, 0.);
            return Ok(out);
        }
        let size = match a.kind {
            5120 | 5121 => 1,
            5122 | 5123 => 2,
            _ => 4,
        };
        for i in 0..a.count {
            for c in 0..components {
                let at = i * a.stride + c * size;
                let b = &a.bytes[at..at + size];
                let v = match (a.kind, a.normalized) {
                    (5126, _) => f32::from_le_bytes(b.try_into().unwrap()),
                    (5121, true) => f32::from(b[0]) / 255.,
                    (5121, false) => f32::from(b[0]),
                    (5120, true) => (f32::from(b[0] as i8) / 127.).max(-1.),
                    (5120, false) => f32::from(b[0] as i8),
                    (5123, true) => f32::from(u16::from_le_bytes([b[0], b[1]])) / 65535.,
                    (5123, false) => f32::from(u16::from_le_bytes([b[0], b[1]])),
                    (5122, true) => (f32::from(i16::from_le_bytes([b[0], b[1]])) / 32767.).max(-1.),
                    (5122, false) => f32::from(i16::from_le_bytes([b[0], b[1]])),
                    _ => u32::from_le_bytes(b.try_into().unwrap()) as f32,
                };
                out.push(if v.is_finite() { v } else { 0. });
            }
        }
        Ok(out)
    }
    /// Accessor `index` as indices (unsigned bytes, shorts or ints).
    fn indices(&self, index: usize) -> Result<Vec<u32>, ModelError> {
        let a = self.view(index, 1)?;
        if a.bytes.is_empty() {
            return Ok(vec![0; a.count]);
        }
        (0..a.count)
            .map(|i| {
                let b = &a.bytes[i * a.stride..];
                match a.kind {
                    5121 => Ok(u32::from(b[0])),
                    5123 => Ok(u32::from(u16::from_le_bytes([b[0], b[1]]))),
                    5125 => Ok(u32::from_le_bytes([b[0], b[1], b[2], b[3]])),
                    kind => error(format!("indices of component type {kind}")),
                }
            })
            .collect()
    }
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

    #[test]
    fn base64_reads_padded_and_url_safe_text() {
        assert_eq!(base64("aGVsbG8=").unwrap(), b"hello");
        assert_eq!(base64("aGVsbG8").unwrap(), b"hello");
        assert_eq!(base64("_-8").unwrap(), [0xff, 0xef]);
        assert_eq!(base64("a*b"), None);
    }
}
