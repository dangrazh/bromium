from bromium import WinDriver
import time
import os

def demo_app_launch():
    print("Testing bromium app launch/activation functionality...")
    
    # Create a WinDriver instance
    print("Getting WinDriver Instance...")
    driver = WinDriver(timeout_ms=5)
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
            login_button = driver.get_element_by_xpath(xpath_login_button)
            # if this does not raise an exception, the button was found, hence we need to login
            print("Login button found, performing login...")
            login_button.send_click()
            print("Clicked the login button.")
            # give it some time to process
            time.sleep(2)
            driver.refresh_ui_tree()
            xpath_username = r"//Edit[@Name='E-Mail-Adresse, Telefonnummer oder Skype-Name']"
            try:
                username_field = driver.get_element_by_xpath(xpath_username)
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