---
section: fixed
---
- **A dialog asked for as a script loads opens, or says why not** (zgx-script): `ui.open_files`, `ui.save_file`, `ui.open_folder` and the clipboard's images, called from a task at a script's top level, failed silently. They looked for the window's platform at once, before the view had mounted, and the load's success then cleared the error. They now look for it as their promise's work first runs, when the window is there, and an error reported while a script loads (a refused dialog, a task's failure) stays shown.
