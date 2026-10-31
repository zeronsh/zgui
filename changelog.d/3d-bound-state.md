---
section: performance
---
- **3D frames set each pipeline, binding and buffer once a run of meshes needs it** (zgx-3d): every run set them all again, the same or not. A frame of 2,000 varied meshes costs 6% fewer instructions.
