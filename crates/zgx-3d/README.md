# zgx-3d

Retained 3D scenes for [zgx](https://github.com/zeronsh/zgx): a `World` of
meshes (primitives or your own triangles) in metallic-roughness materials,
lights (a sun, point lights, a sky hemisphere) and a camera, shown in any
zgx box through a GPU view (`zgx::compose::gpu_view(label, world.view())`).

A world draws a frame only when it changes: a still scene costs nothing, and
its pipelines and buffers are made with its first frame. Each geometry's
meshes draw in one instanced call, with depth and 4x multisampling.

```sh
cargo run --release -p zgx-3d --example scene
```

What a world takes beyond meshes and lights, each costing nothing until used:

- **Textures**: `Texture` from RGBA8 pixels or a PNG or JPEG, and
  `World::set_texture(id, texture)`; normal, metal-roughness, emission and
  occlusion maps with `World::set_map(id, map, texture)`.
- **glTF models** (the `gltf` feature): `Model::from_bytes` (a `.glb`, or a
  `.gltf` with inline buffers), `Model::from_bytes_with` (buffers beside it),
  and `World::add_model`, added as one group with its textures. Its clips
  (`Model::clips`) move its nodes: `World::place_model` keeps them, and
  `Placed::pose(world, clip, seconds)` poses one at any time.
  `cargo run -p zgx-3d --features gltf --example model -- FILE`.
- **Surfaces**: `World::set_double_sided(id, true)` draws a mesh's back faces
  too, and `World::set_alpha_cutoff(id, Some(0.5))` cuts it out by its base
  colour texture's alpha, in its shadow as well.
- **Map placement**: `World::set_map_placement(id, map, Some(placement))` moves,
  scales and turns a map on its mesh (`MapPlacement`), says how it repeats
  (`Wrap`) and which coordinates it reads (`Geometry::with_second_uvs`). glTF
  models bring theirs: `KHR_texture_transform`, samplers' wrapping, `TEXCOORD_1`.
- **Vertex colours**: `Geometry::with_colors(colors)` colours a mesh at each
  vertex, multiplying its material and base colour map; glTF models bring
  theirs (`COLOR_0`).
- **Skins**: `Geometry::with_skin(joints, weights)` and
  `World::set_skin(id, Some(skin))` bend a mesh by its joints on the GPU, its
  shadow and picking following the pose; glTF models bring theirs.
- **Morph targets**: `Geometry::with_morph_targets(targets)` and
  `World::set_morph_weights(id, weights)` blend a mesh toward other shapes on
  the GPU, its shadow and picking following; glTF models bring theirs, and
  their clips move the weights.
- **Picking**: `World::pick(point, size)` gives the mesh a point of the view
  shows, where on it and how far (`Hit`), from the meshes' triangles on the
  CPU.
- **Shadows**: `World::set_shadows(true)` has the sun (the first directional
  light) cast soft shadows from opaque meshes, into a 2048 by 2048 depth map.
- **Image-based light**: `World::set_environment(Some(Environment::studio()))`
  lights every surface from all around, its reflections sharp in polished
  metal and blurred in rough. `Environment::decode_hdr` takes a Radiance
  `.hdr` image instead, `from_equirect` and `from_cube` linear RGB; each is
  prefiltered once on the CPU (a 128-texel cube at five roughnesses, nine
  spherical harmonics for its diffuse light).

In Luau, `ui.scene3d`, `ui.mesh`, `ui.group` and `ui.model` show them
([3D scenes](../../docs/reference/3d.md)).
