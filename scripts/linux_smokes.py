#!/usr/bin/env python3
"""Run the Linux native smoke probes concurrently and report each one.

Every probe owns its display or compositor session, so independent probes run
side by side. Probes that expect an ambient X11 desktop get a private
Xvfb/Openbox pair. Build the zgx-desktop examples (and, for the ignored host
tests, `cargo test -p zgx-desktop --lib --no-run`) first. A failed or
timed-out probe fails the run after all the others finish; nothing is retried.
"""
import argparse
import os
import pathlib
import re
import select
import shutil
import signal
import subprocess
import sys
import time
from concurrent.futures import ThreadPoolExecutor, as_completed

# (name, command, ambient X11 desktop). `{bin}` is the examples directory and
# `{out}` the probe's evidence directory, /tmp/zgx-<name> by default.
SMOKES = [
    ("component-host", "python3 scripts/component_host_smoke.py {bin}/component_workload --output {out}", True),
    ("gallery", "python3 scripts/desktop_smoke.py {bin}/gallery", True),
    ("windows", "timeout 30s {bin}/windows --smoke-test", True),
    ("failures", "python3 scripts/failures_smoke.py {bin}/failures", True),
    ("viewport", "python3 scripts/viewport_smoke.py {bin}/viewport", True),
    ("window-controls", "timeout 30s {bin}/window_controls --smoke-test", True),
    ("standard-menu-items", "timeout 30s {bin}/standard_menu_items --smoke-test", True),
    ("form", "python3 scripts/form_smoke.py {bin}/form", True),
    ("terminal", "python3 scripts/terminal_smoke.py {bin}/terminal_pane --output {out}", True),
    ("terminal-orca", "python3 scripts/terminal_smoke.py {bin}/terminal_pane --orca --output {out}", True),
    ("checkbox", "python3 scripts/checkbox_smoke.py {bin}/checkbox --output {out}", True),
    ("slider", "python3 scripts/slider_smoke.py {bin}/slider --output {out}", True),
    ("progress", "python3 scripts/progress_smoke.py {bin}/progress --output {out}", True),
    ("images", "python3 scripts/images_smoke.py {bin}/images --output {out}", True),
    ("scroll", "python3 scripts/scroll_smoke.py {bin}/scroll --output {out}", True),
    ("focus-scroll", "python3 scripts/focus_scroll_smoke.py {bin}/focus_scroll --output {out}", True),
    ("horizontal-scroll", "python3 scripts/horizontal_scroll_smoke.py {bin}/horizontal_scroll --output {out}", True),
    ("scrollbars", "python3 scripts/scrollbars_smoke.py {bin}/scrollbars --output {out}", True),
    ("virtual-keyboard", "python3 scripts/virtual_keyboard_smoke.py {bin}/virtual_keyboard --output {out}", True),
    ("overlays", "python3 scripts/overlays_smoke.py {bin}/overlays --output {out}", True),
    ("menus", "python3 scripts/menus_smoke.py {bin}/menus --output {out}", True),
    ("submenus", "python3 scripts/submenus_smoke.py {bin}/submenus --output {out}", True),
    ("hidden-updates", "python3 scripts/hidden_updates_smoke.py {bin}/hidden_updates --output {out}", True),
    ("hidden-window", "python3 scripts/hidden_window_x11_smoke.py {bin}/hidden_window --output {out}", True),
    ("paint-media", "python3 scripts/paint_media_smoke.py --binaries {bin} --output {out}", True),
    ("cursors", "python3 scripts/cursors_smoke.py {bin}/cursors --output {out}", True),
    ("wayland-cursors", "python3 scripts/wayland_cursors_smoke.py {bin}/cursors --output {out}", False),
    ("rich-text", "python3 scripts/rich_text_smoke.py {bin}/rich_text --output {out}", False),
    ("actions", "python3 scripts/actions_smoke.py {bin}/actions --output {out}", False),
    ("layout", "python3 scripts/layout_smoke.py {bin}/layout --output {out}", False),
    ("drag-drop", "python3 scripts/drag_drop_smoke.py {bin}/drag_drop --output {out}", False),
    ("native-menus", "python3 scripts/native_menu_smoke.py {bin}/dynamic_menus --output {out}", False),
    ("native-menus-wayland", "python3 scripts/native_menu_smoke.py {bin}/dynamic_menus --wayland --output {out}", False),
    ("native-services", "python3 scripts/native_services_smoke.py {bin}/platform_services --native-prompts --output {out}", False),
    ("file-drop", "python3 scripts/file_drop_smoke.py {bin}/drag_drop --output {out}", False),
    ("file-drop-scaled", "python3 scripts/file_drop_smoke.py {bin}/drag_drop --output {out} --scale 1.5", False),
    ("file-drop-wayland", "python3 scripts/wayland_file_drop_smoke.py {bin}/drag_drop --output {out}", False),
    ("variable-virtual-list", "python3 scripts/variable_virtual_list_smoke.py {bin}/variable_virtual_list --output {out}", False),
    ("measured-virtual-list", "python3 scripts/measured_virtual_list_smoke.py {bin}/measured_virtual_list --output {out}", False),
    ("wayland-input", "python3 scripts/wayland_input_smoke.py {bin}/form --output {out}", False),
    ("primary-selection", "python3 scripts/primary_selection_smoke.py {bin}/primary_selection --output {out}", False),
    ("portal-appearance", "python3 scripts/portal_appearance_smoke.py {bin}/primary_selection --output {out}", False),
    ("native-platform", "python3 scripts/native_platform_smoke.py {bin}/native_platform --output {out}", False),
    ("keyboard-layout", "python3 scripts/keyboard_layout_smoke.py {bin}/primary_selection --output {out}", False),
    ("popup-windows", "python3 scripts/popup_windows_smoke.py {bin}/popup_probe --output {out}", False),
    ("wayland-popups", "python3 scripts/wayland_popups_smoke.py {bin}/popup_probe --output {out}", False),
    ("drag-preview", "python3 scripts/drag_preview_smoke.py {bin}/drag_drop --output {out}", False),
    ("drag-windows", "python3 scripts/drag_windows_smoke.py {bin} --output {out}", False),
    ("window-frames", "python3 scripts/window_frames_smoke.py {bin}/window_probe --output {out}", False),
    ("resize-sync", "python3 scripts/resize_sync_smoke.py {bin}/window_probe --output {out}", False),
    ("user-time", "python3 scripts/user_time_smoke.py {bin}/window_probe --output {out}", False),
    ("window-icon", "python3 scripts/window_icon_smoke.py {bin}/window_probe --output {out}", False),
    ("xdnd-source", "python3 scripts/xdnd_source_smoke.py {bin} --output {out}", False),
    ("xdnd-text-drop", "python3 scripts/xdnd_text_drop_smoke.py {bin} --output {out}", False),
    ("wayland-text-drop", "python3 scripts/wayland_text_drop_smoke.py {bin} --output {out}", False),
    ("wayland-drag-out", "python3 scripts/wayland_drag_out_smoke.py {bin} --output {out}", False),
    ("zenity-dialog", "python3 scripts/zenity_dialog_smoke.py {bin}/platform_services --output {out}", False),
    ("portal-dialog", "python3 scripts/portal_dialog_smoke.py {bin}/platform_services --output {out}", False),
    ("web-view-schemes", "timeout 60s {bin}/web_view_schemes --fixture", True),
    ("web-view", "python3 scripts/web_view_smoke.py {bin}/web_view_probe --output {out}", False),
    ("web-view-slow-replies", "python3 scripts/web_view_smoke.py {bin}/web_view_probe --slow-replies --output {out}", False),
    ("web-view-ime-slow-replies", "python3 scripts/web_view_smoke.py {bin}/web_view_probe --ime --slow-replies --output {out}", False),
    ("web-view-wayland", "python3 scripts/web_view_smoke.py {bin}/web_view_probe --wayland --output {out}", False),
    ("web-view-ime", "python3 scripts/web_view_smoke.py {bin}/web_view_probe --ime --output {out}", False),
    ("web-view-transparent", "python3 scripts/web_view_smoke.py {bin}/web_view_probe --transparent --output {out}", False),
    ("web-view-orca", "python3 scripts/web_view_smoke.py {bin}/web_view_probe --orca --output {out}", False),
    ("web-view-media", "python3 scripts/web_view_smoke.py {bin}/web_view_probe --media --output {out}", False),
    ("web-view-wayland-ime", "python3 scripts/web_view_smoke.py {bin}/web_view_probe --wayland --ime --output {out}", False),
    ("launch-token", "python3 scripts/launch_token_smoke.py {bin}/gallery {bin}/native_platform --output {out}", False),
    ("caret-blink", "python3 scripts/caret_blink_smoke.py {bin}/primary_selection --output {out}", False),
    ("double-click-time", "python3 scripts/double_click_time_smoke.py {bin}/primary_selection --output {out}", False),
    ("drag-threshold", "python3 scripts/drag_threshold_smoke.py {bin}/drag_drop --output {out}", False),
    ("primary-paste-setting", "python3 scripts/primary_paste_setting_smoke.py {bin}/primary_selection --output {out}", False),
    ("overlay-scrolling", "python3 scripts/overlay_scrolling_smoke.py {bin}/scrollbars --output {out}", False),
    ("xsettings", "python3 scripts/xsettings_smoke.py {bin}/drag_drop {bin}/primary_selection --output {out}", False),
    ("touchpad-fling", "python3 scripts/touchpad_fling_smoke.py {bin}/scroll --output {out}", False),
    ("window-buttons", "python3 scripts/window_buttons_smoke.py {bin}/glass_counter --output {out}", False),
    ("titlebar-double-click", "python3 scripts/titlebar_double_click_smoke.py {bin}/glass_counter --output {out}", False),
    ("desktop-font", "python3 scripts/desktop_font_smoke.py {bin}/gallery --output {out}", False),
    ("work-area", "python3 scripts/work_area_smoke.py {bin} --output {out}", False),
    ("gl-fallback", "python3 scripts/gl_fallback_smoke.py {bin}/gallery --output {out}", False),
    ("no-fonts", "python3 scripts/no_fonts_smoke.py {bin}/gallery --output {out}", False),
    ("x11-depths", "python3 scripts/x11_depths_smoke.py {bin}/gallery --output {out}", False),
    ("primary-selection-wayland", "python3 scripts/primary_selection_smoke.py {bin}/primary_selection --wayland --output {out}", False),
    ("event-hooks", "python3 scripts/event_hooks_smoke.py {bin}/event_hooks --output {out}", False),
    ("wrapped-editor", "python3 scripts/wrapped_editor_smoke.py {bin}/wrapped_editor --output {out}", False),
    ("editor-paging", "python3 scripts/editor_paging_smoke.py {bin}/editor_paging --output {out}", False),
    ("large-document", "python3 scripts/large_document_smoke.py {bin}/large_document --output {out}", False),
    ("editor-selection", "python3 scripts/editor_selection_smoke.py {bin}/editor_selection --output {out}", False),
    ("selection-copy", "python3 scripts/selection_copy_smoke.py {bin}/selectable_text --output {out}", False),
    ("selection-copy-wayland", "python3 scripts/selection_copy_smoke.py {bin}/selectable_text --wayland --output {out}", False),
    ("clipboard-image", "python3 scripts/clipboard_image_smoke.py {bin}/clipboard_image --output {out}", False),
    ("selectable-text", "python3 scripts/selectable_text_smoke.py {bin}/selectable_text --output {out}", False),
    ("find-in-page", "python3 scripts/find_in_page_smoke.py {bin}/selectable_text --output {out}", False),
    ("zoom", "python3 scripts/zoom_smoke.py {bin}/zoom_probe --output {out}", False),
    ("luau-views", "python3 scripts/luau_views_smoke.py {bin}/luau_probe --output {out}", False),
    ("covered-window", "python3 scripts/covered_window_smoke.py {bin}/../zgx-run --output {out}", False),
    ("inspect-memory", "python3 scripts/inspect_memory_smoke.py {bin}/../zgx-run --output {out}", False),
    ("window-state", "python3 scripts/window_state_smoke.py {bin}/../zgx-run --output {out}", False),
    ("editor-autoscroll", "python3 scripts/editor_autoscroll_smoke.py {bin}/editor_autoscroll --output {out}", False),
    ("transparency", "python3 scripts/transparency_smoke.py {bin}/transparency --output {out}", False),
    ("wayland-transparency", "python3 scripts/wayland_transparency_smoke.py {bin}/transparency --output {out}", False),
    ("native-ime", "python3 scripts/native_ime_smoke.py {bin}/native_ime --output {out}", False),
    ("ime-server-gone", "python3 scripts/native_ime_smoke.py {bin}/native_ime --server-gone --output {out}", False),
    ("ime-server-stopped", "python3 scripts/native_ime_smoke.py {bin}/native_ime --server-stopped --output {out}", False),
    ("ime-password", "python3 scripts/native_ime_smoke.py {bin}/native_ime --password --output {out}", False),
    ("wayland-ime", "python3 scripts/wayland_ime_smoke.py {bin}/native_ime --output {out}", False),
    ("wayland-input-modes", "python3 scripts/wayland_ime_smoke.py {bin}/native_ime --input-modes --output {out}", False),
    ("wayland-ime-burst", "python3 scripts/wayland_ime_smoke.py {bin}/native_ime --burst --output {out}", False),
    ("wayland-external-ime", "python3 scripts/wayland_external_ime_smoke.py {bin}/external_ime --output {out}", False),
    ("wayland-readonly-ime", "python3 scripts/wayland_external_ime_smoke.py {bin}/external_ime --read-only-test --output {out}", False),
    ("held-keys", "python3 scripts/held_keys_smoke.py {bin}/held_keys --output {out}", False),
    ("live-dpi", "python3 scripts/dpi_transition_smoke.py {bin}/dpi_transition --output {out}", False),
    ("live-dpi-pointer", "python3 scripts/live_dpi_pointer_smoke.py {bin}/live_dpi_pointer --output {out}", False),
    ("live-dpi-maximized", "python3 scripts/live_dpi_pointer_smoke.py {bin}/live_dpi_pointer --maximized --output {out}", False),
    ("line-height", "python3 scripts/line_height_smoke.py {bin}/line_height --output {out}", False),
    ("percent-sizes", "python3 scripts/percent_sizes_smoke.py {bin}/percent_sizes --output {out}", False),
    ("letter-spacing", "python3 scripts/letter_spacing_smoke.py {bin}/letter_spacing --output {out}", False),
    ("clipboard-shutdown", "cargo test -p zgx-desktop --lib retained_window_factory_releases_native_clipboard_after_manager_shutdown --locked -- --ignored --test-threads=1", True),
    ("host-suspend", "cargo test -p zgx-desktop --lib host_suspend_recreates_native_resources_and_retains_editor_state --locked -- --ignored --test-threads=1", True),
    ("exit-closes-windows", "cargo test -p zgx-desktop --lib windows_still_open_close_before_the_event_loop_ends --locked -- --ignored --test-threads=1", True),
    ("wheel-tape", "cargo test -p zgx-desktop --lib a_wheel_tape_plays_back_through_the_windows_input_path --locked -- --ignored --test-threads=1", True),
    ("atspi", "/usr/bin/python3 scripts/atspi_smoke.py {bin}/atspi --output {out}", False),
    ("components-atspi", "/usr/bin/python3 scripts/components_atspi_smoke.py {bin}/components_probe --output {out}", False),
    ("image-fit", "python3 scripts/image_fit_smoke.py {bin}/image_fit --output {out}", False),
    ("effects", "python3 scripts/effects_smoke.py {bin}/effects --output {out}", False),
    ("pointer-chords", "python3 scripts/pointer_chords_smoke.py {bin}/pointer_chords --output {out}", False),
    ("keyboard-chords", "python3 scripts/keyboard_chords_smoke.py {bin}/keyboard_chords --output {out}", False),
    ("disabled-interaction", "python3 scripts/disabled_interaction_smoke.py {bin}/disabled_interaction --output {out}", False),
    ("history-limits", "python3 scripts/history_limits_smoke.py {bin}/history_limits --output {out}", False),
    ("external-ime", "python3 scripts/external_ime_smoke.py {bin}/external_ime --output {out}", False),
    ("model-text", "python3 scripts/model_text_smoke.py {bin}/model_text --output {out}", False),
    ("read-only", "python3 scripts/read_only_smoke.py {bin}/read_only --output {out}", False),
    ("readonly-ime", "python3 scripts/external_ime_smoke.py {bin}/external_ime --read-only-test --output {out}", False),
]

# Examples probes run by name rather than as `{bin}/name` in their command
# (the shader warm-up's, and those of probes given the examples directory).
FIXTURES = ("windows", "drag_probe", "zoom_probe", "tab_windows", "submenus", "gallery", "kitchen_sink", "paint_styles",
            "canvas", "svg_transform", "animated_image")


# Fixtures that need a feature of zgx-desktop's, which CI builds with it.
FEATURED = {"web_view_probe": "webkitgtk", "web_view_schemes": "webkitgtk"}


def fixtures():
    """Every zgx-desktop example a probe runs: what CI builds for them, as
    `cargo build` flags. Each must be an example."""
    names = set(FIXTURES)
    for _, command, _ in SMOKES:
        names.update(re.findall(r"\{bin\}/([a-z_0-9]+)", command))
    # Built with their features in a step of their own (ci.yml).
    names -= set(FEATURED)
    examples = pathlib.Path(__file__).resolve().parent.parent / "crates/zgx-desktop/examples"
    missing = [name for name in names if not (examples / f"{name}.rs").exists() and not (examples / name).is_dir()]
    if missing:
        raise SystemExit(f"probes run examples that do not exist: {sorted(missing)}")
    return " ".join(f"--example {name}" for name in sorted(names))


# Seconds each probe took on a 4-vCPU runner; the slowest start first so the
# pool drains evenly. Unlisted probes count as 10.
COST = {
    "editor-selection": 24,
    "effects": 21, "native-ime": 19, "large-document": 15, "external-ime": 12, "readonly-ime": 12,
    "wayland-ime": 8,
    "wayland-input-modes": 8,
    "wayland-ime-burst": 8,
    "native-menus": 12, "native-menus-wayland": 12,
    "wayland-input": 13,
    "clipboard-shutdown": 3, "host-suspend": 3, "exit-closes-windows": 3,
}


def desktop(log):
    """A private Xvfb/Openbox desktop: (processes, environment)."""
    xvfb = subprocess.Popen(["Xvfb", "-r", "-noreset", "-displayfd", "1", "-screen", "0", "1280x900x24", "-ac"],
                            stdout=subprocess.PIPE, stderr=log, text=True)
    if not select.select([xvfb.stdout], [], [], 30)[0]:
        xvfb.kill()
        raise RuntimeError("Xvfb startup timed out")
    display = ":" + xvfb.stdout.readline().strip()
    env = dict(os.environ, DISPLAY=display, XDG_RUNTIME_DIR="/tmp",
               DBUS_SESSION_BUS_ADDRESS="unix:path=/nonexistent")
    env.pop("WAYLAND_DISPLAY", None)
    wm = subprocess.Popen(["openbox"], env=env, stdout=log, stderr=log)
    from platform_smoke import wait_for_wm
    wait_for_wm(env)
    return [wm, xvfb], env


def run(smoke, args):
    name, command, ambient = smoke
    # Probes create their own evidence directory; logs sit beside it.
    output = args.output / f"zgx-{name}"
    shutil.rmtree(output, ignore_errors=True)
    probe_log = args.output / f"zgx-{name}.log"
    argv = [part.format(bin=args.binaries, out=output) for part in command.split()]
    started = time.monotonic()
    helpers, env = [], dict(os.environ)
    with probe_log.open("w") as log, (args.output / f"zgx-{name}-desktop.log").open("w") as desktop_log:
        try:
            if ambient:
                helpers, env = desktop(desktop_log)
            process = subprocess.Popen(argv, env=env, stdout=log, stderr=subprocess.STDOUT,
                                       start_new_session=True)
            try:
                status = process.wait(timeout=args.timeout)
            except subprocess.TimeoutExpired:
                os.killpg(process.pid, signal.SIGKILL)
                process.wait()
                status = "timeout"
        except Exception as error:  # A broken harness is a failed probe.
            print(f"harness error: {error!r}", file=log)
            status = "error"
        finally:
            for helper in helpers:
                helper.terminate()
            for helper in helpers:
                try:
                    helper.wait(timeout=5)
                except subprocess.TimeoutExpired:
                    helper.kill()
    return name, status, time.monotonic() - started, probe_log


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binaries", type=pathlib.Path, default=pathlib.Path("target/debug/examples"))
    parser.add_argument("--output", type=pathlib.Path, default=pathlib.Path("/tmp"))
    parser.add_argument("--jobs", "-j", type=int, default=os.cpu_count())
    parser.add_argument("--timeout", type=float, default=180, help="seconds per probe")
    parser.add_argument("--only", action="append", help="run probes whose name contains this")
    parser.add_argument("--list", action="store_true")
    parser.add_argument("--fixtures", action="store_true",
                        help="print the cargo build flags for the examples the probes run, and exit")
    args = parser.parse_args()
    if args.fixtures:
        print(fixtures())
        return
    smokes = [s for s in SMOKES if not args.only or any(part in s[0] for part in args.only)]
    if args.list:
        print("\n".join(name for name, _, _ in smokes))
        return
    smokes.sort(key=lambda s: -COST.get(s[0], 10))
    args.output.mkdir(parents=True, exist_ok=True)
    group = os.environ.get("GITHUB_ACTIONS") == "true"
    failures, results = [], []
    started = time.monotonic()
    # A fresh runner builds font caches on first use: once, here, rather
    # than in every probe of the first wave at the same time.
    subprocess.run(["fc-cache"], check=False)
    # Likewise shaders: zgx shows a window once its renderer exists, and on
    # the software Vulkan driver a cold shader cache in each of the first
    # probes at once took longer than their wait for the window. Every probe
    # shares one cache, warmed by one window first.
    os.environ.setdefault("MESA_SHADER_CACHE_DIR", str(args.output / "zgx-mesa-shader-cache"))
    # Each probe starts its own Xvfb, many at once: one that loses a display
    # number to a server still closing is started again (`xvfb_retry/Xvfb`).
    os.environ["PATH"] = str(pathlib.Path(__file__).resolve().parent / "xvfb_retry") + os.pathsep + os.environ["PATH"]
    warm = args.output / "zgx-warm-shaders"
    warm.mkdir(exist_ok=True)
    with (warm / "desktop.log").open("w") as log:
        try:
            helpers, env = desktop(log)
            subprocess.run([str(args.binaries / "windows"), "--smoke-test"], env=env, stdout=log,
                           stderr=log, timeout=60, check=False)
        except (OSError, RuntimeError, subprocess.TimeoutExpired) as error:
            print(f"shader warm-up failed: {error!r}", file=log)
            helpers = []
        for helper in helpers:
            helper.terminate()
    jobs = max(1, args.jobs)

    def staggered(index, smoke):
        # The first wave starts half a second apart: cold binaries and
        # shader caches load one at a time.
        if index < jobs:
            time.sleep(index * .5)
        return run(smoke, args)

    with ThreadPoolExecutor(max_workers=jobs) as pool:
        for done in as_completed([pool.submit(staggered, i, smoke) for i, smoke in enumerate(smokes)]):
            name, status, seconds, log = done.result()
            results.append((seconds, name, status))
            ok = status == 0
            text = log.read_text(errors="replace")
            header = f"{'PASS' if ok else 'FAIL'} {name} ({seconds:.1f}s{'' if ok else f', status {status}'})"
            if group and ok:
                print(f"::group::{header}\n{text}::endgroup::", flush=True)
            else:
                print(f"{header}\n{text}", flush=True)
            if not ok:
                failures.append(name)
                for path in sorted((args.output / f"zgx-{name}").rglob("*")):
                    if path.is_file() and path.suffix in (".log", ".json"):
                        body = path.read_text(errors="replace")
                        print(f"::group::{path}\n{body}::endgroup::" if group else f"--- {path}\n{body}", flush=True)
                if group:
                    print(f"::error title=Linux smoke {name} failed::{log}", flush=True)
    print(f"\n{len(smokes)} probes in {time.monotonic() - started:.0f}s wall, "
          f"{sum(r[0] for r in results):.0f}s total, {args.jobs} at a time")
    for seconds, name, status in sorted(results, reverse=True):
        print(f"  {seconds:6.1f}s  {'ok  ' if status == 0 else 'FAIL'}  {name}")
    if failures:
        sys.exit("failed: " + ", ".join(failures))


if __name__ == "__main__":
    main()
