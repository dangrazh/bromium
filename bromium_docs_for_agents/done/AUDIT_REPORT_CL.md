# Bromium Workspace - Code Audit Report

**Date:** 2026-06-18  
**Scope:** Full workspace (6 crates: bromium, screen-capture, uitree, uiexplore, winevent-monitor, xmlutil)  
**Version audited:** 0.6.4 (bromium), commit c452e21

---

## Executive Summary

Bromium is a well-conceived Windows UI Automation toolkit with a Rust core exposed to Python via PyO3, plus a companion desktop UI explorer app. The architecture is sound and the feature set is impressive. However, the codebase has several systemic issues that affect reliability, performance, and maintainability:

1. **Excessive `.unwrap()` calls** throughout the codebase create crash risk in production
2. **Memory leaks** in the screen-capture crate's Win32 callback pattern
3. **Significant code duplication** across crates (3x copies of SaveUIElement, commons, macros)
4. **Global mutable state** in the bromium crate forces clone-heavy patterns
5. **No CI pipeline**, minimal automated testing, no Rust test coverage for core logic

The sections below detail findings by category, ordered by severity.

---

## 1. Reliability Issues (Crash Risk)

### 1.1 Pervasive `.unwrap()` on Fallible Operations

**Severity: HIGH** | Affects: all crates

The single most widespread issue. These are `.unwrap()` calls on operations that *can* fail in normal usage, not just in theory:

| Location | Call | Failure scenario |
|----------|------|-----------------|
| `bromium/src/uiauto.rs:57` | `get_ui_automation_instance().unwrap()` | COM init fails (e.g., wrong apartment) |
| `bromium/src/windriver.rs:900` | `rx.recv().unwrap()` | Sender thread panics during tree refresh |
| `uitree/src/uiexplore.rs:213,215` | UIAutomation init + root element | UIAutomation service unavailable |
| `uitree/src/uiexplore_xml.rs:159,169,190,199` | `get_element_by_runtime_id().unwrap()` | Stale runtime_id after UI changed |
| `xmlutil/src/xpath_gen.rs:116` | `Document::parse(xml).unwrap()` | Malformed XML from UI tree |
| `xmlutil/src/xpath_eval.rs:87` | `documents.add_string_without_uri().unwrap()` | Invalid XML input |
| `winevent-monitor/src/winevent.rs:112` | `self.hook.uninstall().unwrap()` in **Drop impl** | Panicking in Drop is UB-adjacent |
| `winevent-monitor/src/winevent.rs:204` | `WinEventHook::install().unwrap()` | Hook registration fails |

**Recommendation:** Replace with `?` operator (in Result-returning functions) or `.map_err()` + `PyValueError` (in PyO3 methods). The Drop-impl unwrap in winevent-monitor is especially dangerous — use `.ok()` or log-and-ignore.

### 1.2 No Timeout on Channel `.recv()`

**Severity: MEDIUM** | Affects: bromium, uitree

Multiple places call `rx.recv().unwrap()` without a timeout. If the sender thread panics or hangs (e.g., UIAutomation deadlock), the main thread blocks forever:

- `bromium/src/windriver.rs:573-585` (UI tree refresh)
- `uitree/src/main.rs:73,96,114`

**Recommendation:** Use `rx.recv_timeout(Duration::from_secs(timeout))` and propagate the error.

### 1.3 Unsafe `Send`/`Sync` on COM Objects

**Severity: MEDIUM** | Affects: uitree

```rust
// uitree/src/uiexplore.rs:195-196
unsafe impl Send for SaveUIElement {}
unsafe impl Sync for SaveUIElement {}
```

`SaveUIElement` wraps `UIElement` which holds COM interface pointers. COM objects created in an STA thread cannot safely be called from other threads. This is used in the parallel tree-building code (`uiexplore_xml.rs:520-545`) and could cause intermittent crashes or hangs.

**Recommendation:** Instead of marking the wrapper as Send/Sync, marshal element data (name, classname, runtime_id, bounding_rect) into a plain data struct before crossing thread boundaries.

---

## 2. Memory and Resource Issues

### 2.1 Memory Leaks in screen-capture Callbacks

**Severity: HIGH** | Affects: screen-capture

The Win32 callback pattern uses `Box::leak` to pass data through `LPARAM`:

```rust
// screen-capture/src/mswindows/impl_monitor.rs:48
let state = Box::leak(Box::from_raw(state.0 as *mut Vec<HMONITOR>));

// screen-capture/src/mswindows/impl_window.rs:159,171
// Same pattern in enum_valid_windows() and enum_all_windows()
```

Every call to `Monitor::all()`, `Window::all()`, or `Window::z()` leaks a `Vec`. In a long-running automation session calling `refresh()` repeatedly, this accumulates.

**Recommendation:** Reclaim the Box after the callback returns, e.g.:
```rust
let boxed = Box::new(vec);
let ptr = Box::into_raw(boxed);
// ... call EnumWindows with ptr as LPARAM ...
let result = unsafe { Box::from_raw(ptr) }; // reclaim
```

### 2.2 Unbounded Background Thread in VideoRecorder

**Severity: MEDIUM** | Affects: screen-capture

`impl_video_recorder.rs:148` spawns a thread that loops forever acquiring frames. The `stop()` method pauses acquisition but never terminates the thread. The thread holds D3D device references.

**Recommendation:** Add a shutdown flag (e.g., `AtomicBool`) checked in the loop, and join the thread on Drop.

### 2.3 Debug `println!` in Production Code

**Severity: MEDIUM** | Affects: screen-capture

`screen-capture/src/mswindows/impl_window.rs:428-446` contains multiple `println!()` calls in `capture_image()`. These will be visible to all Python users.

**Recommendation:** Remove or replace with `log::debug!()`.

### 2.4 Unbuffered Log File I/O

**Severity: MEDIUM** | Affects: bromium

Each log message opens the file, writes, and closes:
```rust
// bromium/src/logging.rs:77-83
let mut file = OpenOptions::new().create(true).append(true).open(path)?;
```

This is a filesystem roundtrip per log line. At Trace level during UI tree refresh (thousands of elements), this significantly impacts performance.

**Recommendation:** Use a `BufWriter` wrapping a persistent file handle, or use an async logging backend. Note: `commons.rs` already defines a `FileWriter` with `BufWriter` — but it's unused.

---

## 3. Architecture and Design Issues

### 3.1 Global Mutable State (WINDRIVER)

**Severity: HIGH** | Affects: bromium

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

**Severity: HIGH** | Affects: bromium, uitree, uiexplore

The following are duplicated nearly identically across 2-3 crates:

| Code | Locations |
|------|-----------|
| `SaveUIElement` struct + impls | `uitree/save_ui_element.rs`, `uitree/uiexplore.rs:186-208`, `uitree/uiexplore_iter.rs:193-194` |
| `execute_with_timeout()` | `bromium/commons.rs`, `uitree/commons.rs`, `uiexplore/commons.rs` (identical) |
| `printfmt!` macro | `bromium/macros.rs`, `uitree/macros.rs`, `uiexplore/macros.rs` (identical) |
| `sendmsg!` macro | `bromium/macros.rs`, `uitree/macros.rs`, `uiexplore/macros.rs` (identical, unused) |
| `get_ui_automation_instance()` | `bromium/uiauto.rs`, `uitree/save_ui_element.rs:212-241`, `winevent-monitor/winevent.rs` |
| XPath generation logic | `uitree/uiexplore.rs:70-403`, `uitree/uiexplore_iter.rs:68-458` (near-identical) |

**Recommendation:** Extract shared code into a `bromium-common` crate or consolidate in `uitree`. The `sendmsg!` macro is unused everywhere and should be deleted.

### 3.3 Duplicate XML/DOM Libraries

**Severity: LOW** | Affects: xmlutil

The xmlutil crate depends on 4 XML libraries simultaneously:
- `roxmltree` (read-only DOM)
- `quick-xml` (streaming writer)
- `xot` (full DOM)
- `xee-xpath` (XPath engine)

Additionally, `xml.rs` defines a custom `XMLDomNode` tree that duplicates `xot`'s functionality, and `XMLDomManager` wraps `Xot`. This results in multiple competing representations.

**Recommendation:** Standardize on one DOM library (likely `xot` since it's used for XPath too) and remove the others.

### 3.4 Error Types as Strings

**Severity: MEDIUM** | Affects: xmlutil, bromium

`xmlutil/xpath_gen.rs:114` returns error messages as regular strings — callers cannot distinguish success from failure without string matching. Similarly, `screen-capture/error.rs` uses a generic `Error(String)` variant that loses context.

**Recommendation:** Use proper `Result<T, E>` types with structured error enums throughout.

---

## 4. Performance Improvements

### 4.1 Linear Element Lookup by Coordinates

**Severity: MEDIUM** | Affects: bromium

`bromium/src/rectangle.rs:12-25` does a linear scan through all UI elements to find one at given coordinates. For a typical desktop with 5,000+ elements, this is O(n) per lookup.

**Recommendation:** Build a spatial index (R-tree or grid) at tree-refresh time for O(log n) coordinate lookups.

### 4.2 Full Tree Clone on Every State Update

**Severity: MEDIUM** | Affects: bromium

Because of the global `WINDRIVER` mutex pattern, the entire UI tree is cloned whenever:
- Timeout is changed (`windriver.rs:615`)
- Window title is set (`windriver.rs:620`)
- Tree is refreshed (`windriver.rs:882,906`)

**Recommendation:** This is solved by fixing issue 3.1 (removing global state).

### 4.3 Repeated DPI Lookups Without Caching

**Severity: LOW** | Affects: screen-capture

`screen-capture/src/mswindows/impl_monitor.rs:89-149` calls `get_process_is_dpi_awareness()` for every monitor property access, which loads `Shcore.dll` and calls `GetProcAddress` each time.

**Recommendation:** Cache the DPI awareness result in a `OnceLock<bool>` since it doesn't change during process lifetime.

### 4.4 Double Sort Where Single Sort Suffices

**Severity: LOW** | Affects: uitree

```rust
// uitree/src/uiexplore_xml.rs:401-403
elements.sort_by(|a, b| a.bounding_rect_size.cmp(&b.bounding_rect_size));
elements.sort_by(|a, b| a.z_order.cmp(&b.z_order));
```

The second sort overrides the first. Use a single sort with tuple comparison:
```rust
elements.sort_by(|a, b| (a.z_order, a.bounding_rect_size).cmp(&(b.z_order, b.bounding_rect_size)));
```

### 4.5 String Allocations in XPath Evaluation

**Severity: LOW** | Affects: xmlutil

`eval_xpath()` takes owned `String` parameters instead of `&str`:
```rust
pub fn eval_xpath(expr: String, srcxml: String) -> XpathResult
```

All result getters return clones:
```rust
pub fn get_result_items(&self) -> Vec<XpathQueryResult> {
    self.result.clone()
}
```

**Recommendation:** Accept `&str` parameters, return `&[XpathQueryResult]` references.

---

## 5. Testing Gaps

### 5.1 No CI/CD Pipeline

**Severity: HIGH**

No `.github/workflows`, no CI configuration of any kind. Changes can be pushed without any automated verification.

### 5.2 Minimal Rust Test Coverage

Current tests:
- `xmlutil`: 4 XPath evaluation tests (good)
- `screen-capture`: ~10 tests for capture and utilities (reasonable)
- `bromium`: 3 tests (timeout helper + 2 UIAutomation creation smoke tests)
- `uitree`: 1 test (timeout helper — duplicated from bromium)
- `uiexplore`: 1 test (timeout helper — duplicated from bromium)
- `winevent-monitor`: 0 tests

**Missing test coverage:**
- No tests for Element click/send_keys/send_text logic
- No tests for XPath generation from runtime IDs
- No tests for UI tree building or refresh
- No tests for app launch/activate logic
- No tests for logging configuration

### 5.3 Python Tests Are Manual Scripts

The two test files (`tests/app_start_danipc.py`, `tests/log_test.py`) are manual scripts, not pytest test suites. They require specific applications (MS Teams) to be installed and running.

**Recommendation:** Add pytest-compatible tests with mocking for the Python API surface.

---

## 6. Minor Issues and Code Hygiene

### 6.1 Typo in Filename

`bromium/src/sreen_context.rs` — missing 'c' in "screen".

### 6.2 Commented-Out Code Bloat

Large blocks of commented-out code exist throughout:
- `sreen_context.rs`: ~170 lines of commented DPI code
- `uiexplore/border.rs`: Entire file is commented out (lines 2-83)
- `uiexplore/app_ui.rs:632-669`: Disabled highlighting code
- `uitree/uiexplore_iter.rs:249-292`: Original recursive implementation
- `bromium/lib.rs:20-27`: Commented imports

### 6.3 `eprintln!` Debug Statements in Production

`xmlutil/src/xpath_gen.rs` lines 11, 30, 61, 63, 66 contain `eprintln!` calls for debugging.

### 6.4 Clippy Warnings

23 clippy warnings in the workspace, including:
- Redundant type casts (`f32` to `f32`)
- Collapsible if/else blocks
- `.len() > 0` instead of `!is_empty()`
- Tuple struct initialization style

### 6.5 README Inconsistencies

- README shows `WinDriver(timeout_ms=5_000)` but actual constructor requires `window_title` parameter too
- README references `get_ui_element_by_xpath` and `refresh_ui_tree` but current API uses `get_element_by_xpath` and `refresh`
- Build instructions say `git clone https://github.com/yourusername/bromium.git` (placeholder URL)

### 6.6 `.pyi` Stub Drift

The `.pyi` stub file's `WinDriver` class documents `get_cursor_pos()` but the README says `get_curser_pos()`. The `launch_or_activate_app` return type differs between `.pyi` (returns `Element`) and README (returns `bool`).

### 6.7 Unused Dependency

`xmlutil` is listed as a dependency of `bromium` in Cargo.toml but never imported in bromium's source code.

---

## 7. Prioritized Recommendations

### Immediate (safety/correctness)

1. Replace `.unwrap()` calls with proper error handling — especially the Drop impl in winevent-monitor
2. Fix `Box::leak` memory leaks in screen-capture callbacks
3. Remove `println!` debug statements from screen-capture
4. Add `recv_timeout` to all channel receives

### Short-term (architecture)

5. Remove global `WINDRIVER` static — use PyO3 instance storage
6. Extract duplicated code into a shared crate
7. Replace `unsafe impl Send/Sync` with data-only structs for cross-thread transfer
8. Set up a basic CI pipeline (cargo clippy + cargo test)

### Medium-term (quality)

9. Add unit tests for XPath generation, element lookup, and tree building
10. Add pytest test suite for the Python API
11. Consolidate XML libraries in xmlutil
12. Fix clippy warnings and clean up commented-out code
13. Rename `sreen_context.rs` to `screen_context.rs`
14. Update README to match current API

### Long-term (performance)

15. Add spatial indexing for coordinate-based element lookup
16. Buffer log file writes
17. Cache DPI awareness checks
18. Consider async logging backend for high-throughput scenarios
