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

## Cursor tracking and paused inspection

**Track Cursor** uses the shared popup-aware coordinate lookup on a background
worker, with at most one lookup in flight. Pointer movement is coalesced with a
100 ms settling delay and a 250 ms cooldown; a stationary pointer is checked again
one second after completion so newly opened menus can still be detected. The
default five-second point-query deadline applies. A slow or failed lookup does
not blank the previous selection: the toolbar identifies the captured revision
and indicates when an update is pending. Normal tracking never requests a whole-window
refresh or uses cached rectangles to bypass provider hit testing.
Teams native-host reconciliation is shared with Python: empty unnamed host panes
can resolve to a unique live-validated control in the sibling WebView document,
within the same deadline. Ambiguous candidates are not selected. The explicit
**Verify XPath (full scan)** option instead requests a full exposed-desktop scan
after point lookup, with a shared 120-second deadline; keep it off for narrow capture.

Uncheck **Track Cursor** to freeze the displayed tree, selected properties and
generated XPath. Escape also pauses when **UI Explore has keyboard focus** (it is
not a global hotkey). Moving over UI Explore itself does not select the inspector;
wait for the desired capture, then return to the checkbox to pause it. Switching
to Test Xpath also pauses tracking. Late worker results cannot replace a paused
view, even if tracking is quickly restarted.

The toolbar labels this **Paused snapshot — … (not live)**. Rust continues to
maintain its live cache, but background commits do not alter this displayed
snapshot. Expanding captured branches and copying details remain available;
uncaptured branches do not trigger capture. XPath testing in this mode evaluates
only the frozen snapshot, and highlighting is hidden because historical bounds
may no longer represent the desktop.

Resume **Track Cursor** for new point captures, or press **Refresh** to leave the
paused view and resume normal incremental display updates. Refresh requests the
selected region if its identity still exists, otherwise desktop membership.
The refresh action is also available in paused Test Xpath mode. Locators copied
from a paused snapshot are historical observations, not guarantees that a control
still exists.

## Generated locators

Locators use semantic window anchors and prefer AutomationId/Name descendants,
avoiding incidental pane indices. No copied locator embeds runtime IDs, handles
or NodeKey. Partial coverage is explicitly labelled **Candidate — uniqueness not
verified**. **Simple XPath** prefers structural steps below the semantic window
anchor; positions require complete sibling enumeration. Indistinguishable windows
or unsafe structural steps show an error and disable copying instead of falling
back to a session identity. Generating a candidate does not acquire more state.

Enable **Verify XPath (full scan)** before cursor tracking to force fresh full
desktop coverage before accepting a result. The original target must still exist
and the locator must identify it uniquely. Self-exclusion remains in effect.
Verification can time out, or fail for transient popups omitted by enumeration.
Names, titles, AutomationIds, language and layout may change in later sessions:
snapshot uniqueness is not a permanent-identity guarantee.

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
The test also tracks the controlled button, bounds stationary lookup frequency,
pauses the view, renames the button, verifies the frozen revision/details, and
checks that Refresh exposes the new name. It moves the cursor only over its owned
test windows and restores the original cursor position on exit.
  
