# Bromium

Bromium as a project aims to provide the required infrastructure to automate tasks in Microsoft Windows Desktop Applications. It is devided in two main components:
- the bromium Python library that provides bindings to interact with the Windows UI Automation API through Rust. It enables users to automate tasks and interact with Windows UI elements programmatically.
- the UI Expore Desktop application (inspired by inspect.exe) which allows users to inspect the current Windows Desktop, get xpath locators to any ui element on the desktop and test custom xpah locators. It can be run without a need to install the application and without admin rights. 

## Library Features

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

#### Methods

- `launch_or_activate_app() -> bool`: Launches a new application or activates an existing window
- `get_ui_element_by_xpath(xpath: str) -> Element`: Get UI element from an xpath
- `get_curser_pos() -> tuple[int, int]`: Get current cursor coordinates
- `get_ui_element(x: int, y: int) -> Element`: Get UI element at specified coordinates
- `get_screen_context() -> ScreenContext`: Get screen size and scaling information


### Element

Represents a Windows UI Automation element.

#### Methods

- `get_name() -> str`: Get the name of the UI element
- `get_xpath() -> str`: Get the xpath of the UI element
- `get_handle() -> int`: Get the window handle of the UI element    
- `send_click()`: Send a left mouse click to the UI element

### ScreenContext

Contains information about the screen configuration.

#### Methods

- `get_screen_width() -> int`: Get screen width in pixels
- `get_screen_height() -> int`: Get screen height in pixels
- `get_screen_scale() -> float`: Get screen scaling factor (as a decimal, e.g., 1.25 for 125%)

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
