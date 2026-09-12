# Bromium

Bromium provides Windows desktop UI automation through Rust and Python.

- **Python library:** query UI Automation elements, launch or activate applications,
  perform mouse/keyboard actions, inspect displays, and capture the primary screen.
- **UI Explore:** inspect the desktop's cached UI tree, obtain XPath locators, and
  test queries in a desktop application.

## Tree state and freshness

Rust owns each driver's cached tree. Startup acquires shallow desktop/window
membership; descendants are acquired on demand. Events and age checks trigger
incremental property, child-list, or subtree repairs.

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

## Validation and release status

See [Python regression test instructions](crates/bromium/tests/README_INCREMENTAL.md)
and the [follow-up completion record](PYTHON_LIBRARY_FOLLOWUP_TASKS.md).
Desktop tests are opt-in; Teams interaction requires a separate opt-in.
Local correctness tests passed, but representative slow-target performance
acceptance remains outstanding. See the
[implementation status](TREE_INCREMENTAL_IMPLEMENTATION_STATUS.md) for limits.

Do not use `ci.ps1` for ordinary local validation: it includes release/version
and publication operations.

## License

Apache License 2.0.
