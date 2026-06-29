# Bromium Workspace - Combined Audit Report

**Date:** 2026-06-19  
**Scope:** Full workspace (6 crates: bromium, screen-capture, uitree, uiexplore, winevent-monitor, xmlutil)  
**Version audited:** 0.6.4 (bromium), commit c452e21  
**Sources:** Merged from two independent audits (CL: 2026-06-18, CX: 2026-06-19)

---

## Executive Summary

Bromium is a well-conceived Windows UI Automation toolkit with a Rust core exposed to Python via PyO3, plus a companion desktop UI explorer app. The architecture is sound and the feature set is impressive. However, both audits converge on the same systemic issues:

1. **Pervasive `.unwrap()` calls** turn recoverable Windows/UIA failures into panics — the single most widespread issue
2. **Memory leaks** in the screen-capture crate's Win32 callback pattern (`Box::leak` never reclaimed)
3. **Unsafe `Send`/`Sync` on COM wrappers** without documented invariants, used in multi-threaded tree building
4. **Global mutable state** (`WINDRIVER` static) forces full tree clones and creates TOCTOU races
5. **Significant code duplication** across crates (3× copies of `SaveUIElement`, `commons`, macros)
6. **No CI pipeline**, minimal automated testing, existing tests tied to specific machine state
7. **Library code writes to stdout/stderr** and opens log files per record, adding noise and overhead

The workspace compiles (`cargo check --workspace --all-targets` passes), but is not CI-ready: `cargo fmt` fails, `cargo test` fails on a hard-coded build-number assertion, and `cargo clippy` reports many warnings.

---

## Validation Results

Commands run against the workspace:

| Command | Result |
|---------|--------|
| `cargo check --workspace --all-targets` | Passed |
| `cargo fmt --check --all` | **Failed** — large diffs; trailing whitespace in `uiexplore/src/app_ui.rs` blocks rustfmt |
| `cargo clippy --workspace --all-targets --all-features` | Passed with ~23 warnings (`new_without_default`, `needless_bool`, `collapsible_if`, `len_zero`, `too_many_arguments`, redundant casts, etc.) |
| `cargo test --workspace --all-features` | **Failed** — `screen-capture` test asserts build number `26100`, actual is `26200` |
| `cargo tree -d` | Duplicate dependency families: `windows` 0.58/0.61/0.62, `xot` 0.29/0.31, `thiserror` 1/2, multiple `icu` generations |

---

## 1. Reliability Issues (Crash Risk)

### 1.1 Pervasive `.unwrap()` on Fallible Operations

**Severity: HIGH** | Affects: all crates | Sources: CL, CX

The single most widespread issue. These are `.unwrap()` calls on operations that *can* fail in normal usage:

| Location | Call | Failure scenario |
|----------|------|-----------------|
| `bromium/src/uiauto.rs:57` | `get_ui_automation_instance().unwrap()` | COM init fails (e.g., wrong apartment) |
| `bromium/src/windriver.rs:640-643` | `reload()` unwraps global driver | Global state missing or poisoned |
| `bromium/src/windriver.rs:807-816` | `monitor.capture_image().unwrap()`, `monitor.name().unwrap()`, `to_str().unwrap()` | Missing primary monitor, non-UTF-8 path |
| `bromium/src/windriver.rs:876,900` | `rx.recv().unwrap()` | Sender thread panics during tree refresh |
| `uitree/src/uiexplore.rs:213,215` | UIAutomation init + root element | UIAutomation service unavailable |
| `uitree/src/uiexplore_xml.rs:159,169,190,199` | `get_element_by_runtime_id().unwrap()` | Stale runtime_id after UI changed |
| `xmlutil/src/xpath_gen.rs:116` | `Document::parse(xml).unwrap()` | Malformed XML from UI tree |
| `xmlutil/src/xpath_eval.rs:82-95` | Multiple unwraps in XML parsing/query setup | Invalid XML input |
| `winevent-monitor/src/winevent.rs:112` | `self.hook.uninstall().unwrap()` in **Drop impl** | Panicking in Drop is UB-adjacent |
| `winevent-monitor/src/winevent.rs:204` | `WinEventHook::install().unwrap()` | Hook registration fails |

**Recommendation:** Push `Result` through library layers and convert at the PyO3 boundary with context-rich `PyErr`s. Replace with `?` operator in Result-returning functions, or `.map_err()` + `PyValueError` in PyO3 methods. The Drop-impl unwrap in winevent-monitor is especially dangerous — use `.ok()` or log-and-ignore. For `xmlutil`, change `eval_xpath` and `get_xpath_full_from_runtime_id` to return `Result<..., Error>` or encode parse failures in the existing result object.

### 1.2 No Timeout on Channel `.recv()`

**Severity: MEDIUM** | Affects: bromium, uitree | Sources: CL, CX

Multiple places call `rx.recv().unwrap()` without a timeout. If the sender thread panics or hangs (e.g., UIAutomation deadlock), the main thread blocks forever:

- `bromium/src/windriver.rs:573-585` (UI tree refresh)
- `bromium/src/windriver.rs:876,900` (refresh methods)
- `uitree/src/main.rs:73,96,114`

**Recommendation:** Use `rx.recv_timeout(Duration::from_secs(timeout))` and propagate the error.

### 1.3 Unsafe `Send`/`Sync` on COM Objects

**Severity: HIGH** | Affects: uitree | Sources: CL, CX

```rust
// uitree/src/uiexplore.rs:195-196, uitree/src/uiexplore_iter.rs:193-194
unsafe impl Send for SaveUIElement {}
unsafe impl Sync for SaveUIElement {}
```

`SaveUIElement` wraps `UIElement` which holds COM interface pointers. COM objects created in an STA thread cannot safely be called from other threads. This is used in the parallel tree-building code (`uiexplore_xml.rs:520-545`) which spawns one thread per top-level UI element and passes UIA elements into those threads.

**Recommendation:** Remove manual `Send`/`Sync` unless the underlying crate explicitly guarantees the invariant. Prefer worker threads that create their own `UIAutomation` instance and reacquire elements by runtime id, or keep traversal on the creating apartment. If manual impls remain, document the COM initialization and lifetime invariants beside the unsafe impls. Alternatively, marshal element data (name, classname, runtime_id, bounding_rect) into a plain data struct before crossing thread boundaries.

### 1.4 Failing Tests Tied to Machine State

**Severity: HIGH** | Affects: screen-capture | Source: CX

`crates/screen-capture/src/mswindows/utils.rs:263-266` asserts `get_build_number() == 26100`. On other Windows installations the API returns different values (e.g., `26200`), so the workspace test suite fails.

**Recommendation:** Assert a range or behavioral property instead (e.g., `build >= 22000` for Windows 11), or make OS-specific expectations configurable.

---

## 2. Memory and Resource Issues

### 2.1 Memory Leaks in screen-capture Callbacks

**Severity: HIGH** | Affects: screen-capture | Source: CL

The Win32 callback pattern uses `Box::leak` to pass data through `LPARAM`:

```rust
// screen-capture/src/mswindows/impl_monitor.rs:48
let state = Box::leak(Box::from_raw(state.0 as *mut Vec<HMONITOR>));

// screen-capture/src/mswindows/impl_window.rs:159,171
// Same pattern in enum_valid_windows() and enum_all_windows()
```

Every call to `Monitor::all()`, `Window::all()`, or `Window::z()` leaks a `Vec`. In a long-running automation session calling `refresh()` repeatedly, this accumulates.

**Recommendation:** Reclaim the Box after the callback returns:
```rust
let boxed = Box::new(vec);
let ptr = Box::into_raw(boxed);
// ... call EnumWindows with ptr as LPARAM ...
let result = unsafe { Box::from_raw(ptr) }; // reclaim
```

### 2.2 GDI Capture Paths Need Resource and Dimension Validation

**Severity: MEDIUM** | Affects: screen-capture | Source: CX

- `capture.rs:30-45`: `buffer_size = width * height * 4` can overflow or become negative before casting to `usize`.
- `capture.rs:161-171`: `CreateCompatibleDC`, `CreateCompatibleBitmap`, and `SelectObject` results are not checked before later use.
- `capture.rs:203-205`: previous GDI object is restored before extracting pixels, but failure paths do not report detailed GDI context.

**Recommendation:** Validate `width > 0`, `height > 0`, and use checked multiplication before allocating. Check handle return values immediately and include `GetLastError()` in errors. Consider small RAII wrappers for selected objects so restoration happens even if later calls fail.

### 2.3 Unbounded Background Thread in VideoRecorder

**Severity: MEDIUM** | Affects: screen-capture | Source: CL

`impl_video_recorder.rs:148` spawns a thread that loops forever acquiring frames. The `stop()` method pauses acquisition but never terminates the thread. The thread holds D3D device references.

**Recommendation:** Add a shutdown flag (e.g., `AtomicBool`) checked in the loop, and join the thread on Drop.

### 2.4 Debug `println!` / `eprintln!` in Production Code

**Severity: MEDIUM** | Affects: screen-capture, xmlutil, uitree | Sources: CL, CX

Library code writes directly to stdout/stderr:

- `screen-capture/src/mswindows/impl_window.rs:428-446`: capture prints progress messages on every window capture
- `xmlutil/src/xpath_gen.rs:11, 30, 61-66`: XPath generation prints debug messages to stderr
- `uitree/src/tree_map.rs:109`: tree mutation prints directly on errors

**Recommendation:** Remove or replace with `log::debug!()` / `log::trace!()`. Keep noisy details at `trace` level.

### 2.5 Unbuffered Log File I/O

**Severity: MEDIUM** | Affects: bromium | Sources: CL, CX

Each log message opens the file, writes, and closes:
```rust
// bromium/src/logging.rs:77-83
let mut file = OpenOptions::new().create(true).append(true).open(path)?;
```

Additionally, `logging.rs:56-84` locks global mutexes for every log record. `init_logger` mutates global state before the `Once` initialization block (`logging.rs:129-174`), so later calls can partially change settings while the installed logger remains global. At Trace level during UI tree refresh (thousands of elements), this significantly impacts performance.

Note: `commons.rs` already defines a `FileWriter` with `BufWriter` — but it's unused.

**Recommendation:** Keep an open `Mutex<BufWriter<File>>` or use `tracing`/`tracing-subscriber` with a rolling file appender. Make repeated initialization explicit: either update the installed logger atomically or return a clear error.

---

## 3. Architecture and Design Issues

### 3.1 Global Mutable State (WINDRIVER)

**Severity: HIGH** | Affects: bromium | Sources: CL, CX

```rust
// bromium/src/windriver.rs:34
pub static WINDRIVER: Mutex<Option<WinDriver>> = Mutex::new(None);
```

Every WinDriver operation clones the entire driver from global state, mutates it, then writes the clone back. This:
- Forces `Clone` on WinDriver (which clones the entire UI tree)
- Creates a TOCTOU race between read-modify-write cycles
- Makes it impossible to have multiple independent WinDriver instances

**Recommendation:** Remove the global static. Let each Python `WinDriver` object own its state directly via PyO3's normal instance storage (`#[pyo3(get)]` fields or `self` methods).

### 3.2 Massive Code Duplication

**Severity: HIGH** | Affects: bromium, uitree, uiexplore | Source: CL

The following are duplicated nearly identically across 2-3 crates:

| Code | Locations |
|------|-----------|
| `SaveUIElement` struct + impls | `uitree/save_ui_element.rs`, `uitree/uiexplore.rs:186-208`, `uitree/uiexplore_iter.rs:193-194` |
| `execute_with_timeout()` | `bromium/commons.rs`, `uitree/commons.rs`, `uiexplore/commons.rs` (identical) |
| `printfmt!` macro | `bromium/macros.rs`, `uitree/macros.rs`, `uiexplore/macros.rs` (identical) |
| `sendmsg!` macro | `bromium/macros.rs`, `uitree/macros.rs`, `uiexplore/macros.rs` (identical, unused everywhere) |
| `get_ui_automation_instance()` | `bromium/uiauto.rs`, `uitree/save_ui_element.rs:212-241`, `winevent-monitor/winevent.rs` |
| XPath generation logic | `uitree/uiexplore.rs:70-403`, `uitree/uiexplore_iter.rs:68-458` (near-identical) |

**Recommendation:** Extract shared code into a `bromium-common` crate or consolidate in `uitree`. The `sendmsg!` macro is unused everywhere and should be deleted.

### 3.3 Error Types as Strings

**Severity: MEDIUM** | Affects: xmlutil, screen-capture | Source: CL

`xmlutil/xpath_gen.rs:114` returns error messages as regular strings — callers cannot distinguish success from failure without string matching. Similarly, `screen-capture/error.rs` uses a generic `Error(String)` variant that loses context.

**Recommendation:** Use proper `Result<T, E>` types with structured error enums throughout.

### 3.4 Duplicate XML/DOM Libraries

**Severity: LOW** | Affects: xmlutil | Source: CL

The xmlutil crate depends on 4 XML libraries simultaneously: `roxmltree` (read-only DOM), `quick-xml` (streaming writer), `xot` (full DOM), `xee-xpath` (XPath engine). Additionally, `xml.rs` defines a custom `XMLDomNode` tree that duplicates `xot`'s functionality.

**Recommendation:** Standardize on one DOM library (likely `xot` since it's used for XPath too) and remove the others.

### 3.5 Tree Map Lookup State Can Become Stale After Removals

**Severity: MEDIUM** | Affects: uitree | Source: CX

`uitree/src/tree_map.rs:114-128` removes runtime-id entries using `self.nodes[index].name` instead of the runtime-id key. Nodes are replaced with placeholders rather than removed, while hash maps and parent/child relationships are manually maintained.

**Recommendation:** Store each node's runtime id as a field so removal can update the correct map key. Add tests for remove/lookup behavior, including nested removals and duplicate names.

### 3.6 Dependency Graph Heavier Than Necessary

**Severity: LOW** | Affects: all crates | Source: CX

The workspace pulls multiple versions of `windows` (0.58, 0.61, 0.62), `windows-core`, `xot` (0.29, 0.31), `thiserror` (1, 2), and multiple `icu` generations. Some duplication is transitive and unavoidable, but direct dependencies are not fully centralized.

**Recommendation:** Move shared direct dependencies into `[workspace.dependencies]` consistently. Review whether `uiexplore` needs a direct `wgpu` dependency when `eframe` already brings it in. Keep `screen-capture`'s image features narrow if only PNG output is required.

---

## 4. Performance Issues

### 4.1 UI Tree Refresh Is Expensive and Can Block Interaction

**Severity: MEDIUM** | Affects: bromium, uitree, uiexplore | Sources: CL, CX

- `bromium/src/windriver.rs:681-710`: `get_element_by_xpath` repeatedly refreshes the full UI tree until timeout (tight-loop full refreshes)
- `uiexplore/src/app_ui.rs:1014-1044`: auto-refresh requests continuous repaint and starts full tree refreshes from the UI update flow
- Thread count scales with top-level children (`uiexplore_xml.rs:517-544`) and waits for all subtrees synchronously

**Recommendation:**
- Add refresh scopes: target window title, process id, native handle, or changed subtree based on WinEvents
- In `get_element_by_xpath`, sleep/back off or refresh only when a relevant event occurs rather than tight-looping
- Replace unbounded per-child thread spawning with a bounded worker pool (only after verifying UIA calls are safe on those threads)
- Use a bounded background worker for UI refreshes and coalesce multiple refresh requests while one is already running

### 4.2 Linear Element Lookup by Coordinates

**Severity: MEDIUM** | Affects: bromium | Source: CL

`bromium/src/rectangle.rs:12-25` does a linear scan through all UI elements to find one at given coordinates. For a typical desktop with 5,000+ elements, this is O(n) per lookup.

**Recommendation:** Build a spatial index (R-tree or grid) at tree-refresh time for O(log n) coordinate lookups.

### 4.3 Double Sort Where Single Sort Suffices

**Severity: LOW** | Affects: uitree | Sources: CL, CX

```rust
// uitree/src/uiexplore_xml.rs:401-403 (also 472-475)
elements.sort_by(|a, b| a.bounding_rect_size.cmp(&b.bounding_rect_size));
elements.sort_by(|a, b| a.z_order.cmp(&b.z_order));
```

The second sort overrides the first. Because `sort_by` is unstable, the second sort can destroy the ordering established by the first.

**Recommendation:** Use a single sort with tuple comparison:
```rust
elements.sort_by_key(|e| (e.z_order, e.bounding_rect_size));
```

### 4.4 XPath/XML Utilities Do Repeated Full-Document Scans

**Severity: MEDIUM** | Affects: xmlutil, uitree | Source: CX

`xmlutil/src/xpath_gen.rs:5-40` scans all descendants for each uniqueness check. `uitree/src/uiexplore.rs:300-367` parses attributes from a generated string using manual `find()` calls.

**Recommendation:** Build an attribute frequency index once per XML document and reuse it while generating XPaths. Prefer structured XML node data over string-form XPath intermediate parsing.

### 4.5 Full Tree Clone on Every State Update

**Severity: MEDIUM** | Affects: bromium | Sources: CL, CX

Because of the global `WINDRIVER` mutex pattern, the entire UI tree is cloned whenever timeout is changed, window title is set, or tree is refreshed. Solved by fixing issue 3.1.

### 4.6 Repeated DPI Lookups Without Caching

**Severity: LOW** | Affects: screen-capture | Source: CL

`screen-capture/src/mswindows/impl_monitor.rs:89-149` calls `get_process_is_dpi_awareness()` for every monitor property access, which loads `Shcore.dll` and calls `GetProcAddress` each time.

**Recommendation:** Cache the DPI awareness result in a `OnceLock<bool>` since it doesn't change during process lifetime.

### 4.7 String Allocations in XPath Evaluation

**Severity: LOW** | Affects: xmlutil | Source: CL

`eval_xpath()` takes owned `String` parameters instead of `&str`. All result getters return clones.

**Recommendation:** Accept `&str` parameters, return `&[XpathQueryResult]` references.

---

## 5. Testing Gaps

### 5.1 No CI/CD Pipeline

**Severity: HIGH** | Sources: CL, CX

No `.github/workflows`, no CI configuration of any kind. Changes can be pushed without any automated verification. Formatting and lint hygiene are below CI-ready level.

**Recommendation:** Set up a basic CI pipeline:
```powershell
cargo fmt --check --all
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
```
Use `-D warnings` only after the current warning backlog is cleared.

### 5.2 Minimal Rust Test Coverage

**Severity: HIGH** | Sources: CL, CX

Current test counts:

| Crate | Tests | Notes |
|-------|-------|-------|
| xmlutil | 4 | XPath evaluation (reasonable) |
| screen-capture | ~10 | Capture and utilities (one test fails on build number) |
| bromium | 3 | Timeout helper + 2 UIAutomation smoke tests |
| uitree | 1 | Timeout helper (duplicated from bromium) |
| uiexplore | 1 | Timeout helper (duplicated from bromium) |
| winevent-monitor | 0 | No tests at all |

**Missing test coverage:**
- No integration tests covering PyO3 public API failure behavior
- No tests for Element click/send_keys/send_text logic
- No tests for XPath generation (including invalid XML, quotes in attributes, duplicate names, missing runtime ids, large trees)
- No tests for UI tree building, refresh, mutation, or removal behavior
- No tests for app launch/activate logic
- No tests for logging configuration
- Auto-refresh and WinEvent handling need tests around coalescing and filtering behavior
- Screen capture tests depend on the live desktop; add pure unit tests for dimension validation and conversion logic

### 5.3 Python Tests Are Manual Scripts

**Severity: LOW** | Source: CL

The two test files (`tests/app_start_danipc.py`, `tests/log_test.py`) are manual scripts, not pytest test suites. They require specific applications (MS Teams) to be installed and running.

**Recommendation:** Add pytest-compatible tests with mocking for the Python API surface. Mark environment-dependent tests explicitly.

---

## 6. Minor Issues and Code Hygiene

### 6.1 Typo in Filename

`bromium/src/sreen_context.rs` — missing 'c' in "screen". (Source: CL)

### 6.2 Commented-Out Code Bloat

Large blocks of commented-out code exist throughout (Source: CL):
- `sreen_context.rs`: ~170 lines of commented DPI code
- `uiexplore/border.rs`: Entire file is commented out (lines 2-83)
- `uiexplore/app_ui.rs:632-669`: Disabled highlighting code
- `uitree/uiexplore_iter.rs:249-292`: Original recursive implementation
- `bromium/lib.rs:20-27`: Commented imports

### 6.3 Clippy Warnings

~23 clippy warnings including: redundant type casts, collapsible if/else blocks, `.len() > 0` instead of `!is_empty()`, `new_without_default`, `needless_bool`, `needless_borrow`, `too_many_arguments`, `manual_ok_err`, `bool_comparison`. (Sources: CL, CX)

### 6.4 Formatting Issues

`cargo fmt --check --all` fails with large diffs. `uiexplore/src/app_ui.rs` has trailing whitespace that blocks rustfmt entirely. (Source: CX)

### 6.5 README and Stub Inconsistencies

- README shows `WinDriver(timeout_ms=5_000)` but actual constructor requires `window_title` parameter too
- README references `get_ui_element_by_xpath` and `refresh_ui_tree` but current API uses `get_element_by_xpath` and `refresh`
- Build instructions say `git clone https://github.com/yourusername/bromium.git` (placeholder URL)
- `.pyi` stub documents `get_cursor_pos()` but README says `get_curser_pos()`
- `launch_or_activate_app` return type differs between `.pyi` (returns `Element`) and README (returns `bool`)

(Sources: CL)

### 6.6 Unused Dependencies and Code

- `xmlutil` is listed as a dependency of `bromium` in Cargo.toml but never imported in bromium's source code
- `sendmsg!` macro is defined identically in 3 crates but unused everywhere

(Source: CL)

---

## Prioritized Remediation Plan

### Phase 1 — Immediate (safety, correctness, CI-unblocking)

| # | Action | Severity | Effort |
|---|--------|----------|--------|
| 1 | Fix failing `screen-capture` build-number test | HIGH | Low |
| 2 | Run `cargo fmt --all`, fix trailing whitespace in `app_ui.rs` | HIGH | Low |
| 3 | Replace `.unwrap()` calls with proper error handling in PyO3-facing code — especially the Drop impl in winevent-monitor | HIGH | Medium |
| 4 | Fix `Box::leak` memory leaks in screen-capture callbacks | HIGH | Low |
| 5 | Remove `println!`/`eprintln!` debug statements from library code | MEDIUM | Low |
| 6 | Add `recv_timeout` to all channel receives | MEDIUM | Low |

### Phase 2 — Short-term (architecture)

| # | Action | Severity | Effort |
|---|--------|----------|--------|
| 7 | Remove global `WINDRIVER` static — use PyO3 instance storage | HIGH | High |
| 8 | Audit and remove or document unsafe `Send`/`Sync` implementations around UIA objects | HIGH | Medium |
| 9 | Extract duplicated code into a shared crate (`bromium-common`) | HIGH | Medium |
| 10 | Set up a basic CI pipeline (fmt + clippy + test) | HIGH | Low |
| 11 | Validate GDI capture dimensions and handle return values | MEDIUM | Medium |
| 12 | Fix tree map removal to use correct runtime-id key | MEDIUM | Medium |

### Phase 3 — Medium-term (quality and robustness)

| # | Action | Severity | Effort |
|---|--------|----------|--------|
| 13 | Add unit tests for XPath generation, element lookup, tree building, and tree mutation | HIGH | High |
| 14 | Add pytest test suite for the Python API | MEDIUM | High |
| 15 | Replace string error types with structured error enums | MEDIUM | Medium |
| 16 | Consolidate XML libraries in xmlutil | LOW | Medium |
| 17 | Fix clippy warnings and clean up commented-out code | LOW | Low |
| 18 | Rename `sreen_context.rs` to `screen_context.rs` | LOW | Low |
| 19 | Update README and `.pyi` stubs to match current API | LOW | Low |
| 20 | Delete unused `sendmsg!` macro and unused `xmlutil` dependency | LOW | Low |
| 21 | Centralize workspace dependencies to reduce duplication | LOW | Medium |

### Phase 4 — Long-term (performance)

| # | Action | Severity | Effort |
|---|--------|----------|--------|
| 22 | Add scoped/indexed UI tree lookup (spatial index, runtime-id index, XPath key index) | MEDIUM | High |
| 23 | Buffer log file writes (use `BufWriter` or `tracing-subscriber`) | MEDIUM | Medium |
| 24 | Add refresh scopes and coalesce multiple refresh requests | MEDIUM | High |
| 25 | Cache DPI awareness checks in `OnceLock` | LOW | Low |
| 26 | Accept `&str` instead of `String` in XPath evaluation API | LOW | Low |
| 27 | Add bounded worker pool for parallel tree building | LOW | High |
| 28 | Build attribute frequency index for XPath generation | LOW | Medium |
