use log::{debug, error, info, warn};
use uiautomation::{UIAutomation, UIElement};

pub fn get_ui_automation_instance() -> Option<UIAutomation> {
    debug!("Creating UIAutomation instance");

    let uia: UIAutomation;
    let uia_res = UIAutomation::new();

    match uia_res {
        Ok(uia_ok) => {
            uia = uia_ok;
            info!("UIAutomation instance created successfully");
        }
        Err(e) => {
            warn!(
                "Failed to create UIAutomation instance, trying direct method: {:?}",
                e
            );
            let uia_direct_res = UIAutomation::new_direct();
            match uia_direct_res {
                Ok(uia_direct_ok) => {
                    uia = uia_direct_ok;
                    info!("UIAutomation instance created successfully using direct method.");
                }
                Err(e_direct) => {
                    error!(
                        "Failed to create UIAutomation instance using direct method: {:?}",
                        e_direct
                    );
                    return None;
                }
            }
        }
    }
    Some(uia)
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
        assert!(uia.is_some(), "Failed to create UIAutomation instance");
    }

    #[test]
    fn test_ui_automation_creation_mta() {
        let uia = get_ui_automation_instance();
        assert!(uia.is_some(), "Failed to create UIAutomation instance");
    }
}
