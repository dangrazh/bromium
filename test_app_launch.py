from bromium import WinDriver
import time, os

def test_app_launch():
    print("Testing bromium app launch/activation functionality...")
    
    # Create a WinDriver instance
    driver = WinDriver(timeout_ms=5)
    
    # Path to Windows Calculator (available on all Windows systems)
    # app_path = r"C:\Windows\System32\calc.exe"
    # XPath for Calculator
    # This is a sample XPath for the Calculator window and the "9" button
    # xpath = r'/Pane[@ClassName="#32769"][@Name="Desktop 1"]/Window[@ClassName="ApplicationFrameWindow"][@Name="Calculator"]/Window[@ClassName="Windows.UI.Core.CoreWindow"][@Name="Calculator"]/Custom[@AutomationId="NavView"]/Group[@ClassName="LandmarkTarget"]/Group[@Name="Number pad"][@AutomationId="NumberPad"]/Button[@Name="Nine"][@AutomationId="num9Button"]'

    # Path to MS Teams
    app_path = r"C:\Program Files\WindowsApps\MSTeams_25094.310.3616.953_x64__8wekyb3d8bbwe\ms-teams.exe"
    # XPath for MS Teams
    # This is a sample XPath for the Teams window and the "Besprechung" button
    xpath = r'/Pane[@ClassName=\"#32769\"][@Name=\"Desktop 1\"]/Window[@ClassName=\"TeamsWebView\"][@Name=\"Besprechungen | Microsoft Teams\"]/Pane[@ClassName=\"Chrome_WidgetWin_0\"]/Pane[@ClassName=\"Chrome_WidgetWin_1\"][@Name=\"Besprechungen | Microsoft Teams\"]/Pane[@ClassName=\"BrowserRootView\"][@Name=\"Besprechungen | Microsoft Teams – Webinhalt – Profilo 3\"]/Pane[@ClassName=\"NonClientView\"]/Pane[@ClassName=\"EmbeddedBrowserFrameView\"]/Pane[@ClassName=\"BrowserView\"]/Pane[@ClassName=\"SidebarContentsSplitView\"]/Pane[@ClassName=\"SidebarContentsSplitView\"]/Pane[@ClassName=\"View\"]/Document[@Name=\"Besprechungen | Microsoft Teams\"][@AutomationId=\"RootWebArea\"]/Group/Group[@AutomationId=\"app\"]/Group/Group[@Name=\"Apps\"]/Group[@Name=\"Besprechungen\"]/Button[@Name=\"Besprechungen\"][@AutomationId=\"40472f6e-248f-4599-842c-ff3ed8f0ae34\"]'

    file_name = os.path.basename(app_path)
    print(f"Launching/activating {file_name} with path: {app_path}")
    
    # Try to launch or activate the application
    success = driver.launch_or_activate_app(app_path, xpath)
    print(f"First attempt result: {'Success' if success else 'Failed'}")
    
    if success:
        print(f"{file_name} should now be in focus")
        
        # Wait a moment to observe the result
        time.sleep(3)
        
        # Test getting the UI element at the current cursor position
        x, y = driver.get_curser_pos()
        print(f"Current cursor position: ({x}, {y})")
        
        # Try to get UI element at cursor position
        try:
            element = driver.get_ui_element(x, y)
            print(f"UI Element at cursor: {element.get_name()}")
        except Exception as e:
            print(f"Error getting UI element: {e}")
    
    # Test again to demonstrate activation of already running app
    print("\nTesting activation of already running app...")
    success2 = driver.launch_or_activate_app(app_path, xpath)
    print(f"Second attempt result: {'Success' if success2 else 'Failed'}")
    
    # Wait to observe
    time.sleep(2)
    
    print("Test completed!")

if __name__ == "__main__":
    test_app_launch()