# Incremental tree design direction

Status: proposal following the audit; no implementation changes. User requirement: queries use the cached tree; acquisition must be incremental and as narrow as correctness permits because capture latency on target systems is prohibitive.

## Ownership and consistency

Maintain one canonical desktop tree with stable internal node IDs, parent/ordered-child relationships, and stored properties. Runtime IDs identify live provider elements within their lifetime; do not use an HWND alone as durable identity. Keep capture roots separate from desktop parent/depth/window-order metadata.

Capture small patches off-thread, validate them, then commit under a single owner. Readers observe a coherent committed revision, never a half-applied merge. Unaffected branches remain cached. Revision denotes internal consistency, not simultaneous acquisition of the entire desktop.

Derive XML, XPath indices, and geometry views from canonical state. Invalidate derived caches on commit; initially rebuilding these local representations may be acceptable because it avoids desktop/provider acquisition. Measure local rebuild time separately; optimize with incremental DOM edits or structural sharing only if it matters on target systems. Never expose independently mutable node and element collections.

## Narrowest sufficient acquisition

| Observed change | Acquisition and update |
|---|---|
| Known property change on an identified node | Read that property or a small required property bundle; patch the node. Mark dependent geometry/filters dirty if affected. |
| Child added, removed, or reordered | Reconcile the affected parent's immediate child identities and order. Retain verified surviving child subtrees; capture newly discovered children to the required coverage. A child-list check does not prove descendants unchanged. |
| Container contents invalidated without reliable detail | Recapture that container's subtree, preserving its external parent and sibling position. |
| Window appears or disappears | Reconcile the desktop's immediate UIA children. Capture new window contents as needed; remove confirmed departed window subtrees. Preserve other windows. |
| Window moves, resizes, or layout changes | Refresh affected geometry/layout coverage. Parent motion can invalidate descendant screen rectangles; refreshing only the window rectangle is insufficient. Do not translate all descendants unless the provider/layout semantics justify it. |
| Foreground/window stacking changes | Refresh native window ordering/visibility metadata used by hit testing; preserve UIA child order separately. No descendant traversal solely to update stacking. |
| Event target cannot be resolved | Reconcile its nearest known surviving parent; widen to the owning window only if needed. Missing parents escalate to shallow desktop membership reconciliation. |
| Query reaches a branch never captured or known dirty | Schedule only relevant coverage; preserve the distinction between unavailable coverage and an actual no-match. |

UIA supports cache requests scoped to an element, immediate children, or descendants, and updated caches do not mutate previously returned references. Batch required property reads using those capabilities where the crate/API supports them. Validate latency against the actual providers; smaller scope alone does not guarantee cheap calls. [Microsoft caching documentation](https://learn.microsoft.com/en-us/windows/win32/winauto/uiauto-cachingforclients)

Structure-change event interpretation must account for event kind: most describe the containing parent, with ChildAdded an exception. Treat uncertain events as invalidation hints rather than unconditional edits. [Microsoft structure-change documentation](https://learn.microsoft.com/en-us/windows/win32/api/uiautomationcore/ne-uiautomationcore-structurechangetype)

## Event routing and refresh scheduling

The current WinEvent callback discards object ID, child ID, thread ID, and timestamp (`crates/winevent-monitor/src/winevent.rs:97`). Retaining event identity is a prerequisite for precise routing; these IDs are not automatically UIA runtime IDs. Combine native window events with UIA property/structure events where supported, resolving event sources to cached nodes. Keep callbacks lightweight and perform provider work on workers.

Replace the unbounded event queue with coalesced dirty state by node and dirty kind: properties, geometry, children, or subtree. Ancestor subtree refreshes subsume descendant refreshes; an ancestor property refresh does not. Bound scheduling and add a maximum defer interval so constant input cannot starve refresh. On overflow, retain a dirty marker for the affected owning window or desktop membership rather than silently dropping uncertainty.

Assign dirty epochs to affected scopes. A patch records its target identity, expected structural context, and starting epoch. Reject patches whose target was removed/replaced or whose parent relationship is incompatible. If relevant changes arrive during capture, never clear them when the patch commits; retain dirty status and schedule follow-up. Changes in unrelated windows should not force a valid patch to be discarded.

Use bounded workers, deadlines, and backoff for unhealthy providers. Cancellation is cooperative and does not terminate an already blocked provider call; avoid repeatedly spawning replacements for a hung scope. Hard isolation would be a separate design choice if target-system measurements show it is necessary.

## Query semantics and coverage

Confirmed API decisions (2026-09-06): queries whose relevant coverage remains stale/incomplete at the deadline raise `StaleTreeError`, extending `TimeoutError`, with reason, scope, revision, and coverage status. A successful `refresh(window_title=...)` persists its title override for subsequent operations. Explicit scope clearing remains available through the setter. These decisions are recorded in the implementation plan and require no further confirmation.

Queries operate on a committed cached revision. Track observed times, dirty state, failed refreshes, and whether children/descendants have actually been enumerated. Store this per relevant coverage, not just as a single global last-refresh timestamp.

XPath evaluation and XPath-based acquisition planning must be separate. Start with explicit query scope or conservatively recognized XPath forms. String matching for a window title is not a correct general XPath planner. A global `//Button` or sibling/ancestor-sensitive expression can depend on multiple branches; return cached results with corresponding coverage/status instead of silently performing a full desktop recapture or claiming global freshness.

For a query miss in incomplete coverage, progressively inspect the deepest confidently resolved scope. Avoid broadening to desktop descendants automatically. Unknown coverage, confirmed empty coverage, and a failed read must be different outcomes.

An internal revision must bind displayed XPath results, selected node properties, and overlays. On commit, invalidate or recompute affected results and clear disappeared selections. A global derived-result invalidation is a correct inexpensive first implementation if dependency tracking is premature.

Events cannot prove absence of changes: providers can omit property-change notifications. Use budgeted targeted validation driven by query interest/age and shallow window membership checks as repair mechanisms, while preserving uncertainty elsewhere. This is compatible with incremental acquisition; it does not require routine full-tree rebuilds. [Microsoft properties/events documentation](https://learn.microsoft.com/en-us/dotnet/framework/ui-automation/ui-automation-properties-overview)

## Implementation order

1. Establish canonical root/identity/coverage invariants and atomic patch tests. Fix all subtree replacement defects identified in the audit, including deletion membership and rollback, before using merging in ordinary queries.
2. Implement separately testable property patches, immediate-child reconciliation, and subtree replacement. Preserve desktop ancestry, native window membership/order, and node identity consistently.
3. Retain event targeting information and introduce coalesced dirty scopes with epochs and one bounded scheduler shared by Python and UI Explore.
4. Route queries and GUI state through committed revisions, expose coverage/failure status, and implement the selected dirty-query behavior.
5. Add scoped batch property acquisition and measure provider calls, elements captured, update latency, local XML/index rebuild time, and time spent dirty on representative slow systems.

Tests must assert both correctness and narrowness: a changed control must not capture unrelated windows; a removal must disappear from every representation; a failed patch must leave the previous revision intact; and events during capture must remain pending. Include reparenting, identity reuse, reordering, moved-window descendant geometry, partial traversal, and stale worker results.

Confirmed decision: when a query intersects known dirty coverage, wait for the relevant incremental update within the query deadline; report explicitly if the coverage is still stale when the deadline expires. The deadline covers queueing, provider acquisition, commit, and retries rather than restarting for each step. A caller timing out does not need to discard a shared update that can still benefit other callers. Never convert incomplete/stale coverage into a definitive no-match. GUI rendering remains nonblocking while a query waits. Initial capture coverage and background repair budgets still require target-system measurements before selecting defaults.
