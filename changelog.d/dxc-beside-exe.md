---
section: added
---
- **Direct3D 12 compiles with DXC where the app ships it** (zgx-gpu): with Microsoft's `dxcompiler.dll` beside the executable (14.3 MB; `dxil.dll` isn't needed), Windows shaders compile with DXC instead of FXC, and with FXC if that fails. A cold window's first text came sooner (437 ms against 776 on an RTX 4090), and DXC's output is kept in the shader cache as FXC's is. Nothing changes for an app that doesn't ship the DLLs ([shipping](docs/windows.md#shipping-a-zgx-app-on-windows)).
