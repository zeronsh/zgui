"""Which new windows a Windows probe's guard (`windows_input.guard_windows`)
takes as the probe's own app's rather than something to stop at. Pure
Python, no Win32: the guard reads windows and processes, this decides.

A window is the app's when, checked as it appears:
- it is a `ZgxPopup` window (a menu or popover shown outside the window)
  of the app's own process;
- it is a Win32 menu (`#32768`) of the app's own process: its window's
  system menu (Restore, Move, Size, Minimize, Maximize, Close); or
- its process is `msedgewebview2.exe`, a direct child of the app's process,
  and it bears the app's title: WebView2 hosts its surface in an unowned
  top-level window of its own process.

Anything else new still stops the run: consent.exe, SystemSettings,
credential UI, an input method's settings, another app's web view (one not
started by the app), a menu of another process, another window of the
app's own.
"""

POPUP_CLASS = "ZgxPopup"
# A Win32 menu: the system menu the app's window shows.
MENU_CLASS = "#32768"
WEBVIEW_HOST = "msedgewebview2.exe"


def is_own(window, own):
    """Whether `window` is the probe's own: `own` is (pid, class) of windows
    the probe opens itself, or None. Both must match, so another process's
    window of that class, or another class of the probe's, still stops it."""
    return own is not None and window[1] == own[0] and window[2] == own[1]


def is_ours(window, app, processes):
    """Whether `window`, (hwnd, pid, class, title, (width, height)) as
    `visible_windows` gives it, is the probe's app's. `app` is (pid, title)
    of the process the probe launched and its window's title, or None (no
    window is then the app's). `processes` is {pid: (parent pid, exe name)},
    read when the window appeared, so a reused PID is never taken for the
    app's: the app must be in it too."""
    if app is None:
        return False
    _, pid, kind, text, _ = window
    app_pid, app_title = app
    if app_pid not in processes:
        return False
    if pid == app_pid:
        return kind in (POPUP_CLASS, MENU_CLASS)
    entry = processes.get(pid)
    if entry is None:
        return False
    parent, exe = entry
    return exe.lower() == WEBVIEW_HOST and parent == app_pid and text == app_title
