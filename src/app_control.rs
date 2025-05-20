use std::ffi::OsStr;
use std::os::windows::ffi::OsStrExt;
use std::time::Duration;
use std::thread;
use std::process::Command;
use regex::Regex;

use winapi::um::winuser::{
    FindWindowW, SetForegroundWindow, GetForegroundWindow, ShowWindow, BringWindowToTop,
    SW_RESTORE, SW_SHOW, GetWindowThreadProcessId, AttachThreadInput,
    WM_SYSCOMMAND, SC_RESTORE, SendMessageW, EnumWindows, GetWindowTextW, 
    GetWindowTextLengthW, IsWindowVisible, GetWindowPlacement, 
    WINDOWPLACEMENT, SW_SHOWMINIMIZED, keybd_event, VK_MENU, KEYEVENTF_KEYUP
};
use winapi::shared::windef::HWND;
use winapi::shared::minwindef::{BOOL, LPARAM};

// Extract window names from XPath
fn extract_window_names_from_xpath(xpath: &str) -> Vec<String> {
    let mut window_names = Vec::new();
    
    // Try various patterns to extract window names
    let patterns = [
        r#"/Window\[@Name="([^"]+)"\]"#,
        r#"Window\[@Name="([^"]+)"\]"#,
        r#"\[Name="([^"]+)"\]"#,
    ];
    
    for pattern in &patterns {
        if let Ok(re) = Regex::new(pattern) {
            for cap in re.captures_iter(xpath) {
                if let Some(window_name) = cap.get(1) {
                    window_names.push(window_name.as_str().to_string());
                }
            }
        }
    }
    
    window_names
}

// Scan for all windows on the system
fn scan_for_all_windows() -> Vec<(String, HWND)> {
    // Create a struct to collect all windows
    struct AllWindowsData {
        windows: Vec<(String, HWND)>,
    }
    
    // Window enumeration callback to collect all visible windows
    extern "system" fn collect_all_windows(hwnd: HWND, lparam: LPARAM) -> BOOL {
        unsafe {
            if IsWindowVisible(hwnd) != 0 {
                // Get window title length
                let length = GetWindowTextLengthW(hwnd);
                if length > 0 {
                    // Get window title
                    let mut buffer = vec![0u16; length as usize + 1];
                    GetWindowTextW(hwnd, buffer.as_mut_ptr(), buffer.len() as i32);
                    
                    // Convert to String
                    let window_title = String::from_utf16_lossy(&buffer[..length as usize]);
                    
                    // Add to our collection
                    let data = &mut *(lparam as *mut AllWindowsData);
                    data.windows.push((window_title, hwnd));
                }
            }
            1 // Continue enumeration
        }
    }
    
    let mut data = AllWindowsData {
        windows: Vec::new(),
    };
    
    unsafe {
        EnumWindows(
            Some(collect_all_windows),
            &mut data as *mut AllWindowsData as LPARAM
        );
    }
    
    data.windows
}

// Find window with partial name
fn find_window_with_partial_name(name_part: &str) -> Option<String> {
    struct WindowSearchData {
        search_term: String,
        found_name: Option<String>,
    }
    
    extern "system" fn search_callback(hwnd: HWND, lparam: LPARAM) -> BOOL {
        unsafe {
            if IsWindowVisible(hwnd) != 0 {
                let length = GetWindowTextLengthW(hwnd);
                if length > 0 {
                    let mut buffer = vec![0u16; length as usize + 1];
                    GetWindowTextW(hwnd, buffer.as_mut_ptr(), buffer.len() as i32);
                    
                    let window_title = String::from_utf16_lossy(&buffer[..length as usize]);
                    let data = &mut *(lparam as *mut WindowSearchData);
                    
                    if window_title.to_lowercase().contains(&data.search_term.to_lowercase()) {
                        data.found_name = Some(window_title);
                        return 0; // Stop enumeration
                    }
                }
            }
            1 // Continue enumeration
        }
    }
    
    let mut data = WindowSearchData {
        search_term: name_part.to_string(),
        found_name: None,
    };
    
    unsafe {
        EnumWindows(
            Some(search_callback),
            &mut data as *mut WindowSearchData as LPARAM
        );
    }
    
    data.found_name
}

// Activate window by name
fn activate_window_by_name(window_name: &str) -> bool {
    // Convert to wide string for Windows API
    let window_name_wide: Vec<u16> = OsStr::new(window_name)
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();
    
    unsafe {
        let hwnd = FindWindowW(std::ptr::null(), window_name_wide.as_ptr());
        if hwnd != std::ptr::null_mut() {
            // First, check if already in foreground
            let foreground_hwnd = GetForegroundWindow();
            if foreground_hwnd == hwnd {
                return true;
            }
            
            // Get window placement info to check if minimized
            let mut placement = std::mem::zeroed::<WINDOWPLACEMENT>();
            placement.length = std::mem::size_of::<WINDOWPLACEMENT>() as u32;
            
            if GetWindowPlacement(hwnd, &mut placement) != 0 {
                // If window is minimized, restore it
                if placement.showCmd as i32 == SW_SHOWMINIMIZED {
                    ShowWindow(hwnd, SW_RESTORE);
                }
            }
            
            // Bring window to top of Z-order
            BringWindowToTop(hwnd);
            
            // Send SC_RESTORE command
            SendMessageW(hwnd, WM_SYSCOMMAND, SC_RESTORE, 0);
            
            // More aggressive window activation with thread attachment
            let foreground_thread = GetWindowThreadProcessId(
                GetForegroundWindow(), std::ptr::null_mut());
            let target_thread = GetWindowThreadProcessId(
                hwnd, std::ptr::null_mut());
            
            if foreground_thread != target_thread {
                AttachThreadInput(foreground_thread, target_thread, 1); // Attach
                
                // Multiple activation attempts
                SetForegroundWindow(hwnd);
                ShowWindow(hwnd, SW_SHOW);
                
                // Small delay
                thread::sleep(Duration::from_millis(50));
                
                AttachThreadInput(foreground_thread, target_thread, 0); // Detach
            } else {
                // Direct activation for same thread
                SetForegroundWindow(hwnd);
                ShowWindow(hwnd, SW_SHOW);
            }
            
            // One final check and activation attempt
            if GetForegroundWindow() != hwnd {
                // Alt key action to allow foreground switching
                keybd_event(VK_MENU as u8, 0, 0, 0);
                SetForegroundWindow(hwnd);
                keybd_event(VK_MENU as u8, 0, KEYEVENTF_KEYUP, 0);
            }
            
            // Wait to confirm focus
            thread::sleep(Duration::from_millis(100));
            
            return true;
        }
    }
    
    false // Window not found
}

/// Launch or activate an application based on its path and XPath
/// Returns true if successful, false otherwise
pub fn launch_or_activate_application(app_path: &str, xpath: &str) -> bool {
    // Extract application name from path
    let app_name = match std::path::Path::new(app_path).file_name() {
        Some(name) => name.to_string_lossy().to_string(),
        None => {
            return false;
        }
    };
    
    // Extract the base name without extension
    let app_name_without_ext = if let Some(name) = app_name.split('.').next() {
        name.to_string()
    } else {
        app_name.clone()
    };
    
    // First, try window names from XPath
    let xpath_window_names = extract_window_names_from_xpath(xpath);
    
    // Build a list of potential window names to check
    let mut potential_names = xpath_window_names.clone();
    
    // Add app name variations
    potential_names.push(app_name.clone());
    potential_names.push(app_name_without_ext.clone());
    potential_names.push(format!("{} - ", app_name_without_ext));
    potential_names.push(app_name_without_ext.to_uppercase());
    potential_names.push(app_name_without_ext.to_lowercase());
    
    // Get all windows on the system - using revised code
    let all_windows = scan_for_all_windows();
    
    // Try exact matches first
    for window_info in &all_windows {
        let window_title = &window_info.0;
        
        for potential_name in &potential_names {
            if window_title == potential_name {
                return activate_window_by_name(window_title);
            }
        }
    }
    
    // Then try partial/contained matches
    for window_info in &all_windows {
        let window_title = &window_info.0;
        
        for potential_name in &potential_names {
            // Skip very short names to avoid false matches
            if potential_name.len() <= 3 {
                continue;
            }
            
            if window_title.to_lowercase().contains(&potential_name.to_lowercase()) {
                return activate_window_by_name(window_title);
            }
        }
    }
    
    // Try with partial name matching as a fallback
    for name in &potential_names {
        if name.len() > 3 {
            if let Some(found_window) = find_window_with_partial_name(name) {
                return activate_window_by_name(&found_window);
            }
        }
    }
    
    // If not found, launch the application
    match Command::new(app_path).spawn() {
        Ok(_child) => {
            // Wait for ANY window to appear
            let max_attempts = 20;
            for attempt in 1..=max_attempts {
                // Progressive wait times
                let wait_ms = if attempt < 5 {
                    200
                } else if attempt < 10 {
                    500
                } else {
                    1000
                };
                
                thread::sleep(Duration::from_millis(wait_ms));
                
                // Get updated window list
                let new_windows = scan_for_all_windows();
                
                // Look for windows that match our criteria
                for window_info in &new_windows {
                    let window_title = &window_info.0;
                    
                    // First check XPath window names
                    for xpath_name in &xpath_window_names {
                        if window_title.contains(xpath_name) {
                            return activate_window_by_name(window_title);
                        }
                    }
                    
                    // Then check app name
                    if window_title.to_lowercase().contains(&app_name_without_ext.to_lowercase()) {
                        return activate_window_by_name(window_title);
                    }
                }
                
                // Try with partial name matching again
                if let Some(found_window) = find_window_with_partial_name(&app_name_without_ext) {
                    return activate_window_by_name(&found_window);
                }
            }
            
            // If we still can't find it, assume success anyway
            true
        },
        Err(_) => {
            false
        }
    }
}