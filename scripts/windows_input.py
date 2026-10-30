"""Native Win32 input for the Windows smoke scripts: the xdotool of the X11 ones.

Standard library only (ctypes): find a process's window, bring it to the
front, click and type through SendInput (the same path real devices take, so
winit sees WM_* messages, not synthesized events), resize its client area and
capture it as PNG. Coordinates are physical client pixels: the script is
per-monitor DPI aware, like the app.
"""
import ctypes
import os
import re
import struct
import subprocess
import time
import zlib
from ctypes import wintypes

import windows_guard

user32 = ctypes.WinDLL("user32", use_last_error=True)
gdi32 = ctypes.WinDLL("gdi32", use_last_error=True)
kernel32 = ctypes.WinDLL("kernel32", use_last_error=True)

# Handles are pointer-sized: ctypes' default `int` would truncate them.
H = wintypes.HANDLE
for dll, name, res, args in [
    (user32, "SetProcessDpiAwarenessContext", wintypes.BOOL, [H]),
    (user32, "GetForegroundWindow", wintypes.HWND, []),
    (user32, "SetForegroundWindow", wintypes.BOOL, [wintypes.HWND]),
    (user32, "ShowWindow", wintypes.BOOL, [wintypes.HWND, ctypes.c_int]),
    (user32, "IsWindowVisible", wintypes.BOOL, [wintypes.HWND]),
    (user32, "GetWindowThreadProcessId", wintypes.DWORD, [wintypes.HWND, ctypes.POINTER(wintypes.DWORD)]),
    (user32, "GetWindowTextW", ctypes.c_int, [wintypes.HWND, wintypes.LPWSTR, ctypes.c_int]),
    (user32, "GetClientRect", wintypes.BOOL, [wintypes.HWND, ctypes.POINTER(wintypes.RECT)]),
    (user32, "GetWindowRect", wintypes.BOOL, [wintypes.HWND, ctypes.POINTER(wintypes.RECT)]),
    (user32, "ClientToScreen", wintypes.BOOL, [wintypes.HWND, ctypes.POINTER(wintypes.POINT)]),
    (user32, "ScreenToClient", wintypes.BOOL, [wintypes.HWND, ctypes.POINTER(wintypes.POINT)]),
    (user32, "SetWindowPos", wintypes.BOOL, [wintypes.HWND, wintypes.HWND, ctypes.c_int, ctypes.c_int,
                                             ctypes.c_int, ctypes.c_int, wintypes.UINT]),
    (user32, "GetDpiForWindow", wintypes.UINT, [wintypes.HWND]),
    (user32, "PostMessageW", wintypes.BOOL, [wintypes.HWND, wintypes.UINT, wintypes.WPARAM, wintypes.LPARAM]),
    (user32, "GetDC", wintypes.HDC, [wintypes.HWND]),
    (user32, "ReleaseDC", ctypes.c_int, [wintypes.HWND, wintypes.HDC]),
    (user32, "PrintWindow", wintypes.BOOL, [wintypes.HWND, wintypes.HDC, wintypes.UINT]),
    (gdi32, "CreateCompatibleDC", wintypes.HDC, [wintypes.HDC]),
    (gdi32, "CreateCompatibleBitmap", wintypes.HBITMAP, [wintypes.HDC, ctypes.c_int, ctypes.c_int]),
    (gdi32, "SelectObject", wintypes.HGDIOBJ, [wintypes.HDC, wintypes.HGDIOBJ]),
    (gdi32, "BitBlt", wintypes.BOOL, [wintypes.HDC, ctypes.c_int, ctypes.c_int, ctypes.c_int, ctypes.c_int,
                                      wintypes.HDC, ctypes.c_int, ctypes.c_int, wintypes.DWORD]),
    (gdi32, "GetDIBits", ctypes.c_int, [wintypes.HDC, wintypes.HBITMAP, wintypes.UINT, wintypes.UINT,
                                        ctypes.c_void_p, ctypes.c_void_p, wintypes.UINT]),
    (gdi32, "DeleteObject", wintypes.BOOL, [wintypes.HGDIOBJ]),
    (gdi32, "DeleteDC", wintypes.BOOL, [wintypes.HDC]),
    (kernel32, "OpenProcess", H, [wintypes.DWORD, wintypes.BOOL, wintypes.DWORD]),
    (kernel32, "CloseHandle", wintypes.BOOL, [H]),
    (kernel32, "GetProcessTimes", wintypes.BOOL, [H] + [ctypes.POINTER(wintypes.FILETIME)] * 4),
    (kernel32, "QueryProcessCycleTime", wintypes.BOOL, [H, ctypes.POINTER(ctypes.c_uint64)]),
]:
    function = getattr(dll, name)
    function.restype, function.argtypes = res, args

# DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2
user32.SetProcessDpiAwarenessContext(H(-4))

ULONG_PTR = ctypes.c_size_t


class MOUSEINPUT(ctypes.Structure):
    _fields_ = [("dx", wintypes.LONG), ("dy", wintypes.LONG), ("mouseData", wintypes.DWORD),
                ("dwFlags", wintypes.DWORD), ("time", wintypes.DWORD), ("dwExtraInfo", ULONG_PTR)]


class KEYBDINPUT(ctypes.Structure):
    _fields_ = [("wVk", wintypes.WORD), ("wScan", wintypes.WORD), ("dwFlags", wintypes.DWORD),
                ("time", wintypes.DWORD), ("dwExtraInfo", ULONG_PTR)]


class HARDWAREINPUT(ctypes.Structure):
    _fields_ = [("uMsg", wintypes.DWORD), ("wParamL", wintypes.WORD), ("wParamH", wintypes.WORD)]


class _INPUTUNION(ctypes.Union):
    _fields_ = [("mi", MOUSEINPUT), ("ki", KEYBDINPUT), ("hi", HARDWAREINPUT)]


class INPUT(ctypes.Structure):
    _fields_ = [("type", wintypes.DWORD), ("u", _INPUTUNION)]


INPUT_MOUSE, INPUT_KEYBOARD = 0, 1
KEYEVENTF_EXTENDEDKEY, KEYEVENTF_KEYUP, KEYEVENTF_UNICODE = 0x1, 0x2, 0x4
MOUSEEVENTF_MOVE, MOUSEEVENTF_LEFTDOWN, MOUSEEVENTF_LEFTUP = 0x1, 0x2, 0x4
MOUSEEVENTF_RIGHTDOWN, MOUSEEVENTF_RIGHTUP = 0x8, 0x10
MOUSEEVENTF_WHEEL, MOUSEEVENTF_ABSOLUTE, MOUSEEVENTF_VIRTUALDESK = 0x800, 0x8000, 0x4000

VK = {
    "ctrl": 0x11, "control": 0x11, "shift": 0x10, "alt": 0x12, "win": 0x5B, "super": 0x5B,
    "hangul": 0x15, "hanja": 0x19, "convert": 0x1C,
    "backspace": 0x08, "tab": 0x09, "enter": 0x0D, "return": 0x0D, "escape": 0x1B, "esc": 0x1B,
    "space": 0x20, "pageup": 0x21, "pagedown": 0x22, "end": 0x23, "home": 0x24,
    "left": 0x25, "up": 0x26, "right": 0x27, "down": 0x28, "insert": 0x2D, "delete": 0x2E,
    **{f"f{n}": 0x6F + n for n in range(1, 25)},
}
# Keys whose scan codes need the extended flag (arrows and the navigation block).
EXTENDED = {0x21, 0x22, 0x23, 0x24, 0x25, 0x26, 0x27, 0x28, 0x2D, 0x2E, 0x5B}


# The window `activate` last brought to the front: input goes nowhere else.
_target = None
# Run before every input sent once `guard_windows` set it: it raises, and
# nothing is sent, when a window the probe didn't expect is showing.
_window_guard = None


def _send(*inputs, guard=True):
    if guard and _window_guard is not None:
        _window_guard()
    if guard and _target is not None:
        require_foreground(_target)
    array = (INPUT * len(inputs))(*inputs)
    if user32.SendInput(len(inputs), array, ctypes.sizeof(INPUT)) != len(inputs):
        raise OSError(ctypes.get_last_error(), "SendInput was blocked (is the desktop locked?)")


def _vk(code, up=False):
    flags = (KEYEVENTF_KEYUP if up else 0) | (KEYEVENTF_EXTENDEDKEY if code in EXTENDED else 0)
    scan = user32.MapVirtualKeyW(code, 0)
    return INPUT(INPUT_KEYBOARD, _INPUTUNION(ki=KEYBDINPUT(code, scan, flags, 0, 0)))


def _code(name):
    name = name.lower()
    if name in VK:
        return VK[name]
    if len(name) == 1:
        scan = user32.VkKeyScanW(ord(name))
        if scan == -1:
            raise ValueError(f"no key types {name!r} on this layout")
        return scan & 0xFF
    raise ValueError(f"unknown key {name!r}")


def key(*chords, delay=0.03):
    """Press each chord (`"ctrl+a"`, `"shift+Tab"`, `"Left"`) in turn."""
    for chord in chords:
        codes = [_code(part) for part in chord.split("+")]
        _send(*[_vk(c) for c in codes], *[_vk(c, up=True) for c in reversed(codes)])
        time.sleep(delay)


def hold(name):
    _send(_vk(_code(name)))


def release(name):
    _send(_vk(_code(name), up=True))


def type_text(text, delay=0.01):
    """Type `text` as Unicode characters (WM_CHAR, as an IME commit or paste of keys)."""
    for unit in struct.unpack(f"<{len(text.encode('utf-16-le')) // 2}H", text.encode("utf-16-le")):
        down = INPUT(INPUT_KEYBOARD, _INPUTUNION(ki=KEYBDINPUT(0, unit, KEYEVENTF_UNICODE, 0, 0)))
        up = INPUT(INPUT_KEYBOARD, _INPUTUNION(ki=KEYBDINPUT(0, unit, KEYEVENTF_UNICODE | KEYEVENTF_KEYUP, 0, 0)))
        _send(down, up)
        time.sleep(delay)


WNDENUMPROC = ctypes.WINFUNCTYPE(wintypes.BOOL, wintypes.HWND, wintypes.LPARAM)


def windows_of(pid):
    """Visible top-level windows of process `pid` with a client area, front to back
    (helper windows some libraries make are visible but empty)."""
    found = []

    def visit(hwnd, _):
        owner = wintypes.DWORD()
        user32.GetWindowThreadProcessId(hwnd, ctypes.byref(owner))
        if owner.value == pid and user32.IsWindowVisible(hwnd) and all(client_size(hwnd)):
            found.append(hwnd)
        return True

    user32.EnumWindows(WNDENUMPROC(visit), 0)
    return found


def popups_of(pid):
    """Shown popup windows (menus, popovers past the window) of process `pid`."""
    buffer = ctypes.create_unicode_buffer(64)

    def popup(hwnd):
        user32.GetClassNameW(hwnd, buffer, 64)
        return buffer.value == "ZgxPopup"

    return [hwnd for hwnd in windows_of(pid) if popup(hwnd)]


def visible_windows():
    """Every visible top-level window with an area, of any process: (hwnd,
    pid, class, title, (width, height))."""
    found = []
    name = ctypes.create_unicode_buffer(256)
    text = ctypes.create_unicode_buffer(256)

    def visit(hwnd, _):
        rect = wintypes.RECT()
        if user32.IsWindowVisible(hwnd) and user32.GetWindowRect(hwnd, ctypes.byref(rect)):
            size = (rect.right - rect.left, rect.bottom - rect.top)
            if size[0] > 0 and size[1] > 0:
                owner = wintypes.DWORD()
                user32.GetWindowThreadProcessId(hwnd, ctypes.byref(owner))
                user32.GetClassNameW(hwnd, name, 256)
                user32.GetWindowTextW(hwnd, text, 256)
                found.append((hwnd, owner.value, name.value, text.value, size))
        return True

    user32.EnumWindows(WNDENUMPROC(visit), 0)
    return found


class PROCESSENTRY32W(ctypes.Structure):
    _fields_ = [("dwSize", wintypes.DWORD), ("cntUsage", wintypes.DWORD), ("th32ProcessID", wintypes.DWORD),
                ("th32DefaultHeapID", ctypes.c_size_t), ("th32ModuleID", wintypes.DWORD),
                ("cntThreads", wintypes.DWORD), ("th32ParentProcessID", wintypes.DWORD),
                ("pcPriClassBase", wintypes.LONG), ("dwFlags", wintypes.DWORD), ("szExeFile", wintypes.WCHAR * 260)]


def processes():
    """{pid: (parent pid, exe name)} of every process now (`windows_guard`)."""
    snapshot = kernel32.CreateToolhelp32Snapshot(0x2, 0)  # TH32CS_SNAPPROCESS
    if not snapshot or snapshot == H(-1).value:
        raise OSError(ctypes.get_last_error(), "CreateToolhelp32Snapshot")
    entry = PROCESSENTRY32W()
    entry.dwSize = ctypes.sizeof(entry)
    found = {}
    try:
        ok = kernel32.Process32FirstW(snapshot, ctypes.byref(entry))
        while ok:
            found[entry.th32ProcessID] = (entry.th32ParentProcessID, entry.szExeFile)
            ok = kernel32.Process32NextW(snapshot, ctypes.byref(entry))
    finally:
        kernel32.CloseHandle(snapshot)
    return found


WM_INPUTLANGCHANGEREQUEST = 0x0050


def loaded_layout(language):
    """The loaded keyboard layout (HKL) of `language` (0x0804, 0x0411), if
    the user has it."""
    user32.GetKeyboardLayoutList.argtypes = [ctypes.c_int, ctypes.POINTER(ctypes.c_void_p)]
    count = user32.GetKeyboardLayoutList(0, None)
    layouts = (ctypes.c_void_p * count)()
    user32.GetKeyboardLayoutList(count, layouts)
    return next((h for h in layouts if h and h & 0xFFFF == language), None)


def layout_of(hwnd):
    """The keyboard layout (HKL) window `hwnd`'s thread types with."""
    user32.GetKeyboardLayout.argtypes = [ctypes.c_uint32]
    user32.GetKeyboardLayout.restype = ctypes.c_void_p
    return user32.GetKeyboardLayout(user32.GetWindowThreadProcessId(hwnd, None)) or 0


def input_path_args(parser, imm32=True):
    """`--imm32` (winit's IMM32 path, the opt-out) and `--force-tsf`: which
    input path the probe's app takes. Without either, the app's default."""
    group = parser.add_mutually_exclusive_group()
    if imm32:
        group.add_argument("--imm32", action="store_true",
                           help="through winit's IMM32 path (ZGX_TSF=0), not the default")
    group.add_argument("--force-tsf", action="store_true",
                       help="the Text Services Framework forced (ZGX_TSF=1), not the default")


def input_path(env, mode):
    """Set the app's ZGX_TSF in `env` for `mode` ("default", "imm32" or
    "force-tsf") and print which ran. The default leaves it unset, removed
    if the probe's own environment has it (or its old name), so the run
    tests what an app gets with nothing set."""
    for name in ("ZGX_TSF", "ZGUI_TSF"):  # rename-to-zgx: keep
        env.pop(name, None)
    if mode == "imm32":
        env["ZGX_TSF"] = "0"
        said = "IMM32 (ZGX_TSF=0, the opt-out)"
    elif mode == "force-tsf":
        env["ZGX_TSF"] = "1"
        said = "the Text Services Framework, forced (ZGX_TSF=1)"
    else:
        said = "the default (ZGX_TSF unset): the Text Services Framework"
    print(f"MODE: {said}", flush=True)
    return said


def mode_of(args):
    """The mode `input_path_args`' flags name."""
    if getattr(args, "imm32", False):
        return "imm32"
    if getattr(args, "force_tsf", False):
        return "force-tsf"
    return "default"


def use_layout(hwnd, layout, timeout=3.0):
    """Ask Windows for keyboard layout `layout` in window `hwnd` alone, as
    the language bar does for the window in front: the user's languages are
    not touched, and the window's own layout comes back with `use_layout` of
    what `layout_of` said before."""
    user32.PostMessageW(hwnd, WM_INPUTLANGCHANGEREQUEST, 0, layout)
    deadline = time.monotonic() + timeout
    while (layout_of(hwnd) & 0xFFFF) != (layout & 0xFFFF):
        if time.monotonic() > deadline:
            raise RuntimeError(f"window {hwnd:#x} did not take keyboard layout {layout:#x}")
        time.sleep(0.05)


# Input methods' candidate lists, which a probe composing expects to show.
# Their text services run in the app's process, so their windows are the
# app's: whose window it is says nothing; its class says what it is.
CANDIDATE_CLASSES = {
    "Microsoft.IME.UIManager.CandidateWindow.Host",
    "Microsoft.IME.CandidateWindow.View",
    "IME",
    "MSCTFIME UI",
}


class UnexpectedWindow(RuntimeError):
    pass


# What a candidate list may not hold: an offer or a prompt shows inside the
# candidate host itself (Pinyin's Bing offer did), as a control or its text.
OFFER_CONTROLS = {
    "ControlType.Button", "ControlType.Hyperlink", "ControlType.CheckBox",
    "ControlType.RadioButton", "ControlType.ComboBox", "ControlType.MenuItem",
    "ControlType.SplitButton", "ControlType.ToggleButton",
}
OFFER_WORDS = ("bing", "turn on", "cloud", "privacy", "consent", "learn more",
               "必应", "开启", "打开", "云", "隐私", "クラウド", "オンにする", "プライバシー")
OFFER_CLASSES = {"Button", "SysLink"}

# Input methods' preferences (under HKEY_CURRENT_USER) a probe leaves as
# they were: Microsoft Pinyin's, the Japanese IME's.
IME_SETTINGS = (
    r"Software\Microsoft\InputMethod\Settings\CHS",
    r"Software\Microsoft\InputMethod\Settings\JPN",
    r"Software\Microsoft\IME\15.0\IMEJP",
)


def ime_settings(paths=IME_SETTINGS):
    """Every value under `paths` (HKEY_CURRENT_USER, with their subkeys):
    {(key, name): (data, type)}. A key not there has none."""
    import winreg

    found = {}

    def walk(path):
        try:
            key = winreg.OpenKey(winreg.HKEY_CURRENT_USER, path)
        except OSError:
            return
        with key:
            index = 0
            while True:
                try:
                    name, data, kind = winreg.EnumValue(key, index)
                except OSError:
                    break
                found[(path, name)] = (data, kind)
                index += 1
            index = 0
            subkeys = []
            while True:
                try:
                    subkeys.append(winreg.EnumKey(key, index))
                except OSError:
                    break
                index += 1
        for sub in subkeys:
            walk(path + "\\" + sub)

    for path in paths:
        walk(path)
    return found


def restore_ime_settings(before, now):
    """Put back what `before` (`ime_settings`) held: values changed or gone
    are written again, values that appeared are removed."""
    import winreg

    for (path, name), (data, kind) in before.items():
        if now.get((path, name)) != (data, kind):
            with winreg.CreateKeyEx(winreg.HKEY_CURRENT_USER, path, 0, winreg.KEY_SET_VALUE) as key:
                winreg.SetValueEx(key, name, 0, kind, data)
    for path, name in now.keys() - before.keys():
        with winreg.OpenKey(winreg.HKEY_CURRENT_USER, path, 0, winreg.KEY_SET_VALUE) as key:
            winreg.DeleteValue(key, name)


def window_contents(hwnds, timeout=10.0):
    """What windows `hwnds` hold: each child window's class and text, then
    each element of their UI Automation trees, as (kind, name) pairs. Raises
    when it can't tell (a probe then stops rather than guess)."""
    found = []
    name = ctypes.create_unicode_buffer(256)
    text = ctypes.create_unicode_buffer(256)

    def child(hwnd, _):
        user32.GetClassNameW(hwnd, name, 256)
        user32.GetWindowTextW(hwnd, text, 256)
        found.append(("window " + name.value, text.value))
        return True

    for hwnd in hwnds:
        user32.EnumChildWindows(hwnd, WNDENUMPROC(child), 0)
    handles = ", ".join(str(int(h)) for h in hwnds)
    script = f"""
$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName UIAutomationClient,UIAutomationTypes
foreach ($h in @({handles})) {{
  $root = [System.Windows.Automation.AutomationElement]::FromHandle([IntPtr]$h)
  $all = $root.FindAll([System.Windows.Automation.TreeScope]::Subtree, [System.Windows.Automation.Condition]::TrueCondition)
  foreach ($e in $all) {{ "{{0}}`t{{1}}" -f $e.Current.ControlType.ProgrammaticName, ($e.Current.Name -replace "`r?`n", " ") }}
}}
"done"
"""
    done = subprocess.run(["powershell", "-NoProfile", "-NonInteractive", "-Command", script],
                          capture_output=True, text=True, encoding="utf-8", timeout=timeout)
    rows = done.stdout.strip().splitlines()
    if done.returncode != 0 or not rows or rows[-1] != "done":
        raise RuntimeError(f"UI Automation didn't read them: {done.stderr.strip()[:300]}")
    found += [tuple(row.split("\t", 1)) if "\t" in row else (row, "") for row in rows[:-1]]
    return found


# Windows 11's candidate list, as UI Automation reads the host found under
# the composition: what it may hold, by control type and name. Anything else
# in it (another button, a link, a check box, an offer's text, an element
# not listed) stops a probe. Names are the English UI's; an IME showing its
# UI in another language adds its names beside them.
CANDIDATE_HOST = "Windows Input Experience"
CANDIDATE_UI = {
    ("ControlType.Window", CANDIDATE_HOST),
    ("ControlType.Menu", "Microsoft Candidate UI"),
    ("ControlType.List", "Candidate Item Panel"),
    ("ControlType.Button", "Previous Page"),
    ("ControlType.Button", "Next Page"),
    ("ControlType.Button", "Emoji and Symbol"),
    ("ControlType.Button", "Search Suggestion"),
    # The Japanese IME's pager: "Page 1", "Page 2"… buttons under it.
    ("ControlType.Menu", "Pager"),
}
PAGER = ("ControlType.Menu", "Pager")
# The candidate list: its items (and their text) are allowed whatever they say.
CANDIDATE_ITEMS = ("ControlType.List", "Candidate Item Panel")


def candidate_ui():
    """`CANDIDATE_UI` less the names in ZGX_GUARD_WITHOUT (comma-separated):
    the guard's own check, that an element it no longer allows stops a probe."""
    without = {n.strip() for n in os.environ.get("ZGX_GUARD_WITHOUT", "").split(",") if n.strip()}
    return {(kind, name) for kind, name in CANDIDATE_UI if name not in without}


def host_at(points):
    """The candidate host under any of `points` (screen pixels): Windows 11's
    "Windows Input Experience" window, which window enumeration doesn't list
    and only a hit-test finds ("host"), or a classic candidate window
    ("classic"). None when the points show neither (the app, the desktop)."""
    user32.WindowFromPoint.argtypes = [wintypes.POINT]
    user32.WindowFromPoint.restype = wintypes.HWND
    user32.GetAncestor.argtypes = [wintypes.HWND, wintypes.UINT]
    user32.GetAncestor.restype = wintypes.HWND
    name = ctypes.create_unicode_buffer(256)
    text = ctypes.create_unicode_buffer(256)
    for x, y in points:
        hwnd = user32.WindowFromPoint(wintypes.POINT(int(x), int(y)))
        while hwnd:
            user32.GetClassNameW(hwnd, name, 256)
            user32.GetWindowTextW(hwnd, text, 256)
            if name.value == "Windows.UI.Core.CoreWindow" and text.value == CANDIDATE_HOST:
                return ("host", hwnd)
            if name.value in CANDIDATE_CLASSES:
                return ("classic", hwnd)
            hwnd = user32.GetAncestor(hwnd, 1)  # GA_PARENT
    return None


def ui_tree(hwnd, timeout=10.0):
    """Window `hwnd`'s UI Automation tree (its control view), depth first:
    (depth, control type, name). Raises when it can't be read."""
    script = f"""
$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName UIAutomationClient,UIAutomationTypes
$walker = [System.Windows.Automation.TreeWalker]::ControlViewWalker
function Walk($e, $d) {{
  "{{0}}`t{{1}}`t{{2}}" -f $d, $e.Current.ControlType.ProgrammaticName, ($e.Current.Name -replace "`r?`n", " ")
  $c = $walker.GetFirstChild($e)
  while ($c -ne $null) {{ Walk $c ($d + 1); $c = $walker.GetNextSibling($c) }}
}}
Walk ([System.Windows.Automation.AutomationElement]::FromHandle([IntPtr]{int(hwnd)})) 0
"done"
"""
    done = subprocess.run(["powershell", "-NoProfile", "-NonInteractive", "-Command", script],
                          capture_output=True, text=True, encoding="utf-8", timeout=timeout)
    rows = done.stdout.strip().splitlines()
    if done.returncode != 0 or not rows or rows[-1] != "done":
        raise RuntimeError(f"UI Automation didn't read it: {done.stderr.strip()[:300]}")
    tree = []
    for row in rows[:-1]:
        depth, kind, label = (row.split("\t", 2) + ["", ""])[:3]
        tree.append((int(depth), kind, label))
    return tree


def not_candidates(tree, allowed):
    """What in a candidate host's `tree` isn't its candidate UI: not
    `allowed` by type and name, not an item (or an item's text) of its
    candidate list, not a numbered page of its pager, and not an unnamed
    Pane. An unnamed Pane lays the rest
    out and passes only as what it holds does: every element under it is
    judged on its own, so a Pane holding an offer's button still stops a
    probe, at any depth."""
    found = []
    ancestors = []
    for depth, kind, label in tree:
        del ancestors[depth:]
        in_list = CANDIDATE_ITEMS in ancestors and CANDIDATE_ITEMS in allowed
        item = in_list and kind in ("ControlType.ListItem", "ControlType.Text")
        layout = kind == "ControlType.Pane" and not label.strip()
        # A page of the pager's, by number: "Page 2", not "Page 2 of Bing".
        page = (kind == "ControlType.Button" and PAGER in ancestors and PAGER in allowed
                and re.fullmatch(r"Page [0-9]+", label) is not None)
        if (kind, label) not in allowed and not item and not layout and not page:
            found.append((depth, kind, label))
        ancestors.append((kind, label))
    return found


def below_caret(hwnd, lines, field="first"):
    """A function giving screen points just below the caret of `field` in
    window `hwnd`, as the example's last `CARET <field> Rect {..}` line in
    `lines` (its output) places it: where an input method shows its
    candidates. Points from the window's top-left before it reports one."""
    import re

    def points():
        rect = None
        for line in reversed(lines):
            if line.startswith(f"CARET {field} "):
                rect = [float(v) for v in re.findall(r"(?:x|y|width|height): ([\d.]+)", line)]
                break
        scale = dpi(hwnd) / 96
        x, y, _, height = rect if rect and len(rect) == 4 else (0.0, 0.0, 0.0, 0.0)
        return [to_screen(hwnd, (x + dx) * scale, (y + height + dy) * scale)
                for dx, dy in ((4, 12), (4, 36), (40, 24), (120, 24))]

    return points


def guard_windows(shots, allowed=CANDIDATE_CLASSES, settings=IME_SETTINGS, where=None, app=None, own=None):
    """Fail closed from now on: before every input sent (and when the
    returned check is called, after the last), the probe stops with a
    screenshot in directory `shots`, sending nothing, when
    - an input method's preferences (`settings`) changed: they are put back
      as they were first, and the keystroke that changed them is the last;
    - a visible top-level window that wasn't showing now isn't a classic
      candidate list (`allowed` classes), or such a list holds anything but
      candidates: a button, a link, a check box, an offer's text
      (`OFFER_*`), or can't be read;
    - with `where` (a function giving screen points just below the
      composition, where candidates show): the candidate host hit-tested
      there, Windows 11's "Windows Input Experience" window that window
      enumeration doesn't list, holds anything but its candidate UI
      (`CANDIDATE_UI` and its list's items), or the hit-test or its UI
      Automation read fails.
    With `app`, (pid, title) of the app the probe launched, its own new
    windows are passed, each checked once as it appears (`windows_guard`):
    a `ZgxPopup` or Win32 menu (`#32768`, the system menu) of its
    process, and its WebView2 surface (a
    msedgewebview2.exe child of it bearing its title); any other window
    still stops the run. With `own`, (pid, class), the probe's windows of
    that class are passed too (`windows_guard.is_own`).
    An offer or a consent prompt is never answered or closed."""
    baseline = {window[0] for window in visible_windows()}
    trusted = set()
    before = ime_settings(settings)
    stops = 0

    def stop(reason):
        nonlocal stops
        stops += 1
        shots.mkdir(parents=True, exist_ok=True)
        path = shots / f"stopped-{stops}.png"
        metrics = [user32.GetSystemMetrics(i) for i in (76, 77, 78, 79)]
        save_png(capture_rect(*metrics), str(path))
        raise UnexpectedWindow(f"{reason}; no input sent and nothing dismissed. Screenshot: {path}")

    def check():
        now = ime_settings(settings)
        if now != before:
            changed = sorted(f"{k[0]}\\{k[1]}" for k in now.keys() | before.keys() if now.get(k) != before.get(k))
            restore_ime_settings(before, now)
            stop(f"an input method's settings changed ({', '.join(changed)}); put back as they were")
        new = [w for w in visible_windows() if w[0] not in baseline and w[0] not in trusted
               and not windows_guard.is_own(w, own)]
        if app is not None and any(w[2] not in allowed for w in new):
            # Read as they appear: a PID is only the app's while it runs.
            running = processes()
            for w in new:
                if w[2] not in allowed and windows_guard.is_ours(w, app, running):
                    trusted.add(w[0])
            new = [w for w in new if w[0] not in trusted]
        unknown = [w for w in new if w[2] not in allowed]
        if unknown:
            found = "; ".join(f"class {c!r} title {t!r} {w}x{h} pid {pid}" for _, pid, c, t, (w, h) in unknown)
            stop(f"a window the probe didn't expect is showing ({found})")
        candidates = [w[0] for w in new]
        if candidates:
            try:
                held = window_contents(candidates)
            except Exception as error:
                stop(f"couldn't read what a candidate list holds ({error})")
            offers = [(kind, text) for kind, text in held
                      if kind in OFFER_CONTROLS or kind.removeprefix("window ") in OFFER_CLASSES
                      or any(word in text.lower() for word in OFFER_WORDS)]
            if offers:
                stop(f"a candidate list holds more than candidates: {offers}")
        if where is not None:
            try:
                found = host_at(where())
                if found and found[0] == "host":
                    extra = not_candidates(ui_tree(found[1]), candidate_ui())
                    if extra:
                        stop(f"Windows 11's candidate host holds more than its candidate UI: {extra}")
            except UnexpectedWindow:
                raise
            except Exception as error:
                stop(f"couldn't hit-test or read the candidate host ({error})")

    global _window_guard
    _window_guard = check
    return check


def wait_window(proc, timeout=15.0):
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        if proc.poll() is not None:
            raise RuntimeError(f"process exited with {proc.returncode} before showing a window")
        found = windows_of(proc.pid)
        if found:
            return found[0]
        time.sleep(0.05)
    raise RuntimeError("no visible window appeared")


def title(hwnd):
    buffer = ctypes.create_unicode_buffer(512)
    user32.GetWindowTextW(hwnd, buffer, 512)
    return buffer.value


def _handle(hwnd):
    return getattr(hwnd, "value", hwnd)


def is_foreground(hwnd):
    return user32.GetForegroundWindow() == _handle(hwnd)


def activate(hwnd):
    """Bring `hwnd` to the front with keyboard focus, past the foreground lock.
    From then on, input is sent only while it stays in front."""
    global _target
    user32.ShowWindow(hwnd, 9)  # SW_RESTORE
    for _ in range(20):
        # A key event from this process lifts the foreground lock for it.
        # F24, which nothing binds: a tap of Alt put dialogs in their
        # mnemonic mode, which took the next key typed. Not to the foreground
        # `_target` (this changes it), but still past the window guard.
        if _window_guard is not None:
            _window_guard()
        _send(_vk(VK["f24"]), _vk(VK["f24"], up=True), guard=False)
        user32.SetForegroundWindow(hwnd)
        if is_foreground(hwnd):
            _target = _handle(hwnd)
            time.sleep(0.15)
            return
        time.sleep(0.05)
    raise RuntimeError(f"could not activate {title(hwnd)!r}")


def require_foreground(hwnd):
    """Fail rather than type into whatever window the user has in front."""
    if not is_foreground(hwnd):
        raise RuntimeError(f"{title(hwnd)!r} is not in front: input would go elsewhere")


def client_size(hwnd):
    rect = wintypes.RECT()
    user32.GetClientRect(hwnd, ctypes.byref(rect))
    return rect.right - rect.left, rect.bottom - rect.top


def to_screen(hwnd, x, y):
    point = wintypes.POINT(round(x), round(y))
    user32.ClientToScreen(hwnd, ctypes.byref(point))
    return point.x, point.y


def text_rects(hwnd, *words, timeout=10.0):
    """Where each of `words` shows in `hwnd`'s text, as UI Automation's
    text pattern places it (the first match in the first text that has
    them all): {word: (left, top, right, bottom)} in client pixels. A probe
    finds text where it is laid out now, not at coordinates that go stale
    as an example's layout changes. Each word's range is moved to by
    characters from the document's start (AccessKit's text pattern has no
    FindText). The tree is built when first asked for, so this asks again
    until `timeout`."""
    quoted = ", ".join("'" + w.replace("'", "''") + "'" for w in words)
    script = f"""
$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName UIAutomationClient,UIAutomationTypes
$root = [System.Windows.Automation.AutomationElement]::FromHandle([IntPtr]{int(hwnd)})
$texts = $root.FindAll([System.Windows.Automation.TreeScope]::Subtree, [System.Windows.Automation.Condition]::TrueCondition)
$start = [System.Windows.Automation.Text.TextPatternRangeEndpoint]::Start
$end = [System.Windows.Automation.Text.TextPatternRangeEndpoint]::End
$character = [System.Windows.Automation.Text.TextUnit]::Character
foreach ($element in $texts) {{
  $pattern = $null
  if (-not $element.TryGetCurrentPattern([System.Windows.Automation.TextPattern]::Pattern, [ref]$pattern)) {{ continue }}
  $document = $pattern.DocumentRange
  $all = $document.GetText(-1)
  $found = @()
  foreach ($word in @({quoted})) {{
    $at = $all.IndexOf($word)
    if ($at -lt 0) {{ $found = $null; break }}
    $range = $document.Clone()
    $range.MoveEndpointByRange($end, $range, $start)
    [void]$range.MoveEndpointByUnit($end, $character, $at + $word.Length)
    [void]$range.MoveEndpointByUnit($start, $character, $at)
    $r = $range.GetBoundingRectangles()
    if ($r.Count -eq 0) {{ $found = $null; break }}
    $found += "{{0}} {{1}} {{2}} {{3}}" -f $r[0].Left, $r[0].Top, $r[0].Right, $r[0].Bottom
  }}
  if ($found) {{ $found -join ";"; break }}
}}
"""
    deadline = time.monotonic() + timeout
    while True:
        done = subprocess.run(["powershell", "-NoProfile", "-NonInteractive", "-Command", script],
                              capture_output=True, text=True)
        out = done.stdout.strip()
        if out:
            rects = {}
            for word, rect in zip(words, out.split(";")):
                left, top, right, bottom = (float(v) for v in rect.split())
                l, t = to_client(hwnd, left, top)
                r, b = to_client(hwnd, right, bottom)
                rects[word] = (l, t, r, b)
            return rects
        if time.monotonic() > deadline:
            raise RuntimeError(f"no text in the window shows {words!r} to UI Automation: {done.stderr.strip()[-800:]}")
        time.sleep(0.5)


def to_client(hwnd, x, y):
    point = wintypes.POINT(round(x), round(y))
    user32.ScreenToClient(hwnd, ctypes.byref(point))
    return point.x, point.y


def move(hwnd, x, y):
    sx, sy = to_screen(hwnd, x, y)
    user32.SetCursorPos(sx, sy)
    # A relative zero move after SetCursorPos makes the move an input event.
    _send(INPUT(INPUT_MOUSE, _INPUTUNION(mi=MOUSEINPUT(0, 0, 0, MOUSEEVENTF_MOVE, 0, 0))))
    time.sleep(0.02)


def click(hwnd, x, y, count=1, button="left", delay=0.03):
    move(hwnd, x, y)
    down, up = ((MOUSEEVENTF_LEFTDOWN, MOUSEEVENTF_LEFTUP) if button == "left"
                else (MOUSEEVENTF_RIGHTDOWN, MOUSEEVENTF_RIGHTUP))
    for _ in range(count):
        _send(INPUT(INPUT_MOUSE, _INPUTUNION(mi=MOUSEINPUT(0, 0, 0, down, 0, 0))),
              INPUT(INPUT_MOUSE, _INPUTUNION(mi=MOUSEINPUT(0, 0, 0, up, 0, 0))))
        time.sleep(delay)


def drag(hwnd, start, end, steps=12, delay=0.01):
    move(hwnd, *start)
    _send(INPUT(INPUT_MOUSE, _INPUTUNION(mi=MOUSEINPUT(0, 0, 0, MOUSEEVENTF_LEFTDOWN, 0, 0))))
    for i in range(1, steps + 1):
        t = i / steps
        move(hwnd, start[0] + (end[0] - start[0]) * t, start[1] + (end[1] - start[1]) * t)
        time.sleep(delay)
    _send(INPUT(INPUT_MOUSE, _INPUTUNION(mi=MOUSEINPUT(0, 0, 0, MOUSEEVENTF_LEFTUP, 0, 0))))


def wheel(hwnd, x, y, notches):
    move(hwnd, x, y)
    _send(INPUT(INPUT_MOUSE, _INPUTUNION(mi=MOUSEINPUT(0, 0, ctypes.c_uint32(int(notches * 120)).value,
                                                      MOUSEEVENTF_WHEEL, 0, 0))))


def resize_client(hwnd, width, height, timeout=5.0):
    """Resize so the client area is `width` x `height` physical pixels."""
    window, client = wintypes.RECT(), wintypes.RECT()
    user32.GetWindowRect(hwnd, ctypes.byref(window))
    user32.GetClientRect(hwnd, ctypes.byref(client))
    extra_w = (window.right - window.left) - client.right
    extra_h = (window.bottom - window.top) - client.bottom
    # SWP_NOMOVE | SWP_NOZORDER | SWP_NOACTIVATE
    user32.SetWindowPos(hwnd, None, 0, 0, width + extra_w, height + extra_h, 0x2 | 0x4 | 0x10)
    deadline = time.monotonic() + timeout
    while client_size(hwnd) != (width, height):
        if time.monotonic() > deadline:
            raise AssertionError(f"resize not applied: wanted {(width, height)}, got {client_size(hwnd)}")
        time.sleep(0.05)


def dpi(hwnd):
    return user32.GetDpiForWindow(hwnd)


def close(hwnd):
    user32.PostMessageW(hwnd, 0x0010, 0, 0)  # WM_CLOSE


class BITMAPINFOHEADER(ctypes.Structure):
    _fields_ = [("biSize", wintypes.DWORD), ("biWidth", wintypes.LONG), ("biHeight", wintypes.LONG),
                ("biPlanes", wintypes.WORD), ("biBitCount", wintypes.WORD),
                ("biCompression", wintypes.DWORD), ("biSizeImage", wintypes.DWORD),
                ("biXPelsPerMeter", wintypes.LONG), ("biYPelsPerMeter", wintypes.LONG),
                ("biClrUsed", wintypes.DWORD), ("biClrImportant", wintypes.DWORD)]


def capture(hwnd):
    """The client area's pixels as (width, height, BGRA rows top-down), as composed on screen."""
    width, height = client_size(hwnd)
    x, y = to_screen(hwnd, 0, 0)
    return capture_rect(x, y, width, height)


def capture_window(hwnd):
    """The client area's pixels as the window drew them, as `capture`, even
    while other windows cover it (`PrintWindow`, `PW_RENDERFULLCONTENT`:
    the compositor's copy, DirectComposition content included)."""
    width, height = client_size(hwnd)
    screen = user32.GetDC(None)
    memory = gdi32.CreateCompatibleDC(screen)
    bitmap = gdi32.CreateCompatibleBitmap(screen, width, height)
    old = gdi32.SelectObject(memory, bitmap)
    user32.PrintWindow(hwnd, memory, 1 | 2)  # PW_CLIENTONLY | PW_RENDERFULLCONTENT
    header = BITMAPINFOHEADER(ctypes.sizeof(BITMAPINFOHEADER), width, -height, 1, 32, 0, 0, 0, 0, 0, 0)
    pixels = ctypes.create_string_buffer(width * height * 4)
    gdi32.GetDIBits(memory, bitmap, 0, height, pixels, ctypes.byref(header), 0)
    gdi32.SelectObject(memory, old)
    gdi32.DeleteObject(bitmap)
    gdi32.DeleteDC(memory)
    user32.ReleaseDC(None, screen)
    return width, height, pixels.raw


def capture_rect(x, y, width, height):
    """The screen's pixels in a rectangle (physical), as `capture`."""
    screen = user32.GetDC(None)
    memory = gdi32.CreateCompatibleDC(screen)
    bitmap = gdi32.CreateCompatibleBitmap(screen, width, height)
    old = gdi32.SelectObject(memory, bitmap)
    # SRCCOPY | CAPTUREBLT: what the compositor shows, flip-model swap chains included.
    gdi32.BitBlt(memory, 0, 0, width, height, screen, x, y, 0x00CC0020 | 0x40000000)
    header = BITMAPINFOHEADER(ctypes.sizeof(BITMAPINFOHEADER), width, -height, 1, 32, 0, 0, 0, 0, 0, 0)
    pixels = ctypes.create_string_buffer(width * height * 4)
    gdi32.GetDIBits(memory, bitmap, 0, height, pixels, ctypes.byref(header), 0)
    gdi32.SelectObject(memory, old)
    gdi32.DeleteObject(bitmap)
    gdi32.DeleteDC(memory)
    user32.ReleaseDC(None, screen)
    return width, height, pixels.raw


def pixel(capture_result, x, y):
    """(r, g, b) at physical client pixel (x, y) of a `capture`."""
    width, _, data = capture_result
    b, g, r, _ = data[(y * width + x) * 4:(y * width + x) * 4 + 4]
    return r, g, b


def save_png(capture_result, path):
    width, height, data = capture_result
    rows = bytearray()
    for y in range(height):
        rows.append(0)
        row = data[y * width * 4:(y + 1) * width * 4]
        for x in range(width):
            b, g, r, _ = row[x * 4:x * 4 + 4]
            rows += bytes((r, g, b))

    def chunk(kind, body):
        return struct.pack(">I", len(body)) + kind + body + struct.pack(">I", zlib.crc32(kind + body))

    with open(path, "wb") as out:
        out.write(b"\x89PNG\r\n\x1a\n")
        out.write(chunk(b"IHDR", struct.pack(">IIBBBBB", width, height, 8, 2, 0, 0, 0)))
        out.write(chunk(b"IDAT", zlib.compress(bytes(rows), 6)))
        out.write(chunk(b"IEND", b""))


def cpu_seconds(pid):
    """Kernel plus user CPU time of process `pid` so far."""
    handle = kernel32.OpenProcess(0x1000, False, pid)  # PROCESS_QUERY_LIMITED_INFORMATION
    if not handle:
        raise OSError(ctypes.get_last_error(), "OpenProcess")
    try:
        times = [wintypes.FILETIME() for _ in range(4)]
        kernel32.GetProcessTimes(handle, *[ctypes.byref(t) for t in times])
        to_s = lambda t: ((t.dwHighDateTime << 32) | t.dwLowDateTime) / 1e7
        return to_s(times[2]) + to_s(times[3])
    finally:
        kernel32.CloseHandle(handle)


class THREADENTRY32(ctypes.Structure):
    _fields_ = [("dwSize", wintypes.DWORD), ("cntUsage", wintypes.DWORD), ("th32ThreadID", wintypes.DWORD),
                ("th32OwnerProcessID", wintypes.DWORD), ("tpBasePri", wintypes.LONG),
                ("tpDeltaPri", wintypes.LONG), ("dwFlags", wintypes.DWORD)]


for name, res, args in [
    ("CreateToolhelp32Snapshot", H, [wintypes.DWORD, wintypes.DWORD]),
    ("Thread32First", wintypes.BOOL, [H, ctypes.POINTER(THREADENTRY32)]),
    ("Thread32Next", wintypes.BOOL, [H, ctypes.POINTER(THREADENTRY32)]),
    ("Process32FirstW", wintypes.BOOL, [H, ctypes.POINTER(PROCESSENTRY32W)]),
    ("Process32NextW", wintypes.BOOL, [H, ctypes.POINTER(PROCESSENTRY32W)]),
    ("OpenThread", H, [wintypes.DWORD, wintypes.BOOL, wintypes.DWORD]),
    ("QueryThreadCycleTime", wintypes.BOOL, [H, ctypes.POINTER(ctypes.c_uint64)]),
    ("GetThreadDescription", ctypes.HRESULT, [H, ctypes.POINTER(wintypes.LPWSTR)]),
    ("LocalFree", H, [H]),
]:
    function = getattr(kernel32, name)
    function.restype, function.argtypes = res, args


psapi = ctypes.WinDLL("psapi", use_last_error=True)
ntdll = ctypes.WinDLL("ntdll")


class MODULEINFO(ctypes.Structure):
    _fields_ = [("lpBaseOfDll", ctypes.c_void_p), ("SizeOfImage", wintypes.DWORD), ("EntryPoint", ctypes.c_void_p)]


psapi.EnumProcessModulesEx.argtypes = [H, ctypes.POINTER(H), wintypes.DWORD, ctypes.POINTER(wintypes.DWORD),
                                       wintypes.DWORD]
psapi.GetModuleBaseNameW.argtypes = [H, H, wintypes.LPWSTR, wintypes.DWORD]
psapi.GetModuleInformation.argtypes = [H, H, ctypes.POINTER(MODULEINFO), wintypes.DWORD]
ntdll.NtQueryInformationThread.argtypes = [H, ctypes.c_int, ctypes.c_void_p, wintypes.ULONG, ctypes.c_void_p]


def modules(pid):
    """[(base, end, name)] of process `pid`'s loaded modules."""
    process = kernel32.OpenProcess(0x0410, False, pid)  # QUERY_INFORMATION | VM_READ
    if not process:
        return []
    handles = (H * 1024)()
    needed = wintypes.DWORD()
    psapi.EnumProcessModulesEx(process, handles, ctypes.sizeof(handles), ctypes.byref(needed), 3)
    found = []
    for module in handles[:needed.value // ctypes.sizeof(H)]:
        info, name = MODULEINFO(), ctypes.create_unicode_buffer(260)
        psapi.GetModuleInformation(process, module, ctypes.byref(info), ctypes.sizeof(info))
        psapi.GetModuleBaseNameW(process, module, name, 260)
        found.append((info.lpBaseOfDll or 0, (info.lpBaseOfDll or 0) + info.SizeOfImage, name.value))
    kernel32.CloseHandle(process)
    return found


def thread_cycles(pid):
    """{(thread id, name): cycles so far} for each thread of process `pid`; an
    unnamed thread is named by the module its start address is in."""
    loaded = modules(pid)
    snapshot = kernel32.CreateToolhelp32Snapshot(0x4, 0)  # TH32CS_SNAPTHREAD
    entry = THREADENTRY32()
    entry.dwSize = ctypes.sizeof(entry)
    found = {}
    ok = kernel32.Thread32First(snapshot, ctypes.byref(entry))
    while ok:
        if entry.th32OwnerProcessID == pid:
            handle = kernel32.OpenThread(0x0040, False, entry.th32ThreadID)  # QUERY_INFORMATION
            if handle:
                count = ctypes.c_uint64()
                kernel32.QueryThreadCycleTime(handle, ctypes.byref(count))
                text = wintypes.LPWSTR()
                name = ""
                if kernel32.GetThreadDescription(handle, ctypes.byref(text)) >= 0 and text:
                    name = text.value
                    kernel32.LocalFree(ctypes.cast(text, H))
                start = ctypes.c_void_p()
                # ThreadQuerySetWin32StartAddress
                ntdll.NtQueryInformationThread(handle, 9, ctypes.byref(start), ctypes.sizeof(start), None)
                where = next((f"{module}+{start.value - base:#x}" for base, end, module in loaded
                              if start.value and base <= start.value < end), "?")
                name = f"{name} [{where}]" if name else where
                kernel32.CloseHandle(handle)
                found[(entry.th32ThreadID, name)] = count.value
        ok = kernel32.Thread32Next(snapshot, ctypes.byref(entry))
    kernel32.CloseHandle(snapshot)
    return found


def cycles(pid):
    """CPU cycles process `pid` has used: finer than `cpu_seconds`, whose clock
    advances in scheduler ticks."""
    handle = kernel32.OpenProcess(0x1000, False, pid)
    if not handle:
        raise OSError(ctypes.get_last_error(), "OpenProcess")
    try:
        count = ctypes.c_uint64()
        kernel32.QueryProcessCycleTime(handle, ctypes.byref(count))
        return count.value
    finally:
        kernel32.CloseHandle(handle)


class PROCESS_MEMORY_COUNTERS_EX(ctypes.Structure):
    _fields_ = [("cb", wintypes.DWORD), ("PageFaultCount", wintypes.DWORD),
                ("PeakWorkingSetSize", ctypes.c_size_t), ("WorkingSetSize", ctypes.c_size_t),
                ("QuotaPeakPagedPoolUsage", ctypes.c_size_t), ("QuotaPagedPoolUsage", ctypes.c_size_t),
                ("QuotaPeakNonPagedPoolUsage", ctypes.c_size_t), ("QuotaNonPagedPoolUsage", ctypes.c_size_t),
                ("PagefileUsage", ctypes.c_size_t), ("PeakPagefileUsage", ctypes.c_size_t),
                ("PrivateUsage", ctypes.c_size_t)]


psapi.GetProcessMemoryInfo.argtypes = [H, ctypes.POINTER(PROCESS_MEMORY_COUNTERS_EX), wintypes.DWORD]


def memory(pid):
    """(private bytes, working set bytes) of process `pid`."""
    handle = kernel32.OpenProcess(0x1000, False, pid)
    counters = PROCESS_MEMORY_COUNTERS_EX()
    counters.cb = ctypes.sizeof(counters)
    psapi.GetProcessMemoryInfo(handle, ctypes.byref(counters), counters.cb)
    kernel32.CloseHandle(handle)
    return counters.PrivateUsage, counters.WorkingSetSize
