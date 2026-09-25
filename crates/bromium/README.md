# Bromium

Bromium is a Python extension for Windows UI Automation, implemented in Rust.
The workspace also includes UI Explore, a desktop tree inspector and XPath tool.

## Key Features of python library

- Query a cached UI tree with on-demand, incremental coverage repair
- Launch an application or activate a matching element in an existing window
- Request normal closure of an element supporting the UIA Window pattern
- Interact with UI elements on the current desktop
- Get screen context information (size, scaling, etc.)
- Capture the primary monitor to a PNG file
- Get cursor position coordinates
- Retrieve UI element information at specific coordinates

Coordinate lookup generates a window-scoped XPath such as
`/Pane[@Name='Desktop 1']/Window[@Name='resume.txt – Notepad']//Button[@Name='Einstellungen']`
when the control's type and nonempty name are unique within its completely captured
window. This avoids positional shifts caused by transient intermediate panes such
as tooltip `PopupHost` nodes. Ambiguous or unnamed controls retain a full-path
fallback. Uniqueness is established for the captured state, not guaranteed across
future application changes; duplicate window titles can still require positions.



## Installation

```powershell
python -m pip install bromium
```

## Usage

### Cached trees and query deadlines

Startup observes desktop/window membership only. Descendant coverage is acquired
on demand and then maintained by incremental property, child-list, and subtree
updates. Set `window_title` to avoid acquiring unrelated application contents.
The cache uses UIA's control view; it is not a simultaneous snapshot of all providers.

XPath, filtering, membership and point queries wait for relevant dirty or missing
coverage within the query deadline, including when a cached match already exists.
`bromium.StaleTreeError` extends `TimeoutError` and exposes `reason`, `scope`,
`revision`, and `coverage`. A stale query is **not** a definitive empty result.
Malformed XPath raises `ValueError`. A singular clean no-match retains
`ElementNotFoundError`; plural clean no-match returns `[]`.

```python
import bromium

driver = bromium.WinDriver(timeout_ms=5000, window_title="My application")
try:
    button = driver.get_element_by_xpath("//Button[@Name='Save']", timeout_ms=500)
except bromium.StaleTreeError as error:
    print(error.reason, error.coverage)
    print(driver.tree_status)
    cached = driver.snapshot_elements()  # explicitly permits incomplete/stale data
```

`timeout_ms=0` performs no waiting and raises `StaleTreeError` if the required
coverage is not usable. A query timeout does not restart for each capture, and
does not cancel useful shared work. Python's GIL is released during waits.

`len(driver)`, iteration, `element_count`, formatting and
`snapshot_elements()` describe cached data without provider acquisition. Consult
`tree_status` for pending/incomplete coverage; use collection queries when a
validated result is required.

`refresh(window_title="Other application")` persists the title on success only.
Calling `refresh()` retains the configured scope; set `driver.window_title = None`
to clear it. `refresh_region(element, timeout_ms=500)` repairs only that cached
region without changing scope. Manual unscoped refresh explicitly requests all
windows, acquired separately; ordinary repair never walks desktop descendants
as a fallback. Property-only changes read one cached property bundle. Ambiguous
layout/geometry events conservatively repair the affected subtree because child
rectangles cannot safely be inferred by translation.

Missed-event validation currently uses a 2-second desktop membership age and
30-second relevant coverage age. One capture worker bounds blocked-provider
capacity; a provider that never returns can leave later queries stale. These
are initial policies, not guarantees about unreported changes or target latency.

### Actions, snapshots and concurrency

`Element` properties are snapshots: even after a refresh, an already-returned
object keeps its captured name and rectangle. Query again for updated properties.
Driver-created elements retain cached lifetime identity for action validation.
Actions release the GIL during resolution/provider work, then invalidate the
affected region. They remain synchronous and have no enforced execution timeout;
`timeout_ms` does not cancel a blocked action.

The public `Element(...)` constructor remains supported. A nonzero HWND is
validated against the runtime ID; without a HWND the legacy runtime-ID search is
used. These manually constructed objects do not own a driver cache and cannot
directly invalidate one. Prefer elements returned by a driver.

`launch_or_activate_app()` validates coverage before deciding whether to launch.
Discovery, process creation, polling, and activation share `driver.timeout_ms`.
Stale discovery raises `StaleTreeError`, malformed XPath raises `ValueError`, and
other deadline expiry raises `TimeoutError`. A native operation already started
may finish after caller expiry; timeout is not a promise that no app was launched
or focused. The configured title scope is preserved.

Use `timeout_ms=5000` for five seconds, not `5`. `tree_timeout_secs` controls manual
refresh and provider-job budgets (default 120 seconds); it does not extend query
deadlines. Startup itself has a 120-second shallow-membership budget.

Serialize calls on a shared `WinDriver` with a Python lock. Concurrent mutable
calls can raise PyO3's `RuntimeError` for an already-borrowed object; the API does
not promise simultaneous queries on one Python driver. Rust shares in-flight
repair internally. Separate drivers own separate services. Released GIL permits
other Python work, but does not make concurrent input to one application safe.

### Quickstart

The following inspects the element under the cursor; clicking it is deliberately
left commented out because the cursor may be over a destructive control.

```python
import bromium

# Initialize logging (optional, but helpful for debugging)
bromium.init_logging(log_level="Info", enable_console=True)

# Create a WinDriver — observes window membership; descendants remain lazy
driver = bromium.WinDriver(timeout_ms=5000, window_title=None)
print(f"Cached elements (possibly incomplete): {len(driver)}")

# Get cursor position and find the element under it
x, y = driver.get_cursor_pos()
element = driver.get_element_by_coordinates(x, y)
print(f"Element at cursor: {element.name} ({element.control_type})")

# Look up an element by XPath (retries until timeout_ms if not found)
found = driver.get_element_by_xpath(element.xpath)
print(f"Found: {found.name}")

# Click the element
# found.send_click()  # opt in only after checking the target
```

### App Launch Example

Teams must be installed and locators must match its language and current UI.
Discovery uses the XPath within the configured title scope, not the executable
name or process ID. A locator that misses an existing app can therefore launch
another instance. This example activates and inspects; it does not sign in.

```python
import bromium

bromium.init_logging(log_level="Info", enable_console=True)

driver = bromium.WinDriver(timeout_ms=15000, window_title="Microsoft Teams")
print(f"Driver has {driver.element_count} elements.")

# Launch or activate an application
app_path = r"ms-teams.exe"
xpath = "//Window[contains(@Name,'Microsoft Teams')]"

try:
    app_window = driver.launch_or_activate_app(app_path, xpath)
    print(f"App window: {app_window.name}")
    # Subsequent queries repair relevant coverage; no routine refresh is needed.

    # Find a button without changing accounts or submitting credentials
    login_btn = driver.get_element_by_xpath("//Button[@Name='Sign in']", timeout_ms=3000)
    print(f"Sign-in control: {login_btn.name}")

except bromium.ElementNotFoundError:
    print("Element not found — app may already be logged in.")
except bromium.StaleTreeError as e:
    print(f"Discovery/query coverage is stale: {e.reason}")
except TimeoutError as e:
    print(f"Launch/activation deadline expired: {e}")
except bromium.AutomationError as e:
    print(f"Automation error: {e}")
```

### Iterating & Filtering Elements

```python
import bromium

driver = bromium.WinDriver(timeout_ms=5000)

# Collection protocols
print(f"Cached elements: {len(driver)}")
ok_xpath = '//Button[@Name="OK"]'
print(f"XPath exists: {ok_xpath in driver}")  # may raise StaleTreeError

# Iterate cached elements only; missing coverage is not acquired
for elem in driver:
    if elem.control_type == "Button":
        print(f"  Button: {elem.name}")

# Filter with find_elements (case-insensitive substring match)
buttons = driver.find_elements(control_type="Button")
edits = driver.find_elements(control_type="Edit", name="Search")
```

### Closing a window

`Element.close() -> None` checks the resolved live element's **Window pattern**,
not just its cached control type. It requests closure of that element only;
there is no automatic ancestor selection, Alt+F4 fallback, or process termination.
`launch_or_activate_app()` can return a descendant if its XPath selects one, so
use a locator for the actual window when you intend to close it.

```python
import bromium

driver = bromium.WinDriver(timeout_ms=5000, window_title="My test application")
window = driver.get_element_by_xpath("//Window[contains(@Name,'My test application')]")
try:
    window.close()  # Only use this on a window you intend to close.
except bromium.ElementNotFoundError as error:
    print(f"Window identity no longer available: {error}")
except bromium.AutomationError as error:
    print(f"Closure unsupported or provider failed: {error}")
```

Unsupported elements raise `AutomationError` with a Window-pattern diagnostic.
Provider lookup and close failures preserve their error details. Identity
resolution rejects removed/replaced targets with `ElementNotFoundError`.
A successful return does not guarantee that the window has disappeared or that
its process has exited; the application may display a prompt or handle the
request without exiting. This operation does not answer prompts or force closure.

For driver-created elements, affected coverage and the parent's immediate
membership are invalidated automatically. Subsequent queries repair them without
a manual refresh. The old Python object's properties remain snapshots. Manually
constructed elements retain the same compatibility limitations as other actions.
Like other actions, `close()` releases the GIL but has no enforced execution
timeout; `driver.timeout_ms` does not interrupt an already-running close call.

## API Reference

### Module-level Functions

These are the recommended entry points for logging configuration:

- `init_logging(log_path=None, log_level=None, enable_console=None, enable_file=None) -> None`: Initialize logging. `log_path` is a directory; defaults are `%USERPROFILE%/.bromium`, Info, console off, file on. Initialization opens a log file even if file output is disabled.
- `get_version() -> str`: Returns the current bromium version string.
- `get_log_file() -> str`: Returns the current log file path, opening a default file if needed; returns an empty string if that fails.
- `set_log_file(log_file: str) -> None`: Sets the full path for the log file. Creates parent directories if needed.
- `get_log_level() -> str`: Returns the current logging level as a string.
- `set_log_level(log_level: str) -> None`: Sets the logging level ("Off", "Error", "Warn", "Info", "Debug", "Trace").
- `set_log_directory(log_directory: str) -> None`: Sets a custom directory for log files. A timestamped file is created automatically.
- `enable_console_logging(enable: bool) -> None`: Enable or disable console logging.
- `enable_file_logging(enable: bool) -> None`: Enable or disable file logging.
- `reset_log_file() -> None`: Truncates the current log file; raises `ValueError` if none is set, or `OSError` on an I/O failure.

Logging level arguments are strings (case-insensitive), not `LogLevel` objects.
Unrecognized strings fall back to Info. File/directory setters can raise
`OSError`; initialization reports file setup failures on stderr instead.

### Exceptions

- `ElementNotFoundError`: Raised when a UI element cannot be located (by xpath, coordinates, or runtime ID).
- `AutomationError`: Raised when a UI Automation operation fails (click, send_keys, etc.).
- `TreeConstructionError` (extends `TimeoutError`): Initial desktop membership could not be acquired.
- `StaleTreeError` (extends `TimeoutError`): Relevant query/refresh/discovery coverage remained stale at the deadline.

### WinDriver

The main class for interacting with the Windows UI Automation tree.

#### Constructor

- `WinDriver(timeout_ms: Optional[int] = None, window_title: Optional[str] = None)`: Creates a shallow cache. None uses a 120000 ms query/launch budget; descendants remain lazy. Titles are case-sensitive substring filters.

#### Properties

| Property | Type | Access | Description |
|----------|------|--------|-------------|
| `timeout_ms` | `int` | read/write | Default timeout in milliseconds for element lookup retries |
| `tree_timeout_secs` | `int` | read/write | Manual refresh and subsequent provider-job budget, default 120 seconds |
| `tree_status` | `str` | read-only | Scope and service-wide revision, dirty/unobserved counts, last error if any |
| `element_count` | `int` | read-only | Cached count in scope; may be incomplete or stale |
| `window_title` | `Optional[str]` | read/write | The window title filter, if set |

#### Collection Protocols

- `len(driver)` — cached count in scope; no acquisition
- `for elem in driver` — cached snapshot iterator; no acquisition
- `xpath in driver` — repairs coverage within `timeout_ms`, then returns a bool;
  clean absence is immediately False, invalid XPath and stale errors propagate

#### Methods

- `get_cursor_pos() -> tuple[int, int]`: Returns the current cursor position as (x, y) coordinates.
- `get_element_by_coordinates(x: int, y: int) -> Element`: Returns the UI element at the given screen coordinates.
- `get_element_by_xpath(xpath: str, timeout_ms: Optional[int] = None) -> Element`: Validates relevant coverage and retries clean misses within one deadline. None uses the driver budget; zero waits for nothing and raises stale if coverage is unusable.
- `get_elements_by_xpath(xpath: str) -> list[Element]`: Repairs relevant coverage within `timeout_ms`, then returns matches or `[]`; does not wait for a clean absence to become a match.
- `find_elements(control_type: Optional[str] = None, name: Optional[str] = None) -> list[Element]`: Filters elements by case-insensitive substring match on control type and/or name. Returns an empty list if none match.
- `refresh(window_title: Optional[str] = None) -> None`: Explicit membership reconciliation and scoped repair within `tree_timeout_secs`; a successful title override persists. None retains scope. Raises `StaleTreeError` on incomplete coverage.
- `refresh_ui_tree(window_title: Optional[str] = None) -> None`: Compatibility alias for `refresh`.
- `refresh_region(element: Element, timeout_ms: Optional[int] = None) -> None`: Requests subtree repair by the element's runtime ID in this driver's cache. None uses the driver query budget; scope is unchanged. Missing cached identity raises `ValueError`, incomplete repair raises `StaleTreeError`.
- `snapshot_elements() -> list[Element]`: Returns cached elements in scope without waiting for coverage.
- `launch_or_activate_app(app_path: str, xpath: str) -> Element`: Launches or activates an application, returning the element matching the XPath.
- `get_screen_context() -> ScreenContext`: Returns information about all connected display screens.
- `take_screenshot() -> str`: Captures the primary monitor to a PNG in `%TEMP%/bromium_screenshots` and returns its path; errors raise `AutomationError`.
- `pretty_print_ui_tree() -> None`: Prints the cached scoped tree to stdout without acquisition.

### Element

Captured UI Automation metadata with live action methods. Properties are read-only.
`Element(name, xpath, handle, control_type, runtime_id, bounding_rectangle)` is
also public; see the manual-element limitations above. Equality and hashing use
only runtime ID, not cached incarnation, name or rectangle. Runtime IDs are not
permanent identifiers across removal/replacement.

#### Properties

| Property | Type | Description |
|----------|------|-------------|
| `name` | `str` | The name of the UI element |
| `xpath` | `str` | The XPath locator for this element |
| `handle` | `int` | The native window handle (HWND) |
| `control_type` | `str` | The UI Automation control type (e.g. "Button", "Edit") |
| `runtime_id` | `list[int]` | The runtime ID uniquely identifying this element |
| `bounding_rectangle` | `tuple[int, int, int, int]` | Bounding rectangle as (left, top, right, bottom) |

#### Methods

- `close() -> None`: Requests closure using the live Window pattern. Unsupported capability/provider failures raise `AutomationError`; unresolved/obsolete identity raises `ElementNotFoundError`. See [Closing a window](#closing-a-window).
- `send_click() -> None`: Uses Invoke, else SelectionItem, else a mouse click at the live center. A failed supported pattern raises rather than trying another action.
- `send_double_click() -> None`: Sends a double-click at the element center.
- `send_right_click() -> None`: Sends a right-click at the element center.
- `hold_click(holdkeys: str) -> None`: Clicks while holding modifier keys ("ctrl", "shift", "alt").
- `send_keys(keys: str) -> None`: Sends keyboard input. Special keys use `{}` syntax (e.g., `{ctrl}{alt}{delete}`). Groups with `()` (e.g., `{ctrl}(AB)` for Ctrl+A+B).
- `send_text(text: str) -> None`: Sends plain text (uses Value pattern if available, otherwise simulated keystrokes).
- `hold_send_keys(holdkeys: str, keys: str, interval: int) -> None`: Sends keys while holding modifiers with a custom interval in milliseconds.
- `show_context_menu() -> None`: Shows the context menu for this element.

### ScreenContext

`ScreenContext()` captures connected displays on construction. Its properties
are snapshots. Enumeration failure or no displays raises `RuntimeError`.
If no display is flagged primary, `primary_screen` falls back to the first display.

#### Properties

| Property | Type | Description |
|----------|------|-------------|
| `primary_screen` | `ScreenInfo` | The primary display screen |
| `screens` | `list[ScreenInfo]` | All available display screens |

### ScreenInfo

Information about a single display screen.

#### Properties

| Property | Type | Description |
|----------|------|-------------|
| `id` | `int` | Unique identifier associated with the display |
| `name` | `str` | The display name |
| `friendly_name` | `str` | The display friendly name |
| `x` | `int` | The display x coordinate |
| `y` | `int` | The display y coordinate |
| `width` | `int` | The display pixel width |
| `height` | `int` | The display pixel height |
| `width_mm` | `int` | Width in millimeters (may be 0) |
| `height_mm` | `int` | Height in millimeters (may be 0) |
| `rotation` | `float` | Rotation in clock-wise degrees (0, 90, 180, 270) |
| `scale_factor` | `float` | Pixel scale factor |
| `frequency` | `float` | Refresh rate |
| `is_primary` | `bool` | Whether this is the primary display |

### LogLevel

Exported enum values: `LogLevel.Error`, `LogLevel.Warn`, `LogLevel.Info`, `LogLevel.Debug`, `LogLevel.Trace`, `LogLevel.Off`. Logging functions accept strings, not these enum instances.

### Bromium (Legacy)

A static-method-only class that mirrors the module-level functions above. Prefer using `bromium.init_logging(...)` directly instead of `Bromium.init_logging(...)`.

It additionally exposes `Bromium.get_win_driver(timeout_ms=None, window_title=None)`,
equivalent to constructing `WinDriver` with those arguments.

## Requirements

- Python 3.12 or higher
- Windows operating system

Local validation used CPython 3.12 on Windows x64. Package metadata permits
additional interpreters/versions, but those are not all validated. A compatible
wheel or working source-build environment is required.

## Building from Source

To build the project from source, you'll need:

1. Rust toolchain (cargo, rustc)
2. Python 3.12+
3. maturin (for building Python wheels)

```powershell
# Clone the repository
git clone https://github.com/dangrazh/bromium.git
cd bromium

python -m venv .venv
.\.venv\Scripts\Activate.ps1
python -m pip install "maturin>=1.8,<2.0"

# Build the project using maturin
maturin build --release --manifest-path crates/bromium/Cargo.toml --out target/wheels

# Install in development mode
maturin develop --release --manifest-path crates/bromium/Cargo.toml
```

Run these commands from the workspace root, with MSVC build tools and the Windows
SDK available. From `crates/bromium`, omit `--manifest-path` instead.
See [regression test instructions](tests/README_INCREMENTAL.md). Desktop tests
are opt-in; Teams interaction requires a separate opt-in. Local tests are not
representative slow-target performance acceptance. Do not use the workspace's
`ci.ps1` for ordinary checks: it includes release/publication operations.

## License

Apache License 2.0

<!-- ## Contributing

[Add contribution guidelines here] -->
