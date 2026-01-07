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

Here's a basic example of how to use Bromium:

```python
from bromium import WinDriver
import time
import os

def demo_app_launch():
    print("Testing bromium app launch/activation functionality...")

    # Initialize Bromium logging
    print("Initializing Bromium logging...")
    Bromium.init_logging(log_path=None, log_level=None, enable_console=True, enable_file=True)

    # Create a WinDriver instance
    print("Getting WinDriver Instance...")
    driver = WinDriver(timeout_ms=5_000)
    print("WinDriver instance obtained.")
    no_of_elements = driver.get_no_of_ui_elements()
    print(f"Driver has {no_of_elements} elements.")
    # Path to Windows Calculator (available on all Windows systems)
    # app_path = r"C:\Windows\System32\calc.exe"
    # XPath for Calculator
    # This is a sample XPath for the Calculator window and the "9" button
    # xpath = r'/Pane[@ClassName="#32769"][@Name="Desktop 1"]/Window[@ClassName="ApplicationFrameWindow"][@Name="Calculator"]/Window[@ClassName="Windows.UI.Core.CoreWindow"][@Name="Calculator"]/Custom[@AutomationId="NavView"]/Group[@ClassName="LandmarkTarget"]/Group[@Name="Number pad"][@AutomationId="NumberPad"]/Button[@Name="Nine"][@AutomationId="num9Button"]'

    # Path to MS Teams
    app_path = r"ms-teams.exe"
    # XPath for MS Teams
    # This is a sample XPath for the Teams window 
    xpath = r"/Pane[@Name='Desktop 1']/Window[@Name='Microsoft Teams']"

    file_name = os.path.basename(app_path)
    print(f"Launching/activating {file_name} with path: {app_path}")
    
    # Try to launch or activate the application
    try:
        app_window = driver.launch_or_activate_app(app_path, xpath)
        print(f"First attempt to launch/activate {file_name} returned: {app_window}")
        print(f"{app_window} should now be in focus")
            
        # Wait a moment to observe the result
        time.sleep(3)
        
        # Reload the driver to ensure we have the latest UI tree
        driver = driver.reload()
        no_of_elements = driver.get_no_of_ui_elements()
        print(f"Driver reloaded to refresh UI tree. It now has {no_of_elements} elements.")
 
        # Teams login - if required
        xpath_login_button = r"//Button[@Name='Sign in']"
        try:
            login_button = driver.get_ui_element_by_xpath(xpath_login_button)
            # if this does not raise an exception, the button was found, hence we need to login
            print("Login button found, performing login...")
            login_button.send_click()
            print("Clicked the login button.")
            # give it some time to process
            time.sleep(2)
            driver.refresh_ui_tree()
            xpath_username = r"//Edit[@Name='E-Mail-Adresse, Telefonnummer oder Skype-Name']"
            try:
                username_field = driver.get_ui_element_by_xpath(xpath_username)
                username_field.send_keys("john.doe@gmail.com")
            except Exception as e:
                print("Username field not found, aborting login.")


        except Exception as e:
            print("Login button not found, assuming already logged in.")
    except Exception as e:
        print(f"Error during launch/activation of {file_name}: {e}")        
    
    print("Test completed!")

if __name__ == "__main__":
    demo_app_launch()
```

## API Reference

### Bromium

A class providing access to static methods to initialize and control the logging system of bromium and allowing to obtain new WinDriver instances (simply wrapping a call to the constructor of the WinDriver Class).

### Static Methods
- `get_version -> str`: Returns the current version of the bromium module.
- `get_win_driver -> WinDriver`: Return a new WinDriver object.
- `init_logging(log_path: Optional[str], log_level: Optional[Literal["Off", "Error", "Warn", "Info", "Debug", "Trace"]], enable_console: Optional[bool], enable_file: Optional[bool]) -> None`: Initializes logging with the given parameters.
- `get_log_file() -> str:` Returns the current log file path. Returns default path if not set.
- `set_log_file(path: str) -> None:` Sets the full path for the log file. Creates parent directories if needed.
- `set_log_directory(dir_path: str) -> None:` Sets a custom directory for log files. A timestamped log file will be created in this directory.
- `set_log_level(level: Literal["Off", "Error", "Warn", "Info", "Debug", "Trace"]) -> None:` Sets the logging level for the bromium module.
- `get_log_level() -> str:` Returns the current logging level.
- `enable_console_logging(enable: bool) -> None:` Enable or disable console logging.
- `enable_file_logging(enable: bool) -> None:` Enable or disable file logging.
- `reset_log_file() -> None:` Clear all contents from the current log file.

### WinDriver

The main class for interacting with the Windows UI tree.

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
