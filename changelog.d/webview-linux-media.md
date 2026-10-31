---
section: fixed
---
- **Video and audio play in a web page on Linux** (zgx-desktop's `webkitgtk`): WebKitGTK took a page in a `GtkOffscreenWindow` for one in no window and started no media there, so no `<video>` or `<audio>` ever loaded and an `AudioContext` stayed suspended. A page's window is now zgx's own offscreen window, which WebKitGTK counts as a window; WebM and Ogg play with the GStreamer plugins WebKitGTK depends on, H.264 with `gstreamer1.0-libav`.
