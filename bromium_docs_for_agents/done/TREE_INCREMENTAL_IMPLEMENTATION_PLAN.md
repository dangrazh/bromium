# Incremental UI tree implementation plan

Status: implementation and local integration validation delivered. See
[implementation status](TREE_INCREMENTAL_IMPLEMENTATION_STATUS.md) for tested
behavior, implementation choices, and the remaining target-system measurement gate.

Based on [the current audit](TREE_STATE_AUDIT_2026-09-06.md) and [the agreed design direction](TREE_INCREMENTAL_DESIGN.md). This plan supersedes the earlier remediation plan's sequencing and whole-tree refresh proposals for this work. Existing unrelated work remains out of scope.

## Agreed behavior

- Retain a cached desktop tree. Routine updates acquire the narrowest sufficient surface: selected properties, immediate children, geometry, or an affected subtree.
- Queries that intersect known dirty or missing relevant coverage wait for incremental repair within one query deadline. If that deadline expires, explicitly report stale/incomplete coverage. Never disguise it as a definitive no-match.
- Keep desktop scope and ancestry stable across captures. No routine full-desktop descendant traversal as a hidden fallback.
- Publish internally consistent revisions atomically. Unaffected cached branches survive an update.
- UI rendering remains responsive while capture or queries are pending.

Freshness means no known outstanding invalidation for the relevant observed coverage, under a stated validation policy. It does not promise a simultaneous snapshot of every provider or detection of every unreported change. Capture and validation times belong to individual coverage, not only the whole tree.

## Confirmed API decisions and implementation defaults

The user confirmed the first two API decisions on 2026-09-06: stale queries raise `StaleTreeError`, and successful refresh title overrides persist. No further confirmation is needed for these behaviors. The other rows are starting defaults to validate with measurements.

| Topic | Decision / default |
|---|---|
| Reporting expired dirty queries | Existing lookup methods raise a dedicated `StaleTreeError` extending `TimeoutError`, containing reason, scope, revision, and coverage status. An explicit snapshot API permits inspecting cached results/status without waiting. A clean completed search with no result retains existing no-match behavior. |
| Persistent scope | Supplying a title to `refresh(window_title=...)` persists that scope on successful completion. Scope setters mark the requested view pending; they must not label the previous scope as refreshed. Explicit scope clearing remains supported through the setter. |
| Startup | Acquire desktop/root membership shallowly, mark deeper coverage unobserved, and fill requested branches incrementally. Do not publish shallow coverage as a complete desktop. |
| Coverage/view | Keep the current UIA control view. Maintain native window metadata separately for live point resolution and stacking. |
| Scheduling | Start with bounded capture execution and one owner for commits; tune worker count, debounce, age thresholds, and background budgets from target-system measurements. |
| Manual refresh | Retain an explicitly requested broad refresh for recovery/diagnostics if desired, but ordinary queries and auto-refresh never invoke it implicitly. Add a scoped refresh entry point. |

## Architecture and file ownership

Keep the shared tree service in `uitree` initially; avoid introducing a new crate until a dependency boundary requires it. `uitree` may depend on `winevent-monitor` through an event adapter; `winevent-monitor` must remain independent of the tree and Python/egui. Both consumers own service handles rather than independent refresh implementations. This is shared implementation, not a cross-process shared desktop cache.

| Area | Intended responsibility |
|---|---|
| `uitree/src/tree_map.rs`, new `model.rs` | Canonical nodes, generation-aware internal IDs, relationships, property validity, coverage, revision |
| New `uitree/src/patch.rs` | Validated property, children, and subtree transactions |
| `uitree/src/uiexplore_xml.rs`, `xmlutil` | Compatibility adapters and derived XML/XPath views of a committed revision |
| New `uitree/src/capture.rs` | Capture interface and UIA implementation; batched property reads and bounded scope |
| New `uitree/src/service.rs`, `invalidation.rs` | Ownership, dirty epochs, scheduling, deadlines, query coordination, lifecycle |
| `winevent-monitor/src/winevent.rs`, tree event adapter | Preserve native event identity; map native/UIA invalidations to cached coverage |
| `bromium/src/windriver.rs`, `app_control.rs`, `exceptions.rs`, `bromium.pyi` | Python contract, service integration, application discovery and action invalidation |
| `uiexplore/src/app_ui.rs`, `rectangle.rs`, startup code | Nonblocking service integration, selection/result revisions, status, point targeting |

Use owned capture data across threads. Keep UIA interfaces in their correctly initialized worker context unless their transfer is explicitly supported. Keep the non-`Send` XPath document cache on its owner thread, or construct a reader-local cache from serialized data. Do not carry forward an unsupported `unsafe impl Send` merely to make the new service compile.

## Phase 0 — Establish a measurable baseline and test seam

**Dependencies:** none. **Outcome:** reproducible correctness failures and capture-cost measurements.

1. Add realistic synthetic tree builders with distinct nonempty runtime IDs, properties, parentage, and explicit incomplete coverage. Keep them independent of COM.
2. Define a small capture interface with requests for properties, immediate children, subtree, and geometry, returning owned data plus completion/diagnostic information. Implement a recording fake and controllable clock for tests.
3. Add regression cases for duplicate root, empty tree access, removed descendants, wrong index remapping, mismatched XML ancestry, failure rollback, and stale-positive lookup. Tests exposing current failures should land with their corresponding fix rather than leave main failing.
4. Instrument acquisition separately from local work: scope, nodes read, property bundles, capture time, queue wait, patch time, XML/XPath rebuild time, and lookup time. Avoid recording UI text by default.
5. Establish a repeatable Windows fixture for window creation/removal, nested controls, rename, reorder, move/resize, and overlapping windows. A fake provider covers errors, delays, and blocked calls deterministically.

**Acceptance:** fake capture asserts exact requested scope and call count; target-system measurement procedure is documented; baseline unit tests still run. No absolute latency target is invented before measurements exist.

## Phase 1 — Make cached revisions structurally sound

**Dependencies:** Phase 0. **Outcome:** one authoritative node/property model.

1. Store properties on canonical nodes. Remove duplicated runtime-ID ownership and mutable parallel element storage where practical. Sorted iteration/hit-test views contain node IDs rather than copied properties.
2. Represent empty/uninitialized state explicitly and create exactly one desktop root when initialized. Checked access rejects missing/dead IDs. If arena slots are reused, use generations so old references cannot address replacement nodes.
3. Index only valid runtime IDs. Inspect the raw ID before formatting: `format_runtime_id([])` currently produces `0-0-0-0`, so testing the formatted string for emptiness does not prevent collisions. Nodes with unreadable identity need explicit unresolved status, not a fabricated shared identity.
4. Separate native HWND identity hints, owning window, structural depth, and native stacking from capture-local traversal depth.
5. Make XML, XPath cache, counts, iteration, and node lookup derive from the same revision. Remove public raw mutation paths or restrict them behind invariant-preserving methods.
6. Use adapters to keep existing callers compiling while migrating. Do not maintain two independently writable tree models.

**Acceptance:** live canonical nodes and XML represent the same membership and ancestry; root count is one; empty state is safe; invalid IDs cannot alias another node; property and child ordering are deterministic. Local projection rebuilding performs no provider acquisition.

## Phase 2 — Implement atomic incremental patches

**Dependencies:** Phase 1. **Outcome:** reliable small updates before they enter query paths.

Implement three core operations plus geometry/window metadata updates:

- `UpdateProperties`: patches only validated properties on the expected identity and marks affected dependent coverage appropriately.
- `ReconcileChildren`: consumes a complete immediate-child observation, establishes membership/order, retains surviving cached descendants, removes confirmed departed branches, and marks new descendants unobserved until acquired.
- `ReplaceSubtree`: replaces the target's complete membership while preserving its external parent and position; never re-roots the desktop or carries capture-local node indices into the live arena.

Validate target identity, expected structural context, duplicate IDs, cycles, and completeness before publication. Stage changes so any validation/projection failure leaves the previous revision intact. A partial enumeration cannot delete unobserved siblings or certify completion; default to retaining the prior region with failed/dirty coverage.

Handle reparenting as one coordinated transaction where both contexts are known. Otherwise reconcile the relevant parent contexts before accepting a move. Preserve internal identity only when lifetime and membership validation supports it.

Advance revision once per committed transaction and invalidate derived results. Start with a simple local candidate build or staged transaction; optimize local copying only after measuring it. Replace the existing unsafe merge path rather than adding another parallel merge implementation.

**Acceptance:** removal disappears from every API; rename affects XPath filtering; reorder agrees between XML and arena; failure rolls back; replaced targets reject stale patches; unrelated branches retain IDs/properties; sequential and scoped capture produce equivalent structure metadata.

## Phase 3 — Capture the smallest reliable surface

**Dependencies:** Phase 2. **Outcome:** production UIA capture behind the tested interface.

1. Implement element/property, immediate-child, subtree, and geometry acquisition. Use explicit view and coverage parameters. Emit leaf roots correctly and distinguish normal enumeration completion from provider failure.
2. Batch required property reads using supported UIA cache requests. Match the request filter to the current control view. Cache actual unavailable/error status rather than zero rectangles and empty IDs as successful values.
3. Resolve targets from validated cached identity and owning-window context. Remove full-desktop runtime-ID search from routine scoped capture. If direct resolution fails, reconcile the nearest surviving ancestor or window membership within the same budget.
4. Reconcile desktop immediate children for top-level membership; use native APIs for stacking/foreground metadata without descending through all UIA windows.
5. Treat window movement/layout as potential descendant geometry invalidation. Refresh the needed geometry scope; never assume all rectangles translate unchanged.
6. Thread cancellation and absolute deadlines through requests. Mark sibling/depth limits as incomplete coverage. Preserve known data when acquisition fails.
7. Retire the separate parallel walker from normal use until it delegates to the same capture/finalization model and passes equivalence tests.

**Acceptance:** recording tests show property updates do not walk descendants; a window update reads no unrelated windows; failed target resolution does not silently initiate desktop descendant capture; live fixtures confirm membership and geometry. Collect target-provider batch-versus-individual read timings.

## Phase 4 — Route invalidations and schedule bounded repair

**Dependencies:** Phase 3. **Outcome:** cached state stays updated without event storms or lost changes.

1. Preserve WinEvent object/child identifiers, thread, and timestamp. Add UIA property/structure event adapters for precise targeting where supported. Native object IDs must be resolved, not mistaken for UIA runtime IDs.
2. Resolve events into property, geometry, child-membership, or subtree invalidation. Treat ambiguous events conservatively within the owning window. An unresolved descendant event must not be reduced solely to a desktop membership check.
3. Coalesce dirty regions. Subtree work can subsume descendant work; property-only work cannot. Bound queues and retain broader dirty coverage on overflow.
4. Track per-region epochs and structural preconditions. Events during capture remain dirty after commit; unrelated-window changes do not invalidate every in-flight patch. Register subscriptions around initial discovery with reconciliation so startup does not lose intervening events.
5. Share in-flight work among waiting queries. Prioritize query-relevant coverage while reserving a bounded repair budget to avoid starvation. Separate input debounce from capture age and impose a maximum defer interval.
6. Handle worker completion, disconnect, timeout, shutdown, and backoff explicitly. Blocked COM calls consume bounded capacity; do not create an unlimited replacement thread stream. Report degraded capacity. Consider process isolation only if measurements justify its cost.
7. Add budgeted age/query-interest validation and shallow window reconciliation to compensate for missed events. Their intervals remain configuration informed by target measurements.

**Acceptance:** event storms stay bounded; updates occur during cursor tracking and movement; events arriving mid-capture survive; queue/disconnect failure is visible; shutdown unregisters events and avoids waiting forever; slow scopes cannot create unbounded workers.

## Phase 5 — Apply the query/deadline contract to Python

**Dependencies:** Phase 4. Exception and persistent-scope semantics are confirmed. **Outcome:** consistent cached-query behavior across public APIs.

1. Add one query coordinator: resolve conservative dependency coverage, schedule/join repairs, wait until relevant coverage is usable or the absolute deadline expires, then evaluate one committed revision.
2. Do not use string-based window hints as a general XPath planner. Support explicit scope and provably narrow expressions first. Ancestor/sibling axes, unions, positional predicates, and global descendants require conservative coverage. Global queries may span many branches; schedule bounded incremental work and report incomplete coverage when the deadline prevents completion.
3. Apply the coordinator to singular/plural XPath, filtering, membership, and point lookup. Parse invalid XPath before acquisition and return a query error. Retry for clean no-match is separate from freshness repair and consumes the same deadline.
4. Specify `timeout_ms=0`: inspect currently usable cached coverage without waiting; report stale immediately if required coverage is dirty/unobserved. Query timeouts do not restart at each capture. Caller expiry does not automatically cancel useful shared work.
5. Keep `len`, iteration, count, and formatting as explicit cached-revision introspection to avoid surprising property-side I/O. Document their coverage/status and provide deadline-aware collection queries when callers require repaired coverage.
6. Release the GIL while waiting. Migrate refresh helpers and app launch polling to the service. Validate current application membership before launch/activate decisions; private shallow discovery must not overwrite the driver's published coverage. Mark relevant coverage dirty after successful actions and validate action identity immediately before use.
7. Update exception exports, Python stubs, README/examples, and compatibility notes together. Define clone/handle ownership deliberately: service handles must not accidentally share an orphan cancellation flag while holding divergent tree copies.

**Acceptance:** stale positive hits wait just like misses; plural/contains do not hide timeout as empty/false; a 500 ms deadline is not extended to the 120 s capture timeout; clean no-match remains distinguishable from invalid query, stale, incomplete, and provider failure; another Python thread progresses during waits.

## Phase 6 — Integrate UI Explore and live point targeting

**Dependencies:** Phase 4; reuse Phase 5 query coordination. **Outcome:** displayed state consistently describes committed cached data.

1. Replace GUI-specific refresh spawning/polling with service requests and completion wakeups. Coalesce repeated refresh clicks and offer cancellation/status without blocking rendering.
2. Store selected identity and evaluated query/revision together. Derive properties and ancestor paths from the current revision. Clear disappeared selections and overlays. Re-evaluate or visibly invalidate XPath results on affected commits and input changes.
3. Display relevant age, pending repair, incomplete coverage, and refresh failure. Do not replace a failure with a temporary success-looking timestamp.
4. Share point-resolution logic with Python: resolve the live window/element when needed, constrain cached geometry accordingly, and repair stale relevant geometry. Keep native stacking separate from UIA sibling order.
5. Retain valid selection/expansion by identity through unrelated patches; remove obsolete copied-state fields and duplicate refresh code after migration.

**Acceptance:** tree/details/XPath output/highlight agree on revision; unrelated window updates preserve selection; a removed control clears its border; covered background controls do not win point lookup; UI repaints when a worker completes even with no input.

## Phase 7 — Validate on slow systems and remove migration code

**Dependencies:** Phases 5–6. **Outcome:** demonstrated correctness and reduced acquisition cost.

Run deterministic capture/scheduler tests plus controlled Windows fixtures for two apps, nested lists, reorder/reparent, window replacement, resize, overlapping windows, partial failure, and changes during capture. Use a quiescent full capture as a test oracle only where practical; it is not the runtime refresh strategy.

Report median and tail query/update latency, provider calls, nodes captured, unrelated-window acquisitions, queue size, dirty duration, and local projection costs on representative target systems. Set repair budgets and age thresholds from those results. Broadening capture must have a recorded reason.

Required qualitative gates: warm usable cached queries acquire nothing; property-only changes stay local; structural updates stay within necessary parents/subtrees; unrelated windows are never captured without a dependency or repair reason; deadlines and memory remain bounded under provider stalls. If local XML rebuilding dominates, optimize that measured bottleneck without changing acquisition semantics.

Remove legacy full-refresh loops, unsafe independent mutation/merge APIs, duplicated walkers/hit testing, and stale documentation once replacements pass. Preserve compatibility aliases only where they do not bypass invariants.

## Validation commands and delivery gates

Use focused crate tests for each phase, for example `cargo test -p uitree -p xmlutil --lib --locked`, then the affected consumer tests. Run `cargo check --workspace --all-targets --locked`, formatting checks, and Clippy on touched targets before integration. Distinguish pre-existing failures from regressions; do not reformat unrelated dirty files.

Before final delivery, run workspace tests in the appropriate Windows environment, build/install the Python extension into a dedicated test environment, exercise public API compatibility, and run the interactive UI fixture. Desktop-dependent tests must be identified separately from deterministic tests. Do not use the existing personal app-launch script as the only integration test.

Each phase should land as a coherent reviewable change with its acceptance tests. The sequence is 0 → 1 → 2 → 3 → 4, followed by Python and GUI integration, then target validation/cleanup. The first useful end-to-end milestone is a property-change event updating one cached control, with a query waiting for that patch and no unrelated provider capture.
