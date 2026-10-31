---
section: fixed
---
- **A layer as large as the window draws at its scale** (zgx-gpu): each isolated layer had 16 MiB of its own, so one window-sized layer at 2x (2880 by 1800, 20.7 MB) rasterized coarser, softer, and warned of the 64 MiB budget with far less resident. A layer now takes what the budget leaves; the budget is four of the window's frames, at least 64 MiB, counted against what the textures allocate. The warning names the layer and what was resident, allocated and live, once per layer. `zgx inspect memory` lists each layer (`gpu layer`: node, pixels, scale, live and allocated bytes, transformed or parked) and `layers live` beside `layers`.
