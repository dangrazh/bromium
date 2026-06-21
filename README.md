# Bromium

Bromium as a project aims to provide the required infrastructure to automate tasks in Microsoft Windows Desktop Applications. It is devided in two main components:
- the bromium Python library that provides bindings to interact with the Windows UI Automation API through Rust. It enables users to automate tasks and interact with Windows UI elements programmatically.
- the UI Expore Desktop application (inspired by inspect.exe) which allows users to inspect the current Windows Desktop, get xpath locators to any ui element on the desktop and test custom xpah locators. It can be run without a need to install the application and without admin rights. This can be built from source by cloning the github repository https://github.com/dangrazh/bromium/

## Key Features of python library

- Get representation of all UI elements on the current desktop (UI tree)
- Launch an application or activate an already running appliation window
- Interact with UI elements on the current desktop
- Get screen context information (size, scaling, etc.)
- Take screen shots
- Get cursor position coordinates
- Retrieve UI element information at specific coordinates



## Installation

```bash.\
pip install bromium
```

## Usage

### Quickstart

```python
import bromium

# Initialize logging (optional, but helpful for debugging)
bromium.init_logging(log_level="Info", enable_console=True)

# Create a WinDriver — builds the UI Automation tree
driver = bromium.WinDriver(timeout_ms=5000, window_title=None)
print(f"Elements in tree: {len(driver)}")

# Get cursor position and find the element under it
x, y = driver.get_cursor_pos()
element = driver.get_element_by_coordinates(x, y)
print(f"Element at cursor: {element.name} ({element.control_type})")

# Look up an element by XPath (retries until timeout_ms if not found)
found = driver.get_element_by_xpath(element.xpath)
print(f"Found: {found.name}")

# Click the element
found.send_click()
```

### App Launch Example

```python
import bromium
import time

bromium.init_logging(log_level="Info", enable_console=True)

driver = bromium.WinDriver(timeout_ms=5000)
print(f"Driver has {driver.element_count} elements.")

# Launch or activate an application
app_path = r"ms-teams.exe"
xpath = r"/Pane[@Name='Desktop 1']/Window[@Name='Microsoft Teams']"

try:
    app_window = driver.launch_or_activate_app(app_path, xpath)
    print(f"App window: {app_window.name}")
    time.sleep(3)

    # Refresh the tree in place (no need to reassign)
    driver.refresh(window_title="Microsoft Teams")
    print(f"Tree refreshed: {driver.element_count} elements.")

    # Find and click a button
    login_btn = driver.get_element_by_xpath("//Button[@Name='Sign in']", timeout_ms=3000)
    login_btn.send_click()

except bromium.ElementNotFoundError:
    print("Element not found — app may already be logged in.")
except bromium.AutomationError as e:
    print(f"Automation error: {e}")
```

### Iterating & Filtering Elements

```python
import bromium

driver = bromium.WinDriver(timeout_ms=5000)

# Collection protocols
print(f"Total elements: {len(driver)}")
print(f"XPath exists: {'//Button[@Name=\"OK\"]' in driver}")

# Iterate all elements
for elem in driver:
    if elem.control_type == "Button":
        print(f"  Button: {elem.name}")

# Filter with find_elements (case-insensitive substring match)
buttons = driver.find_elements(control_type="Button")
edits = driver.find_elements(control_type="Edit", name="Search")
```

## API Reference

### Module-level Functions

These are the recommended entry points for logging configuration:

- `init_logging(log_path=None, log_level=None, enable_console=None, enable_file=None) -> None`: Initialize the bromium logging system.
- `get_version() -> str`: Returns the current bromium version string.
- `get_log_file() -> str`: Returns the current log file path.
- `set_log_file(log_file: str) -> None`: Sets the full path for the log file. Creates parent directories if needed.
- `get_log_level() -> str`: Returns the current logging level as a string.
- `set_log_level(log_level: str) -> None`: Sets the logging level ("Off", "Error", "Warn", "Info", "Debug", "Trace").
- `set_log_directory(log_directory: str) -> None`: Sets a custom directory for log files. A timestamped file is created automatically.
- `enable_console_logging(enable: bool) -> None`: Enable or disable console logging.
- `enable_file_logging(enable: bool) -> None`: Enable or disable file logging.
- `reset_log_file() -> None`: Clear all contents from the current log file.

### Exceptions

- `ElementNotFoundError`: Raised when a UI element cannot be located (by xpath, coordinates, or runtime ID).
- `AutomationError`: Raised when a UI Automation operation fails (click, send_keys, etc.).
- `TreeConstructionError` (extends `TimeoutError`): Raised when the UI tree cannot be built or refreshed.

### WinDriver

The main class for interacting with the Windows UI Automation tree.

#### Constructor

- `WinDriver(timeout_ms: int, window_title: Optional[str] = None)`: Creates a new driver and builds the UI tree. `timeout_ms` is the default retry duration for element lookups. `window_title` optionally filters the tree to a specific window.

#### Properties

| Property | Type | Access | Description |
|----------|------|--------|-------------|
| `timeout_ms` | `int` | read/write | Default timeout in milliseconds for element lookup retries |
| `element_count` | `int` | read-only | Number of UI elements currently in the tree |
| `window_title` | `Optional[str]` | read/write | The window title filter, if set |

#### Collection Protocols

- `len(driver)` — returns element count
- `for elem in driver` — iterates all elements in the tree
- `xpath in driver` — checks if an XPath exists in the tree

#### Methods

- `get_cursor_pos() -> tuple[int, int]`: Returns the current cursor position as (x, y) coordinates.
- `get_element_by_coordinates(x: int, y: int) -> Element`: Returns the UI element at the given screen coordinates.
- `get_element_by_xpath(xpath: str, timeout_ms: Optional[int] = None) -> Element`: Finds an element by XPath. Retries with tree refreshes until `timeout_ms` elapses. When `None`, uses the driver's default `timeout_ms`. Pass `0` to disable retrying.
- `get_elements_by_xpath(xpath: str) -> list[Element]`: Returns all elements matching an XPath expression.
- `find_elements(control_type: Optional[str] = None, name: Optional[str] = None) -> list[Element]`: Filters elements by case-insensitive substring match on control type and/or name. Returns an empty list if none match.
- `refresh(window_title: Optional[str] = None) -> None`: Refreshes the UI tree in place. Uses the stored `window_title` if no argument is provided.
- `launch_or_activate_app(app_path: str, xpath: str) -> Element`: Launches or activates an application, returning the element matching the XPath.
- `get_screen_context() -> ScreenContext`: Returns information about all connected display screens.
- `take_screenshot() -> str`: Takes a screenshot, saves it to a temp directory, and returns the file path.
- `pretty_print_ui_tree() -> None`: Prints the UI tree to stdout for debugging.

### Element

Represents a Windows UI Automation element.

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

- `send_click() -> None`: Sends a click (uses Invoke pattern if available, otherwise mouse click at center).
- `send_double_click() -> None`: Sends a double-click at the element center.
- `send_right_click() -> None`: Sends a right-click at the element center.
- `hold_click(holdkeys: str) -> None`: Clicks while holding modifier keys ("ctrl", "shift", "alt").
- `send_keys(keys: str) -> None`: Sends keyboard input. Special keys use `{}` syntax (e.g., `{ctrl}{alt}{delete}`). Groups with `()` (e.g., `{ctrl}(AB)` for Ctrl+A+B).
- `send_text(text: str) -> None`: Sends plain text (uses Value pattern if available, otherwise simulated keystrokes).
- `hold_send_keys(holdkeys: str, keys: str, interval: int) -> None`: Sends keys while holding modifiers with a custom interval in milliseconds.
- `show_context_menu() -> None`: Shows the context menu for this element.

### ScreenContext

Information about all display screens in the system. Automatically detects all connected displays on construction.

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

Enum for log level values: `LogLevel.Error`, `LogLevel.Warn`, `LogLevel.Info`, `LogLevel.Debug`, `LogLevel.Trace`, `LogLevel.Off`.

### Bromium (Legacy)

A static-method-only class that mirrors the module-level functions above. Prefer using `bromium.init_logging(...)` directly instead of `Bromium.init_logging(...)`.

## Requirements

- Python 3.12 or higher
- Windows operating system

## Building from Source

To build the project from source, you'll need:

1. Rust toolchain (cargo, rustc)
2. Python 3.12+
3. maturin (for building Python wheels)

```bash
# Clone the repository
git clone https://github.com/dangrazh/bromium.git
cd bromium

# Build the project using maturin
maturin build

# Install in development mode
maturin develop
```

## License

Apache License 2.0

<!-- ## Contributing

[Add contribution guidelines here] -->
