"""
Bromium Quickstart Example
===========================

Demonstrates the core workflow:
  1. Initialize logging
  2. Create a WinDriver (builds the UI tree)
  3. Inspect cursor position and element at that point
  4. Look up an element by XPath
  5. Click the element
"""

import bromium

# ─── 1. Initialize logging (optional but helpful for debugging) ───────────────
bromium.init_logging(log_level="Info", enable_console=True)
print(f"bromium version: {bromium.get_version()}")

# ─── 2. Create a WinDriver ───────────────────────────────────────────────────
# timeout_ms is the default retry duration when an element isn't found immediately.
# window_title filters the tree to a specific window (None = full desktop, depth 2).
driver = bromium.WinDriver(timeout_ms=5000, window_title=None)

print(f"Driver: {driver!r}")
print(f"Elements in tree: {len(driver)}")

# ─── 3. Screen info ──────────────────────────────────────────────────────────
screen_context = driver.get_screen_context()
print(f"Number of screens: {len(screen_context.screens)}")
print(f"Primary screen: {screen_context.primary_screen!r}")

# ─── 4. Cursor position & element at that point ──────────────────────────────
x, y = driver.get_cursor_pos()
print(f"Current cursor position: ({x}, {y})")

try:
    element = driver.get_element_by_coordinates(x, y)
    print(f"Element at cursor: {element!r}")
    print(f"  name: {element.name}")
    print(f"  control_type: {element.control_type}")
    print(f"  xpath: {element.xpath}")
    print(f"  bounding_rectangle: {element.bounding_rectangle}")
except bromium.ElementNotFoundError as e:
    print(f"No element at cursor position: {e}")

# ─── 5. XPath-based lookup ───────────────────────────────────────────────────
# Use the xpath from the element we just found (if any)
if element:
    xpath = element.xpath
    try:
        found = driver.get_element_by_xpath(xpath)
        print(f"\nLooked up by XPath: {found.name} ({found.control_type})")
    except bromium.ElementNotFoundError as e:
        print(f"XPath lookup failed: {e}")

# ─── 6. Iterate & filter elements ────────────────────────────────────────────
# Check if an xpath exists in the tree
print(f"\nXPath exists in tree: {xpath in driver}")

# Find all buttons in the tree
buttons = driver.find_elements(control_type="Button")
print(f"Found {len(buttons)} buttons in the UI tree")
for btn in buttons[:5]:  # Show first 5
    print(f"  - {btn.name}")

# ─── 7. Click an element ─────────────────────────────────────────────────────
# Uncomment to actually click:
# found.send_click()
# print("Click action completed.")
