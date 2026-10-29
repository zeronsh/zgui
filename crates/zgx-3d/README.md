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
  `World::set_texture(id, texture)`; normal and metal-roughness maps with
  `World::set_map(id, Map::Normal | Map::MetalRoughness, texture)`.
- **glTF models** (the `gltf` feature): `Model::from_bytes` (a `.glb`, or a
  `.gltf` with inline buffers), `Model::from_bytes_with` (buffers beside it),
  and `World::add_model`, added as one group with its base colour,
  metal-roughness and normal textures.
  `cargo run -p zgx-3d --features gltf --example model -- FILE`.
- **Picking**: `World::pick(point, size)` gives the mesh a point of the view
  shows, where on it and how far (`Hit`), from the meshes' triangles on the
  CPU.
- **Shadows**: `World::set_shadows(true)` has the sun (the first directional
  light) cast soft shadows from opaque meshes, into a 2048 by 2048 depth map.

In Luau, `ui.scene3d`, `ui.mesh`, `ui.group` and `ui.model` show them
([3D scenes](../../docs/reference/3d.md)).
