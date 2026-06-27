from bromium import Bromium, WinDriver
import time
import os

def demo_app_launch():
    print("Testing bromium app launch/activation functionality...")
    
    #initialize Bromium logging
    print("Initializing Bromium logging...")
    Bromium.init_logging(log_path=None, log_level="Info", enable_console=True, enable_file=True)
    
    # Create a WinDriver instance
    print("Getting WinDriver Instance...")
    driver = WinDriver(timeout_ms=5, window_title=None)
    print("WinDriver instance obtained.")
    no_of_elements = driver.element_count
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
    xpath = r"/Pane[@Name='Desktop 1']/Window[contains(@Name,'Microsoft Teams')]"

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
        driver.refresh(None)
        no_of_elements = driver.element_count
        print(f"Driver reloaded to refresh UI tree. It now has {no_of_elements} elements.")
 
        # Increase logging level to Trace for detailed output
        # print("Setting Bromium log level to Trace for detailed output...")
        # Bromium.set_log_level("Trace")

        # Teams login - if required
        xpath_search = r"/Pane[@Name='Desktop 1']/Window[contains(@Name, 'Microsoft Teams')]//ComboBox[@Name='Suche']"
        try:
            search_field = driver.get_element_by_xpath(xpath_search, None)
            # if this does not raise an exception, the button was found, hence we need to login
            print("Search field found...")
            # Bromium.set_log_level("Trace")
            search_field.send_text("Test")
            # sometimes Teams takes a bit to process the input
            time.sleep(2)
            driver.refresh(None)
            xpath_1st_listitem = r"/Pane[@Name='Desktop 1']/Window[contains(@Name, 'Microsoft Teams')]//Group[@Name='Personen']/ListItem[1]"
            try:
                username_field = driver.get_element_by_xpath(xpath_1st_listitem, None)
                print("First User ListItem  found...")
                username_field.send_click()
                print("Successfully clicked")
            except Exception as e:
                print(f"First ListItem not found, aborting user selection. got error: {e}")
        except Exception as e:
            print(f"Search field not found, got error: {e}")
    except Exception as e:
        print(f"Error during launch/activation of {file_name}: {e}")        
    
    # print("Part 1 of test completed!")

    # print("Getting a new WinDriver instance to check if the MS Teams is running...")
    # driver1 = WinDriver(timeout_ms=5, window_title="Microsoft Teams")
    # no_of_elements = driver1.get_no_of_ui_elements()
    # print(f"New Driver instance has {no_of_elements} elements.")

    print("Test completed!")

if __name__ == "__main__":
    demo_app_launch()