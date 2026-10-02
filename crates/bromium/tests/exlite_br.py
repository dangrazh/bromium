from platform import system

import bromium
import keyboard
import parutils as u
from bromium import Element, WinDriver
from parutils import wrap

driver: WinDriver = None

tests = [
    "Notepad - File menu -> New",
    "Notepad - Settings button",
    "Explorer - New -> Folder",
    "Explorer - context menu -> New -> Folder",
    "Excel - Cell A2",
    "Excel - Data tab -> Sort & Filter -> Filter",
    "Excel - Toolbar -> Save button",
    "VS Code - File menu -> New File...",
    ]
test_iterator = iter(tests)

def br_announce_next_test():
    try:
        test = next(test_iterator)
        print(f"Next test: {test}")
        bromium.log(f"Next test: {test}")
    except StopIteration:
        print("No more tests available.")
        bromium.log("No more tests available.")
        # end the program gracefully
        system.exit(0)

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
        # print("getting Panes...")
        # panes = driver.get_elements_by_xpath(
        #     "/Pane/Window[@Name='*Unbenannt – Notepad']/Pane"
        # )
        # print(f"Found {len(panes)} panes:")
        # for index, pane in enumerate(panes, 1):
        #     print(
        #         f"Pane[{index}]:",
        #         repr(pane.name),
        #         pane.runtime_id,
        #         pane.bounding_rectangle,
        #     )        
    
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

def add_hotkeys():
    u.log("Adding hotkey for retrieving XPath...")
    keyboard.add_hotkey('ctrl+shift+q', br_get_xpath)
    print("Press Ctrl + Shift + q to retrieve XPath.")
    keyboard.add_hotkey('ctrl+alt+n', br_announce_next_test)
    print("Press Ctrl + Alt + n to announce the next test.")
    keyboard.wait()
        
def Exlite():
    global driver
    driver = br_init()
    add_hotkeys()
    print()


if __name__ == "__main__":
    Exlite()
