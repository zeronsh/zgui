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
