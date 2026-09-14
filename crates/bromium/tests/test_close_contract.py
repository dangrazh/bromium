"""Opt-in closure tests: only windows owned by incremental_fixture.py are touched."""
import os
import sys
import threading
import time
import unittest

from test_app_start_incremental import fixture, wait_for


@unittest.skipUnless(os.environ.get("BROMIUM_LIVE_TESTS") == "1", "requires controlled Windows desktop fixtures")
class CloseContractTests(unittest.TestCase):
    def test_close_window_repairs_membership_and_releases_gil(self):
        import bromium
        self.assertTrue(hasattr(bromium.Element, "close"), "Element.close is not exported")
        with fixture() as (titles, command):
            driver = bromium.WinDriver(timeout_ms=5000, window_title=titles[0])
            window_xpath = f"//*[@Name='{titles[0]}']"
            window = driver.get_element_by_xpath(window_xpath)
            command("pause")
            progressed = []
            worker = threading.Thread(target=lambda: (time.sleep(0.1), progressed.append(time.perf_counter())))
            worker.start()
            try:
                self.assertIsNone(window.close())
                finished = time.perf_counter()
            finally:
                worker.join(timeout=2)
            self.assertTrue(progressed and progressed[0] < finished,
                            "Python thread could not progress while close waited on the provider")
            wait_for(lambda: not driver.get_elements_by_xpath(window_xpath))
            self.assertEqual(window.name, titles[0])  # immutable captured properties
            self.assertEqual(driver.window_title, titles[0])
            with self.assertRaises(bromium.ElementNotFoundError):
                window.close()
            # Closing one window must not terminate its process or close its sibling.
            driver.window_title = titles[1]
            self.assertEqual(driver.get_element_by_xpath(f"//*[@Name='{titles[1]}']").name, titles[1])
            command("pause")  # the fixture process still responds

    def test_unsupported_and_obsolete_elements_never_close_ancestor(self):
        import bromium
        self.assertTrue(hasattr(bromium.Element, "close"), "Element.close is not exported")
        with fixture() as (titles, command):
            driver = bromium.WinDriver(timeout_ms=5000, window_title=titles[0])
            edit = driver.get_element_by_xpath("//Edit")
            with self.assertRaisesRegex(bromium.AutomationError, "does not support window closure"):
                edit.close()
            old = driver.get_element_by_xpath("//Button[@Name='Before']")
            command("remove")
            wait_for(lambda: not driver.get_elements_by_xpath("//Button[@Name='Before']"))
            command("replace")
            replacement = driver.get_element_by_xpath("//Button[@Name='Replacement']")
            with self.assertRaises(bromium.ElementNotFoundError):
                old.close()
            self.assertEqual(driver.get_element_by_xpath("//Button[@Name='Replacement']").runtime_id,
                             replacement.runtime_id)
            self.assertEqual(driver.get_element_by_xpath(f"//*[@Name='{titles[0]}']").name, titles[0])


if __name__ == "__main__":
    unittest.main(testRunner=unittest.TextTestRunner(stream=sys.stdout, verbosity=2))
