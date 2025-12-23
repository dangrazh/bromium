# Bromium

Bromium as a project aims to provide the required infrastructure to automate tasks in Microsoft Windows Desktop Applications. It is devided in two main components:
- the bromium Python library that provides bindings to interact with the Windows UI Automation API through Rust. It enables users to automate tasks and interact with Windows UI elements programmatically.
- the UI Expore Desktop application (inspired by inspect.exe) which allows users to inspect the current Windows Desktop, get xpath locators to any ui element on the desktop and test custom xpah locators. It can be run without a need to install the application and without admin rights. This can be built from source by cloning the github repository https://github.com/dangrazh/bromium/

## Features

- Get cursor position coordinates
- Retrieve UI element information at specific coordinates
- Get screen context information (size and scaling)


<!-- ## Installation

```bash.\
pip install bromium
``` -->

## Usage

Here's a basic example of how to use Bromium:

```python
from bromium import WinDriver

# Create a WinDriver instance with a timeout value
driver = WinDriver(timeout=5)

# Get current cursor position
x, y = driver.get_curser_pos()
print(f"Cursor position: ({x}, {y})")

# Get UI element at specific coordinates
element = driver.get_ui_element(x, y)
print(f"UI Element name: {element.get_name()}")

# Get screen context information
screen_context = driver.get_screen_context()
print(f"Screen width: {screen_context.get_screen_width()}")
print(f"Screen height: {screen_context.get_screen_height()}")
print(f"Screen scale: {screen_context.get_screen_scale()}")

# Launch or activate an application
app_path = r"C:\Windows\System32\calc.exe"
xpath = r'/Window[@ClassName="ApplicationFrameWindow"][@Name="Calculator"]'
success = driver.launch_or_activate_app(app_path, xpath)
if success:
    print("Calculator is now in focus")
```

## API Reference

### WinDriver

The main class for interacting with Windows UI elements.

#### Constructor

- `__init__(timeout_ms: int) -> None`: Initializes the WinDriver instance with a timeout in milliseconds.

#### Methods

- `get_timeout() -> int`: Returns the current timeout value in milliseconds.
- `set_timeout(timeout_ms: int) -> None`: Sets a new timeout value in milliseconds.
- `get_curser_pos() -> tuple[int, int]`: Returns the current cursor position as a tuple of (x, y) coordinates.
- `get_ui_element_by_coordinates(x: int, y: int) -> Element`: Returns the UI element at the given pixel coordinates.
- `get_ui_element_by_xpath(xpath: str) -> Element`: Returns the UI element matching the given XPath. Uses a three-step search approach: exact match, subtree search with single match, and pattern matching across multiple matches.
- `get_screen_context() -> ScreenContext`: Returns screen size and scale information for all displays.
- `get_no_of_ui_elements() -> int`: Returns the number of UI elements in the current UI tree.
- `launch_or_activate_app(app_path: str, xpath: str) -> Element`: Launches a new application or activates an existing window. Returns the Element matching the provided XPath.
- `take_screenshot() -> str`: Takes a screenshot of the current screen, saves it to a temporary directory, and returns the file path.
- `refresh() -> None`: Refreshes the internal UI tree representation by scanning the current window state. Runs in a separate thread to avoid blocking.
- `reload() -> WinDriver`: Reloads the WinDriver instance to refresh its internal state and returns a new WinDriver instance.

### Element

Represents a Windows UI Automation element.

#### Attributes

- `name` (str): The name of the UI element.
- `xpath` (str): The XPath locator for the UI element.
- `handle` (int): The window handle of the UI element.
- `runtime_id` (list[int]): The runtime ID of the UI element.
- `bounding_rectangle` (tuple[int, int, int, int]): The bounding rectangle coordinates (left, top, right, bottom).

#### Methods

- `get_name() -> str`: Returns the name of the UI element.
- `get_xpath() -> str`: Returns the XPath locator of the UI element.
- `get_handle() -> int`: Returns the window handle of the UI element.
- `get_runtime_id() -> list[int]`: Returns the runtime ID of the UI element.
- `send_click() -> None`: Sends a left mouse click to the element.
- `send_double_click() -> None`: Sends a double click to the element.
- `send_right_click() -> None`: Sends a right click to the element.
- `hold_click(holdkeys: str) -> None`: Performs a click while holding specified keys (e.g., "ctrl", "shift", "alt").
- `send_keys(keys: str) -> None`: Sends keyboard input with a 20ms interval between keystrokes. Supports special key syntax with `{}` (e.g., `{ctrl}{alt}{delete}`) and grouping with `()` (e.g., `{ctrl}(AB)` for Ctrl+A+B).
- `send_text(text: str) -> None`: Sends plain text input with a 20ms interval between characters. Use this for text without special keys.
- `hold_send_keys(holdkeys: str, keys: str, interval: int) -> None`: Sends keys while holding modifier keys with a specified interval in milliseconds between keystrokes.
- `show_context_menu() -> None`: Shows the context menu for the element.

### ScreenContext

Contains information about all display screens in the system.

#### Methods

- `get_primary_screen() -> ScreenInfo`: Returns information about the primary display screen.
- `get_screens() -> list[ScreenInfo]`: Returns information about all available display screens.

### ScreenInfo

Represents information about a single display screen.

#### Attributes

- `id` (int): Unique identifier associated with the display.
- `name` (str): The display name.
- `friendly_name` (str): The display friendly name.
- `x` (int): The display x coordinate.
- `y` (int): The display y coordinate.
- `width` (int): The display pixel width.
- `height` (int): The display pixel height.
- `width_mm` (int): The width of the display in millimeters (may be 0).
- `height_mm` (int): The height of the display in millimeters (may be 0).
- `rotation` (float): Screen rotation in clock-wise degrees (0, 90, 180, 270).
- `scale_factor` (float): Output device's pixel scale factor.
- `frequency` (float): The display refresh rate.
- `is_primary` (bool): Whether the screen is the primary display.

#### Methods

- `get_id() -> int`: Returns the unique identifier associated with the display.
- `get_name() -> str`: Returns the display name.
- `get_friendly_name() -> str`: Returns the display friendly name.
- `get_x() -> int`: Returns the display x coordinate.
- `get_y() -> int`: Returns the display y coordinate.
- `get_width() -> int`: Returns the display pixel width.
- `get_height() -> int`: Returns the display pixel height.
- `get_width_mm() -> int`: Returns the width of the display in millimeters.
- `get_height_mm() -> int`: Returns the height of the display in millimeters.
- `get_rotation() -> float`: Returns the screen rotation in clock-wise degrees.
- `get_scale_factor() -> float`: Returns the output device's pixel scale factor.
- `get_frequency() -> float`: Returns the display refresh rate.
- `is_primary() -> bool`: Returns whether this screen is the primary display.

### Logging Functions

- `set_log_level(level: LogLevel) -> None`: Sets the logging level for the bromium module. Use `LogLevel.Error`, `LogLevel.Warn`, `LogLevel.Info`, `LogLevel.Debug`, or `LogLevel.Trace`.
- `get_log_level() -> str`: Returns the current logging level as a string.
- `set_log_file(path: str) -> None`: Sets the full path for the log file. Creates parent directories if needed.
- `set_log_directory(dir_path: str) -> None`: Sets a custom directory for log files. A timestamped log file will be created in this directory.
- `get_log_file() -> str`: Returns the current log file path. Returns the default path if not set.
- `get_default_log_directory() -> str`: Returns the default log directory path (C:\bromium_logs on Windows).

## Requirements

- Python 3.8 or higher
- Windows operating system

## Building from Source

To build the project from source, you'll need:

1. Rust toolchain (cargo, rustc)
2. Python 3.8+
3. maturin (for building Python wheels)

```bash
# Clone the repository
git clone https://github.com/yourusername/bromium.git
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
