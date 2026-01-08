use crate::windriver::WINDRIVER;
use crate::windriver::WinDriver;


use std::process::Command;
use std::thread;
use std::time::Duration;


use log::{debug, error, info, trace, warn};
use uitree::SaveUIElementXML;



pub fn launch_or_activate_application<>(app_path: &str, xpath: &str) -> Result<SaveUIElementXML, String> {
    
    
    let win_driver_opt: Option<WinDriver>;
    {
        // This is in a dedicated scope to limit the lock duration
        win_driver_opt = WINDRIVER.lock().unwrap().clone();
    }
    
    match win_driver_opt {
        None => {
            error!("WinDriver instance is not initialized");
            return Err("WinDriver instance is not initialized".to_string());
        },
        Some(mut win_driver) => {
            debug!("WinDriver instance is available");
            let ui_tree = win_driver.get_ui_tree();
            debug!("UI Tree is available with {} elements", ui_tree.get_elements().len());
            
            // try to find the application window using the UI tree and xpath
            let element_opt = ui_tree.get_element_by_xpath(xpath);
            match element_opt {
                Some(element) => {
                    info!("Found UI element for xpath: {}", xpath);
                    // set the focus to the application window (the ui element) and return 
                    // a reference the obtained SaveUIElement
                    info!("Activating application window for element: {:?}", element);
                    element.set_focus().map_err(|e| format!("Failed to set focus: {:?}", e))?;
                    return Ok(element.clone());
                },
                None => {
                    warn!("No UI element found for xpath: {}", xpath);
                    // This is the logic to launch the application
                    info!("Launching application at path: {}", app_path);
 
                    match Command::new(app_path).spawn() {
                        Ok(child) => {
                            info!("Successfully spawned process with PID: {:?}", child.id());
                            // Wait for ANY window to appear
                            let max_attempts = 20;
                            let mut attempt = 1;
                            let mut success: bool = false;
                            let mut result: Result::<SaveUIElementXML, String> = Err(format!("UI element not found for xpath: {} after launching application", xpath));
                            debug!("Waiting for application window to appear (max {} attempts)", max_attempts);
                            
                            while attempt <= max_attempts && !success {
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


                                // Try to find the element again
                                win_driver.refresh_ui_tree_top_2().map_err(|e| format!("Failed to refresh UI tree: {:?}", e))?;
                                let ui_tree = win_driver.get_ui_tree();
                                debug!("UI Tree is available with {} elements", ui_tree.get_elements().len());
                                
                                // try to find the application window using the UI tree and xpath
                                let element_opt = ui_tree.get_element_by_xpath(xpath);
                                match element_opt {
                                    Some(element) => {
                                        info!("Found UI element for xpath: {}", xpath);
                                        // set the focus to the application window (the ui element) and return 
                                        // a reference the obtained SaveUIElement
                                        // For now, we just log and return true
                                        info!("Activating (set focus) application window for element: {:?}", element);
                                        element.set_focus().map_err(|e| format!("Failed to set focus: {:?}", e))?;
                                        success = true;
                                        let element_out = element.clone();
                                        info!("Running a full refresh o f the UI tree after activation");
                                        win_driver.refresh_ui_tree(None).map_err(|e| format!("Failed to refresh UI tree: {:?}", e))?;
                                        result = Ok(element_out);
                                    },
                                    None => {
                                        trace!("No UI element found for xpath: {} on attempt {}", xpath, attempt);
                                        result = Err(format!("No UI element found for xpath: {} on attempt {}", xpath, attempt));
                                    }
                                }
                                attempt += 1;
                            }
                            return result;
                        },
                        Err(e) => {
                            error!("Failed to spawn application process: {} - Error: {:?}", app_path, e);
                            return Err(format!("Failed to spawn application process: {} - Error: {:?}", app_path, e));
                        }
                    }
                }
            }
        }
    }
}
