# Audit Findings

Date: 2026-06-21

Scope: Rust workspace at `C:\LocalData\Rust\bromium-ws-new`, including the `bromium`, `bromium-common`, `screen-capture`, `uiexplore`, `uitree`, `xmlutil`, and `winevent-monitor` crates.

## Summary

The workspace is in generally buildable shape: formatting, clippy, and `cargo check` pass. The main risks found are concentrated in Windows screen/window capture and Python-facing error handling. One workspace test currently fails because it depends on desktop capture behavior that is not stable across environments.

## Validation

| Command | Result |
| --- | --- |
| `cargo check --workspace --all-targets` | Passed |
| `cargo fmt --check --all` | Passed |
| `cargo clippy --workspace --all-targets --all-features` | Passed |
| `cargo test --workspace --all-features` | Failed |
| `cargo test -p bromium --all-features` | Passed |
| `cargo test -p bromium-common --all-features` | Passed |
| `cargo test -p xmlutil -p uitree -p uiexplore -p winevent-monitor --all-features` | Passed |

Note: `git status` could not be checked because Git rejected the repository as a dubious ownership path for the sandbox user.

## Findings

### F-001: Unsafe Out-of-Bounds Read in Window Metadata

Severity: High

Files:
- `crates/screen-capture/src/mswindows/impl_window.rs`

Affected code:
- `LangCodePage`
- `get_app_name`
- `slice::from_raw_parts(lang_code_pages_ptr.cast(), lang_code_pages_length as usize)`

`VerQueryValueW("\\VarFileInfo\\Translation")` returns a byte length, but the implementation passes that byte length directly as the element count to `slice::from_raw_parts`. For a single 4-byte language/codepage record, the code builds a 4-element `LangCodePage` slice and can read past the returned version-resource buffer.

`LangCodePage` also lacks `#[repr(C)]`, so Rust is not required to lay it out like the Windows `LANGANDCODEPAGE` structure.

Impact:
- Potential undefined behavior.
- Possible process crash while enumerating or inspecting window application names.
- Incorrect metadata extraction from executable version resources.

### F-002: Integer Overflow in Monitor Region Validation

Severity: Medium

Files:
- `crates/screen-capture/src/mswindows/impl_monitor.rs`

Affected code:
- `ImplMonitor::capture_region`
- `x + width > monitor_width`
- `y + height > monitor_height`

The bounds check adds `u32` values directly. In debug builds, oversized inputs can panic. In release builds, overflow wraps and can allow invalid capture regions to pass validation.

Impact:
- Panic in debug/test builds.
- Invalid capture coordinates in release builds.
- Potential confusing capture errors from deeper Win32 calls.

### F-003: Python-Facing `ScreenContext::new()` Can Panic

Severity: Medium

Files:
- `crates/bromium/src/screen_context.rs`

Affected code:
- `DisplayInfo::all().unwrap_or_default()`
- `screens.first().cloned().expect("No screens found")`

`DisplayInfo::all()` errors are converted to an empty list, then the constructor panics if no screen is found. Because this is exposed through the Python extension, headless sessions, service contexts, remote desktop edge cases, or permission-limited sessions can produce a Rust panic instead of a Python exception.

Impact:
- Python caller receives a panic/abort path instead of a recoverable error.
- Automation can fail abruptly in CI, service, or remote environments.

### F-004: Workspace Test Suite Fails on Environment-Coupled Monitor Capture Test

Severity: Medium

Files:
- `crates/screen-capture/src/mswindows/capture.rs`

Affected test:
- `mswindows::capture::tests::test_capture_monitor`

`cargo test --workspace --all-features` fails because `test_capture_monitor` calls `capture_monitor(0, 0, 100, 100)` and asserts success. The test assumes `(0, 0)` is capturable in the current Windows desktop session. That is not guaranteed across multi-monitor layouts, remote sessions, sandboxed sessions, non-interactive environments, or desktops where capture APIs are restricted.

Impact:
- CI and local validation can fail even when unrelated crates are healthy.
- A single environment-sensitive test blocks workspace-level test completion.

## Remediation Actions

### R-001: Correct Version Resource Translation Parsing

Linked findings: F-001

Actions:
- Add `#[repr(C)]` to `LangCodePage`.
- Treat `lang_code_pages_length` as bytes, not element count.
- Compute the element count as `lang_code_pages_length as usize / std::mem::size_of::<LangCodePage>()`.
- Reject or fall back when `lang_code_pages_length` is not a multiple of `size_of::<LangCodePage>()`.
- Add a small helper for parsing the translation block so the unsafe slice creation is isolated and reviewed.

Suggested validation:
- `cargo test -p screen-capture --all-features`
- Add a focused unit test for the translation-length conversion using a test helper where possible.

### R-002: Use Checked Arithmetic for Capture Region Bounds

Linked findings: F-002

Actions:
- Replace `x + width` and `y + height` with `checked_add`.
- Return `ScreenCaptureError::InvalidCaptureRegion` if either checked addition overflows.
- Keep the existing bound comparisons after successful checked addition.
- Consider rejecting zero-width or zero-height regions explicitly if those are not meaningful captures.

Suggested validation:
- Add tests for overflow inputs such as `x = u32::MAX`, `width = 1`.
- Add tests for boundary-valid regions where `x + width == monitor_width`.
- Run `cargo test -p screen-capture --all-features`.

### R-003: Make Screen Context Construction Fallible

Linked findings: F-003

Actions:
- Change the Python-exposed constructor path to return `PyResult<Self>` instead of panicking.
- Preserve the original `DisplayInfo::all()` error and map it to a Python exception, preferably `AutomationError` or `PyRuntimeError`.
- Return a Python error when display enumeration succeeds but returns an empty list.
- Avoid `unwrap_or_default()` for display enumeration in user-facing APIs.

Suggested validation:
- Add a unit-testable helper that accepts `Vec<DisplayInfo>`-like data or an internal screen DTO and returns `Result<ScreenContext, _>`.
- Run `cargo test -p bromium --all-features`.

### R-004: Make Monitor Capture Tests Environment-Aware

Linked findings: F-004

Actions:
- Replace hard-coded `(0, 0)` capture coordinates with coordinates from `Monitor::all()` or `ImplMonitor::all()`.
- Capture a small region inside an enumerated monitor, for example the monitor's own `x`, `y`, and a clamped `min(width, 100)`, `min(height, 100)` region.
- If no monitor is available or the desktop session cannot be captured, skip the test with a clear message instead of asserting success.
- Consider separating pure unit tests from interactive desktop integration tests.

Suggested validation:
- `cargo test -p screen-capture --all-features`
- `cargo test --workspace --all-features`

## Priority Order

1. R-001, because F-001 is an unsafe memory-read issue.
2. R-002, because F-002 can cause panics and invalid Win32 capture calls from public inputs.
3. R-003, because F-003 affects Python API reliability in common automation environments.
4. R-004, because F-004 blocks reliable workspace validation.

