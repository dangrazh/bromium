# Bromium Project Audit Report

**Date:** 2026-06-21
**Auditor:** Claude Code (Opus 4.6)
**Branch:** `Consolidation`
**Scope:** Full codebase review — architecture, correctness, safety, performance, idiomacy, testing, dependencies, CI/CD

---

## 1. Project Overview

Bromium is a Rust workspace with 7 crates (~10,200 lines of Rust) that provides infrastructure for automating Windows Desktop Applications:

| Crate | Purpose | LOC (approx) |
|-------|---------|-------------|
| `bromium` | PyO3 Python extension — the shipped library | ~1,700 |
| `uitree` | UI tree traversal, serialization, XPath queries | ~2,200 |
| `xmlutil` | XPath evaluation/generation, XML DOM management | ~1,100 |
| `bromium-common` | Shared utilities (UIA instance, runtime ID formatting, timeout) | ~200 |
| `screen-capture` | Windows screen capture library | ~2,500 |
| `uiexplore` | egui/eframe desktop inspector app | ~1,700 |
| `winevent-monitor` | Windows event hook wrapper | ~210 |

**Build status at time of audit:** Compiles clean, zero clippy warnings, all 19 tests pass.

---

## 2. Findings

Each finding is assigned a severity and a unique ID for cross-referencing with remediation actions.

### 2.1 Critical / Correctness

#### F-C01: `get_element_by_xpath` retry loop holds the Python GIL

- **File:** `crates/bromium/src/windriver.rs`, lines 960–1020
- **Description:** The retry loop calls `thread::sleep(250ms)` and `refresh_ui_tree()` while holding the Python GIL. During retries (up to `timeout_ms`, default 5,000ms), all other Python threads are blocked. Each iteration also rebuilds the entire UI tree via COM traversal and XML serialization.
- **Impact:** Python applications using `get_element_by_xpath` with retries will experience unresponsive threads and degraded performance. In async Python contexts this can cause cascading timeouts.

#### F-C02: `get_point_bounding_rect` may return wrong element

- **File:** `crates/bromium/src/rectangle.rs`, lines 15–30
- **Description:** The linear scan returns the **first** element whose bounding rect contains the point. Elements are sorted ascending by z-order then ascending by bounding rect size. The root element (z-order 999) sorts last, but among same-z-order elements the smallest sorts first. However, parent containers and child elements at the same z-order level may cause the parent to be returned instead of the deepest enclosing child, depending on insertion order.
- **Impact:** `get_element_by_coordinates()` may return a parent container instead of the innermost clickable element.

#### F-C03: `WinEvtMonitorEvent` always returns stale placeholder values

- **File:** `crates/winevent-monitor/src/winevent.rs`, lines 1, 56–108
- **Description:** The `#![allow(unused)]` blanket suppresses all warnings. The variables `name` and `rt_id` in `check_for_events` are initialized to `"no name retrieved"` and `[0,0,0,0]` respectively, but are never updated because the element-from-handle code (lines 82–94) is commented out. Every `WinEvtMonitorEvent` carries meaningless metadata.
- **Impact:** Consumers of the `winevent-monitor` crate receive events with no usable element identification.

### 2.2 Design / Architecture

#### F-D01: Three nearly-identical `UITree` implementations

- **Files:** `crates/uitree/src/uiexplore.rs`, `uiexplore_xml.rs`, `uiexplore_iter.rs`
- **Description:** Each file defines its own `UITree` struct with duplicated logic for `build_node_to_elem`, `for_each`, `root`, `children`, `node`, and other methods. They are re-exported via type aliases (`UITree`, `UITreeXML`, `UITreeIter`). Only `UITreeXML` is used by the `bromium` crate.
- **Impact:** Triple maintenance burden. Bug fixes or behavior changes must be applied in three places.

#### F-D02: Redundant `From` conversions re-fetch COM elements

- **File:** `crates/bromium/src/windriver.rs`, lines 528–613
- **Description:** `From<&SaveUIElementXML> for Element` calls `get_ui_automation_ui_element()` which performs a full tree search via COM to get a live `UIElement` — only to extract properties (`name`, `control_type`, `runtime_id`, etc.) that are already stored in the `SaveUIElement`. The `element_from_save_ui` helper (line 692) correctly reads from the saved properties without COM roundtrip.
- **Impact:** Unnecessary COM calls on every conversion, adding latency and potential failure points.

#### F-D03: Imperative push-loops where `map`/`collect` suffices

- **File:** `crates/bromium/src/windriver.rs`, lines 1046–1087 (`get_elements_by_xpath`)
- **Description:** Manual `for` loop with `Vec::new()` and `.push()` where `.iter().map(...).collect()` would be clearer and more idiomatic. Same pattern in `get_element_by_xpath`'s success path.
- **Impact:** Readability; makes the code harder to review at a glance.

#### F-D04: Excessive `.clone()` in the `bromium` crate

- **Files:** `crates/bromium/src/windriver.rs` (27 occurrences), others (7 occurrences)
- **Description:** Many clones are unnecessary — e.g., `window_title.clone()` on line 723 where the value is only matched for `Some`; PyO3 getters returning `String` via `.clone()` when `&str` would work for some cases.
- **Impact:** Unnecessary heap allocations on every Python attribute access and method call.

#### F-D05: `conversion.rs` reimplements `Display`/`FromStr`

- **File:** `crates/uitree/src/conversion.rs`
- **Description:** `ConvertFromControlType` and `ConvertToControlType` manually match every `ControlType` variant. Idiomatic Rust would use `Display`/`FromStr` implementations (or a derive macro).
- **Impact:** Fragile — any new `ControlType` variant added to the `uiautomation` crate will cause a silent mismatch or compile error depending on the `match` exhaustiveness.

### 2.3 Performance

#### F-P01: Full UI tree rebuild on every retry

- **File:** `crates/bromium/src/windriver.rs`, lines 975–1005
- **Description:** `get_element_by_xpath`'s retry loop calls `refresh_ui_tree()` on each iteration, which spawns a thread, walks the entire Windows UI Automation tree via COM, serializes to XML, sorts elements, and constructs the `UITreeMap`. On a desktop with 10,000+ elements this can take several seconds per iteration.
- **Impact:** A 5-second timeout with 250ms sleep intervals could trigger 20 full tree rebuilds — taking up to a minute on complex desktops.

#### F-P02: Double lookup in `append_or_replace_subtree`

- **File:** `crates/uitree/src/uiexplore_xml.rs`, lines 246–254
- **Description:** Calls `get_element_by_runtime_id()` once to check `.is_some()` and a second time to `.unwrap()` the value. This performs two hash lookups instead of one.
- **Impact:** Minor — but easily fixed and sets a bad precedent.

### 2.4 Safety / Robustness

#### F-S01: `Mutex::lock().unwrap()` in logging can crash the Python process

- **File:** `crates/bromium/src/logging.rs` — 18 occurrences
- **Description:** All four global mutexes (`LOG_LEVEL`, `LOG_FILE`, `LOG_TO_CONSOLE`, `LOG_TO_FILE`) are accessed via `.lock().unwrap()`. If any thread panics while holding a lock, the mutex is poisoned and subsequent `.unwrap()` calls will panic — crashing the entire Python process.
- **Impact:** An unrecoverable crash in production if any logging thread panics.

#### F-S02: `unsafe` blocks lack safety documentation

- **Files:** `crates/bromium/src/windriver.rs` (line 897), `crates/bromium/src/rectangle.rs` (lines 38–97), `crates/uiexplore/src/main.rs` (lines 148–196)
- **Description:** Win32 FFI calls are wrapped in `unsafe` blocks without `// SAFETY: ...` comments explaining what invariants are being upheld and why the call is sound.
- **Impact:** Audit and review difficulty. Future maintainers cannot verify correctness without re-researching each FFI call.

#### F-S03: `ScreenContext::new()` panics if no screens found

- **File:** `crates/bromium/src/screen_context.rs`, line 185
- **Description:** `.expect("No screens found")` will panic if no primary screen is found and the screen list is empty. Since this is called from a `#[pymethods]` constructor, it will crash the Python process.
- **Impact:** Unrecoverable crash on headless or unusual display configurations.

#### F-S04: `TryFrom<&SaveUIElement> for UIElement` uses `()` as error type

- **File:** `crates/uitree/src/save_ui_element.rs`, lines 222–232
- **Description:** The `type Error = ()` discards all diagnostic information. Callers cannot distinguish between "element not found" and "COM initialization failed."
- **Impact:** Silent failure with no actionable error information.

### 2.5 Code Quality / Idiomacy

#### F-Q01: Getters return `&String` instead of `&str`

- **File:** `crates/uitree/src/save_ui_element.rs`, lines 65–85
- **Description:** All getters (`get_name()`, `get_classname()`, `get_control_type()`, etc.) return `&String`. Idiomatic Rust returns `&str` from string getters per the API guidelines (prefer `&str` over `&String` in function signatures).
- **Impact:** Forces callers to work with `&String` unnecessarily, and violates Rust API conventions.

#### F-Q02: `FromStrLevelFilter` reinvents `FromStr`

- **File:** `crates/bromium/src/logging.rs`, lines 9–25
- **Description:** A custom trait `FromStrLevelFilter` is defined on `LevelFilter` to parse strings like `"info"` into levels. The `log` crate's `LevelFilter` already implements `std::str::FromStr`.
- **Impact:** Unnecessary code that duplicates standard library functionality and may diverge in behavior.

#### F-Q03: Empty macro files

- **Files:** `crates/bromium/src/macros.rs`, `crates/uitree/src/macros.rs`
- **Description:** Both files are empty (0–1 bytes) but are still declared as modules via `mod macros;`.
- **Impact:** Dead code that adds confusion for new contributors.

#### F-Q04: `XpathResult::get_error_msg` allocates unnecessarily

- **File:** `crates/xmlutil/src/xpath_eval.rs`, lines 76–80
- **Description:** Creates `"".to_string()` when `self.error_msg` is `None`. Should return `Option<&str>` or borrow.
- **Impact:** Minor allocation per call; non-idiomatic.

#### F-Q05: Inconsistent `PyResult::Ok(...)` vs `Ok(...)`

- **File:** `crates/bromium/src/windriver.rs` — throughout
- **Description:** Some methods return `PyResult::Ok(value)` and others return `Ok(value)`. Both compile to the same thing but the inconsistency reduces readability.
- **Impact:** Code style inconsistency.

### 2.6 Testing

#### F-T01: Zero test coverage for the `bromium` crate

- **Description:** The PyO3 binding crate — the shipped artifact — has no unit tests. `Element` methods, `WinDriver` construction, `ElementIterator`, `find_elements`, and `launch_or_activate_app` are all untested.
- **Impact:** Regressions in the Python-facing API will not be caught before release.

#### F-T02: No integration tests in any crate

- **Description:** There are no `tests/` integration test directories. The end-to-end pipeline (UI tree construction -> XPath generation -> XPath evaluation -> element lookup) is only tested through manual use.
- **Impact:** Cross-crate regressions are invisible.

#### F-T03: `tests/` directory is gitignored

- **File:** `.gitignore`, line 63
- **Description:** The pattern `tests/` in `.gitignore` will prevent any future Rust integration test directory from being committed.
- **Impact:** Any developer who creates `crates/*/tests/` will find their tests silently excluded from source control.

### 2.7 Dependencies

#### F-Dep01: Four overlapping XML libraries

- **File:** Workspace `Cargo.toml` files
- **Description:** The workspace depends on `quick-xml` (XML writing), `roxmltree` (XML parsing for XPath gen), `xot` (XML DOM tree manipulation), and `xee-xpath` (XPath evaluation). All four operate on XML but are used for different tasks.
- **Impact:** Large compile-time and binary-size footprint. Maintenance burden of keeping four XML libraries compatible and up to date.

#### F-Dep02: `fs_extra` used only for `dir::create_all`

- **File:** `crates/bromium/src/windriver.rs`, line 1121
- **Description:** `fs_extra::dir::create_all()` is functionally equivalent to `std::fs::create_dir_all()`. The entire `fs_extra` crate is pulled in for this single call.
- **Impact:** Unnecessary dependency; increases supply-chain attack surface and compile time.

#### F-Dep03: `anyhow` underused in `xmlutil`

- **File:** `crates/xmlutil/Cargo.toml`
- **Description:** `anyhow` is listed as a dependency but is only used as `anyhow::Error` and `anyhow::anyhow!` in `xpath_eval.rs`. The crate already uses `thiserror` for structured error types.
- **Impact:** Mixed error-handling strategies in one crate; slight unnecessary compile-time cost.

---

## 3. Remediation Actions

Each action is linked to one or more findings and assigned a priority.

### Priority Definitions

| Priority | Definition |
|----------|------------|
| **P0** | Fix immediately — correctness or crash bug affecting production |
| **P1** | Fix before next release — significant quality or performance issue |
| **P2** | Fix soon — code quality, idiomacy, or maintainability improvement |
| **P3** | Fix when convenient — nice-to-have cleanup |

---

### R-01: Release the GIL during `get_element_by_xpath` retries

| | |
|---|---|
| **Priority** | P0 |
| **Findings** | F-C01, F-P01 |
| **Description** | Wrap the retry loop body in `py.allow_threads(\|\| { ... })` to release the GIL during `thread::sleep` and `refresh_ui_tree`. Alternatively, restructure to perform the blocking work in a spawned thread and poll from Python. |
| **Acceptance criteria** | Other Python threads can execute during `get_element_by_xpath` retries. Verified by a multi-threaded Python test. |

### R-02: Fix element ordering for `get_point_bounding_rect`

| | |
|---|---|
| **Priority** | P0 |
| **Findings** | F-C02 |
| **Description** | Change `get_point_bounding_rect` to return the **smallest** enclosing element (by bounding rect area) rather than the first match. The simplest approach: iterate all elements, filter to those containing the point, and return the one with the smallest `bounding_rect_size`. Alternatively, reverse the iteration order if the sort guarantees smallest-last. |
| **Acceptance criteria** | Given a button inside a panel, `get_element_by_coordinates` at the button's center returns the button, not the panel. |

### R-03: Fix or remove stale `WinEvtMonitorEvent` fields

| | |
|---|---|
| **Priority** | P1 |
| **Findings** | F-C03 |
| **Description** | Either (a) uncomment and fix the element-from-handle code so `name` and `rt_id` are populated, or (b) remove these fields from `WinEvtMonitorEvent` to avoid misleading consumers. Remove the `#![allow(unused)]` and address each warning individually. |
| **Acceptance criteria** | `WinEvtMonitorEvent` fields are either populated with real data or removed. No blanket `allow(unused)`. |

### R-04: Consolidate the three `UITree` implementations

| | |
|---|---|
| **Priority** | P1 |
| **Findings** | F-D01 |
| **Description** | Merge `UITree` (from `uiexplore.rs`), `UITreeXML` (from `uiexplore_xml.rs`), and `UITreeIter` (from `uiexplore_iter.rs`) into a single `UITree` struct. The XML DOM tree (`xml_dom_tree: String`) can be an `Option<String>` for the non-XML variant, or the struct can always carry it. Keep the type aliases as deprecated re-exports during the transition. |
| **Acceptance criteria** | One `UITree` struct. Existing callers compile without change (via aliases). Shared logic is defined once. |

### R-05: Remove redundant `From` conversions that re-fetch COM elements

| | |
|---|---|
| **Priority** | P1 |
| **Findings** | F-D02 |
| **Description** | Delete `From<&UIElement> for Element` and `From<&SaveUIElementXML> for Element` in `windriver.rs` (lines 528–613). Replace all call sites with `element_from_save_ui()` which reads from saved properties without COM roundtrips. |
| **Acceptance criteria** | No `From` impl for `Element` that calls `get_ui_automation_ui_element()`. All element construction goes through `element_from_save_ui` or `Element::new`. |

### R-06: Handle mutex poisoning in logging

| | |
|---|---|
| **Priority** | P1 |
| **Findings** | F-S01 |
| **Description** | Replace all `.lock().unwrap()` on the four global mutexes with `.lock().unwrap_or_else(\|e\| e.into_inner())` to recover from poisoned mutexes gracefully. Alternatively, use `parking_lot::Mutex` which does not have poisoning. |
| **Acceptance criteria** | A poisoned logging mutex does not crash the Python process. |

### R-07: Fix `ScreenContext::new()` to not panic

| | |
|---|---|
| **Priority** | P1 |
| **Findings** | F-S03 |
| **Description** | Change `ScreenContext::new()` to return `PyResult<Self>` and replace the `.expect(...)` with a proper `PyErr` (e.g., `AutomationError::new_err("No display screens found")`). |
| **Acceptance criteria** | `ScreenContext::new()` raises a Python exception instead of crashing on headless systems. |

### R-08: Remove `fs_extra` dependency

| | |
|---|---|
| **Priority** | P2 |
| **Findings** | F-Dep02 |
| **Description** | Replace `fs_extra::dir::create_all(out_dir.clone(), true)` with `std::fs::create_dir_all(&out_dir)` in `windriver.rs`. Remove `fs_extra` from `crates/bromium/Cargo.toml`. |
| **Acceptance criteria** | `fs_extra` is not in `Cargo.lock`. Screenshot directory creation still works. |

### R-09: Remove `.gitignore` entry for `tests/`

| | |
|---|---|
| **Priority** | P2 |
| **Findings** | F-T03 |
| **Description** | Remove the `tests/` line from `.gitignore` (line 63) so that Rust integration test directories can be committed. If the intent was to ignore Python test files, use a more specific pattern (e.g., `test_bromium.py` which is already listed on line 2). |
| **Acceptance criteria** | `git add crates/uitree/tests/` works. |

### R-10: Add `// SAFETY:` comments to all `unsafe` blocks

| | |
|---|---|
| **Priority** | P2 |
| **Findings** | F-S02 |
| **Description** | Document the safety invariants for every `unsafe` block. For Win32 FFI calls, explain: (1) why the function pointer / handle is valid, (2) what the caller guarantees about lifetimes, (3) any thread-safety requirements. |
| **Acceptance criteria** | Every `unsafe` block has a `// SAFETY:` comment. Clippy `undocumented_unsafe_blocks` lint passes (when enabled). |

### R-11: Change `SaveUIElement` getters to return `&str`

| | |
|---|---|
| **Priority** | P2 |
| **Findings** | F-Q01 |
| **Description** | Change all `get_*()` methods that currently return `&String` to return `&str`. This is a non-breaking change for most callers since `&String` auto-derefs to `&str`. |
| **Acceptance criteria** | All `SaveUIElement` string getters have return type `&str`. All callers compile. |

### R-12: Replace `FromStrLevelFilter` with `std::str::FromStr`

| | |
|---|---|
| **Priority** | P2 |
| **Findings** | F-Q02 |
| **Description** | Remove the `FromStrLevelFilter` trait. Use `log_level.parse::<LevelFilter>().unwrap_or(LevelFilter::Info)` at call sites. |
| **Acceptance criteria** | `FromStrLevelFilter` trait and its `impl` are deleted. Parsing behavior is preserved. |

### R-13: Remove empty macro files

| | |
|---|---|
| **Priority** | P3 |
| **Findings** | F-Q03 |
| **Description** | Delete `crates/bromium/src/macros.rs` and `crates/uitree/src/macros.rs`. Remove the corresponding `mod macros;` declarations from `lib.rs`. |
| **Acceptance criteria** | No empty macro files. Workspace compiles. |

### R-14: Refactor imperative loops to iterators

| | |
|---|---|
| **Priority** | P3 |
| **Findings** | F-D03 |
| **Description** | Replace the manual `for` + `push` patterns in `get_elements_by_xpath` and similar methods with `.iter().map(...).collect()`. |
| **Acceptance criteria** | No `Vec::new()` + manual push where `collect` is applicable in the `bromium` crate. |

### R-15: Reduce unnecessary `.clone()` calls

| | |
|---|---|
| **Priority** | P3 |
| **Findings** | F-D04 |
| **Description** | Audit each `.clone()` call in the `bromium` crate. Remove clones where borrowing is sufficient (e.g., `window_title.clone()` for pattern matching, `control_type.clone()` where the owned value is not needed). For PyO3 getters, keep clones where PyO3 requires owned values. |
| **Acceptance criteria** | At least 10 unnecessary clones removed. All tests pass. |

### R-16: Use proper error type for `TryFrom<&SaveUIElement>`

| | |
|---|---|
| **Priority** | P3 |
| **Findings** | F-S04 |
| **Description** | Replace `type Error = ()` with a meaningful error type (e.g., a variant of `UITreeError` or a new `ElementResolutionError`). |
| **Acceptance criteria** | `TryFrom` error type carries diagnostic information. |

### R-17: Normalize `PyResult::Ok(...)` to `Ok(...)`

| | |
|---|---|
| **Priority** | P3 |
| **Findings** | F-Q05 |
| **Description** | Replace all `PyResult::Ok(...)` with `Ok(...)` throughout the codebase for consistency (the type is inferred). |
| **Acceptance criteria** | No occurrences of `PyResult::Ok` in source files. |

### R-18: Fix double lookup in `append_or_replace_subtree`

| | |
|---|---|
| **Priority** | P3 |
| **Findings** | F-P02 |
| **Description** | Replace the `.is_some()` + `.unwrap()` double lookup with a single `if let Some(existing) = ...`. |
| **Acceptance criteria** | Single call to `get_element_by_runtime_id` in that code path. |

### R-19: Make `XpathResult::get_error_msg` zero-allocation

| | |
|---|---|
| **Priority** | P3 |
| **Findings** | F-Q04 |
| **Description** | Change `get_error_msg()` to return `Option<&str>` or `&str` (with `""` default via `as_deref().unwrap_or("")`). |
| **Acceptance criteria** | No heap allocation when there is no error message. |

### R-20: Evaluate XML library consolidation

| | |
|---|---|
| **Priority** | P3 |
| **Findings** | F-Dep01, F-Dep03 |
| **Description** | Investigate whether `xot` (already used for DOM manipulation) can replace `roxmltree` (used only for XPath generation) and `quick-xml` (used for XML writing during tree construction). Evaluate whether `xee-xpath` can be used with `xot`'s DOM directly. Also remove `anyhow` from `xmlutil` by converting its few uses to `thiserror` errors or propagating concrete error types. |
| **Acceptance criteria** | At minimum: `anyhow` removed from `xmlutil`. Ideally: one fewer XML library. |

### R-21: Add test coverage for the `bromium` crate

| | |
|---|---|
| **Priority** | P2 |
| **Findings** | F-T01, F-T02 |
| **Description** | Add unit tests for non-COM-dependent logic: `Element::new`, `Element::default`, `From` conversions, `ElementIterator`, `find_elements` (with a mock `UITreeXML`), `normalized()`, `WinDriver::element_from_save_ui`. Add at least one integration test for the `uitree` crate covering the XPath generation -> evaluation roundtrip. |
| **Acceptance criteria** | At least 10 new tests. `cargo test --workspace` passes. Test count > 30. |

### R-22: Replace `conversion.rs` with idiomatic trait impls

| | |
|---|---|
| **Priority** | P3 |
| **Findings** | F-D05 |
| **Description** | Replace `ConvertFromControlType` with `impl Display for ControlType` (via a newtype wrapper if the orphan rule prevents direct impl). Replace `ConvertToControlType` with `impl FromStr`. |
| **Acceptance criteria** | Custom conversion traits are removed. Newtype or direct `Display`/`FromStr` implementations are in place. |

---

## 4. Remediation Priority Summary

| Priority | Count | Actions |
|----------|-------|---------|
| **P0** | 2 | R-01, R-02 |
| **P1** | 5 | R-03, R-04, R-05, R-06, R-07 |
| **P2** | 6 | R-08, R-09, R-10, R-11, R-12, R-21 |
| **P3** | 9 | R-13, R-14, R-15, R-16, R-17, R-18, R-19, R-20, R-22 |

---

## 5. Findings-to-Remediation Cross-Reference

| Finding | Remediation(s) |
|---------|----------------|
| F-C01 | R-01 |
| F-C02 | R-02 |
| F-C03 | R-03 |
| F-D01 | R-04 |
| F-D02 | R-05 |
| F-D03 | R-14 |
| F-D04 | R-15 |
| F-D05 | R-22 |
| F-P01 | R-01 |
| F-P02 | R-18 |
| F-S01 | R-06 |
| F-S02 | R-10 |
| F-S03 | R-07 |
| F-S04 | R-16 |
| F-Q01 | R-11 |
| F-Q02 | R-12 |
| F-Q03 | R-13 |
| F-Q04 | R-19 |
| F-Q05 | R-17 |
| F-T01 | R-21 |
| F-T02 | R-21 |
| F-T03 | R-09 |
| F-Dep01 | R-20 |
| F-Dep02 | R-08 |
| F-Dep03 | R-20 |

---

## 6. Strengths (No Action Required)

The following areas are in good shape and require no remediation:

1. **Workspace architecture** — clean crate separation with a unidirectional dependency graph.
2. **Python exception hierarchy** — `ElementNotFoundError`, `AutomationError`, `TreeConstructionError` with descriptive messages.
3. **`UITreeMap<T>` data structure** — well-designed arena-allocated tree with O(1) lookup, 15 unit tests.
4. **CI pipeline** — `fmt` + `clippy` + `test` on `windows-latest`.
5. **Pythonic API surface** — proper `__len__`, `__iter__`, `__contains__`, `__repr__`, `__str__` protocols.
6. **README documentation** — comprehensive API reference with Python examples.
