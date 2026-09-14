# Python library follow-up tasks

Work through these tasks in order. Progress is recorded below. This checklist
covers the remaining Python-facing work after the incremental tree implementation;
it does not replace slow-target performance acceptance.

## 1. Complete launch/activate integration

- [x] Route application discovery through coverage-aware query coordination.
      Shallow desktop membership alone must not certify absence for a locator
      that depends on descendants.
- [x] Distinguish a clean no-match from invalid XPath, stale/incomplete coverage,
      and provider failure. Only a clean no-match may trigger application launch.
- [x] Use one absolute deadline across discovery and post-launch polling instead
      of restarting the timeout at each attempt.
- [x] Preserve `StaleTreeError` and its diagnostic attributes at the Python boundary
      instead of wrapping stale failures as `AutomationError`.
- [x] Use bounded live identity resolution for activation and invalidate the
      affected coverage afterward. Preserve the driver's configured title scope.
- [x] Add focused regression tests before moving to task 2.

Primary files: `crates/bromium/src/app_control.rs`,
`crates/bromium/src/windriver.rs`, and shared query-service code where necessary.

Acceptance:

- An already-running app found through a descendant locator is activated without
  launching another instance.
- Dirty/incomplete discovery never becomes a definitive absence result.
- The total operation respects its defined deadline, and no hidden full-desktop
  descendant capture is introduced.
- Existing launch/activate callers retain documented behavior on success.

## 2. Harden action execution

- [x] Release Python's GIL around potentially slow element resolution and
      click/keyboard/provider calls. Keep Python objects outside those operations.
- [x] Review action timeout behavior separately from query deadlines. A blocked
      COM call must not be described as cancellable unless that is actually enforced.
- [x] Define how manually constructed `Element` objects participate in live
      resolution and cache invalidation. They currently lack a service handle and
      use the legacy runtime-ID search.
- [x] Ask for a decision before changing the public `Element` constructor contract
      or requiring explicit driver binding. Do not silently remove compatibility.
      No signature change or binding requirement was introduced: manual elements
      retain validated HWND/runtime-ID resolution without owning a tree service.
- [x] Ensure removed/replaced targets fail safely, resolution failures preserve
      useful diagnostics, and actions invalidate the appropriate cached region.
- [x] Add focused action/GIL/identity tests before moving to task 3.

Primary files: `crates/bromium/src/windriver.rs`,
`crates/uitree/src/capture.rs`, and `crates/uitree/src/service.rs`.

Acceptance:

- Another Python thread can progress while an action is waiting on a provider.
- An action cannot silently operate on a replacement node through an obsolete
  cached reference.
- A subsequent query repairs affected coverage without a routine Python refresh.
- Manually constructed elements have an explicit, tested compatibility contract.

## 3. Align documentation and Python stubs

- [x] Compare exported Python signatures, defaults, return types and exceptions
      with `crates/bromium/bromium.pyi`.
- [x] Correct obsolete refresh exception documentation, including references to
      `TreeConstructionError` where `StaleTreeError` is now raised.
- [x] Document launch/activate and action behavior finalized in tasks 1 and 2.
- [x] Clearly distinguish cached introspection, deadline-aware queries, explicit
      refresh, and the captured properties on previously returned `Element` objects.
- [x] Explain persistent refresh title overrides, scope clearing, timeout units,
      and when a narrow explicit refresh remains appropriate.
- [x] Update README/examples and add lightweight API/signature checks where practical.

Acceptance:

- Published examples and stubs agree with the actual extension.
- Documentation does not imply that cached properties are live or that all
  provider operations can be interrupted by a query timeout.

## 4. Expand Python integration coverage

### Required basis and new script

Use [`crates/bromium/tests/app_start_danipc.py`](crates/bromium/tests/app_start_danipc.py)
as the basis. Preserve that existing script and create a new test script at
`crates/bromium/tests/test_app_start_incremental.py` when implementing this task.

Carry forward its real application workflow: initialize logging, construct a
driver, launch or activate Teams, find the search field, enter search text, and
observe the resulting list. Extend it into explicit, assertion-based test cases
rather than a demonstration that prints exceptions and continues successfully.

- [x] Make executable, title scope, localized locators, search text and timeouts
      configurable. The current script uses German Teams locators and
      `timeout_ms=5`; do not silently treat that value as five seconds.
- [x] Gate real-desktop/application interaction behind explicit opt-in. Do not
      send messages, change accounts, or close pre-existing application windows.
      Make selecting a search result a separate opt-in if it changes application state.
- [x] Replace fixed sleeps with bounded condition/query waits where possible.
- [x] Remove routine broad `driver.refresh(None)` calls from the normal flow so
      the tests demonstrate automatic Rust-managed repair. Test explicit scoped
      refresh separately.
- [x] Turn failures into failed assertions/nonzero exit status. Report unavailable
      applications, authentication requirements and missing fixtures as explicit
      skips or setup failures, not successful tests.
- [x] Retain and reuse the controlled fixture in `test_incremental_live.py` and
      `incremental_fixture.py` for scenarios that cannot safely or reproducibly
      be induced in Teams. Do not make deterministic tests depend on a personal account.

### Scenarios to cover

- [x] Launch an absent test application and activate an existing one, including
      descendant-based discovery without duplicate launch.
- [x] Query -> action -> query updates without manual refresh, including search
      results appearing or changing after text entry.
- [x] Rename, removal and replacement of elements; old Python objects remain
      snapshots and obsolete action targets fail safely.
- [x] Persistent title overrides, scope clearing, scoped refresh, and cached
      introspection semantics.
- [x] Clean no-match versus malformed XPath versus `StaleTreeError`, including
      diagnostic attributes and zero-deadline behavior.
- [x] Concurrent callers, shared repair, and Python thread progress during both
      query waits and actions. Define and test same-driver concurrency behavior
      explicitly rather than assuming it is supported.
- [x] Delayed/failing providers and deadline expiry, using controlled test seams
      where the real application cannot provide reproducible failure conditions.
- [x] Repeated construction, mutation and teardown to exercise event callback
      ownership and subscription lifetime regressions.

Acceptance:

- The new script derives from the existing app-launch workflow, preserves the
  original script, and provides clear setup instructions and reproducible outcomes.
- Normal automation succeeds without explicit refresh between every operation.
- Test-created resources are cleaned up without disturbing existing user sessions.
- Record local results separately from representative slow-target measurements.

## Completion record

For each task, record changed files, tests run, results and any remaining limitations
here before marking the task complete. Do not run the repository's version-bump or
publication workflow as part of these tasks without a separate request.

| Task | Status | Validation / notes |
| --- | --- | --- |
| 1. Launch/activate | Complete | 23 bromium Rust tests, strict Clippy, and `test_launch_contract.py` live regression passed before task 2. |
| 2. Actions | Complete | 55 focused Rust tests, strict Clippy, and `test_action_contract.py` passed before task 3. COM actions release the GIL but remain non-cancellable; no constructor break. |
| 3. Documentation/stubs | Complete | README, bromium.pyi and explicit binding defaults aligned; 2 installed-wheel API contract tests and 55 focused Rust tests passed before task 4. |
| 4. Expanded integration tests | Complete | New configurable script, native executable adapter, fixture extensions, and concurrent shared-repair Rust test. Final controlled suite: 8 passed, Teams explicitly skipped. |

### Final validation — 2026-09-12

Tasks were implemented and their focused tests passed in order. Final verification
used the locally rebuilt 0.8.0 development wheel in `target/incremental-venv`:

- Workspace Rust unit/integration/doc tests: **93 passed, 2 existing screenshot
  tests ignored** (`cargo test --workspace --locked --target-dir target/followup-tests`).
- Strict Clippy for `bromium` and `uitree`, all targets: passed with `-D warnings`.
- Rust formatting checks on changed files and `git diff --check`: passed.
- `test_api_contract.py`: **2 passed**, checking the installed extension's exports,
  signatures/defaults, snapshot value types and exception hierarchy.
- `test_app_start_incremental.py --live`: **8 passed, 1 skipped** in 60.4 seconds.
  Includes the existing launch, action and incremental live regressions. Teams
  was deliberately not run, per user direction. All three repeated mutation/
  replacement/teardown cycles passed.

Implementation files for task 1: `app_control.rs`, `deadline_worker.rs`, binding
changes in `windriver.rs`/`lib.rs`, and `test_launch_contract.py`. Task 2:
`windriver.rs`, `uiauto.rs`, `uitree/src/service.rs`, `incremental_fixture.py`, and
`test_action_contract.py`. Task 3: `README.md`, `bromium.pyi`, binding defaults in
`logging.rs`/`windriver.rs`, and `test_api_contract.py`. Task 4:
`test_app_start_incremental.py`, `fixture_launcher.rs`, fixture extensions,
`README_INCREMENTAL.md`, and shared-service concurrency regression coverage.
Final activation also uses the expected-incarnation resolver from task 2.
The pre-existing `app_start_danipc.py` was not edited as part of this work.

Reproduction commands and Teams opt-in/setup details are in
[`README_INCREMENTAL.md`](crates/bromium/tests/README_INCREMENTAL.md).
No release/publication or version-bump workflow was run. The existing project
version 0.8.0 was preserved and its lockfile entry synchronized.

Limits: these are local correctness results, not representative slow-target
latency acceptance. Already-running COM calls remain non-cancellable. Provider
failure/shared-repair scenarios use deterministic Rust seams; actual property
events, action waits and lifecycle checks use controlled native windows. Same
Python-driver mutable calls must be serialized with a lock; overlapping calls
are explicitly tested to fail with `RuntimeError`, not silently run concurrently.
