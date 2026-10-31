---
section: fixed
---
- **An input method's candidates show under a web page's caret on a busy machine** (zgx-desktop's `webkitgtk`, Linux): IBus over XIM forgets the spot as a composition begins, and a page's caret comes from WebKit's web process, sometimes after the candidates had shown, which then stayed at the window's bottom left. The window gives the spot again as a page's composition begins, and tells the input method each caret the page reports as it comes, not at its next frame.
