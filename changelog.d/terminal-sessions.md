---
section: added
---
- **Terminal sessions on every platform** (zgx-terminal, new): a shell or any command on a pseudoterminal (openpty on Linux and macOS, ConPTY on Windows) and the screen it draws, kept by `alacritty_terminal`: colours and true colour, wide characters, scrollback, the alternate screen, synchronized output. `Session` does its I/O on a thread of its own that sleeps until the program or the app writes, and wakes the app once until it reads, so a flood of output costs one update per read and an idle shell nothing. Titles, the working directory (OSC 7), bells, progress (OSC 9;4), finished commands (OSC 133), notifications (OSC 9/777), clipboard requests (OSC 52), the foreground process and exit codes come back as events. The engine-drawn terminal view builds on it.
