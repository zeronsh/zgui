---
section: added
---
- **The kitty keyboard protocol in engine-drawn terminals** (zgx-terminal): a program that asks (`CSI > flags u`: Neovim, Claude Code and other agents) gets its keys as the protocol encodes them: Ctrl+C as `CSI 99;5u` rather than ^C, Escape as `CSI 27u`, repeats and releases, shifted and base keys, every key as an escape with its text, and its flags answer `CSI ? u`.
