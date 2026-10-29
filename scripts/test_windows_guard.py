"""Which windows a Windows probe's guard takes as its app's
(`windows_guard.is_ours`), from window and process tuples as the guard reads
them: the web view case is WING-PC's (its WebView2 surface, pid 3812, under
the app); the other values stand in for what such windows read as. No GUI,
no Win32: runs anywhere.

    python3 scripts/test_windows_guard.py -v
"""
import importlib.util
from pathlib import Path
import unittest

spec = importlib.util.spec_from_file_location("windows_guard", Path(__file__).with_name("windows_guard.py"))
guard = importlib.util.module_from_spec(spec)
spec.loader.exec_module(guard)

APP = (5240, "zgx web view")
# {pid: (parent pid, exe)} as the guard reads them when a window appears.
PROCESSES = {
    5240: (4012, "web_view.exe"),          # the app the probe launched
    3812: (5240, "msedgewebview2.exe"),    # its WebView2 browser process
    3990: (3812, "msedgewebview2.exe"),    # a renderer under it: a grandchild
    7120: (1188, "msedgewebview2.exe"),    # another app's WebView2
    6620: (904, "consent.exe"),
    6644: (904, "SystemSettings.exe"),
    2216: (904, "ctfmon.exe"),
}


def window(pid, kind, title, size=(800, 600), hwnd=0x1A2B):
    return (hwnd, pid, kind, title, size)


class Ours(unittest.TestCase):
    def ours(self, w, app=APP, processes=PROCESSES):
        return guard.is_ours(w, app, processes)

    def test_the_apps_webview2_surface_is_ours(self):
        self.assertTrue(self.ours(window(3812, "Chrome_WidgetWin_0", "zgx web view")))

    def test_the_apps_popup_window_is_ours(self):
        self.assertTrue(self.ours(window(5240, "ZgxPopup", "", (220, 180))))

    def test_the_apps_system_menu_is_ours(self):
        self.assertTrue(self.ours(window(5240, "#32768", "", (180, 240))))

    def test_another_processs_menu_stops(self):
        self.assertFalse(self.ours(window(6644, "#32768", "", (180, 240))))
        self.assertFalse(self.ours(window(3812, "#32768", "", (180, 240))))

    def test_another_apps_webview2_stops(self):
        self.assertFalse(self.ours(window(7120, "Chrome_WidgetWin_0", "zgx web view")))

    def test_a_webview2_not_directly_under_the_app_stops(self):
        self.assertFalse(self.ours(window(3990, "Chrome_WidgetWin_0", "zgx web view")))

    def test_the_apps_webview2_under_another_title_stops(self):
        self.assertFalse(self.ours(window(3812, "Chrome_WidgetWin_0", "Sign in to your account")))

    def test_another_window_of_the_app_stops(self):
        self.assertFalse(self.ours(window(5240, "Window Class", "zgx web view")))

    def test_a_popup_class_of_another_process_stops(self):
        self.assertFalse(self.ours(window(6644, "ZgxPopup", "")))

    def test_system_windows_stop(self):
        for w in [window(6620, "Credential Dialog Xaml Host", "User Account Control"),
                  window(6644, "ApplicationFrameWindow", "Settings"),
                  window(2216, "IME", "Microsoft Pinyin settings")]:
            self.assertFalse(self.ours(w), w)

    def test_a_process_gone_or_reused_stops(self):
        # The surface's process is not in the snapshot taken as it appeared.
        gone = {k: v for k, v in PROCESSES.items() if k != 3812}
        self.assertFalse(self.ours(window(3812, "Chrome_WidgetWin_0", "zgx web view"), processes=gone))
        # The app itself is gone: its PID may now be another's.
        no_app = {k: v for k, v in PROCESSES.items() if k != 5240}
        self.assertFalse(self.ours(window(3812, "Chrome_WidgetWin_0", "zgx web view"), processes=no_app))
        self.assertFalse(self.ours(window(5240, "ZgxPopup", ""), processes=no_app))

    def test_with_no_app_nothing_is_ours(self):
        self.assertFalse(self.ours(window(3812, "Chrome_WidgetWin_0", "zgx web view"), app=None))

    def test_the_exe_name_is_matched_whatever_its_case(self):
        processes = {**PROCESSES, 3812: (5240, "MSEdgeWebView2.exe")}
        self.assertTrue(self.ours(window(3812, "Chrome_WidgetWin_0", "zgx web view"), processes=processes))


if __name__ == "__main__":
    unittest.main()
