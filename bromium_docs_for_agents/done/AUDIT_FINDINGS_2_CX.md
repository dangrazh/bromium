# Audit Findings Report

Date: 2026-06-22

Scope: Rust workspace at `C:\LocalData\Rust\bromium-ws-new`, including the PyO3 `bromium` package, `uiexplore`, `uitree`, `xmlutil`, `screen-capture`, `winevent-monitor`, and shared crates.

## Validation Summary

Commands run:

- `cargo check --workspace --all-targets --all-features`: passed
- `cargo clippy --workspace --all-targets --all-features`: passed
- `cargo fmt --check --all`: failed
- `cargo test --workspace --all-features`: failed

CI mirrors the failing commands in `.github/workflows/ci.yml`, so the current workspace state is not CI-clean.

## Findings

### F-001: CI is currently failing

Severity: High

Evidence:

- `.github/workflows/ci.yml` runs `cargo fmt --check --all`.
- Formatting fails in:
  - `crates/bromium/src/screen_context.rs`
  - `crates/bromium/src/windriver.rs`
  - `crates/uiexplore/src/app_ui.rs`
- `.github/workflows/ci.yml` runs `cargo test --workspace --all-features`.
- `cargo test --workspace --all-features` fails in `crates/screen-capture/src/mswindows/capture.rs` at `test_capture_monitor`, where `capture_monitor(x, y, 100, 100)` returns an error.

Impact:

The main branch/PR validation is not reliable. Any further changes will be hard to evaluate because CI starts from a failing baseline.

Remediation actions:

- R-001: Run `cargo fmt --all` and commit the formatting-only changes.
- R-002: Diagnose `test_capture_monitor` by capturing/logging the concrete error returned from `capture_monitor`.
- R-003: Make `test_capture_monitor` deterministic for CI. If monitor capture depends on interactive desktop/session capabilities unavailable in CI, gate it behind an explicit ignored/integration test or skip with a clear environment check.
- R-004: Re-run `cargo fmt --check --all` and `cargo test --workspace --all-features` after the fix.

### F-002: Generated XPath is invalid for UI names containing quotes

Severity: High

Evidence:

- `crates/xmlutil/src/xpath_gen.rs` interpolates XML attribute values directly into XPath string literals, for example `//*[@{}='{}']` and `{}[@Name='{}']`.
- `crates/uitree/src/uiexplore_xml.rs` writes Windows UI Automation names directly into the `Name` XML attribute.

Impact:

Windows UI element names can contain apostrophes or quotes. A name such as `Bob's App` will generate an invalid XPath expression, which breaks locator generation and lookup for legitimate UI elements.

Remediation actions:

- R-005: Add an XPath string literal encoder that emits single-quoted strings, double-quoted strings, or `concat(...)` for values containing both quote types.
- R-006: Use the encoder for every generated XPath attribute predicate in `xmlutil::xpath_gen`.
- R-007: Add unit tests for names containing apostrophes, double quotes, and both quote types.
- R-008: Add a regression test proving that generated XPath can be evaluated against XML containing quoted `Name` values.

### F-003: UI tree refresh retry path can accumulate stuck background threads

Severity: High

Evidence:

- `crates/bromium/src/windriver.rs` retries `get_element_by_xpath` by spawning a new UI tree build thread on each retry.
- The spawned tree build waits through `recv_timeout(Duration::from_secs(120))`.
- `crates/bromium-common/src/timeout.rs` documents that timed-out work keeps running in the background and is not cancelled.

Impact:

If Windows UI Automation blocks or stalls, repeated lookup retries can leave multiple UIA worker threads running. This can leak process resources, add COM/UIA contention, slow future lookups, and make a long-running Python automation session unstable.

Remediation actions:

- R-009: Replace repeated unbounded `thread::spawn` calls in the lookup retry loop with a bounded refresh strategy.
- R-010: Ensure only one outstanding UI tree refresh can exist per `WinDriver` instance.
- R-011: Add cooperative cancellation or a stale-result discard mechanism so timed-out refreshes cannot update or interfere with newer state.
- R-012: Add tests around retry behavior using a fake/delayed tree builder where possible, or isolate the refresh policy into testable non-UIA code.

### F-004: Logger initialization has incorrect repeated-call behavior and can panic in host applications

Severity: Medium

Evidence:

- `crates/bromium/src/logging.rs` calls `log::set_logger(&LOGGER).expect("Failed to initialize logger")`.
- `set_log_level_internal(log_level)` is inside `INIT.call_once`, so repeated calls to `init_logging` update file/console state but do not apply the requested log level after the first initialization.
- Python tests call `Bromium.init_logging(...)` repeatedly with different log levels.

Impact:

As a Python extension, `bromium` may be imported into a process where another package has already initialized Rust logging. In that case, this code can panic instead of returning a Python error or degrading gracefully. Separately, repeated `init_logging` calls do not behave as the public API and tests imply.

Remediation actions:

- R-013: Move log-level updates outside `INIT.call_once` so every `init_logging` call applies the requested level.
- R-014: Replace `expect` on `log::set_logger` with non-panicking handling. If another logger is already installed, update Bromium state where possible and return a clear Python error or no-op according to the desired API contract.
- R-015: Add Rust unit tests for repeated `init_logger` calls.
- R-016: Add or enable Python tests that verify repeated `init_logging` changes the effective level.

### F-005: `uiexplore` temp-file signaling is globally named and cross-instance unsafe

Severity: Medium

Evidence:

- `crates/uiexplore/src/signal_file.rs` writes `%TEMP%\signal_file.txt`.
- The same fixed path is used to detect termination and remove the file.

Impact:

Multiple `uiexplore` instances can interfere with each other. Any local process can create the same file and trigger termination behavior. This is brittle for normal multi-instance use and weak as an inter-process control mechanism.

Remediation actions:

- R-017: Replace the fixed `signal_file.txt` with a per-process or per-session path containing the process ID and a unique token.
- R-018: Pass the signal path/token explicitly to the cooperating process instead of relying on a global temp filename.
- R-019: Consider replacing temp-file signaling with a named event, pipe, or channel if this is intended as durable IPC.
- R-020: Add tests for multiple independent signal files to prevent cross-instance collisions.

### F-006: Screenshots overwrite previous captures

Severity: Medium

Evidence:

- `crates/bromium/src/windriver.rs` saves screenshots as `temp\bromium_screenshots\monitor-{monitor_name}.png`.
- The filename does not include a timestamp, sequence number, UUID, or caller-provided destination.

Impact:

Repeated `take_screenshot()` calls on the same monitor overwrite prior captures. This can destroy useful automation evidence and makes the returned path ambiguous in workflows that capture more than once.

Remediation actions:

- R-021: Generate unique screenshot filenames, for example with timestamp plus monotonic counter or UUID.
- R-022: Optionally add a Python API parameter for an explicit output directory or filename, while preserving the current default behavior through unique generated names.
- R-023: Add a test around filename generation that proves two consecutive calls do not produce the same path. If direct capture is hard to test, isolate filename generation into a pure helper.

### F-007: Python package behavior is not validated in CI

Severity: Medium

Evidence:

- `.github/workflows/ci.yml` only runs Rust formatting, clippy, and cargo tests.
- `crates/bromium/tests` contains Python tests, but CI does not build the extension with `maturin` or run `pytest`.

Impact:

PyO3 binding regressions, packaging errors, stub/API drift, and Python-level behavior changes can pass CI. This is a significant gap because `bromium` is presented as a Python library.

Remediation actions:

- R-024: Add a CI job that installs Python 3.12+, installs `maturin`, builds or develops the extension, and runs `pytest`.
- R-025: Split Python tests into fast CI-safe tests and environment-dependent desktop automation tests.
- R-026: Mark desktop/UIA tests requiring an interactive Windows session with explicit pytest markers.
- R-027: Add CI documentation for which Python tests are expected to run in headless GitHub Actions and which require manual/interactive validation.

### F-008: Repository hygiene issues create avoidable noise and ambiguity

Severity: Low

Evidence:

- `crates/bromium/.pyo3venv` exists under the package directory.
- `crates/screen-capture/Cargo.lock` exists inside a workspace member even though the workspace root has `Cargo.lock`.

Impact:

Local virtual environments inside crate trees can pollute searches, packaging, backups, and review diffs. A nested lockfile in a workspace member can confuse dependency review unless it is intentionally maintained for standalone publishing.

Remediation actions:

- R-028: Confirm whether `crates/bromium/.pyo3venv` is intentionally ignored and never packaged. If not needed, remove it from the repo tree and ensure `.gitignore` covers local virtual environments.
- R-029: Decide whether `crates/screen-capture` is intended to build standalone outside the workspace. If not, remove its nested `Cargo.lock`; if yes, document why it exists.
- R-030: Add repo hygiene checks or documentation for generated/local artifacts.

## Remediation Sequence

1. R-001: Run `cargo fmt --all` and commit the formatting-only changes. Links to F-001.
2. R-002: Diagnose `test_capture_monitor` by surfacing the concrete capture error. Links to F-001.
3. R-003: Make `test_capture_monitor` deterministic for CI or explicitly gate it as environment-dependent. Links to F-001.
4. R-004: Re-run `cargo fmt --check --all` and `cargo test --workspace --all-features`. Links to F-001.
5. R-005: Add an XPath string literal encoder. Links to F-002.
6. R-006: Use the encoder in all generated XPath predicates. Links to F-002.
7. R-007: Add quote-handling unit tests for XPath generation. Links to F-002.
8. R-008: Add an XPath evaluation regression test for quoted names. Links to F-002.
9. R-013: Move repeated log-level updates outside `INIT.call_once`. Links to F-004.
10. R-014: Replace logger initialization panic behavior with non-panicking handling. Links to F-004.
11. R-015: Add Rust tests for repeated logger initialization. Links to F-004.
12. R-016: Add or enable Python tests for repeated logging configuration. Links to F-004.
13. R-009: Replace repeated unbounded refresh thread spawning with a bounded strategy. Links to F-003.
14. R-010: Enforce one outstanding UI tree refresh per `WinDriver`. Links to F-003.
15. R-011: Add cancellation or stale-result handling for timed-out refreshes. Links to F-003.
16. R-012: Add tests around retry/refresh policy. Links to F-003.
17. R-021: Generate unique screenshot filenames. Links to F-006.
18. R-023: Add filename generation regression tests. Links to F-006.
19. R-022: Optionally add explicit screenshot output path API support. Links to F-006.
20. R-017: Replace global temp signal filename with per-process/per-session naming. Links to F-005.
21. R-018: Pass signal identity explicitly to cooperating processes. Links to F-005.
22. R-020: Add multi-instance signal-file tests. Links to F-005.
23. R-019: Consider replacing file signaling with named event/pipe/channel. Links to F-005.
24. R-024: Add Python package CI with `maturin` and `pytest`. Links to F-007.
25. R-025: Split Python tests into CI-safe and desktop/UIA-dependent groups. Links to F-007.
26. R-026: Add pytest markers for interactive desktop automation tests. Links to F-007.
27. R-027: Document Python test expectations for CI and manual validation. Links to F-007.
28. R-028: Clean up or document `.pyo3venv` handling. Links to F-008.
29. R-029: Decide and document/remove nested `screen-capture/Cargo.lock`. Links to F-008.
30. R-030: Add repo hygiene checks or documentation for local/generated artifacts. Links to F-008.

## Recommended First Milestone

The first milestone should stop at R-016. That produces a CI-clean baseline, fixes the most likely user-facing locator bug, and hardens the Python extension logger behavior before larger threading changes are attempted.
