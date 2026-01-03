use uiautomation::{UIAutomation, UIElement}; // controls::ControlType, 
use log::{debug, error, info, warn}; // trace, 



fn get_ui_automation_instance() -> Option<UIAutomation> {
    debug!("Creating UIAutomation instance");

    let uia: UIAutomation;
    let uia_res = UIAutomation::new();
    
    match uia_res {
        Ok(uia_ok) => {
            uia = uia_ok;
            info!("UIAutomation instance created successfully");
        },
        Err(e) => {
            warn!("Failed to create UIAutomation instance, trying direct method: {:?}", e);
            let uia_direct_res = UIAutomation::new_direct();
            match uia_direct_res {
                Ok(uia_direct_ok) => {
                    uia = uia_direct_ok;
                    info!("UIAutomation instance created successfully using direct method.");
                },
                Err(e_direct) => {
                    error!("Failed to create UIAutomation instance using direct method: {:?}", e_direct);
                    return None; // Return None if we cannot create a UIAutomation instance
                }
            }
        }
        
    }
    Some(uia)

}




struct RuntimeIdFilter(Vec<i32>);

impl uiautomation::filters::MatcherFilter for RuntimeIdFilter {
    fn judge(&self, element: &UIElement) -> uiautomation::Result<bool> {
        // self is the element we are looking for
        // element is the element we are checking against
        let id = element.get_runtime_id()?;
        Ok(id == self.0)
    }
}


pub fn get_ui_element_by_runtimeid(runtime_id: Vec<i32>) -> Option<UIElement> {
    debug!("Searching for element with runtime id: {:?}", runtime_id);
    // let automation = UIAutomation::new().unwrap();
    let uia = get_ui_automation_instance().unwrap();
    let matcher = uia.create_matcher().timeout(0).filter(Box::new(RuntimeIdFilter(runtime_id))).depth(99);
    let element = matcher.find_first();
    
    match element {
        Ok(e) => {
            info!("Element found by runtime id: {:?}", e);
            Some(e)
        },
        Err(e) => {
            error!("Error finding element by runtime id: {:?}", e);
            None
        }
    }
    
}

use windows::{
    Win32::UI::Accessibility::{
        IUIAutomationElement,
        IUIAutomationInvokePattern,
        IUIAutomationSelectionItemPattern,
        UIA_InvokePatternId,
        UIA_SelectionItemPatternId,
    },
};

    




pub fn invoke_click(element: &IUIAutomationElement) -> windows::core::Result<()> {
    unsafe {
        let invoke: IUIAutomationInvokePattern =
            element.GetCurrentPatternAs(UIA_InvokePatternId)?;

        invoke.Invoke()?;
    }
    Ok(())
}

pub fn select_item(element: &IUIAutomationElement) -> windows::core::Result<()> {
    unsafe {
        let select: IUIAutomationSelectionItemPattern =
            element.GetCurrentPatternAs(UIA_SelectionItemPatternId)?;

        select.Select()?;
    }
    Ok(())
}

pub fn supports_invoke(element: &IUIAutomationElement) -> bool {
    unsafe {
        let check  = element.GetCurrentPattern(UIA_InvokePatternId).is_ok();
        check
    }
}

pub fn supports_select(element: &IUIAutomationElement) -> bool {
    unsafe {
        let check = element.GetCurrentPattern(UIA_SelectionItemPatternId).is_ok();
        check
    }
}

mod tests {
    // use super::get_ui_automation_instance;
     #[allow(unused_imports)]
     use log::debug;

    #[test]
    fn test_ui_automation_creation_sta() {
        debug!("UIAutomation::test_ui_automation_creation_sta called.");

        use windows::Win32::System::Com::{CoInitializeEx, COINIT_APARTMENTTHREADED};
        
        // Initialize COM library for the current thread with STA (Single Threaded Apartment) model
        // This is done to force the runtime error when uiautomation is initialized with MTA (Multi Threaded Apartment) model
        let _result = unsafe {
            CoInitializeEx(None, COINIT_APARTMENTTHREADED)
        }; 

        // Create a UIAutomation instance
        
        let uia = super::get_ui_automation_instance();
        assert!(uia.is_some(), "Failed to create UIAutomation instance");

    }

    #[test]
    fn test_ui_automation_creation_mta() {
        debug!("UIAutomation::test_ui_automation_creation_mta called.");

        // Create a UIAutomation instance
        let uia = super::get_ui_automation_instance();
        assert!(uia.is_some(), "Failed to create UIAutomation instance");

    }

}