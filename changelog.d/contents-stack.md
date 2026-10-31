---
section: performance
---
- **`display = "contents"` in a column or row costs what a box does** (zgx): its parent lays its children out as its own with zgx's own column and row layout, not taffy, unless they need taffy as its own children would. 500 rows in a column: 6.0 M instructions to mount and 0.54 M each relayout, against 9.2 M and 1.2 M before (a box: 5.9 M and 0.30 M).
