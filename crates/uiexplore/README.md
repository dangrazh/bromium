# UI Explore

A UI Explorer for Windows inspired by inspect.exe, which enables users to explore/discover the windows UI tree. Features include:

- a tree explorer similar to inspect.exe
- getting xpath locators for any ui element in the tree
- an xpath tester, to validate custom xpath locators
- no admin rights required for installation / usage

## Running

```powershell
cargo run -p uiexplore --bin uiexplore --release
```

The tree starts at Desktop and is repaired in the background. Expanding an
uncaptured branch shows a loading indicator and requests its immediate children;
unopened grandchildren remain lazy. A branch is shown without an expander only
after its children have been observed to be empty. Selecting a branch does not
prevent collapsing it.

UI Explore excludes its own process, including its highlight overlay, before
capturing descendants or subscribing to window events. This policy applies only
to the inspector's service; the Python library retains its normal desktop view.

## Generated locators

With **Simple XPath** off, locators prefer a named descendant unique within the
owning window (`.../Window[...]//Button[@Name='...']`). This avoids intermediate
pane indices that change when tooltips appear. This shortcut requires complete
cached window coverage; partially captured windows, unnamed controls, and ambiguous
names use the existing full-path fallback. Generating a locator does not request
additional capture. **Simple XPath** retains the positional format. Names and
window identity can still change, so generated locators are not permanent IDs.

## Regression tests

Headless renderer and shared-tree tests:

```powershell
cargo test -p uiexplore --bin uiexplore
cargo test -p uitree --lib
```

The opt-in live test requires an interactive Windows desktop and a Python
environment with bromium installed. Run from the workspace root:

```powershell
cargo build -p uiexplore --bin uiexplore
$env:BROMIUM_LIVE_TESTS = '1'
$env:UIEXPLORE_EXE = 'target/debug/uiexplore.exe'
python crates/uiexplore/tests/test_gui_live.py
```

It starts its own UI Explore instance and the controlled native fixture from
`crates/bromium/tests/incremental_fixture.py`, then checks root visibility, lazy
expansion, collapse/reopen, self-exclusion before capture/subscription, responsiveness,
and bounded capture work for the controlled branch. Global node/revision counts are
reported, not bounded, because unrelated desktop windows can legitimately update.
The latest capture log is retained at `target/uiexplore-live.log`.
It closes only its test processes and never runs Teams. Avoid interacting with the
test windows while it runs. The external observer explicitly refreshes its own snapshot
to verify the displayed GUI; UI Explore itself continues using incremental updates.
  
