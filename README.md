# Bromium

Bromium provides Windows desktop UI automation through Rust and Python.

- **Python library:** query UI Automation elements, launch or activate applications,
  request supported window closure, perform mouse/keyboard actions, inspect
  displays, and capture the primary screen.
- **UI Explore:** inspect the desktop's cached UI tree, obtain XPath locators, and
  test queries in a desktop application.

## Tree state and freshness

Rust owns each driver's cached tree. Startup acquires shallow desktop/window
membership; descendants are acquired on demand. Events and age checks trigger
incremental property, child-list, or subtree repairs.

Tree identity combines the provider's runtime ID with the element's native
window handle. For handle-less elements it also includes every parent identity
back to the nearest native anchor. This preserves separate tree occurrences both
under different popup hosts and under different rows within one native table.
Names, bounds and sibling positions are not part of identity.
Raw runtime IDs remain unchanged; ambiguous runtime-ID-only actions fail rather
than selecting an arbitrary element. Python `Element` equality and hashing use
this occurrence identity, not an element's lifetime token. Public runtime IDs and
handles remain the provider's values; a handle-less element still reports `0`.

Missing IDs and duplicate occurrences with identical ancestry are retained using
snapshot-only tokens. Fresh captures replace these tokens; actions on these
elements (or handle-less descendants depending on them) raise
`ElementNotFoundError` rather than guessing from names, bounds, or positions.
Valid siblings remain available. Debug `tree_missing_identity` records include
ancestry, provider metadata, and original-versus-cached runtime IDs.

Queries validate relevant coverage within a deadline. If coverage remains stale,
they raise `bromium.StaleTreeError(TimeoutError)` with `reason`, `scope`,
`revision`, and `coverage`. Stale coverage is not a definitive no-match.
An undelivered provider event can remain undetected until age validation; this
is not an instantaneous, atomic view of every application.

Routine Python refresh calls are unnecessary. Explicit `refresh_region()` is
available when a caller knows a region needs repair. Successful
`refresh(window_title="...")` overrides persist; `refresh(None)` retains the
scope. Assign `driver.window_title = None` to clear it.

`len(driver)`, iteration, `snapshot_elements()`, and returned `Element`
properties are cached views, not live queries. Query again for updated metadata.
XPath membership (`xpath in driver`) is a coverage-aware query.

Coordinate lookup is popup-aware: native handles, ownership and bounded UIA
ancestry discovery map menus to their cached regions. It captures immediate
children along the hit ancestry, not the entire Office/window subtree. All work
shares one deadline. Debug `point_lookup` logs
show the mapping and outcome; a popup disappearing during lookup reports stale
state instead of returning a control beneath it.
For empty unnamed Chromium host panes in `TeamsWebView` windows, a bounded
same-window search reconciles the hit with a unique live-validated WebView control.
Cached identities guide traversal only; ambiguous matches fail closed. This shared
Python/UI Explore path retains the original deadline and does not scan the desktop.
Known XAML `PopupRoot` / `Light Dismiss` layers over menu bars are reconciled to
a unique live-validated menu heading while the same application's flyout is active.
This is **logical locator discovery**, not a guarantee of the physical click target:
the dismissal layer may still receive a click. Python callers can disable this
specific reconciliation with `resolve_xaml_overlays=False`; provider ancestry
inconsistencies may then still raise `StaleTreeError`. No menu is closed or clicked
by lookup, and overlay/menu identity is checked through capture and retries.
Transient publication invalidations allow up to three narrow capture attempts
within that same deadline, without switching target identity or native window.
Debug `point_retry` explains retry decisions; `point_ancestry_mismatch` records
captured membership and bounded raw/control-view probes for missing or ambiguous
ancestry. A point-discovered XAML popup omitted by downward enumeration can be
retained only after its live ancestry is revalidated. Its parent's membership
remains partial. Reusable locators are explicitly unverified candidates and never
contain `NodeKey`, runtime IDs or native handles.
Other missing/ambiguous links still fail conservatively. Debug
`point_invalidation` identifies overlapping invalidations and epochs;
`point_capture_timing` records per-ancestor capture work.
Point queries repair their anchored region without waiting for unrelated desktop
membership changes. Missing windows can be discovered through UIA ancestry without
enumerating desktop siblings. Global XPath queries still require their full query
scope to be current. A separate bounded point worker avoids waiting behind a
background subtree capture, but a blocked call on that point worker can still
cause a deadline error. With partial coverage, generated locators do not claim
uniqueness across uncaptured descendants (`element.xpath_status == "candidate"`).
For explicit fresh full-desktop verification, use
`driver.get_element_by_coordinates(x, y, verify_xpath=True)`; it shares the driver's
timeout and returns `snapshot_unique` or fails, never silently downgrading.

## Python library

Requires Windows and Python 3.12 or later, with a compatible wheel or a source
build. Local validation covered CPython 3.12 on Windows x64; this is not a
validation claim for every interpreter/version allowed by package metadata.

```powershell
python -m pip install bromium
```

The published package may precede this checkout. See the
[Python README](crates/bromium/README.md) for the full API, logging configuration,
examples, and failure semantics, and [bromium.pyi](crates/bromium/bromium.pyi)
for type declarations.

```python
import bromium

driver = bromium.WinDriver(timeout_ms=5000, window_title="My application")
try:
    buttons = driver.get_elements_by_xpath("//Button")
    print([button.name for button in buttons])
except bromium.StaleTreeError as error:
    print(error.reason, error.coverage)
    print(driver.tree_status)
```

Replace the title with an application on your desktop. The timeout is in
milliseconds: `5000` means five seconds. Startup has a separate 120-second
membership budget. Element actions release the GIL but are synchronous and have
no enforced execution timeout. Serialize calls on a shared Python driver with
a lock. A permanently blocked provider can consume its capture worker and leave
later queries stale; a query deadline does not cancel an already-running COM call.

`app_window.close()` requests normal closure through the live element's UIA
Window pattern. Unsupported elements raise `AutomationError`; unresolved or
obsolete identities raise `ElementNotFoundError`. It never closes an ancestor,
sends a keyboard shortcut, or terminates a process. Successful return means the
provider call succeeded, not that the window has already disappeared. Closing
a driver-returned element triggers automatic coverage and membership repair;
no manual refresh is needed. See the [closure API](crates/bromium/README.md#closing-a-window)
for details and an example.

## Build from this workspace

Use a Windows Rust toolchain with the MSVC build tools/Windows SDK and a matching
Python installation. Run from the workspace root:

```powershell
python -m venv .venv
.\.venv\Scripts\Activate.ps1
python -m pip install "maturin>=1.8,<2.0"
maturin develop --release --manifest-path crates/bromium/Cargo.toml
```

To produce a wheel instead of installing into the active virtual environment:

```powershell
maturin build --release --manifest-path crates/bromium/Cargo.toml --out target/wheels
```

Run UI Explore with both package and binary specified (the package also contains
a `start_screen` binary):

```powershell
cargo run -p uiexplore --bin uiexplore --release
```

UI Explore loads and repairs the tree in the background. Desktop access is
subject to Windows session, privilege, and provider restrictions.
Cursor tracking performs bounded, popup-aware point queries. Stopping tracking
freezes the displayed snapshot for inspection; resume tracking or press Refresh
to see current state again. The paused view is explicitly labelled and does not
stop Rust's live-cache maintenance. See the [UI Explore guide](crates/uiexplore/README.md).

Python coordinate lookup and UI Explore share reusable XPath generation: semantic
window anchors, then AutomationId/Name selectors, with structural fallback only
where siblings were enumerated. Duplicate indistinguishable windows produce an
explicit error, not a window index or identity-based fallback. UI Explore labels
unverified candidates and offers opt-in “Verify XPath (full scan)” during cursor
tracking (up to 120 seconds, still off the GUI thread). Names/titles, language,
AutomationIds and structural positions can change; validate saved locators in
each automation session. Full-scan uniqueness is not a future-stability guarantee.

## Validation and release status

See [Python regression test instructions](crates/bromium/tests/README_INCREMENTAL.md)
and the [follow-up completion record](bromium_docs_for_agents/done/PYTHON_LIBRARY_FOLLOWUP_TASKS.md).
Desktop tests are opt-in; Teams interaction requires a separate opt-in.
Local correctness tests passed, but representative slow-target performance
acceptance remains outstanding. See the
[implementation status](bromium_docs_for_agents/done/TREE_INCREMENTAL_IMPLEMENTATION_STATUS.md) for limits.

Do not use `ci.ps1` for ordinary local validation: it includes release/version
and publication operations.

## License

Apache License 2.0.
