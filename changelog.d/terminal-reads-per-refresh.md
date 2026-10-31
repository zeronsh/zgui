---
section: performance
---
- **A terminal reads streaming output once a refresh** (zgx-terminal): an engine-drawn terminal read its session and worked its rows out again for every chunk of output, about a thousand a second where Windows' ConPTY hands output over in small chunks, though no more than one frame a refresh shows. Output arriving faster than the display refreshes is now read at the next refresh, all of it at once; output after a pause (a key's echo) is still read as it arrives. `NodeHandle::refresh_interval` gives the display's refresh period to code like it (zgx).
