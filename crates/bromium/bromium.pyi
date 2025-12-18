# This is a stub file for the bromium module, which provides a Windows driver for the MS Windows operating system.

class Element:
    """
    A class representing a UI element in the Windows UI Automation API.
    
    Attributes:
    - name (str): The name of the UI element.
    - xpath (str): The XPath locator for the UI element.
    - handle (int): The window handle of the UI element.
    - runtime_id (list[int]): The runtime ID of the UI element.
    - bounding_rectangle (tuple[int, int, int, int]): The bounding rectangle coordinates (left, top, right, bottom).
    
    Methods:
    - __repr__(self) -> str: Returns a string representation of the Element instance.
    - __str__(self) -> str: Returns the name of the element.
    - get_name(self) -> str: Returns the name of the UI element.
    - get_xpath(self) -> str: Returns the XPath locator of the UI element.
    - get_handle(self) -> int: Returns the window handle of the UI element.
    - get_runtime_id(self) -> list[int]: Returns the runtime ID of the UI element.
    - send_click(self) -> None: Sends a mouse click to the element.
    - send_double_click(self) -> None: Sends a double click to the element.
    - send_right_click(self) -> None: Sends a right click to the element.
    - hold_click(self, holdkeys: str) -> None: Performs a click while holding specified keys.
    - send_keys(self, keys: str) -> None: Sends keyboard input to the element.
    - send_text(self, text: str) -> None: Sends text input to the element.
    - hold_send_keys(self, holdkeys: str, keys: str, interval: int) -> None: Sends keys while holding modifier keys.
    - show_context_menu(self) -> None: Shows the context menu for the element.
    """
    
    def __init__(self, name: str, xpath: str, handle: int, runtime_id: list[int], bounding_rectangle: tuple[int, int, int, int]) -> None:
        """
        Initializes the Element instance.
        
        Parameters:
        - name (str): The name of the UI element.
        - xpath (str): The XPath locator for the UI element.
        - handle (int): The window handle of the UI element.
        - runtime_id (list[int]): The runtime ID of the UI element.
        - bounding_rectangle (tuple[int, int, int, int]): The bounding rectangle coordinates (left, top, right, bottom).
        """
        pass  # Implementation not provided in the stub

    def __repr__(self) -> str:
        """
        Returns a string representation of the Element instance.
        
        Returns:
        - str: A string representation of the Element instance.
        """
        pass

    def __str__(self) -> str:
        """
        Returns the name of the element.
        
        Returns:
        - str: The name of the element.
        """
        pass

    def get_name(self) -> str:
        """
        Returns the name of the UI element.
        
        Returns:
        - str: The name of the UI element.
        """
        pass

    def get_xpath(self) -> str:
        """
        Returns the XPath locator of the UI element.
        
        Returns:
        - str: The XPath locator of the UI element.
        """
        pass

    def get_handle(self) -> int:
        """
        Returns the window handle of the UI element.
        
        Returns:
        - int: The window handle of the UI element.
        """
        pass

    def get_runtime_id(self) -> list[int]:
        """
        Returns the runtime ID of the UI element.
        
        Returns:
        - list[int]: The runtime ID of the UI element.
        """
        pass

    def send_click(self) -> None:
        """
        Sends a mouse click to the element.
        
        Raises:
        - ValueError: If the element cannot be found or the click action fails.
        """
        pass

    def send_double_click(self) -> None:
        """
        Sends a double click to the element.
        
        Raises:
        - ValueError: If the element cannot be found or the double click action fails.
        """
        pass

    def send_right_click(self) -> None:
        """
        Sends a right click to the element.
        
        Raises:
        - ValueError: If the element cannot be found or the right click action fails.
        """
        pass

    def hold_click(self, holdkeys: str) -> None:
        """
        Performs a click while holding specified keys.
        
        Parameters:
        - holdkeys (str): The keys to hold while clicking (e.g., "ctrl", "shift", "alt")
        
        Raises:
        - ValueError: If the element cannot be found or the hold click action fails.
        """
        pass

    def send_keys(self, keys: str) -> None:
        """
        Sends keyboard input to the element with a 20ms interval between keystrokes.
        
        `{}` is used for some special keys. For example: `{ctrl}{alt}{delete}`, `{shift}{home}`.
        `()` is used for group keys. The '(' symbol only takes effect after the '{}' symbol. For example: `{ctrl}(AB)` types `Ctrl+A+B`.
         `{}()` can be quoted by `{}`. For example: `{{}Hi,{(}bromium!{)}{}}` types `{Hi,(bromium)}`.
  
        When inputting only texts without special keys, you should use `send_text()` instead.

        Parameters:
        - keys (str): The keys to send to the element
        
        Raises:
        - ValueError: If the element cannot be found or the send keys action fails.
        """
        pass

    def send_text(self, text: str) -> None:
        """
        Sends text input to the element with a 20ms interval between characters.
        
        Parameters:
        - text (str): The text to send to the element
        
        Raises:
        - ValueError: If the element cannot be found or the send text action fails.
        """
        pass

    def hold_send_keys(self, holdkeys: str, keys: str, interval: int) -> None:
        """
        Sends keys while holding modifier keys.
        
        Parameters:
        - holdkeys (str): The modifier keys to hold (e.g., "ctrl", "shift", "alt")
        - keys (str): The keys to send while holding the modifier keys
        - interval (int): The interval in milliseconds between keystrokes
        
        Raises:
        - ValueError: If the element cannot be found or the hold send keys action fails.
        """
        pass

    def show_context_menu(self) -> None:
        """
        Shows the context menu for the element.
        
        Raises:
        - ValueError: If the element cannot be found or showing the context menu fails.
        """
        pass
    
class WinDriver:
    """
    A class representing a windows driver for the MS Windows operating system.
    
    Attributes:
    - timeout_ms (int): timeout in milliseconds for the driver to respond.
    - ui_tree (UITree): internal representation of the UI hierarchy
    - tree_needs_update (bool): flag indicating if the UI tree needs to be refreshed
    
    Methods:
    - __init__(self, timeout_ms: int) -> None: Initializes the WinDriver instance with a timeout in milliseconds.
    - __repr__(self) -> str: Returns a string representation of the Windriver instance.
    - __str__(self) -> str: Returns a string representation of the Windriver instance.
    - get_timeout(self) -> int: Returns the current timeout value in milliseconds.
    - set_timeout(self, timeout_ms: int) -> None: Sets a new timeout value in milliseconds.
    - get_curser_pos(self) -> tuple[int, int]: Returns the current cursor position.
    - get_ui_element(self, x: int, y: int) -> Element: Returns the UI Element at the given coordinates.
    - get_screen_context(self) -> ScreenContext: Returns the screen context information.
    - take_screenshot(self) -> str: Takes a screenshot of the current screen, saves it and returns the path to the file created.
    - launch_or_activate_app(self, app_path: str, xpath: str) -> bool: Launches or activates an application.
    - refresh(self) -> None: Refreshes the internal UI tree representation.
    """
    
    def __init__(self, timeout_ms: int) -> None:
        """
        Initializes the WinDriver instance with a timeout in milliseconds.
        
        Parameters:
        - timeout_ms (int): The timeout value in milliseconds for UI operations.
        """
        pass

    def __repr__(self) -> str:
        """
        Returns a string representation of the Windriver instance.
        
        Returns:
        - str: A string representation of the Windriver instance.
        """
        pass

    def __str__(self) -> str:
        """
        Returns a string representation of the Windriver instance.
        
        Returns:
        - str: A string representation of the Windriver instance.
        """
        pass

    def reload() -> WinDriver:
        """
        Reloads the WinDriver instance to refresh its internal state.
        
        Returns:
        - WinDriver: A new instance of WinDriver with refreshed state.
        """
        pass
    
    def get_timeout(self) -> int:
        """
        Returns the current timeout value in milliseconds.
        
        Returns:
        - int: The current timeout value in milliseconds.
        """
        pass

    def get_no_of_ui_elements(self) -> int:
        """
        Returns the number of UI elements in the current UI tree.
        
        Returns:
        - int: The number of UI elements.
        """
        pass

    def set_timeout(self, timeout_ms: int) -> None:
        """
        Sets a new timeout value in milliseconds.
        
        Parameters:
        - timeout_ms (int): The new timeout value in milliseconds.
        """
        pass

    def get_curser_pos(self) -> tuple[int, int]:
        """
        Returns the current cursor position as a tuple of (x, y) coordinates.
        
        Returns:
        - tuple[int, int]: The current cursor position as (x, y) coordinates.
        """
        pass

    def get_ui_element_by_coordinates(self, x: int, y: int) -> 'Element':
        """
        Returns the Windows UI Automation API UI element of the window at the given coordinates.
        
        Parameters:
        - x (int): The x-coordinate of the window.
        - y (int): The y-coordinate of the window.
        
        Returns:
        - Element: The Windows UI Automation API UI element of the window at the given coordinates.
        """
        pass

    def get_ui_element_by_xpath(self, xpath: str) -> 'Element':
        """
        Returns the Windows UI Automation API UI element of the window at the given xpath. As an xpath
        is a string representation of the UI element, it is not a valid xpath in the XML sense.
        The search is following a three step approach:
        1. A UI element is searched by its exact xpath.
        2. If the xpath does not provide a unique way to identify an elemt, the element is 
           searched for in the entire UI sub-tree.
           2.1. If there is a single matching element, this element is returned (irrespective if the xpath is a 100% match).
           2.2. If there are multiple matching elements, each found element is checked if the xpath
                matches and if a matching xpath is found the respective element is returned.
        3. if no matching element is found, an exception is raised.
            
        Parameters:
        - xpath (str): The xpath of the window.
        
        Returns:
        - Element: The Windows UI Automation API UI element of the window at the given xpath.
        """
        pass

    def get_screen_context(self) -> 'ScreenContext':
        """
        Returns the screen size and scale as a ScreenContext object.
        
        Returns:
        - ScreenContext: The screen size and scale as a ScreenContext object.
        """
        pass

    def take_screenshot(self) -> str:
        """
        Takes a screenshot of the current screen, saves it and returns the path to the file created.
        
        The screenshot is saved in a temporary directory and the path to the file is returned.
        
        Returns:
        - str: The path to the screenshot file.
        
        Raises:
        - RuntimeError: If taking the screenshot fails.
        """
        pass

    def launch_or_activate_app(self, app_path: str, xpath: str) -> Element:
        """
        Launch or activate an application using its path and an XPath.
        
        This method will:
        1. Try to find and activate an existing window that matches the application name or XPath
        2. If no matching window is found, launch the application from the provided path
        3. Wait for the application window to appear and bring it to the foreground
        
        Parameters:
        - app_path (str): Full path to the application executable
        - xpath (str): XPath that identifies an element in the application window
        
        Returns:
        - bool: True if the application was successfully launched or activated
        """
        pass

    def refresh(self) -> None:
        """
        Refreshes the internal UI tree representation.
        
        This method updates the UI hierarchy by scanning the current window state.
        It runs in a separate thread to avoid blocking the main thread.
        
        Raises:
        - RuntimeError: If refreshing the UI tree fails.
        """
        pass


class ScreenInfo:
    """
    A class representing information about a display screen.
    
    Attributes:
        id (int): Unique identifier associated with the display
        name (str): The display name
        friendly_name (str): The display friendly name
        x (int): The display x coordinate
        y (int): The display y coordinate
        width (int): The display pixel width
        height (int): The display pixel height
        width_mm (int): The width of a display in millimeters (may be 0)
        height_mm (int): The height of a display in millimeters (may be 0)
        rotation (float): Screen rotation in clock-wise degrees (0, 90, 180, 270)
        scale_factor (float): Output device's pixel scale factor
        frequency (float): The display refresh rate
        is_primary (bool): Whether the screen is the main screen
    """
    def __init__(self, id: int, name: str, friendly_name: str, x: int, y: int, 
                 width: int, height: int, width_mm: int, height_mm: int, 
                 rotation: float, scale_factor: float, frequency: float, 
                 is_primary: bool) -> None:
        """
        Initialize a ScreenInfo instance.
        
        Args:
            id (int): Unique identifier associated with the display
            name (str): The display name
            friendly_name (str): The display friendly name
            x (int): The display x coordinate
            y (int): The display y coordinate
            width (int): The display pixel width
            height (int): The display pixel height
            width_mm (int): The width of a display in millimeters
            height_mm (int): The height of a display in millimeters
            rotation (float): Screen rotation in clock-wise degrees
            scale_factor (float): Output device's pixel scale factor
            frequency (float): The display refresh rate
            is_primary (bool): Whether the screen is the main screen
        """
        pass

    def is_primary(self) -> bool:
        """Returns whether this screen is the primary display."""
        pass

    def get_name(self) -> str:
        """Returns the display name."""
        pass

    def get_friendly_name(self) -> str:
        """Returns the display friendly name."""
        pass

    def get_id(self) -> int:
        """Returns the unique identifier associated with the display."""
        pass

    def get_x(self) -> int:
        """Returns the display x coordinate."""
        pass

    def get_y(self) -> int:
        """Returns the display y coordinate."""
        pass

    def get_width(self) -> int:
        """Returns the display pixel width."""
        pass

    def get_height(self) -> int:
        """Returns the display pixel height."""
        pass

    def get_width_mm(self) -> int:
        """Returns the width of the display in millimeters."""
        pass

    def get_height_mm(self) -> int:
        """Returns the height of the display in millimeters."""
        pass

    def get_rotation(self) -> float:
        """Returns the screen rotation in clock-wise degrees."""
        pass

    def get_scale_factor(self) -> float:
        """Returns the output device's pixel scale factor."""
        pass

    def get_frequency(self) -> float:
        """Returns the display refresh rate."""
        pass

    def __repr__(self) -> str:
        """Returns a string representation of the ScreenInfo instance."""
        pass

    def __str__(self) -> str:
        """Returns a string representation of the ScreenInfo instance."""
        pass

class ScreenContext:
    """
    A class representing information about all display screens in the system.
    
    Attributes:
        screens (list[ScreenInfo]): List of all available display screens
        primary_screen (ScreenInfo): The primary display screen
    """
    
    def __init__(self) -> None:
        """
        Initialize a ScreenContext instance.
        Automatically detects and initializes information about all available displays.
        """
        pass

    def __repr__(self) -> str:
        """Returns a string representation of the ScreenContext instance."""
        pass

    def __str__(self) -> str:
        """Returns a string representation of the ScreenContext instance."""
        pass

    def get_primary_screen(self) -> 'ScreenInfo':
        """
        Returns information about the primary display screen.
        
        Returns:
            ScreenInfo: The primary display screen information
        """
        pass

    def get_screens(self) -> list['ScreenInfo']:
        """
        Returns information about all available display screens.
        
        Returns:
            list[ScreenInfo]: List of all available display screens
        """
        pass
    
class LogLevel:
    """Log level enumeration for controlling logging verbosity."""
    Error: 'LogLevel'
    Warn: 'LogLevel'
    Info: 'LogLevel'
    Debug: 'LogLevel'
    Trace: 'LogLevel'

def set_log_level(level: LogLevel) -> None:
    """
    Set the logging level for the bromium module.
    
    Parameters:
    - level (LogLevel): The desired log level (Error, Warn, Info, Debug, or Trace)
    
    Example:
        import bromium
        bromium.set_log_level(bromium.LogLevel.Info)
    """
    pass

def get_log_level() -> str:
    """
    Get the current logging level.
    
    Returns:
    - str: The current log level as a string
    """
    pass

def set_log_file(path: str) -> None:
    """
    Set the full path for the log file. Creates parent directories if needed.
    
    Parameters:
    - path (str): Full path to the log file
    
    Example:
        import bromium
        bromium.set_log_file("D:/my_logs/bromium_custom.log")
    """
    pass

def set_log_directory(dir_path: str) -> None:
    """
    Set a custom directory for log files. A timestamped log file will be created in this directory.
    
    Parameters:
    - dir_path (str): Directory path where log files should be created
    
    Example:
        import bromium
        bromium.set_log_directory("D:/my_custom_logs")
    """
    pass

def get_log_file() -> str:
    """
    Get the current log file path. Returns default path if not set.
    
    Returns:
    - str: The current log file path
    """
    pass

def get_default_log_directory() -> str:
    """
    Get the default log directory path (C:\\bromium_logs on Windows).
    
    Returns:
    - str: The default log directory path
    """
    pass