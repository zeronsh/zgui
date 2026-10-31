---
section: added
---
- **Terminals on Windows and Linux** (zgx-desktop's `terminal` feature, on in zgx-run): `zgx::terminal::terminal` and Luau's `ui.terminal` draw a session with zgx itself on every platform: a pseudoterminal (openpty, ConPTY) and its screen, in the window's own text, each row drawn again only when it changes. Colours to true colour, wide and emoji cells, cursor shapes, the alternate screen, scrollback, selection and copy, bracketed paste, mouse reporting, OSC 8 links, input methods at the cursor, and the session's title, directory, progress, bells, exit and foreground process (an app's resolver included) as with Ghostty. An idle terminal does no work. On macOS, `ghostty` still draws them when it is on. What it can't do yet (images) is in docs/native-surfaces.md.
