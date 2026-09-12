"""Assertion-based successor to app_start_danipc.py. See README_INCREMENTAL.md.

Default: offline workflow checks only. --live: controlled Windows fixtures.
--teams: explicitly allow typing search text in Teams; never sends a message.
"""
import argparse
from contextlib import contextmanager
from dataclasses import dataclass
import json
import os
from pathlib import Path
import queue
import subprocess
import sys
import tempfile
import threading
import time
import unittest
from unittest.mock import Mock, patch

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[2]


@dataclass
class Workflow:
    executable: str = "ms-teams.exe"
    title: str = "Microsoft Teams"
    app_xpath: str = "//Window[contains(@Name,'Microsoft Teams')]"
    search_xpath: str = "//ComboBox[@Name='Suche']"
    result_xpath: str = "//Group[@Name='Personen']/ListItem[1]"
    text: str = "Test"
    timeout_ms: int = 15000
    select_result: bool = False
    log_path: str = str(ROOT / "target/followup-logs")


def run_workflow(api, config):
    """Use Rust's bounded query waits; no routine refresh or fixed sleeps."""
    api.init_logging(log_path=config.log_path, log_level="Warn", enable_console=True, enable_file=False)
    driver = api.WinDriver(timeout_ms=config.timeout_ms, window_title=config.title)
    app = driver.launch_or_activate_app(config.executable, config.app_xpath)
    assert app.runtime_id, "launch returned no live identity"
    search = driver.get_element_by_xpath(config.search_xpath, timeout_ms=config.timeout_ms)
    search.send_text(config.text)
    result = driver.get_element_by_xpath(config.result_xpath, timeout_ms=config.timeout_ms)
    assert result.runtime_id, "result has no identity"
    if config.select_result:
        result.send_click()
    return driver, result


def wait_for(predicate, timeout=5):
    deadline = time.monotonic() + timeout
    while True:
        value = predicate()  # do not swallow malformed XPath/provider exceptions
        if value:
            return value
        remaining = deadline - time.monotonic()
        if remaining <= 0:
            raise AssertionError("condition did not become true before deadline")
        threading.Event().wait(min(0.02, remaining))


@contextmanager
def fixture():
    process = subprocess.Popen([sys.executable, "-u", str(HERE / "incremental_fixture.py")],
        stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.PIPE,
        text=True, creationflags=subprocess.CREATE_NO_WINDOW)
    replies = queue.Queue()
    def read():
        for line in process.stdout:
            replies.put(json.loads(line))
        replies.put({"error": "fixture exited before reply"})
    reader = threading.Thread(target=read, daemon=True)
    reader.start()
    def receive():
        try:
            reply = replies.get(timeout=10)
        except queue.Empty as error:
            raise AssertionError("fixture reply deadline expired") from error
        assert "error" not in reply, reply
        return reply
    def command(name):
        process.stdin.write(json.dumps({"command": name}) + "\n")
        process.stdin.flush()
        assert receive() == {"done": name}
    try:
        yield receive()["titles"], command
    finally:
        if process.poll() is None:
            try:
                process.stdin.write('{"command":"quit"}\n')
                process.stdin.flush()
                process.wait(timeout=5)
            except (BrokenPipeError, subprocess.TimeoutExpired):
                process.terminate()  # only this test's child
                process.wait(timeout=5)
        reader.join(timeout=2)
        for stream in (process.stdin, process.stdout, process.stderr):
            stream.close()


class WorkflowUnitTests(unittest.TestCase):
    def test_configured_flow_never_refreshes_or_selects_by_default(self):
        api = Mock()
        config = Workflow(timeout_ms=5, text="configured text")
        driver, result = run_workflow(api, config)
        api.WinDriver.assert_called_once_with(timeout_ms=5, window_title=config.title)
        driver.launch_or_activate_app.assert_called_once_with(config.executable, config.app_xpath)
        driver.get_element_by_xpath.assert_any_call(config.search_xpath, timeout_ms=5)
        result.send_text.assert_called_once_with("configured text")
        driver.refresh.assert_not_called()
        result.send_click.assert_not_called()

    def test_errors_propagate_and_selection_is_separate(self):
        api = Mock()
        api.WinDriver.return_value.launch_or_activate_app.side_effect = TimeoutError("deadline")
        with self.assertRaisesRegex(TimeoutError, "deadline"):
            run_workflow(api, Workflow())
        api = Mock()
        _, result = run_workflow(api, Workflow(select_result=True))
        result.send_click.assert_called_once_with()


@unittest.skipUnless(os.environ.get("BROMIUM_LIVE_TESTS") == "1", "controlled desktop tests require --live")
class ControlledWorkflowTests(unittest.TestCase):
    def test_absent_launch_then_existing_descendant_search(self):
        import bromium
        launcher = Path(os.environ.get("BROMIUM_FIXTURE_LAUNCHER", str(ROOT / "target/fixture_launcher.exe")))
        self.assertTrue(launcher.is_file(), "setup failure: build fixture_launcher.exe per README_INCREMENTAL.md")
        with tempfile.TemporaryDirectory(prefix="bromium-launch-") as directory:
            session = Path(directory)
            title = f"Bromium Incremental A {session.name}"
            environment = {"BROMIUM_FIXTURE_PYTHON": sys.executable,
                           "BROMIUM_FIXTURE_SCRIPT": str(HERE / "incremental_fixture.py"),
                           "BROMIUM_FIXTURE_SESSION": str(session)}
            with patch.dict(os.environ, environment):
                try:
                    config = Workflow(str(launcher), title, "//Button[@Name='Before']",
                                      "//Edit", "//Button[@Name='Result: Test']")
                    driver, result = run_workflow(bromium, config)
                    ready = json.loads((session / "ready.json").read_text(encoding="utf-8"))
                    self.assertEqual(ready["titles"][0], title)
                    existing = driver.launch_or_activate_app(str(session / "must-not-launch.exe"), config.result_xpath)
                    self.assertEqual(existing.runtime_id, result.runtime_id)
                    self.assertEqual(driver.window_title, title)
                finally:
                    (session / "stop").touch()
                    # Even a late native launch observes the stop sentinel.
                    if (session / "ready.json").exists():
                        wait_for(lambda: (session / "done").exists(), 10)

    def test_replacement_scope_and_repeated_lifetimes(self):
        import bromium
        for cycle in range(3):
            with self.subTest(cycle=cycle), fixture() as (titles, command):
                driver = bromium.WinDriver(timeout_ms=5000, window_title=titles[0])
                old = driver.get_element_by_xpath("//Button[@Name='Before']")
                command("rename")
                current = driver.get_element_by_xpath("//Button[@Name='After']")
                self.assertEqual(old.name, "Before")
                self.assertEqual(current.runtime_id, old.runtime_id)
                command("remove")
                wait_for(lambda: not driver.get_elements_by_xpath("//Button[@Name='After']"))
                command("replace")
                replacement = driver.get_element_by_xpath("//Button[@Name='Replacement']")
                self.assertEqual(replacement.name, "Replacement")
                with self.assertRaises(bromium.ElementNotFoundError):
                    old.send_click()
                driver.refresh_region(replacement, timeout_ms=5000)
                driver.refresh(window_title=titles[1])
                driver.refresh(None)
                self.assertEqual(driver.window_title, titles[1])
                self.assertFalse(any(e.name == titles[0] for e in driver.snapshot_elements()))
                driver.window_title = None
                self.assertIsNone(driver.window_title)
                self.assertIn(titles[0], [e.name for e in driver.snapshot_elements()])
                del replacement, current, old, driver

    def test_same_driver_requires_serialization_and_query_releases_gil(self):
        import bromium
        with fixture() as (titles, _):
            driver = bromium.WinDriver(timeout_ms=5000, window_title=titles[0])
            driver.get_element_by_xpath("//Edit")
            started = threading.Event()
            outcome = queue.Queue()
            def query():
                started.set()
                try:
                    driver.get_element_by_xpath("//Button[@Name='Never exists']", timeout_ms=600)
                except Exception as error:
                    outcome.put(error)
            worker = threading.Thread(target=query)
            worker.start()
            try:
                self.assertTrue(started.wait(2))
                def borrowed():
                    try:
                        _ = driver.timeout_ms
                    except RuntimeError as error:
                        self.assertIn("borrow", str(error).lower())
                        return True
                    return False
                wait_for(borrowed, 0.5)
            finally:
                worker.join(timeout=3)
            self.assertFalse(worker.is_alive())
            error = outcome.get(timeout=1)
            self.assertIsInstance(error, (bromium.ElementNotFoundError, bromium.StaleTreeError))
            if isinstance(error, bromium.StaleTreeError):
                for field in ("reason", "scope", "revision", "coverage"):
                    self.assertTrue(hasattr(error, field))
            lock = threading.Lock()
            def serialized():
                with lock:
                    return driver.get_element_by_xpath("//Edit").runtime_id
            from concurrent.futures import ThreadPoolExecutor
            with ThreadPoolExecutor(max_workers=2) as pool:
                ids = list(pool.map(lambda _: serialized(), range(4)))
            self.assertTrue(all(identity == ids[0] for identity in ids))


class TeamsWorkflowTests(unittest.TestCase):
    config = None

    def test_search(self):
        if self.config is None:
            self.skipTest("Teams interaction requires separate --teams opt-in")
        import bromium
        # Missing executable/login/search/result UI is a failed setup/query, not a pass.
        run_workflow(bromium, self.config)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--live", action="store_true", help="run controlled fixtures only")
    parser.add_argument("--teams", action="store_true", help="allow real Teams search interaction")
    parser.add_argument("--select-result", action="store_true", help="additional Teams result-selection opt-in")
    defaults = Workflow()
    for field in ("executable", "title", "app_xpath", "search_xpath", "result_xpath", "text", "log_path"):
        parser.add_argument("--" + field.replace("_", "-"), default=getattr(defaults, field))
    parser.add_argument("--timeout-ms", type=int, default=defaults.timeout_ms)
    args = parser.parse_args()
    if args.timeout_ms < 0 or (args.select_result and not args.teams):
        parser.error("timeout must be nonnegative; --select-result requires --teams")
    if args.teams:
        TeamsWorkflowTests.config = Workflow(**{field: getattr(args, field) for field in Workflow.__dataclass_fields__})
    if args.live:
        # Decorator evaluated at import time: enable explicitly for this invocation.
        ControlledWorkflowTests.__unittest_skip__ = False
        os.environ["BROMIUM_LIVE_TESTS"] = "1"
    suite = unittest.defaultTestLoader.loadTestsFromModule(sys.modules[__name__])
    if args.live:
        for module in ("test_launch_contract", "test_action_contract", "test_incremental_live"):
            suite.addTests(unittest.defaultTestLoader.loadTestsFromName(module))
    result = unittest.TextTestRunner(stream=sys.stdout, verbosity=2).run(suite)
    return 0 if result.wasSuccessful() else 1


if __name__ == "__main__":
    sys.exit(main())
