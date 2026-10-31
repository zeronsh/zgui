---
section: changed
---
- **Windows draws streaming changes once a refresh** (zgx-desktop): a change no input asked for (a terminal's output, a value from the network) drew a frame at once, however soon after the last, and Windows' presents replace one another until the compositor takes the newest, so a stream of changes drew a frame for each: a terminal printing as fast as it could drew 800 to 1,200 frames a second, nearly none shown. Soon after a frame, such changes now draw together about a refresh after it, so on Windows every app's streaming updates (a live chart, a log, a ticker) draw about once a refresh; a timer at the display's rate still draws every tick. At rest, for input and within a quarter second after it (a terminal's echo of a key), a frame still draws at once.
