# UI Tree State & Refresh — Remediation Plan

**Plan date:** 2026-09-02
**Source audit:** [`TREE_STATE_AUDIT.md`](TREE_STATE_AUDIT.md) — 16 findings (T-01…T-16)
**Scope:** `uitree`, `bromium`, `uiexplore`, `xmlutil`, `winevent-monitor`
**Validation gate (every phase):** `cargo test --workspace` green, `cargo clippy --workspace --all-targets --all-features -- -D warnings` clean, `cargo run -p uiexplore` starts and refreshes, `crates/bromium/tests/app_start_danipc.py` runs after `maturin develop`.

---

## Amendment to the audit — T-02 is live, not latent

The audit recorded T-02 (z-order semantics) as latent because only the benchmark binary calls `get_all_elements_par_xml`. That is wrong, and it changes the plan's ordering.

The root cause is `get_element`'s rule at `uiexplore_xml.rs:837`:

```rust
let effective_z_order = if level == 0 { 999 } else { z_order };
```

Every walk root is stamped 999 and its children are numbered `0, 1, 2…`. That fires for **any** walk that is not rooted at the desktop — which includes the scoped refresh in `WinDriver::get_element_by_xpath`, since `find_scoped_root_element` returns `SaveUIElementXML::new(&element, 0, 999)` (`windriver.rs:1265`) and the walk is rooted there.

So today, after any scoped retry, the driver's tree carries the same broken z-order data the parallel walker produces. The practical impact is currently small only because `bromium`'s hit-test ignores z-order altogether (T-15) — i.e. **two defects are cancelling each other out**. Fixing T-15 alone would turn a silent inconsistency into wrong hit-test results.

**Consequences for sequencing:** T-02 moves from "before the parallel walker is adopted" to Phase 2, and it becomes a hard prerequisite for both T-03 and T-15.

---

## Dependency graph

```
T-INFRA (test constructors)
   │
   ├──> T-01  invariant + debug_assert ──────────────┐
   │                                                  │
   ├──> T-02  base_z_order                            │
   │      │                                           │
   ├──> T-04  tree_index remap on merge               │
   │      │                                           ├──> T-03  scoped refresh via merge
   ├──> T-10 root double-count ───────────────────────┘
   │
   └──> T-15  z-order in bromium hit-test  (needs T-02)
```

**T-03 must not be implemented first**, despite being the highest-value fix. Its fix routes the scoped result through `append_or_replace_subtree`, which is precisely the code path carrying T-04 (stale `tree_index`) and T-02 (z-order re-derivation). Merging into the driver's hot path before those are fixed would promote two latent defects into the primary Python API. The audit's "Recommended order" listed T-03 second; this plan corrects that to T-01 → T-02/T-04/T-10 → T-03.

---

## Remediation index

| Finding | Sev | Remediation | Phase | Size | Depends on |
|---|---|---|---|---|---|
| — | — | R-T00 — Test constructors for `SaveUIElement` / `UIElementInTree` | 0 | S | — |
| T-01 | 🔴 | R-T01 — Make `UITree::empty()` self-consistent; assert the invariant | 1 | S | R-T00 |
| T-11 | 🟡 | R-T11 — Make tombstone slots fail loudly | 1 | S | R-T01 |
| T-13 | 🟡 | R-T13 — Propagate XML write errors instead of truncating | 1 | S | — |
| T-09 | 🟠 | R-T09 — Correct the `unsafe impl Send` safety contract | 1 | S | — |
| T-02 | 🔴 | R-T02 — Thread a `base_z_order` through the walker | 2 | M | R-T00 |
| T-04 | 🟠 | R-T04 — Remap `tree_index` when merging a subtree | 2 | M | R-T00 |
| T-10 | 🟠 | R-T10 — Record the walk root exactly once | 2 | S | R-T00 |
| T-03 | 🔴 | R-T03 — Merge scoped results; re-resolve the scoped root each iteration | 3 | L | R-T02, R-T04, R-T10 |
| T-07 | 🟠 | R-T07 — One filter-resolution helper for all refresh paths | 3 | S | — |
| T-08 | 🟠 | R-T08 — Track the constructor's cancel flag; document the shallow tree | 3 | S | — |
| T-06 | 🟠 | R-T06 — Bound and cancel UI Explore's refresh | 4 | M | — |
| T-05 | 🟠 | R-T05 — Stop the WinEvent backlog; remove dead `mouse_hwnd` | 4 | M | — |
| T-15 | 🟡 | R-T15 — Port the z-order filter into `bromium`'s hit-test | 5 | S | R-T02 |
| T-14 | 🟡 | R-T14 — Resolve the `get_elements_by_xpath` retry asymmetry | 5 | S | R-T03 |
| T-12 | 🟡 | R-T12 — Surface sibling truncation on the tree | 5 | S | — |
| T-16 | 🟡 | R-T16 — Single `xot` document across merges; sort once | 5 | M | R-T04 |

Sizes: **S** ≈ under an hour, **M** ≈ half a day, **L** ≈ a day including tests.

---

## Phase 0 — Test infrastructure (enabler)

**Goal:** Make the tree invariants testable without COM. Four of the audit's findings (T-02, T-04, T-10 and the `build_test_tree` weakness) cannot get a regression test today because `SaveUIElement` has private fields and its only constructor needs a live `UIElement`.

### R-T00 — Test constructors

**File:** `crates/uitree/src/save_ui_element.rs`, `crates/uitree/src/common_types.rs`

**Action:** add a test-only constructor that lets a test set the fields the tree invariants depend on:

```rust
impl SaveUIElement {
    /// Test-only constructor: builds a `SaveUIElement` without COM.
    #[cfg(test)]
    pub(crate) fn for_test(
        name: &str,
        control_type: &str,
        runtime_id: Vec<i32>,
        level: usize,
        z_order: usize,
        bounding_rect: uiautomation::types::Rect,
    ) -> Self { /* … remaining fields default … */ }
}
```

Keep the affected tests **in-crate** (`#[cfg(test)] mod tests` inside `uiexplore_xml.rs`), not in `tests/`, so `pub(crate)` visibility suffices and no test-only API leaks into the published crate.

**Then rewrite `build_test_tree`** (`uiexplore_xml.rs:954-971`) to give every element a **distinct, non-empty** runtime ID matching its XML `RtID`. As written, every element uses `SaveUIElement::default()` with an empty runtime ID, so the existing XPath round-trip tests pass through `build_node_to_elem`'s `tree_index` fallback and never exercise the primary runtime-ID mapping at all.

**Test:** the existing five tests in that module must still pass after the rewrite — if any fails, it was passing for the wrong reason and the failure is itself the finding.

---

## Phase 1 — Panic and invariant safety

**Goal:** No public method can panic on a well-formed `UITree`, and the core invariant is machine-checked.

### R-T01 — Make `UITree::empty()` self-consistent

**Linked finding:** T-01
**File:** `crates/uitree/src/uiexplore_xml.rs:46-56`

**Action:** route `empty()` through the same construction path as every other tree, so the invariant cannot be established incorrectly:

```rust
pub fn empty() -> Self {
    let tree = UITreeMap::new("Root".to_string(), String::new(), ());
    let elements = vec![UIElementInTree::new(SaveUIElement::default(), 0)];
    UITree::new(tree, String::new(), elements)   // builds node_to_elem correctly
}
```

Then make the invariant non-optional. In both `UITree::new` and `rebuild_node_to_elem`, after building the map:

```rust
debug_assert_eq!(
    node_to_elem.len(),
    tree.node_count(),
    "node_to_elem must have one entry per arena node"
);
```

**Test (in-crate):**
```rust
#[test]
fn empty_tree_upholds_node_to_elem_invariant() {
    let t = UITree::empty();
    assert_eq!(t.node_to_elem.len(), t.get_tree().node_count());
    t.pretty_print_tree();            // must not panic
    t.for_each(|_, _| {});            // must not panic
    let _ = t.node(0);                // must not panic
}
```
Add the same length assertion after an `append_or_replace_subtree` call. These three calls are the exact reproducer from the audit and panicked at `uiexplore_xml.rs:135`, `:150`, `:174` before the fix.

**Risk:** none. `empty()` has two call sites, both in `uiexplore`, both of which only read `get_elements()` and `children(0)` today.

### R-T11 — Make tombstone slots fail loudly

**Linked finding:** T-11
**File:** `crates/uitree/src/uiexplore_xml.rs:75-101`

**Action:** in `build_node_to_elem`, initialise dead-node slots to `usize::MAX` instead of leaving them at `0`, so an accidental raw-index lookup on a tombstone panics on the spot instead of silently returning the root element's properties. Keep the `continue` for dead nodes; change only the initial fill:

```rust
let mut map = vec![usize::MAX; tree.node_count()];
```

Do this **after** R-T01, since `empty()`'s single node must map to a real element first.

**Test:** remove a node, then assert `node_to_elem[removed_index] == usize::MAX` and that a traversal (`for_each`) still visits every live node without touching it.

**Risk:** low, but it converts a silent-wrong-data path into a panic. Grep for raw `node_to_elem` indexing outside the guarded accessors before merging — currently there is none, which is what makes this safe.

### R-T13 — Propagate XML write errors

**Linked finding:** T-13
**File:** `crates/uitree/src/uiexplore_xml.rs:859-865, 931-936`

**Action:** a failed `Event::Start` write currently `return`s without the matching `Event::End` and without signalling failure, so a malformed `xml_dom_tree` is delivered as `Ok`. Give `get_element` a `Result<(), UITreeError>` return (or an `&mut Option<UITreeError>` out-parameter to avoid churning the 13-argument signature), propagate write failures to `get_all_elements_xml`, and send `Err(UITreeError::Xml(..))` rather than a corrupt tree.

Note the cancellation early-returns at `:786` and `:902` have the same shape but are already safe — `get_all_elements_xml` re-checks the flag at `:552-556` and discards the tree. Only the write-error path lacks a backstop.

**Test:** not economically unit-testable without a failing writer; verify by inspection that every early return after the `Start` write either writes `End` or propagates an error.

### R-T09 — Correct the `unsafe impl Send` safety contract

**Linked finding:** T-09
**File:** `crates/xmlutil/src/xpath_eval.rs:23-26`

**Action:** documentation only — no behaviour change. Replace the comment, which currently claims single-thread ownership that the design contradicts (`UITree` crosses threads on every refresh):

```rust
// SAFETY: `Documents` holds `Rc`s, so this type is not `Send` by construction.
// It is sound here only because of two invariants that MUST be preserved:
//   1. The cache is reachable exclusively through `UITree::xpath_cache`, a
//      `Mutex`. All access is serialised, and the mutex supplies the
//      happens-before edges the non-atomic `Rc` refcounts require.
//   2. No `Rc`, `DocumentHandle` borrow, or other interior handle ever escapes
//      this type — every accessor returns owned data.
// A `UITree` IS moved between threads (walker thread -> main thread on every
// refresh), so any new accessor that breaks invariant 2 makes this unsound.
unsafe impl Send for XpathDocCache {}
```

**Risk:** none. Consider a follow-up ticket to drop the `unsafe impl` entirely by rebuilding the cache per owning thread, but that is a design change, not a fix.

---

## Phase 2 — Tree identity correctness

**Goal:** A merged tree is indistinguishable from a freshly walked one. This is the prerequisite for Phase 3.

### R-T02 — Thread a `base_z_order` through the walker

**Linked finding:** T-02 (see the amendment above — this is live in `bromium` today)
**File:** `crates/uitree/src/uiexplore_xml.rs:459-577, 579-767, 769-938`

**Action:** the walker currently re-derives z-order from its own position (`level == 0 → 999`), so the value means "distance from *this* walk's root" rather than "which top-level window". Make it an explicit input:

1. Add a parameter to `get_all_elements_xml`:
   ```rust
   pub fn get_all_elements_xml(
       tx: Sender<Result<UITree, UITreeError>>,
       root_element: Option<SaveUIElement>,
       base_z_order: Option<usize>,   // NEW: z-order to stamp on the walk root
       max_depth: Option<usize>,
       calling_window_caption: Option<String>,
       target_window_caption: Option<String>,
       cancel: Option<Arc<AtomicBool>>,
   )
   ```
   `None` preserves today's desktop-root behaviour (root = 999, top-level children numbered from 0). `Some(z)` stamps the root with `z` and makes all descendants inherit `z`.

2. In `get_element`, replace the positional rule with the passed-through value; increment only when `base_z_order.is_none() && level + 1 == 1`.

3. In `get_all_elements_par_xml`, pass each window's true index as `base_z_order` when spawning its sub-walk (`:700-714`), so the parallel tree reproduces the sequential numbering.

4. In `find_scoped_root_element` (`windriver.rs:1265`), stop hard-coding `999` — carry the window's z-order from the existing tree when it is known, or pass it as `base_z_order` at the call site.

**Test:** the decisive test needs no COM. Build two `UITree`s from the same fixture — one flat, one assembled from subtrees via `append_or_replace_subtree` — and assert the `(runtime_id → z_order)` maps are equal. Add an assertion that exactly one element has `z_order == 999`.

**Verification against a live desktop** (this is how the defect was found, and it is the acceptance check):
```
cargo run --release -p uitree
```
Then compare the two emitted XML files: the count of `z-order="999"` must be **1** in both. Before the fix it is 1 sequential and 11 parallel on a 10-window desktop.

**Risk:** medium — touches the walker's hot signature, used by six call sites. All six are in-repo (`windriver.rs:856,1141`, `app_ui.rs:221,1034`, `uiexplore/main.rs:38`, `uitree/main.rs:55,82`).

### R-T04 — Remap `tree_index` when merging a subtree

**Linked finding:** T-04
**File:** `crates/uitree/src/uiexplore_xml.rs:299-392`, `crates/uitree/src/common_types.rs`

**Action:** `append_children` adds subtree nodes at new arena indices while the merged elements keep their `tree_index` from the subtree's arena. Pick one:

- **Preferred — remap.** Have `append_children` build an `old_index → new_index` map as it recurses, then rewrite `tree_index` on the merged elements before `rebuild_node_to_elem()`. Requires a `set_tree_index(&mut self, idx: usize)` on `UIElementInTree` (currently the field is private with no setter).
- **Alternative — eliminate.** Drop `tree_index` entirely and resolve node↔element strictly by runtime ID. Cleaner, but `get_element_by_coordinates` (`windriver.rs:778`) and `build_node_to_elem`'s empty-runtime-ID fallback both depend on it, so this is a larger change; prefer it only if empty runtime IDs can be shown not to occur.

**Test (the T-04 detector, in-crate, needs R-T00):**
```rust
#[test]
fn tree_index_is_consistent_after_merge() {
    let mut tree = build_test_tree();          // distinct runtime IDs after R-T00
    tree.append_or_replace_subtree(tree.get_tree().root(), build_other_subtree()).unwrap();
    for elem in tree.get_elements() {
        let node = tree.get_tree().node(elem.get_tree_index());
        assert_eq!(node.runtime_id, format_runtime_id(elem.get_element_props().get_runtime_id()));
    }
}
```
Run the same assertion before the merge too, so the test proves the merge is what breaks it.

### R-T10 — Record the walk root exactly once

**Linked finding:** T-10
**File:** `crates/uitree/src/uiexplore_xml.rs:516-548`

**Action:** delete the pre-walk `ui_elements.push` at `:530-531` and let `get_element` be the single writer into `ui_elements`. Keep the explicit `UITreeMap::new` root — the GUI's `render_ui_tree_recursive` starts at node 0's children and needs node 1 to exist as the displayed desktop root, so the *arena* duplication is load-bearing and must stay.

Also make sure the walk still records the root when it has no children: the `ui_elements.push` currently happens unconditionally, while `get_element` is only called inside `if let Ok(_first_child) = walker.get_first_child(&root)` (`:533`). After the change, an empty desktop would produce zero elements. Restructure so `get_element` is called unconditionally and handles the no-children case internally.

**Test (the T-10 detector):** assert `ui_elements.len()` equals the number of `RtID="` occurrences in `xml_dom_tree`. On the live run this is 648 vs 647 (sequential) and 661 vs 650 (parallel) before the fix; both must be equal after.

**Note:** this changes `len(driver)` / `driver.element_count` by one. That is a user-visible correction — mention it in the release notes for the version that ships it.

---

## Phase 3 — Refresh-path correctness (`bromium`)

**Goal:** A refresh changes how fresh the tree is, never what it represents.

### R-T03 — Merge scoped results instead of replacing the tree

**Linked finding:** T-03
**File:** `crates/bromium/src/windriver.rs:837-902, 1235-1266`
**Depends on:** R-T02, R-T04, R-T10 — do not start before those land.

**Action, three parts:**

1. **Merge, don't assign.** Replace `self.ui_tree = tree_result…?` (`:871`) on the scoped branch with a merge into the existing tree:
   ```rust
   if scoped_root.is_some() {
       let parent = self.ui_tree.get_tree().root();
       self.ui_tree
           .append_or_replace_subtree(parent, new_subtree)
           .map_err(|e| TreeConstructionError::new_err(format!("scoped merge failed: {e}")))?;
   } else {
       self.ui_tree = new_tree;      // unscoped full rebuild keeps today's behaviour
   }
   ```
   This preserves the desktop root, so absolute XPaths of the form `/Pane[@Name='Desktop 1']/…` — the form the README example uses and the form `Element.xpath` returns — keep resolving after a retry.

2. **Re-resolve the scoped root inside the loop.** `find_scoped_root_element` is called once at `:837`, before the retry loop. In the dominant "wait for the app window to appear" case the window does not exist yet, so it returns `None` and *every* iteration does a full desktop walk and never re-checks. Move the call inside the loop (it is a cheap depth-1, `timeout(0)` matcher lookup) so the walk narrows as soon as the window appears.

3. **Fall back safely.** If the merge fails (parent gone, malformed subtree), fall back to a full unscoped rebuild rather than surfacing an error — the caller asked for an element, not for a tree operation.

**Test:**
- In-crate, no COM: assert that after `append_or_replace_subtree`, an absolute XPath from the desktop root to an element *inside* the merged subtree still resolves. This is the regression that T-03 describes and is currently impossible.
- Manual, with COM: run `crates/bromium/tests/app_start_danipc.py`, then assert `driver.element_count` after a scoped retry is still desktop-scale (hundreds), not window-scale, and that `'/Pane[@Name=\'Desktop 1\']' in driver` is still `True`.

**Risk:** highest-value and highest-risk item in the plan. It moves `append_or_replace_subtree` — until now exercised only by a benchmark binary — into the primary Python code path. Phases 0–2 exist to make that safe; do not compress them.

### R-T07 — One filter-resolution helper

**Linked finding:** T-07
**File:** `crates/bromium/src/windriver.rs:760-763, 1089, 845`

**Action:** `refresh(window_title=…)` uses the argument for one rebuild but never writes it back, so the next automatic refresh inside `get_element_by_xpath` silently reverts to the old filter. Decide the semantics explicitly and apply it in one place:

```rust
/// Resolve the window-title filter for a rebuild: explicit argument wins,
/// otherwise the driver's stored filter.
fn resolve_window_filter(&self, explicit: Option<String>) -> Option<String> {
    explicit.or_else(|| self.window_title.clone())
}
```

**Recommended semantics — persist it.** In `refresh`, when `window_title` is `Some`, also assign `self.window_title`. That makes manual and automatic refreshes agree, which is the property the audit found violated, and it matches what a caller passing a title plainly intends. Update the `refresh` docstring in `bromium.pyi:433-446` accordingly — it currently documents only the resolution rule, not the persistence.

If persistence is undesirable (a genuine one-shot override is wanted), then instead give `get_element_by_xpath` an explicit filter parameter so the two paths cannot silently diverge — but do not leave it as-is.

### R-T08 — Track the constructor's cancel flag; document the shallow tree

**Linked finding:** T-08
**File:** `crates/bromium/src/windriver.rs:572-607`; docs in `crates/bromium/bromium.pyi:294-306` and `README.md`

**Action, two independent parts:**

1. **Cancel flag.** Store the flag that was actually handed to the walker instead of minting a fresh one:
   ```rust
   let cancel_flag = Arc::new(AtomicBool::new(false));
   let flag_for_driver = Arc::clone(&cancel_flag);
   let tree_result = py.allow_threads(move || Self::spawn_tree_construction(cancel_flag, …));
   let driver = WinDriver { …, cancel_flag: flag_for_driver };
   ```
   Currently harmless — a constructor timeout aborts construction anyway — but it breaks the "the field tracks the most recent walker" invariant every other path maintains.

2. **Document the depth-2 tree.** The constructor passes `Some(2)`, so a fresh driver holds only top-level windows and their immediate children. `bromium.pyi:301` mentions "(depth 2)" but only inside the `window_title=None` branch, where it reads as a property of the desktop capture rather than of the constructor; `README.md:136` does not mention it at all and its quickstart prints `len(driver)` as though it were the whole desktop. Fix both: state that the constructor builds a **shallow (depth-2)** tree and that the first deep lookup either needs an explicit `refresh()` or will trigger a full rebuild through the retry path.

---

## Phase 4 — UI Explore lifecycle

**Goal:** The GUI's refresh has the same bounds and cancellability as the Python driver's.

### R-T06 — Bound and cancel UI Explore's refresh

**Linked finding:** T-06
**File:** `crates/uiexplore/src/app_ui.rs:179-183, 1030-1066`

**Action:** carry a deadline and a cancel flag in the refreshing state, mirroring what the startup path already does:

```rust
enum AppMode {
    Normal(LastRefresh),
    NeedsTreeRefresh,
    IsRefreshingTree {
        rx: Receiver<Result<UITreeXML, UITreeError>>,
        started: std::time::Instant,
        cancel: Arc<AtomicBool>,
    },
}
```

- In `NeedsTreeRefresh`, build the flag and pass `Some(cancel.clone())` into `get_all_elements_xml` (currently `None`, `:1034`).
- In `IsRefreshingTree`, after the `Err(_) => {}` "not ready" arm, check `started.elapsed() > TREE_TIMEOUT` (reuse the 120 s constant from `main.rs`); on expiry set `cancel`, return to `AppMode::Normal`, and set an error status.
- Call `ctx.request_repaint()` while in `IsRefreshingTree` so the deadline is still evaluated when no input events arrive — otherwise, in reactive mode with the mouse outside the window, the timeout would never be reached.

**Test:** manual — start UI Explore, trigger a refresh, confirm it completes and the status clears. The hang path itself is not reproducible on demand; the code review criterion is that every arm of `IsRefreshingTree` has an exit edge.

### R-T05 — Stop the WinEvent backlog

**Linked finding:** T-05
**File:** `crates/winevent-monitor/src/winevent.rs:22-65`; `crates/uiexplore/src/app_ui.rs:1013-1028`

**Action:**

1. **Drain unconditionally.** The cheapest correct fix: call `check_for_events()` every frame and discard the result when `!auto_refresh`, instead of gating the drain itself. The channel is unbounded and fed by `ObjectLocationChange` / `ObjectShow` / `ObjectHide` / `ObjectCreate` / `ObjectDestroy` from a dedicated hook thread, so with the default `auto_refresh = false` it currently grows for the whole session.
   Alternative, better but larger: install the hook lazily when auto-refresh is enabled and uninstall on disable. Prefer this if the hook's own cost matters; it also removes the "first enable always fires a refresh from the accumulated backlog" behaviour.
2. **Remove the dead filter.** `mouse_hwnd` is initialised to `HWND::default()` (0) and never assigned, so `hwnd.0 != self.mouse_hwnd.0` only ever excludes null handles. Delete the field, or implement it if excluding the cursor's own window was the intent.
3. **Use the fallible constructor.** `UIExplorer` calls `WinEventMonitor::new()`, which `expect`s on hook-install failure, while `try_new()` returns a `Result` and is unused. Switch to `try_new()` and degrade to "auto-refresh unavailable" with a status message rather than panicking at startup.

**Test:** add a `winevent-monitor` unit test that pushes N synthetic events into the channel and asserts `check_for_events()` returns them and leaves the channel empty. For the leak itself, verify manually: run UI Explore with auto-refresh off, move windows around for a minute, and confirm process memory is flat.

---

## Phase 5 — API consistency and performance

**Goal:** Remove the remaining asymmetries; recover the parallelism the merge currently eats.

### R-T15 — Port the z-order filter into `bromium`'s hit-test

**Linked finding:** T-15 · **Depends on:** R-T02

**File:** `crates/bromium/src/rectangle.rs:12-35`

**Action:** give `bromium`'s `get_point_bounding_rect` the `target_z_order: Option<usize>` parameter that `uiexplore`'s copy already has (`crates/uiexplore/src/rectangle.rs:17-47`), and have `get_element_by_coordinates` resolve the window under the cursor via `WindowFromPoint` the way the GUI does. Without it, an occluded window's small control can win the hit-test.

**Order matters:** this must land *after* R-T02. Filtering on z-order while the scoped-refresh path still stamps 999 on every walk root would make the hit-test fail outright rather than merely ignore occlusion — the two current defects are cancelling out.

**Consolidation opportunity:** the two implementations differ only by this parameter. Move the function to `bromium-common::rectangle` (which already hosts `is_inside_rectangle`) and delete both copies.

### R-T14 — Resolve the `get_elements_by_xpath` retry asymmetry

**Linked finding:** T-14 · **Depends on:** R-T03

**File:** `crates/bromium/src/windriver.rs:937-971`; `bromium.pyi:385-394`

**Action:** the singular lookup retries with rebuilds until `timeout_ms`; the plural takes `&self`, queries once, and returns `[]`. Pick one and make it explicit:

- **Preferred:** give the plural form the same `timeout_ms: Option<u64>` parameter and retry path, changing it to `&mut self`. Consistent, and it is what a caller migrating from the singular expects.
- **Minimum:** keep it snapshot-only and say so in the docstring and README — "does not refresh; call `refresh()` first if the tree may be stale."

### R-T12 — Surface sibling truncation

**Linked finding:** T-12
**File:** `crates/uitree/src/uiexplore_xml.rs:894-900`, `crates/uitree/src/uiexplore_xml.rs:27`

**Action:** the `MAX_SIBLINGS` (10 000) break logs a `warn!` and produces a silently incomplete tree. Add a `truncated: bool` to `UITree`, set it when the cap trips, expose it via a getter, and have `WinDriver::get_element_by_xpath` stop retrying (and say so in the error message) when the tree it just built is truncated — retrying a truncated walk burns the full `timeout_ms` to fail identically each time.

### R-T16 — Single `xot` document across merges; sort once

**Linked finding:** T-16 (also `PERF_UITREE_REPORT.md` §P-04, line 155 — still open) · **Depends on:** R-T04

**File:** `crates/uitree/src/uiexplore_xml.rs:340-366, 403-443`

**Action:** `append_or_replace_node_by_rt_id` parses the entire accumulated XML string with `xot` and re-serialises it **on every** subtree merge, and `append_or_replace_subtree` re-sorts the whole growing element vector each time. For N top-level windows that is N full parse/serialise cycles over a document reaching ~60 KB, plus N sorts.

Restructure `get_all_elements_par_xml` to hold one `xot::Xot` document across all merges, append each subtree into it, and serialise once at the end; hoist `sort_elements` out of `append_or_replace_subtree` to a single call after the last merge. Note this changes `append_or_replace_subtree`'s contract (it would no longer leave the tree sorted), so either keep a sorted-on-exit wrapper for the single-merge callers R-T03 introduces, or make sorting an explicit `finalize()` step.

**Acceptance:** on the live benchmark the parallel walker currently runs 1.13 s against the sequential 1.56 s — a 1.4× return on 10-way parallelism. Re-run `cargo run --release -p uitree` and confirm a materially better ratio.

---

## Cross-cutting: regression tests to add

Consolidated from the phases above. None of these need COM, which is why they are cheap and currently absent.

| Test | Detects | Phase |
|---|---|---|
| `node_to_elem.len() == tree.node_count()` for `empty()`, `new()`, post-merge | T-01 | 1 |
| `empty()` survives `pretty_print_tree` / `for_each` / `node(0)` | T-01 | 1 |
| Tombstone slots are `usize::MAX`; traversal unaffected | T-11 | 1 |
| Flat vs merged tree agree on `(runtime_id → z_order)`; exactly one element at 999 | T-02 | 2 |
| `tree.node(elem.tree_index()).runtime_id == format_runtime_id(elem.runtime_id())`, pre- and post-merge | T-04 | 2 |
| `ui_elements.len()` == count of `RtID="` in `xml_dom_tree` | T-10 | 2 |
| Absolute XPath from the desktop root resolves into a merged subtree | T-03 | 3 |
| `check_for_events()` returns all queued events and empties the channel | T-05 | 4 |
| `build_test_tree` rebuilt with distinct non-empty runtime IDs | T-04 (test validity) | 0 |

---

## Decisions needed before implementation

Three items have a genuine fork that changes the work; the rest are unambiguous.

1. **R-T07 — is `refresh(window_title=…)` persistent or one-shot?** This plan recommends persistent, because it removes the manual/automatic divergence outright. One-shot is defensible but then `get_element_by_xpath` needs its own filter parameter. Affects the published Python semantics either way.
2. **R-T10 — `element_count` will change by one.** A visible correction to a documented property. Confirm this should ship as a normal patch rather than being held for a minor version.
3. **R-T14 — should the plural lookup retry?** Making it retry changes `get_elements_by_xpath` to `&mut self`, which is a Python-visible behaviour change (it can now block for `timeout_ms`). The minimum alternative is documentation only.

A fourth is worth flagging even though it is not a fork: **`get_all_elements_par_xml` should be marked `#[doc(hidden)]` or feature-gated until R-T02, R-T04 and R-T16 land.** It is currently public API presented as a drop-in faster walker, and it is not one.
