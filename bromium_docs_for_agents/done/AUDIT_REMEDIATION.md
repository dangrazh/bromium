# Audit Remediation Plan

**Audit date:** 2026-06-20
**Scope:** Reliability, correctness, performance across all 7 workspace crates
**Codebase:** ~10,500 LOC Rust

---

## Findings Index

| ID | Severity | Title | Phase |
|----|----------|-------|-------|
| F-01 | 🔴 Critical | Unguarded `.unwrap()` on fallible COM calls in `uitree` | Phase 1 |
| F-02 | 🔴 Critical | `WinDriver::new()` blocks the Python GIL indefinitely | Phase 1 |
| F-03 | 🔴 Critical | Thread leak in `execute_with_timeout` | Phase 1 |
| F-04 | 🔴 Critical | Panic on channel send after receiver timeout | Phase 1 |
| F-05 | 🔴 Critical | `unsafe impl Send + Sync` with incomplete safety invariant | Phase 1 |
| F-06 | 🟠 Bug | Double-sort destroys compound ordering — wrong element hit | Phase 2 |
| F-07 | 🟠 Bug | First WinEvent is silently dropped | Phase 2 |
| F-08 | 🟠 Bug | Wrong error message in `send_text` | Phase 2 |
| F-09 | 🟠 Bug | `reload()` does not reload | Phase 2 |
| F-10 | 🟠 Bug | `name_to_index` HashMap silently loses entries | Phase 2 |
| F-11 | 🟠 Bug | `dbg!()` left in production code | Phase 2 |
| F-12 | 🟠 Bug | Deprecated `env::home_dir()` — incorrect on Windows | Phase 2 |
| F-13 | 🟡 Perf | Full XML string cloned on every XPath query | Phase 3 |
| F-14 | 🟡 Perf | `get_ui_automation_ui_element()` does depth-99 search per interaction | Phase 3 |
| F-15 | 🟡 Perf | String cloning in recursive tree traversal | Phase 3 |
| F-16 | 🟡 Perf | Runtime ID formatting duplicated ~10 times | Phase 3 |
| F-17 | 🟡 Perf | `child_elements` vector cloned unnecessarily | Phase 3 |
| F-18 | 🔵 Quality | Three near-identical `SaveUIElement` types | Phase 4 |
| F-19 | 🔵 Quality | Three near-identical `UIElementInTree`, `UITree`, `match_original_format` | Phase 4 |
| F-20 | 🔵 Quality | Dead `InstanceLogger` module | Phase 4 |
| F-21 | 🔵 Quality | Blanket `#[allow(dead_code)]` hides real issues | Phase 4 |
| F-22 | 🔵 Quality | Return `&Vec<T>` instead of `&[T]` in public APIs | Phase 4 |
| F-23 | 🔵 Quality | `get_point_bounding_rect` takes `&Vec<T>` instead of `&[T]` | Phase 4 |

---

## Phase 1 — Critical Reliability (crash/hang prevention)

**Goal:** Eliminate all paths that can panic or hang in production.
**Validation:** `cargo build` passes, `cargo test` passes, `cargo clippy` shows no new warnings. Manual smoke test: create a `WinDriver` from Python, call `get_element_by_xpath` on a stale xpath, confirm no crash.

---

### R-01 — Replace `.unwrap()` with error propagation on COM calls in `uitree`

**Linked finding:** F-01

Every `.unwrap()` on a `UIElement` method (`get_name()`, `get_classname()`, `get_control_type()`, `get_runtime_id()`, etc.) must be replaced with proper error propagation. The functions that call these should return `Result` and propagate errors upward with `?`. Where the caller is a fire-and-forget formatting operation (e.g., building a display string), use `.unwrap_or_default()` only as a last resort.

**Files and locations:**

| File | Lines | Call(s) |
|------|-------|---------|
| `crates/uitree/src/uiexplore.rs` | 125 | `get_runtime_id().unwrap()` in `get_xpath_raw_for_element` |
| `crates/uitree/src/uiexplore.rs` | 336–340 | `get_name().unwrap()`, `get_localized_control_type().unwrap()`, `get_classname().unwrap()`, `get_framework_id().unwrap()` in `get_element` |
| `crates/uitree/src/uiexplore_xml.rs` | 457–500 | `get_ui_automation_instance().unwrap()`, `get_control_view_walker().unwrap()`, `get_root_element().unwrap()`, `get_name().unwrap()`, `get_control_type().unwrap()`, `get_classname().unwrap()`, `get_framework_id().unwrap()`, `get_root_mut().unwrap()` in `get_all_elements_xml` |
| `crates/uitree/src/uiexplore_xml.rs` | 541 | `xml_writer.to_string().unwrap()` in `get_all_elements_xml` |
| `crates/uitree/src/uiexplore_xml.rs` | 590–622 | Same pattern in `get_all_elements_par_xml` |
| `crates/uitree/src/uiexplore_xml.rs` | 666 | `xml_writer.to_string().unwrap()` in `get_all_elements_par_xml` |
| `crates/uitree/src/uiexplore_xml.rs` | 756 | `rx_par.recv().unwrap()` in `get_all_elements_par_xml` |
| `crates/uitree/src/uiexplore_xml.rs` | 763 | `handle.join().unwrap()` in `get_all_elements_par_xml` |
| `crates/uitree/src/uiexplore_xml.rs` | 889 | `element.get_control_type().unwrap()` in `get_element` |
| `crates/uitree/src/uiexplore_iter.rs` | 125 | `get_runtime_id().unwrap()` in `get_xpath_raw_for_element` |
| `crates/uitree/src/uiexplore_iter.rs` | 238–259 | Same pattern as `uiexplore.rs` in `get_all_elements_iterative` |
| `crates/uitree/src/uiexplore_iter.rs` | 305 | `tx.send(ui_tree).unwrap()` |
| `crates/uitree/src/save_ui_element.rs` | 106 | `get_ui_automation_instance().unwrap()` in `get_ui_automation_ui_element` |

**Approach:**

1. Define an error type in `uitree` (or extend the existing `TreeMapError`) to cover COM failures and channel errors:
   ```rust
   #[derive(Debug, thiserror::Error)]
   pub enum UITreeError {
       #[error("UI Automation error: {0}")]
       UIAutomation(#[from] uiautomation::Error),
       #[error("Channel receive error: {0}")]
       ChannelRecv(String),
       #[error("XML serialization error: {0}")]
       XmlError(String),
       #[error("UIAutomation instance creation failed")]
       NoUIAutomation,
   }
   ```
2. Change `get_all_elements_xml`, `get_all_elements_par_xml`, `get_all_elements`, and `get_all_elements_iterative` to return `Result<UITree, UITreeError>` instead of sending via channel (or send a `Result` through the channel).
3. For display-string formatting inside `get_element()` where the result is a human-readable label (the `item` variable), use `.unwrap_or_default()` — these are non-critical.
4. For `get_runtime_id().unwrap()` in `get_xpath_raw_for_element` (lines 125 in `uiexplore.rs` and `uiexplore_iter.rs`), propagate the error — this is XPath generation and a bad runtime ID makes the XPath wrong.
5. For `save_ui_element.rs:106` — `get_ui_automation_instance().unwrap()` — return `None` early (the function already returns `Option<UIElement>`).

---

### R-02 — Add timeout to `WinDriver::new()` channel receive

**Linked finding:** F-02

**File:** `crates/bromium/src/windriver.rs`, lines 639–657

**Current code:**
```rust
let ui_tree: UITreeXML = match rx.recv() { // blocks forever
```

**Action:** Replace `rx.recv()` with `rx.recv_timeout(Duration::from_secs(120))` and map the error to a `PyRuntimeError`, consistent with how `refresh_ui_tree` already handles this at line 1022. Additionally, wrap the blocking portion in `py.allow_threads(|| ...)` to release the GIL during the wait.

---

### R-03 — Fix thread leak in `execute_with_timeout`

**Linked finding:** F-03

**File:** `crates/bromium-common/src/timeout.rs`, lines 5–18

**Action:** Store the `JoinHandle` and document that the thread is not cancelled on timeout — only the result is discarded. Add a doc comment warning callers that the closure continues to run after timeout. If cancellation is needed in the future, consider using a `std::sync::atomic::AtomicBool` flag that the closure checks periodically.

Minimal fix (documentation + handle retention):
```rust
/// Runs `f` on a background thread and waits up to `timeout_ms` for its result.
///
/// **Important:** If the timeout fires, the background thread continues running
/// to completion. The closure should avoid holding long-lived resources (locks,
/// file handles) if timeout is a realistic scenario.
pub fn execute_with_timeout<T, F>(timeout_ms: u64, f: F) -> Option<T>
where ...
```

---

### R-04 — Replace panicking `tx.send().unwrap()` with graceful handling

**Linked finding:** F-04

**Files:**
- `crates/uitree/src/uiexplore.rs`, line 307
- `crates/uitree/src/uiexplore_iter.rs`, line 305
- `crates/uitree/src/uiexplore_xml.rs`, line 756

**Action:** Replace `tx.send(ui_tree).unwrap()` with:
```rust
if let Err(e) = tx.send(ui_tree) {
    log::error!("Failed to send UI tree — receiver dropped: {}", e);
}
```

For `rx_par.recv().unwrap()` (line 756), use `recv_timeout` with error logging, consistent with R-02.

---

### R-05 — Tighten or remove `unsafe impl Send + Sync` for COM-backed `SaveUIElement`

**Linked finding:** F-05

**Files:**
- `crates/uitree/src/uiexplore.rs`, lines 213–214
- `crates/uitree/src/uiexplore_iter.rs`, lines 213–214

**Action:** These two `SaveUIElement` types hold raw `UIElement` (COM pointer). The safety comment claims MTA guarantees, but the codebase's `uia.rs` falls back to `UIAutomation::new_direct()` which bypasses COM initialization.

Options (choose one):
1. **Preferred:** Remove these types entirely as part of Phase 4 consolidation (F-18). The `save_ui_element.rs` version stores extracted properties (no COM pointer) and does not need `unsafe impl Send`.
2. **If keeping:** Assert MTA at construction time by calling `CoInitializeEx` with a check, and `panic!` if COM is not MTA. Document this requirement.

---

## Phase 2 — Correctness Bugs

**Goal:** Fix all known logic errors.
**Validation:** `cargo test` passes, `cargo clippy` clean. Write targeted unit tests for F-06 (sort ordering) and F-07 (event consumption).

---

### R-06 — Fix double-sort to use compound comparator

**Linked finding:** F-06

**Files and locations (5 pairs):**
- `crates/uitree/src/uiexplore.rs`, lines 288–297
- `crates/uitree/src/uiexplore_xml.rs`, lines 321–329, 545–554, 670–679
- `crates/uitree/src/uiexplore_iter.rs`, lines 286–295

**Action:** Replace every pair of sequential `sort_by` calls with a single compound sort:
```rust
ui_elements.sort_by(|a, b| {
    a.get_z_order().cmp(&b.get_z_order())
        .then(a.get_bounding_rect_size().cmp(&b.get_bounding_rect_size()))
});
```

This ensures that within the same z-order, elements are sorted by ascending bounding rect size (smallest/most specific first), which is what `get_point_bounding_rect` relies on.

**Test:** Create a vector of mock `UIElementInTree` with overlapping z-orders and different sizes. Assert that after sorting, a linear scan finds the smallest element first.

---

### R-07 — Fix first WinEvent being silently dropped

**Linked finding:** F-07

**File:** `crates/winevent-monitor/src/winevent.rs`, lines 64–106

**Current code:**
```rust
let mut rx_iter = self.rx_channel.try_iter();
if rx_iter.next().is_none() {   // ← consumes + discards first event
    return output;
}
for event_info in rx_iter { ... }
```

**Action:** Use `peekable()` or restructure:
```rust
let mut rx_iter = self.rx_channel.try_iter().peekable();
if rx_iter.peek().is_none() {
    return output;
}
for event_info in rx_iter { ... }
```

**Test:** Send exactly one event through the channel and verify `check_for_events()` returns it.

---

### R-08 — Fix wrong error message in `send_text`

**Linked finding:** F-08

**File:** `crates/bromium/src/windriver.rs`, line 381

**Action:** Change `"Invoke click failed"` to `"Set value failed"`.

---

### R-09 — Fix `reload()` to either refresh or rename

**Linked finding:** F-09

**File:** `crates/bromium/src/windriver.rs`, lines 715–718

**Action:** Either:
- Rename to `clone_driver()` to match its behavior, or
- Implement actual reloading by calling `self.refresh_ui_tree(self.window_title.clone())?; Ok(self.clone())`

Recommended: rename to `clone_driver()` and deprecate the old name if Python consumers depend on it.

---

### R-10 — Fix `name_to_index` collision by switching to multi-map

**Linked finding:** F-10

**File:** `crates/uitree/src/tree_map.rs`, line 106

**Action:** Change `name_to_index` from `HashMap<String, usize>` to `HashMap<String, Vec<usize>>`. Update `get_element_by_name` to return the first match or all matches. The `rtid_to_index` map is fine since runtime IDs should be unique.

Alternatively, if `get_element_by_name` is not used in practice (it is not called from any non-test code), remove it entirely and document that name-based lookup is intentionally unsupported because names are not unique.

---

### R-11 — Remove `dbg!()` from production code

**Linked finding:** F-11

**File:** `crates/bromium/src/windriver.rs`, line 54

**Action:** Replace `dbg!(log_level_parsed)` with `debug!("Log level parsed: {:?}", log_level_parsed)`.

---

### R-12 — Replace deprecated `env::home_dir()` with `dirs::home_dir()`

**Linked finding:** F-12

**Files:**
- `crates/bromium/src/logging.rs`, lines 38, 152
- `crates/bromium/src/instance_logging.rs`, line 80

**Action:** Use the `dirs` crate (or `home` crate) which provides a correct, non-deprecated `home_dir()`. This requires adding a dependency — confirm with team first. Alternatively, use `std::env::var("USERPROFILE")` on Windows as a targeted fix.

---

## Phase 3 — Performance

**Goal:** Eliminate unnecessary allocations and redundant work on the hot paths.
**Validation:** `cargo test` passes. Measure UI tree retrieval time and XPath query time before and after with the `uitree` main binary.

---

### R-13 — Avoid cloning XML DOM string on every XPath query

**Linked finding:** F-13

**File:** `crates/uitree/src/uiexplore_xml.rs`, lines 168, 206

**Current code:**
```rust
let xml = self.get_xml_dom_tree().to_string();  // full copy every call
let xpath_result = eval_xpath(&xpath, &xml);
```

**Action:** `get_xml_dom_tree()` already returns `&str`. Pass it directly:
```rust
let xpath_result = eval_xpath(&xpath, self.get_xml_dom_tree());
```

The `eval_xpath` function takes `&str`, so no copy is needed.

---

### R-14 — Cache UIAutomation instance in `save_ui_element.rs`

**Linked finding:** F-14

**File:** `crates/uitree/src/save_ui_element.rs`, lines 98–124

**Action:** `get_ui_automation_ui_element()` creates a new `UIAutomation` instance and does a full depth-99 tree search on every call. This is invoked on every click/send_keys/etc.

1. Use a thread-local or `OnceLock` cached `UIAutomation` instance instead of creating a new one each time.
2. Consider using `UIAutomation::element_from_handle()` when `self.handle` is non-zero (O(1) lookup vs O(n) tree walk).
3. Reduce the search depth from 99 to a sensible maximum, or use `UIAutomation::element_from_point()` / `element_from_handle()` as primary lookup.

---

### R-15 — Pass `&str` references instead of cloning Strings in recursive traversal

**Linked finding:** F-15

**File:** `crates/uitree/src/uiexplore_xml.rs`, lines 794–808 (function signature), 931–960 (recursive calls)

**Action:** Change the `get_element` function signature:
```rust
fn get_element(
    ...
    calling_window_caption: Option<&str>,   // was Option<String>
    target_window_caption: Option<&str>,     // was Option<String>
    tree_path: &mut String,                  // was String (owned, cloned each call)
)
```

This eliminates thousands of `String` heap allocations during tree traversal.

---

### R-16 — Extract runtime ID formatting into a utility function

**Linked finding:** F-16

**Action:** The pattern:
```rust
element.get_runtime_id()
    .unwrap_or(vec![0, 0, 0, 0])
    .iter()
    .map(|x| x.to_string())
    .collect::<Vec<String>>()
    .join("-")
```

appears in approximately 10 locations across `uiexplore.rs`, `uiexplore_xml.rs`, `uiexplore_iter.rs`, and `save_ui_element.rs`.

Add to `bromium-common`:
```rust
pub fn format_runtime_id(id: &[i32]) -> String {
    id.iter()
        .map(|x| x.to_string())
        .collect::<Vec<String>>()
        .join("-")
}
```

Replace all inline instances.

---

### R-17 — Remove unnecessary clone of `child_elements` in parallel traversal

**Linked finding:** F-17

**File:** `crates/uitree/src/uiexplore_xml.rs`, line 730

**Current code:**
```rust
for element in child_elements.clone() {  // clones all UIElements
```

**Action:** Capture the count before the consuming loop:
```rust
let child_count = child_elements.len();
for element in child_elements {  // moves, no clone
    ...
}
// use child_count instead of iterating child_elements again (line 755)
for _ in 0..child_count {
    let subtree: UITree = rx_par.recv_timeout(Duration::from_secs(120))
        .map_err(|e| ...)?;
    subtrees.push(subtree);
}
```

---

## Phase 4 — Code Quality & Consolidation

**Goal:** Reduce duplication, remove dead code, improve API idioms.
**Validation:** `cargo build` passes, `cargo test` passes, `cargo clippy` clean. No public API signature changes in the `bromium` (PyO3) crate.

---

### R-18 — Consolidate the three `SaveUIElement` types

**Linked finding:** F-18

**Files:**
- `crates/uitree/src/uiexplore.rs` — `SaveUIElement` with `pub element: UIElement`
- `crates/uitree/src/uiexplore_iter.rs` — identical copy
- `crates/uitree/src/save_ui_element.rs` — stores extracted properties, no COM pointer

**Action:**

1. Designate `save_ui_element.rs::SaveUIElement` as the single canonical type (it extracts properties at construction time, avoiding COM lifetime issues).
2. Update `uiexplore.rs` and `uiexplore_iter.rs` to use `save_ui_element::SaveUIElement`.
3. This eliminates the need for `unsafe impl Send + Sync` (resolves F-05 fully).
4. Update all call sites that access `.element` directly to use the getter methods.

---

### R-19 — Consolidate duplicated `UIElementInTree`, `UITree`, and `match_original_format`

**Linked finding:** F-19

**Files:**
- `crates/uitree/src/uiexplore.rs`
- `crates/uitree/src/uiexplore_xml.rs`
- `crates/uitree/src/uiexplore_iter.rs`

**Action:**

1. Extract `UIElementInTree` and `match_original_format` into a shared module (e.g., `crates/uitree/src/common_types.rs`).
2. Parameterize or trait-ify the differences between the three `UITree` variants if they exist, or collapse them into one.
3. Re-export the shared types from `lib.rs`.

---

### R-20 — Remove dead `InstanceLogger` module

**Linked finding:** F-20

**File:** `crates/bromium/src/instance_logging.rs` (153 lines)

**Action:**

1. Verify `InstanceLogger` is not used anywhere (it is imported in `windriver.rs` only for the `FromStrLevelFilter` trait).
2. Move the `FromStrLevelFilter` trait to `logging.rs` or `commons.rs`.
3. Delete `instance_logging.rs`.
4. Remove the `mod instance_logging` line from `lib.rs`.

---

### R-21 — Remove blanket `#[allow(dead_code)]` suppressions

**Linked finding:** F-21

**Files:**
- `crates/bromium/src/logging.rs`, line 1
- `crates/bromium/src/instance_logging.rs`, line 1
- `crates/uitree/src/commons.rs`, line 1
- `crates/uitree/src/tree_map.rs`, line 2
- `crates/xmlutil/src/xml_dom_manager.rs`, line 1

**Action:** Remove the blanket `#![allow(dead_code)]` from each file. Add targeted `#[allow(dead_code)]` only to specific items that are intentionally unused (e.g., library functions intended for future use). Fix or remove any items the compiler flags as actually dead.

---

### R-22 — Change `&Vec<T>` return types to `&[T]`

**Linked finding:** F-22

**Files:**
- `crates/uitree/src/uiexplore_xml.rs` — `get_elements() -> &Vec<UIElementInTree>`
- `crates/uitree/src/uiexplore.rs` — `get_elements() -> &Vec<UIElementInTree>`
- `crates/uitree/src/uiexplore_iter.rs` — `get_elements() -> &Vec<UIElementInTree>`
- `crates/uitree/src/save_ui_element.rs` — `get_runtime_id() -> &Vec<i32>`
- `crates/uitree/src/tree_map.rs` — `nodes() -> &Vec<UITreeNode<T>>`

**Action:** Change return types from `&Vec<T>` to `&[T]`. This is a non-breaking change for callers (all `&Vec<T>` methods are available on `&[T]` via `Deref`).

---

### R-23 — Change `get_point_bounding_rect` parameter from `&Vec<T>` to `&[T]`

**Linked finding:** F-23

**File:** `crates/bromium/src/rectangle.rs`, line 17

**Action:** Change:
```rust
pub fn get_point_bounding_rect<'a>(
    point: &'a POINT,
    ui_elements: &'a Vec<UIElementInTreeXML>,  // ← change to &'a [UIElementInTreeXML]
) -> Option<&'a UIElementInTreeXML>
```

---

## Phase Execution Summary

| Phase | Scope | Findings Addressed | Validation Criteria |
|-------|-------|--------------------|---------------------|
| **Phase 1** | Critical reliability | F-01 through F-05 | `cargo build` + `cargo test` pass. No panics when running against a stale UI tree from Python. No GIL deadlock on timeout. |
| **Phase 2** | Correctness bugs | F-06 through F-12 | `cargo test` passes with new tests for sort ordering and event consumption. `cargo clippy` clean. Manual verification of `send_text` error paths. |
| **Phase 3** | Performance | F-13 through F-17 | `cargo test` passes. Benchmark UI tree retrieval and XPath query time before/after. Expect measurable improvement on XPath queries (F-13) and element interactions (F-14). |
| **Phase 4** | Code quality | F-18 through F-23 | `cargo build` + `cargo test` + `cargo clippy` clean. Line count reduction from type consolidation. No public API changes in the `bromium` PyO3 crate. |

---

## Notes

- **Phase ordering is intentional.** Phase 1 fixes must land first because they prevent crashes and hangs. Phase 2 fixes correctness that users may already be experiencing. Phase 3 and 4 can be interleaved.
- **Each phase should be a separate PR** (or set of PRs) to keep reviews focused and rollback easy.
- **Adding the `dirs` crate** (R-12) requires team confirmation per the project's dependency management rules.
- **R-18 (consolidation) is the highest-effort item** and will touch many files. It should be done on a dedicated branch with careful review.
