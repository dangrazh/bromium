use log::{debug, error, info, warn};
use uiautomation::{UIAutomation, UIElement};

pub fn get_ui_automation_instance() -> Result<UIAutomation, uiautomation::Error> {
    debug!("Creating UIAutomation instance");

    match UIAutomation::new() {
        Ok(uia) => {
            info!("UIAutomation instance created successfully");
            Ok(uia)
        }
        Err(e) => {
            warn!(
                "Failed to create UIAutomation instance, trying direct method: {:?}",
                e
            );
            match UIAutomation::new_direct() {
                Ok(uia) => {
                    info!("UIAutomation instance created successfully using direct method.");
                    Ok(uia)
                }
                Err(e_direct) => {
                    error!(
                        "Failed to create UIAutomation instance using direct method: {:?}",
                        e_direct
                    );
                    Err(e_direct)
                }
            }
        }
    }
}

pub struct RuntimeIdFilter(pub Vec<i32>);

impl uiautomation::filters::MatcherFilter for RuntimeIdFilter {
    fn judge(&self, element: &UIElement) -> uiautomation::Result<bool> {
        let id = element.get_runtime_id()?;
        Ok(id == self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ui_automation_creation_sta() {
        use windows::Win32::System::Com::{COINIT_APARTMENTTHREADED, CoInitializeEx};
        let _result = unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) };
        let uia = get_ui_automation_instance();
        assert!(uia.is_ok(), "Failed to create UIAutomation instance");
    }

    #[test]
    fn test_ui_automation_creation_mta() {
        let uia = get_ui_automation_instance();
        assert!(uia.is_ok(), "Failed to create UIAutomation instance");
    }
}
