# Bromium Project Audit Findings Report

**Date:** 2025-06-22
**Branch:** `Consolidation`
**Auditor:** Claude Code (automated static analysis)
**Scope:** Full workspace — 7 crates, all Rust source files

---

## Table of Contents

- [1. Findings](#1-findings)
  - [F-01 Memory Safety: Video Recorder Row Pitch Mismatch](#f-01-memory-safety-video-recorder-row-pitch-mismatch)
  - [F-02 Infinite Loop Risk in UI Tree Sibling Walkers](#f-02-infinite-loop-risk-in-ui-tree-sibling-walkers)
  - [F-03 Orphaned Thread Accumulation on Tree Construction Timeout](#f-03-orphaned-thread-accumulation-on-tree-construction-timeout)
  - [F-04 Tombstone Corruption in UITreeMap After Node Removal](#f-04-tombstone-corruption-in-uitreemap-after-node-removal)
  - [F-05 Duplicated Element Action Boilerplate in bromium::Element](#f-05-duplicated-element-action-boilerplate-in-bromiumelement)
  - [F-06 rectangle.rs Duplicated Across Crates With Divergent Behaviour](#f-06-rectanglers-duplicated-across-crates-with-divergent-behaviour)
  - [F-07 ScreenContext and ScreenInfo Not Registered in PyO3 Module](#f-07-screencontext-and-screeninfo-not-registered-in-pyo3-module)
  - [F-08 GDI Resource Cleanup Without RAII Guards](#f-08-gdi-resource-cleanup-without-raii-guards)
  - [F-09 Double COM Round-Trip for element.get_name()](#f-09-double-com-round-trip-for-elementget_name)
  - [F-10 Error Context Lost in get_ui_automation_instance](#f-10-error-context-lost-in-get_ui_automation_instance)
  - [F-11 Panic on Hook Installation Failure](#f-11-panic-on-hook-installation-failure)
  - [F-12 Silent Discarding of XML Write Errors](#f-12-silent-discarding-of-xml-write-errors)
  - [F-13 printfmt! Macro Not Atomic Under Concurrency](#f-13-printfmt-macro-not-atomic-under-concurrency)
  - [F-14 Unnecessary String Cloning in Element Getters](#f-14-unnecessary-string-cloning-in-element-getters)
  - [F-15 TreeState Cloned Every Frame in uiexplore](#f-15-treestate-cloned-every-frame-in-uiexplore)
  - [F-16 Full SaveUIElement Clone for Set Membership Test](#f-16-full-saveuielement-clone-for-set-membership-test)
  - [F-17 Dead Code Across Multiple Crates](#f-17-dead-code-across-multiple-crates)
  - [F-18 Duplicated Root-Setup and Sorting Logic in Tree Walkers](#f-18-duplicated-root-setup-and-sorting-logic-in-tree-walkers)
  - [F-19 Workspace Dependencies Not Centralised](#f-19-workspace-dependencies-not-centralised)
  - [F-20 XMLDomWriter::to_string Shadows the ToString Trait](#f-20-xmldomwriterto_string-shadows-the-tostring-trait)
  - [F-21 Runtime ID Collision in node_to_elem Mapping](#f-21-runtime-id-collision-in-node_to_elem-mapping)
  - [F-22 tokensave.db Tracked in Git](#f-22-tokensavedb-tracked-in-git)
  - [F-23 Hardcoded 120-Second Timeout Not Configurable](#f-23-hardcoded-120-second-timeout-not-configurable)
- [2. Remediation Actions](#2-remediation-actions)
- [3. Implementation Sequence](#3-implementation-sequence)

---

## 1. Findings

### F-01 Memory Safety: Video Recorder Row Pitch Mismatch

| Attribute   | Value |
|-------------|-------|
| **Severity**  | Critical |
| **Crate**     | `screen-capture` |
| **File**      | `src/mswindows/impl_video_recorder.rs` |
| **Category**  | Memory Safety |

**Description:**
`slice::from_raw_parts` constructs a slice using `Height * RowPitch` as the byte length. The subsequent `bgra_to_rgba` conversion assumes tightly-packed pixel data (`Width * 4` bytes per row). GPU textures frequently have `RowPitch > Width * 4` due to alignment padding. When this occurs, the RGBA conversion reads padding bytes as pixel data, producing corrupted output. In the worst case, the slice may extend beyond the mapped allocation, triggering undefined behaviour.

**Evidence:**
`impl_video_recorder.rs` — `texture_to_frame` constructs the slice from `mapped.pData` with a length derived from `RowPitch`, but downstream processing ignores the pitch.

---

### F-02 Infinite Loop Risk in UI Tree Sibling Walkers

| Attribute   | Value |
|-------------|-------|
| **Severity**  | Critical |
| **Crate**     | `uitree` |
| **Files**     | `src/uiexplore.rs`, `src/uiexplore_iter.rs`, `src/uiexplore_xml.rs` |
| **Category**  | Correctness / Robustness |

**Description:**
All three tree-walker implementations use `while let Ok(sibling) = walker.get_next_sibling(&next)` to iterate horizontally. There is no visited-set guard and no sibling counter limit. The `max_depth` parameter only bounds vertical depth. If the COM UIAutomation API returns a cycle (possible with crashed or hung processes), the loop runs indefinitely, hanging the calling thread.

**Evidence:**
`uiexplore.rs` ~line 132, `uiexplore_xml.rs` ~line 816 — unbounded sibling loops with no termination safeguard beyond the COM call returning `Err`.

---

### F-03 Orphaned Thread Accumulation on Tree Construction Timeout

| Attribute   | Value |
|-------------|-------|
| **Severity**  | High |
| **Crate**     | `bromium` |
| **File**      | `src/windriver.rs` |
| **Category**  | Resource Management |

**Description:**
Tree construction is dispatched via `thread::spawn` with a 120-second `recv_timeout`. If the receiver times out, the spawned thread continues running with no cancellation signal. Repeated refresh attempts (e.g., via a retry loop) can accumulate orphaned threads, each holding COM resources and consuming memory.

**Evidence:**
`windriver.rs` — `new()`, `refresh_ui_tree()`, `refresh_ui_tree_top_2()`, and the retry loop all spawn fire-and-forget threads.

---

### F-04 Tombstone Corruption in UITreeMap After Node Removal

| Attribute   | Value |
|-------------|-------|
| **Severity**  | High |
| **Crate**     | `uitree` |
| **File**      | `src/tree_map.rs` |
| **Category**  | Correctness |

**Description:**
`remove_node` replaces a removed node with a default-constructed placeholder but does not reclaim the arena slot or mark it as dead. Subsequent operations (`for_each`, `build_node_to_elem`, `debug_tree`) iterate over all arena slots including tombstones, potentially returning default/garbage data. The default node index `0` maps to the root element in `node_to_elem`, so tombstones silently alias the root.

**Evidence:**
`tree_map.rs` ~line 161 — `remove_node` writes a default `T` value; no `is_alive` flag or compaction logic exists.

---

### F-05 Duplicated Element Action Boilerplate in bromium::Element

| Attribute   | Value |
|-------------|-------|
| **Severity**  | Medium |
| **Crate**     | `bromium` |
| **File**      | `src/windriver.rs` |
| **Category**  | Maintainability / Code Duplication |

**Description:**
The pattern `if let Ok(e) = convert_to_ui_element(self) { match e.action() { ... } } else { Err(ElementNotFoundError) }` is repeated verbatim across seven `Element` methods: `send_click`, `send_double_click`, `send_right_click`, `hold_click`, `send_keys`, `hold_send_keys`, and `show_context_menu`. This accounts for approximately 300 lines of duplicated code, increasing maintenance burden and bug surface.

**Evidence:**
`windriver.rs` lines ~223-522 — seven near-identical method bodies.

---

### F-06 rectangle.rs Duplicated Across Crates With Divergent Behaviour

| Attribute   | Value |
|-------------|-------|
| **Severity**  | Medium |
| **Crates**    | `bromium`, `uiexplore` |
| **Files**     | `bromium/src/rectangle.rs`, `uiexplore/src/rectangle.rs` |
| **Category**  | Code Duplication / Correctness |

**Description:**
`draw_frame`, `clear_frame`, and `is_inside_rectangle` are near-identical across both files. However, `get_point_bounding_rect` differs: `uiexplore` returns the **first** match (z-order dependent), while `bromium` returns the **smallest-area** match. The smallest-area algorithm is more correct for nested elements. This divergence means the two consumers silently apply different hit-test semantics.

**Evidence:**
Comparing `bromium/src/rectangle.rs` and `uiexplore/src/rectangle.rs` — shared functions are copy-pasted, with the critical `get_point_bounding_rect` diverging.

---

### F-07 ScreenContext and ScreenInfo Not Registered in PyO3 Module

| Attribute   | Value |
|-------------|-------|
| **Severity**  | Medium |
| **Crate**     | `bromium` |
| **File**      | `src/lib.rs` |
| **Category**  | API Completeness |

**Description:**
`ScreenContext` and `ScreenInfo` are `#[pyclass]` types returned by `WinDriver.get_screen_context()`, but they are not registered in the `#[pymodule]` function. Python code can use returned instances, but `isinstance()` checks, type annotations, and direct construction from Python will fail.

**Evidence:**
`lib.rs` lines 19-51 — `ScreenContext` and `ScreenInfo` are absent from the `m.add_class::<...>()` registrations.

---

### F-08 GDI Resource Cleanup Without RAII Guards

| Attribute   | Value |
|-------------|-------|
| **Severity**  | Medium |
| **Crate**     | `bromium` |
| **File**      | `src/rectangle.rs` |
| **Category**  | Resource Safety |

**Description:**
`draw_frame` manually calls `ReleaseDC`/`DeleteObject` at every error branch and at the end of the function. This pattern is fragile — any future edit that adds an early return risks leaking a GDI handle. A RAII wrapper with a `Drop` implementation would make cleanup automatic and robust.

**Evidence:**
`rectangle.rs` lines ~43-102 — repeated manual cleanup calls at each error path.

---

### F-09 Double COM Round-Trip for element.get_name()

| Attribute   | Value |
|-------------|-------|
| **Severity**  | Medium |
| **Crate**     | `uitree` |
| **File**      | `src/uiexplore_xml.rs` |
| **Category**  | Performance |

**Description:**
In `get_element()`, `element.get_name()` is called twice — each call is a COM interprocess round-trip. The result should be cached in a local variable and reused, halving the COM overhead per element during tree construction.

**Evidence:**
`uiexplore_xml.rs` lines ~733-741 — two separate calls to `element.get_name()`.

---

### F-10 Error Context Lost in get_ui_automation_instance

| Attribute   | Value |
|-------------|-------|
| **Severity**  | Medium |
| **Crate**     | `bromium-common` |
| **File**      | `src/uia.rs` |
| **Category**  | Error Handling |

**Description:**
`get_ui_automation_instance` returns `Option<UIAutomation>` instead of `Result<UIAutomation, Error>`. Callers lose all context about *why* the instance creation failed, making diagnosis difficult. COM initialisation failures have multiple root causes (e.g., wrong apartment model, missing COM registration) that are all collapsed into `None`.

**Evidence:**
`uia.rs` — return type is `Option` rather than `Result`.

---

### F-11 Panic on Hook Installation Failure

| Attribute   | Value |
|-------------|-------|
| **Severity**  | Medium |
| **Crate**     | `winevent-monitor` |
| **File**      | `src/winevent.rs` |
| **Category**  | Error Handling |

**Description:**
`create_hook` calls `.expect()` on the hook installation result, causing a panic if the Win32 `SetWinEventHook` call fails. This should propagate the error to the caller so the application can handle it gracefully rather than crashing.

**Evidence:**
`winevent.rs` ~line 146 — `.expect()` on a fallible Win32 API call.

---

### F-12 Silent Discarding of XML Write Errors

| Attribute   | Value |
|-------------|-------|
| **Severity**  | Medium |
| **Crate**     | `uitree` |
| **File**      | `src/uiexplore_xml.rs` |
| **Category**  | Error Handling |

**Description:**
Multiple calls to `xml_writer.write_event(...)` have their `Result` silently discarded with `let _ = ...`. If an XML write fails mid-tree, the output will be silently truncated or malformed, with no indication to the caller.

**Evidence:**
`uiexplore_xml.rs` ~line 791 — `let _ = xml_writer.write_event(...)`.

---

### F-13 printfmt! Macro Not Atomic Under Concurrency

| Attribute   | Value |
|-------------|-------|
| **Severity**  | Low |
| **Crate**     | `bromium-common` |
| **File**      | `src/macros.rs` |
| **Category**  | Correctness |

**Description:**
The `printfmt!` macro issues two separate calls (`print!` for the timestamp prefix, then `println!` for the message body). Under concurrent use, output from different threads can interleave between the two calls, producing garbled console output.

**Evidence:**
`macros.rs` — macro body contains separate `print!` and `println!` invocations.

---

### F-14 Unnecessary String Cloning in Element Getters

| Attribute   | Value |
|-------------|-------|
| **Severity**  | Low |
| **Crate**     | `bromium` |
| **File**      | `src/windriver.rs` |
| **Category**  | Performance |

**Description:**
`Element` property getters (`name()`, `xpath()`, `control_type()`, `runtime_id()`) clone their `String`/`Vec<i32>` fields on every access. PyO3 supports returning `&str` from getters, and `screen_context.rs` already demonstrates this pattern. Cloning on every property access creates unnecessary allocations in hot paths.

**Evidence:**
`windriver.rs` lines ~183-209 — `.clone()` in each getter. Contrast with `screen_context.rs` lines ~99, 103 which return `&str`.

---

### F-15 TreeState Cloned Every Frame in uiexplore

| Attribute   | Value |
|-------------|-------|
| **Severity**  | Low |
| **Crate**     | `uiexplore` |
| **File**      | `src/app_ui.rs` |
| **Category**  | Performance |

**Description:**
In the egui `update` method, `TreeState` is cloned on every frame repaint rather than mutated in place or swapped via `Option::take()`. This creates unnecessary allocations on every UI frame.

**Evidence:**
`app_ui.rs` lines ~1041-1046 — `TreeState` clone in the render loop.

---

### F-16 Full SaveUIElement Clone for Set Membership Test

| Attribute   | Value |
|-------------|-------|
| **Severity**  | Low |
| **Crate**     | `uitree` |
| **File**      | `src/uiexplore_xml.rs` |
| **Category**  | Performance |

**Description:**
`remove_in_place` clones the entire `check` slice into a `HashSet<SaveUIElement>`, including all `String` fields, just to test set membership. A `HashSet<Vec<i32>>` of runtime IDs would achieve the same membership test without cloning any strings.

**Evidence:**
`uiexplore_xml.rs` ~line 328 — full clone of `SaveUIElement` vector into `HashSet`.

---

### F-17 Dead Code Across Multiple Crates

| Attribute   | Value |
|-------------|-------|
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

### F-18 Duplicated Root-Setup and Sorting Logic in Tree Walkers

| Attribute   | Value |
|-------------|-------|
| **Severity**  | Low |
| **Crate**     | `uitree` |
| **Files**     | `src/uiexplore.rs`, `src/uiexplore_iter.rs`, `src/uiexplore_xml.rs` |
| **Category**  | Code Duplication |

**Description:**
The root-element setup (create `SaveUIElement`, format `runtime_id`, build item string, create `UITreeMap`, push to vector) and the child-sorting lambda are copy-pasted across all three tree-walker modules — approximately 80 lines of identical code.

**Evidence:**
Compare the opening ~20 lines of the main walk function in each of the three modules — they are structurally identical.

---

### F-19 Workspace Dependencies Not Centralised

| Attribute   | Value |
|-------------|-------|
| **Severity**  | Low |
| **Crates**    | Multiple |
| **Files**     | Various `Cargo.toml` |
| **Category**  | Build Configuration |

**Description:**
`thiserror = "2.0"` is independently declared in `bromium`, `screen-capture`, `uitree`, and `xmlutil`. `quick-xml = "0.38.0"` is independently declared in `uitree` and `xmlutil`. `screen-capture` declares its own `windows` version outside `workspace = true`. These should use `[workspace.dependencies]` to prevent version drift.

---

### F-20 XMLDomWriter::to_string Shadows the ToString Trait

| Attribute   | Value |
|-------------|-------|
| **Severity**  | Low |
| **Crate**     | `xmlutil` |
| **File**      | `src/xml_dom_manager.rs` |
| **Category**  | API Design |

**Description:**
`XMLDomWriter` has a method named `to_string()` that shadows the `ToString` trait's `to_string()` method. This can confuse callers who expect standard trait semantics. The method should be renamed (e.g., `serialize()`).

**Evidence:**
`xml_dom_manager.rs` ~line 243 — `fn to_string(...)` defined on `XMLDomWriter`.

---

### F-21 Runtime ID Collision in node_to_elem Mapping

| Attribute   | Value |
|-------------|-------|
| **Severity**  | Low |
| **Crate**     | `uitree` |
| **File**      | `src/uiexplore_xml.rs` |
| **Category**  | Correctness |

**Description:**
`build_node_to_elem` uses runtime ID as a HashMap key. Some UI controls report empty runtime IDs. When two elements share the same (empty) runtime ID, the HashMap silently overwrites the earlier entry, causing one tree node to point at the wrong element.

**Evidence:**
`uiexplore_xml.rs` ~line 49 — HashMap insertion with no collision handling.

---

### F-22 tokensave.db Tracked in Git

| Attribute   | Value |
|-------------|-------|
| **Severity**  | Low |
| **File**      | `.tokensave/tokensave.db` |
| **Category**  | Repository Hygiene |

**Description:**
`.tokensave/tokensave.db` appears as modified in git status. Although `.gitignore` may now list it, the file was tracked before the ignore rule was added. It needs to be removed from tracking with `git rm --cached`.

---

### F-23 Hardcoded 120-Second Timeout Not Configurable

| Attribute   | Value |
|-------------|-------|
| **Severity**  | Low |
| **Crate**     | `bromium` |
| **File**      | `src/windriver.rs` |
| **Category**  | Usability |

**Description:**
Tree construction timeout is hardcoded to 120 seconds in multiple locations (`windriver.rs` lines ~652, 897, 1130, 1166). This is not configurable by the Python caller. If the UI tree is large, 120 seconds may be insufficient; if it hangs, the Python process blocks for 2 minutes with no way to interrupt.

**Evidence:**
`windriver.rs` — literal `120` (or `Duration::from_secs(120)`) appears in four places.

---

## 2. Remediation Actions

| ID | Finding | Action | Effort |
|----|---------|--------|--------|
| **R-01** | [F-01](#f-01-memory-safety-video-recorder-row-pitch-mismatch) | Rewrite `texture_to_frame` to iterate row-by-row, copying only `Width * 4` bytes per row and skipping the `RowPitch - Width * 4` padding. Apply the BGRA-to-RGBA conversion per-row on the tightly-packed output buffer. | Medium |
| **R-02** | [F-02](#f-02-infinite-loop-risk-in-ui-tree-sibling-walkers) | Add a `HashSet<Vec<i32>>` visited-set keyed on runtime ID to each sibling loop. Additionally, add a configurable `max_siblings` counter (default: 10,000) as a hard upper bound. Apply to all three walker modules. | Medium |
| **R-03** | [F-03](#f-03-orphaned-thread-accumulation-on-tree-construction-timeout) | Introduce a shared `Arc<AtomicBool>` cancellation flag. Pass it to the spawned tree-construction thread; check it periodically during the walk. Set it to `true` when the receiver times out. | Medium |
| **R-04** | [F-04](#f-04-tombstone-corruption-in-uitreemap-after-node-removal) | Add an `is_alive: bool` field to the arena node type. Set it to `false` in `remove_node`. Update `for_each`, `build_node_to_elem`, and `debug_tree` to skip dead nodes. | Small |
| **R-05** | [F-05](#f-05-duplicated-element-action-boilerplate-in-bromiumelement) | Extract a private helper method `fn with_ui_element<F, R>(&self, action: F) -> PyResult<R> where F: FnOnce(&UIElement) -> Result<R, Error>` that encapsulates the `convert_to_ui_element` + match + error mapping pattern. Rewrite all seven methods to call it. | Small |
| **R-06** | [F-06](#f-06-rectanglers-duplicated-across-crates-with-divergent-behaviour) | Move `draw_frame`, `clear_frame`, `is_inside_rectangle`, and `get_point_bounding_rect` (using the smallest-area algorithm) into `bromium-common`. Delete both crate-local `rectangle.rs` files and re-export from `bromium-common`. | Medium |
| **R-07** | [F-07](#f-07-screencontext-and-screeninfo-not-registered-in-pyo3-module) | Add `m.add_class::<ScreenContext>()?;` and `m.add_class::<ScreenInfo>()?;` to the `#[pymodule]` function in `lib.rs`. | Trivial |
| **R-08** | [F-08](#f-08-gdi-resource-cleanup-without-raii-guards) | Create a small `GdiGuard` struct that holds the DC and pen handles and implements `Drop` to call `ReleaseDC`/`DeleteObject`. Replace manual cleanup in `draw_frame` and `clear_frame`. | Small |
| **R-09** | [F-09](#f-09-double-com-round-trip-for-elementget_name) | Cache the result of `element.get_name()` in a local `let name = element.get_name();` at the top of `get_element()` and reference it in both usages. | Trivial |
| **R-10** | [F-10](#f-10-error-context-lost-in-get_ui_automation_instance) | Change return type from `Option<UIAutomation>` to `Result<UIAutomation, uiautomation::Error>` (or a wrapped error type). Update all call sites. | Small |
| **R-11** | [F-11](#f-11-panic-on-hook-installation-failure) | Replace `.expect()` with `?` or `.map_err(...)` to propagate the error. Change the function return type to `Result<..., Error>` if not already. | Trivial |
| **R-12** | [F-12](#f-12-silent-discarding-of-xml-write-errors) | Replace `let _ = xml_writer.write_event(...)` with `xml_writer.write_event(...)?` (propagate via `Result`), or collect errors into a `Vec` and report them at the end. | Small |
| **R-13** | [F-13](#f-13-printfmt-macro-not-atomic-under-concurrency) | Rewrite the macro to use a single `println!("{} {}", timestamp, format_args!(...))` call, or lock `stdout` explicitly with `io::stdout().lock()` for the duration of both writes. | Trivial |
| **R-14** | [F-14](#f-14-unnecessary-string-cloning-in-element-getters) | Change `Element` getters to return `&str` instead of `String`. PyO3's `#[getter]` supports `&str`. For `runtime_id`, return a borrowed slice `&[i32]` or keep clone if PyO3 requires ownership. | Small |
| **R-15** | [F-15](#f-15-treestate-cloned-every-frame-in-uiexplore) | Refactor the `update` method to take a mutable reference to `TreeState` or use `Option::take()` to swap the state out, process it, and swap it back, avoiding per-frame clones. | Small |
| **R-16** | [F-16](#f-16-full-saveuielement-clone-for-set-membership-test) | Change `remove_in_place` to build a `HashSet<String>` of formatted runtime IDs (or `HashSet<Vec<i32>>`) from the `check` slice, then test membership using only the runtime ID field. | Trivial |
| **R-17** | [F-17](#f-17-dead-code-across-multiple-crates) | Remove `tree_needs_update` field from `WinDriver`, `last_hwnd` from `WinEventMonitor`, the no-op `get_element()` from `SaveUIElement`. Either wire up or remove `draw_frame`/`clear_frame` from `bromium/rectangle.rs` (deferred to R-06 if consolidation includes them). | Trivial |
| **R-18** | [F-18](#f-18-duplicated-root-setup-and-sorting-logic-in-tree-walkers) | Extract shared helper functions: `fn create_root_setup(element: &UIElement) -> (SaveUIElement, String, UITreeMap)` and `fn sort_children(children: &mut Vec<UIElement>)`. Place them in a shared module within `uitree` (e.g., `walker_common.rs`). Call from all three walker modules. | Small |
| **R-19** | [F-19](#f-19-workspace-dependencies-not-centralised) | Add `thiserror`, `quick-xml`, and `windows` to `[workspace.dependencies]` in the root `Cargo.toml`. Update each crate's `Cargo.toml` to use `thiserror.workspace = true`, etc. | Small |
| **R-20** | [F-20](#f-20-xmldomwriterto_string-shadows-the-tostring-trait) | Rename `XMLDomWriter::to_string()` to `XMLDomWriter::serialize()` or `XMLDomWriter::to_xml_string()`. Update all call sites. | Trivial |
| **R-21** | [F-21](#f-21-runtime-id-collision-in-node_to_elem-mapping) | When inserting into the `node_to_elem` HashMap, check for collisions on empty runtime IDs. For elements with empty runtime IDs, use a composite key (e.g., `(runtime_id, tree_node_index)`) or skip the index entry and fall back to linear search. | Small |
| **R-22** | [F-22](#f-22-tokensavedb-tracked-in-git) | Run `git rm --cached .tokensave/tokensave.db` and commit. Verify the `.gitignore` entry covers `.tokensave/`. | Trivial |
| **R-23** | [F-23](#f-23-hardcoded-120-second-timeout-not-configurable) | Extract the timeout value into a constant or a field on `WinDriver` that can be set from Python (e.g., `WinDriver::new(timeout_secs: Option<u64>)` defaulting to 120). Consolidate the four literal occurrences. | Small |

---

## 3. Implementation Sequence

The remediations are ordered to maximise safety impact first, then resolve structural issues that unblock later work, and finally address polish items.

### Phase 1 — Safety-Critical Fixes

These address memory safety and hang/crash risks. Ship before any release.

| Step | Action | Rationale |
|------|--------|-----------|
| 1 | **R-01** Fix row-pitch handling in video recorder | Memory safety — potential undefined behaviour. Standalone fix, no dependencies. |
| 2 | **R-02** Add cycle guards to tree-walker sibling loops | Eliminates infinite-loop hang risk. Standalone fix across three files. |
| 3 | **R-03** Add cancellation flag to spawned tree-construction threads | Prevents orphaned thread accumulation. Builds on R-02 (the cancellation flag can also be checked by the walker). |
| 4 | **R-04** Fix tombstone handling in UITreeMap | Prevents silent data corruption after subtree replacement. |

### Phase 2 — Error Handling Hardening

These replace panics and silent failures with proper error propagation.

| Step | Action | Rationale |
|------|--------|-----------|
| 5 | **R-10** Change `get_ui_automation_instance` to return `Result` | Foundation change — affects call sites in multiple crates. Do early so dependent changes can adapt. |
| 6 | **R-11** Replace `.expect()` with error propagation in `create_hook` | Trivial fix, prevents panic. |
| 7 | **R-12** Propagate XML write errors instead of discarding | Prevents silent data corruption in tree XML output. |

### Phase 3 — Structural Consolidation

These reduce duplication and complete the consolidation branch's goals. Order matters — shared infrastructure first, then consumers.

| Step | Action | Rationale |
|------|--------|-----------|
| 8 | **R-19** Centralise workspace dependencies | Foundational build config — do before touching individual `Cargo.toml` files. |
| 9 | **R-06** Consolidate `rectangle.rs` into `bromium-common` | Eliminates the largest cross-crate duplication; resolves the behaviour divergence. |
| 10 | **R-08** Add RAII `GdiGuard` for GDI resources | Natural to do during R-06 as `rectangle.rs` is being rewritten. |
| 11 | **R-18** Extract shared walker helpers in `uitree` | Reduces duplication across three walker modules. |
| 12 | **R-05** Extract `with_ui_element` helper in `bromium::Element` | Largest single-crate deduplication (~300 lines). |
| 13 | **R-17** Remove dead code across crates | Clean sweep after structural changes; avoids removing code that R-06 might relocate. |

### Phase 4 — API & Usability Improvements

These improve the public API surface and developer experience.

| Step | Action | Rationale |
|------|--------|-----------|
| 14 | **R-07** Register `ScreenContext`/`ScreenInfo` in PyO3 module | Trivial fix, completes the Python API. |
| 15 | **R-23** Make tree-construction timeout configurable | Usability improvement for Python consumers. |
| 16 | **R-20** Rename `XMLDomWriter::to_string` | API clarity fix. |
| 17 | **R-21** Handle runtime ID collisions in `node_to_elem` | Correctness edge case for controls with empty runtime IDs. |

### Phase 5 — Performance & Polish

These are low-risk optimisations and cleanup items.

| Step | Action | Rationale |
|------|--------|-----------|
| 18 | **R-09** Cache `element.get_name()` in `get_element` | Trivial performance win — halves COM calls per element. |
| 19 | **R-14** Return `&str` from `Element` getters | Eliminates per-access allocations. |
| 20 | **R-16** Use runtime ID set instead of full-element clone | Eliminates unnecessary allocations in `remove_in_place`. |
| 21 | **R-15** Eliminate per-frame `TreeState` clone in uiexplore | UI performance improvement. |
| 22 | **R-13** Make `printfmt!` macro output atomic | Prevents interleaved output under concurrency. |
| 23 | **R-22** Remove `tokensave.db` from git tracking | Repository hygiene — do last to avoid merge noise. |

---

*End of report.*
