# Tree state audit — 2026-09-06

Audited the current working files, including existing uncommitted changes. No implementation changes made. Earlier audit/remediation documents were treated as leads, not proof. Findings below are from source inspection; this audit did not run an interactive desktop experiment.

The project does not currently guarantee fresh answers when queried. It holds snapshots with no age, generation, completeness, or freshness policy. Fixing refresh triggers alone will not suffice: the representations within a snapshot can also disagree.

**User constraint confirmed after this audit:** capture latency on target systems rules out fresh capture before every query. Use the cached tree and incrementally acquire the narrowest sufficient surface. This supersedes the fresh-by-default and full-replacement recommendations below; the correctness findings remain applicable. The revised direction is in [TREE_INCREMENTAL_DESIGN.md](TREE_INCREMENTAL_DESIGN.md). Queries wait for relevant dirty coverage within one query deadline and explicitly report if it remains stale.

## Findings, in priority order

### 1. High: successful queries can return indefinitely stale matches

`crates/bromium/src/windriver.rs:817` queries the saved tree first; refresh happens only on a miss with a positive timeout. Close or rename a matching window after capture and the old XPath can still succeed. Coordinates, plural XPath, filtering, membership, iteration, and counts also use the saved snapshot (`:663`, `:681`, `:690`, `:717`, `:767`, `:936`). Returned `Element` properties are copied values and do not track subsequent changes.

The constructor builds only depth 2 (`:578`), so a new driver is also incomplete for deeper queries. Refresh failure retains the previous tree without recording its degraded status. `launch_or_activate_application` (`crates/bromium/src/app_control.rs:36`) makes its launch-versus-activate decision from a snapshot: a stale hit can fail activation, while a stale miss can launch an already-running app.

Recommendation: define fresh query and explicit snapshot APIs. All fresh queries, including successful matches and collection queries, must pass through one acquisition policy. Retry-on-miss is a separate concern. Keep shallow launch discovery private instead of publishing it as the driver's current complete tree.

### 2. High: a lookup changes the meaning of the driver's tree

`windriver.rs:837` resolves a scoped window from the XPath; `:871` replaces the entire desktop snapshot with that window's subtree. Absolute desktop-rooted XPath expressions then fail; subsequent unrelated queries silently operate on a different scope. The scope hint is resolved once before the retry loop, so a newly appearing window is not reconsidered.

The filter has similar divergence: the setter changes the configured title without rebuilding (`:675`); refresh uses an override without persisting it (`:1091`); shallow launch polling rebuilds without any title filter (`:1201`). Configuration and actual captured scope can therefore disagree.

Recommendation: keep declared scope stable and record it on every snapshot. Initially use full replacement within that scope. Do not adopt the existing merge method as a direct fix without addressing finding 3.

### 3. High for merge consumers: subtree replacement breaks multiple invariants

`crates/uitree/src/uiexplore_xml.rs:299` has four independent problems:

- **Removed descendants survive in the element list.** The arena removes the entire old subtree, but `remove_in_place` (`:394`) removes only IDs present in the incoming subtree. Replace `Window -> [A, B]` with `Window -> [A]`: B disappears from XML/arena but remains in `get_elements()`, filtering, counting, and coordinate candidates.
- **Element indices are not remapped.** Appended nodes receive new indices (`:336`, `:385`), but incoming `UIElementInTree.tree_index` values retain their old indices. Coordinate-to-XPath resolution can identify the wrong node.
- **Failure is not atomic.** Arena and element mutations precede XML parsing/merging (`:350`). An XML error returns after those mutations but before rebuilding `node_to_elem` or invalidating the old XPath cache.
- **Parent semantics differ.** The arena inserts under `parent_index`; XML appends under the document element for a new subtree (`:437`). For an existing subtree XML preserves its old location, while the arena can relocate it. A non-root parent therefore need not agree between representations.

Current production Python/GUI paths do not call this method; the public parallel walker does. Severity becomes immediate if the earlier remediation's proposed scoped merge is adopted.

Recommendation: prefer immutable replacement now. If incremental replacement is retained, construct a candidate, remove the old subtree's complete membership, remap indices, preserve identical parent/order semantics, validate, then publish once.

### 4. High: refresh can publish incomplete or misleading data as success

The walker treats every `get_first_child`/`get_next_sibling` error as traversal termination (`uiexplore_xml.rs:539`, `:870`, `:893`). It cannot distinguish normal end-of-children from a provider failure. The sibling cap only logs a warning. A leaf root skips XML emission altogether because entry into the walk is conditional on finding a first child.

`SaveUIElement::new` (`crates/uitree/src/save_ui_element.rs:29`) replaces property errors with empty names/types/runtime IDs and zero rectangles. These defaults look like actual data. Empty runtime IDs also collide in the arena's ID index; XPath resolution cannot reliably identify those nodes.

Recommendation: distinguish absent values, failed reads, and traversal completion. Return capture diagnostics and completeness with the snapshot. A fresh query must not report a definitive absence from a failed or truncated relevant traversal. Emit the root even when it has no children.

### 5. Medium: GUI refresh can stall, accumulate work, or be starved

`crates/uiexplore/src/app_ui.rs:1030` spawns refresh without a deadline or cancellation token. `:1063` treats a disconnected channel exactly like an empty channel, waiting forever after a worker exits without sending. During refreshing there is no dedicated repaint schedule or worker completion wakeup; after the five-second status expires, an idle GUI may not collect a completed result until another UI event arrives.

Auto-refresh drains events only in Normal mode, with tracking off, after the delay (`:1016`). Pointer movement resets that delay (`:481`), so continued movement can defer refreshing indefinitely; cursor tracking disables it entirely. `WinEventMonitor` uses an unbounded channel even when auto-refresh is off (`crates/winevent-monitor/src/winevent.rs:112`).

The manual refresh button remains available when auto-refresh is disabled (`app_ui.rs:539`), including while a build is running. Repeated clicks can abandon receivers and spawn more uncancelled workers. This corrects the earlier audit's assertion that the refresh button is unavailable during refresh.

Recommendation: one refresh controller with idle/building/failed state, deadline, cancellation, generation, and completion wakeup. Coalesce events into bounded dirty state. Separate actual capture time from interaction debounce. A cancellation flag is cooperative; it does not interrupt a COM call already blocked inside a provider.

### 6. Medium: XPath results and highlights can belong to different snapshots

Successful refresh replaces `ui_tree` and resets `TreeState` (`app_ui.rs:1042`), but retains `xpath_eval_result` and `xpath_highlighting`. The results panel displays the stored result (`:773`), while highlighting queries the new tree using the current input (`:836`). Editing the input without pressing Enter can also change the highlighted query without changing displayed results. A failed lookup does not explicitly clear the existing border here.

Recommendation: store evaluated expression, snapshot generation, and result together. Invalidate or re-evaluate on replacement. Clear selection/highlighting when the target disappears, and resolve selections by identity within the current generation.

### 7. High for coordinate accuracy: smallest rectangle can belong to a covered window

`crates/bromium/src/rectangle.rs:12` chooses the smallest containing rectangle across all saved windows. A background application's small control can beat the foreground window under the pointer, even immediately after refresh. UI Explore already uses `WindowFromPoint`/`GetAncestor` to narrow candidates (`app_ui.rs:949`), demonstrating divergent implementations.

Recommendation: share a live point-resolution path, with explicit handling for scope and unavailable targets. Resolve the live target/window before consulting snapshot geometry. Sorting alone cannot repair unrestricted hit testing.

### 8. Medium: empty and ordinary trees do not share a coherent root model

`UITree::empty()` (`uiexplore_xml.rs:46`) creates one live arena node but no elements or mapping. `node(0)`, `for_each`, and pretty printing index empty vectors and panic.

Normal construction inserts a root element/node, then `get_element` inserts the same root again as a child (`:532`, `:842`). The XML contains only the walked copy. Runtime-ID mapping overwrites the first root's index, while element counts include both copies. Missing/dead mappings default to element position zero (`:89`), potentially aliasing unrelated properties.

Public `get_tree_mut` and `get_elements_mut` allow callers to invalidate these relationships without rebuilding the XML or indices.

Recommendation: exactly one canonical root, a valid explicitly empty state, checked node access, and no independently mutable derived collections. Store element properties directly in canonical nodes; maintain sorted node IDs as a derived view if needed.

### 9. Medium, currently outside normal Python/GUI use: parallel walking changes structure metadata

`get_all_elements_par_xml` calls the sequential builder with each window as level zero (`uiexplore_xml.rs:696`). That window gets z-order 999; its children receive independent sibling orders (`:838`, `:906`) instead of inheriting desktop window order. Depth limits also become relative to each window. Completion-order merging further changes arena child order while replacement XML keeps existing order. Merge errors are logged and the final result is still sent as success (`:746`).

Recommendation: use one builder until equivalence is proven. Parallelize only independent capture work with explicit base depth/window ordering and one deterministic finalization step.

### 10. Medium: lookup errors and time limits are misleading

`UITree` lookup inspects result count without propagating XPath evaluation errors (`uiexplore_xml.rs:232`, `:266`). Malformed expressions become absence, and singular lookup may rebuild repeatedly for an expression that cannot succeed.

`windriver.rs:835` allows each tree build the full tree timeout, independently of the remaining lookup timeout. A lookup requested for 500 ms can wait up to the default 120 seconds for one build, plus other work. Scoped-root resolution also happens outside that worker wait.

Recommendation: typed query errors and one absolute deadline propagated through resolution, capture, retries, and sleep. Distinguish timeout, incomplete capture, invalid XPath, and genuine no-match.

## Simplification direction

Use one immutable `TreeSnapshot` owning canonical nodes and properties. Derive XML, runtime-ID lookup, hit-test ordering, and XPath cache from that generation. Attach scope, depth/view, capture start/end, completeness, and diagnostics. A separate controller owns refresh jobs, invalidation, deadlines, and the last successful snapshot.

Keep UI-only state small: selected identity, expanded identities, and the last evaluated query with its generation. Derive selected properties and ancestor paths from the current snapshot. A full replacement naturally clears derived caches and avoids merge tombstones and index remapping.

For fresh calls, acquire the declared query scope before evaluating, even if an old snapshot has a match. For repeated reads, expose an explicit snapshot object so callers can intentionally amortize capture costs. Keep public desktop scope stable: a cross-window query cannot claim desktop freshness after refreshing only one window. Revalidate action targets immediately before use; Python actions already resolve by runtime ID (`windriver.rs:465`), but do not refresh the driver's snapshot afterward.

Event notifications should mark state dirty and improve responsiveness, not serve as proof of freshness. Microsoft documents that not all property changes raise events. The local hook also does not subscribe to name/value/reorder events. See [Microsoft's properties/events documentation](https://learn.microsoft.com/en-us/dotnet/framework/ui-automation/ui-automation-properties-overview).

The attainable contract is a recent, internally consistent observation with an explicit scope and capture interval. A desktop changes while it is being traversed; a completed walk is not an atomic snapshot of every application. This project uses control view, which is a filtered view of provider-exposed UI, not every native window or pixel. See [Microsoft's UI Automation tree overview](https://learn.microsoft.com/en-us/dotnet/framework/ui-automation/ui-automation-tree-overview).

## Verification and next decisions

Executed `cargo test -p uitree -p xmlutil --lib --locked`: 47 passed. Existing UITree fixtures use default properties with empty runtime IDs (`uiexplore_xml.rs:964`), so they mainly exercise the index fallback; they do not establish realistic identity consistency or refresh correctness. No new tests or implementation files were added.

Before shipping fixes, cover: stale positive and negative queries; removal during replacement; invalid merge rollback; empty and leaf roots; real distinct runtime IDs; identical XML/arena membership and ancestry; delayed/disconnected workers; events arriving during capture; XPath results after replacement; overlapping windows; and deadline expiry. Use controlled provider/fake capture tests for lifecycle cases and a small interactive Windows fixture for live identity and overlap behavior.

Pending user choice: fresh acquisition on every query versus explicitly aged cached snapshots. Also clarify whether the target is the existing UIA control view or native window inventory, and whether a title passed to refresh should persist. Recommended default for automation is fresh acquisition with an explicit snapshot API for batching; preserve control view unless broader coverage is required. No implementation decision has been applied.

The Rust skill informed the recommendation to centralize ownership, use typed lifecycle/error states, and derive UI state instead of keeping independently mutable copies.
