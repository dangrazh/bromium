# Audit Findings 3 CX

Date: 2026-06-25

Scope: Rust workspace at `C:\LocalData\Rust\bromium-ws-new`, including the `bromium`, `screen-capture`, `uiexplore`, `uitree`, `xmlutil`, `bromium-common`, and `winevent-monitor` crates.

## Validation Summary

The following commands were executed successfully:

- `cargo check --workspace --all-targets`
- `cargo test --workspace --all-targets`
- `cargo clippy --workspace --all-targets --all-features -- -D warnings`

Test result summary:

- 71 tests passed.
- 2 screen-capture tests were ignored.

Git status could not be checked because Git rejected the repository under the sandbox user with a dubious-ownership safety error.

## Findings

### F-001: Window Capture Can Return a Successful Blank or Stale Image

Severity: High

Location: `crates/screen-capture/src/mswindows/capture.rs`

`capture_window` attempts several capture strategies, including `PrintWindow` and `BitBlt`. If all strategies fail, the function still proceeds to read the compatible bitmap and returns an image. This can report success while returning a blank, stale, or otherwise invalid capture.

Impact:

- Callers may trust invalid screenshot data.
- Automation flows can make decisions based on incorrect visual state.
- Failures in Windows capture APIs become silent data quality problems instead of actionable errors.

Linked remediation actions:

- R-001
- R-002
- R-003

### F-002: Video Recorder Worker Can Block Permanently on Frame Delivery

Severity: Medium

Location: `crates/screen-capture/src/mswindows/impl_video_recorder.rs`

The video recorder uses `sync_channel(0)` and sends frames from the capture thread with blocking `send`. If the receiver is not actively waiting, the capture thread blocks inside `send`. Calling `stop()` only parks future capture loop iterations and does not unblock a thread already blocked on delivery.

Impact:

- Recorder shutdown can become unreliable.
- Threads can remain blocked until the receiver is dropped.
- Long-running applications can accumulate stuck worker threads or become difficult to stop cleanly.

Linked remediation actions:

- R-004
- R-005
- R-006

### F-003: Monitor Duplication Can Fail Before Reaching the Requested Monitor

Severity: Medium

Location: `crates/screen-capture/src/mswindows/impl_video_recorder.rs`

`ImplVideoRecorder::new` calls `DuplicateOutput` for each enumerated output before checking whether the output belongs to the requested monitor. If duplication fails for a non-target output, recorder creation aborts even when the requested monitor appears later in the enumeration.

Impact:

- Video recording may fail on multi-monitor systems for reasons unrelated to the selected monitor.
- Users can see nondeterministic behavior depending on adapter/output order and per-output duplication support.

Linked remediation actions:

- R-007
- R-008

### F-004: UI Explorer Startup Can Hang or Panic During UI Tree Construction

Severity: Medium

Locations:

- `crates/uiexplore/src/main.rs`
- `crates/uiexplore/src/app_ui.rs`

UI Explorer starts UI tree construction on a background thread but then waits with blocking `recv()` and uses `expect` on both channel receipt and tree construction. If UI Automation traversal stalls, the app can hang indefinitely. If traversal fails, the app panics instead of presenting a recoverable error state.

Impact:

- Desktop app startup can hang with no user-facing recovery path.
- UI Automation errors crash the app.
- The behavior differs from the Python `WinDriver` path, which already uses timeout and cancellation handling.

Linked remediation actions:

- R-009
- R-010
- R-011

### F-005: Log File Configuration Can Silently Succeed When Opening Fails

Severity: Low/Medium

Location: `crates/bromium/src/logging.rs`

`LogFileState::open` returns `Option<Self>`, and callers such as `set_log_file` store `None` while still returning `Ok(())`. Python callers can believe file logging has been configured successfully even when the log file could not be opened.

Impact:

- Diagnostic logging can be unavailable during failures.
- Python API behavior is misleading.
- File permission and path errors are hidden from callers.

Linked remediation actions:

- R-012
- R-013
- R-014

### F-006: Cursor Position Errors Are Ignored

Severity: Low

Location: `crates/bromium/src/windriver.rs`

`WinDriver::get_cursor_pos` ignores the result of `GetCursorPos` and returns `(0, 0)` even if the Windows API call fails.

Impact:

- `(0, 0)` can be mistaken for a valid cursor position.
- Follow-on element lookup can target the wrong screen location.
- Windows API failures lose useful diagnostic context.

Linked remediation actions:

- R-015
- R-016

## Remediation Actions

### R-001: Return an Error When All Window Capture Strategies Fail

Linked findings: F-001

Add an explicit failure check after the final capture attempt in `capture_window`. If `is_success` is still false, return `ScreenCaptureError` with the target `HWND` and `GetLastError` context before calling `to_rgba_image`.

### R-002: Add a Regression Test for Failed Capture Strategy Handling

Linked findings: F-001

Introduce a testable seam around the capture strategy result or extract the post-capture decision logic into a small function. Add a unit test proving that failed capture attempts return an error instead of an image.

### R-003: Add Interactive Validation for Real Window Capture

Linked findings: F-001

Extend the ignored interactive screen-capture tests to verify that a captured window image is non-empty and has expected dimensions. Keep the test ignored by default if it requires a real desktop session.

### R-004: Replace Blocking Zero-Capacity Frame Send

Linked findings: F-002

Replace `sync_channel(0)` with either a bounded buffered channel or a non-blocking frame delivery strategy. For live capture, dropping older frames is usually preferable to blocking the capture thread indefinitely.

### R-005: Add Shutdown Signaling and Join Worker Threads

Linked findings: F-002

Add an explicit shutdown flag or channel to the recorder worker and store its `JoinHandle`. Ensure `stop` or `Drop` can request termination and join the worker without relying on receiver drop side effects.

### R-006: Add Recorder Backpressure Tests

Linked findings: F-002

Add tests around recorder state behavior where the receiver does not read frames. If direct DXGI testing is not practical in CI, extract the delivery loop behavior behind a small testable component.

### R-007: Check Output Monitor Before Calling `DuplicateOutput`

Linked findings: F-003

In `ImplVideoRecorder::new`, enumerate outputs, read `output_desc`, and compare `output_desc.Monitor` with the requested `HMONITOR` before calling `DuplicateOutput`. Only duplicate the matching output.

### R-008: Return a Clear Error When the Requested Monitor Is Not Found

Linked findings: F-003

Replace the open-ended enumeration loop with explicit handling for enumeration completion. Return a domain-specific error when no output matches the requested monitor.

### R-009: Use Timeout and Cancellation for UI Explorer Tree Construction

Linked findings: F-004

Replace blocking `recv()` calls in UI Explorer startup paths with `recv_timeout` and the same cancellation flag pattern used by `WinDriver`.

### R-010: Replace Startup Panics With User-Facing Error State

Linked findings: F-004

Replace `expect` calls in UI Explorer startup and refresh paths with recoverable errors. The app should either show an error view or launch with an empty tree and a status message.

### R-011: Share UI Tree Construction Helpers Across Python and Desktop App Paths

Linked findings: F-004

Extract the timeout/cancel tree construction pattern into a shared helper so `WinDriver` and UI Explorer use consistent behavior and error handling.

### R-012: Make Log File Opening Return `Result`

Linked findings: F-005

Change `LogFileState::open` from `Option<Self>` to `std::io::Result<Self>`. Preserve the source error so callers can report permission, parent directory, or invalid path failures.

### R-013: Propagate Logging Configuration Errors to Python

Linked findings: F-005

Update `set_log_file`, `set_log_directory`, and `enable_file_logging(true)` to return `PyIOError` when the target log file cannot be opened.

### R-014: Add Logging API Tests for Invalid Paths

Linked findings: F-005

Add tests that configure an invalid or inaccessible log file path and assert that the Python-facing API returns an error instead of success.

### R-015: Check the `GetCursorPos` Return Value

Linked findings: F-006

Update `WinDriver::get_cursor_pos` to check the Windows API result and return `AutomationError` when cursor retrieval fails.

### R-016: Preserve Windows Error Context for Cursor Failures

Linked findings: F-006

Include `GetLastError` or equivalent Windows error context in the Python exception message so failures can be diagnosed from logs or caller output.

## Suggested Remediation Order

1. Fix F-001 first because it can silently produce incorrect data while reporting success.
2. Fix F-002 and F-003 together because both affect the DXGI video recorder lifecycle.
3. Fix F-004 to make the desktop app more robust under UI Automation failures.
4. Fix F-005 and F-006 as API correctness improvements with low implementation risk.
