"""Task 2: actions repair coverage and do not hold the GIL during provider waits."""
import json
import os
from pathlib import Path
import subprocess
import sys
import threading
import time
import unittest

@unittest.skipUnless(os.environ.get("BROMIUM_LIVE_TESTS") == "1", "requires interactive Windows desktop")
class ActionContractTests(unittest.TestCase):
    def test_action_invalidation_gil_and_legacy_constructor(self):
        import bromium
        fixture = subprocess.Popen([sys.executable, "-u", str(Path(__file__).with_name("incremental_fixture.py"))],
            stdin=subprocess.PIPE, stdout=subprocess.PIPE, text=True, creationflags=subprocess.CREATE_NO_WINDOW)
        try:
            title = json.loads(fixture.stdout.readline())["titles"][0]
            driver = bromium.WinDriver(timeout_ms=5000, window_title=title)
            edit = driver.get_element_by_xpath("//Edit")
            old = driver.get_element_by_xpath("//Button[@Name='Before']")
            fixture.stdin.write('{"command":"pause"}\n')
            fixture.stdin.flush()
            self.assertEqual(json.loads(fixture.stdout.readline())["done"], "pause")
            progressed = []
            worker = threading.Thread(target=lambda: (time.sleep(0.1), progressed.append(time.perf_counter())))
            worker.start()
            edit.send_text("action update")
            finished = time.perf_counter()
            worker.join(timeout=2)
            self.assertTrue(progressed and progressed[0] < finished, "GIL was held across a provider wait")
            self.assertEqual(driver.get_element_by_xpath("//Button[@Name='Result: action update']").runtime_id, old.runtime_id)
            self.assertEqual(old.name, "Before")  # previously returned properties are snapshots
            manual = bromium.Element(edit.name, edit.xpath, edit.handle, edit.control_type, edit.runtime_id, edit.bounding_rectangle)
            manual.send_text("legacy update")
            self.assertEqual(driver.get_element_by_xpath("//Button[@Name='Result: legacy update']").runtime_id, old.runtime_id)
            fixture.stdin.write('{"command":"remove"}\n')
            fixture.stdin.flush()
            self.assertEqual(json.loads(fixture.stdout.readline())["done"], "remove")
            driver.refresh_region(edit, timeout_ms=5000)
            # Refreshing a sibling doesn't validate the removed sibling's lifetime;
            # provider resolution must still reject the destroyed native handle.
            with self.assertRaises(bromium.ElementNotFoundError):
                old.send_click()
            invalid = bromium.Element("", "", 0, "", [], (0,0,0,0))
            with self.assertRaisesRegex(bromium.ElementNotFoundError, "Empty runtime ID"):
                invalid.send_click()
        finally:
            if fixture.poll() is None:
                try:
                    fixture.communicate('{"command":"quit"}\n', timeout=5)
                except subprocess.TimeoutExpired:
                    fixture.terminate()
                    fixture.wait(timeout=5)

if __name__ == "__main__":
    unittest.main(testRunner=unittest.TextTestRunner(stream=sys.stdout, verbosity=2))
