---
section: performance
---
- **3D frames allocate almost nothing of their own** (zgx-3d): every skinned or morphing mesh's pose went into a new list each frame, and a frame's working lists grew from empty. Poses now share one list apiece and the lists are kept between frames, so a frame of 2,000 skinned, morphing and textured meshes makes 180 allocations where it made 1,511, the rest wgpu's. Frames cost 1–4% fewer instructions and no longer change cost when other code shifts the heap. `World::set_morph_weights` copies into the list a mesh has.
