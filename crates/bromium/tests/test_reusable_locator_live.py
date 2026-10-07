"""Opt-in coordinate-to-XPath reuse across two controlled application processes.

Requires a rebuilt installed wheel and BROMIUM_LIVE_TESTS=1. No user-app actions.
Full-desktop verification is covered by the Rust mock suite, not triggered here.
"""
import ctypes as c
from ctypes import wintypes as w
import json
import os
from pathlib import Path
import subprocess
import sys
import unittest
import uuid


@unittest.skipUnless(os.environ.get("BROMIUM_LIVE_TESTS") == "1", "requires interactive desktop")
class ReusableLocatorLiveTests(unittest.TestCase):
    def test_two_application_processes_and_drivers_share_locator(self):
        import bromium
        stable_title = "reusable-" + uuid.uuid4().hex
        paths, pids = [], []
        for _ in range(2):
            fixture = subprocess.Popen(
                [sys.executable, "-u", str(Path(__file__).with_name("incremental_fixture.py"))],
                stdin=subprocess.PIPE, stdout=subprocess.PIPE, text=True,
                creationflags=subprocess.CREATE_NO_WINDOW,
                env={**os.environ, "BROMIUM_FIXTURE_IDENTITY": stable_title},
            )
            try:
                ready = json.loads(fixture.stdout.readline())
                pids.append(ready["pid"])
                user = c.WinDLL("user32", use_last_error=True)
                user.GetAncestor.argtypes = [w.HWND, w.UINT]
                user.GetAncestor.restype = w.HWND
                user.SetWindowPos.argtypes = [w.HWND, w.HWND, c.c_int, c.c_int, c.c_int, c.c_int, w.UINT]
                user.GetWindowRect.argtypes = [w.HWND, c.POINTER(w.RECT)]
                self.assertTrue(user.SetWindowPos(user.GetAncestor(ready["button"], 2), -1, 0, 0, 0, 0, 0x13))
                rect = w.RECT()
                self.assertTrue(user.GetWindowRect(ready["button"], c.byref(rect)))
                driver = bromium.WinDriver(timeout_ms=10000, window_title=ready["titles"][0])
                element = driver.get_element_by_coordinates((rect.left+rect.right)//2, (rect.top+rect.bottom)//2)
                self.assertEqual(element.name, "Before")
                self.assertIn(element.xpath_status, ("candidate", "snapshot_unique"))
                for forbidden in ("NodeKey", "RtID", "NativeWindowHandle"):
                    self.assertNotIn(forbidden, element.xpath)
                paths.append(element.xpath)
                # In the second process, execute the XPath saved from the first.
                resolved = driver.get_element_by_xpath(paths[0])
                self.assertEqual(resolved.runtime_id, element.runtime_id)
                self.assertEqual(resolved.handle, element.handle)
                print(f"pid={ready['pid']} status={element.xpath_status} xpath={element.xpath}")
                del resolved, element, driver
            finally:
                if fixture.poll() is None:
                    try:
                        fixture.communicate('{"command":"quit"}\n', timeout=5)
                    except subprocess.TimeoutExpired:
                        fixture.terminate()
                        fixture.wait(timeout=5)
        self.assertNotEqual(pids[0], pids[1])
        self.assertEqual(paths[0], paths[1])


if __name__ == "__main__":
    unittest.main(verbosity=2)
