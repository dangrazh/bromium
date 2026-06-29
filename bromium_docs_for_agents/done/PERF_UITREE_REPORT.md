# UITree Construction & Refresh — Performance Audit Report

**Audit date:** 2026-06-20
**Scope:** UITree construction, refresh, and XPath query hot paths across the `uitree`, `bromium`, `xmlutil`, and `bromium-common` crates
**Codebase:** ~10,500 LOC Rust workspace (7 crates), Windows UI Automation via COM

---

## Executive Summary

UITree construction is dominated by **Windows COM inter-process communication** — each property access on a `UIElement` is a cross-process RPC call. The current code makes **~10 COM calls per element** where **~5 would suffice**, and clones COM smart pointers unnecessarily. For a typical desktop with ~5,000 UI elements this translates to ~25,000 wasted COM round-trips per tree build.

Secondary costs come from redundant heap allocations (cloning `SaveUIElement` with 7 `String` fields + `Vec<i32>` for every node), an intermediate in-memory XML DOM that is immediately serialized to a string and then re-parsed for queries, and minor inefficiencies in sorting and cycle detection.

---

## Findings Index

| ID | Severity | Title | Remediation |
|----|----------|-------|-------------|
| P-01 | 🔴 High | Redundant COM IPC calls per element during tree walk | R-P01 |
| P-02 | 🔴 High | COM smart-pointer cloned into `SaveUIElement::new` then discarded | R-P02 |
| P-03 | 🟡 Medium | `SaveUIElement` fully cloned on every tree insertion | R-P03 |
| P-04 | 🟡 Medium | XML DOM built in-memory then serialized, then re-parsed for queries | R-P04 |
| P-05 | 🟢 Low-Med | `format_runtime_id` allocates intermediate `Vec<String>` on every call | R-P05 |
| P-06 | 🟢 Low | Parallel walker spawns unbounded OS threads (one per top-level window) | R-P06 |
| P-07 | 🟢 Low | Stable sort used where unstable sort suffices | R-P07 |
| P-08 | 🟢 Low | `HashSet`-based cycle detection in `for_each` on an acyclic tree | R-P08 |

---

## Findings

---

### P-01 — Redundant COM IPC calls per element during tree walk

**Severity:** 🔴 High
**Impact:** ~40–60% of all COM calls during tree construction are wasted

In each of the three tree walker functions (`get_element` in `uiexplore.rs`, `uiexplore_iter.rs`, `uiexplore_xml.rs`), every `UIElement` has its properties queried **twice**:

1. **First pass** — to build the `item` display string used as the tree node name:
   ```rust
   let runtime_id = format_runtime_id(&element.get_runtime_id().unwrap_or(...));
   let item = format!("'{}' {} ({} | {} | {})",
       element.get_name().unwrap_or_default(),           // COM call #1
       element.get_localized_control_type().unwrap_or_default(), // COM call #2
       element.get_classname().unwrap_or_default(),       // COM call #3
       element.get_framework_id().unwrap_or_default(),    // COM call #4
       runtime_id                                         // COM call #5 (get_runtime_id)
   );
   ```

2. **Second pass** — inside `SaveUIElement::from(element.clone())`, invoked by `SaveUIElement::new()`:
   ```rust
   let name = item.get_name().unwrap_or("".to_string());          // COM call #6  (duplicate!)
   let classname = item.get_classname().unwrap_or("".to_string()); // COM call #7  (duplicate!)
   let control_type = item.get_control_type()...;                  // COM call #8
   let localized_control_type = item.get_localized_control_type()...; // COM call #9  (duplicate!)
   let framework_id = item.get_framework_id()...;                 // COM call #10 (duplicate!)
   let runtime_id = item.get_runtime_id()...;                     // COM call #11 (duplicate!)
   let automation_id = item.get_automation_id()...;               // COM call #12
   let handle = item.get_native_window_handle()...;               // COM call #13
   let bounding_rect = item.get_bounding_rectangle()...;          // COM call #14
   ```

   Of these ~14 COM calls per element, **5 are exact duplicates** of calls already made for the `item` string.

Additionally, in `uiexplore_xml.rs`, the XML DOM node attributes query the COM element a **third time** for `get_name()` and `get_control_type()` (lines 878–893).

**Affected files:**

| File | Function | Lines (approx.) |
|------|----------|-----------------|
| `crates/uitree/src/uiexplore.rs` | `get_element()` | 200–222 |
| `crates/uitree/src/uiexplore.rs` | `get_all_elements()` (root setup) | 128–141 |
| `crates/uitree/src/uiexplore_iter.rs` | `get_element_iterative()` | 206–225 |
| `crates/uitree/src/uiexplore_iter.rs` | `get_all_elements_iterative()` (root setup) | 127–140 |
| `crates/uitree/src/uiexplore_xml.rs` | `get_element()` | 845–893 |
| `crates/uitree/src/uiexplore_xml.rs` | `get_all_elements_xml()` (root setup) | 441–471, 581–616 |

---

### P-02 — COM smart-pointer cloned into `SaveUIElement::new` then discarded

**Severity:** 🔴 High
**Impact:** One unnecessary `AddRef`/`Release` COM round-trip per element

Every call site passes `element.clone()` to `SaveUIElement::new()`:

```rust
let ui_elem_props = SaveUIElement::new(element.clone(), level, z_order);
```

`SaveUIElement::new` delegates to `From<UIElement>`, which extracts all scalar properties into owned `String`s and `Vec<i32>`, then **drops** the `UIElement`. The `.clone()` calls `AddRef` on the COM pointer, and the drop calls `Release` — both are COM IPC calls that serve no purpose since the `UIElement` is never stored.

**Affected files:**

| File | Lines (approx.) |
|------|-----------------|
| `crates/uitree/src/uiexplore.rs` | 137, 209–213 |
| `crates/uitree/src/uiexplore_iter.rs` | 136, 217–221 |
| `crates/uitree/src/uiexplore_xml.rs` | 450, 590, 854–858 |

---

### P-03 — `SaveUIElement` fully cloned on every tree insertion

**Severity:** 🟡 Medium
**Impact:** ~7 heap-allocated `String` copies + 1 `Vec<i32>` copy per element, twice

After constructing a `SaveUIElement`, the code clones it twice:

```rust
let ui_elem_props = SaveUIElement::new(element.clone(), level, z_order);

// Clone #1: into the tree node
let parent = tree.add_child(parent, item.as_str(), runtime_id.as_str(), ui_elem_props.clone());

// Clone #2: into the flat element list
let ui_elem_in_tree = UIElementInTree::new(ui_elem_props, parent);
ui_elements.push(ui_elem_in_tree);
```

`SaveUIElement` contains 7 `String` fields (`name`, `classname`, `control_type`, `localized_control_type`, `framework_id`, `automation_id`, `xpath`) plus a `Vec<i32>` (`runtime_id`). Each clone allocates and copies all of them. For a 5,000-element tree that's ~10,000 unnecessary string allocations.

The root cause is that the same data lives in two places: `UITreeMap<SaveUIElement>` (the tree structure) and `Vec<UIElementInTree>` (the flat sorted list). The tree stores a full `SaveUIElement` copy, but callers primarily access it through the flat list.

**Affected files:**

| File | Lines (approx.) |
|------|-----------------|
| `crates/uitree/src/uiexplore.rs` | 215–222 |
| `crates/uitree/src/uiexplore_iter.rs` | 223–225 |
| `crates/uitree/src/uiexplore_xml.rs` | 860–867 |
| `crates/uitree/src/tree_map.rs` | `add_child()` signature — takes `data: T` by value | 94 |

---

### P-04 — XML DOM built in-memory then serialized, then re-parsed for queries

**Severity:** 🟡 Medium
**Impact:** The full UI tree XML is materialized three times in different representations

The current flow in `uiexplore_xml.rs`:

1. **Build** an `XMLDomNode` tree in memory during the walk (heap-allocated nodes with `Vec<XMLDomNode>` children).
2. **Serialize** the `XMLDomNode` tree to an XML `String` via `quick_xml::Writer` (`XMLDomWriter::to_string()`).
3. **Discard** the `XMLDomNode` tree.
4. Later, **re-parse** the XML string with `roxmltree` (for XPath generation in `xpath_gen.rs`) or `xot` (for subtree merging in `append_or_replace_node_by_rt_id`).

The intermediate `XMLDomNode` representation serves no purpose beyond being serialized once. The data could be written directly to a `quick_xml::Writer` during the walk, eliminating step 1 and 3 entirely.

Additionally, `append_or_replace_node_by_rt_id` parses the *entire* XML string with `xot` on every subtree merge during parallel construction. For `N` top-level windows, that's `N` full XML parses.

**Affected files:**

| File | Function | Lines (approx.) |
|------|----------|-----------------|
| `crates/uitree/src/uiexplore_xml.rs` | `get_element()` | 869–893 (XMLDomNode construction) |
| `crates/uitree/src/uiexplore_xml.rs` | `get_all_elements_xml()` | 499–507 (serialization) |
| `crates/uitree/src/uiexplore_xml.rs` | `append_or_replace_node_by_rt_id()` | 339–369 (re-parse with xot) |
| `crates/xmlutil/src/xml.rs` | `XMLDomWriter::write_node()` | 266–282 (recursive serialization) |

---

### P-05 — `format_runtime_id` allocates intermediate `Vec<String>` on every call

**Severity:** 🟢 Low-Medium
**Impact:** One throwaway `Vec<String>` allocation per element

`format_runtime_id` is called for every element during the tree walk:

```rust
pub fn format_runtime_id(id: &[i32]) -> String {
    id.iter()
        .map(|x| x.to_string())      // allocates a String per i32
        .collect::<Vec<String>>()     // collects into a Vec (heap alloc)
        .join("-")                    // allocates the final joined String
}
```

This creates `N+1` temporary `String` allocations plus one `Vec` allocation, all of which are immediately discarded. A single `String` built via `write!` avoids all intermediate allocations.

The same pattern (`iter().map(|x| x.to_string()).collect::<Vec<String>>().join(".")`) appears in `get_xpath_raw_for_element` in both `uiexplore.rs` and `uiexplore_iter.rs` for formatting runtime IDs in XPath strings.

**Affected files:**

| File | Lines (approx.) |
|------|-----------------|
| `crates/bromium-common/src/lib.rs` | 12–20 |
| `crates/uitree/src/uiexplore.rs` | `get_xpath_raw_for_element()` lines 85–90 |
| `crates/uitree/src/uiexplore_iter.rs` | `get_xpath_raw_for_element()` lines 85–90 |

---

### P-06 — Parallel walker spawns unbounded OS threads

**Severity:** 🟢 Low
**Impact:** Thread creation overhead + COM apartment initialization per thread

`get_all_elements_par_xml` spawns one `std::thread::spawn` per top-level child element:

```rust
for element in child_elements {
    let handle = std::thread::spawn(move || {
        get_all_elements_xml(tx_par_clone, ...);
    });
    handles.push(handle);
}
```

On a busy desktop with 20+ top-level windows, this creates 20+ OS threads. Each thread must initialize a COM apartment (`CoInitializeEx`) before it can call UI Automation APIs. Thread creation and COM initialization are both non-trivial costs (~0.5–1 ms each).

**Affected files:**

| File | Lines (approx.) |
|------|-----------------|
| `crates/uitree/src/uiexplore_xml.rs` | `get_all_elements_par_xml()` lines 700–720 |

---

### P-07 — Stable sort used where unstable sort suffices

**Severity:** 🟢 Low
**Impact:** One extra allocation (temporary buffer) for the sort

After building the flat element list, the code sorts by `(z_order, bounding_rect_size)`:

```rust
ui_elements.sort_by(|a, b| {
    a.get_element_props().get_z_order().cmp(&b.get_element_props().get_z_order())
        .then(a.get_element_props().get_bounding_rect_size()
              .cmp(&b.get_element_props().get_bounding_rect_size()))
});
```

`sort_by` is a **stable** sort that allocates a temporary buffer of `O(n)` size. Since elements at the same `(z_order, bounding_rect_size)` have no meaningful relative order (they are selected by point-in-rect, not position in the list), stability is not required. `sort_unstable_by` performs the same sort in-place without the temporary allocation.

**Affected files:**

| File | Lines (approx.) |
|------|-----------------|
| `crates/uitree/src/uiexplore.rs` | 159–168 |
| `crates/uitree/src/uiexplore_iter.rs` | 160–169 |
| `crates/uitree/src/uiexplore_xml.rs` | 511–520, 651–660, 267–276 |

---

### P-08 — `HashSet`-based cycle detection in `for_each` on an acyclic tree

**Severity:** 🟢 Low
**Impact:** One `HashSet` allocation + `N` hash-and-insert operations per traversal

`UITreeMap::for_each` and `debug_tree_map` allocate a `HashSet<usize>` and insert/check every node index:

```rust
pub fn for_each<F>(&self, mut callback: F) where F: FnMut(usize, &T) {
    let mut visited = UIHashSet::new();
    self.for_each_recursive(self.root(), &mut callback, &mut visited);
}
```

`UITreeMap` is a well-formed tree by construction: `add_child` only creates forward references from parent to child, and `remove_node` cleans up properly. Cycles cannot exist. The `HashSet` adds a hash computation plus a memory allocation for every traversal.

A simple depth limit (e.g., bail at depth > `nodes.len()`) would provide the same safety guarantee at zero allocation cost, or the cycle check can be removed entirely with a `debug_assert` guarding the invariant.

**Affected files:**

| File | Lines (approx.) |
|------|-----------------|
| `crates/uitree/src/tree_map.rs` | `for_each()` 201–207, `for_each_recursive()` 210–225 |
| `crates/uitree/src/tree_map.rs` | `debug_tree_map()` 227–249 |
| `crates/uitree/src/tree_map.rs` | `debug_with()` / `debug_fmt_node_with()` 251–291 |

---

## Remediation Plan

**Goal:** Reduce UITree construction time by eliminating redundant COM calls, unnecessary allocations, and intermediate data representations.
**Validation:** `cargo build` + `cargo test` + `cargo clippy` clean. No public API signature changes in the `bromium` PyO3 crate. Measured improvement via before/after timing of `refresh_ui_tree()` on a representative desktop.
**Constraint:** Changes are internal to the Rust workspace. The Python-facing API (`bromium` crate) must remain unchanged.

Remediation items are ordered by **expected performance gain** (highest first).

---

### R-P01 — Eliminate redundant COM calls by building `SaveUIElement` first

**Linked finding:** P-01
**Expected gain:** 🔴 High — eliminates ~40–60% of all COM calls during tree construction

In all three walker modules and their root-setup code, reverse the order of operations: construct `SaveUIElement` from the `UIElement` **first**, then build the `item` display string and XML attributes from the already-extracted fields on `SaveUIElement`.

**Before (current pattern):**

```rust
// ❌ Queries COM 5 times for the display string
let runtime_id = format_runtime_id(&element.get_runtime_id().unwrap_or(vec![0, 0, 0, 0]));
let item = format!("'{}' {} ({} | {} | {})",
    element.get_name().unwrap_or_default(),
    element.get_localized_control_type().unwrap_or_default(),
    element.get_classname().unwrap_or_default(),
    element.get_framework_id().unwrap_or_default(),
    runtime_id
);
// ❌ Queries COM ~9 more times inside SaveUIElement::from()
let ui_elem_props = SaveUIElement::new(element.clone(), level, z_order);
```

**After (remediated):**

```rust
// ✅ All COM calls happen once inside SaveUIElement::new()
let ui_elem_props = SaveUIElement::new(&element, level, z_order);
let runtime_id = format_runtime_id(ui_elem_props.get_runtime_id());
let item = format!("'{}' {} ({} | {} | {})",
    ui_elem_props.get_name(),
    ui_elem_props.get_localized_control_type(),
    ui_elem_props.get_classname(),
    ui_elem_props.get_framework_id(),
    runtime_id
);
```

For the XML variant (`uiexplore_xml.rs`), also use `SaveUIElement` fields when setting XML attributes instead of querying the COM element again:

```rust
// ✅ Use already-extracted values for XML attributes
curr_xml_dom_node.set_attribute("Name", ui_elem_props.get_name());
curr_xml_dom_node.set_attribute("ControlType", ui_elem_props.get_control_type());
```

**Files to modify:**

| File | Scope |
|------|-------|
| `crates/uitree/src/uiexplore.rs` | `get_all_elements()` root setup + `get_element()` |
| `crates/uitree/src/uiexplore_iter.rs` | `get_all_elements_iterative()` root setup + `get_element_iterative()` |
| `crates/uitree/src/uiexplore_xml.rs` | `get_all_elements_xml()` root setup (both code paths) + `get_all_elements_par_xml()` root setup + `get_element()` |

---

### R-P02 — Accept `&UIElement` in `SaveUIElement::new` instead of consuming a clone

**Linked finding:** P-02
**Expected gain:** 🔴 High — eliminates one COM `AddRef`/`Release` round-trip per element

Change `SaveUIElement::new` and the `From<UIElement>` implementation to accept a `&UIElement` reference. Since `SaveUIElement` extracts all properties into owned types and never stores the `UIElement`, it has no need for ownership.

**Steps:**

1. Replace `impl From<UIElement> for SaveUIElement` with a private constructor that takes `&UIElement`:

   ```rust
   impl SaveUIElement {
       pub fn new(element: &UIElement, level: usize, z_order: usize) -> Self {
           let name = element.get_name().unwrap_or_default();
           let classname = element.get_classname().unwrap_or_default();
           // ... all other property extractions using &element ...
           SaveUIElement {
               name, classname, /* ... */
               level, z_order,
               xpath: None,
           }
       }
   }
   ```

2. Remove or deprecate `impl From<UIElement>` — it is not used outside `new()`.

3. Update all call sites from `SaveUIElement::new(element.clone(), ...)` to `SaveUIElement::new(&element, ...)`.

**Files to modify:**

| File | Change |
|------|--------|
| `crates/uitree/src/save_ui_element.rs` | Rewrite `new()` and `From<UIElement>` to `fn new(&UIElement, ...)` |
| `crates/uitree/src/uiexplore.rs` | `element.clone()` → `&element` (2 sites) |
| `crates/uitree/src/uiexplore_iter.rs` | `element.clone()` → `&element` (2 sites) |
| `crates/uitree/src/uiexplore_xml.rs` | `element.clone()` → `&element` (3 sites) + `root.clone()` → `&root` (2 sites) |

---

### R-P03 — Store tree indices instead of full `SaveUIElement` copies in `UITreeMap` ✅ DONE

**Linked finding:** P-03
**Expected gain:** 🟡 Medium — eliminates ~10,000 heap-allocated string copies per tree build
**Status:** ✅ Implemented 2026-06-21

**Approach taken:** Kept `UITreeMap<T>` generic but changed the type parameter from `SaveUIElement` to `()` in all three `UITree` wrappers. The tree stores no element data at all — only structural fields (name, runtime_id, index, parent, children). A `node_to_elem: Vec<usize>` index maps tree node indices to positions in the `ui_elements` vector, rebuilt via runtime_id matching after each sort or subtree merge.

**Key changes:**

1. All three `UITree` structs (`uiexplore.rs`, `uiexplore_iter.rs`, `uiexplore_xml.rs`) changed from `UITreeMap<SaveUIElement>` to `UITreeMap<()>` with an added `node_to_elem: Vec<usize>` field.

2. Walker functions pass `()` to `tree.add_child()` — zero `SaveUIElement` clones into the tree.

3. In the XML walker, reordered operations so XML attribute reads happen before moving `ui_elem_props` into `UIElementInTree`, eliminating **both** clones per element (tree clone + UIElementInTree clone).

4. Methods like `node()`, `for_each()`, `get_element_by_xpath()` use `node_to_elem` to look up `SaveUIElement` from the flat vector.

5. `append_or_replace_subtree` and `append_children` simplified: use `node.runtime_id` directly from the tree (already stored) instead of extracting from `SaveUIElement`, pass `()` to `add_child`. Index rebuilt after subtree merge via `rebuild_node_to_elem()`.

6. `tree_map.rs` unchanged — generic `UITreeMap<T>` preserved, tests still use `UITreeMap<String>`.

---

### R-P04 — Stream XML directly during tree walk, skip intermediate DOM

**Linked finding:** P-04
**Expected gain:** 🟡 Medium — eliminates one full in-memory tree representation

Replace the `XMLDomNode` / `XMLDomWriter` in-memory tree with a direct `quick_xml::Writer` that streams XML events as elements are visited during the tree walk.

**Steps:**

1. In `get_all_elements_xml` and `get_element`, replace `XMLDomNode` construction with `quick_xml::Writer` calls:
   ```rust
   // Instead of:
   let curr_xml_dom_node = xml_dom_node.add_child(XMLDomNode::new(control_type_tag));
   curr_xml_dom_node.set_attribute("RtID", runtime_id.as_str());
   // ...children visited...
   
   // Use:
   let mut start = BytesStart::new(control_type_tag);
   start.push_attribute(("RtID", runtime_id.as_str()));
   start.push_attribute(("Name", ui_elem_props.get_name().as_str()));
   start.push_attribute(("ControlType", ui_elem_props.get_control_type().as_str()));
   writer.write_event(Event::Start(start))?;
   // ...children visited...
   writer.write_event(Event::End(BytesEnd::new(control_type_tag)))?;
   ```

2. Pass a `&mut Writer<Cursor<Vec<u8>>>` through the recursive `get_element` calls instead of `&mut XMLDomNode`.

3. After the walk completes, call `writer.into_inner().into_inner()` → `String::from_utf8(...)` to get the XML string.

4. Remove `XMLDomNode` and `XMLDomWriter` from `xmlutil/src/xml.rs` if no longer used elsewhere (verify with `grep`).

**Files to modify:**

| File | Change |
|------|--------|
| `crates/uitree/src/uiexplore_xml.rs` | Replace `XMLDomNode` / `XMLDomWriter` usage with direct `quick_xml::Writer` |
| `crates/xmlutil/src/xml.rs` | Remove `XMLDomNode`, `XMLDomWriter` if unused |

**Note:** This is a significant refactor of `uiexplore_xml.rs`. The recursive `get_element` function's signature changes from `xml_dom_node: &mut XMLDomNode` to `writer: &mut Writer<Cursor<Vec<u8>>>`. The `End` events must be written after all children are visited, matching the current implicit behavior of `XMLDomWriter::write_node`. Consider implementing after R-P01 and R-P02.

---

### R-P05 — Optimize `format_runtime_id` to avoid intermediate allocations

**Linked finding:** P-05
**Expected gain:** 🟢 Low-Medium — eliminates `N+1` throwaway string allocations per call

**Steps:**

1. Rewrite `format_runtime_id` in `bromium-common/src/lib.rs`:

   ```rust
   pub fn format_runtime_id(id: &[i32]) -> String {
       use std::fmt::Write;
       if id.is_empty() {
           return "0-0-0-0".to_string();
       }
       // Typical runtime IDs are 4 ints, ~3 digits each → ~15 chars
       let mut s = String::with_capacity(id.len() * 5);
       for (i, val) in id.iter().enumerate() {
           if i > 0 {
               s.push('-');
           }
           let _ = write!(s, "{}", val);
       }
       s
   }
   ```

2. Apply the same pattern to the runtime ID formatting in `get_xpath_raw_for_element` (both `uiexplore.rs` and `uiexplore_iter.rs`):

   ```rust
   // Before:
   let runtime_id = ui_elem_props.get_runtime_id()
       .iter().map(|x| x.to_string()).collect::<Vec<String>>().join(".");
   
   // After:
   let runtime_id = {
       use std::fmt::Write;
       let id = ui_elem_props.get_runtime_id();
       let mut s = String::with_capacity(id.len() * 5);
       for (i, val) in id.iter().enumerate() {
           if i > 0 { s.push('.'); }
           let _ = write!(s, "{}", val);
       }
       s
   };
   ```

   Or extract a shared helper `fn join_runtime_id(id: &[i32], sep: char) -> String` in `bromium-common`.

**Files to modify:**

| File | Change |
|------|--------|
| `crates/bromium-common/src/lib.rs` | Rewrite `format_runtime_id` |
| `crates/uitree/src/uiexplore.rs` | Update `get_xpath_raw_for_element` |
| `crates/uitree/src/uiexplore_iter.rs` | Update `get_xpath_raw_for_element` |

---

### R-P06 — Use a bounded thread pool for parallel tree construction

**Linked finding:** P-06
**Expected gain:** 🟢 Low — reduces thread creation + COM apartment initialization overhead

**Steps:**

1. Determine the available parallelism: `std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4)`.

2. Replace the per-element `thread::spawn` loop with a simple work-stealing pattern using a shared `Mutex<Vec<UIElement>>` work queue and a fixed number of worker threads. Each worker loops: lock → pop element → unlock → process → send result.

   Alternatively, use a scoped thread pool via `std::thread::scope` (available since Rust 1.63) to avoid the `Arc` overhead:

   ```rust
   let (tx_par, rx_par) = channel();
   let max_threads = std::thread::available_parallelism()
       .map(|n| n.get()).unwrap_or(4);
   
   // Process in batches of max_threads
   for chunk in child_elements.chunks(max_threads) {
       std::thread::scope(|s| {
           for element in chunk {
               let tx = tx_par.clone();
               s.spawn(move || {
                   get_all_elements_xml(tx, Some(element.clone()), ...);
               });
           }
       });
   }
   ```

**Files to modify:**

| File | Change |
|------|--------|
| `crates/uitree/src/uiexplore_xml.rs` | `get_all_elements_par_xml()` lines 700–720 |

**Note:** COM threading constraints may limit the actual parallelism benefit. Each thread must enter an STA or MTA apartment. Verify that the `uiautomation` crate handles this correctly when called from worker threads. Consider benchmarking with `available_parallelism` vs. the current unbounded approach before committing.

---

### R-P07 — Use `sort_unstable_by` instead of `sort_by`

**Linked finding:** P-07
**Expected gain:** 🟢 Low — eliminates one `O(n)` temporary allocation per sort

Replace all `ui_elements.sort_by(...)` calls with `ui_elements.sort_unstable_by(...)`. The sort key `(z_order, bounding_rect_size)` has no meaningful relative ordering for equal keys, so stability is not required.

**Steps:**

1. Global search-and-replace `sort_by(|a, b|` → `sort_unstable_by(|a, b|` in the following locations:

**Files to modify:**

| File | Lines (approx.) |
|------|-----------------|
| `crates/uitree/src/uiexplore.rs` | 159 |
| `crates/uitree/src/uiexplore_iter.rs` | 160 |
| `crates/uitree/src/uiexplore_xml.rs` | 511, 651, 267 |

---

### R-P08 — Remove `HashSet` cycle detection from `for_each` and debug methods

**Linked finding:** P-08
**Expected gain:** 🟢 Low — eliminates one `HashSet` allocation + `N` hash operations per traversal

Replace the `HashSet<usize>` visited set with either:

**(a) No protection** — `UITreeMap` is acyclic by construction. Add a `debug_assert!` that the recursion depth doesn't exceed `self.nodes.len()`:

```rust
fn for_each_recursive<F>(&self, index: usize, callback: &mut F, depth: usize)
where F: FnMut(usize, &T)
{
    debug_assert!(depth <= self.nodes.len(), "cycle detected in UITreeMap");
    let node = &self.nodes[index];
    callback(index, &node.data);
    for &child in &node.children {
        self.for_each_recursive(child, callback, depth + 1);
    }
}
```

**(b) Depth-only guard** — pass a `depth: usize` counter instead of a `HashSet`, bail if `depth > nodes.len()`:

```rust
fn for_each_recursive<F>(&self, index: usize, callback: &mut F, depth: usize)
where F: FnMut(usize, &T)
{
    if depth > self.nodes.len() {
        log::error!("Possible cycle at node {}, aborting traversal", index);
        return;
    }
    // ...
}
```

Apply the same change to `debug_tree_map` and `debug_fmt_node_with`.

**Files to modify:**

| File | Functions |
|------|-----------|
| `crates/uitree/src/tree_map.rs` | `for_each()`, `for_each_recursive()`, `debug_tree_map()`, `debug_with()`, `debug_fmt_node_with()` |

---

## Remediation Summary (ordered by performance gain)

| Priority | ID | Title | Gain | Effort | Linked Finding |
|----------|----|-------|------|--------|----------------|
| 1 | R-P01 ✅ | Eliminate redundant COM calls — build `SaveUIElement` first | 🔴 High | Low | P-01 |
| 2 | R-P02 ✅ | Accept `&UIElement` — avoid COM `AddRef`/`Release` clone | 🔴 High | Low | P-02 |
| 3 | R-P03 ✅ | Store indices in tree, not full `SaveUIElement` clones | 🟡 Medium | Medium | P-03 |
| 4 | R-P04 ✅ | Stream XML directly during walk, skip `XMLDomNode` intermediate | 🟡 Medium | High | P-04 |
| 5 | R-P05 ✅ | Optimize `format_runtime_id` — single `String` via `write!` | 🟢 Low-Med | Low | P-05 |
| 6 | R-P06 | Bounded thread pool for parallel walker (see footnote...) | 🟢 Low | Medium | P-06 |
| 7 | R-P07 ✅ | Use `sort_unstable_by` instead of `sort_by` | 🟢 Low | Trivial | P-07 |
| 8 | R-P08 ✅ | Remove `HashSet` cycle detection in `for_each` | 🟢 Low | Trivial | P-08 |

**Recommended implementation order:** R-P01 → R-P02 (both low-effort, high-gain, no structural changes), then R-P05 + R-P07 + R-P08 (quick wins), then R-P03 (medium effort, medium gain), and finally R-P04 + R-P06 (larger refactors, evaluate with benchmarks).

**Implementation footnote:** R-P06 is only called in uitree/src/main.rs (a test/dev binary), never from the bromium PyO3 crate that ships to users. The production code in bromium/src/windriver.rs  exclusively uses get_all_elements_xml. R-P06 would optimize dead code from a shipping perspective — probably not worth the effort. Hence this is not implemented for now.
