// use std::ffi::OsStr;
// use std::os::windows::ffi::OsStrExt;
// use std::time::Duration;
// use std::thread;

use crate::windriver::WINDRIVER;

#[allow(unused_imports)]
use std::process::Command;
// use regex::Regex;

// use winapi::um::winuser::{
//     FindWindowW, SetForegroundWindow, GetForegroundWindow, ShowWindow, BringWindowToTop,
//     SW_RESTORE, SW_SHOW, GetWindowThreadProcessId, AttachThreadInput,
//     WM_SYSCOMMAND, SC_RESTORE, SendMessageW, EnumWindows, GetWindowTextW, 
//     GetWindowTextLengthW, IsWindowVisible, GetWindowPlacement, 
//     WINDOWPLACEMENT, SW_SHOWMINIMIZED, keybd_event, VK_MENU, KEYEVENTF_KEYUP
// };
// use winapi::shared::windef::HWND;
// use winapi::shared::minwindef::{BOOL, LPARAM};
 #[allow(unused_imports)]
 use log::{debug, error, info, trace, warn};


/* **********************************
 *
 * 
 * replacing with ui tree based logic
 * 
 * 
 * **********************************

 // Extract window names from XPath
fn extract_window_names_from_xpath(xpath: &str) -> Vec<String> {
    debug!("Extract window names from XPath: {}", xpath);

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
    debug!("Scanning for all windows on the system");

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
    
    debug!("Window scan complete, found {} windows", data.windows.len());
    data.windows
}

// Find window with partial name
fn find_window_with_partial_name(name_part: &str) -> Option<String> {
    debug!("Find window with partial name: '{}'", name_part);

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
    
    match data.found_name {
        Some(ref name) => {
            debug!("Found window with partial name match: '{}'", name);
        }
        None => {
            debug!("No window found with partial name: '{}'", name_part);
        }
    }
    
    data.found_name
}

// Activate window by name
fn activate_window_by_name(window_name: &str) -> bool {
    debug!("Activate window by name: '{}'", window_name);

    // Convert to wide string for Windows API
    let window_name_wide: Vec<u16> = OsStr::new(window_name)
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();
    
    unsafe {
        let hwnd = FindWindowW(std::ptr::null(), window_name_wide.as_ptr());
        if hwnd != std::ptr::null_mut() {
            trace!("Found window handle for: '{}'", window_name);
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
            
            if GetForegroundWindow() == hwnd {
                info!("Successfully brought window to foreground: '{}'", window_name);
                return true;
            } else {
                warn!("Window activation may have failed for: '{}'", window_name);
                return true; // Still return true as activation was attempted
            }
        }
    }
    
    error!("Window not found for activation: '{}'", window_name);
    false // Window not found
}

/// Launch or activate an application based on its path and XPath
/// Returns true if successful, false otherwise
pub fn launch_or_activate_application(app_path: &str, xpath: &str) -> bool {
    info!("Attempting to launch or activate application: {}", app_path);
    debug!("Using xpath: {}", xpath);

    // Extract application name from path
    let app_name = match std::path::Path::new(app_path).file_name() {
        Some(name) => name.to_string_lossy().to_string(),
        None => {
            error!("Invalid application path: {}", app_path);
            return false;
        }
    };
    
    debug!("Application name extracted: {}", app_name);

    // Extract the base name without extension
    let app_name_without_ext = if let Some(name) = app_name.split('.').next() {
        name.to_string()
    } else {
        app_name.clone()
    };
    debug!("Base name without extension: {}", app_name_without_ext);
    
    // First, try window names from XPath
    let xpath_window_names = extract_window_names_from_xpath(xpath);
    debug!("Extracted {} window names from XPath: {:?}", 
           xpath_window_names.len(), xpath_window_names);
    
    // Build a list of potential window names to check
    let mut potential_names = xpath_window_names.clone();
    
    // Add app name variations
    potential_names.push(app_name.clone());
    potential_names.push(app_name_without_ext.clone());
    potential_names.push(format!("{} - ", app_name_without_ext));
    potential_names.push(app_name_without_ext.to_uppercase());
    potential_names.push(app_name_without_ext.to_lowercase());
    
    debug!("Built {} potential window names to check", potential_names.len());
    trace!("Potential names: {:?}", potential_names);
    
    // Get all windows on the system - using revised code
    let all_windows = scan_for_all_windows();
    info!("Found {} windows currently open on the system", all_windows.len());
    trace!("All windows: {:?}", all_windows);
    
    // Try exact matches first
    debug!("Attempting exact window title matches");
    for window_info in &all_windows {
        let window_title = &window_info.0;
        
        for potential_name in &potential_names {
            if window_title == potential_name {
                info!("Found exact match for window: '{}'", window_title);
                let result = activate_window_by_name(window_title);
                if result {
                    info!("Successfully activated existing window: '{}'", window_title);
                } else {
                    error!("Failed to activate window: '{}'", window_title);
                }
                return result;
            }
        }
    }
    debug!("No exact window title matches found");
    
    // Then try partial/contained matches
    debug!("Attempting partial/contained window title matches");
    for window_info in &all_windows {
        let window_title = &window_info.0;
        
        for potential_name in &potential_names {
            // Skip very short names to avoid false matches
            if potential_name.len() <= 3 {
                trace!("Skipping short potential name: '{}'", potential_name);
                continue;
            }
            
            if window_title.to_lowercase().contains(&potential_name.to_lowercase()) {
                info!("Found partial match: '{}' contains '{}'", window_title, potential_name);
                let result = activate_window_by_name(window_title);
                if result {
                    info!("Successfully activated existing window: '{}'", window_title);
                } else {
                    error!("Failed to activate window: '{}'", window_title);
                }
                return result;
            }
        }
    }
    debug!("No partial window title matches found");

    // Try with partial name matching as a fallback
    debug!("Attempting fallback partial name matching");
    for name in &potential_names {
        if name.len() > 3 {
            trace!("Searching for partial name: '{}'", name);
            if let Some(found_window) = find_window_with_partial_name(name) {
                info!("Found window via partial name search: '{}'", found_window);
                let result = activate_window_by_name(&found_window);
                if result {
                    info!("Successfully activated existing window: '{}'", found_window);
                } else {
                    error!("Failed to activate window: '{}'", found_window);
                }
                return result;
            }
        }
    }
    debug!("No windows found via partial name matching");
    
    // If not found, launch the application
    info!("Window not found, launching new application instance: {}", app_path);
    match Command::new(app_path).spawn() {
        Ok(child) => {
            info!("Successfully spawned process with PID: {:?}", child.id());
            
            // Wait for ANY window to appear
            let max_attempts = 20;
            debug!("Waiting for application window to appear (max {} attempts)", max_attempts);
            
            for attempt in 1..=max_attempts {
                // Progressive wait times
                let wait_ms = if attempt < 5 {
                    200
                } else if attempt < 10 {
                    500
                } else {
                    1000
                };
                
                trace!("Attempt {}/{}: waiting {}ms", attempt, max_attempts, wait_ms);
                thread::sleep(Duration::from_millis(wait_ms));
                
                // Get updated window list
                let new_windows = scan_for_all_windows();
                trace!("Found {} windows after waiting", new_windows.len());
                
                // Look for windows that match our criteria
                for window_info in &new_windows {
                    let window_title = &window_info.0;
                    
                    // First check XPath window names
                    for xpath_name in &xpath_window_names {
                        if window_title.contains(xpath_name) {
                            info!("Found new window matching XPath name: '{}' (attempt {})", 
                                 window_title, attempt);
                            let result = activate_window_by_name(window_title);
                            if result {
                                info!("Successfully activated new window: '{}'", window_title);
                            } else {
                                error!("Failed to activate new window: '{}'", window_title);
                            }
                            return result;
                        }
                    }
                    
                    // Then check app name
                    if window_title.to_lowercase().contains(&app_name_without_ext.to_lowercase()) {
                        info!("Found new window matching app name: '{}' (attempt {})", 
                             window_title, attempt);
                        let result = activate_window_by_name(window_title);
                        if result {
                            info!("Successfully activated new window: '{}'", window_title);
                        } else {
                            error!("Failed to activate new window: '{}'", window_title);
                        }
                        return result;
                    }
                }
                
                // Try with partial name matching again
                if let Some(found_window) = find_window_with_partial_name(&app_name_without_ext) {
                    info!("Found new window via partial name: '{}' (attempt {})", 
                         found_window, attempt);
                    let result = activate_window_by_name(&found_window);
                    if result {
                        info!("Successfully activated new window: '{}'", found_window);
                    } else {
                        error!("Failed to activate new window: '{}'", found_window);
                    }
                    return result;
                }
                
                if attempt == 5 {
                    debug!("Window not found after 5 attempts, increasing wait time");
                } else if attempt == 10 {
                    warn!("Window not found after 10 attempts, still trying...");
                } else if attempt == 15 {
                    warn!("Window not found after 15 attempts, application may be slow to start");
                }
            }
            
            // If we still can't find it, assume success anyway
            warn!("Could not find window after {} attempts, assuming success", max_attempts);
            true
        },
        Err(e) => {
            error!("Failed to spawn application process: {} - Error: {:?}", app_path, e);
            false
        }
    }
}*/

pub fn launch_or_activate_application(_app_path: &str, _xpath: &str) -> Result<bool, String> {
    // Placeholder implementation
    

    let win_driver_opt = WINDRIVER.lock().unwrap().clone();
    match win_driver_opt {
        None => {
            error!("WinDriver instance is not initialized");
            return Err("WinDriver instance is not initialized".to_string());
        },
        Some(win_driver) => {
            debug!("WinDriver instance is available");
            let ui_tree = win_driver.get_ui_tree();
            debug!("UI Tree is available with {} elements", ui_tree.get_elements().len());
            // TODO: Further logic to utilize the UI tree for application activation would go here
            // todo!("Implementation of  application launch or activation logic currently missing");
        }
    }
    
    Ok(true)

    
}