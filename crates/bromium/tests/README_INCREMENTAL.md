# Python incremental-tree regression tests

Use a development wheel built from this checkout with Python 3.12 on an
interactive Windows desktop. Do not use the publishing/version-bump script.
These tests use the standard library and the installed `bromium` extension.
Run commands from the workspace root; substitute your development interpreter.

## Offline contract tests

```powershell
& target/incremental-venv/Scripts/python.exe crates/bromium/tests/test_api_contract.py
& target/incremental-venv/Scripts/python.exe crates/bromium/tests/test_app_start_incremental.py
```

The second command explicitly skips desktop tests. It checks the configurable
workflow and failure propagation without accessing applications.

## Controlled desktop suite

```powershell
rustc --edition 2024 crates/bromium/tests/fixture_launcher.rs -o target/fixture_launcher.exe
& target/incremental-venv/Scripts/python.exe -u crates/bromium/tests/test_app_start_incremental.py --live
```

The executable adapter is needed because `launch_or_activate_app` accepts an
executable path, not interpreter arguments. It starts only the supplied fixture
script with the test interpreter. Missing adapters are setup failures; override
its path with `BROMIUM_FIXTURE_LAUNCHER` if needed. Tests supply isolated session
directories and unique titles. The launched fixture exits on its stop sentinel
or after 60 seconds; stdin-controlled fixtures are stopped in `finally` blocks.
Only test-owned child processes are eligible for termination. Existing windows
and user applications are never closed.

This command also runs `test_launch_contract.py`, `test_action_contract.py`,
`test_incremental_live.py`, and `test_close_contract.py`. They remain independently runnable using
`BROMIUM_LIVE_TESTS=1`. The suite covers:

- Absent launch, descendant-based activation, and no duplicate launch.
- Search-style query/action/query repair without routine refresh.
- Rename, remove, replace, captured Python properties, and obsolete identities.
- Explicit narrow refresh, persistent title overrides, clearing scope, and cached views.
- Clean absence, invalid XPath, stale diagnostics, and zero-deadline queries.
- GIL release during query and controlled provider/action waits. A shared Python
  driver rejects overlapping mutable calls with `RuntimeError`; use a Python
  lock to serialize them. Separate driver instances have separate tree services.
- Repeated fixture/driver teardown to exercise callback subscription ownership.
- Window-pattern closure, unsupported and obsolete target errors, unchanged
  sibling windows/process lifetime, automatic membership repair, and GIL release
  while closing a delayed fixture. Closure tests never interact with Teams.

Deterministic provider failure, shared Rust-service repair, reused runtime IDs,
and deadline/late-worker behavior are tested below the Python boundary:

```powershell
$env:PYO3_PYTHON = (Resolve-Path target/incremental-venv/Scripts/python.exe).Path
cargo test -p bromium -p uitree --lib --locked --target-dir target/followup-tests
```

The fixture's explicit `pause` command deliberately blocks message handling for
400 ms. Other new waits are bounded conditions or Rust's query deadlines. Actions
release the GIL but synchronous COM actions are not forcibly cancellable.

## Optional Teams workflow

This derives from `app_start_danipc.py`, which remains unchanged. The default
Teams locators are German; configure them for the installed version/language:

```powershell
& target/incremental-venv/Scripts/python.exe -u crates/bromium/tests/test_app_start_incremental.py --teams --executable ms-teams.exe --title 'Microsoft Teams' --search-xpath "//ComboBox[@Name='Suche']" --result-xpath "//Group[@Name='Personen']/ListItem[1]" --text Test --timeout-ms 15000
```

`--app-xpath` also configures launch discovery. Timeout units are milliseconds:
`5` means five milliseconds, not five seconds. Each API call has its own stated
budget; this is not one deadline for the entire workflow. No broad refresh or
fixed sleep is used between launch, search-field lookup, text entry, and result
lookup. Logging output uses the console only. The logger currently also opens an
empty file at initialization; `--log-path` defaults to `target/followup-logs` so
this stays in the test workspace rather than the user's normal log directory.

Teams must be installed, signed in, and have a matching search result. Missing
application, authentication UI, or missing controls/results produce a nonzero
test result with the original exception; tests never silently log and pass.
Without `--teams`, this test is explicitly skipped, even with `--live`.
No messages are sent, accounts changed, or existing windows closed.
Selecting a result requires the additional `--select-result` option.

## Interpretation

Local fixture results establish correctness on the tested desktop, not capture
latency acceptance for representative slow target systems. Record those timings
separately. A provider that is already blocked may outlive a caller's deadline;
the tests do not claim that it can be forcibly cancelled.

## Close-method validation — 2026-09-14

Validated with the rebuilt local CPython 3.12 Windows x64 development wheel:

- `test_api_contract.py`: 6 passed, including `Element.close()` signature and
  rejection of an empty identity before provider access.
- `test_app_start_incremental.py --live`: 10 passed, Teams explicitly skipped.
  The two close tests cover actual window closure, automatic membership repair,
  Python thread progress during a provider wait, unsupported controls, obsolete
  targets, and preservation of sibling windows/the fixture process.
- `cargo test --workspace --locked --target-dir target/close-tests --quiet`:
  96 passed, 2 existing screenshot tests ignored. A fresh target directory was
  used after the linker could not open old test executables in `followup-tests`.
- Strict Clippy on `bromium`/`uitree`, changed-file Rust formatting, and
  `git diff --check`: passed.

The absent-pattern regression also covers the pinned Windows binding's null
interface result (error code zero), so unsupported closure reports a clear
capability error rather than the misleading system text "operation completed
successfully". No Teams windows or user applications were closed. No version
bump or publication workflow was run.
