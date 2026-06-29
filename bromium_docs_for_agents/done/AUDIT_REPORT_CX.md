# Bromium Workspace Audit Report

Date: 2026-06-19

Scope: static review plus local validation of the six-crate Rust workspace: `bromium`, `screen-capture`, `uiexplore`, `uitree`, `xmlutil`, and `winevent-monitor`.

## Executive Summary

The project builds with `cargo check --workspace --all-targets`, but it is not currently clean enough for a reliable CI baseline. `cargo fmt --check --all` fails because many files need formatting and `uiexplore/src/app_ui.rs` has trailing whitespace that blocks rustfmt. `cargo test --workspace --all-features` fails on a hard-coded Windows build assertion in `screen-capture`. `cargo clippy --workspace --all-targets --all-features` completes but reports many warnings across crates.

The most important engineering risks are:

- Recoverable Windows/UI Automation failures frequently become panics through `unwrap()` and `expect()`.
- Some COM/UIA wrapper types are marked `Send`/`Sync` manually without documented invariants, then moved across threads.
- UI tree refresh is expensive and is triggered from interactive paths; several loops rebuild and sort whole trees instead of incrementally updating or indexing.
- Library code writes directly to stdout/stderr and opens log files per log record, adding noise and avoidable overhead.
- Tests depend on specific machine state and OS build numbers.

## Validation Results

Commands run:

- `cargo check --workspace --all-targets`: passed.
- `cargo fmt --check --all`: failed. Rustfmt reported large diffs and trailing whitespace errors in `crates/uiexplore/src/app_ui.rs`.
- `cargo clippy --workspace --all-targets --all-features`: passed with warnings. Main classes: `new_without_default`, `needless_bool`, `collapsible_if`, `needless_borrow`, `too_many_arguments`, `manual_ok_err`, `bool_comparison`, `len_zero`, and `unwrap`-adjacent API patterns.
- `cargo test --workspace --all-features`: failed. `screen-capture::mswindows::utils::tests::test_get_build_number` expected Windows build `26100`, but this machine returned `26200`.
- `cargo tree -d`: shows duplicate major/minor dependency families, including `windows` `0.58`, `0.61`, and `0.62`; `xot` `0.29` and `0.31`; `thiserror` `1` and `2`; multiple `icu` generations. Some are transitive and unavoidable, but they increase compile time and binary size.

## Findings

### High: `screen-capture` tests are tied to one Windows build

`crates/screen-capture/src/mswindows/utils.rs:263-266` asserts `get_build_number() == 26100`. On this system the API returns `26200`, so the workspace test suite fails.

Impact: CI and local validation fail on valid Windows installations. The test is checking the environment, not the function contract.

Recommendation: Assert a range or behavioral property instead, such as `build >= 22000` for Windows 11 in the current test environment, or make OS-specific expectations configurable.

### High: Python-facing APIs can panic instead of returning Python errors

Examples:

- `crates/bromium/src/windriver.rs:807-816`: `monitor.capture_image().unwrap()`, `monitor.name().unwrap()`, and `to_str().unwrap()` in `take_screenshot`.
- `crates/bromium/src/windriver.rs:876` and `900`: refresh methods use `rx.recv().unwrap()`.
- `crates/bromium/src/windriver.rs:640-643`: `reload()` unwraps the global driver.
- `crates/xmlutil/src/xpath_eval.rs:82-95`: XML parsing/query setup uses several `unwrap()` calls before returning an `XpathResult`.
- `crates/xmlutil/src/xpath_gen.rs:116`: invalid XML panics in `Document::parse(xml).unwrap()`.

Impact: normal runtime failures such as inaccessible windows, invalid XML, stale UIA elements, missing primary monitors, closed worker channels, or non-UTF-8 paths can abort the extension rather than producing actionable Python errors.

Recommendation: Push `Result` through library layers and convert at the PyO3 boundary with context-rich `PyErr`s. For `xmlutil`, change `eval_xpath` and `get_xpath_full_from_runtime_id` to return `Result<..., Error>` or encode parse failures in the existing result object.

### High: Manual `Send`/`Sync` on UI Automation wrappers lacks safety justification

`crates/uitree/src/uiexplore_iter.rs:193-194` and similar UI tree element wrappers manually declare UIA-backed values as `Send` and `Sync`. `crates/uitree/src/uiexplore_xml.rs:520-528` then spawns one thread per top-level UI element and passes UIA elements into those threads.

Impact: COM apartment affinity and UI Automation object threading rules are subtle. If the underlying `UIElement` is not safely transferable across these threads, this can produce intermittent failures, deadlocks, or undefined behavior at the FFI boundary.

Recommendation: Remove manual `Send`/`Sync` unless the underlying crate explicitly guarantees the invariant. Prefer worker threads that create their own `UIAutomation` instance and reacquire elements by runtime id, or keep traversal on the creating apartment. If manual impls remain, document the COM initialization and lifetime invariants beside the unsafe impls.

### Medium: UI tree refresh is expensive and can block interaction

Examples:

- `crates/bromium/src/windriver.rs:681-710`: `get_element_by_xpath` repeatedly refreshes the full UI tree until timeout.
- `crates/uiexplore/src/app_ui.rs:1014-1044`: auto-refresh requests continuous repaint and starts full tree refreshes from the UI update flow.
- `crates/uitree/src/uiexplore_xml.rs:472-475`: sorting is done twice; because `sort_by` is unstable, the second sort can destroy the ordering established by the first.
- `crates/uitree/src/uiexplore_xml.rs:517-544`: thread count scales with top-level children and waits for all subtrees synchronously.

Impact: large desktops or UI-heavy applications will make the Python API and UI explorer sluggish. Excessive thread creation can hurt more than it helps because UIA calls are often cross-process and COM-bound.

Recommendations:

- Use one `sort_by_key(|e| (z_order, bounding_rect_size))` or a stable sort if exact tie behavior matters.
- Add indexes for common lookups: runtime id, XPath key, bounding rectangle hit testing, and window/title scope.
- Replace unbounded per-child thread spawning with a bounded worker pool only after verifying UIA calls are safe on those threads.
- Add refresh scopes: top-level windows, target process/window handle, or changed subtree based on WinEvents.
- In `get_element_by_xpath`, sleep/back off or refresh only when a relevant event occurs rather than tight-looping full refreshes.

### Medium: GDI capture paths need stronger resource and dimension validation

Examples:

- `crates/screen-capture/src/mswindows/capture.rs:30-45`: `buffer_size = width * height * 4` can overflow or become negative before casting to `usize`.
- `crates/screen-capture/src/mswindows/capture.rs:161-171`: `CreateCompatibleDC`, `CreateCompatibleBitmap`, and `SelectObject` results are not checked before later use.
- `crates/screen-capture/src/mswindows/capture.rs:203-205`: previous GDI object is restored before extracting pixels, but failure paths do not report detailed GDI context.

Impact: invalid window sizes, minimized windows, DPI edge cases, or GDI allocation failures can become incorrect allocation sizes, unclear errors, or invalid handle use.

Recommendation: Validate `width > 0`, `height > 0`, and use checked multiplication before allocating. Check handle return values immediately and include `GetLastError()` in errors. Consider small RAII wrappers for selected objects so restoration happens even if later calls fail.

### Medium: Logging is unnecessarily expensive and globally mutable

`crates/bromium/src/logging.rs:56-84` locks global mutexes for every log record and opens the log file for every write. `init_logger` mutates global state before the `Once` initialization block at `crates/bromium/src/logging.rs:129-174`, so later calls can partially change settings while the installed logger remains global.

Impact: high-volume tracing during UI traversal can create a lot of lock contention and file open/close overhead. Reinitialization behavior is hard to reason about from Python.

Recommendation: Keep an open `Mutex<BufWriter<File>>` or use `tracing`/`tracing-subscriber` with a rolling file appender. Make repeated initialization explicit: either update the installed logger atomically or return a clear error.

### Medium: Library code writes directly to stdout/stderr

Examples:

- `crates/screen-capture/src/mswindows/impl_window.rs:428-446`: capture prints progress messages on every window capture.
- `crates/xmlutil/src/xpath_gen.rs:11`, `30`, `61-66`: XPath generation prints debug messages to stderr.
- `crates/uitree/src/tree_map.rs:109`: tree mutation prints directly on errors.

Impact: Python consumers and GUI users get unexpected console output. It also adds overhead to hot paths.

Recommendation: Replace direct printing with `log` or `tracing`, and keep noisy details at `trace` level.

### Medium: XPath/XML utilities do repeated full-document scans and string parsing

`crates/xmlutil/src/xpath_gen.rs:5-40` scans all descendants for each uniqueness check. `crates/uitree/src/uiexplore.rs:300-367` parses attributes from a generated string using manual `find()` calls.

Impact: XPath generation becomes O(depth * document_size) and allocates heavily. Manual parsing is brittle for escaped quotes and unusual attribute values.

Recommendation: Build an attribute frequency index once per XML document and reuse it while generating XPaths. Prefer structured XML node data over string-form XPath intermediate parsing.

### Medium: Tree map lookup state can become stale or incorrect after removals

`crates/uitree/src/tree_map.rs:114-128` removes runtime-id entries using `self.nodes[index].name` instead of the runtime-id key. Nodes are replaced with placeholders rather than removed, while hash maps and parent/child relationships are manually maintained.

Impact: stale `rtid_to_index` entries can point at removed or placeholder nodes. Future lookups may return incorrect elements.

Recommendation: Store each node's runtime id as a field so removal can update the correct map key. Add tests for remove/lookup behavior, including nested removals and duplicate names.

### Low: Dependency graph is heavier than necessary

The workspace pulls multiple versions of `windows`, `windows-core`, `xot`, `thiserror`, `icu`, and related crates. Some duplication comes from external crates (`eframe`, `display-info`, `xee-xpath`), but direct dependencies are not fully centralized.

Impact: slower clean builds, larger artifacts, and more resolver complexity.

Recommendation: Move shared direct dependencies into `[workspace.dependencies]` consistently (`windows`, `log`, `thiserror`, `xot`, `image` where practical). Review whether `uiexplore` needs a direct `wgpu` dependency when `eframe` already brings it in. Keep `screen-capture`'s image features narrow if only PNG output is required.

### Low: Formatting and lint hygiene are below CI-ready level

`cargo fmt --check --all` fails with large diffs. Clippy reports many straightforward issues that make real findings harder to see.

Recommendation: Run `cargo fmt --all`, fix trailing whitespace in `app_ui.rs`, then adopt a CI gate:

```powershell
cargo fmt --check --all
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
```

Use `-D warnings` only after the current warning backlog is cleared.

## Performance Opportunities

1. Build and maintain lookup indexes in `UITreeXML` instead of scanning vectors for every XPath, coordinate, or runtime-id query.
2. Replace two-pass sorting with tuple-key sorting: `(z_order, bounding_rect_size)`.
3. Limit refresh scope using target window title, process id, native handle, or WinEvent runtime id instead of rebuilding the desktop tree.
4. Cache XML document parsing and XPath static context where possible; avoid reparsing the full XML string for repeated queries.
5. Replace direct `println!`/`eprintln!` in hot paths with disabled-by-default trace logging.
6. Use a bounded background worker for UI refreshes and coalesce multiple refresh requests while one is already running.
7. Avoid cloning large `UITree`/`UITreeXML` values for global state updates unless a snapshot is actually needed.

## Test Coverage Gaps

- No integration tests cover the PyO3 public API failure behavior.
- UI tree mutation and removal behavior is not covered.
- XPath generation needs tests for invalid XML, quotes in attributes, duplicate names, missing runtime ids, and large trees.
- Screen capture tests depend on the live desktop and OS details; add pure unit tests for dimension validation and conversion logic, and mark environment-dependent tests explicitly.
- Auto-refresh and WinEvent handling need tests around coalescing and filtering behavior.

## Suggested Remediation Order

1. Fix the failing `screen-capture` build-number test and apply formatting.
2. Replace PyO3-facing `unwrap()` paths with `PyResult` error conversion in `bromium::windriver`.
3. Audit and remove or document unsafe `Send`/`Sync` implementations around UIA objects.
4. Add scoped/indexed UI tree lookup to reduce full refresh frequency.
5. Clean up logging/printing and adopt a CI lint baseline.
6. Reduce dependency duplication where direct workspace dependencies can be aligned.
