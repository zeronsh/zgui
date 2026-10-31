---
section: added
---
- **Screen readers read web pages on Linux** (zgx-desktop's `webkitgtk`): a page's accessible tree, which WebKit serves from its own process, is the window's too, under the page's box (an internal frame), with the box for its parent, so Orca follows focus into the page and reads it: its focused field, its headings by its browse mode's heading keys. GTK's own window for the page says nothing to a reader, and GTK here names WebKitGTK as its toolkit, so Orca keeps its web script (and its browse mode) for the page. `NodeHandle::touch_semantics` (and `Semantics::touch`) has assistive technology read a node anew when what a host adds to it changes; a `gpu_view` with an empty label is none of assistive technology's.
