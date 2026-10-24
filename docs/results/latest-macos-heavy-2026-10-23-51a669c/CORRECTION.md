# Correction

This run built zgui with mimalloc as its allocator, unintentionally (a dev-dependency turned zgui-run's default `mimalloc` feature on for every zgui-desktop example), while GPUI, GPUI Fast and QuickGUI used the system allocator. It was re-run with the system allocator for zgui as [`latest-macos-heavy-2026-10-24-7d559ad`](../latest-macos-heavy-2026-10-24-7d559ad/); old and new numbers are side by side in [the results README](../README.md#correction-2026-10-24-macos-runs-of-2026-1023-24-built-zgui-with-mimalloc).
