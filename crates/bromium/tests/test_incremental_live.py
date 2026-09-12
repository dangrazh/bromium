"""Opt-in live integration: BROMIUM_LIVE_TESTS=1, installed development wheel."""
import json
import os
from pathlib import Path
import subprocess
import sys
import threading
import time
import unittest

@unittest.skipUnless(os.environ.get("BROMIUM_LIVE_TESTS") == "1", "requires interactive Windows desktop")
class IncrementalLiveTests(unittest.TestCase):
    def test_mutations_scope_and_invalid_query(self):
        import bromium
        fixture = subprocess.Popen(
            [sys.executable, "-u", str(Path(__file__).with_name("incremental_fixture.py"))],
            stdin=subprocess.PIPE, stdout=subprocess.PIPE, text=True,
            creationflags=subprocess.CREATE_NO_WINDOW,
        )
        try:
            ready = json.loads(fixture.stdout.readline())
            title_a, title_b = ready["titles"]
            driver = bromium.WinDriver(timeout_ms=5000, window_title=title_a)
            initial = driver.get_element_by_xpath("//*[@Name='Before']")
            self.assertEqual(initial.name, "Before")
            start = time.perf_counter()
            for _ in range(5):
                self.assertEqual(driver.get_element_by_xpath("//*[@Name='Before']").runtime_id, initial.runtime_id)
            print(f"warm_query_mean_ms={(time.perf_counter()-start)*200:.2f}")
            def command(name):
                fixture.stdin.write(json.dumps({"command": name}) + "\n")
                fixture.stdin.flush()
                self.assertEqual(json.loads(fixture.stdout.readline())["done"], name)
                time.sleep(0.2)  # allow the OS to deliver invalidation notifications
            command("rename")
            self.assertEqual(driver.get_element_by_xpath("//*[@Name='After']").runtime_id, initial.runtime_id)
            self.assertEqual(driver.get_elements_by_xpath("//*[@Name='Before']"), [])
            driver.refresh_region(initial, timeout_ms=5000)
            self.assertEqual(driver.window_title, title_a)
            command("move")
            moved = driver.get_element_by_xpath("//*[@Name='After']")
            self.assertNotEqual(moved.bounding_rectangle, initial.bounding_rectangle)
            command("remove")
            self.assertEqual(driver.get_elements_by_xpath("//*[@Name='After']"), [])
            with self.assertRaises(ValueError):
                driver.get_elements_by_xpath("//[")
            driver.refresh(window_title=title_b)
            self.assertEqual(driver.window_title, title_b)
            self.assertFalse(any(e.name == title_a for e in driver.snapshot_elements()))
            self.assertTrue(issubclass(bromium.StaleTreeError, TimeoutError))
            # A clean no-match wait releases the GIL and respects the single deadline.
            progressed = threading.Event()
            thread = threading.Thread(target=lambda: (time.sleep(0.05), progressed.set()))
            thread.start()
            start = time.perf_counter()
            with self.assertRaises(bromium.ElementNotFoundError):
                driver.get_element_by_xpath("//*[@Name='Definitely absent fixture control']", timeout_ms=300)
            self.assertLess(time.perf_counter() - start, 1.0)
            self.assertTrue(progressed.is_set())
            thread.join(timeout=1)
            # A new driver's shallow snapshot must not be treated as full coverage.
            shallow = bromium.WinDriver(timeout_ms=5000, window_title=title_b)
            with self.assertRaises(bromium.StaleTreeError) as stale:
                shallow.get_element_by_xpath("//Button", timeout_ms=0)
            for field in ("reason", "scope", "revision", "coverage"):
                self.assertTrue(hasattr(stale.exception, field))
            print(driver.tree_status)
        finally:
            if fixture.poll() is None:
                try:
                    fixture.stdin.write('{"command":"quit"}\n')
                    fixture.stdin.flush()
                    fixture.communicate(timeout=5)
                except (BrokenPipeError, subprocess.TimeoutExpired):
                    fixture.terminate()
                    fixture.wait(timeout=5)

if __name__ == "__main__":
    unittest.main(testRunner=unittest.TextTestRunner(stream=sys.stdout, verbosity=2))
