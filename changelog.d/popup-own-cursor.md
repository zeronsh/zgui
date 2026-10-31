---
section: fixed
---
- **A menu opened by a press stays open when released over its trigger** (zgx-desktop, Wayland): when a compositor cut a tall menu and slid it over its trigger, the popup's pointer events (on zgx's own queue) could be handled before the window's release that came first. The release then counted as a drag away, and the menu closed. A window's own buttons and wheel are now heard where the window last saw the pointer.
