# Bromium Project — Combined Audit Findings Report

**Date:** 2025-06-22
**Branch:** `Consolidation`
**Sources:** Claude Code static analysis (CC), Claude CX static analysis (CX)
**Scope:** Full workspace — 7 crates, all Rust source files, CI configuration, Python packaging

---

## Table of Contents

- [1. Source Concordance](#1-source-concordance)
- [2. Findings](#2-findings)
  - [Critical](#critical)
  - [High](#high)
  - [Medium](#medium)
  - [Low](#low)
- [3. Remediation Actions](#3-remediation-actions)
- [4. Implementation Sequence](#4-implementation-sequence)

---

## 1. Source Concordance

The two audits overlap on two areas and diverge elsewhere. This table maps equivalent or related findings.

| Combined ID | CC Finding | CX Finding | Topic |
|-------------|-----------|-----------|-------|
| CF-03 | F-03 | F-003 | Orphaned thread accumulation on tree refresh timeout |
| CF-25 | F-22 (partial) | F-008 (partial) | Repository hygiene (`tokensave.db`, nested lockfile, `.pyo3venv`) |

All other findings are unique to one audit source and are included without modification.

---

## 2. Findings

### Critical

#### CF-01 Memory Safety: Video Recorder Row Pitch Mismatch

| Attribute   | Value |
|-------------|-------|
| **Source**    | CC F-01 |
| **Severity**  | Critical |
| **Crate**     | `screen-capture` |
| **File**      | `src/mswindows/impl_video_recorder.rs` |
| **Category**  | Memory Safety |

**Description:**
`slice::from_raw_parts` constructs a slice using `Height * RowPitch` as the byte length. The subsequent `bgra_to_rgba` conversion assumes tightly-packed pixel data (`Width * 4` bytes per row). GPU textures frequently have `RowPitch > Width * 4` due to alignment padding. When this occurs, the RGBA conversion reads padding bytes as pixel data, producing corrupted output. In the worst case, the slice may extend beyond the mapped allocation, triggering undefined behaviour.

**Evidence:**
`impl_video_recorder.rs` — `texture_to_frame` constructs the slice from `mapped.pData` with a length derived from `RowPitch`, but downstream processing ignores the pitch.

---

#### CF-02 Infinite Loop Risk in UI Tree Sibling Walkers

| Attribute   | Value |
|-------------|-------|
| **Source**    | CC F-02 |
| **Severity**  | Critical |
| **Crate**     | `uitree` |
| **Files**     | `src/uiexplore.rs`, `src/uiexplore_iter.rs`, `src/uiexplore_xml.rs` |
| **Category**  | Correctness / Robustness |

**Description:**
All three tree-walker implementations use `while let Ok(sibling) = walker.get_next_sibling(&next)` to iterate horizontally. There is no visited-set guard and no sibling counter limit. The `max_depth` parameter only bounds vertical depth. If the COM UIAutomation API returns a cycle (possible with crashed or hung processes), the loop runs indefinitely, hanging the calling thread.

**Evidence:**
`uiexplore.rs` ~line 132, `uiexplore_xml.rs` ~line 816 — unbounded sibling loops with no termination safeguard beyond the COM call returning `Err`.

---

### High

#### CF-03 Orphaned Thread Accumulation on Tree Construction Timeout

| Attribute   | Value |
|-------------|-------|
| **Source**    | CC F-03 + CX F-003 (corroborated) |
| **Severity**  | High |
| **Crate**     | `bromium` |
| **File**      | `src/windriver.rs` |
| **Category**  | Resource Management |

**Description:**
Tree construction is dispatched via `thread::spawn` with a 120-second `recv_timeout`. If the receiver times out, the spawned thread continues running with no cancellation signal. Repeated refresh attempts (e.g., via a retry loop in `get_element_by_xpath`) can accumulate orphaned threads, each holding COM resources and consuming memory.

Both audits independently identified this issue. The CX audit additionally notes that `bromium-common/src/timeout.rs` documents that timed-out work keeps running in the background and is not cancelled, and that repeated lookup retries compound the problem.

**Evidence:**
- `windriver.rs` — `new()`, `refresh_ui_tree()`, `refresh_ui_tree_top_2()`, and the retry loop all spawn fire-and-forget threads.
- `bromium-common/src/timeout.rs` — documents that timed-out work is not cancelled.

---

#### CF-04 Tombstone Corruption in UITreeMap After Node Removal

| Attribute   | Value |
|-------------|-------|
| **Source**    | CC F-04 |
| **Severity**  | High |
| **Crate**     | `uitree` |
| **File**      | `src/tree_map.rs` |
| **Category**  | Correctness |

**Description:**
`remove_node` replaces a removed node with a default-constructed placeholder but does not reclaim the arena slot or mark it as dead. Subsequent operations (`for_each`, `build_node_to_elem`, `debug_tree`) iterate over all arena slots including tombstones, potentially returning default/garbage data. The default node index `0` maps to the root element in `node_to_elem`, so tombstones silently alias the root.

**Evidence:**
`tree_map.rs` ~line 161 — `remove_node` writes a default `T` value; no `is_alive` flag or compaction logic exists.

---

#### CF-05 CI is Currently Failing

| Attribute   | Value |
|-------------|-------|
| **Source**    | CX F-001 |
| **Severity**  | High |
| **Crate**     | Workspace-wide |
| **Files**     | `.github/workflows/ci.yml`, various source files |
| **Category**  | CI / Build Infrastructure |

**Description:**
`cargo fmt --check --all` fails due to formatting issues in `bromium/src/screen_context.rs`, `bromium/src/windriver.rs`, and `uiexplore/src/app_ui.rs`. `cargo test --workspace --all-features` fails in `screen-capture` at `test_capture_monitor`. The main branch/PR validation is not reliable.

**Evidence:**
- Formatting failures in three files.
- `test_capture_monitor` returns an error when `capture_monitor(x, y, 100, 100)` is called.

---

#### CF-06 Generated XPath is Invalid for UI Names Containing Quotes

| Attribute   | Value |
|-------------|-------|
| **Source**    | CX F-002 |
| **Severity**  | High |
| **Crate**     | `xmlutil`, `uitree` |
| **Files**     | `xmlutil/src/xpath_gen.rs`, `uitree/src/uiexplore_xml.rs` |
| **Category**  | Correctness |

**Description:**
`xpath_gen.rs` interpolates XML attribute values directly into XPath string literals, for example `//*[@{}='{}']` and `{}[@Name='{}']`. `uiexplore_xml.rs` writes Windows UI Automation names directly into the `Name` XML attribute. Windows UI element names can contain apostrophes or quotes. A name such as `Bob's App` will generate an invalid XPath expression, breaking locator generation and lookup.

**Evidence:**
- `xpath_gen.rs` — direct string interpolation into XPath attribute predicates.
- `uiexplore_xml.rs` — unescaped names written into XML `Name` attribute.

---

### Medium

#### CF-07 Duplicated Element Action Boilerplate in bromium::Element

| Attribute   | Value |
|-------------|-------|
| **Source**    | CC F-05 |
| **Severity**  | Medium |
| **Crate**     | `bromium` |
| **File**      | `src/windriver.rs` |
| **Category**  | Maintainability / Code Duplication |

**Description:**
The pattern `if let Ok(e) = convert_to_ui_element(self) { match e.action() { ... } } else { Err(ElementNotFoundError) }` is repeated verbatim across seven `Element` methods: `send_click`, `send_double_click`, `send_right_click`, `hold_click`, `send_keys`, `hold_send_keys`, and `show_context_menu`. This accounts for approximately 300 lines of duplicated code.

**Evidence:**
`windriver.rs` lines ~223-522 — seven near-identical method bodies.

---

#### CF-08 rectangle.rs Duplicated Across Crates With Divergent Behaviour

| Attribute   | Value |
|-------------|-------|
| **Source**    | CC F-06 |
| **Severity**  | Medium |
| **Crates**    | `bromium`, `uiexplore` |
| **Files**     | `bromium/src/rectangle.rs`, `uiexplore/src/rectangle.rs` |
| **Category**  | Code Duplication / Correctness |

**Description:**
`draw_frame`, `clear_frame`, and `is_inside_rectangle` are near-identical across both files. However, `get_point_bounding_rect` differs: `uiexplore` returns the **first** match (z-order dependent), while `bromium` returns the **smallest-area** match. The smallest-area algorithm is more correct for nested elements. This divergence means the two consumers silently apply different hit-test semantics.

**Evidence:**
Comparing `bromium/src/rectangle.rs` and `uiexplore/src/rectangle.rs` — shared functions are copy-pasted, with the critical `get_point_bounding_rect` diverging.

---

#### CF-09 ScreenContext and ScreenInfo Not Registered in PyO3 Module

| Attribute   | Value |
|-------------|-------|
| **Source**    | CC F-07 |
| **Severity**  | Medium |
| **Crate**     | `bromium` |
| **File**      | `src/lib.rs` |
| **Category**  | API Completeness |

**Description:**
`ScreenContext` and `ScreenInfo` are `#[pyclass]` types returned by `WinDriver.get_screen_context()`, but they are not registered in the `#[pymodule]` function. Python code can use returned instances, but `isinstance()` checks, type annotations, and direct construction from Python will fail.

**Evidence:**
`lib.rs` lines 19-51 — `ScreenContext` and `ScreenInfo` are absent from the `m.add_class::<...>()` registrations.

---

#### CF-10 GDI Resource Cleanup Without RAII Guards

| Attribute   | Value |
|-------------|-------|
| **Source**    | CC F-08 |
| **Severity**  | Medium |
| **Crate**     | `bromium` |
| **File**      | `src/rectangle.rs` |
| **Category**  | Resource Safety |

**Description:**
`draw_frame` manually calls `ReleaseDC`/`DeleteObject` at every error branch and at the end of the function. This pattern is fragile — any future edit that adds an early return risks leaking a GDI handle. A RAII wrapper with a `Drop` implementation would make cleanup automatic and robust.

**Evidence:**
`rectangle.rs` lines ~43-102 — repeated manual cleanup calls at each error path.

---

#### CF-11 Double COM Round-Trip for element.get_name()

| Attribute   | Value |
|-------------|-------|
| **Source**    | CC F-09 |
| **Severity**  | Medium |
| **Crate**     | `uitree` |
| **File**      | `src/uiexplore_xml.rs` |
| **Category**  | Performance |

**Description:**
In `get_element()`, `element.get_name()` is called twice — each call is a COM interprocess round-trip. The result should be cached in a local variable and reused, halving the COM overhead per element during tree construction.

**Evidence:**
`uiexplore_xml.rs` lines ~733-741 — two separate calls to `element.get_name()`.

---

#### CF-12 Error Context Lost in get_ui_automation_instance

| Attribute   | Value |
|-------------|-------|
| **Source**    | CC F-10 |
| **Severity**  | Medium |
| **Crate**     | `bromium-common` |
| **File**      | `src/uia.rs` |
| **Category**  | Error Handling |

**Description:**
`get_ui_automation_instance` returns `Option<UIAutomation>` instead of `Result<UIAutomation, Error>`. Callers lose all context about *why* the instance creation failed. COM initialisation failures have multiple root causes (e.g., wrong apartment model, missing COM registration) that are all collapsed into `None`.

**Evidence:**
`uia.rs` — return type is `Option` rather than `Result`.

---

#### CF-13 Panic on Hook Installation Failure

| Attribute   | Value |
|-------------|-------|
| **Source**    | CC F-11 |
| **Severity**  | Medium |
| **Crate**     | `winevent-monitor` |
| **File**      | `src/winevent.rs` |
| **Category**  | Error Handling |

**Description:**
`create_hook` calls `.expect()` on the hook installation result, causing a panic if the Win32 `SetWinEventHook` call fails. This should propagate the error to the caller so the application can handle it gracefully rather than crashing.

**Evidence:**
`winevent.rs` ~line 146 — `.expect()` on a fallible Win32 API call.

---

#### CF-14 Silent Discarding of XML Write Errors

| Attribute   | Value |
|-------------|-------|
| **Source**    | CC F-12 |
| **Severity**  | Medium |
| **Crate**     | `uitree` |
| **File**      | `src/uiexplore_xml.rs` |
| **Category**  | Error Handling |

**Description:**
Multiple calls to `xml_writer.write_event(...)` have their `Result` silently discarded with `let _ = ...`. If an XML write fails mid-tree, the output will be silently truncated or malformed, with no indication to the caller.

**Evidence:**
`uiexplore_xml.rs` ~line 791 — `let _ = xml_writer.write_event(...)`.

---

#### CF-15 Logger Initialization Has Incorrect Repeated-Call Behavior and Can Panic

| Attribute   | Value |
|-------------|-------|
| **Source**    | CX F-004 |
| **Severity**  | Medium |
| **Crate**     | `bromium` |
| **File**      | `src/logging.rs` |
| **Category**  | Error Handling / API Correctness |

**Description:**
`log::set_logger(&LOGGER).expect("Failed to initialize logger")` can panic if another logger is already installed. `set_log_level_internal(log_level)` is inside `INIT.call_once`, so repeated calls to `init_logging` update file/console state but do not apply the requested log level after the first initialization. Python tests call `Bromium.init_logging(...)` repeatedly with different log levels.

**Evidence:**
- `logging.rs` — `expect()` on `set_logger`, log level update inside `call_once`.
- Python tests call `init_logging` repeatedly with different levels.

---

#### CF-16 uiexplore Temp-File Signaling is Globally Named and Cross-Instance Unsafe

| Attribute   | Value |
|-------------|-------|
| **Source**    | CX F-005 |
| **Severity**  | Medium |
| **Crate**     | `uiexplore` |
| **File**      | `src/signal_file.rs` |
| **Category**  | Correctness / IPC |

**Description:**
`signal_file.rs` writes `%TEMP%\signal_file.txt` using a fixed path. Multiple `uiexplore` instances can interfere with each other. Any local process can create the same file and trigger termination behavior.

**Evidence:**
`signal_file.rs` — hardcoded global temp path for inter-process signaling.

---

#### CF-17 Screenshots Overwrite Previous Captures

| Attribute   | Value |
|-------------|-------|
| **Source**    | CX F-006 |
| **Severity**  | Medium |
| **Crate**     | `bromium` |
| **File**      | `src/windriver.rs` |
| **Category**  | Usability / Data Loss |

**Description:**
Screenshots are saved as `temp\bromium_screenshots\monitor-{monitor_name}.png`. The filename does not include a timestamp, sequence number, or UUID. Repeated `take_screenshot()` calls on the same monitor overwrite prior captures, destroying useful automation evidence.

**Evidence:**
`windriver.rs` — static filename pattern without uniqueness component.

---

#### CF-18 Python Package Behavior is Not Validated in CI

| Attribute   | Value |
|-------------|-------|
| **Source**    | CX F-007 |
| **Severity**  | Medium |
| **Crate**     | `bromium` |
| **Files**     | `.github/workflows/ci.yml`, `crates/bromium/tests/` |
| **Category**  | CI / Testing |

**Description:**
CI only runs Rust formatting, clippy, and cargo tests. `crates/bromium/tests` contains Python tests, but CI does not build the extension with `maturin` or run `pytest`. PyO3 binding regressions, packaging errors, stub/API drift, and Python-level behavior changes can pass CI undetected.

**Evidence:**
`.github/workflows/ci.yml` — no maturin build or pytest step.

---

### Low

#### CF-19 printfmt! Macro Not Atomic Under Concurrency

| Attribute   | Value |
|-------------|-------|
| **Source**    | CC F-13 |
| **Severity**  | Low |
| **Crate**     | `bromium-common` |
| **File**      | `src/macros.rs` |
| **Category**  | Correctness |

**Description:**
The `printfmt!` macro issues two separate calls (`print!` for the timestamp prefix, then `println!` for the message body). Under concurrent use, output from different threads can interleave between the two calls, producing garbled console output.

**Evidence:**
`macros.rs` — macro body contains separate `print!` and `println!` invocations.

---

#### CF-20 Unnecessary String Cloning in Element Getters

| Attribute   | Value |
|-------------|-------|
| **Source**    | CC F-14 |
| **Severity**  | Low |
| **Crate**     | `bromium` |
| **File**      | `src/windriver.rs` |
| **Category**  | Performance |

**Description:**
`Element` property getters (`name()`, `xpath()`, `control_type()`, `runtime_id()`) clone their `String`/`Vec<i32>` fields on every access. PyO3 supports returning `&str` from getters, and `screen_context.rs` already demonstrates this pattern.

**Evidence:**
`windriver.rs` lines ~183-209 — `.clone()` in each getter. Contrast with `screen_context.rs` lines ~99, 103 which return `&str`.

---

#### CF-21 TreeState Cloned Every Frame in uiexplore

| Attribute   | Value |
|-------------|-------|
| **Source**    | CC F-15 |
| **Severity**  | Low |
| **Crate**     | `uiexplore` |
| **File**      | `src/app_ui.rs` |
| **Category**  | Performance |

**Description:**
In the egui `update` method, `TreeState` is cloned on every frame repaint rather than mutated in place or swapped via `Option::take()`. This creates unnecessary allocations on every UI frame.

**Evidence:**
`app_ui.rs` lines ~1041-1046 — `TreeState` clone in the render loop.

---

#### CF-22 Full SaveUIElement Clone for Set Membership Test

| Attribute   | Value |
|-------------|-------|
| **Source**    | CC F-16 |
| **Severity**  | Low |
| **Crate**     | `uitree` |
| **File**      | `src/uiexplore_xml.rs` |
| **Category**  | Performance |

**Description:**
`remove_in_place` clones the entire `check` slice into a `HashSet<SaveUIElement>`, including all `String` fields, just to test set membership. A `HashSet<Vec<i32>>` of runtime IDs would achieve the same membership test without cloning any strings.

**Evidence:**
`uiexplore_xml.rs` ~line 328 — full clone of `SaveUIElement` vector into `HashSet`.

---

#### CF-23 Dead Code Across Multiple Crates

| Attribute   | Value |
|-------------|-------|
| **Source**    | CC F-17 |
| **Severity**  | Low |
| **Crates**    | `bromium`, `winevent-monitor`, `uitree` |
| **Category**  | Maintainability |

**Description:**
Several fields and functions are unused:

| Item | Location | Status |
|------|----------|--------|
| `tree_needs_update` field | `bromium/windriver.rs:592` | Set to `false`, never read |
| `last_hwnd` field | `winevent-monitor/winevent.rs:52` | Written, never consumed |
| `draw_frame` / `clear_frame` | `bromium/rectangle.rs` | Suppressed with `#[allow(dead_code)]` |
| `get_element(&self) -> &Self` | `uitree/save_ui_element.rs:108` | Returns `self` — no-op indirection |

---

#### CF-24 Duplicated Root-Setup and Sorting Logic in Tree Walkers

| Attribute   | Value |
|-------------|-------|
| **Source**    | CC F-18 |
| **Severity**  | Low |
| **Crate**     | `uitree` |
| **Files**     | `src/uiexplore.rs`, `src/uiexplore_iter.rs`, `src/uiexplore_xml.rs` |
| **Category**  | Code Duplication |

**Description:**
The root-element setup (create `SaveUIElement`, format `runtime_id`, build item string, create `UITreeMap`, push to vector) and the child-sorting lambda are copy-pasted across all three tree-walker modules — approximately 80 lines of identical code.

**Evidence:**
Compare the opening ~20 lines of the main walk function in each of the three modules — they are structurally identical.

---

#### CF-25 Repository Hygiene Issues

| Attribute   | Value |
|-------------|-------|
| **Source**    | CC F-22 + CX F-008 (merged) |
| **Severity**  | Low |
| **Files**     | `.tokensave/tokensave.db`, `crates/bromium/.pyo3venv`, `crates/screen-capture/Cargo.lock` |
| **Category**  | Repository Hygiene |

**Description:**
Multiple repository hygiene issues were identified by both audits:
- `.tokensave/tokensave.db` appears as modified in git status. Although `.gitignore` may now list it, the file was tracked before the ignore rule was added. It needs `git rm --cached` to untrack it.
- `crates/bromium/.pyo3venv` exists under the package directory and can pollute searches and packaging.
- `crates/screen-capture/Cargo.lock` exists inside a workspace member even though the workspace root has `Cargo.lock`.

---

#### CF-26 Workspace Dependencies Not Centralised

| Attribute   | Value |
|-------------|-------|
| **Source**    | CC F-19 |
| **Severity**  | Low |
| **Crates**    | Multiple |
| **Files**     | Various `Cargo.toml` |
| **Category**  | Build Configuration |

**Description:**
`thiserror = "2.0"` is independently declared in `bromium`, `screen-capture`, `uitree`, and `xmlutil`. `quick-xml = "0.38.0"` is independently declared in `uitree` and `xmlutil`. `screen-capture` declares its own `windows` version outside `workspace = true`. These should use `[workspace.dependencies]` to prevent version drift.

---

#### CF-27 XMLDomWriter::to_string Shadows the ToString Trait

| Attribute   | Value |
|-------------|-------|
| **Source**    | CC F-20 |
| **Severity**  | Low |
| **Crate**     | `xmlutil` |
| **File**      | `src/xml_dom_manager.rs` |
| **Category**  | API Design |

**Description:**
`XMLDomWriter` has a method named `to_string()` that shadows the `ToString` trait's `to_string()` method. This can confuse callers who expect standard trait semantics.

**Evidence:**
`xml_dom_manager.rs` ~line 243 — `fn to_string(...)` defined on `XMLDomWriter`.

---

#### CF-28 Runtime ID Collision in node_to_elem Mapping

| Attribute   | Value |
|-------------|-------|
| **Source**    | CC F-21 |
| **Severity**  | Low |
| **Crate**     | `uitree` |
| **File**      | `src/uiexplore_xml.rs` |
| **Category**  | Correctness |

**Description:**
`build_node_to_elem` uses runtime ID as a HashMap key. Some UI controls report empty runtime IDs. When two elements share the same (empty) runtime ID, the HashMap silently overwrites the earlier entry, causing one tree node to point at the wrong element.

**Evidence:**
`uiexplore_xml.rs` ~line 49 — HashMap insertion with no collision handling.

---

#### CF-29 Hardcoded 120-Second Timeout Not Configurable

| Attribute   | Value |
|-------------|-------|
| **Source**    | CC F-23 |
| **Severity**  | Low |
| **Crate**     | `bromium` |
| **File**      | `src/windriver.rs` |
| **Category**  | Usability |

**Description:**
Tree construction timeout is hardcoded to 120 seconds in multiple locations (`windriver.rs` lines ~652, 897, 1130, 1166). This is not configurable by the Python caller. If the UI tree is large, 120 seconds may be insufficient; if it hangs, the Python process blocks for 2 minutes with no way to interrupt.

**Evidence:**
`windriver.rs` — literal `120` (or `Duration::from_secs(120)`) appears in four places.

---

## 3. Remediation Actions

| ID | Finding | Action | Effort |
|----|---------|--------|--------|
| **R-01** | [CF-05](#cf-05-ci-is-currently-failing) | Run `cargo fmt --all` and commit the formatting-only changes. | Trivial |
| **R-02** | [CF-05](#cf-05-ci-is-currently-failing) | Diagnose `test_capture_monitor` by surfacing the concrete capture error. Gate it behind an environment check or mark as `#[ignore]` for CI. | Small |
| **R-03** | [CF-01](#cf-01-memory-safety-video-recorder-row-pitch-mismatch) | Rewrite `texture_to_frame` to iterate row-by-row, copying only `Width * 4` bytes per row and skipping the `RowPitch - Width * 4` padding. Apply BGRA-to-RGBA conversion per-row on the tightly-packed output buffer. | Medium |
| **R-04** | [CF-02](#cf-02-infinite-loop-risk-in-ui-tree-sibling-walkers) | Add a `HashSet<Vec<i32>>` visited-set keyed on runtime ID to each sibling loop. Additionally, add a configurable `max_siblings` counter (default: 10,000) as a hard upper bound. Apply to all three walker modules. | Medium |
| **R-05** | [CF-03](#cf-03-orphaned-thread-accumulation-on-tree-construction-timeout) | Introduce a shared `Arc<AtomicBool>` cancellation flag. Pass it to the spawned tree-construction thread; check it periodically during the walk. Set it to `true` when the receiver times out. Enforce only one outstanding refresh per `WinDriver`. | Medium |
| **R-06** | [CF-04](#cf-04-tombstone-corruption-in-uitreemap-after-node-removal) | Add an `is_alive: bool` field to the arena node type. Set it to `false` in `remove_node`. Update `for_each`, `build_node_to_elem`, and `debug_tree` to skip dead nodes. | Small |
| **R-07** | [CF-06](#cf-06-generated-xpath-is-invalid-for-ui-names-containing-quotes) | Add an XPath string literal encoder that emits single-quoted strings, double-quoted strings, or `concat(...)` for values containing both quote types. Use it for every generated XPath attribute predicate. Add unit tests. | Medium |
| **R-08** | [CF-12](#cf-12-error-context-lost-in-get_ui_automation_instance) | Change return type from `Option<UIAutomation>` to `Result<UIAutomation, uiautomation::Error>`. Update all call sites. | Small |
| **R-09** | [CF-13](#cf-13-panic-on-hook-installation-failure) | Replace `.expect()` with `?` or `.map_err(...)` to propagate the error. Change the function return type to `Result` if not already. | Trivial |
| **R-10** | [CF-14](#cf-14-silent-discarding-of-xml-write-errors) | Replace `let _ = xml_writer.write_event(...)` with `xml_writer.write_event(...)?` to propagate errors. | Small |
| **R-11** | [CF-15](#cf-15-logger-initialization-has-incorrect-repeated-call-behavior-and-can-panic) | Move log-level updates outside `INIT.call_once`. Replace `expect` on `set_logger` with non-panicking handling. Add tests for repeated initialization. | Small |
| **R-12** | [CF-26](#cf-26-workspace-dependencies-not-centralised) | Add `thiserror`, `quick-xml`, and `windows` to `[workspace.dependencies]` in root `Cargo.toml`. Update each crate to use `workspace = true`. | Small |
| **R-13** | [CF-08](#cf-08-rectanglers-duplicated-across-crates-with-divergent-behaviour) | Move `draw_frame`, `clear_frame`, `is_inside_rectangle`, and `get_point_bounding_rect` (smallest-area algorithm) into `bromium-common`. Delete both crate-local files and re-export. | Medium |
| **R-14** | [CF-10](#cf-10-gdi-resource-cleanup-without-raii-guards) | Create a `GdiGuard` struct implementing `Drop` for `ReleaseDC`/`DeleteObject`. Replace manual cleanup in `draw_frame`/`clear_frame`. | Small |
| **R-15** | [CF-24](#cf-24-duplicated-root-setup-and-sorting-logic-in-tree-walkers) | Extract shared helpers: `create_root_setup()` and `sort_children()`. Place in a shared module (e.g., `walker_common.rs`). | Small |
| **R-16** | [CF-07](#cf-07-duplicated-element-action-boilerplate-in-bromiumelement) | Extract a private `fn with_ui_element<F, R>(&self, action: F) -> PyResult<R>` helper that encapsulates the convert + match + error mapping pattern. Rewrite all seven methods. | Small |
| **R-17** | [CF-23](#cf-23-dead-code-across-multiple-crates) | Remove `tree_needs_update`, `last_hwnd`, no-op `get_element()`. Resolve `draw_frame`/`clear_frame` as part of R-13. | Trivial |
| **R-18** | [CF-09](#cf-09-screencontext-and-screeninfo-not-registered-in-pyo3-module) | Add `m.add_class::<ScreenContext>()?;` and `m.add_class::<ScreenInfo>()?;` to the `#[pymodule]` function. | Trivial |
| **R-19** | [CF-29](#cf-29-hardcoded-120-second-timeout-not-configurable) | Extract timeout into a field on `WinDriver` settable from Python (default 120s). Consolidate the four literal occurrences. | Small |
| **R-20** | [CF-17](#cf-17-screenshots-overwrite-previous-captures) | Generate unique screenshot filenames with timestamp + monotonic counter. Optionally add an explicit output path parameter. | Small |
| **R-21** | [CF-27](#cf-27-xmldomwriterto_string-shadows-the-tostring-trait) | Rename `XMLDomWriter::to_string()` to `serialize()` or `to_xml_string()`. Update all call sites. | Trivial |
| **R-22** | [CF-28](#cf-28-runtime-id-collision-in-node_to_elem-mapping) | Handle empty runtime ID collisions with a composite key or fallback strategy. | Small |
| **R-23** | [CF-11](#cf-11-double-com-round-trip-for-elementget_name) | Cache `element.get_name()` in a local variable in `get_element()`. | Trivial |
| **R-24** | [CF-20](#cf-20-unnecessary-string-cloning-in-element-getters) | Change `Element` getters to return `&str` instead of `String`. | Small |
| **R-25** | [CF-22](#cf-22-full-saveuielement-clone-for-set-membership-test) | Change `remove_in_place` to build a `HashSet<Vec<i32>>` of runtime IDs instead of cloning full elements. | Trivial |
| **R-26** | [CF-21](#cf-21-treestate-cloned-every-frame-in-uiexplore) | Refactor `update` to mutate `TreeState` in place or use `Option::take()`. | Small |
| **R-27** | [CF-19](#cf-19-printfmt-macro-not-atomic-under-concurrency) | Rewrite the macro to use a single `println!` call or lock stdout for the duration of both writes. | Trivial |
| **R-28** | [CF-25](#cf-25-repository-hygiene-issues) | Run `git rm --cached .tokensave/tokensave.db`. Evaluate `.pyo3venv` and nested `Cargo.lock`. Update `.gitignore`. | Trivial |
| **R-29** | [CF-16](#cf-16-uiexplore-temp-file-signaling-is-globally-named-and-cross-instance-unsafe) | Replace fixed signal filename with per-process path containing PID and unique token. | Small |
| **R-30** | [CF-18](#cf-18-python-package-behavior-is-not-validated-in-ci) | Add CI job for maturin build + pytest. Split Python tests into CI-safe and desktop-dependent groups with pytest markers. | Medium |

---

## 4. Implementation Sequence

### Phase 1 — Establish CI Baseline ✅ COMPLETED

Restore a green CI pipeline before making any functional changes. All subsequent phases depend on reliable CI.

*Completed 2026-06-23 — commit `635f4e8`*

| Step | Status | Action | Finding | Effort |
|------|--------|--------|---------|--------|
| 1 | ✅ | **R-01** Run `cargo fmt --all` and commit | CF-05 | Trivial |
| 2 | ✅ | **R-02** Mark `test_capture_monitor` and `test_capture_window` with `#[ignore]` (require interactive desktop) | CF-05 | Small |

### Phase 2 — Safety-Critical Fixes ✅

Address memory safety, infinite loops, and hang/crash risks. Ship before any release.

| Step | | Action | Finding | Effort |
|------|--|--------|---------|--------|
| 3 | ✅ | **R-03** Fix row-pitch handling in video recorder | CF-01 | Medium |
| 4 | ✅ | **R-04** Add cycle guards to tree-walker sibling loops | CF-02 | Medium |
| 5 | ✅ | **R-05** Add cancellation flag + bounded refresh for spawned threads | CF-03 | Medium |
| 6 | ✅ | **R-06** Fix tombstone handling in UITreeMap | CF-04 | Small |

### Phase 3 — Correctness & Error Handling ✅

Fix data corruption risks, XPath generation bugs, and replace panics with proper error propagation.

| Step | | Action | Finding | Effort |
|------|--|--------|---------|--------|
| 7 | ✅ | **R-07** Add XPath string literal encoder for quoted names | CF-06 | Medium |
| 8 | ✅ | **R-08** Change `get_ui_automation_instance` to return `Result` | CF-12 | Small |
| 9 | ✅ | **R-09** Replace `.expect()` with error propagation in `create_hook` | CF-13 | Trivial |
| 10 | ✅ | **R-10** Propagate XML write errors instead of discarding | CF-14 | Small |
| 11 | ✅ | **R-11** Fix logger initialization: repeated calls + panic risk | CF-15 | Small |

### Phase 4 — Structural Consolidation

Reduce duplication and complete the Consolidation branch's goals. Shared infrastructure first, then consumers.

| Step | Action | Finding | Effort |
|------|--------|---------|--------|
| 12 | ✅ | **R-12** Centralise workspace dependencies | CF-26 | Small |
| 13 | ✅ | **R-13** Consolidate `rectangle.rs` into `bromium-common` | CF-08 | Medium |
| 14 | ✅ | **R-14** Add RAII `GdiGuard` for GDI resources | CF-10 | Small |
| 15 | ✅ | **R-15** Extract shared walker helpers in `uitree` | CF-24 | Small |
| 16 | ✅ | **R-16** Extract `with_ui_element` helper in `bromium::Element` | CF-07 | Small |
| 17 | ✅ | **R-17** Remove dead code across crates | CF-23 | Trivial |

### Phase 5 — API & Usability Improvements

Improve the public API surface and developer experience.

| Step | Action | Finding | Effort |
|------|--------|---------|--------|
| 18 | **R-18** Register `ScreenContext`/`ScreenInfo` in PyO3 module | CF-09 | Trivial |
| 19 | **R-19** Make tree-construction timeout configurable | CF-29 | Small |
| 20 | **R-20** Generate unique screenshot filenames | CF-17 | Small |
| 21 | **R-21** Rename `XMLDomWriter::to_string` | CF-27 | Trivial |
| 22 | **R-22** Handle runtime ID collisions in `node_to_elem` | CF-28 | Small |
| 23 | **R-29** Replace global signal filename with per-process path | CF-16 | Small |

### Phase 6 — Performance & Polish

Low-risk optimisations, cleanup items, and CI hardening.

| Step | Action | Finding | Effort |
|------|--------|---------|--------|
| 24 | **R-23** Cache `element.get_name()` in `get_element` | CF-11 | Trivial |
| 25 | **R-24** Return `&str` from `Element` getters | CF-20 | Small |
| 26 | **R-25** Use runtime ID set instead of full-element clone | CF-22 | Trivial |
| 27 | **R-26** Eliminate per-frame `TreeState` clone | CF-21 | Small |
| 28 | **R-27** Make `printfmt!` macro output atomic | CF-19 | Trivial |
| 29 | **R-28** Repository hygiene cleanup | CF-25 | Trivial |
| 30 | **R-30** Add Python package CI with maturin + pytest | CF-18 | Medium |

---

### Summary

| Severity | Count | Sources |
|----------|-------|---------|
| Critical | 2 | CC only |
| High | 4 | 1 corroborated (CC+CX), 2 CX only, 1 CC only |
| Medium | 12 | 6 CC only, 4 CX only, 2 CC only |
| Low | 11 | 10 CC only, 1 merged (CC+CX) |
| **Total** | **29** | 21 CC unique, 6 CX unique, 2 overlapping |

**Recommended first milestone:** Complete Phases 1–3 (steps 1–11). This produces a CI-clean baseline, eliminates all critical and high-severity risks, fixes the most likely user-facing locator bug, and hardens error handling before larger structural refactoring.

---

*End of combined report.*
