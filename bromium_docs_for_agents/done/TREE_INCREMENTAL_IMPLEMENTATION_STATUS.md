# Incremental tree implementation status

## Delivered

The shared `uitree::TreeService` now owns the cached desktop tree for both Python
and UI Explore. Startup reconciles immediate window membership; requested
descendants remain explicitly unobserved until acquired. Properties, immediate
children, and affected subtrees commit atomically. Missing/duplicate identity,
partial provider failures, and stale removed targets cannot silently publish
successful coverage. Node storage is reclaimed without reusing internal IDs.

UIA and bounded native event adapters invalidate relevant regions. Events received
during acquisition remain dirty. A single capture worker bounds stalled-provider
capacity, shares work across callers, retries failures with backoff, prioritizes
query interests, and reserves every fourth scheduling opportunity for background
repair. Desktop capture requests never silently become descendant traversals.
New dirty entries use a fixed 20 ms coalescing window; later events cannot keep
extending it. Queue-wait telemetry is separate from capture and coverage age.

Python lookup APIs validate required coverage even for cached positive matches.
They share one deadline, release the GIL while waiting, and report stale/incomplete
coverage through `StaleTreeError(TimeoutError)` with diagnostic attributes.
Successful refresh title overrides persist. `refresh_region` provides explicit
scoped repair; `snapshot_elements` and `tree_status` expose cached state without
claiming completeness. Actions returned from collection queries also retain the
service for bounded identity resolution and subsequent invalidation.

UI Explore starts without blocking on capture, receives completion wakeups,
preserves selection identity across revisions, clears obsolete query results and
overlays, and schedules point-region repair without waiting in an egui frame.
Both consumers use native window-constrained hit testing and UIA screen coordinates.
The two superseded consumer hit-testing modules were removed.

## Implementation choices

- The canonical model and transactions live together in `uiexplore_xml.rs`; event
  routing and scheduling live together in `service.rs`. The plan's proposed extra
  model/patch/invalidation files were not necessary to establish their boundaries.
- Derived element projections remain immutable copies rebuilt from a canonical
  revision. They are not independently writable state.
- Geometry invalidation uses an affected-subtree capture with validated batched
  property bundles. A specialized geometry-only provider walk is not implemented;
  this avoids assuming that layout changes preserve membership or relative bounds.
- Precise UIA property events capture one element's property bundle. Ambiguous
  native events conservatively reconcile the owning subtree. Destroyed native
  children recover their owner through cached handles because live HWND ancestry
  is no longer available.
- Native live hit testing replaces an independently maintained stacking cache.
- Tests use recording captures, controlled channels, deadlines and bounded delays,
  rather than a general virtual-clock abstraction. They include unchanged-cache
  acquisition counts, narrow-window queries, mid-capture events, failed publication,
  removed-target rejection, coverage age separation and repeated storage churn.
- Initial age policies are 2 seconds for desktop membership and 30 seconds for
  queried coverage; provider-job timeout is 120 seconds, independently of caller
  deadlines. These remain provisional until measured on the slow target systems.
- Compatibility capture entry points delegate to the same walker/model. They are
  not called by the production consumers' ordinary refresh/query paths.

## Local validation

- Workspace unit/doc tests: 85 passed; 2 existing screenshot tests ignored.
- Workspace all-target compilation and strict Clippy (`-D warnings`) passed.
- Touched Rust files formatted without formatting unrelated workspace files.
- Dedicated Python 3.12 wheel built and installed under `target/incremental-venv`.
- Controlled native two-window fixture passed rename, movement, child removal,
  invalid XPath, cached inspection, persistent scope, scoped refresh, zero-deadline
  stale attributes and GIL-release checks. Warm lookup mean
  was approximately 3.3 ms in one local debug-wheel run, not a target benchmark.
- UI Explore startup smoke test: window appeared and remained responsive; the
  test process was closed afterward. This is not full interactive GUI acceptance.

Reproduce from the workspace root:

```powershell
cargo test --workspace --locked --target-dir target/incremental-tests
cargo check --workspace --all-targets --locked
cargo clippy --workspace --all-targets --locked --target-dir target/incremental-tests -- -D warnings
$env:BROMIUM_LIVE_TESTS='1'
& 'target/incremental-venv/Scripts/python.exe' crates/bromium/tests/test_incremental_live.py
```

The live test requires an interactive Windows desktop and the rebuilt development
wheel. It creates and removes only its own fixture windows. The isolated Cargo
target avoids previously locked test executables in the main target directory.
No version bump, publication, or changes to the personal Python environment were
performed. The existing `ci.ps1` publication workflow was deliberately not run.

## Target-system acceptance gate

Access to the user's slow target systems is still required to complete Phase 7's
performance acceptance. Local correctness checks do not substitute for that gate.
Run the same fixture repeatedly on representative providers, including nested and
virtualized controls, reorder/reparent, replacement windows and overlapping windows.
Compare quiescent captures as a test oracle where practical, never as a runtime
fallback. Manually check GUI selection, expansion and overlays under these changes.

Enable debug logging to collect `tree_schedule` target/kind and queue wait,
`tree_capture` kind, nodes and acquisition
time, and `tree_commit` revision, nodes and local publication time (no UI text in
these records). Record warm and dirty query median/p95/p99, failures, unrelated
window acquisitions, queue/dirty counts and observed memory. Repeat under sustained
event storms and slow/unresponsive providers. Property-bundle versus geometry-only
acquisition measurements, provider-call counts, and separately measured XML rebuild
costs remain target profiling work; do not infer them from node counts alone.

A permanently blocked COM call can consume the sole capture worker. Queries still
expire with stale coverage; the service does not spawn unbounded replacement
threads. Process isolation is a follow-up only if target evidence warrants it.

## Native callback regression

The subsequent Python-library follow-up is complete; see
[`PYTHON_LIBRARY_FOLLOWUP_TASKS.md`](PYTHON_LIBRARY_FOLLOWUP_TASKS.md) for the
2026-09-12 validation record (93 Rust tests, Python API checks, and expanded
controlled-fixture coverage). Teams remains opt-in and slow-target performance
acceptance remains outstanding as described above.

Repeated live validation exposed heap corruption in property-change callbacks.
Inspection of the pinned `windows 0.61.3` generated trampoline showed an owned
by-value `VARIANT` parameter being dropped even though UIA lends its payload to
the callback. A small private COM adapter in `event_handler.rs` now preserves
borrowed ownership, bounds its lifetime through reference counting, requires a
Send + Sync callback, and prevents Rust panics crossing the ABI. A regression
test repeatedly sends a borrowed string payload and verifies the caller still
owns it, including interface query/reference lifetime checks. No registry crate
source or dependency versions were changed to apply this workaround.

Event sender identity is supplied through an event cache rather than synchronous
provider calls inside callbacks. Missing cached identity conservatively dirties
the owning window. The live fixture is also the regression seam for actual
provider events and teardown. Python `-X faulthandler` can print handled COM
first-chance exceptions during provider calls; those messages alone are not a
process termination. The failing pre-fix run terminated with heap-corruption
code `0xc0000374`; the repaired fixture completes normally.
