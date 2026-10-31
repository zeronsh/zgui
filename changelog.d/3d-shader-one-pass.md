---
section: performance
---
- **3D pipelines cost less to make** (zgx-3d): a mesh variant's shader is written in one pass, copying only the lines it keeps, instead of being copied three times (prefixes, anti-aliasing, comments). Setup of the `3d-textured` cell falls by 0.29 M instructions and of `3d-mixed` by 1.18 M.
