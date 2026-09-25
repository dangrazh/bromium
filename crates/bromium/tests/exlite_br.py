import bromium
import keyboard
import parutils as u
from bromium import Element, WinDriver
from parutils import wrap

driver: WinDriver = None

def br_get_xpath():
    print("Retrieving XPath of the element under the cursor...")
    try:
        # Get the current cursor position
        x, y = driver.get_cursor_pos()
        print(f"Cursor position: ({x}, {y})")
        element: Element = driver.get_element_by_coordinates(x, y)
        # xpath = element.xpath
        print("Element found with the following properties:")
        print("Name:", element.name)
        print("Runtime ID:", element.runtime_id)
        print("Bounds:", element.bounding_rectangle)
        print("XPath:", element.xpath)        
        print("getting Panes...")
        panes = driver.get_elements_by_xpath(
            "/Pane/Window[@Name='*Unbenannt – Notepad']/Pane"
        )
        print(f"Found {len(panes)} panes:")
        for index, pane in enumerate(panes, 1):
            print(
                f"Pane[{index}]:",
                repr(pane.name),
                pane.runtime_id,
                pane.bounding_rectangle,
            )        
    
    except (bromium.ElementNotFoundError, AttributeError, RuntimeError, ValueError) as e:
        print(f"Error while trying to get XPath: {e}\n ")

@wrap.simple
def br_init():
    u.log("Initializing bromium driver...")
    bromium.init_logging(log_path="log", log_level="TRACE", enable_console=False, enable_file=True)    
    log_file = bromium.get_log_file()
    u.log(f"Bromium log file initialized at: {log_file}")
    driver = WinDriver(timeout_ms=5000)
    u.log("Bromium driver initialized successfully.")

    return driver

def add_hotkey():
    u.log("Adding hotkey for retrieving XPath...")
    keyboard.add_hotkey('ctrl+alt+q', br_get_xpath)
    print("Press Ctrl + Alt + q to retrieve XPath.\n")
    keyboard.wait()
        
def Exlite():
    global driver
    driver = br_init()
    add_hotkey()
    print()


if __name__ == "__main__":
    Exlite()
