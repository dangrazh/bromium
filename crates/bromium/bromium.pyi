"""
Type stubs for the bromium module.

bromium is a Python library for Windows UI Automation built on Rust + PyO3.
It provides programmatic access to UI elements via the Windows UI Automation API.
"""

from typing import Iterator, Literal, Optional

# ─── Exceptions ───────────────────────────────────────────────────────────────

class ElementNotFoundError(Exception):
    """Raised when a UI element cannot be located (by xpath, coordinates, or runtime ID)."""
    ...

class AutomationError(Exception):
    """Raised when a UI Automation operation fails (click, send_keys, set_value, etc.)."""
    ...

class TreeConstructionError(TimeoutError):
    """Raised when the UI tree cannot be built or refreshed (COM failures, channel timeouts)."""
    ...

# ─── Enums ────────────────────────────────────────────────────────────────────

class LogLevel:
    """Log level enumeration for controlling logging verbosity."""
    Error: LogLevel
    Warn: LogLevel
    Info: LogLevel
    Debug: LogLevel
    Trace: LogLevel
    Off: LogLevel

# ─── Module-level functions ───────────────────────────────────────────────────

def init_logging(
    log_path: Optional[str] = None,
    log_level: Optional[Literal["Off", "Error", "Warn", "Info", "Debug", "Trace"]] = None,
    enable_console: Optional[bool] = None,
    enable_file: Optional[bool] = None,
) -> None:
    """
    Initialize the bromium logging system.

    Args:
        log_path: Directory for log files. Defaults to ~/.bromium.
        log_level: One of "Off","Error","Warn","Info","Debug","Trace". Defaults to "Info".
        enable_console: Enable console output. Defaults to False.
        enable_file: Enable file output. Defaults to True.
    """
    ...

def get_version() -> str:
    """Get the current bromium version string."""
    ...

def get_log_file() -> str:
    """Get the current log file path. Returns default path if not set."""
    ...

def set_log_file(log_file: str) -> None:
    """
    Set the full path for the log file. Creates parent directories if needed.

    Args:
        log_file: Full path to the log file.
    """
    ...

def get_log_level() -> str:
    """Get the current logging level as a string."""
    ...

def set_log_level(log_level: Literal["Off", "Error", "Warn", "Info", "Debug", "Trace"]) -> None:
    """
    Set the logging level.

    Args:
        log_level: The desired log level.
    """
    ...

def set_log_directory(log_directory: str) -> None:
    """
    Set a custom directory for log files. A timestamped log file will be created.

    Args:
        log_directory: Directory path where log files should be created.
    """
    ...

def enable_console_logging(enable: bool) -> None:
    """Enable or disable console logging."""
    ...

def enable_file_logging(enable: bool) -> None:
    """Enable or disable file logging."""
    ...

def reset_log_file() -> None:
    """Clear all contents from the current log file."""
    ...

# ─── Element ──────────────────────────────────────────────────────────────────

class Element:
    """
    A UI element discovered via the Windows UI Automation API.

    Properties provide read-only access to element metadata.
    Methods perform actions on the element (click, type, etc.).
    """

    def __init__(
        self,
        name: str,
        xpath: str,
        handle: int,
        control_type: str,
        runtime_id: list[int],
        bounding_rectangle: tuple[int, int, int, int],
    ) -> None: ...

    def __repr__(self) -> str: ...
    def __str__(self) -> str: ...

    # ─── Properties ───────────────────────────────────────────────────────

    @property
    def name(self) -> str:
        """The name of the UI element."""
        ...

    @property
    def xpath(self) -> str:
        """The XPath locator for this element within the UI tree."""
        ...

    @property
    def handle(self) -> int:
        """The native window handle (HWND) of this element."""
        ...

    @property
    def control_type(self) -> str:
        """The UI Automation control type (e.g. "Button", "Edit", "Window")."""
        ...

    @property
    def runtime_id(self) -> list[int]:
        """The runtime ID uniquely identifying this element in the current session."""
        ...

    @property
    def bounding_rectangle(self) -> tuple[int, int, int, int]:
        """The bounding rectangle as (left, top, right, bottom)."""
        ...

    # ─── Actions ──────────────────────────────────────────────────────────

    def send_click(self) -> None:
        """
        Send a click to the element.

        Uses the Invoke pattern if supported, otherwise falls back to
        a coordinate-based mouse click at the element center.

        Raises:
            ElementNotFoundError: If the element cannot be located.
            AutomationError: If the click action fails.
        """
        ...

    def send_double_click(self) -> None:
        """
        Send a double-click to the element center.

        Raises:
            ElementNotFoundError: If the element cannot be located.
            AutomationError: If the action fails.
        """
        ...

    def send_right_click(self) -> None:
        """
        Send a right-click to the element center.

        Raises:
            ElementNotFoundError: If the element cannot be located.
            AutomationError: If the action fails.
        """
        ...

    def hold_click(self, holdkeys: str) -> None:
        """
        Click the element while holding modifier keys.

        Args:
            holdkeys: Modifier keys to hold (e.g. "ctrl", "shift", "alt").

        Raises:
            ElementNotFoundError: If the element cannot be located.
            AutomationError: If the action fails.
        """
        ...

    def send_keys(self, keys: str) -> None:
        """
        Send keyboard input to the element.

        Special keys use ``{}`` syntax: ``{ctrl}{alt}{delete}``, ``{shift}{home}``.
        Group keys with ``()``: ``{ctrl}(AB)`` sends Ctrl+A then Ctrl+B.
        Escape braces/parens: ``{{}Hi{}}`` types ``{Hi}``.

        For plain text without special keys, prefer ``send_text()`` instead.

        Args:
            keys: The key sequence to send.

        Raises:
            ElementNotFoundError: If the element cannot be located.
            AutomationError: If the action fails.
        """
        ...

    def send_text(self, text: str) -> None:
        """
        Send plain text to the element.

        Uses the Value pattern if supported, otherwise falls back to
        simulated key strokes with a 20ms interval.

        Args:
            text: The text to type into the element.

        Raises:
            ElementNotFoundError: If the element cannot be located.
            AutomationError: If the action fails.
        """
        ...

    def hold_send_keys(self, holdkeys: str, keys: str, interval: int) -> None:
        """
        Send keys while holding modifier keys with a custom interval.

        Args:
            holdkeys: Modifier keys to hold (e.g. "ctrl", "shift").
            keys: The keys to send while holding modifiers.
            interval: Interval in milliseconds between keystrokes.

        Raises:
            ElementNotFoundError: If the element cannot be located.
            AutomationError: If the action fails.
        """
        ...

    def show_context_menu(self) -> None:
        """
        Show the context menu for this element.

        Raises:
            ElementNotFoundError: If the element cannot be located.
            AutomationError: If showing the context menu fails.
        """
        ...

# ─── ElementIterator ──────────────────────────────────────────────────────────

class ElementIterator:
    """Iterator over Element objects. Returned by ``WinDriver.__iter__()``."""

    def __iter__(self) -> "ElementIterator": ...
    def __next__(self) -> Element: ...
    def __len__(self) -> int: ...

# ─── WinDriver ────────────────────────────────────────────────────────────────

class WinDriver:
    """
    The main driver for interacting with the Windows UI Automation tree.

    Construct with a timeout (used as the default retry duration for element
    lookups) and an optional window-title filter to scope the tree.

    Supports Python collection protocols:
        - ``len(driver)`` — number of elements in the tree
        - ``for elem in driver`` — iterate all elements
        - ``xpath in driver`` — check if an XPath exists in the tree
    """

    def __init__(self, timeout_ms: int, window_title: Optional[str] = None) -> None:
        """
        Create a new WinDriver, building the UI Automation tree.

        Args:
            timeout_ms: Default timeout in milliseconds for element lookup retries.
            window_title: Optional window title to filter the tree. If None, the
                full desktop tree (depth 2) is captured.

        Raises:
            TreeConstructionError: If the UI tree cannot be built within 120 seconds.
        """
        ...

    def __repr__(self) -> str: ...
    def __str__(self) -> str: ...
    def __len__(self) -> int: ...
    def __iter__(self) -> ElementIterator: ...
    def __contains__(self, xpath: str) -> bool: ...

    # ─── Properties ───────────────────────────────────────────────────────

    @property
    def timeout_ms(self) -> int:
        """The default timeout in milliseconds for element lookup operations."""
        ...

    @timeout_ms.setter
    def timeout_ms(self, value: int) -> None: ...

    @property
    def element_count(self) -> int:
        """Number of UI elements currently in the tree."""
        ...

    @property
    def window_title(self) -> Optional[str]:
        """The window title filter, if set."""
        ...

    @window_title.setter
    def window_title(self, value: Optional[str]) -> None: ...

    # ─── Element lookup ───────────────────────────────────────────────────

    def get_element_by_coordinates(self, x: int, y: int) -> Element:
        """
        Find the UI element at the given screen coordinates.

        Args:
            x: The x screen coordinate.
            y: The y screen coordinate.

        Returns:
            The innermost element containing the point.

        Raises:
            ElementNotFoundError: If no element exists at those coordinates.
        """
        ...

    def get_element_by_xpath(self, xpath: str, timeout_ms: Optional[int] = None) -> Element:
        """
        Find a single element by XPath.

        If not found immediately, retries with tree refreshes until ``timeout_ms``
        elapses. When ``timeout_ms`` is None, the driver's default ``timeout_ms``
        is used. Pass ``0`` to disable retrying.

        Args:
            xpath: The XPath locator string.
            timeout_ms: Per-call timeout override in milliseconds, or None to
                use the driver default.

        Returns:
            The matching Element.

        Raises:
            ElementNotFoundError: If no element matches after the timeout.
            TreeConstructionError: If tree refresh fails during retries.
        """
        ...

    def get_elements_by_xpath(self, xpath: str) -> list[Element]:
        """
        Find all elements matching an XPath expression.

        Args:
            xpath: The XPath locator string.

        Returns:
            A list of matching Elements.

        Raises:
            ElementNotFoundError: If no elements match.
        """
        ...

    def find_elements(
        self,
        control_type: Optional[str] = None,
        name: Optional[str] = None,
    ) -> list[Element]:
        """
        Find elements matching optional filters.

        Filters are case-insensitive substring matches applied to all
        elements currently in the tree.

        Args:
            control_type: Filter by control type (e.g. "Button", "Edit").
            name: Filter by element name.

        Returns:
            All matching elements. Returns an empty list if none match.

        Examples:
            >>> driver.find_elements(control_type="Button")
            >>> driver.find_elements(name="Save")
            >>> driver.find_elements(control_type="Edit", name="Search")
        """
        ...

    # ─── Actions ──────────────────────────────────────────────────────────

    def get_cursor_pos(self) -> tuple[int, int]:
        """
        Get the current mouse cursor position.

        Returns:
            A tuple of (x, y) screen coordinates.
        """
        ...

    def refresh(self, window_title: Optional[str] = None) -> None:
        """
        Refresh the UI tree by re-scanning the current window state.

        Mutates this driver in place. The old tree is replaced.

        Args:
            window_title: Optional title to filter by. If None, uses the
                stored window_title (if any), or scans the full desktop.

        Raises:
            TreeConstructionError: If the refresh fails.
        """
        ...

    def pretty_print_ui_tree(self) -> None:
        """Print the current UI tree to stdout for debugging."""
        ...

    def get_screen_context(self) -> "ScreenContext":
        """
        Get information about all connected display screens.

        Returns:
            A ScreenContext containing all screen metadata.
        """
        ...

    def take_screenshot(self) -> str:
        """
        Take a screenshot of the current screen.

        The screenshot is saved to a temporary directory.

        Returns:
            The file path of the saved screenshot.

        Raises:
            AutomationError: If taking the screenshot fails.
        """
        ...

    def launch_or_activate_app(self, app_path: str, xpath: str) -> Element:
        """
        Launch or activate an application.

        If a window matching the app or XPath is already open, it is brought
        to the foreground. Otherwise the application is launched from
        ``app_path`` and the method waits for the window to appear.

        Args:
            app_path: Full path to the application executable.
            xpath: XPath identifying an element in the application window.

        Returns:
            The Element matching the provided XPath.

        Raises:
            AutomationError: If launch/activation fails.
        """
        ...

# ─── ScreenInfo ───────────────────────────────────────────────────────────────

class ScreenInfo:
    """Information about a single display screen."""

    def __init__(
        self,
        id: int,
        name: str,
        friendly_name: str,
        x: int,
        y: int,
        width: int,
        height: int,
        width_mm: int,
        height_mm: int,
        rotation: float,
        scale_factor: float,
        frequency: float,
        is_primary: bool,
    ) -> None: ...

    def __repr__(self) -> str: ...
    def __str__(self) -> str: ...

    @property
    def id(self) -> int:
        """Unique identifier associated with the display."""
        ...

    @property
    def name(self) -> str:
        """The display name."""
        ...

    @property
    def friendly_name(self) -> str:
        """The display friendly name."""
        ...

    @property
    def x(self) -> int:
        """The display x coordinate."""
        ...

    @property
    def y(self) -> int:
        """The display y coordinate."""
        ...

    @property
    def width(self) -> int:
        """The display pixel width."""
        ...

    @property
    def height(self) -> int:
        """The display pixel height."""
        ...

    @property
    def width_mm(self) -> int:
        """The width of the display in millimeters. May be 0."""
        ...

    @property
    def height_mm(self) -> int:
        """The height of the display in millimeters. May be 0."""
        ...

    @property
    def rotation(self) -> float:
        """Screen rotation in clock-wise degrees (0, 90, 180, 270)."""
        ...

    @property
    def scale_factor(self) -> float:
        """Output device's pixel scale factor."""
        ...

    @property
    def frequency(self) -> float:
        """The display refresh rate."""
        ...

    @property
    def is_primary(self) -> bool:
        """Whether this is the primary display."""
        ...

# ─── ScreenContext ────────────────────────────────────────────────────────────

class ScreenContext:
    """
    Information about all display screens in the system.

    Automatically detects all connected displays on construction.
    """

    def __init__(self) -> None: ...
    def __repr__(self) -> str: ...
    def __str__(self) -> str: ...

    @property
    def primary_screen(self) -> ScreenInfo:
        """The primary display screen."""
        ...

    @property
    def screens(self) -> list[ScreenInfo]:
        """List of all available display screens."""
        ...

# ─── Legacy class (use module-level functions instead) ────────────────────────

class Bromium:
    """
    Legacy configuration namespace. Prefer module-level functions instead.

    For example, use ``bromium.init_logging(...)`` rather than
    ``Bromium.init_logging(...)``.
    """

    @staticmethod
    def init_logging(
        log_path: Optional[str] = None,
        log_level: Optional[Literal["Off", "Error", "Warn", "Info", "Debug", "Trace"]] = None,
        enable_console: Optional[bool] = None,
        enable_file: Optional[bool] = None,
    ) -> None: ...

    @staticmethod
    def get_win_driver(timeout_ms: int, window_title: Optional[str] = None) -> WinDriver: ...

    @staticmethod
    def get_version() -> str: ...

    @staticmethod
    def get_log_file() -> str: ...

    @staticmethod
    def set_log_file(log_file: str) -> None: ...

    @staticmethod
    def get_log_level() -> str: ...

    @staticmethod
    def set_log_level(log_level: str) -> None: ...

    @staticmethod
    def set_log_directory(log_directory: str) -> None: ...

    @staticmethod
    def enable_console_logging(enable: bool) -> None: ...

    @staticmethod
    def enable_file_logging(enable: bool) -> None: ...

    @staticmethod
    def reset_log_file() -> None: ...
