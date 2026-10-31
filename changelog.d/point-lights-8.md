---
section: changed
---
- **`zgx_3d::POINT_LIGHTS` is 8** (zgx-3d, #1123), the point lights a frame shades (the nearest the camera's target); it was 4. Its spelling is the same, but an array sized by it now holds eight, and a scene with five to eight point lights now shades them all. *Migrate:* size by the constant, not by 4.
