---
section: changed
---
- **Screen readers hear a terminal's new output, not each keystroke's line** (zgx-terminal): an engine-drawn terminal's rows are text labels (they were images), and new output is a live region of its own saying the lines output brought in or what a line gained; before, the whole terminal was the live region and a reader re-read the prompt's line with every character the shell echoed. Checked with Orca on Linux and the AX tree on macOS.
