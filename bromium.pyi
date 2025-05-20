# This is a stub file for the bromium module, which provides a Windows driver for the MS Windows operating system.

class Element:
    """
    A class representing a UI element in the Windows UI Automation API.
    
    Attributes:
    - name (str): The name of the UI element.
    
    
    Methods:
    - __repr__(self) -> str: Returns a string representation of the Element instance.
    - __str__(self) -> str: Returns a string representation of the Element instance.
    - get_name(self) -> str: Returns the name of the UI element.
    """
    
    def __init__(self, name: str) -> None:
        """
        Initializes the Element instance with the given name.
        
        Parameters:
        - name (str): The name of the UI element.
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
        Returns a string representation of the Element instance.
        
        Returns:
        - str: A string representation of the Element instance.
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
        Returns the xpath of the UI element.
        
        Returns:
        - str: The xpath of the UI element.
        """
        pass

    def get_handle(self) -> int:
        """
        Returns the handle of the UI element.
        
        Returns:
        - int: The handle of the UI element.
        """
        pass

    def send_click(self) -> None:
        """
        Sends a click event to the UI element.
        
        Returns:
        - None: This method does not return any value.
        """
        pass
    
class ScreenContext:
    """
    A class representing the screen size and scale.
    
    Attributes:
    - width (int): The width of the screen in pixels.
    - height (int): The height of the screen in pixels.
    - scale (float): The scale of the screen as a percentage.
    
    Methods:
    - __repr__(self) -> str: Returns a string representation of the ScreenContext instance.
    - __str__(self) -> str: Returns a string representation of the ScreenContext instance.
    - get_width(self) -> int: Returns the width of the screen in pixels.
    - get_height(self) -> int: Returns the height of the screen in pixels.
    - get_scale(self) -> float: Returns the scale of the screen as a decimal number representing the percentage set in the Windows screen settings.
    """

    def __init__(self, width: int, height: int, scale: float) -> None:
        """
        Initializes the ScreenContext instance with the given width, height, and scale.
        
        Parameters:
        - width (int): The width of the screen in pixels.
        - height (int): The height of the screen in pixels.
        - scale (float): The scale of the screen as a decimal number representing the percentage set in the Windows screen settings.
        """
        pass  # Implementation not provided in the stub

    def __repr__(self) -> str:
        """
        Returns a string representation of the ScreenContext instance.
        
        Returns:
        - str: A string representation of the ScreenContext instance.
        """
        pass

    def __str__(self) -> str:    
        """
        Returns a string representation of the ScreenContext instance.
        
        Returns:
        - str: A string representation of the ScreenContext instance.
        """
        pass

    def get_screen_width(self) -> int:
        """
        Returns the width of the screen in pixels.
        
        Returns:
        - int: The width of the screen in pixels.
        """
        pass

    def get_screen_height(self) -> int:
        """
        Returns the height of the screen in pixels.
        
        Returns:
        - int: The height of the screen in pixels.
        """
        pass

    def get_screen_scale(self) -> float:
        """
        Returns the scale of the screen as a decimal number representing the percentage set in the Windows screen settings.
        
        Returns:
        - float: The scale of the screen as a decimal number representing the percentage set in the Windows screen settings.
        """
        pass    


class WinDriver:
    """
    A class representing a windows driver for the MS Windows operating system.
    
    Attributes:
    - timeout (int): tiemout in seconds for the driver to respond.
    
    Methods:
    - __init__(self, timeout: int) -> None: Initializes the Winddriver instance with a timeout.
    - __repr__(self) -> str: Returns a string representation of the Winddriver instance.
    - __str__(self) -> str: Returns a string representation of the Winddriver instance.
    - get_curser_pos(self) -> tuple[int, int]: Returns the current cursor position as a tuple of (x, y) coordinates.
    - get_ui_element(self, x: int, y: int) -> Element: Returns the Windows UI Automation API UI Element of the window at the given coordinates.
    - get_screen_context(self) -> SreenContext: Returns the screen size and scale as a ScreenContext object.
    - launch_or_activate_app(self, app_path: str, xpath: str) -> bool: Launches a new application or activates an existing window.
    """
    
    def __init__(self, timeout: int) -> None:
        """
        Initializes the Winddriver instance with a name and version.
        
        Parameters:
        - name (str): The name of the wind driver.
        - version (str): The version of the wind driver.
        """
        pass  # Implementation not provided in the stub

    def __repr__(self) -> str:
        """
        Returns a string representation of the Winddriver instance.
        
        Returns:
        - str: A string representation of the Winddriver instance.
        """
        pass

    def __str__(self) -> str:
        """
        Returns a string representation of the Winddriver instance.
        
        Returns:
        - str: A string representation of the Winddriver instance.
        """
        pass

    def get_curser_pos(self) -> tuple[int, int]:
        """
        Returns the current cursor position as a tuple of (x, y) coordinates.
        
        Returns:
        - tuple[int, int]: The current cursor position as (x, y) coordinates.
        """
        pass

    def get_ui_element(self, x: int, y: int) -> 'Element':
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
        Returns the Windows UI Automation API UI element of the window at the given xpath.
        
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

    def launch_or_activate_app(self, app_path: str, xpath: str) -> bool:
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