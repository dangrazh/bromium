"""
Bromium App Launch Example
===========================

Demonstrates launching/activating an application and interacting with it:
  1. Initialize logging with console output
  2. Create a WinDriver
  3. Launch or activate an application
  4. Refresh the UI tree
  5. Interact with elements in the application
"""

import time

import bromium

def demo_app_launch():
    print("Testing bromium app launch/activation functionality...")

    # ─── 1. Initialize logging ────────────────────────────────────────────────
    bromium.init_logging(log_level="Info", enable_console=True, enable_file=True)

    # ─── 2. Create a WinDriver ───────────────────────────────────────────────
    print("Creating WinDriver instance...")
    driver = bromium.WinDriver(timeout_ms=5000, window_title=None)
    print(f"WinDriver created with {driver.element_count} elements.")

    # ─── 3. Launch or activate an application ─────────────────────────────────
    # Example: MS Teams
    app_path = r"ms-teams.exe"
    xpath = r"/Pane[@Name='Desktop 1']/Window[@Name='Microsoft Teams']"

    print(f"Launching/activating: {app_path}")

    try:
        app_window = driver.launch_or_activate_app(app_path, xpath)
        print(f"Application window: {app_window!r}")
        print(f"  name: {app_window.name}")
        print(f"  control_type: {app_window.control_type}")

        # Wait for the window to settle
        time.sleep(3)

        # ─── 4. Refresh the UI tree ──────────────────────────────────────────
        # refresh() mutates the driver in place (no need to reassign)
        driver.refresh(window_title="Microsoft Teams")
        print(f"UI tree refreshed. Now has {driver.element_count} elements.")

        # ─── 5. Interact with the application ────────────────────────────────
        # Example: look for a Sign-in button (if not already logged in)
        xpath_login = r"//Button[@Name='Sign in']"
        try:
            login_button = driver.get_element_by_xpath(xpath_login, timeout_ms=3000)
            print("Login button found, clicking...")
            login_button.send_click()
            time.sleep(2)

            # Refresh and fill in username
            driver.refresh(None)
            xpath_username = r"//Edit[@Name='E-Mail-Adresse, Telefonnummer oder Skype-Name']"
            try:
                username_field = driver.get_element_by_xpath(xpath_username, timeout_ms=3000)
                username_field.send_text("john.doe@example.com")
            except bromium.ElementNotFoundError as e:
                print(f"Username field not found: {e}")

        except bromium.ElementNotFoundError:
            print("Login button not found — assuming already logged in.")

    except bromium.AutomationError as e:
        print(f"Error during launch/activation: {e}")

    # ─── 6. Create a scoped driver for verification ──────────────────────────
    print("\nCreating scoped WinDriver for Microsoft Teams...")
    teams_driver = bromium.WinDriver(timeout_ms=5000, window_title="Microsoft Teams")
    print(f"Scoped driver has {teams_driver.element_count} elements.")

    # Show some element types in the Teams window
    buttons = teams_driver.find_elements(control_type="Button")
    print(f"Found {len(buttons)} buttons in Teams window")

    print("\nDemo completed!")


if __name__ == "__main__":
    demo_app_launch()
