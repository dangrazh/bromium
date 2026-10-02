"""Opt-in UI Explore regression on a controlled fixture, never Teams.

Build uiexplore, install bromium in the invoking Python environment, then:
  $env:BROMIUM_LIVE_TESTS='1'
  $env:UIEXPLORE_EXE='target/close-tests/debug/uiexplore.exe'
  python crates/uiexplore/tests/test_gui_live.py

Only activates controls in the owned UI Explore process; both owned processes are cleaned up.
"""
import ctypes as c
from contextlib import ExitStack
from ctypes import wintypes as w
import json
import os
from pathlib import Path
import queue
import re
import shutil
import subprocess
import sys
import tempfile
import threading
import time
import unittest

ROOT = Path(__file__).resolve().parents[3]


@unittest.skipUnless(os.environ.get("BROMIUM_LIVE_TESTS") == "1", "requires interactive Windows desktop")
class ExplorerLiveTests(unittest.TestCase):
    def test_root_lazy_expansion_self_exclusion_and_bounded_updates(self):
        import bromium
        user = c.WinDLL("user32", use_last_error=True)
        user.SetProcessDPIAware()
        user.GetWindowThreadProcessId.argtypes = [w.HWND, c.POINTER(w.DWORD)]
        user.GetWindowTextW.argtypes = [w.HWND, w.LPWSTR, c.c_int]
        user.GetAncestor.argtypes = [w.HWND, w.UINT]
        user.GetAncestor.restype = w.HWND
        user.SetWindowTextW.argtypes = [w.HWND, w.LPCWSTR]
        user.SetWindowPos.argtypes = [w.HWND, w.HWND, c.c_int, c.c_int, c.c_int, c.c_int, w.UINT]
        user.GetWindowRect.argtypes = [w.HWND, c.POINTER(w.RECT)]
        user.GetCursorPos.argtypes = [c.POINTER(w.POINT)]
        user.SetCursorPos.argtypes = [c.c_int, c.c_int]
        original_cursor = w.POINT()
        restore_cursor = bool(user.GetCursorPos(c.byref(original_cursor)))
        user.ScreenToClient.argtypes = [w.HWND, c.POINTER(w.POINT)]
        user.GetClientRect.argtypes = [w.HWND, c.POINTER(w.RECT)]
        user.SendMessageTimeoutW.argtypes = [w.HWND, w.UINT, w.WPARAM, w.LPARAM, w.UINT, w.UINT, c.POINTER(c.c_size_t)]
        user.SendMessageTimeoutW.restype = c.c_ssize_t
        callback = c.WINFUNCTYPE(w.BOOL, w.HWND, w.LPARAM)
        user.EnumWindows.argtypes = [callback, w.LPARAM]
        exe = Path(os.environ.get("UIEXPLORE_EXE", str(ROOT / "target/debug/uiexplore.exe"))).resolve()
        self.assertTrue(exe.is_file(), f"Build UI Explore first: {exe}")
        fixture = subprocess.Popen(
            [sys.executable, "-u", str(ROOT / "crates/bromium/tests/incremental_fixture.py")],
            stdin=subprocess.PIPE, stdout=subprocess.PIPE, text=True,
            creationflags=subprocess.CREATE_NO_WINDOW,
        )
        app = None
        try:
            ready_lines = queue.Queue()
            threading.Thread(target=lambda: ready_lines.put(fixture.stdout.readline()), daemon=True).start()
            ready = json.loads(ready_lines.get(timeout=10))
            title_a, title_b = ready["titles"]
            fixture_window = user.GetAncestor(ready["button"], 2)  # GA_ROOT
            with tempfile.TemporaryDirectory(prefix="uiexplore-regression-") as temp:
                log_path = Path(temp) / "app.log"
                with log_path.open("w", encoding="utf-8") as log, ExitStack() as cleanup:
                    app = subprocess.Popen([str(exe)], stdout=log, stderr=log,
                                           env={**os.environ, "RUST_LOG": "uitree=debug,uiexplore=debug"},
                                           creationflags=subprocess.CREATE_NO_WINDOW)
                    def stop_app():
                        if app.poll() is None:
                            app.terminate()
                            app.wait(timeout=5)
                        artifact = ROOT / "target/uiexplore-live.log"
                        artifact.parent.mkdir(parents=True, exist_ok=True)
                        shutil.copyfile(log_path, artifact)
                    cleanup.callback(stop_app)

                    def owned_window():
                        found = []
                        def visit(hwnd, _):
                            pid = w.DWORD()
                            user.GetWindowThreadProcessId(hwnd, c.byref(pid))
                            name = c.create_unicode_buffer(512)
                            user.GetWindowTextW(hwnd, name, len(name))
                            if pid.value == app.pid and name.value == "UI Explore":
                                found.append(hwnd)
                            return True
                        user.EnumWindows(callback(visit), 0)
                        return found[0] if found else None

                    def wait_for(fn, description, seconds=15):
                        until = time.monotonic() + seconds
                        while time.monotonic() < until:
                            self.assertIsNone(app.poll(), "UI Explore exited unexpectedly")
                            try:
                                result = fn()
                            except bromium.StaleTreeError:
                                result = None
                            if result:
                                return result
                            time.sleep(0.1)
                        self.fail(f"Timed out: {description}")

                    hwnd = wait_for(owned_window, "owned UI Explore window")
                    own_title = f"UI Explore Regression {app.pid}"
                    user.SetWindowTextW(hwnd, own_title)
                    user.SetWindowPos(hwnd, None, 30, 30, 1250, 900, 0x0040)
                    driver = bromium.WinDriver(timeout_ms=5000, window_title=own_title)

                    def elements():
                        # This external test observer must sample the actual GUI, not
                        # assert against its own previous accessibility snapshot.
                        driver.refresh()
                        return driver.snapshot_elements()

                    def row(prefix):
                        return next((e for e in elements() if e.name.startswith(prefix)), None)

                    def responsive():
                        result = c.c_size_t()
                        self.assertTrue(user.SendMessageTimeoutW(hwnd, 0, 0, 0, 2, 500, c.byref(result)), "UI Explore did not respond within 500 ms")

                    def click(element):
                        pid = w.DWORD()
                        user.GetWindowThreadProcessId(hwnd, c.byref(pid))
                        self.assertEqual(pid.value, app.pid)
                        left, top, right, bottom = element.bounding_rectangle
                        point = w.POINT(left + 8, (top + bottom) // 2)
                        user.ScreenToClient(hwnd, c.byref(point))
                        rect = w.RECT()
                        user.GetClientRect(hwnd, c.byref(rect))
                        self.assertTrue(0 <= point.x < rect.right and 0 <= point.y < rect.bottom,
                                        f"Row is not visible: {element.name}")
                        element.send_click()
                        time.sleep(0.2)  # Wait for input and accessibility events, not a cache-only read.

                    wait_for(lambda: row("'Desktop"), "visible Desktop root")
                    lazy = wait_for(lambda: row(f"'{title_a}'"), "fixture branch")
                    self.assertIn("[not captured]", lazy.name, "Fixture was eagerly captured")
                    click(lazy)
                    wait_for(lambda: row("'Before'"), "children after expanding lazy branch")
                    sibling = wait_for(lambda: row(f"'{title_b}'"), "unexpanded sibling")
                    self.assertIn("[not captured]", sibling.name, "Expansion unnecessarily captured sibling")

                    def excluded():
                        rows = elements()
                        self.assertTrue(rows)
                        self.assertFalse(any(e.name.startswith((f"'{own_title}'", "'UI Explore'")) for e in rows),
                                         "UI Explore captured itself")
                        return rows

                    def counts(rows):
                        count = next(int(m.group(1)) for e in rows if (m := re.fullmatch(r"(\d+) Elements detected", e.name)))
                        revision = next(int(m.group(1)) for e in rows if (m := re.match(r"revision=(\d+)", e.name)))
                        return count, revision

                    first_count, first_revision = counts(excluded())
                    log_offset = len(log_path.read_text(encoding="utf-8", errors="replace"))
                    for _ in range(3):
                        click(wait_for(lambda: row(f"'{title_a}'"), "expanded fixture"))
                        wait_for(lambda: not row("'Before'"), "collapsed selected branch")
                        click(wait_for(lambda: row(f"'{title_a}'"), "collapsed fixture"))
                        wait_for(lambda: row("'Before'"), "reopened selected branch")
                        responsive()
                    time.sleep(2)
                    final_count, final_revision = counts(excluded())
                    responsive()
                    capture_log = log_path.read_text(encoding="utf-8", errors="replace")
                    scheduled = re.findall(r"tree_schedule .*window_handle=(\d+)", capture_log)
                    subscribed = re.findall(r"tree_subscribe .*window_handle=(\d+)", capture_log)
                    self.assertTrue(scheduled, "Missing scoped capture diagnostics")
                    self.assertNotIn(str(hwnd), scheduled, "UI Explore scheduled capture of itself")
                    self.assertNotIn(str(hwnd), subscribed, "UI Explore subscribed to its own window")
                    # Global revisions also include unrelated desktop events. Bound work
                    # caused by this controlled branch, not another application's activity.
                    fixture_captures = re.findall(
                        rf"tree_schedule .*window_handle={fixture_window}(?:\s|$)", capture_log[log_offset:])
                    self.assertLessEqual(len(fixture_captures), 4,
                                         "Repeated collapse/reopen recaptured unchanged fixture contents")
                    print(f"GUI PASS: nodes {first_count}->{final_count}, revision {first_revision}->{final_revision}", flush=True)

                    def control(name):
                        return next((e for e in elements() if e.name == name), None)

                    def point_at_control(element):
                        left, top, right, bottom = element.bounding_rectangle
                        self.assertTrue(user.SetCursorPos((left + right) // 2, (top + bottom) // 2))

                    # The only physical cursor movements in this test are over the
                    # owned inspector/fixture, restored on exit. Never click user apps.
                    toggle = wait_for(lambda: control("Track Cursor"), "tracking checkbox")
                    point_at_control(toggle)
                    click(toggle)
                    self.assertTrue(user.SetWindowPos(fixture_window, -1, 80, 180, 420, 280, 0x0050))
                    button_rect = w.RECT()
                    self.assertTrue(user.GetWindowRect(ready["button"], c.byref(button_rect)))
                    self.assertTrue(user.SetCursorPos((button_rect.left + button_rect.right) // 2,
                                                     (button_rect.top + button_rect.bottom) // 2))
                    wait_for(lambda: control("Before"), "point-selected button details", seconds=25)
                    tracking_offset = len(log_path.read_text(encoding="utf-8", errors="replace"))
                    for _ in range(3):
                        time.sleep(1)
                        responsive()
                    tracking_log = log_path.read_text(encoding="utf-8", errors="replace")[tracking_offset:]
                    requests = re.findall(r"cursor_tracking request", tracking_log)
                    self.assertLessEqual(len(requests), 4, "stationary pointer caused a capture storm")

                    toggle = wait_for(lambda: control("Track Cursor"), "stop tracking checkbox")
                    point_at_control(toggle)
                    click(toggle)
                    frozen = wait_for(lambda: row("Paused snapshot"), "explicit paused snapshot label")
                    frozen_label = frozen.name
                    self.assertIsNotNone(control("Before"), "stopping tracking cleared the last hit")
                    fixture.stdin.write('{"command":"rename"}\n')
                    fixture.stdin.flush()
                    responses = queue.Queue()
                    threading.Thread(target=lambda: responses.put(fixture.stdout.readline()), daemon=True).start()
                    self.assertEqual(json.loads(responses.get(timeout=5))["done"], "rename")
                    for _ in range(3):
                        time.sleep(0.5)
                        rows = elements()
                        self.assertTrue(any(e.name == frozen_label for e in rows), "background commit replaced frozen revision")
                        self.assertTrue(any(e.name == "Before" for e in rows), "background update changed frozen details")
                        self.assertFalse(any(e.name == "After" for e in rows))
                        responsive()

                    click(wait_for(lambda: control("🔄"), "refresh frozen snapshot"))
                    wait_for(lambda: control("After"), "refreshed current button details")
                    self.assertFalse(any(e.name.startswith("Paused snapshot") for e in elements()))
                    print(f"TRACKING PASS: {len(requests)} requests in 3s; freeze, retained details and refresh verified", flush=True)
                    app.terminate()
                    app.wait(timeout=5)
        finally:
            if restore_cursor:
                user.SetCursorPos(original_cursor.x, original_cursor.y)
            if app is not None and app.poll() is None:
                app.terminate()
                app.wait(timeout=5)
            if fixture.poll() is None:
                fixture.stdin.write('{"command":"quit"}\n')
                fixture.stdin.flush()
                try:
                    fixture.communicate(timeout=5)
                except subprocess.TimeoutExpired:
                    fixture.terminate()
                    fixture.communicate(timeout=5)


if __name__ == "__main__":
    unittest.main(verbosity=2)
