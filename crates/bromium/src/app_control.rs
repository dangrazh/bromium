use crate::windriver::WinDriver;

use std::process::Command;
use std::thread;
use std::time::Duration;

use log::{debug, error, info, trace, warn};
use uitree::SaveUIElementXML;

#[derive(Debug, thiserror::Error)]
pub enum AppControlError {
    #[error("Failed to set focus: {0}")]
    SetFocusFailed(String),
    #[error("Failed to refresh UI tree: {0}")]
    RefreshFailed(String),
    #[error("Failed to spawn application process '{path}': {source}")]
    SpawnFailed {
        path: String,
        source: std::io::Error,
    },
    #[error("UI element not found for xpath '{xpath}' after {attempts} attempts")]
    ElementNotFound { xpath: String, attempts: u32 },
}

pub fn launch_or_activate_application(
    win_driver: &mut WinDriver,
    app_path: &str,
    xpath: &str,
) -> Result<SaveUIElementXML, AppControlError> {
    debug!("WinDriver instance is available");
    let ui_tree = win_driver.get_ui_tree();
    debug!(
        "UI Tree is available with {} elements",
        ui_tree.get_elements().len()
    );

    let element_opt = ui_tree.get_element_by_xpath(xpath);
    match element_opt {
        Some(element) => {
            info!("Found UI element for xpath: {}", xpath);
            info!("Activating application window for element: {:?}", element);
            element
                .set_focus()
                .map_err(|e| AppControlError::SetFocusFailed(format!("{:?}", e)))?;
            Ok(element.clone())
        }
        None => {
            warn!("No UI element found for xpath: {}", xpath);
            info!("Launching application at path: {}", app_path);

            match Command::new(app_path).spawn() {
                Ok(child) => {
                    info!("Successfully spawned process with PID: {:?}", child.id());
                    let max_attempts: u32 = 20;
                    let mut attempt: u32 = 1;
                    let mut success: bool = false;
                    let mut result: Result<SaveUIElementXML, AppControlError> =
                        Err(AppControlError::ElementNotFound {
                            xpath: xpath.to_string(),
                            attempts: max_attempts,
                        });
                    debug!(
                        "Waiting for application window to appear (max {} attempts)",
                        max_attempts
                    );

                    while attempt <= max_attempts && !success {
                        let wait_ms = if attempt < 5 {
                            200
                        } else if attempt < 10 {
                            500
                        } else {
                            1000
                        };

                        trace!(
                            "Attempt {}/{}: waiting {}ms",
                            attempt, max_attempts, wait_ms
                        );
                        thread::sleep(Duration::from_millis(wait_ms));

                        win_driver
                            .refresh_ui_tree_top_2()
                            .map_err(|e| AppControlError::RefreshFailed(format!("{:?}", e)))?;
                        let ui_tree = win_driver.get_ui_tree();
                        debug!(
                            "UI Tree is available with {} elements",
                            ui_tree.get_elements().len()
                        );

                        let element_opt = ui_tree.get_element_by_xpath(xpath);
                        match element_opt {
                            Some(element) => {
                                info!("Found UI element for xpath: {}", xpath);
                                info!(
                                    "Activating (set focus) application window for element: {:?}",
                                    element
                                );
                                element.set_focus().map_err(|e| {
                                    AppControlError::SetFocusFailed(format!("{:?}", e))
                                })?;
                                success = true;
                                let element_out = element.clone();
                                info!("Running a full refresh of the UI tree after activation");
                                win_driver.refresh_ui_tree(None).map_err(|e| {
                                    AppControlError::RefreshFailed(format!("{:?}", e))
                                })?;
                                result = Ok(element_out);
                            }
                            None => {
                                trace!(
                                    "No UI element found for xpath: {} on attempt {}",
                                    xpath, attempt
                                );
                                result = Err(AppControlError::ElementNotFound {
                                    xpath: xpath.to_string(),
                                    attempts: attempt,
                                });
                            }
                        }
                        attempt += 1;
                    }
                    result
                }
                Err(e) => {
                    error!(
                        "Failed to spawn application process: {} - Error: {:?}",
                        app_path, e
                    );
                    Err(AppControlError::SpawnFailed {
                        path: app_path.to_string(),
                        source: e,
                    })
                }
            }
        }
    }
}
