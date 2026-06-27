from bromium import Bromium, WinDriver

# info
def test_log_settings_info_console_and_file():
    print("Test: Info level logging to console and file")
    Bromium.init_logging(log_path=None, log_level="Info", enable_console=True, enable_file=True)
    driver = Bromium.get_win_driver(timeout_ms=5000, window_title=None)

def test_log_settings_info_file_only():
    print("Test: Info level logging to file only")
    Bromium.init_logging(log_path=None, log_level="Info", enable_console=False, enable_file=True)
    driver = Bromium.get_win_driver(timeout_ms=5000, window_title=None)

def test_log_settings_info_console_only():
    print("Test: Info level logging to console only")
    Bromium.init_logging(log_path=None, log_level="Info", enable_console=True, enable_file=False)
    driver = Bromium.get_win_driver(timeout_ms=5000, window_title=None)

# trace 
def test_log_settings_trace_console_and_file():
    print("Test: Trace level logging to console and file")
    Bromium.init_logging(log_path=None, log_level="Trace", enable_console=True, enable_file=True)
    driver = Bromium.get_win_driver(timeout_ms=5000, window_title=None)

def test_log_settings_trace_console_only():
    print("Test: Trace level logging to console only")
    Bromium.init_logging(log_path=None, log_level="Trace", enable_console=True, enable_file=False)
    driver = Bromium.get_win_driver(timeout_ms=5000, window_title=None)

def test_log_settings_trace_file_only():
    print("Test: Trace level logging to file only")
    Bromium.init_logging(log_path=None, log_level="Trace", enable_console=False, enable_file=True)
    driver = Bromium.get_win_driver(timeout_ms=5000, window_title=None)

# error than info
def test_log_settings_error_then_info_console_and_file():
    print("Test: Error level logging to console and file")
    Bromium.init_logging(log_path=None, log_level="Error", enable_console=True, enable_file=True)
    driver = Bromium.get_win_driver(timeout_ms=5000, window_title=None)
    print("Switching to info level logging now...")
    Bromium.set_log_level("Info")
    driver.refresh(None)

# error
def test_log_settings_error_console_and_file():
    print("Test: Error level logging to console and file")
    Bromium.init_logging(log_path=None, log_level="Error", enable_console=True, enable_file=True)
    driver = Bromium.get_win_driver(timeout_ms=5000, window_title=None)
    elem_xpath = r"/Pane[@Name='Desktop 1']/Window[contains(@Name, 'Microsoft Teams')]//ComboBox[@Name='Search']"
    try:
        element = driver.get_element_by_xpath(elem_xpath, None)
    except Exception as e:
        print(f"Error occurred while getting element: {e}") 

def test_log_settings_error_file_only():
    print("Test: Error level logging to file only")
    Bromium.init_logging(log_path=None, log_level="Error", enable_console=False, enable_file=True)
    driver = Bromium.get_win_driver(timeout_ms=5000, window_title=None) 
    elem_xpath = r"/Pane[@Name='Desktop 1']/Window[contains(@Name, 'Microsoft Teams')]//ComboBox[@Name='Search']"
    try:
        element = driver.get_element_by_xpath(elem_xpath, None)
    except Exception as e:
        print(f"Error occurred while getting element: {e}")

def test_log_settings_error_console_only():
    print("Test: Error level logging to console only")
    Bromium.init_logging(log_path=None, log_level="Error", enable_console=True, enable_file=False)
    driver = Bromium.get_win_driver(timeout_ms=5000, window_title=None)
    elem_xpath = r"/Pane[@Name='Desktop 1']/Window[contains(@Name, 'Microsoft Teams')]//ComboBox[@Name='Search']"
    try:
        element = driver.get_element_by_xpath(elem_xpath, None)
    except Exception as e:
        print(f"Error occurred while getting element: {e}")

# warning
def test_log_settings_warning_console_and_file():
    print("Test: Warning level logging to console and file")
    Bromium.init_logging(log_path=None, log_level="Warning", enable_console=True, enable_file=True)
    driver = Bromium.get_win_driver(timeout_ms=5000, window_title=None)

def test_log_settings_warning_console_only():
    print("Test: Warning level logging to console only")
    Bromium.init_logging(log_path=None, log_level="Warning", enable_console=True, enable_file=False)
    driver = Bromium.get_win_driver(timeout_ms=5000, window_title=None)

def test_log_settings_warning_file_only():
    print("Test: Warning level logging to file only")
    Bromium.init_logging(log_path=None, log_level="Warning", enable_console=False, enable_file=True)
    driver = Bromium.get_win_driver(timeout_ms=5000, window_title=None)

# debug
def test_log_settings_debug_console_and_file():
    print("Test: Debug level logging to console and file")
    Bromium.init_logging(log_path=None, log_level="Debug", enable_console=True, enable_file=True)
    driver = Bromium.get_win_driver(timeout_ms=5000, window_title=None)

def test_log_settings_debug_console_only():
    print("Test: Debug level logging to console only")
    Bromium.init_logging(log_path=None, log_level="Debug", enable_console=True, enable_file=False)
    driver = Bromium.get_win_driver(timeout_ms=5000, window_title=None)

def test_log_settings_debug_file_only():
    print("Test: Debug level logging to file only")
    Bromium.init_logging(log_path=None, log_level="Debug", enable_console=False, enable_file=True)
    driver = Bromium.get_win_driver(timeout_ms=5000, window_title=None)


if __name__ == "__main__":
    # test_log_settings_info_console_and_file()
    # test_log_settings_info_file_only()
    # test_log_settings_info_console_only()
    # test_log_settings_trace_console_and_file()
    # test_log_settings_trace_console_only()
    # test_log_settings_trace_file_only()
    # test_log_settings_error_console_and_file()
    # test_log_settings_error_file_only()
    # test_log_settings_error_console_only()
    # test_log_settings_warning_console_and_file()
    # test_log_settings_warning_console_only()
    # test_log_settings_warning_file_only()
    # test_log_settings_debug_console_and_file()
    # test_log_settings_debug_console_only()
    # test_log_settings_debug_file_only()
    test_log_settings_error_then_info_console_and_file()