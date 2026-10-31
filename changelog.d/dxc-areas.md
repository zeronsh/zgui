---
section: performance
---
- **A canvas's filled areas draw on the GPU on Windows when the app ships DXC** (zgx-gpu): with `dxcompiler.dll` beside the executable, an area chart draws as one quad again, where with FXC alone, which can't compile its shader, it rasterizes every redraw.
