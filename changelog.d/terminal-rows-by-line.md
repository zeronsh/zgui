---
section: performance
---
- **A scrolling terminal draws only its new lines** (zgx-terminal): when output scrolled the screen, every row of an engine-drawn terminal was worked out and drawn again. The rows still on screen now move up with it, without layout, and only the new lines draw, so ordinary output (a line a frame) costs about a third less on the UI thread and 15% less in all, and a flood of output costs the same. Rows stay in reading order for screen readers.
