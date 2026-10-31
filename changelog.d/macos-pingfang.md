---
section: fixed
---
- **Chinese text on macOS draws in PingFang** (zgx-gpu, the cosmic-text fork): macOS 26 keeps PingFang, its Chinese face, in a private framework, outside the font directories fontdb reads. The fallback lists named only PingFang, so Chinese Han drew in Arial Unicode MS, and traditional and simplified Chinese looked alike. The fork now loads PingFang from where Core Text finds it, if no PingFang is already loaded. Heiti SC and TC, in every macOS, follow it in the lists in case it moves. The system font cache's format is bumped, so the cache is rebuilt once with PingFang in it: 32 more faces, about 0.01 ms more to read it at launch.
