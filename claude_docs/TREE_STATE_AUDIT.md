# UI Tree State & Refresh — Audit

**Audit date:** 2026-09-02
**Scope:** Ownership, lifecycle, invariants and refresh triggers of the UI Automation tree across `uitree`, `bromium`, `uiexplore`, `xmlutil`, `winevent-monitor`
**Method:** Full read of every tree-touching path, plus two empirical checks — a scratch test harness against `UITree::empty()`, and a live desktop run of the `uitree` benchmark binary comparing the sequential and parallel walkers
**Baseline:** Findings already closed in `bromium_docs_for_agents/done/` (`F-01…F-23`, `P-01…P-08`, `CF-*`, `M-*`, `L-*`) are not repeated; where a finding here touches one of those, it is cross-referenced.

---

## Executive summary

The tree is a **snapshot**, owned by value, replaced wholesale on refresh. That model is sound and the concurrency scaffolding around it (off-thread walk, `mpsc` handoff, `AtomicBool` cancellation, GIL release) is well built and consistently applied on the Python side.

The problems are in the **invariants that hold the five parallel representations of a tree together**, and in the **asymmetry between the paths that rebuild it**:

- One constructor (`UITree::empty()`) ships a tree that violates the core index invariant and panics on three public methods — **verified, not inferred**.
- The two walkers that are supposed to produce equivalent trees produce **semantically different z-order data** — verified against a live desktop.
- The scoped-refresh optimisation silently changes what the driver's tree *is*, not just how it was built.
- Refresh triggers differ in cancellability, timeout, and filter persistence depending on which entry point you came through.

16 findings: 3 High, 7 Medium, 6 Low.

---

## The state model (as built)

One `UITree` is owned by value in exactly two places — `WinDriver.ui_tree` (`crates/bromium/src/windriver.rs:518`) and `UIExplorer.ui_tree` (`crates/uiexplore/src/app_ui.rs:203`). There is no shared or global tree, and no incremental sync with the live desktop. Every read API answers from the snapshot and may be arbitrarily stale.

A `UITree` (`crates/uitree/src/uiexplore_xml.rs:27`) carries five views of the same data that must stay mutually consistent:

| Field | Role | Consistency requirement |
|---|---|---|
| `tree: UITreeMap<()>` | arena tree, tombstoned removal | indices stable; dead nodes must be skipped |
| `xml_dom_tree: String` | serialized desktop; the actual XPath target | must match `tree` node-for-node |
| `ui_elements: Vec<UIElementInTree>` | flat element list with properties | sorted by (z-order, area) |
| `node_to_elem: Vec<usize>` | node index → position in `ui_elements` | **`len()` must equal `tree.node_count()`** |
| `xpath_cache: Mutex<Option<XpathDocCache>>` | parsed-document cache | must be `None`ed whenever `xml_dom_tree` changes |

**The load-bearing invariant is `node_to_elem.len() == tree.node_count()`.** Every accessor (`node`, `for_each`, `debug_tree`, both XPath lookups) indexes `node_to_elem` with a raw node index and panics if it is short. Only three code paths establish it: `UITree::new` (correct), `rebuild_node_to_elem` (correct), and `UITree::empty` (**broken — see T-01**).

### Refresh trigger inventory

| Entry point | Scope | Cancellable | Timeout | GIL released |
|---|---|---|---|---|
| `WinDriver::new` | depth 2 (shallow) | flag discarded (T-08) | 120 s | yes |
| `WinDriver::refresh` / `refresh_ui_tree` | full | yes | `tree_timeout_secs` | yes |
| `get_element_by_xpath` miss + timeout > 0 | scoped or full (T-03) | yes | `tree_timeout_secs` | yes |
| `refresh_ui_tree_top_2` (app launch poll, ×20) | depth 2 | yes | `tree_timeout_secs` | **no** |
| `refresh_ui_tree_internal` (post-launch) | full | yes | `tree_timeout_secs` | **no** |
| UI Explore startup | full | yes | 120 s | n/a |
| UI Explore 🔄 button | full | **no** | **none** (T-06) | n/a |
| UI Explore WinEvent auto-refresh | full | **no** | **none** (T-06) | n/a |

Only `get_element_by_xpath` refreshes automatically. `get_element_by_coordinates`, `get_elements_by_xpath`, `find_elements`, `__iter__` and `__contains__` never rebuild — they read the snapshot and return stale data or fail.

---

## Findings

| ID | Sev | Title |
|---|---|---|
| T-01 | 🔴 High | `UITree::empty()` violates the `node_to_elem` invariant — panics on three public methods |
| T-02 | 🔴 High | Parallel walker produces z-order data incompatible with the sequential walker |
| T-03 | 🔴 High | Scoped refresh silently re-roots the driver's tree |
| T-04 | 🟠 Med | `tree_index` is never remapped when a subtree is merged |
| T-05 | 🟠 Med | WinEvent channel is never drained while auto-refresh is off — unbounded growth |
| T-06 | 🟠 Med | UI Explore refresh has no timeout and no cancellation — permanent stuck state |
| T-07 | 🟠 Med | `refresh(window_title=…)` does not persist the filter |
| T-08 | 🟠 Med | Constructor builds a depth-2 tree, undocumented; its cancel flag is discarded |
| T-09 | 🟠 Med | `unsafe impl Send for XpathDocCache` — safety comment contradicts actual usage |
| T-10 | 🟠 Med | Root element is double-counted in `ui_elements` (+1 sequential, +N parallel) |
| T-11 | 🟡 Low | Tombstone nodes silently resolve to the root element's properties |
| T-12 | 🟡 Low | `MAX_SIBLINGS` truncation is indistinguishable from a missing element |
| T-13 | 🟡 Low | XML writer error path emits an unbalanced document |
| T-14 | 🟡 Low | `get_elements_by_xpath` never retries while `get_element_by_xpath` does |
| T-15 | 🟡 Low | `bromium`'s coordinate hit-test ignores z-order entirely |
| T-16 | 🟡 Low | Subtree merge is O(N²) in XML size and re-sorts on every merge |

---

### 🔴 T-01 — `UITree::empty()` violates the `node_to_elem` invariant

**Location:** `crates/uitree/src/uiexplore_xml.rs:46-56`

```rust
pub fn empty() -> Self {
    UITree {
        tree: UITreeMap::new("Root".to_string(), String::new(), ()), // 1 node
        ui_elements: Vec::new(),                                     // 0 elements
        node_to_elem: Vec::new(),                                    // 0 entries  ← invariant broken
        ...
    }
}
```

`UITreeMap::new` always creates a live root at index 0, so `node_count() == 1` while `node_to_elem.len() == 0`. Every accessor that indexes `node_to_elem` panics on the root.

**Verified** with a scratch integration test (`catch_unwind` around each call, since removed):

```
for_each          → panicked at crates\uitree\src\uiexplore_xml.rs:135:45: index out of bounds: the len is 0 but the index is 0
node(0)           → panicked at crates\uitree\src\uiexplore_xml.rs:150:41: index out of bounds: the len is 0 but the index is 0
pretty_print_tree → panicked at crates\uitree\src\uiexplore_xml.rs:174:41: index out of bounds: the len is 0 but the index is 0
get_element_by_xpath → no panic (safe)
```

**Reachability.** `UITree::empty()` was introduced by remediation step 3.2 (M-9) precisely so UI Explore could survive a failed or timed-out tree build instead of panicking — so it is constructed on exactly the path where the app is already degraded (`crates/uiexplore/src/main.rs:51,57`; `app_ui.rs` startup). Today the GUI happens to survive it: `render_ui_tree_recursive` starts at `tree.children(0)`, which is empty, so `node()` is never reached. That is luck, not design — the guard is one `pretty_print_tree()` or `for_each()` call away from a crash, and `WinDriver::pretty_print_ui_tree` (`windriver.rs:973`) is a public Python API sitting directly on top of it.

**Impact:** a fallback constructor added to prevent a panic is itself a panic hazard. High because it is a broken invariant in the type's own constructor, and the blast radius is any future caller.

**Fix:** make `empty()` self-consistent — push a default `UIElementInTree` and set `node_to_elem: vec![0]`, or route through `UITree::new(tree, String::new(), vec![default])`. Then add a debug assertion in `UITree::new`/`rebuild_node_to_elem` that `node_to_elem.len() == tree.node_count()`, so the invariant is enforced rather than assumed.

---

### 🔴 T-02 — Parallel walker produces incompatible z-order data

**Location:** `crates/uitree/src/uiexplore_xml.rs:579-767` vs `459-577`, z-order assignment at `837` and `905-907`

The two walkers are presented as equivalent (`get_all_elements_xml` / `get_all_elements_par_xml`, both exported from `uitree`). They are not. `get_element` assigns `effective_z_order = 999` when `level == 0`, and increments `z_order` only at `level + 1 == 1`. In the sequential walk the desktop is level 0, so each **top-level window** gets 0, 1, 2… and every descendant inherits its window's index — that is what makes z-order a window identifier. In the parallel walk each top-level window is itself the walk root, so every window gets **999** and its *children* get 0, 1, 2… per window.

**Verified** on a live desktop (`cargo run --release -p uitree`, 10 top-level windows):

| | sequential | parallel |
|---|---|---|
| elements with `z-order="999"` | **1** (desktop root only) | **11** (root + all 10 windows) |
| Taskbar's z-order | `0` | `999` |
| Taskbar's children | `0, 0, 0` (inherited) | `0, 1, 2` (per-window) |

**Impact:** z-order is not decoration. `walker_common::sort_elements` orders `ui_elements` by (z-order, area) — the ordering contract that R-06 was written to fix — and `uiexplore::rectangle::get_point_bounding_rect(point, elements, target_z_order)` uses z-order to restrict a cursor hit-test to the window under the cursor. On a parallel-built tree both are meaningless: sorting groups all windows into one bucket, and a `target_z_order` filter matches either everything or nothing.

Currently latent — only `crates/uitree/src/main.rs:82` calls the parallel walker — but it is public crate API presented as a drop-in faster alternative, and it is the obvious candidate for speeding up the very refresh paths audited here.

**Fix:** pass the window's true z-order into the sub-walk rather than letting each sub-walk re-derive it from `level == 0` (e.g. an explicit `base_z_order` parameter on `get_all_elements_xml`), and add a test asserting the two walkers agree on the (rtid → z-order) mapping.

---

### 🔴 T-03 — Scoped refresh silently re-roots the driver's tree

**Location:** `crates/bromium/src/windriver.rs:837-880`, `1235-1266`

On an XPath miss, `find_scoped_root_element` extracts a `Window[@Name='…']` / `Pane[@Name='…']` hint, resolves that window live, and the retry walk is rooted there. The result is then **assigned wholesale**:

```rust
self.ui_tree = tree_result...?;   // windriver.rs:871
```

So after one scoped retry the driver's entire tree — arena, XML document element, element list — is that one window. The desktop `Pane` is gone.

**Consequences:**
- Absolute XPaths that start above the scoped root (`/Pane[@Name='Desktop 1']/…` — the form the README's own example uses, and the form `Element.xpath` hands back) no longer match, because the XML document element is now the `Window`.
- `get_element_by_coordinates`, `find_elements`, `__iter__` and `element_count` afterwards silently describe one window instead of the desktop, with no signal that the scope changed.
- The retry loop re-queries the same `xpath` against this re-rooted tree (`windriver.rs:882`), so the scoped path only pays off for `//`-relative expressions — for the absolute XPaths the API otherwise encourages, every retry iteration rebuilds a tree the query then cannot match.
- `scoped_root` is resolved **once, before** the loop (`:837`). In the common "wait for the app window to appear" case the window does not exist yet, so `scoped_root` is `None` and every iteration does a full desktop walk — the expensive path — and never re-checks once the window appears.

**Impact:** the optimisation changes semantics, not just cost. High because it silently corrupts the meaning of every subsequent read on the driver.

**Fix:** the correct primitive already exists — merge the scoped result into the existing tree with `UITree::append_or_replace_subtree` (which correctly re-writes the XML, clears the XPath cache and rebuilds `node_to_elem`) instead of assigning. Additionally, re-resolve `scoped_root` inside the loop so a window that appears mid-wait starts being used.

---

### 🟠 T-04 — `tree_index` is never remapped when a subtree is merged

**Location:** `crates/uitree/src/uiexplore_xml.rs:299-392`, consumed at `crates/bromium/src/windriver.rs:778`

`append_or_replace_subtree` appends the subtree's `UIElementInTree`s into `self.ui_elements` unchanged, then calls `append_children` to add the corresponding nodes to the arena at **new indices**. The elements' `tree_index` fields still hold indices from the *subtree's* arena. Nothing rewrites them; only `node_to_elem` is rebuilt.

Two consequences:
1. `get_element_by_coordinates` does `self.ui_tree.get_xpath_for_element(ui_element_in_tree.get_tree_index(), true)` — after a merge this reads an unrelated node's runtime ID and generates the wrong XPath, or panics via `tree.node(index)` if the index is out of range.
2. `build_node_to_elem`'s secondary `idx_to_pos` fallback (`:82-86`), which exists to handle elements with empty runtime IDs, is keyed on the same stale `tree_index` and can map a node to the wrong element.

Note the existing tests do not catch this: `build_test_tree` (`:964-968`) constructs every element with `SaveUIElement::default()`, so all runtime IDs are empty and the tests pass *because* of the `tree_index` fallback rather than exercising the rtid path.

**Fix:** have `append_children` return the old→new index mapping and rewrite `tree_index` on the merged elements (a setter on `UIElementInTree` is needed), or drop the field and resolve node↔element strictly by runtime ID.

---

### 🟠 T-05 — WinEvent channel is never drained while auto-refresh is off

**Location:** `crates/winevent-monitor/src/winevent.rs:47-65,112-153`; consumed at `crates/uiexplore/src/app_ui.rs:1016-1019`

The hook is installed in `WinEventMonitor::new()` — i.e. at `UIExplorer` construction — on a dedicated thread feeding an **unbounded** `mpsc` channel. The only drain is `check_for_events()`, gated on `!self.recording && self.auto_refresh && last_refresh.time.elapsed().as_secs() > 2`.

`auto_refresh` defaults to `false` (`app_ui.rs:260,290`). So in the default configuration the channel is never read and grows for the entire session. The subscribed set includes `ObjectLocationChange`, `ObjectShow/Hide` and `ObjectCreate/Destroy` — high-frequency events fired by every window move, resize and cursor change.

Secondary: when the user first ticks "Auto Refresh", the initial `check_for_events()` returns the whole accumulated backlog, so a refresh always fires immediately regardless of whether anything relevant changed.

Also dead logic: `mouse_hwnd` is initialised to `HWND::default()` (0) and never assigned, so the `hwnd.0 != self.mouse_hwnd.0` filter only ever excludes null handles.

**Fix:** install the hook lazily when auto-refresh is enabled and uninstall when disabled, or drain unconditionally each frame and discard when auto-refresh is off. Remove or implement `mouse_hwnd`. (Related: `WinEventMonitor::new()` `expect`s on hook install failure while the unused `try_new()` returns `Result` — the GUI takes the panicking path.)

---

### 🟠 T-06 — UI Explore refresh has no timeout and no cancellation

**Location:** `crates/uiexplore/src/app_ui.rs:1030-1066`

The startup path is careful — `recv_timeout(120s)` plus an `AtomicBool` cancel flag, falling back to `UITree::empty()` (`main.rs:38-59`). The refresh path built by the same remediation is not:

```rust
AppMode::NeedsTreeRefresh => {
    thread::spawn(|| { get_all_elements_xml(tx, None, None, Some(app_name), None, None); });
    //                                                                          ^^^^ no cancel flag
    self.app_mode = AppMode::IsRefreshingTree(rx);
}
AppMode::IsRefreshingTree(rx) => { match rx.try_recv() { ... Err(_) => { /* keep waiting */ } } }
```

`try_recv` is polled once per frame with no deadline. If the walker hangs on an unresponsive COM server, the app sits in `IsRefreshingTree` forever: the tree is never replaced, the "UI Tree change detected, refreshing…" status never clears, and there is no way back to `Normal` — the 🔄 button is not rendered in that state and cannot be, because the state machine has no escape edge. Because there is no cancel flag, the orphaned walker thread also cannot be retired (the failure mode CF-03/F-03 addressed on the Python side).

**Fix:** record an `Instant` when entering `IsRefreshingTree`, give it the same 120 s deadline, thread the `AtomicBool` through, and transition back to `Normal` with an error status on expiry.

---

### 🟠 T-07 — `refresh(window_title=…)` does not persist the filter

**Location:** `crates/bromium/src/windriver.rs:1089`

```rust
let window_title_filter = window_title.or_else(|| self.window_title.clone());
```

The argument is used for this rebuild but never written back to `self.window_title`. A subsequent automatic refresh inside `get_element_by_xpath` reads `self.window_title` (`:845`) and therefore rebuilds with the **old** filter. So `driver.refresh(window_title="App B")` followed by a lookup that misses produces a tree scoped to `App A` — the manual and automatic refresh paths disagree about scope.

The setter `set_window_title` exists and is the documented way to change scope persistently, which makes the divergence easy to trip over rather than obviously wrong.

**Fix:** either assign `self.window_title = window_title` when the argument is `Some`, or document `refresh`'s argument as a one-shot override and have `get_element_by_xpath` take its filter from the same resolution helper.

---

### 🟠 T-08 — Constructor builds a depth-2 tree; its cancel flag is discarded

**Location:** `crates/bromium/src/windriver.rs:575-607`

```rust
Self::spawn_tree_construction(cancel_flag, window_title_clone, Some(2), ...)
```

Two issues:

1. **Undocumented shallow tree.** A freshly constructed `WinDriver` contains only top-level windows and their immediate children. `README.md` describes the constructor as "Creates a new driver and builds the UI tree" and its quickstart prints `len(driver)` as if it were the desktop. Any element lookup deeper than two levels misses and silently triggers a full rebuild through the retry path — so the documented "elements in tree" number and the first-call latency are both surprising. `bromium.pyi` and the README should state this.

2. **Discarded cancel flag.** `new()` builds `cancel_flag`, hands it to the walker, then stores *a different* `Arc` in the struct:
   ```rust
   let cancel_flag = Arc::new(AtomicBool::new(false));
   let tree_result = py.allow_threads(move || Self::spawn_tree_construction(cancel_flag, ...));
   let driver = WinDriver { ..., cancel_flag: Arc::new(AtomicBool::new(false)) };  // :606 — not the same flag
   ```
   The constructor's walker thread is therefore untracked by the driver. It is self-limiting (`spawn_tree_construction` sets the flag itself on timeout, and a timeout aborts construction), so this is currently harmless — but it breaks the "the field always tracks the most recent walker" invariant every other refresh path maintains, and reads as a bug at every call site that trusts it.

---

### 🟠 T-09 — `unsafe impl Send for XpathDocCache` has a false safety comment

**Location:** `crates/xmlutil/src/xpath_eval.rs:23-26`

```rust
// SAFETY: XpathDocCache is only ever accessed from the single thread that owns the
// UITree. The inner Rc<RefCell<…>> inside xee_xpath::Documents is never shared
// across threads — it is created on the owning thread and all access stays there.
unsafe impl Send for XpathDocCache {}
```

The stated invariant is not true. `UITree` is built on a walker thread and sent over an `mpsc` channel to the main thread on every single refresh — that is the core of the design. The cache is populated lazily by whichever thread first runs a query (`eval_xpath_cached`, `:215-224`), so it is routinely created on one thread and used from another.

The implementation is *probably* sound anyway — access is serialised by the `Mutex`, which also supplies the happens-before edges the non-atomic `Rc` refcount needs, and no `Rc` clone escapes the struct. But the justification on record is the wrong one, so the next person to add an accessor (e.g. anything handing out a borrowed handle, or a second cache field) has no correct rule to reason against. This is the same class as the closed F-05.

**Fix:** rewrite the comment to state the real invariant — "all access is serialised through `UITree::xpath_cache`'s `Mutex`; no `Rc` handle may escape this type" — and add a `#[deny]`-style note, or avoid the `unsafe impl` entirely by rebuilding the cache per owning thread.

---

### 🟠 T-10 — Root element is double-counted in `ui_elements`

**Location:** `crates/uitree/src/uiexplore_xml.rs:516-548`

The walk root is recorded twice: once explicitly before the walk (`:516-531`, tree node 0 + one `ui_elements` entry), then again by `get_element` itself, which is called *with the root* at `level == 0` and does its own `tree.add_child` + `ui_elements.push` (`:842,867-868`). The arena ends up with node 0 and node 1 sharing a name and runtime ID.

**Verified** on the live run:

| | `ui_elements.len()` | elements in XML | delta |
|---|---|---|---|
| sequential | 648 | 647 | **+1** (desktop root) |
| parallel | 661 | 650 | **+11** (root + each of 10 window sub-walk roots) |

For the arena this duplication is load-bearing — `render_ui_tree_recursive` starts at node 0's children, so node 1 is what displays the desktop root. For `ui_elements` it is pure over-count: Python's `len(driver)` and `driver.element_count` are off by one (by N+1 on a parallel-built tree), and `for elem in driver` yields the desktop root twice. `rtid_to_index` also silently resolves the root's runtime ID to node 1, since `add_child` overwrites node 0's entry (`tree_map.rs:129`).

**Fix:** drop the pre-walk `ui_elements.push` and let `get_element` be the single writer, keeping the explicit `UITreeMap::new` root as the synthetic parent.

---

### 🟡 T-11 — Tombstone nodes resolve to the root element's properties

`build_node_to_elem` initialises the map to `vec![0; node_count]` and `continue`s on dead nodes (`uiexplore_xml.rs:88-92`), leaving them pointing at element position 0. `UITree::node(index)` on a tombstone therefore returns `("", <root element props>)` rather than failing. Currently unreachable — `remove_node` unlinks tombstones from their parent's `children`, so no traversal reaches them — but it converts a would-be panic into silently wrong data for any future caller that indexes by raw node id. Consider `Option<usize>` entries, or mapping dead nodes to `usize::MAX` so misuse fails loudly.

### 🟡 T-12 — `MAX_SIBLINGS` truncation is indistinguishable from a missing element

`uiexplore_xml.rs:894-900` breaks out of the sibling loop after 10 000 siblings with a `warn!` only. The resulting tree is silently incomplete, and the caller cannot tell "this element does not exist" from "the walk gave up". On the Python side that difference matters: a truncated walk makes `get_element_by_xpath` burn its full `timeout_ms` re-walking and re-failing. Surface truncation on the `UITree` (a `truncated: bool`) so callers can stop retrying and report accurately.

### 🟡 T-13 — XML writer error path emits an unbalanced document

`uiexplore_xml.rs:859-865` returns after a failed `Event::Start` write without writing the matching `Event::End`, and without signalling failure — the walk continues and the tree is sent as `Ok`. The cancellation early-returns (`:786`, `:902`) have the same shape but are safe, because `get_all_elements_xml` re-checks the flag and sends `Err(UITreeError::Cancelled)` before the XML is ever used (`:552-556`); a write error has no such backstop. Result: a malformed `xml_dom_tree` that fails every subsequent XPath query with no explanation. Propagate the write error as a `UITreeError` instead.

### 🟡 T-14 — `get_elements_by_xpath` never retries while `get_element_by_xpath` does

`windriver.rs:937` takes `&self`, queries the snapshot once, and returns `[]` on a miss. Its singular sibling takes `&mut self` and retries with tree rebuilds until `timeout_ms`. Same input, same staleness, opposite behaviour, and the plural form has no `timeout_ms` parameter to opt in. Either give it the same retry path or document the asymmetry in `bromium.pyi` and the README.

### 🟡 T-15 — `bromium`'s coordinate hit-test ignores z-order entirely

`crates/bromium/src/rectangle.rs:12-35` picks the smallest-area rectangle containing the point across the whole element list. `crates/uiexplore/src/rectangle.rs:17-47` is the same function plus a `target_z_order` filter, which the GUI resolves externally via `WindowFromPoint` to restrict the search to the window actually under the cursor. Without it, `WinDriver.get_element_by_coordinates` can return a small control belonging to an **occluded** window that happens to sit under the cursor. Given that `sort_elements` exists specifically to order by (z-order, area), the Python-side hit-test is the one consumer ignoring the ordering the tree pays to maintain. Port the z-order parameter.

### 🟡 T-16 — Subtree merge is O(N²) in XML size and re-sorts on every merge

`append_or_replace_node_by_rt_id` (`uiexplore_xml.rs:403-443`) parses the **entire** accumulated XML string with `xot` and re-serialises it on every subtree merge, and `append_or_replace_subtree` calls `sort_elements` over the whole growing vector each time (`:345`). For N top-level windows that is N full parse/serialise cycles of a document that reaches ~60 KB, plus N sorts. Measured on the live run: parallel 1.13 s vs sequential 1.56 s — a 1.4× speedup from 10-way parallelism, most of it eaten by the merge. Previously noted in `PERF_UITREE_REPORT.md` (§P-04 discussion, line 155) and still open. Merge into a single `xot` document held across all subtrees and serialise once; sort once after all merges.

---

## What is solid

Worth recording so it is not regressed:

- **Cancellation protocol.** Every Python-side refresh retires the previous walker before spawning (`store(true)` → new flag → spawn → `recv_timeout` → `store(true)` on timeout). This correctly prevents the orphaned-thread accumulation of CF-03/F-03.
- **GIL discipline.** Every blocking walk and sleep reachable from a `#[pymethods]` entry point is wrapped in `py.allow_threads`. The two `refresh_ui_tree_*` variants that hold the GIL are internal and reached only from `launch_or_activate_app` — worth revisiting (that path can hold the GIL across 20 shallow walks plus a full one), but it is a deliberate, contained choice.
- **Cache invalidation on merge.** `append_or_replace_subtree` is the only post-construction mutator and it correctly does all three fixups — XML rewrite, `xpath_cache = None`, `rebuild_node_to_elem` — in that order.
- **Detached `Element` handles.** `Element` carries copied properties, and actions re-resolve the live `UIElement` by HWND with a runtime-ID fallback (`save_ui_element.rs:124-173`). So an `Element` captured before a refresh keeps working; tree staleness does not translate into stale COM pointers.
- **Cancelled walks never surface.** The flag is re-checked after the walk and before the send, so a partially-built tree (and its unbalanced XML) is discarded rather than delivered.

---

## Recommended order

1. **T-01** — one-line-ish fix plus a `debug_assert!` on the invariant. Removes a live panic hazard.
2. **T-03** — switch scoped refresh to `append_or_replace_subtree`; re-resolve the scoped root inside the loop. Fixes correctness *and* the case where the optimisation currently does nothing.
3. **T-06**, **T-05** — make UI Explore's refresh cancellable and bounded; stop the WinEvent backlog.
4. **T-07**, **T-08** — small consistency fixes with user-visible impact; update `README.md` / `bromium.pyi` for the depth-2 constructor.
5. **T-02**, **T-04**, **T-16** — required before the parallel walker can be used anywhere real. Until then, consider marking `get_all_elements_par_xml` `#[doc(hidden)]` or feature-gating it, so it is not adopted as a drop-in.
6. **T-09**–**T-15** — hygiene, best done alongside whichever area is next touched.

### Suggested regression tests

None of these need COM, which is why they are cheap and currently absent:

- `node_to_elem.len() == tree.node_count()` for `UITree::empty()`, `UITree::new`, and after `append_or_replace_subtree`.
- Round-trip `tree_index`: for every element, `tree.node(elem.tree_index()).runtime_id == format_runtime_id(elem.runtime_id())` — before *and* after a merge (this is the T-04 detector).
- Element/XML parity: `ui_elements.len()` equals the number of tags in `xml_dom_tree` (the T-10 detector).
- Existing `build_test_tree` should be rebuilt with **distinct, non-empty runtime IDs** so it exercises the primary rtid mapping instead of the empty-id fallback.
