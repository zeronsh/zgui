"""Native Win32 input for the Windows smoke scripts: the xdotool of the X11 ones.

Standard library only (ctypes): find a process's window, bring it to the
front, click and type through SendInput (the same path real devices take, so
winit sees WM_* messages, not synthesized events), resize its client area and
capture it as PNG. Coordinates are physical client pixels: the script is
per-monitor DPI aware, like the app.
"""
import ctypes
import struct
import time
import zlib
from ctypes import wintypes

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
    "backspace": 0x08, "tab": 0x09, "enter": 0x0D, "return": 0x0D, "escape": 0x1B, "esc": 0x1B,
    "space": 0x20, "pageup": 0x21, "pagedown": 0x22, "end": 0x23, "home": 0x24,
    "left": 0x25, "up": 0x26, "right": 0x27, "down": 0x28, "insert": 0x2D, "delete": 0x2E,
    **{f"f{n}": 0x6F + n for n in range(1, 25)},
}
# Keys whose scan codes need the extended flag (arrows and the navigation block).
EXTENDED = {0x21, 0x22, 0x23, 0x24, 0x25, 0x26, 0x27, 0x28, 0x2D, 0x2E, 0x5B}


# The window `activate` last brought to the front: input goes nowhere else.
_target = None


def _send(*inputs, guard=True):
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
        # mnemonic mode, which took the next key typed.
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
