---
section: fixed
---
- **Coarse curved meshes no longer shadow themselves in speckles** (zgx-3d): where a curve turns from the sun, its flat faces fall behind its smooth normals and caught their own shadow as acne; the shadow lookup now moves off the surface further the more it turns away. A shadow still starts at the base of what casts it, however low the sun. The `model` example's floor sits just below a model's base, where a model's own floor fought it in speckles.
