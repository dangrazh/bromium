"""Opt-in shared-Rust point capture check; only creates/raises the controlled fixture.

Build first: cargo build -p uitree --example point_smoke --target-dir target/close-tests
Run with BROMIUM_LIVE_TESTS=1. No installed bromium wheel is required.
"""
import ctypes as c
from ctypes import wintypes as w
import json
import os
from pathlib import Path
import subprocess
import sys
import unittest


@unittest.skipUnless(os.environ.get("BROMIUM_LIVE_TESTS") == "1", "requires interactive Windows desktop")
class PointCaptureLiveTests(unittest.TestCase):
    def test_narrow_capture_and_live_resolution(self):
        root = Path(__file__).resolve().parents[3]
        probe = root / "target/close-tests/debug/examples/point_smoke.exe"
        self.assertTrue(probe.is_file(), "build the point_smoke example first")
        fixture = subprocess.Popen(
            [sys.executable, "-u", str(Path(__file__).with_name("incremental_fixture.py"))],
            stdin=subprocess.PIPE, stdout=subprocess.PIPE, text=True,
            creationflags=subprocess.CREATE_NO_WINDOW,
        )
        try:
            ready = json.loads(fixture.stdout.readline())
            user = c.WinDLL("user32", use_last_error=True)
            user.GetAncestor.argtypes = [w.HWND, w.UINT]
            user.GetAncestor.restype = w.HWND
            user.SetWindowPos.argtypes = [w.HWND, w.HWND, c.c_int, c.c_int, c.c_int, c.c_int, w.UINT]
            user.GetWindowRect.argtypes = [w.HWND, c.POINTER(w.RECT)]
            button = ready["button"]
            # Raise only this test's own window; never activate or type into user apps.
            self.assertTrue(user.SetWindowPos(user.GetAncestor(button, 2), -1, 0, 0, 0, 0, 0x13))
            rect = w.RECT()
            self.assertTrue(user.GetWindowRect(button, c.byref(rect)))
            result = subprocess.run(
                [str(probe), ready["titles"][0], str((rect.left + rect.right) // 2),
                 str((rect.top + rect.bottom) // 2), "Before"],
                capture_output=True, text=True, timeout=40, creationflags=subprocess.CREATE_NO_WINDOW,
            )
            print(result.stdout)
            self.assertEqual(result.returncode, 0, result.stderr)
        finally:
            if fixture.poll() is None:
                try:
                    fixture.communicate('{"command":"quit"}\n', timeout=5)
                except subprocess.TimeoutExpired:
                    fixture.terminate()
                    fixture.wait(timeout=5)


if __name__ == "__main__":
    unittest.main(verbosity=2)
