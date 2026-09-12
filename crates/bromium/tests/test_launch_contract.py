"""Task 1 live regression: opt in with BROMIUM_LIVE_TESTS=1."""
import json
import os
from pathlib import Path
import subprocess
import sys
import unittest

@unittest.skipUnless(os.environ.get("BROMIUM_LIVE_TESTS") == "1", "requires interactive Windows desktop")
class LaunchContractTests(unittest.TestCase):
    def test_descendant_activation_and_stale_never_launch(self):
        import bromium
        fixture = subprocess.Popen([sys.executable, "-u", str(Path(__file__).with_name("incremental_fixture.py"))],
            stdin=subprocess.PIPE, stdout=subprocess.PIPE, text=True, creationflags=subprocess.CREATE_NO_WINDOW)
        try:
            title = json.loads(fixture.stdout.readline())["titles"][0]
            driver = bromium.WinDriver(timeout_ms=5000, window_title=title)
            # A nonexistent executable makes accidental launch an observable failure.
            executable = str(Path(__file__).with_name("must_not_launch_missing.exe"))
            element = driver.launch_or_activate_app(executable, "//Button[@Name='Before']")
            self.assertEqual(element.name, "Before")
            self.assertEqual(driver.window_title, title)
            driver.timeout_ms = 0
            with self.assertRaises(bromium.StaleTreeError) as caught:
                driver.launch_or_activate_app(executable, "//Button[@Name='Before']")
            for field in ("reason", "scope", "revision", "coverage"):
                self.assertTrue(hasattr(caught.exception, field))
            with self.assertRaises(ValueError):
                driver.launch_or_activate_app(executable, "//[")
        finally:
            if fixture.poll() is None:
                try:
                    fixture.communicate('{"command":"quit"}\n', timeout=5)
                except subprocess.TimeoutExpired:
                    fixture.terminate()
                    fixture.wait(timeout=5)

if __name__ == "__main__":
    unittest.main(testRunner=unittest.TextTestRunner(stream=sys.stdout, verbosity=2))
